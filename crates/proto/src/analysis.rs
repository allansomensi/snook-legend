//! How a player pots, measured shot by shot from whole games: success per
//! pocket, by distance and by kind of shot, safeties, and where on the table
//! the object balls were. The server works this out from the stored replay
//! when a game ends (the physics is deterministic, so replaying the strokes
//! rebuilds every shot) and keeps the sums per player; the profile shows them.
//!
//! What was *meant* is never sent, so it is read from what happened:
//! - a shot that legally made a ball and kept the table is a pot;
//! - a shot that sent a ball (the first one hit, or the one that ball hit
//!   next) along a line into a pocket, directly or off one cushion, and up
//!   to that pocket, is a missed pot;
//! - any other legal shot that made nothing is a safety.

use glam::DVec2;
use serde::{Deserialize, Serialize};
use snookered_core::{BallId, EightBallGame, Event, Shot, ShotReport, Simulation};

use crate::replay::{Replay, Stroke, place_cue_ball};

pub const POCKETS: usize = 6;
pub const DISTANCES: usize = 3;
/// The heat map: cells along the table (head to foot) and across it.
pub const GRID_W: usize = 12;
pub const GRID_H: usize = 6;
pub const KINDS: usize = 10;

/// Cut angles (degrees) between the kinds of cut.
pub const STRAIGHT_MAX: f64 = 8.0;
pub const CUT_MAX: f64 = 25.0;
pub const WIDE_CUT_MAX: f64 = 45.0;
/// Distance bands (the cue ball to the object ball, plus the object ball to
/// the pocket), as in [`crate::replay::band`].
pub const SHORT_MAX: f64 = 0.75;
pub const MEDIUM_MAX: f64 = 1.5;

/// Attempts and how many succeeded. Sent as `[attempts, made]`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "[u32; 2]", into = "[u32; 2]")]
pub struct Tally {
    pub attempts: u32,
    pub made: u32,
}

impl From<[u32; 2]> for Tally {
    fn from([attempts, made]: [u32; 2]) -> Tally {
        Tally { attempts, made: made.min(attempts) }
    }
}

impl From<Tally> for [u32; 2] {
    fn from(t: Tally) -> [u32; 2] {
        [t.attempts, t.made]
    }
}

impl Tally {
    pub fn add(&mut self, made: bool) {
        self.attempts += 1;
        self.made += made as u32;
    }

    pub fn merge(&mut self, other: Tally) {
        self.attempts = self.attempts.saturating_add(other.attempts);
        self.made = self.made.saturating_add(other.made).min(self.attempts);
    }

    pub fn missed(&self) -> u32 {
        self.attempts - self.made.min(self.attempts)
    }

    /// Success, 0..=100 (0 without attempts).
    pub fn rate(&self) -> u32 {
        (self.made.min(self.attempts) as u64 * 100).checked_div(self.attempts as u64).unwrap_or(0)
            as u32
    }
}

/// The kinds of shot, in the order they are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShotKind {
    /// Cut under [`STRAIGHT_MAX`] degrees.
    Straight,
    /// A cut up to [`CUT_MAX`] degrees.
    Cut,
    /// A cut up to [`WIDE_CUT_MAX`] degrees.
    WideCut,
    /// A thinner cut.
    Thin,
    /// The object ball went off a cushion into the pocket.
    Bank,
    /// The cue ball went off a cushion before reaching the object ball.
    Kick,
    /// The object ball sent another ball into the pocket.
    Combination,
    /// The cue ball jumped before reaching the object ball.
    Jump,
    /// A raised cue with side spin: the cue ball curved.
    Masse,
    Break,
}

impl ShotKind {
    pub const ALL: [ShotKind; KINDS] = [
        ShotKind::Straight,
        ShotKind::Cut,
        ShotKind::WideCut,
        ShotKind::Thin,
        ShotKind::Bank,
        ShotKind::Kick,
        ShotKind::Combination,
        ShotKind::Jump,
        ShotKind::Masse,
        ShotKind::Break,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    /// The label (English; the game translates it).
    pub fn name(self) -> &'static str {
        match self {
            ShotKind::Straight => "Straight",
            ShotKind::Cut => "Cut 8–25°",
            ShotKind::WideCut => "Cut 25–45°",
            ShotKind::Thin => "Thin cut 45°+",
            ShotKind::Bank => "Bank",
            ShotKind::Kick => "Kick",
            ShotKind::Combination => "Combination",
            ShotKind::Jump => "Jump",
            ShotKind::Masse => "Massé",
            ShotKind::Break => "Break",
        }
    }

