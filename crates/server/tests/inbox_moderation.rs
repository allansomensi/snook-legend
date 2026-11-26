//! The inbox and the moderation of 0.12: notifications kept until read
//! (and confirmed when important), the staff's messages, warnings and
//! mutes with their reasons, reports answered, ratings put right, games
//! called off, presents to many, the banned words, the pinned line and
//! mentions in the lounge, the history of names and countries, and the
//! timeline of a player. The stores (memory always, PostgreSQL when
//! `SNOOK_LEGEND_TEST_DATABASE_URL` points at a database that may be wiped)
//! and the server end to end.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use snook_legend_core::GameOverReason;
use snook_legend_core::stats::ShotStats;
use snook_legend_proto::inbox::{Action, Kind, MAX_KEPT};
use snook_legend_proto::moderation::{ChangeKind, TextPlace, TextRefusal};
use snook_legend_proto::replay::{Replay, Stroke};
use snook_legend_proto::{
    ClientMsg, OLDEST_PROTOCOL_VERSION, PROTOCOL_VERSION, ReportReason, Role, Rules, ServerMsg,
    parse_server,
};
use snook_legend_server::lounge::Settings;
use snook_legend_server::store::{MatchRecord, NewNotification, NewUser, Store};
use snook_legend_server::{Config, Server, serve};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

// ---------------------------------------------------------------- stores

fn new_user(name: &str) -> NewUser<'_> {
    NewUser {
        username: name,
        password_hash: "h",
        country: "BR",
        rating: 1200,
        role: Role::Player,
        ip: "1.2.3.4",
        minor: None,
    }
}

fn rated_game(players: [i64; 2], delta: [i32; 2], ended_at: i64) -> MatchRecord {
    MatchRecord {
        players,
        winner: 0,
        reason: GameOverReason::Resignation,
        rated: true,
        rules: Rules::Official,
        delta,
        ratings_after: [1200 + delta[0], 1200 + delta[1]],
        shots: 30,
        tournament: None,
        eight_win: false,
        runout: false,
        shutout: false,
        break_win: false,
        ended_at,
        replay: Some(Replay {
            seed: 9,
            breaker: 0,
            rules: Rules::Official,
            strokes: vec![Stroke::Resign { seat: 1 }],
            calls: false,
            eight_call: true,
        }),
        stats: [ShotStats::default(); 2],
        abandoned: None,
        room: Some(4242),
        counts: true,
    }
}

