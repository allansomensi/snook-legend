//! Games against the computer, server side: the ladder's reward (granted
//! only for a final game the server plays again and finds genuine) and the
//! style of a player's ghost. Store tests run on the in-memory store, and
//! on PostgreSQL when `SNOOK_LEGEND_TEST_DATABASE_URL` points at a database
//! that may be wiped.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use snook_legend_core::{Difficulty, GameOverReason, ai};
use snook_legend_proto::ghost::{self, GhostAnswer};
use snook_legend_proto::ladder::{self, ClaimFailure, ClaimResult};
use snook_legend_proto::replay::{Replay, Stroke, apply};
use snook_legend_proto::{ClientMsg, PROTOCOL_VERSION, Role, Rules, ServerMsg, parse_server};
use snook_legend_server::lounge::Settings;
use snook_legend_server::store::{MatchRecord, NewUser, Store};
use snook_legend_server::{Config, Server, serve};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A game won against the ladder's last opponent, recorded by this version
/// (see [`record_the_fixture`] to make it again after a change to the AI
/// or the physics).
const FIXTURE: &str = include_str!("data/ladder_final_win.json");

// ---------------------------------------------------------------------------
// Helpers.

async fn start() -> (String, Arc<Server>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let settings = Settings { max_connections_per_ip: 100, ..Settings::default() };
    let config = Config { bind: addr, settings, ..Config::default() };
    let server = Server::new(config).await.unwrap();
    tokio::spawn(serve(listener, server.clone()));
    (format!("ws://{addr}/ws"), server)
}

struct Client {
    ws: Ws,
    /// Every message read, in order.
    seen: Vec<ServerMsg>,
}

impl Client {
    async fn connect(url: &str) -> Client {
        let (ws, _) = connect_async(url).await.unwrap();
        let mut c = Client { ws, seen: Vec::new() };
        c.send(ClientMsg::Hello { version: PROTOCOL_VERSION, rules: Rules::Brazilian }).await;
        c
    }

    async fn send(&mut self, msg: ClientMsg) {
        self.ws.send(Message::Text(msg.to_json().into())).await.unwrap();
    }

    /// Reads messages until one matches `pred` (within `secs`).
    async fn expect_within<F: Fn(&ServerMsg) -> bool>(
        &mut self,
        what: &str,
        secs: u64,
        pred: F,
    ) -> ServerMsg {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            let msg = tokio::time::timeout(left, self.ws.next())
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
                .expect("connection closed")
                .unwrap();
            if let Message::Text(t) = msg {
                let m = parse_server(t.as_str()).unwrap();
                self.seen.push(m.clone());
                if pred(&m) {
                    return m;
                }
            }
        }
    }

    async fn expect<F: Fn(&ServerMsg) -> bool>(&mut self, what: &str, pred: F) -> ServerMsg {
        self.expect_within(what, 20, pred).await
    }

    async fn register(url: &str, name: &str) -> Client {
        let mut c = Client::connect(url).await;
        c.send(ClientMsg::Register {
            username: name.into(),
            password: "secret123".into(),
            country: "BR".into(),
            minor: None,
        })
        .await;
        c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
        c
    }

    async fn claim(&mut self, replay: Replay, secs: u64) -> ClaimResult {
        self.send(ClientMsg::LadderClaim { replay }).await;
        match self
            .expect_within("claim", secs, |m| matches!(m, ServerMsg::LadderClaim { .. }))
            .await
        {
            ServerMsg::LadderClaim { result } => result,
            _ => unreachable!(),
        }
    }

    /// A final dealt by the server: its rack and breaker.
    async fn ticket(&mut self) -> Replay {
        self.send(ClientMsg::LadderTicket).await;
        match self.expect("ticket", |m| matches!(m, ServerMsg::LadderTicket { .. })).await {
            ServerMsg::LadderTicket { seed, breaker } => {
                Replay::new(seed, breaker, Rules::Brazilian)
            }
            _ => unreachable!(),
        }
    }

    async fn ghost(&mut self, name: &str) -> GhostAnswer {
        self.send(ClientMsg::GhostStyle { username: name.into() }).await;
        match self.expect_within("ghost", 60, |m| matches!(m, ServerMsg::GhostStyle { .. })).await {
            ServerMsg::GhostStyle { answer, .. } => answer,
            _ => unreachable!(),
        }
    }
}

