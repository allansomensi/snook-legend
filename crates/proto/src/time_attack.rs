//! Against the clock: clear a full rack alone, as fast as possible (the
//! rules are in `snook_legend_core::rules::time_attack`).
//!
//! Offline it is played any time, for the player's own best. Online the
//! attempt counts on the week's table: the server deals the rack (a seed
//! of its own for each attempt, told when it starts), the game sends the
//! strokes once the table is clear, and the server plays them again
//! ([`replay`]) as it does the daily challenge's. The time is the server's:
//! from the start it answered to the strokes reaching it, never less than
//! the balls took to roll (a game can't skip its shots), plus
//! [`FOUL_PENALTY_MS`] for each shot with a foul.
//!
//! The week runs from Monday 00:00 UTC to Sunday; each player's best time
//! of the week ranks them, and the week's first three get coins
//! ([`PRIZES`]) once it is over.

use glam::DVec2;
use serde::{Deserialize, Serialize};
use snook_legend_core::rules::time_attack::penalised;
use snook_legend_core::{BallId, EightBallGame, Shot};

/// What each shot with a foul (or the 8 made early) adds to the time.
pub const FOUL_PENALTY_MS: u32 = 10_000;
/// Attempts that count, per player and day (UTC): enough to improve, too
/// few to grind the table with thousands of racks.
pub const ATTEMPTS_PER_DAY: u32 = 10;
/// An attempt must be sent this long after it started at the latest.
pub const TIME_LIMIT_SECS: i64 = 20 * 60;
/// The most strokes an attempt may have.
pub const MAX_STROKES: usize = 150;
/// Coins for the week's first, second and third, until the staff set
/// others (the live settings `time_attack_prize_1` to `_3`).
pub const PRIZES: [i64; 3] = [100, 60, 30];
/// How many lead the week's table (and are shown).
pub const BOARD_SIZE: usize = 20;

/// The week (Monday to Sunday, UTC) of a time in Unix seconds: weeks
/// counted from the Monday before 1 January 1970.
pub fn week_of(t: i64) -> i64 {
    (t.div_euclid(86_400) + 3).div_euclid(7)
}

/// When a week starts (Monday 00:00 UTC, Unix seconds).
pub fn week_start(week: i64) -> i64 {
    (week * 7 - 3) * 86_400
}

/// The day (UTC) of a time in Unix seconds (the attempts are counted by it).
pub fn day_of(t: i64) -> i64 {
    t.div_euclid(86_400)
}

/// One stroke of an attempt: the shot, and where the cue ball was put when
/// it was in hand (the break's too).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub shot: Shot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cue_ball: Option<DVec2>,
}

/// What the server found playing an attempt again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub shots: u32,
    /// Shots with a foul (each costs [`FOUL_PENALTY_MS`]).
    pub fouls: u32,
    /// How long the balls rolled, all shots together (milliseconds).
    pub rolling_ms: u32,
}

impl Run {
    pub fn penalty_ms(&self) -> u32 {
        self.fouls.saturating_mul(FOUL_PENALTY_MS)
    }

    /// The attempt's time given the time it took on the server's clock.
    pub fn time_ms(&self, wall_ms: u32) -> u32 {
        wall_ms.max(self.rolling_ms).saturating_add(self.penalty_ms())
    }
}

/// Why an attempt was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refused {
    /// A stroke that can't be played (not a number, a cue ball put where
    /// it can't be, a stroke after the table was clear).
    BadStroke,
    /// The strokes don't clear the table.
    NotCleared,
    /// No attempt was started, it was already sent, or it took too long.
    NotStarted,
    /// No attempts left today.
    NoAttemptsLeft,
    #[serde(other)]
    Other,
}

/// Plays an attempt's strokes on the rack of `seed`: what it took, if they
/// clear the table.
pub fn replay(seed: u64, strokes: &[Stroke]) -> Result<Run, Refused> {
    if strokes.len() > MAX_STROKES {
        return Err(Refused::BadStroke);
    }
    let mut g = EightBallGame::time_attack(seed);
    let mut run = Run { shots: 0, fouls: 0, rolling_ms: 0 };
    let mut rolling = 0.0f64;
    for s in strokes {
        let shot = s.shot;
        let finite = shot.yaw.is_finite()
            && shot.elevation.is_finite()
            && shot.tip.is_finite()
            && shot.speed.is_finite()
            && s.cue_ball.is_none_or(|p| p.is_finite());
        if !finite || g.game_over().is_some() {
            return Err(Refused::BadStroke);
        }
        if let Some(p) = s.cue_ball
            && g.ball_in_hand().is_some()
            && g.ball(BallId::CUE).xy() != p
            && !g.place_cue_ball(p)
        {
            return Err(Refused::BadStroke);
        }
        let mut pending = g.prepare_shot(&shot, None).map_err(|_| Refused::BadStroke)?;
        pending.simulation.run_to_rest();
        rolling += pending.simulation.time();
        let report = g.finish_shot(pending);
        run.shots += 1;
        if penalised(&report) {
            run.fouls += 1;
        }
    }
    if g.game_over().is_none() {
        return Err(Refused::NotCleared);
    }
    run.rolling_ms = (rolling * 1000.0).round().min(u32::MAX as f64) as u32;
    Ok(run)
}

