//! The shop's showroom: an item looked at closely before it is bought.
//!
//! The item stands alone on a dark stage under the table's lamps (a cue
//! floating over its shadow, a cube of chalk with a cue chalking it, a ball
//! set racked, a table on its own): the camera turns slowly round it until
//! the player takes it, and then goes where they please, as at the table
//! (drag with the right button to turn round it, smoothly and never from
//! under the floor or the table; the wheel to come closer to what is under
//! the pointer; a double click to look closely at a point). A play
//! button turns it again. Ready-made views take the camera to the parts
//! worth seeing (a cue's tip, its joint, its butt; a table's pocket, its
//! leg), a ball of a set can be picked to see it close up, turning, and a
//! table or a cloth can be seen in any of the halls. Everything can be
//! hidden but the item.

use macroquad::prelude::*;
use snook_legend_core::BallId;
use snook_legend_proto::shop::{BallFinish, Category, HallStyle, ITEMS, Item, Look, TableStyle};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_circle_lines, draw_line, draw_rectangle};

use crate::config::Config;
use crate::coords::TABLE_HEIGHT;
use crate::gfx::SceneCamera;
use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::scene::{CHALK_SIZE, Looks, ShowCue, Showcase};
use crate::ui::{BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};

/// What kind of thing is shown (an item's category, for the showroom).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Cue,
    Chalk,
    Balls,
    Cloth,
    Table,
    Hall,
}

impl Kind {
    fn of(category: Category) -> Option<Kind> {
        Some(match category {
            Category::Cue => Kind::Cue,
            Category::Chalk => Kind::Chalk,
            Category::Balls => Kind::Balls,
            Category::Cloth => Kind::Cloth,
            Category::Table => Kind::Table,
            Category::Hall => Kind::Hall,
            _ => return None,
        })
    }
}

/// Where the item is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// Alone on the showroom's stage.
    Studio,
    /// On the player's own table, as it will look in a game.
    Table,
    /// In a hall (a table or a cloth).
    Hall(HallStyle),
}

/// Where the camera is: turned `yaw` round the vertical and raised
/// `pitch` (radians), `dist` away from `target`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct View {
    yaw: f32,
    pitch: f32,
    dist: f32,
    target: Vec3,
}

impl View {
    fn eye(&self) -> Vec3 {
        let (sp, cp) = self.pitch.sin_cos();
        self.target + vec3(cp * self.yaw.cos(), sp, cp * self.yaw.sin()) * self.dist
    }
}

/// How far the camera may go for what is shown.
struct Limits {
    dist: (f32, f32),
    pitch: (f32, f32),
    low: Vec3,
    high: Vec3,
}

/// What the showroom asks of the shop.
pub enum ShowroomAction {
    None,
    Close,
    /// Buy the item (the shop's confirmation).
    Buy,
}

/// How fast the stage turns by itself (radians a second).
const TURN_SPEED: f32 = 0.22;
/// How fast a ball looked at closely turns on itself.
const BALL_SPEED: f32 = 0.7;
/// How long the hints stay at first (seconds), and after the first touch.
const HINT_SECS: f64 = 9.0;
/// How high a ball looked at closely floats over the set (in radii).
const FLOAT: f32 = 2.8;
/// The height a cue floats at over the stage.
const CUE_HEIGHT: f32 = 0.09;
/// The length of the cue mesh (see `scene::cue_mesh`).
const CUE_LENGTH: f32 = 1.48;
/// The order the balls are racked in on the stage, row by row from the apex.
const RACK: [[u8; 5]; 5] =
    [[1, 0, 0, 0, 0], [9, 2, 0, 0, 0], [3, 8, 10, 0, 0], [11, 4, 12, 5, 0], [6, 13, 7, 14, 15]];

pub struct Showroom {
    pub code: &'static str,
    item: &'static Item,
    kind: Kind,
    setting: Setting,
    settings: Vec<Setting>,
    /// Half the table's playing area (m) and a ball's radius.
    half: Vec2,
    ball_r: f32,
    view: View,
    /// A view the camera is easing to, and how fast.
    goal: Option<(View, f32)>,
    /// The stage turns by itself (or, close to a ball, the ball does).
    spinning: bool,
    /// The ready-made view chosen last (none once the camera is moved).
    preset: Option<usize>,
    /// The ball looked at closely, how far it is raised (0..1) and its turn.
    ball: Option<usize>,
    lift: f32,
    ball_turn: f32,
    hide_ui: bool,
    /// A drag going on: panning (not turning), the last pointer position
    /// (pixels), and whether it has moved enough to be a drag.
    drag: Option<(bool, Vec2, bool)>,
    press_at: Vec2,
    last_click: Option<(Vec2, f64)>,
    opened: f64,
    touched: Option<f64>,
    /// The camera of the last frame (for picking under the pointer).
    cam: (Vec3, Vec3, f32),
    /// The rectangles of the interface this frame (the stage under them
    /// doesn't take the pointer).
    panels: Vec<Rect>,
}

impl Showroom {
    /// The showroom for the item `code` (none for an item with nothing to
    /// show in 3D). `table` gives the table's size.
    pub fn open(code: &str, table: &snook_legend_core::Table) -> Option<Showroom> {
        let item = snook_legend_proto::shop::item(code)?;
        let kind = Kind::of(item.category)?;
        let settings = match kind {
            Kind::Cue | Kind::Chalk | Kind::Balls => vec![Setting::Studio, Setting::Table],
            Kind::Cloth | Kind::Table => std::iter::once(Setting::Studio)
                .chain(ITEMS.iter().filter_map(|i| match i.look {
                    Look::Hall(h) => Some(Setting::Hall(h)),
                    _ => None,
                }))
                .collect(),
            Kind::Hall => vec![Setting::Table],
        };
        let half = vec2(table.spec.length as f32 / 2.0, table.spec.width as f32 / 2.0);
        let mut s = Showroom {
            code: item.code,
            item,
            kind,
            setting: settings[0],
            settings,
            half,
            ball_r: table.spec.ball_radius as f32,
            view: View { yaw: 0.0, pitch: 0.3, dist: 2.0, target: Vec3::ZERO },
            goal: None,
            spinning: true,
            preset: Some(0),
            ball: None,
            lift: 0.0,
            ball_turn: 0.0,
            hide_ui: false,
            drag: None,
            press_at: Vec2::ZERO,
            last_click: None,
            opened: now(),
            touched: None,
            cam: (Vec3::ZERO, Vec3::ONE, 0.6),
            panels: Vec::new(),
        };
        s.view = s.presets()[0].1;
        // It comes in from a little further and higher, settling.
        let settle = s.view;
        s.view.dist *= 1.35;
        s.view.pitch += 0.12;
        s.view.yaw -= 0.5;
        s.goal = Some((settle, 2.2));
        Some(s)
    }

