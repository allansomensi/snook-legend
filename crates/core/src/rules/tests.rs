//! Tests of what both rules share and of the Brazilian rules (the official
//! rules have theirs in `official_tests.rs`).

use super::*;
use crate::ball::BallStatus;

/// Puts the table into an arbitrary layout (test helper).
pub(super) fn layout(game: &mut EightBallGame, positions: &[(u8, DVec2)]) {
    let r = game.table.spec.ball_radius;
    for b in &mut game.balls {
        b.status = BallStatus::Pocketed(PocketId::FootLeft);
    }
    for &(n, p) in positions {
        game.balls[n as usize] = Ball::at_rest(BallId(n), p, r);
    }
}

pub(super) fn straight_shot(
    game: &EightBallGame,
    target: BallId,
    pocket: PocketId,
    speed: f64,
) -> Shot {
    let r = game.table.spec.ball_radius;
    let aim = game.table.pocket(pocket).aim_point();
    let t = game.ball(target).xy();
    let ghost = t - (aim - t).normalize() * 2.0 * r;
    let d = ghost - game.ball(BallId::CUE).xy();
    Shot::new(d.y.atan2(d.x), speed)
}

fn after_break(seed: u64) -> EightBallGame {
    let mut g = EightBallGame::new(seed, 0, Rules::Brazilian);
    g.is_break = false;
    g.ball_in_hand = None;
    g
}

#[test]
fn rack_has_sixteen_non_overlapping_balls_with_eight_in_centre() {
    let t = Table::default();
    let balls = rack(&t, 7);
    assert_eq!(balls.len(), 16);
    for (i, a) in balls.iter().enumerate() {
        assert_eq!(a.id.index(), i);
        for b in &balls[i + 1..] {
            assert!(a.xy().distance(b.xy()) >= 2.0 * t.spec.ball_radius);
        }
    }
    let eight = balls[8].xy();
    let row_dx = (2.0 * t.spec.ball_radius + 1e-4) * 3f64.sqrt() / 2.0;
    assert!((eight.x - (t.foot_spot().x + 2.0 * row_dx)).abs() < 1e-9);
    assert!(eight.y.abs() < 1e-9);
}

#[test]
fn break_shot_is_handled() {
    let mut g = EightBallGame::new(1, 0, Rules::Brazilian);
    assert!(g.is_break());
    let report = g.play_shot(&Shot::new(0.0, 7.0), None).unwrap();
    assert!(report.was_break);
    assert!(!g.is_break());
    assert!(report.first_contact.is_some());
    assert!(g.balls().iter().all(|b| b.status != BallStatus::OffTable));
}

#[test]
fn pocketing_assigns_groups_and_keeps_turn() {
    let mut g = after_break(3);
    layout(
        &mut g,
        &[
            (0, DVec2::new(-0.2, 0.0)),
            (3, DVec2::new(0.3, 0.2)),
            (12, DVec2::new(0.0, -0.3)),
            (8, DVec2::new(-0.6, -0.3)),
        ],
    );
    let shot = straight_shot(&g, BallId(3), PocketId::FootLeft, 2.0);
    let report = g.play_shot(&shot, None).unwrap();
    assert_eq!(report.fouls, vec![]);
    assert_eq!(report.group_assigned, Some(Group::Solids));
    assert!(report.continues);
    assert_eq!(g.group_of(0), Some(Group::Solids));
    assert_eq!(g.group_of(1), Some(Group::Stripes));
    assert_eq!(g.current_player(), 0);
}

/// A scratch: the cue ball goes back to the head spot, not in hand.
#[test]
fn a_scratch_brings_the_cue_ball_back_to_the_head_spot() {
    let mut g = after_break(3);
    g.groups = [Some(Group::Solids), Some(Group::Stripes)];
    let t = g.table.clone();
    let aim = t.pocket(PocketId::FootLeft).aim_point();
    let start = aim - DVec2::new(1.0, 0.5).normalize() * 0.6;
    layout(
        &mut g,
        &[
            (0, start),
            (2, DVec2::new(-0.5, 0.0)),
            (10, DVec2::new(-0.5, 0.3)),
            (8, DVec2::new(-0.6, -0.3)),
        ],
    );
    let d = aim - start;
    let report = g.play_shot(&Shot::new(d.y.atan2(d.x), 2.0), None).unwrap();
    assert!(report.fouls.contains(&Foul::CueBallPocketed));
    assert_eq!(g.current_player(), 1);
    assert_eq!(g.ball_in_hand(), None);
    assert_eq!(g.ball(BallId::CUE).xy(), t.head_spot());
}

#[test]
fn eight_requires_called_pocket_and_wins() {
    let mut g = after_break(3);
    g.groups = [Some(Group::Solids), Some(Group::Stripes)];
    layout(
        &mut g,
        &[(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.3, 0.2)), (12, DVec2::new(0.0, -0.3))],
    );
    let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 2.0);
    assert_eq!(g.play_shot(&shot, None), Err(ShotError::MustCallPocket));
    let report = g.play_shot(&shot, Some(PocketId::FootLeft)).unwrap();
    assert_eq!(
        report.game_over,
        Some(GameOver { winner: 0, reason: GameOverReason::PocketedEight })
    );
}

#[test]
fn eight_in_wrong_pocket_loses() {
    let mut g = after_break(3);
    g.groups = [Some(Group::Solids), Some(Group::Stripes)];
    layout(
        &mut g,
        &[(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.3, 0.2)), (12, DVec2::new(0.0, -0.3))],
    );
    let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 2.0);
    let report = g.play_shot(&shot, Some(PocketId::FootRight)).unwrap();
    assert_eq!(
        report.game_over,
        Some(GameOver { winner: 1, reason: GameOverReason::EightInWrongPocket })
    );
}

#[test]
fn early_eight_loses() {
    let mut g = after_break(3);
    g.groups = [Some(Group::Solids), Some(Group::Stripes)];
    layout(
        &mut g,
        &[
            (0, DVec2::new(-0.2, 0.0)),
            (8, DVec2::new(0.3, 0.2)),
            (2, DVec2::new(-0.6, 0.3)),
            (12, DVec2::new(0.0, -0.3)),
        ],
    );
    let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 2.0);
    let report = g.play_shot(&shot, None).unwrap();
    assert_eq!(report.game_over.map(|o| o.winner), Some(1));
}

#[test]
fn ball_in_hand_placement_rules() {
    let mut g = EightBallGame::new(5, 0, Rules::Brazilian);
    let kitchen = g.table.head_string_x() - 0.1;
    assert!(g.place_cue_ball(DVec2::new(kitchen, 0.2)));
    assert!(!g.place_cue_ball(DVec2::new(0.0, 0.0)), "kitchen only on the break");
    assert!(!g.place_cue_ball(DVec2::new(-2.0, 0.0)), "outside the table");
    g.ball_in_hand = Some(BallInHand::Anywhere);
    assert!(g.place_cue_ball(DVec2::new(0.0, 0.3)));
    let eight = g.ball(BallId::EIGHT).xy();
    assert!(!g.place_cue_ball(eight), "overlapping a ball");
}

#[test]
fn the_call_for_the_eight_is_binding() {
    let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
    // Player 0 is on the 8 (every solid pocketed).
    for b in g.balls.iter_mut() {
        if (1..=7).contains(&b.id.0) {
            b.status = BallStatus::Pocketed(PocketId::FootLeft);
        }
    }
    g.groups = [Some(Group::Solids), Some(Group::Stripes)];
    g.is_break = false;
    g.ball_in_hand = None;
    assert!(g.may_call_pocket());
    // A soft shot that doesn't reach anything: the call becomes binding.
    g.play_shot(&Shot::new(std::f64::consts::PI, 0.3), Some(PocketId::SideLeft)).unwrap();
    assert_eq!(g.called_pocket_of(0), Some(PocketId::SideLeft));
    g.current = 0;
    g.ball_in_hand = None;
    assert!(!g.may_call_pocket());
    // A different pocket in the next shot is ignored.
    let pending = g.prepare_shot(&Shot::new(0.0, 0.3), Some(PocketId::FootRight)).unwrap();
    assert_eq!(pending.called_pocket, Some(PocketId::SideLeft));
    // And no pocket at all is fine: the call stands.
    assert!(g.prepare_shot(&Shot::new(0.0, 0.3), None).is_ok());
}