async fn exercise(store: Store) {
    let t = 1_800_000_000;
    let a = store.create_user(&new_user("ana")).await.unwrap().unwrap();
    let b = store.create_user(&new_user("bia")).await.unwrap().unwrap();

    // Notifications: newest first, read, taken out (not an important one
    // unread, nor a pinned one).
    let kind = |k: Kind| serde_json::to_string(&k).unwrap();
    let warning = kind(Kind::Warning);
    let message = kind(Kind::StaffMessage);
    fn note(
        user: i64,
        from: i64,
        kind: &str,
        important: bool,
        pinned_until: i64,
        at: i64,
    ) -> NewNotification<'_> {
        NewNotification {
            user_id: user,
            kind,
            text: "hello",
            from_id: Some(from),
            created_at: at,
            important,
            pinned_until,
            action: r#"{"to":"none"}"#,
        }
    }
    let new =
        |kind, important, pinned_until, at| note(a.id, b.id, kind, important, pinned_until, at);
    let w = store.add_notification(&new(&warning, true, t + 100, t), MAX_KEPT).await.unwrap();
    let m = store.add_notification(&new(&message, false, 0, t + 1), MAX_KEPT).await.unwrap();
    let list = store.notifications(a.id, 50).await.unwrap();
    assert_eq!(list.iter().map(|n| n.id).collect::<Vec<_>>(), [m, w]);
    assert_eq!(list[1].from, "bia");
    assert!(list[1].important && list[1].read_at.is_none());
    assert_eq!(store.delete_notifications(a.id, &[w], t + 2).await.unwrap(), 0, "important");
    assert_eq!(store.read_notifications(a.id, &[w, m], t + 3).await.unwrap(), 2);
    assert_eq!(store.read_notifications(a.id, &[w], t + 4).await.unwrap(), 0, "once");
    assert_eq!(store.delete_notifications(a.id, &[w], t + 5).await.unwrap(), 0, "pinned");
    assert_eq!(store.delete_notifications(a.id, &[w], t + 101).await.unwrap(), 1);
    assert_eq!(store.delete_notifications(b.id, &[m], t + 101).await.unwrap(), 0, "not hers");
    assert_eq!(store.notifications(a.id, 50).await.unwrap().len(), 1);
    // The staff still see it, with when it was read and taken out.
    let sent = store.notifications_sent(a.id, 50).await.unwrap();
    assert_eq!(sent.len(), 2);
    assert_eq!((sent[1].read_at, sent[1].deleted_at), (Some(t + 3), Some(t + 101)));
    // Past the most kept, the oldest read ones go (unread ones stay).
    for k in 0..(MAX_KEPT as i64 + 3) {
        store.add_notification(&new(&message, false, 0, t + 200 + k), MAX_KEPT).await.unwrap();
    }
    let kept = store.notifications(a.id, 1000).await.unwrap();
    assert_eq!(kept.len(), MAX_KEPT + 3, "unread ones stay");
    assert!(!kept.iter().any(|n| n.id == m), "the read one went first");
    let ids: Vec<i64> = kept.iter().map(|n| n.id).collect();
    store.read_notifications(a.id, &ids, t + 400).await.unwrap();
    store.add_notification(&new(&message, false, 0, t + 500), MAX_KEPT).await.unwrap();
    let kept = store.notifications(a.id, 1000).await.unwrap();
    assert_eq!(kept.len(), MAX_KEPT);
    assert_eq!(kept[0].created_at, t + 500, "the newest stays");

    // The banned words: the starting list, added, taken off.
    let words = store.banned_words().await.unwrap();
    assert_eq!(words.len(), snook_legend_server::wordfilter::SEED.len());
    assert!(store.add_banned_word("zonk", "en", b.id, t).await.unwrap());
    assert!(!store.add_banned_word("zonk", "en", b.id, t).await.unwrap());
    let words = store.banned_words().await.unwrap();
    let zonk = words.iter().find(|w| w.word == "zonk").unwrap();
    assert_eq!((zonk.lang.as_str(), zonk.added_by.as_str()), ("en", "bia"));
    assert!(store.remove_banned_word("zonk", "en").await.unwrap());
    assert!(!store.remove_banned_word("zonk", "en").await.unwrap());

    // Names and countries, newest first.
    store.add_profile_change(a.id, "country", "BR", "PT", t, None).await.unwrap();
    store.add_profile_change(a.id, "name", "ana0", "ana", t + 1, Some(b.id)).await.unwrap();
    let changes = store.profile_changes(a.id, 10).await.unwrap();
    assert_eq!(changes.len(), 2);
    assert_eq!((changes[0].kind.as_str(), changes[0].by.as_str()), ("name", "bia"));
    assert_eq!((changes[1].old.as_str(), changes[1].new.as_str()), ("BR", "PT"));

    // A rating put right: the peak follows, the history has it.
    let fresh = store
        .adjust_rating(a.id, Rules::Official, 50, "fixed", None, b.id, t + 10)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fresh.records.official.rating, 1250);
    assert_eq!(fresh.records.official.peak_rating, 1250);
    let fresh = store
        .adjust_rating(a.id, Rules::Official, -30, "farmed", None, b.id, t + 11)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fresh.records.official.rating, 1220);
    assert_eq!(fresh.records.official.peak_rating, 1220, "down by as much");
    assert_eq!(fresh.records.brazilian.rating, 1200, "the other rules untouched");
    let history = store.rating_history(a.id, Rules::Official, 10).await.unwrap();
    assert_eq!(history, [(t + 10, 1250), (t + 11, 1220)]);
    let adj = store.rating_adjustments(a.id, 10).await.unwrap();
    assert_eq!((adj[0].delta, adj[0].staff.as_str()), (-30, "bia"));

    // A game called off: the changes given back, the games and wins too,
    // the peak it brought worked out again; never twice.
    let mut game = rated_game([a.id, b.id], [20, -20], t + 20);
    game.ratings_after = [1240, 1180];
    let (users, id) = store.record_match(&game).await.unwrap().unwrap();
    let after = users[0].clone().unwrap();
    assert_eq!(after.records.official.rating, 1240);
    assert_eq!(after.records.official.peak_rating, 1240);
    let row = store.match_row(id).await.unwrap().unwrap();
    assert_eq!((row.names[0].as_str(), row.voided_at), ("ana", None));
    let voided = store.void_match(id, b.id, "cheating", t + 30).await.unwrap().unwrap();
    let (va, vb) = (voided[0].clone().unwrap(), voided[1].clone().unwrap());
    assert_eq!(va.records.official.rating, 1220);
    assert_eq!(va.records.official.peak_rating, 1220);
    assert_eq!((va.records.official.games, va.records.official.wins), (0, 0));
    assert_eq!(vb.records.official.rating, 1200);
    assert_eq!(vb.records.official.games, 0);
    assert!(store.void_match(id, b.id, "again", t + 31).await.unwrap().is_none());
    assert_eq!(store.match_row(id).await.unwrap().unwrap().voided_at, Some(t + 30));
    let history = store.rating_history(a.id, Rules::Official, 10).await.unwrap();
    assert_eq!(history.last(), Some(&(t + 30, 1220)));

    // How long the shots took, by the game's room.
    store.save_shot_times(4242, &[Some(35), None], t).await.unwrap();
    assert_eq!(store.shot_times(4242).await.unwrap(), Some(vec![Some(35), None]));
    assert_eq!(store.shot_times(1).await.unwrap(), None);

    // Reports sent.
    let r = snook_legend_server::store::NewReport {
        reporter: a.id,
        target: b.id,
        reason: "Spam".into(),
        details: String::new(),
        context: String::new(),
        at: t,
    };
    store.create_report(&r).await.unwrap();
    assert_eq!(store.reports_by(a.id, 10).await.unwrap().len(), 1);
    assert!(store.reports_by(b.id, 10).await.unwrap().is_empty());
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
    exercise(Store::postgres(&url, 3).await.unwrap()).await;
}

