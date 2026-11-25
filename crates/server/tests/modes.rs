//! The new modes of 0.12: doubles (two teams of two, each team's visits
//! taken by its players in turn) and against the clock (a rack cleared
//! alone, timed and checked by the server, the week's table and prizes).

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use glam::DVec2;
use snook_legend_core::{BallId, EightBallGame, GameOverReason, Rules, Shot};
use snook_legend_proto::doubles::Lineup;
use snook_legend_proto::time_attack::{self, Refused, Stroke};
use snook_legend_proto::{ClientMsg, PROTOCOL_VERSION, ServerMsg, parse_server};
use snook_legend_server::lounge::Settings;
use snook_legend_server::{Config, Server, serve};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

const PASSWORD: &str = "secret123";

async fn start(settings: Settings) -> (String, std::sync::Arc<Server>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let settings = Settings { max_connections_per_ip: 100, ..settings };
    let config = Config { bind: addr, settings, ..Config::default() };
    let server = Server::new(config).await.unwrap();
    tokio::spawn(serve(listener, server.clone()));
    (format!("ws://{addr}/ws"), server)
}

struct Client {
    ws: Ws,
}

impl Client {
    async fn register(url: &str, name: &str) -> Client {
        let (ws, _) = connect_async(url).await.unwrap();
        let mut c = Client { ws };
        c.send(ClientMsg::Hello { version: PROTOCOL_VERSION, rules: Rules::Official }).await;
        c.send(ClientMsg::Register {
            username: name.into(),
            password: PASSWORD.into(),
            country: "BR".into(),
            minor: Some(false),
        })
        .await;
        c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
        c
    }

    async fn send(&mut self, msg: ClientMsg) {
        self.ws.send(Message::Text(msg.to_json().into())).await.unwrap();
    }

    /// Reads messages until one matches `pred`.
    async fn expect<F: Fn(&ServerMsg) -> bool>(&mut self, what: &str, pred: F) -> ServerMsg {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
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

    async fn lineup(&mut self) -> Lineup {
        match self.expect("line-up", |m| matches!(m, ServerMsg::DoublesLineup { .. })).await {
            ServerMsg::DoublesLineup { lineup } => lineup,
            _ => unreachable!(),
        }
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

/// `a` asks their friend `b` to be partners, and `b` accepts.
async fn pair(a: &mut Client, b: &mut Client, a_name: &str, b_name: &str) {
    befriend(a, b, a_name, b_name).await;
    a.send(ClientMsg::DoublesInvite { to: b_name.into() }).await;
    b.expect("invited", |m| matches!(m, ServerMsg::DoublesInvited { .. })).await;
    b.send(ClientMsg::DoublesAnswer { from: a_name.into(), accept: true }).await;
    for c in [a, b] {
        c.expect("pair", |m| matches!(m, ServerMsg::DoublesPair { pair: Some(_) })).await;
    }
}

/// A stroke from where the cue ball lies that hits nothing (a foul: the
/// table goes to the other team).
fn miss(g: &EightBallGame) -> Shot {
    let cue = g.ball(BallId::CUE).xy();
    let yaw = (0..360)
        .map(|d| (d as f64).to_radians())
        .find(|&yaw| {
            let dir = DVec2::new(yaw.cos(), yaw.sin());
            g.balls().iter().filter(|b| b.in_play() && b.id != BallId::CUE).all(|b| {
                let rel = b.xy() - cue;
                let along = rel.dot(dir);
                along < 0.0 || (rel - dir * along).length() > 0.08
            })
        })
        .unwrap_or(0.0);
    Shot::new(yaw, 0.6)
}

/// Four players, two pairs, a rated doubles table under the official
/// rules: returns the clients by name and the first line-up and game.
async fn doubles_game(url: &str) -> (Vec<(String, Client)>, Lineup, EightBallGame) {
    let names = ["ana", "bia", "caio", "duda"];
    let mut c = Vec::new();
    for n in names {
        c.push(Client::register(url, n).await);
    }
    let (ab, cd) = c.split_at_mut(2);
    let (a, b) = ab.split_at_mut(1);
    pair(&mut a[0], &mut b[0], "ana", "bia").await;
    let (cc, d) = cd.split_at_mut(1);
    pair(&mut cc[0], &mut d[0], "caio", "duda").await;
    c[0].send(ClientMsg::DoublesOpenTable { rated: true, rules: Rules::Official }).await;
    let ServerMsg::DoublesTables { tables } = c[2]
        .expect(
            "the table",
            |m| matches!(m, ServerMsg::DoublesTables { tables } if !tables.is_empty()),
        )
        .await
    else {
        unreachable!()
    };
    assert_eq!(tables[0].rules, Rules::Official);
    assert_eq!(tables[0].rating, 1200);
    c[2].send(ClientMsg::DoublesJoinTable { id: tables[0].id }).await;
    let mut lineup = None;
    let mut game = None;
    for (k, client) in c.iter_mut().enumerate() {
        // The line-up comes first, then the table, the player's team the seat.
        let l = client.lineup().await;
        let ServerMsg::MatchStarted { seat, game: g, rated, stakes, .. } =
            client.expect("start", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
        else {
            unreachable!()
        };
        assert!(rated && stakes.is_some());
        let (team, _) = l.place_of(names[k]).expect("in the line-up");
        assert_eq!(seat, team, "{}'s seat is their team", names[k]);
        lineup = Some(l);
        game = Some(g);
    }
    let lineup = lineup.unwrap();
    // Partners play together.
    assert_eq!(lineup.place_of("ana").unwrap().0, lineup.place_of("bia").unwrap().0);
    assert_ne!(lineup.place_of("ana").unwrap().0, lineup.place_of("caio").unwrap().0);
    let clients = names.iter().map(|n| n.to_string()).zip(c).collect();
    (clients, lineup, game.unwrap())
}

fn client<'a>(clients: &'a mut [(String, Client)], name: &str) -> &'a mut Client {
    &mut clients.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)).unwrap().1
}

