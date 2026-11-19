//! Numbers about how a player played a game: potting success, balls made,
//! runs, fouls. Kept by the game for the end-of-game panel and by the server
//! for profiles.

use serde::{Deserialize, Serialize};

use crate::ShotReport;

/// One player's numbers for the current game (the end-of-game panel).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShotStats {
    pub shots: u32,
    /// Shots that legally made at least one ball.
    pub potting_shots: u32,
    /// Object balls legally made: the player's own, and the 8 that won (see
    /// [`ShotReport::balls_made`]).
    pub balls: u32,
    pub fouls: u32,
    /// Most balls made in one visit to the table.
    pub best_run: u32,
    /// Balls made in the current visit.
    #[serde(skip)]
    pub run: u32,
    /// Shots taken with the cue ball in hand.
    pub in_hand: u32,
    /// Balls legally made with spin or a raised cue (see [`ShotStats::add_spin`]).
    #[serde(default)]
    pub spin_pots: u32,
    /// Visits to the table with three or more balls made.
    #[serde(default)]
    pub runs: u32,
}

impl ShotStats {
    /// Updates the numbers with a shot by this player. Only the balls made
    /// for them count: one of the opponent's that went down passed the turn
    /// and counts for the opponent.
    pub fn add(&mut self, report: &ShotReport, in_hand: bool) {
        self.shots += 1;
        self.in_hand += in_hand as u32;
        if report.is_foul() {
            self.fouls += 1;
        }
        let made = report.balls_made().len() as u32;
        if made > 0 {
            self.potting_shots += 1;
            self.balls += made;
            if self.run < 3 && self.run + made >= 3 {
                self.runs += 1;
            }
            self.run += made;
            self.best_run = self.best_run.max(self.run);
        }
        if !report.continues {
            self.run = 0;
        }
    }

    /// Counts the balls `report` legally made (the same as [`Self::add`]) if
    /// `shot` had spin: the tip clearly off centre, or the cue raised (a
    /// jump or a massé).
    pub fn add_spin(&mut self, shot: &crate::Shot, report: &ShotReport) {
        if report.is_foul() || (shot.tip.length() < 0.3 && shot.elevation < 0.2) {
            return;
        }
        self.spin_pots += report.balls_made().len() as u32;
    }

    /// Potting success, 0..=100.
    pub fn pot_rate(&self) -> u32 {
        (self.potting_shots * 100).checked_div(self.shots).unwrap_or(0)
    }

    /// Share of shots with ball in hand, 0..=100.
    pub fn in_hand_rate(&self) -> u32 {
        (self.in_hand * 100).checked_div(self.shots).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BallId, Foul, GameOver, GameOverReason, Group, PocketId, Shot};

    /// A shot by player 0 that hit `first` first and made `made`.
    fn report(first: u8, made: &[u8]) -> ShotReport {
        ShotReport {
            shooter: 0,
            was_break: false,
            first_contact: Some(BallId(first)),
            pocketed: made.iter().map(|n| (BallId(*n), PocketId::FootLeft)).collect(),
            fouls: vec![],
            spotted: vec![],
            group_assigned: None,
            continues: false,
            game_over: None,
            opponent_ball: false,
            penalty: vec![],
            reracked: false,
            break_again: false,
        }
    }

    #[test]
    fn only_the_shooters_own_balls_count() {
        let mut s = ShotStats::default();
        // Their own 3 (they hit it first: solids) with the opponent's 12:
        // one ball, and the turn passes, which ends the run.
        s.add(&ShotReport { opponent_ball: true, ..report(3, &[3, 12]) }, false);
        assert_eq!((s.potting_shots, s.balls, s.best_run), (1, 1, 1));
        // Only the opponent's ball, off a legal hit: nothing made.
        s.add(&ShotReport { opponent_ball: true, ..report(5, &[12]) }, false);
        assert_eq!((s.shots, s.potting_shots, s.balls), (2, 1, 1));
        assert_eq!(s.pot_rate(), 50);
        // An open table: the first ball made (the 10) gives the shooter the
        // stripes, so the 2 that followed it went down for the opponent.
        let mut s = ShotStats::default();
        let open = ShotReport { group_assigned: Some(Group::Stripes), ..report(2, &[10, 2]) };
        s.add(&ShotReport { continues: true, ..open }, false);
        assert_eq!(s.balls, 1);
    }

