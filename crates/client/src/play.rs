//! Player controls and the per-frame table overlay (cue, aim line, markers).
//!
//! | Action | Mouse | Keyboard (defaults, see `keys.rs`) |
//! |---|---|---|
//! | Aim | left-drag on the table or the cue, click a spot (Shift: fine) | A / D or ← / → |
//! | Spin | spin ball in the panel | ↑ / ↓, Q / E |
//! | Cue elevation | elevation slider | W / S |
//! | Shoot | power slider + Shoot | hold Space, release |
//! | Ball in hand | drag the cue ball | B toggles, Enter confirms |
//! | Call the 8-ball pocket | click a pocket | |
//! | Camera | right-drag turns the table (3D), wheel zoom | Tab 3D/top, C top view, = / - zoom |

use ::glam::DVec2;
use macroquad::prelude::*;
use snook_legend_core::{BallId, BallInHand, PocketId};

use crate::camera::{CameraMode, CameraRig};
use crate::config::AimGuide;
use crate::coords::{TABLE_HEIGHT, surface, to_table};
use crate::game::{Controller, Match, Phase};
use crate::gfx::SceneCamera;
use crate::keys::{self, Action};
use crate::scene::{Guide, Overlay, RAIL_HEIGHT, Scene};

/// Fraction of a full charge gained per second while Space is held.
const CHARGE_RATE: f32 = 0.7;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum DragMode {
    #[default]
    None,
    /// Moving the cue ball in hand.
    Place,
    /// Swinging the cue stick itself around the cue ball.
    Cue,
    /// The cue points at the pointer.
    Point,
    /// Shift-drag: very fine rotation.
    Fine,
}

#[derive(Default)]
pub struct Controls {
    drag: f32,
    last_mouse: Vec2,
    mode: DragMode,
    yaw_before: f64,
    /// How long an aim key has been held (keys speed up).
    key_hold: f64,
    /// Fine-aim speed multiplier from the settings.
    pub sensitivity: f32,
    /// The pointer is over the cue (shown with a "move" cursor).
    over_cue: bool,
    /// A right-button drag that turns the camera.
    orbiting: bool,
    /// How far the pointer moved during that drag.
    orbit_drag: f32,
}

impl Controls {
    /// True while the player is aiming with the mouse (dragging the cue or
    /// the aim point, or moving the cue ball): the HUD gets out of the way.
    pub fn aiming(&self) -> bool {
        (self.mode != DragMode::None && is_mouse_button_down(MouseButton::Left) && self.drag > 3.0)
            // Turning the view with the right button: the table, nothing else.
            || (self.orbiting && self.orbit_drag > 4.0)
    }

    fn set_over_cue(&mut self, over: bool) {
        if over != self.over_cue {
            self.over_cue = over;
            use macroquad::miniquad::{CursorIcon, window::set_mouse_cursor};
            set_mouse_cursor(if over { CursorIcon::Move } else { CursorIcon::Default });
        }
    }
}