/// Each team's visits go to its players in turn; only the player whose
/// visit it is may shoot; a resignation ends it for the team, and the
/// doubles ranking counts the four (not their singles rating).
#[tokio::test]
async fn doubles_take_turns_by_visit() {
    let (url, _server) = start(Settings::default()).await;
    let (mut clients, mut lineup, mut game) = doubles_game(&url).await;
    // Who held the cue on each team's visits, in order.
    let mut visits: [Vec<String>; 2] = [Vec::new(), Vec::new()];
    let mut last_team = None;
    for _ in 0..6 {
        if game.game_over().is_some() {
            break;
        }
        let team = game.current_player();
        let holder = lineup.shooter_of(team).username.clone();
        if last_team != Some(team) {
            visits[team].push(holder.clone());
            last_team = Some(team);
        }
        let turn = game.shots_taken();
        if let Some(choice) = game.pending_choice() {
            client(&mut clients, &holder)
                .send(ClientMsg::Choose { turn, option: choice.options()[0] })
                .await;
            let ServerMsg::Chose { game: g, .. } = client(&mut clients, "ana")
                .expect("choice", |m| matches!(m, ServerMsg::Chose { .. }))
                .await
            else {
                unreachable!()
            };
            game = g;
            lineup = client(&mut clients, "ana").lineup().await;
            continue;
        }
        // The partner can't take the shot.
        let partner = lineup.teams[team].iter().find(|p| p.username != holder).unwrap();
        let partner = partner.username.clone();
        let shot = if game.is_break() { Shot::new(game.default_aim(), 8.0) } else { miss(&game) };
        let p = client(&mut clients, &partner);
        p.send(ClientMsg::Shoot { turn, shot, called_pocket: None, cue_ball: None }).await;
        p.expect("refused", |m| matches!(m, ServerMsg::ShotRejected { .. })).await;
        client(&mut clients, &holder)
            .send(ClientMsg::Shoot { turn, shot, called_pocket: None, cue_ball: None })
            .await;
        let ServerMsg::ShotResult { end, .. } = client(&mut clients, "ana")
            .expect("result", |m| matches!(m, ServerMsg::ShotResult { turn: t, .. } if *t == turn))
            .await
        else {
            unreachable!()
        };
        game = end;
        lineup = client(&mut clients, "ana").lineup().await;
    }
    // A team's visits went to its two players in turn.
    for v in &visits {
        for w in v.windows(2) {
            assert_ne!(w[0], w[1], "the same player took two visits of a team in a row: {v:?}");
        }
    }
    assert!(visits.iter().map(Vec::len).sum::<usize>() >= 3, "{visits:?}");
    // Either player of a team resigns for it.
    let team = lineup.place_of("bia").unwrap().0;
    client(&mut clients, "bia").send(ClientMsg::Resign).await;
    for (_, c) in clients.iter_mut() {
        let ServerMsg::MatchEnded { result } =
            c.expect("the end", |m| matches!(m, ServerMsg::MatchEnded { .. })).await
        else {
            unreachable!()
        };
        assert_eq!(result.winner, 1 - team);
        assert_eq!(result.reason, GameOverReason::Resignation);
    }
    let end = client(&mut clients, "caio").lineup().await;
    let deltas = end.deltas.expect("a rated game's changes");
    assert!(deltas[1 - team].iter().all(|d| *d > 0) && deltas[team].iter().all(|d| *d < 0));
    // The doubles ranking has the four; the singles one nobody.
    let c = client(&mut clients, "ana");
    let mut entries = Vec::new();
    for _ in 0..50 {
        c.send(ClientMsg::DoublesLeaderboard { rules: Rules::Official }).await;
        let ServerMsg::DoublesLeaderboard { entries: e, .. } =
            c.expect("ranking", |m| matches!(m, ServerMsg::DoublesLeaderboard { .. })).await
        else {
            unreachable!()
        };
        if e.len() == 4 {
            entries = e;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(entries.len(), 4);
    assert!(entries[0].rating > 1200 && entries[0].wins == 1);
    c.send(ClientMsg::Leaderboard { rules: Rules::Official }).await;
    let ServerMsg::Leaderboard { entries, .. } =
        c.expect("singles", |m| matches!(m, ServerMsg::Leaderboard { .. })).await
    else {
        unreachable!()
    };
    assert!(entries.is_empty(), "doubles don't count for the singles rating");
    // The game is in each one's history, and can be watched again.
    let c = client(&mut clients, "bia");
    c.send(ClientMsg::Profile { username: "bia".into() }).await;
    let ServerMsg::Profile { profile } =
        c.expect("profile", |m| matches!(m, ServerMsg::Profile { .. })).await
    else {
        unreachable!()
    };
    let h = &profile.history[0];
    let side = h.doubles.as_ref().expect("a doubles game");
    assert_eq!(side.partner, "ana");
    assert!(!h.won && h.rated && h.delta < 0 && h.replay && h.match_id < 0);
    assert!(side.opponents.contains(&"caio".to_string()));
    c.send(ClientMsg::Replays).await;
    let ServerMsg::Replays { games, .. } =
        c.expect("replays", |m| matches!(m, ServerMsg::Replays { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(games[0].match_id, h.match_id);
    c.send(ClientMsg::Replay { match_id: h.match_id }).await;
    let ServerMsg::Replay { replay } =
        c.expect("the replay", |m| matches!(m, ServerMsg::Replay { .. })).await
    else {
        unreachable!()
    };
    assert!(replay.players.iter().any(|p| p == "ana & bia"), "{:?}", replay.players);
    assert!(!replay.replay.strokes.is_empty());
}

/// A server on its own port over `store`.
async fn start_on(store: snook_legend_server::store::Store) -> (String, std::sync::Arc<Server>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let settings = Settings { max_connections_per_ip: 100, ..Settings::default() };
    let config = Config { bind: addr, settings, ..Config::default() };
    let server = Server::with_store(config, store).await;
    tokio::spawn(serve(listener, server.clone()));
    (format!("ws://{addr}/ws"), server)
}

impl Client {
    async fn login(url: &str, name: &str) -> Client {
        let (ws, _) = connect_async(url).await.unwrap();
        let mut c = Client { ws };
        c.send(ClientMsg::Hello { version: PROTOCOL_VERSION, rules: Rules::Official }).await;
        c.send(ClientMsg::Login { username: name.into(), password: PASSWORD.into() }).await;
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

/// A server going away stops the doubles "Play now" and tables: no new
/// doubles game starts.
#[tokio::test]
async fn a_restart_stops_the_doubles_search() {
    let (url, server) = start(Settings::default()).await;
    let mut eva = Client::register(&url, "eva").await;
    let mut fabi = Client::register(&url, "fabi").await;
    pair(&mut eva, &mut fabi, "eva", "fabi").await;
    eva.send(ClientMsg::DoublesQueue { rated: true, rules: Rules::Official }).await;
    eva.expect(
        "searching",
        |m| matches!(m, ServerMsg::DoublesPair { pair: Some(p) } if p.searching.is_some()),
    )
    .await;
    server.with(|l| l.start_drain(60));
    for c in [&mut eva, &mut fabi] {
        c.expect(
            "stopped",
            |m| matches!(m, ServerMsg::DoublesPair { pair: Some(p) } if p.searching.is_none()),
        )
        .await;
    }
    for msg in [
        ClientMsg::DoublesQueue { rated: true, rules: Rules::Official },
        ClientMsg::DoublesOpenTable { rated: true, rules: Rules::Official },
    ] {
        fabi.send(msg).await;
        fabi.expect("refused", |m| matches!(m, ServerMsg::Error { .. })).await;
    }
}

/// A server going away hands a doubles game in progress to the next
/// server: all four find it again, and it is played to its end there.
#[tokio::test]
async fn a_restart_hands_a_doubles_game_over() {
    let store = snook_legend_server::store::Store::memory();
    let (url_a, server_a) = start_on(store.share().unwrap()).await;
    let (mut clients, _lineup, game) = doubles_game(&url_a).await;
    let drain = tokio::spawn(snook_legend_server::drain(server_a.clone(), 1));
    for (_, c) in clients.iter_mut() {
        c.closed().await;
    }
    drain.await.unwrap();

    let (url_b, _server_b) = start_on(store).await;
    let mut back = Vec::new();
    for name in ["ana", "bia", "caio", "duda"] {
        let mut c = Client::login(&url_b, name).await;
        let lineup = c.lineup().await;
        assert!(lineup.place_of(name).is_some());
        let ServerMsg::MatchStarted { game: rebuilt, rated, .. } =
            c.expect("game back", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
        else {
            unreachable!()
        };
        assert!(rated);
        assert_eq!(rebuilt, game, "the same table");
        back.push((name.to_string(), c));
    }
    // And it goes on to its end: recorded once, with all four.
    client(&mut back, "bia").send(ClientMsg::Resign).await;
    for (_, c) in back.iter_mut() {
        c.expect("the end", |m| matches!(m, ServerMsg::MatchEnded { .. })).await;
    }
    let c = client(&mut back, "ana");
    let mut entries = Vec::new();
    for _ in 0..50 {
        c.send(ClientMsg::DoublesLeaderboard { rules: Rules::Official }).await;
        let ServerMsg::DoublesLeaderboard { entries: e, .. } =
            c.expect("ranking", |m| matches!(m, ServerMsg::DoublesLeaderboard { .. })).await
        else {
            unreachable!()
        };
        if e.len() == 4 {
            entries = e;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(entries.len(), 4);
}

/// One player of a team dropping doesn't lose the game (the partner plays
/// on); both gone past the grace, the team has left it.
#[tokio::test]
async fn a_team_leaves_only_when_both_are_gone() {
    let settings = Settings { reconnect_grace: Duration::from_secs(1), ..Settings::default() };
    let (url, _server) = start(settings).await;
    let (mut clients, lineup, _game) = doubles_game(&url).await;
    let team = lineup.place_of("caio").unwrap().0;
    // Caio goes: the game goes on, Caio shown away.
    let (_, caio) = clients.remove(2);
    drop(caio);
    let l = client(&mut clients, "ana").lineup().await;
    let (t, p) = l.place_of("caio").unwrap();
    assert!(!l.teams[t][p].connected);
    tokio::time::sleep(Duration::from_millis(1600)).await;
    // Past the grace the game still goes on (Duda is there): nothing ended.
    client(&mut clients, "duda").send(ClientMsg::Ping).await;
    client(&mut clients, "duda").expect("pong", |m| matches!(m, ServerMsg::Pong)).await;
    // Duda goes too: after the grace, the team has left the game.
    let k = clients.iter().position(|(n, _)| n == "duda").unwrap();
    let (_, duda) = clients.remove(k);
    drop(duda);
    let ServerMsg::MatchEnded { result } = client(&mut clients, "ana")
        .expect("the end", |m| matches!(m, ServerMsg::MatchEnded { .. }))
        .await
    else {
        unreachable!()
    };
    assert_eq!(result.winner, 1 - team);
    assert_eq!(result.reason, GameOverReason::Abandoned);
}

/// A rack cleared against the clock is timed by the server and ranks the
/// week; strokes that don't clear it, or an attempt sent twice, don't.
#[tokio::test]
async fn time_attack_attempts_are_checked_and_ranked() {
    let (url, _server) = start(Settings::default()).await;
    let mut a = Client::register(&url, "ana").await;
    a.send(ClientMsg::TimeAttack).await;
    let ServerMsg::TimeAttack { info } =
        a.expect("info", |m| matches!(m, ServerMsg::TimeAttack { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(info.attempts_left, time_attack::ATTEMPTS_PER_DAY);
    assert!(info.board.is_empty() && info.best_ms.is_none());
    // An attempt that doesn't clear the table.
    a.send(ClientMsg::TimeAttackStart).await;
    let ServerMsg::TimeAttackStarted { attempt, .. } =
        a.expect("started", |m| matches!(m, ServerMsg::TimeAttackStarted { .. })).await
    else {
        unreachable!()
    };
    let brk = Stroke { shot: Shot::new(0.0, 8.0), cue_ball: None };
    a.send(ClientMsg::TimeAttackSubmit { attempt, strokes: vec![brk] }).await;
    let ServerMsg::TimeAttackResult { result, .. } =
        a.expect("refused", |m| matches!(m, ServerMsg::TimeAttackResult { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(result, Err(Refused::NotCleared));
    // One that does: its rack is the server's.
    a.send(ClientMsg::TimeAttackStart).await;
    let ServerMsg::TimeAttackStarted { attempt, seed } =
        a.expect("started", |m| matches!(m, ServerMsg::TimeAttackStarted { .. })).await
    else {
        unreachable!()
    };
    let strokes = time_attack::plain_strokes(seed).expect("a way through");
    let run = time_attack::replay(seed, &strokes).unwrap();
    a.send(ClientMsg::TimeAttackSubmit { attempt, strokes: strokes.clone() }).await;
    let ServerMsg::TimeAttackResult { result, .. } =
        a.expect("counted", |m| matches!(m, ServerMsg::TimeAttackResult { .. })).await
    else {
        unreachable!()
    };
    let outcome = result.expect("counted");
    assert!(outcome.best);
    assert_eq!(outcome.rank, Some(1));
    assert_eq!(outcome.fouls, run.fouls);
    // Never quicker than the balls rolled, and the fouls' seconds on top.
    assert!(outcome.ms >= run.rolling_ms + run.penalty_ms());
    let ServerMsg::TimeAttack { info } =
        a.expect("info", |m| matches!(m, ServerMsg::TimeAttack { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(info.best_ms, Some(outcome.ms));
    assert_eq!(info.board.len(), 1);
    assert_eq!(info.attempts_left, time_attack::ATTEMPTS_PER_DAY - 2);
    // The same attempt twice is refused.
    a.send(ClientMsg::TimeAttackSubmit { attempt, strokes }).await;
    let ServerMsg::TimeAttackResult { result, .. } =
        a.expect("again", |m| matches!(m, ServerMsg::TimeAttackResult { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(result, Err(Refused::NotStarted));
    // The day's attempts run out.
    for _ in 2..time_attack::ATTEMPTS_PER_DAY {
        a.send(ClientMsg::TimeAttackStart).await;
        a.expect("started", |m| matches!(m, ServerMsg::TimeAttackStarted { .. })).await;
    }
    a.send(ClientMsg::TimeAttackStart).await;
    let ServerMsg::TimeAttackResult { attempt, result } =
        a.expect("no more", |m| matches!(m, ServerMsg::TimeAttackResult { .. })).await
    else {
        unreachable!()
    };
    assert_eq!((attempt, result), (0, Err(Refused::NoAttemptsLeft)));
}

/// Last week's three quickest are paid once each, however often it runs.
#[tokio::test]
async fn the_weeks_prizes_are_paid_once() {
    let (_url, server) = start(Settings::default()).await;
    let week = time_attack::week_of(snook_legend_server::now()) - 1;
    let mut ids = Vec::new();
    for (k, name) in ["ana", "bia", "caio", "duda"].iter().enumerate() {
        let user = snook_legend_server::store::NewUser {
            username: name,
            password_hash: "x",
            country: "BR",
            rating: 1200,
            role: snook_legend_proto::Role::Player,
            ip: "127.0.0.1",
            minor: Some(false),
        };
        let u = server.store.create_user(&user).await.unwrap().unwrap();
        let store = &server.store;
        let id = store.time_attack_start(u.id, 1, 0, 7, 10).await.unwrap().unwrap();
        // Duda the quickest, then Caio, Bia, Ana.
        let ms = 100_000 - k as u32 * 10_000;
        store.time_attack_finish(u.id, id, 1, week, 1, Some((ms, 20, 0))).await.unwrap();
        ids.push(u.id);
    }
    let (board, players) = server.store.time_attack_board(week, 10).await.unwrap();
    assert_eq!(players, 4);
    assert_eq!(board[0].username, "duda");
    // A server that stopped after crediting the first place's coins (and
    // before recording the prize): they aren't paid again.
    let first = snook_legend_server::time_attack::prizes(&server)[0];
    let reference = format!("time_attack:{week}:1");
    assert!(
        server.store.credit(ids[3], first, "against the clock", Some(&reference), 1).await.unwrap()
    );
    snook_legend_server::time_attack::pay_prizes(&server).await;
    snook_legend_server::time_attack::pay_prizes(&server).await;
    let coins = |id| {
        let server = server.clone();
        async move { server.store.wallet(id).await.unwrap().coins }
    };
    let before = coins(ids[0]).await;
    assert_eq!(coins(ids[3]).await - before, snook_legend_server::time_attack::prizes(&server)[0]);
    assert_eq!(coins(ids[2]).await - before, snook_legend_server::time_attack::prizes(&server)[1]);
    assert_eq!(coins(ids[1]).await - before, snook_legend_server::time_attack::prizes(&server)[2]);
    // Every prize recorded (the first one too, after the "crash").
    for (place, id) in [(1, ids[3]), (2, ids[2]), (3, ids[1])] {
        assert!(!server.store.time_attack_prize(week, place, id, 1, 1, 1).await.unwrap());
    }
}