/// The starting list of banned words is the same in the database's
/// migration and in the memory store.
#[test]
fn the_seed_matches_the_migration() {
    let sql = include_str!("../migrations/0027_inbox_moderation.sql");
    let mut listed: Vec<(String, String)> = Vec::new();
    for part in sql.split("('").skip(1) {
        if let Some((word, rest)) = part.split_once("', '")
            && let Some((lang, _)) = rest.split_once("', 0)")
        {
            listed.push((word.to_string(), lang.to_string()));
        }
    }
    let seed: Vec<(String, String)> = snook_legend_server::wordfilter::SEED
        .iter()
        .map(|(w, l)| (w.to_string(), l.to_string()))
        .collect();
    assert_eq!(listed, seed);
}

// ---------------------------------------------------------------- server

const PASSWORD: &str = "correct-horse-1";

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Running {
    url: String,
    host: String,
    server: Arc<Server>,
}

/// A server with an administrator, "boss".
async fn start() -> Running {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let settings = Settings { max_connections_per_ip: 100, ..Settings::default() };
    let config = Config { bind: addr, settings, admins: vec!["boss".into()], ..Config::default() };
    let server = Server::with_store(config, Store::memory()).await;
    let hash = server.hash_password(PASSWORD).await.unwrap();
    let boss = NewUser {
        username: "boss",
        password_hash: &hash,
        country: "BR",
        rating: 1200,
        role: Role::Player,
        ip: "1.2.3.4",
        minor: Some(false),
    };
    server.store.create_user(&boss).await.unwrap().expect("a new account");
    tokio::spawn(serve(listener, server.clone()));
    Running { url: format!("ws://{addr}/ws"), host: addr.to_string(), server }
}

struct Client {
    ws: Ws,
}

