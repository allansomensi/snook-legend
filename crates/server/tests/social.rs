//! Status messages, recent opponents and the "looking for a game" board:
//! the stores (memory always; PostgreSQL when `SNOOKERED_TEST_DATABASE_URL`
//! points at a database that may be wiped) and the server end to end.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use snookered_core::GameOverReason;
use snookered_proto::{
    ClientMsg, MAX_STATUS_LEN, PROTOCOL_VERSION, Role, SOCIAL_PROTOCOL, ServerMsg, parse_server,
};
use snookered_server::lounge::Rules;
use snookered_server::store::{MatchRecord, NewUser, Store};
use snookered_server::{Config, Server, serve};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

// --- The stores. ---

fn new_user(name: &str) -> NewUser<'_> {
    NewUser {
        username: name,
        password_hash: "h",
        country: "BR",
        rating: 1200,
        role: Role::Player,
        ip: "1.2.3.4",
    }
}

fn game(players: [i64; 2], winner: usize, rated: bool, ended_at: i64) -> MatchRecord {
    MatchRecord {
        players,
        winner,
        reason: GameOverReason::Resignation,
        rated,
        delta: [0, 0],
        ratings_after: [1200, 1200],
        shots: 10,
        tournament: None,
        eight_win: false,
        runout: false,
        shutout: false,
        ended_at,
        replay: None,
        stats: Default::default(),
        abandoned: None,
    }
}

async fn exercise(store: Store) {
    let t = 1_700_000_000;
    let a = store.create_user(&new_user("Ana")).await.unwrap().unwrap();
    let b = store.create_user(&new_user("Bia")).await.unwrap().unwrap();
    let c = store.create_user(&new_user("Caio")).await.unwrap().unwrap();
    let d = store.create_user(&new_user("Davi")).await.unwrap().unwrap();
    assert_eq!(a.status_message, "");

    // Status messages are stored, read back and cleared.
    store.set_status_message(a.id, "procurando parceiro para treino").await.unwrap();
    let got = store.user_by_name("ana").await.unwrap().unwrap();
    assert_eq!(got.status_message, "procurando parceiro para treino");
    assert_eq!(store.user_by_id(a.id).await.unwrap().unwrap().status_message, got.status_message);
    store.set_status_message(b.id, "só casual").await.unwrap();
    store.friend_request(a.id, b.id, t).await.unwrap();
    store.friend_accept(b.id, a.id).await.unwrap();
    let friends = store.friends(a.id).await.unwrap();
    assert_eq!(friends[0].0.status_message, "só casual", "friends come with their status");
    store.set_status_message(b.id, "").await.unwrap();
    assert_eq!(store.user_by_id(b.id).await.unwrap().unwrap().status_message, "");

    // No games yet: nobody recent.
    assert!(store.recent_opponents(a.id, 20).await.unwrap().is_empty());

    // Ana played Bia twice (the latest won from seat 1), Caio once (lost)
    // and Davi once, casual, in between (games are ordered by when they
    // ended, not by when they were stored).
    store.record_match(&game([a.id, b.id], 1, true, t)).await.unwrap();
    store.record_match(&game([a.id, c.id], 1, true, t + 10)).await.unwrap();
    store.record_match(&game([b.id, a.id], 1, true, t + 20)).await.unwrap();
    store.record_match(&game([d.id, a.id], 0, false, t + 5)).await.unwrap();
    // Games without Ana don't count.
    store.record_match(&game([b.id, c.id], 0, true, t + 30)).await.unwrap();

    let recent = store.recent_opponents(a.id, 20).await.unwrap();
    let names: Vec<&str> = recent.iter().map(|r| r.user.username.as_str()).collect();
    assert_eq!(names, ["Bia", "Caio", "Davi"]);
    assert_eq!(
        (recent[0].at, recent[0].won, recent[0].rated, recent[0].games),
        (t + 20, true, true, 2)
    );
    assert_eq!((recent[1].at, recent[1].won, recent[1].games), (t + 10, false, 1));
    assert_eq!((recent[2].at, recent[2].won, recent[2].rated), (t + 5, false, false));
    let limited = store.recent_opponents(a.id, 2).await.unwrap();
    assert_eq!(limited.len(), 2);
    assert_eq!(limited[1].user.username, "Caio");
    // From the other side.
    let of_bia = store.recent_opponents(b.id, 20).await.unwrap();
    let names: Vec<&str> = of_bia.iter().map(|r| r.user.username.as_str()).collect();
    assert_eq!(names, ["Caio", "Ana"]);
    assert!(!of_bia[1].won && of_bia[0].won);

    // A deleted account is left out, and its status message goes with it.
    store.set_status_message(d.id, "até mais").await.unwrap();
    store.delete_account(d.id, t + 100).await.unwrap();
    let names: Vec<String> = store
        .recent_opponents(a.id, 20)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.user.username)
        .collect();
    assert_eq!(names, ["Bia", "Caio"]);
    assert_eq!(store.user_by_id(d.id).await.unwrap().unwrap().status_message, "");
}