    /// Development aid (screenshots): SNOOK_LEGEND_SHOWROOM="view=<n>,
    /// ball=<n>,setting=<n>,still,bare" picks a view, a ball of a set, a
    /// setting, stops the turning and hides the interface; the camera is
    /// there at once.
    pub fn dev(&mut self) {
        let Some(spec) = crate::dev::var("SNOOK_LEGEND_SHOWROOM") else { return };
        for part in spec.split(',') {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            let n: usize = value.parse().unwrap_or(0);
            match key.trim() {
                "setting" => {
                    if let Some(s) = self.settings.get(n).copied() {
                        self.set_setting(s);
                    }
                }
                "view" => self.choose_preset(n),
                "ball" => self.look_at_ball(n),
                "still" => self.spinning = false,
                "bare" => self.hide_ui = true,
                _ => {}
            }
        }
        if let Some((goal, _)) = self.goal.take() {
            self.view = goal;
        }
        self.lift = if self.ball.is_some() { 1.0 } else { 0.0 };
    }

    /// The item alone on the stage (not on a table in a hall).
    pub fn in_studio(&self) -> bool {
        self.setting == Setting::Studio && self.kind != Kind::Hall
    }

    /// A hall is shown around the table (even with the dark stage chosen).
    pub fn shows_hall(&self) -> bool {
        matches!(self.setting, Setting::Hall(_)) || self.kind == Kind::Hall
    }

    /// The cloth, balls, table and hall on the stage: the item's, and the
    /// player's own for the rest (the hall of the setting chosen).
    pub fn looks(&self, config: &Config) -> Looks {
        let code = self.code;
        let pick = |prefix: &str, own: &str| -> String {
            if code.starts_with(prefix) { code.to_string() } else { own.to_string() }
        };
        let mut looks = Looks::of(&pick("cloth", &config.cloth), &pick("balls", &config.balls))
            .themed(&pick("table", &config.table), &pick("hall", &config.hall));
        if let Setting::Hall(h) = self.setting {
            looks.hall = h;
        }
        looks
    }

    /// On the player's table: the cue (design, chalk) lined up behind the
    /// cue ball, for a cue or a chalk.
    pub fn cue_on_table(&self, config: &Config) -> Option<(String, String)> {
        if self.setting != Setting::Table {
            return None;
        }
        match self.kind {
            Kind::Cue => Some((self.code.to_string(), config.chalk.clone())),
            Kind::Chalk => Some((config.cue.clone(), self.code.to_string())),
            _ => None,
        }
    }

    /// What stands on the stage (see [`Scene::draw_showcase`]): `level` is
    /// the level of the cue shown (its marks), `own_level` that of the
    /// player's own cue (chalking a chalk).
    ///
    /// [`Scene::draw_showcase`]: crate::scene::Scene::draw_showcase
    pub fn showcase(&self, config: &Config, level: u8, own_level: u8) -> Showcase {
        let floor = TABLE_HEIGHT;
        match self.kind {
            Kind::Cue => Showcase {
                floor,
                floor_radius: 8.0,
                cue: Some(ShowCue {
                    design: self.code.to_string(),
                    chalk: config.chalk.clone(),
                    level,
                    model: Mat4::from_rotation_translation(
                        Quat::from_rotation_arc(Vec3::Y, -Vec3::X),
                        vec3(CUE_LENGTH / 2.0, floor + CUE_HEIGHT, 0.0),
                    ),
                }),
                ..Default::default()
            },
            Kind::Chalk => {
                // The player's cue chalking it: its tip in the hollow, the
                // cue rising away behind.
                let up = chalk_cue_direction();
                // (The tip's face, tilted, just touching the hollow.)
                let tip = vec3(0.0, floor + CHALK_SIZE + 0.0045, 0.0);
                Showcase {
                    floor,
                    floor_radius: 8.0,
                    chalk: Some((
                        self.code.to_string(),
                        Mat4::from_translation(vec3(0.0, floor, 0.0)),
                    )),
                    cue: Some(ShowCue {
                        design: config.cue.clone(),
                        chalk: self.code.to_string(),
                        level: own_level,
                        model: Mat4::from_rotation_translation(
                            Quat::from_rotation_arc(Vec3::Y, up),
                            tip,
                        ),
                    }),
                    ..Default::default()
                }
            }
            Kind::Balls => Showcase {
                floor,
                floor_radius: 8.0,
                balls: self.ball_poses(),
                ..Default::default()
            },
            Kind::Cloth | Kind::Table | Kind::Hall => Showcase {
                floor: 0.0,
                floor_radius: 12.0,
                table: true,
                balls: self.table_rack(),
                ..Default::default()
            },
        }
    }

    /// The set racked on the stage, the apex towards the camera at first,
    /// the cue ball in front; every number up, reading from the front.
    /// The ball looked at closely rises and turns on itself.
    fn ball_poses(&self) -> Vec<(usize, Mat4)> {
        let face_front = Quat::from_mat3(&Mat3::from_cols(Vec3::Z, Vec3::X, Vec3::Y));
        let mut poses = Vec::with_capacity(16);
        for (n, p) in self.ball_spots() {
            let mut at = p;
            let mut rot = face_front;
            if self.ball == Some(n as usize) {
                let k = self.lift * self.lift * (3.0 - 2.0 * self.lift);
                at.y += k * self.ball_r * FLOAT;
                // Its number turned out towards the side and a little up,
                // going round: upright, readable, then its stripe.
                let tilt: f32 = 0.32;
                let a = self.ball_turn;
                let z = vec3(a.cos() * tilt.cos(), tilt.sin(), a.sin() * tilt.cos()).normalize();
                let y = (Vec3::Y - z * z.y).normalize();
                let turned = Quat::from_mat3(&Mat3::from_cols(y.cross(z), y, z));
                rot = face_front.slerp(turned, k);
            }
            poses.push((n as usize, Mat4::from_rotation_translation(rot, at)));
        }
        poses
    }

    /// Where each ball of the set stands on the stage (its centre).
    fn ball_spots(&self) -> Vec<(u8, Vec3)> {
        let r = self.ball_r;
        let spacing = 2.0 * r + 0.0004;
        let row_dx = spacing * 3f32.sqrt() / 2.0;
        let y = TABLE_HEIGHT + r;
        let apex = -2.0 * row_dx;
        // The cue ball beside the rack, towards the front.
        let mut spots = vec![(0u8, vec3(apex + 0.8 * row_dx, y, -0.16))];
        for (row, numbers) in RACK.iter().enumerate() {
            for (k, n) in numbers.iter().take(row + 1).enumerate() {
                let z = (k as f32 - row as f32 / 2.0) * spacing;
                spots.push((*n, vec3(apex + row as f32 * row_dx, y, z)));
            }
        }
        spots
    }

