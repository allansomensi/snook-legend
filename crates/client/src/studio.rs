//! The shop's pictures of the cloths and the ball sets: the game's own
//! table, rendered once into a picture under its lamps on the dark stage
//! and kept for as long as it is shown. A cloth is seen on a corner of the
//! table, low over the cloth towards a corner pocket, three balls on it
//! with their shadows; a ball set racked on the classic cloth, the cue ball
//! before it, the foot rail behind.
//!
//! A picture is asked for while the interface is drawn ([`cloth`],
//! [`balls`]) and made at the start of a later frame ([`render`]), a few at
//! most a frame, at the size it is shown at (in steps), drawn at twice that
//! and scaled down so its edges are smooth. The cloth's texture and the
//! balls' are made on other threads first. The least recently shown
//! pictures are let go beyond [`KEEP`]. Where pictures can't be drawn into
//! (an old driver), the first one comes out black, and the shop keeps its
//! drawn pictures (see [`crate::swatch`]).

use std::cell::RefCell;
use std::sync::mpsc::{Receiver, TryRecvError};

use ::glam::DVec2;
use macroquad::prelude::*;
use snook_legend_core::BallId;
use snook_legend_proto::shop::{BallFinish, Look, TableStyle};

use crate::coords::surface;
use crate::gfx::{self, SceneCamera, TargetCamera};
use crate::scene::{Scene, Shot};
use crate::textures::{self, Pixels};

/// The pictures kept (the least recently shown go first).
const KEEP: usize = 48;
/// The cloths' textures kept (each is woven once for all its sizes).
const FELTS: usize = 6;
/// The ball sets kept on the GPU (the classic one, for the cloths, and
/// the sets shown last).
const SETS: usize = 3;
/// How many cloths are woven at once.
const WEAVING: usize = 3;
/// The cloth's texture (as the table's on all but Ultra), and the balls'.
const FELT_SIZE: u16 = 512;
const BALL_SIZE: u16 = 256;
/// A picture is drawn this many times larger, then scaled down.
const SUPER: u32 = 2;
/// The largest side of a picture (pixels).
const MAX_SIDE: u32 = 512;
/// How long a new picture takes to fade in over what stood in for it.
const FADE_IN: f32 = 0.3;
/// The time a frame may spend drawing pictures, and how many it draws at
/// most (more than one: on a graphics card each takes well under a
/// millisecond; on a software renderer one is all a frame can spare).
const BUDGET: f64 = 0.004;
const PER_FRAME: usize = 3;
/// The tournament green, under the ball sets.
const GREEN: [u8; 3] = [24, 96, 48];
const CLASSIC: &str = "balls_classic";

/// What a picture shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Subject {
    /// A cloth (its colour), on a corner of the table.
    Cloth([u8; 3]),
    /// A ball set (its shop code), racked.
    Balls(&'static str),
}

impl Subject {
    fn cloth(self) -> [u8; 3] {
        match self {
            Subject::Cloth(c) => c,
            Subject::Balls(_) => GREEN,
        }
    }

    fn set(self) -> &'static str {
        match self {
            Subject::Cloth(_) => CLASSIC,
            Subject::Balls(code) => code,
        }
    }

    fn finish(self) -> BallFinish {
        match snook_legend_proto::shop::item(self.set()).map(|i| i.look) {
            Some(Look::Balls { finish, .. }) => finish,
            _ => BallFinish::Gloss,
        }
    }
}

/// What a picture shows and its size in pixels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Key {
    subject: Subject,
    w: u16,
    h: u16,
}

struct Picture {
    key: Key,
    target: RenderTarget,
    /// The last frame it was shown, and when it was made.
    used: u64,
    born: f64,
}

/// A ball set on the GPU (the sixteen balls, the cue ball first).
struct Set {
    code: &'static str,
    models: Vec<gfx::Model>,
    used: u64,
}