#[tokio::test]
async fn memory_store() {
    exercise(Store::memory()).await;
}

#[tokio::test]
async fn postgres_store() {
    let Ok(url) = std::env::var("SNOOKERED_TEST_DATABASE_URL") else {
        eprintln!("SNOOKERED_TEST_DATABASE_URL not set: skipping");
        return;
    };
    {
        use sqlx::Executor;
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        pool.execute("DROP SCHEMA public CASCADE; CREATE SCHEMA public;").await.unwrap();
    }
    exercise(Store::postgres(&url, 3).await.unwrap()).await;
}

// --- The server, end to end. ---

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

const PASSWORD: &str = "secret123";

async fn start_with(rules: Rules, admins: Vec<String>) -> (String, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let rules = Rules { max_connections_per_ip: 100, ..rules };
    let config = Config { bind: addr, rules, admins, ..Config::default() };
    let server = Server::new(config).await.unwrap();
    tokio::spawn(serve(listener, server));
    (format!("ws://{addr}/ws"), format!("{addr}"))
}

async fn start(rules: Rules) -> String {
    start_with(rules, Vec::new()).await.0
}

struct Client {
    ws: Ws,
}

impl Client {
    async fn connect_as(url: &str, version: u32) -> Client {
        let (ws, _) = connect_async(url).await.unwrap();
        let mut c = Client { ws };
        c.send(ClientMsg::Hello { version }).await;
        c
    }

    async fn send(&mut self, msg: ClientMsg) {
        self.ws.send(Message::Text(msg.to_json().into())).await.unwrap();
    }

    /// Reads messages until one matches `pred`.
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

    /// Every message until the answer to a ping (what the server sent so far).
    async fn drain(&mut self) -> Vec<ServerMsg> {
        self.send(ClientMsg::Ping).await;
        let mut seen = Vec::new();
        loop {
            let msg = tokio::time::timeout(Duration::from_secs(20), self.ws.next())
                .await
                .expect("timed out waiting for pong")
                .expect("connection closed")
                .unwrap();
            if let Message::Text(t) = msg {
                match parse_server(t.as_str()).unwrap() {
                    ServerMsg::Pong => return seen,
                    m => seen.push(m),
                }
            }
        }
    }

    async fn register_as(url: &str, name: &str, version: u32) -> (Client, String) {
        let mut c = Client::connect_as(url, version).await;
        c.send(ClientMsg::Register {
            username: name.into(),
            password: PASSWORD.into(),
            country: "BR".into(),
        })
        .await;
        let ServerMsg::AuthOk { token, .. } =
            c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await
        else {
            unreachable!()
        };
        (c, token)
    }

    async fn register(url: &str, name: &str) -> Client {
        Client::register_as(url, name, PROTOCOL_VERSION).await.0
    }
}

fn is_error(m: &ServerMsg, text: &str) -> bool {
    matches!(m, ServerMsg::Error { message } if message == text)
}

fn seek_message(m: &ServerMsg) -> bool {
    matches!(
        m,
        ServerMsg::SeekBoard { .. } | ServerMsg::SeekPosted { .. } | ServerMsg::SeekRemoved { .. }
    )
}

