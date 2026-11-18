//! The bar room, the table, balls, cue and aiming guides.

use std::collections::HashMap;

use ::glam::{DVec2, DVec3};
use macroquad::prelude::*;
use snook_legend_core::{Ball, BallId, BallStatus, PhysicsParams, PocketId, Table};
use snook_legend_proto::shop::{HallStyle, TableStyle};

use crate::config::Quality;
use crate::coords::{TABLE_HEIGHT, ball_rotation, dir_to_world, surface, to_world};
use crate::gfx::{self, LAMP_COUNT, Materials, MeshBuilder, SceneCamera, Surface};
use crate::textures;

/// Height of the cushion top above the cloth.
pub(crate) const CUSHION_TOP: f32 = 0.043;
/// How far the cushion extends behind its nose.
pub(crate) const CUSHION_DEPTH: f32 = 0.05;
/// Height of the wooden rail top above the cloth.
pub(crate) const RAIL_TOP: f32 = 0.046;
/// Thickness of the metal pocket castings on the rail.
pub(crate) const CAST_H: f32 = 0.006;
pub(crate) const LAMP_Y: f32 = TABLE_HEIGHT + 0.95;

pub(crate) struct Part {
    mesh: gfx::Model,
    surface: Surface,
}

pub struct Scene {
    pub table: Table,
    pub materials: Materials,
    parts: Vec<Part>,
    /// The table's theme: body, rails, legs, pockets (see [`crate::tables`]).
    table_parts: Vec<gfx::Built>,
    holes: [Vec4; 6],
    throats: [Vec4; 12],
    floor_shadow: Part,
    /// The hall around the table and its lamp (see [`crate::halls`]), and
    /// which hall it is (none until it is first shown).
    room: Vec<gfx::Built>,
    room_style: Option<HallStyle>,
    /// The dark stage was chosen (no hall around the table)...
    stage: bool,
    /// ...but the shop shows a hall all the same.
    pub hall_preview: bool,
    quality: Quality,
    balls: Vec<gfx::Model>,
    cue: gfx::Model,
    white: Texture2D,
    shadow: Texture2D,
    lamps: [Vec3; LAMP_COUNT],
    sinking: HashMap<BallId, Sinking>,
    /// Cue meshes by (cue, chalk, level) design.
    cues: HashMap<(String, String, u8), (gfx::Model, Option<gfx::Model>)>,
    pub lamp_visible: bool,
    /// The camera looks down on the table: nothing of the room (cords,
    /// fans, beams) is drawn between it and the table.
    pub clear_over_table: bool,
    /// The cloth and balls on the table now, and those being made.
    looks: Looks,
    wanted: Looks,
    /// Parts drawn with the cloth texture.
    felt_parts: Vec<usize>,
    ball_tex_size: u16,
    felt_size: u16,
    /// New textures being made on another thread.
    making: Option<std::sync::mpsc::Receiver<Made>>,
    /// Made textures waiting to go to the GPU, a few per frame.
    uploading: Option<(Made, Vec<Texture2D>)>,
    lamp_count: u32,
    /// The table built now.
    table_style: TableStyle,
    /// Per-pixel soft shadows and contact occlusion from the balls (High, Ultra).
    soft_shadows: bool,
    /// The balls and the cue cast shadows on the table (a setting; the
    /// soft ones on High and Ultra, a blob under each ball otherwise).
    pub ball_shadows: bool,
    /// The polished floors of the halls reflect the room (Ultra).
    reflections: bool,
    /// Halos round the hall's lights and haze under the lamp (High, Ultra).
    halos: bool,
    /// The hall's lights that get a halo, and the order they are drawn in.
    glows: Vec<gfx::Glow>,
    glow_order: Vec<usize>,
    glow_tex: Texture2D,
    /// Where each ball is drawn this frame (ball index, transform).
    ball_poses: Vec<(usize, Mat4)>,
    /// The cue drawn last (its design), so the look-up is skipped.
    cue_key: Option<(String, String, u8)>,
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba(r, g, b, 255)
}

fn upload(p: textures::Pixels, repeat: bool) -> Texture2D {
    gfx::texture(p.width, p.height, &p.rgba, repeat)
}