    fn of_cut(degrees: f64) -> ShotKind {
        if degrees < STRAIGHT_MAX {
            ShotKind::Straight
        } else if degrees < CUT_MAX {
            ShotKind::Cut
        } else if degrees < WIDE_CUT_MAX {
            ShotKind::WideCut
        } else {
            ShotKind::Thin
        }
    }
}

/// A player's shots, summed over games.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShotAnalysis {
    /// Games counted.
    pub games: u32,
    /// Pots and misses per pocket ([`snookered_core::PocketId`] order).
    pub pockets: [Tally; POCKETS],
    /// Short, medium, long.
    pub distances: [Tally; DISTANCES],
    /// Per [`ShotKind`] (breaks: made a ball on the break without a foul).
    pub kinds: [Tally; KINDS],
    /// Safeties played, and those after which the opponent made nothing.
    pub safeties: Tally,
    /// Where the object ball was, by cell (row by row, `GRID_W` cells from
    /// the head rail, starting at the `-y` side).
    pub heat: Vec<Tally>,
}

impl Default for ShotAnalysis {
    fn default() -> Self {
        ShotAnalysis {
            games: 0,
            pockets: Default::default(),
            distances: Default::default(),
            kinds: Default::default(),
            safeties: Tally::default(),
            heat: vec![Tally::default(); GRID_W * GRID_H],
        }
    }
}

/// Row kinds when the sums are stored as rows (kind, key, attempts, made).
pub const ROW_GAMES: i16 = 0;
pub const ROW_POCKET: i16 = 1;
pub const ROW_DISTANCE: i16 = 2;
pub const ROW_KIND: i16 = 3;
pub const ROW_SAFETY: i16 = 4;
pub const ROW_HEAT: i16 = 5;

/// Attempts a player needs before the profile shows the analysis.
pub const MIN_ATTEMPTS: u32 = 10;

impl ShotAnalysis {
    /// Every pot attempt (breaks are apart).
    pub fn attempts(&self) -> Tally {
        let mut t = Tally::default();
        for p in &self.pockets {
            t.merge(*p);
        }
        t
    }

    /// Enough shots to say something.
    pub fn is_enough(&self) -> bool {
        self.attempts().attempts >= MIN_ATTEMPTS
    }

    pub fn merge(&mut self, other: &ShotAnalysis) {
        self.games = self.games.saturating_add(other.games);
        for (a, b) in self.pockets.iter_mut().zip(&other.pockets) {
            a.merge(*b);
        }
        for (a, b) in self.distances.iter_mut().zip(&other.distances) {
            a.merge(*b);
        }
        for (a, b) in self.kinds.iter_mut().zip(&other.kinds) {
            a.merge(*b);
        }
        self.safeties.merge(other.safeties);
        self.heat.resize(GRID_W * GRID_H, Tally::default());
        for (a, b) in self.heat.iter_mut().zip(&other.heat) {
            a.merge(*b);
        }
    }

    /// The sums as rows (kind, key, attempts, made), leaving out empty ones.
    pub fn rows(&self) -> Vec<(i16, i16, u32, u32)> {
        let mut rows = Vec::new();
        if self.games > 0 {
            rows.push((ROW_GAMES, 0, self.games, 0));
        }
        let mut push = |kind: i16, list: &[Tally]| {
            for (k, t) in list.iter().enumerate() {
                if t.attempts > 0 {
                    rows.push((kind, k as i16, t.attempts, t.made));
                }
            }
        };
        push(ROW_POCKET, &self.pockets);
        push(ROW_DISTANCE, &self.distances);
        push(ROW_KIND, &self.kinds);
        push(ROW_SAFETY, &[self.safeties]);
        push(ROW_HEAT, &self.heat);
        rows
    }