    /// The player's set racked on the table at its foot spot, the cue ball
    /// at the head spot (a table or a cloth on the stage).
    fn table_rack(&self) -> Vec<(usize, Mat4)> {
        let table = snook_legend_core::Table::default();
        let rest = crate::coords::ball_rotation(::glam::DQuat::IDENTITY);
        snook_legend_core::rules::rack(&table, 2024)
            .iter()
            .map(|b| {
                (
                    b.id.index(),
                    Mat4::from_rotation_translation(rest, crate::coords::to_world(b.position)),
                )
            })
            .collect()
    }

    /// The ready-made views for what is shown, and where it is shown.
    fn presets(&self) -> Vec<(&'static str, View)> {
        let h = TABLE_HEIGHT;
        let (l, w) = (self.half.x, self.half.y);
        let v = |yaw: f32, pitch: f32, dist: f32, target: Vec3| View { yaw, pitch, dist, target };
        let hall = |label| (label, v(3.95, 0.16, 4.1, vec3(0.0, 1.05, 0.0)));
        let table = |label| (label, v(3.95, 0.45, 3.6, vec3(0.0, h - 0.12, 0.0)));
        match (self.kind, self.setting) {
            (Kind::Hall, _) => vec![
                hall(t("Hall")),
                table(t("Table")),
                (t("Corner"), v(0.65, 0.5, 1.0, vec3(l, h, -w))),
            ],
            (Kind::Cloth | Kind::Table, Setting::Hall(_)) => vec![
                table(t("Table")),
                hall(t("Hall")),
                (t("Corner"), v(0.65, 0.5, 1.0, vec3(l, h, -w))),
            ],
            (Kind::Table, _) => vec![
                table(t("Table")),
                (t("Corner"), v(0.65, 0.48, 0.95, vec3(l, h, -w))),
                (t("Side"), v(1.9, 0.38, 0.85, vec3(0.0, h, w))),
                (t("Leg"), v(0.72, 0.36, 1.25, vec3(l - 0.2, 0.42, w - 0.12))),
            ],
            (Kind::Cloth, _) => vec![
                table(t("Table")),
                // The rack on the cloth, close: its colour under the balls.
                (t("Close-up"), v(3.55, 0.5, 0.5, vec3(l / 2.0 - 0.06, h + 0.02, 0.05))),
                (t("Cushion"), v(1.95, 0.32, 0.5, vec3(0.62, h + 0.02, w))),
            ],
            (Kind::Cue, Setting::Studio) => {
                let y = h + CUE_HEIGHT;
                let tip = CUE_LENGTH / 2.0;
                vec![
                    (t("Whole"), v(1.22, 0.3, 1.95, vec3(0.0, y, 0.0))),
                    (t("Tip"), v(1.0, 0.24, 0.2, vec3(tip - 0.05, y, 0.0))),
                    (t("Joint"), v(1.3, 0.28, 0.33, vec3(tip - 0.81, y, 0.0))),
                    (t("Butt"), v(1.62, 0.32, 0.62, vec3(tip - 1.16, y, 0.0))),
                ]
            }
            (Kind::Chalk, Setting::Studio) => {
                // Just above the tip: the chalk it took, and the cube's top.
                let tip = chalk_cue_direction() * 0.012 + vec3(0.0, h + CHALK_SIZE + 0.007, 0.0);
                vec![
                    (t("Chalk"), v(0.85, 0.36, 0.2, vec3(0.0, h + 0.016, 0.0))),
                    (t("Top"), v(0.85, 1.2, 0.13, vec3(0.0, h + CHALK_SIZE, 0.0))),
                    (t("Tip"), v(0.5, 0.22, 0.085, tip)),
                ]
            }
            (Kind::Balls, Setting::Studio) => {
                // (A little below the balls: they sit higher on the screen,
                // clear of the controls.)
                let at = vec3(0.0, h - 0.025, -0.02);
                vec![
                    (t("The set"), v(3.5, 0.52, 0.72, at)),
                    (t("From above"), v(3.5, 1.38, 0.7, vec3(0.0, h, -0.02))),
                    (t("Low angle"), v(3.3, 0.1, 0.55, vec3(0.0, h + 0.01, -0.02))),
                ]
            }
            // On the player's own table.
            (Kind::Cue | Kind::Chalk, _) => vec![
                (t("Cue"), v(2.25, 0.3, 1.3, vec3(-l / 2.0 - 0.45, h + 0.05, 0.0))),
                table(t("Table")),
            ],
            (Kind::Balls, _) => vec![
                (t("The set"), v(3.45, 0.5, 0.8, vec3(l / 2.0 + 0.1, h + self.ball_r, 0.0))),
                table(t("Table")),
            ],
        }
    }

    fn limits(&self) -> Limits {
        let h = TABLE_HEIGHT;
        let lim = |dist: (f32, f32), pitch: (f32, f32), low: Vec3, high: Vec3| Limits {
            dist,
            pitch,
            low,
            high,
        };
        match (self.kind, self.setting) {
            (Kind::Cue, Setting::Studio) => {
                lim((0.07, 3.0), (0.03, 1.5), vec3(-0.85, h, -0.4), vec3(0.85, h + 0.4, 0.4))
            }
            (Kind::Chalk, Setting::Studio) => {
                lim((0.05, 0.9), (0.03, 1.5), vec3(-0.3, h, -0.3), vec3(0.3, h + 0.5, 0.3))
            }
            (Kind::Balls, Setting::Studio) => {
                lim((0.07, 1.8), (0.03, 1.5), vec3(-0.4, h - 0.05, -0.3), vec3(0.4, h + 0.3, 0.3))
            }
            (Kind::Cloth | Kind::Table, Setting::Studio) => {
                lim((0.15, 6.0), (0.12, 1.45), vec3(-1.8, 0.05, -1.1), vec3(1.8, 1.3, 1.1))
            }
            _ => lim((0.15, 4.4), (0.12, 1.45), vec3(-2.5, 0.1, -2.0), vec3(2.5, 2.2, 2.0)),
        }
    }

    /// Keeps a view within what may be seen: never from under the stage's
    /// floor, nor from under the table's top (a table, a cloth, a hall).
    fn clamp(&self, mut v: View) -> View {
        let l = self.limits();
        v.dist = v.dist.clamp(l.dist.0, l.dist.1);
        v.pitch = v.pitch.clamp(l.pitch.0, l.pitch.1);
        v.target = v.target.clamp(l.low, l.high);
        let need = self.lowest_eye() - v.target.y;
        if need > 0.0 {
            let least = (need / v.dist).min(0.99).asin();
            v.pitch = v.pitch.max(least).min(l.pitch.1);
        }
        v
    }

