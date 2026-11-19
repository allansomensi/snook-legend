//! The age group and the chat of players under 18 (Lei 15.211/2025, the ECA
//! Digital, art. 20): off until a parent or guardian turns it on, both ways,
//! enforced by the server; older games are not asked and keep playing.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use snook_legend_proto::Role;
use snook_legend_proto::{
    AGE_PROTOCOL, ClientMsg, OLDEST_PROTOCOL_VERSION, ServerMsg, parse_server,
};
use snook_legend_server::lounge::{CHAT_OFF_FOR_MINORS, Rules};
use snook_legend_server::store::{NewUser, Store};
use snook_legend_server::{Config, Server, serve};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

const PASSWORD: &str = "secret123";

async fn start() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let rules = Rules { max_connections_per_ip: 100, ..Rules::default() };
    let config = Config { bind: addr, rules, ..Config::default() };
    let server = Server::new(config).await.unwrap();
    tokio::spawn(serve(listener, server));
    format!("ws://{addr}/ws")
}

struct Client {
    ws: Ws,
}

impl Client {
    async fn register(url: &str, name: &str, version: u32, minor: Option<bool>) -> Client {
        let (ws, _) = connect_async(url).await.unwrap();
        let mut c = Client { ws };
        c.send(ClientMsg::Hello { version }).await;
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
    msgs.iter().any(|m| matches!(m, ServerMsg::Error { message } if message == CHAT_OFF_FOR_MINORS))
}

#[tokio::test]
async fn players_under_18_chat_only_once_a_guardian_allows_it() {
    let url = start().await;
    let mut kid = Client::register(&url, "Kid", AGE_PROTOCOL, Some(true)).await;
    assert_eq!(kid.age().await, (Some(true), false));
    let mut adult = Client::register(&url, "Adult", AGE_PROTOCOL, Some(false)).await;
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
    kid.send(ClientMsg::Chat { text: "oi".into() }).await;
    assert!(chat_from(&adult.drain().await, "Kid", "oi"));
    adult.send(ClientMsg::Chat { text: "hello again".into() }).await;
    assert!(chat_from(&kid.drain().await, "Adult", "hello again"));
    // And off again.
    kid.send(ClientMsg::AllowChat { allowed: false }).await;
    assert_eq!(kid.age().await, (Some(true), false));
    kid.send(ClientMsg::Chat { text: "tchau".into() }).await;
    assert!(refused(&kid.drain().await));
}

#[tokio::test]
async fn older_games_are_not_asked_and_older_accounts_answer_once() {
    let url = start().await;
    // A game of the oldest version still let in: no age, nothing new sent.
    let mut old = Client::register(&url, "Old", OLDEST_PROTOCOL_VERSION, None).await;
    let seen = old.drain().await;
    assert!(!seen.iter().any(|m| matches!(m, ServerMsg::Age { .. })));
    old.send(ClientMsg::Chat { text: "still here".into() }).await;
    assert!(chat_from(&old.drain().await, "Old", "still here"));

    // An account without an age group, on a newer game: asked once.
    let mut later = Client::register(&url, "Later", AGE_PROTOCOL, None).await;
    assert_eq!(later.age().await, (None, false));
    later.send(ClientMsg::SetAge { minor: true }).await;
    assert_eq!(later.age().await, (Some(true), false));
    later.send(ClientMsg::SetAge { minor: false }).await;
    assert_eq!(later.age().await, (Some(true), false));
    later.drain().await;
    later.send(ClientMsg::Chat { text: "hey".into() }).await;
    assert!(refused(&later.drain().await));
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
    assert!(!old.chat_restricted());
    // Once given, the age group stays.
    assert!(store.set_minor(old.id, false).await.unwrap());
    assert!(!store.set_minor(old.id, true).await.unwrap());
    assert!(!store.set_minor(kid.id, false).await.unwrap());
    let old = store.user_by_id(old.id).await.unwrap().unwrap();
    assert_eq!(old.minor, Some(false));
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