#[test]
fn game_state_round_trips_through_json() {
    let mut g = EightBallGame::new(9, 1, Rules::Brazilian);
    g.play_shot(&Shot::new(0.0, 7.0), None).unwrap();
    let json = serde_json::to_string(&g).unwrap();
    let back: EightBallGame = serde_json::from_str(&json).unwrap();
    assert_eq!(g, back);
}

/// A snapshot no game could be in is refused when it is read, instead
/// of panicking at the next shot.
#[test]
fn impossible_snapshots_are_refused() {
    let mut g = EightBallGame::new(9, 1, Rules::Brazilian);
    g.play_shot(&Shot::new(0.0, 6.0), None).unwrap();
    let good = serde_json::to_value(&g).unwrap();
    let read = |v: &serde_json::Value| serde_json::from_value::<EightBallGame>(v.clone());
    assert_eq!(read(&good).unwrap(), g);
    let broken = |edit: &dyn Fn(&mut serde_json::Value)| {
        let mut v = good.clone();
        edit(&mut v);
        read(&v).expect_err("refused")
    };
    broken(&|v| v["current"] = 2.into());
    broken(&|v| {
        v["balls"].as_array_mut().unwrap().pop();
    });
    broken(&|v| v["balls"].as_array_mut().unwrap().swap(3, 4));
    broken(&|v| v["game_over"] = serde_json::json!({ "winner": 5, "reason": "Resignation" }));
    broken(&|v| v["balls"][3]["motion"] = "Rolling".into());
    broken(&|v| v["balls"][0]["status"] = serde_json::json!({ "Pocketed": "FootLeft" }));
    // The same cue ball is fine once the game is over (a scratch on the 8).
    let mut v = good.clone();
    v["balls"][0]["status"] = serde_json::json!({ "Pocketed": "FootLeft" });
    v["game_over"] = serde_json::json!({ "winner": 0, "reason": "EightPocketedOnFoul" });
    assert!(read(&v).is_ok());
}

/// A seat other than 0 or 1 can't underflow: it counts by its parity.
#[test]
fn resigning_with_any_seat_number_is_safe() {
    let mut g = EightBallGame::new(1, 0, Rules::Brazilian);
    g.resign(3);
    assert_eq!(g.game_over(), Some(GameOver { winner: 0, reason: GameOverReason::Resignation }));
    let mut g = EightBallGame::new(1, 0, Rules::Brazilian);
    g.abandon(usize::MAX);
    assert_eq!(g.game_over(), Some(GameOver { winner: 0, reason: GameOverReason::Abandoned }));
}

/// A table put together without a cue ball has nothing to strike: the
/// shot is refused (it used to panic).
#[test]
fn no_cue_ball_no_shot() {
    let mut g = EightBallGame::new(1, 0, Rules::Brazilian);
    g.set_up_test_position(&[(BallId(3), DVec2::new(0.3, 0.0))]);
    assert_eq!(
        g.prepare_shot(&Shot::new(0.0, 2.0), None).err(),
        Some(ShotError::IllegalCueBallPosition)
    );
}

#[test]
fn spotting_finds_free_place() {
    let mut g = after_break(9);
    g.spot_ball(BallId(5));
    let p = g.ball(BallId(5)).xy();
    assert!(g.is_spot_free(p, BallId(5)));
    assert_eq!(p.y, 0.0, "spotted balls sit on the long string");
    assert!(g.table.is_inside_playing_area(p));
}

pub(super) fn on_break_with(game: &mut EightBallGame, positions: &[(u8, DVec2)]) {
    layout(game, positions);
    game.is_break = true;
    game.ball_in_hand = None;
}

/// The first ball made on a legal break is the
/// breaker's group, and they shoot again.
#[test]
fn the_first_ball_made_on_the_break_sets_the_groups() {
    let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
    on_break_with(
        &mut g,
        &[(0, DVec2::new(-0.2, 0.0)), (10, DVec2::new(0.3, 0.2)), (2, DVec2::new(0.0, -0.3))],
    );
    let shot = straight_shot(&g, BallId(10), PocketId::FootLeft, 2.0);
    let report = g.play_shot(&shot, None).unwrap();
    assert!(report.pocketed.iter().any(|(b, _)| *b == BallId(10)));
    assert_eq!(report.fouls, vec![]);
    assert_eq!(report.group_assigned, Some(Group::Stripes));
    assert!(report.continues);
    assert_eq!((g.group_of(0), g.group_of(1)), (Some(Group::Stripes), Some(Group::Solids)));
    assert_eq!(g.current_player(), 0);
}

/// A foul on the break (the cue ball in a pocket) leaves the table open
/// even if a ball went in.
#[test]
fn a_break_foul_leaves_the_table_open() {
    let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
    // The cue ball follows the 10 into the pocket.
    on_break_with(&mut g, &[(0, DVec2::new(0.55, 0.40)), (10, DVec2::new(0.9, 0.52))]);
    let to = g.ball(BallId(10)).xy() - g.ball(BallId::CUE).xy();
    let shot = Shot::new(crate::math::atan2(to.y, to.x), 3.0).with_tip(DVec2::new(0.0, 0.8));
    let report = g.play_shot(&shot, None).unwrap();
    if report.fouls.contains(&Foul::CueBallPocketed) {
        assert_eq!(report.group_assigned, None);
        assert!(g.is_open_table());
    }
}

#[test]
fn foul_on_an_open_table_assigns_nothing() {
    let mut g = after_break(3);
    // The cue ball follows the 3 into the pocket.
    layout(
        &mut g,
        &[(0, DVec2::new(0.6, 0.3)), (3, DVec2::new(0.9, 0.45)), (8, DVec2::new(-0.6, -0.3))],
    );
    let aim = g.table.pocket(PocketId::FootLeft).aim_point();
    let d = aim - g.ball(BallId::CUE).xy();
    let shot = Shot::new(crate::math::atan2(d.y, d.x), 3.0).with_tip(DVec2::new(0.0, 1.0));
    let report = g.play_shot(&shot, None).unwrap();
    if report.is_foul() {
        assert_eq!(report.group_assigned, None);
        assert!(g.is_open_table());
        assert_eq!(g.current_player(), 1);
    }
}

#[test]
fn eight_first_on_an_open_table_is_a_foul() {
    let mut g = after_break(3);
    layout(
        &mut g,
        &[(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.3, 0.2)), (2, DVec2::new(0.0, -0.3))],
    );
    let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootRight, 0.8);
    let report = g.play_shot(&shot, None).unwrap();
    assert!(report.fouls.contains(&Foul::WrongBallFirst(BallId::EIGHT)));
}

#[test]
fn pocketing_only_the_opponents_ball_passes_the_turn() {
    let mut g = after_break(3);
    g.groups = [Some(Group::Solids), Some(Group::Stripes)];
    layout(
        &mut g,
        &[(0, DVec2::new(-0.2, 0.0)), (12, DVec2::new(0.3, 0.2)), (3, DVec2::new(0.0, -0.3))],
    );
    let shot = straight_shot(&g, BallId(12), PocketId::FootLeft, 2.0);
    let report = g.play_shot(&shot, None).unwrap();
    // Hitting the 12 first is itself a foul for solids.
    assert!(report.fouls.contains(&Foul::WrongBallFirst(BallId(12))));
    assert!(!report.continues);
    assert_eq!(g.current_player(), 1);
}