/// A plain way through a rack (tests, and anything that needs an attempt
/// that clears): with the cue ball in hand, a straight pot of a ball with a
/// clear way to a pocket; without it, a stroke that hits nothing (a foul,
/// so the ball is in hand again). Slow on the clock, but sure. None if the
/// rack of `seed` isn't cleared within [`MAX_STROKES`].
#[doc(hidden)]
pub fn plain_strokes(seed: u64) -> Option<Vec<Stroke>> {
    use snook_legend_core::PocketId;
    let mut g = EightBallGame::time_attack(seed);
    let r = g.table().spec.ball_radius;
    let mut strokes = Vec::new();
    let brk = Shot::new(g.default_aim(), g.max_cue_speed() * 0.8);
    let cue = g.ball(BallId::CUE).xy();
    g.play_shot(&brk, None).ok()?;
    strokes.push(Stroke { shot: brk, cue_ball: Some(cue) });
    // Nothing of the table's balls within `room` of the way from `a` to `b`
    // (but `skip`).
    let clear = |g: &EightBallGame, a: DVec2, b: DVec2, skip: BallId, room: f64| {
        let d = b - a;
        let len = d.length();
        let dir = d / len;
        g.balls().iter().filter(|x| x.in_play() && x.id != skip && x.id != BallId::CUE).all(|x| {
            let rel = x.xy() - a;
            let along = rel.dot(dir).clamp(0.0, len);
            (rel - dir * along).length() > room
        })
    };
    while g.game_over().is_none() && strokes.len() < MAX_STROKES {
        let mut stroke = None;
        if g.ball_in_hand().is_some() {
            let last = g.balls_left() == 1;
            'search: for b in g.balls().iter().filter(|b| b.in_play() && b.id.is_object()) {
                if b.id == BallId::EIGHT && !last {
                    continue;
                }
                for p in PocketId::ALL {
                    let target = g.table().pocket(p).aim_point();
                    let dir = (target - b.xy()).normalize();
                    let ghost = b.xy() - dir * 2.0 * r;
                    if !clear(&g, b.xy(), target, b.id, 2.1 * r) {
                        continue;
                    }
                    // Straight behind it, or cutting it from either side.
                    for deg in [0.0f64, 15.0, -15.0, 30.0, -30.0, 45.0, -45.0] {
                        let a = DVec2::from_angle(deg.to_radians()).rotate(dir);
                        let at = ghost - a * 0.15;
                        if g.can_place_cue_ball(at) && clear(&g, at, ghost, b.id, 2.1 * r) {
                            let speed = 1.2 + 0.8 * b.xy().distance(target) + deg.abs() / 30.0;
                            let shot = Shot::new(a.y.atan2(a.x), speed);
                            stroke = Some(Stroke { shot, cue_ball: Some(at) });
                            break 'search;
                        }
                    }
                }
            }
            // Nothing to pot: into the balls from somewhere free, to
            // spread them.
            if stroke.is_none() {
                let n = strokes.len();
                let targets: Vec<DVec2> = g
                    .balls()
                    .iter()
                    .filter(|b| b.in_play() && b.id.is_object())
                    .map(|b| b.xy())
                    .collect();
                let target = targets[n % targets.len()];
                let spots = (0..16).map(|k| {
                    DVec2::from_angle(k as f64 * std::f64::consts::TAU / 16.0) * 0.3 + target
                });
                for at in spots {
                    if g.can_place_cue_ball(at) {
                        let d = target - at;
                        stroke = Some(Stroke {
                            shot: Shot::new(d.y.atan2(d.x), 4.0),
                            cue_ball: Some(at),
                        });
                        break;
                    }
                }
            }
        }
        let stroke = match stroke {
            Some(s) => s,
            // A stroke that hits nothing, or (nowhere clear) one into the
            // nearest ball, to spread the balls.
            None => {
                let cue = g.ball(BallId::CUE).xy();
                let away = (0..72).map(|k| k as f64 * 5f64.to_radians()).find(|&yaw| {
                    let end = cue + DVec2::new(yaw.cos(), yaw.sin()) * 3.0;
                    clear(&g, cue, end, BallId::CUE, 2.5 * r)
                });
                let yaw = away.unwrap_or_else(|| g.default_aim());
                let speed = if away.is_some() { 0.8 } else { 4.0 };
                Stroke { shot: Shot::new(yaw, speed), cue_ball: None }
            }
        };
        if let Some(p) = stroke.cue_ball {
            g.place_cue_ball(p);
        }
        g.play_shot(&stroke.shot, None).ok()?;
        strokes.push(stroke);
    }
    g.game_over().map(|_| strokes)
}

