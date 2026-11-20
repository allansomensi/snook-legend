//! The shop on the server: wallets, buying and using items, coin packs and
//! VIP paid through Stripe (see [`crate::payments`]).
//!
//! The server is the only authority: prices, ownership and balances are
//! checked here, never trusted from the game.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use serde_json::Value;
use snook_legend_proto::shop::{
    self, COIN_PACKS, Category, Price, VIP_GIFT, VIP_GIFT_SECS, VIP_MONTHLY_COINS, VIP_PLAN,
    WELCOME_COINS, WELCOME_ITEMS, WELCOME_PACK,
};
use snook_legend_proto::{ClientMsg, PackOffer, ServerMsg};

use crate::lounge::Look;
use crate::store::{BuyOutcome, Payment, User, Wallet};
use crate::{Server, now};

/// A few hours of VIP beyond the paid period, so a renewal that arrives a
/// little late never interrupts it.
const VIP_GRACE: i64 = 6 * 3600;

pub fn is_vip(w: &Wallet, now: i64) -> bool {
    w.vip_until > now
}

/// The item actually used in `category` (what is equipped, if still usable;
/// else the default).
pub fn effective(
    w: &Wallet,
    achievements: &[String],
    category: Category,
    now: i64,
) -> &'static str {
    w.equipped
        .get(category.code())
        .and_then(|code| shop::item(code))
        .filter(|i| {
            i.category == category && shop::can_use(i, &w.owned, achievements, is_vip(w, now))
        })
        .map(|i| i.code)
        .unwrap_or_else(|| category.default_item())
}

fn wallet_msg(w: &Wallet) -> ServerMsg {
    ServerMsg::Wallet {
        coins: w.coins,
        vip_until: w.vip_until,
        owned: w.owned.clone(),
        equipped: w.equipped.iter().map(|(c, i)| (c.clone(), i.clone())).collect(),
        cue_wins: w.cue_wins.iter().map(|(c, n)| (c.clone(), *n)).collect(),
    }
}

/// The level of the cue `code` in this wallet.
pub fn cue_level_of(w: &Wallet, code: &str) -> u8 {
    shop::cue_level(w.cue_wins.get(code).copied().unwrap_or(0))
}

/// What others see of a player: the VIP crown and the items in use.
pub fn look_of(w: &Wallet, achievements: &[String], t: i64) -> Look {
    let item = |c: Category| effective(w, achievements, c, t).to_string();
    let cue = item(Category::Cue);
    Look {
        vip: is_vip(w, t),
        cue_level: cue_level_of(w, &cue),
        cue,
        chalk: item(Category::Chalk),
        cloth: item(Category::Cloth),
        balls: item(Category::Balls),
        frame: item(Category::Frame),
        name_style: item(Category::Name),
        effect: item(Category::Effect),
    }
}

async fn achievements_of(server: &Server, id: i64) -> Vec<String> {
    server.store.achievements(id).await.unwrap_or_default().into_iter().map(|(c, _)| c).collect()
}

/// Sends `user` their wallet and updates what others see of them.
pub async fn push_wallet(server: &Server, user: &User) {
    let (w, achievements) =
        tokio::join!(server.store.wallet(user.id), achievements_of(server, user.id));
    let Ok(w) = w else { return };
    let look = look_of(&w, &achievements, now());
    let msg = wallet_msg(&w);
    server.with(|l| {
        l.notify(&user.username, msg);
        l.set_look(&user.username, look);
    });
}

/// A game won online by `user` (see [`count_cue_game`]).
pub async fn count_cue_win(server: &Server, user: &User) {
    count_cue_game(server, user, true, 0).await
}

/// A game played online by `user` (rated or casual, never on a LAN or
/// against the computer): one more game for the cue they had in use, with
/// the balls they made; a win may raise its level. Sends them their wallet
/// (with the count) and shows the new level to others.
pub async fn count_cue_game(server: &Server, user: &User, won: bool, potted: u32) {
    let (w, achievements) =
        tokio::join!(server.store.wallet(user.id), achievements_of(server, user.id));
    let Ok(w) = w else { return };
    let cue = effective(&w, &achievements, Category::Cue, now());
    match server.store.add_cue_game(user.id, cue, won, potted).await {
        Ok(wins) => {
            if won && shop::cue_level(wins) > shop::cue_level(wins - 1) {
                tracing::info!(user = user.id, cue, wins, "cue levelled up");
            }
            push_wallet(server, user).await;
        }
        Err(e) => tracing::warn!("cue win of {}: {e}", user.username),
    }
}

