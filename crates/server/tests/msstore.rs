//! Coin packs bought in the Microsoft Store, end to end: the game's messages
//! over a WebSocket, and a stand-in for Microsoft's sign-in and collections
//! services that behaves as their documentation says (the same tracking id
//! always gets the same answer; a developer-managed consumable can only be
//! consumed once) and can lose answers or fail on purpose.
//!
//! The store tests run on the in-memory store always, and on PostgreSQL when
//! `SNOOK_LEGEND_TEST_DATABASE_URL` points at a database that may be wiped.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use snook_legend_proto::{ClientMsg, PROTOCOL_VERSION, Role, Rules, ServerMsg, parse_server};
use snook_legend_server::lounge::Settings;
use snook_legend_server::msstore::{self, MsStoreConfig, parse_products};
use snook_legend_server::store::{MsConsume, MsState, NewUser, Store};
use snook_legend_server::{Config, Server, now, serve};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

const PASSWORD: &str = "secret123";
const SECRET: &str = "entra-secret";
const PACK_500: &str = "9AAAAAAAAAAA";
const PACK_1200: &str = "9BBBBBBBBBBB";
const KEY_AUDIENCE: &str = "https://collections.mp.microsoft.com/v6.0/keys";
const USER_ID_CLAIM: &str = "http://schemas.microsoft.com/marketplace/2015/08/claims/key/userId";

/// What the stand-in does with the next consume request.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Fault {
    /// Consumes, then the answer is lost (a 502 instead).
    LoseAnswer,
    /// Fails before doing anything (a 503).
    Unavailable,
    /// Refuses the request (a 400): nothing consumed.
    Refuse,
    /// Answers 409 Conflict without doing anything.
    Conflict,
}

#[derive(Debug, Clone)]
struct Purchase {
    account: String,
    product: String,
    transaction: String,
    consumed: bool,
}

#[derive(Default)]
struct Microsoft {
    purchases: Vec<Purchase>,
    /// tracking id -> (status, reply): the same answer every time.
    answers: HashMap<String, (u16, Value)>,
    faults: VecDeque<Fault>,
    /// Purchases actually consumed, and consume requests received.
    consumed: u32,
    consume_calls: u32,
    token_calls: u32,
    query_calls: u32,
}

type Fake = Arc<Mutex<Microsoft>>;

impl Microsoft {
    fn buy(&mut self, account: &str, product: &str) -> String {
        let transaction = format!("{:08x}-0000-4000-8000-{:012x}", self.purchases.len() + 1, 7);
        self.purchases.push(Purchase {
            account: account.into(),
            product: product.into(),
            transaction: transaction.clone(),
            consumed: false,
        });
        transaction
    }
}

/// The Microsoft account a key stands for (the stand-in's own claim).
fn account_of(key: &str) -> Option<String> {
    let payload = key.split('.').nth(1)?;
    let bytes = decode(payload)?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    claims["acct"].as_str().map(str::to_string)
}

fn bearer_ok(headers: &HeaderMap) -> bool {
    headers.get("authorization").and_then(|v| v.to_str().ok())
        == Some("Bearer token:https://onestore.microsoft.com/.default")
}

async fn token(State(fake): State<Fake>, body: String) -> Response {
    fake.lock().unwrap().token_calls += 1;
    let form: HashMap<String, String> = body
        .split('&')
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.to_string(), urldecode(v)))
        .collect();
    if form.get("client_secret").map(String::as_str) != Some(SECRET)
        || form.get("grant_type").map(String::as_str) != Some("client_credentials")
    {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "invalid_client" })))
            .into_response();
    }
    let scope = form.get("scope").cloned().unwrap_or_default();
    Json(json!({ "access_token": format!("token:{scope}"), "expires_in": 3599 })).into_response()
}

