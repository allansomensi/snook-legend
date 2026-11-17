//! Computer opponent.
//!
//! The AI enumerates candidate shots with the "ghost ball" method, simulates each
//! one with the real physics engine and scores the result with the real rules.
//!
//! A computer player ([`AiPlayer`]) is a [`Skill`], how well it plays (how
//! accurately it strikes, how widely it searches, whether it plans the cue
//! ball's position and plays safeties), and a [`Style`], how it likes to play
//! (power or touch, spin, bank shots, how soon it settles for a safety). The
//! three [`Difficulty`] levels are skills with the neutral style; the named
//! opponents of [`crate::personalities`] mix the two.
//!
//! Decisions are reproducible: the same game, player and seed give the same
//! shot, bit for bit, on every platform. The server relies on it to check
//! games won against the computer, so this module, like the physics, uses
//! only [`crate::math`] for anything beyond the basic operations.

use glam::DVec2;
use serde::{Deserialize, Serialize};

use crate::ball::BallId;
use crate::math;
use crate::physics::Shot;
use crate::rng::Rng;
use crate::rules::EightBallGame;
use crate::table::PocketId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

impl Difficulty {
    pub const ALL: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard];

    pub fn name(self) -> &'static str {
        match self {
            Difficulty::Easy => "Easy",
            Difficulty::Medium => "Medium",
            Difficulty::Hard => "Hard",
        }
    }

    /// How well a computer player of this level plays.
    pub const fn skill(self) -> Skill {
        match self {
            Difficulty::Easy => Skill {
                aim_noise_deg: 1.6,
                speed_noise: 0.2,
                break_speed: 5.5,
                search: Search::Narrow,
                position_weight: 0.0,
                plays_safe: false,
                banks: false,
            },
            Difficulty::Medium => Skill {
                aim_noise_deg: 0.75,
                speed_noise: 0.1,
                break_speed: 6.3,
                search: Search::Normal,
                position_weight: 180.0,
                plays_safe: true,
                banks: false,
            },
            Difficulty::Hard => Skill {
                aim_noise_deg: 0.35,
                speed_noise: 0.05,
                break_speed: 7.2,
                search: Search::Wide,
                position_weight: 450.0,
                plays_safe: true,
                banks: true,
            },
        }
    }

    /// The computer player of this level: its skill with the neutral style.
    pub const fn player(self) -> AiPlayer {
        AiPlayer { skill: self.skill(), style: Style::NEUTRAL }
    }
}

/// How widely the computer looks for its shot: how many speeds, spins and
/// aim points it tries, how carefully it checks that a shot still works
/// when struck a little off, and how often it settles for a lesser one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Search {
    Narrow,
    Normal,
    Wide,
}

/// How well a computer player plays.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Skill {
    /// Execution error (standard deviation) in aim, degrees.
    pub aim_noise_deg: f64,
    /// Relative execution error in speed.
    pub speed_noise: f64,
    /// Cue speed of the break (m/s).
    pub break_speed: f64,
    pub search: Search,
    /// How much the cue ball's position for the next shot counts (0: not at all).
    pub position_weight: f64,
    /// Values leaving the opponent in trouble when nothing is on.
    pub plays_safe: bool,
    /// Sees one-cushion bank shots.
    pub banks: bool,
}

impl Skill {
    /// Every number finite and within its range.
    pub fn clamped(self) -> Skill {
        let f = |v: f64, lo: f64, hi: f64| if v.is_finite() { v.clamp(lo, hi) } else { hi };
        Skill {
            aim_noise_deg: f(self.aim_noise_deg, 0.0, 5.0),
            speed_noise: f(self.speed_noise, 0.0, 0.5),
            break_speed: f(self.break_speed, 3.0, 7.5),
            position_weight: f(self.position_weight, 0.0, 1000.0),
            ..self
        }
    }
}