fn shop_info(server: &Server) -> ServerMsg {
    // Only the Microsoft Store build shows these (and only it can buy them).
    let store_packs = server.msstore.as_ref().map(|m| m.cfg.offers()).unwrap_or_default();
    let store_vip = server.msstore.as_ref().map(|m| m.cfg.vip_offer()).unwrap_or_default();
    match &server.stripe {
        Some(stripe) => ServerMsg::ShopInfo {
            payments: true,
            packs: COIN_PACKS
                .iter()
                .filter_map(|p| {
                    Some(PackOffer {
                        code: p.code.to_string(),
                        coins: p.coins,
                        bonus: p.bonus,
                        price: stripe.label(p.code)?,
                    })
                })
                .collect(),
            vip_price: stripe.label(VIP_PLAN).unwrap_or_default(),
            vip_coins: VIP_MONTHLY_COINS,
            vip_gift_price: stripe.label(VIP_GIFT).unwrap_or_default(),
            welcome_price: stripe.label(WELCOME_PACK).unwrap_or_default(),
            store_packs,
            store_vip,
        },
        None => ServerMsg::ShopInfo {
            payments: false,
            packs: Vec::new(),
            vip_price: String::new(),
            vip_coins: VIP_MONTHLY_COINS,
            vip_gift_price: String::new(),
            welcome_price: String::new(),
            store_packs,
            store_vip,
        },
    }
}

/// A shop message from a signed-in player (never a guest).
pub async fn handle(server: &Arc<Server>, me: &User, msg: ClientMsg, send: &impl Fn(ServerMsg)) {
    let error = |text: &str| send(ServerMsg::Error { message: text.to_string() });
    match msg {
        ClientMsg::Shop => {
            send(shop_info(server));
            push_wallet(server, me).await;
        }
        ClientMsg::BuyItem { code } => {
            let Some(item) = shop::item(&code) else { return error("No such item.") };
            let Price::Coins(price) = item.price else {
                return error("This item can't be bought with coins.");
            };
            match server.store.buy_item(me.id, item.code, price as i64, now()).await {
                Ok(BuyOutcome::Bought) => {
                    tracing::info!(user = me.id, item = item.code, price, "item bought");
                    // Bought to be used: put it on straight away.
                    let _ = server.store.equip(me.id, item.category.code(), item.code).await;
                    send(ServerMsg::ItemBought { code: item.code.to_string(), gift_to: None });
                    push_wallet(server, me).await;
                }
                Ok(BuyOutcome::AlreadyOwned) => error("You already have this item."),
                Ok(BuyOutcome::NotEnoughCoins) => error("Not enough coins."),
                Err(e) => {
                    tracing::error!("buying {code} for {}: {e}", me.username);
                    error("The purchase failed; no coins were taken.")
                }
            }
        }
        ClientMsg::GiftItem { to, code, message } => {
            let Some(item) = shop::item(&code) else { return error("No such item.") };
            let Some(price) = item.coins() else {
                return error("Only items sold for coins can be given.");
            };
            let friend = match friend_named(server, me, &to).await {
                Ok(f) => f,
                Err(e) => return error(e),
            };
            // A message is player text: filtered, and not from the muted
            // (nor between players under 18 whose chat is off).
            let message = if me.chat_restricted() || friend.chat_restricted() {
                String::new()
            } else {
                snook_legend_proto::clean_text(&message, snook_legend_proto::MAX_GIFT_MESSAGE)
            };
            let t = now();
            // (The lounge's copy of the account follows mutes by the staff.)
            let muted_until = server
                .with(|l| l.session_user(&me.username).map(|u| u.muted_until))
                .unwrap_or(me.muted_until)
                .filter(|until| *until > t);
            if let Some(until) = muted_until.filter(|_| !message.is_empty()) {
                let mins = (until - t + 59) / 60;
                return error(&format!("You are muted for {mins} more minutes."));
            }
            let gift = crate::store::NewGift {
                from: me.id,
                to: friend.id,
                code: item.code,
                message: &message,
                at: t,
            };
            match server
                .store
                .give_item(&gift, price as i64)
                .await
                .map(|(o, id)| (o, id.unwrap_or(0)))
            {
                Ok((BuyOutcome::Bought, gift_id)) => {
                    tracing::info!(from = me.id, to = friend.id, item = item.code, "item given");
                    send(ServerMsg::ItemBought {
                        code: item.code.to_string(),
                        gift_to: Some(friend.username.clone()),
                    });
                    push_wallet(server, me).await;
                    crate::social::deliver_gift(server, &friend, gift_id).await;
                    push_wallet(server, &friend).await;
                }
                Ok((BuyOutcome::AlreadyOwned, _)) => error("Your friend already has this item."),
                Ok((BuyOutcome::NotEnoughCoins, _)) => error("Not enough coins."),
                Err(e) => {
                    tracing::error!("gift {code} from {}: {e}", me.username);
                    error("The purchase failed; no coins were taken.")
                }
            }
        }
        ClientMsg::Equip { code } => {
            let Some(item) = shop::item(&code) else { return error("No such item.") };
            let (w, achievements) =
                tokio::join!(server.store.wallet(me.id), achievements_of(server, me.id));
            let w = w.unwrap_or_default();
            if !shop::can_use(item, &w.owned, &achievements, is_vip(&w, now())) {
                return error("You don't have this item.");
            }
            if server.store.equip(me.id, item.category.code(), item.code).await.is_ok() {
                push_wallet(server, me).await;
            }
        }
        ClientMsg::BuyCoins { pack } => {
            let Some(stripe) = &server.stripe else { return error("Purchases are not available.") };
            if shop::coin_pack(&pack).is_none() {
                return error("This pack is not on sale.");
            }
            let customer = server.store.wallet(me.id).await.ok().and_then(|w| w.stripe_customer);
            match stripe.checkout_pack(me.id, &me.username, customer.as_deref(), &pack).await {
                Ok(url) => send(ServerMsg::PaymentPage { url }),
                Err(e) => {
                    tracing::warn!("checkout for {}: {e}", me.username);
                    error("The payment page could not be opened. Try again in a moment.")
                }
            }
        }
        ClientMsg::BuyWelcomePack => {
            let Some(stripe) = &server.stripe else { return error("Purchases are not available.") };
            let Ok(w) = server.store.wallet(me.id).await else {
                return error("Something went wrong. Please try again.");
            };
            if shop::has_welcome_pack(&w.owned) {
                return error("You already have the welcome pack.");
            }
            match stripe
                .checkout_pack(me.id, &me.username, w.stripe_customer.as_deref(), WELCOME_PACK)
                .await
            {
                Ok(url) => send(ServerMsg::PaymentPage { url }),
                Err(e) => {
                    tracing::warn!("welcome pack checkout for {}: {e}", me.username);
                    error("The payment page could not be opened. Try again in a moment.")
                }
            }
        }
        ClientMsg::SubscribeVip => {
            let Some(stripe) = &server.stripe else { return error("Purchases are not available.") };
            let Ok(w) = server.store.wallet(me.id).await else {
                return error("Something went wrong. Please try again.");
            };
            if is_vip(&w, now()) {
                return error("You are already VIP.");
            }
            // A subscription whose renewal failed (VIP over, Stripe still
            // trying to charge it) is taken up again in the portal: a second
            // one would charge twice.
            if let Some(c) = &w.stripe_customer {
                match stripe.has_live_subscription(c).await {
                    Ok(false) => {}
                    Ok(true) => {
                        return error(
                            "You already have a VIP subscription: manage it with Manage subscription.",
                        );
                    }
                    Err(e) => {
                        tracing::warn!("VIP subscriptions of {}: {e}", me.username);
                        return error(
                            "The payment page could not be opened. Try again in a moment.",
                        );
                    }
                }
            }
            match stripe.checkout_vip(me.id, &me.username, w.stripe_customer.as_deref()).await {
                Ok(url) => send(ServerMsg::PaymentPage { url }),
                Err(e) => {
                    tracing::warn!("VIP checkout for {}: {e}", me.username);
                    error("The payment page could not be opened. Try again in a moment.")
                }
            }
        }
        ClientMsg::GiftVip { to } => {
            let Some(stripe) = &server.stripe else { return error("Purchases are not available.") };
            let friend = match friend_named(server, me, &to).await {
                Ok(f) => f,
                Err(e) => return error(e),
            };
            // A subscriber's months come from the subscription; a gift on top
            // would be cut short by its next renewal.
            let Ok(theirs) = server.store.wallet(friend.id).await else {
                return error("Something went wrong. Please try again.");
            };
            if is_vip(&theirs, now()) {
                return error("Your friend is already VIP.");
            }
            let customer = server.store.wallet(me.id).await.ok().and_then(|w| w.stripe_customer);
            match stripe
                .checkout_vip_gift(me.id, &me.username, customer.as_deref(), friend.id)
                .await
            {
                Ok(url) => send(ServerMsg::PaymentPage { url }),
                Err(e) => {
                    tracing::warn!("VIP gift checkout for {}: {e}", me.username);
                    error("The payment page could not be opened. Try again in a moment.")
                }
            }
        }
        ClientMsg::ManageVip => {
            let Some(stripe) = &server.stripe else { return error("Purchases are not available.") };
            let w = server.store.wallet(me.id).await.unwrap_or_default();
            let Some(customer) = w.stripe_customer else {
                return error("There is no subscription to manage.");
            };
            match stripe.portal(&customer).await {
                Ok(url) => send(ServerMsg::PaymentPage { url }),
                Err(e) => {
                    tracing::warn!("portal for {}: {e}", me.username);
                    error("The page could not be opened. Try again in a moment.")
                }
            }
        }
        _ => {}
    }
}

