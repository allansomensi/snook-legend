//! Operations and the economy end to end (0.12): settings changed live
//! from the portal, sales in the shop, auctions, the daily bonus's streak,
//! paid username changes, maintenance announced, the supporters' credits,
//! official tournaments kept, the analytics and the public API.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use snook_legend_proto::ops::AuctionState;
use snook_legend_proto::{ClientMsg, PROTOCOL_VERSION, Role, Rules, ServerMsg, parse_server};
use snook_legend_server::lounge::Settings;
use snook_legend_server::store::{
    BidOutcome, Export, NewAuction, NewPromotion, NewUser, OfficialRecord, Store,
};
use snook_legend_server::{Config, Server, now, serve};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

const PASSWORD: &str = "correct-horse-1";

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Running {
    url: String,
    host: String,
    server: Arc<Server>,
}

async fn user(server: &Server, name: &str, role: Role) -> i64 {
    let hash = server.hash_password(PASSWORD).await.unwrap();
    let new = NewUser {
        username: name,
        password_hash: &hash,
        country: "BR",
        rating: 1200,
        role,
        ip: "1.2.3.4",
        minor: Some(false),
    };
    server.store.create_user(&new).await.unwrap().expect("a new account").id
}

/// A server with an administrator ("boss") and a moderator ("mod").
async fn start() -> Running {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let settings = Settings { max_connections_per_ip: 100, ..Settings::default() };
    let config = Config { bind: addr, settings, admins: vec!["boss".into()], ..Config::default() };
    let server = Server::with_store(config, Store::memory()).await;
    user(&server, "boss", Role::Player).await;
    user(&server, "mod", Role::Moderator).await;
    tokio::spawn(serve(listener, server.clone()));
    Running { url: format!("ws://{addr}/ws"), host: addr.to_string(), server }
}

struct Client {
    ws: Ws,
}

impl Client {
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

    async fn login(url: &str, name: &str) -> Client {
        let (ws, _) = connect_async(url).await.unwrap();
        let mut c = Client { ws };
        c.send(ClientMsg::Hello { version: PROTOCOL_VERSION, rules: Rules::Brazilian }).await;
        c.send(ClientMsg::Login { username: name.into(), password: PASSWORD.into() }).await;
        c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
        c
    }

    async fn error(&mut self) -> String {
        match self.expect("an error", |m| matches!(m, ServerMsg::Error { .. })).await {
            ServerMsg::Error { message } => message,
            _ => unreachable!(),
        }
    }
}

