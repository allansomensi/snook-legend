//! 0.12's match and social features end to end: settings kept with the
//! account, direct messages between friends, what friends are doing, the
//! spectators' chat, the chat kept to the quick phrases, the rating at
//! stake and the advanced statistics.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use snook_legend_proto::activity::Activity;
use snook_legend_proto::analysis::{ShotAnalysis, Tally};
use snook_legend_proto::{ClientMsg, PROTOCOL_VERSION, Rules, ServerMsg, parse_server};
use snook_legend_server::lounge::Settings;
use snook_legend_server::store::Store;
use snook_legend_server::{Config, Server, serve};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

const PASSWORD: &str = "secret123";

async fn start() -> (String, Arc<Server>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let settings = Settings { max_connections_per_ip: 100, ..Settings::default() };
    let config = Config { bind: addr, settings, ..Config::default() };
    let server = Server::with_store(config, Store::memory()).await;
    tokio::spawn(serve(listener, server.clone()));
    (format!("ws://{addr}/ws"), server)
}

struct Client {
    ws: Ws,
}

impl Client {
    async fn connect(url: &str) -> Client {
        let (ws, _) = connect_async(url).await.unwrap();
        let mut c = Client { ws };
        c.send(ClientMsg::Hello { version: PROTOCOL_VERSION, rules: Rules::Brazilian }).await;
        c
    }

    async fn send(&mut self, msg: ClientMsg) {
        self.ws.send(Message::Text(msg.to_json().into())).await.unwrap();
    }

    /// Reads messages until one matches `pred`.
    async fn expect<F: Fn(&ServerMsg) -> bool>(&mut self, what: &str, pred: F) -> ServerMsg {
        self.until(what, pred).await.1
    }

    /// Reads messages until one matches `pred`: those before it, and it.
    async fn until<F: Fn(&ServerMsg) -> bool>(
        &mut self,
        what: &str,
        pred: F,
    ) -> (Vec<ServerMsg>, ServerMsg) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        let mut before = Vec::new();
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
                    return (before, m);
                }
                before.push(m);
            }
        }
    }

    async fn register_as(url: &str, name: &str, minor: Option<bool>) -> (Client, String) {
        let mut c = Client::connect(url).await;
        c.send(ClientMsg::Register {
            username: name.into(),
            password: PASSWORD.into(),
            country: "BR".into(),
            minor,
        })
        .await;
        let ServerMsg::AuthOk { token, .. } =
            c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await
        else {
            unreachable!()
        };
        (c, token)
    }

    /// An adult (their chat is on).
    async fn register(url: &str, name: &str) -> (Client, String) {
        Client::register_as(url, name, Some(false)).await
    }

    async fn resume(url: &str, token: &str) -> Client {
        let mut c = Client::connect(url).await;
        c.send(ClientMsg::Resume { token: token.into() }).await;
        c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
        c
    }
}

/// `a` and `b` become friends.
async fn befriend(a: &mut Client, b: &mut Client, a_name: &str, b_name: &str) {
    a.send(ClientMsg::FriendRequest { username: b_name.into() }).await;
    b.expect("request", |m| matches!(m, ServerMsg::FriendRequestReceived { .. })).await;
    b.send(ClientMsg::FriendAnswer { username: a_name.into(), accept: true }).await;
    for c in [a, b] {
        c.expect(
            "friends",
            |m| matches!(m, ServerMsg::Friends { friends, .. } if !friends.is_empty()),
        )
        .await;
    }
}

