//! A local server full of made-up players, for screenshots of the game's
//! online screens (see docs/UX.md, "Screenshots of every screen").
//!
//! `cargo run -p snook_legend_server --example ux_seed -- [port] [tokens.txt]`
//!
//! Keeps everything in memory. Creates the account `Ana` (VIP, coins,
//! items, achievements of every metal, friends, direct messages, an inbox,
//! a history and a rating graph) and a few dozen others, then keeps some of
//! them signed in (the lounge's players, an open table and a tournament).
//! The session tokens are written to `tokens.txt` as `name=token` lines,
//! to put in the game's `settings.ini` (`token=`, `token_server=`).

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use snook_legend_proto::{ClientMsg, PROTOCOL_VERSION, Role, Rules, ServerMsg};
use snook_legend_server::store::{MatchRecord, NewUser, SessionKind};
use snook_legend_server::{Config, Server, inbox, new_token, now, serve, token_hash};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const PASSWORD: &str = "secret123";

const PLAYERS: &[(&str, &str, i32)] = &[
    ("Ana", "BR", 1684),
    ("Bruno", "BR", 1530),
    ("Kenji", "JP", 1902),
    ("Olga", "UA", 1455),
    ("Maximilian_Großmann", "DE", 1761),
    ("Fatima", "MA", 1390),
    ("Lucas", "PT", 1288),
    ("Sofia", "AR", 1612),
    ("Mehmet", "TR", 1344),
    ("Priya", "IN", 1501),
    ("Jean-Pierre", "FR", 1477),
    ("Wei", "CN", 1820),
    ("Aisha", "EG", 1230),
    ("Tomasz", "PL", 1409),
    ("Emma", "GB", 1555),
    ("Diego", "MX", 1366),
    ("Ingrid", "NO", 1702),
    ("Yusuf", "SA", 1318),
    ("Chloé", "CA", 1444),
    ("Luca", "IT", 1599),
    ("Hana", "KR", 1650),
    ("Rafael", "ES", 1205),
    ("Sven", "SE", 1489),
    ("Nadia", "RU", 1377),
];

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let port: u16 = args.next().and_then(|p| p.parse().ok()).unwrap_or(7878);
    let tokens_path = args.next().unwrap_or_else(|| "tokens.txt".into());
    let addr = format!("127.0.0.1:{port}").parse().unwrap();
    let mut config = Config { bind: addr, ..Config::default() };
    config.settings.max_connections_per_ip = 200;
    // The staff portal against these players (`npm run dev` in the
    // website's repository): its origin, and who signs in to it.
    config.admin_origin = std::env::var("SNOOK_LEGEND_ADMIN_ORIGIN").ok();
    // E-mails (a forgotten password, a confirmation) go to the log, with
    // their links.
    config.mail = snook_legend_server::mail::MailSetup::Outbox;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "warn,snook_legend_server::mail=info".into()),
        )
        .try_init();
    config.admins = std::env::var("SNOOK_LEGEND_ADMINS")
        .map(|v| v.split(',').map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect())
        .unwrap_or_default();
    let server = Server::new(config).await.unwrap();
    let store = &server.store;
    let t0 = now();
    let hash = server.hash_password(PASSWORD).await.unwrap();
    let mut ids = Vec::new();
    for (name, country, rating) in PLAYERS {
        let user = NewUser {
            username: name,
            password_hash: &hash,
            country,
            rating: *rating,
            role: Role::Player,
            ip: "127.0.0.1",
            minor: Some(false),
        };
        let u = store.create_user(&user).await.unwrap().expect("a new account");
        ids.push(u.id);
    }
    let ana = ids[0];
    // Tokens for every account (the game signs in with them).
    let mut tokens = String::new();
    for (k, (name, ..)) in PLAYERS.iter().enumerate() {
        let token = new_token();
        store
            .create_session(&token_hash(&token), ids[k], SessionKind::Game, t0, t0 + 86_400 * 30)
            .await
            .unwrap();
        tokens.push_str(&format!("{name}={token}\n"));
    }
    std::fs::write(&tokens_path, tokens).unwrap();

    // Ana: coins, VIP, XP, a status, an avatar.
    store.credit(ana, 12_450, "staff", None, t0).await.unwrap();
    store.set_vip(ana, t0 + 86_400 * 21, true).await.unwrap();
    store.add_xp(ana, 18_400).await.unwrap();
    store.set_status_message(ana, "Training for the cup 🎱").await.unwrap();
    // Shot analyses (the profile's Analysis, VIP): Ana's with her weeks,
    // and everyone's (the leagues' numbers come from them).
    {
        use snook_legend_proto::analysis::{ShotAnalysis, ShotKind, Tally};
        let shots = |seed: u32| {
            let t = |a: u32, m: u32| Tally { attempts: a, made: m.min(a) };
            let mut a = ShotAnalysis { games: 30 + seed % 20, ..Default::default() };
            for (k, p) in a.pockets.iter_mut().enumerate() {
                let n = 30 + (k as u32 * 7 + seed) % 15;
                *p = t(n, n * (45 + (k as u32 * 11 + seed * 3) % 35) / 100);
            }
            // Straight, cuts, thin, bank, kick, combination, jump, massé, break.
            let sizes = [60, 70, 40, 20, 18, 9, 6, 4, 3, 21];
            let rates = [82, 64, 45, 26, 38, 30, 33, 25, 30, 60];
            for (k, kind) in ShotKind::ALL.iter().enumerate() {
                let n = sizes[k] + seed % 5;
                a.kinds[kind.index()] = t(n, n * (rates[k] + seed % 7) / 100);
            }
            a.distances = [t(80, 60), t(78, 44), t(39, 14)];
            a.safeties = t(25, 12 + seed % 8);
            for (k, cell) in a.heat.iter_mut().enumerate() {
                let n = (k as u32 * 37 + seed * 11) % 9;
                *cell = t(n, n * 3 / 5);
            }
            a
        };
        for (k, &id) in ids.iter().enumerate() {
            store.add_shot_analysis(id, &shots(k as u32 + 7)).await.unwrap();
        }
        let week = snook_legend_proto::advanced::week_of(t0);
        for k in 0..8 {
            store.add_shot_week(ana, week - 7 + k, 40, 18 + (k as u32 * 5) % 14).await.unwrap();
        }
    }
    // Every other item of the shop.
    for (k, item) in snook_legend_proto::shop::ITEMS.iter().enumerate() {
        if k % 3 == 0 {
            let _ = store.grant_item(ana, item.code, "staff", t0 - k as i64 * 3600).await;
        }
    }
    // Achievements of every metal (about two thirds of them).
    let codes: Vec<&str> = snook_legend_proto::ACHIEVEMENTS
        .iter()
        .enumerate()
        .filter(|(k, _)| k % 3 != 2)
        .map(|(_, a)| a.code)
        .collect();
    store.add_achievements(ana, &codes, t0 - 86_400).await.unwrap();
    for (k, &id) in ids.iter().enumerate().skip(1) {
        let some: Vec<&str> = snook_legend_proto::ACHIEVEMENTS
            .iter()
            .enumerate()
            .filter(|(j, _)| (j + k) % 4 == 0)
            .map(|(_, a)| a.code)
            .collect();
        store.add_achievements(id, &some, t0 - 86_400).await.unwrap();
        store.add_xp(id, 900 * k as u32).await.unwrap();
    }
    // Medals of the tournaments against the computer.
    use snook_legend_proto::cup::{Format, Medal};
    for (cup, place, format) in [
        (1, 1, Format::Knockout),
        (2, 2, Format::Knockout),
        (3, 1, Format::Arena),
        (4, 3, Format::Arena),
    ] {
        store.add_cup_medal(ana, &Medal { cup, format, place }, t0).await.unwrap();
    }
    // Friends: some accepted, one asking.
    for &f in &ids[1..8] {
        store.friend_request(ana, f, t0).await.unwrap();
        store.friend_accept(f, ana).await.unwrap();
    }
    store.friend_request(ids[9], ana, t0).await.unwrap();
    // Games: a history and a rating graph for Ana, results for the others.
    let reasons = [
        snook_legend_core::GameOverReason::PocketedEight,
        snook_legend_core::GameOverReason::EightPocketedEarly,
        snook_legend_core::GameOverReason::EightInWrongPocket,
    ];
    let mut rating = 1500;
    for g in 0..36u32 {
        let opp = ids[1 + (g as usize * 7) % (ids.len() - 1)];
        let ana_wins = g % 3 != 1;
        let delta = if ana_wins { 9 + (g % 7) as i32 } else { -(6 + (g % 5) as i32) };
        rating += delta;
        let record = MatchRecord {
            players: [ana, opp],
            winner: if ana_wins { 0 } else { 1 },
            reason: reasons[g as usize % reasons.len()],
            rated: g % 5 != 4,
            rules: if g % 4 == 3 { Rules::Official } else { Rules::Brazilian },
            delta: [delta, -delta],
            ratings_after: [rating, 1500 - delta],
            shots: 20 + g % 17,
            tournament: None,
            eight_win: ana_wins,
            runout: g % 11 == 0,
            shutout: g % 13 == 0,
            break_win: false,
            ended_at: t0 - (36 - g as i64) * 7_200,
            replay: None,
            stats: Default::default(),
            abandoned: None,
            room: None,
            counts: true,
        };
        let _ = store.record_match(&record).await;
    }
    for g in 0..60usize {
        let a = ids[1 + g % (ids.len() - 1)];
        let b = ids[1 + (g * 5 + 3) % (ids.len() - 1)];
        if a == b {
            continue;
        }
        let record = MatchRecord {
            players: [a, b],
            winner: g % 2,
            reason: snook_legend_core::GameOverReason::PocketedEight,
            rated: true,
            rules: if g % 3 == 0 { Rules::Official } else { Rules::Brazilian },
            delta: if g % 2 == 0 { [12, -12] } else { [-12, 12] },
            ratings_after: [1500, 1500],
            shots: 25,
            tournament: None,
            eight_win: true,
            runout: false,
            shutout: false,
            break_win: false,
            ended_at: t0 - g as i64 * 3_600,
            replay: None,
            stats: Default::default(),
            abandoned: None,
            room: None,
            counts: true,
        };
        let _ = store.record_match(&record).await;
    }
    // Direct messages.
    let dms = [
        (1, "Good game yesterday! Rematch tonight?"),
        (0, "Sure, after dinner. 9 pm?"),
        (1, "Deal 👍"),
        (2, "That bank shot on the 8 was unreal."),
        (4, "Hallo Ana! Turnier am Samstag?"),
    ];
    for (k, (who, text)) in dms.iter().enumerate() {
        let (from, to) = match who {
            0 => (ana, ids[1]),
            n => (ids[*n], ana),
        };
        let _ = store.dm_send(from, to, text, t0 - 3_000 + k as i64 * 60).await;
    }
    // The inbox.
    use snook_legend_proto::inbox::Kind;
    inbox::send(
        &server,
        ana,
        inbox::New::game(Kind::StaffMessage)
            .text("Welcome to season 4! The weekly cup now plays on Saturdays at 20:00.")
            .from("Ana")
            .important()
            .pinned_until(t0 + 86_400 * 7),
    )
    .await;
    inbox::send(&server, ana, inbox::New::game(Kind::Achievement { code: codes[3].into() })).await;
    inbox::send(
        &server,
        ana,
        inbox::New::game(Kind::Warning).text("Please keep the lounge chat friendly."),
    )
    .await;
    inbox::send(&server, ana, inbox::New::game(Kind::Maintenance { at: t0 + 7_200, minutes: 15 }))
        .await;

    // Auctions: three going on (one with bids), one over.
    let items: Vec<&str> = snook_legend_proto::shop::ITEMS
        .iter()
        .enumerate()
        .filter(|(k, _)| k % 3 != 0)
        .map(|(_, i)| i.code)
        .step_by(7)
        .take(4)
        .collect();
    for (k, code) in items.iter().enumerate() {
        let ends_at = if k == 3 { t0 - 3_600 } else { t0 + 3_600 * (2 + 20 * k as i64) };
        let id = store
            .add_auction(&snook_legend_server::store::NewAuction {
                code,
                starts_at: t0 - 86_400,
                ends_at,
                min_bid: 500 * (k as i64 + 1),
                min_increment: 50,
                by: ana,
                at: t0 - 86_400,
            })
            .await
            .unwrap();
        if k % 2 == 0 {
            store.credit(ids[2], 50_000, "staff", None, t0).await.unwrap();
            let _ = store.bid(id, ids[2], 900 * (k as i64 + 1), t0 - 600, 120, 120).await;
        }
    }
    // The supporters in the credits.
    for &id in [ana, ids[2], ids[4], ids[10], ids[16]].iter() {
        store.set_vip(id, t0 + 86_400 * 30, true).await.unwrap();
        store.set_credits(id, true, t0).await.unwrap();
    }

    let listener = TcpListener::bind(addr).await.unwrap();
    tokio::spawn(serve(listener, server.clone()));
    snook_legend_server::spawn_background(server.clone());
    println!("ux_seed: ws://{addr}/ws (tokens in {tokens_path})");

    // Some of them in the lounge: one opens a table, one a tournament.
    let url = format!("ws://{addr}/ws");
    for (k, (name, ..)) in PLAYERS.iter().enumerate().skip(1).take(14) {
        let url = url.clone();
        let name = name.to_string();
        tokio::spawn(async move {
            let Ok((mut ws, _)) = connect_async(&url).await else { return };
            let send = |m: ClientMsg| Message::text(m.to_json());
            let _ = ws
                .send(send(ClientMsg::Hello { version: PROTOCOL_VERSION, rules: Rules::Brazilian }))
                .await;
            let _ = ws
                .send(send(ClientMsg::Login { username: name.clone(), password: PASSWORD.into() }))
                .await;
            tokio::time::sleep(Duration::from_millis(400)).await;
            match k {
                3 | 6 => {
                    let rules = if k == 3 { Rules::Brazilian } else { Rules::Official };
                    let _ = ws
                        .send(send(ClientMsg::OpenTable { rated: k == 3, rules, password: None }))
                        .await;
                }
                5 => {
                    let _ = ws
                        .send(send(ClientMsg::CreateTournament {
                            name: "Friday night knockout".into(),
                            size: 8,
                            rated: true,
                            rules: Rules::Brazilian,
                            password: None,
                            entry_fee: 0,
                            format: Default::default(),
                        }))
                        .await;
                }
                // (VIPs: their lines styled.)
                2 => {
                    let text = "*Good luck* everyone, __have fun__ ~~or not~~ [blue]see you[/]";
                    let _ = ws.send(send(ClientMsg::Chat { text: text.into() })).await;
                    tokio::time::sleep(Duration::from_millis(1200)).await;
                    let text = "Anyone up for a **rated** game? [gold]Brazilian rules[/]";
                    let _ = ws.send(send(ClientMsg::Chat { text: text.into() })).await;
                }
                _ => {}
            }
            // A ping now and then, or the server lets them go when idle (and
            // the lounge empties in a long run of screenshots).
            let mut tick = tokio::time::interval(Duration::from_secs(25));
            loop {
                tokio::select! {
                    m = ws.next() => {
                        let Some(Ok(m)) = m else { break };
                        // Wei, Aisha and Tomasz take any invitation and greet the table
                        // (the screenshots of an online game, its chat).
                        let Message::Text(text) = m else { continue };
                        let Ok(msg) = snook_legend_proto::parse_server(&text) else { continue };
                        if !["Wei", "Aisha", "Tomasz"].contains(&name.as_str()) {
                            continue;
                        }
                        match msg {
                            ServerMsg::Invited { from, .. } => {
                                let _ = ws.send(send(ClientMsg::AnswerInvite { from, accept: true })).await;
                            }
                            ServerMsg::MatchStarted { .. } => {
                                for text in ["Good luck, have fun!", "Nice break 👏"] {
                                    let text = text.to_string();
                                    let _ = ws.send(send(ClientMsg::MatchChat { text })).await;
                                }
                            }
                            // The game's window closed: resign and go back
                            // to the lounge, free for the next screenshot.
                            ServerMsg::OpponentConnection { connected: false, .. } => {
                                let _ = ws.send(send(ClientMsg::Resign)).await;
                                let _ = ws.send(send(ClientMsg::LeaveMatch)).await;
                            }
                            ServerMsg::MatchEnded { .. } | ServerMsg::OpponentLeft => {
                                let _ = ws.send(send(ClientMsg::LeaveMatch)).await;
                            }
                            _ => {}
                        }
                    }
                    _ = tick.tick() => { let _ = ws.send(send(ClientMsg::Ping)).await; }
                }
            }
        });
    }
    std::future::pending::<()>().await;
}
