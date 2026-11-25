//! The storage back ends behave the same. The in-memory store is always
//! tested; PostgreSQL when `SNOOK_LEGEND_TEST_DATABASE_URL` points at an empty
//! database (it is wiped first), e.g.
//! `SNOOK_LEGEND_TEST_DATABASE_URL=postgres://snook-legend:snook-legend@localhost/snook_legend_test`.

use snook_legend_core::stats::ShotStats;
use snook_legend_core::{GameOverReason, Shot};
use snook_legend_proto::replay::{Replay, Stroke};
use snook_legend_proto::{PerRules, Role, Rules};
use snook_legend_server::store::{
    AuditSearch, BuyOutcome, FriendOutcome, FriendState, LiveMatch, MatchRecord, NewNote,
    NewReport, NewUser, Payment, Rename, SessionKind, Store, TournamentRecord, User, UserFilter,
    UserSearch, UserSort, day_of,
};

/// A search of the staff's list by name only.
fn named(q: &str) -> UserSearch {
    UserSearch { q: q.into(), ..Default::default() }
}

fn new_user<'a>(name: &'a str) -> NewUser<'a> {
    NewUser {
        username: name,
        password_hash: "h",
        country: "BR",
        rating: 1200,
        role: Role::Player,
        ip: "1.2.3.4",
        minor: None,
    }
}