/// A game between two computer players, recorded.
fn played(seed: u64, a: Difficulty, b: Difficulty) -> (Replay, usize) {
    let mut replay = Replay::new(seed, (seed % 2) as usize, Rules::Brazilian);
    let mut game = replay.start();
    let players = [a.player(), b.player()];
    while game.game_over().is_none() && replay.strokes.len() < 250 {
        let p = &players[game.current_player()];
        let stroke = ladder::computer_stroke(&game, p, seed ^ 0xabc);
        apply(&mut game, &stroke).unwrap();
        replay.strokes.push(stroke);
    }
    let winner = game.game_over().map_or(0, |o| o.winner);
    (replay, winner)
}

fn record(players: [i64; 2], replay: Replay, winner: usize, at: i64) -> MatchRecord {
    MatchRecord {
        players,
        winner,
        reason: GameOverReason::PocketedEight,
        rated: false,
        rules: Rules::Brazilian,
        delta: [0, 0],
        ratings_after: [1200, 1200],
        shots: replay.strokes.len() as u32,
        tournament: None,
        eight_win: true,
        runout: false,
        shutout: false,
        break_win: false,
        ended_at: at,
        replay: Some(replay),
        stats: Default::default(),
        abandoned: None,
        room: None,
        counts: true,
    }
}

// ---------------------------------------------------------------------------
// The store.

async fn exercise(store: Store) {
    let new = |name: &'static str| NewUser {
        username: name,
        password_hash: "h",
        country: "BR",
        rating: 1200,
        role: Role::Player,
        ip: "1.2.3.4",
        minor: None,
    };
    let a = store.create_user(&new("Ladderer")).await.unwrap().unwrap();
    let b = store.create_user(&new("Other")).await.unwrap().unwrap();

    // The reward, once per account.
    assert_eq!(store.ladder_claim(a.id).await.unwrap(), None);
    assert!(store.record_ladder_claim(a.id, u64::MAX - 3, 57, 1_000).await.unwrap());
    assert!(!store.record_ladder_claim(a.id, 5, 60, 2_000).await.unwrap(), "only once");
    assert_eq!(store.ladder_claim(a.id).await.unwrap(), Some(1_000));
    assert_eq!(store.ladder_claim(b.id).await.unwrap(), None);

    // Finals dealt: per account, the full range of seeds.
    assert_eq!(store.ladder_ticket(a.id, u64::MAX).await.unwrap(), None);
    store.issue_ladder_ticket(a.id, u64::MAX, 1, 1_500).await.unwrap();
    store.issue_ladder_ticket(a.id, 42, 0, 1_600).await.unwrap();
    assert_eq!(store.ladder_ticket(a.id, u64::MAX).await.unwrap(), Some((1, 1_500)));
    assert_eq!(store.ladder_ticket(a.id, 42).await.unwrap(), Some((0, 1_600)));
    assert_eq!(store.ladder_ticket(b.id, u64::MAX).await.unwrap(), None);
    // Only the newest are kept.
    for k in 0..snook_legend_server::store::LADDER_TICKETS_KEPT as u64 {
        store.issue_ladder_ticket(a.id, 100 + k, 1, 1_700 + k as i64).await.unwrap();
    }
    assert_eq!(store.ladder_ticket(a.id, u64::MAX).await.unwrap(), None, "the oldest went");
    assert_eq!(store.ladder_ticket(a.id, 100).await.unwrap(), Some((1, 1_700)));
    // Dealt twice (the same rack by chance): the first stays.
    store.issue_ladder_ticket(a.id, 100, 0, 9_000).await.unwrap();
    assert_eq!(store.ladder_ticket(a.id, 100).await.unwrap(), Some((1, 1_700)));

    // A player's latest games with their seat, newest first.
    assert!(store.recent_replays(a.id, 10).await.unwrap().is_empty());
    let r1 = Replay { strokes: vec![Stroke::TimeFoul], ..Replay::new(1, 0, Rules::Brazilian) };
    let r2 =
        Replay { strokes: vec![Stroke::TimeFoul; 2], ..Replay::new(u64::MAX, 1, Rules::Brazilian) };
    let (_, first) =
        store.record_match(&record([a.id, b.id], r1.clone(), 0, 10)).await.unwrap().unwrap();
    let (_, second) =
        store.record_match(&record([b.id, a.id], r2.clone(), 1, 20)).await.unwrap().unwrap();
    // A game without a replay is left out.
    let mut bare = record([a.id, b.id], r1.clone(), 0, 30);
    bare.replay = None;
    store.record_match(&bare).await.unwrap();
    let mine = store.recent_replays(a.id, 10).await.unwrap();
    assert_eq!(mine, vec![(second, 1, r2.clone()), (first, 0, r1.clone())]);
    assert_eq!(store.recent_replays(a.id, 1).await.unwrap().len(), 1);
    let theirs = store.recent_replays(b.id, 10).await.unwrap();
    assert_eq!(theirs.iter().map(|(_, s, _)| *s).collect::<Vec<_>>(), vec![0, 1]);

    // The medals of the tournaments against the computer: one per
    // tournament, counted by kind and place.
    use snook_legend_proto::cup::{Format, Medal, Medals};
    let gold = Medal { cup: 11, format: Format::Knockout, place: 1 };
    assert!(store.add_cup_medal(a.id, &gold, 100).await.unwrap());
    assert!(!store.add_cup_medal(a.id, &gold, 101).await.unwrap(), "the same tournament once");
    let bronze = Medal { cup: u64::MAX, format: Format::Arena, place: 3 };
    assert!(store.add_cup_medal(a.id, &bronze, 200).await.unwrap());
    assert_eq!(
        store.cup_medals(a.id).await.unwrap(),
        Medals { knockout: [1, 0, 0], arena: [0, 0, 1] }
    );
    assert_eq!(store.cup_medals(b.id).await.unwrap(), Medals::default());
    assert_eq!(store.cup_medals_since(a.id, 150).await.unwrap(), 1);

    // A deleted account loses its claim with its achievements.
    store.delete_account(a.id, 3_000).await.unwrap();
    assert_eq!(store.ladder_claim(a.id).await.unwrap(), None);
    assert_eq!(store.ladder_ticket(a.id, 100).await.unwrap(), None);
    assert_eq!(store.cup_medals(a.id).await.unwrap(), Medals::default());
}

