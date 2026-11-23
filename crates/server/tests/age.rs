//! The chat with other players (Lei 15.211/2025, the ECA Digital, art. 20):
//! off for everyone, both ways, until the player says they are 18 or older,
//! or, under 18, until a parent or guardian turns it on; enforced by the
//! server. A LAN server's guests chat as before.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use snook_legend_proto::{ClientMsg, PROTOCOL_VERSION, Role, Rules, ServerMsg, parse_server};
use snook_legend_server::lounge::{CHAT_OFF, CHAT_OFF_FOR_MINORS, Settings};
use snook_legend_server::store::{NewUser, Store};
use snook_legend_server::{Config, Server, serve};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

const PASSWORD: &str = "secret123";

async fn start() -> String {
    start_with(Settings { max_connections_per_ip: 100, ..Settings::default() }).await
}

async fn start_with(settings: Settings) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let config = Config { bind: addr, settings, ..Config::default() };
    let server = Server::new(config).await.unwrap();
    tokio::spawn(serve(listener, server));
    format!("ws://{addr}/ws")
}

struct Client {
    ws: Ws,
}

impl Client {
    async fn register(url: &str, name: &str, minor: Option<bool>) -> Client {
        let (ws, _) = connect_async(url).await.unwrap();
        let mut c = Client { ws };
        c.send(ClientMsg::Hello {
            version: PROTOCOL_VERSION,
            rules: Rules::Brazilian,
            store_sales: None,
        })
        .await;
        c.send(ClientMsg::Register {
            username: name.into(),
            password: PASSWORD.into(),
            country: "BR".into(),
            minor,
        })
        .await;
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

    /// Every message until the answer to a ping.
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

    async fn age(&mut self) -> (Option<bool>, bool) {
        match self.expect("age", |m| matches!(m, ServerMsg::Age { .. })).await {
            ServerMsg::Age { minor, chat_allowed } => (minor, chat_allowed),
            _ => unreachable!(),
        }
    }
}

fn chat_from(msgs: &[ServerMsg], who: &str, text: &str) -> bool {
    msgs.iter()
        .any(|m| matches!(m, ServerMsg::Chat { line } if line.from == who && line.text == text))
}

fn refused(msgs: &[ServerMsg]) -> bool {
    refused_with(msgs, CHAT_OFF_FOR_MINORS)
}

fn refused_with(msgs: &[ServerMsg], why: &str) -> bool {
    msgs.iter().any(|m| matches!(m, ServerMsg::Error { message } if message == why))
}

/// The age group and chat among `msgs` (the last told).
fn age_in(msgs: &[ServerMsg]) -> Option<(Option<bool>, bool)> {
    msgs.iter().rev().find_map(|m| match m {
        ServerMsg::Age { minor, chat_allowed } => Some((*minor, *chat_allowed)),
        _ => None,
    })
}

/// The players' lines in the lounge's chat history among `msgs` (the last
/// history sent).
fn history(msgs: &[ServerMsg]) -> Option<Vec<String>> {
    msgs.iter().rev().find_map(|m| match m {
        ServerMsg::ChatHistory { lines } => Some(
            lines
                .iter()
                .filter(|l| l.kind == snook_legend_proto::ChatKind::Player)
                .map(|l| l.text.clone())
                .collect(),
        ),
        _ => None,
    })
}

#[tokio::test]
async fn players_under_18_chat_only_once_a_guardian_allows_it() {
    let url = start().await;
    let mut kid = Client::register(&url, "Kid", Some(true)).await;
    assert_eq!(kid.age().await, (Some(true), false));
    let mut adult = Client::register(&url, "Adult", Some(false)).await;
    assert_eq!(adult.age().await, (Some(false), false));
    kid.drain().await;
    adult.drain().await;

    // Off both ways: the kid's line is refused, the adult's never arrives.
    kid.send(ClientMsg::Chat { text: "oi".into() }).await;
    assert!(refused(&kid.drain().await));
    adult.send(ClientMsg::Chat { text: "hello".into() }).await;
    assert!(chat_from(&adult.drain().await, "Adult", "hello"));
    assert!(!chat_from(&kid.drain().await, "Adult", "hello"));
    // No status message, and the age group can't be changed from the game.
    kid.send(ClientMsg::SetStatusMessage { text: "hi".into() }).await;
    assert!(refused(&kid.drain().await));
    kid.send(ClientMsg::SetAge { minor: false }).await;
    assert_eq!(kid.age().await, (Some(true), false));
    // An adult has no chat to allow.
    adult.send(ClientMsg::AllowChat { allowed: false }).await;
    assert_eq!(adult.age().await, (Some(false), false));

    // A guardian turns the chat on: both ways.
    kid.send(ClientMsg::AllowChat { allowed: true }).await;
    assert_eq!(kid.age().await, (Some(true), true));
    // (Each line is awaited where it is sent: the other connection's ping
    // may otherwise be answered before the line is even read.)
    kid.send(ClientMsg::Chat { text: "oi".into() }).await;
    kid.expect("own line", |m| matches!(m, ServerMsg::Chat { line } if line.text == "oi")).await;
    adult
        .expect(
            "the kid's line",
            |m| matches!(m, ServerMsg::Chat { line } if line.from == "Kid" && line.text == "oi"),
        )
        .await;
    adult.send(ClientMsg::Chat { text: "hello again".into() }).await;
    kid.expect("the adult's line", |m| {
        matches!(m, ServerMsg::Chat { line } if line.from == "Adult" && line.text == "hello again")
    })
    .await;
    // And off again.
    kid.send(ClientMsg::AllowChat { allowed: false }).await;
    assert_eq!(kid.age().await, (Some(true), false));
    kid.send(ClientMsg::Chat { text: "tchau".into() }).await;
    assert!(refused(&kid.drain().await));
}

#[tokio::test]
async fn the_chat_is_off_until_the_player_turns_it_on() {
    let url = start().await;
    let mut adult = Client::register(&url, "Adult", Some(false)).await;
    assert_eq!(adult.age().await, (Some(false), false));
    adult.drain().await;
    adult.send(ClientMsg::Chat { text: "first".into() }).await;
    adult
        .expect("own line", |m| matches!(m, ServerMsg::Chat { line } if line.text == "first"))
        .await;

    // An account without an age group (the game asks when the player
    // first writes): off both ways.
    let mut new = Client::register(&url, "New", None).await;
    let seen = new.drain().await;
    assert_eq!(age_in(&seen), Some((None, false)));
    assert_eq!(history(&seen), Some(vec![]), "no players' lines in the history");
    new.send(ClientMsg::Chat { text: "hey".into() }).await;
    assert!(refused_with(&new.drain().await, CHAT_OFF));
    new.send(ClientMsg::SetStatusMessage { text: "hi".into() }).await;
    assert!(refused_with(&new.drain().await, CHAT_OFF));
    adult.send(ClientMsg::Chat { text: "second".into() }).await;
    adult
        .expect("own line", |m| matches!(m, ServerMsg::Chat { line } if line.text == "second"))
        .await;
    assert!(!chat_from(&new.drain().await, "Adult", "second"));

    // 18 or older: the chat is on, and the lounge's chat comes with what
    // players wrote.
    new.send(ClientMsg::SetAge { minor: false }).await;
    let seen = new.drain().await;
    assert_eq!(age_in(&seen), Some((Some(false), false)));
    assert_eq!(history(&seen), Some(vec!["first".to_string(), "second".to_string()]));
    new.send(ClientMsg::Chat { text: "hey".into() }).await;
    new.expect("own line", |m| matches!(m, ServerMsg::Chat { line } if line.text == "hey")).await;
    // Given once, it stays.
    new.send(ClientMsg::SetAge { minor: true }).await;
    assert_eq!(new.age().await, (Some(false), false));

    // Under 18, with a parent or guardian there: both answers in a row.
    let mut kid = Client::register(&url, "Kid", None).await;
    assert_eq!(kid.age().await, (None, false));
    kid.send(ClientMsg::SetAge { minor: true }).await;
    kid.send(ClientMsg::AllowChat { allowed: true }).await;
    assert_eq!(kid.age().await, (Some(true), false));
    assert_eq!(kid.age().await, (Some(true), true));
    kid.send(ClientMsg::Chat { text: "oi".into() }).await;
    kid.expect("own line", |m| matches!(m, ServerMsg::Chat { line } if line.text == "oi")).await;
    // And no way to say 18 or older afterwards.
    kid.send(ClientMsg::AllowChat { allowed: false }).await;
    assert_eq!(kid.age().await, (Some(true), false));
    kid.send(ClientMsg::SetAge { minor: false }).await;
    assert_eq!(kid.age().await, (Some(true), false));
    kid.drain().await;
    kid.send(ClientMsg::Chat { text: "tchau".into() }).await;
    assert!(refused(&kid.drain().await));
}

#[tokio::test]
async fn guests_of_a_lan_server_chat() {
    let url =
        start_with(Settings { lan: true, max_connections_per_ip: 100, ..Settings::default() })
            .await;
    let guest = |name: &'static str| {
        let url = url.clone();
        async move {
            let (ws, _) = connect_async(&url).await.unwrap();
            let mut c = Client { ws };
            c.send(ClientMsg::Hello {
                version: PROTOCOL_VERSION,
                rules: Rules::Brazilian,
                store_sales: None,
            })
            .await;
            c.send(ClientMsg::Guest { name: name.into() }).await;
            c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
            c
        }
    };
    let mut host = guest("host").await;
    let mut visitor = guest("visitor").await;
    visitor.drain().await;
    host.send(ClientMsg::Chat { text: "bem-vindo".into() }).await;
    visitor
        .expect(
            "the host's line",
            |m| matches!(m, ServerMsg::Chat { line } if line.from == "host" && line.text == "bem-vindo"),
        )
        .await;
}

