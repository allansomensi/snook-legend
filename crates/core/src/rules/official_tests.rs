//! Tests of the official rules (WPA Rules of Play, 2025-09-15, "8-Ball"):
//! one for each rule, the section it comes from in its name or its note.
//! The helpers make games with calls (as recorded until 0.11.1, where the
//! rules' calls hold); the ones without calls (0.11.2 to 0.11.x) and the
//! ones with only the 8 called, as played since 0.12.0, have tests of their
//! own (see "No calls" and "The 8 called" below).

use super::tests::{layout, straight_shot};
use super::*;
use crate::ball::BallStatus;

/// A break struck softly (it makes nothing by itself and drives few balls
/// to a cushion), with the balls `down` made, in that order, and the balls
/// `off` knocked off the table at its end: what the rules do with them
/// alone. Ball 0 is the cue ball.
fn break_with(down: &[u8], off: &[u8]) -> (EightBallGame, ShotReport) {
    let mut g = EightBallGame::new(3, 0, Rules::Official).with_calls(true);
    let mut pending = g.prepare_shot(&Shot::new(g.default_aim(), 1.0), None).unwrap();
    pending.simulation.run_to_rest();
    assert!(
        pending.simulation.events().iter().all(|e| !matches!(e, Event::Pocketed { .. })),
        "a soft break makes nothing by itself"
    );
    for (k, n) in down.iter().enumerate() {
        pending.simulation.pocket_for_tests(BallId(*n), PocketId::ALL[k % 6]);
    }
    for n in off {
        pending.simulation.knock_off_for_tests(BallId(*n));
    }
    let report = g.finish_shot(pending);
    (g, report)
}

/// Player 0 on solids, player 1 on stripes, past the break, with these
/// balls on the table.
fn solids_to_shoot(positions: &[(u8, DVec2)]) -> EightBallGame {
    let mut g = EightBallGame::new(3, 0, Rules::Official).with_calls(true);
    g.is_break = false;
    g.ball_in_hand = None;
    g.groups = [Some(Group::Solids), Some(Group::Stripes)];
    layout(&mut g, positions);
    g
}

/// An open table past the break with these balls on it, player 0 to shoot.
fn open(positions: &[(u8, DVec2)]) -> EightBallGame {
    let mut g = EightBallGame::new(3, 0, Rules::Official).with_calls(true);
    g.is_break = false;
    g.ball_in_hand = None;
    layout(&mut g, positions);
    g
}

/// The pocket `ball` went down, in a report.
fn pocket_of(report: &ShotReport, ball: u8) -> Option<PocketId> {
    report.pocketed.iter().find(|(b, _)| b.0 == ball).map(|(_, p)| *p)
}

/// A straight pot of `target` into `pocket` that drops (a stroke from the
/// usual set that makes it there and nothing else goes wrong).
fn pot(g: &EightBallGame, target: u8, pocket: PocketId) -> Shot {
    (0..40)
        .map(|k| straight_shot(g, BallId(target), pocket, 1.4 + k as f64 * 0.05))
        .find(|s| {
            let mut trial = g.clone();
            trial.play_shot(s, Some(pocket)).is_ok_and(|r| {
                pocket_of(&r, target) == Some(pocket)
                    && !r.fouls.iter().any(|f| matches!(f, Foul::CueBallPocketed))
            })
        })
        .expect("a stroke that makes the ball")
}

// ---- 4.3 The break ------------------------------------------------------

/// 4.3 (b, c): nothing is called on the break, a ball made without a foul
/// keeps the table, and the table stays open whatever was made (4.4).
#[test]
fn a_ball_made_on_the_break_keeps_the_table_open() {
    for down in [&[3u8][..], &[13], &[3, 13, 9]] {
        let (g, report) = break_with(down, &[]);
        assert_eq!(report.fouls, vec![], "{report:?}");
        assert!(report.continues && report.choice.is_none(), "{report:?}");
        assert_eq!(report.group_assigned, None);
        assert!(g.is_open_table() && !g.is_break());
        assert_eq!(g.current_player(), 0);
        assert_eq!(g.ball_in_hand(), None);
        assert!(report.balls_made().is_empty(), "a break makes nothing for anyone");
    }
    // Whatever is called on the break is not a call.
    let mut g = EightBallGame::new(3, 0, Rules::Official);
    assert!(!g.calls_shots());
    let report = g.play_shot(&Shot::new(g.default_aim(), 1.0), Some(PocketId::FootLeft)).unwrap();
    assert_eq!(report.called, None);
}

/// 4.3 (d): no ball made and fewer than four object balls driven to a
/// cushion is an illegal break (not a foul). The incoming player plays the
/// table as it lies, or breaks again, or has the breaker break again.
#[test]
fn an_illegal_break_is_the_incoming_players_choice() {
    let (g, report) = break_with(&[], &[]);
    assert_eq!(report.fouls, vec![Foul::IllegalBreak], "{report:?}");
    assert_eq!(report.choice, Some(BreakChoice::IllegalBreak { scratch: false, ball_off: false }));
    assert_eq!(g.pending_choice(), report.choice);
    assert_eq!(g.current_player(), 1, "the incoming player decides");
    assert_eq!(g.prepare_shot(&Shot::new(0.0, 1.0), None).unwrap_err(), ShotError::ChoicePending);
    assert!(!g.calls_shots());

    let mut as_lies = g.clone();
    assert_eq!(as_lies.choose(BreakOption::CueBallInHand), Err(ChoiceError::NotAnOption));
    as_lies.choose(BreakOption::PlayOn).unwrap();
    assert_eq!(as_lies.pending_choice(), None);
    assert_eq!((as_lies.current_player(), as_lies.ball_in_hand()), (1, None));
    assert!(!as_lies.is_break() && as_lies.is_open_table() && as_lies.calls_shots());
    assert_eq!(as_lies.balls(), g.balls(), "the table as it lies");
    assert_eq!(as_lies.choose(BreakOption::PlayOn), Err(ChoiceError::NoChoice));

    let mut again = g.clone();
    again.choose(BreakOption::BreakAgain).unwrap();
    assert!(again.is_break() && again.rack_untouched());
    assert_eq!((again.current_player(), again.ball_in_hand()), (1, Some(BallInHand::Kitchen)));
    assert_ne!(again.balls(), EightBallGame::new(3, 0, Rules::Official).balls(), "a new rack");

    let mut theirs = g.clone();
    theirs.choose(BreakOption::OpponentBreaks).unwrap();
    assert!(theirs.is_break() && theirs.rack_untouched());
    assert_eq!(theirs.current_player(), 0, "the breaker breaks again");
}