async fn query(State(fake): State<Fake>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    if !bearer_ok(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut ms = fake.lock().unwrap();
    ms.query_calls += 1;
    let key = body["beneficiaries"][0]["identityValue"].as_str().unwrap_or("");
    let Some(account) = account_of(key) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let items: Vec<Value> = ms
        .purchases
        .iter()
        .filter(|p| p.account == account && !p.consumed)
        .map(|p| {
            json!({
                "productId": p.product, "productKind": "UnmanagedConsumable",
                "status": "Active", "quantity": 1, "transactionId": p.transaction,
                "id": format!("item-{}", p.product), "acquiredDate": "2026-11-16T10:00:00Z",
            })
        })
        .collect();
    Json(json!({ "items": items })).into_response()
}

async fn consume(
    State(fake): State<Fake>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if !bearer_ok(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut ms = fake.lock().unwrap();
    ms.consume_calls += 1;
    let tracking = body["trackingId"].as_str().unwrap_or("").to_string();
    let product = body["productId"].as_str().unwrap_or("").to_string();
    assert!(body.get("removeQuantity").is_none(), "unmanaged consumables take no quantity");
    let fault = ms.faults.pop_front();
    if fault == Some(Fault::Unavailable) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if fault == Some(Fault::Conflict) {
        return StatusCode::CONFLICT.into_response();
    }
    if fault == Some(Fault::Refuse) {
        let e = json!({ "innererror": { "code": "Testing", "message": "refused on purpose" } });
        return (StatusCode::BAD_REQUEST, Json(e)).into_response();
    }
    // The same tracking id: the same answer, whatever happened since.
    let (status, reply) = match ms.answers.get(&tracking) {
        Some(answer) => answer.clone(),
        None => {
            let key = body["beneficiary"]["identityValue"].as_str().unwrap_or("");
            let account = account_of(key).unwrap_or_default();
            let found = ms
                .purchases
                .iter()
                .position(|p| p.account == account && p.product == product && !p.consumed);
            let answer = match found {
                Some(i) => {
                    ms.purchases[i].consumed = true;
                    ms.consumed += 1;
                    let order = format!("order-{}", ms.purchases[i].transaction);
                    (
                        200,
                        json!({
                            "itemId": "x", "newQuantity": 0, "trackingId": tracking,
                            "productId": product,
                            "orderTransactions": [{ "orderId": order, "quantityConsumed": 1 }],
                        }),
                    )
                }
                None => (400, json!({ "innererror": { "code": "InsufficientQuantity" } })),
            };
            ms.answers.insert(tracking, answer.clone());
            answer
        }
    };
    if fault == Some(Fault::LoseAnswer) {
        return StatusCode::BAD_GATEWAY.into_response();
    }
    (StatusCode::from_u16(status).unwrap(), Json(reply)).into_response()
}

async fn microsoft() -> (String, Fake) {
    let fake: Fake = Default::default();
    let app = axum::Router::new()
        .route("/tenant/oauth2/v2.0/token", post(token))
        .route("/v8.0/collections/b2bLicensePreview", post(query))
        .route("/v8.0/collections/consume", post(consume))
        .with_state(fake.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    (format!("http://{addr}"), fake)
}

fn store_config(base: &str) -> MsStoreConfig {
    MsStoreConfig {
        tenant_id: "tenant".into(),
        client_id: "client".into(),
        client_secret: SECRET.into(),
        products: parse_products(&format!("coins_500={PACK_500};coins_1200={PACK_1200}")).unwrap(),
        login_url: base.into(),
        collections_url: base.into(),
    }
}

async fn start_on(store: Store, base: &str) -> (String, Arc<Server>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let settings = Settings { max_connections_per_ip: 100, ..Settings::default() };
    let config =
        Config { bind: addr, settings, msstore: Some(store_config(base)), ..Config::default() };
    let server = Server::with_store(config, store).await;
    tokio::spawn(serve(listener, server.clone()));
    (format!("ws://{addr}/ws"), server)
}

/// A key as the Store makes it, for `account`, made for `user_id`.
fn store_key(account: &str, user_id: &str) -> String {
    let claims = json!({
        "aud": KEY_AUDIENCE, "iss": KEY_AUDIENCE, "exp": now() + 30 * 86_400,
        USER_ID_CLAIM: user_id, "acct": account,
    });
    format!(
        "{}.{}.signature",
        encode(br#"{"typ":"JWT","alg":"RS256"}"#),
        encode(claims.to_string().as_bytes())
    )
}

struct Client {
    ws: Ws,
}

impl Client {
    async fn register(url: &str, name: &str) -> Client {
        let (ws, _) = connect_async(url).await.unwrap();
        let mut c = Client { ws };
        c.send(ClientMsg::Hello { version: PROTOCOL_VERSION, rules: Rules::Brazilian }).await;
        c.send(ClientMsg::Register {
            username: name.into(),
            password: PASSWORD.into(),
            country: "BR".into(),
            minor: None,
        })
        .await;
        c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
        c
    }

    async fn login(url: &str, name: &str) -> Client {
        let (ws, _) = connect_async(url).await.unwrap();
        let mut c = Client { ws };
        c.send(ClientMsg::Hello { version: PROTOCOL_VERSION, rules: Rules::Brazilian }).await;
        c.send(ClientMsg::Login { username: name.into(), password: PASSWORD.into() }).await;
        c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
        c
    }

    async fn send(&mut self, msg: ClientMsg) {
        self.ws.send(Message::Text(msg.to_json().into())).await.unwrap();
    }

    async fn expect<F: Fn(&ServerMsg) -> bool>(&mut self, what: &str, pred: F) -> ServerMsg {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            let msg = tokio::time::timeout(left, self.ws.next())
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
                .expect("connection closed")
                .unwrap();
            if let Message::Text(t) = msg {
                let m = parse_server(t.as_str()).unwrap();
                if pred(&m) {
                    return m;
                }
            }
        }
    }

    /// The ticket and the publisher id, as the game asks for them.
    async fn ticket(&mut self) -> (String, String) {
        self.send(ClientMsg::StoreTicket).await;
        match self.expect("ticket", |m| matches!(m, ServerMsg::StoreTicket { .. })).await {
            ServerMsg::StoreTicket { ticket, user_id } => (ticket, user_id),
            _ => unreachable!(),
        }
    }

    /// Sends a key; returns (coins, pending, error).
    async fn redeem(&mut self, key: &str) -> (i64, bool, String) {
        self.send(ClientMsg::StoreRedeem { key: key.into() }).await;
        match self.expect("redeemed", |m| matches!(m, ServerMsg::StoreRedeemed { .. })).await {
            ServerMsg::StoreRedeemed { coins, pending, error } => (coins, pending, error),
            _ => unreachable!(),
        }
    }

    /// What the Store build does after a purchase: ticket, key, redeem.
    async fn check(&mut self, account: &str) -> (i64, bool, String) {
        let (_, user_id) = self.ticket().await;
        self.redeem(&store_key(account, &user_id)).await
    }
}

/// Coins credited from Store purchases (an account also gets a daily bonus).
async fn coins_of(server: &Server, name: &str) -> i64 {
    let user = server.store.user_by_name(name).await.unwrap().unwrap();
    let ledger = server.store.ledger(user.id, 1000).await.unwrap();
    let from_store: i64 = ledger
        .iter()
        .filter(|l| l.reference.as_deref().is_some_and(|r| r.starts_with("ms:")))
        .map(|l| l.delta)
        .sum();
    let total: i64 = ledger.iter().map(|l| l.delta).sum();
    assert_eq!(server.store.wallet(user.id).await.unwrap().coins, total, "wallet = ledger");
    from_store
}

#[tokio::test]
async fn the_shop_offers_the_store_packs() {
    let (base, _) = microsoft().await;
    let (url, _) = start_on(Store::memory(), &base).await;
    let mut a = Client::register(&url, "ana").await;
    a.send(ClientMsg::Shop).await;
    let info = a.expect("shop", |m| matches!(m, ServerMsg::ShopInfo { .. })).await;
    let ServerMsg::ShopInfo { store_packs, .. } = info else { unreachable!() };
    assert_eq!(store_packs.len(), 2);
    assert_eq!(store_packs[1].code, "coins_1200");
    assert_eq!(store_packs[1].store_id, PACK_1200);
    assert_eq!((store_packs[1].coins, store_packs[1].bonus), (1200, 100));
}

#[tokio::test]
async fn the_ticket_only_makes_collections_keys() {
    let (base, fake) = microsoft().await;
    let (url, _) = start_on(Store::memory(), &base).await;
    let mut a = Client::register(&url, "ana").await;
    let (ticket, user_id) = a.ticket().await;
    assert_eq!(ticket, "token:https://onestore.microsoft.com/b2b/keys/create/collections/.default");
    assert_eq!(user_id.len(), 32);
    assert!(!user_id.contains("ana"));
    // The token is cached.
    a.ticket().await;
    assert_eq!(fake.lock().unwrap().token_calls, 1);
}

#[tokio::test]
async fn a_purchase_is_credited_once() {
    let (base, fake) = microsoft().await;
    let (url, server) = start_on(Store::memory(), &base).await;
    let mut a = Client::register(&url, "ana").await;
    fake.lock().unwrap().buy("msa-ana", PACK_1200);
    assert_eq!(a.check("msa-ana").await, (1300, false, String::new()));
    assert_eq!(coins_of(&server, "ana").await, 1300);
    // Checking again, or with the same key twice, finds nothing new.
    assert_eq!(a.check("msa-ana").await, (0, false, String::new()));
    assert_eq!(coins_of(&server, "ana").await, 1300);
    {
        let ms = fake.lock().unwrap();
        assert_eq!((ms.consumed, ms.consume_calls), (1, 1));
    }
    // Recorded for the staff: a payment and the purchase.
    let user = server.store.user_by_name("ana").await.unwrap().unwrap();
    let payments = server.store.payments_of(user.id, 10).await.unwrap();
    assert_eq!(payments.len(), 1);
    assert_eq!((payments[0].coins, payments[0].currency.as_str()), (1300, "msstore"));
    let rows = server.store.ms_consumes_of(user.id, 10).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].state, "done");
    assert!(rows[0].order_ids.as_deref().unwrap().starts_with("order-"));
    // Two packs bought before a check: both arrive.
    {
        let mut ms = fake.lock().unwrap();
        ms.buy("msa-ana", PACK_500);
        ms.buy("msa-ana", PACK_1200);
    }
    assert_eq!(a.check("msa-ana").await.0, 1800);
    assert_eq!(coins_of(&server, "ana").await, 3100);
}

#[tokio::test]
async fn a_lost_answer_is_asked_again_and_credited_once() {
    let (base, fake) = microsoft().await;
    let (url, server) = start_on(Store::memory(), &base).await;
    let mut a = Client::register(&url, "ana").await;
    {
        let mut ms = fake.lock().unwrap();
        ms.buy("msa-ana", PACK_500);
        ms.faults.push_back(Fault::LoseAnswer);
    }
    // Consumed at Microsoft, but the server never heard: no coins yet.
    assert_eq!(a.check("msa-ana").await, (0, true, String::new()));
    assert_eq!(coins_of(&server, "ana").await, 0);
    assert_eq!(fake.lock().unwrap().consumed, 1);
    // The next check sends the same request again and gets the answer.
    assert_eq!(a.check("msa-ana").await, (500, false, String::new()));
    assert_eq!(coins_of(&server, "ana").await, 500);
    let ms = fake.lock().unwrap();
    assert_eq!(ms.consumed, 1, "consumed once");
    assert_eq!(ms.answers.len(), 1, "one tracking id for the purchase");
}

#[tokio::test]
async fn the_background_task_finishes_what_the_player_left() {
    let (base, fake) = microsoft().await;
    let (url, server) = start_on(Store::memory(), &base).await;
    let mut a = Client::register(&url, "ana").await;
    {
        let mut ms = fake.lock().unwrap();
        ms.buy("msa-ana", PACK_500);
        ms.faults.push_back(Fault::Unavailable);
        ms.faults.push_back(Fault::LoseAnswer);
    }
    assert_eq!(a.check("msa-ana").await, (0, true, String::new()));
    // Not due yet: nothing is sent.
    msstore::retry_due(&server, now()).await;
    assert_eq!(fake.lock().unwrap().consume_calls, 1);
    // Due: sent (consumed, answer lost), then sent again (answered).
    msstore::retry_due(&server, now() + 3600).await;
    assert_eq!(coins_of(&server, "ana").await, 0);
    msstore::retry_due(&server, now() + 7200).await;
    assert_eq!(coins_of(&server, "ana").await, 500);
    // The player hears about it.
    a.expect("credited", |m| matches!(m, ServerMsg::StoreRedeemed { coins: 500, .. })).await;
    msstore::retry_due(&server, now() + 99_999).await;
    assert_eq!(coins_of(&server, "ana").await, 500);
    assert_eq!(fake.lock().unwrap().consumed, 1);
}

#[tokio::test]
async fn a_refusal_credits_nothing_and_the_purchase_is_found_again() {
    let (base, fake) = microsoft().await;
    let (url, server) = start_on(Store::memory(), &base).await;
    let mut a = Client::register(&url, "ana").await;
    {
        let mut ms = fake.lock().unwrap();
        ms.buy("msa-ana", PACK_500);
        ms.faults.push_back(Fault::Refuse);
    }
    assert_eq!(a.check("msa-ana").await, (0, false, String::new()));
    assert_eq!(coins_of(&server, "ana").await, 0);
    assert_eq!(a.check("msa-ana").await.0, 500);
    let user = server.store.user_by_name("ana").await.unwrap().unwrap();
    let states: Vec<String> = server
        .store
        .ms_consumes_of(user.id, 10)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.state)
        .collect();
    let mut states = states;
    states.sort();
    assert_eq!(states, ["done", "failed"]);
}

