//! Coin packs and months of VIP bought in the Microsoft Store (the Store
//! build of the game).
//!
//! The Store's policy for games (10.8.1) asks for digital items to be sold
//! through the Store's own purchases, so the Store build sells coin packs,
//! and a month of VIP ([`shop::VIP_MONTH`]: bought once, with the month's
//! coins; nothing renews by itself), as add-ons of the kind
//! "developer-managed consumable". The money never comes near the game or
//! this server: Microsoft takes it, and this server only learns, from
//! Microsoft itself, that an add-on was bought.
//!
//! How a purchase becomes coins:
//!
//! 1. The game buys the add-on through the Store's own dialog. That alone
//!    credits nothing: the game is not trusted.
//! 2. The game asks for a ticket ([`ClientMsg::StoreTicket`]): an access
//!    token of this server's Microsoft Entra application, for the audience
//!    that only lets the Store make *collections keys*, and the player's
//!    anonymous publisher id (an HMAC of the account id, no personal data).
//! 3. On the player's computer, the Store turns the ticket into a Microsoft
//!    Store ID key for the Microsoft account signed in to the Store, with the
//!    publisher id inside, and the game sends the key back
//!    ([`ClientMsg::StoreRedeem`]).
//! 4. The server checks that the key carries this player's publisher id, asks
//!    Microsoft which of the game's add-ons that Microsoft account bought
//!    and has not received yet (the collections query), and for each one:
//!    a row with a fresh tracking id is saved first, then Microsoft is asked
//!    to consume the purchase with that tracking id, and only Microsoft's
//!    confirmation credits the coins, and the month of VIP (in the same
//!    database transaction that marks the row done; the ledger reference
//!    makes the credit single).
//!
//! Answers lost on the way: Microsoft gives the same answer to the same
//! tracking id however often it is sent, and says to send it again when
//! unsure. A row without an answer stays pending and the request is sent
//! again (the next time the player checks, and in the background) until
//! there is one. A refusal (4xx) means nothing was consumed: the purchase
//! stays in the player's collection and is found again by the next check.
//! Rows without an answer for a week are marked *stuck* for the staff.
//!
//! Configuration (all required to take Store purchases):
//! - `SNOOK_LEGEND_MSSTORE_TENANT_ID`, `SNOOK_LEGEND_MSSTORE_CLIENT_ID`,
//!   `SNOOK_LEGEND_MSSTORE_CLIENT_SECRET`: the Entra application associated
//!   with the game in Partner Center (Services → Product collections and
//!   purchases);
//! - `SNOOK_LEGEND_MSSTORE_PRODUCTS`: `product=StoreID` pairs separated by `;`,
//!   one per coin pack sold in the Store and `vip_month` for the month of
//!   VIP, e.g. `coins_500=9NXXXXXXXXXX;vip_month=9NYYYYYYYYYY`.
//!
//! See `docs/MSSTORE.md` for setting it up.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use snook_legend_proto::shop::{self, CoinPack};
use snook_legend_proto::{ClientMsg, ServerMsg, StorePack};

use crate::store::{MsConsume, MsState, User};
use crate::{Server, now};

/// Entra audience of the token that calls the collections API (never leaves
/// the server).
const SERVICE_AUDIENCE: &str = "https://onestore.microsoft.com";
/// Entra audience of the ticket the game gets: it only lets the Store make a
/// collections key.
const COLLECTIONS_AUDIENCE: &str = "https://onestore.microsoft.com/b2b/keys/create/collections";
/// The `aud` of a Microsoft Store ID key for the collections API.
const KEY_AUDIENCE: &str = "https://collections.mp.microsoft.com/v6.0/keys";
/// The claim with the publisher user id given when the key was made.
const USER_ID_CLAIM: &str = "http://schemas.microsoft.com/marketplace/2015/08/claims/key/userId";
/// The add-on kind every product must have in Partner Center.
const UNMANAGED_CONSUMABLE: &str = "UnmanagedConsumable";

pub const LOGIN_URL: &str = "https://login.microsoftonline.com";
pub const COLLECTIONS_URL: &str = "https://collections.mp.microsoft.com";
const QUERY_PATH: &str = "/v8.0/collections/b2bLicensePreview";
const CONSUME_PATH: &str = "/v8.0/collections/consume";

/// Keys are a few kilobytes.
const MAX_KEY_BYTES: usize = 16 * 1024;
/// Pages of a collection read at most (100 items each).
const MAX_PAGES: usize = 10;
/// A request without an answer for this long is marked stuck (well inside
/// the 30 days a key is valid, so every retry can still be sent).
const GIVE_UP_AFTER: i64 = 7 * 86_400;
/// How often the background task looks for requests to send again.
const RETRY_EVERY: Duration = Duration::from_secs(60);

/// What the player sees when the check could not be made.
const UNREACHABLE: &str = "The Microsoft Store can't be reached right now. Try again in a moment.";
const NOT_CONFIRMED: &str = "The Microsoft Store could not confirm the purchase.";

/// What an add-on sold in the Store gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Product {
    /// A coin pack.
    Coins(&'static CoinPack),
    /// A month of VIP and the month's coins ([`shop::VIP_MONTH`]).
    VipMonth,
}