/// How a computer player likes to play, whatever its skill. The neutral
/// style ([`Style::NEUTRAL`]) leaves the skill's own choices as they are.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Style {
    /// Below this score the best pot is "not likely", and safeties are
    /// weighed against it (a sure pot scores about 1000, a miss about 0;
    /// neutral 700). Low: goes for everything; high: plays safe at the
    /// first doubt.
    pub safety_threshold: f64,
    /// How much a safety that leaves nothing is worth (neutral 1).
    pub safety_value: f64,
    /// Bank shots: below -0.25 never, above 0.25 always looked for and
    /// preferred (more, the closer to 1), otherwise as the skill allows.
    pub banks: f64,
    /// Power (up to 1) or touch (down to -1): scales the speeds tried and
    /// leans towards the harder (softer) ones. Neutral 0.
    pub power: f64,
    /// Spin: 0 only centre ball, 1 the skill's own spins, up to 1.5 bigger
    /// spins with side English as well.
    pub spin: f64,
    /// Multiplies the skill's care for position (neutral 1).
    pub position: f64,
    /// Cue elevation on ordinary shots (radians, 0 to 0.15).
    pub elevation: f64,
}

impl Style {
    pub const NEUTRAL: Style = Style {
        safety_threshold: 700.0,
        safety_value: 1.0,
        banks: 0.0,
        power: 0.0,
        spin: 1.0,
        position: 1.0,
        elevation: 0.0,
    };

    /// Every number finite and within its range (a style can come from the
    /// network: the ghost of a real player).
    pub fn clamped(self) -> Style {
        let n = Style::NEUTRAL;
        let f = |v: f64, lo: f64, hi: f64, neutral: f64| {
            if v.is_finite() { v.clamp(lo, hi) } else { neutral }
        };
        Style {
            safety_threshold: f(self.safety_threshold, 150.0, 1100.0, n.safety_threshold),
            safety_value: f(self.safety_value, 0.0, 2.5, n.safety_value),
            banks: f(self.banks, -1.0, 1.0, n.banks),
            power: f(self.power, -1.0, 1.0, n.power),
            spin: f(self.spin, 0.0, 1.5, n.spin),
            position: f(self.position, 0.0, 2.5, n.position),
            elevation: f(self.elevation, 0.0, 0.15, n.elevation),
        }
    }
}

impl Default for Style {
    fn default() -> Self {
        Style::NEUTRAL
    }
}

/// A computer player: how well it plays and how it likes to play.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AiPlayer {
    pub skill: Skill,
    pub style: Style,
}

impl From<Difficulty> for AiPlayer {
    fn from(d: Difficulty) -> Self {
        d.player()
    }
}

/// What the AI wants to do on its turn.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AiDecision {
    /// Where to put the cue ball first, when it is in hand.
    pub cue_ball: Option<DVec2>,
    pub shot: Shot,
    pub called_pocket: Option<PocketId>,
}

/// Everything the search needs, worked out from an [`AiPlayer`].
struct Profile {
    aim_noise_deg: f64,
    speed_noise: f64,
    break_speed: f64,
    /// Speeds tried for every candidate in the first pass.
    coarse_speeds: Vec<f64>,
    /// Speeds and tips tried for the most promising candidates.
    refine_speeds: Vec<f64>,
    tips: Vec<(f64, f64)>,
    /// Candidates kept for refinement.
    keep: usize,
    /// Aim points across the pocket mouth (fractions of the half-mouth).
    aim_spread: &'static [f64],
    position_weight: f64,
    plays_safe: bool,
    banks: bool,
    /// Extra score for a bank shot that drops.
    bank_bonus: f64,
    /// Simulated imperfect executions used to judge how safe a shot is.
    robustness_samples: usize,
    choose_among_top: usize,
    safety_threshold: f64,
    safety_value: f64,
    /// Extra score per m/s above (below, when negative) a medium pace.
    power_bias: f64,
    elevation: f64,
}

/// The fastest the computer strikes an ordinary shot (m/s).
const MAX_PLAYED_SPEED: f64 = 6.5;

/// How widely each [`Search`] looks.
struct Breadth {
    coarse: &'static [f64],
    refine: &'static [f64],
    tips: &'static [(f64, f64)],
    keep: usize,
    aim_spread: &'static [f64],
    robustness: usize,
    top: usize,
}