    /// How low the camera may come: a little over the stage's floor for
    /// what stands on it, over the table's top wherever there is a table.
    fn lowest_eye(&self) -> f32 {
        match (self.kind, self.setting) {
            (Kind::Cue | Kind::Chalk | Kind::Balls, Setting::Studio) => TABLE_HEIGHT + 0.02,
            _ => TABLE_HEIGHT + 0.12,
        }
    }

    /// The camera for this frame: eases towards a view chosen, and turns
    /// the stage (or the ball looked at) while it plays.
    pub fn camera(&mut self, dt: f32) -> SceneCamera {
        let dt = dt.min(0.1);
        // The ball looked at rises (or settles back) and turns.
        let lift_goal = if self.ball.is_some() { 1.0 } else { 0.0 };
        self.lift += (lift_goal - self.lift) * (1.0 - (-dt * 5.0).exp());
        if self.ball.is_none() && self.lift < 0.01 {
            self.lift = 0.0;
        }
        let close_to_ball = self.ball.is_some();
        if self.spinning && self.drag.is_none() {
            if close_to_ball {
                self.ball_turn += BALL_SPEED * dt;
            } else {
                self.view.yaw += TURN_SPEED * dt;
                if let Some((g, _)) = self.goal.as_mut() {
                    g.yaw += TURN_SPEED * dt;
                }
            }
        }
        if let Some((goal, rate)) = self.goal {
            let k = 1.0 - (-dt * rate).exp();
            let mut d_yaw = (goal.yaw - self.view.yaw).rem_euclid(std::f32::consts::TAU);
            if d_yaw > std::f32::consts::PI {
                d_yaw -= std::f32::consts::TAU;
            }
            self.view.yaw += d_yaw * k;
            self.view.pitch += (goal.pitch - self.view.pitch) * k;
            // The distance eases in proportion (a long way out, a short
            // way in, at the same pace).
            self.view.dist *= (goal.dist / self.view.dist).powf(k);
            self.view.target += (goal.target - self.view.target) * k;
            let close = d_yaw.abs() < 1e-3
                && (goal.pitch - self.view.pitch).abs() < 1e-3
                && (goal.dist / self.view.dist - 1.0).abs() < 1e-3
                && goal.target.distance(self.view.target) < 1e-4;
            if close {
                self.goal = None;
            }
        }
        self.view = self.clamp(self.view);
        let fovy = if self.kind == Kind::Hall || matches!(self.setting, Setting::Hall(_)) {
            48f32.to_radians()
        } else {
            34f32.to_radians()
        };
        let eye = self.view.eye();
        self.cam = (eye, self.view.target, fovy);
        SceneCamera { eye, target: self.view.target, up: Vec3::Y, fovy }
    }

    fn last_cam(&self) -> SceneCamera {
        let (eye, target, fovy) = self.cam;
        SceneCamera { eye, target, up: Vec3::Y, fovy }
    }

    /// Takes the camera to a view, easing.
    fn go(&mut self, view: View, rate: f32) {
        let view = self.clamp(view);
        self.goal = Some((view, rate));
    }

    /// The camera was moved by hand: the stage stops turning.
    fn touch(&mut self) {
        self.spinning = false;
        self.preset = None;
        self.touched.get_or_insert(now());
    }

    fn choose_preset(&mut self, k: usize) {
        let presets = self.presets();
        if let Some((_, view)) = presets.get(k) {
            let mut view = *view;
            // Turning: arrive from where the stage has turned to.
            view.yaw = nearest_turn(self.view.yaw, view.yaw);
            self.ball = None;
            self.preset = Some(k);
            self.go(view, 3.2);
        }
    }

    fn look_at_ball(&mut self, n: usize) {
        let Some((_, at)) = self.ball_spots().into_iter().find(|(b, _)| *b as usize == n) else {
            return;
        };
        if self.ball != Some(n) {
            // Its number starts out facing the camera.
            let eye = self.view.eye();
            self.ball_turn = (eye.z - at.z).atan2(eye.x - at.x);
        }
        self.ball = Some(n);
        self.preset = None;
        self.spinning = true;
        // Level with it, the set below: it stands out against the dark.
        let target = at + Vec3::Y * self.ball_r * FLOAT;
        self.go(View { yaw: self.view.yaw, pitch: 0.1, dist: self.ball_r * 7.0, target }, 3.6);
    }

    fn set_setting(&mut self, setting: Setting) {
        if self.setting == setting {
            return;
        }
        let was_hall = matches!(self.setting, Setting::Hall(_));
        self.setting = setting;
        self.ball = None;
        // From hall to hall, the camera stays where it is.
        if !(was_hall && matches!(setting, Setting::Hall(_))) {
            self.choose_preset(0);
        }
    }

    fn zoom_by(&mut self, factor: f32, toward: Option<Vec2>) {
        let base = self.goal.map_or(self.view, |(g, _)| g);
        let mut view = base;
        view.dist = (base.dist * factor).clamp(self.limits().dist.0, self.limits().dist.1);
        // The point under the pointer stays under it.
        if let Some(px) = toward {
            let cam = self.last_cam();
            let (o, d) = cam.ray(px);
            let forward = (base.target - base.eye()).normalize_or_zero();
            let denom = d.dot(forward);
            if denom.abs() > 1e-4 {
                let s = (base.target - o).dot(forward) / denom;
                if s > 0.0 {
                    let p = o + d * s;
                    let kept = 1.0 - view.dist / base.dist;
                    view.target = base.target + (p - base.target) * kept;
                }
            }
        }
        self.go(view, 16.0);
    }

    /// What lies under the pointer (pixels): a ball of the set (its
    /// number), or a point worth looking at closely.
    fn pick(&self, px: Vec2) -> Option<Pick> {
        let cam = self.last_cam();
        let (o, d) = cam.ray(px);
        match (self.kind, self.setting) {
            (Kind::Balls, Setting::Studio) => {
                let r = self.ball_r;
                let mut best: Option<(f32, u8)> = None;
                for (n, mut c) in self.ball_spots() {
                    if self.ball == Some(n as usize) {
                        let k = self.lift * self.lift * (3.0 - 2.0 * self.lift);
                        c.y += k * r * FLOAT;
                    }
                    let oc = o - c;
                    let b = oc.dot(d);
                    let disc = b * b - (oc.dot(oc) - r * r);
                    if disc >= 0.0 {
                        let s = -b - disc.sqrt();
                        if s > 0.0 && best.is_none_or(|(bs, _)| s < bs) {
                            best = Some((s, n));
                        }
                    }
                }
                best.map(|(_, n)| Pick::Ball(n as usize))
            }
            (Kind::Cue, Setting::Studio) => {
                // The nearest point of the cue's axis to the pointer's ray.
                let y = TABLE_HEIGHT + CUE_HEIGHT;
                let (a, b) = (vec3(CUE_LENGTH / 2.0, y, 0.0), vec3(-CUE_LENGTH / 2.0, y, 0.0));
                let (p, gap) = closest_on_segment(o, d, a, b);
                (gap < 0.03).then_some(Pick::Point(p, 0.3))
            }
            (Kind::Chalk, Setting::Studio) => {
                let c = vec3(0.0, TABLE_HEIGHT + CHALK_SIZE * 0.6, 0.0);
                let s = (c - o).dot(d);
                (s > 0.0 && (o + d * s).distance(c) < 0.05).then_some(Pick::Point(c, 0.12))
            }
            _ => {
                // A point on the cloth (or the floor).
                let plane = if self.kind == Kind::Hall { 0.0 } else { TABLE_HEIGHT };
                if d.y.abs() < 1e-4 {
                    return None;
                }
                let s = (plane - o.y) / d.y;
                let p = o + d * s;
                let inside = p.x.abs() < self.half.x + 0.3 && p.z.abs() < self.half.y + 0.3;
                (s > 0.0 && inside).then_some(Pick::Point(p, 0.7))
            }
        }
    }