/// 4.3 (d): a real break struck hard drives far more than four balls to
/// the cushions: legal, made balls or not.
#[test]
fn a_hard_break_is_legal() {
    for seed in 0..8 {
        let mut g = EightBallGame::new(seed, 0, Rules::Official);
        let report = g.play_shot(&Shot::new(g.default_aim(), g.max_cue_speed()), None).unwrap();
        assert!(!report.fouls.contains(&Foul::IllegalBreak), "seed {seed}: {report:?}");
    }
}

/// 4.3 (e): the 8 made on a legal break is no foul. The breaker has it
/// spotted and plays on, or breaks again.
#[test]
fn the_eight_on_a_legal_break_is_the_breakers_choice() {
    let (g, report) = break_with(&[8], &[]);
    assert_eq!(report.fouls, vec![]);
    assert_eq!(report.game_over, None);
    assert_eq!(report.choice, Some(BreakChoice::EightOnBreak));
    assert_eq!(g.current_player(), 0, "the breaker decides");
    assert!(!g.ball(BallId::EIGHT).in_play());
    // The table as it waits for the choice is read back (a game sent over
    // the network, or saved, meanwhile).
    let back: EightBallGame = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
    assert_eq!(back.pending_choice(), Some(BreakChoice::EightOnBreak));

    let mut spotted = g.clone();
    spotted.choose(BreakOption::PlayOn).unwrap();
    let eight = spotted.ball(BallId::EIGHT);
    assert!(eight.in_play() && eight.xy().y == 0.0 && eight.xy().x >= spotted.table.foot_spot().x);
    assert_eq!((spotted.current_player(), spotted.ball_in_hand()), (0, None));
    assert!(spotted.calls_shots());

    let mut again = g.clone();
    again.choose(BreakOption::BreakAgain).unwrap();
    assert!(again.is_break() && again.rack_untouched() && again.current_player() == 0);
}

/// 4.3 (f): the 8 made on a break with a foul. The incoming player has it
/// spotted and the cue ball in hand behind the head string, or breaks.
#[test]
fn the_eight_on_a_break_foul_is_the_incoming_players_choice() {
    let (g, report) = break_with(&[8, 0], &[]);
    assert!(report.fouls.contains(&Foul::CueBallPocketed), "{report:?}");
    assert_eq!(report.choice, Some(BreakChoice::EightOnFoulBreak));
    let back: EightBallGame = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
    assert_eq!(back.pending_choice(), Some(BreakChoice::EightOnFoulBreak));
    assert_eq!(g.current_player(), 1);
    assert!(g.ball(BallId::CUE).in_play(), "the cue ball waits on the table");

    let mut spotted = g.clone();
    spotted.choose(BreakOption::PlayOn).unwrap();
    assert!(spotted.ball(BallId::EIGHT).in_play());
    assert_eq!((spotted.current_player(), spotted.ball_in_hand()), (1, Some(BallInHand::Kitchen)));
    assert!(spotted.ball(BallId::CUE).xy().x <= spotted.table.head_string_x());

    let mut again = g.clone();
    again.choose(BreakOption::BreakAgain).unwrap();
    assert!(again.is_break() && again.current_player() == 1);
}

/// 4.3 (h): a scratch on a legal break is a foul: the incoming player has
/// the cue ball in hand behind the head string, the balls made stay down
/// and the table is open.
#[test]
fn a_scratch_on_the_break_gives_the_cue_ball_in_hand_behind_the_head_string() {
    let (g, report) = break_with(&[3, 0], &[]);
    assert_eq!(report.fouls, vec![Foul::CueBallPocketed], "{report:?}");
    assert!(report.choice.is_none() && !report.continues);
    assert_eq!((g.current_player(), g.ball_in_hand()), (1, Some(BallInHand::Kitchen)));
    assert!(g.is_open_table() && !g.ball(BallId(3)).in_play());
    let hx = g.table.head_string_x();
    assert!(g.can_place_cue_ball(g.ball(BallId::CUE).xy()));
    assert!(!g.can_place_cue_ball(DVec2::new(hx + 0.2, 0.3)));
    assert!(g.can_place_cue_ball(DVec2::new(hx - 0.2, 0.3)));
}

/// 4.3 (d) with a scratch: the incoming player who plays on has the cue
/// ball in hand behind the head string.
#[test]
fn an_illegal_break_with_a_scratch() {
    let (g, report) = break_with(&[0], &[]);
    assert!(report.fouls.contains(&Foul::IllegalBreak), "{report:?}");
    assert_eq!(report.choice, Some(BreakChoice::IllegalBreak { scratch: true, ball_off: false }));
    let mut g = g;
    g.choose(BreakOption::PlayOn).unwrap();
    assert_eq!((g.current_player(), g.ball_in_hand()), (1, Some(BallInHand::Kitchen)));
}

/// 4.3 (d) and (g) at once: after an illegal break that drove a ball off
/// the table, the incoming player may also take the cue ball in hand
/// behind the head string.
#[test]
fn an_illegal_break_with_a_ball_off_the_table() {
    let (g, report) = break_with(&[], &[5]);
    assert!(report.fouls.contains(&Foul::IllegalBreak), "{report:?}");
    let choice = BreakChoice::IllegalBreak { scratch: false, ball_off: true };
    assert_eq!(report.choice, Some(choice));
    assert!(choice.options().contains(&BreakOption::CueBallInHand));
    let mut as_lies = g.clone();
    as_lies.choose(BreakOption::PlayOn).unwrap();
    assert_eq!((as_lies.current_player(), as_lies.ball_in_hand()), (1, None));
    let mut in_hand = g;
    in_hand.choose(BreakOption::CueBallInHand).unwrap();
    assert_eq!((in_hand.current_player(), in_hand.ball_in_hand()), (1, Some(BallInHand::Kitchen)));
}