fn breadth(search: Search) -> Breadth {
    match search {
        Search::Narrow => Breadth {
            coarse: &[2.0],
            refine: &[1.6, 2.6],
            tips: &[(0.0, 0.0)],
            keep: 2,
            aim_spread: &[0.0],
            robustness: 0,
            top: 4,
        },
        Search::Normal => Breadth {
            coarse: &[1.6, 2.8],
            refine: &[1.2, 1.7, 2.4, 3.3, 4.4],
            tips: &[(0.0, 0.0), (0.0, 0.5), (0.0, -0.5)],
            keep: 4,
            aim_spread: &[-0.3, 0.0, 0.3],
            robustness: 1,
            top: 2,
        },
        Search::Wide => Breadth {
            coarse: &[1.4, 2.4, 3.8],
            refine: &[0.9, 1.25, 1.7, 2.2, 2.9, 3.8, 5.0],
            tips: &[
                (0.0, 0.0),
                (0.0, 0.35),
                (0.0, 0.7),
                (0.0, -0.4),
                (0.0, -0.75),
                (0.45, 0.2),
                (-0.45, 0.2),
                (0.45, -0.4),
                (-0.45, -0.4),
            ],
            keep: 6,
            aim_spread: &[-0.35, 0.0, 0.35],
            robustness: 3,
            top: 1,
        },
    }
}

fn profile(player: &AiPlayer) -> Profile {
    let skill = player.skill.clamped();
    let style = player.style.clamped();
    let b = breadth(skill.search);
    // Power scales every speed (the neutral style leaves them exactly as
    // they are).
    let scale = 1.0 + 0.3 * style.power;
    let speeds = |list: &[f64]| -> Vec<f64> {
        list.iter().map(|s| (s * scale).clamp(0.4, MAX_PLAYED_SPEED)).collect()
    };
    // Spin: none, the skill's own, or bigger (with side English as well).
    let tips: Vec<(f64, f64)> = if style.spin < 0.05 {
        vec![(0.0, 0.0)]
    } else {
        let mut tips: Vec<(f64, f64)> = b
            .tips
            .iter()
            .map(|&(x, y)| {
                let (x, y) = (x * style.spin, y * style.spin);
                let len = (x * x + y * y).sqrt();
                if len > 0.95 { (x * 0.95 / len, y * 0.95 / len) } else { (x, y) }
            })
            .collect();
        if style.spin > 1.05 && b.tips.len() > 1 {
            let side = 0.5 * style.spin;
            tips.extend([(side, 0.0), (-side, 0.0)]);
        }
        tips
    };
    let banks = if style.banks < -0.25 {
        false
    } else if style.banks > 0.25 {
        true
    } else {
        skill.banks
    };
    Profile {
        aim_noise_deg: skill.aim_noise_deg,
        speed_noise: skill.speed_noise,
        break_speed: skill.break_speed * (1.0 + 0.1 * style.power),
        coarse_speeds: speeds(b.coarse),
        refine_speeds: speeds(b.refine),
        tips,
        keep: b.keep,
        aim_spread: b.aim_spread,
        position_weight: skill.position_weight * style.position,
        plays_safe: skill.plays_safe,
        banks,
        bank_bonus: 300.0 * style.banks.max(0.0),
        robustness_samples: b.robustness,
        choose_among_top: b.top,
        safety_threshold: style.safety_threshold,
        safety_value: style.safety_value,
        power_bias: 45.0 * style.power,
        elevation: style.elevation,
    }
}

/// A shot to try: aim direction plus what it is meant to do.
#[derive(Clone, Copy)]
struct Candidate {
    yaw: f64,
    called: Option<PocketId>,
    /// Off a cushion first.
    bank: bool,
}

#[derive(Clone, Copy)]
struct Scored {
    score: f64,
    shot: Shot,
    called: Option<PocketId>,
    bank: bool,
    /// Played to leave the opponent nothing, not to pot.
    safety: bool,
}

/// Chooses the next action of a computer player of this level. `seed`
/// makes the decision reproducible.
pub fn decide(game: &EightBallGame, difficulty: Difficulty, seed: u64) -> AiDecision {
    decide_with(game, &difficulty.player(), seed)
}