/// One line of the week's table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub username: String,
    #[serde(default)]
    pub country: String,
    /// The time (milliseconds, penalties included).
    pub ms: u32,
    /// When it was made (Unix seconds).
    pub at: i64,
}

/// The week against the clock as one player sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Info {
    pub week: i64,
    /// When the week ends (Unix seconds).
    pub ends_at: i64,
    pub attempts_left: u32,
    /// Their best this week (milliseconds).
    pub best_ms: Option<u32>,
    /// Their place on the week's table (1 is first).
    pub rank: Option<u32>,
    /// Everyone who cleared the table this week.
    pub players: u32,
    /// The leaders, quickest first.
    pub board: Vec<Entry>,
    /// Last week's first three (they got the prizes).
    pub last_week: Vec<Entry>,
    /// An attempt is under way (started, not sent).
    pub open: bool,
    /// The coins of this week's first three (set by the staff; a server
    /// before 0.12.0's prizes were live sends none: [`PRIZES`]).
    #[serde(default = "default_prizes")]
    pub prizes: [i64; 3],
}

fn default_prizes() -> [i64; 3] {
    PRIZES
}

/// What a counted attempt achieved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub ms: u32,
    pub shots: u32,
    pub fouls: u32,
    /// The player's best of the week (now).
    pub best: bool,
    pub rank: Option<u32>,
}

/// A time as players read it: "1:23.4" (minutes, seconds, tenths).
pub fn clock(ms: u32) -> String {
    let tenths = ms / 100;
    format!("{}:{:02}.{}", tenths / 600, tenths / 10 % 60, tenths % 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weeks_start_on_monday() {
        // 2026-11-23 was a Monday.
        let monday = 1_795_392_000;
        assert_eq!(week_start(week_of(monday)), monday);
        assert_eq!(week_of(monday - 1), week_of(monday) - 1);
        assert_eq!(week_of(monday + 7 * 86_400 - 1), week_of(monday));
        // 1970-01-05 was the first Monday after the epoch.
        assert_eq!(week_start(1), 4 * 86_400);
    }

    #[test]
    fn times_read_as_minutes_and_seconds() {
        assert_eq!(clock(83_456), "1:23.4");
        assert_eq!(clock(5_000), "0:05.0");
    }

    #[test]
    fn the_time_never_beats_the_rolling_balls() {
        let run = Run { shots: 20, fouls: 2, rolling_ms: 50_000 };
        assert_eq!(run.time_ms(40_000), 70_000);
        assert_eq!(run.time_ms(90_000), 110_000);
    }

    /// Strokes that don't clear the table, or can't be played, are refused.
    #[test]
    fn only_a_cleared_table_counts() {
        let brk = Stroke { shot: Shot::new(0.0, 8.0), cue_ball: None };
        assert_eq!(replay(1, &[brk]), Err(Refused::NotCleared));
        let nan = Stroke { shot: Shot::new(f64::NAN, 1.0), cue_ball: None };
        assert_eq!(replay(1, &[nan]), Err(Refused::BadStroke));
        let far = Stroke { shot: Shot::new(0.0, 8.0), cue_ball: Some(DVec2::new(0.5, 0.0)) };
        assert_eq!(replay(1, &[far]), Err(Refused::BadStroke));
        assert_eq!(replay(1, &[brk; MAX_STROKES + 1]), Err(Refused::BadStroke));
    }

    /// A cleared rack counts with its fouls, and the same strokes give the
    /// same run.
    #[test]
    fn a_cleared_table_counts_with_its_fouls() {
        let seed = 42;
        let strokes = plain_strokes(seed).expect("a plain way through the rack");
        let run = replay(seed, &strokes).unwrap();
        assert_eq!(run.shots as usize, strokes.len());
        assert!(run.fouls > 0 && run.rolling_ms > 0);
        assert_eq!(replay(seed, &strokes), Ok(run));
        // A stroke after the clearance is refused, and another rack's
        // strokes don't clear this one.
        let mut more = strokes.clone();
        more.push(strokes[1]);
        assert_eq!(replay(seed, &more), Err(Refused::BadStroke));
        assert!(replay(seed + 1, &strokes).is_err());
        // (The way through works for any rack: the server's tests use it.)
        for seed in 0..12 {
            let strokes = plain_strokes(seed).unwrap_or_else(|| panic!("rack {seed}"));
            assert!(replay(seed, &strokes).is_ok(), "rack {seed}");
        }
    }
}