    /// The camera, from the pointer and the keyboard (over the stage only,
    /// not over the interface).
    fn input(&mut self, ui: &Ui) {
        let mouse_px = Vec2::from(mouse_position());
        let over_ui = self.panels.iter().any(|r| r.contains(ui.mouse()));
        // As at the table: the right button turns the camera round (the
        // left one too, here: it has nothing to aim). The camera follows
        // the pointer smoothly, and never goes under the floor or the table.
        let left = is_mouse_button_pressed(MouseButton::Left);
        let right = is_mouse_button_pressed(MouseButton::Right);
        if (left || right) && !over_ui && self.drag.is_none() {
            self.drag = Some((right, mouse_px, false));
            self.press_at = mouse_px;
        }
        if let Some((with_right, last, moved)) = self.drag {
            let button = if with_right { MouseButton::Right } else { MouseButton::Left };
            let down = is_mouse_button_down(button);
            let delta = mouse_px - last;
            let moved = moved || mouse_px.distance(self.press_at) > 4.0;
            if moved && delta != Vec2::ZERO {
                if !self.drag.is_some_and(|(_, _, m)| m) {
                    self.touch();
                }
                self.turn_by(delta.x * 0.005, delta.y * 0.004);
            }
            if down {
                self.drag = Some((with_right, mouse_px, moved));
            } else {
                self.drag = None;
                if !moved && !with_right {
                    self.clicked(mouse_px);
                }
            }
        }
        if !over_ui {
            let (_, wheel) = mouse_wheel();
            if wheel != 0.0 {
                self.touch_zoom();
                self.zoom_by(if wheel > 0.0 { 0.86 } else { 1.0 / 0.86 }, Some(mouse_px));
            }
        }
        if ui.wants_keyboard {
            return;
        }
        let dt = get_frame_time().min(0.1);
        let turn = |k: KeyCode| is_key_down(k) as u8 as f32;
        let yaw = turn(KeyCode::Right) - turn(KeyCode::Left);
        let pitch = turn(KeyCode::Up) - turn(KeyCode::Down);
        if yaw != 0.0 || pitch != 0.0 {
            self.touch();
            self.turn_by(-yaw * 1.3 * dt, pitch * 0.9 * dt);
        }
        let zoom_in = is_key_down(KeyCode::Equal) || is_key_down(KeyCode::KpAdd);
        let zoom_out = is_key_down(KeyCode::Minus) || is_key_down(KeyCode::KpSubtract);
        if zoom_in != zoom_out {
            self.touch_zoom();
            let rate = 2.2f32.powf(dt);
            self.zoom_by(if zoom_in { 1.0 / rate } else { rate }, None);
        }
        if is_key_pressed(KeyCode::Space) {
            self.toggle_spin();
        }
        if is_key_pressed(KeyCode::R) {
            self.reset();
        }
        if is_key_pressed(KeyCode::H) {
            self.hide_ui = !self.hide_ui;
        }
        let digits = [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4];
        for (k, key) in digits.iter().enumerate() {
            if is_key_pressed(*key) && k < self.presets().len() {
                self.choose_preset(k);
            }
        }
        if self.settings.len() > 1 {
            if is_key_pressed(KeyCode::LeftBracket) {
                self.step_setting(-1);
            }
            if is_key_pressed(KeyCode::RightBracket) {
                self.step_setting(1);
            }
        }
    }

    /// Turns the camera round what it looks at: the view it is easing to
    /// moves, and the camera follows it smoothly.
    fn turn_by(&mut self, yaw: f32, pitch: f32) {
        let mut view = self.goal.map_or(self.view, |(g, _)| g);
        view.yaw += yaw;
        view.pitch += pitch;
        self.goal = Some((self.clamp(view), 9.0));
    }

    /// Zooming stops the stage turning, but keeps the ball looked at.
    fn touch_zoom(&mut self) {
        if self.ball.is_none() {
            self.spinning = false;
        }
        self.preset = None;
        self.touched.get_or_insert(now());
    }

    /// A click on the stage (no drag): a ball of the set is looked at; a
    /// second click soon after on the same spot looks closely there.
    fn clicked(&mut self, px: Vec2) {
        let now = now();
        let double =
            self.last_click.is_some_and(|(at, when)| now - when < 0.4 && at.distance(px) < 8.0);
        self.last_click = Some((px, now));
        match self.pick(px) {
            Some(Pick::Ball(n)) => self.look_at_ball(n),
            Some(Pick::Point(p, dist)) if double => {
                self.touch();
                let view = View { dist: self.view.dist.min(dist), target: p, ..self.view };
                self.go(view, 4.0);
            }
            _ if double && self.ball.is_some() => self.choose_preset(0),
            _ => {}
        }
    }

    fn toggle_spin(&mut self) {
        self.spinning = !self.spinning;
        self.touched.get_or_insert(now());
    }

    fn reset(&mut self) {
        self.choose_preset(0);
        self.spinning = true;
    }

    fn step_setting(&mut self, step: i32) {
        let n = self.settings.len() as i32;
        let at = self.settings.iter().position(|s| *s == self.setting).unwrap_or(0) as i32;
        let next = self.settings[(at + step).rem_euclid(n) as usize];
        self.set_setting(next);
    }