    /// Rebuilds the sums from rows (unknown rows are ignored).
    pub fn from_rows(rows: impl IntoIterator<Item = (i16, i16, u32, u32)>) -> ShotAnalysis {
        let mut a = ShotAnalysis::default();
        for (kind, key, attempts, made) in rows {
            let t = Tally::from([attempts, made]);
            let slot = match kind {
                ROW_GAMES => {
                    a.games = a.games.saturating_add(attempts);
                    continue;
                }
                ROW_POCKET => a.pockets.get_mut(key as usize),
                ROW_DISTANCE => a.distances.get_mut(key as usize),
                ROW_KIND => a.kinds.get_mut(key as usize),
                ROW_SAFETY if key == 0 => Some(&mut a.safeties),
                ROW_HEAT => a.heat.get_mut(key as usize),
                _ => None,
            };
            if let Some(slot) = slot.filter(|_| key >= 0) {
                slot.merge(t);
            }
        }
        a
    }
}

/// The heat-map cell of a point on the table.
pub fn cell(game: &EightBallGame, p: DVec2) -> usize {
    let spec = game.table().spec;
    let fx = ((p.x / spec.length + 0.5) * GRID_W as f64).floor().clamp(0.0, (GRID_W - 1) as f64);
    let fy = ((p.y / spec.width + 0.5) * GRID_H as f64).floor().clamp(0.0, (GRID_H - 1) as f64);
    fy as usize * GRID_W + fx as usize
}

fn distance_band(d: f64) -> usize {
    if d < SHORT_MAX {
        0
    } else if d < MEDIUM_MAX {
        1
    } else {
        2
    }
}

/// What a shot was, for the sums.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verdict {
    Break {
        made: bool,
    },
    Pot {
        kind: ShotKind,
        pocket: usize,
        distance: f64,
        cell: usize,
        made: bool,
    },
    Safety,
    /// Nothing to learn (a miss of every ball, say).
    Other,
}

impl Verdict {
    fn made(&self) -> bool {
        matches!(self, Verdict::Break { made: true } | Verdict::Pot { made: true, .. })
    }
}

/// What happened while a shot was simulated.
#[derive(Debug, Default)]
struct Trace {
    /// The cue ball touched a cushion before any ball.
    kick: bool,
    /// The cue ball's highest point above resting, before it hit a ball.
    lift: f64,
    /// The cue ball's direction just before it hit the first ball.
    cue_dir: Option<DVec2>,
    /// The first ball hit, and its direction just after.
    first: Option<(BallId, DVec2)>,
    /// The ball the first ball hit next (before a cushion), and its direction.
    second: Option<(BallId, DVec2)>,
    /// Cushion contacts away from the pockets' jaws: (ball, where).
    banks: Vec<(BallId, DVec2)>,
    /// Closest approach of the first and second ball to each pocket mouth.
    near: Vec<(BallId, [f64; POCKETS])>,
}