#[tokio::test]
async fn a_refusal_after_a_lost_answer_proves_nothing() {
    // The first request consumed the purchase but its answer was lost; the
    // resend is refused. The purchase must not be given up: the next resend
    // gets Microsoft's answer to the first one.
    let (base, fake) = microsoft().await;
    let (url, server) = start_on(Store::memory(), &base).await;
    let mut a = Client::register(&url, "ana").await;
    {
        let mut ms = fake.lock().unwrap();
        ms.buy("msa-ana", PACK_500);
        ms.faults.extend([Fault::LoseAnswer, Fault::Refuse, Fault::Conflict]);
    }
    assert_eq!(a.check("msa-ana").await, (0, true, String::new()));
    msstore::retry_due(&server, now() + 3600).await;
    msstore::retry_due(&server, now() + 7200).await;
    assert_eq!(coins_of(&server, "ana").await, 0);
    let user = server.store.user_by_name("ana").await.unwrap().unwrap();
    let rows = server.store.ms_consumes_of(user.id, 10).await.unwrap();
    assert_eq!((rows[0].state.as_str(), rows[0].attempts), ("pending", 3));
    msstore::retry_due(&server, now() + 99_999).await;
    assert_eq!(coins_of(&server, "ana").await, 500);
    assert_eq!(fake.lock().unwrap().consumed, 1);
}