/// What the pictures put on the table: the ball sets and, when the
/// player's own table is another, the classic one. Kept by the scene
/// (meshes on the GPU go with it).
#[derive(Default)]
pub struct Props {
    sets: Vec<Set>,
    making: Vec<(&'static str, Receiver<Vec<Pixels>>)>,
    classic: Option<Receiver<gfx::Kit>>,
    classic_parts: Option<Vec<gfx::Built>>,
}

struct Studio {
    pictures: Vec<Picture>,
    /// Pictures asked for since the last [`render`], in order.
    wanted: Vec<Key>,
    felts: Vec<([u8; 3], Texture2D, u64)>,
    weaving: Vec<([u8; 3], Receiver<Pixels>)>,
    /// Drawn into first, at [`SUPER`] times the size, with a depth buffer.
    scratch: Option<(RenderTarget, u32, u32)>,
    vignette: Option<Texture2D>,
    frame: u64,
    /// Pictures can't be drawn into here: none are made.
    broken: bool,
    geometry: Geometry,
    /// The first picture was checked.
    checked: bool,
}

thread_local! {
    static STUDIO: RefCell<Studio> = RefCell::new(Studio {
        pictures: Vec::new(),
        wanted: Vec::new(),
        felts: Vec::new(),
        weaving: Vec::new(),
        scratch: None,
        vignette: None,
        frame: 0,
        broken: crate::dev::flag("SNOOK_LEGEND_NO_STUDIO"),
        geometry: Geometry::DEFAULT,
        checked: false,
    });
}

/// What to show for a picture: the picture fading in (with the part of it
/// that fills the place, in its pixels, and how far it has faded in), and
/// under it, while it does, a picture of the same thing at another size.
#[derive(Default)]
pub struct Layers {
    pub under: Option<(Texture2D, Rect)>,
    pub over: Option<(Texture2D, Rect, f32)>,
}

/// The pictures of the cloth `base` to show in `r` (interface units). The
/// picture of this size is asked for if it isn't made yet; one of another
/// size stands in for it meanwhile.
pub fn cloth(r: Rect, base: [u8; 3]) -> Layers {
    layers(r, Subject::Cloth(base))
}

/// The pictures of the ball set `code` (see [`cloth`]).
pub fn balls(r: Rect, code: &'static str) -> Layers {
    layers(r, Subject::Balls(code))
}

fn layers(r: Rect, subject: Subject) -> Layers {
    let Some(key) = key(r, subject) else { return Layers::default() };
    STUDIO.with(|s| {
        let mut s = s.borrow_mut();
        if s.broken {
            return Layers::default();
        }
        let frame = s.frame;
        let now = get_time();
        let mut layers = Layers::default();
        match s.pictures.iter_mut().find(|p| p.key == key) {
            Some(p) => {
                p.used = frame;
                let fade = ((now - p.born) as f32 / FADE_IN).clamp(0.0, 1.0);
                layers.over = Some((p.target.texture.clone(), cover(p.key, r), fade));
                if fade >= 1.0 {
                    return layers;
                }
            }
            None => {
                if !s.wanted.contains(&key) {
                    s.wanted.push(key);
                }
            }
        }
        // The nearest other size of the same thing.
        let area = key.w as f32 * key.h as f32;
        let d = |p: &Picture| (p.key.w as f32 * p.key.h as f32 / area).ln().abs();
        if let Some(p) = s
            .pictures
            .iter_mut()
            .filter(|p| p.key.subject == subject && p.key != key)
            .filter(|p| now - p.born >= FADE_IN as f64)
            .min_by(|a, b| d(a).total_cmp(&d(b)))
        {
            p.used = frame;
            layers.under = Some((p.target.texture.clone(), cover(p.key, r)));
        }
        layers
    })
}

/// The part of a picture that fills a rectangle of `r`'s shape, centred.
fn cover(key: Key, r: Rect) -> Rect {
    let (w, h) = (key.w as f32, key.h as f32);
    let want = r.w / r.h.max(1.0);
    if w / h > want {
        let cw = h * want;
        Rect::new((w - cw) / 2.0, 0.0, cw, h)
    } else {
        let ch = w / want;
        Rect::new(0.0, (h - ch) / 2.0, w, ch)
    }
}

/// The steps a picture's height is made in (pixels), and its shapes.
const HEIGHTS: [u32; 8] = [48, 64, 90, 128, 180, 256, 360, 512];
const SHAPES: [f32; 7] = [0.8, 1.0, 1.25, 1.6, 2.0, 2.5, 3.2];

/// The picture to make for `r` (interface units): its shape and height in
/// steps, so a few sizes serve every place it is shown.
fn key(r: Rect, subject: Subject) -> Option<Key> {
    let k = crate::ui::ui_scale();
    let (pw, ph) = (r.w * k, r.h * k);
    if pw < 8.0 || ph < 8.0 {
        return None;
    }
    let aspect = pw / ph;
    let shape = SHAPES
        .iter()
        .copied()
        .min_by(|a, b| (a / aspect).ln().abs().total_cmp(&(b / aspect).ln().abs()))
        .unwrap_or(1.5);
    // Big enough to cover `r` once cropped to its shape.
    let need = if shape > aspect { ph } else { pw / shape };
    let h = HEIGHTS.iter().copied().find(|&h| h as f32 >= need * 0.94).unwrap_or(512);
    let (mut w, mut h) = ((h as f32 * shape).round() as u32, h);
    if w > MAX_SIDE {
        h = (h as f32 * MAX_SIDE as f32 / w as f32).round() as u32;
        w = MAX_SIDE;
    }
    Some(Key { subject, w: w as u16, h: h as u16 })
}

/// Makes the pictures asked for (a few a frame): called once a frame,
/// before the interface is drawn.
pub fn render(scene: &mut Scene) {
    let mut props = std::mem::take(&mut scene.props);
    STUDIO.with(|s| s.borrow_mut().render(scene, &mut props));
    scene.props = props;
}

impl Studio {
    fn render(&mut self, scene: &mut Scene, props: &mut Props) {
        self.frame += 1;
        let geometry = Geometry::of(scene);
        if geometry != self.geometry {
            // Another table: the pictures show the old one.
            self.geometry = geometry;
            self.pictures.clear();
        }
        let wanted = std::mem::take(&mut self.wanted);
        if self.broken {
            return;
        }
        self.collect(scene, props);
        if wanted.is_empty() {
            self.forget(props);
            return;
        }
        // The classic table, when the player's own is another.
        let (style, quality) = scene.table_style();
        let own_table = style == TableStyle::Classic;
        if !own_table && props.classic_parts.is_none() && props.classic.is_none() {
            let table = scene.table.clone();
            props.classic =
                Some(spawn(move || crate::tables::build(&table, quality, TableStyle::Classic)));
        }
        // The cloths and the ball sets to make.
        for key in &wanted {
            let cloth = key.subject.cloth();
            let known = self.felts.iter().any(|f| f.0 == cloth)
                || self.weaving.iter().any(|w| w.0 == cloth);
            if !known && self.weaving.len() < WEAVING {
                self.weaving.push((cloth, spawn(move || felt(cloth))));
            }
            let set = key.subject.set();
            let known =
                props.sets.iter().any(|s| s.code == set) || props.making.iter().any(|m| m.0 == set);
            if !known && props.making.is_empty() {
                props.making.push((set, spawn(move || ball_pixels(set))));
            }
        }
        if !own_table && props.classic_parts.is_none() {
            return;
        }
        let (start, mut shot) = (get_time(), 0);
        for key in wanted {
            if shot >= PER_FRAME || get_time() - start > BUDGET {
                break;
            }
            if self.pictures.iter().any(|p| p.key == key) {
                continue;
            }
            let Some(felt) = self.felts.iter_mut().find(|f| f.0 == key.subject.cloth()) else {
                continue;
            };
            let Some(set) = props.sets.iter().position(|s| s.code == key.subject.set()) else {
                continue;
            };
            felt.2 = self.frame;
            props.sets[set].used = self.frame;
            let felt = felt.1.clone();
            let table = if own_table { None } else { props.classic_parts.as_deref() };
            self.shoot(scene, key, &felt, table, &props.sets[set].models);
            shot += 1;
            if self.broken {
                return;
            }
        }
        self.forget(props);
    }