async fn exercise(store: Store) {
    let t = 1_700_000_000;
    let a = store.create_user(&new_user("Alice")).await.unwrap().unwrap();
    assert!(store.create_user(&new_user("alice")).await.unwrap().is_none(), "case-insensitive");
    let b = store.create_user(&new_user("bob")).await.unwrap().unwrap();
    assert_eq!(store.user_by_name("ALICE").await.unwrap().unwrap().id, a.id);
    // Many at once (the lists' names): those that exist.
    let mut many: Vec<String> = store
        .users_by_ids(&[b.id, a.id, 999_999])
        .await
        .unwrap()
        .into_iter()
        .map(|u| u.username)
        .collect();
    many.sort();
    assert_eq!(many, ["Alice", "bob"]);
    assert!(store.users_by_ids(&[]).await.unwrap().is_empty());

    // Sessions.
    store.create_session("hash1", a.id, SessionKind::Game, t, t + 100).await.unwrap();
    assert!(store.session_user("hash1", SessionKind::Admin, t).await.unwrap().is_none());
    let (u, created) = store.session_user("hash1", SessionKind::Game, t).await.unwrap().unwrap();
    assert_eq!((u.id, created), (a.id, t));
    assert!(store.session_user("hash1", SessionKind::Game, t + 200).await.unwrap().is_none());
    assert_eq!(store.purge_sessions(t + 200).await.unwrap(), 1);

    // The profile picture, kept with the account (and in its session).
    assert_eq!(a.avatar, "");
    store.set_avatar(a.id, "zuri").await.unwrap();
    assert_eq!(store.user_by_id(a.id).await.unwrap().unwrap().avatar, "zuri");
    store.create_session("hash2", a.id, SessionKind::Game, t, t + 100).await.unwrap();
    let (u, _) = store.session_user("hash2", SessionKind::Game, t).await.unwrap().unwrap();
    assert_eq!(u.avatar, "zuri");
    store.set_avatar(a.id, "").await.unwrap();
    assert_eq!(store.user_by_id(a.id).await.unwrap().unwrap().avatar, "");

    // A rated match.
    let m = MatchRecord {
        players: [a.id, b.id],
        winner: 0,
        reason: GameOverReason::PocketedEight,
        rated: true,
        rules: Rules::Brazilian,
        delta: [16, -16],
        ratings_after: [1216, 1184],
        shots: 30,
        tournament: None,
        eight_win: true,
        runout: true,
        shutout: false,
        break_win: false,
        ended_at: t,
        replay: Some(Replay {
            seed: u64::MAX - 5,
            breaker: 1,
            rules: Rules::Brazilian,
            calls: false,
            eight_call: true,
            strokes: vec![
                Stroke::Shot {
                    shot: Shot::new(0.25, 3.5),
                    called: None,
                    cue_ball: None,
                    timeout: false,
                },
                Stroke::TimeFoul,
            ],
        }),
        stats: [
            ShotStats {
                shots: 10,
                potting_shots: 6,
                balls: 7,
                fouls: 1,
                best_run: 4,
                ..Default::default()
            },
            ShotStats {
                shots: 8,
                potting_shots: 2,
                balls: 2,
                fouls: 2,
                best_run: 1,
                ..Default::default()
            },
        ],
        abandoned: None,
        room: None,
        counts: true,
    };
    let ([ua, ub], match_id) = store.record_match(&m).await.unwrap().unwrap();
    let (ua, ub) = (ua.unwrap(), ub.unwrap());
    let record = |u: &User, rules| {
        let r = u.records.get(rules);
        (r.rating, r.peak_rating, r.games, r.wins)
    };
    assert_eq!((record(&ua, Rules::Brazilian), ua.streak), ((1216, 1216, 1, 1), 1));
    assert_eq!((ua.eight_wins, ua.runouts, ua.shutouts), (1, 1, 0));
    assert_eq!((record(&ub, Rules::Brazilian), ub.streak), ((1184, 1200, 1, 0), 0));
    // The other rules' record is untouched.
    assert_eq!(record(&ua, Rules::Official), (1200, 1200, 0, 0));
    let casual = MatchRecord {
        rated: false,
        rules: Rules::Brazilian,
        delta: [0, 0],
        winner: 1,
        replay: None,
        stats: Default::default(),
        abandoned: Some(0),
        room: None,
        counts: true,
        ..m.clone()
    };
    let ([ua, ub], _) = store.record_match(&casual).await.unwrap().unwrap();
    let ua = ua.unwrap();
    assert_eq!((ua.casual_games, ua.abandons), (1, 1));
    // Feats only count in rated games.
    assert_eq!(ub.unwrap().eight_wins, 0);

    // Replays, end-of-game numbers and the rating chart.
    let r = store.replay(match_id).await.unwrap().expect("replay kept");
    assert_eq!(r.replay, m.replay.clone().unwrap());
    assert_eq!((r.players[0].as_str(), r.ratings, r.winner), ("Alice", [1200, 1200], 0));
    assert!(store.replay(match_id + 1).await.unwrap().is_none(), "the casual game has none");
    let c = store.career(a.id).await.unwrap();
    assert_eq!((c.games, c.shots, c.balls, c.best_run, c.pot_rate()), (1, 10, 7, 4, 60));
    assert_eq!(store.rating_history(a.id, Rules::Brazilian, 10).await.unwrap(), vec![(t, 1216)]);
    assert!(store.rating_history(a.id, Rules::Official, 10).await.unwrap().is_empty());

    let history = store.history(b.id, 10).await.unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(
        (history[1].match_id, history[1].replay, history[0].replay),
        (match_id, true, false)
    );
    assert_eq!(history[1].opponent, "Alice");
    assert_eq!((history[1].won, history[1].delta, history[1].rating_after), (false, -16, 1184));
    assert_eq!(history[1].reason, "pocketed_eight");
    assert_eq!(history[1].rules, Rules::Brazilian);

    let lb = store.leaderboard(Rules::Brazilian, 10, t).await.unwrap();
    assert_eq!(lb[0].username, "Alice");
    assert_eq!(lb[0].country, "BR");
    assert!(!lb[0].vip);
    assert_eq!(store.rank_of(b.id, Rules::Brazilian).await.unwrap(), Some(2));
    // Nobody has played under the official rules yet.
    assert!(store.leaderboard(Rules::Official, 10, t).await.unwrap().is_empty());
    assert_eq!(store.rank_of(b.id, Rules::Official).await.unwrap(), None);

    // A rated game under the official rules: its own rating, peak and
    // ranking, the Brazilian ones left as they were.
    let official = MatchRecord {
        rules: Rules::Official,
        winner: 1,
        delta: [-16, 16],
        ratings_after: [1184, 1216],
        abandoned: None,
        room: None,
        counts: true,
        ended_at: t + 5,
        // Played with the calls of before 0.11.2: kept with the game.
        replay: m.replay.clone().map(|r| Replay {
            rules: Rules::Official,
            calls: true,
            eight_call: false,
            ..r
        }),
        ..m.clone()
    };
    let ([ua, ub], official_id) = store.record_match(&official).await.unwrap().unwrap();
    let (ua, ub) = (ua.unwrap(), ub.unwrap());
    assert_eq!(record(&ua, Rules::Official), (1184, 1200, 1, 0));
    assert_eq!(record(&ub, Rules::Official), (1216, 1216, 1, 1));
    assert_eq!(record(&ua, Rules::Brazilian), (1216, 1216, 1, 1));
    let lb = store.leaderboard(Rules::Official, 10, t).await.unwrap();
    assert_eq!(lb.iter().map(|e| e.username.as_str()).collect::<Vec<_>>(), ["bob", "Alice"]);
    assert_eq!(store.rank_of(b.id, Rules::Official).await.unwrap(), Some(1));
    assert_eq!(store.rank_of(b.id, Rules::Brazilian).await.unwrap(), Some(2));
    assert_eq!(store.rating_history(a.id, Rules::Official, 10).await.unwrap(), vec![(t + 5, 1184)]);
    let r = store.replay(official_id).await.unwrap().expect("replay kept");
    assert_eq!(r.replay.rules, Rules::Official);
    assert!(r.replay.calls && !r.replay.eight_call);
    let r = store.replay(match_id).await.unwrap().expect("replay kept");
    assert!(!r.replay.calls && r.replay.eight_call);
    assert_eq!(store.history(a.id, 10).await.unwrap()[0].rules, Rules::Official);

    // The shop: credits are applied once per reference, purchases never
    // go below zero, refunds are recorded once.
    assert_eq!(store.wallet(a.id).await.unwrap().coins, 0);
    assert_eq!(
        store.buy_item(a.id, "cloth_blue", 400, t).await.unwrap(),
        BuyOutcome::NotEnoughCoins
    );
    assert!(store.credit(a.id, 500, "pack", Some("pay:pi_1"), t).await.unwrap());
    assert!(!store.credit(a.id, 500, "pack", Some("pay:pi_1"), t).await.unwrap(), "same reference");
    assert_eq!(store.wallet(a.id).await.unwrap().coins, 500);
    assert_eq!(store.buy_item(a.id, "cloth_blue", 400, t).await.unwrap(), BuyOutcome::Bought);
    assert_eq!(store.buy_item(a.id, "cloth_blue", 400, t).await.unwrap(), BuyOutcome::AlreadyOwned);
    store.equip(a.id, "cloth", "cloth_blue").await.unwrap();
    store.equip(a.id, "cloth", "cloth_green").await.unwrap();
    let w = store.wallet(a.id).await.unwrap();
    assert_eq!((w.coins, w.owned.clone()), (100, vec!["cloth_blue".to_string()]));
    assert_eq!(w.equipped.get("cloth").map(String::as_str), Some("cloth_green"));
    store.set_vip(a.id, t + 100, false).await.unwrap();
    store.set_vip(a.id, t + 50, false).await.unwrap();
    assert_eq!(store.wallet(a.id).await.unwrap().vip_until, t + 100, "never shortened");
    store.set_vip(a.id, t, true).await.unwrap();
    assert_eq!(store.wallet(a.id).await.unwrap().vip_until, t);
    store.set_stripe_customer(a.id, "cus_1", true).await.unwrap();
    assert_eq!(store.user_by_stripe_customer("cus_1").await.unwrap(), Some(a.id));
    let p = Payment {
        id: "pi_1".into(),
        user_id: a.id,
        kind: "coins".into(),
        coins: 500,
        amount: 490,
        currency: "brl".into(),
        at: t,
        refunded: false,
        test: false,
        email: Some("alice@example.com".into()),
    };
    assert!(store.record_payment(&p).await.unwrap());
    assert!(!store.record_payment(&p).await.unwrap());
    assert!(store.refund_payment("pi_1").await.unwrap());
    assert!(!store.refund_payment("pi_1").await.unwrap());
    assert!(store.payment("pi_1").await.unwrap().unwrap().refunded);
    assert!(store.grant_item(a.id, "cue_ebony", "staff", t).await.unwrap());
    assert!(!store.grant_item(a.id, "cue_ebony", "staff", t).await.unwrap());
    assert_eq!(store.ledger(a.id, 10).await.unwrap().len(), 2);
    assert_eq!(store.payments_of(a.id, 10).await.unwrap().len(), 1);
    assert_eq!(store.payment_email(a.id).await.unwrap().as_deref(), Some("alice@example.com"));
    store.set_vip(a.id, t + 1000, false).await.unwrap();
    assert!(
        store.leaderboard(Rules::Brazilian, 10, t).await.unwrap()[0].vip,
        "the crown shows in the ranking"
    );

    // Coins for playing: the daily bonus once a day, games up to a cap.
    let day = day_of(t);
    let before = store.wallet(a.id).await.unwrap().coins;
    assert!(store.claim_daily(a.id, day, 20).await.unwrap());
    assert!(!store.claim_daily(a.id, day, 20).await.unwrap());
    assert_eq!(store.earn(a.id, day, 25, 40, "win", t).await.unwrap(), 25);
    assert_eq!(store.earn(a.id, day, 25, 40, "win", t).await.unwrap(), 15, "capped");
    assert_eq!(store.earn(a.id, day, 25, 40, "win", t).await.unwrap(), 0);
    assert_eq!(store.earn(a.id, day + 1, 25, 40, "win", t).await.unwrap(), 25, "a new day");
    assert_eq!(store.wallet(a.id).await.unwrap().coins, before + 20 + 25 + 15 + 25);
    // Spending and gifts never go below zero.
    let coins = store.wallet(a.id).await.unwrap().coins;
    assert!(!store.spend(a.id, coins + 1, "fee", Some("tour:1:1"), t).await.unwrap());
    assert!(store.spend(a.id, 10, "fee", Some("tour:1:1"), t).await.unwrap());
    assert_eq!(
        store.gift_item(a.id, b.id, "chalk_red", 1_000_000, t).await.unwrap(),
        BuyOutcome::NotEnoughCoins
    );
    assert_eq!(store.gift_item(a.id, b.id, "chalk_red", 150, t).await.unwrap(), BuyOutcome::Bought);
    // A ledger reference names one change: the same one again is refused,
    // by both stores alike.
    assert!(store.spend(a.id, 1, "fee", Some("tour:1:1"), t).await.is_err());
    assert_eq!(
        store.gift_item(a.id, b.id, "chalk_red", 150, t).await.unwrap(),
        BuyOutcome::AlreadyOwned
    );
    assert!(store.wallet(b.id).await.unwrap().owned.contains(&"chalk_red".to_string()));
    assert_eq!(store.wallet(a.id).await.unwrap().coins, coins - 10 - 150);

    // Games in progress survive a restart.
    let live = LiveMatch {
        id: 1 << 40,
        players: [a.id, b.id],
        rated: true,
        replay: m.replay.clone().unwrap(),
        updated_at: t,
    };
    store.save_live(&live).await.unwrap();
    let mut later = live.clone();
    later.replay.strokes.push(Stroke::Resign { seat: 0 });
    later.updated_at = t + 5;
    store.save_live(&later).await.unwrap();
    assert_eq!(store.live_match_of(b.id).await.unwrap(), Some(later.clone()));
    assert_eq!(store.purge_live(t).await.unwrap(), 0);
    store.delete_live(live.id).await.unwrap();
    assert!(store.live_match_of(a.id).await.unwrap().is_none());

    // Aim statistics.
    store.add_shot_stats(a.id, &[(14, 10, 9), (2, 5, 5)]).await.unwrap();
    store.add_shot_stats(a.id, &[(14, 10, 8)]).await.unwrap();
    let rows = store.aim_rows(&[11, 14], 5).await.unwrap();
    assert_eq!((rows.len(), rows[0].attempts, rows[0].made), (1, 20, 17));
    assert!(store.aim_rows(&[11, 14], 50).await.unwrap().is_empty());

    // Activity and metrics.
    store.mark_active(a.id, day).await.unwrap();
    store.mark_active(a.id, day).await.unwrap();
    store.mark_active(b.id, day).await.unwrap();
    let metrics = store.metrics(t).await.unwrap();
    assert_eq!(metrics.days.len(), 30);
    assert_eq!(metrics.days.last().unwrap().1, 2);

    // Achievements.
    let new = store.add_achievements(a.id, &["wins_1", "games_1"], t).await.unwrap();
    assert_eq!(new.len(), 2);
    let new = store.add_achievements(a.id, &["wins_1", "eight_10"], t).await.unwrap();
    assert_eq!(new, vec!["eight_10".to_string()]);
    assert_eq!(store.achievements(a.id).await.unwrap().len(), 3);

    // Friends.
    assert_eq!(store.friend_request(a.id, b.id, t).await.unwrap(), FriendOutcome::Sent);
    assert_eq!(store.friend_request(a.id, b.id, t).await.unwrap(), FriendOutcome::AlreadySent);
    let fb = store.friends(b.id).await.unwrap();
    assert_eq!(fb[0].1, FriendState::Incoming);
    assert!(store.friend_accept(b.id, a.id).await.unwrap());
    assert_eq!(store.friend_count(a.id).await.unwrap(), 1);
    assert_eq!(store.friends(a.id).await.unwrap()[0].1, FriendState::Friends);
    store.friend_remove(b.id, a.id).await.unwrap();
    assert_eq!(store.friend_count(a.id).await.unwrap(), 0);
    // Asking someone who already asked you makes you friends.
    store.friend_request(b.id, a.id, t).await.unwrap();
    assert_eq!(store.friend_request(a.id, b.id, t).await.unwrap(), FriendOutcome::Accepted);

    // Blocks, reports, staff.
    store.block(a.id, b.id).await.unwrap();
    assert_eq!(store.blocked(a.id).await.unwrap()[0].id, b.id);
    store.unblock(a.id, b.id).await.unwrap();
    assert!(store.blocked(a.id).await.unwrap().is_empty());
    store
        .create_report(&NewReport {
            reporter: a.id,
            target: b.id,
            reason: "Spam".into(),
            details: "d".into(),
            context: "c".into(),
            at: t,
        })
        .await
        .unwrap();
    let open = store.reports(Some(0), 10).await.unwrap();
    assert_eq!((open.len(), open[0].target.as_str()), (1, "bob"));
    assert!(store.resolve_report(open[0].id, a.id, 2, "ok", t).await.unwrap());
    assert!(store.reports(Some(0), 10).await.unwrap().is_empty());
    assert_eq!(store.reports_about(b.id, 10).await.unwrap().len(), 1);
    store.set_ban(b.id, Some(t + 50), Some("spam")).await.unwrap();
    assert!(store.user_by_id(b.id).await.unwrap().unwrap().is_banned(t));
    // Only a ban still running hides a player from the ranking (this one
    // ended long ago).
    assert!(
        store
            .leaderboard(Rules::Brazilian, 10, snook_legend_server::now())
            .await
            .unwrap()
            .iter()
            .any(|e| e.username == "bob")
    );
    store.set_ban(a.id, Some(i64::MAX), Some("cheating")).await.unwrap();
    assert!(
        !store
            .leaderboard(Rules::Brazilian, 10, snook_legend_server::now())
            .await
            .unwrap()
            .iter()
            .any(|e| e.username == "Alice")
    );
    assert_eq!(store.rank_of(b.id, Rules::Brazilian).await.unwrap(), Some(1));
    store.set_ban(a.id, None, None).await.unwrap();
    assert_eq!(store.rank_of(b.id, Rules::Brazilian).await.unwrap(), Some(2));
    store.set_mute(b.id, Some(t + 50)).await.unwrap();
    store.set_role(a.id, Role::Moderator).await.unwrap();
    assert_eq!(store.user_by_id(a.id).await.unwrap().unwrap().role, Role::Moderator);
    store.audit(a.id, "ban", "bob", "1h", t).await.unwrap();
    assert_eq!(store.audit_log(5).await.unwrap()[0].staff, "Alice");
    let (found, total) = store.search_users(&named("o"), 0, 10).await.unwrap();
    assert_eq!((found.len(), total), (1, 1));
    let (all, total) = store.search_users(&named(""), 0, 1).await.unwrap();
    assert_eq!((all.len(), total), (1, 2));
    let o = store.overview(t).await.unwrap();
    assert_eq!((o.users, o.matches_total), (2, 3));
    assert_eq!(o.matches_24h_by_rules, PerRules::new(2, 1));

    store
        .record_tournament(&TournamentRecord {
            name: "Cup".into(),
            host: Some(a.id),
            size: 4,
            rated: false,
            players: vec![a.id, b.id],
            winner: Some(a.id),
            finished_at: t,
            entry_fee: 50,
            prizes: "Alice:70".into(),
            lounge_id: Some(42),
        })
        .await
        .unwrap();
    // The server's own (the weekly cup, an event) has no host.
    store
        .record_tournament(&TournamentRecord {
            name: "Weekly Cup".into(),
            host: None,
            size: 4,
            rated: true,
            players: vec![a.id, b.id],
            winner: Some(b.id),
            finished_at: t,
            entry_fee: 0,
            prizes: String::new(),
            lounge_id: None,
        })
        .await
        .unwrap();
    let ua = store.user_by_id(a.id).await.unwrap().unwrap();
    assert_eq!((ua.tournaments_played, ua.tournaments_won), (2, 1));
    // The same tournament written again (an answer lost on the way): once.
    let again = TournamentRecord {
        name: "Cup".into(),
        host: Some(a.id),
        size: 4,
        rated: false,
        players: vec![a.id, b.id],
        winner: Some(a.id),
        finished_at: t,
        entry_fee: 50,
        prizes: "Alice:70".into(),
        lounge_id: Some(42),
    };
    assert!(!store.record_tournament(&again).await.unwrap());
    let ua = store.user_by_id(a.id).await.unwrap().unwrap();
    assert_eq!((ua.tournaments_played, ua.tournaments_won), (2, 1));
    // The daily challenge.
    let d = 20_000;
    assert_eq!(store.daily_seed(d, 42).await.unwrap(), 42);
    assert_eq!(store.daily_seed(d, 43).await.unwrap(), 42, "drawn once");
    assert_eq!(
        store.daily_finish(a.id, d, t, Some(3), false, 60).await.unwrap(),
        snook_legend_server::store::DailyFinish::NotStarted
    );
    assert!(store.daily_start(a.id, d, t, 2).await.unwrap());
    assert_eq!(store.daily_row(a.id, d).await.unwrap().started_at, t);
    assert_eq!(
        store.daily_finish(a.id, d, t + 61, Some(3), false, 60).await.unwrap(),
        snook_legend_server::store::DailyFinish::NotStarted,
        "too late"
    );
    assert!(store.daily_start(a.id, d, t + 100, 2).await.unwrap());
    assert!(!store.daily_start(a.id, d, t + 101, 2).await.unwrap(), "two attempts");
    assert_eq!(
        store.daily_finish(a.id, d, t + 110, Some(4), false, 60).await.unwrap(),
        snook_legend_server::store::DailyFinish::Counted { improved: true, first_clear: false }
    );
    assert!(store.daily_start(b.id, d, t, 2).await.unwrap());
    assert_eq!(
        store.daily_finish(b.id, d, t + 5, Some(4), true, 60).await.unwrap(),
        snook_legend_server::store::DailyFinish::Counted { improved: true, first_clear: true }
    );
    let (board, players) = store.daily_board(d, 10).await.unwrap();
    assert_eq!(players, 2);
    // A tie: the quicker attempt leads (5 seconds against 10).
    assert_eq!(
        board.iter().map(|r| (r.user_id, r.balls, r.secs)).collect::<Vec<_>>(),
        [(b.id, 4, 5), (a.id, 4, 10)]
    );
    assert_eq!(store.daily_rank(a.id, d).await.unwrap(), Some(2));
    assert_eq!(store.daily_rank(b.id, 999).await.unwrap(), None);
    assert_eq!(store.daily_wins(b.id, d + 1).await.unwrap(), vec![(d, 2)]);
    assert!(store.daily_wins(a.id, d + 1).await.unwrap().is_empty());
    let row = store.daily_row(b.id, d).await.unwrap();
    assert_eq!((row.attempts, row.started_at, row.best, row.cleared), (1, 0, Some(4), true));
    // As many balls more slowly isn't better; more quickly is, and leads
    // although made later.
    assert!(store.daily_start(b.id, d, t + 200, 3).await.unwrap());
    assert_eq!(
        store.daily_finish(b.id, d, t + 230, Some(4), false, 60).await.unwrap(),
        snook_legend_server::store::DailyFinish::Counted { improved: false, first_clear: false }
    );
    assert!(store.daily_start(a.id, d, t + 300, 3).await.unwrap());
    assert_eq!(
        store.daily_finish(a.id, d, t + 302, Some(4), false, 60).await.unwrap(),
        snook_legend_server::store::DailyFinish::Counted { improved: true, first_clear: false }
    );
    assert_eq!(store.daily_rank(a.id, d).await.unwrap(), Some(1));
    assert_eq!(store.daily_row(a.id, d).await.unwrap().best_secs, 2);
    assert_eq!(store.daily_wins(a.id, d + 1).await.unwrap(), vec![(d, 2)]);

    // Drills: bests, tables and the challenge rating.
    assert_eq!(store.drill_record(a.id, "cut", 2, t).await.unwrap(), None);
    assert_eq!(store.drill_record(a.id, "cut", 1, t + 1).await.unwrap(), Some(2));
    assert_eq!(store.drill_record(b.id, "cut", 2, t - 5).await.unwrap(), None);
    assert_eq!(store.drill_bests(a.id).await.unwrap(), vec![("cut".to_string(), 2)]);
    let board = store.drill_board("cut", 5).await.unwrap();
    assert_eq!(board.iter().map(|r| r.score).collect::<Vec<_>>(), [2, 2]);
    assert_eq!(board[0].username, "bob", "earlier first on a tie");
    store.set_challenge_rating(a.id, 302, 3, t).await.unwrap();
    store.set_challenge_rating(b.id, 202, 2, t).await.unwrap();
    let (leaders, rank) = store.challenge_board(b.id, 5).await.unwrap();
    assert_eq!(
        (leaders[0].username.as_str(), leaders[0].score, leaders[0].stars),
        ("Alice", 302, 3)
    );
    assert_eq!(rank, Some(2));

    // The trophy shelf: the tournaments won, newest first.
    assert_eq!(store.tournaments_won(a.id, 10).await.unwrap(), vec![("Cup".to_string(), 2, t)]);
    assert_eq!(
        store.tournaments_won(b.id, 10).await.unwrap(),
        vec![("Weekly Cup".to_string(), 2, t)]
    );
    store.set_country(a.id, "PT").await.unwrap();
    assert_eq!(store.user_by_id(a.id).await.unwrap().unwrap().country, "PT");

    // Recovery e-mail, then deleting the account: it is anonymised and the
    // opponent keeps the games.
    store.set_recovery_email(b.id, Some("bob@example.com")).await.unwrap();
    let (found, _) = store.search_users(&named("bob@example.com"), 0, 10).await.unwrap();
    assert_eq!(found[0].id, b.id);
    store.create_session("hash_b", b.id, SessionKind::Game, t, t + 100).await.unwrap();
    store.delete_account(b.id, t).await.unwrap();
    let gone = store.user_by_id(b.id).await.unwrap().unwrap();
    assert_eq!(gone.username, format!("deleted-{}", b.id));
    assert!(
        gone.deleted_at.is_some() && gone.recovery_email.is_none() && gone.password_hash.is_empty()
    );
    assert!(store.session_user("hash_b", SessionKind::Game, t).await.unwrap().is_none());
    assert!(store.user_by_name("bob").await.unwrap().is_none());
    let history = store.history(a.id, 10).await.unwrap();
    assert_eq!(history.len(), 3, "the other player's history stays");
    assert_eq!(history[0].opponent, format!("deleted-{}", b.id));
    for rules in Rules::ALL {
        let board = store.leaderboard(rules, 10, t).await.unwrap();
        assert!(board.iter().all(|e| e.username != gone.username));
    }
    // It leaves the other boards too: their rows, counts and places, and
    // the days it led.
    let (board, players) = store.daily_board(d, 10).await.unwrap();
    assert_eq!((board.len(), players), (1, 1));
    assert_eq!(store.daily_rank(a.id, d).await.unwrap(), Some(1));
    assert_eq!(store.daily_wins(a.id, d + 1).await.unwrap(), vec![(d, 1)]);
    let drills = store.drill_board("cut", 5).await.unwrap();
    assert_eq!(drills.iter().map(|r| r.username.as_str()).collect::<Vec<_>>(), ["Alice"]);
    let (leaders, rank) = store.challenge_board(a.id, 5).await.unwrap();
    assert_eq!((leaders.len(), rank), (1, Some(1)));

    // Missions: progress adds up to the target, per period.
    let day = "d19700".to_string();
    assert_eq!(store.advance_mission(a.id, &day, "d_balls", 9, 15, t + 50).await.unwrap(), 9);
    assert_eq!(store.advance_mission(a.id, &day, "d_balls", 9, 15, t + 50).await.unwrap(), 15);
    assert_eq!(store.advance_mission(a.id, "w2800", "w_win", 1, 10, t + 500).await.unwrap(), 1);
    let mut rows = store.mission_progress(a.id, &[day.clone(), "w2800".into()]).await.unwrap();
    rows.sort();
    assert_eq!(
        rows,
        vec![(day.clone(), "d_balls".into(), 15), ("w2800".into(), "w_win".into(), 1)]
    );
    assert!(store.mission_progress(a.id, &["d1".into()]).await.unwrap().is_empty());
    assert_eq!(store.purge_missions(t + 100).await.unwrap(), 1, "the day is over");
    assert_eq!(store.mission_progress(a.id, &[day]).await.unwrap(), vec![]);

    // The lounge's board: pinned notes first, then the newest.
    let note = |text, at, expires_at, pinned, action| NewNote {
        text,
        author_id: a.id,
        at,
        expires_at,
        kind: "event",
        pinned,
        action,
    };
    let first = store.add_board_note(&note("Rules", t, None, true, None)).await.unwrap();
    let second =
        store.add_board_note(&note("Cup", t + 1, Some(t + 60), false, Some("shop"))).await.unwrap();
    let notes = store.board_notes(t + 10).await.unwrap();
    assert_eq!(notes.iter().map(|n| n.id).collect::<Vec<_>>(), vec![first, second]);
    assert_eq!(notes[0].author, "Alice");
    assert_eq!((notes[1].kind.as_str(), notes[1].action.as_deref()), ("event", Some("shop")));
    assert!(notes[0].pinned && !notes[1].pinned);
    assert_eq!(store.board_notes(t + 61).await.unwrap().len(), 1, "expired");
    assert!(store.remove_board_note(first).await.unwrap());
    assert!(!store.remove_board_note(first).await.unwrap());
    // Items can be taken back (a refunded pack's), once.
    let owner = store.create_user(&new_user("revoked")).await.unwrap().unwrap();
    store.credit(owner.id, 500, "test", None, t).await.unwrap();
    assert_eq!(store.buy_item(owner.id, "chalk_red", 150, t).await.unwrap(), BuyOutcome::Bought);
    assert_eq!(store.revoke_items(owner.id, &["chalk_red"]).await.unwrap(), 1);
    assert_eq!(store.revoke_items(owner.id, &["chalk_red"]).await.unwrap(), 0);
}