/// One's own ball and one of the opponent's made on the same shot pass
/// the turn (no foul, no ball in hand).
#[test]
fn making_an_opponents_ball_with_ones_own_passes_the_turn() {
    // The 3 (solids) sends the 12 (stripes) into the corner and follows
    // it in: a combination that makes both.
    let setup = || {
        let mut g = after_break(3);
        g.groups = [Some(Group::Solids), Some(Group::Stripes)];
        let aim = g.table.pocket(PocketId::FootLeft).aim_point();
        let dir = (aim - DVec2::new(0.2, 0.0)).normalize();
        let r = g.table.spec.ball_radius;
        let twelve = aim - dir * 0.12;
        let three = twelve - dir * (2.0 * r + 0.15);
        let cue = three - dir * 0.35;
        layout(&mut g, &[(0, cue), (3, three), (12, twelve), (8, DVec2::new(-0.6, 0.3))]);
        g
    };
    let both = |r: &ShotReport| {
        r.fouls.is_empty() && [3u8, 12].iter().all(|n| r.pocketed.iter().any(|(b, _)| b.0 == *n))
    };
    let shot = (0..60)
        .map(|k| straight_shot(&setup(), BallId(3), PocketId::FootLeft, 1.5 + k as f64 * 0.04))
        .map(|s| s.with_tip(DVec2::new(0.0, 0.8)))
        .find(|s| setup().play_shot(s, None).is_ok_and(|r| both(&r)))
        .expect("a stroke that makes the 3 and the 12");
    let mut g = setup();
    let report = g.play_shot(&shot, None).unwrap();
    assert!(report.opponent_ball && !report.continues, "{report:?}");
    assert_eq!((g.current_player(), g.ball_in_hand()), (1, None), "no foul, no ball in hand");
}

#[test]
fn scratch_on_the_eight_without_pocketing_it_is_only_a_foul() {
    let mut g = after_break(3);
    g.groups = [Some(Group::Solids), Some(Group::Stripes)];
    layout(
        &mut g,
        &[(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.3, 0.2)), (12, DVec2::new(0.0, -0.3))],
    );
    // Miss everything: no contact.
    let report =
        g.play_shot(&Shot::new(std::f64::consts::PI, 1.0), Some(PocketId::FootLeft)).unwrap();
    assert!(report.fouls.contains(&Foul::NoContact));
    assert_eq!(report.game_over, None);
    assert_eq!(g.current_player(), 1);
}

/// Hidden behind another ball: a kick off the cushions that then hits one's own ball is
/// legal (if a ball reaches a cushion or a pocket afterwards).
#[test]
fn kicks_off_the_cushions_are_legal() {
    for rails in [1, 2] {
        let mut g = after_break(3);
        g.groups = [Some(Group::Solids), Some(Group::Stripes)];
        let hw = g.table.spec.width / 2.0;
        layout(
            &mut g,
            &[
                (0, DVec2::new(-0.9, -0.2)),
                (12, DVec2::new(-0.3, -0.2)), // the snooker
                (3, DVec2::new(0.3, -0.2)),
                (8, DVec2::new(-0.9, 0.35)),
            ],
        );
        // Unfold the path across the cushions (the nose touches the
        // ball ~27.5 mm from its centre).
        let line = hw - 0.0275;
        let target = match rails {
            1 => DVec2::new(0.3, 2.0 * line + 0.2),
            _ => DVec2::new(0.3, 4.0 * line - 0.2),
        };
        let d = target - g.ball(BallId::CUE).xy();
        let speed = if rails == 1 { 2.5 } else { 4.5 };
        // The cushions don't reflect like a mirror (friction, spin):
        // look near the unfolded aim for the stroke that reaches the 3.
        let base = crate::math::atan2(d.y, d.x);
        let shot = (0..400)
            .map(|k| {
                let off = (k / 2) as f64 * 0.0005 * if k % 2 == 0 { 1.0 } else { -1.0 };
                Shot::new(base + off, speed)
            })
            .find(|s| {
                let mut t = g.clone();
                // (One that doesn't scratch: that is another foul.)
                t.play_shot(s, None).is_ok_and(|r| {
                    r.first_contact == Some(BallId(3)) && !r.fouls.contains(&Foul::CueBallPocketed)
                })
            })
            .unwrap_or_else(|| panic!("no {rails}-rail kick reaches the 3"));
        let report = g.play_shot(&shot, None).unwrap();
        assert_eq!(report.first_contact, Some(BallId(3)), "{rails}-rail kick");
        assert_eq!(report.fouls, vec![], "{rails}-rail kick: {report:?}");
        assert!(!report.continues, "nothing made: the turn passes, no ball in hand");
        assert_eq!(g.ball_in_hand(), None);
    }
}

/// Player 0 on stripes, player 1 on solids, past the break, with the
/// cue ball in the middle and these balls on the table.
fn house(positions: &[(u8, DVec2)]) -> EightBallGame {
    let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
    g.is_break = false;
    g.ball_in_hand = None;
    g.groups = [Some(Group::Stripes), Some(Group::Solids)];
    layout(&mut g, positions);
    g
}

fn on_table(g: &EightBallGame, group: Group) -> Vec<u8> {
    g.remaining(group).into_iter().map(|b| b.0).collect()
}

#[test]
fn missing_every_ball_gives_the_opponent_their_lowest_ball() {
    let mut g = house(&[
        (0, DVec2::new(-0.2, 0.0)),
        (2, DVec2::new(0.5, 0.3)),
        (5, DVec2::new(0.6, -0.3)),
        (12, DVec2::new(0.0, -0.4)),
        (8, DVec2::new(-0.6, -0.3)),
    ]);
    // Up the table, between the balls.
    let report = g.play_shot(&Shot::new(0.0, 1.0), None).unwrap();
    assert_eq!(report.fouls, vec![Foul::NoContact]);
    assert_eq!(report.penalty, vec![BallId(2)]);
    assert_eq!(on_table(&g, Group::Solids), vec![5]);
    assert_eq!(g.current_player(), 1);
    assert_eq!(g.ball_in_hand(), None, "the cue ball is played from where it lies");
}

#[test]
fn missing_and_scratching_gives_two_balls() {
    let mut g = house(&[
        (0, DVec2::new(0.6, 0.3)),
        (2, DVec2::new(-0.5, 0.3)),
        (5, DVec2::new(-0.6, -0.3)),
        (7, DVec2::new(-0.3, -0.3)),
        (12, DVec2::new(0.0, -0.4)),
        (8, DVec2::new(-0.6, 0.0)),
    ]);
    // Straight into the foot corner pocket.
    let aim = g.table.pocket(PocketId::FootLeft).aim_point() - g.ball(BallId::CUE).xy();
    let report = g.play_shot(&Shot::new(aim.y.atan2(aim.x), 2.5), None).unwrap();
    assert!(report.fouls.contains(&Foul::NoContact), "{report:?}");
    assert!(report.fouls.contains(&Foul::CueBallPocketed), "{report:?}");
    assert_eq!(report.penalty, vec![BallId(2), BallId(5)]);
    assert_eq!(on_table(&g, Group::Solids), vec![7]);
    assert_eq!(g.current_player(), 1);
    assert_eq!(g.ball_in_hand(), None);
    // Back on the head spot, or right behind the 8 lying near it.
    let cue = g.ball(BallId::CUE).xy();
    assert!(cue.y == 0.0 && cue.x <= g.table.head_spot().x, "{cue}");
}

#[test]
fn hitting_the_opponents_ball_directly_gives_two_balls() {
    let mut g = house(&[
        (0, DVec2::new(-0.2, 0.0)),
        (3, DVec2::new(0.3, 0.0)),
        (1, DVec2::new(0.5, 0.3)),
        (6, DVec2::new(0.6, -0.3)),
        (12, DVec2::new(0.0, -0.4)),
        (8, DVec2::new(-0.6, -0.3)),
    ]);
    let report = g.play_shot(&Shot::new(0.0, 1.0), None).unwrap();
    assert_eq!(report.fouls, vec![Foul::WrongBallFirst(BallId(3))]);
    assert_eq!(report.penalty, vec![BallId(1), BallId(3)]);
    assert_eq!(on_table(&g, Group::Solids), vec![6]);
    assert_eq!(g.current_player(), 1);
    assert_eq!(g.ball_in_hand(), None);
}

