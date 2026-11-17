//! Correspondence ("daily") games: the store on memory (and on PostgreSQL
//! when `SNOOK_LEGEND_TEST_DATABASE_URL` points at a database it may wipe), and
//! the whole flow through real WebSocket clients: challenges, visits, shots
//! refused, turns that run out, resignations, the history and the gating of
//! games too old to know these messages.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use snook_legend_core::stats::ShotStats;
use snook_legend_core::{Difficulty, EightBallGame, GameOverReason, Shot, ai};
use snook_legend_proto::correspondence::{CorrEvent, CorrGame, MAX_SENT_CHALLENGES, TURN_SECS};
use snook_legend_proto::replay::{Replay, Stroke};
use snook_legend_proto::{ClientMsg, PROTOCOL_VERSION, Role, ServerMsg, parse_server};
use snook_legend_server::lounge::Rules;
use snook_legend_server::store::{AsyncGame, MatchRecord, NewAsyncGame, NewUser, Store};
use snook_legend_server::{Config, Server, serve};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

// ---------------------------------------------------------------------------
// The store.

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

async fn exercise(store: Store) {
    let t = 1_700_000_000;
    let a = store.create_user(&new_user("ana")).await.unwrap().unwrap();
    let b = store.create_user(&new_user("bia")).await.unwrap().unwrap();
    let c = store.create_user(&new_user("caio")).await.unwrap().unwrap();

    // Challenges: one per pair and direction.
    let ch = store.create_async_challenge(a.id, b.id, true, t).await.unwrap().unwrap();
    assert!(store.create_async_challenge(a.id, b.id, false, t).await.unwrap().is_none());
    let back = store.create_async_challenge(b.id, a.id, false, t + 1).await.unwrap().unwrap();
    let old = store.create_async_challenge(c.id, a.id, false, t - 10_000).await.unwrap().unwrap();
    let got = store.async_challenge(ch).await.unwrap().unwrap();
    assert_eq!((got.from, got.to, got.rated, got.created_at), (a.id, b.id, true, t));
    let of_a: Vec<i64> =
        store.async_challenges_of(a.id).await.unwrap().iter().map(|c| c.id).collect();
    assert_eq!(of_a, vec![old, ch, back], "oldest first, sent and received");
    assert_eq!(store.async_challenges_of(c.id).await.unwrap().len(), 1);
    assert!(store.delete_async_challenge(back).await.unwrap());
    assert!(!store.delete_async_challenge(back).await.unwrap());
    let purged = store.purge_async_challenges(t - 100).await.unwrap();
    assert_eq!(purged.iter().map(|c| c.id).collect::<Vec<_>>(), vec![old]);
    assert!(store.async_challenge(old).await.unwrap().is_none());

    // Accepting starts the game and takes the challenge away.
    let new = NewAsyncGame { seed: u64::MAX - 3, breaker: 1, deadline: t + 500, at: t };
    let g = store.accept_async_challenge(ch, &new).await.unwrap().unwrap();
    assert!(store.accept_async_challenge(ch, &new).await.unwrap().is_none(), "only once");
    assert!(store.async_challenge(ch).await.unwrap().is_none());
    assert_eq!(g.players, [a.id, b.id]);
    assert!(g.rated && g.is_active());
    assert_eq!(
        (g.replay.seed, g.replay.breaker, g.turn, g.deadline),
        (u64::MAX - 3, 1, 1, t + 500)
    );
    assert_eq!((g.seen, g.left, g.groups), ([0, 0], [7, 7], [None, None]));
    assert_eq!(g.seat_of(b.id), Some(1));
    assert_eq!(store.async_game(g.id).await.unwrap().unwrap(), g);

    // A change is saved only against the version it was made on.
    let mut next = g.clone();
    next.replay.strokes.push(Stroke::Shot {
        shot: Shot::new(0.5, 3.0),
        called: None,
        cue_ball: None,
        timeout: false,
    });
    next.turn = 0;
    next.deadline = t + 900;
    next.seen = [0, 1];
    next.groups = [Some(snook_legend_core::Group::Stripes), Some(snook_legend_core::Group::Solids)];
    next.left = [6, 7];
    next.updated_at = t + 5;
    assert!(store.update_async_game(&next, 0).await.unwrap());
    assert!(!store.update_async_game(&next, 0).await.unwrap(), "stale version");
    let saved = store.async_game(g.id).await.unwrap().unwrap();
    assert_eq!(saved, next);
    store.set_async_seen(g.id, 0, 1).await.unwrap();
    store.set_async_seen(g.id, 1, 0).await.unwrap();
    assert_eq!(store.async_game(g.id).await.unwrap().unwrap().seen, [1, 1], "never goes back");

    // Turns that ran out.
    assert!(store.expired_async_games(t + 900, 10).await.unwrap().is_empty());
    let due = store.expired_async_games(t + 901, 10).await.unwrap();
    assert_eq!(due.iter().map(|g| g.id).collect::<Vec<_>>(), vec![g.id]);

    // A second game, for the lists.
    let ch2 = store.create_async_challenge(c.id, a.id, false, t).await.unwrap().unwrap();
    let new2 = NewAsyncGame { seed: 7, breaker: 0, deadline: t + 2000, at: t + 1 };
    let g2 = store.accept_async_challenge(ch2, &new2).await.unwrap().unwrap();
    let ids = |list: Vec<AsyncGame>| list.iter().map(|g| g.id).collect::<Vec<_>>();
    assert_eq!(ids(store.async_games_of(a.id, 5).await.unwrap()), vec![g.id, g2.id]);
    assert_eq!(ids(store.async_games_of(b.id, 5).await.unwrap()), vec![g.id]);

    // The end: recorded as a match (history, ratings) with the game.
    let mut end = saved.clone();
    end.replay.strokes.push(Stroke::Abandon { seat: 0 });
    end.timeout = true;
    end.seen = [2, 1];
    let record = MatchRecord {
        players: end.players,
        winner: 1,
        reason: GameOverReason::Abandoned,
        rated: true,
        delta: [-12, 12],
        ratings_after: [1188, 1212],
        shots: 1,
        tournament: None,
        eight_win: false,
        runout: false,
        shutout: false,
        ended_at: t + 950,
        replay: Some(end.replay.clone()),
        stats: [ShotStats { shots: 1, ..Default::default() }, ShotStats::default()],
        abandoned: None,
    };
    assert!(store.finish_async_game(&end, 0, &record).await.unwrap().is_none(), "stale version");
    let ([ua, ub], match_id) = store.finish_async_game(&end, 1, &record).await.unwrap().unwrap();
    assert_eq!((ua.unwrap().rating, ub.unwrap().rating), (1188, 1212));
    assert!(store.finish_async_game(&end, 2, &record).await.unwrap().is_none(), "already over");
    let done = store.async_game(g.id).await.unwrap().unwrap();
    assert!(!done.is_active());
    assert_eq!(done.finished_at, Some(t + 950));
    assert_eq!(
        (done.winner, done.reason.as_deref(), done.timeout),
        (Some(1), Some("abandoned"), true)
    );
    assert_eq!(
        (done.delta, done.ratings, done.match_id),
        ([-12, 12], [1188, 1212], Some(match_id))
    );
    assert_eq!(done.replay.strokes.len(), 2);
    assert!(!store.update_async_game(&done, 2).await.unwrap(), "a finished game stays finished");
    assert!(store.expired_async_games(t + 10_000, 10).await.unwrap().iter().all(|x| x.id != g.id));
    let h = store.history(a.id, 5).await.unwrap();
    assert_eq!(h.len(), 1);
    assert!(h[0].correspondence && h[0].replay && !h[0].won);
    assert_eq!(h[0].match_id, match_id);
    let replay = store.replay(match_id).await.unwrap().unwrap();
    assert_eq!(replay.replay, end.replay);

    // Lists: active first, then the latest finished.
    assert_eq!(ids(store.async_games_of(a.id, 5).await.unwrap()), vec![g2.id, g.id]);
    assert_eq!(ids(store.async_games_of(a.id, 0).await.unwrap()), vec![g2.id]);
    assert_eq!(ids(store.async_games_of(b.id, 0).await.unwrap()), Vec::<i64>::new());

    // A deleted account takes its challenges away.
    store.create_async_challenge(b.id, c.id, false, t).await.unwrap().unwrap();
    store.delete_account(c.id, t).await.unwrap();
    assert!(store.async_challenges_of(b.id).await.unwrap().is_empty());
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
// The server.

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

const PASSWORD: &str = "secret123";

struct Client {
    ws: Ws,
}

impl Client {
    async fn send(&mut self, msg: ClientMsg) {
        self.ws.send(Message::Text(msg.to_json().into())).await.unwrap();
    }

    /// The next message, or `None` after `left`.
    async fn next(&mut self, left: Duration) -> Option<ServerMsg> {
        let deadline = tokio::time::Instant::now() + left;
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(left, self.ws.next()).await {
                Ok(Some(Ok(Message::Text(t)))) => return Some(parse_server(t.as_str()).unwrap()),
                Ok(Some(Ok(_))) => {}
                Ok(_) => panic!("connection closed"),
                Err(_) => return None,
            }
        }
    }

    /// Reads messages until one matches `pred`.
    async fn expect<F: Fn(&ServerMsg) -> bool>(&mut self, what: &str, pred: F) -> ServerMsg {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            match self.next(left).await {
                Some(m) if pred(&m) => return m,
                Some(_) => {}
                None => panic!("timed out waiting for {what}"),
            }
        }
    }

    async fn error(&mut self) -> String {
        match self.expect("error", |m| matches!(m, ServerMsg::Error { .. })).await {
            ServerMsg::Error { message } => message,
            _ => unreachable!(),
        }
    }

    async fn list(
        &mut self,
    ) -> (Vec<CorrGame>, Vec<snook_legend_proto::correspondence::CorrChallenge>) {
        match self.expect("list", |m| matches!(m, ServerMsg::CorrList { .. })).await {
            ServerMsg::CorrList { games, challenges } => (games, challenges),
            _ => unreachable!(),
        }
    }

    async fn notice(&mut self) -> (i64, String, CorrEvent) {
        match self.expect("notice", |m| matches!(m, ServerMsg::CorrNotice { .. })).await {
            ServerMsg::CorrNotice { id, opponent, event } => (id, opponent, event),
            _ => unreachable!(),
        }
    }

    async fn open(&mut self, id: i64) -> (CorrGame, Replay, u32) {
        self.send(ClientMsg::CorrOpen { id }).await;
        match self.expect("state", |m| matches!(m, ServerMsg::CorrState { .. })).await {
            ServerMsg::CorrState { game, replay, seen } => (game, replay, seen),
            _ => unreachable!(),
        }
    }
}