/// Retention, on a store of its own (the accounts are created now).
async fn retention(store: Store) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
        as i64;
    let a = store.create_user(&new_user("ret_a")).await.unwrap().unwrap();
    let b = store.create_user(&new_user("ret_b")).await.unwrap().unwrap();
    let d = day_of(a.created_at);
    store.mark_active(a.id, d + 1).await.unwrap();
    store.mark_active(b.id, d + 7).await.unwrap();
    store.mark_active(a.id, d + 30).await.unwrap();
    let day = 86_400;
    // Two days later: one of the two came back the next day; day 7 hasn't come.
    let m = store.metrics(now + 2 * day).await.unwrap();
    assert_eq!((m.retention_d1, m.retention_d7, m.retention_d30), (50.0, 0.0, 0.0));
    let week = m.cohorts.iter().find(|c| c.accounts > 0).expect("their week");
    assert_eq!((week.accounts, week.d1, week.d7, week.d30), (2, Some(50.0), None, None));
    assert_eq!(m.cohorts.len() as i64, snook_legend_server::store::COHORT_WEEKS);
    // A month later: all three measured.
    let m = store.metrics(now + 31 * day).await.unwrap();
    assert_eq!((m.retention_d7, m.retention_d30), (50.0, 50.0));
    let week = m.cohorts.iter().find(|c| c.accounts > 0).expect("their week");
    assert_eq!((week.d1, week.d7, week.d30), (Some(50.0), Some(50.0), Some(50.0)));
}