    /// Takes what the other threads have made and sends it to the GPU.
    fn collect(&mut self, scene: &Scene, props: &mut Props) {
        let frame = self.frame;
        self.weaving.retain(|(cloth, rx)| match rx.try_recv() {
            Ok(p) => {
                let tex = gfx::texture(p.width, p.height, &p.rgba, true);
                self.felts.push((*cloth, tex, frame));
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => false,
        });
        let r = scene.ball_radius();
        let mut made = Vec::new();
        props.making.retain(|(code, rx)| match rx.try_recv() {
            Ok(pixels) => {
                made.push((*code, pixels));
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => false,
        });
        for (code, pixels) in made {
            let models = pixels
                .into_iter()
                .map(|p| {
                    let tex = gfx::texture(p.width, p.height, &p.rgba, false);
                    gfx::sphere_mesh(r, &tex, 40)
                })
                .collect();
            props.sets.push(Set { code, models, used: frame });
        }
        if let Some(rx) = &props.classic {
            match rx.try_recv() {
                Ok(kit) => {
                    props.classic_parts = Some(kit.finish(scene.white()));
                    props.classic = None;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => props.classic = None,
            }
        }
    }

    /// Lets go of the pictures, cloths and ball sets shown least recently.
    fn forget(&mut self, props: &mut Props) {
        if self.pictures.len() > KEEP {
            self.pictures.sort_by_key(|p| std::cmp::Reverse(p.used));
            self.pictures.truncate(KEEP);
        }
        if self.felts.len() > FELTS {
            self.felts.sort_by_key(|f| std::cmp::Reverse(f.2));
            self.felts.truncate(FELTS);
        }
        if props.sets.len() > SETS {
            // (The classic set stays: every cloth's picture needs it.)
            props.sets.sort_by_key(|s| std::cmp::Reverse((s.code == CLASSIC, s.used)));
            props.sets.truncate(SETS);
        }
    }

    /// Draws the picture `key` into a new picture of its own.
    fn shoot(
        &mut self,
        scene: &mut Scene,
        key: Key,
        felt: &Texture2D,
        table: Option<&[gfx::Built]>,
        set: &[gfx::Model],
    ) {
        let (w, h) = (key.w as u32, key.h as u32);
        let (sw, sh) = (w * SUPER, h * SUPER);
        // The scratch picture, with a depth buffer, as large as needed.
        if self.scratch.as_ref().is_none_or(|(_, cw, ch)| *cw < sw || *ch < sh) {
            let (cw, ch) = self.scratch.as_ref().map_or((0, 0), |s| (s.1, s.2));
            let (cw, ch) = (cw.max(sw).max(256), ch.max(sh).max(256));
            let rt = render_target_ex(cw, ch, RenderTargetParams { sample_count: 1, depth: true });
            rt.texture.set_filter(FilterMode::Linear);
            self.scratch = Some((rt, cw, ch));
        }
        let Some((scratch, ..)) = self.scratch.as_ref() else { return };
        let aspect = w as f32 / h as f32;
        let g = &self.geometry;
        let (view, poses) = match key.subject {
            Subject::Cloth(_) => (g.camera(aspect), g.poses()),
            Subject::Balls(_) => (g.rack_camera(aspect), g.rack_poses()),
        };
        let cam = TargetCamera { view, pass: scratch.render_pass.clone(), size: (sw, sh) };
        set_camera(&cam);
        clear_background(Color::new(0.014, 0.012, 0.011, 1.0));
        let balls: Vec<(&gfx::Model, Mat4)> =
            poses.iter().filter_map(|&(n, at)| set.get(n as usize).map(|m| (m, at))).collect();
        let finish = key.subject.finish();
        scene.draw_shot(&cam, &Shot { felt, table, balls: &balls, finish });

        // Scaled down into the picture kept, with a vignette.
        let target = render_target(w, h);
        target.texture.set_filter(FilterMode::Linear);
        let vignette = self.vignette.get_or_insert_with(|| {
            let p = vignette_pixels();
            let tex = Texture2D::from_rgba8(p.width, p.height, &p.rgba);
            tex.set_filter(FilterMode::Linear);
            tex
        });
        set_camera(&Camera2D {
            target: vec2(w as f32 / 2.0, h as f32 / 2.0),
            zoom: vec2(2.0 / w as f32, 2.0 / h as f32),
            render_target: Some(target.clone()),
            ..Default::default()
        });
        clear_background(BLACK);
        let size = vec2(w as f32, h as f32);
        draw_texture_ex(
            &scratch.texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(size),
                source: Some(Rect::new(0.0, 0.0, sw as f32, sh as f32)),
                flip_y: true,
                ..Default::default()
            },
        );
        let params = DrawTextureParams { dest_size: Some(size), ..Default::default() };
        draw_texture_ex(vignette, 0.0, 0.0, WHITE, params);
        set_default_camera();
        if !self.checked {
            // The first picture: drawing into pictures works here (where it
            // doesn't, it comes out black).
            self.checked = true;
            let image = target.texture.get_texture_data();
            let lit = image.bytes.chunks_exact(4).any(|p| p[0].max(p[1]).max(p[2]) > 40);
            if !lit {
                eprintln!("shop pictures: drawn pictures (render targets) don't work here");
                self.broken = true;
                self.pictures.clear();
                return;
            }
        }
        self.pictures.push(Picture { key, target, used: self.frame, born: get_time() });
    }
}

fn spawn<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Receiver<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx
}

/// The sixteen balls of a set, made side by side.
fn ball_pixels(code: &str) -> Vec<Pixels> {
    let palette = textures::Palette::of(code);
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..16u8)
            .collect::<Vec<_>>()
            .chunks(4)
            .map(|part| {
                let part = part.to_vec();
                s.spawn(move || {
                    part.iter()
                        .map(|&n| textures::ball(BallId(n), BALL_SIZE, &palette))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    })
}

/// The table in the pictures (table coordinates, metres; the foot of the
/// table at +x).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    /// Half the playing area (between the cushions' noses).
    pub half: Vec2,
    /// The width of the rail beyond the cushions' noses.
    pub rail: f32,
    /// The pockets' holes: centre and radius.
    pub holes: [(Vec2, f32); 6],
    /// The corner pocket of the cloths' pictures (at the foot, on the
    /// right: +x, +y) and the way out of the table through it.
    pub hole: Vec2,
    pub out: Vec2,
    pub ball_radius: f32,
}

impl Geometry {
    fn of(scene: &Scene) -> Geometry {
        let table = &scene.table;
        let spec = table.spec;
        let mut g = Geometry {
            half: vec2(spec.length as f32 / 2.0, spec.width as f32 / 2.0),
            rail: spec.rail_width as f32,
            ball_radius: spec.ball_radius as f32,
            ..Geometry::DEFAULT
        };
        for (slot, p) in g.holes.iter_mut().zip(&table.pockets) {
            *slot = (flat(p.hole_center), p.hole_radius as f32);
        }
        if let Some(p) = table.pockets.iter().max_by(|a, b| {
            (a.hole_center.x + a.hole_center.y).total_cmp(&(b.hole_center.x + b.hole_center.y))
        }) {
            g.hole = flat(p.hole_center);
            g.out = flat(p.outward);
        }
        g
    }