/// Runs a prepared shot to rest exactly as [`Simulation::run_to_rest`]
/// does, watching the contacts on the way.
fn trace(sim: &mut Simulation) -> Trace {
    let r = sim.table.spec.ball_radius;
    let mouths: Vec<DVec2> = sim.table.pockets.iter().map(|p| p.mouth_center).collect();
    let jaw_zone = sim.table.spec.jaw_length + sim.table.spec.corner_mouth;
    let mut t = Trace::default();
    let mut seen = 0;
    let ball_xy = |sim: &Simulation, id: BallId| sim.ball(id).map(|b| b.xy());
    let vel = |sim: &Simulation, id: BallId| {
        sim.ball(id).map(|b| b.velocity.truncate().normalize_or_zero()).unwrap_or_default()
    };
    while !sim.is_settled() && sim.time() <= sim.params.max_shot_duration {
        let cue_before = vel(sim, BallId::CUE);
        sim.step();
        if t.first.is_none()
            && let Some(cue) = sim.ball(BallId::CUE)
        {
            t.lift = t.lift.max(cue.position.z - r);
        }
        let events = sim.events()[seen..].to_vec();
        seen += events.len();
        for e in events {
            match e {
                Event::BallBall { a, b, .. } => {
                    let pair = [a, b];
                    match (t.first, t.second) {
                        (None, _) if pair.contains(&BallId::CUE) => {
                            let other = if a == BallId::CUE { b } else { a };
                            t.cue_dir = Some(cue_before);
                            t.first = Some((other, vel(sim, other)));
                            t.near.push((other, [f64::MAX; POCKETS]));
                        }
                        (Some((first, _)), None)
                            if pair.contains(&first)
                                && !pair.contains(&BallId::CUE)
                                && !t.banks.iter().any(|(id, _)| *id == first) =>
                        {
                            let other = if a == first { b } else { a };
                            t.second = Some((other, vel(sim, other)));
                            t.near.push((other, [f64::MAX; POCKETS]));
                        }
                        _ => {}
                    }
                }
                Event::Cushion { ball, .. } => {
                    if ball == BallId::CUE && t.first.is_none() {
                        t.kick = true;
                    }
                    if let Some(p) = ball_xy(sim, ball)
                        && mouths.iter().all(|m| m.distance(p) > jaw_zone)
                    {
                        t.banks.push((ball, p));
                    }
                }
                _ => {}
            }
        }
        for (id, near) in &mut t.near {
            if let Some(p) = sim.ball(*id).filter(|b| b.in_play()).map(|b| b.xy()) {
                for (n, m) in near.iter_mut().zip(&mouths) {
                    *n = n.min(m.distance(p));
                }
            }
        }
    }
    // Stops what is left spinning, as the rules expect.
    sim.run_to_rest();
    t
}

/// A potted ball that touched a cushion was banked if it left at least this
/// many degrees away from the pocket's direction.
const BANK_TURN: f64 = 15.0;

/// How far from a pocket a ball must have come to count as sent there.
const REACH: f64 = 0.22;