struct Rig {
    url: String,
    server: Arc<Server>,
    store: Store,
}

async fn rig() -> Rig {
    let store = Store::memory();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let rules = Rules { max_connections_per_ip: 100, ..Rules::default() };
    let config = Config { bind: addr, rules, ..Config::default() };
    let server = Server::with_store(config, store.share().unwrap()).await;
    tokio::spawn(serve(listener, server.clone()));
    Rig { url: format!("ws://{addr}/ws"), server, store }
}

impl Rig {
    /// A new account, signed in with a game of protocol `version`.
    async fn player_v(&self, name: &str, version: u32) -> Client {
        let hash = self.server.hash_password(PASSWORD).await.unwrap();
        let user = NewUser { password_hash: &hash, ..new_user(name) };
        self.store.create_user(&user).await.unwrap().unwrap();
        let (ws, _) = connect_async(&self.url).await.unwrap();
        let mut c = Client { ws };
        c.send(ClientMsg::Hello { version }).await;
        c.send(ClientMsg::Login { username: name.into(), password: PASSWORD.into() }).await;
        c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
        c
    }

    async fn player(&self, name: &str) -> Client {
        let mut c = self.player_v(name, PROTOCOL_VERSION).await;
        // The list comes after signing in.
        let (games, challenges) = c.list().await;
        assert!(games.is_empty() && challenges.is_empty());
        c
    }