/// `a` invites `b`, `b` accepts; both get the match.
async fn start_match(a: &mut Client, b: &mut Client, a_name: &str, b_name: &str) {
    a.send(ClientMsg::Invite { to: b_name.into(), rated: true }).await;
    b.expect("invite", |m| matches!(m, ServerMsg::Invited { .. })).await;
    b.send(ClientMsg::AnswerInvite { from: a_name.into(), accept: true }).await;
    a.expect("match a", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    b.expect("match b", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
}

/// `loser` resigns; both go back to the lounge.
async fn resign(loser: &mut Client, winner: &mut Client) {
    loser.send(ClientMsg::Resign).await;
    for c in [&mut *loser, &mut *winner] {
        c.expect("end", |m| matches!(m, ServerMsg::MatchEnded { .. })).await;
        c.send(ClientMsg::LeaveMatch).await;
    }
}

#[tokio::test]
async fn status_messages_are_cleaned_shared_kept_and_moderated() {
    let (url, _) = start_with(Rules::default(), vec!["boss".into()]).await;
    let (mut ana, token) = Client::register_as(&url, "ana", PROTOCOL_VERSION).await;
    let mut bia = Client::register(&url, "bia").await;
    let mut boss = Client::register(&url, "boss").await;

    let long = format!("  procurando\u{202E}   parceiro para treino {}", "x".repeat(100));
    ana.send(ClientMsg::SetStatusMessage { text: long }).await;
    let ServerMsg::PlayerUpdated { player } = bia
        .expect("ana's new status", |m| {
            matches!(m, ServerMsg::PlayerUpdated { player } if player.username == "ana"
                && !player.status_message.is_empty())
        })
        .await
    else {
        unreachable!()
    };
    assert!(player.status_message.starts_with("procurando parceiro para treino x"));
    assert_eq!(player.status_message.chars().count(), MAX_STATUS_LEN);
    let status = player.status_message.clone();

    // On the profile, and kept for the next session.
    bia.send(ClientMsg::Profile { username: "ana".into() }).await;
    let ServerMsg::Profile { profile } =
        bia.expect("profile", |m| matches!(m, ServerMsg::Profile { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(profile.player.status_message, status);
    drop(ana);
    bia.expect("ana left", |m| matches!(m, ServerMsg::PlayerLeft { .. })).await;
    let mut ana = Client::connect_as(&url, PROTOCOL_VERSION).await;
    ana.send(ClientMsg::Resume { token }).await;
    let ServerMsg::AuthOk { player, .. } =
        ana.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(player.status_message, status);

    // Players can't clear someone else's; the staff can.
    bia.send(ClientMsg::ClearStatusMessage { username: "ana".into() }).await;
    bia.expect("refused", |m| is_error(m, "You can't do that.")).await;
    boss.send(ClientMsg::ClearStatusMessage { username: "ana".into() }).await;
    ana.expect("told", |m| {
        matches!(m, ServerMsg::Notice { text } if text == "A moderator cleared your status message.")
    })
    .await;
    bia.expect("cleared for everyone", |m| {
        matches!(m, ServerMsg::PlayerUpdated { player } if player.username == "ana"
            && player.status_message.is_empty())
    })
    .await;
    boss.expect(
        "done",
        |m| matches!(m, ServerMsg::Notice { text } if text == "Status message cleared."),
    )
    .await;

    // A few changes a minute, no more (four at once on a new connection).
    for k in 0..5 {
        ana.send(ClientMsg::SetStatusMessage { text: format!("status {k}") }).await;
    }
    ana.expect("rate limited", |m| is_error(m, "Slow down a little.")).await;
}

#[tokio::test]
async fn the_board_reaches_new_games_only_and_follows_the_players() {
    let url = start(Rules::default()).await;
    let mut ana = Client::register(&url, "ana").await;
    let mut bia = Client::register(&url, "bia").await;
    let (mut old, _) = Client::register_as(&url, "oldgame", SOCIAL_PROTOCOL - 1).await;

    // A note, cleaned, for 30 minutes.
    ana.send(ClientMsg::PostSeek { text: "  treino\u{200B}   rápido ".into(), rated: false }).await;
    let ServerMsg::SeekPosted { note } =
        bia.expect("note", |m| matches!(m, ServerMsg::SeekPosted { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(
        (note.username.as_str(), note.text.as_str(), note.rated),
        ("ana", "treino rápido", false)
    );
    assert_eq!(note.expires_at - note.at, 30 * 60);
    // One per player: a new one replaces it.
    ana.send(ClientMsg::PostSeek { text: "valendo rating".into(), rated: true }).await;
    bia.expect("replaced", |m| matches!(m, ServerMsg::SeekPosted { note } if note.rated)).await;

    // Whoever comes in gets the board.
    let mut caio = Client::register(&url, "caio").await;
    let ServerMsg::SeekBoard { notes } =
        caio.expect("board", |m| matches!(m, ServerMsg::SeekBoard { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].text, "valendo rating");

    // Players can't take others' notes down.
    bia.send(ClientMsg::RemoveSeek { username: "ana".into() }).await;
    bia.expect("refused", |m| is_error(m, "You can't do that.")).await;

    // A challenge is a normal invitation; once the game starts the note goes.
    bia.send(ClientMsg::PostSeek { text: String::new(), rated: false }).await;
    caio.expect(
        "bia's note",
        |m| matches!(m, ServerMsg::SeekPosted { note } if note.username == "bia"),
    )
    .await;
    start_match(&mut caio, &mut ana, "caio", "ana").await;
    bia.expect(
        "ana found a game",
        |m| matches!(m, ServerMsg::SeekRemoved { username } if username == "ana"),
    )
    .await;
    // Posting from a game is refused.
    ana.send(ClientMsg::PostSeek { text: "again".into(), rated: true }).await;
    ana.expect("refused", |m| is_error(m, "Finish your game first.")).await;

    // Taken down by its owner, or when they leave.
    bia.send(ClientMsg::RemoveSeek { username: String::new() }).await;
    caio.expect(
        "bia took hers down",
        |m| matches!(m, ServerMsg::SeekRemoved { username } if username == "bia"),
    )
    .await;
    bia.send(ClientMsg::PostSeek { text: "de novo".into(), rated: false }).await;
    caio.expect("bia's new note", |m| matches!(m, ServerMsg::SeekPosted { .. })).await;
    drop(bia);
    caio.expect(
        "bia left",
        |m| matches!(m, ServerMsg::SeekRemoved { username } if username == "bia"),
    )
    .await;

    // Older games never see any of it.
    let seen = old.drain().await;
    assert!(!seen.iter().any(seek_message), "{seen:?}");
    assert!(seen.iter().any(|m| matches!(m, ServerMsg::PlayerUpdated { .. })));
}

#[tokio::test]
async fn notes_are_rate_limited_and_expire() {
    let rules = Rules { seek_lifetime: Duration::from_secs(1), ..Rules::default() };
    let url = start(rules).await;
    let mut ana = Client::register(&url, "ana").await;
    let mut bia = Client::register(&url, "bia").await;
    ana.send(ClientMsg::PostSeek { text: "rapidinho".into(), rated: true }).await;
    bia.expect("note", |m| matches!(m, ServerMsg::SeekPosted { .. })).await;
    bia.expect(
        "expired",
        |m| matches!(m, ServerMsg::SeekRemoved { username } if username == "ana"),
    )
    .await;
    // Three at once, then one every 20 seconds.
    for _ in 0..3 {
        ana.send(ClientMsg::PostSeek { text: "outra".into(), rated: true }).await;
    }
    ana.expect("rate limited", |m| is_error(m, "Slow down a little.")).await;
}

#[tokio::test]
async fn recent_opponents_latest_first_with_their_status() {
    let url = start(Rules::default()).await;
    let mut ana = Client::register(&url, "ana").await;
    let mut bia = Client::register(&url, "bia").await;
    let mut caio = Client::register(&url, "caio").await;

    ana.send(ClientMsg::RecentOpponents).await;
    let ServerMsg::RecentOpponents { players } =
        ana.expect("none yet", |m| matches!(m, ServerMsg::RecentOpponents { .. })).await
    else {
        unreachable!()
    };
    assert!(players.is_empty());

    start_match(&mut ana, &mut bia, "ana", "bia").await;
    resign(&mut bia, &mut ana).await;
    start_match(&mut caio, &mut ana, "caio", "ana").await;
    resign(&mut ana, &mut caio).await;
    caio.send(ClientMsg::SetStatusMessage { text: "bora jogar".into() }).await;

    // The results are stored in the background: ask until both are there.
    let mut players = Vec::new();
    for _ in 0..50 {
        ana.send(ClientMsg::RecentOpponents).await;
        let ServerMsg::RecentOpponents { players: p } =
            ana.expect("recent", |m| matches!(m, ServerMsg::RecentOpponents { .. })).await
        else {
            unreachable!()
        };
        players = p;
        if players.len() == 2 && !players[0].player.status_message.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let names: Vec<&str> = players.iter().map(|r| r.player.username.as_str()).collect();
    assert_eq!(names, ["caio", "bia"]);
    assert!(!players[0].won && players[1].won);
    assert!(players.iter().all(|r| r.online && r.rated && r.games == 1));
    assert_eq!(players[0].player.status_message, "bora jogar");

    // Offline ones stay, as they are; blocked ones go.
    drop(caio);
    ana.expect("caio left", |m| matches!(m, ServerMsg::PlayerLeft { .. })).await;
    ana.send(ClientMsg::Block { username: "bia".into() }).await;
    ana.send(ClientMsg::RecentOpponents).await;
    let ServerMsg::RecentOpponents { players } =
        ana.expect("recent", |m| matches!(m, ServerMsg::RecentOpponents { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(players.len(), 1);
    assert_eq!(players[0].player.username, "caio");
    assert!(!players[0].online);
    assert_eq!(players[0].player.status_message, "bora jogar");
}

#[tokio::test]
async fn lan_guests_have_none_of_it() {
    let url = start(Rules { lan: true, ..Rules::default() }).await;
    let mut a = Client::connect_as(&url, PROTOCOL_VERSION).await;
    a.send(ClientMsg::Guest { name: "host".into() }).await;
    a.expect("guest", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
    a.send(ClientMsg::SetStatusMessage { text: "oi".into() }).await;
    a.expect("refused", |m| is_error(m, "Guests don't have this.")).await;
    a.send(ClientMsg::RecentOpponents).await;
    a.expect("refused", |m| is_error(m, "Guests don't have this.")).await;
    a.send(ClientMsg::PostSeek { text: "oi".into(), rated: false }).await;
    let seen = a.drain().await;
    assert!(!seen.iter().any(seek_message), "{seen:?}");
}

/// A minimal HTTP client over a raw socket (no extra dependencies).
async fn request(host: &str, method: &str, path: &str, token: &str, body: &str) -> (u16, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = TcpStream::connect(host).await.unwrap();
    let auth =
        if token.is_empty() { String::new() } else { format!("Authorization: Bearer {token}\r\n") };
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).await.unwrap();
    let code = out[9..12].parse().unwrap();
    let body = out.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (code, body)
}

fn json_of(body: &str) -> serde_json::Value {
    // Possibly chunked: the JSON object between the first '{' and the last '}'.
    let start = body.find('{').unwrap();
    let end = body.rfind('}').unwrap();
    serde_json::from_str(&body[start..=end]).unwrap()
}

#[tokio::test]
async fn the_staff_panel_clears_statuses_and_notes_and_mutes_posting() {
    let (url, host) = start_with(Rules::default(), vec!["boss".into()]).await;
    let _boss = Client::register(&url, "boss").await;
    let mut eve = Client::register(&url, "eve").await;
    let mut bia = Client::register(&url, "bia").await;
    let (code, body) = request(
        &host,
        "POST",
        "/admin/api/login",
        "",
        &format!(r#"{{"username":"boss","password":"{PASSWORD}"}}"#),
    )
    .await;
    assert_eq!(code, 200, "{body}");
    let token = json_of(&body)["token"].as_str().unwrap().to_string();

    eve.send(ClientMsg::SetStatusMessage { text: "spam spam".into() }).await;
    eve.send(ClientMsg::PostSeek { text: "spam".into(), rated: false }).await;
    bia.expect("note", |m| matches!(m, ServerMsg::SeekPosted { .. })).await;

    let (code, body) = request(&host, "GET", "/admin/api/users/eve", &token, "").await;
    assert_eq!(code, 200);
    assert_eq!(json_of(&body)["user"]["status_message"], "spam spam");
    let (code, _) = request(&host, "POST", "/admin/api/users/eve/clear-status", &token, "{}").await;
    assert_eq!(code, 200);
    bia.expect("cleared", |m| {
        matches!(m, ServerMsg::PlayerUpdated { player } if player.username == "eve"
            && player.status_message.is_empty())
    })
    .await;
    let (code, _) = request(&host, "POST", "/admin/api/users/eve/clear-status", &token, "{}").await;
    assert_eq!(code, 400, "nothing left to clear");

    let (code, body) = request(&host, "GET", "/admin/api/seeks", &token, "").await;
    assert_eq!(code, 200);
    assert_eq!(json_of(&body)["notes"][0]["username"], "eve");
    let (code, _) = request(&host, "DELETE", "/admin/api/seeks/eve", &token, "").await;
    assert_eq!(code, 200);
    bia.expect(
        "removed",
        |m| matches!(m, ServerMsg::SeekRemoved { username } if username == "eve"),
    )
    .await;
    eve.expect(
        "told",
        |m| matches!(m, ServerMsg::Notice { text } if text == "A moderator took down your note."),
    )
    .await;
    let (code, _) = request(&host, "DELETE", "/admin/api/seeks/eve", &token, "").await;
    assert_eq!(code, 404);

    // Muted: no notes and no status messages.
    let (code, _) =
        request(&host, "POST", "/admin/api/users/eve/mute", &token, r#"{"minutes":10}"#).await;
    assert_eq!(code, 200);
    eve.send(ClientMsg::PostSeek { text: "hi".into(), rated: false }).await;
    eve.expect(
        "muted",
        |m| matches!(m, ServerMsg::Error { message } if message.starts_with("You are muted")),
    )
    .await;
    eve.send(ClientMsg::SetStatusMessage { text: "hi".into() }).await;
    eve.expect(
        "muted",
        |m| matches!(m, ServerMsg::Error { message } if message.starts_with("You are muted")),
    )
    .await;
}