#[tokio::test]
async fn too_many_checks_get_an_answer_the_game_understands() {
    let (base, _) = microsoft().await;
    let (url, _) = start_on(Store::memory(), &base).await;
    let mut a = Client::register(&url, "ana").await;
    for _ in 0..13 {
        a.send(ClientMsg::StoreTicket).await;
    }
    let m = a
        .expect("slow down", |m| {
            matches!(m, ServerMsg::StoreRedeemed { error, .. } if error == "Slow down a little.")
        })
        .await;
    assert!(matches!(m, ServerMsg::StoreRedeemed { coins: 0, .. }));
}

#[tokio::test]
async fn keys_only_work_for_the_player_they_were_made_for() {
    let (base, fake) = microsoft().await;
    let (url, server) = start_on(Store::memory(), &base).await;
    let mut ana = Client::register(&url, "ana").await;
    let mut bob = Client::register(&url, "bob").await;
    fake.lock().unwrap().buy("msa-ana", PACK_1200);
    let (_, ana_id) = ana.ticket().await;
    let ana_key = store_key("msa-ana", &ana_id);
    // Bob got hold of Ana's key: refused before Microsoft is asked anything.
    let (coins, _, error) = bob.redeem(&ana_key).await;
    assert_eq!(coins, 0);
    assert!(!error.is_empty());
    // A key that isn't one, and an expired one.
    assert!(!ana.redeem("not-a-key").await.2.is_empty());
    let expired = {
        let claims = json!({ "aud": KEY_AUDIENCE, "exp": now() - 1, USER_ID_CLAIM: ana_id, "acct": "msa-ana" });
        format!("e30.{}.s", encode(claims.to_string().as_bytes()))
    };
    assert!(!ana.redeem(&expired).await.2.is_empty());
    {
        let ms = fake.lock().unwrap();
        assert_eq!((ms.query_calls, ms.consume_calls), (0, 0));
    }
    assert_eq!(coins_of(&server, "bob").await, 0);
    assert_eq!(ana.redeem(&ana_key).await.0, 1300);
}

