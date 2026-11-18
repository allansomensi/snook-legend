//! The shot analysis on profiles, reports with the game attached and the
//! staff panel's hourly metrics: the stores (memory always, PostgreSQL when
//! `SNOOK_LEGEND_TEST_DATABASE_URL` points at a database that may be wiped)
//! and the server end to end.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use snook_legend_core::stats::ShotStats;
use snook_legend_core::{Difficulty, EightBallGame, GameOverReason, ai};
use snook_legend_proto::analysis::{ShotAnalysis, ShotKind, Tally};
use snook_legend_proto::replay::{Replay, Stroke};
use snook_legend_proto::{
    ClientMsg, PROTOCOL_VERSION, ReportReason, Role, ServerMsg, parse_server,
};
use snook_legend_server::lounge::Rules;
use snook_legend_server::store::{MatchRecord, NewReport, NewUser, Payment, Store, hour_of};
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
    }
}

fn record(players: [i64; 2], ended_at: i64, replay: bool) -> MatchRecord {
    MatchRecord {
        players,
        winner: 0,
        reason: GameOverReason::Resignation,
        rated: false,
        delta: [0, 0],
        ratings_after: [1200, 1200],
        shots: 1,
        tournament: None,
        eight_win: false,
        runout: false,
        shutout: false,
        break_win: false,
        ended_at,
        replay: replay.then(|| Replay {
            seed: 9,
            breaker: 0,
            ruleset: snook_legend_core::RULESET,
            strokes: vec![Stroke::Resign { seat: 1 }],
        }),
        stats: [ShotStats::default(); 2],
        abandoned: None,
    }
}