/// Username changes: a limited number, and never to someone else's name.
async fn renames(store: &Store) {
    let a = store.create_user(&new_user("ren_a")).await.unwrap().unwrap();
    store.create_user(&new_user("ren_b")).await.unwrap().unwrap();
    assert_eq!(a.name_changes, 0);
    assert_eq!(store.change_username(a.id, "REN_B", 3).await.unwrap(), Rename::Taken);
    assert_eq!(store.change_username(a.id, "ren_c", 3).await.unwrap(), Rename::Done);
    // Only the letter case: the same name to the index, still a change.
    assert_eq!(store.change_username(a.id, "Ren_C", 3).await.unwrap(), Rename::Done);
    let u = store.user_by_name("ren_c").await.unwrap().unwrap();
    assert_eq!((u.id, u.username.as_str(), u.name_changes), (a.id, "Ren_C", 2));
    assert!(store.user_by_name("ren_a").await.unwrap().is_none(), "the old name is free");
    assert_eq!(store.change_username(a.id, "ren_d", 3).await.unwrap(), Rename::Done);
    assert_eq!(store.change_username(a.id, "ren_e", 3).await.unwrap(), Rename::NoChangesLeft);
    assert_eq!(store.user_by_id(a.id).await.unwrap().unwrap().username, "ren_d");
    // The old name is anyone's again.
    assert!(store.create_user(&new_user("ren_a")).await.unwrap().is_some());
    // A deleted account keeps its anonymous name.
    let gone = store.create_user(&new_user("ren_gone")).await.unwrap().unwrap();
    store.delete_account(gone.id, 1).await.unwrap();
    assert_eq!(store.change_username(gone.id, "ren_back", 3).await.unwrap(), Rename::NoChangesLeft);
}

#[tokio::test]
async fn memory_store() {
    exercise(Store::memory()).await;
    retention(Store::memory()).await;
    renames(&Store::memory()).await;
    staff_tools(&Store::memory()).await;
    audit_fixes(&Store::memory()).await;
    match_social(&Store::memory()).await;
    ops_and_modes(&Store::memory()).await;
}

#[tokio::test]
async fn postgres_store() {
    let Ok(url) = std::env::var("SNOOK_LEGEND_TEST_DATABASE_URL") else {
        eprintln!("SNOOK_LEGEND_TEST_DATABASE_URL not set: skipping");
        return;
    };
    // Start from an empty database.
    {
        use sqlx::Executor;
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        pool.execute("DROP SCHEMA public CASCADE; CREATE SCHEMA public;").await.unwrap();
    }
    exercise(Store::postgres(&url, 3).await.unwrap()).await;
    {
        use sqlx::Executor;
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        pool.execute("DROP SCHEMA public CASCADE; CREATE SCHEMA public;").await.unwrap();
    }
    retention(Store::postgres(&url, 3).await.unwrap()).await;
    renames(&Store::postgres(&url, 3).await.unwrap()).await;
    staff_tools(&Store::postgres(&url, 3).await.unwrap()).await;
    audit_fixes(&Store::postgres(&url, 3).await.unwrap()).await;
    match_social(&Store::postgres(&url, 3).await.unwrap()).await;
    ops_and_modes(&Store::postgres(&url, 3).await.unwrap()).await;
}