/// The friend called `name`, or why a present can't go to them.
async fn friend_named(server: &Server, me: &User, name: &str) -> Result<User, &'static str> {
    let Ok(Some(friend)) = server.store.user_by_name(name.trim()).await else {
        return Err("No such player.");
    };
    let friends = server.store.friends(me.id).await.unwrap_or_default();
    let is_friend = friends
        .iter()
        .any(|(u, st)| u.id == friend.id && *st == crate::store::FriendState::Friends);
    if !is_friend {
        return Err("You can only give items to your friends.");
    }
    Ok(friend)
}

/// `POST /stripe/webhook`: Stripe's signed notifications.
pub async fn webhook(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(stripe) = &server.stripe else { return StatusCode::NOT_FOUND.into_response() };
    let signature = headers.get("stripe-signature").and_then(|v| v.to_str().ok()).unwrap_or("");
    if !stripe.verify(&body, signature, now()) {
        tracing::warn!("webhook with a bad signature");
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Ok(event) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match apply_event(&server, &event).await {
        Ok(()) => StatusCode::OK.into_response(),
        // Stripe retries: every step is idempotent.
        Err(e) => {
            tracing::error!("webhook {}: {e}", event["type"]);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// The pages Stripe sends the player back to.
pub async fn thanks() -> Html<&'static str> {
    Html(PAGE_THANKS)
}

pub async fn cancelled() -> Html<&'static str> {
    Html(PAGE_CANCELLED)
}

const PAGE_THANKS: &str = "<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'>\
<title>Snook Legend</title><body style='background:#140d08;color:#efe3cc;font:18px system-ui;text-align:center;padding:12vh 20px'>\
<h1 style='color:#e8bf61'>Obrigado! · Thank you!</h1><p>Pode voltar ao jogo: a compra aparece em instantes.</p>\
<p>You can go back to the game: your purchase appears in a moment.</p></body>";

const PAGE_CANCELLED: &str = "<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'>\
<title>Snook Legend</title><body style='background:#140d08;color:#efe3cc;font:18px system-ui;text-align:center;padding:12vh 20px'>\
<h1 style='color:#e8bf61'>Compra cancelada · Purchase cancelled</h1><p>Nada foi cobrado. · Nothing was charged.</p></body>";

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// The e-mail the customer gave on the payment page (a checkout session
/// or an invoice).
fn email_of(obj: &Value) -> Option<String> {
    obj.pointer("/customer_details/email")
        .and_then(Value::as_str)
        .or_else(|| str_of(obj, "customer_email"))
        .map(|e| e.trim().to_lowercase())
        .filter(|e| crate::account::valid_email(e))
}

/// After a payment: a player without a recovery e-mail is offered the one
/// they just paid with.
async fn offer_email(server: &Server, user: &User) {
    if user.recovery_email.is_some() {
        return;
    }
    if let Ok(Some(email)) = server.store.payment_email(user.id).await {
        let msg = crate::account::account_msg(user, Some(email));
        server.with(|l| l.notify(&user.username, msg));
    }
}

/// The account an object names itself (our reference or metadata), if any.
fn named_account(obj: &Value) -> Option<i64> {
    str_of(obj, "client_reference_id")
        .or_else(|| obj.get("metadata").and_then(|m| str_of(m, "user_id")))
        // A subscription's invoice: where the subscription's own metadata is
        // copied to (the place moved between Stripe API versions).
        .or_else(|| {
            [
                "/subscription_details/metadata/user_id",
                "/parent/subscription_details/metadata/user_id",
                "/lines/data/0/metadata/user_id",
            ]
            .iter()
            .find_map(|p| obj.pointer(p).and_then(Value::as_str))
        })
        .and_then(|s| s.parse::<i64>().ok())
}

/// Whether the account an object names was deleted (its subscription
/// would bill no one, every month). Not a payment that names no account
/// of ours (another product of the same Stripe account).
async fn names_deleted_account(server: &Server, obj: &Value) -> Result<bool, String> {
    let Some(id) = named_account(obj) else { return Ok(false) };
    let user = server.store.user_by_id(id).await.map_err(|e| e.to_string())?;
    Ok(user.is_some_and(|u| u.deleted_at.is_some()))
}

/// Ends the subscriptions of the customer an object names (its account is
/// gone); Err (Stripe sends the event again) when they could not be ended.
async fn cancel_orphan_subscriptions(server: &Server, obj: &Value) -> Result<(), String> {
    if let (Some(stripe), Some(c)) = (&server.stripe, str_of(obj, "customer")) {
        let ended = stripe.cancel_subscriptions(c).await?;
        tracing::warn!(customer = c, ended, "subscriptions of a deleted account cancelled");
    }
    Ok(())
}

async fn user_of(server: &Server, obj: &Value) -> Option<User> {
    // Our own reference first, then the customer.
    let id = named_account(obj);
    let id = match id {
        Some(id) => Some(id),
        None => match str_of(obj, "customer") {
            Some(c) => server.store.user_by_stripe_customer(c).await.ok().flatten(),
            None => None,
        },
    }?;
    // A deleted account is no one's: nothing is credited to it.
    server.store.user_by_id(id).await.ok().flatten().filter(|u| u.deleted_at.is_none())
}

async fn notify(server: &Server, user: &User, text: &str) {
    let text = text.to_string();
    server.with(|l| l.notify(&user.username, ServerMsg::Notice { text }));
    push_wallet(server, user).await;
}

/// A VIP month was paid with `invoice`, for the period `start`..`end`:
/// its time and its coins. Safe to run more than once for the same invoice.
#[allow(clippy::too_many_arguments)]
async fn vip_paid(
    server: &Server,
    user: &User,
    invoice: &str,
    start: i64,
    end: i64,
    amount: i64,
    currency: &str,
    email: Option<String>,
) -> Result<(), String> {
    let err = |e: crate::store::StoreError| e.to_string();
    // (A month refunded before its delivery was done still gets what it
    // gives, once: the refund took it back already, and the two cancel out.)
    let first = server
        .store
        .record_payment(&Payment {
            id: invoice.to_string(),
            user_id: user.id,
            kind: "vip".into(),
            // What a refund takes back: only coins that were given.
            coins: if amount > 0 { VIP_MONTHLY_COINS as i64 } else { 0 },
            amount,
            currency: currency.to_string(),
            at: now(),
            refunded: false,
            test: false,
            email,
        })
        .await
        .map_err(err)?;
    // The month's time, added once (with the coins: none for a free trial
    // month) to whatever VIP there is: a month given or bought in the
    // Store on top of the subscription isn't swallowed by its renewal. VIP
    // that had lapsed starts again with the period (and a few hours'
    // grace, which a renewal on time carries over).
    let coins = if amount > 0 { VIP_MONTHLY_COINS as i64 } else { 0 };
    let secs = (end - start).clamp(0, 40 * 86_400);
    let vip = crate::store::VipChange::Add { from: start + VIP_GRACE, secs };
    server
        .store
        .credit_vip(user.id, coins, "vip month", &format!("vip:{invoice}"), now(), vip)
        .await
        .map_err(err)?;
    if first {
        tracing::info!(user = user.id, until = end, "VIP paid");
        notify(server, user, "Welcome to VIP!").await;
        offer_email(server, user).await;
    } else {
        push_wallet(server, user).await;
    }
    Ok(())
}

/// A month of VIP given by `giver` to the player with id `friend` was paid
/// (checkout session `obj`). Safe to run more than once.
async fn vip_gift_paid(
    server: &Server,
    giver: &User,
    friend: &str,
    obj: &Value,
) -> Result<(), String> {
    let err = |e: crate::store::StoreError| e.to_string();
    let Some(friend) = friend.parse::<i64>().ok() else {
        // Not something this server made: answered, or Stripe would send
        // it again and again (and in the end stop sending anything).
        tracing::warn!("VIP gift for {friend:?}: ignored");
        return Ok(());
    };
    let friend = server.store.user_by_id(friend).await.map_err(err)?;
    let Some(friend) = friend.filter(|f| f.deleted_at.is_none()) else {
        // Paid for an account deleted meanwhile: the staff refund it (the
        // payment's id is in the log).
        let payment = str_of(obj, "payment_intent").or(str_of(obj, "id")).unwrap_or("");
        tracing::error!(payment, giver = giver.id, "VIP gift for a deleted player: refund it");
        return Ok(());
    };
    let payment_id = str_of(obj, "payment_intent").or(str_of(obj, "id")).unwrap_or("");
    // (A month refunded before its delivery was done still gets what it
    // gives, once: the refund took it back already, and the two cancel out.)
    // Recorded under the friend: a refund takes back what they were given.
    let first = server
        .store
        .record_payment(&Payment {
            id: payment_id.to_string(),
            user_id: friend.id,
            kind: "vip_gift".into(),
            coins: VIP_MONTHLY_COINS as i64,
            amount: obj["amount_total"].as_i64().unwrap_or(0),
            currency: str_of(obj, "currency").unwrap_or("").to_string(),
            at: now(),
            refunded: false,
            test: false,
            // The giver's e-mail is not the friend's: never kept on their
            // record (it would be offered to them as theirs).
            email: None,
        })
        .await
        .map_err(err)?;
    // The month counts from the payment. Two gifts at once add up; a
    // repeated delivery only makes sure the month is there.
    // The time and the coins go in one step, once: a delivery that stopped
    // half way gives both when it comes again.
    let from = obj["created"].as_i64().unwrap_or(now());
    server
        .store
        .credit_vip(
            friend.id,
            VIP_MONTHLY_COINS as i64,
            &format!("vip gift from {}", giver.username),
            &format!("vipgift:{payment_id}"),
            now(),
            crate::store::VipChange::Add { from, secs: VIP_GIFT_SECS },
        )
        .await
        .map_err(err)?;
    if first {
        tracing::info!(from = giver.id, to = friend.id, "VIP given");
        let msg = ServerMsg::VipGift { from: giver.username.clone(), to: friend.username.clone() };
        server.with(|l| l.notify(&giver.username, msg.clone()));
        let gift = crate::store::NewGift {
            from: giver.id,
            to: friend.id,
            code: VIP_GIFT,
            message: "",
            at: now(),
        };
        match server.store.record_gift(&gift).await {
            Ok(id) => crate::social::deliver_gift(server, &friend, id).await,
            Err(e) => {
                tracing::error!("recording a VIP gift: {e}");
                server.with(|l| l.notify(&friend.username, msg));
            }
        }
        offer_email(server, giver).await;
    }
    push_wallet(server, &friend).await;
    Ok(())
}

/// The welcome pack was paid (checkout session `obj`): its items and
/// coins. Safe to run more than once.
async fn welcome_paid(server: &Server, user: &User, obj: &Value) -> Result<(), String> {
    let err = |e: crate::store::StoreError| e.to_string();
    let payment_id = str_of(obj, "payment_intent").or(str_of(obj, "id")).unwrap_or("");
    // Refunded before its delivery was done: its coins (taken back by the
    // refund already) cancel out, once; its items stay away.
    if server.store.payment(payment_id).await.map_err(err)?.is_some_and(|p| p.refunded) {
        let reference = format!("pay:{payment_id}");
        let coins = WELCOME_COINS as i64;
        server
            .store
            .credit(user.id, coins, "welcome pack", Some(&reference), now())
            .await
            .map_err(err)?;
        return Ok(());
    }
    // Two pages paid (opened together, or a boleto paid late): the coins of
    // the second come too, the items were already there; the staff can
    // refund it (its items stay: the other pack paid for them).
    if welcome_paid_elsewhere(server, user.id, payment_id).await? {
        tracing::error!(
            user = user.id,
            payment = payment_id,
            "welcome pack paid twice: refund one"
        );
    }
    server
        .store
        .record_payment(&Payment {
            id: payment_id.to_string(),
            user_id: user.id,
            kind: "welcome".into(),
            coins: WELCOME_COINS as i64,
            amount: obj["amount_total"].as_i64().unwrap_or(0),
            currency: str_of(obj, "currency").unwrap_or("").to_string(),
            at: now(),
            refunded: false,
            test: false,
            email: email_of(obj),
        })
        .await
        .map_err(err)?;
    for code in WELCOME_ITEMS {
        server.store.grant_item(user.id, code, "welcome pack", now()).await.map_err(err)?;
    }
    let applied = server
        .store
        .credit(
            user.id,
            WELCOME_COINS as i64,
            "welcome pack",
            Some(&format!("pay:{payment_id}")),
            now(),
        )
        .await
        .map_err(err)?;
    if applied {
        tracing::info!(user = user.id, "welcome pack bought");
        // Bought to be used: the new cue goes on straight away.
        let cue = WELCOME_ITEMS[0];
        let _ = server.store.equip(user.id, Category::Cue.code(), cue).await;
        notify(server, user, "Your welcome pack is here!").await;
        offer_email(server, user).await;
    } else {
        push_wallet(server, user).await;
    }
    Ok(())
}

/// Whether the player has a welcome pack paid (and not refunded) other than
/// `payment`.
async fn welcome_paid_elsewhere(server: &Server, user: i64, payment: &str) -> Result<bool, String> {
    let payments = server.store.payments_of(user, 200).await.map_err(|e| e.to_string())?;
    Ok(payments.iter().any(|p| p.kind == "welcome" && !p.refunded && p.id != payment))
}

/// Whether an invoice is a VIP month: one of a subscription, and of the VIP
/// price when it says which (not another product of the same Stripe
/// account).
fn is_vip_invoice(server: &Server, obj: &Value) -> bool {
    let subscription = str_of(obj, "subscription").is_some()
        || obj.pointer("/parent/subscription_details").is_some_and(|d| !d.is_null())
        || obj.pointer("/subscription_details").is_some_and(|d| !d.is_null())
        || obj.pointer("/lines/data/0/subscription").is_some_and(|d| !d.is_null());
    let price = obj
        .pointer("/lines/data/0/price/id")
        .or_else(|| obj.pointer("/lines/data/0/pricing/price_details/price"))
        .and_then(Value::as_str);
    let vip_price = server
        .stripe
        .as_ref()
        .and_then(|s| s.cfg.prices.get(snook_legend_proto::shop::VIP_PLAN))
        .map(|(id, _)| id.as_str());
    subscription && (price.is_none() || vip_price.is_none() || price == vip_price)
}

/// Applies one Stripe event. Safe to run more than once.
pub async fn apply_event(server: &Server, event: &Value) -> Result<(), String> {
    apply(server, event).await?;
    // Made with the test keys: kept apart from real money.
    if event["livemode"].as_bool() == Some(false)
        && let Some(obj) = event.pointer("/data/object")
    {
        for id in [str_of(obj, "payment_intent"), str_of(obj, "invoice"), str_of(obj, "id")]
            .into_iter()
            .flatten()
        {
            server.store.set_payment_test(id, true).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

async fn apply(server: &Server, event: &Value) -> Result<(), String> {
    let kind = str_of(event, "type").unwrap_or("");
    let obj = event.pointer("/data/object").ok_or("no object")?;
    let err = |e: crate::store::StoreError| e.to_string();
    match kind {
        "checkout.session.completed" | "checkout.session.async_payment_succeeded" => {
            let Some(user) = user_of(server, obj).await else {
                if names_deleted_account(server, obj).await? {
                    // A page opened before the account was deleted (a boleto
                    // paid days later): a subscription would bill no one,
                    // every month; the money paid goes back by hand.
                    if str_of(obj, "mode") == Some("subscription") {
                        cancel_orphan_subscriptions(server, obj).await?;
                    }
                    let payment = str_of(obj, "payment_intent").or(str_of(obj, "id")).unwrap_or("");
                    tracing::error!(payment, "{kind} for a deleted account: refund it");
                } else {
                    tracing::warn!("{kind} for an unknown player");
                }
                return Ok(());
            };
            // Two VIP pages paid (opened together): the second subscription
            // would charge every month on top of the first. It is ended (its
            // first month stands, its money goes back by hand), and the
            // customer of the first stays the account's.
            let subscription = str_of(obj, "mode") == Some("subscription");
            let mut duplicate = false;
            if subscription
                && let Some(stripe) = &server.stripe
                && let Some(this) = str_of(obj, "subscription")
            {
                let stored = server.store.wallet(user.id).await.map_err(err)?.stripe_customer;
                let mut customers: Vec<String> = stored.into_iter().collect();
                if let Some(c) = str_of(obj, "customer")
                    && !customers.iter().any(|x| x == c)
                {
                    customers.push(c.to_string());
                }
                for c in &customers {
                    if stripe.live_subscriptions(c).await?.iter().any(|s| s != this) {
                        duplicate = true;
                    }
                }
                if duplicate {
                    stripe.cancel_subscription(this).await?;
                    let invoice = str_of(obj, "invoice").unwrap_or("");
                    tracing::error!(
                        user = user.id,
                        invoice,
                        "a second VIP subscription, ended: refund it"
                    );
                }
            }
            // The customer the subscription is under is the account's (the
            // one its page is managed and cancelled through); a coin pack's
            // doesn't replace one the account has.
            if let Some(c) = str_of(obj, "customer") {
                let replace = subscription && !duplicate;
                server.store.set_stripe_customer(user.id, c, replace).await.map_err(err)?;
            }
            // A new subscription, paid: its first month. The invoice says the
            // same (and the exact dates), but the two can come in any order,
            // and the invoice alone doesn't always say who the player is.
            if str_of(obj, "mode") == Some("subscription")
                && str_of(obj, "payment_status") == Some("paid")
                && let Some(invoice) = str_of(obj, "invoice")
            {
                let amount = obj["amount_total"].as_i64().unwrap_or(0);
                let currency = str_of(obj, "currency").unwrap_or("");
                let email = email_of(obj);
                // From the payment (now), not from when the page was opened.
                let start = now();
                let end = start + 31 * 86_400;
                vip_paid(server, &user, invoice, start, end, amount, currency, email).await?;
            }
            // Coin packs; Pix and boleto are paid later (async_payment_succeeded).
            if str_of(obj, "mode") == Some("payment")
                && str_of(obj, "payment_status") == Some("paid")
                && let Some(friend) = obj.pointer("/metadata/gift_vip_to").and_then(Value::as_str)
            {
                return vip_gift_paid(server, &user, friend, obj).await;
            }
            if str_of(obj, "mode") == Some("payment")
                && str_of(obj, "payment_status") == Some("paid")
            {
                let pack = obj.pointer("/metadata/pack").and_then(Value::as_str).unwrap_or("");
                if pack == WELCOME_PACK {
                    return welcome_paid(server, &user, obj).await;
                }
                let Some(pack) = shop::coin_pack(pack) else {
                    // Not one of ours (another product of the same Stripe
                    // account, or a pack no longer sold): answered, or Stripe
                    // would send it again and again (and in the end stop
                    // sending anything).
                    tracing::warn!("{kind} for an unknown pack {pack:?}: ignored");
                    return Ok(());
                };
                let coins = (pack.coins + pack.bonus) as i64;
                let payment_id = str_of(obj, "payment_intent").or(str_of(obj, "id")).unwrap_or("");
                server
                    .store
                    .record_payment(&Payment {
                        id: payment_id.to_string(),
                        user_id: user.id,
                        kind: "coins".into(),
                        coins,
                        amount: obj["amount_total"].as_i64().unwrap_or(0),
                        currency: str_of(obj, "currency").unwrap_or("").to_string(),
                        at: now(),
                        refunded: false,
                        test: false,
                        email: email_of(obj),
                    })
                    .await
                    .map_err(err)?;
                let applied = server
                    .store
                    .credit(
                        user.id,
                        coins,
                        &format!("pack {}", pack.code),
                        Some(&format!("pay:{payment_id}")),
                        now(),
                    )
                    .await
                    .map_err(err)?;
                if applied {
                    tracing::info!(user = user.id, coins, "coins bought");
                    notify(server, &user, "Coins added to your wallet.").await;
                    offer_email(server, &user).await;
                }
            }
        }
        "invoice.paid" => {
            // An invoice that arrives before the checkout that says whose
            // customer this is: the checkout event grants the month (above).
            if !is_vip_invoice(server, obj) {
                tracing::warn!("invoice.paid for something else than VIP: ignored");
                return Ok(());
            }
            let Some(user) = user_of(server, obj).await else {
                // A renewal of an account deleted meanwhile: its subscription
                // is ended (else it bills no one, every month).
                if names_deleted_account(server, obj).await? {
                    let invoice = str_of(obj, "id").unwrap_or("");
                    tracing::error!(invoice, "a renewal for a deleted account: refund it");
                    return cancel_orphan_subscriptions(server, obj).await;
                }
                tracing::warn!("invoice.paid for an unknown player");
                return Ok(());
            };
            let invoice = str_of(obj, "id").unwrap_or("");
            let end = obj
                .pointer("/lines/data/0/period/end")
                .and_then(Value::as_i64)
                .or_else(|| obj["period_end"].as_i64())
                .unwrap_or(now() + 31 * 86_400);
            let start = obj
                .pointer("/lines/data/0/period/start")
                .and_then(Value::as_i64)
                .or_else(|| obj["period_start"].as_i64())
                .filter(|s| *s < end)
                .unwrap_or(end - 31 * 86_400);
            let amount = obj["amount_paid"].as_i64().unwrap_or(0);
            let currency = str_of(obj, "currency").unwrap_or("");
            vip_paid(server, &user, invoice, start, end, amount, currency, email_of(obj)).await?;
        }
        "customer.subscription.deleted" => {
            if let Some(user) = user_of(server, obj).await {
                // The paid period is over (or the subscription was ended now):
                // what it would still have given goes, and only that. VIP
                // from elsewhere (a month given, bought in the Store, from
                // the staff) stays, after now.
                let ended = obj["ended_at"].as_i64().unwrap_or(now());
                // Ended for a renewal never paid: the period under way wasn't
                // paid for (it began where the paid time ended).
                let unpaid = obj.pointer("/cancellation_details/reason").and_then(Value::as_str)
                    == Some("payment_failed");
                let field = if unpaid { "current_period_start" } else { "current_period_end" };
                let period_end = obj[field]
                    .as_i64()
                    .or_else(|| obj.pointer(&format!("/items/data/0/{field}"))?.as_i64())
                    .unwrap_or(ended);
                let unused = (period_end + VIP_GRACE - ended).max(0);
                // Taken once (a delivery again changes nothing), and once
                // for the month: a refund of its invoice takes the same back
                // (whichever comes first does).
                let reference = match str_of(obj, "latest_invoice") {
                    Some(invoice) => format!("vipback:{invoice}"),
                    None => format!("vipend:{}", str_of(obj, "id").unwrap_or("")),
                };
                let vip = crate::store::VipChange::Take { secs: unused, floor: ended };
                let reason = "vip subscription ended";
                server
                    .store
                    .credit_vip(user.id, 0, reason, &reference, now(), vip)
                    .await
                    .map_err(err)?;
                push_wallet(server, &user).await;
            }
        }
        "charge.refunded" | "charge.dispute.created" => {
            // The money went back: so do the coins (the balance may go below
            // zero; nothing can be bought until it is positive again).
            // A partial refund is a courtesy from the staff: the purchase stands.
            if kind == "charge.refunded" && obj["refunded"].as_bool() == Some(false) {
                return Ok(());
            }
            // Coin packs are recorded under the payment, VIP months under the
            // invoice; a charge carries both ids.
            let mut found = None;
            for id in [str_of(obj, "payment_intent"), str_of(obj, "invoice")].into_iter().flatten()
            {
                if let Some(p) = server.store.payment(id).await.map_err(err)? {
                    found = Some(p);
                    break;
                }
            }
            // A dispute names no invoice, and a charge no longer does on the
            // newer API: a VIP month (recorded under its invoice) is found
            // through the payment's invoice, asked from Stripe.
            // Stripe that can't be asked now is asked again: the event fails
            // and comes back (a month refunded is never kept by mistake).
            if found.is_none()
                && let (Some(stripe), Some(pi)) = (&server.stripe, str_of(obj, "payment_intent"))
                && let Some(invoice) = stripe.invoice_of_payment(pi).await?
            {
                found = server.store.payment(&invoice).await.map_err(err)?;
            }
            let Some(p) = found else { return Ok(()) };
            // The welcome pack's items go back with its coins (whatever of
            // them is in use is no longer shown), unless another pack paid
            // for them too. Before the payment is marked refunded: a delivery
            // that stops half way is done again in full, and one that comes
            // after (the pack bought again) takes nothing.
            if p.kind == "welcome"
                && !p.refunded
                && !welcome_paid_elsewhere(server, p.user_id, &p.id).await?
            {
                server.store.revoke_items(p.user_id, &WELCOME_ITEMS).await.map_err(err)?;
            }
            let first = server.store.refund_payment(&p.id).await.map_err(err)?;
            // Keyed by the payment: taken back exactly once, even when an
            // earlier delivery stopped half-way.
            let reference = format!("refund:{}", p.id);
            let taken = server
                .store
                .credit(p.user_id, -p.coins, kind, Some(&reference), now())
                .await
                .map_err(err)?;
            // The month refunded goes, and only it: other VIP time (other
            // months, gifts, the Store, the staff) stays. Once for the month:
            // its subscription ended now takes the same back (whichever
            // comes first does).
            if p.kind == "vip" || p.kind == "vip_gift" {
                let vip = crate::store::VipChange::Take { secs: VIP_GIFT_SECS, floor: now() };
                let back = format!("vipback:{}", p.id);
                server
                    .store
                    .credit_vip(p.user_id, 0, kind, &back, now(), vip)
                    .await
                    .map_err(err)?;
            }
            if first || taken {
                tracing::warn!(user = p.user_id, payment = %p.id, "payment reversed ({kind})");
                if let Ok(Some(user)) = server.store.user_by_id(p.user_id).await {
                    push_wallet(server, &user).await;
                }
            }
        }
        _ => {}
    }
    Ok(())
}
