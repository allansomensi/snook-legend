//! "Play the ghost": a computer opponent that plays in the style of a real
//! player. The server measures how a player plays from their recorded
//! online games ([`Tally`]) and sends only the totals ([`GhostStyle`]);
//! the game turns them into a computer player ([`GhostStyle::player`]).

use glam::DVec2;
use serde::{Deserialize, Serialize};
use snook_legend_core::ai::{AiPlayer, Search, Skill, Style};
use snook_legend_core::{EightBallGame, Shot, ShotReport, math};

use crate::replay::{Replay, Stroke, apply};

/// Shots needed before a style says anything about a player.
pub const MIN_SHOTS: u32 = 60;

/// The most recent games a style is measured on.
pub const MAX_GAMES: u32 = 25;

/// How a player plays, in aggregated numbers only (no game, no opponent).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct GhostStyle {
    /// Games and shots measured (breaks apart).
    pub games: u32,
    pub shots: u32,
    /// Average cue speed of ordinary shots and of breaks (m/s).
    pub speed: f64,
    pub break_speed: f64,
    /// Average distance of the tip from the centre of the cue ball (0 to 1).
    pub spin: f64,
    /// Share of shots with side English.
    pub side: f64,
    /// Average cue elevation (radians).
    pub elevation: f64,
    /// Share of shots that were safeties (no pot on).
    pub safety_rate: f64,
    /// Share of pot attempts played off a cushion.
    pub bank_rate: f64,
    /// Share of pot attempts that dropped.
    pub pot_rate: f64,
}

/// The server's answer to a request for a player's style.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GhostAnswer {
    Ready {
        style: GhostStyle,
    },
    /// Too few recorded shots to say how they play.
    NotEnough {
        shots: u32,
        needed: u32,
    },
    NoSuchPlayer,
    /// The server is measuring other players: try again in a moment.
    Busy,
    /// Asked too often: try again in a minute.
    TooSoon,
    #[serde(other)]
    Unknown,
}

/// A number in range (or the default, when it is not a number).
fn finite(v: f64, lo: f64, hi: f64, default: f64) -> f64 {
    if v.is_finite() { v.clamp(lo, hi) } else { default }
}

/// `a` at `k` = 0 to `b` at `k` = 1.
fn lerp(a: f64, b: f64, k: f64) -> f64 {
    a + (b - a) * k.clamp(0.0, 1.0)
}

impl GhostStyle {
    /// Every number finite and within its range.
    pub fn sanitized(self) -> GhostStyle {
        GhostStyle {
            games: self.games,
            shots: self.shots,
            speed: finite(self.speed, 0.0, 7.0, 2.5),
            break_speed: finite(self.break_speed, 0.0, 7.0, 0.0),
            spin: finite(self.spin, 0.0, 1.0, 0.3),
            side: finite(self.side, 0.0, 1.0, 0.0),
            elevation: finite(self.elevation, 0.0, 1.4, 0.0),
            safety_rate: finite(self.safety_rate, 0.0, 1.0, 0.15),
            bank_rate: finite(self.bank_rate, 0.0, 1.0, 0.05),
            pot_rate: finite(self.pot_rate, 0.0, 1.0, 0.5),
        }
    }

    /// A computer player that plays like this: its accuracy from the potting
    /// success, its taste from the rest.
    pub fn player(&self) -> AiPlayer {
        let s = self.sanitized();
        // 25% of pots made reads as a beginner, 85% as a master.
        let k = (s.pot_rate - 0.25) / 0.6;
        let skill = Skill {
            aim_noise_deg: lerp(1.8, 0.25, k),
            speed_noise: lerp(0.2, 0.04, k),
            break_speed: if s.break_speed > 0.0 { s.break_speed.clamp(4.0, 7.2) } else { 6.3 },
            // A person always picks among speeds and spins.
            search: if k >= 0.65 { Search::Wide } else { Search::Normal },
            position_weight: lerp(120.0, 420.0, k),
            plays_safe: s.safety_rate >= 0.04,
            banks: s.bank_rate >= 0.03,
        };
        let style = Style {
            safety_threshold: (700.0 + (s.safety_rate - 0.15) * 2000.0).clamp(200.0, 1050.0),
            safety_value: (1.0 + (s.safety_rate - 0.15) * 4.0).clamp(0.3, 2.0),
            banks: ((s.bank_rate - 0.06) * 12.0).clamp(-1.0, 1.0),
            power: ((s.speed - 2.6) / 1.4).clamp(-1.0, 1.0),
            spin: (s.spin / 0.3 + if s.side > 0.25 { 0.15 } else { 0.0 }).clamp(0.0, 1.5),
            position: 1.0,
            elevation: if s.elevation > 0.02 { s.elevation.min(0.15) } else { 0.0 },
        };
        AiPlayer { skill: skill.clamped(), style: style.clamped() }
    }
}