// ---------------------------------------------------------------------------
// The medals of the tournaments against the computer.

/// The game sends a medal once the account is online; the server keeps it
/// once per tournament and answers with the account's count (and the
/// tournament kept, so the game can forget it). A medal that can't be is
/// refused; signed out, nothing is kept.
#[tokio::test]
async fn cup_medals_are_kept_once_per_tournament() {
    use snook_legend_proto::cup::{Format, Medal, Medals};
    let (url, _server) = start().await;
    let mut anon = Client::connect(&url).await;
    anon.send(ClientMsg::CupMedals).await;
    anon.expect("refused", |m| matches!(m, ServerMsg::AuthError { .. })).await;

    let mut c = Client::register(&url, "Medalist").await;
    let medals = |m: &ServerMsg| matches!(m, ServerMsg::CupMedals { .. });
    c.send(ClientMsg::CupMedals).await;
    let ServerMsg::CupMedals { medals: m, kept } = c.expect("medals", medals).await else {
        unreachable!()
    };
    assert_eq!((m, kept), (Medals::default(), None));
    let silver = Medal { cup: 42, format: Format::Knockout, place: 2 };
    for _ in 0..2 {
        // Sent again (the answer was lost): kept once, and said so again.
        c.send(ClientMsg::CupMedal { medal: silver }).await;
        let ServerMsg::CupMedals { medals: m, kept } = c.expect("kept", medals).await else {
            unreachable!()
        };
        assert_eq!((m, kept), (Medals { knockout: [0, 1, 0], arena: [0; 3] }, Some(42)));
    }
    c.send(ClientMsg::CupMedal { medal: Medal { cup: 43, format: Format::Arena, place: 4 } }).await;
    c.expect("refused", |m| matches!(m, ServerMsg::Error { .. })).await;
    c.send(ClientMsg::CupMedal { medal: Medal { cup: 44, format: Format::Arena, place: 1 } }).await;
    let ServerMsg::CupMedals { medals: m, .. } = c.expect("arena", medals).await else {
        unreachable!()
    };
    assert_eq!(m, Medals { knockout: [0, 1, 0], arena: [1, 0, 0] });
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

// ---------------------------------------------------------------------------
// The ladder's reward.

#[tokio::test]
async fn the_reward_needs_a_genuine_win() {
    let (url, server) = start().await;

    // Signed out: nothing.
    let mut anon = Client::connect(&url).await;
    anon.send(ClientMsg::LadderClaim { replay: Replay::new(1, 0, Rules::Brazilian) }).await;
    anon.expect("refused", |m| matches!(m, ServerMsg::AuthError { .. })).await;

    let mut c = Client::register(&url, "Climber").await;
    // Far too long: refused unplayed (and not counted as an attempt).
    let mut long = Replay::new(1, 0, Rules::Brazilian);
    long.strokes = vec![Stroke::TimeFoul; ladder::MAX_STROKES + 1];
    assert_eq!(c.claim(long, 20).await, ClaimResult::Failed { reason: ClaimFailure::TooLong });
    // A rack the server didn't deal: refused unplayed.
    let no_ticket = ClaimResult::Failed { reason: ClaimFailure::NoTicket };
    assert_eq!(c.claim(Replay::new(7, 1, Rules::Brazilian), 20).await, no_ticket);
    // The server's rack, but the player breaking.
    let dealt = c.ticket().await;
    assert_ne!(dealt.seed, 0);
    assert_eq!(dealt.breaker, 1 - ladder::PLAYER_SEAT);
    let player_breaks = Replay::new(dealt.seed, ladder::PLAYER_SEAT, Rules::Brazilian);
    assert_eq!(c.claim(player_breaks, 20).await, no_ticket);
    // Someone else's rack.
    let mut thief = Client::register(&url, "Thief").await;
    assert_eq!(thief.claim(dealt.clone(), 20).await, no_ticket);
    // The game dealt, never started.
    assert_eq!(
        c.claim(dealt.clone(), 20).await,
        ClaimResult::Failed { reason: ClaimFailure::NotFinished }
    );
    // Again at once: too soon.
    assert_eq!(c.claim(dealt, 20).await, ClaimResult::Failed { reason: ClaimFailure::TooSoon });

    // A forged computer stroke (the computer breaks, softly and straight).
    let mut other = Client::register(&url, "Forger").await;
    let mut forged = other.ticket().await;
    forged.strokes.push(Stroke::Shot {
        shot: snook_legend_core::Shot::new(0.0, 1.0),
        called: None,
        cue_ball: None,
        timeout: false,
    });
    assert_eq!(
        other.claim(forged, 60).await,
        ClaimResult::Failed { reason: ClaimFailure::ComputerMismatch }
    );

    // The genuine game, edited after the end: refused.
    let genuine: Replay = serde_json::from_str(FIXTURE).unwrap();
    let mut third = Client::register(&url, "Editor").await;
    deal(&server, "Editor", &genuine).await;
    let mut edited = genuine.clone();
    edited.strokes.push(Stroke::TimeFoul);
    assert_eq!(
        third.claim(edited, 180).await,
        ClaimResult::Failed { reason: ClaimFailure::Illegal }
    );
}

#[tokio::test]
async fn a_genuine_win_earns_the_item_once() {
    let (url, server) = start().await;
    let genuine: Replay = serde_json::from_str(FIXTURE).unwrap();
    let mut c = Client::register(&url, "Champion").await;
    deal(&server, "Champion", &genuine).await;
    let result = c.claim(genuine.clone(), 180).await;
    assert_eq!(result, ClaimResult::Granted { already: false });
    // Told before the verdict: the achievement, and the list with it.
    assert!(c.seen.iter().any(|m| {
        matches!(m, ServerMsg::AchievementUnlocked { code } if code == ladder::ACHIEVEMENT)
    }));
    assert!(c.seen.iter().any(|m| {
        matches!(m, ServerMsg::MyAchievements { codes } if codes.iter().any(|c| c == ladder::ACHIEVEMENT))
    }));
    let me = server.store.user_by_name("Champion").await.unwrap().unwrap();
    assert!(server.store.ladder_claim(me.id).await.unwrap().is_some());
    // The item comes with the achievement: the shop lets them use it.
    let item = snook_legend_proto::shop::item(ladder::REWARD_ITEM).unwrap();
    let codes: Vec<String> =
        server.store.achievements(me.id).await.unwrap().into_iter().map(|(c, _)| c).collect();
    assert!(snook_legend_proto::shop::can_use(item, &[], &codes, false));
    c.send(ClientMsg::Equip { code: ladder::REWARD_ITEM.into() }).await;
    c.expect("equipped", |m| {
        matches!(m, ServerMsg::Wallet { equipped, .. }
            if equipped.iter().any(|(_, code)| code == ladder::REWARD_ITEM))
    })
    .await;

    // Again (another computer, the same account): at once, nothing new.
    let result = c.claim(genuine.clone(), 20).await;
    assert_eq!(result, ClaimResult::Granted { already: true });

    // The same game sent by someone else: the rack was dealt to the
    // champion, not to them.
    let mut friend = Client::register(&url, "Copycat").await;
    assert_eq!(
        friend.claim(genuine, 20).await,
        ClaimResult::Failed { reason: ClaimFailure::NoTicket }
    );
}

/// Deals `replay`'s rack to `name`, as the server does when it is asked
/// for a final (the recorded game has a rack of its own).
async fn deal(server: &Server, name: &str, replay: &Replay) {
    let id = server.store.user_by_name(name).await.unwrap().unwrap().id;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
        as i64;
    server.store.issue_ladder_ticket(id, replay.seed, replay.breaker, now).await.unwrap();
}

// ---------------------------------------------------------------------------
// Ghosts.

#[tokio::test]
async fn ghosts_need_recorded_games() {
    let (url, server) = start().await;
    let mut c = Client::register(&url, "Watcher").await;
    let _p = Client::register(&url, "Pro").await;
    let _q = Client::register(&url, "Rival").await;
    let id = |name: &'static str| {
        let server = server.clone();
        async move { server.store.user_by_name(name).await.unwrap().unwrap().id }
    };
    let (pro, rival) = (id("Pro").await, id("Rival").await);

    // Nobody by that name; nobody at all; too few shots.
    assert_eq!(c.ghost("nobody_here").await, GhostAnswer::NoSuchPlayer);
    assert_eq!(c.ghost("x").await, GhostAnswer::NoSuchPlayer);
    assert_eq!(c.ghost("Pro").await, GhostAnswer::NotEnough { shots: 0, needed: ghost::MIN_SHOTS });

    // A few recorded games.
    let mut at = 1_000;
    let mut shots = 0;
    let mut seed = 0;
    while shots < ghost::MIN_SHOTS as usize + 10 {
        let (replay, winner) = played(seed, Difficulty::Easy, Difficulty::Easy);
        shots += replay.strokes.len() / 2;
        server.store.record_match(&record([pro, rival], replay, winner, at)).await.unwrap();
        at += 100;
        seed += 1;
    }
    // Asked too often in a row: wait (the cache was for zero games).
    let answer = loop {
        match c.ghost("pro").await {
            GhostAnswer::TooSoon => tokio::time::sleep(Duration::from_secs(2)).await,
            other => break other,
        }
    };
    let GhostAnswer::Ready { style } = answer else { panic!("{answer:?}") };
    assert!(style.shots >= ghost::MIN_SHOTS, "{style:?}");
    assert_eq!(style.games, seed as u32);
    assert!(style.pot_rate > 0.0 && style.pot_rate <= 1.0);
    assert!(style.speed > 0.5 && style.speed < 7.0);
    // Only numbers: the style plays.
    let player = style.player();
    let game = snook_legend_core::EightBallGame::new(1, 0, Rules::Official);
    let d = ai::decide_with(&game, &player, 1);
    assert!(d.shot.speed > 0.0);
}