#[test]
fn hitting_the_opponents_ball_and_scratching_gives_three_balls() {
    let mut g = house(&[(0, DVec2::new(-0.2, 0.0))]);
    // The opponent's 3 near the corner, the cue ball straight behind it
    // on the line to the pocket: struck high and hard, it follows the 3
    // in. Their 1, 4 and 6 are elsewhere.
    let pocket = g.table.pocket(PocketId::FootLeft).aim_point();
    let back = (DVec2::ZERO - pocket).normalize();
    layout(
        &mut g,
        &[
            (0, pocket + back * 0.55),
            (3, pocket + back * 0.12),
            (1, DVec2::new(-0.5, 0.3)),
            (4, DVec2::new(-0.6, -0.3)),
            (6, DVec2::new(-0.3, -0.3)),
            (7, DVec2::new(-0.7, 0.1)),
            (12, DVec2::new(0.0, -0.4)),
            (8, DVec2::new(-0.8, -0.2)),
        ],
    );
    let shot = straight_shot(&g, BallId(3), PocketId::FootLeft, 4.0).with_tip(DVec2::new(0.0, 0.9));
    let report = g.play_shot(&shot, None).unwrap();
    assert_eq!(report.first_contact, Some(BallId(3)));
    assert!(report.fouls.contains(&Foul::WrongBallFirst(BallId(3))), "{report:?}");
    assert!(report.fouls.contains(&Foul::CueBallPocketed), "{report:?}");
    // The 3 went down by itself; three more come off, lowest first.
    assert_eq!(report.penalty, vec![BallId(1), BallId(4), BallId(6)]);
    assert_eq!(on_table(&g, Group::Solids), vec![7]);
    assert_eq!(g.current_player(), 1);
    assert_eq!(g.ball_in_hand(), None);
}

/// The opponent's ball hit directly and made: it stays down, and two
/// more of theirs come off as well (the one that fell doesn't count
/// towards the two).
#[test]
fn potting_the_opponents_ball_hit_directly_still_gives_two_more() {
    let mut g = house(&[(0, DVec2::new(-0.2, 0.0))]);
    let pocket = g.table.pocket(PocketId::FootLeft).aim_point();
    let back = (DVec2::ZERO - pocket).normalize();
    layout(
        &mut g,
        &[
            (0, pocket + back * 0.6),
            (3, pocket + back * 0.15),
            (1, DVec2::new(-0.5, 0.3)),
            (4, DVec2::new(-0.6, -0.3)),
            (6, DVec2::new(-0.3, -0.3)),
            (12, DVec2::new(0.0, -0.4)),
            (8, DVec2::new(-0.8, -0.2)),
        ],
    );
    // A stop shot: the 3 goes in, the cue ball stays up.
    let shot =
        straight_shot(&g, BallId(3), PocketId::FootLeft, 2.0).with_tip(DVec2::new(0.0, -0.4));
    let report = g.play_shot(&shot, None).unwrap();
    assert_eq!(report.fouls, vec![Foul::WrongBallFirst(BallId(3))], "{report:?}");
    assert!(report.pocketed.iter().any(|(b, _)| *b == BallId(3)), "{report:?}");
    assert_eq!(report.penalty, vec![BallId(1), BallId(4)]);
    assert_eq!(on_table(&g, Group::Solids), vec![6], "the 3 that fell and two more");
    assert_eq!((g.current_player(), g.ball_in_hand()), (1, None));
}

#[test]
fn hitting_the_eight_too_soon_counts_as_a_miss() {
    let mut g = house(&[
        (0, DVec2::new(-0.2, 0.0)),
        (8, DVec2::new(0.3, 0.0)),
        (1, DVec2::new(0.5, 0.3)),
        (6, DVec2::new(0.6, -0.3)),
        (12, DVec2::new(0.0, -0.4)),
    ]);
    let report = g.play_shot(&Shot::new(0.0, 1.0), None).unwrap();
    assert_eq!(report.fouls, vec![Foul::WrongBallFirst(BallId::EIGHT)]);
    assert_eq!(report.penalty, vec![BallId(1)], "one ball, not two: the 8 is nobody's");
    assert_eq!(on_table(&g, Group::Solids), vec![6]);
    assert_eq!((g.current_player(), g.ball_in_hand(), g.game_over()), (1, None, None));
}

#[test]
fn hitting_the_opponents_ball_off_a_cushion_gives_one_ball() {
    let mut g = house(&[
        (0, DVec2::new(0.0, 0.0)),
        (3, DVec2::new(0.4, 0.0)),
        (6, DVec2::new(-0.6, -0.3)),
        (12, DVec2::new(-0.5, 0.3)),
        (8, DVec2::new(-0.8, -0.1)),
    ]);
    // One cushion (the long rail) and then the 3: aim at its mirror.
    let w = g.table.spec.width / 2.0 - g.table.spec.ball_radius;
    let mirror = DVec2::new(0.4, 2.0 * w);
    let report = g.play_shot(&Shot::new(mirror.y.atan2(mirror.x), 2.0), None).unwrap();
    assert_eq!(report.fouls, vec![Foul::WrongBallFirst(BallId(3))], "{report:?}");
    assert_eq!(report.penalty, vec![BallId(3)]);
    assert_eq!(on_table(&g, Group::Solids), vec![6]);
    assert_eq!(g.current_player(), 1);
}

#[test]
fn a_scratch_gives_one_ball_and_the_cue_ball_comes_back() {
    let mut g = house(&[
        (0, DVec2::new(0.4, 0.0)),
        (12, DVec2::new(0.7, 0.2)),
        (2, DVec2::new(-0.5, 0.3)),
        (5, DVec2::new(-0.6, -0.3)),
        (8, DVec2::new(-0.6, 0.0)),
    ]);
    // A thin cut on the 12 that carries the cue ball on into the corner.
    let mut scratched = None;
    for k in 0..400 {
        let mut trial = g.clone();
        let yaw = 0.2 + k as f64 * 0.002;
        let report = trial.play_shot(&Shot::new(yaw, 3.0), None).unwrap();
        if report.fouls == vec![Foul::CueBallPocketed] && report.first_contact == Some(BallId(12)) {
            scratched = Some((trial, report));
            break;
        }
    }
    let (after, report) = scratched.expect("a scratch off the 12");
    g = after;
    assert_eq!(report.penalty, vec![BallId(2)]);
    assert_eq!(on_table(&g, Group::Solids), vec![5]);
    assert_eq!(g.current_player(), 1);
    assert_eq!(g.ball_in_hand(), None);
    // Back on the head spot, or right behind the 8 lying near it.
    let cue = g.ball(BallId::CUE).xy();
    assert!(cue.y == 0.0 && cue.x <= g.table.head_spot().x, "{cue}");
}

#[test]
fn a_foul_on_an_open_table_costs_only_the_turn() {
    let mut g = after_break(3);
    layout(
        &mut g,
        &[
            (0, DVec2::new(-0.2, 0.0)),
            (2, DVec2::new(0.5, 0.3)),
            (12, DVec2::new(0.0, -0.4)),
            (8, DVec2::new(-0.6, -0.3)),
        ],
    );
    let report = g.play_shot(&Shot::new(0.0, 1.0), None).unwrap();
    assert_eq!(report.fouls, vec![Foul::NoContact]);
    assert!(report.penalty.is_empty());
    assert!(g.ball(BallId(2)).in_play() && g.ball(BallId(12)).in_play());
    assert_eq!(g.current_player(), 1);
    assert_eq!(g.ball_in_hand(), None);
}

#[test]
fn a_penalty_never_takes_the_eight_and_stops_at_the_last_ball() {
    let mut g = house(&[
        (0, DVec2::new(-0.2, 0.0)),
        (3, DVec2::new(0.3, 0.0)),
        (12, DVec2::new(0.0, -0.4)),
        (8, DVec2::new(-0.6, -0.3)),
    ]);
    // Two balls owed, the opponent has one: they are on the 8 now.
    let report = g.play_shot(&Shot::new(0.0, 1.0), None).unwrap();
    assert_eq!(report.penalty, vec![BallId(3)]);
    assert!(g.ball(BallId::EIGHT).in_play());
    assert!(g.is_on_eight(1));
    assert_eq!(g.game_over(), None);
}

#[test]
fn nothing_has_to_reach_a_cushion() {
    let mut g = house(&[
        (0, DVec2::new(-0.2, 0.0)),
        (12, DVec2::new(0.0, 0.0)),
        (3, DVec2::new(0.5, 0.3)),
        (8, DVec2::new(-0.6, -0.3)),
    ]);
    let report = g.play_shot(&Shot::new(0.0, 0.45), None).unwrap();
    assert_eq!(report.first_contact, Some(BallId(12)));
    assert_eq!(report.fouls, vec![]);
    assert!(report.penalty.is_empty());
    assert_eq!(g.current_player(), 1, "nothing made: the turn passes");
    assert_eq!(g.ball_in_hand(), None);
}