/// 4.3 (g): a ball driven off the table on the break is a foul and stays
/// off; the incoming player plays the table as it lies or takes the cue
/// ball in hand behind the head string. The 8 off the table is spotted.
#[test]
fn a_ball_off_the_table_on_the_break() {
    let (g, report) = break_with(&[3], &[5]);
    assert_eq!(report.fouls, vec![Foul::ObjectBallOffTable(BallId(5))], "{report:?}");
    assert_eq!(report.choice, Some(BreakChoice::BallOffOnBreak));
    assert_eq!(g.current_player(), 1);
    assert_eq!(g.ball(BallId(5)).status, BallStatus::OffTable, "it stays off");

    let mut as_lies = g.clone();
    as_lies.choose(BreakOption::PlayOn).unwrap();
    assert_eq!(as_lies.ball_in_hand(), None);
    let mut in_hand = g.clone();
    in_hand.choose(BreakOption::CueBallInHand).unwrap();
    assert_eq!(in_hand.ball_in_hand(), Some(BallInHand::Kitchen));

    let (g, report) = break_with(&[3], &[8]);
    assert!(report.spotted.contains(&BallId::EIGHT), "{report:?}");
    assert!(g.ball(BallId::EIGHT).in_play() && report.game_over.is_none());
}

// ---- 1.7, 4.4, 4.6 Calls and groups ------------------------------------

/// 4.6: a ball of one's group made in the called pocket keeps the table;
/// made in another pocket, or on a safety (nothing called), it stays down
/// and the turn passes, with no foul.
#[test]
fn only_the_called_pocket_keeps_the_table() {
    let at = [
        (0, DVec2::new(0.2, 0.1)),
        (3, DVec2::new(0.6, 0.25)),
        (5, DVec2::new(-0.5, -0.3)),
        (12, DVec2::new(-0.3, 0.3)),
        (8, DVec2::new(-0.7, 0.0)),
    ];
    let g = solids_to_shoot(&at);
    assert!(g.calls_shots());
    let pocket = PocketId::FootLeft;
    let shot = pot(&g, 3, pocket);

    let mut called = g.clone();
    let report = called.play_shot(&shot, Some(pocket)).unwrap();
    assert!(report.continues && report.fouls.is_empty(), "{report:?}");
    assert_eq!((report.called, called.current_player()), (Some(pocket), 0));

    for call in [Some(PocketId::HeadRight), None] {
        let mut other = g.clone();
        let report = other.play_shot(&shot, call).unwrap();
        assert!(!report.continues && report.fouls.is_empty(), "{call:?}: {report:?}");
        assert!(!other.ball(BallId(3)).in_play(), "the 3 stays down");
        assert_eq!((other.current_player(), other.ball_in_hand()), (1, None));
    }
}

/// 4.4: on an open table the group of the called ball made is the
/// shooter's; one made in another pocket decides nothing.
#[test]
fn the_called_ball_on_an_open_table_decides_the_groups() {
    let at = [
        (0, DVec2::new(0.2, 0.1)),
        (11, DVec2::new(0.6, 0.25)),
        (5, DVec2::new(-0.5, -0.3)),
        (8, DVec2::new(-0.7, 0.0)),
    ];
    let g = open(&at);
    let shot = pot(&g, 11, PocketId::FootLeft);
    let mut called = g.clone();
    let report = called.play_shot(&shot, Some(PocketId::FootLeft)).unwrap();
    assert_eq!(report.group_assigned, Some(Group::Stripes), "{report:?}");
    assert_eq!(
        (called.group_of(0), called.group_of(1)),
        (Some(Group::Stripes), Some(Group::Solids))
    );
    assert!(report.continues);

    let mut uncalled = g.clone();
    let report = uncalled.play_shot(&shot, Some(PocketId::SideLeft)).unwrap();
    assert_eq!(report.group_assigned, None);
    assert!(uncalled.is_open_table() && !report.continues);
}

/// 4.4: on an open table any ball may be hit first but the 8, unless a
/// group is off the table: then the shooter may claim it and play the 8.
#[test]
fn the_eight_on_an_open_table() {
    let mut g = open(&[
        (0, DVec2::new(-0.2, 0.0)),
        (8, DVec2::new(0.3, 0.2)),
        (3, DVec2::new(-0.5, 0.3)),
        (12, DVec2::new(0.5, -0.3)),
    ]);
    assert!(!g.legal_targets().contains(&BallId::EIGHT));
    let soft = Shot::new(straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 1.0).yaw, 0.8);
    let report = g.clone().play_shot(&soft, Some(PocketId::FootLeft)).unwrap();
    assert!(report.fouls.contains(&Foul::WrongBallFirst(BallId::EIGHT)), "{report:?}");

    // The solids gone: the 8 is fair game, and made in the called pocket it
    // wins.
    layout(
        &mut g,
        &[(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.3, 0.2)), (12, DVec2::new(0.5, -0.3))],
    );
    assert!(g.is_on_eight(0) && g.legal_targets().contains(&BallId::EIGHT));
    let report = g.clone().play_shot(&soft, Some(PocketId::FootLeft)).unwrap();
    assert!(!report.fouls.iter().any(|f| matches!(f, Foul::WrongBallFirst(_))), "{report:?}");
    let shot = pot(&g, 8, PocketId::FootLeft);
    let report = g.play_shot(&shot, Some(PocketId::FootLeft)).unwrap();
    assert_eq!(
        report.game_over,
        Some(GameOver { winner: 0, reason: GameOverReason::PocketedEight })
    );
}

// ---- 4.8 Losing the rack ------------------------------------------------

