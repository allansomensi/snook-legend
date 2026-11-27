//! End-to-end tests: real WebSocket clients against a server on a random port.

mod common;

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use snook_legend_core::{BallInHand, Difficulty, EightBallGame, ai};
use snook_legend_proto::{
    ClientMsg, PROTOCOL_VERSION, ReportReason, Rules, ServerMsg, TournamentState, parse_server,
};
use snook_legend_server::lounge::Settings;
use snook_legend_server::{Config, Server, serve};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// How often a test connection pings the server, as the game does: the
/// server lets go of a connection it hasn't heard from in 75 s, and a test
/// held up that long (the whole file running at once, games played out on
/// every side) would otherwise lose a player who was only waiting.
const KEEPALIVE: Duration = Duration::from_secs(20);

/// A test's connection to the server. What it reads comes in order; what it
/// sends goes out through a writer of its own, which also pings the server
/// now and then (a WebSocket ping: never answered with a message, so no
/// test sees it). Dropping it closes the connection.
struct Ws {
    read: futures_util::stream::SplitStream<Socket>,
    write: tokio::sync::mpsc::UnboundedSender<Message>,
}

impl Ws {
    async fn connect(url: &str) -> Ws {
        let (socket, _) = connect_async(url).await.unwrap();
        let (mut sink, read) = socket.split();
        let (write, mut outgoing) = tokio::sync::mpsc::unbounded_channel::<Message>();
        tokio::spawn(async move {
            let mut ping = tokio::time::interval(KEEPALIVE);
            ping.tick().await;
            loop {
                let msg = tokio::select! {
                    msg = outgoing.recv() => match msg {
                        Some(msg) => msg,
                        None => {
                            let _ = sink.send(Message::Close(None)).await;
                            return;
                        }
                    },
                    _ = ping.tick() => Message::Ping(Vec::new().into()),
                };
                if sink.send(msg).await.is_err() {
                    return;
                }
            }
        });
        Ws { read, write }
    }

    /// Sends `msg` (after whatever was sent before it); an error once the
    /// connection is gone.
    async fn send(&mut self, msg: Message) -> Result<(), &'static str> {
        self.write.send(msg).map_err(|_| "the connection is closed")
    }

    async fn next(&mut self) -> Option<Result<Message, tokio_tungstenite::tungstenite::Error>> {
        self.read.next().await
    }
}

const PASSWORD: &str = "secret123";

async fn start_with(settings: Settings, admins: Vec<String>) -> (String, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let settings = Settings { max_connections_per_ip: 100, ..settings };
    let config = Config { bind: addr, settings, admins: admins.clone(), ..Config::default() };
    let server = Server::new(config).await.unwrap();
    // An administrator's name can't be registered: its account comes
    // first (with the test password), and signing in makes it one.
    for name in &admins {
        make_account(&server, name).await;
    }
    tokio::spawn(serve(listener, server));
    (format!("ws://{addr}/ws"), format!("http://{addr}"))
}

/// An account made straight in the store, with the test password.
async fn make_account(server: &Server, name: &str) {
    let hash = server.hash_password(PASSWORD).await.unwrap();
    let user = snook_legend_server::store::NewUser {
        username: name,
        password_hash: &hash,
        country: "BR",
        rating: 1200,
        role: snook_legend_proto::Role::Player,
        ip: "127.0.0.1",
        // Adults: their chat is on (it is off until the age group is given).
        minor: Some(false),
    };
    server.store.create_user(&user).await.unwrap().expect("a new account");
}

async fn start(settings: Settings) -> String {
    start_with(settings, Vec::new()).await.0
}

struct Client {
    ws: Ws,
}

impl Client {
    async fn connect(url: &str) -> Client {
        Client::connect_as(url, PROTOCOL_VERSION).await
    }