impl Scene {
    /// `focused`: a dark stage around the table instead of the whole bar.
    pub fn new(table: Table, quality: Quality, focused: bool, looks: &Looks) -> Self {
        textures::set_detail(quality == Quality::Ultra);
        let (ball_tex, ball_segments) = match quality {
            Quality::Low => (512, 32),
            Quality::Medium => (512, 32),
            Quality::High => (512, 48),
            Quality::Ultra => (1024, 64),
        };
        let white = gfx::white_texture();
        let felt_size = felt_size(quality);
        let felt = upload(textures::felt_sized(looks.cloth, felt_size), true);
        let shadow = upload(textures::shadow(), false);

        let mut parts = Vec::new();
        let mut add = |b: MeshBuilder, tex: &Texture2D, surface: Surface| -> usize {
            parts.push(Part { mesh: b.build(tex), surface });
            parts.len() - 1
        };

        // --- The hall and the table's theme ----------------------------------
        let (room, glows) = if focused {
            (Vec::new(), Vec::new())
        } else {
            let mut kit = hall_kit(&table, quality, looks.hall);
            let glows = std::mem::take(&mut kit.glows);
            (kit.finish(&white), glows)
        };
        let table_parts = crate::tables::build(&table, quality, looks.table).finish(&white);

        // --- Table ----------------------------------------------------------
        let spec = table.spec;
        let (l, w) = (spec.length as f32, spec.width as f32);

        // Cloth bed.
        let mut bed = MeshBuilder::default();
        let m = CUSHION_DEPTH + 0.01;
        let y = TABLE_HEIGHT;
        bed.quad_world_uv(
            [
                vec3(-l / 2.0 - m, y, w / 2.0 + m),
                vec3(l / 2.0 + m, y, w / 2.0 + m),
                vec3(l / 2.0 + m, y, -w / 2.0 - m),
                vec3(-l / 2.0 - m, y, -w / 2.0 - m),
            ],
            WHITE,
            0.6,
        );
        let mut felt_parts = vec![add(bed, &felt, Surface::MATTE.cut(1.0))];

        // Cushions: a sloped rubber profile extruded along every cushion segment.
        let profile = [
            vec2(0.010, 0.0),
            vec2(0.0, spec.cushion_height as f32),
            vec2(0.008, CUSHION_TOP),
            vec2(CUSHION_DEPTH, CUSHION_TOP),
            vec2(CUSHION_DEPTH, 0.0),
        ];
        let mut cushions = MeshBuilder::default();
        // Jaw rubbers are drawn after the pocket holes are opened, so their
        // ends stay whole where they meet the hole.
        let mut jaws = MeshBuilder::default();
        for seg in &table.cushions {
            let mid = (seg.a + seg.b) / 2.0;
            let dir = (seg.b - seg.a).normalize();
            let mut out = dir.perp();
            let axis_aligned = dir.x.abs() < 1e-6 || dir.y.abs() < 1e-6;
            if axis_aligned {
                // A rail: the rubber goes away from the middle of the table.
                if out.dot(mid) < 0.0 {
                    out = -out;
                }
            } else {
                // A pocket jaw: the rubber goes away from the pocket's channel.
                let mouth = table
                    .pockets
                    .iter()
                    .map(|p| p.mouth_center)
                    .min_by(|a, b| a.distance(mid).total_cmp(&b.distance(mid)))
                    .unwrap_or(DVec2::ZERO);
                if out.dot(mouth - mid) > 0.0 {
                    out = -out;
                }
            }
            let out3 = vec3(out.x as f32, 0.0, -out.y as f32);
            if !axis_aligned {
                // The jaws are drawn below (see `jaw_lines`).
                continue;
            }
            // Rail cushions run on into the pocket corners (the throat and hole
            // are cut out in the shader); a jaw stops where it meets the hole.
            let (start, end) = if axis_aligned {
                // Each end runs on towards the nearest pocket hole, but stops
                // where the cushion line passes closest to the hole's centre:
                // further on it would come out of the cut, past the pocket.
                let run_on = |p: DVec2, out: DVec2| -> f64 {
                    let nearest = table.pockets.iter().min_by(|x, y| {
                        x.mouth_center.distance(p).total_cmp(&y.mouth_center.distance(p))
                    });
                    match nearest {
                        Some(pk) => (pocket_center_of(&table, pk.id) - p).dot(out).clamp(0.0, 0.13),
                        None => 0.0,
                    }
                };
                (seg.a - dir * run_on(seg.a, -dir), seg.b + dir * run_on(seg.b, dir))
            } else {
                unreachable!("jaws handled above")
            };
            cushions.extrude(
                surface(start, 0.0),
                surface(end, 0.0),
                out3,
                &profile,
                rgb(205, 215, 205),
            );
        }
        for (a, end, across) in jaw_lines(&table) {
            // The part through the rail belongs to the table's theme.
            jaw_wall(&mut jaws, &mut MeshBuilder::default(), &table, a, end, across, WHITE);
        }
        felt_parts.push(add(cushions, &felt, Surface::MATTE.cut(2.0)));
        felt_parts.push(add(jaws, &felt, Surface::MATTE));

        // Pockets. The cloth, cushions and rails are cut by the shader along
        // the throat (between the jaws) and the hole; the hole's wall closes
        // the cut: it rises to the rail top on the rail side and starts at the
        // cloth on the throat side.
        let mut holes = [Vec4::ZERO; 6];
        let mut throats = [Vec4::ZERO; 12];
        for (k, pocket) in table.pockets.iter().enumerate() {
            let id = pocket.id;
            let r = pocket_radius_of(&table, id);
            let c2 = pocket_center_of(&table, id);
            let center = surface(c2, 0.0);
            // Side pockets: w = where the cushion line is, across the table
            // (the leather lip must not reach in front of it, over the cloth).
            let edge = if id.is_side() { (table.spec.width / 2.0) as f32 + 0.008 } else { 0.0 };
            holes[k] = vec4(center.x, center.z, r, edge);
            let jaws: Vec<(DVec2, DVec2)> = table
                .cushions
                .iter()
                .filter(|c| !is_axis_aligned(c.a, c.b))
                .filter(|c| c.a.distance(pocket.mouth_center) < pocket.half_mouth + 1e-6)
                .map(|c| {
                    let end = clip_to_hole(&table, c.a, c.b);
                    (c.a, end + (end - c.a).normalize() * 0.0015)
                })
                .collect();
            if let [(a1, b1), (a2, b2)] = jaws[..] {
                let w = |p: DVec2| {
                    let q = surface(p, 0.0);
                    vec2(q.x, q.z)
                };
                let (p0, p1, p2, p3) = (w(a1), w(b1), w(b2), w(a2));
                throats[2 * k] = vec4(p0.x, p0.y, p1.x, p1.y);
                throats[2 * k + 1] = vec4(p2.x, p2.y, p3.x, p3.y);
            }
        }

        // Soft shadow of the table on the floor.
        let mut fs = MeshBuilder::default();
        let (sx, sz) = (l / 2.0 + 0.6, w / 2.0 + 0.6);
        fs.quad(
            [
                vec3(-sx, 0.002, sz),
                vec3(sx, 0.002, sz),
                vec3(sx, 0.002, -sz),
                vec3(-sx, 0.002, -sz),
            ],
            [vec2(0.0, 1.0), vec2(1.0, 1.0), vec2(1.0, 0.0), vec2(0.0, 0.0)],
            Color::new(1.0, 1.0, 1.0, 0.75),
        );
        let floor_shadow = Part { mesh: fs.build(&shadow), surface: Surface::UNLIT };

        // --- Balls and cue --------------------------------------------------
        let r = spec.ball_radius as f32;
        let balls = ball_pixels(ball_tex, &looks.balls)
            .into_iter()
            .map(|p| gfx::sphere_mesh(r, &upload(p, false), ball_segments))
            .collect();
        let cue = cue_mesh("cue_classic", "chalk_blue", 0, &white).0;

        // The lamps span the table's length, as the halls hang them.
        let lamps = crate::halls::lamp_xs(crate::halls::lamp_reach(half_table(&table)))
            .map(|x| vec3(x, LAMP_Y - 0.02, 0.0));
        Scene {
            table,
            materials: Materials::new(),
            parts,
            table_parts,
            holes,
            throats,
            floor_shadow,
            room_style: (!focused).then_some(looks.hall),
            room,
            stage: focused,
            hall_preview: false,
            quality,
            balls,
            cue,
            white,
            shadow,
            lamps,
            sinking: HashMap::new(),
            cues: HashMap::new(),
            lamp_visible: !focused,
            clear_over_table: false,
            looks: looks.clone(),
            wanted: looks.clone(),
            felt_parts,
            ball_tex_size: ball_tex,
            felt_size,
            making: None,
            uploading: None,
            lamp_count: if quality == Quality::Low { 1 } else { 3 },
            table_style: looks.table,
            soft_shadows: matches!(quality, Quality::High | Quality::Ultra),
            ball_shadows: true,
            reflections: quality == Quality::Ultra,
            halos: quality >= Quality::High,
            glows,
            glow_order: Vec::new(),
            glow_tex: upload(textures::halo(), false),
            ball_poses: Vec::with_capacity(16),
            cue_key: None,
        }
    }

    /// Dresses the table with another cloth, ball set, table or hall. The
    /// textures and meshes are made on another thread and sent to the GPU a
    /// few at a time, so the game never stops (buying an item used to freeze
    /// it for a second).
    pub fn set_looks(&mut self, looks: &Looks) {
        if *looks == self.wanted {
            return;
        }
        self.wanted = looks.clone();
        self.request();
    }

    /// Shows the hall around the table even on the dark stage (the shop's
    /// preview of a hall).
    pub fn preview_hall(&mut self, on: bool) {
        if self.hall_preview != on {
            self.hall_preview = on;
            if on && self.room_style != Some(self.wanted.hall) {
                self.request();
            }
        }
    }

    /// Whether the hall is shown (not the dark stage).
    fn hall_shown(&self) -> bool {
        (!self.stage || self.hall_preview) && self.room_style.is_some()
    }

    /// Starts making what differs between the table now and the one wanted.
    fn request(&mut self) {
        let looks = self.wanted.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let cloth = (looks.cloth != self.looks.cloth).then_some(looks.cloth);
        let balls = (looks.balls != self.looks.balls).then_some(looks.balls);
        let theme = (looks.table != self.table_style).then_some(looks.table);
        let hall = ((!self.stage || self.hall_preview) && self.room_style != Some(looks.hall))
            .then_some(looks.hall);
        let (felt_size, ball_size) = (self.felt_size, self.ball_tex_size);
        let (table, quality) = (self.table.clone(), self.quality);
        std::thread::spawn(move || {
            let felt = cloth.map(|c| textures::felt_sized(c, felt_size));
            let balls = balls.map(|p| ball_pixels(ball_size, &p));
            let theme = theme.map(|t| (t, crate::tables::build(&table, quality, t)));
            let hall = hall.map(|h| (h, hall_kit(&table, quality, h)));
            let _ = tx.send(Made { looks, felt, balls, theme, hall });
        });
        // A newer request replaces one still being made.
        self.making = Some(rx);
        self.uploading = None;
    }

