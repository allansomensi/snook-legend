//! Deterministic fixed-step rigid-body simulation of pool balls.
//!
//! The model follows the classic treatment of billiard physics:
//! - balls **slide** on the cloth after being struck (kinetic friction acts at the
//!   contact point and converts slip into roll),
//! - then **roll** without slipping (small rolling resistance),
//! - vertical-axis **spin** (English) decays independently,
//! - ball/ball, ball/cushion and ball/slate contacts are resolved with impulses that
//!   include friction, so spin is transferred and "throw" emerges naturally,
//! - balls can leave the slate (jump shots) and fly under gravity.
//!
//! The simulation uses a fixed time step and only `f64` arithmetic, so the same
//! inputs produce the same outputs on every platform running the same binary. The
//! game server re-simulates every shot to keep online matches authoritative.

use glam::{DQuat, DVec2, DVec3};
use serde::{Deserialize, Serialize};

use crate::ball::{Ball, BallId, BallStatus, Motion};
use crate::table::{PocketId, Table};

/// Tunable physical constants.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PhysicsParams {
    pub gravity: f64,
    /// Kinetic friction between a sliding ball and the cloth.
    pub sliding_friction: f64,
    /// Rolling resistance coefficient.
    pub rolling_friction: f64,
    /// Coefficient controlling how fast vertical-axis spin decays.
    pub spin_friction: f64,
    pub ball_restitution: f64,
    /// Friction between two balls (produces throw and spin transfer).
    pub ball_friction: f64,
    pub cushion_restitution: f64,
    pub cushion_friction: f64,
    pub slate_restitution: f64,
    pub slate_friction: f64,
    /// Effective mass of the cue (kg).
    pub cue_mass: f64,
    /// Restitution of the cue tip / ball collision.
    pub cue_restitution: f64,
    /// Maximum cue speed the player can produce (m/s).
    pub max_cue_speed: f64,
    /// Maximum tip offset from the ball centre, as a fraction of the radius.
    /// Strikes further out than this would miscue.
    pub max_tip_offset: f64,
    /// Maximum cue elevation (radians).
    pub max_elevation: f64,
    /// Cue-ball deflection ("squirt") at full side spin (radians).
    pub squirt: f64,
    /// Integration time step (s).
    pub time_step: f64,
    /// Safety limit on the duration of a single shot (s).
    pub max_shot_duration: f64,
    /// How much of its restitution a cushion loses per m/s of impact: the
    /// rubber gives back most of a soft hit and much less of a hard one.
    pub cushion_restitution_slope: f64,
    /// The least a cushion gives back, however hard the hit.
    pub cushion_restitution_min: f64,
    /// How much of their restitution two balls lose per m/s of impact.
    pub ball_restitution_slope: f64,
    /// The least two balls give back, however hard the hit.
    pub ball_restitution_min: f64,
    /// Friction between two balls depends on how fast their surfaces rub:
    /// `ball_friction + ball_friction_low * exp(-ball_friction_decay * v)`
    /// (`v` in m/s), the fit to Marlow's measurements. Zero: constant.
    pub ball_friction_low: f64,
    pub ball_friction_decay: f64,
    /// The fastest the cue moves on a stroke other than the break (m/s): a
    /// player puts their whole body into a break and never into a pot.
    pub max_stroke_speed: f64,
}

impl Default for PhysicsParams {
    /// A bar table's balls and cloth, on the figures measured for a
    /// tournament table (Alciatore's and Marlow's, for phenolic balls on
    /// worsted cloth with K-66 cushions), what depends on speed included,
    /// and strokes at the speeds people really play them. The balls feel
    /// heavier than on the tournament figures, where a break scattered them
    /// all over the table at a pace that read as light balls: the cloth is
    /// slower (a bar table's napped cloth: 1.6% of g against 1%), the balls
    /// give back less of a hard hit (the rack soaks up more of the break),
    /// the cushions a little less, and the break is struck at 18 mph for
    /// the cue ball.
    fn default() -> Self {
        PhysicsParams {
            gravity: 9.81,
            // A sliding ball loses 0.2 g; a ball turning in place loses
            // about 10 rad/s every second.
            sliding_friction: 0.2,
            rolling_friction: 0.016,
            spin_friction: 0.0117,
            // Phenolic balls give back 94% of a soft hit and less of a hard
            // one (the rack takes a good part of a break's energy).
            ball_restitution: 0.94,
            ball_restitution_slope: 0.016,
            ball_restitution_min: 0.78,
            // Marlow's fit: 0.118 for surfaces barely rubbing (a slow cut
            // throws the object ball), 0.01 at speed.
            ball_friction: 0.009_951,
            ball_friction_low: 0.108,
            ball_friction_decay: 1.088,
            // A cushion returns a soft hit almost whole and soaks up a hard
            // one.
            cushion_restitution: 0.90,
            cushion_restitution_slope: 0.05,
            cushion_restitution_min: 0.50,
            cushion_friction: 0.2,
            slate_restitution: 0.5,
            slate_friction: 0.2,
            // A 19 oz cue with a leather tip: the ball leaves at 1.32 times
            // the cue's speed. A power stroke sends it at 12 mph (5.4 m/s).
            cue_mass: 0.54,
            cue_restitution: 0.73,
            max_cue_speed: 6.2,
            max_stroke_speed: 4.1,
            max_tip_offset: 0.5,
            max_elevation: 75f64.to_radians(),
            squirt: 1.6f64.to_radians(),
            time_step: 1.0 / 1000.0,
            max_shot_duration: 40.0,
        }
    }
}

impl PhysicsParams {
    /// The fastest the cue may move on this stroke (m/s).
    pub fn cue_speed_limit(&self, is_break: bool) -> f64 {
        if is_break { self.max_cue_speed } else { self.max_stroke_speed.min(self.max_cue_speed) }
    }
}

/// A cue stroke as chosen by the player.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Shot {
    /// Aim direction in the table plane, radians counter-clockwise from `+x`.
    pub yaw: f64,
    /// Cue elevation above horizontal (radians). Raising the butt lets the player
    /// jump balls or curve the cue ball (massé).
    pub elevation: f64,
    /// Where the tip strikes the cue ball, in the plane facing the cue:
    /// `x` is right (+) / left (-) English, `y` is top (+) / bottom (-) spin.
    /// Length is clamped to 1, where 1 means [`PhysicsParams::max_tip_offset`].
    pub tip: DVec2,
    /// Cue speed at impact (m/s).
    pub speed: f64,
}

impl Shot {
    /// A level, centre-ball shot.
    pub fn new(yaw: f64, speed: f64) -> Self {
        Shot { yaw, elevation: 0.0, tip: DVec2::ZERO, speed }
    }

    pub fn with_tip(mut self, tip: DVec2) -> Self {
        self.tip = tip;
        self
    }

    pub fn with_elevation(mut self, elevation: f64) -> Self {
        self.elevation = elevation;
        self
    }

    /// Returns a copy with every field clamped to what the cue can physically do.
    pub fn sanitized(&self, params: &PhysicsParams) -> Shot {
        self.sanitized_for(params, true)
    }