    /// The interface over the stage: the item and its price, the controls
    /// of the camera, the ready-made views, the setting; and the stage's
    /// own pointer and keys. `dressing`: the stage is still being made
    /// (a hall, a set's textures).
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        shop: &mut crate::shop::ShopScreen,
        online: &mut crate::online::Online,
        dressing: bool,
    ) -> ShowroomAction {
        let mut action = ShowroomAction::None;
        self.panels.clear();
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        if self.hide_ui {
            // Only the way back.
            let r = Rect::new(sw - 52.0, sh - 52.0, 40.0, 40.0);
            self.panels.push(r);
            if ui.icon_button_ex(r, Icon::Eye, t("Show the interface (H)"), true, true) {
                self.hide_ui = false;
            }
            if ui.escape() {
                self.hide_ui = false;
            }
            self.input(ui);
            return action;
        }

        // --- The item, top left ----------------------------------------
        let back = Rect::new(18.0, 18.0, 44.0, 44.0);
        self.panels.push(back);
        if ui.icon_button(back, Icon::Back, t("Back to the shop (Esc)"), true) {
            action = ShowroomAction::Close;
        }
        let title_x = back.x + back.w + 14.0;
        let title_room = (sw * 0.42).min(sw - title_x - 360.0).max(160.0);
        let (name, size) = ui.fitted(t(self.item.name), 26.0, title_room, true);
        text_shadowed(ui, &name, title_x, 18.0, size, GOLD, true);
        let sub =
            format!("{} · {}", crate::shop::category_name(self.item.category), self.description());
        for (k, line) in ui.wrap(&sub, title_room, 13.0).iter().take(2).enumerate() {
            text_shadowed(ui, line, title_x, 50.0 + k as f32 * 17.0, 13.0, CREAM, false);
        }

        // --- What it costs, and the button, top right ---------------------
        let offer = Rect::new(sw - 18.0 - 330.0, 18.0, 330.0, 64.0);
        glass(offer);
        self.panels.push(offer);
        match shop.showroom_offer(ui, online, self.code, offer) {
            crate::shop::PreviewAction::Buy => action = ShowroomAction::Buy,
            crate::shop::PreviewAction::Close => action = ShowroomAction::Close,
            crate::shop::PreviewAction::None => {}
        }

        // --- The setting, under the offer ---------------------------------
        if self.settings.len() > 1 {
            let r = Rect::new(offer.x, offer.y + offer.h + 10.0, offer.w, 44.0);
            glass(r);
            self.panels.push(r);
            let prev = Rect::new(r.x + 6.0, r.y + 6.0, 32.0, 32.0);
            let next = Rect::new(r.x + r.w - 38.0, r.y + 6.0, 32.0, 32.0);
            let mid = r.y + r.h / 2.0;
            if ui.icon_button_ex(prev, Icon::ChevronLeft, t("Previous setting ([)"), true, true) {
                self.step_setting(-1);
            }
            if ui.icon_button_ex(next, Icon::ChevronRight, t("Next setting (])"), true, true) {
                self.step_setting(1);
            }
            let label = self.setting_name(self.setting);
            let caption = t("Setting");
            let cx = r.x + r.w / 2.0;
            ui.text_centered(caption, cx, mid - 17.0, 11.0, MUTED, false);
            let (label, size) = ui.fitted(&label, 15.0, r.w - 100.0, true);
            ui.text_centered(&label, cx, mid - 3.0, size, CREAM, true);
            if dressing {
                crate::ui::spinner(vec2(next.x - 14.0, mid), 7.0, GOLD);
            }
        } else if dressing {
            crate::ui::spinner(vec2(offer.x - 22.0, offer.y + offer.h / 2.0), 9.0, GOLD);
        }

        // --- The dock, bottom centre ---------------------------------------
        let presets = self.presets();
        let chip_h = 34.0;
        let chips: Vec<(String, f32)> = presets
            .iter()
            .map(|(label, _)| (label.to_string(), ui.text_size(label, 13.0, true).x + 26.0))
            .collect();
        let chips_w: f32 =
            chips.iter().map(|(_, w)| w).sum::<f32>() + 6.0 * (chips.len() as f32 - 1.0);
        let icon = 40.0;
        let zoom_w = 58.0;
        let dock_w = 14.0
            + icon
            + 12.0
            + icon
            + zoom_w
            + icon
            + 8.0
            + icon
            + 14.0
            + chips_w
            + 14.0
            + icon
            + 14.0;
        let dock_w = dock_w.min(sw - 20.0);
        let dock = Rect::new((sw - dock_w) / 2.0, sh - 18.0 - 56.0, dock_w, 56.0);
        glass(dock);
        self.panels.push(dock);
        let y = dock.y + 8.0;
        let mut x = dock.x + 14.0;
        let (play_icon, play_tip) = if self.spinning {
            (Icon::Pause, t("Stop turning (Space)"))
        } else {
            (Icon::Play, t("Turn (Space)"))
        };
        if ui.icon_button(Rect::new(x, y, icon, icon), play_icon, play_tip, true) {
            self.toggle_spin();
        }
        x += icon + 12.0;
        if ui.icon_button_ex(
            Rect::new(x, y, icon, icon),
            Icon::Minus,
            t("Zoom out (−)"),
            true,
            true,
        ) {
            self.touch_zoom();
            self.zoom_by(1.0 / 0.7, None);
        }
        x += icon;
        let zoom = presets[0].1.dist / self.view.dist;
        let zoom_text = format!("×{:.1}", zoom);
        let zr = Rect::new(x, y, zoom_w, icon);
        ui.text_centered(&zoom_text, zr.x + zr.w / 2.0, y + 12.0, 14.0, CREAM, true);
        x += zoom_w;
        if ui.icon_button_ex(Rect::new(x, y, icon, icon), Icon::Plus, t("Zoom in (+)"), true, true)
        {
            self.touch_zoom();
            self.zoom_by(0.7, None);
        }
        x += icon + 8.0;
        if ui.icon_button_ex(
            Rect::new(x, y, icon, icon),
            Icon::CameraReset,
            t("Back to the start (R)"),
            true,
            true,
        ) {
            self.reset();
        }
        x += icon + 14.0;
        draw_line(
            x - 7.0,
            y + 6.0,
            x - 7.0,
            y + icon - 6.0,
            1.0,
            Color::new(BORDER.r, BORDER.g, BORDER.b, 0.7),
        );
        for (k, (label, w)) in chips.iter().enumerate() {
            let r = Rect::new(x, y + (icon - chip_h) / 2.0, *w, chip_h);
            if chip(ui, r, label, self.preset == Some(k)) {
                self.choose_preset(k);
            }
            x += w + 6.0;
        }
        x += 8.0;
        draw_line(
            x - 7.0,
            y + 6.0,
            x - 7.0,
            y + icon - 6.0,
            1.0,
            Color::new(BORDER.r, BORDER.g, BORDER.b, 0.7),
        );
        if ui.icon_button_ex(
            Rect::new(x, y, icon, icon),
            Icon::Clean,
            t("Hide the interface (H)"),
            true,
            true,
        ) {
            self.hide_ui = true;
        }

        // --- A set's balls, over the dock -----------------------------------
        if self.kind == Kind::Balls && self.setting == Setting::Studio {
            self.ball_picker(ui, dock);
        }

        // --- What can be done, for a while --------------------------------
        let shown = match self.touched {
            None => now() - self.opened < HINT_SECS,
            Some(at) => now() - at < 2.5,
        };
        if shown {
            let alpha = match self.touched {
                None => ((HINT_SECS - (now() - self.opened)) as f32).clamp(0.0, 1.0),
                Some(at) => ((2.5 - (now() - at)) as f32).clamp(0.0, 1.0),
            };
            let hint = if self.kind == Kind::Balls && self.setting == Setting::Studio {
                t("Drag with the right button to turn · wheel to zoom · click a ball to look at it")
            } else {
                t(
                    "Drag with the right button to turn · wheel to zoom · double-click to look closer",
                )
            };
            let lines = ui.wrap(hint, (sw - 40.0).min(760.0), 13.0);
            let top = dock.y
                - 14.0
                - lines.len() as f32 * 18.0
                - if self.kind == Kind::Balls && self.setting == Setting::Studio {
                    54.0
                } else {
                    0.0
                };
            for (k, line) in lines.iter().enumerate() {
                let c = Color::new(CREAM.r, CREAM.g, CREAM.b, 0.85 * alpha);
                text_shadowed(
                    ui,
                    line,
                    (sw - ui.text_size(line, 13.0, false).x) / 2.0,
                    top + k as f32 * 18.0,
                    13.0,
                    c,
                    false,
                );
            }
        }

        if ui.escape() {
            if self.ball.is_some() {
                self.choose_preset(0);
            } else {
                action = ShowroomAction::Close;
            }
        }
        for r in self.panels.clone() {
            ui.block(r);
        }
        self.input(ui);
        action
    }

    /// The set's sixteen balls in a row: one picked is looked at closely;
    /// "The set" goes back to them all.
    fn ball_picker(&mut self, ui: &mut Ui, dock: Rect) {
        let palette = crate::textures::Palette::of(self.code);
        let d = 30.0;
        let gap = 6.0;
        let all_label = t("The set");
        let all_w = ui.text_size(all_label, 12.0, true).x + 22.0;
        let w = 16.0 * d + 15.0 * gap + 12.0 + all_w + 24.0;
        let r = Rect::new((crate::ui::vw() - w) / 2.0, dock.y - 10.0 - 46.0, w, 46.0);
        glass(r);
        self.panels.push(r);
        let mut x = r.x + 12.0;
        let cy = r.y + r.h / 2.0;
        for n in 0..16u8 {
            let c = vec2(x + d / 2.0, cy);
            let hover = ui.hovering(Rect::new(x, cy - d / 2.0, d, d));
            let picked = self.ball == Some(n as usize);
            ball_chip(ui, BallId(n), &palette, c, d / 2.0 - 1.0, picked || hover);
            if picked {
                draw_circle_lines(c.x, c.y, d / 2.0 + 2.5, 2.0, GOLD);
            }
            let hit = Rect::new(x, cy - d / 2.0, d, d);
            if hover {
                let name =
                    if n == 0 { t("Cue ball").to_string() } else { tw("Ball {n}", &[("n", &n)]) };
                ui.tooltip(&name, hit);
            }
            if hover && ui.clicked(hit) {
                self.look_at_ball(n as usize);
            }
            x += d + gap;
        }
        x += 6.0;
        let all = Rect::new(x, cy - 15.0, all_w, 30.0);
        if chip(ui, all, all_label, self.ball.is_none()) && self.ball.is_some() {
            self.choose_preset(0);
        }
    }

    /// A line on what the item is, under its name.
    fn description(&self) -> String {
        match self.item.look {
            Look::Balls { finish, .. } => match finish {
                BallFinish::Gloss => t("Phenolic resin under a bright clear coat"),
                BallFinish::Pearl => t("A pearly sheen that shifts with the light"),
                BallFinish::Metal => t("Brushed metal, bright as a mirror"),
                BallFinish::Matte => t("Old ivory: soft, warm and matte"),
            }
            .to_string(),
            Look::Table(style) => table_description(style).to_string(),
            Look::Hall(style) => hall_description(style).to_string(),
            Look::Cue { .. } => {
                t("A full-size cue, tip to butt: turn it, come close to its joint and its butt")
                    .to_string()
            }
            Look::Chalk(_) => t("A cube of chalk, and the colour it leaves on the tip").to_string(),
            Look::Cloth(_) => t("The cloth on the table: come close to see its weave").to_string(),
            _ => String::new(),
        }
    }

    fn setting_name(&self, setting: Setting) -> String {
        match setting {
            Setting::Studio => t("Studio").to_string(),
            Setting::Table => t("Your table").to_string(),
            Setting::Hall(h) => ITEMS
                .iter()
                .find(|i| i.look == Look::Hall(h))
                .map(|i| t(i.name).to_string())
                .unwrap_or_default(),
        }
    }
}