    /// A tournament table's (until the scene gives its own).
    const DEFAULT: Geometry = {
        let (x, y, r) = (1.205, 0.621, 0.07);
        Geometry {
            half: vec2(1.1684, 0.5842),
            rail: 0.115,
            holes: [
                (vec2(-x, y), r),
                (vec2(-x, -y), r),
                (vec2(0.0, y + 0.03), r),
                (vec2(0.0, -y - 0.03), r),
                (vec2(x, y), r),
                (vec2(x, -y), r),
            ],
            hole: vec2(x, y),
            out: vec2(std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2),
            ball_radius: 0.028575,
        }
    };

    /// The cloths' camera: low over the cloth, looking into the corner
    /// pocket a little from the side, the long rail across the top of the
    /// picture and the short one down its right; for a picture `aspect`
    /// wide.
    pub fn camera(&self, aspect: f32) -> SceneCamera {
        // The point looked at: on the cloth, in front of the pocket.
        let at = self.hole - self.out * 0.36;
        // Looking out through the pocket, turned a little towards the long
        // rail; from 1.3 m away, 18° over the cloth.
        let look = turned(self.out, 0.25);
        self.view(at, look, 1.3, 0.32, 0.4, aspect)
    }

    /// The camera of the ball sets: from the head of the table, a little
    /// to the side, low onto the rack at the foot spot, the foot rail
    /// across the top of the picture.
    pub fn rack_camera(&self, aspect: f32) -> SceneCamera {
        let at = self.foot_spot() + vec2(0.18, 0.0);
        self.view(at, turned(Vec2::X, 0.35), 0.95, 0.25, 0.36, aspect)
    }