/// A small HTTP client over a raw socket: (status, headers, body).
async fn raw(
    host: &str,
    method: &str,
    path: &str,
    token: &str,
    body: &str,
) -> (u16, String, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = TcpStream::connect(host).await.unwrap();
    let auth =
        if token.is_empty() { String::new() } else { format!("Authorization: Bearer {token}\r\n") };
    let req = format!(
        "{method} {path} HTTP/1.0\r\nHost: {host}\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.unwrap();
    let out = String::from_utf8_lossy(&out).to_string();
    let code = out[9..12].parse().unwrap();
    let (head, body) = out.split_once("\r\n\r\n").unwrap_or((&out, ""));
    (code, head.to_string(), body.to_string())
}

async fn request(host: &str, method: &str, path: &str, token: &str, body: &str) -> (u16, Value) {
    let (code, _, body) = raw(host, method, path, token, body).await;
    (code, serde_json::from_str(&body).unwrap_or(Value::Null))
}

async fn token(host: &str, name: &str) -> String {
    let body = format!(r#"{{"username":"{name}","password":"{PASSWORD}"}}"#);
    let (code, v) = request(host, "POST", "/admin/api/login", "", &body).await;
    assert_eq!(code, 200, "{v}");
    v["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn settings_change_live_and_are_audited() {
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let moderator = token(&s.host, "mod").await;
    let (code, v) = request(&s.host, "GET", "/admin/api/settings", &moderator, "").await;
    assert_eq!(code, 200, "moderators read them: {v}");
    let clock = v["settings"].as_array().unwrap().iter().find(|x| x["key"] == "shot_clock_secs");
    let clock = clock.expect("the shot clock is a setting");
    assert_eq!((clock["value"].clone(), clock["source"].clone()), (json!(30), json!("default")));
    let body = r#"{"value": 45}"#;
    let (code, _) =
        request(&s.host, "POST", "/admin/api/settings/shot_clock_secs", &moderator, body).await;
    assert_eq!(code, 403, "only administrators change them");
    let (code, v) =
        request(&s.host, "POST", "/admin/api/settings/shot_clock_secs", &boss, body).await;
    assert_eq!((code, v["value"].clone()), (200, json!(45)), "{v}");
    assert_eq!(s.server.with(|l| l.settings.shot_clock), Duration::from_secs(45), "at once");
    let (code, v) =
        request(&s.host, "POST", "/admin/api/settings/shot_clock_secs", &boss, r#"{"value": 5}"#)
            .await;
    assert_eq!(code, 400, "out of range: {v}");
    let (code, _) =
        request(&s.host, "POST", "/admin/api/settings/nothing", &boss, r#"{"value": 1}"#).await;
    assert_eq!(code, 400);
    // The weekly cup from the portal.
    for (k, v) in [("cup_enabled", "true"), ("cup_format", "\"knockout\""), ("cup_size", "32")] {
        let (code, r) = request(
            &s.host,
            "POST",
            &format!("/admin/api/settings/{k}"),
            &boss,
            &format!(r#"{{"value": {v}}}"#),
        )
        .await;
        assert_eq!(code, 200, "{k}: {r}");
    }
    let cup = s.server.with(|l| l.settings.cup.clone()).expect("the cup is on");
    assert_eq!((cup.format, cup.size), (snook_legend_proto::TournamentFormat::Knockout, 32));
    let (code, v) =
        request(&s.host, "POST", "/admin/api/settings/shot_clock_secs/reset", &boss, "").await;
    assert_eq!((code, v["value"].clone()), (200, json!(30)));
    assert_eq!(s.server.with(|l| l.settings.shot_clock), Duration::from_secs(30));
    let audit = s.server.store.audit_log(20).await.unwrap();
    assert!(audit.iter().any(|a| a.action == "setting" && a.target == "shot_clock_secs"));
    assert!(audit.iter().any(|a| a.action == "setting-reset"));
    // A server started on the same store finds them in force.
    let config = Config { settings: Settings::default(), ..Config::default() };
    let again = Server::with_store(config, s.server.store.share().unwrap()).await;
    assert_eq!(again.with(|l| l.settings.cup.as_ref().map(|c| c.size)), Some(32));
}

#[tokio::test]
async fn a_sale_lowers_what_items_cost() {
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let ana = user(&s.server, "ana", Role::Player).await;
    let item = snook_legend_proto::shop::item("cloth_blue").unwrap();
    let price = item.coins().unwrap() as i64;
    s.server.store.credit(ana, price, "staff: test", None, now()).await.unwrap();
    let mut c = Client::login(&s.url, "ana").await;
    let body = json!({
        "title": "Weekend sale", "kind": "discount", "percent": 30,
        "categories": ["cloth"], "starts_at": now(), "ends_at": now() + 3600,
    });
    let (code, v) =
        request(&s.host, "POST", "/admin/api/promotions", &boss, &body.to_string()).await;
    assert_eq!(code, 200, "{v}");
    let msg = c
        .expect(
            "the promotions",
            |m| matches!(m, ServerMsg::Promotions { list } if !list.is_empty()),
        )
        .await;
    let ServerMsg::Promotions { list } = msg else { unreachable!() };
    assert_eq!((list[0].percent, list[0].title.as_str()), (30, "Weekend sale"));
    c.send(ClientMsg::BuyItem { code: item.code.into() }).await;
    c.expect("bought", |m| matches!(m, ServerMsg::ItemBought { .. })).await;
    let left = s.server.store.wallet(ana).await.unwrap().coins;
    // (With the daily bonus of signing in.)
    let paid = snook_legend_proto::ops::discounted(price as u32, 30) as i64;
    assert_eq!(left, price + 5 - paid);
    // Called off: players are told (the list is empty).
    let id = v["id"].as_i64().unwrap();
    let (code, _) =
        request(&s.host, "POST", &format!("/admin/api/promotions/{id}/cancel"), &boss, "").await;
    assert_eq!(code, 200);
    c.expect("none any more", |m| matches!(m, ServerMsg::Promotions { list } if list.is_empty()))
        .await;
    let bad = json!({"title": "x", "kind": "discount", "percent": 95, "codes": ["cloth_blue"],
                     "starts_at": now(), "ends_at": now() + 60});
    let (code, _) =
        request(&s.host, "POST", "/admin/api/promotions", &boss, &bad.to_string()).await;
    assert_eq!(code, 400, "at most 90% off");
}

#[tokio::test]
async fn auctions_hold_bids_and_give_them_back() {
    let store = Store::memory();
    let t = now();
    let mk = |name: &'static str| NewUser {
        username: name,
        password_hash: "x",
        country: "BR",
        rating: 1200,
        role: Role::Player,
        ip: "1.2.3.4",
        minor: Some(false),
    };
    let a = store.create_user(&mk("ana")).await.unwrap().unwrap().id;
    let b = store.create_user(&mk("bob")).await.unwrap().unwrap().id;
    store.credit(a, 1000, "staff: test", None, t).await.unwrap();
    store.credit(b, 1000, "staff: test", None, t).await.unwrap();
    let new = NewAuction {
        code: "cloth_blue",
        starts_at: t - 10,
        ends_at: t + 3600,
        min_bid: 100,
        min_increment: 50,
        by: a,
        at: t,
    };
    let id = store.add_auction(&new).await.unwrap();
    assert_eq!(store.bid(id, a, 50, t, 120, 120).await.unwrap(), BidOutcome::TooLow(100));
    let placed = store.bid(id, a, 100, t, 120, 120).await.unwrap();
    assert_eq!(placed, BidOutcome::Placed { outbid: None, ends_at: t + 3600 });
    assert_eq!(store.wallet(a).await.unwrap().coins, 900, "held");
    assert_eq!(store.bid(id, b, 120, t, 120, 120).await.unwrap(), BidOutcome::TooLow(150));
    assert_eq!(store.bid(id, b, 5000, t, 120, 120).await.unwrap(), BidOutcome::NotEnoughCoins);
    let placed = store.bid(id, b, 150, t, 120, 120).await.unwrap();
    assert_eq!(placed, BidOutcome::Placed { outbid: Some((a, 100)), ends_at: t + 3600 });
    assert_eq!(store.wallet(a).await.unwrap().coins, 1000, "back at once");
    assert_eq!(store.wallet(b).await.unwrap().coins, 850);
    // At the last moment: the end moves on.
    let late = t + 3550;
    let placed = store.bid(id, a, 300, late, 120, 120).await.unwrap();
    assert_eq!(placed, BidOutcome::Placed { outbid: Some((b, 150)), ends_at: late + 120 });
    assert_eq!(store.settle_auction(id, t + 3600).await.unwrap(), None, "not over yet");
    let row = store.settle_auction(id, late + 120).await.unwrap().expect("settled");
    assert_eq!((row.leader.as_str(), row.bid), ("ana", 300));
    assert!(store.wallet(a).await.unwrap().owned.iter().any(|o| o == "cloth_blue"));
    assert_eq!(store.wallet(a).await.unwrap().coins, 700, "the coins left the economy");
    assert_eq!(store.bid(id, b, 400, late + 200, 120, 120).await.unwrap(), BidOutcome::NotOpen);
    assert_eq!(store.cancel_auction(id, late + 300).await.unwrap(), None, "settled already");
    // Called off: the leader's coins come back.
    let id = store.add_auction(&NewAuction { code: "cloth_red", ..new }).await.unwrap();
    store.bid(id, b, 200, t, 120, 120).await.unwrap();
    assert_eq!(store.cancel_auction(id, t + 1).await.unwrap(), Some(Some((b, 200))));
    assert_eq!(store.wallet(b).await.unwrap().coins, 1000);
    assert_eq!(store.auction_bids(id, 10).await.unwrap().len(), 1);
}

#[tokio::test]
async fn players_bid_and_are_told_when_outbid() {
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let ana = user(&s.server, "ana", Role::Player).await;
    let bob = user(&s.server, "bob", Role::Player).await;
    for id in [ana, bob] {
        s.server.store.credit(id, 1000, "staff: test", None, now()).await.unwrap();
    }
    let body = json!({"code": "cloth_blue", "starts_at": now(), "ends_at": now() + 3600,
                      "min_bid": 100, "min_increment": 10});
    let (code, v) = request(&s.host, "POST", "/admin/api/auctions", &boss, &body.to_string()).await;
    assert_eq!(code, 200, "{v}");
    let id = v["id"].as_i64().unwrap();
    let body = json!({"code": "cloth_green", "starts_at": now(), "ends_at": now() + 3600,
                      "min_bid": 1});
    let (code, _) = request(&s.host, "POST", "/admin/api/auctions", &boss, &body.to_string()).await;
    assert_eq!(code, 400, "a free item isn't auctioned");
    let mut a = Client::login(&s.url, "ana").await;
    let mut b = Client::login(&s.url, "bob").await;
    a.send(ClientMsg::Auctions).await;
    let msg = a.expect("the auctions", |m| matches!(m, ServerMsg::Auctions { .. })).await;
    let ServerMsg::Auctions { list } = msg else { unreachable!() };
    assert_eq!((list.len(), list[0].state), (1, AuctionState::Open));
    a.send(ClientMsg::Bid { id, amount: 50 }).await;
    assert_eq!(a.error().await, "Bid at least 100 coins.");
    a.send(ClientMsg::Bid { id, amount: 100 }).await;
    b.expect(
        "ana leads",
        |m| matches!(m, ServerMsg::AuctionUpdated { auction } if auction.leader == "ana"),
    )
    .await;
    b.send(ClientMsg::Bid { id, amount: 110 }).await;
    a.expect("bob leads", |m| {
        matches!(m, ServerMsg::AuctionUpdated { auction } if auction.leader == "bob" && auction.bid == 110)
    })
    .await;
    // (With the daily bonus of signing in.)
    assert_eq!(s.server.store.wallet(ana).await.unwrap().coins, 1005, "ana's coins back");
    assert_eq!(s.server.store.wallet(bob).await.unwrap().coins, 895);
    // Called off from the portal: bob's coins come back too.
    let (code, v) =
        request(&s.host, "POST", &format!("/admin/api/auctions/{id}/cancel"), &boss, "").await;
    assert_eq!((code, v["refunded"].clone()), (200, json!(110)));
    assert_eq!(s.server.store.wallet(bob).await.unwrap().coins, 1005);
}

#[tokio::test]
async fn the_daily_bonus_grows_with_a_streak() {
    let store = Store::memory();
    let id = store
        .create_user(&NewUser {
            username: "ana",
            password_hash: "x",
            country: "BR",
            rating: 1200,
            role: Role::Player,
            ip: "1.2.3.4",
            minor: Some(false),
        })
        .await
        .unwrap()
        .unwrap()
        .id;
    let day = 20_000;
    let claim = |d: i64| {
        let store = &store;
        async move { store.claim_daily_streak(id, d, 5, 5, 3).await.unwrap() }
    };
    assert_eq!(claim(day).await.map(|c| (c.day, c.coins)), Some((1, 5)));
    assert_eq!(claim(day).await, None, "once a day");
    assert_eq!(claim(day + 1).await.map(|c| (c.day, c.coins)), Some((2, 10)));
    assert_eq!(claim(day + 2).await.map(|c| (c.day, c.coins)), Some((3, 15)));
    assert_eq!(claim(day + 3).await.map(|c| (c.day, c.coins)), Some((4, 15)), "the top stays");
    assert_eq!(claim(day + 5).await.map(|c| (c.day, c.coins)), Some((1, 5)), "a day missed");
    assert_eq!(store.wallet(id).await.unwrap().coins, 50);
}

#[tokio::test]
async fn signing_in_brings_the_streak_and_a_paid_rename_costs_coins() {
    let s = start().await;
    let ana = user(&s.server, "ana", Role::Player).await;
    let mut c = Client::login(&s.url, "ana").await;
    let msg = c.expect("the streak", |m| matches!(m, ServerMsg::DailyStreak { .. })).await;
    assert!(matches!(msg, ServerMsg::DailyStreak { day: 1, coins: 5, tomorrow: 10 }), "{msg:?}");
    // The free changes used: one more costs coins.
    let mut fresh = s.server.store.user_by_id(ana).await.unwrap().unwrap();
    for n in 0..snook_legend_proto::MAX_NAME_CHANGES {
        let name = format!("ana{n}");
        s.server
            .store
            .change_username(ana, &name, snook_legend_proto::MAX_NAME_CHANGES)
            .await
            .unwrap();
        fresh.username = name;
    }
    drop(c);
    let mut c = Client::login(&s.url, &fresh.username).await;
    c.send(ClientMsg::Account).await;
    let msg = c.expect("the account", |m| matches!(m, ServerMsg::Account { .. })).await;
    let ServerMsg::Account { name_changes_left, name_change_price, .. } = msg else {
        unreachable!()
    };
    assert_eq!((name_changes_left, name_change_price), (0, 1500));
    c.send(ClientMsg::ChangeUsername { username: "anna".into() }).await;
    assert_eq!(c.error().await, "Not enough coins.");
    s.server.store.credit(ana, 2000, "staff: test", None, now()).await.unwrap();
    c.send(ClientMsg::ChangeUsername { username: "anna".into() }).await;
    c.expect("renamed", |m| matches!(m, ServerMsg::Notice { text } if text == "Username changed."))
        .await;
    let coins = s.server.store.wallet(ana).await.unwrap().coins;
    assert_eq!(coins, 5 + 2000 - 1500);
}

#[tokio::test]
async fn maintenance_is_announced_and_starts_once() {
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let _ana = user(&s.server, "ana", Role::Player).await;
    let mut c = Client::login(&s.url, "ana").await;
    let at = now() + 1800;
    let body = json!({"at": at, "minutes": 20, "message": "New tables"});
    let (code, v) =
        request(&s.host, "POST", "/admin/api/maintenance", &boss, &body.to_string()).await;
    assert_eq!(code, 200, "{v}");
    let msg = c
        .expect("the banner", |m| matches!(m, ServerMsg::Maintenance { at: a, .. } if *a == at))
        .await;
    assert!(matches!(msg, ServerMsg::Maintenance { minutes: 20, .. }));
    let (code, _) =
        request(&s.host, "POST", "/admin/api/maintenance", &boss, &body.to_string()).await;
    assert_eq!(code, 400, "one at a time");
    let id = v["id"].as_i64().unwrap();
    let (code, _) =
        request(&s.host, "POST", &format!("/admin/api/maintenance/{id}/cancel"), &boss, "").await;
    assert_eq!(code, 200);
    c.expect("called off", |m| matches!(m, ServerMsg::Maintenance { at: 0, .. })).await;
    // Only one server goes down for a window.
    let store = &s.server.store;
    let id = store.add_maintenance(now() - 1, 10, "", 1, now()).await.unwrap();
    assert!(store.start_maintenance(id, now()).await.unwrap());
    assert!(!store.start_maintenance(id, now()).await.unwrap());
    assert!(!store.cancel_maintenance(id, now()).await.unwrap(), "started already");
}

#[tokio::test]
async fn supporters_choose_to_be_in_the_credits() {
    let s = start().await;
    let ana = user(&s.server, "ana", Role::Player).await;
    let _bob = user(&s.server, "bob", Role::Player).await;
    let mut b = Client::login(&s.url, "bob").await;
    b.send(ClientMsg::SetCredits { show: true }).await;
    assert_eq!(b.error().await, "The credits are for VIP members, now or before.");
    s.server.store.set_vip(ana, now() + 86_400, true).await.unwrap();
    let mut a = Client::login(&s.url, "ana").await;
    a.send(ClientMsg::SetCredits { show: true }).await;
    let msg = a.expect("the account", |m| matches!(m, ServerMsg::Account { .. })).await;
    assert!(matches!(msg, ServerMsg::Account { supporter: true, in_credits: true, .. }));
    a.send(ClientMsg::Supporters).await;
    let msg = a.expect("the credits", |m| matches!(m, ServerMsg::Supporters { .. })).await;
    let ServerMsg::Supporters { list } = msg else { unreachable!() };
    assert_eq!(
        list.iter().map(|x| (x.username.as_str(), x.vip)).collect::<Vec<_>>(),
        [("ana", true)]
    );
    let (code, v) = request(&s.host, "GET", "/api/supporters", "", "").await;
    assert_eq!((code, v[0]["username"].clone()), (200, json!("ana")));
    let (code, v) = request(&s.host, "GET", "/api/players/ANA", "", "").await;
    assert_eq!((code, v["supporter"].clone(), v["vip"].clone()), (200, json!(true), json!(true)));
}

#[tokio::test]
async fn the_public_api_shows_players_and_official_tournaments() {
    let s = start().await;
    let ana = user(&s.server, "ana", Role::Player).await;
    let bob = user(&s.server, "bob", Role::Player).await;
    let (code, head, body) = raw(&s.host, "GET", "/api/players/ana", "", "").await;
    assert_eq!(code, 200, "{body}");
    assert!(head.to_lowercase().contains("access-control-allow-origin: *"));
    assert!(head.to_lowercase().contains("max-age=60"));
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["username"], "ana");
    assert_eq!(v["ratings"][0]["rules"], "brazilian");
    assert_eq!(v["ratings"][0]["rank"], Value::Null, "no rated game");
    assert_eq!(v["ratings"][0]["tier"], "rookie");
    assert_eq!(v["role"], "player");
    for path in ["/api/players/nobody", "/api/players/a%20b", "/api/tournaments/99"] {
        let (code, _) = request(&s.host, "GET", path, "", "").await;
        assert_eq!(code, 404, "{path}");
    }
    // A banned player is not shown.
    s.server.store.set_ban(bob, Some(now() + 3600), Some("cheating")).await.unwrap();
    let (code, _) = request(&s.host, "GET", "/api/players/bob", "", "").await;
    assert_eq!(code, 404);
    let detail = json!({
        "players": [{"id": ana, "username": "ana", "country": "BR", "seed": 1},
                    {"id": bob, "username": "bob", "country": "BR", "seed": 2}],
        "rounds": [[{"a": ana, "b": bob, "winner": ana}]],
        "standings": [],
    });
    let record = OfficialRecord {
        key: "7:100".into(),
        event_id: Some(3),
        name: "Winter Cup".into(),
        rules: Rules::Official,
        format: "knockout".into(),
        size: 4,
        players: 2,
        rated: true,
        entry_fee: 100,
        prize_pool: 180,
        prizes: vec![140, 40],
        item_prize: String::new(),
        champion_id: Some(ana),
        runner_up_id: Some(bob),
        detail: detail.to_string(),
        starts_at: 100,
        ends_at: 200,
    };
    let id = s.server.store.record_official(&record).await.unwrap().expect("kept");
    assert_eq!(s.server.store.record_official(&record).await.unwrap(), None, "once");
    let (code, v) = request(&s.host, "GET", "/api/tournaments?limit=5", "", "").await;
    assert_eq!(code, 200);
    assert_eq!((v[0]["id"].clone(), v[0]["champion"].clone()), (json!(id), json!("ana")));
    assert_eq!(v[0]["prize_pool"], 180);
    // Renamed since: the bracket shows the name now.
    s.server.store.change_username(ana, "anna", 3).await.unwrap();
    let (code, v) = request(&s.host, "GET", &format!("/api/tournaments/{id}"), "", "").await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["rounds"][0][0]["a"], "anna");
    assert_eq!(v["rounds"][0][0]["winner"], "anna");
    assert_eq!(v["players"][1]["seed"], 2);
    assert_eq!(v["prizes"], json!([140, 40]));
    assert_eq!(v["champion"], "anna");
    let (code, _) = request(&s.host, "GET", "/api/players/ana", "", "").await;
    assert_eq!(code, 404, "a name left behind");
}

#[tokio::test]
async fn the_analytics_and_exports_read_the_economy() {
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let moderator = token(&s.host, "mod").await;
    let ana = user(&s.server, "ana", Role::Player).await;
    s.server.store.credit(ana, 1000, "staff: test", None, now()).await.unwrap();
    s.server.store.buy_item(ana, "cloth_blue", 500, now()).await.unwrap();
    let (code, v) =
        request(&s.host, "GET", "/admin/api/analytics/coins?days=7", &moderator, "").await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["days"].as_array().unwrap().len(), 7);
    assert_eq!(v["totals"]["created"]["staff"], 1000);
    assert_eq!(v["totals"]["spent"]["shop:cloth"], 500);
    assert_eq!(v["supply"], 500);
    let (code, v) = request(&s.host, "GET", "/admin/api/analytics/sellers", &moderator, "").await;
    assert_eq!(code, 200);
    assert_eq!(v["items"][0]["code"], "cloth_blue");
    assert_eq!(v["items"][0]["days_7"]["sales"], 1);
    assert!(v["never_sold"].as_array().unwrap().iter().all(|i| i["code"] != "cloth_blue"));
    let (code, v) = request(&s.host, "GET", "/admin/api/analytics/countries", &moderator, "").await;
    assert_eq!((code, v["countries"][0]["country"].clone()), (200, json!("BR")));
    let (code, v) =
        request(&s.host, "GET", "/admin/api/analytics/hours?weeks=2&tz=-180", &moderator, "").await;
    assert_eq!((code, v["cells"].as_array().map(|c| c.len())), (200, Some(7 * 24)));
    let (code, _) = request(&s.host, "GET", "/admin/api/analytics/vip", &moderator, "").await;
    assert_eq!(code, 403, "money is for administrators");
    let (code, v) = request(&s.host, "GET", "/admin/api/analytics/vip", &boss, "").await;
    assert_eq!((code, v["active"].clone()), (200, json!(0)));
    let (code, _, csv) = raw(&s.host, "GET", "/admin/api/export/ledger.csv", &moderator, "").await;
    assert_eq!(code, 403, "{csv}");
    let (code, head, csv) = raw(&s.host, "GET", "/admin/api/export/ledger.csv", &boss, "").await;
    assert_eq!(code, 200, "{csv}");
    assert!(head.contains("text/csv"));
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], "id,at,user_id,username,delta,reason,reference");
    assert!(lines.iter().any(|l| l.contains(",ana,-500,buy cloth_blue,")), "{csv}");
    let (code, _, csv) = raw(&s.host, "GET", "/admin/api/export/players.csv", &boss, "").await;
    assert_eq!(code, 200);
    assert_eq!(csv.lines().count(), 4, "the header and three accounts: {csv}");
    let page = s.server.store.export_page(Export::Players, 0, now() + 1, 0, 2).await.unwrap();
    assert_eq!(page.len(), 2, "a page at a time");
    let (code, v) = request(&s.host, "GET", "/admin/api/calendar", &moderator, "").await;
    assert_eq!(code, 200, "{v}");
    assert!(s.server.store.audit_log(5).await.unwrap().iter().any(|a| a.action == "export"));
}

#[tokio::test]
async fn promotions_are_kept() {
    let store = Store::memory();
    let codes = vec!["cloth_blue".to_string()];
    let none: Vec<String> = Vec::new();
    let p = NewPromotion {
        title: "Sale",
        kind: "discount",
        percent: 20,
        codes: &codes,
        categories: &none,
        starts_at: 10,
        ends_at: 20,
        by: 0,
        at: 5,
    };
    let id = store.add_promotion(&p).await.unwrap();
    assert!(store.update_promotion(id, &NewPromotion { percent: 25, ..p.clone() }).await.unwrap());
    let list = store.promotions(10).await.unwrap();
    assert_eq!((list[0].percent, list[0].codes.clone()), (25, codes.clone()));
    assert!(store.cancel_promotion(id, 12).await.unwrap());
    assert!(!store.cancel_promotion(id, 13).await.unwrap());
    assert!(!store.update_promotion(id, &p).await.unwrap(), "called off");
}