impl Product {
    /// A coin pack's code, or `vip_month`.
    pub fn parse(code: &str) -> Option<Product> {
        if code == shop::VIP_MONTH {
            return Some(Product::VipMonth);
        }
        shop::coin_pack(code).map(Product::Coins)
    }

    /// What its purchases record (the code it was parsed from).
    pub fn code(self) -> &'static str {
        match self {
            Product::Coins(pack) => pack.code,
            Product::VipMonth => shop::VIP_MONTH,
        }
    }

    /// The coins it brings.
    pub fn coins(self) -> i64 {
        match self {
            Product::Coins(pack) => (pack.coins + pack.bonus) as i64,
            Product::VipMonth => shop::VIP_MONTHLY_COINS as i64,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MsStoreConfig {
    pub tenant_id: String,
    pub client_id: String,
    pub client_secret: String,
    /// Each product sold in the Store and its add-on's Store ID.
    pub products: Vec<(Product, String)>,
    /// Microsoft's addresses (tests point them at a local stand-in).
    pub login_url: String,
    pub collections_url: String,
}

impl MsStoreConfig {
    /// From the environment; `None` (no Store purchases) if anything is
    /// missing or the product list has a mistake (a wrong mapping would
    /// credit the wrong pack, so nothing is taken rather than guessing).
    pub fn from_env() -> Option<MsStoreConfig> {
        let var =
            |k: &str| std::env::var(k).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let products = var("SNOOK_LEGEND_MSSTORE_PRODUCTS")?;
        let products = match parse_products(&products) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("SNOOK_LEGEND_MSSTORE_PRODUCTS: {e}; Store purchases are off");
                return None;
            }
        };
        Some(MsStoreConfig {
            tenant_id: var("SNOOK_LEGEND_MSSTORE_TENANT_ID")?,
            client_id: var("SNOOK_LEGEND_MSSTORE_CLIENT_ID")?,
            client_secret: var("SNOOK_LEGEND_MSSTORE_CLIENT_SECRET")?,
            products,
            login_url: LOGIN_URL.into(),
            collections_url: COLLECTIONS_URL.into(),
        })
    }

    /// The product sold as the add-on `store_id`.
    pub fn product_of(&self, store_id: &str) -> Option<Product> {
        self.products.iter().find(|(_, id)| id.eq_ignore_ascii_case(store_id)).map(|(p, _)| *p)
    }

    /// The coin packs as offered to the game.
    pub fn offers(&self) -> Vec<StorePack> {
        self.products
            .iter()
            .filter_map(|(p, id)| match p {
                Product::Coins(pack) => Some(StorePack {
                    code: pack.code.to_string(),
                    coins: pack.coins,
                    bonus: pack.bonus,
                    store_id: id.clone(),
                }),
                Product::VipMonth => None,
            })
            .collect()
    }

    /// The Store ID of the month of VIP (empty: not sold in the Store).
    pub fn vip_offer(&self) -> String {
        self.products
            .iter()
            .find(|(p, _)| *p == Product::VipMonth)
            .map(|(_, id)| id.clone())
            .unwrap_or_default()
    }

    /// The game's products in a page of the collections query: active,
    /// developer-managed consumables that are still to be received.
    fn products_in(&self, reply: &Value) -> Vec<Owned> {
        let Some(items) = reply["items"].as_array() else { return Vec::new() };
        items
            .iter()
            .filter_map(|item| {
                let product_id = item["productId"].as_str()?;
                let product = self.product_of(product_id)?;
                let kind = item["productKind"].as_str().unwrap_or("");
                if !kind.eq_ignore_ascii_case(UNMANAGED_CONSUMABLE) {
                    tracing::warn!(
                        product = product_id,
                        kind,
                        "Store add-on of the wrong kind; not credited"
                    );
                    return None;
                }
                let active =
                    item["status"].as_str().is_none_or(|s| s.eq_ignore_ascii_case("Active"));
                let quantity = item["quantity"].as_i64().unwrap_or(1);
                if !active || quantity < 1 {
                    return None;
                }
                let Some(transaction_id) = purchase_id(item) else {
                    tracing::error!(
                        product = product_id,
                        "Store purchase without a transaction id; skipped"
                    );
                    return None;
                };
                Some(Owned { product_id: product_id.to_string(), product, transaction_id })
            })
            .collect()
    }
}

/// What identifies one purchase: its transaction id (the item id and the
/// time it was bought if there is none).
fn purchase_id(item: &Value) -> Option<String> {
    let blank = |s: &&str| s.is_empty() || s.chars().all(|c| c == '0' || c == '-');
    if let Some(t) = item["transactionId"].as_str().filter(|s| !blank(s)) {
        return Some(t.to_ascii_lowercase());
    }
    // Without one: the item and what tells its purchases apart (the order
    // line, else when it was bought and last changed).
    let id = item["id"].as_str().filter(|s| !s.is_empty())?;
    let field = |k: &str| item[k].as_str().filter(|s| !s.is_empty());
    let when = match field("orderLineItemId").or(field("orderId")) {
        Some(order) => order.to_ascii_lowercase(),
        None => format!("{}/{}", field("acquiredDate")?, field("modifiedDate").unwrap_or("")),
    };
    Some(format!("item:{id}@{when}"))
}