/// 4.8: the 8 made in the called pocket after one's group wins; made too
/// early, in a pocket not called (or on a safety), or on a foul, it loses;
/// so does the 8 off the table.
#[test]
fn losing_the_rack() {
    let on_the_eight =
        [(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.3, 0.2)), (12, DVec2::new(0.0, -0.3))];
    let g = solids_to_shoot(&on_the_eight);
    assert!(g.is_on_eight(0));
    let shot = pot(&g, 8, PocketId::FootLeft);
    let over =
        |mut g: EightBallGame, call: Option<PocketId>| g.play_shot(&shot, call).unwrap().game_over;
    let lost = |reason| Some(GameOver { winner: 1, reason });
    assert_eq!(
        over(g.clone(), Some(PocketId::FootLeft)),
        Some(GameOver { winner: 0, reason: GameOverReason::PocketedEight })
    );
    assert_eq!(over(g.clone(), Some(PocketId::HeadLeft)), lost(GameOverReason::EightInWrongPocket));
    assert_eq!(over(g.clone(), None), lost(GameOverReason::EightInWrongPocket));

    let early = solids_to_shoot(&[
        (0, DVec2::new(-0.2, 0.0)),
        (8, DVec2::new(0.3, 0.2)),
        (2, DVec2::new(-0.6, 0.35)),
    ]);
    let wrong = pot(&early, 8, PocketId::FootLeft);
    let report = early.clone().play_shot(&wrong, Some(PocketId::FootLeft)).unwrap();
    assert_eq!(
        report.game_over,
        lost(GameOverReason::EightPocketedOnFoul),
        "the 8 first is a foul too"
    );

    // The 8 made with a scratch.
    let mut pending = g.prepare_shot(&shot, Some(PocketId::FootLeft)).unwrap();
    pending.simulation.run_to_rest();
    pending.simulation.pocket_for_tests(BallId::CUE, PocketId::HeadLeft);
    let mut scratched = g.clone();
    let report = scratched.finish_shot(pending);
    assert_eq!(report.game_over, lost(GameOverReason::EightPocketedOnFoul));

    // The 8 off the table.
    let mut pending = g.prepare_shot(&Shot::new(shot.yaw, 0.5), Some(PocketId::FootLeft)).unwrap();
    pending.simulation.run_to_rest();
    pending.simulation.knock_off_for_tests(BallId::EIGHT);
    let mut off = g.clone();
    assert_eq!(off.finish_shot(pending).game_over, lost(GameOverReason::EightOffTable));
}

/// 4.8 (b): the last ball of one's group and the 8 on the same shot: the 8
/// went down before the group was cleared.
#[test]
fn the_last_ball_and_the_eight_together_lose() {
    let g = solids_to_shoot(&[
        (0, DVec2::new(-0.2, 0.0)),
        (2, DVec2::new(0.3, 0.2)),
        (8, DVec2::new(0.0, -0.3)),
    ]);
    let mut pending =
        g.prepare_shot(&pot(&g, 2, PocketId::FootLeft), Some(PocketId::FootLeft)).unwrap();
    pending.simulation.run_to_rest();
    pending.simulation.pocket_for_tests(BallId::EIGHT, PocketId::FootRight);
    let mut both = g.clone();
    let report = both.finish_shot(pending);
    assert_eq!(
        report.game_over,
        Some(GameOver { winner: 1, reason: GameOverReason::EightPocketedEarly })
    );
}

// ---- 4.9 Fouls ----------------------------------------------------------

/// 4.9: after a foul the incoming player has the cue ball in hand anywhere
/// on the table (1.6). A scratch, the wrong ball first.
#[test]
fn a_foul_gives_the_cue_ball_in_hand_anywhere() {
    let at = [
        (0, DVec2::new(-0.2, 0.0)),
        (3, DVec2::new(0.4, 0.2)),
        (12, DVec2::new(0.3, -0.25)),
        (8, DVec2::new(-0.7, 0.0)),
    ];
    let g = solids_to_shoot(&at);
    let mut wrong = g.clone();
    let report =
        wrong.play_shot(&straight_shot(&g, BallId(12), PocketId::FootRight, 1.0), None).unwrap();
    assert_eq!(report.fouls, vec![Foul::WrongBallFirst(BallId(12))], "{report:?}");
    assert_eq!((wrong.current_player(), wrong.ball_in_hand()), (1, Some(BallInHand::Anywhere)));
    assert!(wrong.place_cue_ball(DVec2::new(0.9, 0.0)), "anywhere: the foot of the table too");

    let mut pending = g
        .prepare_shot(
            &straight_shot(&g, BallId(3), PocketId::FootLeft, 1.5),
            Some(PocketId::FootLeft),
        )
        .unwrap();
    pending.simulation.run_to_rest();
    pending.simulation.pocket_for_tests(BallId::CUE, PocketId::SideRight);
    let mut scratch = g.clone();
    let report = scratch.finish_shot(pending);
    assert!(report.fouls.contains(&Foul::CueBallPocketed) && !report.continues);
    assert_eq!((scratch.current_player(), scratch.ball_in_hand()), (1, Some(BallInHand::Anywhere)));
    assert!(
        scratch.ball(BallId::CUE).in_play()
            && scratch.can_place_cue_ball(scratch.ball(BallId::CUE).xy())
    );
}

/// A ball lying against a cushion and pushed into it wasn't driven to it
/// (only by leaving it and coming back): no rail, a foul. The same ball a
/// little off the cushion is driven to it.
#[test]
fn a_ball_frozen_to_a_cushion_pushed_into_it_is_no_rail() {
    let spec = EightBallGame::new(3, 0, Rules::Official).table.spec;
    let r = spec.ball_radius;
    let dh = spec.cushion_height - r;
    let against = spec.width / 2.0 - (r * r - dh * dh).sqrt();
    let shot = Shot::new(std::f64::consts::FRAC_PI_2, 0.5);
    for (gap, foul) in [(0.0, true), (0.02, false)] {
        let y = against - gap;
        let mut g = solids_to_shoot(&[
            (0, DVec2::new(0.4, y - 0.25)),
            (3, DVec2::new(0.4, y)),
            (8, DVec2::new(-0.7, 0.0)),
        ]);
        let report = g.play_shot(&shot, None).unwrap();
        assert_eq!(report.first_contact, Some(BallId(3)), "{report:?}");
        assert_eq!(report.fouls.contains(&Foul::NoRailAfterContact), foul, "gap {gap}: {report:?}");
    }
}