impl Client {
    async fn connect(url: &str, version: u32) -> Client {
        let (ws, _) = connect_async(url).await.unwrap();
        let mut c = Client { ws };
        c.send(ClientMsg::Hello { version, rules: Rules::Brazilian }).await;
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

    async fn register_as(url: &str, name: &str, version: u32) -> Client {
        let mut c = Client::connect(url, version).await;
        c.send(ClientMsg::Register {
            username: name.into(),
            password: PASSWORD.into(),
            country: "BR".into(),
            minor: Some(false),
        })
        .await;
        c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
        c
    }

    async fn register(url: &str, name: &str) -> Client {
        Client::register_as(url, name, PROTOCOL_VERSION).await
    }

    async fn login(url: &str, name: &str) -> Client {
        let mut c = Client::connect(url, PROTOCOL_VERSION).await;
        c.send(ClientMsg::Login { username: name.into(), password: PASSWORD.into() }).await;
        c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
        c
    }

    /// The next notification pushed.
    async fn notified(&mut self) -> (snook_legend_proto::inbox::Notification, u32) {
        match self.expect("a notification", |m| matches!(m, ServerMsg::InboxNew { .. })).await {
            ServerMsg::InboxNew { item, unread } => (item, unread),
            _ => unreachable!(),
        }
    }

    async fn inbox(&mut self) -> (Vec<snook_legend_proto::inbox::Notification>, u32) {
        match self.expect("the inbox", |m| matches!(m, ServerMsg::Inbox { .. })).await {
            ServerMsg::Inbox { items, unread } => (items, unread),
            _ => unreachable!(),
        }
    }

    async fn refused(&mut self) -> (TextPlace, TextRefusal) {
        match self.expect("a refusal", |m| matches!(m, ServerMsg::TextRefused { .. })).await {
            ServerMsg::TextRefused { place, refusal } => (place, refusal),
            _ => unreachable!(),
        }
    }
}

/// A small HTTP client over a raw socket (no extra dependencies).
async fn request(host: &str, method: &str, path: &str, token: &str, body: &str) -> (u16, Value) {
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
    let body = out.split_once("\r\n\r\n").map(|x| x.1).unwrap_or("");
    (code, serde_json::from_str(body).unwrap_or(Value::Null))
}

async fn token(host: &str, name: &str) -> String {
    let body = format!(r#"{{"username":"{name}","password":"{PASSWORD}"}}"#);
    let (code, v) = request(host, "POST", "/admin/api/login", "", &body).await;
    assert_eq!(code, 200, "{v}");
    v["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn the_inbox_keeps_what_the_staff_say() {
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let post = |path: &str, body: &str| {
        let (host, boss, path, body) =
            (s.host.clone(), boss.clone(), path.to_string(), body.to_string());
        async move { request(&host, "POST", &path, &boss, &body).await }
    };
    let mut ana = Client::register(&s.url, "ana").await;
    let (items, unread) = ana.inbox().await;
    assert!(items.is_empty() && unread == 0);

    // A preview sends nothing: the notification, worded in three languages.
    let (code, v) = post(
        "/admin/api/users/ana/message",
        r#"{"text":"Welcome!","important":true,"preview":true}"#,
    )
    .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["notification"]["kind"], "staff_message");
    assert_eq!(v["rendered"]["pt"]["title"], "Mensagem da equipe");
    assert_eq!(v["rendered"]["en"]["body"], "Welcome!");

    // Sent: pushed at once, with the unread count.
    let (code, v) =
        post("/admin/api/users/ana/message", r#"{"text":"Welcome!","important":true}"#).await;
    assert_eq!(code, 200, "{v}");
    let id = v["id"].as_i64().unwrap();
    let (item, unread) = ana.notified().await;
    assert_eq!((item.id, item.kind.clone(), unread), (id, Kind::StaffMessage, 1));
    assert_eq!((item.text.as_str(), item.from.as_str()), ("Welcome!", "boss"));
    assert!(item.important);

    // An important one stays until confirmed; then it can go.
    ana.send(ClientMsg::InboxDelete { ids: vec![id] }).await;
    let (items, _) = ana.inbox().await;
    assert_eq!(items.len(), 1);
    ana.send(ClientMsg::InboxRead { ids: vec![id] }).await;
    let (items, unread) = ana.inbox().await;
    assert!(items[0].read_at > 0 && unread == 0);
    // The portal sees when it was read.
    let (_, v) = request(&s.host, "GET", "/admin/api/users/ana/notifications", &boss, "").await;
    assert!(v["notifications"][0]["read_at"].is_i64(), "{v}");
    assert_eq!(v["notifications"][0]["title"], "Message from the staff");
    let (_, v) = request(&s.host, "GET", "/admin/api/users/ana", &boss, "").await;
    assert_eq!(v["notifications"][0]["id"], id, "{v}");
    ana.send(ClientMsg::InboxDelete { ids: vec![id] }).await;
    let (items, _) = ana.inbox().await;
    assert!(items.is_empty());

    // A warning: pinned for a week, to confirm.
    let (code, _) = post("/admin/api/users/ana/warn", r#"{"reason":"Mind your words."}"#).await;
    assert_eq!(code, 200);
    let (item, _) = ana.notified().await;
    assert_eq!(item.kind, Kind::Warning);
    assert!(item.important && item.pinned_until > snook_legend_server::now() + 6 * 86_400);
    assert_eq!(item.text, "Mind your words.");

    // A mute: pinned until it ends; the chat says why and until when.
    let (code, _) = post("/admin/api/users/ana/mute", r#"{"minutes":30,"reason":"spam"}"#).await;
    assert_eq!(code, 200);
    let (item, _) = ana.notified().await;
    let Kind::Muted { until } = item.kind else { panic!("{item:?}") };
    assert_eq!(item.pinned_until, until);
    ana.send(ClientMsg::Chat { text: "hello?".into() }).await;
    let (place, refusal) = ana.refused().await;
    assert_eq!(place, TextPlace::Chat);
    assert_eq!(refusal, TextRefusal::Muted { until, reason: "spam".into() });
    // Signing in again, the reason is still known.
    drop(ana);
    let mut ana = Client::login(&s.url, "ana").await;
    let (items, unread) = ana.inbox().await;
    assert_eq!((items.len(), unread), (2, 2));
    ana.send(ClientMsg::SetStatusMessage { text: "hi".into() }).await;
    let (place, refusal) = ana.refused().await;
    assert_eq!(
        (place, refusal),
        (TextPlace::Status, TextRefusal::Muted { until, reason: "spam".into() })
    );
    let (code, _) = post("/admin/api/users/ana/unmute", r#"{"reason":"ok now"}"#).await;
    assert_eq!(code, 200);
    let (item, _) = ana.notified().await;
    assert_eq!(item.kind, Kind::Unmuted);

    // Presents from the staff are in the inbox too.
    let (code, _) = post("/admin/api/users/ana/coins", r#"{"coins":300,"reason":"contest"}"#).await;
    assert_eq!(code, 200);
    let (item, _) = ana.notified().await;
    assert_eq!(item.kind, Kind::StaffGift { coins: 300, item: String::new(), vip_days: 0 });

    // Nothing to preview where the player isn't told.
    let (code, _) = post("/admin/api/users/ana/kick", r#"{"preview":true}"#).await;
    assert_eq!(code, 400);
}

/// A game older than the inbox is told as it was: a notice, an error.
#[tokio::test]
async fn older_games_are_told_as_before() {
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let mut old = Client::register_as(&s.url, "olga", OLDEST_PROTOCOL_VERSION).await;
    let (code, _) =
        request(&s.host, "POST", "/admin/api/users/olga/warn", &boss, r#"{"reason":"Be nice."}"#)
            .await;
    assert_eq!(code, 200);
    old.expect(
        "the warning",
        |m| matches!(m, ServerMsg::Notice { text } if text == "Warning from the staff: Be nice."),
    )
    .await;
    let (code, _) =
        request(&s.host, "POST", "/admin/api/users/olga/message", &boss, r#"{"text":"Hi there"}"#)
            .await;
    assert_eq!(code, 200);
    old.expect(
        "the message",
        |m| matches!(m, ServerMsg::Notice { text } if text == "Message from the staff Hi there"),
    )
    .await;
    old.send(ClientMsg::Chat { text: "you are a puta".into() }).await;
    old.expect("refused", |m| {
        matches!(m, ServerMsg::Error { message } if message == "That word isn't allowed here: puta.")
    })
    .await;
}

#[tokio::test]
async fn banned_words_are_kept_out() {
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let mut ana = Client::register(&s.url, "ana").await;
    let mut bia = Client::register(&s.url, "bia").await;

    // The starting list, with its evasions.
    ana.send(ClientMsg::Chat { text: "seu p.u.t.a".into() }).await;
    assert_eq!(
        ana.refused().await,
        (TextPlace::Chat, TextRefusal::BannedWord { word: "puta".into() })
    );
    ana.send(ClientMsg::PostSeek { text: "fuuuck".into(), rated: false, rules: Rules::Brazilian })
        .await;
    assert_eq!(
        ana.refused().await,
        (TextPlace::Seek, TextRefusal::BannedWord { word: "fuck".into() })
    );
    ana.send(ClientMsg::SetStatusMessage { text: "mierda total".into() }).await;
    assert_eq!(ana.refused().await.1, TextRefusal::BannedWord { word: "mierda".into() });
    // An innocent line goes through.
    ana.send(ClientMsg::Chat { text: "boa disputa, computador!".into() }).await;
    bia.expect(
        "the line",
        |m| matches!(m, ServerMsg::Chat { line } if line.text.starts_with("boa")),
    )
    .await;

    // The staff add a word, see what it catches, and take it off.
    let (code, _) =
        request(&s.host, "POST", "/admin/api/words", &boss, r#"{"word":"Zonk","lang":"en"}"#).await;
    assert_eq!(code, 200);
    let (code, _) =
        request(&s.host, "POST", "/admin/api/words", &boss, r#"{"word":"zonk","lang":"en"}"#).await;
    assert_eq!(code, 409);
    let (code, _) =
        request(&s.host, "POST", "/admin/api/words", &boss, r#"{"word":"x","lang":"EN"}"#).await;
    assert_eq!(code, 400);
    let (_, v) =
        request(&s.host, "POST", "/admin/api/words/preview", &boss, r#"{"text":"Z0NK you"}"#).await;
    assert_eq!((v["blocked"].as_bool(), v["word"].as_str()), (Some(true), Some("zonk")), "{v}");
    ana.send(ClientMsg::Chat { text: "zzzonk".into() }).await;
    assert_eq!(ana.refused().await.1, TextRefusal::BannedWord { word: "zonk".into() });
    let (code, _) = request(
        &s.host,
        "POST",
        "/admin/api/words/remove",
        &boss,
        r#"{"word":"zonk","lang":"en"}"#,
    )
    .await;
    assert_eq!(code, 200);
    let (_, v) =
        request(&s.host, "POST", "/admin/api/words/preview", &boss, r#"{"text":"zonk"}"#).await;
    assert_eq!(v["blocked"], false);
    let (_, v) = request(&s.host, "GET", "/admin/api/audit?action=banned-word", &boss, "").await;
    assert_eq!(v["entries"][0]["details"], "zonk", "{v}");

    // A new name with one is refused.
    let mut c = Client::connect(&s.url, PROTOCOL_VERSION).await;
    c.send(ClientMsg::Register {
        username: "xX_fuck_Xx".into(),
        password: PASSWORD.into(),
        country: "BR".into(),
        minor: Some(false),
    })
    .await;
    c.expect(
        "refused",
        |m| matches!(m, ServerMsg::AuthError { reason } if reason.contains("fuck")),
    )
    .await;
}

#[tokio::test]
async fn reports_acted_on_are_answered() {
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let mut ana = Client::register(&s.url, "ana").await;
    let mut bia = Client::register(&s.url, "bia").await;
    let _troll = Client::register(&s.url, "troll").await;
    let report = ClientMsg::Report {
        username: "troll".into(),
        reason: ReportReason::Harassment,
        details: String::new(),
    };
    ana.send(report.clone()).await;
    ana.expect("received", |m| matches!(m, ServerMsg::Notice { .. })).await;
    bia.send(report).await;
    bia.expect("received", |m| matches!(m, ServerMsg::Notice { .. })).await;
    let (_, v) = request(&s.host, "GET", "/admin/api/reports", &boss, "").await;
    let ids: Vec<i64> =
        v["reports"].as_array().unwrap().iter().map(|r| r["id"].as_i64().unwrap()).collect();
    assert_eq!(ids.len(), 2);
    let (code, _) = request(
        &s.host,
        "POST",
        &format!("/admin/api/reports/{}", ids[0]),
        &boss,
        r#"{"status":"actioned","note":"muted"}"#,
    )
    .await;
    assert_eq!(code, 200);
    // Both who reported them hear that the staff acted (never how).
    for c in [&mut ana, &mut bia] {
        let (item, _) = c.notified().await;
        assert_eq!(item.kind, Kind::ReportActed { username: "troll".into() });
        assert!(item.text.is_empty());
    }
}

/// A rated game recorded between two players, as the lounge does.
async fn record_rated(server: &Server, a: &str, b: &str) -> i64 {
    let ua = server.store.user_by_name(a).await.unwrap().unwrap();
    let ub = server.store.user_by_name(b).await.unwrap().unwrap();
    let mut game = rated_game([ua.id, ub.id], [16, -16], snook_legend_server::now());
    game.room = None;
    server.store.record_match(&game).await.unwrap().unwrap().1
}

#[tokio::test]
async fn ratings_put_right_and_games_called_off() {
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let post = |path: String, body: &str| {
        let (host, boss, body) = (s.host.clone(), boss.clone(), body.to_string());
        async move { request(&host, "POST", &path, &boss, &body).await }
    };
    let mut ana = Client::register(&s.url, "ana").await;
    let mut bia = Client::register(&s.url, "bia").await;
    let id = record_rated(&s.server, "ana", "bia").await;

    // A rating put right, about that game.
    let body = format!(r#"{{"rules":"official","delta":-10,"reason":"farmed","match_id":{id}}}"#);
    let (code, v) = post("/admin/api/users/ana/rating".into(), &body).await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["user"]["records"]["official"]["rating"], 1206);
    let (item, _) = ana.notified().await;
    assert_eq!(
        item.kind,
        Kind::RatingAdjusted { rules: Rules::Official, delta: -10, rating: 1206, match_id: id }
    );
    assert_eq!(item.action, Action::Replay { match_id: id });
    let (code, _) = post(
        "/admin/api/users/ana/rating".into(),
        r#"{"rules":"official","rating":9000,"reason":"x"}"#,
    )
    .await;
    assert_eq!(code, 400);
    // The largest numbers are refused the same (not a sum that overflows).
    for body in [
        r#"{"rules":"official","delta":2147483647,"reason":"x"}"#,
        r#"{"rules":"official","delta":-2147483648,"reason":"x"}"#,
        r#"{"rules":"official","rating":-2147483648,"reason":"x"}"#,
    ] {
        let (code, v) = post("/admin/api/users/ana/rating".into(), body).await;
        assert_eq!(code, 400, "{body}: {v}");
    }

    // The game shot by shot.
    let (code, v) = request(&s.host, "GET", &format!("/admin/api/matches/{id}"), &boss, "").await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["strokes"][0]["kind"], "resign");
    assert_eq!(v["match"]["names"][0], "ana");

    // Called off: both changes given back, both told.
    let (code, v) =
        post(format!("/admin/api/matches/{id}/void"), r#"{"reason":"cheating","preview":true}"#)
            .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["players"][1]["notification"]["opponent"], "ana");
    let (code, v) = post(format!("/admin/api/matches/{id}/void"), r#"{"reason":"cheating"}"#).await;
    assert_eq!(code, 200, "{v}");
    let ana_now = s.server.store.user_by_name("ana").await.unwrap().unwrap();
    let bia_now = s.server.store.user_by_name("bia").await.unwrap().unwrap();
    assert_eq!(ana_now.records.official.rating, 1190);
    assert_eq!(bia_now.records.official.rating, 1200);
    let (item, _) = ana.notified().await;
    assert_eq!(item.kind, Kind::MatchVoided { match_id: id, opponent: "bia".into(), delta: 16 });
    let (item, _) = bia.notified().await;
    assert_eq!(item.kind, Kind::MatchVoided { match_id: id, opponent: "ana".into(), delta: -16 });
    let (code, _) = post(format!("/admin/api/matches/{id}/void"), r#"{"reason":"again"}"#).await;
    assert_eq!(code, 409);

    // The timeline has it all, newest first.
    let (code, v) =
        request(&s.host, "GET", "/admin/api/users/ana/timeline?limit=50", &boss, "").await;
    assert_eq!(code, 200, "{v}");
    let kinds: Vec<&str> =
        v["items"].as_array().unwrap().iter().map(|i| i["type"].as_str().unwrap()).collect();
    for k in ["account", "match", "staff_action", "notification"] {
        assert!(kinds.contains(&k), "{k}: {kinds:?}");
    }
    let ats: Vec<i64> =
        v["items"].as_array().unwrap().iter().map(|i| i["at"].as_i64().unwrap()).collect();
    assert!(ats.windows(2).all(|w| w[0] >= w[1]));
}

#[tokio::test]
async fn presents_to_many_at_once() {
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let mut ana = Client::register(&s.url, "ana").await;
    let _bia = Client::register(&s.url, "bia").await;
    let ana_id = s.server.store.user_by_name("ana").await.unwrap().unwrap().id;
    s.server.store.grant_item(ana_id, "chalk_red", "shop", 1).await.unwrap();
    let body = r#"{"text":"ana, bia\nnobody @bia","coins":100,"item":"chalk_red","reason":"cup","dry_run":true}"#;
    let (code, v) = request(&s.host, "POST", "/admin/api/gifts/batch", &boss, body).await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["players"], 2);
    assert_eq!(v["unknown"], serde_json::json!(["nobody"]));
    assert_eq!(v["already_own"], serde_json::json!(["ana"]));
    let body = body.replace(r#","dry_run":true"#, "");
    let (code, v) = request(&s.host, "POST", "/admin/api/gifts/batch", &boss, &body).await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["given"], serde_json::json!(["ana", "bia"]));
    let (item, _) = ana.notified().await;
    // She had the item: the coins alone.
    assert_eq!(item.kind, Kind::StaffGift { coins: 100, item: String::new(), vip_days: 0 });
    let bia = s.server.store.user_by_name("bia").await.unwrap().unwrap();
    assert!(s.server.store.wallet(bia.id).await.unwrap().owned.contains(&"chalk_red".to_string()));
    let (_, v) = request(&s.host, "GET", "/admin/api/audit?action=gift-batch", &boss, "").await;
    assert_eq!(v["entries"].as_array().unwrap().len(), 1, "{v}");
    // Moderators can't.
    let (code, _) =
        request(&s.host, "POST", "/admin/api/users/bia/role", &boss, r#"{"role":"moderator"}"#)
            .await;
    assert_eq!(code, 200);
    let mods = token(&s.host, "bia").await;
    let (code, _) = request(&s.host, "POST", "/admin/api/gifts/batch", &mods, &body).await;
    assert_eq!(code, 403);
}

#[tokio::test]
async fn the_lounge_pins_a_line_and_mentions_players() {
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let mut ana = Client::register(&s.url, "ana").await;
    let mut bia = Client::register(&s.url, "bia").await;
    // An account offline is mentioned too; a name nobody has isn't.
    let _ = Client::register(&s.url, "caio").await;
    ana.send(ClientMsg::Chat { text: "hi @BIA and @caio and @nobody".into() }).await;
    let line = match bia
        .expect("the line", |m| matches!(m, ServerMsg::Chat { line } if line.from == "ana"))
        .await
    {
        ServerMsg::Chat { line } => line,
        _ => unreachable!(),
    };
    assert_eq!(line.mentions, ["bia", "caio"]);

    // Pinned from the portal: everyone sees it, and who signs in later.
    let (code, v) =
        request(&s.host, "POST", &format!("/admin/api/chat/{}/pin", line.id), &boss, "").await;
    assert_eq!(code, 200, "{v}");
    bia.expect(
        "pinned",
        |m| matches!(m, ServerMsg::ChatPinned { line: Some(l) } if l.id == line.id),
    )
    .await;
    let mut caio = Client::login(&s.url, "caio").await;
    caio.expect("pinned", |m| matches!(m, ServerMsg::ChatPinned { line: Some(_) })).await;
    // A player can't pin; the staff can, from the game too.
    ana.send(ClientMsg::UnpinChat).await;
    ana.expect("refused", |m| matches!(m, ServerMsg::Error { .. })).await;
    let (code, _) = request(&s.host, "POST", "/admin/api/chat/unpin", &boss, "").await;
    assert_eq!(code, 200);
    bia.expect("unpinned", |m| matches!(m, ServerMsg::ChatPinned { line: None })).await;
    let (code, _) = request(&s.host, "POST", "/admin/api/chat/unpin", &boss, "").await;
    assert_eq!(code, 400);
    // A deleted line comes down with it.
    request(&s.host, "POST", &format!("/admin/api/chat/{}/pin", line.id), &boss, "").await;
    bia.expect("pinned", |m| matches!(m, ServerMsg::ChatPinned { line: Some(_) })).await;
    request(&s.host, "POST", &format!("/admin/api/chat/{}/delete", line.id), &boss, "{}").await;
    bia.expect("unpinned", |m| matches!(m, ServerMsg::ChatPinned { line: None })).await;
}

#[tokio::test]
async fn names_and_countries_are_remembered() {
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let mut ana = Client::register(&s.url, "ana").await;
    ana.send(ClientMsg::SetCountry { country: "PT".into() }).await;
    // (Once stored, the profile has it.)
    let mut tries = 0;
    let profile = loop {
        ana.send(ClientMsg::Profile { username: "ana".into() }).await;
        let ServerMsg::Profile { profile } =
            ana.expect("profile", |m| matches!(m, ServerMsg::Profile { .. })).await
        else {
            unreachable!()
        };
        if !profile.renames.is_empty() || tries > 20 {
            break profile;
        }
        tries += 1;
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(profile.renames.len(), 1);
    assert_eq!(profile.renames[0].kind, ChangeKind::Country);
    assert_eq!((profile.renames[0].old.as_str(), profile.renames[0].new.as_str()), ("BR", "PT"));
    let (_, v) = request(&s.host, "GET", "/admin/api/users/ana", &boss, "").await;
    assert_eq!(v["renames"][0]["new"], "PT", "{v}");
    // The portal's own account shows its face.
    let (_, v) = request(&s.host, "GET", "/admin/api/me", &boss, "").await;
    assert!(v["avatar"].is_string(), "{v}");
}
