//! Against the clock: one player clears a full rack as fast as they can.
//!
//! The table is the game's usual one, played by seat 0 alone, and the rules
//! are short:
//!
//! - The fifteen balls are racked as for a game; the player breaks with the
//!   cue ball in hand behind the head string.
//! - Any ball may be hit first and every object ball made stays down,
//!   whatever happened on the shot: the table is never handed over.
//! - The 8 goes last: made while other balls are still on the table, it is
//!   spotted again (on the foot spot, or the nearest free place on the long
//!   string) and the shot is a foul. Made as the last ball (or with it), the
//!   table is cleared.
//! - Fouls: the cue ball pocketed or off the table, no ball hit, an object
//!   ball off the table (spotted again) and the early 8. After a foul the
//!   player has the cue ball in hand anywhere, and the shot costs a time
//!   penalty (see `snook_legend_proto::time_attack`).
//!
//! The game knows it is played this way ([`EightBallGame::is_time_attack`]):
//! [`EightBallGame::finish_shot`] applies these rules instead of the
//! eight-ball ones, so the table plays (and replays) like any other.

use super::ShotReport;
use super::{BallInHand, EightBallGame, Facts, Foul, GameOver, GameOverReason, PendingShot, Rules};
use crate::ball::BallId;

impl EightBallGame {
    /// A rack to clear against the clock, shuffled from `seed`.
    pub fn time_attack(seed: u64) -> EightBallGame {
        let mut g = EightBallGame::new(seed, 0, Rules::Official);
        g.time_attack = true;
        // Nothing is called against the clock: the 8 too goes in any pocket.
        g.eight_call = false;
        g
    }

    /// Played against the clock (see [`EightBallGame::time_attack`]).
    pub fn is_time_attack(&self) -> bool {
        self.time_attack
    }

    /// Object balls still on the table (the 8 included).
    pub fn balls_left(&self) -> usize {
        self.balls.iter().filter(|b| b.id.is_object() && b.in_play()).count()
    }

    /// The rules against the clock (see the module's notes).
    pub(super) fn finish_time_attack(&mut self, pending: PendingShot) -> ShotReport {
        let sim = &pending.simulation;
        let facts = Facts::of(sim.events());
        let was_break = self.is_break;
        let open_table = true;
        let mut fouls = Vec::new();
        if facts.cue_pocketed() {
            fouls.push(Foul::CueBallPocketed);
        }
        if facts.cue_off() {
            fouls.push(Foul::CueBallOffTable);
        }
        if facts.first_contact.is_none() {
            fouls.push(Foul::NoContact);
        }
        for &b in facts.off_table.iter().filter(|b| b.is_object()) {
            fouls.push(Foul::ObjectBallOffTable(b));
        }
        self.balls = sim.balls.clone();
        self.balls.sort_by_key(|b| b.id);
        self.shots_taken += 1;
        self.is_break = false;
        self.ball_in_hand = None;
        // Balls that left the table come back first (the 8 among them).
        let mut spotted = Vec::new();
        for &b in facts.off_table.iter().filter(|b| b.is_object()) {
            self.spot_ball(b);
            spotted.push(b);
        }
        let others_left =
            self.balls.iter().any(|b| b.id.is_object() && b.id != BallId::EIGHT && b.in_play());
        let eight_down = facts.pocket_of(BallId::EIGHT).is_some();
        if eight_down && others_left {
            self.spot_ball(BallId::EIGHT);
            spotted.push(BallId::EIGHT);
        }
        let cleared = self.balls_left() == 0;
        let game_over =
            cleared.then_some(GameOver { winner: 0, reason: GameOverReason::PocketedEight });
        self.game_over = game_over;
        if !cleared && (!fouls.is_empty() || spotted.contains(&BallId::EIGHT)) {
            self.grant_hand(BallInHand::Anywhere);
        }
        self.current = 0;
        ShotReport {
            shooter: 0,
            was_break,
            first_contact: facts.first_contact(),
            pocketed: facts.pocketed,
            fouls,
            spotted,
            group_assigned: None,
            continues: !cleared,
            game_over,
            called: None,
            opponent_ball: false,
            penalty: Vec::new(),
            break_again: false,
            choice: None,
            open_table,
        }
    }
}

