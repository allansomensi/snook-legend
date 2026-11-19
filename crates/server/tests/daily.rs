//! The daily challenge end to end: attempts, the server's own scoring, the
//! day's table and the coins for clearing it.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use snook_legend_core::{BallId, Difficulty, Shot, ai};
use snook_legend_proto::daily::{self, Refused};
use snook_legend_proto::{ClientMsg, PROTOCOL_VERSION, Rules, ServerMsg, parse_server};
use snook_legend_server::lounge::Settings;
use snook_legend_server::{Config, Server, serve};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn start() -> (String, std::sync::Arc<Server>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let settings = Settings { max_connections_per_ip: 100, ..Settings::default() };
    let config = Config { bind: addr, settings, ..Config::default() };
    let server = Server::new(config).await.unwrap();
    tokio::spawn(serve(listener, server.clone()));
    (format!("ws://{addr}/ws"), server)
}

struct Client(Ws);

impl Client {
    async fn register(url: &str, name: &str) -> Client {
        let (ws, _) = connect_async(url).await.unwrap();
        let mut c = Client(ws);
        c.send(ClientMsg::Hello { version: PROTOCOL_VERSION, rules: Rules::Brazilian }).await;
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

    async fn send(&mut self, msg: ClientMsg) {
        self.0.send(Message::Text(msg.to_json().into())).await.unwrap();
    }

    async fn expect<F: Fn(&ServerMsg) -> bool>(&mut self, what: &str, pred: F) -> ServerMsg {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            let msg = tokio::time::timeout(left, self.0.next())
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

    async fn info(&mut self) -> daily::Info {
        self.send(ClientMsg::Daily).await;
        match self.expect("daily", |m| matches!(m, ServerMsg::Daily { .. })).await {
            ServerMsg::Daily { info } => info,
            _ => unreachable!(),
        }
    }

    async fn start(&mut self) -> Result<(i64, u64), Refused> {
        self.send(ClientMsg::DailyStart).await;
        match self
            .expect("started", |m| {
                matches!(m, ServerMsg::DailyStarted { .. } | ServerMsg::DailyResult { .. })
            })
            .await
        {
            ServerMsg::DailyStarted { day, seed } => Ok((day, seed)),
            ServerMsg::DailyResult { result: Err(e), .. } => Err(e),
            other => panic!("{other:?}"),
        }
    }

    async fn submit(
        &mut self,
        day: i64,
        strokes: Vec<Shot>,
    ) -> (Result<daily::Outcome, Refused>, i64) {
        self.send(ClientMsg::DailySubmit { day, strokes }).await;
        match self.expect("result", |m| matches!(m, ServerMsg::DailyResult { .. })).await {
            ServerMsg::DailyResult { result, coins, .. } => (result, coins),
            _ => unreachable!(),
        }
    }
}

/// The computer's visit from the day's position: its strokes until one
/// doesn't keep the table (or the table is clear).
fn computer_visit(seed: u64) -> Vec<Shot> {
    let mut g = daily::game(seed);
    let mut strokes = Vec::new();
    for k in 0..=daily::MAX_BALLS {
        let shot = ai::decide(&g, Difficulty::Hard, k as u64).shot;
        strokes.push(shot);
        let report = g.play_shot(&shot, None).unwrap();
        let left = g.balls().iter().filter(|b| b.in_play() && b.id != BallId::CUE).count();
        if !report.continues || left == 0 {
            break;
        }
    }
    strokes
}

/// A stroke that touches nothing (a foul: the visit ends at once).
fn a_miss(seed: u64) -> Shot {
    let g = daily::game(seed);
    let cue = g.ball(BallId::CUE).xy();
    let yaw = (0..360)
        .map(|d| (d as f64).to_radians())
        .find(|&yaw| {
            let dir = glam::DVec2::new(yaw.cos(), yaw.sin());
            g.balls().iter().filter(|b| b.in_play() && b.id != BallId::CUE).all(|b| {
                let rel = b.xy() - cue;
                let along = rel.dot(dir);
                along < 0.0 || (rel - dir * along).length() > 0.08
            })
        })
        .expect("a clear direction");
    Shot::new(yaw, 1.0)
}

#[tokio::test]
async fn attempts_are_scored_by_the_server() {
    let (url, server) = start().await;
    let mut a = Client::register(&url, "alice").await;
    let mut b = Client::register(&url, "bob").await;

    let info = a.info().await;
    assert_eq!(info.attempts_left, daily::ATTEMPTS);
    assert!((daily::MIN_BALLS as u32..=daily::MAX_BALLS as u32).contains(&info.balls));
    assert!(info.board.is_empty() && info.best.is_none() && !info.open);

    // Nothing to send without an attempt under way.
    let (result, _) = a.submit(info.day, vec![]).await;
    assert_eq!(result, Err(Refused::NotStarted));

    // A miss: no balls, one attempt gone.
    let (day, seed) = a.start().await.unwrap();
    assert_eq!(day, info.day);
    assert!(a.info().await.open);
    let (result, coins) = a.submit(day, vec![a_miss(seed)]).await;
    assert_eq!(result, Ok(daily::Outcome { balls: 0, cleared: false }));
    assert_eq!(coins, 0);
    let info = a.info().await;
    assert_eq!((info.attempts_left, info.best, info.rank), (daily::ATTEMPTS - 1, Some(0), Some(1)));
    // Experience for trying, even with nothing made.
    let alice = server.store.user_by_name("alice").await.unwrap().unwrap();
    assert_eq!(alice.xp, snook_legend_proto::xp::DAILY_ATTEMPT as i64);
    // The attempt is closed: sending it again counts nothing.
    let (result, _) = a.submit(day, vec![a_miss(seed)]).await;
    assert_eq!(result, Err(Refused::NotStarted));

    // The computer's visit, as the server plays it again.
    let (_, seed_b) = b.start().await.unwrap();
    assert_eq!(seed_b, seed, "the same table for everyone");
    let visit = computer_visit(seed);
    let expected = daily::score(seed, &visit).unwrap();
    let (result, coins) = b.submit(day, visit).await;
    assert_eq!(result, Ok(expected));
    assert_eq!(coins, if expected.cleared { daily::CLEAR_COINS } else { 0 });
    let info = b.info().await;
    assert_eq!(info.players, 2);
    assert_eq!(info.board[0].balls, expected.balls);
    if expected.balls > 0 {
        assert_eq!(info.board[0].username, "bob");
        assert_eq!(info.rank, Some(1));
    }

    // An edited attempt (a stroke after the end) is refused, and still
    // uses the attempt.
    let (_, _) = a.start().await.unwrap();
    let (result, _) = a.submit(day, vec![a_miss(seed), a_miss(seed)]).await;
    assert_eq!(result, Err(Refused::BadStroke));
    assert_eq!(a.info().await.attempts_left, 1);
    a.start().await.unwrap();
    assert_eq!(a.start().await, Err(Refused::NoAttemptsLeft));

    // Yesterday's winner gets a trophy (the store, a day later).
    let bob = server.store.user_by_name("bob").await.unwrap().unwrap();
    let wins = server.store.daily_wins(bob.id, day + 1).await.unwrap();
    if expected.balls > 0 {
        assert_eq!(wins, vec![(day, 2)]);
    }
    assert!(server.store.daily_wins(bob.id, day).await.unwrap().is_empty(), "today isn't over");
}

#[tokio::test]
async fn drills_keep_the_best_and_the_challenge_rating() {
    use snook_legend_proto::drills;
    let (url, server) = start().await;
    let mut a = Client::register(&url, "alice").await;
    a.send(ClientMsg::Drills).await;
    let ServerMsg::Drills { info } =
        a.expect("drills", |m| matches!(m, ServerMsg::Drills { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(info.drills.len(), drills::DRILLS.len());
    assert_eq!((info.rating, info.stars, info.rank), (0, 0, None));

    // A session is started by the server: its tries' cue balls lie where it
    // says (a stroke that worked once is no answer to every try).
    a.send(ClientMsg::DrillStart { code: "straight".into() }).await;
    let ServerMsg::DrillStarted { session, .. } =
        a.expect("session", |m| matches!(m, ServerMsg::DrillStarted { .. })).await
    else {
        unreachable!()
    };
    assert_ne!(session, 0);
    // Straight at the 1, from where each try's cue ball lies.
    let d = drills::drill("straight").unwrap();
    let pot = |k: u32| {
        let g = drills::game_for_session_try(d, session, k);
        let to = g.ball(BallId(1)).xy() - g.ball(BallId::CUE).xy();
        (0..400)
            .flat_map(|j| {
                [1.5, 2.2, 3.0]
                    .map(|v| Shot::new(to.y.atan2(to.x) + (j as f64 - 200.0) * 0.0001, v))
            })
            .find(|s| drills::try_shot(d, &g, s) == Some(true))
            .expect("a way to pot it")
    };
    let strokes: Vec<Shot> = (0..d.tries).map(pot).collect();
    let shot = strokes[0];
    let expected = drills::score_session("straight", session, &strokes).unwrap();
    assert_eq!(expected, d.tries);
    // Without the session, or with someone else's number, nothing counts.
    a.send(ClientMsg::DrillSubmit {
        code: "straight".into(),
        session: session ^ 1,
        strokes: strokes.clone(),
    })
    .await;
    a.expect("no session", |m| {
        matches!(m, ServerMsg::DrillResult { result: Err(drills::Refused::NoSession), .. })
    })
    .await;
    a.send(ClientMsg::DrillSubmit { code: "straight".into(), session, strokes }).await;
    let ServerMsg::DrillResult { result, .. } =
        a.expect("result", |m| matches!(m, ServerMsg::DrillResult { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(result, Ok(expected));
    let ServerMsg::Drills { info } =
        a.expect("drills", |m| matches!(m, ServerMsg::Drills { .. })).await
    else {
        unreachable!()
    };
    let mine = info.drills.iter().find(|x| x.code == "straight").unwrap();
    assert_eq!(mine.best, Some(expected));
    assert_eq!(mine.board[0].username, "alice");
    let stars = drills::stars(d, expected);
    assert_eq!(info.stars, stars);
    assert_eq!(info.rating, stars * 100 + expected);
    assert_eq!(info.rank, Some(1));
    assert_eq!(info.leaders[0].username, "alice");
    // New stars give experience.
    let alice = server.store.user_by_name("alice").await.unwrap().unwrap();
    assert_eq!(alice.xp, (stars * snook_legend_proto::xp::DRILL_STAR) as i64);

    // A session counts once.
    a.send(ClientMsg::DrillSubmit { code: "straight".into(), session, strokes: vec![] }).await;
    a.expect("used", |m| {
        matches!(m, ServerMsg::DrillResult { result: Err(drills::Refused::NoSession), .. })
    })
    .await;
    // A worse session keeps the best; a bad one is refused.
    let s1 = drill_session(&mut a, "straight").await;
    a.send(ClientMsg::DrillSubmit { code: "straight".into(), session: s1, strokes: vec![] }).await;
    a.expect("result", |m| matches!(m, ServerMsg::DrillResult { result: Ok(0), .. })).await;
    let s2 = drill_session(&mut a, "straight").await;
    a.send(ClientMsg::DrillSubmit { code: "straight".into(), session: s2, strokes: vec![shot; 9] })
        .await;
    a.expect("refused", |m| {
        matches!(m, ServerMsg::DrillResult { result: Err(drills::Refused::BadStroke), .. })
    })
    .await;
    let bests = server.store.drill_bests(alice.id).await.unwrap();
    assert_eq!(bests, vec![("straight".to_string(), expected)]);
}

/// Starts a drill session on the server and returns its number.
async fn drill_session(c: &mut Client, code: &str) -> u64 {
    c.send(ClientMsg::DrillStart { code: code.into() }).await;
    match c.expect("session", |m| matches!(m, ServerMsg::DrillStarted { .. })).await {
        ServerMsg::DrillStarted { session, .. } => session,
        _ => unreachable!(),
    }
}