// --- The stores. ---

async fn keeps_the_age(store: Store) {
    let new = |name: &'static str, minor| NewUser {
        username: name,
        password_hash: "h",
        country: "BR",
        rating: 1200,
        role: Role::Player,
        ip: "1.2.3.4",
        minor,
    };
    let kid = store.create_user(&new("Kid", Some(true))).await.unwrap().unwrap();
    assert_eq!((kid.minor, kid.chat_allowed_at), (Some(true), None));
    assert!(kid.chat_restricted());
    let old = store.create_user(&new("Old", None)).await.unwrap().unwrap();
    assert_eq!(old.minor, None);
    assert!(old.chat_restricted(), "off until the age group is given");
    let adult = store.create_user(&new("Adult", Some(false))).await.unwrap().unwrap();
    assert!(!adult.chat_restricted());
    // Once given, the age group stays.
    assert!(store.set_minor(old.id, false).await.unwrap());
    assert!(!store.set_minor(old.id, true).await.unwrap());
    assert!(!store.set_minor(kid.id, false).await.unwrap());
    let old = store.user_by_id(old.id).await.unwrap().unwrap();
    assert_eq!(old.minor, Some(false));
    assert!(!old.chat_restricted());
    // The guardian's permission, on and off.
    store.set_chat_allowed(kid.id, Some(1_700_000_000)).await.unwrap();
    let kid = store.user_by_id(kid.id).await.unwrap().unwrap();
    assert_eq!(kid.chat_allowed_at, Some(1_700_000_000));
    assert!(!kid.chat_restricted());
    store.set_chat_allowed(kid.id, None).await.unwrap();
    assert!(store.user_by_id(kid.id).await.unwrap().unwrap().chat_restricted());
    // Deleting the account forgets both.
    store.set_chat_allowed(kid.id, Some(1_700_000_000)).await.unwrap();
    store.delete_account(kid.id, 1_700_000_100).await.unwrap();
    let gone = store.user_by_id(kid.id).await.unwrap().unwrap();
    assert_eq!((gone.minor, gone.chat_allowed_at), (None, None));
}

#[tokio::test]
async fn memory_store() {
    keeps_the_age(Store::memory()).await;
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
    keeps_the_age(Store::postgres(&url, 3).await.unwrap()).await;
}