    /// A game of protocol `version`.
    async fn connect_as(url: &str, version: u32) -> Client {
        let mut c = Client { ws: Ws::connect(url).await };
        c.send(ClientMsg::Hello { version, rules: Rules::Brazilian }).await;
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

    async fn register(url: &str, name: &str) -> (Client, String) {
        Client::register_as(url, name, PROTOCOL_VERSION).await
    }

    /// [`Client::register`] from a game of protocol `version`.
    async fn register_as(url: &str, name: &str, version: u32) -> (Client, String) {
        let mut c = Client::connect_as(url, version).await;
        c.send(ClientMsg::Register {
            username: name.into(),
            password: PASSWORD.into(),
            country: "BR".into(),
            // Adults: their chat is on (it is off until the age group is given).
            minor: Some(false),
        })
        .await;
        let ServerMsg::AuthOk { token, player } =
            c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await
        else {
            unreachable!()
        };
        assert_eq!(player.rating_in(Rules::Brazilian), 1200);
        assert_eq!(player.rating_in(Rules::Official), 1200);
        assert_eq!(player.country, "BR");
        (c, token)
    }
}

/// Invites `b` from `a`, accepts, and returns (game, seat of a, seat of b).
async fn start_match(
    a: &mut Client,
    b: &mut Client,
    b_name: &str,
    a_name: &str,
    rated: bool,
) -> (EightBallGame, usize, usize) {
    start_match_under(a, b, b_name, a_name, rated, Rules::Brazilian).await
}

/// [`start_match`] under `rules`.
async fn start_match_under(
    a: &mut Client,
    b: &mut Client,
    b_name: &str,
    a_name: &str,
    rated: bool,
    rules: Rules,
) -> (EightBallGame, usize, usize) {
    a.send(ClientMsg::Invite { to: b_name.into(), rated, rules }).await;
    let ServerMsg::Invited { rules: offered, .. } =
        b.expect("invite", |m| matches!(m, ServerMsg::Invited { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(offered, rules, "the invitation says which rules");
    b.send(ClientMsg::AnswerInvite { from: a_name.into(), accept: true }).await;
    let ServerMsg::MatchStarted { game, seat: sa, .. } =
        a.expect("match a", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
    else {
        unreachable!()
    };
    let ServerMsg::MatchStarted { seat: sb, .. } =
        b.expect("match b", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
    else {
        unreachable!()
    };
    assert_ne!(sa, sb);
    assert_eq!(game.rules(), rules);
    (game, sa, sb)
}

/// Plays a whole game with the AI on both sides; returns the result.
async fn play_out(
    a: &mut Client,
    b: &mut Client,
    game: EightBallGame,
    sa: usize,
) -> snook_legend_proto::MatchResult {
    play_out_with(a, b, game, sa, false).await
}

/// [`play_out`]; `a_calls_nothing`: `a` is a game that can't call the 8
/// (older than 0.12.0), and sends no call.
async fn play_out_with(
    a: &mut Client,
    b: &mut Client,
    mut game: EightBallGame,
    sa: usize,
    a_calls_nothing: bool,
) -> snook_legend_proto::MatchResult {
    let mut shots = 0;
    loop {
        assert!(shots < 400, "game did not finish");
        let shooter_is_a = game.current_player() == sa;
        // The official rules: a break can leave a choice to make first.
        if let Some(option) = ai::choose(&game) {
            let turn = game.shots_taken();
            let msg = ClientMsg::Choose { turn, option };
            if shooter_is_a {
                a.send(msg).await;
            } else {
                b.send(msg).await;
            }
            let mut answers = Vec::new();
            for c in [&mut *a, &mut *b] {
                let ServerMsg::Chose { option: o, game: g, .. } =
                    c.expect("chose", |m| matches!(m, ServerMsg::Chose { .. })).await
                else {
                    unreachable!()
                };
                assert_eq!(o, option);
                answers.push(g);
            }
            assert_eq!(answers[0], answers[1], "both players get the same table");
            let mut local = game.clone();
            local.choose(option).unwrap();
            assert_eq!(local, answers[0]);
            game = local;
            continue;
        }
        let mut decision = ai::decide(&game, Difficulty::Hard, shots);
        if a_calls_nothing && shooter_is_a {
            decision.called_pocket = None;
        }
        let turn = game.shots_taken();
        // The official rules (a game that can call the 8): a shot at the 8
        // without its pocket called is refused, as under the Brazilian
        // rules; the shooter is asked for a pocket.
        if game.calls_the_eight() && !(a_calls_nothing && shooter_is_a) {
            let bare = ClientMsg::Shoot {
                turn,
                shot: decision.shot,
                called_pocket: None,
                cue_ball: decision.cue_ball,
            };
            let shooter = if shooter_is_a { &mut *a } else { &mut *b };
            shooter.send(bare).await;
            let refused = shooter
                .expect("shot refused", |m| {
                    matches!(m, ServerMsg::ShotRejected { .. } | ServerMsg::ShotTaken { .. })
                })
                .await;
            assert!(
                matches!(&refused, ServerMsg::ShotRejected { reason, game: g, .. }
                    if reason.contains("call a pocket") && *g == game),
                "a shot at the 8 without a call: {refused:?}"
            );
        }
        let msg = ClientMsg::Shoot {
            turn,
            shot: decision.shot,
            called_pocket: decision.called_pocket,
            cue_ball: decision.cue_ball,
        };
        if shooter_is_a {
            a.send(msg).await;
        } else {
            b.send(msg).await;
        }
        shots += 1;
        // Both players hear about the shot first, then get the outcome.
        for c in [&mut *a, &mut *b] {
            let taken = c
                .expect("shot taken", |m| {
                    matches!(m, ServerMsg::ShotTaken { .. } | ServerMsg::ShotRejected { .. })
                })
                .await;
            assert!(
                matches!(taken, ServerMsg::ShotTaken { turn: t, .. } if t == turn),
                "shot rejected: {taken:?}"
            );
        }
        let ServerMsg::ShotResult { end, turn: t, .. } =
            a.expect("result", |m| matches!(m, ServerMsg::ShotResult { .. })).await
        else {
            unreachable!()
        };
        assert_eq!(t, turn);
        let ServerMsg::ShotResult { end: end_b, .. } =
            b.expect("result b", |m| matches!(m, ServerMsg::ShotResult { .. })).await
        else {
            unreachable!()
        };
        assert_eq!(end, end_b, "both players get the same authoritative state");
        // The client simulation agrees with the server's.
        let mut local = game.clone();
        if let Some(p) = decision.cue_ball {
            local.place_cue_ball(p);
        }
        let mut pending = local.prepare_shot(&decision.shot, decision.called_pocket).unwrap();
        pending.simulation.run_to_rest();
        // A game that can't call the 8: the 8 counts as called in the
        // pocket it goes into.
        if a_calls_nothing && shooter_is_a {
            local.call_the_eight_where_it_went(&mut pending);
        }
        local.finish_shot(pending);
        assert_eq!(local, end, "deterministic physics");
        game = end;
        if game.game_over().is_some() {
            let ServerMsg::MatchEnded { result } =
                a.expect("end", |m| matches!(m, ServerMsg::MatchEnded { .. })).await
            else {
                unreachable!()
            };
            b.expect("end b", |m| matches!(m, ServerMsg::MatchEnded { .. })).await;
            return result;
        }
    }
}

#[tokio::test]
async fn full_rated_match_updates_ratings_profile_and_leaderboard() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;

    // Lounge chat reaches everyone.
    alice.send(ClientMsg::Chat { text: "hello  ".into() }).await;
    let ServerMsg::Chat { line } =
        bob.expect("chat", |m| matches!(m, ServerMsg::Chat { .. })).await
    else {
        unreachable!()
    };
    assert_eq!((line.from.as_str(), line.text.as_str()), ("alice", "hello"));

    let (game, sa, _sb) = start_match(&mut alice, &mut bob, "bob", "alice", true).await;
    let result = play_out(&mut alice, &mut bob, game, sa).await;
    assert!(result.rated);
    assert!(result.rating_delta[result.winner] > 0);
    assert!(result.rating_delta[1 - result.winner] < 0);
    // The winner unlocks "first win".
    let winner_is_alice = result.winner == sa;
    let w = if winner_is_alice { &mut alice } else { &mut bob };
    w.expect(
        "achievement",
        |m| matches!(m, ServerMsg::AchievementUnlocked { code } if code == "wins_1"),
    )
    .await;

    alice.send(ClientMsg::Leaderboard { rules: Rules::Brazilian }).await;
    let ServerMsg::Leaderboard { rules, entries } =
        alice.expect("leaderboard", |m| matches!(m, ServerMsg::Leaderboard { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(rules, Rules::Brazilian);
    assert_eq!(entries.len(), 2);
    assert!(entries[0].rating > entries[1].rating);
    assert_eq!(entries[0].wins, 1);
    assert_eq!(entries[0].country, "BR");

    alice.send(ClientMsg::Profile { username: "BOB".into() }).await;
    let ServerMsg::Profile { profile } =
        alice.expect("profile", |m| matches!(m, ServerMsg::Profile { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(profile.player.username, "bob");
    assert_eq!(profile.history.len(), 1);
    assert_eq!(profile.history[0].opponent, "alice");
    assert_eq!(profile.history[0].won, !winner_is_alice);
    assert!(profile.achievements.iter().any(|(c, _)| c == "games_1"));

    // Rematch needs both players.
    alice.send(ClientMsg::Rematch).await;
    bob.expect("rematch offer", |m| matches!(m, ServerMsg::RematchOffered)).await;
    bob.send(ClientMsg::Rematch).await;
    alice.expect("rematch", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
}

#[tokio::test]
async fn out_of_turn_and_stale_shots_are_rejected() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (game, sa, _) = start_match(&mut alice, &mut bob, "bob", "alice", false).await;
    let (shooter, other) =
        if game.current_player() == sa { (&mut alice, &mut bob) } else { (&mut bob, &mut alice) };
    let d = ai::decide(&game, Difficulty::Easy, 1);
    let shoot = |turn| ClientMsg::Shoot {
        turn,
        shot: d.shot,
        called_pocket: d.called_pocket,
        cue_ball: d.cue_ball,
    };
    other.send(shoot(0)).await;
    other.expect("not your turn", |m| matches!(m, ServerMsg::ShotRejected { .. })).await;
    shooter.send(shoot(5)).await;
    shooter.expect("stale", |m| matches!(m, ServerMsg::ShotRejected { .. })).await;
    // Infinity can't be written as JSON: send it by hand.
    let text = shoot(0).to_json().replace(&format!("{}", d.shot.yaw), "1e999");
    shooter.ws.send(Message::Text(text.into())).await.unwrap();
    let m = shooter
        .expect("bad", |m| {
            matches!(
                m,
                ServerMsg::ShotRejected { .. }
                    | ServerMsg::Error { .. }
                    | ServerMsg::ShotTaken { .. }
            )
        })
        .await;
    assert!(!matches!(m, ServerMsg::ShotTaken { .. }), "infinite shot accepted");
}

#[tokio::test]
async fn reactions_need_to_be_unlocked() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    alice.expect("achievements", |m| matches!(m, ServerMsg::MyAchievements { .. })).await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    start_match(&mut alice, &mut bob, "bob", "alice", false).await;
    // The crown needs a 1900 rating, the gem VIP: dropped. The clap is for
    // everyone.
    alice.send(ClientMsg::React { code: "crown".into() }).await;
    alice.send(ClientMsg::React { code: "gem".into() }).await;
    alice.send(ClientMsg::React { code: "nonsense".into() }).await;
    alice.send(ClientMsg::React { code: "clap".into() }).await;
    let ServerMsg::Reaction { code, .. } =
        bob.expect("reaction", |m| matches!(m, ServerMsg::Reaction { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(code, "clap");
    // A few a second at most.
    for _ in 0..10 {
        alice.send(ClientMsg::React { code: "wow".into() }).await;
    }
    alice.send(ClientMsg::MatchChat { text: "end".into() }).await;
    let mut seen = 0;
    while let ServerMsg::Reaction { .. } = bob
        .expect("more", |m| matches!(m, ServerMsg::Reaction { .. } | ServerMsg::MatchChat { .. }))
        .await
    {
        seen += 1;
    }
    assert!(seen <= 3, "{seen} reactions got through");
}

#[tokio::test]
async fn friends_can_watch_and_react() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (mut carol, _) = Client::register(&url, "carol").await;
    let (mut dave, _) = Client::register(&url, "dave").await;
    carol.send(ClientMsg::FriendRequest { username: "alice".into() }).await;
    alice.expect("request", |m| matches!(m, ServerMsg::FriendRequestReceived { .. })).await;
    alice.send(ClientMsg::FriendAnswer { username: "carol".into(), accept: true }).await;
    carol
        .expect(
            "friends",
            |m| matches!(m, ServerMsg::Friends { friends, .. } if !friends.is_empty()),
        )
        .await;
    let (game, sa, _) = start_match(&mut alice, &mut bob, "bob", "alice", false).await;

    // Not a friend: refused.
    dave.send(ClientMsg::Spectate { username: "alice".into() }).await;
    dave.expect("refused", |m| matches!(m, ServerMsg::Error { .. })).await;

    // A friend watches, from Alice's side; both players see the count.
    carol.send(ClientMsg::Spectate { username: "alice".into() }).await;
    let ServerMsg::MatchStarted { seat, spectating, .. } =
        carol.expect("watching", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
    else {
        unreachable!()
    };
    assert!(spectating);
    assert_eq!(seat, sa);
    for c in [&mut alice, &mut bob] {
        c.expect("count", |m| matches!(m, ServerMsg::Spectators { count: 1 })).await;
    }

    // The spectator sees the shots and can react (quietly).
    let d = ai::decide(&game, Difficulty::Easy, 1);
    let shooter = if game.current_player() == sa { &mut alice } else { &mut bob };
    shooter
        .send(ClientMsg::Shoot {
            turn: 0,
            shot: d.shot,
            called_pocket: d.called_pocket,
            cue_ball: d.cue_ball,
        })
        .await;
    carol.expect("shot", |m| matches!(m, ServerMsg::ShotTaken { .. })).await;
    carol.expect("result", |m| matches!(m, ServerMsg::ShotResult { .. })).await;
    carol.send(ClientMsg::React { code: "clap".into() }).await;
    bob.expect(
        "crowd",
        |m| matches!(m, ServerMsg::Reaction { seat, .. } if *seat == snook_legend_proto::SPECTATOR_SEAT),
    )
    .await;
    // Carol cheers for Bob's seat: both players see the crowd's side.
    let bob_seat = 1 - sa;
    carol.send(ClientMsg::Cheer { seat: bob_seat }).await;
    alice
        .expect("support", |m| {
            matches!(m, ServerMsg::Support { counts } if counts[bob_seat] == 1 && counts[1 - bob_seat] == 0)
        })
        .await;
    // Players' chat stays between them.
    alice.send(ClientMsg::MatchChat { text: "psst".into() }).await;
    bob.expect("chat", |m| matches!(m, ServerMsg::MatchChat { .. })).await;

    // The game ends; when Alice goes back to the lounge, so does Carol.
    alice.send(ClientMsg::Resign).await;
    carol.expect("ended", |m| matches!(m, ServerMsg::MatchEnded { .. })).await;
    alice.send(ClientMsg::LeaveMatch).await;
    let m = carol
        .expect("back", |m| {
            matches!(m, ServerMsg::SpectateEnded { .. } | ServerMsg::MatchChat { .. })
        })
        .await;
    assert!(matches!(m, ServerMsg::SpectateEnded { .. }), "the spectator got the players' chat");
}

#[tokio::test]
async fn players_against_the_computer_show_it_and_can_be_invited() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    alice.expect("welcome", |m| matches!(m, ServerMsg::Welcome { .. })).await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    // Bob starts a game against the computer: Alice sees it in the lounge.
    bob.send(ClientMsg::SetActivity {
        vs_computer: true,
        away: false,
        focus: false,
        pastime: Default::default(),
    })
    .await;
    let is_bob = |m: &ServerMsg, vs: bool| {
        matches!(m, ServerMsg::PlayerUpdated { player }
            if player.username == "bob" && player.vs_computer == vs)
    };
    alice.expect("bob vs the computer", |m| is_bob(m, true)).await;
    // He leaves the game alone for a while: away, then back.
    bob.send(ClientMsg::SetActivity {
        vs_computer: true,
        away: true,
        focus: false,
        pastime: Default::default(),
    })
    .await;
    alice
        .expect("bob away", |m| {
            matches!(m, ServerMsg::PlayerUpdated { player } if player.username == "bob" && player.away)
        })
        .await;
    bob.send(ClientMsg::SetActivity {
        vs_computer: true,
        away: false,
        focus: false,
        pastime: Default::default(),
    })
    .await;
    alice
        .expect("bob back", |m| {
            matches!(m, ServerMsg::PlayerUpdated { player } if player.username == "bob" && !player.away)
        })
        .await;
    // He can still be invited (and the online game takes over).
    let (_, sa, sb) = start_match(&mut alice, &mut bob, "bob", "alice", false).await;
    assert_ne!(sa, sb);
    let ServerMsg::PlayerUpdated { player } = alice
        .expect("bob playing", |m| {
            matches!(m, ServerMsg::PlayerUpdated { player } if player.username == "bob"
                && player.status == snook_legend_proto::PlayerStatus::Playing)
        })
        .await
    else {
        unreachable!()
    };
    assert!(!player.vs_computer);
}

/// Focus mode: a player in a game with it on is shown so in the lounge,
/// invitations to them are turned away with the reason, and the ones
/// waiting when it began are withdrawn. Once the game is over they can be
/// invited again.
#[tokio::test]
async fn players_in_focus_mode_are_not_invited() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (mut carol, _) = Client::register(&url, "carol").await;
    // Carol's invitation waits for Bob.
    carol.send(ClientMsg::Invite { to: "bob".into(), rated: false, rules: Rules::Brazilian }).await;
    bob.expect(
        "carol's invitation",
        |m| matches!(m, ServerMsg::Invited { from, .. } if from == "carol"),
    )
    .await;
    // Bob starts a game against the computer in focus mode.
    bob.send(ClientMsg::SetActivity {
        vs_computer: true,
        away: false,
        focus: true,
        pastime: Default::default(),
    })
    .await;
    alice
        .expect("bob focused", |m| {
            matches!(m, ServerMsg::PlayerUpdated { player } if player.username == "bob" && player.focus)
        })
        .await;
    // Carol's invitation is withdrawn, and she is told why.
    bob.expect(
        "withdrawn",
        |m| matches!(m, ServerMsg::InviteCancelled { from } if from == "carol"),
    )
    .await;
    let ServerMsg::InviteFailed { reason } =
        carol.expect("turned away", |m| matches!(m, ServerMsg::InviteFailed { .. })).await
    else {
        unreachable!()
    };
    assert!(reason.contains("focus mode"), "{reason}");
    // A new one is turned away at once.
    alice.send(ClientMsg::Invite { to: "bob".into(), rated: false, rules: Rules::Brazilian }).await;
    let ServerMsg::InviteFailed { reason } =
        alice.expect("turned away", |m| matches!(m, ServerMsg::InviteFailed { .. })).await
    else {
        unreachable!()
    };
    assert!(reason.contains("focus mode"), "{reason}");
    // The game is over: Bob can be invited again.
    bob.send(ClientMsg::SetActivity {
        vs_computer: false,
        away: false,
        focus: false,
        pastime: Default::default(),
    })
    .await;
    alice
        .expect("bob free", |m| {
            matches!(m, ServerMsg::PlayerUpdated { player } if player.username == "bob" && !player.focus)
        })
        .await;
    alice.send(ClientMsg::Invite { to: "bob".into(), rated: false, rules: Rules::Brazilian }).await;
    bob.expect(
        "alice's invitation",
        |m| matches!(m, ServerMsg::Invited { from, .. } if from == "alice"),
    )
    .await;
}

#[tokio::test]
async fn play_now_pairs_players_of_the_same_mode() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (mut carol, _) = Client::register(&url, "carol").await;
    // Carol wants a casual game: she isn't paired with the rated queue.
    carol.send(ClientMsg::QueueJoin { rated: false, rules: Rules::Brazilian }).await;
    carol.expect("searching", |m| matches!(m, ServerMsg::Queue { searching: true, .. })).await;
    alice.send(ClientMsg::QueueJoin { rated: true, rules: Rules::Brazilian }).await;
    alice.expect("searching", |m| matches!(m, ServerMsg::Queue { searching: true, .. })).await;
    bob.send(ClientMsg::QueueJoin { rated: true, rules: Rules::Brazilian }).await;
    for c in [&mut alice, &mut bob] {
        let ServerMsg::MatchStarted { rated, players, .. } =
            c.expect("paired", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
        else {
            unreachable!()
        };
        assert!(rated);
        let mut names: Vec<_> = players.iter().map(|p| p.username.clone()).collect();
        names.sort();
        assert_eq!(names, ["alice", "bob"]);
    }
    // Carol leaves the queue.
    carol.send(ClientMsg::QueueLeave).await;
    carol.expect("left", |m| matches!(m, ServerMsg::Queue { searching: false, .. })).await;
}

#[tokio::test]
async fn login_errors_and_validation() {
    let url = start(Settings::default()).await;
    let (_alice, token) = Client::register(&url, "Alice").await;

    // The oldest version let in plays; anything older (0.2.0 and its
    // 7-foot table among them) is told to update.
    for (version, welcome) in
        [(snook_legend_proto::OLDEST_PROTOCOL_VERSION, true), (13, false), (10, false)]
    {
        let mut old = Client { ws: Ws::connect(&url).await };
        old.send(ClientMsg::Hello { version, rules: Rules::Brazilian }).await;
        old.send(ClientMsg::Resume { token: token.clone() }).await;
        let m = old
            .expect("answer", |m| {
                matches!(m, ServerMsg::AuthOk { .. } | ServerMsg::AuthError { .. })
            })
            .await;
        assert_eq!(matches!(m, ServerMsg::AuthOk { .. }), welcome, "version {version}: {m:?}");
    }

    let mut c = Client::connect(&url).await;
    c.send(ClientMsg::Register {
        username: "alice".into(),
        password: PASSWORD.into(),
        country: String::new(),
        minor: None,
    })
    .await;
    let ServerMsg::AuthError { reason } =
        c.expect("taken", |m| matches!(m, ServerMsg::AuthError { .. })).await
    else {
        unreachable!()
    };
    assert!(reason.contains("taken"));
    c.send(ClientMsg::Register {
        username: "admin".into(),
        password: PASSWORD.into(),
        country: String::new(),
        minor: None,
    })
    .await;
    c.expect("reserved", |m| matches!(m, ServerMsg::AuthError { .. })).await;
    c.send(ClientMsg::Login { username: "alice".into(), password: "nope!!nope".into() }).await;
    c.expect("bad password", |m| matches!(m, ServerMsg::AuthError { .. })).await;
    // The letter case doesn't matter: "ALICE" is Alice, under her own
    // spelling (one account per name, whatever the case).
    c.send(ClientMsg::Login { username: "ALICE".into(), password: PASSWORD.into() }).await;
    let ServerMsg::AuthOk { player, .. } =
        c.expect("case-insensitive login", |m| matches!(m, ServerMsg::AuthOk { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(player.username, "Alice");

    let mut d = Client::connect(&url).await;
    d.send(ClientMsg::Resume { token }).await;
    d.expect("token", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
    // The older connection was replaced.
    c.expect("kicked", |m| matches!(m, ServerMsg::Kicked { .. })).await;

    // Too old: update the game. Newer than the server: it's the server
    // that is behind (updating the game wouldn't help).
    for (version, says) in [(0, "out of date"), (PROTOCOL_VERSION + 1, "being updated")] {
        let mut other = Client { ws: Ws::connect(&url).await };
        other.send(ClientMsg::Hello { version, rules: Rules::Brazilian }).await;
        let ServerMsg::AuthError { reason } =
            other.expect("version", |m| matches!(m, ServerMsg::AuthError { .. })).await
        else {
            unreachable!()
        };
        assert!(reason.contains(says), "version {version}: {reason}");
    }

    // Password guessing is throttled per account.
    let mut e = Client::connect(&url).await;
    for _ in 0..4 {
        e.send(ClientMsg::Login { username: "alice".into(), password: "wrongpass1".into() }).await;
        e.expect("wrong", |m| matches!(m, ServerMsg::AuthError { .. })).await;
    }
    let mut f = Client::connect(&url).await;
    for _ in 0..4 {
        f.send(ClientMsg::Login { username: "alice".into(), password: "wrongpass1".into() }).await;
        f.expect("wrong", |m| matches!(m, ServerMsg::AuthError { .. })).await;
    }
    let mut g = Client::connect(&url).await;
    g.send(ClientMsg::Login { username: "alice".into(), password: PASSWORD.into() }).await;
    let ServerMsg::AuthError { reason } =
        g.expect("throttled", |m| matches!(m, ServerMsg::AuthError { .. })).await
    else {
        unreachable!()
    };
    assert!(reason.contains("Too many"), "{reason}");
}

#[tokio::test]
async fn disconnect_reconnect_and_abandon() {
    let settings = Settings { reconnect_grace: Duration::from_secs(2), ..Settings::default() };
    let url = start(settings).await;
    let (mut alice, alice_token) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (_, _, _) = start_match(&mut alice, &mut bob, "bob", "alice", true).await;

    // Alice drops and comes back within the grace period.
    drop(alice);
    bob.expect("lost", |m| matches!(m, ServerMsg::OpponentConnection { connected: false, .. }))
        .await;
    let mut alice = Client::connect(&url).await;
    alice.send(ClientMsg::Resume { token: alice_token.clone() }).await;
    alice.expect("rejoined", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    bob.expect("back", |m| matches!(m, ServerMsg::OpponentConnection { connected: true, .. }))
        .await;

    // Now she leaves for good: Bob wins.
    drop(alice);
    let ServerMsg::MatchEnded { result } =
        bob.expect("abandon", |m| matches!(m, ServerMsg::MatchEnded { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(result.reason, snook_legend_core::GameOverReason::Abandoned);
    // Leaving costs more than losing: the winner gains less than she loses.
    let alice_seat = 1 - result.winner;
    let (alice_delta, bob_delta) =
        (result.rating_delta[alice_seat], result.rating_delta[result.winner]);
    assert!(
        alice_delta <= -bob_delta - snook_legend_server::lounge::ABANDON_RATING,
        "{alice_delta} {bob_delta}"
    );
    // Back online, she can't start another game for a while.
    let mut alice = Client::connect(&url).await;
    alice.send(ClientMsg::Resume { token: alice_token }).await;
    alice.expect("signed in", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
    alice.send(ClientMsg::QueueJoin { rated: true, rules: Rules::Brazilian }).await;
    let ServerMsg::Error { message } =
        alice.expect("refused", |m| matches!(m, ServerMsg::Error { .. })).await
    else {
        unreachable!()
    };
    assert!(message.starts_with("You left a game in progress"), "{message}");
    alice.send(ClientMsg::Invite { to: "bob".into(), rated: false, rules: Rules::Brazilian }).await;
    alice.expect("no invitations", |m| matches!(m, ServerMsg::InviteFailed { .. })).await;
}

/// Leaving a rated game costs more than losing it, but, as any loss (and as
/// in doubles), never takes a rating below the floor.
#[tokio::test]
async fn leaving_a_game_keeps_the_rating_floor() {
    let store = snook_legend_server::store::Store::memory();
    let floor = snook_legend_core::rating::EloConfig::default().floor;
    let settings = Settings { reconnect_grace: Duration::from_secs(1), ..Settings::default() };
    let (url, server) = start_on_with(store, settings).await;
    for name in ["alice", "bob"] {
        let hash = server.hash_password(PASSWORD).await.unwrap();
        let user = snook_legend_server::store::NewUser {
            username: name,
            password_hash: &hash,
            country: "BR",
            rating: floor,
            role: snook_legend_proto::Role::Player,
            ip: "127.0.0.1",
            minor: Some(false),
        };
        server.store.create_user(&user).await.unwrap().expect("a new account");
    }
    let mut alice = Client::login(&url, "alice", PASSWORD).await;
    let mut bob = Client::login(&url, "bob", PASSWORD).await;
    let (_, sa, _) = start_match(&mut alice, &mut bob, "bob", "alice", true).await;
    drop(alice);
    let ServerMsg::MatchEnded { result } =
        bob.expect("abandon", |m| matches!(m, ServerMsg::MatchEnded { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(result.reason, snook_legend_core::GameOverReason::Abandoned);
    assert_eq!(result.winner, 1 - sa);
    assert_eq!((result.rating_delta[sa], result.new_ratings[sa]), (0, floor), "{result:?}");
    // As the account has it.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let alice = server.store.user_by_name("alice").await.unwrap().unwrap();
        let record = alice.records.get(Rules::Brazilian);
        if record.games == 1 {
            assert_eq!(record.rating, floor);
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "the game was never recorded");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn the_shot_clock_is_a_foul() {
    let settings = Settings { shot_clock: Duration::from_secs(1), ..Settings::default() };
    let url = start(settings).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (game, _, _) = start_match(&mut alice, &mut bob, "bob", "alice", false).await;
    let first = game.current_player();
    // No shot is played for the player: the clock is a foul. On the break
    // (the Brazilian rules), the break passes, the cue ball on the head spot.
    let m = alice
        .expect("time foul", |m| {
            matches!(m, ServerMsg::TimeFoul { .. } | ServerMsg::ShotTaken { .. })
        })
        .await;
    let ServerMsg::TimeFoul { seat, game, .. } = m else { panic!("a shot was played: {m:?}") };
    assert_eq!(seat, first);
    assert_eq!(game.current_player(), 1 - first);
    assert!(game.is_break());
    assert_eq!(game.ball_in_hand(), None);
    assert!(game.game_over().is_none());
}

#[tokio::test]
async fn private_tables_need_the_password() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    alice
        .send(ClientMsg::OpenTable {
            rated: false,
            rules: Rules::Brazilian,
            password: Some("pool night".into()),
        })
        .await;
    let ServerMsg::TableOpened { table } =
        bob.expect("table", |m| matches!(m, ServerMsg::TableOpened { .. })).await
    else {
        unreachable!()
    };
    assert!(table.private);
    bob.send(ClientMsg::JoinTable { id: table.id, password: Some("wrong".into()) }).await;
    bob.expect("wrong password", |m| matches!(m, ServerMsg::Error { .. })).await;
    bob.send(ClientMsg::JoinTable { id: table.id, password: Some("pool night".into()) }).await;
    bob.expect("started", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    alice.expect("started", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
}

#[tokio::test]
async fn friends_blocks_and_reports() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    alice.send(ClientMsg::FriendRequest { username: "bob".into() }).await;
    bob.expect("request", |m| matches!(m, ServerMsg::FriendRequestReceived { .. })).await;
    bob.send(ClientMsg::FriendAnswer { username: "alice".into(), accept: true }).await;
    alice
        .expect(
            "friend achievement",
            |m| matches!(m, ServerMsg::AchievementUnlocked { code } if code == "friend_1"),
        )
        .await;
    let ServerMsg::Friends { friends, .. } = alice
        .expect(
            "friends",
            |m| matches!(m, ServerMsg::Friends { friends, .. } if !friends.is_empty()),
        )
        .await
    else {
        unreachable!()
    };
    assert_eq!(friends[0].player.username, "bob");
    assert!(friends[0].online);

    // Bob blocks Alice: her invitations are refused and the friendship ends.
    bob.send(ClientMsg::Block { username: "alice".into() }).await;
    bob.expect(
        "blocked",
        |m| matches!(m, ServerMsg::Friends { blocked, .. } if !blocked.is_empty()),
    )
    .await;
    alice.send(ClientMsg::Invite { to: "bob".into(), rated: false, rules: Rules::Brazilian }).await;
    alice.expect("refused", |m| matches!(m, ServerMsg::InviteFailed { .. })).await;
    alice
        .send(ClientMsg::Report {
            username: "bob".into(),
            reason: ReportReason::Spam,
            details: "test".into(),
        })
        .await;
    alice.expect("report", |m| matches!(m, ServerMsg::Notice { .. })).await;
}

#[tokio::test]
async fn tournament_runs_to_a_champion() {
    // The two semi-finals are played one after the other: a long shot clock
    // keeps the waiting one from playing time-out shots meanwhile (slow CI
    // machines take more than 30 s for a game).
    let settings = Settings { shot_clock: Duration::from_secs(600), ..Settings::default() };
    let url = start(settings).await;
    let mut clients = Vec::new();
    for name in ["ana", "ben", "cid", "dan"] {
        clients.push((name, Client::register(&url, name).await.0));
    }
    clients[0]
        .1
        .send(ClientMsg::CreateTournament {
            name: "Friday cup".into(),
            size: 4,
            rated: false,
            rules: Rules::Brazilian,
            password: None,
            entry_fee: 0,
            format: Default::default(),
        })
        .await;
    let ServerMsg::TournamentUpdated { tournament } =
        clients[1].1.expect("created", |m| matches!(m, ServerMsg::TournamentUpdated { .. })).await
    else {
        unreachable!()
    };
    for (_, c) in clients.iter_mut().skip(1) {
        c.send(ClientMsg::JoinTournament { id: tournament.id, password: None }).await;
    }
    // Full: it starts by itself and two semi-finals begin.
    let mut champion = None;
    for _round in 0..2 {
        let mut games = Vec::new();
        for (i, (_, c)) in clients.iter_mut().enumerate() {
            let started = tokio::time::timeout(
                Duration::from_secs(3),
                c.expect("match", |m| matches!(m, ServerMsg::MatchStarted { .. })),
            )
            .await;
            if let Ok(ServerMsg::MatchStarted { game, seat, players, .. }) = started {
                games.push((i, game, seat, players));
            }
        }
        // Pair up the players of each match and play it out.
        while let Some((i, game, seat, players)) = games.pop() {
            let opp = players[1 - seat].username.clone();
            let j_pos = games.iter().position(|(j, ..)| clients[*j].0 == opp).expect("opponent");
            let (j, ..) = games.remove(j_pos);
            let (lo, hi) = (i.min(j), i.max(j));
            let (left, right) = clients.split_at_mut(hi);
            let (a, b) = (&mut left[lo].1, &mut right[0].1);
            let a_seat = if lo == i { seat } else { 1 - seat };
            let result = play_out(a, b, game, a_seat).await;
            a.send(ClientMsg::LeaveMatch).await;
            b.send(ClientMsg::LeaveMatch).await;
            let _ = result;
        }
        let c = &mut clients[0].1;
        let t = c
            .expect("bracket", |m| {
                matches!(m, ServerMsg::TournamentUpdated { tournament }
                    if tournament.state == TournamentState::Finished
                        || tournament.rounds.get(1).is_some_and(|r| r[0].players.iter().all(Option::is_some)))
            })
            .await;
        if let ServerMsg::TournamentUpdated { tournament } = t
            && tournament.state == TournamentState::Finished
        {
            champion = tournament.champion;
            break;
        }
    }
    if champion.is_none() {
        let ServerMsg::TournamentUpdated { tournament } = clients[0]
            .1
            .expect("finished", |m| {
                matches!(m, ServerMsg::TournamentUpdated { tournament } if tournament.state == TournamentState::Finished)
            })
            .await
        else {
            unreachable!()
        };
        champion = tournament.champion;
    }
    let champion = champion.expect("a champion");
    // The cup goes on the champion's trophy shelf (once recorded).
    let c = &mut clients[0].1;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        c.send(ClientMsg::Profile { username: champion.clone() }).await;
        let ServerMsg::Profile { profile } =
            c.expect("profile", |m| matches!(m, ServerMsg::Profile { .. })).await
        else {
            unreachable!()
        };
        if let Some(t) = profile.trophies.first() {
            assert_eq!(t.kind, snook_legend_proto::TrophyKind::Tournament);
            assert_eq!(t.players, 4);
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "no trophy");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn lan_guests_play_casual_games() {
    let settings = Settings { lan: true, ..Settings::default() };
    let url = start(settings).await;
    let mut a = Client::connect(&url).await;
    a.send(ClientMsg::Register {
        username: "x_y".into(),
        password: PASSWORD.into(),
        country: String::new(),
        minor: None,
    })
    .await;
    a.expect("no accounts", |m| matches!(m, ServerMsg::AuthError { .. })).await;
    a.send(ClientMsg::Guest { name: "host".into() }).await;
    let ServerMsg::AuthOk { player, .. } =
        a.expect("guest", |m| matches!(m, ServerMsg::AuthOk { .. })).await
    else {
        unreachable!()
    };
    assert!(player.guest);
    let mut b = Client::connect(&url).await;
    b.send(ClientMsg::Guest { name: "HOST".into() }).await;
    b.expect("name taken", |m| matches!(m, ServerMsg::AuthError { .. })).await;
    b.send(ClientMsg::Guest { name: "visitor".into() }).await;
    b.expect("guest", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
    a.send(ClientMsg::Invite { to: "visitor".into(), rated: true, rules: Rules::Brazilian }).await;
    let ServerMsg::Invited { rated, .. } =
        b.expect("invite", |m| matches!(m, ServerMsg::Invited { .. })).await
    else {
        unreachable!()
    };
    assert!(!rated, "LAN games are never rated");
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
    // A refused body may not be read: the answer comes before it and the
    // connection may be reset after.
    let mut out = Vec::new();
    let _ = s.read_to_end(&mut out).await;
    let out = String::from_utf8_lossy(&out).to_string();
    let code = out[9..12].parse().unwrap();
    let body = out.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (code, body)
}

/// An administrator's name belongs to the account that has it: it can't be
/// registered (the account comes first), and its owner can't rename it
/// away for someone else to take. Reserved words are refused dressed up too.
#[tokio::test]
async fn administrators_names_stay_theirs() {
    let (url, http) = start_with(Settings::default(), vec!["boss".into(), "chief".into()]).await;
    let host = http.trim_start_matches("http://").to_string();
    let taken = "That username is taken.";
    for name in ["boss", "BOSS", "Admin_1", "moderador", "_suporte2", "staff"] {
        let mut c = Client::connect(&url).await;
        c.send(ClientMsg::Register {
            username: name.into(),
            password: PASSWORD.into(),
            country: "BR".into(),
            minor: None,
        })
        .await;
        let ServerMsg::AuthError { reason } =
            c.expect("refused", |m| matches!(m, ServerMsg::AuthError { .. })).await
        else {
            unreachable!()
        };
        assert_eq!(reason, taken, "{name}");
    }
    // The listed account exists, and signing in makes it an administrator.
    let mut boss = Client::login(&url, "boss", PASSWORD).await;
    boss.send(ClientMsg::Account).await;
    boss.expect("account", |m| matches!(m, ServerMsg::Account { .. })).await;
    let staff = "Staff accounts can't change their username.";
    boss.rename_refused("boss_pool", staff).await;
    // A moderator made in the panel keeps their name too, and is signed
    // out at once (their connection had the powers of the old role).
    let (mut eve, _) = Client::register(&url, "eve").await;
    let token = common::staff_token(&host, "boss", PASSWORD).await;
    let (code, body) =
        request(&host, "POST", "/admin/api/users/eve/role", &token, r#"{"role":"moderator"}"#)
            .await;
    assert_eq!(code, 200, "{body}");
    eve.expect("signed out", |m| matches!(m, ServerMsg::Kicked { .. })).await;
    let mut eve = Client::login(&url, "eve", PASSWORD).await;
    eve.rename_refused("eve_2", staff).await;
}

/// Names no account can have and passwords longer than any can be are
/// turned away at once; a wrong current password when changing it or
/// deleting the account counts like a wrong one when signing in.
#[tokio::test]
async fn password_guesses_are_limited_everywhere() {
    let (url, http) = start_with(Settings::default(), Vec::new()).await;
    let host = http.trim_start_matches("http://").to_string();
    let wrong = "Wrong username or password.";
    let mut c = Client::connect(&url).await;
    for (username, password) in
        [("x".repeat(5000), PASSWORD.to_string()), ("carol".into(), "p".repeat(73))]
    {
        c.send(ClientMsg::Login { username, password }).await;
        let ServerMsg::AuthError { reason } =
            c.expect("refused", |m| matches!(m, ServerMsg::AuthError { .. })).await
        else {
            unreachable!()
        };
        assert_eq!(reason, wrong);
    }
    // The panel's sign-in too, and it reads no large bodies.
    let long = format!(r#"{{"username":"{}","password":"x"}}"#, "y".repeat(100));
    let (code, _) = request(&host, "POST", "/admin/api/login", "", &long).await;
    assert_eq!(code, 401);
    let huge = format!(r#"{{"username":"carol","password":"{}"}}"#, "z".repeat(20_000));
    let (code, _) = request(&host, "POST", "/admin/api/login", "", &huge).await;
    assert_eq!(code, 413);

    let (mut carol, _) = Client::register(&url, "carol").await;
    let wrong_now = "The current password is wrong.";
    for _ in 0..8 {
        carol
            .send(ClientMsg::ChangePassword { old: "not-it-1".into(), new: "another1".into() })
            .await;
        let ServerMsg::Error { message } =
            carol.expect("wrong", |m| matches!(m, ServerMsg::Error { .. })).await
        else {
            unreachable!()
        };
        assert_eq!(message, wrong_now);
    }
    let too_many = "Too many failed attempts. Wait a few minutes and try again.";
    carol.send(ClientMsg::DeleteAccount { password: PASSWORD.into() }).await;
    let ServerMsg::Error { message } =
        carol.expect("limited", |m| matches!(m, ServerMsg::Error { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(message, too_many);
    let mut again = Client::connect(&url).await;
    again.send(ClientMsg::Login { username: "carol".into(), password: PASSWORD.into() }).await;
    let ServerMsg::AuthError { reason } =
        again.expect("limited", |m| matches!(m, ServerMsg::AuthError { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(reason, too_many);
}

/// A saved session that signs in over and over (a new connection each
/// time) is let in a dozen times a minute; then the connection closes
/// without an answer, which a game would take as the end of its session.
#[tokio::test]
async fn signing_in_again_and_again_is_limited() {
    let url = start(Settings::default()).await;
    let (_first, token) = Client::register(&url, "dora").await;
    let mut refused = false;
    for _ in 0..20 {
        let mut c = Client::connect(&url).await;
        c.send(ClientMsg::Resume { token: token.clone() }).await;
        let answer = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match c.ws.next().await {
                    Some(Ok(Message::Text(t))) => {
                        let m = parse_server(t.as_str()).unwrap();
                        if matches!(m, ServerMsg::AuthOk { .. } | ServerMsg::AuthError { .. }) {
                            return Some(m);
                        }
                    }
                    Some(Ok(_)) => {}
                    None | Some(Err(_)) => return None,
                }
            }
        })
        .await
        .expect("an answer or the end");
        match answer {
            Some(ServerMsg::AuthOk { .. }) => assert!(!refused, "let in after being refused"),
            Some(m) => panic!("told {m:?}: the game would forget its session"),
            None => refused = true,
        }
    }
    assert!(refused, "never limited");
}

#[tokio::test]
async fn admin_panel_requires_staff() {
    let (url, http) = start_with(Settings::default(), vec!["boss".into()]).await;
    let mut boss = Client::login(&url, "boss", PASSWORD).await;
    let (_eve, _) = Client::register(&url, "eve").await;
    let host = http.trim_start_matches("http://").to_string();
    let (code, _) = request(&host, "GET", "/admin/api/overview", "", "").await;
    assert_eq!(code, 401);
    let (code, _) = request(
        &host,
        "POST",
        "/admin/api/login",
        "",
        &format!(r#"{{"username":"eve","password":"{PASSWORD}"}}"#),
    )
    .await;
    assert_eq!(code, 403, "players can't sign in to the panel");
    let token = common::staff_token(&host, "boss", PASSWORD).await;
    let (code, _) = request(&host, "GET", "/admin/api/overview", &token, "").await;
    assert_eq!(code, 200);
    let (code, _) = request(
        &host,
        "POST",
        "/admin/api/users/eve/ban",
        &token,
        r#"{"hours":2,"reason":"spam"}"#,
    )
    .await;
    assert_eq!(code, 200);
    // An administrator may change their own wallet, not ban themselves.
    let (code, body) = request(
        &host,
        "POST",
        "/admin/api/users/boss/coins",
        &token,
        r#"{"coins":500,"reason":"test"}"#,
    )
    .await;
    assert_eq!(code, 200, "{body}");
    let (code, body) =
        request(&host, "POST", "/admin/api/users/boss/vip", &token, r#"{"days":30,"reason":"t"}"#)
            .await;
    assert_eq!(code, 200, "{body}");
    let (code, _) =
        request(&host, "POST", "/admin/api/users/boss/ban", &token, r#"{"reason":"x"}"#).await;
    assert_eq!(code, 400, "nobody bans themselves");
    // A note on the board reaches everyone in the lounge at once.
    let (code, body) = request(
        &host,
        "POST",
        "/admin/api/board",
        &token,
        r#"{"text":"Cup on Saturday","days":3}"#,
    )
    .await;
    assert_eq!(code, 200, "{body}");
    let ServerMsg::Board { notes } = boss
        .expect("board", |m| matches!(m, ServerMsg::Board { notes } if !notes.is_empty()))
        .await
    else {
        unreachable!()
    };
    assert_eq!((notes[0].text.as_str(), notes[0].author.as_str()), ("Cup on Saturday", "boss"));
    let (code, _) =
        request(&host, "DELETE", &format!("/admin/api/board/{}", notes[0].id), &token, "").await;
    assert_eq!(code, 200);
    boss.expect("empty board", |m| matches!(m, ServerMsg::Board { notes } if notes.is_empty()))
        .await;
    // Eve can't sign in while banned.
    let mut c = Client::connect(&url).await;
    c.send(ClientMsg::Login { username: "eve".into(), password: PASSWORD.into() }).await;
    let ServerMsg::AuthError { reason } =
        c.expect("banned", |m| matches!(m, ServerMsg::AuthError { .. })).await
    else {
        unreachable!()
    };
    assert!(reason.contains("banned"), "{reason}");
}

/// Signing in again from another place ends what the old connection had
/// going in the lounge: its open table must not stay behind as a ghost.
#[tokio::test]
async fn a_replaced_session_leaves_no_table_or_queue_behind() {
    let url = start(Settings::default()).await;
    let (mut alice, token) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    alice
        .send(ClientMsg::OpenTable { rated: false, rules: Rules::Brazilian, password: None })
        .await;
    let ServerMsg::TableOpened { table } =
        bob.expect("table", |m| matches!(m, ServerMsg::TableOpened { .. })).await
    else {
        unreachable!()
    };
    // Alice opens the game on another computer.
    let mut alice2 = Client::connect(&url).await;
    alice2.send(ClientMsg::Resume { token }).await;
    alice2.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
    bob.expect("closed", |m| matches!(m, ServerMsg::TableClosed { id } if *id == table.id)).await;
    let ServerMsg::Tables { tables } =
        alice2.expect("tables", |m| matches!(m, ServerMsg::Tables { .. })).await
    else {
        unreachable!()
    };
    assert!(tables.is_empty(), "the old table is gone");
    bob.send(ClientMsg::JoinTable { id: table.id, password: None }).await;
    bob.expect("error", |m| matches!(m, ServerMsg::Error { .. })).await;
}

/// An account is live in one place at a time: signing in again (by
/// password or by a saved session, on another device or in another window)
/// closes the connection that was there, which can do nothing more.
#[tokio::test]
async fn an_account_is_used_in_one_place_at_a_time() {
    let url = start(Settings::default()).await;
    let (mut first, token) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    for by_password in [true, false] {
        let mut second = Client::connect(&url).await;
        if by_password {
            second
                .send(ClientMsg::Login { username: "alice".into(), password: PASSWORD.into() })
                .await;
        } else {
            second.send(ClientMsg::Resume { token: token.clone() }).await;
        }
        second.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
        // The lounge the new connection is shown has alice once.
        let ServerMsg::Lounge { players } =
            second.expect("lounge", |m| matches!(m, ServerMsg::Lounge { .. })).await
        else {
            unreachable!()
        };
        assert_eq!(players.iter().filter(|p| p.username == "alice").count(), 1);
        // The connection that was there is told and closed by the server.
        first.expect("kicked", |m| matches!(m, ServerMsg::Kicked { .. })).await;
        let mut closed = false;
        for _ in 0..50 {
            match tokio::time::timeout(Duration::from_secs(5), first.ws.next()).await {
                Ok(None) | Ok(Some(Err(_))) | Ok(Some(Ok(Message::Close(_)))) => {
                    closed = true;
                    break;
                }
                Ok(Some(Ok(_))) => {}
                Err(_) => break,
            }
        }
        assert!(closed, "the replaced connection is closed");
        // Whatever it still sends reaches nobody.
        let _ = first
            .ws
            .send(Message::Text(
                ClientMsg::Chat { text: "from the old place".into() }.to_json().into(),
            ))
            .await;
        second.send(ClientMsg::Chat { text: "from the new place".into() }).await;
        let ServerMsg::Chat { line } =
            bob.expect("chat", |m| matches!(m, ServerMsg::Chat { .. })).await
        else {
            unreachable!()
        };
        assert_eq!(line.text, "from the new place");
        first = second;
        // Chat is rate limited and repeated lines are refused: a pause.
        tokio::time::sleep(Duration::from_millis(1200)).await;
        first.send(ClientMsg::Chat { text: format!("still here {by_password}") }).await;
        bob.expect(
            "chat",
            |m| matches!(m, ServerMsg::Chat { line } if line.text.starts_with("still here")),
        )
        .await;
    }
}

/// Lounge actions that everyone hears about can't be used to flood the lounge.
#[tokio::test]
async fn lounge_actions_are_rate_limited() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    for _ in 0..40 {
        alice
            .send(ClientMsg::OpenTable { rated: false, rules: Rules::Brazilian, password: None })
            .await;
    }
    alice.expect("limit", |m| matches!(m, ServerMsg::Error { .. })).await;
    alice.send(ClientMsg::Chat { text: "done".into() }).await;
    let mut opened = 0;
    while let ServerMsg::TableOpened { .. } = bob
        .expect("tables", |m| matches!(m, ServerMsg::TableOpened { .. } | ServerMsg::Chat { .. }))
        .await
    {
        opened += 1;
    }
    assert!((1..=14).contains(&opened), "{opened} tables announced");
    // Leaving a tournament one never joined says nothing to anyone.
    bob.send(ClientMsg::CreateTournament {
        name: "Friday".into(),
        size: 4,
        rated: false,
        rules: Rules::Brazilian,
        password: None,
        entry_fee: 0,
        format: Default::default(),
    })
    .await;
    let ServerMsg::TournamentUpdated { tournament } =
        bob.expect("tournament", |m| matches!(m, ServerMsg::TournamentUpdated { .. })).await
    else {
        unreachable!()
    };
    let (mut carol, _) = Client::register(&url, "carol").await;
    carol.send(ClientMsg::LeaveTournament { id: tournament.id }).await;
    carol.send(ClientMsg::Chat { text: "hello".into() }).await;
    let next = bob
        .expect("next", |m| {
            matches!(m, ServerMsg::TournamentUpdated { .. } | ServerMsg::Chat { .. })
        })
        .await;
    assert!(matches!(next, ServerMsg::Chat { .. }), "{next:?}");
}

/// A server on its own port over `store`; returns its address and handle.
async fn start_on(store: snook_legend_server::store::Store) -> (String, std::sync::Arc<Server>) {
    start_on_with(store, Settings::default()).await
}

/// [`start_on`] with `settings`.
async fn start_on_with(
    store: snook_legend_server::store::Store,
    settings: Settings,
) -> (String, std::sync::Arc<Server>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let settings = Settings { max_connections_per_ip: 100, ..settings };
    let config = Config { bind: addr, settings, ..Config::default() };
    let server = Server::with_store(config, store).await;
    tokio::spawn(serve(listener, server.clone()));
    (format!("ws://{addr}/ws"), server)
}

impl Client {
    async fn login(url: &str, name: &str, password: &str) -> Client {
        let mut c = Client::connect(url).await;
        c.send(ClientMsg::Login { username: name.into(), password: password.into() }).await;
        c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
        c
    }

    /// Waits until the server closes the connection.
    async fn closed(&mut self) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(left, self.ws.next()).await {
                Err(_) => panic!("the connection stayed open"),
                Ok(None) | Ok(Some(Err(_))) | Ok(Some(Ok(Message::Close(_)))) => return,
                Ok(Some(Ok(_))) => {}
            }
        }
    }
}

/// Plays `n` shots of a game (AI on both sides); returns the game after them.
async fn play_shots(
    a: &mut Client,
    b: &mut Client,
    mut game: EightBallGame,
    sa: usize,
    n: usize,
) -> EightBallGame {
    for k in 0..n {
        if game.game_over().is_some() {
            break;
        }
        let d = ai::decide(&game, Difficulty::Hard, k as u64);
        let turn = game.shots_taken();
        let msg = ClientMsg::Shoot {
            turn,
            shot: d.shot,
            called_pocket: d.called_pocket,
            cue_ball: d.cue_ball,
        };
        if game.current_player() == sa {
            a.send(msg).await
        } else {
            b.send(msg).await
        }
        let ServerMsg::ShotResult { end, .. } =
            a.expect("result", |m| matches!(m, ServerMsg::ShotResult { .. })).await
        else {
            unreachable!()
        };
        b.expect("result b", |m| matches!(m, ServerMsg::ShotResult { .. })).await;
        game = end;
    }
    game
}

#[tokio::test]
async fn a_restart_hands_the_game_to_the_next_server() {
    let store = snook_legend_server::store::Store::memory();
    let (url_a, server_a) = start_on(store.share().unwrap()).await;
    let (mut alice, _) = Client::register(&url_a, "alice").await;
    let (mut bob, _) = Client::register(&url_a, "bob").await;
    let (game, sa, _) = start_match(&mut alice, &mut bob, "bob", "alice", true).await;
    let game = play_shots(&mut alice, &mut bob, game, sa, 3).await;
    assert!(game.game_over().is_none());

    // The first server is told to stop: no new games, a notice, and after
    // the drain time the game is handed over (the connections close).
    let drain = tokio::spawn(snook_legend_server::drain(server_a.clone(), 1));
    alice.expect("draining", |m| matches!(m, ServerMsg::Draining { .. })).await;
    alice.closed().await;
    bob.closed().await;
    drain.await.unwrap();

    // The next server rebuilds the table when the players come back.
    let (url_b, _server_b) = start_on(store).await;
    let mut alice = Client::login(&url_b, "alice", PASSWORD).await;
    let ServerMsg::MatchStarted { game: rebuilt, seat, rated, .. } =
        alice.expect("game back", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
    else {
        unreachable!()
    };
    assert_eq!((seat, rated), (sa, true));
    assert_eq!(rebuilt, game, "the same table, ball for ball");
    let mut bob = Client::login(&url_b, "bob", PASSWORD).await;
    bob.expect("game back", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    // And the game goes on to the end.
    let result = play_out(&mut alice, &mut bob, rebuilt, sa).await;
    assert!(result.rated);
}

#[tokio::test]
async fn account_password_recovery_email_and_deletion() {
    let url = start(Settings::default()).await;
    let (mut carol, _) = Client::register(&url, "carol").await;
    carol.send(ClientMsg::ChangePassword { old: "wrong-one".into(), new: "another1".into() }).await;
    carol.expect("wrong", |m| matches!(m, ServerMsg::Error { .. })).await;
    carol.send(ClientMsg::ChangePassword { old: PASSWORD.into(), new: "another1".into() }).await;
    carol.expect("changed", |m| matches!(m, ServerMsg::Notice { .. })).await;
    let mut again = Client::login(&url, "carol", "another1").await;
    // The password is needed: a session alone can't set where the account
    // is recovered from.
    again
        .send(ClientMsg::SetRecoveryEmail {
            email: "thief@example.com".into(),
            password: "guessing".into(),
            lang: String::new(),
        })
        .await;
    assert_eq!(again.error_text().await, "The current password is wrong.");
    again
        .send(ClientMsg::SetRecoveryEmail {
            email: "not an email".into(),
            password: "another1".into(),
            lang: String::new(),
        })
        .await;
    again.expect("bad e-mail", |m| matches!(m, ServerMsg::Error { .. })).await;
    again
        .send(ClientMsg::SetRecoveryEmail {
            email: " Carol@Example.com ".into(),
            password: "another1".into(),
            lang: String::new(),
        })
        .await;
    let ServerMsg::Account { recovery_email, .. } =
        again.expect("account", |m| matches!(m, ServerMsg::Account { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(recovery_email.as_deref(), Some("carol@example.com"));

    let (mut dave, _) = Client::register(&url, "dave").await;
    again.send(ClientMsg::DeleteAccount { password: "another1".into() }).await;
    again.expect("deleted", |m| matches!(m, ServerMsg::AccountDeleted)).await;
    again.closed().await;
    // The name is free of the old account: it can't sign in, and its profile is gone.
    let mut c = Client::connect(&url).await;
    c.send(ClientMsg::Login { username: "carol".into(), password: "another1".into() }).await;
    c.expect("refused", |m| matches!(m, ServerMsg::AuthError { .. })).await;
    dave.send(ClientMsg::Profile { username: "carol".into() }).await;
    dave.expect("gone", |m| matches!(m, ServerMsg::Error { .. })).await;
}

/// The e-mail of the account's own payment (offered after it) is set
/// without the password: it is the owner's already. Any other needs it.
#[tokio::test]
async fn the_payment_email_needs_the_password_too() {
    use snook_legend_server::store::{Payment, Store};
    let (url, server) = start_on(Store::memory()).await;
    let (mut erin, _) = Client::register(&url, "erin").await;
    let id = server.store.user_by_name("erin").await.unwrap().unwrap().id;
    let payment = Payment {
        id: "pi_erin".into(),
        user_id: id,
        kind: "coins".into(),
        coins: 100,
        amount: 199,
        currency: "usd".into(),
        at: 1,
        refunded: false,
        email: Some("Erin@Example.com".into()),
        test: false,
    };
    assert!(server.store.record_payment(&payment).await.unwrap());
    // Someone at an unlocked computer can't point the account's recovery
    // at an address they read on the screen: the password is asked always.
    for email in ["someone@example.com", " erin@example.com", ""] {
        erin.send(ClientMsg::SetRecoveryEmail {
            email: email.into(),
            password: String::new(),
            lang: String::new(),
        })
        .await;
        assert_eq!(erin.error_text().await, "The current password is wrong.", "{email:?}");
    }
    erin.send(ClientMsg::SetRecoveryEmail {
        email: " erin@example.com".into(),
        password: PASSWORD.into(),
        lang: String::new(),
    })
    .await;
    let ServerMsg::Account { recovery_email, .. } =
        erin.expect("account", |m| matches!(m, ServerMsg::Account { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(recovery_email.as_deref(), Some("erin@example.com"));
}

impl Client {
    /// Asks for a new username and waits for the refusal `why`.
    async fn rename_refused(&mut self, name: &str, why: &str) {
        self.send(ClientMsg::ChangeUsername { username: name.into() }).await;
        let ServerMsg::Error { message } =
            self.expect(why, |m| matches!(m, ServerMsg::Error { .. })).await
        else {
            unreachable!()
        };
        assert_eq!(message, why, "renaming to {name:?}");
    }

    /// Changes the username; returns the changes left.
    async fn rename(&mut self, name: &str) -> u32 {
        self.send(ClientMsg::ChangeUsername { username: name.into() }).await;
        let ServerMsg::Account { name_changes_left, .. } =
            self.expect("account", |m| matches!(m, ServerMsg::Account { .. })).await
        else {
            unreachable!()
        };
        // Signed in again under the new name, on the same connection.
        let ServerMsg::AuthOk { player, .. } =
            self.expect("signed in again", |m| matches!(m, ServerMsg::AuthOk { .. })).await
        else {
            unreachable!()
        };
        assert_eq!(player.username, name);
        name_changes_left
    }
}

#[tokio::test]
async fn username_changes() {
    let url = start(Settings::default()).await;
    let (mut erin, token) = Client::register(&url, "erin").await;
    let (mut frank, _) = Client::register(&url, "frank").await;
    erin.send(ClientMsg::Account).await;
    let ServerMsg::Account { name_changes_left, .. } =
        erin.expect("account", |m| matches!(m, ServerMsg::Account { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(name_changes_left, snook_legend_proto::MAX_NAME_CHANGES);

    // The rules of a new account, and nobody else's name in any case.
    let taken = "That username is taken.";
    let invalid = "Usernames have 3–16 letters, digits or underscores.";
    erin.rename_refused("frank", taken).await;
    erin.rename_refused("FRANK", taken).await;
    erin.rename_refused("Admin", taken).await;
    erin.rename_refused("ab", invalid).await;
    erin.rename_refused("with space", invalid).await;
    erin.rename_refused("deleted-1", invalid).await;
    erin.rename_refused("erin", "That is already your username.").await;

    // A change: everyone in the lounge sees the old name go and the new come.
    assert_eq!(erin.rename("Erin_2").await, 2);
    frank
        .expect(
            "old name gone",
            |m| matches!(m, ServerMsg::PlayerLeft { username } if username == "erin"),
        )
        .await;
    frank
        .expect(
            "new name",
            |m| matches!(m, ServerMsg::PlayerUpdated { player } if player.username == "Erin_2"),
        )
        .await;
    // The lounge knows the new name: an invitation reaches it, and nobody
    // can change their name in a game (or straight after, at the table).
    let (_, sa, _) = start_match(&mut frank, &mut erin, "erin_2", "frank", false).await;
    let _ = sa;
    let in_game = "Finish your game before changing your username.";
    erin.rename_refused("Erin_3", in_game).await;
    erin.send(ClientMsg::Resign).await;
    erin.expect("over", |m| matches!(m, ServerMsg::MatchEnded { .. })).await;
    erin.rename_refused("Erin_3", in_game).await;
    erin.send(ClientMsg::LeaveMatch).await;
    // Nor in the queue.
    erin.send(ClientMsg::QueueJoin { rated: false, rules: Rules::Brazilian }).await;
    let queued = "Leave your table or the queue before changing your username.";
    erin.rename_refused("Erin_3", queued).await;
    erin.send(ClientMsg::QueueLeave).await;

    // Only the case of the letters: still a change.
    assert_eq!(erin.rename("ERIN_2").await, 1);
    assert_eq!(erin.rename("Erin_Last").await, 0);
    // The free ones used, another costs coins (see `settings::NAME_CHANGE_PRICE`).
    erin.rename_refused("Erin_More", "Not enough coins.").await;

    // The session stays good, the old names are free and sign in to nothing.
    let mut c = Client::connect(&url).await;
    c.send(ClientMsg::Resume { token }).await;
    let ServerMsg::AuthOk { player, .. } =
        c.expect("resumed", |m| matches!(m, ServerMsg::AuthOk { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(player.username, "Erin_Last");
    let mut c = Client::connect(&url).await;
    c.send(ClientMsg::Login { username: "erin".into(), password: PASSWORD.into() }).await;
    c.expect("old name", |m| matches!(m, ServerMsg::AuthError { .. })).await;
    Client::login(&url, "erin_last", PASSWORD).await;
    let (_erin_again, _) = Client::register(&url, "erin").await;
}

#[tokio::test]
async fn replays_coins_featured_table_and_codes() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    alice.expect("daily bonus", |m| matches!(m, ServerMsg::CoinsEarned { .. })).await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (mut eve, _) = Client::register(&url, "eve").await;

    // An invitation code: "snook-legend://join/abc123" works as well as "ABC123".
    alice.send(ClientMsg::CreateInviteCode { rated: true, rules: Rules::Brazilian }).await;
    let ServerMsg::InviteCode { code } =
        alice.expect("code", |m| matches!(m, ServerMsg::InviteCode { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(code.len(), 6);
    bob.send(ClientMsg::JoinByCode {
        code: format!("snook-legend://join/{}", code.to_lowercase()),
    })
    .await;
    let ServerMsg::MatchStarted { game, seat: sa, match_id, table_look, .. } =
        alice.expect("match", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(table_look[0], "cloth_green", "the host's table");
    bob.expect("match b", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;

    // Anyone can watch the featured game.
    let ServerMsg::Featured { featured } =
        eve.expect("featured", |m| matches!(m, ServerMsg::Featured { featured: Some(_) })).await
    else {
        unreachable!()
    };
    assert_eq!(featured.unwrap().match_id, match_id);
    eve.send(ClientMsg::WatchMatch { match_id }).await;
    let ServerMsg::MatchStarted { spectating, .. } =
        eve.expect("watching", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
    else {
        unreachable!()
    };
    assert!(spectating);

    let result = play_out(&mut alice, &mut bob, game, sa).await;
    // Coins for a rated game played to the end (a level reached on the
    // way pays its own, told apart by the reason).
    let ServerMsg::CoinsEarned { amount, .. } = alice
        .expect("coins", |m| {
            matches!(m, ServerMsg::CoinsEarned { reason, .. } if reason == "win" || reason == "game")
        })
        .await
    else {
        unreachable!()
    };
    let alice_won = result.winner == sa;
    use snook_legend_server::account::{COINS_PER_GAME, COINS_PER_WIN};
    let expected = COINS_PER_GAME + if alice_won { COINS_PER_WIN } else { 0 };
    assert_eq!(amount as i64, expected);
    // The game moved the missions on: one game played, one won (or not).
    let ServerMsg::Missions { daily, weekly, day_ends, week_ends } =
        alice.expect("missions", |m| matches!(m, ServerMsg::Missions { .. })).await
    else {
        unreachable!()
    };
    assert_eq!((daily.len(), weekly.len()), (3, 2));
    assert!(day_ends > snook_legend_server::now() && week_ends >= day_ends);
    for m in daily.iter().chain(&weekly) {
        let goal = snook_legend_proto::missions::mission(&m.code).unwrap().goal;
        use snook_legend_proto::missions::Goal;
        match goal {
            Goal::Play => assert_eq!(m.progress, 1, "{}", m.code),
            Goal::Win => assert_eq!(m.progress, alice_won as u32, "{}", m.code),
            _ => {}
        }
    }

    // The game can be watched again, stroke by stroke.
    alice.send(ClientMsg::Replays).await;
    let ServerMsg::Replays { games, limit } =
        alice.expect("replays", |m| matches!(m, ServerMsg::Replays { .. })).await
    else {
        unreachable!()
    };
    assert_eq!((games.len(), limit), (1, 10));
    alice.send(ClientMsg::Replay { match_id: games[0].match_id }).await;
    let ServerMsg::Replay { replay } =
        alice.expect("replay", |m| matches!(m, ServerMsg::Replay { .. })).await
    else {
        unreachable!()
    };
    let (end, _) = replay.replay.play(None).unwrap();
    assert_eq!(end.game_over().map(|o| o.winner), Some(result.winner));
    // Only the players (and staff) may.
    eve.send(ClientMsg::Replay { match_id: games[0].match_id }).await;
    eve.expect("not hers", |m| matches!(m, ServerMsg::Error { .. })).await;

    // The profile has the rating chart and the career numbers.
    eve.send(ClientMsg::Profile { username: "alice".into() }).await;
    let ServerMsg::Profile { profile } =
        eve.expect("profile", |m| matches!(m, ServerMsg::Profile { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(profile.rating_history.brazilian.len(), 1);
    assert!(profile.rating_history.official.is_empty());
    assert_eq!(profile.career.games, 1);
    assert!(profile.career.shots > 0);
}

#[tokio::test]
async fn arenas_with_an_entry_fee() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    // The daily bonus (5 coins) pays the entry.
    for c in [&mut alice, &mut bob] {
        c.expect("daily bonus", |m| matches!(m, ServerMsg::CoinsEarned { .. })).await;
    }
    alice
        .send(ClientMsg::CreateTournament {
            name: "Blitz".into(),
            size: 4,
            rated: false,
            rules: Rules::Brazilian,
            password: None,
            entry_fee: 1000,
            format: snook_legend_proto::TournamentFormat::Arena,
        })
        .await;
    alice.expect("too dear", |m| matches!(m, ServerMsg::Error { .. })).await;
    alice
        .send(ClientMsg::CreateTournament {
            name: "Blitz".into(),
            size: 4,
            rated: false,
            rules: Rules::Brazilian,
            password: None,
            entry_fee: 5,
            format: snook_legend_proto::TournamentFormat::Arena,
        })
        .await;
    let ServerMsg::TournamentUpdated { tournament } =
        bob.expect("created", |m| matches!(m, ServerMsg::TournamentUpdated { .. })).await
    else {
        unreachable!()
    };
    assert_eq!((tournament.entry_fee, tournament.prizes), (5, [3, 1]));
    bob.send(ClientMsg::JoinTournament { id: tournament.id, password: None }).await;
    let ServerMsg::TournamentUpdated { tournament } = alice
        .expect("joined", |m| {
            matches!(m, ServerMsg::TournamentUpdated { tournament } if tournament.players.len() == 2)
        })
        .await
    else {
        unreachable!()
    };
    assert_eq!(tournament.prizes, [7, 2], "the pot grows");
    alice.send(ClientMsg::StartTournament { id: tournament.id }).await;
    alice.expect("paired", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    bob.expect("paired", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    // Resigning at once scores nothing: two friends can't trade points.
    alice.send(ClientMsg::Resign).await;
    alice.expect("over", |m| matches!(m, ServerMsg::MatchEnded { .. })).await;
    alice.send(ClientMsg::Tournaments).await;
    let ServerMsg::Tournaments { tournaments } =
        alice.expect("list", |m| matches!(m, ServerMsg::Tournaments { .. })).await
    else {
        unreachable!()
    };
    assert!(tournaments[0].standings.iter().all(|s| s.points == 0 && s.games == 0));
    // A game played (a few shots, then a resignation) does; the same two
    // meet again after a wait.
    for c in [&mut alice, &mut bob] {
        c.send(ClientMsg::LeaveMatch).await;
    }
    let ServerMsg::MatchStarted { game, seat: sa, .. } =
        alice.expect("paired again", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
    else {
        unreachable!()
    };
    bob.expect("paired again", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    let game = play_shots(&mut alice, &mut bob, game, sa, 8).await;
    // (Should someone have won on the 8 already, that game counts the same.)
    let winner = match game.game_over() {
        Some(over) => {
            if over.winner == sa {
                "alice"
            } else {
                "bob"
            }
        }
        None => {
            alice.send(ClientMsg::Resign).await;
            "bob"
        }
    };
    let ServerMsg::TournamentUpdated { tournament } = alice
        .expect("standings", |m| {
            matches!(m, ServerMsg::TournamentUpdated { tournament } if tournament.standings.iter().any(|s| s.points > 0))
        })
        .await
    else {
        unreachable!()
    };
    assert_eq!(tournament.standings[0].username, winner);
    assert_eq!((tournament.standings[0].points, tournament.standings[0].streak), (2, 1));
    assert!(tournament.ends_at.is_some());
}

impl Client {
    /// Every message until the answer to a ping (what the server sent so far).
    async fn drain(&mut self) -> Vec<ServerMsg> {
        self.send(ClientMsg::Ping).await;
        let mut seen = Vec::new();
        loop {
            match self.expect("anything", |_| true).await {
                ServerMsg::Pong => return seen,
                m => seen.push(m),
            }
        }
    }

    /// The next error message.
    async fn error_text(&mut self) -> String {
        let ServerMsg::Error { message } =
            self.expect("error", |m| matches!(m, ServerMsg::Error { .. })).await
        else {
            unreachable!()
        };
        message
    }
}

/// A player's coins, as stored.
async fn coins_of(server: &Server, name: &str) -> i64 {
    let user = server.store.user_by_name(name).await.unwrap().unwrap();
    server.store.wallet(user.id).await.unwrap().coins
}

/// Waits until `name` has `coins` (refunds arrive through the server's
/// worker).
async fn wait_for_coins(server: &Server, name: &str, coins: i64) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let now = coins_of(server, name).await;
        if now == coins {
            return;
        }
        assert!(tokio::time::Instant::now() < deadline, "{name} has {now} coins, not {coins}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Creates a 4-player knockout as `host` and returns its id (`fee`: its
/// entry fee, `password`: private).
async fn new_knockout(host: &mut Client, fee: u32, password: Option<&str>) -> u64 {
    host.send(ClientMsg::CreateTournament {
        name: "Friday".into(),
        size: 4,
        rated: false,
        rules: Rules::Brazilian,
        password: password.map(str::to_string),
        entry_fee: fee,
        format: Default::default(),
    })
    .await;
    let ServerMsg::TournamentUpdated { tournament } =
        host.expect("created", |m| matches!(m, ServerMsg::TournamentUpdated { .. })).await
    else {
        unreachable!()
    };
    tournament.id
}

/// Entry fees: everything that can stop a player is checked before the
/// coins are taken, each payment is given back once when it must be, and
/// joining again after leaving is paid (and refunded) again; it used to be
/// refused for good, or charged twice and refunded once. Guessing a
/// private tournament's password is cut short, and muted players can't
/// name a tournament for everyone to see.
#[tokio::test]
async fn tournament_fees_are_taken_and_given_back_once_each() {
    let (url, server) = start_on(snook_legend_server::store::Store::memory()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    for c in [&mut alice, &mut bob] {
        c.expect("daily bonus", |m| matches!(m, ServerMsg::CoinsEarned { .. })).await;
    }
    let bob_id = server.store.user_by_name("bob").await.unwrap().unwrap().id;
    server.store.credit(bob_id, 20, "test", None, snook_legend_server::now()).await.unwrap();
    let id = new_knockout(&mut alice, 5, Some("secret")).await;
    wait_for_coins(&server, "alice", 0).await;
    let join = |password: &str| ClientMsg::JoinTournament { id, password: Some(password.into()) };
    bob.send(join("guess")).await;
    assert_eq!(bob.error_text().await, "Wrong tournament password.");
    assert_eq!(coins_of(&server, "bob").await, 25, "nothing taken for a refused entry");
    for _ in 0..2 {
        bob.send(join("secret")).await;
        bob.expect("in", |m| {
            matches!(m, ServerMsg::TournamentUpdated { tournament } if tournament.players.len() == 2)
        })
        .await;
        wait_for_coins(&server, "bob", 20).await;
        bob.send(ClientMsg::LeaveTournament { id }).await;
        wait_for_coins(&server, "bob", 25).await;
    }
    for _ in 0..4 {
        bob.send(join("guess")).await;
        assert_eq!(bob.error_text().await, "Wrong tournament password.");
    }
    bob.send(join("secret")).await;
    assert_eq!(bob.error_text().await, "Too many wrong passwords. Try again in a few minutes.");
    assert_eq!(coins_of(&server, "bob").await, 25);

    server.store.set_mute(bob_id, Some(snook_legend_server::now() + 600)).await.unwrap();
    let user = server.store.user_by_id(bob_id).await.unwrap().unwrap();
    server.with(|l| l.refresh_user(user));
    bob.send(ClientMsg::CreateTournament {
        name: "Look at me".into(),
        size: 4,
        rated: false,
        rules: Rules::Brazilian,
        password: None,
        entry_fee: 0,
        format: Default::default(),
    })
    .await;
    assert!(bob.error_text().await.starts_with("You are muted"));
}

/// A server that stops calls its tournaments off (they live in its memory
/// alone) and gives every entry fee back before it goes.
#[tokio::test]
async fn a_restart_calls_tournaments_off_and_gives_the_fees_back() {
    let (url, server) = start_on(snook_legend_server::store::Store::memory()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    for c in [&mut alice, &mut bob] {
        c.expect("daily bonus", |m| matches!(m, ServerMsg::CoinsEarned { .. })).await;
    }
    let id = new_knockout(&mut alice, 5, None).await;
    bob.send(ClientMsg::JoinTournament { id, password: None }).await;
    wait_for_coins(&server, "bob", 0).await;
    let drain = tokio::spawn(snook_legend_server::drain(server.clone(), 1));
    for c in [&mut alice, &mut bob] {
        c.expect(
            "called off",
            |m| matches!(m, ServerMsg::Notice { text } if text.contains("called off")),
        )
        .await;
        c.expect("removed", |m| matches!(m, ServerMsg::TournamentRemoved { .. })).await;
    }
    drain.await.unwrap();
    wait_for_coins(&server, "alice", 5).await;
    wait_for_coins(&server, "bob", 5).await;
}

/// A running knockout is announced to the lounge when it changes, not
/// every tick; and a player who stays on the end of their game is taken
/// to the next one after a moment (the final used to wait for ever).
#[tokio::test]
async fn knockouts_move_on_and_are_announced_when_they_change() {
    let settings = Settings {
        tournament_pause: Duration::from_millis(500),
        shot_clock: Duration::from_secs(600),
        ..Settings::default()
    };
    let url = start(settings).await;
    let (mut ana, _) = Client::register(&url, "ana").await;
    let (mut ben, _) = Client::register(&url, "ben").await;
    let (mut cid, _) = Client::register(&url, "cid").await;
    let (mut dan, _) = Client::register(&url, "dan").await;
    let id = new_knockout(&mut ana, 0, None).await;
    for c in [&mut ben, &mut cid] {
        c.send(ClientMsg::JoinTournament { id, password: None }).await;
    }
    cid.expect("joined", |m| {
        matches!(m, ServerMsg::TournamentUpdated { tournament } if tournament.players.len() == 3)
    })
    .await;
    // Three players: the top seed (ana, first by name) waits in the final.
    ana.send(ClientMsg::StartTournament { id }).await;
    ben.expect("semi-final", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    cid.expect("semi-final", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    dan.drain().await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let heard = dan.drain().await;
    let updates = heard.iter().filter(|m| matches!(m, ServerMsg::TournamentUpdated { .. })).count();
    assert_eq!(updates, 0, "nothing changed, nothing announced: {heard:?}");
    // Cid wins and stays on the end of the game: the final starts anyway.
    ben.send(ClientMsg::Resign).await;
    cid.expect("won", |m| matches!(m, ServerMsg::MatchEnded { .. })).await;
    let ServerMsg::MatchStarted { players, .. } =
        cid.expect("final", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
    else {
        unreachable!()
    };
    assert!(players.iter().any(|p| p.username == "ana"));
    ana.expect("final", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
}

/// A knockout game both players left (for longer than the reconnection
/// allows) is settled like a match nobody came to: the bracket goes on
/// (it used to wait for that game for ever), and neither goes through (a
/// final without a champion: the fees go back).
#[tokio::test]
async fn a_tournament_game_both_players_left_is_settled() {
    let settings = Settings {
        reconnect_grace: Duration::from_millis(500),
        tournament_no_show: Duration::from_millis(500),
        ..Settings::default()
    };
    let url = start(settings).await;
    let (mut ana, _) = Client::register(&url, "ana").await;
    let (mut ben, _) = Client::register(&url, "ben").await;
    let (mut cid, _) = Client::register(&url, "cid").await;
    let id = new_knockout(&mut ana, 0, None).await;
    ben.send(ClientMsg::JoinTournament { id, password: None }).await;
    ben.expect("joined", |m| {
        matches!(m, ServerMsg::TournamentUpdated { tournament } if tournament.players.len() == 2)
    })
    .await;
    ana.send(ClientMsg::StartTournament { id }).await;
    ana.expect("final", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    ben.expect("final", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    drop(ana);
    drop(ben);
    let ServerMsg::TournamentUpdated { tournament } = cid
        .expect("finished", |m| {
            matches!(m, ServerMsg::TournamentUpdated { tournament }
                if tournament.state == TournamentState::Finished)
        })
        .await
    else {
        unreachable!()
    };
    assert!(tournament.champion.is_none(), "{tournament:?}");
}

#[tokio::test]
async fn withdrawing_while_waiting_for_an_opponent_frees_the_player() {
    let url = start(Settings::default()).await;
    let (mut ana, _) = Client::register(&url, "ana").await;
    let (mut ben, _) = Client::register(&url, "ben").await;
    let (mut cid, _) = Client::register(&url, "cid").await;
    ana.send(ClientMsg::CreateTournament {
        name: "Three".into(),
        size: 4,
        rated: false,
        rules: Rules::Brazilian,
        password: None,
        entry_fee: 0,
        format: Default::default(),
    })
    .await;
    let ServerMsg::TournamentUpdated { tournament } =
        ben.expect("created", |m| matches!(m, ServerMsg::TournamentUpdated { .. })).await
    else {
        unreachable!()
    };
    let id = tournament.id;
    for c in [&mut ben, &mut cid] {
        c.send(ClientMsg::JoinTournament { id, password: None }).await;
    }
    cid.expect("joined", |m| {
        matches!(m, ServerMsg::TournamentUpdated { tournament } if tournament.players.len() == 3)
    })
    .await;
    // Three players: the top seed (ana, first by name) waits in the final.
    ana.send(ClientMsg::StartTournament { id }).await;
    ben.expect("semi-final", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    cid.expect("semi-final", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    // Ana withdraws before her opponent is known: she may play elsewhere.
    ana.send(ClientMsg::LeaveTournament { id }).await;
    ana.send(ClientMsg::OpenTable { rated: false, rules: Rules::Brazilian, password: None }).await;
    ana.expect("table", |m| {
        assert!(!matches!(m, ServerMsg::Error { .. }), "still held by the tournament: {m:?}");
        matches!(m, ServerMsg::TableOpened { .. })
    })
    .await;
    // The semi-final's winner takes the final by walkover.
    ben.send(ClientMsg::Resign).await;
    let ServerMsg::TournamentUpdated { tournament } = cid
        .expect("finished", |m| {
            matches!(m, ServerMsg::TournamentUpdated { tournament }
                if tournament.state == TournamentState::Finished)
        })
        .await
    else {
        unreachable!()
    };
    assert_eq!(tournament.champion.as_deref(), Some("cid"));
}

#[tokio::test]
async fn vip_members_have_their_own_reactions() {
    let (url, server) = start_on(snook_legend_server::store::Store::memory()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let user = server.store.user_by_name("alice").await.unwrap().unwrap();
    server.store.set_vip(user.id, snook_legend_server::now() + 86_400, false).await.unwrap();
    snook_legend_server::shop::push_wallet(&server, &user).await;
    alice.expect("wallet", |m| matches!(m, ServerMsg::Wallet { .. })).await;
    start_match(&mut alice, &mut bob, "bob", "alice", false).await;
    alice.send(ClientMsg::React { code: "gem".into() }).await;
    let ServerMsg::Reaction { code, .. } =
        bob.expect("reaction", |m| matches!(m, ServerMsg::Reaction { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(code, "gem");
}

#[tokio::test]
async fn chat_lines_can_be_deleted_and_the_chat_cleared() {
    use snook_legend_proto::Deleted;
    let (url, _) = start_with(Settings::default(), vec!["boss".into()]).await;
    let mut boss = Client::login(&url, "boss", PASSWORD).await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    bob.send(ClientMsg::Chat { text: "oops".into() }).await;
    let oops = said(&mut boss, "oops").await;
    assert!(oops.id > 0 && !oops.admin);
    boss.send(ClientMsg::Chat { text: "rules".into() }).await;
    let rules = said(&mut bob, "rules").await;
    assert!(rules.admin, "an administrator's line");
    bob.send(ClientMsg::Chat { text: "rude".into() }).await;
    let rude = said(&mut boss, "rude").await;

    // Bob deletes his own line; he can't delete the administrator's.
    bob.send(ClientMsg::DeleteChat { id: rules.id, silent: false }).await;
    bob.send(ClientMsg::DeleteChat { id: oops.id, silent: true }).await; // not his to make vanish
    bob.send(ClientMsg::DeleteChat { id: oops.id, silent: false }).await;
    let m = boss.expect("deleted", |m| matches!(m, ServerMsg::ChatDeleted { .. })).await;
    assert_eq!(m, ServerMsg::ChatDeleted { id: oops.id, by: Deleted::Author });
    // The administrator makes a line vanish.
    boss.send(ClientMsg::DeleteChat { id: rude.id, silent: true }).await;
    let m = bob.expect("removed", |m| matches!(m, ServerMsg::ChatRemoved { .. })).await;
    assert_eq!(m, ServerMsg::ChatRemoved { id: rude.id });

    // Someone arriving now sees the deleted line without its text, and no
    // trace of the other.
    let (mut eve, _) = Client::register(&url, "eve").await;
    let ServerMsg::ChatHistory { lines } =
        eve.expect("history", |m| matches!(m, ServerMsg::ChatHistory { .. })).await
    else {
        unreachable!()
    };
    let ids: Vec<u64> = lines.iter().map(|l| l.id).collect();
    assert_eq!(ids, vec![oops.id, rules.id]);
    assert_eq!((lines[0].text.as_str(), lines[0].deleted), ("", Some(Deleted::Author)));

    // Only administrators clear the chat.
    bob.send(ClientMsg::ClearChat).await;
    boss.send(ClientMsg::ClearChat).await;
    let m = eve.expect("cleared", |m| matches!(m, ServerMsg::ChatCleared { .. })).await;
    assert_eq!(m, ServerMsg::ChatCleared { by: "boss".into() });
    let (mut late, _) = Client::register(&url, "late").await;
    let ServerMsg::ChatHistory { lines } =
        late.expect("history", |m| matches!(m, ServerMsg::ChatHistory { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(lines.len(), 1, "only the note that the chat was cleared: {lines:?}");
    assert!(lines[0].text.contains("boss"));
}

async fn said(c: &mut Client, text: &str) -> snook_legend_proto::ChatLine {
    let ServerMsg::Chat { line } =
        c.expect("line", |m| matches!(m, ServerMsg::Chat { line } if line.text == text)).await
    else {
        unreachable!()
    };
    line
}

#[tokio::test]
async fn administrators_watching_see_the_players_chat() {
    let (url, _) = start_with(Settings::default(), vec!["boss".into()]).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let mut boss = Client::login(&url, "boss", PASSWORD).await;
    let (mut eve, _) = Client::register(&url, "eve").await;
    start_match(&mut alice, &mut bob, "bob", "alice", false).await;
    let ServerMsg::Featured { featured: Some(featured) } =
        boss.expect("featured", |m| matches!(m, ServerMsg::Featured { featured: Some(_) })).await
    else {
        unreachable!()
    };
    let match_id = featured.match_id;
    alice.send(ClientMsg::MatchChat { text: "before".into() }).await;
    bob.expect("before", |m| matches!(m, ServerMsg::MatchChat { .. })).await;
    for c in [&mut boss, &mut eve] {
        c.send(ClientMsg::WatchMatch { match_id }).await;
        c.expect("watching", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    }
    // The administrator gets what was said before they came, and after.
    boss.expect("earlier", |m| matches!(m, ServerMsg::MatchChat { line } if line.text == "before"))
        .await;
    alice.send(ClientMsg::MatchChat { text: "gg".into() }).await;
    boss.expect("chat", |m| matches!(m, ServerMsg::MatchChat { line } if line.text == "gg")).await;
    // Anyone else watching doesn't.
    alice.send(ClientMsg::Resign).await;
    let m = eve
        .expect("end", |m| matches!(m, ServerMsg::MatchChat { .. } | ServerMsg::MatchEnded { .. }))
        .await;
    assert!(matches!(m, ServerMsg::MatchEnded { .. }), "a spectator saw the players' chat: {m:?}");
}

/// The lounge's TV shows public games, and only the game on it can be
/// watched that way: not a private table's, nor any game whose number
/// someone guesses.
#[tokio::test]
async fn only_the_game_on_the_tv_can_be_watched_from_it() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (mut eve, _) = Client::register(&url, "eve").await;
    alice
        .send(ClientMsg::OpenTable {
            rated: false,
            rules: Rules::Brazilian,
            password: Some("ours".into()),
        })
        .await;
    let ServerMsg::TableOpened { table } =
        bob.expect("table", |m| matches!(m, ServerMsg::TableOpened { .. })).await
    else {
        unreachable!()
    };
    bob.send(ClientMsg::JoinTable { id: table.id, password: Some("ours".into()) }).await;
    let ServerMsg::MatchStarted { match_id, .. } =
        alice.expect("match", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
    else {
        unreachable!()
    };
    // The TV is looked at every few seconds: still nothing on it.
    tokio::time::sleep(Duration::from_secs(5)).await;
    let heard = eve.drain().await;
    assert!(
        !heard.iter().any(|m| matches!(m, ServerMsg::Featured { featured: Some(_) })),
        "a private game on the TV: {heard:?}"
    );
    eve.send(ClientMsg::WatchMatch { match_id }).await;
    assert_eq!(eve.error_text().await, "You can't watch this game.");
}

/// Resigning while one's shot is being played comes after the shot: both
/// players see its outcome, then the end (the stored game is what the
/// table saw).
#[tokio::test]
async fn resigning_during_a_shot_comes_after_it() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (game, sa, _) = start_match(&mut alice, &mut bob, "bob", "alice", false).await;
    let (shooter, other) =
        if game.current_player() == sa { (&mut alice, &mut bob) } else { (&mut bob, &mut alice) };
    let d = ai::decide(&game, Difficulty::Hard, 1);
    shooter
        .send(ClientMsg::Shoot {
            turn: 0,
            shot: d.shot,
            called_pocket: d.called_pocket,
            cue_ball: d.cue_ball,
        })
        .await;
    shooter.send(ClientMsg::Resign).await;
    for c in [shooter, other] {
        let m = c
            .expect("result or end", |m| {
                matches!(m, ServerMsg::ShotResult { .. } | ServerMsg::MatchEnded { .. })
            })
            .await;
        assert!(matches!(m, ServerMsg::ShotResult { .. }), "the end came first: {m:?}");
        c.expect("end", |m| matches!(m, ServerMsg::MatchEnded { .. })).await;
    }
}

/// Blocking someone withdraws the invitations between the two, and lines
/// they wrote stay hidden from the history even when they have left.
#[tokio::test]
async fn blocks_reach_invitations_and_the_chat_history() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, token) = Client::register(&url, "bob").await;
    let (mut carol, _) = Client::register(&url, "carol").await;
    alice.send(ClientMsg::Invite { to: "bob".into(), rated: false, rules: Rules::Brazilian }).await;
    bob.expect("invited", |m| matches!(m, ServerMsg::Invited { .. })).await;
    bob.send(ClientMsg::Block { username: "alice".into() }).await;
    alice.expect("declined", |m| matches!(m, ServerMsg::InviteDeclined { .. })).await;
    bob.send(ClientMsg::AnswerInvite { from: "alice".into(), accept: true }).await;
    bob.expect("gone", |m| matches!(m, ServerMsg::InviteFailed { .. })).await;

    bob.send(ClientMsg::Block { username: "carol".into() }).await;
    bob.send(ClientMsg::Friends).await;
    bob.expect("friends", |m| matches!(m, ServerMsg::Friends { .. })).await;
    carol.send(ClientMsg::Chat { text: "psst".into() }).await;
    said(&mut alice, "psst").await;
    drop(carol);
    alice
        .expect("left", |m| matches!(m, ServerMsg::PlayerLeft { username } if username == "carol"))
        .await;
    let mut again = Client::connect(&url).await;
    again.send(ClientMsg::Resume { token }).await;
    let ServerMsg::ChatHistory { lines } =
        again.expect("history", |m| matches!(m, ServerMsg::ChatHistory { .. })).await
    else {
        unreachable!()
    };
    assert!(lines.iter().all(|l| l.from != "carol"), "{lines:?}");
}

/// The aim relayed to the opponent is one the cue can take.
#[tokio::test]
async fn relayed_aim_is_within_the_cues_limits() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (game, sa, _) = start_match(&mut alice, &mut bob, "bob", "alice", false).await;
    let (shooter, other) =
        if game.current_player() == sa { (&mut alice, &mut bob) } else { (&mut bob, &mut alice) };
    let wild = snook_legend_proto::AimState {
        yaw: 1e300,
        elevation: 9.0,
        tip: glam::DVec2::new(30.0, -40.0),
        power: 7.0,
        called_pocket: None,
    };
    shooter.send(ClientMsg::Aim { aim: wild }).await;
    let ServerMsg::OpponentAim { aim } =
        other.expect("aim", |m| matches!(m, ServerMsg::OpponentAim { .. })).await
    else {
        unreachable!()
    };
    assert!((0.0..std::f64::consts::TAU).contains(&aim.yaw), "{aim:?}");
    assert!(aim.elevation <= game.params().max_elevation && aim.elevation >= 0.0);
    assert!(aim.tip.length() <= 1.0 + 1e-9 && (0.0..=1.0).contains(&aim.power));
}

/// A guest who leaves while the server is stopping is gone like after any
/// lost connection (a guest's game can't move to the next server): the
/// opponent is told, instead of waiting for nothing.
#[tokio::test]
async fn a_guest_leaving_during_a_restart_is_gone() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let settings = Settings { lan: true, max_connections_per_ip: 100, ..Settings::default() };
    let server = Server::new(Config { bind: addr, settings, ..Config::default() }).await.unwrap();
    tokio::spawn(serve(listener, server.clone()));
    let url = format!("ws://{addr}/ws");
    let mut a = Client::connect(&url).await;
    a.send(ClientMsg::Guest { name: "host".into() }).await;
    a.expect("guest", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
    let mut b = Client::connect(&url).await;
    b.send(ClientMsg::Guest { name: "visitor".into() }).await;
    b.expect("guest", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
    start_match(&mut a, &mut b, "visitor", "host", false).await;
    let _drain = tokio::spawn(snook_legend_server::drain(server.clone(), 60));
    b.expect("draining", |m| matches!(m, ServerMsg::Draining { .. })).await;
    drop(a);
    b.expect("opponent gone", |m| {
        matches!(m, ServerMsg::OpponentConnection { connected: false, .. })
    })
    .await;
}

// ---------------------------------------------------------------------------
// The official rules.

/// A rated game under the official rules (with the choices its breaks
/// leave): the official rating, ranking and chart change, the Brazilian
/// ones don't.
#[tokio::test]
async fn a_rated_official_game_counts_under_the_official_rules_only() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (game, sa, _) =
        start_match_under(&mut alice, &mut bob, "bob", "alice", true, Rules::Official).await;
    let result = play_out(&mut alice, &mut bob, game, sa).await;
    assert!(result.rated);
    let winner = if result.winner == sa { "alice" } else { "bob" };
    for rules in Rules::ALL {
        alice.send(ClientMsg::Leaderboard { rules }).await;
        let ServerMsg::Leaderboard { rules: shown, entries } =
            alice.expect("leaderboard", |m| matches!(m, ServerMsg::Leaderboard { .. })).await
        else {
            unreachable!()
        };
        assert_eq!(shown, rules);
        match rules {
            Rules::Official => {
                assert_eq!(entries.len(), 2);
                assert_eq!(entries[0].username, winner);
                assert!(entries[0].rating > 1200 && entries[1].rating < 1200);
            }
            Rules::Brazilian => assert!(entries.is_empty(), "nobody played the Brazilian rules"),
        }
    }
    alice.send(ClientMsg::Profile { username: "bob".into() }).await;
    let ServerMsg::Profile { profile } =
        alice.expect("profile", |m| matches!(m, ServerMsg::Profile { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(profile.history[0].rules, Rules::Official);
    assert_eq!(profile.player.standing(Rules::Brazilian), Default::default());
    assert_eq!(profile.player.standing(Rules::Official).games, 1);
    assert_eq!(profile.rating_history.official.len(), 1);
    assert!(profile.rating_history.brazilian.is_empty());
    assert!(profile.ranks.official.is_some() && profile.ranks.brazilian.is_none());

    // A rematch is played under the same rules.
    alice.send(ClientMsg::Rematch).await;
    bob.expect("rematch offer", |m| matches!(m, ServerMsg::RematchOffered)).await;
    bob.send(ClientMsg::Rematch).await;
    let ServerMsg::MatchStarted { game, .. } =
        alice.expect("rematch", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(game.rules(), Rules::Official);
}

/// A game older than 0.12.0 (protocol 24) can't call the 8 under the
/// official rules: what it makes counts as called in the pocket it went
/// into, and is recorded so (the game replays the same).
#[tokio::test]
async fn an_older_game_makes_the_eight_in_any_pocket() {
    let url = start(Settings::default()).await;
    let (mut alice, _) =
        Client::register_as(&url, "alice", snook_legend_proto::OLDEST_PROTOCOL_VERSION).await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (game, sa, _) =
        start_match_under(&mut alice, &mut bob, "bob", "alice", true, Rules::Official).await;
    assert!(game.eight_call());
    let result = play_out_with(&mut alice, &mut bob, game, sa, true).await;
    alice.send(ClientMsg::Replays).await;
    let ServerMsg::Replays { games, .. } =
        alice.expect("replays", |m| matches!(m, ServerMsg::Replays { .. })).await
    else {
        unreachable!()
    };
    alice.send(ClientMsg::Replay { match_id: games[0].match_id }).await;
    let ServerMsg::Replay { replay } =
        alice.expect("replay", |m| matches!(m, ServerMsg::Replay { .. })).await
    else {
        unreachable!()
    };
    assert!(replay.replay.eight_call && !replay.replay.calls);
    let (end, _) = replay.replay.play(None).unwrap();
    assert_eq!(
        end.game_over(),
        Some(snook_legend_core::GameOver { winner: result.winner, reason: result.reason })
    );
}

/// "Play now" pairs players who want the same rules; everyone sees who is
/// looking for a game, and under which rules each player plays.
#[tokio::test]
async fn play_now_pairs_players_of_the_same_rules() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (mut carol, _) = Client::register(&url, "carol").await;
    let (mut dave, _) = Client::register(&url, "dave").await;

    // Dave plays the official rules: everyone sees it.
    dave.send(ClientMsg::SetRules { rules: Rules::Official }).await;
    alice
        .expect("dave's rules", |m| {
            matches!(m, ServerMsg::PlayerUpdated { player }
                if player.username == "dave" && player.rules == Rules::Official)
        })
        .await;

    alice.send(ClientMsg::QueueJoin { rated: true, rules: Rules::Official }).await;
    let ServerMsg::Queue { searching, rules, .. } =
        alice.expect("searching", |m| matches!(m, ServerMsg::Queue { .. })).await
    else {
        unreachable!()
    };
    assert!(searching && rules == Rules::Official);
    // The others see that she is looking for a game.
    dave.expect("alice searching", |m| {
        matches!(m, ServerMsg::PlayerUpdated { player } if player.username == "alice" && player.searching)
    })
    .await;
    // Bob wants the Brazilian rules: not paired with her.
    bob.send(ClientMsg::QueueJoin { rated: true, rules: Rules::Brazilian }).await;
    bob.expect("searching", |m| matches!(m, ServerMsg::Queue { searching: true, .. })).await;
    carol.send(ClientMsg::QueueJoin { rated: true, rules: Rules::Official }).await;
    for c in [&mut alice, &mut carol] {
        let ServerMsg::MatchStarted { game, players, .. } =
            c.expect("paired", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
        else {
            unreachable!()
        };
        assert_eq!(game.rules(), Rules::Official);
        let mut names: Vec<_> = players.iter().map(|p| p.username.clone()).collect();
        names.sort();
        assert_eq!(names, ["alice", "carol"]);
    }
    // Bob is still waiting, under his rules.
    bob.send(ClientMsg::QueueLeave).await;
    let ServerMsg::Queue { searching, rules, .. } =
        bob.expect("left", |m| matches!(m, ServerMsg::Queue { searching: false, .. })).await
    else {
        unreachable!()
    };
    assert!(!searching && rules == Rules::Brazilian);
    dave.expect("bob stopped", |m| {
        matches!(m, ServerMsg::PlayerUpdated { player } if player.username == "bob" && !player.searching)
    })
    .await;
}

/// A table opened under the official rules is played under them.
#[tokio::test]
async fn tables_keep_their_rules() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    alice.send(ClientMsg::OpenTable { rated: false, rules: Rules::Official, password: None }).await;
    let ServerMsg::TableOpened { table } =
        bob.expect("table", |m| matches!(m, ServerMsg::TableOpened { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(table.rules, Rules::Official);
    bob.send(ClientMsg::JoinTable { id: table.id, password: None }).await;
    let ServerMsg::MatchStarted { game, .. } =
        bob.expect("match", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(game.rules(), Rules::Official);
}

/// Under the official rules a shot clock that runs out is a foul: the
/// opponent has the cue ball in hand (on the break: they break).
#[tokio::test]
async fn the_shot_clock_is_a_foul_under_the_official_rules() {
    let settings = Settings { shot_clock: Duration::from_secs(1), ..Settings::default() };
    let url = start(settings).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (game, _, _) =
        start_match_under(&mut alice, &mut bob, "bob", "alice", false, Rules::Official).await;
    let first = game.current_player();
    let ServerMsg::TimeFoul { seat, game, .. } = alice
        .expect("time foul", |m| {
            matches!(m, ServerMsg::TimeFoul { .. } | ServerMsg::ShotTaken { .. })
        })
        .await
    else {
        panic!("a shot was played for the player")
    };
    assert_eq!(seat, first);
    assert_eq!(game.current_player(), 1 - first);
    assert!(game.is_break(), "the opponent breaks");
    assert_eq!(game.ball_in_hand(), Some(BallInHand::Kitchen));
}

/// A player who lets the clock run out three times in a row left the game:
/// a game nobody plays (the official rules never end on the clock) ends.
#[tokio::test]
async fn a_player_who_never_shoots_leaves_the_game() {
    let settings = Settings { shot_clock: Duration::from_secs(1), ..Settings::default() };
    let url = start(settings).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (game, _, _) =
        start_match_under(&mut alice, &mut bob, "bob", "alice", false, Rules::Official).await;
    let first = game.current_player();
    // Neither shoots: the first to let their clock run out a third time
    // (after the other's second) loses.
    let ServerMsg::MatchEnded { result } =
        alice.expect("end", |m| matches!(m, ServerMsg::MatchEnded { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(result.reason, snook_legend_core::GameOverReason::Abandoned);
    assert_eq!(result.winner, 1 - first);
}

/// The lounge counts who plays each of the rules (the public status too).
#[tokio::test]
async fn players_are_counted_under_their_rules() {
    let (url, http) = start_with(Settings::default(), Vec::new()).await;
    let (_alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    bob.send(ClientMsg::SetRules { rules: Rules::Official }).await;
    bob.drain().await;
    let host = http.trim_start_matches("http://");
    let (code, status) = http_get(host, "/api/status").await;
    assert_eq!(code, 200);
    assert_eq!(status["online"], 2);
    assert_eq!(status["online_by_rules"]["brazilian"], 1);
    assert_eq!(status["online_by_rules"]["official"], 1);
    let (code, board) = http_get(host, "/api/leaderboard?rules=official").await;
    assert_eq!(code, 200);
    assert!(board.as_array().is_some_and(|b| b.is_empty()));
    assert_eq!(http_get(host, "/api/leaderboard?rules=snooker").await.0, 400);
}

/// A GET to the server's HTTP side: the status code and the JSON body.
async fn http_get(host: &str, path: &str) -> (u16, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = TcpStream::connect(host).await.unwrap();
    let req = format!("GET {path} HTTP/1.0\r\nHost: {host}\r\n\r\n");
    s.write_all(req.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.unwrap();
    let out = String::from_utf8_lossy(&out).to_string();
    let code = out[9..12].parse().unwrap();
    let body = out.split_once("\r\n\r\n").map(|x| x.1).unwrap_or("");
    (code, serde_json::from_str(body).unwrap_or(serde_json::Value::Null))
}

/// A game that ends leaves the lounge's TV at once, not on its next round
/// (a player back in the lounge was offered to watch the game they left).
#[tokio::test]
async fn the_tv_drops_a_game_the_moment_it_ends() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (mut eve, _) = Client::register(&url, "eve").await;
    alice.send(ClientMsg::CreateInviteCode { rated: false, rules: Rules::Brazilian }).await;
    let ServerMsg::InviteCode { code } =
        alice.expect("code", |m| matches!(m, ServerMsg::InviteCode { .. })).await
    else {
        unreachable!()
    };
    bob.send(ClientMsg::JoinByCode { code }).await;
    alice.expect("match", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    eve.expect("on the TV", |m| matches!(m, ServerMsg::Featured { featured: Some(_) })).await;
    alice.send(ClientMsg::Resign).await;
    alice.send(ClientMsg::LeaveMatch).await;
    let off = tokio::time::timeout(
        std::time::Duration::from_millis(600),
        eve.expect("off the TV", |m| matches!(m, ServerMsg::Featured { featured: None })),
    )
    .await;
    assert!(off.is_ok(), "the TV still showed the game that ended");
}

/// A game over is not played any more: its players are free in the lounge
/// at once (also while they still look at the result), and can be invited.
#[tokio::test]
async fn players_are_free_as_soon_as_the_game_is_over() {
    use snook_legend_proto::PlayerStatus;
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (mut carol, _) = Client::register(&url, "carol").await;
    let (game, sa, _) = start_match(&mut alice, &mut bob, "bob", "alice", true).await;
    let status = |m: &ServerMsg, name: &str, s: PlayerStatus| matches!(m, ServerMsg::PlayerUpdated { player } if player.username == name && player.status == s);
    carol.expect("alice playing", |m| status(m, "alice", PlayerStatus::Playing)).await;
    play_out(&mut alice, &mut bob, game, sa).await;
    // Neither has left the result yet: both free for the lounge.
    carol.expect("alice free", |m| status(m, "alice", PlayerStatus::Available)).await;
    carol.expect("bob free", |m| status(m, "bob", PlayerStatus::Available)).await;
    // Carol invites Bob, still at the result; he accepts and plays her.
    carol.send(ClientMsg::Invite { to: "bob".into(), rated: false, rules: Rules::Brazilian }).await;
    bob.expect("invited", |m| matches!(m, ServerMsg::Invited { .. })).await;
    bob.send(ClientMsg::AnswerInvite { from: "carol".into(), accept: true }).await;
    alice.expect("bob left", |m| matches!(m, ServerMsg::OpponentLeft)).await;
    let ServerMsg::MatchStarted { players, .. } =
        bob.expect("new game", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
    else {
        unreachable!()
    };
    assert!(players.iter().any(|p| p.username == "carol"));
    alice.send(ClientMsg::LeaveMatch).await;
    carol.expect("alice still free", |m| status(m, "alice", PlayerStatus::Available)).await;
}

/// Leaving with a shot still rolling: the resignation waits for the shot,
/// and the player is back in the lounge with it (not left at the table).
#[tokio::test]
async fn leaving_during_a_shot_is_not_lost() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (game, sa, _) = start_match(&mut alice, &mut bob, "bob", "alice", false).await;
    let d = ai::decide(&game, Difficulty::Easy, 1);
    let shot = ClientMsg::Shoot {
        turn: 0,
        shot: d.shot,
        called_pocket: d.called_pocket,
        cue_ball: d.cue_ball,
    };
    let breaker_is_alice = game.current_player() == sa;
    let (shooter, other) =
        if breaker_is_alice { (&mut alice, &mut bob) } else { (&mut bob, &mut alice) };
    // The shot, the resignation and the leaving, one after the other.
    shooter.send(shot).await;
    shooter.send(ClientMsg::Resign).await;
    shooter.send(ClientMsg::LeaveMatch).await;
    shooter.expect("ended", |m| matches!(m, ServerMsg::MatchEnded { .. })).await;
    other.expect("they left", |m| matches!(m, ServerMsg::OpponentLeft)).await;
    // Back in the lounge: free to look for a game.
    shooter.send(ClientMsg::QueueJoin { rated: false, rules: Rules::Brazilian }).await;
    let m = shooter
        .expect("queued", |m| matches!(m, ServerMsg::Queue { .. } | ServerMsg::Error { .. }))
        .await;
    assert!(matches!(m, ServerMsg::Queue { .. }), "still at the table: {m:?}");
}

/// Back at the table after a dropped connection, a player gets what the
/// opponent said meanwhile (it used to be lost).
#[tokio::test]
async fn what_was_said_while_away_is_not_lost() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, bob_token) = Client::register(&url, "bob").await;
    start_match(&mut alice, &mut bob, "bob", "alice", false).await;
    drop(bob);
    alice
        .expect("lost", |m| matches!(m, ServerMsg::OpponentConnection { connected: false, .. }))
        .await;
    alice.send(ClientMsg::MatchChat { text: "are you there?".into() }).await;
    alice.expect("said", |m| matches!(m, ServerMsg::MatchChat { .. })).await;
    let mut bob = Client::connect(&url).await;
    bob.send(ClientMsg::Resume { token: bob_token }).await;
    bob.expect("back at the table", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    let ServerMsg::MatchChat { line } =
        bob.expect("the line", |m| matches!(m, ServerMsg::MatchChat { .. })).await
    else {
        unreachable!()
    };
    assert_eq!((line.from.as_str(), line.text.as_str()), ("alice", "are you there?"));
}

/// Both players dropped: the first back is told the other is away (with
/// the time left), not shown a table where the other seems to be.
#[tokio::test]
async fn back_first_sees_the_opponent_away() {
    let url = start(Settings::default()).await;
    let (mut alice, alice_token) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    start_match(&mut alice, &mut bob, "bob", "alice", false).await;
    drop(bob);
    alice
        .expect("lost", |m| matches!(m, ServerMsg::OpponentConnection { connected: false, .. }))
        .await;
    drop(alice);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut alice = Client::connect(&url).await;
    alice.send(ClientMsg::Resume { token: alice_token }).await;
    alice.expect("back at the table", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    let ServerMsg::OpponentConnection { connected, grace_secs, .. } =
        alice.expect("bob away", |m| matches!(m, ServerMsg::OpponentConnection { .. })).await
    else {
        unreachable!()
    };
    assert!(!connected);
    assert!((1..=60).contains(&grace_secs), "{grace_secs}");
}

/// The shot clock of a player whose connection dropped waits for them (a
/// moment's drop at their turn used to cost a foul), and they have a fair
/// moment to shoot when back.
#[tokio::test]
async fn the_shot_clock_waits_for_a_dropped_player() {
    let settings = Settings {
        shot_clock: Duration::from_secs(1),
        reconnect_grace: Duration::from_secs(30),
        ..Settings::default()
    };
    let url = start(settings).await;
    let (mut alice, alice_token) = Client::register(&url, "alice").await;
    let (mut bob, bob_token) = Client::register(&url, "bob").await;
    let (game, sa, _) = start_match(&mut alice, &mut bob, "bob", "alice", false).await;
    let (shooter, shooter_token, mut other) = if game.current_player() == sa {
        (alice, alice_token, bob)
    } else {
        (bob, bob_token, alice)
    };
    // The shooter's connection drops.
    drop(shooter);
    other
        .expect("lost", |m| matches!(m, ServerMsg::OpponentConnection { connected: false, .. }))
        .await;
    // Past the first shot clock (the introduction's seconds and one).
    tokio::time::sleep(Duration::from_millis(5000)).await;
    let mut shooter = Client::connect(&url).await;
    shooter.send(ClientMsg::Resume { token: shooter_token }).await;
    let m = other
        .expect("back", |m| {
            matches!(
                m,
                ServerMsg::TimeFoul { .. } | ServerMsg::OpponentConnection { connected: true, .. }
            )
        })
        .await;
    let ServerMsg::OpponentConnection { clock_left: Some(left), .. } = m else {
        panic!("the clock ran while they were away: {m:?}")
    };
    assert!(left > 0.5, "{left}");
    let ServerMsg::MatchStarted { clock_left, game: back, .. } =
        shooter.expect("the table", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
    else {
        unreachable!()
    };
    assert!(clock_left > 0.5, "{clock_left}");
    assert_eq!(back.shots_taken(), 0, "nothing was played meanwhile");
}

/// While the friend being watched is reconnecting, their game can still be
/// watched (a spectator who reconnects too asks again), and the friend back
/// at the table is told who is watching (a new table showed nobody).
#[tokio::test]
async fn watching_goes_on_while_the_friend_reconnects() {
    let url = start(Settings::default()).await;
    let (mut alice, alice_token) = Client::register(&url, "alice").await;
    let (mut bob, _) = Client::register(&url, "bob").await;
    let (mut carol, _) = Client::register(&url, "carol").await;
    carol.send(ClientMsg::FriendRequest { username: "alice".into() }).await;
    alice.expect("request", |m| matches!(m, ServerMsg::FriendRequestReceived { .. })).await;
    alice.send(ClientMsg::FriendAnswer { username: "carol".into(), accept: true }).await;
    carol
        .expect(
            "friends",
            |m| matches!(m, ServerMsg::Friends { friends, .. } if !friends.is_empty()),
        )
        .await;
    start_match(&mut alice, &mut bob, "bob", "alice", false).await;
    drop(alice);
    bob.expect("lost", |m| matches!(m, ServerMsg::OpponentConnection { connected: false, .. }))
        .await;
    carol.send(ClientMsg::Spectate { username: "alice".into() }).await;
    let m = carol
        .expect("watching", |m| {
            matches!(m, ServerMsg::MatchStarted { .. } | ServerMsg::Error { .. })
        })
        .await;
    assert!(matches!(m, ServerMsg::MatchStarted { spectating: true, .. }), "{m:?}");
    let mut alice = Client::connect(&url).await;
    alice.send(ClientMsg::Resume { token: alice_token }).await;
    alice.expect("back", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    alice.expect("watched", |m| matches!(m, ServerMsg::Spectators { count: 1 })).await;
}

/// The usual way a game reconnects: it signs in again before the server saw
/// its old connection go. What was sent to that connection in its last
/// moments (the opponent's line, a rematch asked for) comes again.
#[tokio::test]
async fn signing_in_again_brings_back_what_the_old_connection_missed() {
    let url = start(Settings::default()).await;
    let (mut alice, _) = Client::register(&url, "alice").await;
    let (mut bob, bob_token) = Client::register(&url, "bob").await;
    start_match(&mut alice, &mut bob, "bob", "alice", false).await;
    alice.send(ClientMsg::MatchChat { text: "good luck".into() }).await;
    alice.expect("said", |m| matches!(m, ServerMsg::MatchChat { .. })).await;
    // (Bob's old connection got it; his game didn't.)
    let mut bob2 = Client::connect(&url).await;
    bob2.send(ClientMsg::Resume { token: bob_token.clone() }).await;
    bob2.expect("back", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    let ServerMsg::MatchChat { line } =
        bob2.expect("the line", |m| matches!(m, ServerMsg::MatchChat { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(line.text, "good luck");
    // The game ends; a rematch asked for meanwhile reaches the next one.
    drop(bob);
    alice.send(ClientMsg::Resign).await;
    bob2.expect("over", |m| matches!(m, ServerMsg::MatchEnded { .. })).await;
    alice.send(ClientMsg::Rematch).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut bob3 = Client::connect(&url).await;
    bob3.send(ClientMsg::Resume { token: bob_token }).await;
    bob3.expect("back", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    bob3.expect("the offer", |m| matches!(m, ServerMsg::RematchOffered)).await;
}