/// `a` invites `b` and the game starts; returns `a`'s start message.
async fn play(
    a: &mut Client,
    b: &mut Client,
    b_name: &str,
    a_name: &str,
    rated: bool,
) -> ServerMsg {
    a.send(ClientMsg::Invite { to: b_name.into(), rated, rules: Rules::Brazilian }).await;
    b.expect("invite", |m| matches!(m, ServerMsg::Invited { .. })).await;
    b.send(ClientMsg::AnswerInvite { from: a_name.into(), accept: true }).await;
    b.expect("started b", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    a.expect("started a", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
}

fn error_text(m: &ServerMsg) -> &str {
    match m {
        ServerMsg::Error { message } => message,
        other => panic!("not an error: {other:?}"),
    }
}

#[tokio::test]
async fn settings_follow_the_account() {
    let (url, _) = start().await;
    let (mut ana, token) = Client::register(&url, "ana").await;
    let data = r#"{"guide":"Short","mute_chat":"true"}"#;
    ana.send(ClientMsg::SaveSettings { data: data.into() }).await;
    let ServerMsg::SettingsSaved { updated_at } =
        ana.expect("saved", |m| matches!(m, ServerMsg::SettingsSaved { .. })).await
    else {
        unreachable!()
    };
    // Not JSON, or too big: not kept.
    ana.send(ClientMsg::SaveSettings { data: "guide = Short".into() }).await;
    ana.expect("refused", |m| matches!(m, ServerMsg::Error { .. })).await;
    let big = format!(r#"{{"x":"{}"}}"#, "a".repeat(9000));
    ana.send(ClientMsg::SaveSettings { data: big }).await;
    ana.expect("refused", |m| matches!(m, ServerMsg::Error { .. })).await;
    drop(ana);
    // Signing in (on another computer): the settings come.
    let mut ana = Client::connect(&url).await;
    ana.send(ClientMsg::Resume { token }).await;
    let ServerMsg::Settings { data: got, updated_at: at } =
        ana.expect("settings", |m| matches!(m, ServerMsg::Settings { .. })).await
    else {
        unreachable!()
    };
    assert_eq!((got.as_str(), at), (data, updated_at));
}

#[tokio::test]
async fn friends_send_each_other_messages() {
    let (url, _) = start().await;
    let (mut ana, _) = Client::register(&url, "ana").await;
    let (mut bob, bob_token) = Client::register(&url, "bob").await;
    befriend(&mut ana, &mut bob, "ana", "bob").await;
    ana.send(ClientMsg::DmSend { to: "Bob".into(), text: "  hi bob  ".into() }).await;
    let ServerMsg::Dm { message: sent } =
        ana.expect("kept", |m| matches!(m, ServerMsg::Dm { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(
        (sent.from.as_str(), sent.to.as_str(), sent.text.as_str()),
        ("ana", "bob", "hi bob")
    );
    let ServerMsg::Dm { message } = bob.expect("got", |m| matches!(m, ServerMsg::Dm { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(message, sent);
    // Offline, it waits: the list after signing in says it's unread.
    drop(bob);
    tokio::time::sleep(Duration::from_millis(200)).await;
    ana.send(ClientMsg::DmSend { to: "bob".into(), text: "are you there?".into() }).await;
    ana.expect("kept", |m| matches!(m, ServerMsg::Dm { .. })).await;
    let mut bob = Client::connect(&url).await;
    bob.send(ClientMsg::Resume { token: bob_token }).await;
    let ServerMsg::DmThreads { threads } =
        bob.expect("threads", |m| matches!(m, ServerMsg::DmThreads { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(threads.len(), 1);
    assert_eq!((threads[0].with.as_str(), threads[0].unread), ("ana", 2));
    assert_eq!(threads[0].last.text, "are you there?");
    bob.send(ClientMsg::DmHistory { with: "ana".into(), before: None }).await;
    let ServerMsg::DmHistory { with, messages, more } =
        bob.expect("history", |m| matches!(m, ServerMsg::DmHistory { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(with, "ana");
    assert!(!more);
    let texts: Vec<&str> = messages.iter().map(|m| m.text.as_str()).collect();
    assert_eq!(texts, ["hi bob", "are you there?"], "oldest first");
    bob.send(ClientMsg::DmRead { with: "ana".into() }).await;
    bob.send(ClientMsg::DmThreads).await;
    let ServerMsg::DmThreads { threads } =
        bob.expect("threads", |m| matches!(m, ServerMsg::DmThreads { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(threads[0].unread, 0);
    // A block ends it (either way), without saying why.
    bob.send(ClientMsg::Block { username: "ana".into() }).await;
    bob.expect("friends", |m| matches!(m, ServerMsg::Friends { .. })).await;
    ana.send(ClientMsg::DmSend { to: "bob".into(), text: "?".into() }).await;
    let m = ana.expect("refused", |m| matches!(m, ServerMsg::Error { .. })).await;
    assert_eq!(error_text(&m), snook_legend_server::dm::CANT_WRITE);
}

/// Adults who aren't friends write to each other: a few messages until
/// the other answers, and not to someone who takes messages from friends
/// only.
#[tokio::test]
async fn players_who_arent_friends_write_to_each_other() {
    use snook_legend_proto::dm::UNANSWERED;
    use snook_legend_server::dm::{THEIR_FRIENDS_ONLY, WAIT_FOR_ANSWER};
    let (url, _) = start().await;
    let (mut ana, _) = Client::register(&url, "ana").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    for k in 0..UNANSWERED {
        ana.send(ClientMsg::DmSend { to: "bob".into(), text: format!("hi {k}") }).await;
        ana.expect("kept", |m| matches!(m, ServerMsg::Dm { .. })).await;
    }
    ana.send(ClientMsg::DmSend { to: "bob".into(), text: "hello?".into() }).await;
    let m = ana.expect("waits", |m| matches!(m, ServerMsg::Error { .. })).await;
    assert_eq!(error_text(&m), WAIT_FOR_ANSWER);
    // Bob got them, and answers: the conversation goes on.
    bob.expect("got", |m| matches!(m, ServerMsg::Dm { message } if message.text == "hi 2")).await;
    bob.send(ClientMsg::DmSend { to: "ana".into(), text: "hey".into() }).await;
    bob.expect("kept", |m| matches!(m, ServerMsg::Dm { .. })).await;
    ana.send(ClientMsg::DmSend { to: "bob".into(), text: "hello!".into() }).await;
    ana.expect("kept", |m| matches!(m, ServerMsg::Dm { message } if message.text == "hello!"))
        .await;
    // Cid takes messages from friends only (Settings → Game).
    let (mut cid, _) = Client::register(&url, "cid").await;
    let data = r#"{"messages_from_anyone":"false"}"#;
    cid.send(ClientMsg::SaveSettings { data: data.into() }).await;
    cid.expect("saved", |m| matches!(m, ServerMsg::SettingsSaved { .. })).await;
    // (Bob writes: Ana has written as quickly as she may for a while.)
    bob.send(ClientMsg::DmSend { to: "cid".into(), text: "hi".into() }).await;
    let m = bob.expect("friends only", |m| matches!(m, ServerMsg::Error { .. })).await;
    assert_eq!(error_text(&m), THEIR_FRIENDS_ONLY);
    befriend(&mut bob, &mut cid, "bob", "cid").await;
    bob.send(ClientMsg::DmSend { to: "cid".into(), text: "hi".into() }).await;
    bob.expect("a friend's", |m| matches!(m, ServerMsg::Dm { .. })).await;
    // Nobody by that name.
    bob.send(ClientMsg::DmSend { to: "nobody".into(), text: "hi".into() }).await;
    let m = bob.expect("no one", |m| matches!(m, ServerMsg::Error { .. })).await;
    assert_eq!(error_text(&m), snook_legend_server::dm::NO_SUCH_PLAYER);
}

/// Minors write to their friends only, and only friends write to them.
#[tokio::test]
async fn minors_write_to_friends_only() {
    use snook_legend_server::dm::{CANT_WRITE, FRIENDS_ONLY};
    let (url, _) = start().await;
    let (mut ana, _) = Client::register(&url, "ana").await;
    let (mut kid, _) = Client::register_as(&url, "kid", Some(true)).await;
    // A parent turned the chat on (not kept to the quick phrases).
    kid.send(ClientMsg::AllowChat { allowed: true, phrases_only: Some(false) }).await;
    kid.expect("age", |m| matches!(m, ServerMsg::Age { .. })).await;
    kid.send(ClientMsg::DmSend { to: "ana".into(), text: "hi".into() }).await;
    let m = kid.expect("refused", |m| matches!(m, ServerMsg::Error { .. })).await;
    assert_eq!(error_text(&m), FRIENDS_ONLY);
    ana.send(ClientMsg::DmSend { to: "kid".into(), text: "hi".into() }).await;
    let m = ana.expect("refused", |m| matches!(m, ServerMsg::Error { .. })).await;
    assert_eq!(error_text(&m), CANT_WRITE, "their age isn't given away");
    befriend(&mut ana, &mut kid, "ana", "kid").await;
    kid.send(ClientMsg::DmSend { to: "ana".into(), text: "hi".into() }).await;
    kid.expect("kept", |m| matches!(m, ServerMsg::Dm { .. })).await;
}

/// A 0.11 game (protocol 24) has no direct messages: it isn't sent them
/// (they wait for when the player updates).
#[tokio::test]
async fn old_games_are_not_sent_messages() {
    let (url, _) = start().await;
    let (mut ana, _) = Client::register(&url, "ana").await;
    let (mut bob, bob_token) = Client::register(&url, "bob").await;
    befriend(&mut ana, &mut bob, "ana", "bob").await;
    ana.send(ClientMsg::DmSend { to: "bob".into(), text: "hi".into() }).await;
    ana.expect("kept", |m| matches!(m, ServerMsg::Dm { .. })).await;
    drop(bob);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (ws, _) = connect_async(&url).await.unwrap();
    let mut old = Client { ws };
    old.send(ClientMsg::Hello { version: 24, rules: Rules::Brazilian }).await;
    old.send(ClientMsg::Resume { token: bob_token }).await;
    old.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
    ana.send(ClientMsg::DmSend { to: "bob".into(), text: "there?".into() }).await;
    ana.expect("kept", |m| matches!(m, ServerMsg::Dm { .. })).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    old.send(ClientMsg::Ping).await;
    let (before, _) = old.until("pong", |m| matches!(m, ServerMsg::Pong)).await;
    let dms = before
        .iter()
        .filter(|m| matches!(m, ServerMsg::Dm { .. } | ServerMsg::DmThreads { .. }))
        .count();
    assert_eq!(dms, 0, "{before:?}");
}

#[tokio::test]
async fn messages_to_strangers_count_against_the_limit() {
    let (url, _) = start().await;
    let (mut ana, _) = Client::register(&url, "ana").await;
    // Each one costs database requests (who it is, the friends, the
    // blocks) before it is refused: past the burst the server stops asking.
    let mut errors = Vec::new();
    for _ in 0..8 {
        ana.send(ClientMsg::DmSend { to: "nobody".into(), text: "hi".into() }).await;
        let m = ana.expect("refused", |m| matches!(m, ServerMsg::Error { .. })).await;
        errors.push(error_text(&m).to_string());
    }
    assert_eq!(errors[0], snook_legend_server::dm::NO_SUCH_PLAYER);
    assert_eq!(errors[7], snook_legend_server::dm::TOO_QUICKLY);
}

#[tokio::test]
async fn messages_follow_the_chat_rules() {
    let (url, _) = start().await;
    let (mut ana, _) = Client::register(&url, "ana").await;
    // Under 18, the chat off: they can be friends, not write.
    let (mut kid, _) = Client::register_as(&url, "kid", Some(true)).await;
    befriend(&mut ana, &mut kid, "ana", "kid").await;
    kid.send(ClientMsg::DmSend { to: "ana".into(), text: "hi".into() }).await;
    let m = kid.expect("off", |m| matches!(m, ServerMsg::Error { .. })).await;
    assert_eq!(error_text(&m), snook_legend_server::lounge::CHAT_OFF_FOR_MINORS);
    ana.send(ClientMsg::DmSend { to: "kid".into(), text: "hi".into() }).await;
    let m = ana.expect("refused", |m| matches!(m, ServerMsg::Error { .. })).await;
    assert_eq!(error_text(&m), snook_legend_server::dm::CANT_WRITE);
    // A parent turns the chat on, the quick phrases only.
    kid.send(ClientMsg::AllowChat { allowed: true, phrases_only: Some(true) }).await;
    let ServerMsg::Age { chat_allowed, phrases_only, .. } =
        kid.expect("age", |m| matches!(m, ServerMsg::Age { .. })).await
    else {
        unreachable!()
    };
    assert!(chat_allowed && phrases_only);
    kid.send(ClientMsg::DmSend { to: "ana".into(), text: "my address is…".into() }).await;
    let m = kid.expect("phrases", |m| matches!(m, ServerMsg::Error { .. })).await;
    assert_eq!(error_text(&m), snook_legend_server::lounge::PHRASES_ONLY);
    kid.send(ClientMsg::DmSend { to: "ana".into(), text: "Good game!".into() }).await;
    ana.expect(
        "a phrase",
        |m| matches!(m, ServerMsg::Dm { message } if message.text == "Good game!"),
    )
    .await;
    // The lounge's chat too.
    kid.send(ClientMsg::Chat { text: "hello everyone".into() }).await;
    let m = kid.expect("phrases", |m| matches!(m, ServerMsg::Error { .. })).await;
    assert_eq!(error_text(&m), snook_legend_server::lounge::PHRASES_ONLY);
    kid.send(ClientMsg::Chat { text: "Wow!".into() }).await;
    ana.expect("lounge", |m| matches!(m, ServerMsg::Chat { line } if line.text == "Wow!")).await;
    // A game before 0.12 turning the chat off and on leaves the choice.
    kid.send(ClientMsg::AllowChat { allowed: true, phrases_only: None }).await;
    let m = kid.expect("age", |m| matches!(m, ServerMsg::Age { .. })).await;
    assert!(matches!(m, ServerMsg::Age { phrases_only: true, .. }));
    kid.send(ClientMsg::AllowChat { allowed: true, phrases_only: Some(false) }).await;
    let m = kid.expect("age", |m| matches!(m, ServerMsg::Age { .. })).await;
    assert!(matches!(m, ServerMsg::Age { phrases_only: false, .. }));
    kid.send(ClientMsg::Chat { text: "hello everyone".into() }).await;
    ana.expect("free", |m| matches!(m, ServerMsg::Chat { line } if line.text == "hello everyone"))
        .await;
}

#[tokio::test]
async fn friends_see_what_each_other_does_and_spectators_chat_apart() {
    let (url, _) = start().await;
    let (mut ana, _) = Client::register(&url, "ana").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (mut cid, _) = Client::register(&url, "cid").await;
    let (mut dan, _) = Client::register(&url, "dan").await;
    befriend(&mut ana, &mut bob, "ana", "bob").await;
    befriend(&mut dan, &mut bob, "dan", "bob").await;
    // Ana plays Cid: Bob, her friend, is told, and can watch.
    play(&mut ana, &mut cid, "cid", "ana", false).await;
    let m = bob
        .expect("activity", |m| {
            matches!(m, ServerMsg::FriendActivity { activity: Activity::Playing { .. }, .. })
        })
        .await;
    let ServerMsg::FriendActivity { username, activity: Activity::Playing { opponent, .. } } = m
    else {
        unreachable!()
    };
    assert_eq!((username.as_str(), opponent.as_str()), ("ana", "cid"));
    // The friends list says it too.
    bob.send(ClientMsg::Friends).await;
    let ServerMsg::Friends { friends, .. } =
        bob.expect("friends", |m| matches!(m, ServerMsg::Friends { .. })).await
    else {
        unreachable!()
    };
    let ana_info = friends.iter().find(|f| f.player.username == "ana").unwrap();
    assert!(matches!(&ana_info.activity, Activity::Playing { opponent, .. } if opponent == "cid"));
    // Bob and Dan watch: Dan (Bob's friend) hears Bob is watching.
    bob.send(ClientMsg::Spectate { username: "ana".into() }).await;
    bob.expect("watching", |m| matches!(m, ServerMsg::MatchStarted { spectating: true, .. })).await;
    dan.expect("bob watches", |m| {
        matches!(m, ServerMsg::FriendActivity { username, activity: Activity::Watching { .. } } if username == "bob")
    })
    .await;
    dan.send(ClientMsg::Spectate { username: "bob".into() }).await;
    let m = dan.expect("not playing", |m| matches!(m, ServerMsg::Error { .. })).await;
    assert!(error_text(&m).contains("isn't playing"));
    // (Dan isn't Ana's friend; Bob is, and chats with the other watchers.)
    bob.send(ClientMsg::SpectatorChat { text: "what a break".into() }).await;
    let ServerMsg::SpectatorChat { line } =
        bob.expect("own line", |m| matches!(m, ServerMsg::SpectatorChat { .. })).await
    else {
        unreachable!()
    };
    assert_eq!((line.from.as_str(), line.text.as_str()), ("bob", "what a break"));
    // The players never see it: Cid's line comes, nothing before it does.
    cid.send(ClientMsg::MatchChat { text: "marker".into() }).await;
    for p in [&mut ana, &mut cid] {
        let (before, _) = p
            .until(
                "marker",
                |m| matches!(m, ServerMsg::MatchChat { line } if line.text == "marker"),
            )
            .await;
        assert!(!before.iter().any(|m| matches!(m, ServerMsg::SpectatorChat { .. })));
    }
    // Not watching: nothing to say there.
    dan.send(ClientMsg::SpectatorChat { text: "hi".into() }).await;
    dan.expect("refused", |m| matches!(m, ServerMsg::Error { .. })).await;
    // Bob stops: Dan hears he is back in the lounge.
    bob.send(ClientMsg::StopSpectating).await;
    dan.expect("bob in the lounge", |m| {
        matches!(m, ServerMsg::FriendActivity { username, activity: Activity::Lounge } if username == "bob")
    })
    .await;
}

#[tokio::test]
async fn a_rated_game_says_what_is_at_stake() {
    let (url, _) = start().await;
    let (mut ana, _) = Client::register(&url, "ana").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let ServerMsg::MatchStarted { stakes, .. } = play(&mut ana, &mut bob, "bob", "ana", true).await
    else {
        unreachable!()
    };
    // Two new players at 1200: half the provisional K each way.
    assert_eq!(stakes, Some([[12, -12], [12, -12]]));
    ana.send(ClientMsg::Resign).await;
    ana.expect("over", |m| matches!(m, ServerMsg::MatchEnded { .. })).await;
    bob.expect("over", |m| matches!(m, ServerMsg::MatchEnded { .. })).await;
    ana.send(ClientMsg::LeaveMatch).await;
    bob.send(ClientMsg::LeaveMatch).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    // A casual game says nothing of rating.
    let ServerMsg::MatchStarted { stakes, .. } =
        play(&mut ana, &mut bob, "bob", "ana", false).await
    else {
        unreachable!()
    };
    assert_eq!(stakes, None);
}

#[tokio::test]
async fn advanced_statistics_are_for_vip_members() {
    let (url, server) = start().await;
    let (mut ana, token) = Client::register(&url, "ana").await;
    ana.send(ClientMsg::AdvancedStats { rules: Rules::Brazilian }).await;
    let ServerMsg::AdvancedStats { stats } =
        ana.expect("stats", |m| matches!(m, ServerMsg::AdvancedStats { .. })).await
    else {
        unreachable!()
    };
    assert!(!stats.vip);
    assert_eq!(stats.mine, Default::default(), "no numbers without VIP");
    assert_eq!(stats.analysis, None, "nor the shot analysis");
    // VIP, with games analysed over two weeks.
    let id = server.store.user_by_name("ana").await.unwrap().unwrap().id;
    let now = snook_legend_server::now();
    server.store.set_vip(id, now + 3600, true).await.unwrap();
    let mut a = ShotAnalysis { games: 3, ..Default::default() };
    a.pockets[0] = Tally { attempts: 10, made: 7 };
    a.kinds[0] = Tally { attempts: 10, made: 7 };
    server.store.add_shot_analysis(id, &a).await.unwrap();
    let week = snook_legend_proto::advanced::week_of(now);
    server.store.add_shot_week(id, week - 1, 4, 1).await.unwrap();
    server.store.add_shot_week(id, week, 6, 6).await.unwrap();
    drop(ana);
    let mut ana = Client::resume(&url, &token).await;
    ana.send(ClientMsg::AdvancedStats { rules: Rules::Brazilian }).await;
    let ServerMsg::AdvancedStats { stats } =
        ana.expect("stats", |m| matches!(m, ServerMsg::AdvancedStats { .. })).await
    else {
        unreachable!()
    };
    assert!(stats.vip);
    assert_eq!(stats.games, 3);
    assert_eq!(stats.analysis.as_deref(), Some(&a), "the whole analysis, for VIP");
    assert_eq!(stats.mine.pockets[0], Tally { attempts: 10, made: 7 });
    assert_eq!(stats.mine.cuts[0], Tally { attempts: 10, made: 7 });
    let weeks: Vec<(i64, u32)> = stats.weeks.iter().map(|w| (w.week, w.pots.made)).collect();
    assert_eq!(weeks, [(week - 1, 1), (week, 6)], "oldest first");
    // No rated game yet: no league of their own (the league shown is the
    // one they start in, which has nobody with rated games).
    assert_eq!(stats.tier, None);
    assert_eq!(stats.league_players, 0);
}