/// Running totals of a player's shots.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Tally {
    pub games: u32,
    pub shots: u32,
    speed: f64,
    breaks: u32,
    break_speed: f64,
    spin: f64,
    side: u32,
    elevation: f64,
    safeties: u32,
    attempts: u32,
    banks: u32,
    made: u32,
}

/// What a shot was meant to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// The object ball was sent straight at a pocket.
    Pot,
    /// The object ball was sent off a cushion at a pocket.
    Bank,
    /// No pocket on the line: a safety.
    Safety,
    /// Can't tell (missed everything, a jump or a massé).
    Unknown,
}

/// How far a line passes from a point (`dir` a unit vector), if the point
/// is ahead of it.
fn miss_distance(from: DVec2, dir: DVec2, point: DVec2) -> Option<f64> {
    let to = point - from;
    (to.dot(dir) > 0.0).then(|| (to.x * dir.y - to.y * dir.x).abs())
}

/// How far off line (radians, about 3°) a pot may go and still read as
/// one.
const AIM_SLACK: f64 = 0.05;

/// What `shot` tried to do on `before`, judged from the line the first ball
/// hit was sent along (the "ghost ball" line).
pub fn intent(before: &EightBallGame, shot: &Shot, report: &ShotReport) -> Intent {
    let Some(target) = report.first_contact else { return Intent::Unknown };
    let table = before.table();
    let r = table.spec.ball_radius;
    let cue = before.ball(snook_legend_core::BallId::CUE).xy();
    let object = before.ball(target).xy();
    let dir = math::from_angle(shot.yaw);
    let to_obj = object - cue;
    let along = to_obj.dot(dir);
    let off2 = to_obj.length_squared() - along * along;
    let reach = 4.0 * r * r - off2;
    if along <= 0.0 || reach < 0.0 {
        return Intent::Unknown;
    }
    let ghost = cue + dir * (along - reach.sqrt());
    let line = (object - ghost).normalize_or_zero();
    if line == DVec2::ZERO {
        return Intent::Unknown;
    }
    // A long pot a little off line was still a pot: allow a few degrees of
    // aim error on top of the pocket's mouth.
    let near = |point: DVec2, slack: f64, half: f64| {
        let reach = (point - object).length() * AIM_SLACK;
        miss_distance(object, line, point).is_some_and(|d| d < half * slack + reach)
    };
    if table.pockets.iter().any(|p| near(p.aim_point(), 1.6, p.half_mouth)) {
        return Intent::Pot;
    }
    let (hl, hw) = (table.spec.length / 2.0 - r, table.spec.width / 2.0 - r);
    let bank = table.pockets.iter().any(|p| {
        let a = p.aim_point();
        [
            DVec2::new(a.x, 2.0 * hw - a.y),
            DVec2::new(a.x, -2.0 * hw - a.y),
            DVec2::new(2.0 * hl - a.x, a.y),
            DVec2::new(-2.0 * hl - a.x, a.y),
        ]
        .into_iter()
        .any(|m| near(m, 2.2, p.half_mouth))
    });
    if bank { Intent::Bank } else { Intent::Safety }
}

impl Tally {
    /// Counts one shot of the player (`shot` as struck, `before` the table
    /// it was struck on).
    pub fn add_shot(&mut self, before: &EightBallGame, shot: &Shot, report: &ShotReport) {
        let shot = shot.sanitized(before.params());
        if report.was_break {
            self.breaks += 1;
            self.break_speed += shot.speed;
            return;
        }
        self.shots += 1;
        self.speed += shot.speed;
        self.spin += shot.tip.length().min(1.0);
        self.side += (shot.tip.x.abs() > 0.15) as u32;
        self.elevation += shot.elevation;
        let made = !report.is_foul()
            && report.first_contact.is_some_and(|t| report.pocketed.iter().any(|(b, _)| *b == t));
        match intent(before, &shot, report) {
            Intent::Pot => {
                self.attempts += 1;
                self.made += made as u32;
            }
            Intent::Bank => {
                self.attempts += 1;
                self.banks += 1;
                self.made += made as u32;
            }
            Intent::Safety if made => {
                // It dropped after all (a kick or a combination): a pot.
                self.attempts += 1;
                self.made += 1;
            }
            Intent::Safety => self.safeties += 1,
            Intent::Unknown => {}
        }
    }