/// What a click on the stage lands on.
enum Pick {
    Ball(usize),
    /// A point, and how close the camera comes to look at it.
    Point(Vec3, f32),
}

/// Seconds on a clock of the showroom's own (macroquad's needs a window).
fn now() -> f64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
}

/// The direction a cue chalking a cube rises in from its tip.
fn chalk_cue_direction() -> Vec3 {
    vec3(-0.5, 0.8, -0.3).normalize()
}

/// `goal` turned by whole turns to be nearest `from`.
fn nearest_turn(from: f32, goal: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut d = (goal - from).rem_euclid(tau);
    if d > std::f32::consts::PI {
        d -= tau;
    }
    from + d
}

/// The point of segment `a`–`b` nearest the ray from `o` along `d`, and how
/// far from the ray it is.
fn closest_on_segment(o: Vec3, d: Vec3, a: Vec3, b: Vec3) -> (Vec3, f32) {
    let ab = b - a;
    let w = a - o;
    let (aa, bb, ab_d) = (ab.dot(ab), d.dot(d), ab.dot(d));
    let denom = aa * bb - ab_d * ab_d;
    let s = if denom.abs() < 1e-8 {
        0.0
    } else {
        ((ab_d * d.dot(w) - bb * ab.dot(w)) / denom).clamp(0.0, 1.0)
    };
    let p = a + ab * s;
    let t = (p - o).dot(d).max(0.0);
    (p, p.distance(o + d * t))
}

/// A see-through dark panel over the stage.
fn glass(r: Rect) {
    crate::ui::soft_shadow(r, 12.0, 0.6);
    rrect(r, 12.0, Color::new(0.05, 0.035, 0.025, 0.80));
    rrect_lines(r, 12.0, 1.0, Color::new(BORDER.r, BORDER.g, BORDER.b, 0.75));
}

/// Text with a soft dark shadow, readable over the stage.
fn text_shadowed(ui: &Ui, text: &str, x: f32, y: f32, size: f32, color: Color, bold: bool) {
    let shade = Color::new(0.0, 0.0, 0.0, 0.7 * color.a);
    ui.text(text, x + 1.0, y + 1.5, size, shade, bold);
    ui.text(text, x, y, size, color, bold);
}