    /// Takes finished textures and puts them on the table.
    fn update_looks(&mut self) {
        if let Some(rx) = &self.making {
            match rx.try_recv() {
                Ok(made) => {
                    self.making = None;
                    self.uploading = Some((made, Vec::new()));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.making = None,
            }
        }
        let Some((made, done)) = self.uploading.as_mut() else { return };
        // A few textures a frame (a big ball texture is a few megabytes).
        let mut budget = 4;
        if let Some(felt) = made.felt.take() {
            let tex = upload(felt, true);
            for &i in &self.felt_parts {
                self.parts[i].mesh.set_texture(&tex);
            }
            budget -= 1;
        }
        if let Some(balls) = made.balls.as_mut() {
            while budget > 0 && !balls.is_empty() {
                done.push(upload(balls.remove(0), false));
                budget -= 1;
            }
            if !balls.is_empty() {
                return;
            }
            // The whole set at once: never two sets on the table.
            for (mesh, tex) in self.balls.iter_mut().zip(done.iter()) {
                mesh.set_texture(tex);
            }
        }
        if let Some((style, kit)) = made.theme.take() {
            self.table_parts = kit.finish(&self.white);
            self.table_style = style;
        }
        if let Some((style, mut kit)) = made.hall.take() {
            self.glows = std::mem::take(&mut kit.glows);
            self.room = kit.finish(&self.white);
            self.room_style = Some(style);
        }
        self.looks = made.looks.clone();
        self.uploading = None;
    }

    pub fn ball_radius(&self) -> f32 {
        self.table.spec.ball_radius as f32
    }

    /// Draws the whole 3D scene.
    pub fn draw(&mut self, cam: &SceneCamera, balls: &[Ball], dt: f32, overlay: &Overlay) {
        self.update_looks();
        let hall = self.hall_shown();
        let focus = if hall { 0.0 } else { 1.0 };
        let tint = match self.room_style.filter(|_| hall) {
            Some(style) => crate::halls::tint(style),
            None => vec3(0.09, 0.06, 0.045),
        };
        // The overhead view looks from above the ceiling through a long
        // lens; the table is lit as seen from under the ceiling, where the
        // view used to be: lit from high above, every glossy thing shone
        // straight back and the table went pale and flat.
        let light_eye = vec3(cam.eye.x, cam.eye.y.min(LIGHT_EYE_MAX_Y), cam.eye.z);
        let lens = (cam.eye.y - TABLE_HEIGHT).max(0.1) / (light_eye.y - TABLE_HEIGHT).max(0.1);
        self.materials.set_frame(light_eye, self.lamps, 1.55, self.lamp_count, focus, tint);
        let soft_shadows = self.soft_shadows && self.ball_shadows;
        let mut casters = [Vec4::ZERO; 16];
        if soft_shadows {
            let r = self.ball_radius();
            for (slot, b) in casters.iter_mut().zip(balls.iter().filter(|b| b.in_play())) {
                let p = to_world(b.position);
                *slot = vec4(p.x, p.y, p.z, r);
            }
        }
        // The cue mesh runs along its local +Y from the tip (see `cue_mesh`).
        let cue = overlay.cue.as_ref().map(|c| {
            let butt = c.tip + c.rotation * (Vec3::Y * CUE_LENGTH);
            (c.tip.extend(0.0061), butt.extend(0.0148))
        });
        self.materials.set_shadows(soft_shadows, soft_shadows, &casters, cue);
        self.materials.set_pockets(&self.holes, &self.throats);
        let half = half_table(&self.table);
        // (A pixel is as large on the table as from the real eye.)
        let glint = gfx::glint_angle(cam.fovy, screen_height(), self.ball_radius()) * lens;
        self.materials.set_table(half, glint);
        self.materials.set_clear(
            self.clear_over_table
                .then(|| (half + Vec2::splat(0.35), TABLE_HEIGHT + 0.12, cam.eye.y + 1.0)),
        );
        self.place_balls(balls, dt);

        // Ultra: a polished floor reflects the room. The scene is drawn once
        // more, mirrored under the floor, and the floor is then laid over it
        // a little see-through (see `Layer::Floor`).
        let mirror = match self.room_style {
            Some(style) if hall && self.reflections => crate::halls::mirror_fade(style),
            _ => 0.0,
        };
        let reflect = mirror > 0.0 && self.room.iter().any(|p| p.layer == gfx::Layer::Floor);
        if reflect {
            let flipped = MirrorCamera(cam);
            set_camera(&flipped);
            self.materials.set_view(flipped.matrix());
            self.materials.set_camera_pos(light_eye * vec3(1.0, -1.0, 1.0));
            self.materials.set_mirror(mirror);
            self.draw_table(true, hall);
            // What starts higher than the reflection's fade lets through
            // (under 5%) is left out of it: the ceiling, the beams, shelves.
            let cutoff = 3.0 / mirror;
            self.materials.set_room(true);
            for part in self.room.iter().filter(|p| p.layer == gfx::Layer::Solid && p.low < cutoff)
            {
                self.materials.use_lit(part.surface);
                self.materials.draw(&part.mesh, None);
            }
            if self.lamp_visible {
                draw_layer(&self.room, gfx::Layer::Lamp, &self.materials);
            }
            self.materials.set_room(false);
            self.draw_placed_balls();
            self.materials.set_mirror(0.0);
            self.materials.set_camera_pos(light_eye);
        }
        set_camera(cam);
        self.materials.set_view(cam.matrix());
        if hall {
            self.materials.set_room(true);
            for part in self.room.iter().filter(|p| p.layer == gfx::Layer::Floor) {
                if reflect {
                    self.materials.use_blended(part.surface);
                } else {
                    self.materials.use_lit(part.surface);
                }
                self.materials.draw(&part.mesh, None);
            }
            self.materials.set_room(false);
        }
        self.draw_table(false, hall);
        if hall {
            self.materials.set_room(true);
            draw_layer(&self.room, gfx::Layer::Solid, &self.materials);
            self.materials.set_room(false);
            self.materials.use_blended(Surface::UNLIT);
            self.materials.draw(&self.floor_shadow.mesh, None);
            self.materials.set_room(true);
            draw_layer(&self.room, gfx::Layer::Blended, &self.materials);
            self.materials.set_room(false);
        }
        if !self.soft_shadows && self.ball_shadows {
            self.draw_ball_shadows(balls);
        }
        draw_layer(&self.table_parts, gfx::Layer::Inside, &self.materials);
        draw_layer(&self.table_parts, gfx::Layer::Detail, &self.materials);
        if self.lamp_visible && hall {
            self.materials.set_room(true);
            draw_layer(&self.room, gfx::Layer::Lamp, &self.materials);
            self.materials.set_room(false);
        }
        self.draw_placed_balls();
        if let Some(cue) = &overlay.cue {
            // The shooter's own cue and chalk (built once per design).
            let wanted = (&overlay.cue_look.0, &overlay.cue_look.1, overlay.cue_level);
            let known = self.cue_key.as_ref().is_some_and(|k| (&k.0, &k.1, k.2) == wanted);
            if !known && !wanted.0.is_empty() {
                let key = (wanted.0.clone(), wanted.1.clone(), wanted.2);
                if !self.cues.contains_key(&key) {
                    let mesh = cue_mesh(&key.0, &key.1, key.2, &self.white);
                    self.cues.insert(key.clone(), mesh);
                }
                self.cue_key = Some(key);
            }
            self.materials.use_lit(Surface::WOOD);
            let model = Mat4::from_rotation_translation(cue.rotation, cue.tip);
            match self.cue_key.as_ref().and_then(|k| self.cues.get(k)) {
                Some((mesh, glow)) if !wanted.0.is_empty() => {
                    self.materials.draw(mesh, Some(model));
                    if let Some(glow) = glow {
                        // The top level's gold glows softly.
                        self.materials.use_blended(Surface::UNLIT);
                        self.materials.draw(glow, Some(model));
                    }
                }
                _ => self.materials.draw(&self.cue, Some(model)),
            }
        }
        self.draw_guides(&overlay.guides, cam);
        if let Some(p) = overlay.ghost {
            self.draw_ghost_ball(p, cam);
        }
        // Last, over everything: the haze under the lamps and the halos
        // round the bulbs (see-through: they write no depth, so the haze
        // never cuts a halo).
        if hall && self.halos {
            if self.lamp_visible {
                self.materials.set_room(true);
                draw_layer(&self.room, gfx::Layer::Haze, &self.materials);
                self.materials.set_room(false);
            }
            self.draw_halos(cam);
        }
        gl_use_default_material();
        set_default_camera();
    }

    /// The cloth, cushions and the table's body (`mirrored`: in the floor's
    /// reflection, where the inside of the pockets cannot be seen).
    fn draw_table(&self, mirrored: bool, hall: bool) {
        let skip: Option<usize> =
            crate::dev::var("SNOOK_LEGEND_SKIP_PART").and_then(|v| v.parse().ok());
        for (k, part) in self.parts.iter().enumerate() {
            if skip == Some(k) {
                continue;
            }
            self.materials.use_lit(part.surface);
            self.materials.draw(&part.mesh, None);
        }
        draw_layer(&self.table_parts, gfx::Layer::Solid, &self.materials);
        if mirrored && hall {
            draw_layer(&self.table_parts, gfx::Layer::Inside, &self.materials);
            draw_layer(&self.table_parts, gfx::Layer::Detail, &self.materials);
        }
    }

    /// Soft halos round the lights of the hall, facing the camera: the glow
    /// a bright bulb leaves in the eye (High and Ultra).
    fn draw_halos(&mut self, cam: &SceneCamera) {
        if self.glows.is_empty() {
            return;
        }
        let forward = (cam.target - cam.eye).normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = right.cross(forward);
        // Far to near, so the nearer ones blend over the farther.
        self.glow_order.clear();
        self.glow_order.extend(0..self.glows.len());
        let glows = &self.glows;
        let eye = cam.eye;
        self.glow_order.sort_by(|&a, &b| {
            glows[b].at.distance_squared(eye).total_cmp(&glows[a].at.distance_squared(eye))
        });
        let mut b = MeshBuilder::default();
        for &k in &self.glow_order {
            let g = &glows[k];
            if (g.lamp && !self.lamp_visible) || (g.at - eye).dot(forward) < 0.1 {
                continue;
            }
            // Drawn a little towards the camera, so the bulb's own shade does
            // not cut the halo in half.
            let to_eye = (eye - g.at).normalize_or_zero();
            let c = g.at + to_eye * g.size * 0.6;
            let (r, u) = (right * g.size, up * g.size);
            b.quad(
                [c - r - u, c + r - u, c + r + u, c - r + u],
                [vec2(0.0, 1.0), vec2(1.0, 1.0), vec2(1.0, 0.0), vec2(0.0, 0.0)],
                g.color,
            );
        }
        if !b.is_empty() {
            self.materials.use_glow(Surface::UNLIT);
            b.build_dynamic(&self.glow_tex).draw();
        }
    }

    fn draw_ball_shadows(&self, balls: &[Ball]) {
        let r = self.ball_radius();
        let mut b = MeshBuilder::default();
        for ball in balls.iter().filter(|b| b.in_play()) {
            let p = to_world(ball.position);
            let height = (p.y - TABLE_HEIGHT - r).max(0.0);
            // Offset away from the nearest lamp, like a real overhead light.
            let lamp = self
                .lamps
                .iter()
                .min_by(|a, c| a.distance(p).total_cmp(&c.distance(p)))
                .copied()
                .unwrap_or(Vec3::Y);
            let away = vec3(p.x - lamp.x, 0.0, p.z - lamp.z) * 0.035;
            let size = r * (1.35 + height * 6.0);
            let alpha = (0.7 - height * 4.0).clamp(0.15, 0.7);
            let c = vec3(p.x + away.x, TABLE_HEIGHT + 0.0008, p.z + away.z);
            b.quad(
                [
                    c + vec3(-size, 0.0, size),
                    c + vec3(size, 0.0, size),
                    c + vec3(size, 0.0, -size),
                    c + vec3(-size, 0.0, -size),
                ],
                [vec2(0.0, 1.0), vec2(1.0, 1.0), vec2(1.0, 0.0), vec2(0.0, 0.0)],
                Color::new(1.0, 1.0, 1.0, alpha),
            );
            // A tight, darker contact shadow right under the ball.
            if height < 0.004 {
                let c = vec3(p.x, TABLE_HEIGHT + 0.0009, p.z);
                let s = r * 0.8;
                b.quad(
                    [
                        c + vec3(-s, 0.0, s),
                        c + vec3(s, 0.0, s),
                        c + vec3(s, 0.0, -s),
                        c + vec3(-s, 0.0, -s),
                    ],
                    [vec2(0.0, 1.0), vec2(1.0, 1.0), vec2(1.0, 0.0), vec2(0.0, 0.0)],
                    Color::new(1.0, 1.0, 1.0, 0.75),
                );
            }
        }
        if !b.vertices.is_empty() {
            self.materials.use_blended(Surface::UNLIT.cut(1.0));
            b.build_dynamic(&self.shadow).draw();
        }
    }

    /// Where each ball is drawn this frame (pocketed balls roll over the lip
    /// and drop); worked out once, as the floor's reflection draws them too.
    fn place_balls(&mut self, balls: &[Ball], dt: f32) {
        self.ball_poses.clear();
        for ball in balls {
            match ball.status {
                BallStatus::InPlay => {
                    self.sinking.remove(&ball.id);
                    let model = Mat4::from_rotation_translation(
                        ball_rotation(ball.orientation),
                        to_world(ball.position),
                    );
                    self.ball_poses.push((ball.id.index(), model));
                }
                BallStatus::Pocketed(pocket) => {
                    let p = self.table.pocket(pocket);
                    let sink = self.sinking.entry(ball.id).or_insert_with(|| Sinking::new(ball, p));
                    if !sink.fall(p, self.table.spec.ball_radius, dt as f64) {
                        continue;
                    }
                    let model = Mat4::from_rotation_translation(
                        ball_rotation(sink.orientation),
                        to_world(sink.position),
                    );
                    self.ball_poses.push((ball.id.index(), model));
                }
                BallStatus::OffTable => {}
            }
        }
    }

    fn draw_placed_balls(&self) {
        self.materials.use_lit(Surface::BALL);
        for &(k, model) in &self.ball_poses {
            self.materials.draw(&self.balls[k], Some(model));
        }
    }

    /// The ghost ball (where the cue ball will be at its first contact): a
    /// see-through ball, lit like the others. Only the half that faces the
    /// viewer is drawn: as a whole sphere its far side showed through the
    /// near one in patches (whichever triangles happened to come first),
    /// and whatever stood over it (a cushion's nose, a pocket's casting)
    /// cut it off, so it read as a squashed ball. It is drawn a little
    /// towards the viewer and smaller by as much (the same picture on
    /// screen), so nothing at the table's height cuts into it.
    fn draw_ghost_ball(&self, center: Vec3, cam: &SceneCamera) {
        let to_eye = cam.eye - center;
        let dist = to_eye.length();
        if dist < 1e-3 {
            return;
        }
        let toward = to_eye / dist;
        let mut right = cam.up.cross(toward);
        if right.length_squared() < 1e-6 {
            right = Vec3::X.cross(toward);
        }
        let right = right.normalize();
        let up = toward.cross(right).normalize();
        let lift = (dist * 0.5).min(0.06);
        let scale = (dist - lift) / dist;
        let mut b = MeshBuilder::default();
        b.dome(
            center + toward * lift,
            self.ball_radius() * scale,
            right,
            up,
            toward,
            40,
            12,
            Color::new(1.0, 1.0, 1.0, 0.30),
        );
        self.materials.use_blended(Surface::lit(0.6, 60.0, 0.0, 0.0));
        b.build_dynamic(&self.white).draw();
    }

    fn draw_guides(&self, guides: &[Guide], cam: &SceneCamera) {
        if guides.is_empty() {
            return;
        }
        // The size of a pixel at `p` (m): lines are never thinner than a
        // pixel, and their edges fade out over most of one, so they are
        // smooth however far the camera is and whatever the screen.
        let pixel = 2.0 * (cam.fovy / 2.0).tan() / screen_height().max(1.0);
        let px = |p: Vec3| p.distance(cam.eye) * pixel;
        let mut b = MeshBuilder::default();
        for g in guides {
            match *g {
                Guide::Line { from, to, width, color } => {
                    let d = to - from;
                    if d.length() < 1e-5 {
                        continue;
                    }
                    // Split long lines so the width can grow with distance.
                    let steps = (d.length() / 0.1).ceil().max(1.0) as usize;
                    for k in 0..steps {
                        let a = from + d * (k as f32 / steps as f32);
                        let c = from + d * ((k + 1) as f32 / steps as f32);
                        let (pa, pc) = (px(a), px(c));
                        b.soft_line(a, c, width.max(pa), width.max(pc), pa * 0.8, pc * 0.8, color);
                    }
                }
                Guide::Ring { center, radius, width, color } => {
                    let p = px(center);
                    b.soft_ring(center, radius, width.max(p), p * 0.8, 64, color);
                }
            }
        }
        self.materials.use_blended(Surface::UNLIT);
        b.build_dynamic(&self.white).draw();
    }

    /// Position and orientation of the cue for the given stroke.
    pub fn cue_pose(
        &self,
        params: &PhysicsParams,
        cue_ball: DVec3,
        yaw: f64,
        elevation: f64,
        tip: DVec2,
        gap: f64,
    ) -> CuePose {
        let r = self.table.spec.ball_radius;
        let forward = DVec3::new(yaw.cos(), yaw.sin(), 0.0);
        let right = forward.cross(DVec3::Z);
        let (s, c) = elevation.sin_cos();
        let axis = forward * c - DVec3::Z * s;
        let up = DVec3::Z * c + forward * s;
        let a = tip.x * params.max_tip_offset * r;
        let b = tip.y * params.max_tip_offset * r;
        let depth = (r * r - a * a - b * b).max(0.0).sqrt();
        let contact = right * a + up * b - axis * depth;
        let tip_pos = cue_ball + contact - axis * gap;
        CuePose {
            tip: to_world(tip_pos),
            rotation: Quat::from_rotation_arc(Vec3::Y, -dir_to_world(axis).normalize()),
        }
    }

    /// Centre of the hole drawn for `pocket` (for highlights and clicks).
    pub fn pocket_center(&self, pocket: PocketId) -> DVec2 {
        pocket_center_of(&self.table, pocket)
    }

    pub fn pocket_radius(&self, pocket: PocketId) -> f32 {
        pocket_radius_of(&self.table, pocket)
    }
}

/// Draws the parts of `parts` in `layer`.
fn draw_layer(parts: &[gfx::Built], layer: gfx::Layer, materials: &Materials) {
    for part in parts.iter().filter(|p| p.layer == layer) {
        if layer == gfx::Layer::Haze {
            materials.use_glow(part.surface);
        } else if layer == gfx::Layer::Blended {
            materials.use_blended(part.surface);
        } else {
            materials.use_lit(part.surface);
        }
        materials.draw(&part.mesh, None);
    }
}

/// The camera seen in the floor: the world mirrored under it (y = 0).
struct MirrorCamera<'a>(&'a SceneCamera);