/// Chooses the next action of `player`. The same game, player and seed
/// always give the same decision, on every platform.
pub fn decide_with(game: &EightBallGame, player: &AiPlayer, seed: u64) -> AiDecision {
    let prof = profile(player);
    let mut rng = Rng::new(seed);
    let mut g = game.clone();
    let me = g.current_player();

    let mut cue_ball = None;
    if g.ball_in_hand().is_some() {
        let p = choose_cue_ball_position(&g, &prof, &mut rng);
        if g.place_cue_ball(p) {
            cue_ball = Some(p);
        }
    }

    if g.is_break() {
        let cue = g.ball(BallId::CUE).xy();
        let d = g.table().foot_spot() - cue;
        // A touch of follow keeps the cue ball near the middle of the table.
        let shot =
            Shot::new(math::atan2(d.y, d.x), prof.break_speed).with_tip(DVec2::new(0.0, 0.15));
        return AiDecision { cue_ball, shot: perturb(shot, &prof, &mut rng), called_pocket: None };
    }

    // 1. Every geometrically sensible pot, tried quickly.
    let mut candidates = pocket_candidates(&g, &prof);
    if prof.banks {
        candidates.extend(bank_candidates(&g));
    }
    let mut coarse: Vec<(f64, Candidate)> = Vec::new();
    for c in &candidates {
        let mut best = f64::MIN;
        for &speed in &prof.coarse_speeds {
            let shot = Shot::new(c.yaw, speed).with_elevation(prof.elevation);
            best = best.max(evaluate(&g, &shot, c.called, c.bank, false, me, &prof));
        }
        coarse.push((best, *c));
    }
    coarse.sort_by(|a, b| b.0.total_cmp(&a.0));
    coarse.truncate(prof.keep);

    // 2. The best ones with every speed and spin, for position.
    let mut options: Vec<Scored> = Vec::new();
    for &(_, c) in coarse.iter().filter(|(s, _)| *s > 0.0) {
        for &speed in &prof.refine_speeds {
            for &(tx, ty) in &prof.tips {
                let shot = Shot::new(c.yaw, speed)
                    .with_tip(DVec2::new(tx, ty))
                    .with_elevation(prof.elevation);
                let score = evaluate(&g, &shot, c.called, c.bank, false, me, &prof);
                options.push(Scored { score, shot, called: c.called, bank: c.bank, safety: false });
            }
        }
    }
    options.sort_by(|a, b| b.score.total_cmp(&a.score));

    // 3. Prefer shots that still work when struck a little imperfectly.
    // On the 8 every level checks: a slip there sinks it in the wrong
    // pocket (or scratches) and loses the game.
    let samples = if g.needs_called_pocket() {
        prof.robustness_samples.max(2)
    } else {
        prof.robustness_samples
    };
    if samples > 0 {
        robust_rescore(&g, &mut options, me, &prof, samples, &mut rng);
    }

    // 4. When nothing is likely enough to drop (for this player's taste),
    // weigh safeties too (or at least a legal hit).
    let best = options.first().map(|o| o.score).unwrap_or(f64::MIN);
    if best < prof.safety_threshold {
        let mut safeties = safety_options(&g, me, &prof);
        if samples > 0 {
            robust_rescore(&g, &mut safeties, me, &prof, samples, &mut rng);
        }
        options.extend(safeties);
        options.sort_by(|a, b| b.score.total_cmp(&a.score));
    }
    if options.is_empty() {
        let called = g.needs_called_pocket().then_some(PocketId::FootLeft);
        return AiDecision { cue_ball, shot: Shot::new(0.0, 2.0), called_pocket: called };
    }

    // Variety among shots about as good as the best, never a much worse one
    // (the runner-up is often the same pot at a speed that scratches, or
    // that sends the 8 into another pocket).
    let best = options[0].score;
    let close = options
        .iter()
        .take(prof.choose_among_top)
        .take_while(|o| o.score >= best - CHOICE_MARGIN)
        .count();
    let pick = rng.below(close.max(1));
    let o = options[pick];
    AiDecision { cue_ball, shot: perturb(o.shot, &prof, &mut rng), called_pocket: o.called }
}

/// How much worse than the best a shot may score and still be picked for
/// variety.
const CHOICE_MARGIN: f64 = 150.0;