#[tokio::test]
async fn two_servers_at_once_credit_a_purchase_once() {
    // A deploy: the old server and the new one share the database for a
    // while, and the player is connected to both.
    let (base, fake) = microsoft().await;
    let store = Store::memory();
    let (url1, server) = start_on(store.share().unwrap(), &base).await;
    let (url2, _) = start_on(store, &base).await;
    let mut a = Client::register(&url1, "ana").await;
    let mut b = Client::login(&url2, "ana").await;
    let (_, id) = a.ticket().await;
    for _ in 0..5 {
        fake.lock().unwrap().buy("msa-ana", PACK_500);
        let key = store_key("msa-ana", &id);
        a.send(ClientMsg::StoreRedeem { key: key.clone() }).await;
        b.send(ClientMsg::StoreRedeem { key }).await;
        a.expect("a", |m| matches!(m, ServerMsg::StoreRedeemed { .. })).await;
        b.expect("b", |m| matches!(m, ServerMsg::StoreRedeemed { .. })).await;
    }
    assert_eq!(coins_of(&server, "ana").await, 2500);
    assert_eq!(fake.lock().unwrap().consumed, 5);
}

#[tokio::test]
async fn without_microsoft_nothing_is_credited() {
    let (url, server) = start_on(Store::memory(), "http://127.0.0.1:9").await;
    let mut a = Client::register(&url, "ana").await;
    a.send(ClientMsg::StoreTicket).await;
    let m = a.expect("answer", |m| matches!(m, ServerMsg::StoreRedeemed { .. })).await;
    assert!(matches!(m, ServerMsg::StoreRedeemed { coins: 0, ref error, .. } if !error.is_empty()));
    assert_eq!(coins_of(&server, "ana").await, 0);
}