/// True if the screen point `mouse` is on (or very near) the cue stick.
fn pointer_on_cue(m: &Match, cam: &SceneCamera, radius: f32, mouse: Vec2) -> bool {
    let cue = m.game.ball(BallId::CUE).xy();
    let dir = DVec2::from_angle(m.aim.yaw);
    let r = radius as f64;
    // Test the first ~70 cm of the stick in short pieces (the far end can be
    // behind the camera).
    let mut prev: Option<Vec2> = None;
    for k in 0..=14 {
        let along = r + 0.02 + k as f64 * 0.05;
        let lift = radius + along as f32 * 0.04;
        let Some(p) = cam.project(surface(cue - dir * along, lift)) else { break };
        if let Some(a) = prev {
            let ab = p - a;
            let t = ((mouse - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
            if mouse.distance(a + ab * t) < 16.0 {
                return true;
            }
        }
        prev = Some(p);
    }
    false
}

/// Point on the table plane (at ball-centre height) under the mouse.
pub fn pointer_on_table(cam: &SceneCamera, radius: f32) -> Option<DVec2> {
    let (origin, dir) = cam.ray(mouse_position().into());
    let plane_y = TABLE_HEIGHT + radius;
    if dir.y.abs() < 1e-5 {
        return None;
    }
    let t = (plane_y - origin.y) / dir.y;
    (t > 0.0).then(|| to_table(origin + dir * t))
}

impl Controls {
    /// Camera controls (allowed at any time during a match). Dragging with
    /// the left button outside the table (or anywhere while it's not this
    /// player's turn) turns the view, going 3D; the right button always does.
    pub fn camera(
        &mut self,
        rig: &mut CameraRig,
        cam: &SceneCamera,
        ui_hovered: bool,
        typing: bool,
    ) {
        let mouse: Vec2 = mouse_position().into();
        let delta = mouse - self.last_mouse;
        // Only the right button turns the table (going 3D from the top view);
        // the left button is for aiming and never moves the camera.
        if is_mouse_button_pressed(MouseButton::Right) && !ui_hovered {
            self.orbiting = true;
            self.orbit_drag = 0.0;
        }
        if self.orbiting && is_mouse_button_down(MouseButton::Right) {
            self.orbit_drag += delta.length();
            // A few pixels of slack, so a click doesn't nudge the view.
            if self.orbit_drag > 4.0 {
                rig.tv_paused = true;
                self.orbit(rig, delta);
            }
        }
        if !is_mouse_button_down(MouseButton::Right) {
            self.orbiting = false;
        }
        if !typing {
            if keys::pressed(Action::ToggleView) {
                rig.tv_paused = true;
                if rig.mode == CameraMode::Top {
                    rig.enter_3d();
                } else {
                    rig.reset();
                }
            }
            if keys::pressed(Action::ResetCamera) {
                rig.reset();
            }
            // Keyboard zoom: steps while held, like the wheel turning.
            let zoom_in = keys::down(Action::ZoomIn);
            if zoom_in != keys::down(Action::ZoomOut) {
                let rate = 1.6f32.powf(get_frame_time());
                rig.tv_paused = true;
                rig.zoom(if zoom_in { 1.0 / rate } else { rate }, None);
            }
        }
        if !ui_hovered {
            let (_, wheel) = mouse_wheel();
            if wheel != 0.0 {
                let step = if wheel > 0.0 { 0.88 } else { 1.0 / 0.88 };
                let toward = pointer_on_table(cam, 0.0).map(|p| surface(p, 0.0));
                rig.tv_paused = true;
                rig.zoom(step, toward);
            }
        }
    }

    fn orbit(&self, rig: &mut CameraRig, delta: Vec2) {
        if rig.top_only {
            return;
        }
        if rig.mode == CameraMode::Top {
            rig.enter_3d();
        }
        rig.orbit += delta.x * 0.005;
        rig.pitch = (rig.pitch + delta.y * 0.004).clamp(0.12, 1.4);
    }

    /// Shot controls for a human player.
    pub fn player(
        &mut self,
        m: &mut Match,
        scene: &Scene,
        cam: &SceneCamera,
        rig: &mut CameraRig,
        ui_hovered: bool,
        typing: bool,
    ) {
        let mouse: Vec2 = mouse_position().into();
        let delta = mouse - self.last_mouse;
        self.last_mouse = mouse;
        rig.hold = false;
        if !m.is_human_turn() || typing {
            self.mode = DragMode::None;
            self.set_over_cue(false);
            return;
        }
        let dt = get_frame_time() as f64;
        let fine = keys::down(Action::FineAim);
        let pointer_free = !ui_hovered;
        let radius = scene.ball_radius();
        let pointer = pointer_on_table(cam, radius);
        let cue = m.game.ball(BallId::CUE).xy();

        // Ball in hand: B toggles placing, Enter confirms.
        if keys::pressed(Action::BallInHand) && m.game.ball_in_hand().is_some() {
            let placing = m.is_placing_cue_ball();
            m.set_placing(!placing);
        }
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
            m.set_placing(false);
        }

        // A "move" cursor over the cue says it can be grabbed.
        let hovering_cue = self.mode == DragMode::Cue
            || (self.mode == DragMode::None
                && !ui_hovered
                && !m.is_placing_cue_ball()
                && pointer_on_cue(m, cam, radius, mouse));
        self.set_over_cue(hovering_cue);

        // Start of a mouse drag: decide what it does.
        if is_mouse_button_pressed(MouseButton::Left) && pointer_free {
            self.drag = 0.0;
            self.yaw_before = m.aim.yaw;
            let on_cue_ball = pointer.is_some_and(|p| p.distance(cue) < radius as f64 * 1.8);
            self.mode =
                if m.game.ball_in_hand().is_some() && (m.is_placing_cue_ball() || on_cue_ball) {
                    m.set_placing(true);
                    DragMode::Place
                } else if !m.is_placing_cue_ball()
                    && (pointer_on_cue(m, cam, radius, mouse)
                        || pointer_on_cue(m, cam, radius, mouse - delta))
                {
                    DragMode::Cue
                } else if fine {
                    DragMode::Fine
                } else {
                    DragMode::Point
                };
        }
        let down = is_mouse_button_down(MouseButton::Left);
        if down {
            self.drag += delta.length();
        }
        match self.mode {
            DragMode::Place if down => {
                rig.hold = true;
                if let Some(p) = pointer {
                    place_near(m, p);
                }
            }
            DragMode::Point if down => {
                // The cue points at the mouse; the camera waits until release.
                rig.hold = true;
                if let Some(p) = pointer
                    && p.distance(cue) > radius as f64 * 1.5
                    && self.drag > 3.0
                {
                    let d = p - cue;
                    m.aim.yaw = d.y.atan2(d.x);
                }
            }
            DragMode::Cue if down => {
                // The cue swings around the cue ball, following the pointer:
                // the shot goes from the pointer through the ball.
                rig.hold = true;
                if let Some(p) = pointer
                    && p.distance(cue) > radius as f64 * 1.5
                {
                    let d = cue - p;
                    m.aim.yaw = d.y.atan2(d.x);
                }
            }
            DragMode::Fine if down => {
                m.aim.yaw -= delta.x as f64 * 0.0002 * self.sensitivity.max(0.1) as f64;
            }
            _ => {}
        }
        if is_mouse_button_released(MouseButton::Left) {
            match self.mode {
                DragMode::Place => m.set_placing(false),
                DragMode::Point | DragMode::Fine if self.drag < 4.0 && pointer_free => {
                    m.aim.yaw = self.yaw_before;
                    click(m, scene, pointer);
                }
                _ => {}
            }
            self.mode = DragMode::None;
        }
        if m.is_placing_cue_ball() {
            if !keys::pressed(Action::Shoot) {
                return;
            }
            m.set_placing(false);
        }

        // Keyboard aim: starts very fine and speeds up while held.
        let left = keys::down(Action::AimLeft);
        let right = keys::down(Action::AimRight);
        if left || right {
            self.key_hold += dt;
            let rate =
                if fine { 0.012 } else { 0.03 + 0.9 * (self.key_hold - 0.25).clamp(0.0, 1.0) };
            m.aim.yaw += if left { rate * dt } else { -rate * dt };
        } else {
            self.key_hold = 0.0;
        }
        m.aim.yaw = m.aim.yaw.rem_euclid(std::f64::consts::TAU);

        // Spin and elevation.
        let tip_rate = 1.2 * dt;
        let mut tip = m.aim.tip;
        if keys::down(Action::SpinUp) {
            tip.y += tip_rate;
        }
        if keys::down(Action::SpinDown) {
            tip.y -= tip_rate;
        }
        if keys::down(Action::SpinLeft) {
            tip.x -= tip_rate;
        }
        if keys::down(Action::SpinRight) {
            tip.x += tip_rate;
        }
        m.aim.tip = if tip.length() > 1.0 { tip.normalize() } else { tip };
        let max_elevation = m.game.params().max_elevation;
        if keys::down(Action::ElevationUp) {
            m.aim.elevation = (m.aim.elevation + 0.6 * dt).min(max_elevation);
        }
        if keys::down(Action::ElevationDown) {
            m.aim.elevation = (m.aim.elevation - 0.6 * dt).max(0.0);
        }

        // Power: hold Space (or the shoot key), release to shoot.
        if keys::pressed(Action::Shoot) {
            m.aim.charging = true;
            m.aim.power = 0.0;
        }
        if m.aim.charging {
            // Esc cancels a stroke being charged with Space; a power set on
            // the bar is no stroke (Esc opens the menu and keeps it).
            if is_key_pressed(KeyCode::Escape) && keys::down(Action::Shoot) {
                m.aim.charging = false;
                m.aim.power = 0.0;
                // This Esc cancelled the stroke: it doesn't open the menu too.
                m.escape_used = true;
            } else if keys::down(Action::Shoot) {
                m.aim.power = (m.aim.power + CHARGE_RATE * dt as f32).min(1.0);
            } else if keys::released(Action::Shoot) {
                if m.aim.power > 0.01 {
                    m.shoot();
                } else {
                    m.aim.charging = false;
                }
            }
        }
    }
}

/// Moves the cue ball in hand to `p`, or to the nearest free legal spot.
fn place_near(m: &mut Match, p: DVec2) {
    if m.game.can_place_cue_ball(p) {
        m.game.place_cue_ball(p);
        return;
    }
    let spec = m.game.table().spec;
    let r = spec.ball_radius;
    let mut q = p;
    // Stay on the cloth and, before the break or after a scratch, in the kitchen.
    q.x = q.x.clamp(-spec.length / 2.0 + r, spec.length / 2.0 - r);
    q.y = q.y.clamp(-spec.width / 2.0 + r, spec.width / 2.0 - r);
    if m.game.ball_in_hand() == Some(BallInHand::Kitchen) {
        q.x = q.x.min(m.game.table().head_string_x() - 1e-4);
    }
    for ring in 0..12 {
        let dist = ring as f64 * r * 0.35;
        let steps = if ring == 0 { 1 } else { 16 };
        let mut best: Option<DVec2> = None;
        for k in 0..steps {
            let a = k as f64 / steps as f64 * std::f64::consts::TAU;
            let c = q + DVec2::from_angle(a) * dist;
            if m.game.can_place_cue_ball(c) && best.is_none_or(|b| b.distance(p) > c.distance(p)) {
                best = Some(c);
            }
        }
        if let Some(c) = best {
            m.game.place_cue_ball(c);
            return;
        }
    }
}

/// A click on the table: calls a pocket for the 8-ball, or points the cue at
/// the clicked spot (a ball, or any point on the cloth).
fn click(m: &mut Match, scene: &Scene, at: Option<DVec2>) {
    let Some(p) = at else { return };
    if !matches!(m.phase, Phase::Turn { .. }) {
        return;
    }
    // Calling a pocket for the 8 (only once: the call is then binding).
    if m.game.may_call_pocket() {
        let nearest = PocketId::ALL
            .into_iter()
            .map(|id| (id, scene.pocket_center(id).distance(p)))
            .filter(|(_, d)| *d < 0.12)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((id, _)) = nearest {
            m.called_pocket = Some(id);
            let name = m.names[m.game.current_player()].clone();
            m.notify(
                crate::i18n::tw(
                    "{name} calls the 8-ball in the {pocket} pocket.",
                    &[("name", &name), ("pocket", &crate::i18n::pocket(id))],
                ),
                false,
            );
            return;
        }
    }
    let r = scene.ball_radius() as f64;
    let cue = m.game.ball(BallId::CUE).xy();
    // Snap to the centre of a ball that was clicked (or nearly).
    let target = m
        .game
        .balls()
        .iter()
        .filter(|b| b.in_play() && b.id != BallId::CUE)
        .map(|b| (b.xy(), b.xy().distance(p)))
        .filter(|(_, d)| *d < r * 1.6)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(c, _)| c)
        .unwrap_or(p);
    let d = target - cue;
    if d.length() > r {
        m.aim.yaw = d.y.atan2(d.x).rem_euclid(std::f64::consts::TAU);
    }
}