/// Replaces the scores of the leading options with their average over
/// `samples` slightly perturbed executions.
fn robust_rescore(
    g: &EightBallGame,
    options: &mut [Scored],
    me: usize,
    prof: &Profile,
    samples: usize,
    rng: &mut Rng,
) {
    let n = options.len().min(5);
    let aim = (prof.aim_noise_deg * 2.0).max(0.15).to_radians();
    for o in options.iter_mut().take(n) {
        let mut total = o.score;
        for k in 0..samples {
            let mut s = o.shot;
            let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
            s.yaw += sign * aim * (0.6 + 0.4 * rng.next_f64());
            s.speed *= 1.0 + rng.gaussian() * prof.speed_noise * 1.5;
            total += evaluate(g, &s, o.called, o.bank, o.safety, me, prof);
        }
        o.score = total / (samples + 1) as f64;
    }
    options[..n].sort_by(|a, b| b.score.total_cmp(&a.score));
}

fn perturb(mut shot: Shot, prof: &Profile, rng: &mut Rng) -> Shot {
    shot.yaw += rng.gaussian() * prof.aim_noise_deg.to_radians();
    shot.speed *= 1.0 + rng.gaussian() * prof.speed_noise;
    shot
}

fn evaluate(
    g: &EightBallGame,
    shot: &Shot,
    called: Option<PocketId>,
    bank: bool,
    safety: bool,
    me: usize,
    prof: &Profile,
) -> f64 {
    let mut trial = g.clone();
    let Ok(report) = trial.play_shot(shot, called) else {
        return f64::MIN / 2.0;
    };
    if let Some(over) = report.game_over {
        return if over.winner == me { 10_000.0 } else { -10_000.0 };
    }
    let mut score = 0.0;
    if report.is_foul() {
        if g.ruleset() >= 4 {
            // The house rules: every ball a foul gives away is a ball the
            // opponent no longer has to make, and only a scratch leaves
            // them the cue ball in hand.
            score -= 500.0 + 350.0 * report.penalty.len() as f64;
            score -= if trial.ball_in_hand().is_some() {
                300.0 + 400.0 * opponent_quality(&trial)
            } else {
                200.0 * opponent_quality(&trial)
            };
        } else {
            // Ball in hand for the opponent is about as bad as it gets.
            score -= 800.0 + 400.0 * opponent_quality(&trial);
        }
    }
    if report.continues {
        score += 1_000.0 + prof.position_weight * position_quality(&trial);
        score += 30.0 * report.pocketed.len() as f64;
        if bank {
            score += prof.bank_bonus;
        }
    } else if !report.is_foul() && prof.plays_safe {
        // Leave the opponent nothing easy. A player's taste for safeties
        // rewards the ones played on purpose: a pot that misses and happens
        // to leave little counts no more than for anyone else, or a careful
        // player would rather gamble on pots than play safe.
        let value = if safety { prof.safety_value } else { prof.safety_value.min(1.0) };
        score += 400.0 * value * (1.0 - opponent_quality(&trial));
    }
    if prof.power_bias != 0.0 {
        score += prof.power_bias * (shot.speed - 2.5);
    }
    score
}

/// How easy the best available shot is for the player to move (0 to ~1).
fn position_quality(g: &EightBallGame) -> f64 {
    let cue = g.ball(BallId::CUE).xy();
    let mut best: f64 = 0.0;
    let mut count = 0.0;
    for target in g.legal_targets() {
        let mut ball_best: f64 = 0.0;
        for pocket in PocketId::ALL {
            ball_best = ball_best.max(shot_ease(g, cue, target, pocket));
        }
        if ball_best > 0.05 {
            count += 1.0;
        }
        best = best.max(ball_best);
    }
    // Several options are better than one.
    best * (1.0 + 0.08 * (count - 1.0f64).max(0.0)).min(1.3)
}

/// How good the table is for the player who shoots next (after a miss).
fn opponent_quality(g: &EightBallGame) -> f64 {
    if g.ball_in_hand().is_some() {
        return 1.0;
    }
    position_quality(g)
}