impl Camera for MirrorCamera<'_> {
    fn matrix(&self) -> Mat4 {
        self.0.matrix() * Mat4::from_scale(vec3(1.0, -1.0, 1.0))
    }
    fn depth_enabled(&self) -> bool {
        true
    }
    fn render_pass(&self) -> Option<macroquad::texture::RenderPass> {
        None
    }
    fn viewport(&self) -> Option<(i32, i32, i32, i32)> {
        None
    }
}

/// Textures made away from the main thread (see [`Scene::set_looks`]).
struct Made {
    looks: Looks,
    felt: Option<textures::Pixels>,
    balls: Option<Vec<textures::Pixels>>,
    theme: Option<(TableStyle, gfx::Kit)>,
    hall: Option<(HallStyle, gfx::Kit)>,
}

/// The hall around `table`, with its lamp.
fn hall_kit(table: &Table, quality: Quality, hall: HallStyle) -> gfx::Kit {
    crate::halls::build(quality, half_table(table), hall)
}

/// The table's half length and width with its rails (and a little more).
fn half_table(table: &Table) -> Vec2 {
    vec2(
        table.spec.length as f32 / 2.0 + table.spec.rail_width as f32 + 0.05,
        table.spec.width as f32 / 2.0 + table.spec.rail_width as f32 + 0.05,
    )
}