#[tokio::test]
async fn servers_without_the_store_say_so() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        Server::with_store(Config { bind: addr, ..Config::default() }, Store::memory()).await;
    tokio::spawn(serve(listener, server.clone()));
    let mut a = Client::register(&format!("ws://{addr}/ws"), "ana").await;
    a.send(ClientMsg::StoreTicket).await;
    let m = a.expect("answer", |m| matches!(m, ServerMsg::StoreRedeemed { .. })).await;
    assert!(matches!(m, ServerMsg::StoreRedeemed { coins: 0, ref error, .. } if !error.is_empty()));
    a.send(ClientMsg::Shop).await;
    let info = a.expect("shop", |m| matches!(m, ServerMsg::ShopInfo { .. })).await;
    assert!(matches!(info, ServerMsg::ShopInfo { ref store_packs, .. } if store_packs.is_empty()));
}

/// The storage rules, on whatever store.
async fn exercise(store: Store) {
    let user = store
        .create_user(&NewUser {
            username: "ana",
            password_hash: "h",
            country: "BR",
            rating: 1200,
            role: Role::Player,
            ip: "1.2.3.4",
            minor: None,
        })
        .await
        .unwrap()
        .unwrap();
    let t = now();
    let row = |tracking: &str, transaction: &str| MsConsume {
        tracking_id: tracking.into(),
        user_id: user.id,
        product_id: PACK_500.into(),
        pack: "coins_500".into(),
        coins: 500,
        transaction_id: transaction.into(),
        store_key: "key".into(),
        attempts: 0,
        created_at: t,
    };
    let coins = || async { store.wallet(user.id).await.unwrap().coins };
    assert!(store.ms_open(&row("a", "tx1")).await.unwrap());
    // The same purchase can't be worked on twice, nor the same tracking id.
    assert!(!store.ms_open(&row("b", "tx1")).await.unwrap());
    assert!(!store.ms_open(&row("a", "tx2")).await.unwrap());
    let pending = store.ms_pending_of(user.id).await.unwrap();
    assert_eq!((pending.len(), pending[0].attempts), (1, 1), "the opener's request counts");
    // Leased to the opener while it sends: nobody else takes it.
    assert!(store.ms_lease("a", t).await.unwrap().is_none());
    assert!(store.ms_due(t + 60, 10).await.unwrap().is_empty(), "leased");
    // No answer: tried again later.
    store.ms_retry_later("a", "no answer", t + 600).await.unwrap();
    assert!(store.ms_due(t + 599, 10).await.unwrap().is_empty());
    assert_eq!(store.ms_due(t + 600, 10).await.unwrap().len(), 1);
    // Two servers take it at once: only one sends.
    let lease = store.ms_lease("a", t + 600).await.unwrap().unwrap();
    assert_eq!((lease.attempts, lease.store_key.as_str()), (2, "key"));
    assert!(store.ms_lease("a", t + 600).await.unwrap().is_none());
    assert!(store.ms_due(t + 600, 10).await.unwrap().is_empty());
    // Credited once, with the payment, whatever is confirmed twice.
    assert!(store.ms_fulfil("a", "o1", t).await.unwrap());
    assert!(!store.ms_fulfil("a", "o1", t).await.unwrap());
    assert!(!store.ms_close("a", MsState::Failed, "late", t).await.unwrap(), "done stays done");
    assert_eq!(coins().await, 500);
    assert_eq!(store.payment("ms:a").await.unwrap().unwrap().coins, 500);
    assert!(store.ms_pending_of(user.id).await.unwrap().is_empty());
    assert!(!store.ms_open(&row("c", "tx1")).await.unwrap(), "credited purchases stay closed");
    // Closed as refused, then Microsoft's confirmation of that very request
    // arrives: it is credited (once).
    assert!(store.ms_open(&row("d", "tx2")).await.unwrap());
    assert!(store.ms_close("d", MsState::Failed, "400", t).await.unwrap());
    assert!(store.ms_fulfil("d", "o2", t).await.unwrap());
    assert!(!store.ms_fulfil("d", "o2", t).await.unwrap());
    assert_eq!(coins().await, 1000);
    // A refused purchase is tried again under a new tracking id; a late
    // confirmation of the old one can't credit the purchase a second time.
    assert!(store.ms_open(&row("g", "tx4")).await.unwrap());
    assert!(store.ms_close("g", MsState::Failed, "400", t).await.unwrap());
    assert!(store.ms_open(&row("h", "tx4")).await.unwrap());
    assert!(store.ms_fulfil("h", "o4", t).await.unwrap());
    assert!(store.ms_fulfil("g", "o4", t).await.is_err(), "the purchase is held by h");
    assert_eq!(coins().await, 1500);
    // A stuck one keeps its key, for the staff to send again.
    assert!(store.ms_open(&row("e", "tx3")).await.unwrap());
    assert!(store.ms_close("e", MsState::Stuck, "no answer", t).await.unwrap());
    assert!(store.ms_open(&row("f", "tx3")).await.unwrap(), "stuck doesn't hold the purchase");
    assert!(store.ms_reopen("e", t).await.is_err(), "f holds it");
    assert!(store.ms_close("f", MsState::Failed, "400", t).await.unwrap());
    assert!(!store.ms_reopen("f", t).await.unwrap(), "only stuck rows");
    assert!(store.ms_reopen("e", t).await.unwrap());
    assert!(!store.ms_reopen("e", t).await.unwrap(), "pending now");
    assert_eq!(store.ms_lease("e", t).await.unwrap().unwrap().store_key, "key");
    let rows = store.ms_consumes_of(user.id, 10).await.unwrap();
    let by: HashMap<String, String> = rows.into_iter().map(|r| (r.tracking_id, r.state)).collect();
    let states: Vec<&str> =
        ["a", "d", "e", "f", "g", "h"].iter().map(|k| by[*k].as_str()).collect();
    assert_eq!(states, ["done", "done", "pending", "failed", "failed", "done"]);
    assert_eq!(coins().await, 1500);
}

#[tokio::test]
async fn memory_store() {
    exercise(Store::memory()).await;
}

#[tokio::test]
async fn postgres_store() {
    let Ok(url) = std::env::var("SNOOK_LEGEND_TEST_DATABASE_URL") else {
        eprintln!("SNOOK_LEGEND_TEST_DATABASE_URL not set: skipping");
        return;
    };
    {
        use sqlx::Executor;
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        pool.execute("DROP SCHEMA public CASCADE; CREATE SCHEMA public;").await.unwrap();
    }
    exercise(Store::postgres(&url, 2).await.unwrap()).await;
}

const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

fn decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        acc = (acc << 6) | ALPHABET.iter().position(|a| *a == c)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let h = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                out.push(h.and_then(|h| u8::from_str_radix(h, 16).ok()).unwrap_or(b'?'));
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