    /// Counts the shots of `seat` in a recorded game (played again stroke
    /// by stroke). Stops at the first stroke that can't be played.
    pub fn add_game(&mut self, replay: &Replay, seat: usize) {
        let mut game = replay.start();
        let mut counted = false;
        for stroke in &replay.strokes {
            if game.game_over().is_some() {
                break;
            }
            let shooter = game.current_player();
            let before = (shooter == seat % 2).then(|| game.clone());
            let Ok(report) = apply(&mut game, stroke) else { break };
            if let (Some(mut before), Some(report), Stroke::Shot { shot, cue_ball, .. }) =
                (before, report, stroke)
            {
                // The table as it was struck: the cue ball where it was put.
                if let Some(p) = cue_ball
                    && before.ball_in_hand().is_some()
                {
                    before.place_cue_ball(*p);
                }
                self.add_shot(&before, shot, &report);
                counted = true;
            }
        }
        self.games += counted as u32;
    }

    /// The averages.
    pub fn style(&self) -> GhostStyle {
        let per = |sum: f64, n: u32| if n == 0 { 0.0 } else { sum / n as f64 };
        let share = |k: u32, n: u32| per(k as f64, n);
        GhostStyle {
            games: self.games,
            shots: self.shots,
            speed: per(self.speed, self.shots),
            break_speed: per(self.break_speed, self.breaks),
            spin: per(self.spin, self.shots),
            side: share(self.side, self.shots),
            elevation: per(self.elevation, self.shots),
            safety_rate: share(self.safeties, self.shots),
            bank_rate: share(self.banks, self.attempts),
            pot_rate: share(self.made, self.attempts),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ladder::computer_stroke;
    use snook_legend_core::Difficulty;
    use snook_legend_core::personalities::personality;

    /// The style of seat 0 over a few games of `who` against a medium player.
    fn measured(who: &AiPlayer, games: u64) -> GhostStyle {
        let mut tally = Tally::default();
        let other = Difficulty::Medium.player();
        for seed in 0..games {
            let mut replay = Replay::new(seed + 50, (seed % 2) as usize);
            let mut game = replay.start();
            while game.game_over().is_none() && replay.strokes.len() < 120 {
                let p = if game.current_player() == 0 { who } else { &other };
                let s = computer_stroke(&game, p, replay.seed);
                apply(&mut game, &s).unwrap();
                replay.strokes.push(s);
            }
            tally.add_game(&replay, 0);
        }
        tally.style()
    }

    #[test]
    fn styles_tell_players_apart() {
        let cannon = measured(&personality("bruno").unwrap().player, 3);
        let wall = measured(&personality("marta").unwrap().player, 3);
        assert!(cannon.shots > 10 && wall.shots > 10);
        assert_eq!(cannon.games, 3);
        assert!(cannon.speed > 2.0 * wall.speed, "{cannon:?} {wall:?}");
        assert!(cannon.break_speed > wall.break_speed, "{cannon:?} {wall:?}");
        // (Over a few games the share of safeties is no test: on a 9-foot
        // table a wild hitter's missed long pots read much like safeties.)
        // And the ghosts play the same way round.
        let (c, w) = (cannon.player(), wall.player());
        assert!(c.style.power > w.style.power);
    }

    #[test]
    fn nonsense_numbers_make_a_sane_player() {
        let wild = GhostStyle {
            games: 1,
            shots: 100,
            speed: f64::NAN,
            break_speed: 99.0,
            spin: -3.0,
            side: 2.0,
            elevation: f64::INFINITY,
            safety_rate: 5.0,
            bank_rate: f64::NEG_INFINITY,
            pot_rate: 0.99,
        };
        let p = wild.player();
        assert_eq!(p.style, p.style.clamped());
        assert_eq!(p.skill, p.skill.clamped());
        assert!(p.skill.aim_noise_deg >= 0.25 && p.skill.break_speed <= 7.2);
        // An empty tally has no numbers, not NaNs.
        let empty = Tally::default().style();
        assert_eq!(empty.shots, 0);
        assert!(empty.pot_rate == 0.0 && empty.speed == 0.0);
    }
}