    #[test]
    fn the_eight_counts_only_when_it_wins() {
        let over = |winner, reason| Some(GameOver { winner, reason });
        let won =
            ShotReport { game_over: over(0, GameOverReason::PocketedEight), ..report(8, &[8]) };
        assert_eq!(won.balls_made(), vec![BallId::EIGHT]);
        let wrong_pocket = ShotReport {
            game_over: over(1, GameOverReason::EightInWrongPocket),
            ..report(8, &[8])
        };
        assert!(wrong_pocket.balls_made().is_empty());
        // Too early, with two of one's own: the game is lost, nothing made.
        let early = ShotReport {
            game_over: over(1, GameOverReason::EightPocketedEarly),
            ..report(3, &[3, 4, 8])
        };
        assert!(early.balls_made().is_empty());
        let mut s = ShotStats::default();
        for r in [&won, &wrong_pocket, &early] {
            s.add(r, false);
        }
        assert_eq!((s.shots, s.potting_shots, s.balls), (3, 1, 1));
    }

    #[test]
    fn spin_counts_the_same_balls() {
        let spin = Shot::new(0.0, 2.0).with_tip(glam::DVec2::new(0.0, -0.6));
        let mut s = ShotStats::default();
        s.add_spin(&spin, &ShotReport { opponent_ball: true, ..report(3, &[3, 12]) });
        assert_eq!(s.spin_pots, 1);
        // A foul makes nothing, whatever went down.
        let foul = ShotReport { fouls: vec![Foul::CueBallPocketed], ..report(3, &[3, 0]) };
        s.add_spin(&spin, &foul);
        assert_eq!(s.spin_pots, 1);
        // Centre ball: not a spin pot.
        s.add_spin(&Shot::new(0.0, 2.0), &ShotReport { continues: true, ..report(3, &[3]) });
        assert_eq!(s.spin_pots, 1);
    }

    /// The same on a real shot, on an open table: the 3 knocks the 12 into
    /// the corner and follows it in. The first ball down gives the shooter
    /// its group; the other one went down for the opponent.
    #[test]
    fn a_combination_that_makes_both_counts_one() {
        use glam::DVec2;
        let mut g = crate::EightBallGame::new(3, 0);
        let pocket = g.table().pocket(PocketId::FootLeft).aim_point();
        let r = g.table().spec.ball_radius;
        let dir = (pocket - DVec2::new(0.2, 0.0)).normalize();
        let twelve = pocket - dir * 0.12;
        let three = twelve - dir * (2.0 * r + 0.15);
        let cue = three - dir * 0.35;
        g.set_up_test_position(&[
            (BallId::CUE, cue),
            (BallId(3), three),
            (BallId(12), twelve),
            (BallId(1), DVec2::new(-0.6, -0.3)),
            (BallId(9), DVec2::new(-0.3, -0.35)),
            (BallId::EIGHT, DVec2::new(-0.6, 0.3)),
        ]);
        let both = (0..60)
            .map(|k| Shot::new(crate::math::atan2(dir.y, dir.x), 1.5 + k as f64 * 0.04))
            .map(|s| s.with_tip(DVec2::new(0.0, 0.8)))
            .find_map(|s| {
                let mut t = g.clone();
                let r = t.play_shot(&s, None).ok()?;
                let made: Vec<u8> = r.pocketed.iter().map(|(b, _)| b.0).collect();
                (!r.is_foul() && made.contains(&3) && made.contains(&12)).then_some(r)
            })
            .expect("a stroke that makes the 3 and the 12");
        let mut s = ShotStats::default();
        s.add(&both, false);
        assert_eq!(both.group_assigned, Some(Group::of(both.pocketed[0].0).unwrap()));
        assert_eq!(s.balls, 1, "{both:?}");
    }
}