async fn exercise(store: Store) {
    let t = 1_800_000_000;
    let a = store.create_user(&new_user("ana")).await.unwrap().unwrap();
    let b = store.create_user(&new_user("bia")).await.unwrap().unwrap();
    let c = store.create_user(&new_user("cris")).await.unwrap().unwrap();

    // Games between two players, newest first, only those with a replay.
    let (_, old) = store.record_match(&record([a.id, b.id], t - 90_000, true)).await.unwrap();
    let (_, first) = store.record_match(&record([b.id, a.id], t - 3_000, true)).await.unwrap();
    let (_, latest) = store.record_match(&record([a.id, b.id], t - 1_000, true)).await.unwrap();
    store.record_match(&record([a.id, b.id], t - 500, false)).await.unwrap();
    store.record_match(&record([a.id, c.id], t - 100, true)).await.unwrap();
    let between = store.matches_between(b.id, a.id, t - 86_400, t, 10).await.unwrap();
    assert_eq!(between, vec![(latest, t - 1_000), (first, t - 3_000)]);
    assert_eq!(store.matches_between(a.id, b.id, t - 86_400, t, 1).await.unwrap().len(), 1);
    assert!(store.matches_between(a.id, b.id, 0, t - 100_000, 5).await.unwrap().is_empty());
    assert_ne!(old, first);

    // Reports carry their game.
    let report = |at: i64| NewReport {
        reporter: a.id,
        target: b.id,
        reason: "Cheating or abuse".into(),
        details: "too good".into(),
        context: String::new(),
        at,
    };
    let r1 = store.create_report_with_game(&report(t), Some(latest), None).await.unwrap();
    let r2 = store.create_report_with_game(&report(t + 1), None, Some(42)).await.unwrap();
    let r3 = store.create_report_with_game(&report(t + 2), Some(999_999), None).await.unwrap();
    store.create_report(&report(t + 3)).await.unwrap();
    let rows = store.reports(None, 10).await.unwrap();
    assert_eq!(rows.len(), 4);
    let row = |id: i64| rows.iter().find(|r| r.id == id).unwrap().clone();
    assert_eq!((row(r1).match_id, row(r1).live_id), (Some(latest), None));
    assert_eq!((row(r2).match_id, row(r2).live_id), (None, Some(42)));
    assert_eq!(row(r3).match_id, None, "an unknown match is not linked");
    assert_eq!((row(r1).reporter_id, row(r1).target_id), (a.id, b.id));
    store.set_report_match(r2, first).await.unwrap();
    store.set_report_match(r3, 999_999).await.unwrap();
    let again = store.report(r2).await.unwrap().unwrap();
    assert_eq!((again.match_id, again.live_id), (Some(first), Some(42)));
    assert_eq!(store.report(r3).await.unwrap().unwrap().match_id, None);
    assert!(store.report(123_456).await.unwrap().is_none());
    assert_eq!(store.reports_about(b.id, 10).await.unwrap()[0].target_id, b.id);

    // The shot analysis adds up.
    assert_eq!(store.shot_analysis(a.id).await.unwrap(), ShotAnalysis::default());
    let mut game = ShotAnalysis { games: 1, ..Default::default() };
    game.pockets[2] = Tally { attempts: 3, made: 2 };
    game.distances[1] = Tally { attempts: 3, made: 2 };
    game.kinds[ShotKind::Thin.index()] = Tally { attempts: 3, made: 2 };
    game.kinds[ShotKind::Break.index()] = Tally { attempts: 1, made: 0 };
    game.safeties = Tally { attempts: 2, made: 1 };
    game.heat[17] = Tally { attempts: 3, made: 2 };
    store.add_shot_analysis(a.id, &game).await.unwrap();
    store.add_shot_analysis(a.id, &game).await.unwrap();
    store.add_shot_analysis(a.id, &ShotAnalysis::default()).await.unwrap();
    let mut twice = game.clone();
    twice.merge(&game);
    assert_eq!(store.shot_analysis(a.id).await.unwrap(), twice);
    assert_eq!(twice.pockets[2], Tally { attempts: 6, made: 4 });
    assert_eq!(store.shot_analysis(b.id).await.unwrap(), ShotAnalysis::default());

    // Who is online, by the hour.
    let now = t + 7_200 + 120;
    let h = hour_of(now);
    store.sample_online(h, &[a.id, b.id, 987_654], 3, 1).await.unwrap();
    store.sample_online(h, &[a.id], 5, 0).await.unwrap();
    store.sample_online(h - 2, &[c.id], 1, 0).await.unwrap();
    store.sample_online(h - 200, &[c.id], 1, 0).await.unwrap();
    store.record_match(&record([b.id, c.id], h * 3600 + 5, true)).await.unwrap();
    let pay = |id: &str, amount: i64, currency: &str, at: i64, test: bool| Payment {
        id: id.into(),
        user_id: a.id,
        kind: "coins".into(),
        coins: 100,
        amount,
        currency: currency.into(),
        at,
        refunded: false,
        email: None,
        test,
    };
    store.record_payment(&pay("p1", 490, "brl", now - 10, false)).await.unwrap();
    store.record_payment(&pay("p2", 500, "brl", now - 20, false)).await.unwrap();
    store.record_payment(&pay("p3", 199, "usd", now - 86_400, false)).await.unwrap();
    store.record_payment(&pay("p4", 999, "usd", now - 30, true)).await.unwrap();
    let m = store.metrics(now).await.unwrap();
    assert_eq!(m.hours.len(), 48);
    let last = m.hours.last().unwrap();
    assert_eq!(last.hour, h);
    assert_eq!((last.active, last.peak_players, last.peak_games), (2, 5, 1));
    assert!((last.avg_players - 4.0).abs() < 1e-9);
    assert_eq!(last.games, 1);
    let two_ago = &m.hours[45];
    assert_eq!((two_ago.hour, two_ago.active, two_ago.peak_players), (h - 2, 1, 1));
    assert_eq!(m.hours[46].avg_players, 0.0);
    let today = now.div_euclid(86_400);
    assert!(m.revenue_days.contains(&(today, "brl".into(), 990, 2)), "{:?}", m.revenue_days);
    assert!(m.revenue_days.contains(&(today - 1, "usd".into(), 199, 1)), "{:?}", m.revenue_days);
    assert_eq!(m.revenue_days.len(), 2, "test payments are left out");
    assert_eq!(store.purge_active_hours(h - 100).await.unwrap(), 1);
    assert_eq!(store.purge_active_hours(h - 100).await.unwrap(), 0);
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

// ---------------------------------------------------------------- server

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

const PASSWORD: &str = "secret123";

struct Running {
    url: String,
    host: String,
    server: Arc<Server>,
}

async fn start() -> Running {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let rules = Rules { max_connections_per_ip: 100, ..Rules::default() };
    let config = Config { bind: addr, rules, admins: vec!["boss".into()], ..Config::default() };
    let server = Server::with_store(config, Store::memory()).await;
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
        c.send(ClientMsg::Hello { version }).await;
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
        let mut c = Client::connect(url, PROTOCOL_VERSION).await;
        c.send(ClientMsg::Register {
            username: name.into(),
            password: PASSWORD.into(),
            country: "BR".into(),
        })
        .await;
        c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
        c
    }

    async fn profile(&mut self, name: &str) -> snook_legend_proto::Profile {
        self.send(ClientMsg::Profile { username: name.into() }).await;
        let ServerMsg::Profile { profile } =
            self.expect("profile", |m| matches!(m, ServerMsg::Profile { .. })).await
        else {
            unreachable!()
        };
        *profile
    }
}