/// Whether a shot against the clock costs a time penalty: a foul, or the 8
/// made before the last ball (spotted again).
pub fn penalised(report: &ShotReport) -> bool {
    report.is_foul() || report.spotted.contains(&BallId::EIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Shot;
    use crate::table::Table;
    use glam::DVec2;

    #[test]
    fn a_rack_against_the_clock_starts_with_the_break() {
        let g = EightBallGame::time_attack(5);
        assert!(g.is_time_attack());
        assert!(g.is_break());
        assert_eq!(g.balls_left(), 15);
        assert_eq!(g.ball_in_hand(), Some(BallInHand::Kitchen));
        // The snapshot keeps the mode.
        let json = serde_json::to_string(&g).unwrap();
        let back: EightBallGame = serde_json::from_str(&json).unwrap();
        assert!(back.is_time_attack());
        // An ordinary game's snapshot doesn't carry it.
        let plain = serde_json::to_string(&EightBallGame::new(5, 0, Rules::Official)).unwrap();
        assert!(!plain.contains("time_attack"));
    }

    #[test]
    fn the_table_is_never_handed_over_and_fouls_give_the_ball_in_hand() {
        let mut g = EightBallGame::time_attack(11);
        // A soft break straight up the table.
        let r = g.play_shot(&Shot::new(0.0, 6.0), None).unwrap();
        assert_eq!(r.shooter, 0);
        assert_eq!(g.current_player(), 0);
        assert!(!g.is_break());
        // A shot away from every ball: no contact, a foul, still at the table.
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
            .expect("a clear direction");
        let r = g.play_shot(&Shot::new(yaw, 0.6), None).unwrap();
        assert!(r.fouls.contains(&Foul::NoContact));
        assert!(penalised(&r));
        assert_eq!(g.current_player(), 0);
        assert_eq!(g.ball_in_hand(), Some(BallInHand::Anywhere));
        assert!(g.game_over().is_none());
    }

    #[test]
    fn the_eight_goes_last() {
        // The 8 and one more ball, the 8 in front of a corner pocket.
        let mut g = EightBallGame::time_attack(3);
        let t = Table::default();
        let pocket = t.pocket(crate::PocketId::FootRight).aim_point();
        let eight = pocket + (DVec2::ZERO - pocket).normalize() * 0.2;
        let cue = eight + (DVec2::ZERO - pocket).normalize() * 0.3;
        g.set_up_test_position(&[
            (BallId::CUE, cue),
            (BallId::EIGHT, eight),
            (BallId(3), DVec2::new(-0.6, 0.2)),
        ]);
        let d = eight - cue;
        let r = g.play_shot(&Shot::new(d.y.atan2(d.x), 2.0), None).unwrap();
        assert!(r.pocketed.iter().any(|(b, _)| *b == BallId::EIGHT), "{r:?}");
        assert!(r.spotted.contains(&BallId::EIGHT));
        assert!(penalised(&r));
        assert!(g.ball(BallId::EIGHT).in_play());
        assert!(g.game_over().is_none());
        // With only the 8 left, making it clears the table.
        let mut g = EightBallGame::time_attack(3);
        g.set_up_test_position(&[(BallId::CUE, cue), (BallId::EIGHT, eight)]);
        // Nothing is called, the 8 neither.
        assert!(!g.may_call_pocket() && g.check_eight_call(None).is_ok());
        let r = g.play_shot(&Shot::new(d.y.atan2(d.x), 2.0), None).unwrap();
        assert!(!penalised(&r));
        assert_eq!(g.balls_left(), 0);
        assert!(g.game_over().is_some_and(|o| o.winner == 0));
    }
}