/// Settings kept with the account, direct messages (a conversation keeps
/// its newest), the chat kept to the quick phrases, weeks of pots and a
/// league's sums (0.12, migration 0030).
async fn match_social(store: &Store) {
    use snook_legend_proto::analysis::{ShotAnalysis, Tally};
    use snook_legend_server::store::DM_KEPT;
    let make = |name: &'static str| async move { store.create_user(&new_user(name)).await };
    let a = make("ms_a").await.unwrap().unwrap().id;
    let b = make("ms_b").await.unwrap().unwrap().id;
    let c = make("ms_c").await.unwrap().unwrap().id;
    // Settings: the last saved.
    assert_eq!(store.account_settings(a).await.unwrap(), None);
    store.save_account_settings(a, "{}", 1).await.unwrap();
    store.save_account_settings(a, r#"{"a":1}"#, 2).await.unwrap();
    assert_eq!(store.account_settings(a).await.unwrap(), Some((r#"{"a":1}"#.into(), 2)));
    // Conversations.
    for k in 0..(DM_KEPT + 3) {
        store.dm_send(a, b, &format!("m{k}"), 100 + k as i64).await.unwrap();
    }
    let from_c = store.dm_send(c, a, "from c", 5000).await.unwrap();
    assert_eq!((from_c.from, from_c.to, from_c.read_at), (c, a, None));
    let ab = store.dm_history(b, a, None, 10).await.unwrap();
    assert_eq!(ab.len(), 10);
    assert_eq!(ab[0].text, format!("m{}", DM_KEPT + 2), "newest first");
    let older = store.dm_history(a, b, Some(ab[9].id), 1000).await.unwrap();
    assert_eq!(older.len() + 10, DM_KEPT, "only the newest are kept");
    assert!(older.iter().all(|m| m.id < ab[9].id));
    let threads = store.dm_threads(a).await.unwrap();
    assert_eq!(threads.iter().map(|t| (t.other, t.unread)).collect::<Vec<_>>(), [(c, 1), (b, 0)]);
    assert_eq!(threads[0].last.text, "from c");
    assert_eq!(store.dm_read(a, c, 6000).await.unwrap(), 1);
    assert_eq!(store.dm_read(a, c, 6001).await.unwrap(), 0);
    assert_eq!(store.dm_threads(a).await.unwrap()[0].unread, 0);
    assert_eq!(store.dm_threads(b).await.unwrap()[0].unread, DM_KEPT as u32);
    // The quick phrases only.
    store.set_phrases_only(c, true).await.unwrap();
    assert!(store.user_by_id(c).await.unwrap().unwrap().chat_phrases_only);
    // Weeks of pots, newest first.
    store.add_shot_week(a, 2900, 5, 3).await.unwrap();
    store.add_shot_week(a, 2901, 4, 4).await.unwrap();
    store.add_shot_week(a, 2900, 5, 1).await.unwrap();
    let weeks = store.shot_weeks(a, 12).await.unwrap();
    let weeks: Vec<(i64, u32, u32, u32)> =
        weeks.iter().map(|w| (w.week, w.games, w.attempts, w.made)).collect();
    assert_eq!(weeks, [(2901, 1, 4, 4), (2900, 2, 10, 4)]);
    // A league: the players of a rating range with rated games, summed.
    let mut x = ShotAnalysis { games: 2, ..Default::default() };
    x.pockets[1] = Tally { attempts: 4, made: 2 };
    for id in [a, b, c] {
        store.add_shot_analysis(id, &x).await.unwrap();
    }
    let (_, players) = store.league_analysis(Rules::Brazilian, 1150, 1250).await.unwrap();
    assert_eq!(players, 0, "nobody has rated games yet");
    let m = MatchRecord {
        players: [a, b],
        winner: 0,
        reason: GameOverReason::Resignation,
        rated: true,
        rules: Rules::Brazilian,
        delta: [10, -10],
        ratings_after: [1210, 1190],
        shots: 30,
        tournament: None,
        eight_win: false,
        runout: false,
        shutout: false,
        break_win: false,
        ended_at: 7000,
        replay: None,
        stats: Default::default(),
        abandoned: None,
        room: None,
        counts: true,
    };
    store.record_match(&m).await.unwrap();
    let (sums, players) = store.league_analysis(Rules::Brazilian, 1150, 1250).await.unwrap();
    assert_eq!(players, 2);
    assert_eq!((sums.games, sums.pockets[1]), (4, Tally { attempts: 8, made: 4 }));
    let (_, players) = store.league_analysis(Rules::Brazilian, 1200, 1250).await.unwrap();
    assert_eq!(players, 1, "the range is the rating's");
    let (_, players) = store.league_analysis(Rules::Official, 1150, 1250).await.unwrap();
    assert_eq!(players, 0, "the other rules' ratings");
    // A deleted account takes its settings and messages with it.
    store.delete_account(a, 8000).await.unwrap();
    assert_eq!(store.account_settings(a).await.unwrap(), None);
    assert!(store.dm_threads(b).await.unwrap().is_empty());
    assert!(store.dm_history(c, a, None, 10).await.unwrap().is_empty());
}

/// VIP time added and taken back in one step, banned players off the
/// boards, the audit log following a renamed or deleted account, and two
/// friend requests crossing.
async fn audit_fixes(store: &Store) {
    let t = snook_legend_server::now();
    let make = |name: &'static str| async move { store.create_user(&new_user(name)).await };
    let v = make("af_vip").await.unwrap().unwrap();
    // VIP: two months at once add up; what is taken back never ends it
    // before the floor, nor lengthens it.
    let (add1, add2) = tokio::join!(store.add_vip(v.id, t, 100), store.add_vip(v.id, t, 100));
    add1.unwrap();
    add2.unwrap();
    assert_eq!(store.wallet(v.id).await.unwrap().vip_until, t + 200);
    store.take_vip(v.id, 150, t + 10).await.unwrap();
    assert_eq!(store.wallet(v.id).await.unwrap().vip_until, t + 50);
    store.take_vip(v.id, 1000, t + 80).await.unwrap();
    assert_eq!(store.wallet(v.id).await.unwrap().vip_until, t + 50);

    // A banned player is off the boards while the ban lasts.
    let good = make("af_good").await.unwrap().unwrap();
    let bad = make("af_bad").await.unwrap().unwrap();
    let day = day_of(t);
    for (u, score) in [(good.id, 3), (bad.id, 9)] {
        store.drill_record(u, "af_drill", score, t).await.unwrap();
        store.set_challenge_rating(u, 1000 + score, 1, t).await.unwrap();
        assert!(store.daily_start(u, day, t, 5).await.unwrap());
        store.daily_finish(u, day, t, Some(score), false, 600).await.unwrap();
    }
    store.set_ban(bad.id, Some(t + 3600), Some("cheating")).await.unwrap();
    let drill = store.drill_board("af_drill", 10).await.unwrap();
    assert_eq!(drill.iter().map(|r| r.username.as_str()).collect::<Vec<_>>(), ["af_good"]);
    let (challenge, rank) = store.challenge_board(good.id, 100).await.unwrap();
    assert!(challenge.iter().all(|r| r.username != "af_bad"));
    assert!(challenge.iter().any(|r| r.username == "af_good"));
    let above = challenge.iter().position(|r| r.username == "af_good").unwrap() as u32 + 1;
    assert_eq!(rank, Some(above));
    let (daily, players) = store.daily_board(day, 10).await.unwrap();
    assert!(daily.iter().all(|r| r.username != "af_bad"));
    let place = daily.iter().position(|r| r.username == "af_good").unwrap() as u32 + 1;
    assert_eq!(store.daily_rank(good.id, day).await.unwrap(), Some(place));
    assert_eq!(players, daily.len() as u32);
    let won = store.daily_wins(good.id, day + 1).await.unwrap().iter().any(|(d, _)| *d == day);
    assert_eq!(won, place == 1);
    assert_eq!(store.rank_of(good.id, Rules::Brazilian).await.unwrap(), None, "no games");

    // The audit log's entries about a player follow their account.
    let staff = make("af_staff").await.unwrap().unwrap();
    let old = make("af_old").await.unwrap().unwrap();
    store.audit(staff.id, "ban", "af_old", "first", t).await.unwrap();
    store.audit(staff.id, "event", "af_old", "an event named so", t).await.unwrap();
    let about = |name: &str| {
        let search = AuditSearch { target: Some(name.into()), ..Default::default() };
        async move {
            let mut a: Vec<String> = store
                .audit_search(&search, 50)
                .await
                .unwrap()
                .into_iter()
                .map(|e| e.action)
                .collect();
            a.sort();
            a
        }
    };
    assert_eq!(store.change_username(old.id, "af_new", 3).await.unwrap(), Rename::Done);
    assert_eq!(about("af_new").await, ["ban"]);
    assert_eq!(about("af_old").await, ["event"]);
    assert_eq!(store.staff_rename(old.id, "af_newer").await.unwrap(), Rename::Done);
    assert_eq!(about("af_newer").await, ["ban"]);
    // Whoever takes the name next doesn't inherit the history.
    store.delete_account(old.id, t).await.unwrap();
    assert!(about("af_newer").await.is_empty());
    let gone = snook_legend_server::store::deleted_name(old.id);
    assert_eq!(about(&gone).await, ["ban"]);

    // A live game's result is stored once (the write tried again after an
    // answer lost on the way), its saved copy deleted with it; a game that
    // doesn't count gives no win nor streak (the rating still moves).
    let p = make("af_p").await.unwrap().unwrap();
    let q = make("af_q").await.unwrap().unwrap();
    // (A game of 0.11.2 to 0.11.x: no calls, the 8 in any pocket.)
    let replay = Replay {
        seed: 7,
        breaker: 0,
        rules: Rules::Official,
        strokes: vec![],
        calls: false,
        eight_call: false,
    };
    let live =
        LiveMatch { id: 9_000_001, players: [p.id, q.id], rated: true, replay, updated_at: t };
    store.save_live(&live).await.unwrap();
    let saved = store.live_match_of(p.id).await.unwrap().expect("saved").replay;
    assert!(!saved.calls && !saved.eight_call);
    let game = MatchRecord {
        players: [p.id, q.id],
        winner: 0,
        reason: GameOverReason::Resignation,
        rated: true,
        rules: Rules::Brazilian,
        delta: [8, -8],
        ratings_after: [1208, 1192],
        shots: 1,
        tournament: None,
        eight_win: false,
        runout: false,
        shutout: false,
        break_win: false,
        ended_at: t,
        replay: None,
        stats: Default::default(),
        abandoned: None,
        room: Some(live.id),
        counts: false,
    };
    assert!(store.record_match(&game).await.unwrap().is_some());
    assert!(store.record_match(&game).await.unwrap().is_none(), "stored once");
    assert!(store.live_match_of(p.id).await.unwrap().is_none(), "its saved copy is gone");
    let p = store.user_by_id(p.id).await.unwrap().unwrap();
    let r = p.records.get(Rules::Brazilian);
    assert_eq!((r.rating, r.games, r.wins, p.streak), (1208, 1, 0, 0));

    // Two requests sent to each other at once make friends.
    let x = make("af_x").await.unwrap().unwrap();
    let y = make("af_y").await.unwrap().unwrap();
    let (r1, r2) =
        tokio::join!(store.friend_request(x.id, y.id, t), store.friend_request(y.id, x.id, t));
    let mut outcomes = [r1.unwrap(), r2.unwrap()];
    outcomes.sort_by_key(|o| format!("{o:?}"));
    assert_eq!(outcomes, [FriendOutcome::Accepted, FriendOutcome::Sent]);
    let friends = store.friends(x.id).await.unwrap();
    assert!(friends.iter().all(|(u, s)| u.id != y.id || *s == FriendState::Friends), "{friends:?}");
}

/// What the staff portal asks of the store: the list of players filtered
/// and sorted, notes and warnings, the audit log searched, renames by the
/// staff, the economy, the latest payments and the players' countries.
async fn staff_tools(store: &Store) {
    let t = snook_legend_server::now();
    let make = |name: &'static str, rating: i32, country: &'static str| NewUser {
        username: name,
        password_hash: "h",
        country,
        rating,
        role: Role::Player,
        ip: "1.2.3.4",
        minor: None,
    };
    let a = store.create_user(&make("st_ana", 1500, "BR")).await.unwrap().unwrap();
    let b = store.create_user(&make("st_bia", 1700, "BR")).await.unwrap().unwrap();
    let c = store.create_user(&make("st_caio", 1300, "PT")).await.unwrap().unwrap();
    let d = store.create_user(&make("st_dora", 1400, "")).await.unwrap().unwrap();
    store.set_role(a.id, Role::Moderator).await.unwrap();
    store.set_ban(b.id, Some(t + 3600), Some("spam")).await.unwrap();
    store.set_mute(c.id, Some(t + 60)).await.unwrap();
    store.delete_account(d.id, t).await.unwrap();
    let search = |filter, sort| UserSearch { q: "st_".into(), filter, sort, now: t };
    let names = |list: Vec<snook_legend_server::store::User>| {
        list.into_iter().map(|u| u.username).collect::<Vec<_>>()
    };
    // Every live account, newest first; deleted ones apart.
    let (all, total) =
        store.search_users(&search(UserFilter::All, UserSort::Newest), 0, 10).await.unwrap();
    assert_eq!(
        (names(all), total),
        (vec!["st_caio".to_string(), "st_bia".into(), "st_ana".into()], 3)
    );
    let (by_rating, _) = store
        .search_users(&search(UserFilter::All, UserSort::Rating(Rules::Brazilian)), 0, 10)
        .await
        .unwrap();
    assert_eq!(names(by_rating), ["st_bia", "st_ana", "st_caio"]);
    // A rated game under the official rules moves only those ratings: Caio
    // first there, Bia last.
    let official = MatchRecord {
        players: [b.id, c.id],
        winner: 1,
        reason: GameOverReason::PocketedEight,
        rated: true,
        rules: Rules::Official,
        delta: [-500, 500],
        ratings_after: [1200, 1800],
        shots: 20,
        tournament: None,
        eight_win: true,
        runout: false,
        shutout: false,
        break_win: false,
        ended_at: t,
        replay: None,
        stats: Default::default(),
        abandoned: None,
        room: None,
        counts: true,
    };
    store.record_match(&official).await.unwrap();
    let by = |rules| search(UserFilter::All, UserSort::Rating(rules));
    let (official, _) = store.search_users(&by(Rules::Official), 0, 10).await.unwrap();
    assert_eq!(names(official), ["st_caio", "st_ana", "st_bia"]);
    let (brazilian, _) = store.search_users(&by(Rules::Brazilian), 0, 10).await.unwrap();
    assert_eq!(names(brazilian), ["st_bia", "st_ana", "st_caio"]);
    let (by_name, _) =
        store.search_users(&search(UserFilter::All, UserSort::Name), 1, 1).await.unwrap();
    assert_eq!(names(by_name), ["st_bia"], "the second by name");
    for (filter, who) in [
        (UserFilter::Staff, "st_ana"),
        (UserFilter::Banned, "st_bia"),
        (UserFilter::Muted, "st_caio"),
    ] {
        let (list, total) =
            store.search_users(&search(filter, UserSort::Name), 0, 10).await.unwrap();
        assert_eq!((names(list), total), (vec![who.to_string()], 1), "{filter:?}");
    }
    let (gone, _) = store
        .search_users(
            &UserSearch { filter: UserFilter::Deleted, now: t, ..Default::default() },
            0,
            10,
        )
        .await
        .unwrap();
    assert!(gone.iter().any(|u| u.id == d.id));
    // A ban over is no ban.
    let later = UserSearch { now: t + 7200, ..search(UserFilter::Banned, UserSort::Name) };
    assert_eq!(store.search_users(&later, 0, 10).await.unwrap().1, 0);
    // A name with LIKE's wildcards in it is taken as it is.
    assert_eq!(store.search_users(&named("st%a"), 0, 10).await.unwrap().1, 0);

    // Notes and warnings: newest first; warnings shown once, oldest first.
    let n1 = store.add_staff_note(c.id, a.id, "note", "Watch the chat.", t).await.unwrap();
    store.add_staff_note(c.id, a.id, "warning", "First warning.", t + 1).await.unwrap();
    store.add_staff_note(c.id, a.id, "warning", "Second warning.", t + 2).await.unwrap();
    let notes = store.staff_notes(c.id, 10).await.unwrap();
    assert_eq!(notes.len(), 3);
    assert_eq!((notes[0].text.as_str(), notes[0].author.as_str()), ("Second warning.", "st_ana"));
    assert!(notes.iter().all(|n| n.delivered_at.is_none()));
    assert_eq!(
        store.take_warnings(c.id, t + 5).await.unwrap(),
        ["First warning.", "Second warning."]
    );
    assert!(store.take_warnings(c.id, t + 6).await.unwrap().is_empty(), "shown once");
    let notes = store.staff_notes(c.id, 10).await.unwrap();
    assert_eq!(notes.iter().filter(|n| n.delivered_at == Some(t + 5)).count(), 2);
    assert!(store.staff_notes(b.id, 10).await.unwrap().is_empty());
    // Only the author removes a note (or anyone, when no author is given).
    assert!(!store.remove_staff_note(n1, Some(b.id)).await.unwrap());
    assert!(store.remove_staff_note(n1, Some(a.id)).await.unwrap());
    assert!(!store.remove_staff_note(n1, None).await.unwrap());

    // The audit log, searched.
    store.audit(a.id, "mute", "st_caio", "60 min: rude", t).await.unwrap();
    store.audit(a.id, "warn", "ST_CAIO", "First warning.", t).await.unwrap();
    store.audit(a.id, "ban", "st_bia", "1h: spam", t).await.unwrap();
    let find = |q: &str, action: Option<&str>, target: Option<&str>| AuditSearch {
        q: q.into(),
        action: action.map(Into::into),
        target: target.map(Into::into),
        before: None,
    };
    let caio = store.audit_search(&find("", None, Some("st_caio")), 10).await.unwrap();
    assert_eq!(caio.iter().map(|e| e.action.as_str()).collect::<Vec<_>>(), ["warn", "mute"]);
    let bans = store.audit_search(&find("", Some("ban"), None), 10).await.unwrap();
    assert!(bans.iter().all(|e| e.action == "ban") && bans.iter().any(|e| e.target == "st_bia"));
    let rude = store.audit_search(&find("RUDE", None, None), 10).await.unwrap();
    assert_eq!(rude.len(), 1);
    let page = store
        .audit_search(
            &AuditSearch { before: Some(caio[0].id), ..find("", None, Some("st_caio")) },
            10,
        )
        .await
        .unwrap();
    assert_eq!(page.iter().map(|e| e.id).collect::<Vec<_>>(), [caio[1].id]);

    // A rename by the staff doesn't count among the player's own.
    assert_eq!(store.staff_rename(c.id, "ST_BIA").await.unwrap(), Rename::Taken);
    assert_eq!(store.staff_rename(c.id, "st_clean").await.unwrap(), Rename::Done);
    let renamed = store.user_by_id(c.id).await.unwrap().unwrap();
    assert_eq!((renamed.username.as_str(), renamed.name_changes), ("st_clean", 0));
    assert_eq!(store.staff_rename(d.id, "st_back").await.unwrap(), Rename::NoChangesLeft);

    // Coins: in wallets, and where they went in the last 30 days.
    store.credit(a.id, 300, "daily bonus", None, t).await.unwrap();
    store.credit(b.id, 1000, "staff: contest prize", None, t).await.unwrap();
    assert_eq!(store.buy_item(b.id, "chalk_red", 150, t).await.unwrap(), BuyOutcome::Bought);
    store.set_vip(a.id, t + 86_400, false).await.unwrap();
    let e = store.economy(t).await.unwrap();
    assert!(e.coins_total >= 1150 && e.wallets >= 2 && e.vip_now >= 1, "{e:?}");
    let flow = |kind: &str| e.flows.iter().find(|f| f.kind == kind).cloned();
    assert!(flow("staff").is_some_and(|f| f.credited >= 1000));
    assert!(flow("buy").is_some_and(|f| f.debited >= 150));
    assert!(e.richest.iter().any(|(n, coins)| n == "st_bia" && *coins == 850));
    assert!(e.richest.windows(2).all(|w| w[0].1 >= w[1].1));

    // The latest payments, with who paid.
    let pay = |id: &str, at| Payment {
        id: id.into(),
        user_id: b.id,
        kind: "coins".into(),
        coins: 500,
        amount: 499,
        currency: "brl".into(),
        at,
        refunded: false,
        email: None,
        test: false,
    };
    store.record_payment(&pay("st_pi_1", t - 10)).await.unwrap();
    store.record_payment(&pay("st_pi_2", t)).await.unwrap();
    let latest = store.recent_payments(1).await.unwrap();
    assert_eq!((latest[0].0.id.as_str(), latest[0].1.as_str()), ("st_pi_2", "st_bia"));

    // Events: kept, listed while not over, called off once.
    let event = |kind: &'static str, title: &'static str, starts_at: i64, ends_at: i64| {
        snook_legend_server::store::NewEvent {
            kind,
            title,
            multiplier: if kind == "tournament" { 1 } else { 2 },
            starts_at,
            ends_at,
            format: if kind == "tournament" { "knockout" } else { "" },
            size: if kind == "tournament" { 8 } else { 0 },
            rated: true,
            rules: snook_legend_proto::Rules::Official,
            entry_fee: 50,
            prize: 1000,
            champion_share: 0,
            item_prize: "",
            created_by: a.id,
            at: t,
        }
    };
    let past = store.add_event(&event("xp", "Old weekend", t - 7200, t - 3600)).await.unwrap();
    let later =
        store.add_event(&event("tournament", "Halloween Cup", t + 7200, t + 9000)).await.unwrap();
    let now_on = store.add_event(&event("coins", "Coins ×2", t - 60, t + 3600)).await.unwrap();
    let coming = store.events_after(t).await.unwrap();
    assert_eq!(coming.iter().map(|e| e.id).collect::<Vec<_>>(), [now_on, later], "soonest first");
    let cup = &coming[1];
    assert_eq!(
        (cup.kind.as_str(), cup.format.as_str(), cup.size, cup.prize),
        ("tournament", "knockout", 8, 1000)
    );
    assert_eq!((cup.entry_fee, cup.rated, cup.created_by.as_str()), (50, true, "st_ana"));
    assert!(store.cancel_event(later, t).await.unwrap());
    assert!(!store.cancel_event(later, t).await.unwrap(), "once");
    assert_eq!(store.events_after(t).await.unwrap().len(), 1);
    let all = store.recent_events(10).await.unwrap();
    assert_eq!(
        all.iter().map(|e| e.id).collect::<Vec<_>>(),
        [later, now_on, past],
        "the latest start first"
    );
    assert_eq!(all[0].cancelled_at, Some(t));

    // Where the active players come from (deleted accounts left out).
    let countries = store.active_countries(t - 86_400).await.unwrap();
    let br = countries.iter().find(|(c, _)| c == "BR").map(|x| x.1).unwrap_or(0);
    assert!(br >= 2);
    assert!(countries.windows(2).all(|w| w[0].1 >= w[1].1));
}

/// Operations, the economy and the new modes of 0.12 (migrations 0028 and
/// 0031) behave the same in memory and in the database: auctions with
/// their held coins, the daily streak, promotions, maintenance, official
/// tournaments, the credits, the staff's analytics and exports, the
/// attempts against the clock and the doubles records.
async fn ops_and_modes(store: &Store) {
    use snook_legend_server::store::{
        AttemptFinish, BidOutcome, DoublesRecord, Export, NewAuction, NewPromotion, OfficialRecord,
    };
    let t = 1_800_000_000;
    let make = |name: &'static str| async move { store.create_user(&new_user(name)).await };
    let a = make("op_a").await.unwrap().unwrap().id;
    let b = make("op_b").await.unwrap().unwrap().id;
    let c = make("op_c").await.unwrap().unwrap().id;
    let e = make("op_e").await.unwrap().unwrap().id;
    for u in [a, b, c] {
        store.credit(u, 1000, "staff: test", None, t).await.unwrap();
    }

    // Auctions: a bid is held, the one outbid gets theirs back, the leader
    // raising puts in only the difference, a late bid moves the end.
    let new = NewAuction {
        code: "cloth_blue",
        starts_at: t - 10,
        ends_at: t + 3600,
        min_bid: 100,
        min_increment: 50,
        by: a,
        at: t,
    };
    let id = store.add_auction(&new).await.unwrap();
    assert_eq!(store.bid(id, a, 50, t, 120, 120).await.unwrap(), BidOutcome::TooLow(100));
    assert_eq!(store.bid(id, a, 5000, t, 120, 120).await.unwrap(), BidOutcome::NotEnoughCoins);
    assert_eq!(store.wallet(a).await.unwrap().coins, 1000, "nothing taken");
    let placed = store.bid(id, a, 100, t, 120, 120).await.unwrap();
    assert_eq!(placed, BidOutcome::Placed { outbid: None, ends_at: t + 3600 });
    assert_eq!(store.wallet(a).await.unwrap().coins, 900, "held");
    // The leader raises: 1000 in all is enough (900 left plus the 100 held).
    let placed = store.bid(id, a, 1000, t + 1, 120, 120).await.unwrap();
    assert_eq!(placed, BidOutcome::Placed { outbid: Some((a, 100)), ends_at: t + 3600 });
    assert_eq!(store.wallet(a).await.unwrap().coins, 0);
    let refused = store.bid(id, b, 1100, t + 2, 120, 120).await.unwrap();
    assert_eq!(refused, BidOutcome::NotEnoughCoins);
    assert_eq!(store.wallet(a).await.unwrap().coins, 0, "a refused bid gives nothing back");
    store.credit(b, 500, "staff: test", None, t).await.unwrap();
    let placed = store.bid(id, b, 1100, t + 3550, 120, 120).await.unwrap();
    assert_eq!(placed, BidOutcome::Placed { outbid: Some((a, 1000)), ends_at: t + 3670 });
    assert_eq!(store.wallet(a).await.unwrap().coins, 1000, "back at once");
    assert_eq!(store.wallet(b).await.unwrap().coins, 400);
    let row = store.auction(id).await.unwrap().unwrap();
    assert_eq!((row.leader.as_str(), row.bid, row.bids, row.ends_at), ("op_b", 1100, 3, t + 3670));
    assert_eq!(store.auction_bids(id, 10).await.unwrap().len(), 3);
    assert_eq!(store.settle_auction(id, t + 3600).await.unwrap(), None, "not over yet");
    let (row, given) = store.settle_auction(id, t + 3670).await.unwrap().expect("settled");
    assert!(given, "the item given");
    assert_eq!((row.leader.as_str(), row.bid, row.settled_at), ("op_b", 1100, Some(t + 3670)));
    assert!(store.wallet(b).await.unwrap().owned.iter().any(|o| o == "cloth_blue"));
    assert_eq!(store.wallet(b).await.unwrap().coins, 400, "the coins left the economy");
    assert_eq!(store.settle_auction(id, t + 3700).await.unwrap(), None, "once");
    assert_eq!(store.cancel_auction(id, t + 3700).await.unwrap(), None, "settled already");
    // Called off: the leader's coins come back.
    let id2 = store.add_auction(&NewAuction { code: "cloth_red", ..new.clone() }).await.unwrap();
    store.bid(id2, c, 200, t, 120, 120).await.unwrap();
    assert_eq!(store.cancel_auction(id2, t + 1).await.unwrap(), Some(Some((c, 200))));
    assert_eq!(store.wallet(c).await.unwrap().coins, 1000);
    assert_eq!(store.cancel_auction(id2, t + 2).await.unwrap(), None);
    // Won by a player who got the item meanwhile: the bid comes back,
    // nothing is lost.
    let id3 = store.add_auction(&NewAuction { code: "cloth_red", ..new.clone() }).await.unwrap();
    store.bid(id3, c, 300, t, 120, 120).await.unwrap();
    store.grant_item(c, "cloth_red", "staff", t + 5).await.unwrap();
    assert_eq!(store.wallet(c).await.unwrap().coins, 700);
    let (row, given) = store.settle_auction(id3, t + 3600).await.unwrap().expect("settled");
    assert!(!given, "had it already");
    assert_eq!(row.leader_id, Some(c));
    assert_eq!(store.wallet(c).await.unwrap().coins, 1000, "the bid back: they had it already");
    let mut list = store.auctions(10).await.unwrap();
    list.sort_by_key(|r| r.id);
    assert_eq!(list.iter().map(|r| r.id).collect::<Vec<_>>(), [id, id2, id3]);
    assert_eq!(list[0].created_by, "op_a");

    // The daily bonus's streak: once a day, a day in a row adds a step.
    let day = t / 86_400;
    let claim = store.claim_daily_streak(a, day, 50, 10, 7).await.unwrap().unwrap();
    assert_eq!((claim.day, claim.coins), (1, 50));
    assert_eq!(store.claim_daily_streak(a, day, 50, 10, 7).await.unwrap(), None);
    let claim = store.claim_daily_streak(a, day + 1, 50, 10, 7).await.unwrap().unwrap();
    assert_eq!((claim.day, claim.coins), (2, 60));
    let claim = store.claim_daily_streak(a, day + 3, 50, 10, 7).await.unwrap().unwrap();
    assert_eq!((claim.day, claim.coins), (1, 50), "a day missed starts again");

    // Promotions.
    let codes = vec!["cloth_blue".to_string()];
    let p = NewPromotion {
        title: "Sale",
        kind: "discount",
        percent: 30,
        codes: &codes,
        categories: &[],
        starts_at: t,
        ends_at: t + 86_400,
        by: a,
        at: t,
    };
    let pid = store.add_promotion(&p).await.unwrap();
    assert!(store.update_promotion(pid, &NewPromotion { percent: 40, ..p.clone() }).await.unwrap());
    let rows = store.promotions(10).await.unwrap();
    assert_eq!((rows[0].percent, rows[0].codes.clone()), (40, codes.clone()));
    assert_eq!((rows[0].created_by.as_str(), rows[0].created_at), ("op_a", t));
    assert!(store.cancel_promotion(pid, t + 5).await.unwrap());
    assert!(!store.cancel_promotion(pid, t + 6).await.unwrap());
    assert!(!store.update_promotion(pid, &p).await.unwrap(), "called off: kept as it was");

    // Maintenance: started once, then neither started nor called off.
    let m = store.add_maintenance(t + 600, 15, "Update", a, t).await.unwrap();
    assert!(store.start_maintenance(m, t + 600).await.unwrap());
    assert!(!store.start_maintenance(m, t + 601).await.unwrap());
    assert!(!store.cancel_maintenance(m, t + 601).await.unwrap());
    let rows = store.maintenances(5).await.unwrap();
    assert_eq!((rows[0].minutes, rows[0].started_at), (15, Some(t + 600)));
    assert_eq!(rows[0].created_by, "op_a");

    // Official tournaments, kept once.
    let record = OfficialRecord {
        key: "event:1".into(),
        event_id: None,
        name: "Cup".into(),
        rules: Rules::Official,
        format: "knockout".into(),
        size: 4,
        players: 4,
        rated: true,
        entry_fee: 50,
        prize_pool: 180,
        prizes: vec![126, 36],
        item_prize: String::new(),
        champion_id: Some(b),
        runner_up_id: Some(a),
        detail: "{}".into(),
        starts_at: t,
        ends_at: t + 3600,
    };
    let oid = store.record_official(&record).await.unwrap().expect("kept");
    assert_eq!(store.record_official(&record).await.unwrap(), None, "once");
    assert_eq!(store.official(oid).await.unwrap(), Some((oid, record.clone())));
    assert_eq!(store.officials_won(b).await.unwrap(), [(oid, "Cup".to_string(), t + 3600)]);
    assert_eq!(store.officials(10).await.unwrap().len(), 1);

    // The credits: for VIP members, now or before.
    store.add_vip(c, t, 30 * 86_400).await.unwrap();
    store.set_credits(c, true, t).await.unwrap();
    assert!(store.in_credits(c).await.unwrap());
    let sup = store.supporters(t).await.unwrap();
    assert_eq!(sup.iter().map(|s| s.username.as_str()).collect::<Vec<_>>(), ["op_c"]);
    store.set_credits(c, false, t).await.unwrap();
    assert!(store.supporters(t).await.unwrap().is_empty());

    // The analytics and exports read the ledger.
    let days = store.coin_days(day - 1, day + 5).await.unwrap();
    let staff: i64 = days.iter().filter(|d| d.reason == "staff: test").map(|d| d.credited).sum();
    assert_eq!(staff, 3500);
    let bids: i64 =
        days.iter().filter(|d| d.reason.starts_with("auction bid")).map(|d| d.debited).sum();
    assert_eq!(bids, 100 + 1000 + 1100 + 200 + 300);
    store.item_sales(0).await.unwrap();
    store.countries(0).await.unwrap();
    store.hour_samples(0).await.unwrap();
    let vip = store.vip_members(0).await.unwrap();
    assert!(vip.iter().any(|m| m.user_id == c && m.gifted_months == 0));
    for kind in Export::ALL {
        let rows = store.export_page(kind, t - 86_400, t + 4 * 86_400, 0, 2000).await;
        let rows = rows.unwrap_or_else(|e| panic!("export {}: {e}", kind.code()));
        for (_, fields) in &rows {
            assert_eq!(fields.len(), kind.header().len(), "{}", kind.code());
        }
        if kind == Export::Ledger {
            let bid = format!("auction bid #{id}");
            assert!(rows.iter().any(|(_, f)| f[3] == "op_b" && f[4] == "-1100" && f[5] == bid));
        }
        if kind == Export::Daily {
            assert_eq!(rows.len(), 6, "a row a day");
        }
    }
    let players = store.export_page(Export::Players, 0, i64::MAX / 2, 0, 2000).await.unwrap();
    let row = players.iter().find(|(_, f)| f[1] == "op_b").expect("listed");
    assert_eq!((row.1[3].as_str(), row.1[12].as_str()), ("player", "400"));

    // Against the clock: two attempts a day, one at a time, the best of
    // the week, each prize once.
    let week = 2900;
    let first = store.time_attack_start(a, day, 1000, 7, 2).await.unwrap().expect("one left");
    let second = store.time_attack_start(a, day, 2000, 8, 2).await.unwrap().expect("one left");
    assert_eq!(store.time_attack_start(a, day, 3000, 9, 2).await.unwrap(), None);
    assert_eq!(store.time_attack_attempts(a, day).await.unwrap(), 2);
    let open = store.time_attack_open(a).await.unwrap().expect("under way");
    assert_eq!((open.id, open.seed, open.started_ms), (second, 8, 2000));
    let gone = store.time_attack_finish(a, first, 2500, week, t, Some((9000, 9, 0))).await;
    assert_eq!(gone.unwrap(), AttemptFinish::NotStarted, "given up when the next began");
    let done = store.time_attack_finish(a, second, 2500, week, t, Some((60_000, 12, 1))).await;
    assert_eq!(done.unwrap(), AttemptFinish::Counted { best: true });
    assert_eq!(store.time_attack_open(a).await.unwrap(), None);
    let again = store.time_attack_finish(a, second, 2600, week, t, Some((1, 1, 0))).await;
    assert_eq!(again.unwrap(), AttemptFinish::NotStarted, "once");
    let bt = store.time_attack_start(b, day, 1000, 1, 2).await.unwrap().unwrap();
    store.time_attack_finish(b, bt, 1500, week, t + 5, Some((50_000, 9, 0))).await.unwrap();
    let ct = store.time_attack_start(c, day, 1000, 1, 2).await.unwrap().unwrap();
    store.time_attack_finish(c, ct, 1500, week, t + 9, Some((50_000, 9, 0))).await.unwrap();
    let ct = store.time_attack_start(c, day, 1600, 1, 2).await.unwrap().unwrap();
    let worse = store.time_attack_finish(c, ct, 1700, week, t + 20, Some((70_000, 9, 0))).await;
    assert_eq!(worse.unwrap(), AttemptFinish::Counted { best: false });
    let (board, players) = store.time_attack_board(week, 2).await.unwrap();
    let names: Vec<_> = board.iter().map(|r| (r.username.as_str(), r.ms)).collect();
    assert_eq!(names, [("op_b", 50_000), ("op_c", 50_000)], "the earlier first on a tie");
    assert_eq!(players, 3);
    assert_eq!(store.time_attack_best(a, week).await.unwrap(), Some((60_000, Some(3))));
    assert_eq!(store.time_attack_best(a, week + 1).await.unwrap(), None);
    store.set_ban(b, Some(i64::MAX / 4), Some("test")).await.unwrap();
    assert_eq!(store.time_attack_best(b, week).await.unwrap(), Some((50_000, None)), "off it");
    assert_eq!(store.time_attack_best(a, week).await.unwrap(), Some((60_000, Some(2))));
    assert_eq!(store.time_attack_board(week, 10).await.unwrap().1, 2);
    store.set_ban(b, None, None).await.unwrap();
    assert!(store.time_attack_prize(week, 1, b, 300, 50_000, t).await.unwrap());
    assert!(!store.time_attack_prize(week, 1, c, 300, 50_000, t).await.unwrap(), "once");

    // Doubles: each player's record moves with a rated game, once a room.
    let d = DoublesRecord {
        players: [[a, b], [c, e]],
        winner: 0,
        reason: GameOverReason::PocketedEight,
        rated: true,
        rules: Rules::Brazilian,
        deltas: [[10, 12], [-11, -9]],
        shots: 30,
        ended_at: t,
        replay: None,
        room: Some(77),
    };
    assert!(store.record_doubles(&d).await.unwrap());
    assert!(!store.record_doubles(&d).await.unwrap(), "once");
    let ra = store.doubles_records(a).await.unwrap();
    assert_eq!((ra.brazilian.rating, ra.brazilian.games, ra.brazilian.wins), (1210, 1, 1));
    assert_eq!(ra.official.games, 0);
    let re = store.doubles_records(e).await.unwrap();
    let re = (re.brazilian.rating, re.brazilian.peak_rating, re.brazilian.wins);
    assert_eq!(re, (1191, 1200, 0));
    let board = store.doubles_leaderboard(Rules::Brazilian, 10, t).await.unwrap();
    let names: Vec<_> = board.iter().map(|r| (r.username.as_str(), r.rating)).collect();
    assert_eq!(names, [("op_b", 1212), ("op_a", 1210), ("op_e", 1191), ("op_c", 1189)]);
}