fn felt_size(quality: Quality) -> u16 {
    if quality == Quality::Ultra { 1024 } else { 512 }
}

/// The sixteen ball textures, made side by side on all cores.
fn ball_pixels(size: u16, palette: &textures::Palette) -> Vec<textures::Pixels> {
    let ids: Vec<BallId> = BallId::all().collect();
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 8);
    let chunk = ids.len().div_ceil(threads);
    std::thread::scope(|s| {
        let handles: Vec<_> = ids
            .chunks(chunk)
            .map(|part| {
                s.spawn(move || {
                    part.iter().map(|id| textures::ball(*id, size, palette)).collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("ball texture")).collect()
    })
}

/// What the shop changes on the table itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Looks {
    pub cloth: [u8; 3],
    pub balls: textures::Palette,
    pub table: TableStyle,
    pub hall: HallStyle,
}

impl Looks {
    /// From the codes of the equipped cloth and ball set.
    pub fn of(cloth: &str, balls: &str) -> Looks {
        let cloth = match snook_legend_proto::shop::item(cloth).map(|i| i.look) {
            Some(snook_legend_proto::shop::Look::Cloth(c)) => c,
            _ => [24, 96, 48],
        };
        Looks {
            cloth,
            balls: textures::Palette::of(balls),
            table: TableStyle::Classic,
            hall: HallStyle::Pool,
        }
    }

    /// With the table and hall of these codes (the defaults if unknown).
    pub fn themed(mut self, table: &str, hall: &str) -> Looks {
        use snook_legend_proto::shop::{Look, item};
        if let Some(Look::Table(t)) = item(table).map(|i| i.look) {
            self.table = t;
        }
        if let Some(Look::Hall(h)) = item(hall).map(|i| i.look) {
            self.hall = h;
        }
        self
    }
}

impl Default for Looks {
    fn default() -> Self {
        Looks::of("cloth_green", "balls_classic")
    }
}

/// A cue of the given design, chalked with `chalk` (shop codes), with the
/// marks of its `level` (see [`cue_marks`]); and, at the top level, a soft
/// glow drawn see-through over its gold.
pub fn cue_mesh(
    cue: &str,
    chalk: &str,
    level: u8,
    white: &Texture2D,
) -> (gfx::Model, Option<gfx::Model>) {
    use snook_legend_proto::shop::CueStyle;
    use snook_legend_proto::shop::{Look, item};
    let (butt, accent, ring, style, shaft) = match item(cue).map(|i| i.look) {
        Some(Look::Cue { butt, accent, ring, style, shaft }) => (butt, accent, ring, style, shaft),
        _ => (
            [35, 20, 12],
            [20, 20, 24],
            [240, 238, 228],
            CueStyle::Plain,
            snook_legend_proto::shop::MAPLE,
        ),
    };
    let chalk = match item(chalk).map(|i| i.look) {
        Some(Look::Chalk(c)) => c,
        _ => [40, 80, 150],
    };
    let c = |v: [u8; 3]| rgb(v[0], v[1], v[2]);
    let mut m = MeshBuilder::default();
    // A regulation pool cue: a 13 mm tip, 58" long, 19 oz (the physics' mass).
    m.frustum(-0.004, 0.0064, 0.0, 0.0065, 12, c(chalk)); // tip, chalked
    m.frustum(0.0, 0.0065, 0.022, 0.0066, 12, rgb(240, 238, 228)); // ferrule
    m.frustum(0.022, 0.0066, 0.80, 0.0105, 16, c(shaft)); // maple (or carbon) shaft
    m.frustum(0.80, 0.0105, 0.815, 0.0106, 16, c(ring)); // joint ring
    // The forearm, with the cue's pattern in the accent colour (as in the
    // shop), so everyone at the table can tell the cue apart.
    let b = c(butt);
    let paint = crate::cues::Paint::of(butt, accent, ring);
    let (round, along) = crate::cues::resolution(style);
    if style == CueStyle::Plain {
        m.frustum(0.815, 0.0106, 1.10, 0.0125, 16, b);
    } else {
        m.painted_frustum(0.815, 0.0106, 1.10, 0.0125, round, along, |u, v| {
            crate::cues::pattern(style, paint, u, v)
        });
    }
    m.frustum(1.10, 0.0125, 1.115, 0.0126, 16, c(ring)); // ring
    m.frustum(1.115, 0.0126, 1.35, 0.0138, 16, c(accent)); // wrap
    if crate::cues::paints_sleeve(style) {
        // The finest cues carry the pattern on the butt sleeve too.
        m.painted_frustum(1.35, 0.0138, 1.47, 0.0148, round, along / 3, |u, v| {
            crate::cues::pattern(style, paint, u, v)
        });
    } else {
        m.frustum(1.35, 0.0138, 1.47, 0.0148, 16, c(butt)); // butt
    }
    m.frustum(1.47, 0.0148, 1.48, 0.0, 16, c(ring)); // cap
    let glow = cue_marks(&mut m, level, b);
    (m.build(white), glow.map(|g| g.build(white)))
}

/// The radius of the cue at `y` (from the tip), for marks laid on it.
fn cue_radius(y: f32) -> f32 {
    let lerp = |y0: f32, r0: f32, y1: f32, r1: f32| r0 + (r1 - r0) * ((y - y0) / (y1 - y0));
    if y < 1.10 {
        lerp(0.815, 0.0106, 1.10, 0.0125)
    } else if y < 1.35 {
        lerp(1.115, 0.0126, 1.35, 0.0138)
    } else {
        lerp(1.35, 0.0138, 1.47, 0.0148)
    }
}

/// The marks of a cue's level, purely for show: metal bands near the joint
/// (silver, then gold), then pearl inlays, then an engraved gold band; the
/// top level's gold glows (the see-through glow is returned).
fn cue_marks(m: &mut MeshBuilder, level: u8, wood: Color) -> Option<MeshBuilder> {
    if level == 0 {
        return None;
    }
    let silver = rgb(214, 218, 226);
    let gold = rgb(236, 194, 92);
    let pearl = rgb(244, 240, 228);
    let metal = if level >= 4 { gold } else { silver };
    // Laid a hair above the wood, so nothing flickers.
    let lift = 0.00025;
    let band = |m: &mut MeshBuilder, y: f32, h: f32, color: Color| {
        m.frustum(y, cue_radius(y) + lift, y + h, cue_radius(y + h) + lift, 16, color);
    };
    // Bands just below the joint: one at level 1, two from level 2, three
    // from level 5.
    let bands = match level {
        1 => 1,
        2..=4 => 2,
        _ => 3,
    };
    for k in 0..bands {
        band(m, 0.826 + k as f32 * 0.012, 0.005, metal);
    }
    // Pearl diamonds round the forearm from level 3 (and on the butt sleeve
    // from level 5).
    let diamonds = |m: &mut MeshBuilder, y0: f32, y1: f32, count: f32, inlay: Color| {
        let (r0, r1) = (cue_radius(y0) + lift * 0.6, cue_radius(y1) + lift * 0.6);
        m.painted_frustum(y0, r0, y1, r1, 48, 10, |u, v| {
            let d = ((u * count).fract() - 0.5).abs() * 2.0 + (v - 0.5).abs() * 2.0;
            if d < 0.8 { inlay } else { wood }
        });
    };
    if level >= 3 {
        let y = 0.826 + bands as f32 * 0.012 + 0.004;
        diamonds(m, y, y + 0.03, 6.0, pearl);
    }
    if level >= 4 {
        // A gold ring on the butt sleeve.
        band(m, 1.13, 0.006, gold);
    }
    if level >= 5 {
        diamonds(m, 1.145, 1.18, 8.0, pearl);
        band(m, 1.19, 0.006, gold);
    }
    if level < 6 {
        return None;
    }
    // The top level: a wide engraved gold band at the joint (a running
    // scroll cut into the gold) and a soft glow around it.
    let (y0, y1) = (0.815, 0.824);
    let deep = rgb(150, 104, 36);
    m.painted_frustum(y0, cue_radius(y0) + lift, y1, cue_radius(y1) + lift, 48, 6, |u, v| {
        let wave = ((u * 12.0 * std::f32::consts::TAU).sin() * 0.35 + 0.5 - v).abs();
        if wave < 0.12 { deep } else { gold }
    });
    // The glow: the gold lit from within (drawn over it, see-through), and
    // a faint haze round it that fades out along the cue.
    let mut glow = MeshBuilder::default();
    glow.frustum(
        y0,
        cue_radius(y0) + lift * 2.0,
        y1,
        cue_radius(y1) + lift * 2.0,
        24,
        Color::new(1.0, 0.86, 0.5, 0.35),
    );
    let rings = [(0.79f32, 0.0f32), (0.812, 0.1), (0.828, 0.1), (0.86, 0.0)];
    for w in rings.windows(2) {
        let ((ya, aa), (yb, ab)) = (w[0], w[1]);
        let (ra, rb) = (cue_radius(ya.max(0.815)) * 1.3, cue_radius(yb.max(0.815)) * 1.3);
        for k in 0..20 {
            let (t0, t1) = (
                k as f32 / 20.0 * std::f32::consts::TAU,
                (k + 1) as f32 / 20.0 * std::f32::consts::TAU,
            );
            let d = |t: f32| vec3(t.cos(), 0.0, t.sin());
            let ca = Color::new(1.0, 0.8, 0.4, aa);
            let cb = Color::new(1.0, 0.8, 0.4, ab);
            let i = [
                glow.push(d(t0) * ra + Vec3::Y * ya, Vec2::ZERO, d(t0), ca),
                glow.push(d(t1) * ra + Vec3::Y * ya, Vec2::ZERO, d(t1), ca),
                glow.push(d(t1) * rb + Vec3::Y * yb, Vec2::ZERO, d(t1), cb),
                glow.push(d(t0) * rb + Vec3::Y * yb, Vec2::ZERO, d(t0), cb),
            ];
            glow.indices.extend_from_slice(&[i[0], i[1], i[2], i[0], i[2], i[3]]);
        }
    }
    Some(glow)
}

pub struct CuePose {
    pub tip: Vec3,
    pub rotation: Quat,
}

/// Flat markings drawn on the cloth.
pub enum Guide {
    Line { from: Vec3, to: Vec3, width: f32, color: Color },
    Ring { center: Vec3, radius: f32, width: f32, color: Color },
}

/// Per-frame extras drawn over the table.
#[derive(Default)]
pub struct Overlay {
    pub cue: Option<CuePose>,
    /// The shooter's cue and chalk (shop codes; empty: the classic ones).
    pub cue_look: (String, String),
    /// The level of the shooter's cue (its marks).
    pub cue_level: u8,
    /// Where the cue ball will touch the first ball: drawn as a see-through ball.
    pub ghost: Option<Vec3>,
    pub guides: Vec<Guide>,
}

pub const RAIL_HEIGHT: f32 = RAIL_TOP;

/// Length of the cue mesh, tip to butt (see `cue_mesh`).
const CUE_LENGTH: f32 = 1.48;

/// Radius of the hole drawn for a pocket: exactly where balls drop.
/// A ball that has gone over the lip of a pocket: it keeps the velocity it
/// came in with and falls under gravity, held by the lip it rolls over and
/// by the pocket's liner, until it is out of sight.
struct Sinking {
    position: DVec3,
    velocity: DVec3,
    spin: DVec3,
    orientation: ::glam::DQuat,
    time: f64,
    /// A ball taken off the table where it lay (a penalty ball of the
    /// house rules): it rolls to the pocket before it drops.
    rolling: bool,
}

/// The highest the eye is taken to be for the lighting (m above the
/// floor): just under the halls' ceilings, where the overhead view was.
const LIGHT_EYE_MAX_Y: f32 = 2.75;

/// How deep below the cloth a ball's centre is when it is out of sight (m).
const SINK_DEPTH: f64 = 0.065;

impl Sinking {
    fn new(ball: &Ball, pocket: &snook_legend_core::Pocket) -> Sinking {
        // A ball that fell in is at the pocket; one resting out on the
        // cloth was taken off as a penalty.
        let away = ball.xy().distance(pocket.hole_center) > pocket.hole_radius + 0.06;
        Sinking {
            position: ball.position,
            velocity: ball.velocity,
            spin: ball.angular_velocity,
            orientation: ball.orientation,
            time: 0.0,
            rolling: away && ball.velocity.length() < 1e-3,
        }
    }

    /// Moves the ball on by `dt`; false once it is out of sight.
    fn fall(&mut self, pocket: &snook_legend_core::Pocket, r: f64, dt: f64) -> bool {
        const GRAVITY: f64 = 9.81;
        // The liner (leather or rubber) takes nearly all of a ball's pace.
        const LINER_RESTITUTION: f64 = 0.2;
        if self.position.z < -SINK_DEPTH || self.time > 2.0 {
            return false;
        }
        if self.rolling {
            // Across the cloth to the pocket at a brisk roll, then in.
            const PACE: f64 = 1.8;
            let to = pocket.hole_center - self.position.truncate();
            let d = to.length();
            let step = PACE * dt.min(0.1);
            if d <= step + pocket.hole_radius * 0.5 {
                self.rolling = false;
                self.velocity = (to / d.max(1e-9) * PACE * 0.6).extend(0.0);
            } else {
                let dir = to / d;
                self.position += (dir * step).extend(0.0);
                let axis = ::glam::DVec3::new(-dir.y, dir.x, 0.0);
                self.spin = axis * (PACE / r);
                let q = ::glam::DQuat::from_axis_angle(axis, step / r);
                self.orientation = (q * self.orientation).normalize();
                return true;
            }
        }
        let mut left = dt.min(0.1);
        while left > 0.0 {
            let h = left.min(0.002);
            left -= h;
            self.time += h;
            self.velocity.z -= GRAVITY * h;
            self.position += self.velocity * h;
            // The room the centre has around the hole's axis: the whole hole
            // while the ball still rides the lip, less and less as it goes
            // down past it, a ball's radius from the liner at the back.
            let rel = self.position.truncate() - pocket.hole_center;
            let d = rel.length();
            if d > 1e-9 {
                let out = rel / d;
                let z = self.position.z.clamp(0.0, r);
                let over_lip = pocket.hole_radius - (r * r - z * z).max(0.0).sqrt();
                let at_liner = pocket.hole_radius - r;
                let back = ((out.dot(pocket.outward) + 0.5) / 0.5).clamp(0.0, 1.0);
                let back = back * back * (3.0 - 2.0 * back);
                let room = (over_lip + (at_liner - over_lip) * back).max(0.0);
                if d > room {
                    // Never snapped: eased back in (a ball can come in beside
                    // the hole of a lined side pocket).
                    let pull = (d - room).min(1.5 * h + (d - room) * 0.2);
                    self.position -= (out * pull).extend(0.0);
                    let vr = self.velocity.truncate().dot(out);
                    if vr > 0.0 {
                        self.velocity -= (out * vr * (1.0 + LINER_RESTITUTION)).extend(0.0);
                        // The rub against the liner takes the rest down too.
                        self.velocity.x *= 0.9;
                        self.velocity.y *= 0.9;
                        self.spin *= 0.8;
                    }
                }
            }
            let rot = self.spin * h;
            let angle = rot.length();
            if angle > 0.0 {
                let q = ::glam::DQuat::from_axis_angle(rot / angle, angle);
                self.orientation = (q * self.orientation).normalize();
            }
        }
        self.position.z >= -SINK_DEPTH
    }
}

pub(crate) fn pocket_radius_of(table: &Table, pocket: PocketId) -> f32 {
    table.pocket(pocket).hole_radius as f32
}

/// Centre of the hole drawn for a pocket.
pub(crate) fn pocket_center_of(table: &Table, pocket: PocketId) -> DVec2 {
    table.pocket(pocket).hole_center
}

/// Shortens the segment `a`→`b` so it ends where it enters the nearest pocket hole.
pub(crate) fn clip_to_hole(table: &Table, a: DVec2, b: DVec2) -> DVec2 {
    let Some(pocket) = table
        .pockets
        .iter()
        .min_by(|p, q| p.mouth_center.distance(b).total_cmp(&q.mouth_center.distance(b)))
    else {
        return b;
    };
    let c = pocket_center_of(table, pocket.id);
    let r = pocket_radius_of(table, pocket.id) as f64 + 0.001;
    let d = b - a;
    // Solve |a + t d - c| = r for the first t in [0, 1].
    let f = a - c;
    let (qa, qb, qc) = (d.dot(d), 2.0 * f.dot(d), f.dot(f) - r * r);
    let disc = qb * qb - 4.0 * qa * qc;
    if qa >= 1e-12 && disc >= 0.0 {
        let t = (-qb - disc.sqrt()) / (2.0 * qa);
        if (0.0..=1.0).contains(&t) {
            return a + d * t.max(0.05);
        }
    }
    // The jaw line passes beside the hole: stop where it comes closest to it,
    // so the throat never cuts a slit into the rail beyond the pocket.
    let t = (-f.dot(d) / qa.max(1e-12)).clamp(0.05, 1.0);
    a + d * t
}

/// The pocket jaws: (the cushion nose end, where it meets the hole, the
/// direction across the throat).
pub(crate) fn jaw_lines(table: &Table) -> Vec<(DVec2, DVec2, DVec2)> {
    let mut out = Vec::new();
    for seg in table.cushions.iter().filter(|c| !is_axis_aligned(c.a, c.b)) {
        let mid = (seg.a + seg.b) / 2.0;
        let dir = (seg.b - seg.a).normalize();
        let end = clip_to_hole(table, seg.a, seg.b);
        // The facing looks across the throat, towards the pocket's axis
        // (from the middle of its mouth to the hole): what side the mouth's
        // centre is on is not reliable for side pockets.
        let Some(pocket) = table
            .pockets
            .iter()
            .min_by(|x, y| x.mouth_center.distance(mid).total_cmp(&y.mouth_center.distance(mid)))
        else {
            continue;
        };
        let axis = (pocket_center_of(table, pocket.id) - pocket.mouth_center).normalize_or_zero();
        let rel = mid - pocket.mouth_center;
        let toward_axis = pocket.mouth_center + axis * rel.dot(axis) - mid;
        let across = if dir.perp().dot(toward_axis) >= 0.0 { dir.perp() } else { -dir.perp() };
        out.push((seg.a, end, across));
    }
    out
}

pub(crate) fn is_axis_aligned(a: DVec2, b: DVec2) -> bool {
    let d = b - a;
    d.x.abs() < 1e-6 || d.y.abs() < 1e-6
}

/// The face of a pocket jaw: the vertical cut through the cushion rubber and
/// the rail along the jaw line, from `a` (the cushion nose) to `b` (the hole).
/// Its top follows what was cut: the nose slope, the cushion top, then the rail.
pub(crate) fn jaw_wall(
    mb: &mut MeshBuilder,
    rail: &mut MeshBuilder,
    table: &Table,
    a: DVec2,
    b: DVec2,
    into_throat: DVec2,
    cast: Color,
) {
    let along = b - a;
    let len = along.length();
    if len < 1e-6 {
        return;
    }
    let dir = along / len;
    // Distance from the rail's cushion nose grows along the jaw.
    let rail_normal = table
        .cushions
        .iter()
        .filter(|c| is_axis_aligned(c.a, c.b))
        .find(|c| c.a.distance(a) < 1e-6 || c.b.distance(a) < 1e-6)
        .map(|c| {
            let n = (c.b - c.a).normalize().perp();
            if n.dot(c.a) < 0.0 { -n } else { n }
        })
        .unwrap_or(dir);
    let rate = dir.dot(rail_normal).abs().max(0.05);
    let nose = table.spec.cushion_height as f32;
    let height = |d: f64| -> f32 {
        if d < 0.008 {
            nose + (CUSHION_TOP - nose) * (d / 0.008) as f32
        } else if d < CUSHION_DEPTH as f64 {
            CUSHION_TOP
        } else {
            RAIL_TOP + CAST_H
        }
    };
    // Samples along the jaw, with a step where the rail starts.
    let d_end = len * rate;
    let mut ds: Vec<f64> = vec![0.0, 0.004, 0.008];
    let rail_d = CUSHION_DEPTH as f64;
    ds.extend([rail_d - 1e-4, rail_d + 1e-4]);
    let mut k = 0.012;
    while k < d_end {
        ds.push(k);
        k += 0.01;
    }
    ds.push(d_end);
    ds.retain(|d| *d <= d_end + 1e-9);
    ds.sort_by(|x, y| x.total_cmp(y));
    ds.dedup_by(|x, y| (*x - *y).abs() < 1e-7);
    // Lit like the cushion nose: facing the throat and tilted a little up,
    // so the facings don't read as black slabs.
    let n = surface(into_throat, 0.0) - surface(DVec2::ZERO, 0.0);
    let n = (n.normalize_or_zero() + Vec3::Y * 0.6).normalize_or_zero();
    let felt = rgb(170, 180, 170);
    // Where the jaw cuts through the rail, the facing is the pocket casting
    // (`cast`; only the rubber part is covered in cloth).
    for w in ds.windows(2) {
        let (d0, d1) = (w[0], w[1]);
        let p0 = a + dir * (d0 / rate);
        let p1 = a + dir * (d1 / rate);
        let (h0, h1) = (height(d0), height(d1));
        let in_rail = d0 >= rail_d - 1e-6;
        let (target, color) = if in_rail { (&mut *rail, cast) } else { (&mut *mb, felt) };
        // The facing goes on down into the pocket (seen where the mouth opens).
        let quad = [surface(p0, -0.06), surface(p1, -0.06), surface(p1, h1), surface(p0, h0)];
        let i: Vec<u16> = quad.iter().map(|q| target.push(*q, Vec2::ZERO, n, color)).collect();
        target.indices.extend_from_slice(&[i[0], i[1], i[2], i[0], i[2], i[3]]);
    }
}

/// A rounded rectangle around the origin in the world XZ plane (as `Vec2`
/// x, z), densely sampled. Loops with different radii have matching points,
/// so rings can be built between them.
pub(crate) fn rounded_loop(ex: f32, ez: f32, radius: f32) -> Vec<Vec2> {
    const CORNER: usize = 12;
    const STEP: f32 = 0.02;
    let r = radius.min(ex).min(ez);
    let corners = [
        (vec2(ex - r, ez - r), 0.0f32),
        (vec2(-(ex - r), ez - r), 90.0),
        (vec2(-(ex - r), -(ez - r)), 180.0),
        (vec2(ex - r, -(ez - r)), 270.0),
    ];
    // A fixed number of points per side, so every loop has the same count.
    let counts = [((2.2 / STEP) as usize).max(4), ((1.3 / STEP) as usize).max(4)];
    let mut pts = Vec::new();
    for (k, (c, a0)) in corners.iter().enumerate() {
        for j in 0..=CORNER {
            let a = (a0 + 90.0 * j as f32 / CORNER as f32).to_radians();
            pts.push(*c + vec2(a.cos(), a.sin()) * r);
        }
        // The straight side to the next corner (exclusive of both ends).
        let (c1, a1) = corners[(k + 1) % 4];
        let end = c1 + vec2(a1.to_radians().cos(), a1.to_radians().sin()) * r;
        let start = *pts.last().expect("corner points");
        let m = counts[k % 2];
        for j in 1..m {
            pts.push(start.lerp(end, j as f32 / m as f32));
        }
    }
    pts
}