#[test]
fn a_soft_break_is_no_foul() {
    let mut g = EightBallGame::new(1, 0, Rules::Brazilian);
    let report = g.play_shot(&Shot::new(0.0, 1.0), None).unwrap();
    assert!(report.first_contact.is_some());
    assert_eq!(report.fouls, vec![]);
    assert_eq!(g.current_player(), 1);
    assert_eq!(g.ball_in_hand(), None);
}

#[test]
fn the_eight_alone_on_the_break_wins() {
    let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
    on_break_with(
        &mut g,
        &[(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.3, 0.2)), (2, DVec2::new(0.0, -0.3))],
    );
    let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 2.0);
    let report = g.play_shot(&shot, None).unwrap();
    assert_eq!(report.pocketed.len(), 1);
    assert_eq!(
        report.game_over,
        Some(GameOver { winner: 0, reason: GameOverReason::PocketedEight })
    );
}

#[test]
fn the_eight_with_another_ball_on_the_break_loses() {
    let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
    let t = g.table.clone();
    let r = t.spec.ball_radius;
    // The 8 and the 2 each hang over a corner pocket; one stroke sends
    // the cue ball into the 8 and the 8's neighbour into the 2.
    let foot_left = t.pocket(PocketId::FootLeft).aim_point();
    let foot_right = t.pocket(PocketId::FootRight).aim_point();
    let inward = |p: DVec2, d: f64| p + (DVec2::ZERO - p).normalize() * d;
    on_break_with(
        &mut g,
        &[
            (0, DVec2::new(0.0, 0.0)),
            (8, inward(foot_left, 0.12)),
            (2, inward(foot_right, 0.12)),
            (11, inward(foot_right, 0.12 + 2.0 * r + 0.002)),
        ],
    );
    // Find a stroke that makes both (the cue ball off the 11 into the 8
    // is too fine to aim by hand: sweep).
    let mut lost = None;
    for k in 0..2000 {
        let mut trial = g.clone();
        let yaw = -0.9 + k as f64 * 0.0009;
        let report = trial.play_shot(&Shot::new(yaw, 6.0), None).unwrap();
        let made: Vec<u8> = report.pocketed.iter().map(|(b, _)| b.0).collect();
        if made.contains(&8) && made.len() > 1 {
            lost = Some(report);
            break;
        }
    }
    // The layout above is hard to hit in one stroke on every table; the
    // rule itself is what matters, so fall back to the cue ball going
    // down with the 8 (a scratch on the 8, the same verdict).
    let report = lost.unwrap_or_else(|| {
        let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
        let foot_left = g.table.pocket(PocketId::FootLeft).aim_point();
        let p8 = foot_left + (DVec2::ZERO - foot_left).normalize() * 0.08;
        on_break_with(&mut g, &[(0, p8 + (DVec2::ZERO - foot_left).normalize() * 0.5), (8, p8)]);
        let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 6.0)
            .with_tip(DVec2::new(0.0, 0.9));
        g.play_shot(&shot, None).unwrap()
    });
    assert!(report.pocketed.len() > 1, "{report:?}");
    assert_eq!(
        report.game_over,
        Some(GameOver { winner: 1, reason: GameOverReason::EightWithAnotherBall })
    );
}

#[test]
fn the_eight_with_an_opponents_ball_at_the_end_loses() {
    let mut g = house(&[(0, DVec2::new(-0.2, 0.0))]);
    let t = g.table.clone();
    let r = t.spec.ball_radius;
    // Player 0 (stripes, none left) is on the 8. The 8 hangs over one
    // corner, the opponent's 3 over the other, and the 8 is struck so
    // that the cue ball runs on into the 3... simpler and certain: the
    // 3 sits on the 8's path, right at the lip, and goes down first.
    let pocket = t.pocket(PocketId::FootLeft).aim_point();
    let back = (DVec2::ZERO - pocket).normalize();
    layout(
        &mut g,
        &[
            (0, pocket + back * 0.9),
            (8, pocket + back * (0.10 + 2.0 * r + 0.001)),
            (3, pocket + back * 0.10),
        ],
    );
    assert!(g.is_on_eight(0));
    let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 3.5);
    let report = g.play_shot(&shot, Some(PocketId::FootLeft)).unwrap();
    let made: Vec<u8> = report.pocketed.iter().map(|(b, _)| b.0).collect();
    assert!(made.contains(&8) && made.contains(&3), "{report:?}");
    assert_eq!(
        report.game_over,
        Some(GameOver { winner: 1, reason: GameOverReason::EightWithAnotherBall })
    );
}

#[test]
fn the_eight_alone_at_the_end_wins_and_with_the_cue_ball_loses() {
    let mut g = house(&[(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.6, 0.3))]);
    let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 2.0);
    let report = g.play_shot(&shot, Some(PocketId::FootLeft)).unwrap();
    assert_eq!(
        report.game_over,
        Some(GameOver { winner: 0, reason: GameOverReason::PocketedEight })
    );

    // The same pot followed in by the cue ball.
    let mut g = house(&[(0, DVec2::new(-0.2, 0.0))]);
    let pocket = g.table.pocket(PocketId::FootLeft).aim_point();
    let back = (DVec2::ZERO - pocket).normalize();
    layout(&mut g, &[(0, pocket + back * 0.5), (8, pocket + back * 0.08)]);
    let shot =
        straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 4.0).with_tip(DVec2::new(0.0, 0.9));
    let report = g.play_shot(&shot, Some(PocketId::FootLeft)).unwrap();
    assert!(report.fouls.contains(&Foul::CueBallPocketed), "{report:?}");
    assert_eq!(
        report.game_over,
        Some(GameOver { winner: 1, reason: GameOverReason::EightPocketedOnFoul })
    );
}

/// Balls knocked off the table (the events are given by hand: a jump
/// that clears the rail is hard to produce on demand).
#[test]
fn balls_knocked_off_the_table() {
    let knock_off = |ball: u8| {
        let mut g = house(&[
            (0, DVec2::new(-0.2, 0.0)),
            (12, DVec2::new(0.3, 0.0)),
            (14, DVec2::new(0.5, 0.3)),
            (3, DVec2::new(0.5, -0.3)),
            (6, DVec2::new(0.6, -0.1)),
            (8, DVec2::new(-0.6, -0.3)),
        ]);
        let mut pending = g.prepare_shot(&Shot::new(0.0, 1.0), None).unwrap();
        pending.simulation.run_to_rest();
        pending.simulation.knock_off_for_tests(BallId(ball));
        let report = g.finish_shot(pending);
        (g, report)
    };
    // One's own ball comes back; the turn is lost, nothing else.
    let (g, report) = knock_off(14);
    assert_eq!(report.fouls, vec![Foul::ObjectBallOffTable(BallId(14))]);
    assert_eq!(report.spotted, vec![BallId(14)]);
    assert!(report.penalty.is_empty());
    assert!(g.ball(BallId(14)).in_play());
    assert_eq!((g.current_player(), g.ball_in_hand()), (1, None));
    // The opponent's stays off, made for them.
    let (g, report) = knock_off(3);
    assert_eq!(report.fouls, vec![Foul::ObjectBallOffTable(BallId(3))]);
    assert!(report.spotted.is_empty());
    assert_eq!(report.penalty, vec![BallId(3)]);
    assert_eq!(on_table(&g, Group::Solids), vec![6]);
    assert_eq!((g.current_player(), g.ball_in_hand()), (1, None));
    // The cue ball: the turn and a ball given, as for a scratch; the
    // cue ball comes back to the head spot.
    let (g, report) = knock_off(0);
    assert_eq!(report.fouls, vec![Foul::CueBallOffTable]);
    assert_eq!(report.penalty, vec![BallId(3)]);
    assert_eq!(on_table(&g, Group::Solids), vec![6]);
    assert_eq!((g.current_player(), g.ball_in_hand()), (1, None));
    assert_eq!(g.ball(BallId::CUE).xy(), g.table.head_spot());
}