fn shot_ease(g: &EightBallGame, cue: DVec2, target: BallId, pocket: PocketId) -> f64 {
    let r = g.table().spec.ball_radius;
    let t = g.ball(target).xy();
    let p = g.table().pocket(pocket);
    let aim = p.aim_point();
    let to_pocket = aim - t;
    let dir = to_pocket.normalize_or_zero();
    // Balls that approach a pocket at a steep angle to its axis rarely drop.
    let approach = dir.dot(p.outward);
    let limit = if pocket.is_side() { 0.45 } else { 0.25 };
    if approach < limit {
        return 0.0;
    }
    let ghost = t - dir * 2.0 * r;
    let to_ghost = ghost - cue;
    if to_ghost.length() < 1e-6 {
        return 0.0;
    }
    let cos_cut = to_ghost.normalize().dot(dir);
    if cos_cut < 0.26 {
        return 0.0; // cut thinner than ~75 degrees
    }
    if !path_clear(g, cue, ghost, &[BallId::CUE, target])
        || !path_clear(g, t, aim, &[BallId::CUE, target])
    {
        return 0.0;
    }
    let pocket_factor = 0.5 + 0.5 * approach;
    cos_cut * cos_cut * pocket_factor / (1.0 + 0.6 * (to_ghost.length() + to_pocket.length()))
}

fn path_clear(g: &EightBallGame, from: DVec2, to: DVec2, ignore: &[BallId]) -> bool {
    let clearance = 2.0 * g.table().spec.ball_radius - 1e-4;
    let seg = to - from;
    let len_sq = seg.length_squared();
    g.balls().iter().filter(|b| b.in_play() && !ignore.contains(&b.id)).all(|b| {
        let p = b.xy();
        let t = if len_sq > 0.0 { ((p - from).dot(seg) / len_sq).clamp(0.0, 1.0) } else { 0.0 };
        (from + seg * t).distance(p) >= clearance
    })
}

/// Aim directions that send a legal ball towards a pocket.
fn pocket_candidates(g: &EightBallGame, prof: &Profile) -> Vec<Candidate> {
    let cue = g.ball(BallId::CUE).xy();
    let r = g.table().spec.ball_radius;
    let mut out = Vec::new();
    for target in g.legal_targets() {
        for pocket in PocketId::ALL {
            if shot_ease(g, cue, target, pocket) <= 0.0 {
                continue;
            }
            // A called pocket is binding: the 8 can only go there.
            if target == BallId::EIGHT
                && g.called_pocket_of(g.current_player()).is_some_and(|c| c != pocket)
            {
                continue;
            }
            let called = (target == BallId::EIGHT).then_some(pocket);
            let p = g.table().pocket(pocket);
            let t = g.ball(target).xy();
            for &k in prof.aim_spread {
                let aim_at = p.aim_point() + p.outward.perp() * (k * p.half_mouth);
                let dir = (aim_at - t).normalize();
                let aim = t - dir * 2.0 * r - cue;
                out.push(Candidate { yaw: math::atan2(aim.y, aim.x), called, bank: false });
            }
        }
    }
    out
}

/// One-cushion bank shots: the object ball goes off a rail into a pocket.
fn bank_candidates(g: &EightBallGame) -> Vec<Candidate> {
    let cue = g.ball(BallId::CUE).xy();
    let spec = g.table().spec;
    let r = spec.ball_radius;
    let (hl, hw) = (spec.length / 2.0 - r, spec.width / 2.0 - r);
    let mut out = Vec::new();
    for target in g.legal_targets() {
        let t = g.ball(target).xy();
        for pocket in PocketId::ALL {
            let aim = g.table().pocket(pocket).aim_point();
            // Mirror the pocket in each rail the ball would bounce off.
            for mirror in [
                DVec2::new(aim.x, 2.0 * hw - aim.y),
                DVec2::new(aim.x, -2.0 * hw - aim.y),
                DVec2::new(2.0 * hl - aim.x, aim.y),
                DVec2::new(-2.0 * hl - aim.x, aim.y),
            ] {
                let dir = (mirror - t).normalize_or_zero();
                let ghost = t - dir * 2.0 * r;
                let to_ghost = ghost - cue;
                if to_ghost.length() < 1e-6 || to_ghost.normalize().dot(dir) < 0.5 {
                    continue;
                }
                if !path_clear(g, cue, ghost, &[BallId::CUE, target]) {
                    continue;
                }
                if target == BallId::EIGHT
                    && g.called_pocket_of(g.current_player()).is_some_and(|c| c != pocket)
                {
                    continue;
                }
                let called = (target == BallId::EIGHT).then_some(pocket);
                let yaw = math::atan2(to_ghost.y, to_ghost.x);
                out.push(Candidate { yaw, called, bank: true });
            }
        }
    }
    out
}