/// `product=StoreID;product=StoreID`: every product a coin pack of the game
/// or `vip_month`, every Store ID twelve letters and digits, nothing twice.
pub fn parse_products(text: &str) -> Result<Vec<(Product, String)>, String> {
    let mut out: Vec<(Product, String)> = Vec::new();
    for entry in text.split(';').map(str::trim).filter(|e| !e.is_empty()) {
        let (code, id) = entry.split_once('=').ok_or_else(|| format!("`{entry}` has no `=`"))?;
        let (code, id) = (code.trim(), id.trim().to_ascii_uppercase());
        let product = Product::parse(code)
            .ok_or_else(|| format!("`{code}` is neither a coin pack nor `{}`", shop::VIP_MONTH))?;
        if id.len() != 12 || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(format!("`{id}` is not a Store ID (12 letters and digits)"));
        }
        if out.iter().any(|(p, i)| *p == product || *i == id) {
            return Err(format!("`{entry}` repeats a product or a Store ID"));
        }
        out.push((product, id));
    }
    if out.is_empty() {
        return Err("no products".into());
    }
    Ok(out)
}

/// A product bought and not yet received.
#[derive(Debug, Clone, PartialEq)]
struct Owned {
    product_id: String,
    product: Product,
    transaction_id: String,
}

/// What purchases credited.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Credited {
    coins: i64,
    /// A month of VIP was among them.
    vip: bool,
}

impl Credited {
    /// Something was credited.
    fn any(self) -> bool {
        self.coins > 0 || self.vip
    }
}

impl std::ops::AddAssign for Credited {
    fn add_assign(&mut self, other: Credited) {
        self.coins += other.coins;
        self.vip |= other.vip;
    }
}

/// Microsoft's answer to a consume request.
#[derive(Debug, Clone, PartialEq)]
enum Consumed {
    /// Consumed (the order ids, for refunds).
    Done(String),
    /// Refused: nothing was consumed.
    Refused(String),
    /// No answer that can be trusted: send the same request again.
    NoAnswer(String),
}

pub struct MsStore {
    pub cfg: MsStoreConfig,
    #[cfg(feature = "payments")]
    http: reqwest::Client,
    /// Entra tokens by audience, until shortly before they expire.
    tokens: tokio::sync::Mutex<HashMap<&'static str, (String, Instant)>>,
    /// Players whose purchases are being checked.
    checking: Mutex<HashSet<String>>,
}

impl MsStore {
    pub fn new(cfg: MsStoreConfig) -> MsStore {
        #[cfg(feature = "payments")]
        {
            // TLS with ring, like the database and Stripe.
            let _ = rustls::crypto::ring::default_provider().install_default();
            let http = reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .expect("http client");
            MsStore { cfg, http, tokens: Default::default(), checking: Default::default() }
        }
        #[cfg(not(feature = "payments"))]
        MsStore { cfg, tokens: Default::default(), checking: Default::default() }
    }

    /// The anonymous id that names a player to the Store (in their keys):
    /// an HMAC of the account id, so it says nothing about who they are. Its
    /// key is the application's identity, which (unlike the client secret)
    /// never changes, so a player's id stays the same.
    pub fn publisher_user_id(&self, user_id: i64) -> String {
        let key = format!("{}/{}", self.cfg.tenant_id, self.cfg.client_id);
        let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("HMAC takes any key");
        mac.update(b"snook-legend/msstore/user:");
        mac.update(user_id.to_string().as_bytes());
        mac.finalize().into_bytes()[..16].iter().map(|b| format!("{b:02x}")).collect()
    }

    /// An Entra access token for `audience` (cached).
    async fn token(&self, audience: &'static str) -> Result<String, String> {
        let mut cache = self.tokens.lock().await;
        if let Some((token, until)) = cache.get(audience)
            && Instant::now() < *until
        {
            return Ok(token.clone());
        }
        let url = format!(
            "{}/{}/oauth2/v2.0/token",
            self.cfg.login_url.trim_end_matches('/'),
            self.cfg.tenant_id
        );
        let form = [
            ("grant_type", "client_credentials".to_string()),
            ("client_id", self.cfg.client_id.clone()),
            ("client_secret", self.cfg.client_secret.clone()),
            ("scope", format!("{audience}/.default")),
        ];
        let (status, reply) = self.post_form(&url, &form).await?;
        if !(200..300).contains(&status) {
            return Err(format!("sign-in to Microsoft refused ({status}): {}", describe(&reply)));
        }
        let token = reply["access_token"]
            .as_str()
            .filter(|t| !t.is_empty())
            .ok_or("sign-in to Microsoft gave no token")?
            .to_string();
        let secs = reply["expires_in"]
            .as_u64()
            .or_else(|| reply["expires_in"].as_str().and_then(|s| s.parse().ok()))
            .unwrap_or(3600);
        // Renewed five minutes early (never kept less than a minute).
        let keep = Duration::from_secs(secs.saturating_sub(300).max(60));
        cache.insert(audience, (token.clone(), Instant::now() + keep));
        Ok(token)
    }