/// 3.3: no ball made, and no ball driven to a cushion after the contact.
#[test]
fn no_rail_after_contact_is_a_foul() {
    let g = solids_to_shoot(&[
        (0, DVec2::new(-0.2, 0.0)),
        (3, DVec2::new(0.0, 0.0)),
        (8, DVec2::new(-0.7, 0.3)),
    ]);
    let mut touch = g.clone();
    let report = touch.play_shot(&Shot::new(0.0, 0.35), None).unwrap();
    assert_eq!(report.first_contact, Some(BallId(3)));
    assert_eq!(report.fouls, vec![Foul::NoRailAfterContact], "{report:?}");
    assert_eq!(touch.ball_in_hand(), Some(BallInHand::Anywhere));
}

/// 3.5, 4.7: an object ball driven off the table is a foul and stays off;
/// only the 8 is ever spotted, and only after the break.
#[test]
fn a_ball_off_the_table_stays_off() {
    let g = solids_to_shoot(&[
        (0, DVec2::new(-0.2, 0.0)),
        (3, DVec2::new(0.3, 0.2)),
        (12, DVec2::new(0.5, -0.3)),
        (8, DVec2::new(-0.7, 0.3)),
    ]);
    let mut pending =
        g.prepare_shot(&straight_shot(&g, BallId(3), PocketId::FootLeft, 1.0), None).unwrap();
    pending.simulation.run_to_rest();
    pending.simulation.knock_off_for_tests(BallId(3));
    let mut off = g.clone();
    let report = off.finish_shot(pending);
    assert!(report.fouls.contains(&Foul::ObjectBallOffTable(BallId(3))), "{report:?}");
    assert!(report.spotted.is_empty());
    assert_eq!(off.ball(BallId(3)).status, BallStatus::OffTable);
    assert_eq!(off.ball_in_hand(), Some(BallInHand::Anywhere));
}

/// 3.11: with the cue ball in hand behind the head string, a ball lying
/// behind it may be hit first only after the cue ball crossed it; one on
/// the far side is fair.
#[test]
fn a_ball_behind_the_head_string_needs_the_cue_ball_to_cross_it_first() {
    let mut g = open(&[
        (0, DVec2::new(-0.75, -0.3)),
        (4, DVec2::new(-0.75, 0.25)),
        (12, DVec2::new(0.3, 0.0)),
        (8, DVec2::new(0.6, 0.3)),
    ]);
    g.ball_in_hand = Some(BallInHand::Kitchen);
    assert!(g.ball(BallId(4)).xy().x < g.table.head_string_x());
    let up = Shot::new(std::f64::consts::FRAC_PI_2, 1.2);
    let report = g.clone().play_shot(&up, None).unwrap();
    assert_eq!(report.first_contact, Some(BallId(4)));
    assert!(report.fouls.contains(&Foul::BallBehindHeadString(BallId(4))), "{report:?}");
    let across = straight_shot(&g, BallId(12), PocketId::FootLeft, 2.0);
    let report = g.clone().play_shot(&across, None).unwrap();
    assert!(!report.fouls.iter().any(|f| matches!(f, Foul::BallBehindHeadString(_))), "{report:?}");

    // The simulation notes the crossing: a ball struck to the foot rail and
    // back crosses the line on its way down.
    let mut sim = Simulation::new(g.table.clone(), *g.params(), g.balls.clone());
    sim.watch_crossing(BallId::CUE, g.table.head_string_x());
    sim.strike(BallId::CUE, &Shot::new(0.0, 3.0)).unwrap();
    sim.run_to_rest();
    assert!(sim.crossed_at().is_some());
}

/// 1.6: in hand behind the head string with every ball one may hit lying
/// behind it too, the one nearest the head string is spotted.
#[test]
fn the_ball_nearest_the_head_string_is_spotted_when_all_lie_behind_it() {
    let mut g = open(&[
        (0, DVec2::new(0.4, 0.0)),
        (2, DVec2::new(-0.8, 0.3)),
        (10, DVec2::new(-0.65, -0.2)),
        (8, DVec2::new(0.6, 0.3)),
    ]);
    g.grant_hand(BallInHand::Kitchen);
    let ten = g.ball(BallId(10)).xy();
    assert!(ten.y == 0.0 && ten.x >= g.table.foot_spot().x, "the 10 spotted: {ten}");
    assert_eq!(g.ball(BallId(2)).xy(), DVec2::new(-0.8, 0.3), "the 2 stays");
    assert!(g.ball(BallId::CUE).xy().x <= g.table.head_string_x());
}

/// 3.14: the shot clock is a standard foul: the cue ball in hand anywhere
/// for the opponent; on the break they break; a choice waiting gets its
/// first option.
#[test]
fn a_clock_that_runs_out() {
    let mut g = solids_to_shoot(&[
        (0, DVec2::new(-0.2, 0.0)),
        (3, DVec2::new(0.3, 0.2)),
        (8, DVec2::new(-0.7, 0.3)),
    ]);
    g.time_foul();
    assert_eq!((g.current_player(), g.ball_in_hand()), (1, Some(BallInHand::Anywhere)));
    // A cue ball frozen against a ball can't be played from where it lies:
    // the ball in hand starts from a place it can.
    let r = g.table.spec.ball_radius;
    let mut f = solids_to_shoot(&[
        (0, DVec2::new(0.3 - 2.0 * r - 0.00005, 0.2)),
        (3, DVec2::new(0.3, 0.2)),
        (8, DVec2::new(-0.7, 0.3)),
    ]);
    f.time_foul();
    assert_eq!(f.ball_in_hand(), Some(BallInHand::Anywhere));
    assert!(f.can_place_cue_ball(f.ball(BallId::CUE).xy()));

    let mut b = EightBallGame::new(1, 0, Rules::Official);
    b.time_foul();
    assert!(b.is_break());
    assert_eq!((b.current_player(), b.ball_in_hand()), (1, Some(BallInHand::Kitchen)));

    let (mut c, _) = break_with(&[], &[]);
    c.time_foul();
    assert_eq!(c.pending_choice(), None);
    assert_eq!((c.current_player(), c.ball_in_hand()), (1, None), "the table as it lies");
}