    /// The same, for a break (`is_break`) or an ordinary stroke, which has
    /// a lower speed limit (see [`PhysicsParams::max_stroke_speed`]).
    pub fn sanitized_for(&self, params: &PhysicsParams, is_break: bool) -> Shot {
        let finite = |v: f64| if v.is_finite() { v } else { 0.0 };
        let tip = DVec2::new(finite(self.tip.x), finite(self.tip.y));
        Shot {
            yaw: finite(self.yaw).rem_euclid(std::f64::consts::TAU),
            elevation: finite(self.elevation).clamp(0.0, params.max_elevation),
            tip: if tip.length() > 1.0 { tip.normalize() } else { tip },
            speed: finite(self.speed).clamp(0.0, params.cue_speed_limit(is_break)),
        }
    }
}

/// Something that happened during a shot. Used by the rules engine and for sound.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Event {
    BallBall {
        time: f64,
        a: BallId,
        b: BallId,
        speed: f64,
    },
    /// `jaw`: only a pocket's jaw was touched (in its throat, past the
    /// cushion's nose): no bank. `frozen`: only a cushion the ball lay
    /// against when the shot was struck, and hasn't left since: it wasn't
    /// driven to it.
    Cushion {
        time: f64,
        ball: BallId,
        speed: f64,
        #[serde(default)]
        jaw: bool,
        #[serde(default)]
        frozen: bool,
    },
    Slate {
        time: f64,
        ball: BallId,
        speed: f64,
    },
    Pocketed {
        time: f64,
        ball: BallId,
        pocket: PocketId,
    },
    OffTable {
        time: f64,
        ball: BallId,
    },
}