    /// Forgets the tokens when Microsoft says ours is no good (not for any
    /// 401: a made-up player key gets one too, and must not make every
    /// player wait for a new sign-in).
    async fn forget_tokens_if_refused(&self, status: u16, reply: &Value) {
        let code = reply["innererror"]["code"].as_str().or_else(|| reply["code"].as_str());
        if status == 401
            && matches!(code, Some("AuthenticationTokenInvalid" | "PartnerAadTicketRequired"))
        {
            self.tokens.lock().await.clear();
        }
    }

    /// The ticket the game hands to the Store to make a collections key.
    pub async fn ticket(&self) -> Result<String, String> {
        self.token(COLLECTIONS_AUDIENCE).await
    }

    /// The game's products that the key's Microsoft account bought and has
    /// not received yet.
    async fn owned(&self, key: &str, user_ref: &str) -> Result<Vec<Owned>, String> {
        let token = self.token(SERVICE_AUDIENCE).await?;
        let url = format!("{}{QUERY_PATH}", self.cfg.collections_url.trim_end_matches('/'));
        let mut out = Vec::new();
        let mut continuation: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut body = json!({
                "beneficiaries": [{
                    "identityType": "b2b",
                    "identityValue": key,
                    "localTicketReference": user_ref,
                }],
                "maxPageSize": 100,
                "excludeDuplicates": true,
                "validityType": "Valid",
                "market": "neutral",
                "expandSatisfyingItems": false,
            });
            if let Some(c) = &continuation {
                body["continuationToken"] = json!(c);
            }
            let (status, reply) = self.post_json(&url, &token, &body).await?;
            if !(200..300).contains(&status) {
                self.forget_tokens_if_refused(status, &reply).await;
                return Err(format!("collections query refused ({status}): {}", describe(&reply)));
            }
            out.extend(self.cfg.products_in(&reply));
            continuation =
                reply["continuationToken"].as_str().filter(|c| !c.is_empty()).map(str::to_string);
            if continuation.is_none() {
                return Ok(out);
            }
        }
        // The rest is found by the next check.
        tracing::warn!("a Store collection longer than {MAX_PAGES} pages: read in part");
        Ok(out)
    }

    /// Asks Microsoft to consume one purchase.
    async fn consume(&self, c: &MsConsume) -> Consumed {
        // Without a token nothing was sent: try again later.
        let token = match self.token(SERVICE_AUDIENCE).await {
            Ok(t) => t,
            Err(e) => return Consumed::NoAnswer(e),
        };
        let url = format!("{}{CONSUME_PATH}", self.cfg.collections_url.trim_end_matches('/'));
        // A developer-managed consumable is always one: no `removeQuantity`.
        let body = json!({
            "beneficiary": {
                "identityType": "b2b",
                "identityValue": c.store_key,
                "localTicketReference": self.publisher_user_id(c.user_id),
            },
            "productId": c.product_id,
            "trackingId": c.tracking_id,
            "includeOrderIds": true,
        });
        match self.post_json(&url, &token, &body).await {
            Err(e) => Consumed::NoAnswer(e),
            Ok((status, reply)) => {
                self.forget_tokens_if_refused(status, &reply).await;
                classify(status, &reply, c)
            }
        }
    }

    #[cfg(feature = "payments")]
    async fn post_json(
        &self,
        url: &str,
        token: &str,
        body: &Value,
    ) -> Result<(u16, Value), String> {
        let res = self
            .http
            .post(url)
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .map_err(|e| format!("Microsoft unreachable: {e}"))?;
        Ok(read(res).await)
    }

    #[cfg(feature = "payments")]
    async fn post_form(&self, url: &str, form: &[(&str, String)]) -> Result<(u16, Value), String> {
        let res = self
            .http
            .post(url)
            .form(form)
            .send()
            .await
            .map_err(|e| format!("Microsoft unreachable: {e}"))?;
        Ok(read(res).await)
    }

    #[cfg(not(feature = "payments"))]
    async fn post_json(&self, _: &str, _: &str, _: &Value) -> Result<(u16, Value), String> {
        Err("this build takes no payments".into())
    }

    #[cfg(not(feature = "payments"))]
    async fn post_form(&self, _: &str, _: &[(&str, String)]) -> Result<(u16, Value), String> {
        Err("this build takes no payments".into())
    }

    /// Marks `id` as taken in `set` until the guard is dropped; `None` if it
    /// already was.
    fn claim<'a>(set: &'a Mutex<HashSet<String>>, id: String) -> Option<Claim<'a>> {
        let added = set.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone());
        added.then_some(Claim { set, id })
    }
}

/// See [`MsStore::claim`].
struct Claim<'a> {
    set: &'a Mutex<HashSet<String>>,
    id: String,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.set.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.id);
    }
}