// ---- Snapshots and random games -----------------------------------------

/// A choice waiting goes through a snapshot; a Brazilian game never waits
/// for one.
#[test]
fn snapshots_keep_the_rules_and_the_choice() {
    let (g, _) = break_with(&[], &[]);
    let json = serde_json::to_string(&g).unwrap();
    let back: EightBallGame = serde_json::from_str(&json).unwrap();
    assert_eq!(back, g);
    assert_eq!((back.rules(), back.pending_choice()), (Rules::Official, g.pending_choice()));
    let mut v = serde_json::to_value(&g).unwrap();
    v["rules"] = "brazilian".into();
    assert!(serde_json::from_value::<EightBallGame>(v).is_err());
}

/// Random games under the official rules, random calls and random answers
/// to the break's choices, in each of the three ways games were played
/// (every shot called, nothing called, only the 8 called): groups are only
/// ever decided by a called ball (with calls) or the first one made, a foul
/// always gives the cue ball in hand anywhere (behind the head string after
/// the break), a shot keeps the table only on its call (with calls), the 8
/// wins only in its called pocket (but from 0.11.2 to 0.11.x), and every
/// snapshot reads back.
#[test]
fn the_official_rules_hold_over_random_games() {
    let r = Table::default().spec.ball_radius;
    for seed in 0..60u64 {
        // A third of the games with the calls of the games recorded until
        // 0.11.1, a third with none (0.11.2 to 0.11.x), a third with the 8
        // called (since 0.12.0).
        let calls = seed % 3 == 0;
        let eight_call = seed % 3 == 2;
        let mut g = EightBallGame::new(seed, (seed % 2) as usize, Rules::Official)
            .with_calls(calls)
            .with_eight_call(eight_call);
        let mut rng = Rng::new(seed + 9000);
        for _ in 0..160 {
            if g.game_over().is_some() {
                break;
            }
            if let Some(choice) = g.pending_choice() {
                let options = choice.options();
                g.choose(options[rng.below(options.len())]).unwrap();
                continue;
            }
            if g.ball_in_hand().is_some() {
                for _ in 0..300 {
                    let p = DVec2::new(rng.next_f64() * 2.4 - 1.2, rng.next_f64() * 1.2 - 0.6);
                    if g.place_cue_ball(p) {
                        break;
                    }
                }
            }
            let before = g.clone();
            let shooter = g.current_player();
            let shot =
                Shot::new(rng.next_f64() * std::f64::consts::TAU, 0.3 + rng.next_f64() * 6.5)
                    .with_tip(DVec2::new(rng.next_f64() - 0.5, rng.next_f64() - 0.5));
            let call = (rng.below(5) > 0).then(|| PocketId::ALL[rng.below(6)]);
            let report = g.play_shot(&shot, call).unwrap();
            match (g.group_of(0), g.group_of(1)) {
                (None, None) => {}
                (Some(a), Some(b)) => assert_eq!(a, b.opposite()),
                other => panic!("half-assigned groups {other:?}"),
            }
            if let Some(gr) = report.group_assigned {
                assert!(!report.was_break && before.is_open_table());
                if calls {
                    let call = report.called.expect("only a called ball decides");
                    assert!(report.pocketed.iter().any(|(b, p)| *p == call && gr.contains(*b)));
                } else {
                    // The first of a group made, in any pocket.
                    let first = report.pocketed.iter().find_map(|(b, _)| Group::of(*b));
                    assert_eq!(first, Some(gr), "{report:?}");
                }
            }
            if !calls && !(eight_call && before.calls_the_eight()) {
                assert_eq!(report.called, None, "nothing is called");
            }
            if let Some(o) = report.game_over.filter(|o| o.reason == GameOverReason::PocketedEight)
            {
                assert_eq!(o.winner, shooter);
                if calls || eight_call {
                    assert!(before.calls_shots(), "{report:?}");
                    let eight = report.pocketed.iter().find(|(b, _)| *b == BallId::EIGHT);
                    assert_eq!(eight.map(|(_, p)| *p), report.called, "{report:?}");
                }
            }
            assert!(report.spotted.iter().all(|b| *b == BallId::EIGHT));
            if g.game_over().is_none() && g.pending_choice().is_none() {
                assert_eq!(g.current_player() == shooter, report.continues, "{report:?}");
                if report.is_foul() && !report.was_break {
                    assert_eq!(g.ball_in_hand(), Some(BallInHand::Anywhere), "{report:?}");
                }
                if !report.is_foul() && !report.was_break {
                    assert_eq!(g.ball_in_hand(), None);
                    let made_call = if calls {
                        report.called.is_some_and(|c| report.pocketed.iter().any(|(_, p)| *p == c))
                    } else {
                        report.pocketed.iter().any(|(b, _)| b.is_object())
                    };
                    assert!(!report.continues || made_call, "{report:?}");
                }
                if g.ball_in_hand().is_some() {
                    assert!(g.can_place_cue_ball(g.ball(BallId::CUE).xy()), "{report:?}");
                }
            }
            let back: EightBallGame =
                serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
            assert_eq!(back, g);
            let live: Vec<_> = g.balls().iter().filter(|b| b.in_play()).collect();
            for (k, a) in live.iter().enumerate() {
                for b in &live[k + 1..] {
                    // (Balls frozen together come to rest a hair apart, or
                    // a few hundredths of a millimetre into each other: the
                    // physics, which tests/invariants.rs watches.)
                    let d = a.xy().distance(b.xy());
                    assert!(d > 2.0 * r - 1e-4, "seed {seed}: {} {}: {d}", a.id, b.id);
                }
            }
        }
    }
}