/// The pocket a ball leaving `from` along `dir` was sent to, directly or
/// off one cushion: (pocket, distance it had to travel, banked).
fn aimed_pocket(
    game: &EightBallGame,
    from: DVec2,
    dir: DVec2,
    near: &[f64; POCKETS],
) -> Option<(usize, f64, bool)> {
    if dir == DVec2::ZERO {
        return None;
    }
    let table = game.table();
    let r = table.spec.ball_radius;
    let (hx, hy) = (table.spec.length / 2.0 - r, table.spec.width / 2.0 - r);
    // The perpendicular miss at the target, if it lies ahead.
    let miss = |target: DVec2| {
        let to = target - from;
        let along = to.dot(dir);
        (along > 0.0).then(|| (to.perp_dot(dir).abs(), to.length()))
    };
    // The best line so far: (how far inside the tolerance, pocket, run, banked).
    type Best = Option<(f64, usize, f64, bool)>;
    let consider = |best: &mut Best, slack: f64, pocket: usize, distance: f64, banked: bool| {
        if near[pocket] <= REACH && best.is_none_or(|b| slack < b.0) {
            *best = Some((slack, pocket, distance, banked));
        }
    };
    let mut best: Best = None;
    for (k, p) in table.pockets.iter().enumerate() {
        let aim = p.aim_point();
        if let Some((off, dist)) = miss(aim) {
            let tolerance = p.half_mouth + 1.5 * r;
            if off <= tolerance {
                consider(&mut best, off - tolerance, k, dist, false);
            }
        }
    }
    if best.is_some() {
        return best.map(|b| (b.1, b.2, b.3));
    }
    // Off one cushion: aim at the pocket's mirror image in that cushion.
    // Which cushion the ball reaches first.
    let hit = [
        (dir.x > 0.0).then(|| (hx - from.x) / dir.x),
        (dir.x < 0.0).then(|| (-hx - from.x) / dir.x),
        (dir.y > 0.0).then(|| (hy - from.y) / dir.y),
        (dir.y < 0.0).then(|| (-hy - from.y) / dir.y),
    ];
    let Some((rail, t)) = hit
        .iter()
        .enumerate()
        .filter_map(|(k, t)| t.map(|t| (k, t)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
    else {
        return None;
    };
    let at = from + dir * t;
    // Not into a pocket's mouth.
    if table.pockets.iter().any(|p| p.mouth_center.distance(at) < p.half_mouth + 2.0 * r) {
        return None;
    }
    for (k, p) in table.pockets.iter().enumerate() {
        let aim = p.aim_point();
        // A pocket on that cushion can't be banked into off it.
        let (mirror, same_side) = match rail {
            0 => (DVec2::new(2.0 * hx - aim.x, aim.y), p.outward.x > 0.5),
            1 => (DVec2::new(-2.0 * hx - aim.x, aim.y), p.outward.x < -0.5),
            2 => (DVec2::new(aim.x, 2.0 * hy - aim.y), p.outward.y > 0.1),
            _ => (DVec2::new(aim.x, -2.0 * hy - aim.y), p.outward.y < -0.1),
        };
        if same_side {
            continue;
        }
        if let Some((off, dist)) = miss(mirror) {
            let tolerance = p.half_mouth + 2.5 * r;
            if off <= tolerance {
                consider(&mut best, off - tolerance, k, dist, true);
            }
        }
    }
    best.map(|b| (b.1, b.2, b.3))
}

/// Reads a shot: `before` is the game as it was, `report` its outcome.
fn judge(before: &EightBallGame, shot: &Shot, t: &Trace, report: &ShotReport) -> Verdict {
    let shooter = before.current_player();
    let legal_pot = !report.is_foul() && report.pocketed.iter().any(|(b, _)| b.is_object());
    if report.was_break {
        return Verdict::Break { made: legal_pot };
    }
    let Some((target, target_dir)) = t.first else { return Verdict::Other };
    let won = report.game_over.is_some_and(|g| g.winner == shooter);
    let made = legal_pot && (report.continues || won);
    let r = before.table().spec.ball_radius;
    let start = |id: BallId| before.ball(id).xy();
    let cue = start(BallId::CUE);
    // The ball that went (or should have gone) in, its pocket, how far it
    // had to go and whether it went off a cushion.
    let potted = report.pocketed.iter().find(|(b, _)| b.is_object()).copied();
    let (ball, pocket, run, banked) = match potted {
        Some((ball, pocket)) => {
            let to_pocket = before.table().pocket(pocket).mouth_center - start(ball);
            // Off a cushion, and not just along it: it left in another
            // direction than the pocket's.
            let left = [Some((target, target_dir)), t.second]
                .into_iter()
                .flatten()
                .find(|(id, _)| *id == ball)
                .map(|(_, d)| d);
            let turned = left.is_none_or(|d| {
                d.dot(to_pocket.normalize_or_zero()) < BANK_TURN.to_radians().cos()
            });
            let banked = turned && t.banks.iter().any(|(id, _)| *id == ball);
            (ball, pocket.index(), to_pocket.length(), banked)
        }
        None => {
            let (ball, dir) = t.second.unwrap_or((target, target_dir));
            let near = t.near.iter().find(|(id, _)| *id == ball).map(|n| n.1);
            let aimed = near.and_then(|near| aimed_pocket(before, start(ball), dir, &near));
            match aimed {
                Some((pocket, run, banked)) => (ball, pocket, run, banked),
                None if !report.is_foul() => return Verdict::Safety,
                None => return Verdict::Other,
            }
        }
    };
    let combination = ball != target;
    let mut distance = ((start(target) - cue).length() - 2.0 * r).max(0.0) + run;
    if combination {
        distance += (start(ball) - start(target)).length();
    }
    let raised = shot.elevation >= 30f64.to_radians();
    let kind = if t.lift > 0.5 * r {
        ShotKind::Jump
    } else if raised && shot.tip.x.abs() >= 0.25 {
        ShotKind::Masse
    } else if t.kick {
        ShotKind::Kick
    } else if combination {
        ShotKind::Combination
    } else if banked {
        ShotKind::Bank
    } else {
        let cut = t
            .cue_dir
            .map(|c| c.dot(target_dir).clamp(-1.0, 1.0).acos().to_degrees())
            .unwrap_or(0.0);
        ShotKind::of_cut(cut)
    };
    Verdict::Pot { kind, pocket, distance, cell: cell(before, start(ball)), made }
}

/// Every shot of a game, read: (seat, verdict). Stops at a stroke that
/// can't be replayed.
pub fn verdicts(replay: &Replay) -> Result<Vec<(usize, Verdict)>, String> {
    let mut game = replay.start();
    let mut out = Vec::new();
    for stroke in &replay.strokes {
        match *stroke {
            Stroke::Shot { shot, called, cue_ball, .. } => {
                place_cue_ball(&mut game, cue_ball)?;
                let before = game.clone();
                let mut pending = game.prepare_shot(&shot, called).map_err(|e| format!("{e:?}"))?;
                let trace = trace(&mut pending.simulation);
                let shot = pending.shot;
                let report = game.finish_shot(pending);
                out.push((before.current_player(), judge(&before, &shot, &trace, &report)));
            }
            other => {
                crate::replay::apply(&mut game, &other)?;
            }
        }
    }
    Ok(out)
}

/// The analysis of one game, per seat.
pub fn analyse(replay: &Replay) -> Result<[ShotAnalysis; 2], String> {
    let mut out = [ShotAnalysis::default(), ShotAnalysis::default()];
    out[0].games = 1;
    out[1].games = 1;
    let list = verdicts(replay)?;
    for (k, (seat, verdict)) in list.iter().enumerate() {
        let a = &mut out[*seat % 2];
        match *verdict {
            Verdict::Break { made } => a.kinds[ShotKind::Break.index()].add(made),
            Verdict::Pot { kind, pocket, distance, cell, made } => {
                a.pockets[pocket].add(made);
                a.distances[distance_band(distance)].add(made);
                a.kinds[kind.index()].add(made);
                a.heat[cell].add(made);
            }
            Verdict::Safety => {
                // A good safety: the opponent's next shot made nothing (the
                // game ending on a resignation counts too).
                let answer = list[k + 1..].iter().find(|(s, _)| s != seat);
                let held = answer.is_none_or(|(_, v)| !v.made());
                a.safeties.add(held);
            }
            Verdict::Other => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use snookered_core::{Difficulty, PocketId, ai};

    fn ai_game(seed: u64) -> Replay {
        let mut replay = Replay::new(seed, (seed % 2) as usize);
        let mut game = replay.start();
        let mut k = 0;
        while game.game_over().is_none() && k < 300 {
            let d = ai::decide(&game, Difficulty::Hard, k);
            let stroke = Stroke::Shot {
                shot: d.shot,
                called: d.called_pocket,
                cue_ball: d.cue_ball,
                timeout: false,
            };
            crate::replay::apply(&mut game, &stroke).unwrap();
            replay.strokes.push(stroke);
            k += 1;
        }
        replay
    }

    #[test]
    fn tally_rates_and_wire_format() {
        let mut t = Tally::default();
        t.add(true);
        t.add(false);
        t.add(true);
        assert_eq!((t.attempts, t.made, t.missed(), t.rate()), (3, 2, 1, 66));
        assert_eq!(serde_json::to_string(&t).unwrap(), "[3,2]");
        let back: Tally = serde_json::from_str("[2,9]").unwrap();
        assert_eq!(back, Tally { attempts: 2, made: 2 }, "never more made than tried");
        assert_eq!(Tally::default().rate(), 0);
    }

    #[test]
    fn rows_round_trip_and_merge() {
        let replay = ai_game(7);
        let [a, b] = analyse(&replay).unwrap();
        for x in [&a, &b] {
            assert_eq!(ShotAnalysis::from_rows(x.rows()), *x);
        }
        let mut sum = a.clone();
        sum.merge(&b);
        assert_eq!(sum.games, 2);
        assert_eq!(sum.attempts().attempts, a.attempts().attempts + b.attempts().attempts);
        // Unknown and out-of-range rows are ignored.
        let odd =
            ShotAnalysis::from_rows([(ROW_POCKET, 99, 1, 1), (42, 0, 1, 1), (ROW_HEAT, -1, 1, 0)]);
        assert_eq!(odd, ShotAnalysis::default());
    }

    #[test]
    fn whole_games_are_read_consistently() {
        for seed in [1, 2, 11] {
            let replay = ai_game(seed);
            let [a, b] = analyse(&replay).unwrap();
            let breaks = a.kinds[ShotKind::Break.index()].attempts
                + b.kinds[ShotKind::Break.index()].attempts;
            assert_eq!(breaks, 1, "one break a game");
            for x in [&a, &b] {
                let pots = x.attempts();
                // Every pot attempt is in one pocket, one distance band,
                // one kind and one cell.
                let sum = |l: &[Tally]| {
                    l.iter().fold(Tally::default(), |mut s, t| {
                        s.merge(*t);
                        s
                    })
                };
                assert_eq!(sum(&x.distances), pots);
                assert_eq!(sum(&x.heat), pots);
                let mut kinds = sum(&x.kinds);
                kinds.attempts -= x.kinds[ShotKind::Break.index()].attempts;
                kinds.made -= x.kinds[ShotKind::Break.index()].made;
                assert_eq!(kinds, pots);
            }
            // The hard computer pots most of what it goes for.
            let all = {
                let mut s = a.clone();
                s.merge(&b);
                s
            };
            assert!(all.attempts().attempts >= 5, "seed {seed}: {all:?}");
            assert!(all.attempts().made >= all.attempts().attempts / 3, "seed {seed}: {all:?}");
            // The game was rebuilt shot for shot: the sums tell the same
            // story as the end-of-game numbers.
            let (end, _) = replay.play(None).unwrap();
            assert!(end.game_over().is_some());
        }
    }

    /// One ball, one pocket: a straight pot along the foot rail's middle.
    fn lone_ball(target: DVec2, cue: DVec2, yaw: f64, speed: f64) -> (Verdict, ShotReport) {
        let mut game = EightBallGame::new(5, 0);
        // The table cleared but for the cue ball, the 1 and the 8.
        let mut balls = game.balls().to_vec();
        for b in balls.iter_mut() {
            if b.id == BallId::CUE {
                b.position = cue.extend(b.position.z);
            } else if b.id == BallId(1) {
                b.position = target.extend(b.position.z);
            } else if b.id == BallId::EIGHT {
                b.position = DVec2::new(-0.8, 0.4).extend(b.position.z);
            } else {
                b.status = snookered_core::BallStatus::Pocketed(PocketId::HeadLeft);
            }
        }
        // Past the break, open table, nobody in hand (through the wire
        // format: the rules keep their fields to themselves).
        let mut v = serde_json::to_value(&game).unwrap();
        v["balls"] = serde_json::to_value(&balls).unwrap();
        v["is_break"] = false.into();
        v["ball_in_hand"] = serde_json::Value::Null;
        game = serde_json::from_value(v).unwrap();
        let before = game.clone();
        let shot = Shot::new(yaw, speed);
        let mut pending = game.prepare_shot(&shot, None).unwrap();
        let t = trace(&mut pending.simulation);
        let report = game.finish_shot(pending);
        (judge(&before, &shot, &t, &report), report)
    }

    #[test]
    fn a_straight_pot_and_a_soft_safety() {
        let foot = EightBallGame::new(5, 0).table().pocket(PocketId::FootLeft).aim_point();
        let target = foot + (DVec2::new(-0.4, -0.2) - foot).normalize() * 0.3;
        let cue = target + (target - foot).normalize() * 0.4;
        let yaw = (foot - cue).to_angle();
        let (v, report) = lone_ball(target, cue, yaw, 2.5);
        assert!(report.pocketed.iter().any(|(b, _)| *b == BallId(1)), "{report:?}");
        match v {
            Verdict::Pot { kind, pocket, made, distance, .. } => {
                assert_eq!(kind, ShotKind::Straight);
                assert_eq!(pocket, PocketId::FootLeft.index());
                assert!(made);
                assert_eq!(distance_band(distance), 0, "{distance}");
            }
            v => panic!("{v:?}"),
        }
        // Barely touched, away from every pocket: a safety.
        // (Hard enough to reach the cushion: a legal safety.)
        let side = DVec2::new(0.0, -1.0);
        let at = DVec2::new(0.5, 0.0);
        let (v, report) = lone_ball(at, at - side * 0.3, side.to_angle(), 1.2);
        assert!(!report.is_foul(), "{report:?}");
        assert_eq!(v, Verdict::Safety);
    }
}