/// The smallest cue elevation (radians) that keeps the cue above the rail and
/// the other balls behind the cue ball.
fn cue_clearance(m: &Match, scene: &Scene, yaw: f64, gap: f64) -> f64 {
    let spec = scene.table.spec;
    let r = spec.ball_radius;
    let cue = m.game.ball(BallId::CUE);
    if !cue.in_play() {
        return 0.0;
    }
    let p = cue.xy();
    let back = -DVec2::from_angle(yaw);
    let (hl, hw) = (spec.length / 2.0, spec.width / 2.0);
    // Cue radius grows from ~6 mm at the tip to ~14 mm at the butt.
    let cue_r = |x: f64| 0.0064 + (x - r - gap).clamp(0.0, 1.45) * 0.0055;
    // Height of the cue axis above the cloth at the cue ball is about one radius.
    let base = r + cue.position.z.max(r) - r;
    let mut slope: f64 = 0.0;
    let mut need = |x: f64, height: f64| {
        if x > 0.01 {
            slope = slope.max((height - base) / x);
        }
    };
    // The cushion nose line behind the cue ball (the rail is just behind it).
    let mut wall = f64::MAX;
    for (d, lim, q) in [(back.x, hl, p.x), (back.y, hw, p.y)] {
        if d > 1e-9 {
            wall = wall.min((lim - q) / d);
        } else if d < -1e-9 {
            wall = wall.min((-lim - q) / d);
        }
    }
    if wall < 2.0 {
        let rail = RAIL_HEIGHT as f64 + 0.004;
        need(wall.max(0.01), rail + cue_r(wall));
    }
    // Balls in the way of the cue.
    for b in m.game.balls().iter().filter(|b| b.in_play() && b.id != BallId::CUE) {
        let rel = b.xy() - p;
        let along = rel.dot(back);
        if along <= 0.0 {
            continue;
        }
        let perp = (rel - back * along).length();
        if perp < r + cue_r(along) + 0.003 {
            need((along - r).max(0.01), 2.0 * r + cue_r(along) + 0.003);
        }
    }
    slope.max(0.0).atan().min(1.2)
}