    /// A camera `dist` from `at` (on the cloth), looking along `look`,
    /// `pitch` over the cloth, the lens `fovy` high for a 1:1 picture.
    fn view(
        &self,
        at: Vec2,
        look: Vec2,
        dist: f32,
        pitch: f32,
        fovy: f32,
        aspect: f32,
    ) -> SceneCamera {
        let eye = on_table(at - look * dist * pitch.cos(), dist * pitch.sin());
        // Narrow pictures need a wider lens to keep it all.
        let fovy = fovy / aspect.clamp(0.8, 1.5).powf(0.6);
        SceneCamera { eye, target: on_table(at, 0.0), up: Vec3::Y, fovy }
    }

    fn foot_spot(&self) -> Vec2 {
        vec2(self.half.x / 2.0, 0.0)
    }

    /// The balls on the cloth in the cloths' pictures: their numbers and
    /// centres (world). The 8 before the pocket, the 1 beside it, the cue
    /// ball in front.
    pub fn balls(&self) -> [(u8, Vec3); 3] {
        let into = -self.out;
        let side = vec2(-into.y, into.x);
        // How far from the pocket, along its way into the table and across.
        const SPOTS: [(u8, f32, f32); 3] = [(0, 0.66, 0.08), (8, 0.26, 0.0), (1, 0.45, -0.18)];
        SPOTS.map(|(n, along, across)| {
            (n, on_table(self.hole + into * along + side * across, self.ball_radius))
        })
    }

