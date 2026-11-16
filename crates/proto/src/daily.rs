//! The daily challenge: the same position for everyone, all day (UTC).
//! The player has a few attempts to make as many balls as possible in one
//! visit to the table: a miss or a foul ends it.
//!
//! The day's position comes from a seed the server keeps to itself until an
//! attempt starts (the code that turns it into a position is public, the
//! seed isn't). The game sends the attempt's strokes and the server plays
//! them again ([`score`]): the result is the server's, never the game's.

use glam::DVec2;
use serde::{Deserialize, Serialize};
use snookered_core::rng::Rng;
use snookered_core::{BallId, EightBallGame, Shot};

/// Attempts per day.
pub const ATTEMPTS: u32 = 3;
/// An attempt must be sent this long after it started at the latest
/// (seconds): enough for a careful run, too little to come back tomorrow.
pub const TIME_LIMIT_SECS: i64 = 20 * 60;
/// Coins for clearing the table (once a day).
pub const CLEAR_COINS: i64 = 25;
/// How many lead the day's table (and are shown).
pub const BOARD_SIZE: usize = 20;
/// The fewest and most object balls on the table.
pub const MIN_BALLS: usize = 5;
pub const MAX_BALLS: usize = 7;

/// The day (UTC) of a time in Unix seconds.
pub fn day_of(t: i64) -> i64 {
    t.div_euclid(86_400)
}

/// The position for a seed: the cue ball and five to seven solids, apart
/// from each other, the cushions and the pockets' mouths.
pub fn position(seed: u64) -> Vec<(BallId, DVec2)> {
    let table = snookered_core::Table::default();
    let r = table.spec.ball_radius;
    let (hl, hw) = (table.spec.length / 2.0 - 2.5 * r, table.spec.width / 2.0 - 2.5 * r);
    let mut rng = Rng::new(seed ^ 0xDA11_C4A1_1E46_E000);
    let n = MIN_BALLS + rng.below(MAX_BALLS - MIN_BALLS + 1);
    let mut placed: Vec<(BallId, DVec2)> = Vec::new();
    // The cue ball first, then solids 1, 2, 3…
    let ids = std::iter::once(BallId::CUE).chain((1..=n as u8).map(BallId));
    for id in ids {
        for _ in 0..500 {
            let p =
                DVec2::new((rng.next_f64() * 2.0 - 1.0) * hl, (rng.next_f64() * 2.0 - 1.0) * hw);
            let clear_of_balls = placed.iter().all(|(_, q)| q.distance(p) > 4.0 * r);
            let clear_of_pockets = snookered_core::PocketId::ALL
                .iter()
                .all(|&k| table.pocket(k).aim_point().distance(p) > 0.12);
            if clear_of_balls && clear_of_pockets {
                placed.push((id, p));
                break;
            }
        }
    }
    placed
}

/// The game set up for a seed: past the break, an open table with only the
/// challenge's balls on it.
pub fn game(seed: u64) -> EightBallGame {
    let mut g = EightBallGame::new(seed, 0);
    g.set_up_test_position(&position(seed));
    g
}

/// How many object balls a seed puts on the table.
pub fn balls(seed: u64) -> u32 {
    position(seed).len().saturating_sub(1) as u32
}

/// What an attempt achieved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    /// Object balls made before the visit ended.
    pub balls: u32,
    /// The table was cleared.
    pub cleared: bool,
}

/// Why an attempt was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refused {
    /// A stroke that can't be played (not a number, or after the end).
    BadStroke,
    /// No attempt was started, it was already sent, or it took too long.
    NotStarted,
    /// No attempts left today.
    NoAttemptsLeft,
    /// The day is over (its board is closed).
    DayOver,
    #[serde(other)]
    Other,
}