async fn start_match(
    a: &mut Client,
    b: &mut Client,
    b_name: &str,
    a_name: &str,
) -> (EightBallGame, usize) {
    a.send(ClientMsg::Invite { to: b_name.into(), rated: false }).await;
    b.expect("invite", |m| matches!(m, ServerMsg::Invited { .. })).await;
    b.send(ClientMsg::AnswerInvite { from: a_name.into(), accept: true }).await;
    let ServerMsg::MatchStarted { game, seat, .. } =
        a.expect("match a", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
    else {
        unreachable!()
    };
    b.expect("match b", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    (game, seat)
}

/// Plays up to `n` shots (the computer on both sides); returns the game.
async fn play(
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
        let msg = ClientMsg::Shoot {
            turn: game.shots_taken(),
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

/// A small HTTP client over a raw socket (no extra dependencies).
async fn request(host: &str, method: &str, path: &str, token: &str, body: &str) -> (u16, Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = TcpStream::connect(host).await.unwrap();
    let auth =
        if token.is_empty() { String::new() } else { format!("Authorization: Bearer {token}\r\n") };
    // HTTP/1.0: the body comes whole, not in chunks.
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

async fn staff_token(host: &str) -> String {
    let body = format!(r#"{{"username":"boss","password":"{PASSWORD}"}}"#);
    let (code, v) = request(host, "POST", "/admin/api/login", "", &body).await;
    assert_eq!(code, 200, "{v}");
    v["token"].as_str().unwrap().to_string()
}

/// Polls `f` until it returns something (the server works in the background).
async fn eventually<T, F, Fut>(what: &str, mut f: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(v) = f().await {
            return v;
        }
        assert!(tokio::time::Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn profiles_carry_the_shot_analysis_for_new_games_only() {
    let s = start().await;
    let mut ana = Client::register(&s.url, "ana").await;
    let mut bia = Client::register(&s.url, "bia").await;
    assert!(ana.profile("bia").await.analysis.as_ref().is_some_and(|a| a.games == 0));
    let (game, sa) = start_match(&mut ana, &mut bia, "bia", "ana").await;
    let game = play(&mut ana, &mut bia, game, sa, 12).await;
    if game.game_over().is_none() {
        ana.send(ClientMsg::Resign).await;
    }
    ana.expect("end", |m| matches!(m, ServerMsg::MatchEnded { .. })).await;
    // The game is replayed after it is stored.
    let analysis = eventually("the analysis", || {
        let store = &s.server.store;
        async move {
            let u = store.user_by_name("ana").await.unwrap().unwrap();
            let a = store.shot_analysis(u.id).await.unwrap();
            (a.games == 1).then_some(a)
        }
    })
    .await;
    let mut both = analysis.clone();
    let bia_id = s.server.store.user_by_name("bia").await.unwrap().unwrap().id;
    both.merge(&s.server.store.shot_analysis(bia_id).await.unwrap());
    let breaks = both.kinds[ShotKind::Break.index()].attempts;
    assert_eq!(breaks, 1, "{both:?}");
    assert!(both.attempts().attempts + both.safeties.attempts >= 1, "{both:?}");
    let p = ana.profile("ana").await;
    assert_eq!(p.analysis.as_deref(), Some(&analysis));
}

#[tokio::test]
async fn reports_carry_the_game_and_the_staff_can_watch_it() {
    let s = start().await;
    let _boss = Client::register(&s.url, "boss").await;
    let mut ana = Client::register(&s.url, "ana").await;
    let mut bia = Client::register(&s.url, "bia").await;
    let mut cris = Client::register(&s.url, "cris").await;
    let token = staff_token(&s.host).await;

    // Nothing between them: the report goes alone.
    cris.send(ClientMsg::Report {
        username: "ana".into(),
        reason: ReportReason::Spam,
        details: String::new(),
    })
    .await;
    let ServerMsg::Notice { text } =
        cris.expect("notice", |m| matches!(m, ServerMsg::Notice { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(text, "Thank you. The moderators will look into it.");

    // During a game: the game in progress goes with it.
    let (game, sa) = start_match(&mut ana, &mut bia, "bia", "ana").await;
    let game = play(&mut ana, &mut bia, game, sa, 3).await;
    bia.send(ClientMsg::Report {
        username: "ana".into(),
        reason: ReportReason::Cheating,
        details: "suspicious".into(),
    })
    .await;
    let ServerMsg::Notice { text } =
        bia.expect("notice", |m| matches!(m, ServerMsg::Notice { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(text, "Thank you. The moderators will look into it, with your game attached.");
    let (code, v) = request(&s.host, "GET", "/admin/api/reports?status=open", &token, "").await;
    assert_eq!(code, 200, "{v}");
    let reports = v["reports"].as_array().unwrap();
    assert_eq!(reports.len(), 2);
    let live = &reports[0]["game"];
    assert_eq!(live["kind"], "live", "{v}");
    assert!(reports[1]["game"].is_null());
    let id = live["id"].as_i64().unwrap();
    let (code, view) =
        request(&s.host, "GET", &format!("/admin/api/live/{id}/view"), &token, "").await;
    assert_eq!(code, 200, "{view}");
    let strokes = view["strokes"].as_array().unwrap();
    assert_eq!(strokes.len() as u32, game.shots_taken());
    assert_eq!(strokes[0]["break"], true);
    assert!(!strokes[0]["frames"].as_array().unwrap().is_empty());
    assert_eq!(view["match"]["live"], true);
    assert_eq!(view["table"]["pockets"].as_array().unwrap().len(), 6);
    // Without a staff session: nothing.
    let (code, _) = request(&s.host, "GET", &format!("/admin/api/live/{id}/view"), "", "").await;
    assert_eq!(code, 401);

    // The game ends: the report now points at the stored match.
    ana.send(ClientMsg::Resign).await;
    ana.expect("end", |m| matches!(m, ServerMsg::MatchEnded { .. })).await;
    let game_of_report = eventually("the stored match", || {
        let host = s.host.clone();
        let token = token.clone();
        async move {
            let (_, v) = request(&host, "GET", "/admin/api/reports?status=open", &token, "").await;
            let g = v["reports"][0]["game"].clone();
            (g["kind"] == "match").then_some(g)
        }
    })
    .await;
    let match_id = game_of_report["id"].as_i64().unwrap();
    let stored = s.server.store.reports(None, 10).await.unwrap();
    assert_eq!(stored[0].match_id, Some(match_id), "the link is kept");
    let (code, view) =
        request(&s.host, "GET", &format!("/admin/api/replay/{match_id}/view"), &token, "").await;
    assert_eq!(code, 200, "{view}");
    let strokes = view["strokes"].as_array().unwrap();
    assert_eq!(strokes.last().unwrap()["kind"], "resign");
    assert_eq!(view["over"]["reason"], "resignation");
    assert!(view["error"].is_null());
    // The player's page in the panel shows it too.
    let (_, user) = request(&s.host, "GET", "/admin/api/users/ana", &token, "").await;
    assert_eq!(user["reports"][0]["game"]["id"], match_id);

    // After the game, from the lounge: the latest game between them.
    ana.send(ClientMsg::LeaveMatch).await;
    ana.send(ClientMsg::Report {
        username: "bia".into(),
        reason: ReportReason::Harassment,
        details: String::new(),
    })
    .await;
    let ServerMsg::Notice { text } =
        ana.expect("notice", |m| matches!(m, ServerMsg::Notice { .. })).await
    else {
        unreachable!()
    };
    assert!(text.contains("with your game attached"), "{text}");
    let (_, v) = request(&s.host, "GET", "/admin/api/reports?status=all", &token, "").await;
    assert_eq!(v["reports"][0]["game"]["kind"], "match");
    assert_eq!(v["reports"][0]["game"]["id"], match_id);
    let (code, _) = request(&s.host, "GET", "/admin/api/replay/999999/view", &token, "").await;
    assert_eq!(code, 404);
}

#[tokio::test]
async fn metrics_sample_who_is_online_by_the_hour() {
    let s = start().await;
    let _boss = Client::register(&s.url, "boss").await;
    let mut ana = Client::register(&s.url, "ana").await;
    let mut bia = Client::register(&s.url, "bia").await;
    let token = staff_token(&s.host).await;
    start_match(&mut ana, &mut bia, "bia", "ana").await;
    snook_legend_server::metrics::sample(&s.server, snook_legend_server::now()).await;
    let (code, v) = request(&s.host, "GET", "/admin/api/metrics", &token, "").await;
    assert_eq!(code, 200, "{v}");
    let hours = v["metrics"]["hours"].as_array().unwrap();
    assert_eq!(hours.len(), 48);
    let last = &hours[47];
    assert_eq!(last["active"], 3, "{last}");
    assert_eq!(last["peak_players"], 3, "{last}");
    assert_eq!(last["peak_games"], 1, "{last}");
    assert_eq!(v["online"], 3);
    assert_eq!(v["games"], 1);
    assert!(v["queue"]["size"].is_u64());
    assert!(v["queue"]["recent"].is_array());
    assert!(v["metrics"]["revenue_days"].is_array());
    // Players only: no access.
    let (code, _) = request(&s.host, "GET", "/admin/api/metrics", "", "").await;
    assert_eq!(code, 401);
}