    /// `a` challenges `b`, who accepts: the game's id.
    async fn start(&self, a: &mut Client, b: &mut Client, b_name: &str, rated: bool) -> i64 {
        a.send(ClientMsg::CorrChallenge { to: b_name.into(), rated }).await;
        let (id, _, event) = b.notice().await;
        assert_eq!(event, CorrEvent::Challenged { rated });
        a.send(ClientMsg::CorrAnswer { id, accept: true }).await;
        assert_eq!(a.error().await, "That challenge is no longer valid.", "not theirs to accept");
        b.send(ClientMsg::CorrAnswer { id, accept: true }).await;
        let (gid, _, event) = a.notice().await;
        assert_eq!(event, CorrEvent::Accepted);
        gid
    }
}

/// The table of a game, rebuilt from what the server sends.
fn table(replay: &Replay) -> EightBallGame {
    replay.play(None).unwrap().0
}

fn shot_of(game: &EightBallGame, k: u64, id: i64) -> ClientMsg {
    let d = ai::decide(game, Difficulty::Hard, k);
    ClientMsg::CorrShoot {
        id,
        turn: game.shots_taken(),
        shot: d.shot,
        called_pocket: d.called_pocket,
        cue_ball: d.cue_ball,
    }
}

#[tokio::test]
async fn a_rated_game_is_played_turn_by_turn_to_the_end() {
    let rig = rig().await;
    let mut ana = rig.player("ana").await;
    let mut bia = rig.player("bia").await;
    let id = rig.start(&mut ana, &mut bia, "bia", true).await;
    let (games, challenges) = ana.list().await;
    assert!(challenges.is_empty());
    assert_eq!(games.len(), 1);
    let g = &games[0];
    assert_eq!((g.id, g.seat, g.rated, g.strokes, g.result.is_some()), (id, 0, true, 0, false));
    assert_eq!(g.players[1].username, "bia");
    assert!(g.deadline > snook_legend_server::now() + TURN_SECS - 60);

    let (info, replay, seen) = ana.open(id).await;
    assert_eq!(seen, 0);
    let mut game = table(&replay);
    let seats = [info.seat, 1 - info.seat];
    let mut k = 0u64;
    loop {
        assert!(k < 400, "the game did not finish");
        let mover_is_ana = game.current_player() == seats[0];
        let (mover, other) = if mover_is_ana { (&mut ana, &mut bia) } else { (&mut bia, &mut ana) };
        // The one waiting can't shoot, nor can a stale shot land.
        let turn = game.shots_taken();
        let shot = Shot::new(0.3, 2.0);
        other
            .send(ClientMsg::CorrShoot { id, turn, shot, called_pocket: None, cue_ball: None })
            .await;
        let ServerMsg::CorrShotRejected { reason, game: back, .. } =
            other.expect("rejected", |m| matches!(m, ServerMsg::CorrShotRejected { .. })).await
        else {
            unreachable!()
        };
        assert_eq!(reason, "It's not your turn.");
        assert_eq!(back, game);
        if k == 3 {
            let stale = ClientMsg::CorrShoot {
                id,
                turn: game.shots_taken() + 1,
                shot,
                called_pocket: None,
                cue_ball: None,
            };
            mover.send(stale).await;
            let ServerMsg::CorrShotRejected { reason, .. } =
                mover.expect("stale", |m| matches!(m, ServerMsg::CorrShotRejected { .. })).await
            else {
                unreachable!()
            };
            assert_eq!(reason, "That shot was for an earlier turn.");
        }
        let msg = shot_of(&game, k, id);
        let ClientMsg::CorrShoot { shot, called_pocket, cue_ball, .. } = msg.clone() else {
            unreachable!()
        };
        mover.send(msg).await;
        let ServerMsg::CorrShot { end, turn, .. } = mover
            .expect("shot", |m| {
                matches!(m, ServerMsg::CorrShot { .. } | ServerMsg::CorrShotRejected { .. })
            })
            .await
        else {
            panic!("shot {k} refused")
        };
        assert_eq!(turn, game.shots_taken());
        // The same physics on both sides.
        let mut local = game.clone();
        if let Some(p) = cue_ball {
            local.place_cue_ball(p);
        }
        local.play_shot(&shot, called_pocket).unwrap();
        assert_eq!(local, end);
        let passed = end.current_player() != game.current_player();
        game = end;
        k += 1;
        if game.game_over().is_some() {
            break;
        }
        if passed {
            // A whole visit is over: the other player hears it is their turn.
            let (nid, opponent, event) = other.notice().await;
            assert_eq!((nid, event), (id, CorrEvent::YourTurn));
            assert_eq!(opponent, if mover_is_ana { "ana" } else { "bia" });
        }
    }
    let over = game.game_over().unwrap();
    let ana_won = over.winner == seats[0];
    for (c, me_won) in [(&mut ana, ana_won), (&mut bia, !ana_won)] {
        let (nid, _, event) = c.notice().await;
        assert_eq!(nid, id);
        assert!(
            matches!(event, CorrEvent::Ended { won, timeout: false, .. } if won == me_won),
            "{event:?}"
        );
        let (games, _) = c.list().await;
        let result = games[0].result.clone().expect("the result");
        assert_eq!(result.winner, over.winner);
        assert!(result.delta[over.winner] > 0 && result.match_id > 0);
    }
    // In the history, with the replay; the ratings moved.
    ana.send(ClientMsg::Profile { username: "ana".into() }).await;
    let ServerMsg::Profile { profile } =
        ana.expect("profile", |m| matches!(m, ServerMsg::Profile { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(profile.history.len(), 1);
    let h = &profile.history[0];
    assert!(h.correspondence && h.replay && h.rated);
    assert_eq!(h.won, ana_won);
    assert_eq!(profile.stats.games, 1);
    assert_ne!(profile.player.rating, 1200);
    // Nothing more can be played.
    ana.send(ClientMsg::CorrResign { id }).await;
    assert_eq!(ana.error().await, "This game is over.");
    let (info, replay, _) = ana.open(id).await;
    assert!(info.result.is_some());
    assert_eq!(table(&replay), game);
}

#[tokio::test]
async fn challenges_can_be_declined_withdrawn_and_are_limited() {
    let rig = rig().await;
    let mut ana = rig.player("ana").await;
    let mut bia = rig.player("bia").await;

    ana.send(ClientMsg::CorrChallenge { to: "ana".into(), rated: true }).await;
    assert_eq!(ana.error().await, "You can't challenge yourself.");
    ana.send(ClientMsg::CorrChallenge { to: "nobody_here".into(), rated: true }).await;
    assert_eq!(ana.error().await, "No such player.");

    // Declined.
    ana.send(ClientMsg::CorrChallenge { to: "bia".into(), rated: false }).await;
    ana.expect("sent", |m| matches!(m, ServerMsg::Notice { .. })).await;
    let (id, from, _) = bia.notice().await;
    assert_eq!(from, "ana");
    let (_, challenges) = bia.list().await;
    assert_eq!(challenges.len(), 1);
    assert_eq!((challenges[0].from.username.as_str(), challenges[0].rated), ("ana", false));
    ana.send(ClientMsg::CorrChallenge { to: "bia".into(), rated: true }).await;
    assert_eq!(ana.error().await, "You already challenged that player.");
    bia.send(ClientMsg::CorrChallenge { to: "ana".into(), rated: true }).await;
    assert_eq!(bia.error().await, "That player already challenged you: answer their challenge.");
    bia.send(ClientMsg::CorrAnswer { id, accept: false }).await;
    assert_eq!(ana.notice().await.2, CorrEvent::Declined);
    let (games, challenges) = ana.list().await;
    assert!(games.is_empty() && challenges.is_empty());

    // Withdrawn (only by whoever sent it).
    ana.send(ClientMsg::CorrChallenge { to: "bia".into(), rated: true }).await;
    let (id, _, _) = bia.notice().await;
    bia.send(ClientMsg::CorrCancel { id }).await;
    assert_eq!(bia.error().await, "That challenge is no longer valid.");
    ana.send(ClientMsg::CorrCancel { id }).await;
    assert_eq!(bia.notice().await.2, CorrEvent::Withdrawn);
    bia.send(ClientMsg::CorrAnswer { id, accept: true }).await;
    assert_eq!(bia.error().await, "That challenge is no longer valid.");

    // Blocked players can't be challenged.
    bia.send(ClientMsg::Block { username: "ana".into() }).await;
    bia.send(ClientMsg::Friends).await;
    bia.expect("friends", |m| matches!(m, ServerMsg::Friends { .. })).await;
    ana.send(ClientMsg::CorrChallenge { to: "bia".into(), rated: true }).await;
    assert_eq!(ana.error().await, "That player is not accepting challenges.");
    bia.send(ClientMsg::Unblock { username: "ana".into() }).await;
    bia.send(ClientMsg::Friends).await;
    bia.expect("friends", |m| matches!(m, ServerMsg::Friends { .. })).await;

    // One game at a time against the same player.
    let gid = rig.start(&mut ana, &mut bia, "bia", true).await;
    assert!(gid > 0);
    ana.send(ClientMsg::CorrChallenge { to: "bia".into(), rated: true }).await;
    assert_eq!(ana.error().await, "You already have a game going on against that player.");

    // A few challenges waiting at once, and not too quickly.
    let mut others = Vec::new();
    for k in 0..=MAX_SENT_CHALLENGES {
        let name = format!("p{k}");
        others.push(rig.player(&name).await);
    }
    let mut ids = Vec::new();
    for (k, other) in others.iter_mut().enumerate().take(MAX_SENT_CHALLENGES) {
        ana.send(ClientMsg::CorrChallenge { to: format!("p{k}"), rated: false }).await;
        ana.expect("sent", |m| matches!(m, ServerMsg::Notice { .. })).await;
        ids.push(other.notice().await.0);
    }
    let to = format!("p{MAX_SENT_CHALLENGES}");
    ana.send(ClientMsg::CorrChallenge { to, rated: false }).await;
    assert_eq!(ana.error().await, "You have too many challenges waiting for an answer.");
    // Withdrawing and sending again and again is not a way round it.
    let mut limited = false;
    for round in 0..12 {
        let k = round % MAX_SENT_CHALLENGES;
        ana.send(ClientMsg::CorrCancel { id: ids[k] }).await;
        ana.send(ClientMsg::CorrChallenge { to: format!("p{k}"), rated: false }).await;
        let m = ana
            .expect("answer", |m| matches!(m, ServerMsg::Notice { .. } | ServerMsg::Error { .. }))
            .await;
        if let ServerMsg::Error { message } = m {
            assert_eq!(message, "You're sending challenges too quickly.");
            limited = true;
            break;
        }
        // The withdrawal, then the new challenge.
        assert_eq!(others[k].notice().await.2, CorrEvent::Withdrawn);
        ids[k] = others[k].notice().await.0;
    }
    assert!(limited, "challenges are rate limited");
}

#[tokio::test]
async fn resigning_ends_the_game_and_old_games_never_hear_of_it() {
    let rig = rig().await;
    let mut ana = rig.player("ana").await;
    let mut bia = rig.player("bia").await;
    let id = rig.start(&mut ana, &mut bia, "bia", false).await;
    // Resigning works whoever's turn it is.
    bia.send(ClientMsg::CorrResign { id }).await;
    let (_, _, event) = ana.notice().await;
    assert_eq!(
        event,
        CorrEvent::Ended { won: true, reason: GameOverReason::Resignation, timeout: false }
    );
    bia.notice().await;
    let (games, _) = bia.list().await;
    let r = games[0].result.clone().unwrap();
    assert_eq!((r.winner, r.delta), (0, [0, 0]), "casual: ratings unchanged");
    let (_, replay, _) = bia.open(id).await;
    assert_eq!(replay.strokes.last(), Some(&Stroke::Resign { seat: 1 }));
}

#[tokio::test]
async fn a_turn_that_runs_out_loses_the_game() {
    let rig = rig().await;
    let mut ana = rig.player("ana").await;
    let mut bia = rig.player("bia").await;
    let id = rig.start(&mut ana, &mut bia, "bia", true).await;
    let (info, _, _) = ana.open(id).await;
    let to_move = info.turn;
    // Not yet.
    snook_legend_server::correspondence::sweep(&rig.server, info.deadline - 1).await;
    assert!(rig.store.async_game(id).await.unwrap().unwrap().is_active());
    // The timer finds it.
    snook_legend_server::correspondence::sweep(&rig.server, info.deadline + 1).await;
    let g = rig.store.async_game(id).await.unwrap().unwrap();
    assert!(!g.is_active() && g.timeout);
    assert_eq!(g.winner, Some(1 - to_move));
    assert_eq!(g.replay.strokes, vec![Stroke::Abandon { seat: to_move }]);
    for (c, seat) in [(&mut ana, 0), (&mut bia, 1)] {
        let (_, _, event) = c.notice().await;
        assert_eq!(
            event,
            CorrEvent::Ended {
                won: seat != to_move,
                reason: GameOverReason::Abandoned,
                timeout: true
            }
        );
    }
    // It doesn't count as leaving a game.
    let loser = rig.store.user_by_name(if to_move == 0 { "ana" } else { "bia" }).await;
    let loser = loser.unwrap().unwrap();
    assert_eq!((loser.games, loser.abandons), (1, 0));
    assert!(loser.rating < 1200);
}

#[tokio::test]
async fn a_turn_that_ran_out_is_settled_when_the_game_is_opened() {
    let rig = rig().await;
    let mut ana = rig.player("ana").await;
    let mut bia = rig.player("bia").await;
    let id = rig.start(&mut ana, &mut bia, "bia", false).await;
    // The deadline passes while the server is not looking.
    let mut g = rig.store.async_game(id).await.unwrap().unwrap();
    g.deadline = snook_legend_server::now() - 5;
    assert!(rig.store.update_async_game(&g, 0).await.unwrap());
    let mover = if g.turn == 0 { &mut ana } else { &mut bia };
    // A shot is refused: too late.
    let game = EightBallGame::new(g.replay.seed, g.replay.breaker);
    mover.send(shot_of(&game, 0, id)).await;
    let ServerMsg::CorrShotRejected { reason, .. } =
        mover.expect("late", |m| matches!(m, ServerMsg::CorrShotRejected { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(reason, "Your time for this turn ran out.");
    let (info, _, _) = ana.open(id).await;
    let result = info.result.expect("over");
    assert!(result.timeout);
    assert_eq!(result.winner, 1 - g.turn);
}

#[tokio::test]
async fn opening_a_game_shows_what_was_not_seen_yet() {
    let rig = rig().await;
    let mut ana = rig.player("ana").await;
    let mut bia = rig.player("bia").await;
    let id = rig.start(&mut ana, &mut bia, "bia", false).await;
    let (info, replay, _) = ana.open(id).await;
    let mut game = table(&replay);
    let breaker_is_ana = info.turn == info.seat;
    let (mover, watcher) = if breaker_is_ana { (&mut ana, &mut bia) } else { (&mut bia, &mut ana) };
    // The breaker plays until the turn passes.
    let mut k = 0;
    loop {
        mover.send(shot_of(&game, k, id)).await;
        let ServerMsg::CorrShot { end, .. } =
            mover.expect("shot", |m| matches!(m, ServerMsg::CorrShot { .. })).await
        else {
            unreachable!()
        };
        let passed = end.current_player() != game.current_player();
        game = end;
        k += 1;
        if passed || game.game_over().is_some() {
            break;
        }
    }
    assert_eq!(watcher.notice().await.2, CorrEvent::YourTurn);
    let (games, _) = watcher.list().await;
    assert_eq!(games[0].unseen, k as u32);
    let (_, replay, seen) = watcher.open(id).await;
    assert_eq!((seen, replay.strokes.len()), (0, k as usize));
    let (info, _, seen) = watcher.open(id).await;
    assert_eq!((seen, info.unseen), (k as u32, 0), "seen now");
}