    /// The balls of the ball sets' pictures: racked at the foot spot (the
    /// 1 at the apex, the 8 in the middle), the cue ball before them.
    pub fn rack(&self) -> Vec<(u8, Vec3)> {
        const ROWS: [&[u8]; 5] = [&[1], &[9, 2], &[3, 8, 10], &[11, 4, 12, 5], &[6, 13, 7, 14, 15]];
        let r = self.ball_radius * 1.004;
        let apex = self.foot_spot();
        let mut out = vec![(0, on_table(apex + vec2(-0.12, -0.15), self.ball_radius))];
        for (i, row) in ROWS.iter().enumerate() {
            for (j, &n) in row.iter().enumerate() {
                let p = apex
                    + vec2(
                        i as f32 * r * 3f32.sqrt(),
                        (j as f32 - (row.len() - 1) as f32 / 2.0) * 2.0 * r,
                    );
                out.push((n, on_table(p, self.ball_radius)));
            }
        }
        out
    }

    /// Where the balls of the cloths' pictures lie, their numbers turned
    /// towards the camera, tipped up a little and turned a little.
    fn poses(&self) -> Vec<(u8, Mat4)> {
        let eye = self.camera(1.5).eye;
        self.balls()
            .iter()
            .enumerate()
            .map(|(k, &(n, p))| (n, facing(p, eye, 0.9, 0.35 - 0.3 * k as f32)))
            .collect()
    }