/// Shots meant to hit a legal ball and leave the opponent in trouble.
fn safety_options(g: &EightBallGame, me: usize, prof: &Profile) -> Vec<Scored> {
    let called = g.needs_called_pocket().then(|| {
        g.called_pocket_of(g.current_player()).unwrap_or_else(|| nearest_pocket(g, BallId::EIGHT))
    });
    let r = g.table().spec.ball_radius;
    let cue = g.ball(BallId::CUE).xy();
    let mut out = Vec::new();
    // On the 8 every level looks for a touch that keeps it out of the other
    // pockets (thin hits, soft speeds): sinking it there loses the game.
    let careful = prof.plays_safe || called.is_some();
    let offsets: &[f64] = if careful {
        &[-1.6, -1.2, -0.7, -0.3, 0.0, 0.3, 0.7, 1.2, 1.6]
    } else {
        &[-0.8, 0.0, 0.8]
    };
    let speeds: &[f64] = if called.is_some() {
        &[0.5, 0.8, 1.3, 2.0, 3.0]
    } else if prof.plays_safe {
        &[0.8, 1.3, 2.0, 3.0]
    } else {
        &[1.4, 2.4]
    };
    for target in g.legal_targets() {
        let t = g.ball(target).xy();
        let dir = (t - cue).normalize_or_zero();
        for &offset in offsets {
            let aim = t + dir.perp() * offset * r - cue;
            for &speed in speeds {
                let shot =
                    Shot::new(math::atan2(aim.y, aim.x), speed).with_elevation(prof.elevation);
                let score = evaluate(g, &shot, called, false, true, me, prof);
                out.push(Scored { score, shot, called, bank: false, safety: true });
            }
        }
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
}

fn nearest_pocket(g: &EightBallGame, ball: BallId) -> PocketId {
    let p = g.ball(ball).xy();
    PocketId::ALL
        .into_iter()
        .min_by(|a, b| {
            let da = g.table().pocket(*a).mouth_center.distance(p);
            let db = g.table().pocket(*b).mouth_center.distance(p);
            da.total_cmp(&db)
        })
        .expect("six pockets")
}

fn choose_cue_ball_position(g: &EightBallGame, prof: &Profile, rng: &mut Rng) -> DVec2 {
    if g.is_break() {
        let head = g.table().head_spot();
        let y = if prof.position_weight > 0.0 {
            0.12 * if rng.below(2) == 0 { 1.0 } else { -1.0 }
        } else {
            0.0
        };
        let p = DVec2::new(head.x, y);
        return if g.can_place_cue_ball(p) { p } else { head };
    }
    let r = g.table().spec.ball_radius;
    let mut best: Vec<(f64, DVec2)> = Vec::new();
    for target in g.legal_targets() {
        let t = g.ball(target).xy();
        for pocket in PocketId::ALL {
            let aim = g.table().pocket(pocket).aim_point();
            let dir = (aim - t).normalize_or_zero();
            let ghost = t - dir * 2.0 * r;
            for dist in [0.12, 0.2, 0.3, 0.45] {
                for angle in [0.0f64, 12.0, -12.0, 25.0, -25.0] {
                    let back = math::from_angle(angle.to_radians()).rotate(dir);
                    let p = ghost - back * dist;
                    if g.can_place_cue_ball(p) {
                        let ease = shot_ease(g, p, target, pocket);
                        if ease > 0.0 {
                            best.push((ease, p));
                        }
                    }
                }
            }
        }
    }
    best.sort_by(|a, b| b.0.total_cmp(&a.0));
    if !best.is_empty() {
        return best[rng.below(prof.choose_among_top.min(best.len()))].1;
    }
    // Fallback: any legal spot near the head of the table.
    let head = g.table().head_spot();
    for _ in 0..500 {
        let p = head + DVec2::new(rng.next_f64() - 0.5, rng.next_f64() - 0.5) * 0.8;
        if g.can_place_cue_ball(p) {
            return p;
        }
    }
    head
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play_game(a: AiPlayer, b: AiPlayer, seed: u64) -> (Option<usize>, u32) {
        let mut g = EightBallGame::new(seed, 0);
        let players = [a, b];
        for turn in 0..300u64 {
            if g.game_over().is_some() {
                break;
            }
            let d = decide_with(&g, &players[g.current_player()], seed * 1000 + turn);
            if let Some(p) = d.cue_ball {
                assert!(g.place_cue_ball(p));
            }
            g.play_shot(&d.shot, d.called_pocket).expect("AI must produce legal shots");
        }
        (g.game_over().map(|o| o.winner), g.shots_taken())
    }

    #[test]
    fn ai_finishes_games() {
        for seed in 1..=3 {
            let medium = Difficulty::Medium.player();
            let (winner, shots) = play_game(medium, medium, seed);
            assert!(winner.is_some(), "seed {seed}: no winner after {shots} shots");
        }
    }

    #[test]
    fn hard_beats_easy_most_of_the_time() {
        let (hard, easy) = (Difficulty::Hard.player(), Difficulty::Easy.player());
        let mut hard_wins = 0;
        let games = 6;
        for seed in 0..games {
            // Alternate who breaks.
            let (winner, _) = if seed % 2 == 0 {
                play_game(hard, easy, seed)
            } else {
                let (w, s) = play_game(easy, hard, seed);
                (w.map(|w| 1 - w), s)
            };
            if winner == Some(0) {
                hard_wins += 1;
            }
        }
        assert!(hard_wins >= 4, "hard won only {hard_wins}/{games}");
    }

    #[test]
    fn decisions_are_reproducible() {
        let g = EightBallGame::new(5, 0);
        let p = Difficulty::Medium.player();
        let mut g2 = g.clone();
        let first = decide_with(&g, &p, 99);
        assert_eq!(first, decide_with(&g, &p, 99));
        // After the break too (a position with pots and safeties).
        if let Some(c) = first.cue_ball {
            g2.place_cue_ball(c);
        }
        g2.play_shot(&first.shot, first.called_pocket).unwrap();
        if g2.game_over().is_none() {
            assert_eq!(decide_with(&g2, &p, 7), decide_with(&g2, &p, 7));
        }
        // The level and its player are the same thing.
        assert_eq!(decide(&g2, Difficulty::Easy, 3), decide_with(&g2, &Difficulty::Easy.into(), 3));
    }

    #[test]
    fn the_neutral_style_changes_nothing() {
        for d in Difficulty::ALL {
            let p = profile(&d.player());
            let b = breadth(d.skill().search);
            assert_eq!(p.coarse_speeds, b.coarse);
            assert_eq!(p.refine_speeds, b.refine);
            assert_eq!(p.tips, b.tips);
            assert_eq!(p.break_speed, d.skill().break_speed);
            assert_eq!(p.banks, d.skill().banks);
            assert_eq!((p.bank_bonus, p.power_bias, p.elevation), (0.0, 0.0, 0.0));
            assert_eq!(p.position_weight, d.skill().position_weight);
        }
    }

    #[test]
    fn styles_are_kept_in_range() {
        let wild = Style {
            safety_threshold: f64::NAN,
            safety_value: 99.0,
            banks: -7.0,
            power: f64::INFINITY,
            spin: -1.0,
            position: 3.0,
            elevation: 1.0,
        }
        .clamped();
        assert_eq!(wild.safety_threshold, Style::NEUTRAL.safety_threshold);
        assert_eq!((wild.safety_value, wild.banks, wild.power), (2.5, -1.0, 0.0));
        assert_eq!((wild.spin, wild.position, wild.elevation), (0.0, 2.5, 0.15));
        // A style that loves spin and power tries bigger spins, harder.
        let p = profile(&AiPlayer {
            skill: Difficulty::Medium.skill(),
            style: Style { spin: 1.4, power: 1.0, ..Style::NEUTRAL },
        });
        assert!(p.tips.len() > 3 && p.tips.iter().all(|(x, y)| (x * x + y * y).sqrt() <= 0.951));
        assert!(p.refine_speeds.iter().all(|s| *s <= MAX_PLAYED_SPEED));
        assert!(p.refine_speeds[0] > 1.2);
        let none = profile(&AiPlayer {
            skill: Difficulty::Hard.skill(),
            style: Style { spin: 0.0, banks: -1.0, ..Style::NEUTRAL },
        });
        assert_eq!(none.tips, vec![(0.0, 0.0)]);
        assert!(!none.banks);
    }
}