/// A reply's status and JSON body (`Null` when it has none).
#[cfg(feature = "payments")]
async fn read(res: reqwest::Response) -> (u16, Value) {
    let status = res.status().as_u16();
    let body = res.bytes().await.ok().and_then(|b| serde_json::from_slice(&b).ok());
    (status, body.unwrap_or(Value::Null))
}

/// Microsoft's answer to a consume request, read cautiously: only a success
/// about this very request counts as consumed. A refusal (4xx) of the
/// *first* request means nothing was consumed; after an earlier request
/// went without an answer, a refusal proves nothing (that one may have
/// consumed the purchase), so it stays unknown and is sent again, like any
/// other doubt (no answer, 5xx, a conflict, an expired or missing token, too
/// many requests). `c.attempts` counts this request.
fn classify(status: u16, reply: &Value, c: &MsConsume) -> Consumed {
    match status {
        200..=299 => {
            // A success that names another request confirms nothing.
            let same = |field: &str, ours: &str| {
                reply[field].as_str().is_none_or(|theirs| theirs.eq_ignore_ascii_case(ours))
            };
            if !same("productId", &c.product_id) || !same("trackingId", &c.tracking_id) {
                let reply: String = reply.to_string().chars().take(300).collect();
                return Consumed::NoAnswer(format!("a reply about another request: {reply}"));
            }
            let orders: Vec<&str> = reply["orderTransactions"]
                .as_array()
                .map(|list| list.iter().filter_map(|o| o["orderId"].as_str()).collect())
                .unwrap_or_default();
            Consumed::Done(orders.join(","))
        }
        401 | 403 | 408 | 409 | 429 => Consumed::NoAnswer(format!("{status}: {}", describe(reply))),
        400..=499 if c.attempts <= 1 => Consumed::Refused(format!("{status}: {}", describe(reply))),
        400..=499 => Consumed::NoAnswer(format!("{status} on a resend: {}", describe(reply))),
        _ => Consumed::NoAnswer(format!("{status}: {}", describe(reply))),
    }
}

/// The useful part of an error reply.
fn describe(reply: &Value) -> String {
    let inner = &reply["innererror"];
    let pick = |v: &Value| {
        let code = v["code"].as_str().or_else(|| v["error"].as_str()).unwrap_or("");
        let msg = v["message"].as_str().or_else(|| v["error_description"].as_str()).unwrap_or("");
        (!code.is_empty() || !msg.is_empty()).then(|| format!("{code} {msg}").trim().to_string())
    };
    let text = pick(inner).or_else(|| pick(reply)).unwrap_or_else(|| reply.to_string());
    text.chars().take(300).collect()
}

/// Checks a Microsoft Store ID key before it is used: a collections key,
/// not expired, made for `user_ref`. The signature is not checked here:
/// Microsoft checks it on every call, so a key with forged claims gets
/// nothing. What this stops is a key made for another player being used
/// from this player's account.
pub fn check_key(key: &str, user_ref: &str, now: i64) -> Result<(), &'static str> {
    if key.is_empty() || key.len() > MAX_KEY_BYTES {
        return Err("bad key size");
    }
    let mut parts = key.split('.');
    let (Some(_), Some(payload), Some(_), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err("not a JWT");
    };
    let claims: Value = base64url(payload)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or("unreadable claims")?;
    if claims["aud"].as_str() != Some(KEY_AUDIENCE) {
        return Err("not a collections key");
    }
    if claims["exp"].as_i64().is_some_and(|exp| exp <= now) {
        return Err("expired key");
    }
    if claims[USER_ID_CLAIM].as_str() != Some(user_ref) {
        return Err("key made for someone else");
    }
    Ok(())
}