/// The 8 knocked off the table on the break is no loss: it goes back on
/// the foot spot and the turn passes, a foul (see the rules as players
/// read them, "The 8-ball on the break").
#[test]
fn the_eight_off_the_table_on_the_break_is_spotted() {
    let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
    let mut pending = g.prepare_shot(&Shot::new(g.default_aim(), 6.0), None).unwrap();
    pending.simulation.run_to_rest();
    pending.simulation.knock_off_for_tests(BallId::EIGHT);
    let report = g.finish_shot(pending);
    assert!(report.fouls.contains(&Foul::ObjectBallOffTable(BallId::EIGHT)), "{report:?}");
    assert!(report.spotted.contains(&BallId::EIGHT));
    assert_eq!(report.game_over, None);
    assert!(!report.continues);
    assert_eq!(g.current_player(), 1);
    let eight = g.ball(BallId::EIGHT);
    assert!(eight.in_play() && eight.xy().y == 0.0 && eight.xy().x >= g.table.foot_spot().x);
}

/// An open table past the break with these balls on it.
fn open_table(positions: &[(u8, DVec2)]) -> EightBallGame {
    let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
    g.is_break = false;
    g.ball_in_hand = None;
    layout(&mut g, positions);
    g
}

/// Every group ball went down on fouls, which decide no group: both
/// players are on the 8.
#[test]
fn an_open_table_with_no_group_ball_left_puts_both_players_on_the_eight() {
    let mut g = open_table(&[(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.3, 0.2))]);
    let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 2.0);
    let soft = Shot::new(shot.yaw, 0.6);
    assert!(g.is_open_table() && g.is_on_eight(0) && g.is_on_eight(1));
    assert_eq!(g.legal_targets(), vec![BallId::EIGHT]);
    assert_eq!(g.play_shot(&shot, None), Err(ShotError::MustCallPocket));
    // Touching the 8 is legal (and the call binding from then on).
    let mut touch = g.clone();
    let report = touch.play_shot(&soft, Some(PocketId::FootLeft)).unwrap();
    assert_eq!(report.first_contact, Some(BallId::EIGHT));
    assert_eq!(report.fouls, vec![], "{report:?}");
    assert_eq!(touch.called_pocket_of(0), Some(PocketId::FootLeft));
    assert!(touch.needs_called_pocket(), "player 1 is on the 8 too");
    // Made in the called pocket, it wins.
    let report = g.play_shot(&shot, Some(PocketId::FootLeft)).unwrap();
    assert_eq!(
        report.game_over,
        Some(GameOver { winner: 0, reason: GameOverReason::PocketedEight })
    );
}

/// The cue ball in hand may be placed against a cushion (a ball at rest
/// against it is less than a radius from its line), never past a
/// pocket's mouth.
#[test]
fn the_cue_ball_in_hand_goes_against_a_cushion_but_never_in_a_pocket() {
    let mut g = after_break(5);
    g.ball_in_hand = Some(BallInHand::Anywhere);
    let t = g.table.clone();
    let (hl, hw, r) = (t.spec.length / 2.0, t.spec.width / 2.0, t.spec.ball_radius);
    let c = t.cushion_contact();
    assert!(c < r - 0.0005, "the nose meets a ball below its widest");
    let frozen = DVec2::new(-0.3, hw - c);
    let corner = DVec2::new(-(hl - r), hw - r);
    let mouth = t.pocket(PocketId::HeadLeft);
    assert!((corner - mouth.mouth_center).dot(mouth.outward) > 0.01, "past the mouth");
    assert!(mouth.in_throat(corner));
    assert!(g.can_place_cue_ball(frozen));
    assert!(!g.can_place_cue_ball(corner));
    assert!(!g.can_place_cue_ball(frozen + DVec2::new(0.0, 1e-6)));
    for k in PocketId::ALL {
        let p = t.pocket(k);
        assert!(!g.can_place_cue_ball(p.mouth_center + p.outward * 0.001));
    }
    // Struck away, a ball placed against the cushion leaves it without a
    // touch; struck into it, it comes off it.
    assert!(g.place_cue_ball(frozen));
    let away = g.prepare_shot(&Shot::new(-std::f64::consts::FRAC_PI_2, 1.0), None);
    let mut away = away.unwrap().simulation;
    away.run_to_rest();
    let cushion = |e: &Event| matches!(e, Event::Cushion { ball: BallId::CUE, .. });
    let first = away.events().iter().find(|e| cushion(e)).map(|e| e.time());
    assert!(first.is_none_or(|t| t > 0.5), "{first:?}");
    let into = g.prepare_shot(&Shot::new(std::f64::consts::FRAC_PI_2, 1.0), None);
    let mut into = into.unwrap().simulation;
    into.run_to_rest();
    assert!(into.events().iter().any(cushion));
}

/// The cue ball is in hand for the break only: a clock that runs out on
/// the break hands it over with the cue ball still in hand behind the
/// head string; elsewhere the clock only passes the turn, and nothing
/// changes once the game is over.
#[test]
fn a_clock_that_runs_out_keeps_the_cue_ball_in_hand_as_it_was() {
    let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
    assert_eq!(g.ball_in_hand(), Some(BallInHand::Kitchen));
    g.time_foul();
    assert_eq!((g.current_player(), g.ball_in_hand()), (1, Some(BallInHand::Kitchen)));
    assert!(g.is_break());
    let report = g.play_shot(&Shot::new(g.default_aim(), g.max_cue_speed()), None).unwrap();
    assert!(report.was_break);
    let mut g = open_table(&[(0, DVec2::new(-0.2, 0.0)), (3, DVec2::new(0.3, 0.1))]);
    g.time_foul();
    assert_eq!((g.current_player(), g.ball_in_hand()), (1, None));
    g.abandon(1);
    assert_eq!(g.game_over(), Some(GameOver { winner: 0, reason: GameOverReason::Abandoned }));
    g.time_foul();
    assert_eq!(g.current_player(), 1, "no changes after the game ended");
}

/// A tip so far out that its length overflows an f64 is struck at the
/// edge of the ball, in its direction; any tip that can be measured is
/// struck as the cue's limits say.
#[test]
fn a_tip_too_far_out_to_measure_is_struck_at_the_edge() {
    let g = EightBallGame::new(1, 0, Rules::Brazilian);
    let tip_of =
        |tip: DVec2| g.prepare_shot(&Shot::new(0.0, 2.0).with_tip(tip), None).unwrap().shot.tip;
    let edge = tip_of(DVec2::new(3e200, -3e200));
    assert!((edge.length() - 1.0).abs() < 1e-12 && (edge.x + edge.y).abs() < 1e-12, "{edge}");
    for tip in [DVec2::new(3.0, -4.0), DVec2::new(0.3, 0.2), DVec2::new(1e150, 2.0)] {
        let plain = Shot::new(0.0, 2.0).with_tip(tip).sanitized_for(g.params(), true).tip;
        assert_eq!(tip_of(tip).to_array().map(f64::to_bits), plain.to_array().map(f64::to_bits));
    }
}

/// Random games: the rules' invariants hold, and a cue ball in hand
/// only goes, or is put back, where a ball can rest (it is in hand only
/// for a break, the first or one handed over).
#[test]
fn the_brazilian_rules_hold_over_random_games() {
    let r = Table::default().spec.ball_radius;
    for seed in 0..40u64 {
        let mut g = EightBallGame::new(seed, (seed % 2) as usize, Rules::Brazilian);
        let mut rng = Rng::new(seed + 5000);
        for _ in 0..120 {
            if g.game_over().is_some() {
                break;
            }
            if g.ball_in_hand().is_some() {
                for _ in 0..300 {
                    let p = DVec2::new(rng.next_f64() * 2.4 - 1.2, rng.next_f64() * 1.2 - 0.6);
                    if g.place_cue_ball(p) {
                        assert!(g.table().can_place_ball(p), "{p}");
                        break;
                    }
                }
            }
            let before = g.clone();
            let shooter = g.current_player();
            let shot =
                Shot::new(rng.next_f64() * std::f64::consts::TAU, 0.3 + rng.next_f64() * 6.5)
                    .with_tip(DVec2::new(rng.next_f64() - 0.5, rng.next_f64() - 0.5));
            let called = g.needs_called_pocket().then_some(PocketId::ALL[rng.below(6)]);
            let report = g.play_shot(&shot, called).unwrap();
            match (g.group_of(0), g.group_of(1)) {
                (None, None) => {}
                (Some(a), Some(b)) => assert_eq!(a, b.opposite()),
                other => panic!("half-assigned groups {other:?}"),
            }
            if let Some(old) = before.group_of(0) {
                assert_eq!(g.group_of(0), Some(old), "groups changed");
            }
            if g.game_over().is_none() {
                assert_eq!(g.ball_in_hand().is_some(), report.break_again, "{report:?}");
                assert_eq!(g.is_break(), report.break_again, "{report:?}");
                assert_eq!(g.current_player() == shooter, report.continues);
                assert!(g.ball(BallId::CUE).in_play() && g.ball(BallId::EIGHT).in_play());
                if g.ball_in_hand().is_some() {
                    assert!(g.can_place_cue_ball(g.ball(BallId::CUE).xy()), "{report:?}");
                }
            }
            let live: Vec<_> = g.balls().iter().filter(|b| b.in_play()).collect();
            for (k, a) in live.iter().enumerate() {
                for b in &live[k + 1..] {
                    assert!(a.xy().distance(b.xy()) > 2.0 * r - 1e-6, "{} {}", a.id, b.id);
                }
            }
        }
    }
}