/// Distance along `dir` until a ball of radius `r` at `start` touches one at `other`.
fn ray_ball(start: DVec2, dir: DVec2, other: DVec2, r: f64) -> Option<f64> {
    let rel = other - start;
    let along = rel.dot(dir);
    if along <= 0.0 {
        return None;
    }
    let perp_sq = rel.length_squared() - along * along;
    let reach = 4.0 * r * r;
    (perp_sq <= reach).then(|| along - (reach - perp_sq).sqrt())
}

/// Where the cue ball, sent from `cue` along `dir`, first stops going
/// straight.
struct Stop {
    /// How far the cue ball's centre travels (m).
    t: f64,
    /// The ball it meets there, if it is a ball.
    target: Option<BallId>,
    /// It goes over the lip of a pocket there.
    in_pocket: bool,
}

/// Distance along `dir` from `start` to the circle of `radius` round
/// `centre`: where it is entered (None when already inside, or missed).
fn ray_circle(start: DVec2, dir: DVec2, centre: DVec2, radius: f64) -> Option<f64> {
    let rel = centre - start;
    let along = rel.dot(dir);
    let perp_sq = rel.length_squared() - along * along;
    if along <= 0.0 || perp_sq > radius * radius || rel.length_squared() < radius * radius {
        return None;
    }
    Some(along - (radius * radius - perp_sq).sqrt())
}