/// Decodes unpadded base64url (padding tolerated).
fn base64url(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// A new GUID (version 4) for a consume request.
fn new_tracking_id() -> String {
    use rand::RngCore;
    let mut b = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// When to send a request without an answer again: 1, 2, 4… minutes, at
/// most every six hours.
fn retry_delay(attempts: i32) -> i64 {
    (60i64 << attempts.clamp(0, 9)).min(6 * 3600)
}

/// The Store messages of a signed-in player.
pub async fn handle(server: &Arc<Server>, me: &User, msg: ClientMsg, send: &impl Fn(ServerMsg)) {
    let Some(ms) = &server.msstore else {
        return send(redeemed(0, false, "Purchases are not available."));
    };
    match msg {
        ClientMsg::StoreTicket => match ms.ticket().await {
            Ok(ticket) => {
                send(ServerMsg::StoreTicket { ticket, user_id: ms.publisher_user_id(me.id) })
            }
            Err(e) => {
                tracing::warn!("Store ticket for {}: {e}", me.username);
                send(redeemed(0, false, UNREACHABLE));
            }
        },
        ClientMsg::StoreRedeem { key } => send(redeem(server, ms, me, key.trim()).await),
        _ => {}
    }
}

fn redeemed(coins: i64, pending: bool, error: &str) -> ServerMsg {
    ServerMsg::StoreRedeemed { coins, pending, error: error.to_string(), vip_until: 0 }
}

/// What a player is told about `got`: with when VIP ends, when a month of
/// VIP was among it.
async fn credited_msg(
    server: &Server,
    user_id: i64,
    got: Credited,
    pending: bool,
    error: &str,
) -> ServerMsg {
    let vip_until = match got.vip {
        true => server.store.wallet(user_id).await.map(|w| w.vip_until).unwrap_or(0),
        false => 0,
    };
    ServerMsg::StoreRedeemed { coins: got.coins, pending, error: error.to_string(), vip_until }
}

/// Finds and credits what `me` bought in the Store (step 4 above).
async fn redeem(server: &Arc<Server>, ms: &MsStore, me: &User, key: &str) -> ServerMsg {
    let user_ref = ms.publisher_user_id(me.id);
    if let Err(why) = check_key(key, &user_ref, now()) {
        tracing::warn!(user = me.id, "Store key refused: {why}");
        return redeemed(0, false, NOT_CONFIRMED);
    }
    // One check per player at a time; the one running answers for both.
    let Some(_checking) = MsStore::claim(&ms.checking, me.id.to_string()) else {
        return redeemed(0, true, "");
    };
    let mut got = Credited::default();
    // Requests still without an answer first: one of them may be the
    // purchase the collection no longer shows.
    match server.store.ms_pending_of(me.id).await {
        Ok(rows) => {
            for row in rows {
                got += lease_and_settle(server, ms, &row.tracking_id).await;
            }
        }
        Err(e) => {
            tracing::error!("Store purchases of {}: {e}", me.username);
            return redeemed(0, false, UNREACHABLE);
        }
    }
    let owned = ms.owned(key, &user_ref).await;
    if let Ok(owned) = &owned {
        for item in owned {
            let c = MsConsume {
                tracking_id: new_tracking_id(),
                user_id: me.id,
                product_id: item.product_id.clone(),
                pack: item.product.code().to_string(),
                coins: item.product.coins(),
                transaction_id: item.transaction_id.clone(),
                store_key: key.to_string(),
                // The request about to go out.
                attempts: 1,
                created_at: now(),
            };
            // Saved (and leased to us) before it is sent, or not sent at all.
            match server.store.ms_open(&c).await {
                Ok(true) => {
                    tracing::info!(user = me.id, pack = c.pack, "Store purchase found");
                    got += settle(server, ms, &c).await;
                }
                // Being received already (or received).
                Ok(false) => {}
                Err(e) => tracing::error!("saving a Store purchase of {}: {e}", me.username),
            }
        }
    }
    let pending = !server.store.ms_pending_of(me.id).await.unwrap_or_default().is_empty();
    if got.any() {
        crate::shop::push_wallet(server, me).await;
    }
    match owned {
        Err(e) => {
            tracing::warn!("Store collection of {}: {e}", me.username);
            let error = if got.any() { "" } else { UNREACHABLE };
            credited_msg(server, me.id, got, pending, error).await
        }
        Ok(_) => credited_msg(server, me.id, got, pending, "").await,
    }
}

/// Takes a pending purchase (if no one else is sending it) and sends it.
async fn lease_and_settle(server: &Server, ms: &MsStore, tracking_id: &str) -> Credited {
    match server.store.ms_lease(tracking_id, now()).await {
        Ok(Some(c)) => settle(server, ms, &c).await,
        // Settled, or being sent by someone else right now.
        Ok(None) => Credited::default(),
        Err(e) => {
            tracing::error!(tracking = tracking_id, "leasing a Store purchase: {e}");
            Credited::default()
        }
    }
}

/// Sends the consume request of a purchase leased to us (`c.attempts`
/// counts it) and acts on the answer. Returns what was credited now.
async fn settle(server: &Server, ms: &MsStore, c: &MsConsume) -> Credited {
    let t = now();
    let nothing = Credited::default();
    if c.store_key.is_empty() {
        let _ = server.store.ms_close(&c.tracking_id, MsState::Stuck, "no key to send", t).await;
        return nothing;
    }
    match ms.consume(c).await {
        Consumed::Done(orders) => match server.store.ms_fulfil(&c.tracking_id, &orders, t).await {
            Ok(true) => {
                tracing::info!(
                    user = c.user_id,
                    pack = c.pack,
                    coins = c.coins,
                    "Store purchase credited"
                );
                Credited { coins: c.coins, vip: c.pack == shop::VIP_MONTH }
            }
            // Only when it was credited already (by another server).
            Ok(false) => {
                tracing::warn!(tracking = c.tracking_id, "Store purchase confirmed again");
                nothing
            }
            // Still pending: the next try gets the same answer and credits.
            Err(e) => {
                tracing::error!(tracking = c.tracking_id, "crediting a Store purchase: {e}");
                nothing
            }
        },
        Consumed::Refused(why) => {
            let why: String = why.chars().take(500).collect();
            tracing::warn!(
                user = c.user_id,
                tracking = c.tracking_id,
                "Store consume refused: {why}"
            );
            let _ = server.store.ms_close(&c.tracking_id, MsState::Failed, &why, t).await;
            nothing
        }
        Consumed::NoAnswer(why) => {
            let why: String = why.chars().take(500).collect();
            if t - c.created_at > GIVE_UP_AFTER {
                tracing::error!(
                    user = c.user_id,
                    tracking = c.tracking_id,
                    "Store purchase without an answer for a week (stuck): {why}"
                );
                let _ = server.store.ms_close(&c.tracking_id, MsState::Stuck, &why, t).await;
            } else {
                tracing::warn!(tracking = c.tracking_id, "Store consume without an answer: {why}");
                let next = t + retry_delay(c.attempts);
                let _ = server.store.ms_retry_later(&c.tracking_id, &why, next).await;
            }
            nothing
        }
    }
}

/// Sends requests without an answer again, now and then (they also go out
/// whenever their player checks).
pub async fn run(server: Arc<Server>) {
    if server.msstore.is_none() {
        return;
    }
    let mut tick = tokio::time::interval(RETRY_EVERY);
    loop {
        tick.tick().await;
        retry_due(&server, now()).await;
    }
}

/// Sends again the requests whose next try is due at `at`, and tells the
/// players who got something.
pub async fn retry_due(server: &Arc<Server>, at: i64) {
    let Some(ms) = &server.msstore else { return };
    let due = match server.store.ms_due(at, 25).await {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("Store purchases to retry: {e}");
            return;
        }
    };
    let mut credited: HashMap<i64, Credited> = HashMap::new();
    for c in due {
        let got = lease_and_settle(server, ms, &c.tracking_id).await;
        if got.any() {
            *credited.entry(c.user_id).or_default() += got;
        }
    }
    for (user_id, got) in credited {
        if let Ok(Some(user)) = server.store.user_by_id(user_id).await {
            let msg = credited_msg(server, user_id, got, false, "").await;
            server.with(|l| l.notify(&user.username, msg));
            crate::shop::push_wallet(server, &user).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(bytes: &[u8]) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let n =
                chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
            for i in 0..=chunk.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            }
        }
        out
    }

    pub(crate) fn key_for(user_ref: &str, aud: &str, exp: i64) -> String {
        let claims = json!({ "aud": aud, "exp": exp, USER_ID_CLAIM: user_ref });
        format!("{}.{}.{}", b64(br#"{"alg":"RS256"}"#), b64(claims.to_string().as_bytes()), "sig")
    }

    #[test]
    fn base64url_round_trip() {
        for text in ["", "a", "ab", "abc", "abcd", "{\"x\":\"ü?>\"}"] {
            assert_eq!(base64url(&b64(text.as_bytes())).unwrap(), text.as_bytes());
        }
        assert_eq!(base64url("aGk=").unwrap(), b"hi");
        assert!(base64url("a b").is_none());
    }

    #[test]
    fn keys_are_checked() {
        let good = key_for("abc", KEY_AUDIENCE, 2000);
        assert_eq!(check_key(&good, "abc", 1000), Ok(()));
        assert!(check_key(&good, "other", 1000).is_err(), "made for someone else");
        assert!(check_key(&good, "abc", 2000).is_err(), "expired");
        let purchase = key_for("abc", "https://purchase.mp.microsoft.com/v6.0/keys", 2000);
        assert!(check_key(&purchase, "abc", 1000).is_err(), "not a collections key");
        assert!(check_key("abc", "abc", 1000).is_err());
        assert!(check_key("a.!!.c", "abc", 1000).is_err());
        assert!(check_key(&"a".repeat(MAX_KEY_BYTES + 1), "abc", 1000).is_err());
    }

    #[test]
    fn product_lists() {
        let p = parse_products("coins_500=9nblggh42cfd; coins_1200 = 9NBLGGH42CFE ;").unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].0.code(), "coins_500");
        assert_eq!(p[0].1, "9NBLGGH42CFD");
        let p = parse_products("vip_month=9NBLGGH42CFF").unwrap();
        assert_eq!(p, [(Product::VipMonth, "9NBLGGH42CFF".to_string())]);
        assert!(parse_products("").is_err());
        assert!(parse_products("coins_1=9NBLGGH42CFD").is_err(), "not a pack");
        assert!(parse_products("vip_monthly=9NBLGGH42CFD").is_err(), "the Stripe plan");
        assert!(parse_products("vip_month=9NBLGGH42CFD;vip_month=9NBLGGH42CFE").is_err());
        assert!(parse_products("coins_500=9NBLGGH42").is_err(), "short id");
        assert!(parse_products("coins_500=9NBLGGH42CF!").is_err());
        assert!(parse_products("coins_500").is_err());
        assert!(parse_products("coins_500=9NBLGGH42CFD;coins_500=9NBLGGH42CFE").is_err());
        assert!(parse_products("coins_500=9NBLGGH42CFD;coins_1200=9NBLGGH42CFD").is_err());
    }

    fn config() -> MsStoreConfig {
        MsStoreConfig {
            tenant_id: "t".into(),
            client_id: "c".into(),
            client_secret: "s".into(),
            products: parse_products(
                "coins_500=9AAAAAAAAAAA;coins_1200=9BBBBBBBBBBB;vip_month=9VVVVVVVVVVV",
            )
            .unwrap(),
            login_url: LOGIN_URL.into(),
            collections_url: COLLECTIONS_URL.into(),
        }
    }

    #[test]
    fn only_the_games_products_are_taken_from_a_collection() {
        let cfg = config();
        let item = |id: &str, kind: &str, status: &str, tx: &str| {
            json!({ "productId": id, "productKind": kind, "status": status,
                    "quantity": 1, "transactionId": tx, "id": "item1",
                    "acquiredDate": "2026-11-16T10:00:00Z" })
        };
        let reply = json!({ "items": [
            item("9AAAAAAAAAAA", "UnmanagedConsumable", "Active", "T1"),
            item("9BBBBBBBBBBB", "UnmanagedConsumable", "Revoked", "T2"),
            item("9CCCCCCCCCCC", "UnmanagedConsumable", "Active", "T3"),
            item("9BBBBBBBBBBB", "Durable", "Active", "T4"),
            item("9BBBBBBBBBBB", "UnmanagedConsumable", "Active", "00000000-0000-0000-0000-000000000000"),
            item("9VVVVVVVVVVV", "UnmanagedConsumable", "Active", "T5"),
            item("9VVVVVVVVVVV", "Consumable", "Active", "T6"),
        ]});
        let owned = cfg.products_in(&reply);
        assert_eq!(owned.len(), 3);
        assert_eq!(owned[0].product.code(), "coins_500");
        assert_eq!(owned[0].transaction_id, "t1");
        assert_eq!(owned[1].product.code(), "coins_1200");
        assert_eq!(owned[1].transaction_id, "item:item1@2026-11-16T10:00:00Z/");
        assert_eq!(owned[2].product, Product::VipMonth);
        assert_eq!(owned[2].product.coins(), shop::VIP_MONTHLY_COINS as i64);
        assert!(cfg.products_in(&json!({})).is_empty());
    }

    /// The game sees the coin packs as packs and the month of VIP apart
    /// (a game from before it never shows it as a pack of coins).
    #[test]
    fn the_month_of_vip_is_offered_apart() {
        let cfg = config();
        let packs: Vec<String> = cfg.offers().into_iter().map(|p| p.code).collect();
        assert_eq!(packs, ["coins_500", "coins_1200"]);
        assert_eq!(cfg.vip_offer(), "9VVVVVVVVVVV");
        let coins_only = MsStoreConfig {
            products: parse_products("coins_500=9AAAAAAAAAAA").unwrap(),
            ..config()
        };
        assert_eq!(coins_only.vip_offer(), "");
    }

    fn consume_row() -> MsConsume {
        MsConsume {
            tracking_id: "1b4e28ba-2fa1-41d2-883f-0016d3cca427".into(),
            user_id: 1,
            product_id: "9AAAAAAAAAAA".into(),
            pack: "coins_500".into(),
            coins: 500,
            transaction_id: "t1".into(),
            store_key: "k".into(),
            attempts: 1,
            created_at: 0,
        }
    }

    #[test]
    fn answers_are_read_cautiously() {
        let c = consume_row();
        let ok = json!({ "productId": "9aaaaaaaaaaa", "trackingId": c.tracking_id.to_uppercase(),
            "orderTransactions": [{ "orderId": "o1" }, { "orderId": "o2" }] });
        assert_eq!(classify(200, &ok, &c), Consumed::Done("o1,o2".into()));
        assert_eq!(classify(204, &Value::Null, &c), Consumed::Done(String::new()));
        let other = json!({ "productId": "9BBBBBBBBBBB" });
        assert!(matches!(classify(200, &other, &c), Consumed::NoAnswer(_)));
        let other = json!({ "trackingId": "another" });
        assert!(matches!(classify(200, &other, &c), Consumed::NoAnswer(_)));
        let refusal = json!({ "innererror": { "code": "InsufficientQuantity", "message": "no" } });
        assert!(
            matches!(classify(400, &refusal, &c), Consumed::Refused(e) if e.contains("InsufficientQuantity"))
        );
        for status in [401, 403, 408, 409, 429, 500, 503] {
            assert!(
                matches!(classify(status, &Value::Null, &c), Consumed::NoAnswer(_)),
                "{status}"
            );
        }
        // A resend that is refused proves nothing about the first request.
        let resent = MsConsume { attempts: 2, ..c.clone() };
        assert!(matches!(classify(400, &refusal, &resent), Consumed::NoAnswer(_)));
    }

    #[test]
    fn tracking_ids_are_guids() {
        let a = new_tracking_id();
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
        assert!("89ab".contains(&a[19..20]));
        assert_ne!(a, new_tracking_id());
    }

    #[test]
    fn publisher_ids_are_stable_and_anonymous() {
        let ms = MsStore::new(config());
        assert_eq!(ms.publisher_user_id(7), ms.publisher_user_id(7));
        assert_ne!(ms.publisher_user_id(7), ms.publisher_user_id(8));
        assert_eq!(ms.publisher_user_id(7).len(), 32);
    }

    #[test]
    fn retries_slow_down() {
        assert_eq!(retry_delay(0), 60);
        assert_eq!(retry_delay(1), 120);
        assert_eq!(retry_delay(30), 6 * 3600);
        assert_eq!(retry_delay(-1), 60);
    }
}