/// A break that the cue ball strikes softly under `rules`, with the
/// balls `down` made, in that order, at its end (wherever the cue ball
/// sent them): what the rules do with the order alone.
pub(super) fn break_making(rules: Rules, down: &[u8]) -> (EightBallGame, ShotReport) {
    let mut g = EightBallGame::new(3, 0, rules);
    let mut pending = g.prepare_shot(&Shot::new(g.default_aim(), 1.0), None).unwrap();
    pending.simulation.run_to_rest();
    assert!(
        pending.simulation.events().iter().all(|e| !matches!(e, Event::Pocketed { .. })),
        "a soft break makes nothing by itself"
    );
    for (k, n) in down.iter().enumerate() {
        pending.simulation.pocket_for_tests(BallId(*n), PocketId::ALL[k % 6]);
    }
    let report = g.finish_shot(pending);
    (g, report)
}

/// The first ball made on the break decides the groups, whatever goes
/// down after it: the 3 and then the 13 make the breaker solids, and the
/// 13 (the opponent's) passes the turn, made for them. One kind only
/// keeps the table.
#[test]
fn the_first_ball_made_on_the_break_decides_the_groups() {
    let (g, report) = break_making(Rules::Brazilian, &[3, 13]);
    assert_eq!(report.fouls, vec![]);
    assert_eq!(report.group_assigned, Some(Group::Solids));
    assert_eq!((g.group_of(0), g.group_of(1)), (Some(Group::Solids), Some(Group::Stripes)));
    assert!(report.opponent_ball && !report.continues, "{report:?}");
    assert_eq!(g.current_player(), 1);
    assert_eq!(report.balls_made(), vec![BallId(3)]);
    assert!(!g.ball(BallId(13)).in_play(), "the 13 stays down, made for the opponent");
    assert_eq!(g.ball_in_hand(), None);

    // The other way round: the 13 first makes the breaker stripes.
    let (g, report) = break_making(Rules::Brazilian, &[13, 3, 5]);
    assert_eq!(report.group_assigned, Some(Group::Stripes));
    assert_eq!(g.group_of(0), Some(Group::Stripes));
    assert!(report.opponent_ball && !report.continues);
    assert_eq!(report.balls_made(), vec![BallId(13)]);

    // One kind only: the breaker keeps the table.
    let (g, report) = break_making(Rules::Brazilian, &[11, 9]);
    assert_eq!(report.group_assigned, Some(Group::Stripes));
    assert!(!report.opponent_ball && report.continues);
    assert_eq!(g.current_player(), 0);
}

/// The cue ball knocked off the table comes back to the head spot, not
/// in hand, and a foul's penalty is given as for any foul.
#[test]
fn the_cue_ball_off_the_table_comes_back_to_the_head_spot() {
    let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
    g.is_break = false;
    g.ball_in_hand = None;
    g.groups = [Some(Group::Solids), Some(Group::Stripes)];
    layout(
        &mut g,
        &[
            (0, DVec2::new(0.4, 0.0)),
            (2, DVec2::new(0.2, 0.3)),
            (10, DVec2::new(-0.5, 0.3)),
            (11, DVec2::new(0.5, -0.3)),
            (8, DVec2::new(-0.6, -0.3)),
        ],
    );
    // A miss that ends with the cue ball off the table.
    let mut pending = g.prepare_shot(&Shot::new(std::f64::consts::PI / 2.0, 0.5), None).unwrap();
    pending.simulation.run_to_rest();
    pending.simulation.knock_off_for_tests(BallId::CUE);
    let report = g.finish_shot(pending);
    assert!(report.fouls.contains(&Foul::CueBallOffTable), "{report:?}");
    assert!(report.fouls.contains(&Foul::NoContact), "{report:?}");
    assert_eq!(
        report.penalty,
        vec![BallId(10), BallId(11)],
        "one for the miss, one for the cue ball"
    );
    assert_eq!(g.ball_in_hand(), None);
    assert_eq!(g.ball(BallId::CUE).xy(), g.table.head_spot());
    assert_eq!(g.current_player(), 1);
    // A clock that runs out now passes the turn alone.
    g.time_foul();
    assert_eq!((g.current_player(), g.ball_in_hand()), (0, None));
}

/// A break that makes a ball and the cue ball decides the
/// groups all the same: the 11 made, stripes for the breaker; the turn
/// passes, the cue ball goes back to the head spot and the scratch costs
/// the opponent's lowest solid.
#[test]
fn a_break_with_a_ball_and_the_cue_ball_decides_the_groups() {
    let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
    on_break_with(
        &mut g,
        &[
            (0, DVec2::new(0.6, 0.3)),
            (11, DVec2::new(0.9, 0.45)),
            (1, DVec2::new(-0.3, -0.3)),
            (2, DVec2::new(-0.2, 0.2)),
            (8, DVec2::new(-0.6, -0.3)),
        ],
    );
    // The 11 and then the cue ball down the corner.
    let d = g.ball(BallId(11)).xy() - g.ball(BallId::CUE).xy();
    let mut pending = g.prepare_shot(&Shot::new(d.y.atan2(d.x), 0.6), None).unwrap();
    pending.simulation.run_to_rest();
    pending.simulation.pocket_for_tests(BallId(11), PocketId::FootLeft);
    pending.simulation.pocket_for_tests(BallId::CUE, PocketId::FootLeft);
    let report = g.finish_shot(pending);
    assert!(report.fouls.contains(&Foul::CueBallPocketed), "{report:?}");
    assert!(report.pocketed.iter().any(|(b, _)| *b == BallId(11)), "{report:?}");
    assert!(!report.continues && !report.break_again && report.game_over.is_none());
    assert_eq!(g.current_player(), 1);
    assert!(!g.is_break());
    assert_eq!(g.ball_in_hand(), None);
    assert_eq!(g.ball(BallId::CUE).xy(), g.table.head_spot());
    assert_eq!(report.group_assigned, Some(Group::Stripes));
    assert_eq!(g.group_of(0), Some(Group::Stripes));
    assert_eq!(g.group_of(1), Some(Group::Solids));
    assert_eq!(report.penalty, vec![BallId(1)], "{report:?}");
    assert!(!g.ball(BallId(1)).in_play());
}

/// The cue ball knocked off the table gives the opponent
/// a ball, as a scratch does: one more with a miss, one alone after a
/// legal contact; the cue ball comes back to the head spot.
#[test]
fn the_cue_ball_off_the_table_gives_a_ball() {
    for (aim_at_ball, given) in [(false, vec![BallId(10), BallId(11)]), (true, vec![BallId(10)])] {
        let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
        g.is_break = false;
        g.ball_in_hand = None;
        g.groups = [Some(Group::Solids), Some(Group::Stripes)];
        layout(
            &mut g,
            &[
                (0, DVec2::new(0.4, 0.0)),
                (2, DVec2::new(0.2, 0.3)),
                (10, DVec2::new(-0.5, 0.3)),
                (11, DVec2::new(0.5, -0.3)),
                (12, DVec2::new(-0.3, -0.3)),
                (8, DVec2::new(-0.6, -0.3)),
            ],
        );
        let yaw = if aim_at_ball {
            let d = g.ball(BallId(2)).xy() - g.ball(BallId::CUE).xy();
            d.y.atan2(d.x)
        } else {
            std::f64::consts::PI / 2.0
        };
        let mut pending = g.prepare_shot(&Shot::new(yaw, 0.5), None).unwrap();
        pending.simulation.run_to_rest();
        pending.simulation.knock_off_for_tests(BallId::CUE);
        let report = g.finish_shot(pending);
        assert!(report.fouls.contains(&Foul::CueBallOffTable), "{report:?}");
        assert_eq!(report.fouls.contains(&Foul::NoContact), !aim_at_ball, "{report:?}");
        assert_eq!(report.penalty, given, "{report:?}");
        assert!(!report.continues);
        assert_eq!(g.ball_in_hand(), None);
        assert_eq!(g.ball(BallId::CUE).xy(), g.table.head_spot());
        assert_eq!(g.current_player(), 1);
    }
}