impl Event {
    pub fn time(&self) -> f64 {
        match *self {
            Event::BallBall { time, .. }
            | Event::Cushion { time, .. }
            | Event::Slate { time, .. }
            | Event::Pocketed { time, .. }
            | Event::OffTable { time, .. } => time,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrikeError {
    BallNotInPlay,
    BallsStillMoving,
}

/// A running simulation.
#[derive(Debug, Clone)]
pub struct Simulation {
    pub table: Table,
    pub params: PhysicsParams,
    pub balls: Vec<Ball>,
    time: f64,
    accumulator: f64,
    events: Vec<Event>,
    crossing: Option<Crossing>,
    /// For each ball, the cushion sections (bits by index) it lay against
    /// when the shot was struck and hasn't left since.
    frozen: Vec<u32>,
}

/// How close to a cushion a ball at rest counts as touching it.
const FROZEN_EPS: f64 = 2e-5;

/// A ball watched for when it first gets past a line across the table
/// (see [`Simulation::watch_crossing`]).
#[derive(Debug, Clone, Copy)]
struct Crossing {
    ball: BallId,
    line: f64,
    at: Option<f64>,
}

const SLIP_EPS: f64 = 1e-4;
const LINEAR_EPS: f64 = 5e-4;
const SPIN_EPS: f64 = 0.05;
const AIRBORNE_EPS: f64 = 1e-7;
/// Most sub-steps a time step is split into (a ball at the maximum speed
/// needs 3).
const MAX_SUBSTEPS: usize = 16;
/// Bounces slower than this are absorbed by the slate.
const LANDING_SPEED: f64 = 0.15;

impl Simulation {
    pub fn new(table: Table, params: PhysicsParams, balls: Vec<Ball>) -> Self {
        Simulation {
            table,
            params,
            balls,
            time: 0.0,
            accumulator: 0.0,
            events: Vec::new(),
            crossing: None,
            frozen: Vec::new(),
        }
    }

    /// Notes the first time `ball` gets past the line `x = line` towards
    /// the foot of the table (see [`Self::crossed_at`]). It only watches:
    /// the shot plays the same.
    pub fn watch_crossing(&mut self, ball: BallId, line: f64) {
        self.crossing = Some(Crossing { ball, line, at: None });
    }

    /// When the ball given to [`Self::watch_crossing`] first got past its
    /// line, if it has.
    pub fn crossed_at(&self) -> Option<f64> {
        self.crossing.and_then(|c| c.at)
    }

    pub fn time(&self) -> f64 {
        self.time
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Tests of the rules: `id` leaves the table at the end of the shot.
    #[cfg(test)]
    pub(crate) fn knock_off_for_tests(&mut self, id: BallId) {
        let time = self.time;
        if let Some(b) = self.balls.iter_mut().find(|b| b.id == id) {
            b.status = BallStatus::OffTable;
            b.velocity = DVec3::ZERO;
            b.angular_velocity = DVec3::ZERO;
            b.motion = Motion::Stationary;
        }
        self.events.push(Event::OffTable { time, ball: id });
    }

    /// Tests of the rules: `id` goes down `pocket` at the end of the shot
    /// (after whatever went down during it).
    #[cfg(test)]
    pub(crate) fn pocket_for_tests(&mut self, id: BallId, pocket: PocketId) {
        let time = self.time;
        if let Some(b) = self.balls.iter_mut().find(|b| b.id == id) {
            b.status = BallStatus::Pocketed(pocket);
            b.velocity = DVec3::ZERO;
            b.angular_velocity = DVec3::ZERO;
            b.motion = Motion::Stationary;
        }
        self.events.push(Event::Pocketed { time, ball: id, pocket });
    }

    pub fn ball(&self, id: BallId) -> Option<&Ball> {
        self.balls.iter().find(|b| b.id == id)
    }

    fn ball_index(&self, id: BallId) -> Option<usize> {
        self.balls.iter().position(|b| b.id == id)
    }

    fn radius(&self) -> f64 {
        self.table.spec.ball_radius
    }

    fn inertia(&self) -> f64 {
        0.4 * self.table.spec.ball_mass * self.radius() * self.radius()
    }

    /// True when nothing on the table can change any more: every ball has
    /// stopped, apart from balls spinning in place (they no longer move).
    pub fn is_settled(&self) -> bool {
        !self.balls.iter().any(|b| b.is_moving() && b.motion != Motion::Spinning)
    }

    /// Stops everything (balls left spinning in place, or the safety limit).
    fn stop_all(&mut self) {
        for b in &mut self.balls {
            if b.is_moving() {
                b.velocity = DVec3::ZERO;
                b.angular_velocity = DVec3::ZERO;
                b.motion = Motion::Stationary;
            }
        }
    }

    /// Strikes `ball` with the cue.
    pub fn strike(&mut self, ball: BallId, shot: &Shot) -> Result<(), StrikeError> {
        if !self.is_settled() {
            return Err(StrikeError::BallsStillMoving);
        }
        let idx = self.ball_index(ball).ok_or(StrikeError::BallNotInPlay)?;
        if !self.balls[idx].in_play() {
            return Err(StrikeError::BallNotInPlay);
        }
        let (v, w) = cue_impulse(&self.params, &self.table, &shot.sanitized(&self.params));
        // The balls lying against a cushion (a ball driven into it from
        // there isn't driven to it).
        let r = self.radius();
        let dh = self.table.spec.cushion_height - r;
        let contact_radius = (r * r - dh * dh).max(0.0).sqrt();
        self.frozen = self
            .balls
            .iter()
            .map(|b| {
                let c = b.position.truncate();
                self.table.cushions.iter().take(32).enumerate().fold(0u32, |mask, (k, seg)| {
                    let near = c.distance(seg.closest_point(c)) <= contact_radius + FROZEN_EPS;
                    if b.in_play() && near { mask | 1 << k } else { mask }
                })
            })
            .collect();
        let b = &mut self.balls[idx];
        b.velocity = v;
        b.angular_velocity = w;
        b.motion = Motion::Sliding;
        Ok(())
    }

    /// Advances the simulation by `dt` seconds of wall-clock time using fixed
    /// sub-steps. Returns true while balls are still moving.
    pub fn advance(&mut self, dt: f64) -> bool {
        self.accumulator += dt;
        while self.accumulator >= self.params.time_step {
            self.accumulator -= self.params.time_step;
            if self.is_settled() || self.time > self.params.max_shot_duration {
                self.accumulator = 0.0;
                break;
            }
            self.step();
        }
        let moving = !self.is_settled() && self.time <= self.params.max_shot_duration;
        if !moving {
            self.stop_all();
        }
        moving
    }

    /// Runs until every ball has stopped (or the safety limit is hit).
    pub fn run_to_rest(&mut self) {
        while !self.is_settled() && self.time <= self.params.max_shot_duration {
            self.step();
        }
        self.stop_all();
    }

    /// One fixed time step. Fast balls are moved in sub-steps, so none goes
    /// further than an eighth of its radius between two collision checks: a
    /// thin cut at full power still registers the contact.
    pub fn step(&mut self) {
        let dt = self.params.time_step;
        let fastest = self
            .balls
            .iter()
            .filter(|b| b.is_moving())
            .map(|b| b.velocity.length())
            .fold(0.0, f64::max);
        let max_travel = self.radius() / 8.0;
        let n = ((fastest * dt / max_travel).ceil() as usize).clamp(1, MAX_SUBSTEPS);
        let sub = dt / n as f64;
        for _ in 0..n {
            self.substep(sub);
        }
    }

    fn substep(&mut self, dt: f64) {
        self.time += dt;
        for i in 0..self.balls.len() {
            if self.balls[i].is_moving() {
                self.apply_cloth_friction(i, dt);
                self.integrate(i, dt);
            }
        }
        for i in 0..self.balls.len() {
            if self.balls[i].is_moving() {
                self.resolve_slate(i);
            }
        }
        // A few passes make clustered collisions (the break) resolve cleanly.
        for _ in 0..3 {
            if !self.resolve_ball_collisions(dt) {
                break;
            }
        }
        for i in 0..self.balls.len() {
            if self.balls[i].is_moving() {
                self.resolve_cushions(i);
                self.resolve_pockets_and_bounds(i);
            }
        }
        for i in 0..self.balls.len() {
            if self.balls[i].in_play() {
                self.classify_motion(i);
            }
        }
        if let Some(c) = self.crossing.as_mut().filter(|c| c.at.is_none())
            && let Some(b) = self.balls.iter().find(|b| b.id == c.ball)
            && b.position.x > c.line
        {
            c.at = Some(self.time);
        }
    }

    fn apply_cloth_friction(&mut self, i: usize, dt: f64) {
        let p = self.params;
        let r = self.radius();
        let b = &mut self.balls[i];
        if b.motion == Motion::Airborne {
            b.velocity.z -= p.gravity * dt;
            return;
        }
        let v = b.velocity.truncate();
        let w = b.angular_velocity;
        let slip = contact_slip(v, w, r);
        let slip_len = slip.length();
        if slip_len > SLIP_EPS {
            let friction_dv = p.sliding_friction * p.gravity * dt;
            if slip_len <= 3.5 * friction_dv {
                // Slip vanishes within this step: switch to pure rolling.
                let v_roll = v - slip * (2.0 / 7.0);
                set_rolling(b, v_roll, r);
            } else {
                let dir = slip / slip_len;
                let v_new = v - dir * friction_dv;
                b.velocity.x = v_new.x;
                b.velocity.y = v_new.y;
                let dw = 2.5 * p.sliding_friction * p.gravity / r * dt;
                b.angular_velocity.x += -dir.y * dw;
                b.angular_velocity.y += dir.x * dw;
            }
        } else {
            let speed = v.length();
            let dv = p.rolling_friction * p.gravity * dt;
            let v_new = if speed <= dv { DVec2::ZERO } else { v * ((speed - dv) / speed) };
            set_rolling(b, v_new, r);
        }
        let spin_decel = 2.5 * p.spin_friction * p.gravity / r * dt;
        let wz = b.angular_velocity.z;
        b.angular_velocity.z =
            if wz.abs() <= spin_decel { 0.0 } else { wz - wz.signum() * spin_decel };
    }

    fn integrate(&mut self, i: usize, dt: f64) {
        let b = &mut self.balls[i];
        b.position += b.velocity * dt;
        let rot = b.angular_velocity * dt;
        if rot.length_squared() > 0.0 {
            // Portable `DQuat::from_scaled_axis` (same bits on every OS).
            let angle = rot.length();
            let axis = rot / angle;
            let half = angle / 2.0;
            let (sh, ch) = (crate::math::sin(half), crate::math::cos(half));
            let q = DQuat::from_xyzw(axis.x * sh, axis.y * sh, axis.z * sh, ch);
            b.orientation = (q * b.orientation).normalize();
        }
    }

    fn resolve_slate(&mut self, i: usize) {
        let r = self.radius();
        let m = self.table.spec.ball_mass;
        let inertia = self.inertia();
        let p = self.params;
        let time = self.time;
        let b = &mut self.balls[i];
        if b.position.z > r {
            return;
        }
        b.position.z = r;
        if b.velocity.z >= 0.0 {
            return;
        }
        let impact = -b.velocity.z;
        if impact < LANDING_SPEED {
            b.velocity.z = 0.0;
            return;
        }
        let jn = (1.0 + p.slate_restitution) * m * impact;
        b.velocity.z += jn / m;
        // Friction at the contact point converts slip into spin during the bounce.
        let slip = contact_slip(b.velocity.truncate(), b.angular_velocity, r);
        let slip_len = slip.length();
        if slip_len > 1e-9 {
            let jt_max = slip_len / (1.0 / m + r * r / inertia);
            let jt = (p.slate_friction * jn).min(jt_max);
            let impulse = (-slip / slip_len * jt).extend(0.0);
            b.velocity += impulse / m;
            let contact = DVec3::new(0.0, 0.0, -r);
            b.angular_velocity += contact.cross(impulse) / inertia;
        }
        if b.velocity.z < LANDING_SPEED {
            b.velocity.z = 0.0;
        }
        let id = b.id;
        self.events.push(Event::Slate { time, ball: id, speed: impact });
    }

    /// Returns true if any collision was resolved. `dt` is the sub-step just
    /// integrated (for contacts that happened during it).
    fn resolve_ball_collisions(&mut self, dt: f64) -> bool {
        let r = self.radius();
        let m = self.table.spec.ball_mass;
        let inertia = self.inertia();
        let p = self.params;
        let min_dist = 2.0 * r;
        let mut any = false;
        let n_balls = self.balls.len();
        for i in 0..n_balls {
            if !self.balls[i].in_play() {
                continue;
            }
            // The cue ball meets the balls it touched during the sub-step in
            // the order it touched them (not by their numbers): which one it
            // hit first decides fouls. On the stack: this runs every sub-step.
            let mut order = [(0.0f64, 0usize); 32];
            let count = n_balls.saturating_sub(i + 1).min(order.len());
            for (k, slot) in order[..count].iter_mut().enumerate() {
                slot.1 = i + 1 + k;
            }
            if self.balls[i].id == BallId::CUE && self.balls[i].is_moving() {
                let cue = self.balls[i];
                for slot in &mut order[..count] {
                    let b = self.balls[slot.1];
                    let delta = b.position - cue.position;
                    let touch = touch_time(delta, b.velocity - cue.velocity, min_dist, dt);
                    slot.0 = if !b.in_play() {
                        f64::MIN
                    } else if delta.length_squared() < min_dist * min_dist {
                        // Touching now: since the moment found, or since the
                        // sub-step began.
                        touch.unwrap_or(dt)
                    } else {
                        touch.unwrap_or(f64::MIN)
                    };
                }
                // Earliest touch first (the longest way back); ties by number
                // (a stable sort, without allocating at this length).
                order[..count].sort_by(|a, b| b.0.total_cmp(&a.0));
            }
            for &(_, j) in &order[..count] {
                if !self.balls[j].in_play() {
                    continue;
                }
                let (bi, bj) = (self.balls[i], self.balls[j]);
                if !bi.is_moving() && !bj.is_moving() {
                    continue;
                }
                let mut delta = bj.position - bi.position;
                let mut dist_sq = delta.length_squared();
                // Apart now, but they may have touched during the last
                // sub-step (a thin cut): find when, go back to that moment,
                // collide, and finish the sub-step with the new velocities.
                // Overlapping now but apart when the sub-step began (a graze
                // past its deepest point): the same, so the touch registers.
                let mut rewind = 0.0;
                let apart = dist_sq >= min_dist * min_dist;
                {
                    match touch_time(delta, bj.velocity - bi.velocity, min_dist, dt) {
                        Some(back) => {
                            rewind = back;
                            self.balls[i].position -= bi.velocity * back;
                            self.balls[j].position -= bj.velocity * back;
                            delta = self.balls[j].position - self.balls[i].position;
                            dist_sq = delta.length_squared();
                        }
                        None if apart => continue,
                        None => {}
                    }
                }
                if dist_sq == 0.0 {
                    continue;
                }
                let (bi, bj) = (self.balls[i], self.balls[j]);
                let dist = dist_sq.sqrt();
                let n = delta / dist;
                let ri = n * r;
                let rj = -n * r;
                let vci = bi.velocity + bi.angular_velocity.cross(ri);
                let vcj = bj.velocity + bj.angular_velocity.cross(rj);
                let rel = vci - vcj;
                let vn = rel.dot(n);

                // Push the balls apart so they never remain interpenetrated.
                let overlap = (min_dist - dist).max(0.0);
                let mut correction = n * (overlap / 2.0 + 1e-9);
                if bi.motion != Motion::Airborne && bj.motion != Motion::Airborne {
                    correction.z = 0.0;
                }
                self.balls[i].position -= correction;
                self.balls[j].position += correction;

                if vn <= 0.0 {
                    // Not approaching after all: put them back where they were.
                    self.balls[i].position += bi.velocity * rewind;
                    self.balls[j].position += bj.velocity * rewind;
                    continue;
                }
                any = true;
                let restitution = (p.ball_restitution - p.ball_restitution_slope * vn)
                    .max(p.ball_restitution_min);
                let jn = (1.0 + restitution) * vn / (2.0 / m);
                let mut impulse_i = -n * jn;
                let tangential = rel - n * vn;
                let t_len = tangential.length();
                if t_len > 1e-9 {
                    let jt_max = t_len / (2.0 * (1.0 / m + r * r / inertia));
                    let friction = if p.ball_friction_low > 0.0 {
                        p.ball_friction
                            + p.ball_friction_low * crate::math::exp(-p.ball_friction_decay * t_len)
                    } else {
                        p.ball_friction
                    };
                    let jt = (friction * jn).min(jt_max);
                    impulse_i -= tangential / t_len * jt;
                }
                let impulse_j = -impulse_i;
                {
                    let b = &mut self.balls[i];
                    b.velocity += impulse_i / m;
                    b.angular_velocity += ri.cross(impulse_i) / inertia;
                    b.motion = Motion::Sliding;
                }
                {
                    let b = &mut self.balls[j];
                    b.velocity += impulse_j / m;
                    b.angular_velocity += rj.cross(impulse_j) / inertia;
                    b.motion = Motion::Sliding;
                }
                for k in [i, j] {
                    let b = &mut self.balls[k];
                    if b.position.z <= r + AIRBORNE_EPS && b.velocity.z < 0.0 {
                        b.velocity.z = 0.0;
                    }
                    // The rest of the sub-step, after the contact.
                    b.position += b.velocity * rewind;
                }
                // When they touched (within the sub-step).
                self.events.push(Event::BallBall {
                    time: self.time - rewind,
                    a: bi.id,
                    b: bj.id,
                    speed: vn,
                });
            }
        }
        any
    }

    fn resolve_cushions(&mut self, i: usize) {
        let r = self.radius();
        let m = self.table.spec.ball_mass;
        let inertia = self.inertia();
        let p = self.params;
        let h = self.table.spec.cushion_height;
        let time = self.time;
        let ball = self.balls[i];
        let dh = h - ball.position.z;
        if dh.abs() >= r {
            return; // Flying over (or below) the cushion nose.
        }
        let contact_radius = (r * r - dh * dh).sqrt();
        let mut b = ball;
        let mut hit_speed: Option<f64> = None;
        let mut rail = false;
        let mut fresh = false;
        let mut frozen = self.frozen.get(i).copied().unwrap_or(0);
        let (hl, hw) = (self.table.spec.length / 2.0, self.table.spec.width / 2.0);
        for (k, seg) in self.table.cushions.iter().enumerate() {
            let c = b.position.truncate();
            let q = seg.closest_point(c);
            let delta = c - q;
            let dist = delta.length();
            let bit = if k < 32 { 1u32 << k } else { 0 };
            if dist > contact_radius + FROZEN_EPS {
                frozen &= !bit; // Off that cushion: a touch from now on counts.
            }
            if dist >= contact_radius || dist == 0.0 {
                continue;
            }
            let inward = delta / dist;
            // Contact point relative to the centre (nose of the cushion).
            let contact = (-inward * contact_radius).extend(dh);
            let normal = -contact / r;
            let vc = b.velocity + b.angular_velocity.cross(contact);
            let vn = vc.dot(normal);
            let fixed = q + inward * contact_radius;
            b.position.x = fixed.x;
            b.position.y = fixed.y;
            if vn >= 0.0 {
                continue;
            }
            let restitution = (p.cushion_restitution + p.cushion_restitution_slope * vn)
                .max(p.cushion_restitution_min);
            let jn = -(1.0 + restitution) * m * vn;
            let mut impulse = normal * jn;
            let tangential = vc - normal * vn;
            let t_len = tangential.length();
            if t_len > 1e-9 {
                let jt_max = t_len / (1.0 / m + r * r / inertia);
                let jt = (p.cushion_friction * jn).min(jt_max);
                impulse -= tangential / t_len * jt;
            }
            b.velocity += impulse / m;
            b.angular_velocity += contact.cross(impulse) / inertia;
            // The slate absorbs the downward part of the cushion reaction.
            if b.position.z <= r + AIRBORNE_EPS && b.velocity.z < 0.0 {
                b.velocity.z = 0.0;
            }
            b.motion = Motion::Sliding;
            hit_speed = Some(hit_speed.unwrap_or(0.0).max(-vn));
            // The jaws run outward from the cushion lines (from the
            // noses either side of a mouth).
            rail |= q.x.abs() <= hl + 1e-9 && q.y.abs() <= hw + 1e-9;
            fresh |= frozen & bit == 0;
        }
        self.balls[i] = b;
        if let Some(f) = self.frozen.get_mut(i) {
            *f = frozen;
        }
        if let Some(speed) = hit_speed {
            let event = Event::Cushion { time, ball: b.id, speed, jaw: !rail, frozen: !fresh };
            self.events.push(event);
        }
    }

    fn resolve_pockets_and_bounds(&mut self, i: usize) {
        let r = self.radius();
        let spec = self.table.spec;
        let time = self.time;
        let b = self.balls[i];
        let xy = b.xy();
        let low_enough = b.position.z - r < spec.cushion_height;
        if low_enough && let Some(pocket) = self.table.pockets.iter().find(|p| p.captures(xy)) {
            let id = pocket.id;
            self.drop_ball(i, BallStatus::Pocketed(id));
            self.events.push(Event::Pocketed { time, ball: b.id, pocket: id });
            return;
        }
        let outside = xy.x.abs() > spec.length / 2.0 + r || xy.y.abs() > spec.width / 2.0 + r;
        let landed = b.position.z <= r + spec.cushion_height;
        // A ball rolling on the slate gets past the cushions only through a
        // pocket's jaws; one in the air may come down anywhere, the rail
        // beside a pocket included: it stays on the rail it lands on (off
        // the table) unless it comes down within a pocket's mouth.
        let flying = b.position.z > r + AIRBORNE_EPS;
        let in_throat = low_enough
            && self
                .table
                .pockets
                .iter()
                .any(|p| if flying { p.in_mouth(xy) } else { p.in_throat(xy) });
        if in_throat && !self.table.is_off_table(xy) {
            // On a pocket shelf or between the facings: still in play.
            return;
        }
        if self.table.is_off_table(xy) || (outside && landed && b.velocity.z <= 0.0) {
            if low_enough && !flying {
                // Squeezed somewhere around a pocket: it ends up in the pocket.
                let nearest = self
                    .table
                    .pockets
                    .iter()
                    .min_by(|a, c| {
                        a.hole_center.distance(xy).total_cmp(&c.hole_center.distance(xy))
                    })
                    .map(|p| p.id)
                    .expect("table has pockets");
                let hole = self.table.pocket(nearest);
                if hole.hole_center.distance(xy) < hole.hole_radius + 2.0 * r {
                    self.drop_ball(i, BallStatus::Pocketed(nearest));
                    self.events.push(Event::Pocketed { time, ball: b.id, pocket: nearest });
                    return;
                }
            }
            self.drop_ball(i, BallStatus::OffTable);
            self.events.push(Event::OffTable { time, ball: b.id });
        }
    }

    fn drop_ball(&mut self, i: usize, status: BallStatus) {
        let b = &mut self.balls[i];
        b.status = status;
        // A ball made keeps the velocity it went in with (it is out of play:
        // nothing here moves it again), so it can be shown falling in.
        if !matches!(status, BallStatus::Pocketed(_)) {
            b.velocity = DVec3::ZERO;
            b.angular_velocity = DVec3::ZERO;
        }
        b.motion = Motion::Stationary;
    }

    fn classify_motion(&mut self, i: usize) {
        let r = self.radius();
        let b = &mut self.balls[i];
        if b.position.z > r + AIRBORNE_EPS || b.velocity.z > 0.0 {
            b.motion = Motion::Airborne;
            return;
        }
        b.position.z = r;
        b.velocity.z = 0.0;
        let v = b.velocity.truncate();
        let slip = contact_slip(v, b.angular_velocity, r);
        b.motion = if slip.length() > SLIP_EPS {
            Motion::Sliding
        } else if v.length() > LINEAR_EPS {
            Motion::Rolling
        } else if b.angular_velocity.z.abs() > SPIN_EPS {
            b.velocity = DVec3::ZERO;
            b.angular_velocity.x = 0.0;
            b.angular_velocity.y = 0.0;
            Motion::Spinning
        } else {
            b.velocity = DVec3::ZERO;
            b.angular_velocity = DVec3::ZERO;
            Motion::Stationary
        };
    }
}

/// Two balls `delta` apart now (`rel` = their relative velocity) were within
/// `min_dist` at some point of the last `dt` seconds: how long ago they
/// first touched.
fn touch_time(delta: DVec3, rel: DVec3, min_dist: f64, dt: f64) -> Option<f64> {
    let a = rel.length_squared();
    if a <= 0.0 {
        return None;
    }
    // Where they were `dt` ago, and |d0 + rel·t| = min_dist for t in [0, dt].
    let d0 = delta - rel * dt;
    let b = 2.0 * d0.dot(rel);
    let c = d0.length_squared() - min_dist * min_dist;
    if c <= 0.0 {
        return None; // Already touching then: handled as an overlap.
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / (2.0 * a);
    (0.0..=dt).contains(&t).then_some(dt - t)
}

/// Horizontal velocity of the point of the ball touching the cloth.
fn contact_slip(v: DVec2, w: DVec3, r: f64) -> DVec2 {
    DVec2::new(v.x - r * w.y, v.y + r * w.x)
}

fn set_rolling(b: &mut Ball, v: DVec2, r: f64) {
    b.velocity.x = v.x;
    b.velocity.y = v.y;
    b.angular_velocity.x = -v.y / r;
    b.angular_velocity.y = v.x / r;
}

/// Linear and angular velocity given to the cue ball by a cue strike.
///
/// The cue is modelled as a rigid body of mass `cue_mass` hitting the ball at the
/// chosen tip offset along the (possibly elevated) cue axis.
pub fn cue_impulse(params: &PhysicsParams, table: &Table, shot: &Shot) -> (DVec3, DVec3) {
    let r = table.spec.ball_radius;
    let m = table.spec.ball_mass;
    let inertia = 0.4 * m * r * r;
    let yaw = shot.yaw + params.squirt * shot.tip.x;
    let forward = DVec3::new(crate::math::cos(yaw), crate::math::sin(yaw), 0.0);
    let up = DVec3::Z;
    let right = forward.cross(up);
    let (sin_e, cos_e) = (crate::math::sin(shot.elevation), crate::math::cos(shot.elevation));
    let axis = forward * cos_e - up * sin_e;
    let cue_up = up * cos_e + forward * sin_e;
    let a = shot.tip.x * params.max_tip_offset * r;
    let b = shot.tip.y * params.max_tip_offset * r;
    let c = (r * r - a * a - b * b).max(0.0).sqrt();
    let contact = right * a + cue_up * b - axis * c;
    let lever = contact.cross(axis);
    let k = 1.0 / params.cue_mass + 1.0 / m + lever.length_squared() / inertia;
    let impulse = (1.0 + params.cue_restitution) * shot.speed / k;
    (axis * (impulse / m), contact.cross(axis * impulse) / inertia)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::TableSpec;
    use std::f64::consts::PI;

    fn sim_with(balls: &[(u8, DVec2)]) -> Simulation {
        let table = Table::default();
        let r = table.spec.ball_radius;
        let balls = balls.iter().map(|&(n, p)| Ball::at_rest(BallId(n), p, r)).collect();
        Simulation::new(table, PhysicsParams::default(), balls)
    }

    #[test]
    fn centre_ball_shot_rolls_at_five_sevenths() {
        let mut sim = sim_with(&[(0, DVec2::new(-0.6, 0.0))]);
        sim.strike(BallId::CUE, &Shot::new(0.0, 2.0)).unwrap();
        let v0 = sim.balls[0].velocity.x;
        while sim.balls[0].motion == Motion::Sliding {
            sim.step();
        }
        let v1 = sim.balls[0].velocity.x;
        assert!((v1 / v0 - 5.0 / 7.0).abs() < 0.01, "ratio {}", v1 / v0);
        assert_eq!(sim.balls[0].motion, Motion::Rolling);
    }

    #[test]
    fn stun_shot_stops_cue_ball_dead() {
        // A firm centre-ball hit from close range arrives sliding with no spin,
        // so the cue ball stops near the contact point.
        let mut sim = sim_with(&[(0, DVec2::new(-0.3, 0.0)), (1, DVec2::new(-0.15, 0.0))]);
        sim.strike(BallId::CUE, &Shot::new(0.0, 3.0)).unwrap();
        let contact_x = -0.15 - 2.0 * 0.028575;
        let mut first_contact = None;
        while !sim.is_settled() && sim.time() < 1.0 {
            sim.step();
            if first_contact.is_none() && !sim.events().is_empty() {
                first_contact = Some(sim.time());
            }
            if first_contact.is_some_and(|t| sim.time() > t + 0.5) {
                break;
            }
        }
        let cue = sim.ball(BallId::CUE).unwrap();
        assert!((cue.position.x - contact_x).abs() < 0.08, "cue at {}", cue.position.x);
    }

    #[test]
    fn draw_shot_comes_back() {
        let mut sim = sim_with(&[(0, DVec2::new(-0.4, 0.0)), (1, DVec2::new(0.0, 0.0))]);
        sim.strike(BallId::CUE, &Shot::new(0.0, 3.5).with_tip(DVec2::new(0.0, -1.0))).unwrap();
        sim.run_to_rest();
        let cue = sim.ball(BallId::CUE).unwrap();
        assert!(cue.position.x < -0.5, "cue should draw back, ended at {}", cue.position.x);
    }

    #[test]
    fn follow_shot_goes_forward() {
        let mut sim = sim_with(&[(0, DVec2::new(-0.4, 0.0)), (1, DVec2::new(0.0, 0.0))]);
        sim.strike(BallId::CUE, &Shot::new(0.0, 2.0).with_tip(DVec2::new(0.0, 1.0))).unwrap();
        let mut max_x = f64::MIN;
        while !sim.is_settled() {
            sim.step();
            max_x = max_x.max(sim.ball(BallId::CUE).unwrap().position.x);
        }
        assert!(max_x > 0.1, "cue should follow through, reached {max_x}");
    }

    #[test]
    fn straight_shot_pots_ball_in_corner() {
        let t = Table::default();
        let pocket = t.pocket(PocketId::FootLeft).aim_point();
        let obj = DVec2::new(0.4, 0.2);
        let dir = (pocket - obj).normalize();
        let cue = obj - dir * 0.5;
        let mut sim = sim_with(&[(0, cue), (3, obj)]);
        sim.strike(BallId::CUE, &Shot::new(dir.y.atan2(dir.x), 2.0)).unwrap();
        sim.run_to_rest();
        assert_eq!(sim.ball(BallId(3)).unwrap().status, BallStatus::Pocketed(PocketId::FootLeft));
        assert!(sim.ball(BallId::CUE).unwrap().in_play());
    }

    #[test]
    fn ball_bounces_off_cushion_and_loses_energy() {
        let mut sim = sim_with(&[(0, DVec2::new(0.4, 0.0))]);
        sim.strike(BallId::CUE, &Shot::new(PI / 2.0, 2.0)).unwrap();
        sim.run_to_rest();
        assert!(sim.events().iter().any(|e| matches!(e, Event::Cushion { .. })));
        let b = sim.ball(BallId::CUE).unwrap();
        assert!(b.in_play());
        assert!((b.position.x - 0.4).abs() < 0.02, "should come straight back, x={}", b.position.x);
        assert!(b.position.z > 0.0);
    }

    #[test]
    fn side_spin_changes_rebound_angle() {
        let run = |side: f64| {
            let mut sim = sim_with(&[(0, DVec2::new(0.4, 0.0))]);
            sim.strike(BallId::CUE, &Shot::new(PI / 2.0, 1.5).with_tip(DVec2::new(side, 0.0)))
                .unwrap();
            sim.run_to_rest();
            sim.ball(BallId::CUE).unwrap().position.x
        };
        let left = run(-1.0);
        let right = run(1.0);
        assert!((left - right).abs() > 0.1, "english should change the rebound: {left} vs {right}");
    }

    #[test]
    fn elevated_shot_jumps() {
        let mut sim = sim_with(&[(0, DVec2::new(-0.5, 0.0))]);
        sim.strike(BallId::CUE, &Shot::new(0.0, 5.0).with_elevation(45f64.to_radians())).unwrap();
        let mut max_z = 0.0f64;
        while !sim.is_settled() {
            sim.step();
            max_z = max_z.max(sim.balls[0].position.z);
        }
        assert!(max_z > 0.028575 + 0.02, "ball should leave the slate, max z {max_z}");
    }

    #[test]
    fn jump_shot_clears_blocker() {
        let r = TableSpec::WPA_8FT.ball_radius;
        let mut sim = sim_with(&[(0, DVec2::new(-0.5, 0.0)), (5, DVec2::new(-0.36, 0.0))]);
        sim.strike(BallId::CUE, &Shot::new(0.0, 5.5).with_elevation(50f64.to_radians())).unwrap();
        sim.run_to_rest();
        let cue = sim.ball(BallId::CUE).unwrap();
        let blocker = sim.ball(BallId(5)).unwrap();
        assert!(cue.position.x > -0.36 + r, "cue ball should land beyond the blocker");
        assert!(blocker.status == BallStatus::InPlay);
    }

    /// The thinnest cuts at full power must count as contact (they used to
    /// slip between two collision checks).
    #[test]
    fn thin_cuts_at_full_power_register() {
        let r = TableSpec::WPA_8FT.ball_radius;
        for k in 0..40 {
            // Overlap of 0.02 mm, at 40 different phases along the path.
            let offset = 2.0 * r - 0.00002;
            let start = -0.5 - k as f64 * 0.00017;
            let mut sim = sim_with(&[(0, DVec2::new(start, 0.0)), (5, DVec2::new(0.0, offset))]);
            sim.strike(BallId::CUE, &Shot::new(0.0, 7.0)).unwrap();
            sim.run_to_rest();
            let hit = sim.events().iter().any(|e| matches!(e, Event::BallBall { .. }));
            assert!(hit, "graze missed at phase {k}");
        }
    }

    /// A ball rolling straight into a cushion: the speed it rolls away
    /// with, as a fraction of the speed it came in with.
    fn rebound(speed: f64) -> f64 {
        let table = Table::default();
        let r = table.spec.ball_radius;
        let mut ball =
            Ball::at_rest(BallId::CUE, DVec2::new(table.spec.length / 2.0 - 0.1, 0.0), r);
        ball.velocity = DVec3::new(speed, 0.0, 0.0);
        ball.angular_velocity = DVec3::new(0.0, speed / r, 0.0);
        ball.motion = Motion::Rolling;
        let mut sim = Simulation::new(table, PhysicsParams::default(), vec![ball]);
        let mut arriving = speed;
        while sim.balls[0].velocity.x > 0.0 {
            arriving = sim.balls[0].velocity.x;
            sim.step();
        }
        let b = &sim.balls[0];
        let slip = contact_slip(b.velocity.truncate(), b.angular_velocity, r);
        (b.velocity.truncate() - slip * (2.0 / 7.0)).length() / arriving
    }

    /// A soft hit comes back at about 55% of its pace, a hard one at much
    /// less (the rubber soaks it up).
    #[test]
    fn cushions_soak_up_hard_hits() {
        let (soft, firm, hard) = (rebound(1.0), rebound(3.0), rebound(8.0));
        assert!((0.5..0.6).contains(&soft), "soft {soft}");
        assert!(firm < soft - 0.03 && hard < firm - 0.05, "{soft} {firm} {hard}");
        assert!((0.2..0.35).contains(&hard), "hard {hard}");
    }

    /// The pace of the cloth: a lag (the ball just reaching the far
    /// cushion and back) is a stroke of medium pace, and the hardest
    /// ordinary stroke goes round the table a couple of times.
    #[test]
    fn strokes_travel_as_far_as_on_a_real_table() {
        let p = PhysicsParams::default();
        let lengths = |cue_speed: f64| {
            let mut sim = sim_with(&[(0, DVec2::new(-0.5, 0.0))]);
            let shot = Shot::new(0.0, cue_speed).sanitized_for(&p, false);
            sim.strike(BallId::CUE, &shot).unwrap();
            let mut dist = 0.0;
            while !sim.is_settled() {
                sim.step();
                dist += sim.balls[0].velocity.length() * p.time_step;
            }
            dist / sim.table.spec.length
        };
        let lag = lengths(1.5);
        let hardest = lengths(99.0);
        println!("TRAVEL lag {lag} hardest {hardest}");
        assert!((0.0..99.0).contains(&lag), "lag {lag}");
        assert!((0.0..99.0).contains(&hardest), "hardest stroke {hardest}");
    }

    /// The break is struck at 18 mph for the cue ball; an ordinary stroke
    /// tops out at a power shot, 12 mph.
    #[test]
    fn only_the_break_is_played_at_break_speed() {
        let p = PhysicsParams::default();
        let table = Table::default();
        let ball_speed = |is_break: bool| {
            let shot = Shot::new(0.0, 99.0).sanitized_for(&p, is_break);
            cue_impulse(&p, &table, &shot).0.length()
        };
        println!("SPEED stroke {} break {}", ball_speed(false), ball_speed(true));
        assert!((5.0..5.8).contains(&ball_speed(false)), "{}", ball_speed(false));
        assert!((7.8..8.5).contains(&ball_speed(true)), "{}", ball_speed(true));
    }

    /// A slow cut throws the object ball off the line of centres more than
    /// a fast one (ball friction falls with speed).
    #[test]
    fn slow_cuts_throw_more() {
        let throw = |speed: f64| {
            let r = Table::default().spec.ball_radius;
            // A half-ball hit: the line of centres is 30° off the stroke.
            let mut sim = sim_with(&[(0, DVec2::new(-0.3, r)), (1, DVec2::new(0.0, 0.0))]);
            sim.strike(BallId::CUE, &Shot::new(0.0, speed)).unwrap();
            while !sim.events().iter().any(|e| matches!(e, Event::BallBall { .. })) {
                sim.step();
            }
            let v = sim.ball(BallId(1)).unwrap().velocity;
            (crate::math::atan2(v.y, v.x).to_degrees() + 30.0).abs()
        };
        let (slow, fast) = (throw(0.6), throw(4.0));
        assert!(slow > fast + 0.5, "slow {slow}°, fast {fast}°");
        assert!(slow < 7.0, "slow {slow}°");
    }

    /// The tip at the miscue limit (half a radius off centre) gives the
    /// most spin a cue can: the surface turning at 1.25 times the ball's
    /// speed. Side spin squirts the ball a little the other way.
    #[test]
    fn the_tip_gives_as_much_spin_as_a_real_one() {
        let (p, t) = (PhysicsParams::default(), Table::default());
        let r = t.spec.ball_radius;
        for tip in [DVec2::X, DVec2::Y, -DVec2::Y, DVec2::new(0.6, -0.8)] {
            let (v, w) = cue_impulse(&p, &t, &Shot::new(0.0, 2.0).with_tip(tip));
            assert!((w.length() * r / v.length() - 1.25).abs() < 0.01);
        }
        let (v, w) = cue_impulse(&p, &t, &Shot::new(0.0, 2.0).with_tip(DVec2::X));
        // Right english: anticlockwise from above, the ball squirts left.
        assert!(w.z > 0.0 && v.y > 0.0 && v.y / v.x < 0.05);
    }

    /// The 30° rule: a rolling cue ball leaves a half-ball hit at about
    /// 34° from its line, and the object ball goes close to the line of
    /// centres (30°), thrown a little by the cut.
    #[test]
    fn a_rolling_cue_ball_follows_the_thirty_degree_rule() {
        let r = Table::default().spec.ball_radius;
        let mut sim = sim_with(&[(0, DVec2::new(-0.9, r)), (1, DVec2::new(0.0, 0.0))]);
        sim.strike(BallId::CUE, &Shot::new(0.0, 1.0)).unwrap();
        while !sim.events().iter().any(|e| matches!(e, Event::BallBall { .. })) {
            sim.step();
        }
        let object = sim.ball(BallId(1)).unwrap().velocity;
        let thrown = crate::math::atan2(object.y, object.x).to_degrees() + 30.0;
        assert!((0.3..4.0).contains(&thrown), "throw {thrown}°");
        // Once the cue ball rolls again (its path has curved forward).
        while sim.balls[0].motion != Motion::Rolling {
            sim.step();
        }
        let v = sim.balls[0].velocity;
        let angle = crate::math::atan2(v.y, v.x).to_degrees();
        assert!((31.0..37.0).contains(&angle), "cue ball leaves at {angle}°");
    }

    /// Side spin, square into a cushion: the ball comes off some 25° to
    /// the side the spin runs to, and half the spin gives less.
    #[test]
    fn side_spin_throws_the_ball_off_a_cushion() {
        let off = |side: f64| {
            let mut sim = sim_with(&[(0, DVec2::new(0.5, 0.0))]);
            sim.strike(BallId::CUE, &Shot::new(PI / 2.0, 2.0).with_tip(DVec2::new(side, 0.0)))
                .unwrap();
            while !sim.events().iter().any(|e| matches!(e, Event::Cushion { .. })) {
                sim.step();
            }
            for _ in 0..300 {
                sim.step();
            }
            let v = sim.balls[0].velocity;
            crate::math::atan2(v.x, -v.y).to_degrees()
        };
        let (full, half) = (off(1.0), off(0.5));
        assert!((20.0..35.0).contains(&full), "full side {full}°");
        assert!(half > 8.0 && half < full - 4.0, "half side {half}°");
        assert!((off(-1.0) + full).abs() < 0.5);
    }

    /// Raising the cue with side spin curves the ball to the side of the
    /// spin: nothing with a level cue, a swerve of a few centimetres at
    /// 5°, a massé that turns right round at 70°.
    #[test]
    fn an_elevated_cue_curves_the_ball() {
        // Where the ball is across the table after a metre along its line
        // (right english: the right is -y), and where it is heading.
        let run = |elevation: f64, speed: f64| {
            let mut sim = sim_with(&[(0, DVec2::new(-0.9, 0.0))]);
            let shot = Shot::new(0.0, speed)
                .with_tip(DVec2::new(1.0, 0.0))
                .with_elevation(elevation.to_radians());
            sim.strike(BallId::CUE, &shot).unwrap();
            let squirt = sim.balls[0].velocity.y / sim.balls[0].velocity.x;
            let mut across = None;
            while !sim.is_settled() && sim.time() < 6.0 {
                sim.step();
                let b = &sim.balls[0];
                if across.is_none() && b.position.x > 0.1 {
                    across = Some(b.position.y - squirt);
                }
                if sim.events().iter().any(|e| matches!(e, Event::Cushion { .. })) {
                    break;
                }
            }
            (across, sim.balls[0].position)
        };
        let level = run(0.0, 2.0).0.unwrap();
        assert!(level.abs() < 0.002, "level cue: {level}");
        let swerve = run(5.0, 2.0).0.unwrap();
        assert!((-0.06..-0.015).contains(&swerve), "5°: {swerve}");
        let more = run(15.0, 2.0).0.unwrap();
        assert!(more < 2.0 * swerve, "15°: {more}");
        // The massé never gets a metre down the table: it hooks to the right.
        let (across, end) = run(70.0, 3.0);
        assert!(across.is_none() && end.y < -0.3 && end.x < 0.0, "massé ends at {end}");
    }

    /// A ball made keeps the velocity it went in with, to be shown falling.
    #[test]
    fn a_ball_made_keeps_its_velocity() {
        let t = Table::default();
        let pocket = t.pocket(PocketId::FootLeft).aim_point();
        let start = DVec2::new(0.4, 0.2);
        let dir = (pocket - start).normalize();
        let mut sim = sim_with(&[(0, start)]);
        sim.strike(BallId::CUE, &Shot::new(dir.y.atan2(dir.x), 2.0)).unwrap();
        sim.run_to_rest();
        let b = &sim.balls[0];
        assert!(matches!(b.status, BallStatus::Pocketed(_)));
        assert!(b.velocity.truncate().dot(dir) > 0.5 && !b.is_moving());
    }

    /// A ball in the air that comes down on the rail beside a pocket is
    /// off the table, even over a jaw; one that comes down within the mouth
    /// drops.
    #[test]
    fn a_ball_that_jumps_onto_the_rail_beside_a_pocket_is_off_the_table() {
        let table = Table::default();
        let r = table.spec.ball_radius;
        let side = *table.pocket(PocketId::SideLeft);
        let land = |lateral: f64, depth: f64| {
            let at = side.mouth_center + side.outward.perp() * lateral + side.outward * depth;
            let mut ball = Ball::at_rest(BallId::EIGHT, at, r);
            ball.position.z = r + table.spec.cushion_height + 0.02;
            ball.velocity = DVec3::new(0.0, 0.0, -0.3);
            ball.motion = Motion::Airborne;
            let mut sim = Simulation::new(table.clone(), PhysicsParams::default(), vec![ball]);
            sim.run_to_rest();
            sim.balls[0].status
        };
        assert_eq!(land(side.half_mouth + 0.03, 0.035), BallStatus::OffTable);
        assert_eq!(land(side.half_mouth + 0.017, 0.035), BallStatus::OffTable);
        assert_eq!(land(0.0, 0.035), BallStatus::Pocketed(PocketId::SideLeft));
    }

    #[test]
    fn determinism() {
        let run = || {
            let mut sim = sim_with(&[
                (0, DVec2::new(-0.5, 0.05)),
                (1, DVec2::new(0.2, 0.0)),
                (2, DVec2::new(0.3, 0.1)),
            ]);
            sim.strike(BallId::CUE, &Shot::new(0.1, 6.0).with_tip(DVec2::new(0.3, -0.4))).unwrap();
            sim.run_to_rest();
            sim.balls.iter().map(|b| b.position).collect::<Vec<_>>()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn shot_sanitizing() {
        let p = PhysicsParams::default();
        let s = Shot { yaw: -0.5, elevation: 3.0, tip: DVec2::new(3.0, 4.0), speed: 99.0 }
            .sanitized(&p);
        assert!(s.yaw >= 0.0 && s.yaw < std::f64::consts::TAU);
        assert_eq!(s.elevation, p.max_elevation);
        assert!((s.tip.length() - 1.0).abs() < 1e-12);
        assert_eq!(s.speed, p.max_cue_speed);
    }
}