// ---- No calls (0.11.2 to 0.11.x) ---------------------------------------

/// The games played from 0.11.2 to 0.11.x: nothing is called, a ball of one's group
/// made in any pocket keeps the table, whatever pocket the game was told.
#[test]
fn without_calls_any_pocket_keeps_the_table() {
    let at = [
        (0, DVec2::new(0.2, 0.1)),
        (3, DVec2::new(0.6, 0.25)),
        (5, DVec2::new(-0.5, -0.3)),
        (12, DVec2::new(-0.3, 0.3)),
        (8, DVec2::new(-0.7, 0.0)),
    ];
    let g = solids_to_shoot(&at).with_calls(false).with_eight_call(false);
    assert!(!g.calls_shots() && !g.calls());
    let shot = pot(&g, 3, PocketId::FootLeft);
    for call in [Some(PocketId::FootLeft), Some(PocketId::HeadRight), None] {
        let mut played = g.clone();
        let report = played.play_shot(&shot, call).unwrap();
        assert!(report.continues && report.fouls.is_empty(), "{call:?}: {report:?}");
        assert_eq!((report.called, played.current_player()), (None, 0));
    }
    // A ball of the opponent's alone: the turn passes, no foul.
    let theirs = solids_to_shoot(&[
        (0, DVec2::new(0.2, 0.1)),
        (12, DVec2::new(0.6, 0.25)),
        (3, DVec2::new(-0.5, -0.3)),
        (8, DVec2::new(-0.7, 0.0)),
    ])
    .with_calls(false)
    .with_eight_call(false);
    let mut played = theirs.clone();
    let shot = Shot::new(straight_shot(&theirs, BallId(12), PocketId::FootLeft, 2.0).yaw, 2.0);
    let report = played.play_shot(&shot, None).unwrap();
    if report.pocketed.iter().all(|(b, _)| b.0 == 12) && !report.pocketed.is_empty() {
        assert!(!report.continues && played.current_player() == 1, "{report:?}");
    }
}

/// Without calls, on an open table the first ball made decides the groups,
/// in any pocket.
#[test]
fn without_calls_the_first_ball_made_decides_the_groups() {
    let g = open(&[
        (0, DVec2::new(0.2, 0.1)),
        (11, DVec2::new(0.6, 0.25)),
        (5, DVec2::new(-0.5, -0.3)),
        (8, DVec2::new(-0.7, 0.0)),
    ])
    .with_calls(false)
    .with_eight_call(false);
    let shot = pot(&g, 11, PocketId::FootLeft);
    let mut played = g.clone();
    let report = played.play_shot(&shot, Some(PocketId::SideLeft)).unwrap();
    assert_eq!(report.group_assigned, Some(Group::Stripes), "{report:?}");
    assert!(report.continues);
}

/// Without calls, the 8 made after one's group wins in any pocket; made
/// too early or on a foul it still loses.
#[test]
fn without_calls_the_eight_wins_in_any_pocket() {
    let g = solids_to_shoot(&[
        (0, DVec2::new(-0.2, 0.0)),
        (8, DVec2::new(0.3, 0.2)),
        (12, DVec2::new(0.0, -0.3)),
    ])
    .with_calls(false)
    .with_eight_call(false);
    let shot = pot(&g, 8, PocketId::FootLeft);
    for call in [Some(PocketId::FootLeft), Some(PocketId::HeadLeft), None] {
        assert_eq!(
            g.clone().play_shot(&shot, call).unwrap().game_over,
            Some(GameOver { winner: 0, reason: GameOverReason::PocketedEight }),
            "{call:?}"
        );
    }
    let early = solids_to_shoot(&[
        (0, DVec2::new(-0.2, 0.0)),
        (8, DVec2::new(0.3, 0.2)),
        (2, DVec2::new(-0.6, 0.35)),
    ])
    .with_calls(false)
    .with_eight_call(false);
    let wrong = pot(&early, 8, PocketId::FootLeft);
    let over = early.clone().play_shot(&wrong, None).unwrap().game_over;
    assert!(over.is_some_and(|o| o.winner == 1), "{over:?}");
}

/// A snapshot of a new game (no calls but the 8's) reads back so; one
/// written before the 8's call (0.11.x) has none, and one written before
/// the calls' field (0.11.1 and earlier) is a game with calls.
#[test]
fn a_snapshot_keeps_its_calls() {
    let g = EightBallGame::new(5, 0, Rules::Official);
    assert!(!g.calls() && g.eight_call());
    let text = serde_json::to_string(&g).unwrap();
    let back: EightBallGame = serde_json::from_str(&text).unwrap();
    assert!(!back.calls() && back.eight_call());
    let mut old: serde_json::Value = serde_json::from_str(&text).unwrap();
    old.as_object_mut().unwrap().remove("eight_call");
    let without: EightBallGame = serde_json::from_value(old.clone()).unwrap();
    assert!(!without.calls() && !without.eight_call());
    old.as_object_mut().unwrap().remove("calls");
    let old: EightBallGame = serde_json::from_value(old).unwrap();
    assert!(old.calls());
}

// ---- The 8 called (since 0.12.0) ----------------------------------------

/// The 8 alone with the solids' player to shoot, in a new game.
fn on_the_eight() -> EightBallGame {
    solids_to_shoot(&[
        (0, DVec2::new(-0.2, 0.0)),
        (8, DVec2::new(0.3, 0.2)),
        (12, DVec2::new(0.0, -0.3)),
    ])
    .with_calls(false)
    .with_eight_call(true)
}