/// Distance along `dir` until a ball at `start` comes within `reach` of the
/// segment `a`–`b` (a cushion nose or a pocket facing), moving towards it.
fn ray_segment(start: DVec2, dir: DVec2, a: DVec2, b: DVec2, reach: f64) -> Option<f64> {
    let e = b - a;
    let len = e.length();
    if len < 1e-9 {
        return ray_circle(start, dir, a, reach);
    }
    let n = e.perp() / len;
    let side = (start - a).dot(n);
    let n = if side < 0.0 { -n } else { n };
    let (dist, closing) = (side.abs(), -dir.dot(n));
    let mut best: Option<f64> = None;
    if closing > 1e-9 {
        // Already against it (a ball frozen to the cushion): stopped at once.
        let t = ((dist - reach) / closing).max(0.0);
        let u = (start + dir * t - a).dot(e) / (len * len);
        if (0.0..=1.0).contains(&u) {
            best = Some(t);
        }
    }
    // The rounded ends (a cushion's point at a pocket).
    for end in [a, b] {
        if let Some(t) = ray_circle(start, dir, end, reach)
            && best.is_none_or(|k| t < k)
        {
            best = Some(t);
        }
    }
    best
}

/// The first thing the cue ball meets going straight along `dir`: what the
/// simulation itself would stop it with (the balls, every cushion and
/// pocket facing, the holes), so the guide is right all over the table —
/// in a pocket's mouth, along a facing, frozen to a ball or a cushion.
fn first_stop(game: &snook_legend_core::EightBallGame, cue: DVec2, dir: DVec2) -> Option<Stop> {
    let table = game.table();
    let r = table.spec.ball_radius;
    let mut stop: Option<Stop> = None;
    let mut closer = |t: f64, target: Option<BallId>, in_pocket: bool| {
        if stop.as_ref().is_none_or(|s| t < s.t) {
            stop = Some(Stop { t: t.max(0.0), target, in_pocket });
        }
    };
    for b in game.balls().iter().filter(|b| b.in_play() && b.id != BallId::CUE) {
        if let Some(t) = ray_ball(cue, dir, b.xy(), r) {
            closer(t, Some(b.id), false);
        }
    }
    // A ball touches a cushion's nose below its own widest point.
    let dh = table.spec.cushion_height - r;
    let reach = (r * r - dh * dh).max(0.0).sqrt();
    for seg in &table.cushions {
        if let Some(t) = ray_segment(cue, dir, seg.a, seg.b, reach) {
            closer(t, None, false);
        }
    }
    for p in &table.pockets {
        if let Some(t) = ray_circle(cue, dir, p.hole_center, p.hole_radius) {
            closer(t, None, true);
        }
    }
    stop
}