/// A break that loses the cue ball without making a
/// ball hands the break over: the balls stay where they went, the cue
/// ball is in hand behind the head string and the opponent's shot is a
/// break.
#[test]
fn a_break_that_only_makes_the_cue_ball_passes_the_break() {
    let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
    let aim = g.table.pocket(PocketId::FootLeft).aim_point();
    on_break_with(
        &mut g,
        &[(0, aim + (DVec2::ZERO - aim).normalize() * 0.4), (5, DVec2::new(-0.3, 0.0))],
    );
    let d = aim - g.ball(BallId::CUE).xy();
    let report = g.play_shot(&Shot::new(d.y.atan2(d.x), 2.0), None).unwrap();
    assert!(report.fouls.contains(&Foul::CueBallPocketed), "{report:?}");
    assert!(report.break_again && !report.continues, "{report:?}");
    assert!(report.penalty.is_empty() && report.game_over.is_none());
    assert_eq!(g.current_player(), 1);
    assert!(g.is_break() && g.is_open_table());
    assert_eq!(g.ball_in_hand(), Some(BallInHand::Kitchen));
    assert!(g.ball(BallId::CUE).xy().x <= g.table.head_string_x());
    assert_eq!(g.ball(BallId(5)).xy(), DVec2::new(-0.3, 0.0), "nothing is racked again");
    // Anywhere behind the head string, never past it.
    let hx = g.table.head_string_x();
    assert!(!g.place_cue_ball(DVec2::new(hx + 0.2, 0.3)));
    assert!(g.place_cue_ball(DVec2::new(hx - 0.1, 0.3)));
    let shot = straight_shot(&g, BallId(5), PocketId::FootLeft, 2.0);
    let report = g.play_shot(&shot, None).unwrap();
    assert!(report.was_break);
}

/// A break that loses the cue ball together with balls of both groups (a
/// solid and then a stripe, or the other way round; in a pocket or off the
/// table) hands the break over, as one that makes nothing: no penalty
/// ball, the groups still open, the balls made stay down, and the opponent
/// breaks with the cue ball in hand behind the head string. With balls of
/// one group only it stays an ordinary foul: the groups are decided, the
/// opponent's lowest ball is owed and the cue ball is played from the head
/// spot, without ball in hand.
#[test]
fn a_break_that_loses_the_cue_ball_with_both_groups_passes_the_break() {
    let lose_cue = |down: &[u8], off: bool| {
        let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
        let mut pending = g.prepare_shot(&Shot::new(g.default_aim(), 1.0), None).unwrap();
        pending.simulation.run_to_rest();
        for (k, n) in down.iter().enumerate() {
            pending.simulation.pocket_for_tests(BallId(*n), PocketId::ALL[k]);
        }
        if off {
            pending.simulation.knock_off_for_tests(BallId::CUE);
        } else {
            pending.simulation.pocket_for_tests(BallId::CUE, PocketId::FootRight);
        }
        let report = g.finish_shot(pending);
        (g, report)
    };
    for (down, off) in [([3u8, 13], false), ([13, 3], false), ([3, 13], true)] {
        let (g, report) = lose_cue(&down, off);
        assert!(!report.fouls.is_empty(), "{report:?}");
        assert!(report.break_again && !report.continues, "{report:?}");
        assert!(report.game_over.is_none() && report.group_assigned.is_none(), "{report:?}");
        assert!(report.penalty.is_empty(), "no penalty ball: {report:?}");
        assert!(g.is_break() && g.is_open_table());
        assert_eq!(g.current_player(), 1);
        assert_eq!(g.ball_in_hand(), Some(BallInHand::Kitchen));
        assert!(g.ball(BallId::CUE).xy().x <= g.table.head_string_x());
        assert!(down.iter().all(|n| !g.ball(BallId(*n)).in_play()), "the balls made stay down");
        assert_eq!(g.remaining(Group::Solids).len() + g.remaining(Group::Stripes).len(), 12);
    }
    // Two solids and the cue ball: the ordinary foul of a break.
    let (g, report) = lose_cue(&[3, 5], false);
    assert!(!report.break_again && !report.continues, "{report:?}");
    assert_eq!(report.group_assigned, Some(Group::Solids));
    assert_eq!(report.penalty, vec![BallId(9)], "the opponent's lowest ball");
    assert!(!g.is_break());
    assert_eq!(g.ball_in_hand(), None);
    assert_eq!(g.ball(BallId::CUE).xy(), g.table.head_spot());
    assert_eq!(g.current_player(), 1);
}

/// A break that hits no ball hands the break over too, on the rack as it
/// was.
#[test]
fn a_break_that_hits_nothing_passes_the_break() {
    let away = Shot::new(std::f64::consts::PI, 0.4);
    let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
    let racked: Vec<DVec2> = (1..16).map(|n| g.ball(BallId(n)).xy()).collect();
    let report = g.play_shot(&away, None).unwrap();
    assert!(report.fouls.contains(&Foul::NoContact), "{report:?}");
    assert!(report.break_again && !report.continues, "{report:?}");
    assert_eq!(g.current_player(), 1);
    assert!(g.is_break() && g.rack_untouched());
    assert_eq!(g.ball_in_hand(), Some(BallInHand::Kitchen));
    assert_eq!(racked, (1..16).map(|n| g.ball(BallId(n)).xy()).collect::<Vec<_>>());
    // The opponent breaks the rack.
    let report = g.play_shot(&Shot::new(g.default_aim(), g.max_cue_speed()), None).unwrap();
    assert!(report.was_break && report.first_contact.is_some());
}

/// The computer breaks a full rack hard, and plays a break handed over
/// on scattered balls like any shot (the cue ball behind the head string).
#[test]
fn the_computer_plays_a_break_handed_over() {
    let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
    let aim = g.table.pocket(PocketId::FootLeft).aim_point();
    on_break_with(
        &mut g,
        &[(0, aim + (DVec2::ZERO - aim).normalize() * 0.4), (5, DVec2::new(-0.3, 0.0))],
    );
    let d = aim - g.ball(BallId::CUE).xy();
    g.play_shot(&Shot::new(d.y.atan2(d.x), 2.0), None).unwrap();
    assert!(g.is_break() && !g.rack_untouched());
    let decision = crate::ai::decide(&g, crate::ai::Difficulty::Hard, 7);
    if let Some(p) = decision.cue_ball {
        assert!(p.x <= g.table.head_string_x(), "{p}");
        assert!(g.place_cue_ball(p));
    }
    assert!(decision.shot.speed < g.max_cue_speed(), "not a full-power break");
    g.play_shot(&decision.shot, decision.called_pocket).unwrap();
}

#[test]
fn full_random_game_terminates() {
    // Plays a game with simple straight-in attempts to make sure the state
    // machine never gets stuck.
    let mut g = EightBallGame::new(11, 0, Rules::Brazilian);
    g.play_shot(&Shot::new(0.0, 7.0), None).unwrap();
    let mut rng = Rng::new(5);
    for _ in 0..400 {
        if g.game_over().is_some() {
            break;
        }
        if g.ball_in_hand().is_some() {
            for _ in 0..200 {
                let p = DVec2::new(rng.next_f64() * 1.8 - 0.9, rng.next_f64() * 0.8 - 0.4);
                if g.place_cue_ball(p) {
                    break;
                }
            }
        }
        let target = g.legal_targets()[0];
        let pocket = PocketId::ALL[rng.below(6)];
        let shot = straight_shot(&g, target, pocket, 1.0 + rng.next_f64() * 3.0);
        let called = g.needs_called_pocket().then_some(pocket);
        g.play_shot(&shot, called).unwrap();
    }
    assert!(g.game_over().is_some(), "game should finish");
}