/// Plays an attempt's strokes on the day's position: the balls made in the
/// one visit, which ends at the first stroke that doesn't keep the table.
/// Strokes after that, or that can't be played, refuse the whole attempt.
pub fn score(seed: u64, strokes: &[Shot]) -> Result<Outcome, Refused> {
    let mut g = game(seed);
    let total = balls(seed);
    let mut made = 0;
    for (k, shot) in strokes.iter().enumerate() {
        let finite = shot.yaw.is_finite()
            && shot.elevation.is_finite()
            && shot.tip.is_finite()
            && shot.speed.is_finite();
        if !finite || made >= total {
            return Err(Refused::BadStroke);
        }
        let report = g.play_shot(shot, None).map_err(|_| Refused::BadStroke)?;
        let keeps = report.continues && report.game_over.is_none();
        if keeps {
            made += report.pocketed.iter().filter(|(b, _)| *b != BallId::CUE).count() as u32;
        }
        if !keeps || made >= total {
            // The visit is over: nothing may follow.
            if k + 1 != strokes.len() {
                return Err(Refused::BadStroke);
            }
            break;
        }
    }
    let made = made.min(total);
    Ok(Outcome { balls: made, cleared: made == total })
}

/// One line of the day's table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub username: String,
    #[serde(default)]
    pub country: String,
    pub balls: u32,
    /// When they made it (Unix seconds; earlier is ahead on a tie).
    pub at: i64,
}

/// Today's challenge as one player sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Info {
    pub day: i64,
    /// Object balls on today's table.
    pub balls: u32,
    pub attempts_left: u32,
    /// Their best today (None: not tried yet).
    pub best: Option<u32>,
    /// Their place on today's table (1 is first).
    pub rank: Option<u32>,
    /// An attempt is under way (started, not sent).
    pub open: bool,
    /// The leaders, best first.
    pub board: Vec<Entry>,
    /// Everyone who tried today.
    pub players: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_are_fixed_by_the_seed_and_playable() {
        for seed in 0..50u64 {
            let p = position(seed);
            assert_eq!(p, position(seed));
            assert!((MIN_BALLS + 1..=MAX_BALLS + 1).contains(&p.len()), "seed {seed}");
            assert_eq!(p[0].0, BallId::CUE);
            let g = game(seed);
            assert!(!g.is_break());
            assert!(g.ball_in_hand().is_none());
            assert!(g.ball(BallId::EIGHT).status != snookered_core::BallStatus::InPlay);
        }
        assert_ne!(position(1), position(2));
    }

    /// A miss scores nothing; strokes after the end are refused; a made
    /// ball counts and the visit goes on.
    #[test]
    fn scores_one_visit() {
        let seed = 7;
        let g = game(seed);
        // Away from every ball: a miss (a foul) ends the visit at once.
        let cue = g.ball(BallId::CUE).xy();
        let away = (0..360)
            .map(|d| (d as f64).to_radians())
            .find(|&yaw| {
                let dir = DVec2::new(yaw.cos(), yaw.sin());
                position(seed).iter().skip(1).all(|(_, p)| {
                    let rel = *p - cue;
                    let along = rel.dot(dir);
                    along < 0.0 || (rel - dir * along).length() > 0.08
                })
            })
            .expect("a clear direction");
        let miss = Shot::new(away, 1.0);
        assert_eq!(score(seed, &[miss]), Ok(Outcome { balls: 0, cleared: false }));
        assert_eq!(score(seed, &[miss, miss]), Err(Refused::BadStroke));
        assert_eq!(score(seed, &[]), Ok(Outcome { balls: 0, cleared: false }));
        let nan = Shot::new(f64::NAN, 1.0);
        assert_eq!(score(seed, &[nan]), Err(Refused::BadStroke));
        // The computer's best pot keeps the table and counts.
        let decision = snookered_core::ai::decide(&g, snookered_core::Difficulty::Hard, 3);
        let mut after = g.clone();
        let report = after.play_shot(&decision.shot, None).unwrap();
        if report.continues {
            let made = score(seed, &[decision.shot, miss_from(&after)]);
            assert!(matches!(made, Ok(o) if o.balls >= 1) || made == Err(Refused::BadStroke));
        }
    }

    /// A stroke that misses everything from the position in `g`.
    fn miss_from(g: &EightBallGame) -> Shot {
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
        Shot::new(yaw, 1.0)
    }
}
