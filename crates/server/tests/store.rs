//! The storage back ends behave the same. The in-memory store is always
//! tested; PostgreSQL when `SNOOKERED_TEST_DATABASE_URL` points at an empty
//! database (it is wiped first), e.g.
//! `SNOOKERED_TEST_DATABASE_URL=postgres://snookered:snookered@localhost/snookered_test`.

use snookered_core::stats::ShotStats;
use snookered_core::{GameOverReason, Shot};
use snookered_proto::Role;
use snookered_proto::replay::{Replay, Stroke};
use snookered_server::store::{
    BuyOutcome, FriendOutcome, FriendState, LiveMatch, MatchRecord, NewNote, NewReport, NewUser,
    Payment, Rename, SessionKind, Store, TournamentRecord, day_of,
};

fn new_user<'a>(name: &'a str) -> NewUser<'a> {
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
    let a = store.create_user(&new_user("Alice")).await.unwrap().unwrap();
    assert!(store.create_user(&new_user("alice")).await.unwrap().is_none(), "case-insensitive");
    let b = store.create_user(&new_user("bob")).await.unwrap().unwrap();
    assert_eq!(store.user_by_name("ALICE").await.unwrap().unwrap().id, a.id);

    // Sessions.
    store.create_session("hash1", a.id, SessionKind::Game, t, t + 100).await.unwrap();
    assert!(store.session_user("hash1", SessionKind::Admin, t).await.unwrap().is_none());
    let (u, created) = store.session_user("hash1", SessionKind::Game, t).await.unwrap().unwrap();
    assert_eq!((u.id, created), (a.id, t));
    assert!(store.session_user("hash1", SessionKind::Game, t + 200).await.unwrap().is_none());
    assert_eq!(store.purge_sessions(t + 200).await.unwrap(), 1);

    // A rated match.
    let m = MatchRecord {
        players: [a.id, b.id],
        winner: 0,
        reason: GameOverReason::PocketedEight,
        rated: true,
        delta: [16, -16],
        ratings_after: [1216, 1184],
        shots: 30,
        tournament: None,
        eight_win: true,
        runout: true,
        shutout: false,
        ended_at: t,
        replay: Some(Replay {
            seed: u64::MAX - 5,
            breaker: 1,
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
    };
    let ([ua, ub], match_id) = store.record_match(&m).await.unwrap();
    let (ua, ub) = (ua.unwrap(), ub.unwrap());
    assert_eq!((ua.rating, ua.peak_rating, ua.games, ua.wins, ua.streak), (1216, 1216, 1, 1, 1));
    assert_eq!((ua.eight_wins, ua.runouts, ua.shutouts), (1, 1, 0));
    assert_eq!((ub.rating, ub.peak_rating, ub.games, ub.wins, ub.streak), (1184, 1200, 1, 0, 0));
    let casual = MatchRecord {
        rated: false,
        delta: [0, 0],
        winner: 1,
        replay: None,
        stats: Default::default(),
        abandoned: Some(0),
        ..m.clone()
    };
    let ([ua, ub], _) = store.record_match(&casual).await.unwrap();
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
    assert_eq!(store.rating_history(a.id, 10).await.unwrap(), vec![(t, 1216)]);

    let history = store.history(b.id, 10).await.unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(
        (history[1].match_id, history[1].replay, history[0].replay),
        (match_id, true, false)
    );
    assert_eq!(history[1].opponent, "Alice");
    assert_eq!((history[1].won, history[1].delta, history[1].rating_after), (false, -16, 1184));
    assert_eq!(history[1].reason, "pocketed_eight");

    let lb = store.leaderboard(10, t).await.unwrap();
    assert_eq!(lb[0].username, "Alice");
    assert_eq!(lb[0].country, "BR");
    assert!(!lb[0].vip);
    assert_eq!(store.rank_of(b.id).await.unwrap(), Some(2));

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
    store.set_stripe_customer(a.id, "cus_1").await.unwrap();
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
    assert!(store.leaderboard(10, t).await.unwrap()[0].vip, "the crown shows in the ranking");

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
            .leaderboard(10, snookered_server::now())
            .await
            .unwrap()
            .iter()
            .any(|e| e.username == "bob")
    );
    store.set_ban(a.id, Some(i64::MAX), Some("cheating")).await.unwrap();
    assert!(
        !store
            .leaderboard(10, snookered_server::now())
            .await
            .unwrap()
            .iter()
            .any(|e| e.username == "Alice")
    );
    assert_eq!(store.rank_of(b.id).await.unwrap(), Some(1));
    store.set_ban(a.id, None, None).await.unwrap();
    assert_eq!(store.rank_of(b.id).await.unwrap(), Some(2));
    store.set_mute(b.id, Some(t + 50)).await.unwrap();
    store.set_role(a.id, Role::Moderator).await.unwrap();
    assert_eq!(store.user_by_id(a.id).await.unwrap().unwrap().role, Role::Moderator);
    store.audit(a.id, "ban", "bob", "1h", t).await.unwrap();
    assert_eq!(store.audit_log(5).await.unwrap()[0].staff, "Alice");
    let (found, total) = store.search_users("o", 0, 10).await.unwrap();
    assert_eq!((found.len(), total), (1, 1));
    let (all, total) = store.search_users("", 0, 1).await.unwrap();
    assert_eq!((all.len(), total), (1, 2));
    let o = store.overview(t).await.unwrap();
    assert_eq!((o.users, o.matches_total), (2, 2));

    store
        .record_tournament(&TournamentRecord {
            name: "Cup".into(),
            host: a.id,
            size: 4,
            rated: false,
            players: vec![a.id, b.id],
            winner: Some(a.id),
            finished_at: t,
            entry_fee: 50,
            prizes: "Alice:70".into(),
        })
        .await
        .unwrap();
    let ua = store.user_by_id(a.id).await.unwrap().unwrap();
    assert_eq!((ua.tournaments_played, ua.tournaments_won), (1, 1));
    // The daily challenge.
    let d = 20_000;
    assert_eq!(store.daily_seed(d, 42).await.unwrap(), 42);
    assert_eq!(store.daily_seed(d, 43).await.unwrap(), 42, "drawn once");
    assert_eq!(
        store.daily_finish(a.id, d, t, 3, false, 60).await.unwrap(),
        snookered_server::store::DailyFinish::NotStarted
    );
    assert!(store.daily_start(a.id, d, t, 2).await.unwrap());
    assert_eq!(store.daily_row(a.id, d).await.unwrap().started_at, t);
    assert_eq!(
        store.daily_finish(a.id, d, t + 61, 3, false, 60).await.unwrap(),
        snookered_server::store::DailyFinish::NotStarted,
        "too late"
    );
    assert!(store.daily_start(a.id, d, t + 100, 2).await.unwrap());
    assert!(!store.daily_start(a.id, d, t + 101, 2).await.unwrap(), "two attempts");
    assert_eq!(
        store.daily_finish(a.id, d, t + 110, 4, false, 60).await.unwrap(),
        snookered_server::store::DailyFinish::Counted { improved: true, first_clear: false }
    );
    assert!(store.daily_start(b.id, d, t, 2).await.unwrap());
    assert_eq!(
        store.daily_finish(b.id, d, t + 5, 4, true, 60).await.unwrap(),
        snookered_server::store::DailyFinish::Counted { improved: true, first_clear: true }
    );
    let (board, players) = store.daily_board(d, 10).await.unwrap();
    assert_eq!(players, 2);
    // A tie: who made it first leads.
    assert_eq!(
        board.iter().map(|r| (r.user_id, r.balls)).collect::<Vec<_>>(),
        [(b.id, 4), (a.id, 4)]
    );
    assert_eq!(store.daily_rank(a.id, d).await.unwrap(), Some(2));
    assert_eq!(store.daily_rank(b.id, 999).await.unwrap(), None);
    assert_eq!(store.daily_wins(b.id, d + 1).await.unwrap(), vec![(d, 2)]);
    assert!(store.daily_wins(a.id, d + 1).await.unwrap().is_empty());
    let row = store.daily_row(b.id, d).await.unwrap();
    assert_eq!((row.attempts, row.started_at, row.best, row.cleared), (1, 0, Some(4), true));

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
    assert!(store.tournaments_won(b.id, 10).await.unwrap().is_empty());
    store.set_country(a.id, "PT").await.unwrap();
    assert_eq!(store.user_by_id(a.id).await.unwrap().unwrap().country, "PT");

    // Recovery e-mail, then deleting the account: it is anonymised and the
    // opponent keeps the games.
    store.set_recovery_email(b.id, Some("bob@example.com")).await.unwrap();
    let (found, _) = store.search_users("bob@example.com", 0, 10).await.unwrap();
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
    assert_eq!(history.len(), 2, "the other player's history stays");
    assert_eq!(history[0].opponent, format!("deleted-{}", b.id));
    assert!(store.leaderboard(10, t).await.unwrap().iter().all(|e| e.username != gone.username));

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
    assert_eq!(m.cohorts.len() as i64, snookered_server::store::COHORT_WEEKS);
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
}

#[tokio::test]
async fn postgres_store() {
    let Ok(url) = std::env::var("SNOOKERED_TEST_DATABASE_URL") else {
        eprintln!("SNOOKERED_TEST_DATABASE_URL not set: skipping");
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
}