// ---------------------------------------------------------------------------
// The fixture.

/// Records a game won against the ladder's last opponent into
/// `tests/data/ladder_final_win.json`. Run it after changing the AI or the
/// physics (the claim tests then fail on the old file):
/// `cargo test --release -p snook_legend_server --test computer -- --ignored record_the_fixture`
#[test]
#[ignore]
fn record_the_fixture() {
    use snook_legend_core::ai::{AiPlayer, Search, Skill, Style};
    // The person: a flawless, far-sighted player.
    let me = AiPlayer {
        skill: Skill {
            aim_noise_deg: 0.0,
            speed_noise: 0.0,
            break_speed: 7.2,
            search: Search::Wide,
            position_weight: 450.0,
            plays_safe: true,
            banks: true,
        },
        style: Style::NEUTRAL,
    };
    let boss = ladder::final_player();
    for seed in 1..50u64 {
        // The computer breaks: the game has strokes of it to check.
        let mut replay = Replay::new(seed * 7919, 1, Rules::Brazilian);
        let mut game = replay.start();
        let mut computer = 0;
        while game.game_over().is_none() && replay.strokes.len() < 300 {
            let stroke = if game.current_player() == ladder::PLAYER_SEAT {
                ladder::computer_stroke(&game, &me, seed)
            } else {
                computer += 1;
                ladder::computer_stroke(&game, &boss, replay.seed)
            };
            apply(&mut game, &stroke).unwrap();
            replay.strokes.push(stroke);
        }
        let won = game.game_over().is_some_and(|o| o.winner == ladder::PLAYER_SEAT);
        println!(
            "seed {seed}: {} strokes ({computer} by the computer), won {won}",
            replay.strokes.len()
        );
        if won && computer >= 4 && ladder::verify(&replay).is_ok() {
            let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/ladder_final_win.json");
            std::fs::write(path, serde_json::to_string(&replay).unwrap()).unwrap();
            return;
        }
    }
    panic!("no win found");
}

/// The recorded game still passes the check (this version plays it the
/// same way).
#[test]
fn the_fixture_is_a_genuine_win() {
    let replay: Replay = serde_json::from_str(FIXTURE).unwrap();
    let game = ladder::verify(&replay).expect("genuine");
    assert_eq!(game.game_over().map(|o| o.winner), Some(ladder::PLAYER_SEAT));
    // Small enough for one message.
    assert!(FIXTURE.len() < 100 * 1024);
}