    /// The rack's: each ball turned its own way, most numbers in sight.
    fn rack_poses(&self) -> Vec<(u8, Mat4)> {
        let eye = self.rack_camera(1.5).eye;
        self.rack()
            .into_iter()
            .map(|(n, p)| {
                let jitter = |seed| textures::hash(n as i32, seed, 11) - 0.5;
                let aim = eye + vec3(jitter(1), 0.0, jitter(2)) * 0.9;
                (n, facing(p, aim, 0.5 + jitter(3) * 1.2, jitter(4) * 2.2))
            })
            .collect()
    }
}

/// A ball at `p` with its number turned towards `eye`, tipped `up`
/// towards the lamps and turned `roll` about itself.
fn facing(p: Vec3, eye: Vec3, up: f32, roll: f32) -> Mat4 {
    let face = ((eye - p).normalize_or_zero() + Vec3::Y * up).normalize_or_zero();
    let tilt = Quat::from_rotation_arc(Vec3::Z, face);
    Mat4::from_rotation_translation(Quat::from_axis_angle(face, roll) * tilt, p)
}

/// `v` turned by `angle` (radians, counter-clockwise in table coordinates).
fn turned(v: Vec2, angle: f32) -> Vec2 {
    let (s, c) = angle.sin_cos();
    vec2(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// The table in the pictures (the last scene's).
pub fn geometry() -> Geometry {
    STUDIO.with(|s| s.borrow().geometry)
}

fn flat(p: DVec2) -> Vec2 {
    vec2(p.x as f32, p.y as f32)
}

/// A point on the table (table coordinates) at `height` over the cloth.
pub fn on_table(p: Vec2, height: f32) -> Vec3 {
    surface(DVec2::new(p.x as f64, p.y as f64), height)
}

/// The cloth's texture for the pictures: the table's own, with the grain
/// of its nap (seen this close, it would look like felt-tip ink without):
/// a speck per thread and short fibres lying along the cloth. Its colour
/// on the whole is the same.
fn felt(cloth: [u8; 3]) -> Pixels {
    let mut p = textures::felt_sized(cloth, FELT_SIZE);
    let n = p.width as i32;
    for (i, px) in p.rgba.chunks_exact_mut(4).enumerate() {
        let (x, y) = (i as i32 % n, i as i32 / n);
        let speck = textures::hash(x, y, 7) - 0.5;
        let fibre = textures::noise(x as f32 / 2.0, y as f32 / 8.0, 64, 31) - 0.5;
        let k = 1.0 + speck * 0.09 + fibre * 0.08;
        for c in &mut px[..3] {
            *c = (*c as f32 * k).round().clamp(0.0, 255.0) as u8;
        }
    }
    p
}

/// A soft darkening towards the edges and the corners.
fn vignette_pixels() -> Pixels {
    let n = 128usize;
    let mut rgba = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let px = (x as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let py = (y as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let d = (px * px * 0.85 + py * py).sqrt();
            let dark = ((d - 0.55) / 0.85).clamp(0.0, 1.0).powf(1.6) * 0.6;
            rgba.extend([0, 0, 0, (dark * 255.0) as u8]);
        }
    }
    Pixels { width: n as u16, height: n as u16, rgba }
}
