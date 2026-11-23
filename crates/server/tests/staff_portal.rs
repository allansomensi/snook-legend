//! The staff portal's API end to end: the list of players filtered,
//! warnings (shown at once, or at the next sign-in), notes, renames by the
//! staff, items taken back, the lounge chat moderated, tournaments called
//! off, the economy, the server's state and the audit log searched — and
//! what a moderator may not do.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use snook_legend_proto::{ClientMsg, PROTOCOL_VERSION, Role, Rules, ServerMsg, parse_server};
use snook_legend_server::lounge::Settings;
use snook_legend_server::store::{NewUser, Store};
use snook_legend_server::{Config, Server, serve};
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

/// A server with an administrator, "boss" (an account made before the
/// name is listed).
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
        // Adults: their chat is on (it is off until the age group is given).
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
    async fn connect(url: &str) -> Client {
        let (ws, _) = connect_async(url).await.unwrap();
        let mut c = Client { ws };
        c.send(ClientMsg::Hello {
            version: PROTOCOL_VERSION,
            rules: Rules::Brazilian,
            store_sales: None,
        })
        .await;
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

    async fn register(url: &str, name: &str) -> Client {
        let mut c = Client::connect(url).await;
        c.send(ClientMsg::Register {
            username: name.into(),
            password: PASSWORD.into(),
            country: "BR".into(),
            // Adults: their chat is on (it is off until the age group is given).
            minor: Some(false),
        })
        .await;
        c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
        c
    }

    async fn login(url: &str, name: &str) -> Client {
        let mut c = Client::connect(url).await;
        c.send(ClientMsg::Login { username: name.into(), password: PASSWORD.into() }).await;
        c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
        c
    }

    /// Waits for a notice containing `part`.
    async fn notice(&mut self, part: &str) -> String {
        match self
            .expect(part, |m| matches!(m, ServerMsg::Notice { text } if text.contains(part)))
            .await
        {
            ServerMsg::Notice { text } => text,
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

/// The usernames on a page of the list.
fn names(v: &Value) -> Vec<String> {
    v["users"].as_array().unwrap().iter().map(|u| u["username"].as_str().unwrap().into()).collect()
}

#[tokio::test]
async fn the_portal_manages_players() {
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let get = |path: &str| {
        let (host, boss, path) = (s.host.clone(), boss.clone(), path.to_string());
        async move { request(&host, "GET", &path, &boss, "").await }
    };
    let post = |path: &str, body: &str| {
        let (host, boss, path, body) =
            (s.host.clone(), boss.clone(), path.to_string(), body.to_string());
        async move { request(&host, "POST", &path, &boss, &body).await }
    };

    let mut ana = Client::register(&s.url, "ana").await;
    let bia = Client::register(&s.url, "bia").await;
    drop(bia);
    let _caio = Client::register(&s.url, "caio").await;

    // The list: online now, by name; a filter that finds nobody.
    let (code, v) = get("/admin/api/users?filter=online&sort=name").await;
    assert_eq!(code, 200, "{v}");
    let online = names(&v);
    assert!(online.contains(&"ana".to_string()) && online.contains(&"caio".to_string()), "{v}");
    assert!(online.windows(2).all(|w| w[0].to_lowercase() <= w[1].to_lowercase()));
    let (_, v) = get("/admin/api/users?filter=banned").await;
    assert_eq!(v["total"], 0);
    let (_, v) = get("/admin/api/users?q=AN&sort=rating-official").await;
    assert_eq!(names(&v), ["ana"]);

    // A warning reaches a player online at once…
    let (code, v) = post("/admin/api/users/ana/warn", r#"{"reason":"Mind your words."}"#).await;
    assert_eq!(code, 200, "{v}");
    let text = ana.notice("Mind your words.").await;
    assert!(text.starts_with("Warning from the staff"), "{text}");
    // …and an offline one when they next sign in, once.
    let (code, _) = post("/admin/api/users/bia/warn", r#"{"reason":"No spamming."}"#).await;
    assert_eq!(code, 200);
    let mut bia = Client::login(&s.url, "bia").await;
    bia.notice("No spamming.").await;
    let (_, v) = get("/admin/api/users/bia").await;
    let warning = &v["notes"][0];
    assert_eq!(
        (warning["kind"].as_str(), warning["author"].as_str()),
        (Some("warning"), Some("boss"))
    );
    assert!(warning["delivered_at"].is_i64(), "{v}");
    assert!(v["presence"].is_object(), "online: {v}");
    // An empty warning is refused.
    let (code, _) = post("/admin/api/users/bia/warn", r#"{"reason":"   "}"#).await;
    assert_eq!(code, 400);

    // Notes among the staff; the actions on a player show on their page.
    let (code, _) = post("/admin/api/users/caio/note", r#"{"reason":"Reported twice."}"#).await;
    assert_eq!(code, 200);
    let (code, _) = post("/admin/api/users/caio/mute", r#"{"minutes":15,"reason":"rude"}"#).await;
    assert_eq!(code, 200);
    let (_, v) = get("/admin/api/users/caio").await;
    let note_id = v["notes"][0]["id"].as_i64().unwrap();
    assert_eq!(v["notes"][0]["text"], "Reported twice.");
    let actions: Vec<&str> =
        v["moderation"].as_array().unwrap().iter().map(|e| e["action"].as_str().unwrap()).collect();
    assert_eq!(actions, ["mute", "note"]);
    let (_, v) = get("/admin/api/users?filter=muted").await;
    assert_eq!(names(&v), ["caio"]);

    // A moderator: warns and notes, but neither renames nor removes the
    // administrator's notes.
    let (code, _) = post("/admin/api/users/bia/role", r#"{"role":"moderator"}"#).await;
    assert_eq!(code, 200);
    let mods = token(&s.host, "bia").await;
    let (code, _) =
        request(&s.host, "POST", "/admin/api/users/caio/warn", &mods, r#"{"reason":"Calm down."}"#)
            .await;
    assert_eq!(code, 200);
    let (code, _) =
        request(&s.host, "POST", "/admin/api/users/caio/rename", &mods, r#"{"name":"caio2"}"#)
            .await;
    assert_eq!(code, 403);
    let (code, _) =
        request(&s.host, "DELETE", &format!("/admin/api/notes/{note_id}"), &mods, "").await;
    assert_eq!(code, 404, "not their note");
    let (code, _) = get("/admin/api/users?filter=staff").await;
    assert_eq!(code, 200);
    let (code, _) =
        request(&s.host, "DELETE", &format!("/admin/api/notes/{note_id}"), &boss, "").await;
    assert_eq!(code, 200);

    // A rename by the staff: the player is signed out and signs in again
    // under the new name, with all their own changes left.
    let (code, v) =
        post("/admin/api/users/ana/rename", r#"{"name":"ana_clean","reason":"name"}"#).await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["user"]["username"], "ana_clean");
    assert_eq!(v["user"]["name_changes"], 0);
    ana.expect("signed out", |m| matches!(m, ServerMsg::Kicked { .. })).await;
    let (code, _) = post("/admin/api/users/ana_clean/rename", r#"{"name":"caio"}"#).await;
    assert_eq!(code, 409, "taken");
    let (code, _) = post("/admin/api/users/ana_clean/rename", r#"{"name":"a b"}"#).await;
    assert_eq!(code, 400);

    // An item given, then taken back (with a reason); not twice.
    let (code, _) =
        post("/admin/api/users/caio/item", r#"{"item":"chalk_red","reason":"test"}"#).await;
    assert_eq!(code, 200);
    let (code, _) = post("/admin/api/users/caio/revoke", r#"{"item":"chalk_red"}"#).await;
    assert_eq!(code, 400, "a reason is needed");
    let (code, _) =
        post("/admin/api/users/caio/revoke", r#"{"item":"chalk_red","reason":"by mistake"}"#).await;
    assert_eq!(code, 200);
    let (code, _) =
        post("/admin/api/users/caio/revoke", r#"{"item":"chalk_red","reason":"again"}"#).await;
    assert_eq!(code, 400);

    // The audit log, searched: by target, by action, by words, paged.
    let (_, v) = get("/admin/api/audit?target=CAIO").await;
    let entries = v["entries"].as_array().unwrap();
    assert!(entries.iter().all(|e| e["target"].as_str().unwrap().eq_ignore_ascii_case("caio")));
    assert!(entries.iter().any(|e| e["action"] == "revoke"));
    // Under the new name (the old one may be someone else's next).
    let (_, v) = get("/admin/api/audit?action=rename").await;
    assert_eq!(v["entries"][0]["target"], "ana_clean");
    assert_eq!(v["entries"][0]["details"], "from ana: name");
    let (_, v) = get("/admin/api/audit?q=mistake").await;
    assert_eq!(v["entries"].as_array().unwrap().len(), 1);
    assert!(v["next"].is_null());
}

/// What only administrators see (e-mails, payments, where the staff sign
/// in from), and reports a staff member can't answer: those about
/// themselves, or about another member of the staff.
#[tokio::test]
async fn moderators_see_less_and_answer_no_reports_about_themselves() {
    use snook_legend_server::store::NewReport;
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let _gil = Client::register(&s.url, "gil").await;
    let _hana = Client::register(&s.url, "hana").await;
    let gil = s.server.store.user_by_name("gil").await.unwrap().unwrap();
    let hana = s.server.store.user_by_name("hana").await.unwrap().unwrap();
    s.server.store.set_recovery_email(gil.id, Some("gil@example.com")).await.unwrap();
    let (code, _) =
        request(&s.host, "POST", "/admin/api/users/hana/role", &boss, r#"{"role":"moderator"}"#)
            .await;
    assert_eq!(code, 200);
    let mods = token(&s.host, "hana").await;

    // The administrator sees the e-mail; the moderator doesn't.
    let (_, v) = request(&s.host, "GET", "/admin/api/users/gil", &boss, "").await;
    assert_eq!(v["user"]["recovery_email"], "gil@example.com", "{v}");
    assert_eq!(v["private_hidden"], false);
    let (code, v) = request(&s.host, "GET", "/admin/api/users/gil", &mods, "").await;
    assert_eq!(code, 200, "{v}");
    assert!(v["user"]["recovery_email"].is_null(), "{v}");
    assert_eq!(v["private_hidden"], true);
    assert_eq!(v["payments"], serde_json::json!([]));
    // Nor finds an account by its e-mail (the administrator does).
    let (_, v) = request(&s.host, "GET", "/admin/api/users?q=gil@example.com", &mods, "").await;
    assert_eq!(v["total"], 0, "{v}");
    let (_, v) = request(&s.host, "GET", "/admin/api/users?q=gil@example.com", &boss, "").await;
    assert_eq!(names(&v), ["gil"]);

    // A report about the moderator is for someone else to answer; one
    // about the administrator is beyond a moderator.
    let report = |reporter: i64, target: i64| NewReport {
        reporter,
        target,
        reason: "Offensive language".into(),
        details: String::new(),
        context: String::new(),
        at: 1,
    };
    let store = &s.server.store;
    let boss_id = store.user_by_name("boss").await.unwrap().unwrap().id;
    let about_hana =
        store.create_report_with_game(&report(gil.id, hana.id), None, None).await.unwrap();
    let about_boss =
        store.create_report_with_game(&report(gil.id, boss_id), None, None).await.unwrap();
    let resolve = |id: i64, token: &str| {
        let (host, token) = (s.host.clone(), token.to_string());
        async move {
            let path = format!("/admin/api/reports/{id}");
            request(&host, "POST", &path, &token, r#"{"status":"dismissed"}"#).await.0
        }
    };
    assert_eq!(resolve(about_hana, &mods).await, 403, "about themselves");
    assert_eq!(resolve(about_boss, &mods).await, 403, "about an administrator");
    assert_eq!(resolve(about_hana, &boss).await, 200);
    // An administrator answers reports about themselves (with no other
    // administrator, nobody else could), in the audit log like any other.
    assert_eq!(resolve(about_boss, &boss).await, 200);
    let (_, v) = request(&s.host, "GET", "/admin/api/audit?action=report", &boss, "").await;
    assert!(
        v["entries"].as_array().unwrap().iter().any(|e| e["target"] == format!("#{about_boss}")),
        "{v}"
    );
    assert_eq!(resolve(999_999, &boss).await, 404);
}

#[tokio::test]
async fn the_portal_moderates_the_lounge() {
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let mut dan = Client::register(&s.url, "dan").await;
    let mut eva = Client::register(&s.url, "eva").await;

    // The chat as the staff see it; a line deleted (shown as deleted by the
    // staff), then removed without a trace (administrators only).
    dan.send(ClientMsg::Chat { text: "something rude".into() }).await;
    eva.expect("line", |m| matches!(m, ServerMsg::Chat { line } if line.text == "something rude"))
        .await;
    let (code, v) = request(&s.host, "GET", "/admin/api/chat", &boss, "").await;
    assert_eq!(code, 200);
    let line = v["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["text"] == "something rude")
        .cloned()
        .unwrap();
    assert!(line["deleted"].is_null());
    let id = line["id"].as_u64().unwrap();
    let path = format!("/admin/api/chat/{id}/delete");
    let (code, _) = request(&s.host, "POST", &path, &boss, "{}").await;
    assert_eq!(code, 200);
    eva.expect("deleted", |m| matches!(m, ServerMsg::ChatDeleted { id: i, .. } if *i == id)).await;
    let (code, _) = request(&s.host, "POST", &path, &boss, "{}").await;
    assert_eq!(code, 400, "deleted already");
    let (_, v) = request(&s.host, "GET", "/admin/api/chat", &boss, "").await;
    let seen = v["lines"].as_array().unwrap().iter().find(|l| l["id"] == id).cloned().unwrap();
    assert_eq!(
        (seen["deleted"].as_str(), seen["text"].as_str()),
        (Some("staff"), Some("something rude"))
    );
    let (code, _) =
        request(&s.host, "POST", "/admin/api/users/eva/role", &boss, r#"{"role":"moderator"}"#)
            .await;
    assert_eq!(code, 200);
    let mods = token(&s.host, "eva").await;
    let (code, _) = request(&s.host, "POST", &path, &mods, r#"{"silent":true}"#).await;
    assert_eq!(code, 403);
    let (code, _) = request(&s.host, "POST", &path, &boss, r#"{"silent":true}"#).await;
    assert_eq!(code, 200);
    dan.expect("removed", |m| matches!(m, ServerMsg::ChatRemoved { id: i } if *i == id)).await;

    // A tournament called off before it starts: the players are told.
    dan.send(ClientMsg::CreateTournament {
        name: "Bad name cup".into(),
        size: 4,
        rated: false,
        rules: Rules::Brazilian,
        password: None,
        entry_fee: 0,
        format: Default::default(),
    })
    .await;
    let tid = eventually("the tournament", || {
        let (host, boss) = (s.host.clone(), boss.clone());
        async move {
            let (_, v) = request(&host, "GET", "/admin/api/live", &boss, "").await;
            v["tournaments"].as_array()?.first()?["id"].as_u64()
        }
    })
    .await;
    let cancel = format!("/admin/api/tournaments/{tid}/cancel");
    let (code, _) = request(&s.host, "POST", &cancel, &mods, "").await;
    assert_eq!(code, 403, "administrators only");
    let (code, _) = request(&s.host, "POST", &cancel, &boss, "").await;
    assert_eq!(code, 200);
    dan.notice("called off").await;
    let (code, _) = request(&s.host, "POST", &cancel, &boss, "").await;
    assert_eq!(code, 400, "gone");
    let (_, v) =
        request(&s.host, "GET", "/admin/api/audit?action=tournament-cancel", &boss, "").await;
    assert_eq!(v["entries"][0]["target"], "Bad name cup");
}

#[tokio::test]
async fn the_portal_shows_the_economy_and_the_server() {
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let _fia = Client::register(&s.url, "fia").await;
    let (code, _) = request(
        &s.host,
        "POST",
        "/admin/api/users/fia/coins",
        &boss,
        r#"{"coins":700,"reason":"contest"}"#,
    )
    .await;
    assert_eq!(code, 200);

    let (code, v) = request(&s.host, "GET", "/admin/api/economy", &boss, "").await;
    assert_eq!(code, 200, "{v}");
    assert!(v["economy"]["coins_total"].as_i64().unwrap() >= 700);
    assert!(v["economy"]["flows"].as_array().unwrap().iter().any(|f| f["kind"] == "staff"));
    assert!(v["economy"]["richest"].as_array().unwrap().iter().any(|r| r[0] == "fia"));
    let catalog = v["catalog"].as_array().unwrap();
    assert!(catalog.iter().any(|i| i["code"] == "chalk_red" && i["how"] == "coins"));
    assert!(catalog.iter().all(|i| i["category"].is_string()));

    let (code, v) = request(&s.host, "GET", "/admin/api/system", &boss, "").await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["protocol"], PROTOCOL_VERSION);
    assert!(v["online"].as_u64().unwrap() >= 1);
    assert_eq!(v["online_by_rules"]["brazilian"].as_u64(), v["online"].as_u64());
    assert_eq!(v["settings"]["shot_clock_secs"], 30);
    // No Store game yet, and no Stripe (so not its test mode either).
    assert!(v["msstore_game"].is_null(), "{v}");
    assert_eq!(v["stripe_test"], false);
    // A Microsoft Store game built without sales connects: the portal says
    // what the Store's game does, whatever the server's settings.
    let (mut store, _) = connect_async(&s.url).await.unwrap();
    let hello = ClientMsg::Hello {
        version: PROTOCOL_VERSION,
        rules: Rules::Brazilian,
        store_sales: Some(false),
    };
    store.send(Message::Text(hello.to_json().into())).await.unwrap();
    let game = eventually("the Store game in the portal", || async {
        let (_, v) = request(&s.host, "GET", "/admin/api/system", &boss, "").await;
        (!v["msstore_game"].is_null()).then(|| v["msstore_game"].clone())
    })
    .await;
    assert_eq!(game["sales"], false);
    assert!(game["seen_at"].as_i64().unwrap() > 0);

    let (code, v) = request(&s.host, "GET", "/admin/api/metrics", &boss, "").await;
    assert_eq!(code, 200);
    assert!(v["countries"].as_array().unwrap().iter().any(|c| c[0] == "BR"), "{v}");

    // Everything here is for the staff only.
    for path in ["/admin/api/economy", "/admin/api/system", "/admin/api/chat", "/admin/api/audit"] {
        let (code, _) = request(&s.host, "GET", path, "", "").await;
        assert_eq!(code, 401, "{path}");
    }
    let _ = &s.server;
}

/// Polls `f` until it returns something.
async fn eventually<T, F, Fut>(what: &str, mut f: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(v) = f().await {
            return v;
        }
        assert!(tokio::time::Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Events: the staff schedule them; boosts multiply experience and coins
/// from games while they run, a themed tournament opens like the weekly cup
/// (with the house's prize), players are told, and calling one off undoes
/// it.
#[tokio::test]
async fn events_boost_and_organise() {
    use snook_legend_proto::events::EventKind;
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let t = snook_legend_server::now();
    let mut gus = Client::register(&s.url, "gus").await;
    let create = |body: String| {
        let (host, boss) = (s.host.clone(), boss.clone());
        async move { request(&host, "POST", "/admin/api/events", &boss, &body).await }
    };

    // What can't be scheduled.
    for body in [
        format!(r#"{{"kind":"raffle","title":"x","starts_at":{t},"ends_at":{}}}"#, t + 60),
        format!(
            r#"{{"kind":"xp","title":"x","multiplier":5,"starts_at":{t},"ends_at":{}}}"#,
            t + 60
        ),
        format!(
            r#"{{"kind":"xp","title":"  ","multiplier":2,"starts_at":{t},"ends_at":{}}}"#,
            t + 60
        ),
        format!(
            r#"{{"kind":"xp","title":"x","multiplier":2,"starts_at":{t},"ends_at":{}}}"#,
            t - 10
        ),
        format!(
            r#"{{"kind":"coins","title":"x","multiplier":2,"starts_at":{t},"ends_at":{}}}"#,
            t + 30 * 86_400
        ),
        format!(
            r#"{{"kind":"tournament","title":"x","format":"knockout","size":5,"starts_at":{},"ends_at":{}}}"#,
            t + 600,
            t + 3600
        ),
        format!(
            r#"{{"kind":"tournament","title":"x","format":"arena","starts_at":{},"ends_at":{}}}"#,
            t + 600,
            t + 660
        ),
        format!(
            r#"{{"kind":"tournament","title":"x","format":"arena","starts_at":{t},"ends_at":{}}}"#,
            t + 1800
        ),
    ] {
        let (code, v) = create(body.clone()).await;
        assert_eq!(code, 400, "{body}: {v}");
    }

    // Double experience, now: the player online is told, and it counts.
    let (code, v) = create(format!(
        r#"{{"kind":"xp","title":"Double XP Friday","multiplier":2,"starts_at":{t},"ends_at":{}}}"#,
        t + 3600
    ))
    .await;
    assert_eq!(code, 200, "{v}");
    let told = gus
        .expect("events", |m| matches!(m, ServerMsg::Events { events } if !events.is_empty()))
        .await;
    let ServerMsg::Events { events } = told else { unreachable!() };
    assert_eq!(
        (events[0].kind, events[0].multiplier, events[0].title.as_str()),
        (EventKind::Xp, 2, "Double XP Friday")
    );
    assert_eq!(s.server.boost(EventKind::Xp), 2);
    assert_eq!(s.server.boost(EventKind::Coins), 1);
    let user = s.server.store.user_by_name("gus").await.unwrap().unwrap();
    snook_legend_server::account::award_xp(&s.server, &user, 10).await;
    gus.expect("double xp", |m| matches!(m, ServerMsg::Xp { gained: 20, .. })).await;

    // Triple coins from games: the game's coins and the day's cap.
    let (code, _) = create(format!(
        r#"{{"kind":"coins","title":"Coins ×3","multiplier":3,"starts_at":{t},"ends_at":{}}}"#,
        t + 3600
    ))
    .await;
    assert_eq!(code, 200);
    snook_legend_server::account::game_coins(&s.server, &user, true).await;
    let per_win = (snook_legend_server::account::COINS_PER_GAME
        + snook_legend_server::account::COINS_PER_WIN)
        * 3;
    gus.expect(
        "triple coins",
        |m| matches!(m, ServerMsg::CoinsEarned { amount, .. } if *amount as i64 == per_win),
    )
    .await;

    // A themed arena in half an hour: open for sign-up at once, with the
    // house's prize (70% and 30%) on top of the fees.
    let (code, v) = create(format!(
        r#"{{"kind":"tournament","title":"Halloween Arena","format":"arena","rated":false,"entry_fee":0,"prize":1000,"starts_at":{},"ends_at":{}}}"#,
        t + 1800,
        t + 3600
    ))
    .await;
    assert_eq!(code, 200, "{v}");
    let cup_id = v["id"].as_i64().unwrap();
    let (_, live) = request(&s.host, "GET", "/admin/api/live", &boss, "").await;
    let cup = live["tournaments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["name"] == "Halloween Arena")
        .cloned()
        .expect("the arena is open");
    assert_eq!(cup["prizes"], serde_json::json!([700, 300]));
    assert_eq!(cup["format"], "arena");
    assert_eq!(cup["starts_at"], t + 1800);
    assert_eq!(cup["host"], snook_legend_proto::GAME_NAME, "organised by the server");

    // Someone signing in now is told of all three.
    let mut ivy = Client::register(&s.url, "ivy").await;
    let ServerMsg::Events { events } =
        ivy.expect("events at sign-in", |m| matches!(m, ServerMsg::Events { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(events.len(), 3);

    // The list in the portal, then calling the arena off: it is gone, and
    // a moderator could do neither.
    let (_, v) = request(&s.host, "GET", "/admin/api/events", &boss, "").await;
    let statuses: Vec<&str> =
        v["events"].as_array().unwrap().iter().map(|e| e["status"].as_str().unwrap()).collect();
    assert_eq!(statuses, ["scheduled", "live", "live"]);
    let (code, _) =
        request(&s.host, "POST", "/admin/api/users/ivy/role", &boss, r#"{"role":"moderator"}"#)
            .await;
    assert_eq!(code, 200);
    let mods = token(&s.host, "ivy").await;
    let cancel = format!("/admin/api/events/{cup_id}/cancel");
    assert_eq!(request(&s.host, "POST", &cancel, &mods, "").await.0, 403);
    assert_eq!(request(&s.host, "POST", "/admin/api/events", &mods, "{}").await.0, 422);
    assert_eq!(request(&s.host, "POST", &cancel, &boss, "").await.0, 200);
    assert_eq!(request(&s.host, "POST", &cancel, &boss, "").await.0, 400, "once");
    let (_, live) = request(&s.host, "GET", "/admin/api/live", &boss, "").await;
    assert!(
        !live["tournaments"].as_array().unwrap().iter().any(|x| x["name"] == "Halloween Arena")
    );
    let (_, v) = request(&s.host, "GET", "/admin/api/audit?action=event-cancel", &boss, "").await;
    assert_eq!(v["entries"][0]["target"], "Halloween Arena");
}

/// A player's record against the computer: the tour, the tournaments'
/// medals, the drills.
#[tokio::test]
async fn the_portal_shows_the_record_against_the_computer() {
    use snook_legend_proto::cup::{Format, Medal};
    let s = start().await;
    let boss = token(&s.host, "boss").await;
    let id = s.server.store.user_by_name("boss").await.unwrap().unwrap().id;
    let (_, v) = request(&s.host, "GET", "/admin/api/users/boss", &boss, "").await;
    let c = &v["computer"];
    assert!(c["tour_won_at"].is_null(), "{v}");
    assert_eq!((c["finals"].as_u64(), c["daily_wins"].as_u64()), (Some(0), Some(0)));
    assert_eq!(c["medals"]["knockout"], serde_json::json!([0, 0, 0]));
    assert_eq!(c["drill_stars_max"].as_u64(), Some(snook_legend_proto::drills::max_stars() as u64));

    let store = &s.server.store;
    let big = u64::MAX - 7;
    store
        .add_cup_medal(id, &Medal { cup: big, format: Format::Knockout, place: 1 }, 100)
        .await
        .unwrap();
    store.add_cup_medal(id, &Medal { cup: 2, format: Format::Arena, place: 3 }, 200).await.unwrap();
    store.issue_ladder_ticket(id, 77, 0, 300).await.unwrap();
    store.issue_ladder_ticket(id, 78, 1, 400).await.unwrap();
    store.record_ladder_claim(id, 78, 30, 500).await.unwrap();
    let code = snook_legend_proto::drills::DRILLS[0].code;
    store.drill_record(id, code, 9_999, 600).await.unwrap();
    let (_, v) = request(&s.host, "GET", "/admin/api/users/boss", &boss, "").await;
    let c = &v["computer"];
    assert_eq!(c["tour_won_at"].as_i64(), Some(500), "{v}");
    assert_eq!((c["finals"].as_u64(), c["last_final"].as_i64()), (Some(2), Some(400)));
    assert_eq!(c["medals"]["knockout"], serde_json::json!([1, 0, 0]));
    assert_eq!(c["medals"]["arena"], serde_json::json!([0, 0, 1]));
    // Newest first; a tournament's number as text, whole.
    assert_eq!(c["recent_medals"][0]["cup"], "2");
    assert_eq!(c["recent_medals"][1]["cup"], big.to_string());
    assert_eq!(c["recent_medals"][1]["format"], "knockout");
    assert_eq!(c["drills"][0]["code"], code);
    assert_eq!(c["drills"][0]["stars"].as_u64(), Some(3));
    assert_eq!(c["drill_stars"].as_u64(), Some(3));
}