/// A new game calls the 8 and nothing else: the group's balls count in any
/// pocket (a call sent with them is dropped), a shot at the 8 is called.
#[test]
fn only_the_eight_is_called() {
    assert!(EightBallGame::new(1, 0, Rules::Official).eight_call());
    let g = solids_to_shoot(&[
        (0, DVec2::new(0.2, 0.1)),
        (3, DVec2::new(0.6, 0.25)),
        (5, DVec2::new(-0.5, -0.3)),
        (12, DVec2::new(-0.3, 0.3)),
        (8, DVec2::new(-0.7, 0.0)),
    ])
    .with_calls(false);
    assert!(g.eight_call() && !g.calls_shots() && !g.calls_the_eight());
    let shot = pot(&g, 3, PocketId::FootLeft);
    for call in [Some(PocketId::FootLeft), Some(PocketId::HeadRight), None] {
        let mut played = g.clone();
        let report = played.play_shot(&shot, call).unwrap();
        assert!(report.continues && report.fouls.is_empty(), "{call:?}: {report:?}");
        assert_eq!((report.called, played.current_player()), (None, 0));
    }
    // On the 8 the shot is called; not on the break, and not in a game
    // with every shot called (that is its own call).
    let g = on_the_eight();
    assert!(g.calls_shots() && g.calls_the_eight());
    assert!(!g.clone().with_calls(true).calls_the_eight());
    assert!(!EightBallGame::new(1, 0, Rules::Official).calls_shots());
    let mut brazilian = g.clone();
    brazilian.rules = Rules::Brazilian;
    assert!(!brazilian.calls_shots());
    // The Brazilian rules' call of the 8 has nothing to do with it.
    assert_eq!(g.called_pocket_of(0), None);
}

/// 4.8: the 8 made in the called pocket on a legal shot wins; made in
/// another pocket, or on a shot that called none, it loses; missed, the
/// turn passes as on any shot.
#[test]
fn the_eight_wins_only_in_the_called_pocket() {
    let g = on_the_eight();
    let shot = pot(&g, 8, PocketId::FootLeft);
    let play = |call: Option<PocketId>| {
        let mut played = g.clone();
        let report = played.play_shot(&shot, call).unwrap();
        (report, played)
    };
    let (report, played) = play(Some(PocketId::FootLeft));
    assert_eq!(
        report.game_over,
        Some(GameOver { winner: 0, reason: GameOverReason::PocketedEight })
    );
    assert_eq!((report.called, played.game_over()), (Some(PocketId::FootLeft), report.game_over));
    let lost = Some(GameOver { winner: 1, reason: GameOverReason::EightInWrongPocket });
    assert_eq!(play(Some(PocketId::HeadLeft)).0.game_over, lost);
    assert_eq!(play(None).0.game_over, lost);
    // Missed: the turn passes, nothing more.
    let soft = Shot::new(shot.yaw + 0.2, 0.9);
    let mut missed = g.clone();
    let report = missed.play_shot(&soft, Some(PocketId::FootLeft)).unwrap();
    if !report.pocketed.iter().any(|(b, _)| *b == BallId::EIGHT) && !report.is_foul() {
        assert_eq!(report.game_over, None, "{report:?}");
        assert_eq!(missed.current_player(), 1);
    }
    // Made too early it still loses, whatever was called.
    let early = solids_to_shoot(&[
        (0, DVec2::new(-0.2, 0.0)),
        (8, DVec2::new(0.3, 0.2)),
        (2, DVec2::new(-0.6, 0.35)),
    ])
    .with_calls(false);
    assert!(!early.calls_shots());
    let wrong = pot(&early, 8, PocketId::FootLeft);
    let over = early.clone().play_shot(&wrong, Some(PocketId::FootLeft)).unwrap().game_over;
    assert!(over.is_some_and(|o| o.winner == 1), "{over:?}");
}

/// 4.4 with the 8 called: on an open table with a group off it, the
/// shooter who plays the 8 calls it too.
#[test]
fn the_eight_on_an_open_table_is_called() {
    let g =
        open(&[(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.3, 0.2)), (12, DVec2::new(0.5, -0.3))])
            .with_calls(false)
            .with_eight_call(true);
    assert!(g.is_on_eight(0) && g.calls_the_eight());
    let shot = pot(&g, 8, PocketId::FootLeft);
    let won = g.clone().play_shot(&shot, Some(PocketId::FootLeft)).unwrap().game_over;
    assert_eq!(won, Some(GameOver { winner: 0, reason: GameOverReason::PocketedEight }));
    let lost = g.clone().play_shot(&shot, Some(PocketId::SideRight)).unwrap().game_over;
    assert_eq!(lost, Some(GameOver { winner: 1, reason: GameOverReason::EightInWrongPocket }));
}

/// A game that can't call the 8 (older than 0.12.0, against a newer
/// server): the 8 counts as called in the pocket it went into, and the
/// call is the shot's (the game replays the same with it).
#[test]
fn an_older_game_has_the_eight_called_where_it_went() {
    let g = on_the_eight();
    let shot = pot(&g, 8, PocketId::FootLeft);
    let mut pending = g.prepare_shot(&shot, None).unwrap();
    pending.simulation.run_to_rest();
    g.call_the_eight_where_it_went(&mut pending);
    assert_eq!(pending.called_pocket, Some(PocketId::FootLeft));
    let mut played = g.clone();
    let report = played.finish_shot(pending);
    assert_eq!(
        report.game_over,
        Some(GameOver { winner: 0, reason: GameOverReason::PocketedEight })
    );
    let mut again = g.clone();
    assert_eq!(again.play_shot(&shot, report.called).unwrap(), report);
    // Missed, nothing is called; and a game without the 8's call calls
    // nothing either.
    let mut pending = g.prepare_shot(&Shot::new(shot.yaw + 0.3, 0.8), None).unwrap();
    pending.simulation.run_to_rest();
    g.call_the_eight_where_it_went(&mut pending);
    let eight_down = pending
        .simulation
        .events()
        .iter()
        .any(|e| matches!(e, Event::Pocketed { ball, .. } if *ball == BallId::EIGHT));
    if !eight_down {
        assert_eq!(pending.called_pocket, None);
    }
    let without = on_the_eight().with_eight_call(false);
    let mut pending = without.prepare_shot(&shot, None).unwrap();
    pending.simulation.run_to_rest();
    without.call_the_eight_where_it_went(&mut pending);
    assert_eq!(pending.called_pocket, None);
}