/// A rounded chip: picked (gold) or not. True when clicked.
fn chip(ui: &mut Ui, r: Rect, label: &str, picked: bool) -> bool {
    ui.block(r);
    let hover = ui.hovering(r);
    let radius = r.h / 2.0;
    if picked {
        rrect(r, radius, Color::new(0.42, 0.29, 0.13, 0.95));
        rrect_lines(r, radius, 1.2, GOLD);
    } else if hover {
        rrect(r, radius, Color::new(1.0, 1.0, 1.0, 0.08));
        rrect_lines(r, radius, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.6));
    }
    let color = if picked || hover { GOLD } else { CREAM };
    ui.text_centered(label, r.x + r.w / 2.0, r.y + r.h / 2.0 - 7.0, 13.0, color, true);
    hover && ui.clicked(r)
}

/// A ball of a set seen from the front, flat: its colour (a band on
/// ivory for a stripe) and its number.
pub(crate) fn ball_chip(
    ui: &Ui,
    id: BallId,
    palette: &crate::textures::Palette,
    c: Vec2,
    r: f32,
    lit: bool,
) {
    let col = crate::textures::ball_color_in(id, palette);
    let color = Color::from_rgba(col[0], col[1], col[2], 255);
    let ivory = Color::from_rgba(palette.ivory[0], palette.ivory[1], palette.ivory[2], 255);
    draw_circle(c.x, c.y + 1.5, r, Color::new(0.0, 0.0, 0.0, 0.45));
    match id.kind() {
        snook_legend_core::BallKind::Stripe => {
            draw_circle(c.x, c.y, r, ivory);
            draw_rectangle(c.x - r * 0.86, c.y - r * 0.5, r * 1.72, r, color);
        }
        _ => draw_circle(c.x, c.y, r, color),
    }
    if id.0 > 0 {
        draw_circle(c.x, c.y, r * 0.5, ivory);
        let label = id.0.to_string();
        let size = if id.0 >= 10 { r * 0.62 } else { r * 0.78 };
        ui.text_centered(&label, c.x, c.y - size * 0.55, size, BLACK, true);
    }
    // A highlight, brighter when hovered or picked.
    let a = if lit { 0.5 } else { 0.28 };
    draw_circle(c.x - r * 0.35, c.y - r * 0.4, r * 0.22, Color::new(1.0, 1.0, 1.0, a));
}

fn table_description(style: TableStyle) -> &'static str {
    match style {
        TableStyle::Classic => t("Dark mahogany, turned legs and brass castings"),
        TableStyle::Beach => t("Light teak, square legs and chrome castings"),
        TableStyle::Club => t("Deep mahogany, carved legs and leather pocket nets"),
        TableStyle::Modern => t("Black lacquer and brushed aluminium on two slab legs"),
        TableStyle::Royal => t("Black ebony with gold castings and inlays"),
        TableStyle::Industrial => t("Reclaimed oak rails on a riveted black steel frame"),
        TableStyle::Tournament => t("A pro tournament table: charcoal rails, block legs, chrome"),
        TableStyle::Heritage => t("Figured walnut on barley-twist legs, brass and leather"),
        TableStyle::Deco => t("Ivory lacquer and gold on fluted columns"),
    }
}

fn hall_description(style: HallStyle) -> &'static str {
    match style {
        HallStyle::Pool => t("Panelling, the bar, cue racks, posters and neon"),
        HallStyle::Beach => t("A deck at sunset, open windows, palms and string lights"),
        HallStyle::Club => t("Panelling, bookshelves, leather sofas and a fireplace"),
        HallStyle::Loft => t("Brick, steel windows over the city and Edison bulbs"),
        HallStyle::Lodge => t("Log walls, a stone fireplace and snowy peaks outside"),
        HallStyle::Arcade => t("A black mirror floor, neon tubes and arcade cabinets"),
        HallStyle::Deco => t("A marble chessboard floor, gold, velvet and mirrors"),
        HallStyle::Penthouse => t("Glass walls on the city at night, pale marble and oak"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(code: &str) -> Showroom {
        Showroom::open(code, &snook_legend_core::Table::default()).expect(code)
    }

    /// Every item with a 3D picture opens in the showroom, its views keep
    /// within its limits, and a table or a cloth can be seen in every hall.
    #[test]
    fn every_item_shown_in_3d_opens() {
        for item in ITEMS.iter().filter(|i| crate::shop::has_3d_preview(i)) {
            let s = room(item.code);
            for (label, view) in s.presets() {
                assert_eq!(s.clamp(view), view, "{}: {label}", item.code);
            }
            if matches!(s.kind, Kind::Table | Kind::Cloth) {
                assert_eq!(s.settings.len(), 9, "{}: the studio and the eight halls", item.code);
            }
        }
        assert!(Showroom::open("frame_gold", &snook_legend_core::Table::default()).is_none());
    }

    /// The set racked on the stage: sixteen balls, each once, none
    /// overlapping another.
    #[test]
    fn the_set_is_racked_cleanly() {
        let s = room("balls_classic");
        let spots = s.ball_spots();
        let mut numbers: Vec<u8> = spots.iter().map(|(n, _)| *n).collect();
        numbers.sort();
        assert_eq!(numbers, (0..16).collect::<Vec<u8>>());
        for (k, (_, a)) in spots.iter().enumerate() {
            for (_, b) in &spots[k + 1..] {
                assert!(a.distance(*b) >= 2.0 * s.ball_r, "{a} {b}");
            }
        }
    }

    /// The point of the cue nearest a ray: picking along its length.
    #[test]
    fn the_nearest_point_of_a_segment_to_a_ray() {
        let (p, gap) = closest_on_segment(
            vec3(0.2, 1.0, 1.0),
            vec3(0.0, 0.0, -1.0),
            vec3(-1.0, 1.0, 0.0),
            vec3(1.0, 1.0, 0.0),
        );
        assert!(gap < 1e-5 && p.distance(vec3(0.2, 1.0, 0.0)) < 1e-5);
        // Past the end: the end, and how far it is.
        let (p, gap) = closest_on_segment(
            vec3(3.0, 1.0, 1.0),
            vec3(0.0, 0.0, -1.0),
            vec3(-1.0, 1.0, 0.0),
            vec3(1.0, 1.0, 0.0),
        );
        assert!(p.distance(vec3(1.0, 1.0, 0.0)) < 1e-5 && (gap - 2.0).abs() < 1e-4);
    }

    #[test]
    fn turns_are_taken_the_short_way() {
        let tau = std::f32::consts::TAU;
        assert!((nearest_turn(0.1, tau - 0.1) + 0.1).abs() < 1e-5);
        assert!((nearest_turn(10.0, 10.0 - 3.0 * tau) - 10.0).abs() < 1e-4);
    }
}