/// Builds the cue and the markings for this frame.
pub fn overlay(m: &Match, scene: &Scene, time: f32) -> Overlay {
    let mut out = Overlay {
        cue_look: m.cue_looks[m.game.current_player() % 2].clone(),
        cue_level: m.cue_levels[m.game.current_player() % 2],
        ..Default::default()
    };
    let table = &scene.table;
    let r = table.spec.ball_radius;
    let cue = m.game.ball(BallId::CUE).xy();
    let line_y = 0.0012;

    // Cue stick.
    let remote_waiting = m.current_controller() == Controller::Remote
        && m.online.as_ref().is_some_and(|o| !o.remote_aimed);
    let gap = match &m.phase {
        Phase::Turn { placing_cue_ball: false, .. } if !remote_waiting => {
            Some(0.006 + if m.aim.charging { 0.03 + m.aim.power as f64 * 0.25 } else { 0.03 })
        }
        Phase::Simulating(_) if m.shot_time().is_some_and(|t| t < 0.15) => Some(-0.012),
        _ => None,
    };
    if m.replay.is_some() {
        return out;
    }
    if let Some(gap) = gap {
        // Raise the cue just enough to clear the rail and other balls, like a
        // player would, so it never passes through anything.
        let elevation = m.aim.elevation.max(cue_clearance(m, scene, m.aim.yaw, gap));
        out.cue = Some(scene.cue_pose(
            m.game.params(),
            m.game.ball(BallId::CUE).position,
            m.aim.yaw,
            elevation,
            m.aim.tip,
            gap,
        ));
    }

    // A drill: the pocket to use, and the circle to leave the cue ball in.
    if let Some(run) = &m.drill {
        use snook_legend_proto::drills::Goal;
        let pulse = 0.5 + 0.5 * (time * 3.0).sin();
        let gold = Color::new(1.0, 0.82, 0.25, 0.55 + 0.35 * pulse);
        let pocket = match run.drill().goal {
            Goal::Pot { pocket, .. }
            | Goal::Bank { pocket, .. }
            | Goal::Position { pocket, .. } => Some(pocket),
            _ => None,
        };
        if let Some(id) = pocket {
            out.guides.push(Guide::Ring {
                center: surface(scene.pocket_center(id), RAIL_HEIGHT + 0.004),
                radius: scene.pocket_radius(id) + 0.018,
                width: 0.008,
                color: gold,
            });
        }
        if let Goal::Position { zone, radius, .. } = run.drill().goal {
            let green = Color::new(0.35, 1.0, 0.5, 0.55 + 0.25 * pulse);
            out.guides.push(Guide::Ring {
                center: surface(zone, line_y),
                radius: radius as f32,
                width: 0.006,
                color: green,
            });
        }
    }

    let Phase::Turn { placing_cue_ball, .. } = m.phase else { return out };

    if placing_cue_ball {
        let ok = m.game.can_place_cue_ball(cue);
        let color =
            if ok { Color::new(0.3, 1.0, 0.45, 0.9) } else { Color::new(1.0, 0.3, 0.25, 0.9) };
        out.guides.push(Guide::Ring {
            center: surface(cue, line_y),
            radius: r as f32 * 1.4,
            width: 0.004,
            color,
        });
        if m.game.ball_in_hand() == Some(BallInHand::Kitchen) {
            let x = table.head_string_x();
            let hw = table.spec.width / 2.0;
            out.guides.push(Guide::Line {
                from: surface(DVec2::new(x, -hw), line_y),
                to: surface(DVec2::new(x, hw), line_y),
                width: 0.003,
                color: Color::new(1.0, 1.0, 1.0, 0.35),
            });
        }
        return out;
    }

    // Pocket call for the 8-ball.
    if m.game.needs_called_pocket() {
        let pulse = 0.5 + 0.5 * (time * 4.0).sin();
        for id in PocketId::ALL {
            let called = m.called_pocket == Some(id);
            if called || m.called_pocket.is_none() {
                let alpha = if called { 1.0 } else { 0.3 + 0.5 * pulse };
                out.guides.push(Guide::Ring {
                    center: surface(scene.pocket_center(id), RAIL_HEIGHT + 0.004),
                    radius: scene.pocket_radius(id) + 0.018,
                    width: 0.008,
                    color: Color::new(1.0, 0.82, 0.25, alpha),
                });
            }
        }
    }

    if m.current_controller() != Controller::Human {
        return out;
    }

    let guide = m.guide_in_use();
    if guide == AimGuide::Off {
        return out;
    }
    // Aim line to the first contact: a ball, a cushion or a pocket's
    // facing, or the hole of a pocket (the cue ball would go in).
    let dir = DVec2::from_angle(m.aim.yaw);
    let Some(Stop { t, target, in_pocket }) = first_stop(&m.game, cue, dir) else {
        return out;
    };
    let white = Color::new(1.0, 1.0, 1.0, 0.5);
    let start = cue + dir * r;
    if guide == AimGuide::Short {
        let len = t.min(0.35);
        if len > r + 0.005 {
            out.guides.push(Guide::Line {
                from: surface(start, line_y),
                to: surface(cue + dir * len, line_y),
                width: 0.0025,
                color: white,
            });
        }
        return out;
    }
    let ghost = cue + dir * t;
    // A fine, discreet line to a see-through "ghost" ball where the cue ball
    // will make contact. Nothing of it is drawn inside either ball: when
    // the contact is closer than a ball's width there is no line, and when
    // the cue ball is already touching what it is aimed at, no ghost
    // either (it would sit on the cue ball itself).
    if t > 2.0 * r {
        out.guides.push(Guide::Line {
            from: surface(start, line_y),
            to: surface(if in_pocket { ghost } else { ghost - dir * r }, line_y),
            width: 0.0012,
            color: Color::new(1.0, 1.0, 1.0, 0.42),
        });
    }
    if in_pocket || t < 0.004 {
        return out;
    }
    out.ghost = Some(surface(ghost, r as f32));
    if guide != AimGuide::Full {
        return out;
    }
    if let Some(id) = target {
        let obj = m.game.ball(id).xy();
        let n = (obj - ghost).normalize();
        let legal = m.game.legal_targets().contains(&id);
        let color =
            if legal { Color::new(0.45, 1.0, 0.55, 0.9) } else { Color::new(1.0, 0.4, 0.35, 0.9) };
        out.guides.push(Guide::Line {
            from: surface(obj + n * r, line_y),
            to: surface(obj + n * 0.3, line_y),
            width: 0.0025,
            color,
        });
        let tangent = dir - n * dir.dot(n);
        if tangent.length() > 1e-3 {
            let tangent = tangent.normalize();
            out.guides.push(Guide::Line {
                from: surface(ghost + tangent * r, line_y),
                to: surface(ghost + tangent * 0.15, line_y),
                width: 0.002,
                color: Color::new(1.0, 1.0, 1.0, 0.35),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guide stops where the simulation would: on a cushion's nose, on
    /// a pocket's facing, over a hole, or at once against a touching ball.
    #[test]
    fn the_guide_stops_where_the_ball_would() {
        use snook_legend_core::{EightBallGame, PocketId};
        let mut g = EightBallGame::new(1, 0);
        let (spec, r) = (g.table().spec, g.table().spec.ball_radius);
        let cue = DVec2::new(-0.4, 0.1);
        g.set_up_test_position(&[(BallId::CUE, cue), (BallId(3), cue + DVec2::new(2.0 * r, 0.0))]);
        // Straight at a long cushion: the nose is met a little before the
        // ball's own radius (it sits above the ball's widest point).
        let s = first_stop(&g, cue, DVec2::Y).unwrap();
        let gap = spec.width / 2.0 - cue.y - s.t;
        assert!(s.target.is_none() && !s.in_pocket && gap < r && gap > r - 0.003, "{gap}");
        // Into each pocket along its axis: over the lip, no ghost ball.
        for id in PocketId::ALL {
            let p = *g.table().pocket(id);
            let from = p.mouth_center - p.outward * 0.4;
            let s = first_stop(&g, from, p.outward).unwrap();
            assert!(s.in_pocket, "{id:?}");
            let end = from + p.outward * s.t;
            assert!((end.distance(p.hole_center) - p.hole_radius).abs() < 1e-6, "{id:?}");
        }
        // Past a corner pocket's mouth, at its facing: stopped before the
        // rectangle of the cushion lines would say.
        let p = *g.table().pocket(PocketId::FootLeft);
        let from = p.mouth_center - p.outward * 0.3 + p.outward.perp() * 0.03;
        let dir = (p.mouth_center + p.outward.perp() * 0.075 - from).normalize();
        let s = first_stop(&g, from, dir).unwrap();
        assert!(!s.in_pocket && s.target.is_none());
        let end = from + dir * s.t;
        assert!(end.x < spec.length / 2.0 && end.y < spec.width / 2.0, "{end}");
        // Frozen to a ball and aimed at it: stopped at once, on that ball.
        let s = first_stop(&g, cue, DVec2::X).unwrap();
        assert!(s.t < 1e-6 && s.target == Some(BallId(3)));
        // Aimed away from it: the far cushion.
        assert!(first_stop(&g, cue, -DVec2::X).unwrap().target.is_none());
    }

    #[test]
    fn ray_hits_ball_in_front() {
        let r = 0.028575;
        let t = ray_ball(DVec2::ZERO, DVec2::X, DVec2::new(0.5, 0.0), r).unwrap();
        assert!((t - (0.5 - 2.0 * r)).abs() < 1e-9);
        assert!(ray_ball(DVec2::ZERO, DVec2::X, DVec2::new(-0.5, 0.0), r).is_none());
        assert!(ray_ball(DVec2::ZERO, DVec2::X, DVec2::new(0.5, 0.2), r).is_none());
    }
}
