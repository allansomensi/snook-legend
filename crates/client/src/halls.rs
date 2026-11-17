//! The halls of the shop: the room around the table and the lamp over it.
//! The pool hall is in [`crate::room`]; the beach bar, the English club and
//! the loft are here.
//!
//! Like the pool hall, every hall is built from code and lit once (the light
//! is baked into the vertices, the room is drawn unlit), so it costs little
//! per frame. Every hall has the same size, so the cameras fit them all.

use macroquad::prelude::*;
use snook_legend_proto::shop::HallStyle;

use crate::config::Quality;
use crate::coords::TABLE_HEIGHT;
use crate::gfx::{Kit, Layer, MeshBuilder, Surface};
use crate::hall_textures as tx;
use crate::more_halls as more;
use crate::room::{HEIGHT, HX, HZ, disc_facing, hoop, picture, tube};
use crate::scene::LAMP_Y;
use crate::textures;

pub(crate) fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba(r, g, b, 255)
}

/// The hall `style` around a table of half size `table` (with its rails).
pub fn build(quality: Quality, table: Vec2, style: HallStyle) -> Kit {
    let mut kit = Kit::default();
    let reach = lamp_reach(table);
    let grow = reach - 0.45;
    match style {
        HallStyle::Pool => {
            crate::room::build(&mut kit, quality, table);
            pool_lamp(&mut kit, quality, reach);
        }
        HallStyle::Beach => {
            beach(&mut kit, quality, table);
            rattan_lamp(&mut kit, quality, reach);
        }
        HallStyle::Club => {
            club(&mut kit, quality, table);
            club_lamp(&mut kit, quality, reach);
        }
        HallStyle::Loft => {
            loft(&mut kit, quality, table);
            loft_lamp(&mut kit, quality, reach);
        }
        HallStyle::Lodge => {
            more::lodge(&mut kit, quality, table);
            more::lodge_lamp(&mut kit, quality, reach);
        }
        HallStyle::Arcade => {
            more::arcade(&mut kit, quality, table);
            more::arcade_lamp(&mut kit, reach);
        }
        HallStyle::Deco => {
            more::deco(&mut kit, quality, table);
            more::deco_lamp(&mut kit, quality, reach);
        }
        HallStyle::Penthouse => {
            more::penthouse(&mut kit, quality, table);
            more::penthouse_lamp(&mut kit, reach);
        }
    }
    if quality >= Quality::High {
        let (shade, warm) = match style {
            HallStyle::Club => (0.3, Color::new(1.0, 0.86, 0.62, 1.0)),
            HallStyle::Loft => (0.13, Color::new(1.0, 0.8, 0.55, 1.0)),
            HallStyle::Beach => (0.17, Color::new(1.0, 0.8, 0.55, 1.0)),
            HallStyle::Pool => (0.16, Color::new(1.0, 0.88, 0.66, 1.0)),
            HallStyle::Lodge => (0.17, Color::new(1.0, 0.82, 0.58, 1.0)),
            HallStyle::Arcade => (0.2, Color::new(0.95, 0.92, 1.0, 1.0)),
            HallStyle::Deco => (0.16, Color::new(1.0, 0.88, 0.68, 1.0)),
            HallStyle::Penthouse => (0.1, Color::new(1.0, 0.95, 0.85, 1.0)),
        };
        match style {
            // Long lamps (a canopy, a box, a bar) light a long block of haze.
            HallStyle::Club => box_haze(&mut kit, vec2(0.85 + grow, 0.3), warm),
            HallStyle::Arcade => box_haze(&mut kit, vec2(0.83 + grow, 0.19), warm),
            HallStyle::Penthouse => box_haze(&mut kit, vec2(0.87 + grow, 0.05), warm),
            _ => lamp_haze(&mut kit, quality, reach, shade, warm),
        }
    }
    kit
}

/// The haze under a long lamp of half size `half`: four soft sheets that
/// spread down to the cloth, fading at their ends and towards the cloth.
fn box_haze(kit: &mut Kit, half: Vec2, color: Color) {
    let mut haze = MeshBuilder::default();
    let (top, bottom) = (LAMP_Y - 0.01, TABLE_HEIGHT + 0.04);
    let spread = 0.4;
    let c = |sx: f32, sz: f32, y: f32| {
        let t = (y - bottom) / (top - bottom);
        let k = spread * (1.0 - t);
        vec3(sx * (half.x + k), y, sz * (half.y + k))
    };
    for (a, b) in [
        ((-1.0, 1.0), (1.0, 1.0)),
        ((1.0, 1.0), (1.0, -1.0)),
        ((1.0, -1.0), (-1.0, -1.0)),
        ((-1.0, -1.0), (-1.0, 1.0)),
    ] {
        let (nu, nv) = (10, 4);
        let first = haze.len() as u16;
        for j in 0..=nv {
            let y = bottom + (top - bottom) * j as f32 / nv as f32;
            for i in 0..=nu {
                let u = i as f32 / nu as f32;
                let p = c(a.0, a.1, y).lerp(c(b.0, b.1, y), u);
                let n = vec3(p.x, 0.0, p.z).normalize_or_zero();
                let t = j as f32 / nv as f32;
                let alpha = t.powf(1.2) * (u * std::f32::consts::PI).sin() * 0.12;
                haze.push(p, Vec2::ZERO, n, Color::new(color.r, color.g, color.b, alpha));
            }
        }
        let row = nu as u16 + 1;
        for j in 0..nv as u16 {
            for i in 0..nu as u16 {
                let k = first + j * row + i;
                haze.indices.extend_from_slice(&[k, k + 1, k + row + 1, k, k + row + 1, k + row]);
            }
        }
    }
    kit.add(haze, None, Surface::haze(1.0), Layer::Haze);
    for x in [-0.5f32, 0.0, 0.5] {
        kit.glow(
            vec3(x * half.x * 1.4, LAMP_Y - 0.02, 0.0),
            0.25,
            Color::new(1.0, 0.9, 0.7, 0.3),
            true,
        );
    }
}

/// The haze in the light under the lamp over the table: a soft cone from
/// each shade down to the cloth, and a halo under each shade (High, Ultra).
/// `shade`: the shade's radius.
fn lamp_haze(kit: &mut Kit, quality: Quality, reach: f32, shade: f32, color: Color) {
    let mut haze = MeshBuilder::default();
    let segs = if quality == Quality::Ultra { 40 } else { 24 };
    let (top, bottom) = (LAMP_Y - 0.01, TABLE_HEIGHT + 0.04);
    for x in lamp_xs(reach) {
        let at = vec3(x, bottom, 0.0);
        let first = haze.len();
        // In bands, so the haze can thin out towards the cloth.
        let bands = 4;
        for k in 0..bands {
            let (t0, t1) = (k as f32 / bands as f32, (k + 1) as f32 / bands as f32);
            let r = |t: f32| shade * 0.95 + (0.62 - shade) * (1.0 - t);
            let y = |t: f32| (top - bottom) * t;
            haze.frustum_at(at, y(1.0 - t1), r(1.0 - t1), y(1.0 - t0), r(1.0 - t0), segs, color);
        }
        for v in &mut haze.vertices[first..] {
            let t = ((v.position.y - bottom) / (top - bottom)).clamp(0.0, 1.0);
            v.color[3] = (t.powf(1.2) * 0.12 * 255.0) as u8;
        }
        kit.glow(vec3(x, LAMP_Y - 0.02, 0.0), shade * 1.8, Color::new(1.0, 0.86, 0.6, 0.5), true);
    }
    kit.add(haze, None, Surface::haze(2.0), Layer::Haze);
}

/// The hall's colour as the balls, the varnish and the brass reflect it.
pub fn tint(style: HallStyle) -> Vec3 {
    match style {
        HallStyle::Pool => vec3(0.07, 0.075, 0.05),
        HallStyle::Beach => vec3(0.22, 0.13, 0.10),
        HallStyle::Club => vec3(0.10, 0.05, 0.035),
        HallStyle::Loft => vec3(0.08, 0.08, 0.09),
        HallStyle::Lodge => vec3(0.12, 0.07, 0.04),
        HallStyle::Arcade => vec3(0.08, 0.03, 0.10),
        HallStyle::Deco => vec3(0.09, 0.08, 0.06),
        HallStyle::Penthouse => vec3(0.10, 0.11, 0.15),
    }
}

/// How fast what a polished floor reflects fades with height above it
/// (0: the floor does not reflect). See `Layer::Floor`.
pub fn mirror_fade(style: HallStyle) -> f32 {
    match style {
        HallStyle::Pool => 1.4,
        HallStyle::Beach => 0.0,
        HallStyle::Club => 1.2,
        HallStyle::Loft => 0.9,
        HallStyle::Lodge => 1.6,
        HallStyle::Arcade => 0.5,
        HallStyle::Deco => 0.7,
        HallStyle::Penthouse => 0.8,
    }
}

// --- Baked light ---------------------------------------------------------------

pub(crate) struct Light {
    pos: Vec3,
    color: Vec3,
    power: f32,
    /// A shade that only lights downwards.
    down: bool,
}

pub(crate) fn light(pos: Vec3, color: Vec3, power: f32, down: bool) -> Light {
    Light { pos, color, power, down }
}

/// The half length (with its rails) of the 7-foot table the lamps were
/// first drawn over: they hang 0.45 m apart over it.
const SEVEN_FOOT: f32 = 1.981 / 2.0 + 0.115 + 0.05;

/// How far from the middle the outer lamps over the table hang, for a
/// table of half size `table` (with its rails): the lamps span its length,
/// whatever its size.
pub(crate) fn lamp_reach(table: Vec2) -> f32 {
    0.45 * table.x / SEVEN_FOOT
}

/// Where the three lamps over the table hang along it.
pub(crate) fn lamp_xs(reach: f32) -> [f32; 3] {
    [-reach, 0.0, reach]
}

/// The three lamps over the table (every hall has them), for a table of
/// half size `table`.
pub(crate) fn table_lights(color: Vec3, table: Vec2) -> Vec<Light> {
    lamp_xs(lamp_reach(table))
        .into_iter()
        .map(|x| light(vec3(x, TABLE_HEIGHT + 0.93, 0.0), color, 1.0, true))
        .collect()
}

pub(crate) fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The size of the cells the light is baked on: finer cells give softer,
/// truer shadows and corners (Ultra bakes on a 10 cm grid).
pub(crate) fn cell(quality: Quality) -> f32 {
    match quality {
        Quality::Ultra => 0.1,
        Quality::High => 0.2,
        _ => 0.4,
    }
}

/// A box that stops light (the table, the bar, a sofa): it casts shadows
/// from every light of the hall and darkens what is near it.
#[derive(Clone, Copy)]
pub(crate) struct Occluder {
    min: Vec3,
    max: Vec3,
}

/// The ways the light from the hall's lamps is worked out per vertex.
#[derive(Clone, Copy, PartialEq)]
enum Shadows {
    /// Low, Medium: the floor under the table is darkened, nothing else.
    Simple,
    /// High: a hard shadow per light (soft enough on 20 cm cells).
    Hard,
    /// Ultra: soft shadows (several points on each light) and occlusion
    /// near every box.
    Soft,
}

/// How a hall is lit: its lights, the light that is everywhere, and the
/// boxes that cast shadows (the table first).
pub(crate) struct Lighting {
    lights: Vec<Light>,
    ambient: Vec3,
    table: Vec2,
    exposure: f32,
    occluders: Vec<Occluder>,
    shadows: Shadows,
    /// Glossy surfaces catch the lamps (High, Ultra).
    gloss: bool,
}

impl Lighting {
    /// `table`: the table's half size with its rails.
    pub(crate) fn new(
        quality: Quality,
        lights: Vec<Light>,
        ambient: Vec3,
        table: Vec2,
        exposure: f32,
    ) -> Lighting {
        let shadows = match quality {
            Quality::Ultra => Shadows::Soft,
            Quality::High => Shadows::Hard,
            _ => Shadows::Simple,
        };
        let mut lit = Lighting {
            lights,
            ambient,
            table: table + vec2(0.12, 0.12),
            exposure,
            occluders: Vec::new(),
            shadows,
            gloss: quality >= Quality::High,
        };
        // The table's body: the slate, the rails and the apron.
        lit.occlude(
            vec3(-table.x, TABLE_HEIGHT - 0.24, -table.y),
            vec3(table.x, TABLE_HEIGHT + 0.05, table.y),
        );
        // Its legs, near the corners.
        for (sx, sz) in [(1.0f32, 1.0f32), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
            let c = vec3(sx * (table.x - 0.25), 0.0, sz * (table.y - 0.22));
            lit.occlude(c + vec3(-0.08, 0.0, -0.08), c + vec3(0.08, TABLE_HEIGHT - 0.24, 0.08));
        }
        lit
    }

    /// Adds a box that casts shadows (before anything is baked).
    pub(crate) fn occlude(&mut self, min: Vec3, max: Vec3) {
        self.occluders.push(Occluder { min: min.min(max), max: min.max(max) });
    }

    /// Adds a box from its centre and size.
    pub(crate) fn occlude_box(&mut self, center: Vec3, size: Vec3) {
        self.occlude(center - size / 2.0, center + size / 2.0);
    }

    /// How much of the way from `p` to `to` is clear of the boxes (0 or 1).
    fn clear(&self, p: Vec3, to: Vec3) -> bool {
        let d = to - p;
        let inv = Vec3::ONE / d;
        self.occluders.iter().all(|o| {
            let t0 = (o.min - p) * inv;
            let t1 = (o.max - p) * inv;
            let near = t0.min(t1).max_element();
            let far = t0.max(t1).min_element();
            // The segment misses the box, or only touches it beyond its ends.
            near > far || far < 0.0 || near > 1.0
        })
    }

    /// How much light from `l` gets past the boxes to `p` (0..1).
    fn visible(&self, l: &Light, p: Vec3) -> f32 {
        match self.shadows {
            Shadows::Simple => 1.0,
            Shadows::Hard => self.clear(p, l.pos) as u8 as f32,
            Shadows::Soft => {
                // Five points across a shade about 25 cm wide.
                const SPOTS: [(f32, f32); 5] =
                    [(0.0, 0.0), (0.12, 0.03), (-0.12, -0.03), (0.03, -0.12), (-0.03, 0.12)];
                let seen =
                    SPOTS.iter().filter(|(x, z)| self.clear(p, l.pos + vec3(*x, 0.0, *z))).count();
                seen as f32 / SPOTS.len() as f32
            }
        }
    }

    /// Light kept from `p` by the boxes nearby: soft dark contact shadows
    /// where the table, a sofa or the bar meet the floor and the walls.
    fn occlusion(&self, p: Vec3, n: Vec3) -> f32 {
        if self.shadows != Shadows::Soft {
            return 1.0;
        }
        let mut ao = 1.0;
        for o in &self.occluders {
            let closest = p.clamp(o.min, o.max);
            let to = closest - p;
            let d = to.length();
            if !(0.01..=0.7).contains(&d) {
                continue;
            }
            let facing = (n.dot(to / d) * 0.5 + 0.5).max(0.2);
            let k = (1.0 - d / 0.7).powi(2) * facing * 0.5;
            ao *= 1.0 - k;
        }
        ao
    }

    /// The light reaching a point of the room (with soft corners), tone mapped.
    pub(crate) fn at(&self, p: Vec3, n: Vec3) -> Vec3 {
        let hemi = 0.72 + 0.28 * n.y;
        let occlusion = self.occlusion(p, n);
        let mut c = self.ambient * hemi * occlusion;
        let under_table = p.y < 0.75 && p.x.abs() < self.table.x && p.z.abs() < self.table.y;
        // Off the surface a little, so a face does not shadow itself.
        let from = p + n * 0.012;
        for l in &self.lights {
            let d = l.pos - p;
            let dist = d.length().max(0.05);
            let dir = d / dist;
            let ndl = (n.dot(dir) * 0.85 + 0.15).max(0.0);
            let cone = if l.down { 0.2 + 0.8 * smooth(0.15, 0.75, dir.y) } else { 1.0 };
            let mut att = l.power / (1.0 + dist * dist * 0.7);
            if self.shadows == Shadows::Simple {
                if under_table && l.pos.y < 2.2 {
                    att *= 0.15;
                }
            } else if ndl * att * cone > 0.004 {
                // Some light still gets round (it bounces off the walls).
                att *= 0.12 + 0.88 * self.visible(l, from);
            }
            c += l.color * ndl * att * cone;
        }
        let to_wall = (HX - p.x.abs()).min(HZ - p.z.abs());
        let ao = 0.55
            + 0.45
                * smooth(0.0, 0.45, p.y).min(smooth(0.0, 0.5, to_wall)).min(smooth(
                    0.0,
                    0.35,
                    HEIGHT - p.y,
                ));
        let c = c * ao * self.exposure;
        vec3(1.0 - (-c.x).exp(), 1.0 - (-c.y).exp(), 1.0 - (-c.z).exp())
    }

    /// Bakes the light into `b` and adds it to the kit (drawn unlit).
    pub(crate) fn add(&self, kit: &mut Kit, b: MeshBuilder, tex: Option<usize>) {
        self.add_as(kit, b, tex, Surface::UNLIT, Layer::Solid);
    }

    /// Like [`Lighting::add`], for a glossy surface: on High and Ultra it
    /// catches the lamps (`specular`, `shininess`) and the room (`coat`).
    pub(crate) fn shiny(
        &self,
        kit: &mut Kit,
        b: MeshBuilder,
        tex: Option<usize>,
        (specular, shininess, coat): (f32, f32, f32),
    ) {
        let surface =
            if self.gloss { Surface::baked(specular, shininess, coat) } else { Surface::UNLIT };
        self.add_as(kit, b, tex, surface, Layer::Solid);
    }

    /// The floor: drawn first, and on Ultra laid over its own reflection,
    /// `reflect` (0..1) being how much of it is seen (see `mirror_fade`).
    pub(crate) fn floor(
        &self,
        kit: &mut Kit,
        mut b: MeshBuilder,
        tex: Option<usize>,
        reflect: f32,
        gloss: (f32, f32, f32),
    ) {
        let alpha = ((1.0 - reflect) * 255.0).round() as u8;
        for v in &mut b.vertices {
            v.color[3] = alpha;
        }
        let (specular, shininess, coat) = gloss;
        // The floor fills much of the screen: its gloss is for Ultra only,
        // so High stays light on integrated graphics.
        let surface = if self.shadows == Shadows::Soft {
            Surface::baked(specular, shininess, coat)
        } else {
            Surface::UNLIT
        };
        self.add_as(kit, b, tex, surface, Layer::Floor);
    }

    /// Adds `b` with the finish `f` (see [`Finish`]).
    pub(crate) fn put(&self, kit: &mut Kit, b: MeshBuilder, tex: Option<usize>, f: Finish) {
        match f {
            Finish::Matte => self.add(kit, b, tex),
            Finish::Glow => add_glow(kit, b, tex),
            Finish::Shiny(g) => self.shiny(kit, b, tex, g),
            Finish::Floor(reflect, g) => self.floor(kit, b, tex, reflect, g),
        }
    }

    fn add_as(
        &self,
        kit: &mut Kit,
        mut b: MeshBuilder,
        tex: Option<usize>,
        surface: Surface,
        layer: Layer,
    ) {
        b.bake(|p, n| self.at(p, n));
        kit.add(b, tex, surface, layer);
    }
}

/// How a part of a hall is finished (see [`Lighting::put`]).
#[derive(Clone, Copy)]
pub(crate) enum Finish {
    /// Baked light only.
    Matte,
    /// It shines by itself.
    Glow,
    /// Baked, and glossy on High and Ultra.
    Shiny((f32, f32, f32)),
    /// The floor: how much of it reflects (Ultra), and its gloss.
    Floor(f32, (f32, f32, f32)),
}

/// Glossy finishes for [`Lighting::shiny`]: (specular, shininess, coat).
pub(crate) const VARNISH: (f32, f32, f32) = (0.5, 60.0, 0.35);
pub(crate) const BRASS: (f32, f32, f32) = (1.6, 90.0, 0.6);
pub(crate) const GLASS: (f32, f32, f32) = (2.0, 160.0, 0.8);
pub(crate) const LEATHER: (f32, f32, f32) = (0.3, 24.0, 0.12);
pub(crate) const SATIN: (f32, f32, f32) = (0.18, 20.0, 0.1);

/// Things that shine by themselves (bulbs, flames, the sky outside).
pub(crate) fn add_glow(kit: &mut Kit, b: MeshBuilder, tex: Option<usize>) {
    kit.add(b, tex, Surface::UNLIT, Layer::Solid);
}

// --- Shared shapes -------------------------------------------------------------

/// A wall: a point on its floor line, the direction along it (so that
/// `along × Y` faces into the room), its length.
pub(crate) struct Wall {
    pub(crate) start: Vec3,
    pub(crate) along: Vec3,
    pub(crate) length: f32,
    pub(crate) inward: Vec3,
}

pub(crate) fn walls() -> [Wall; 4] {
    [
        // Back (-Z), front (+Z), left (-X), right (+X).
        Wall { start: vec3(-HX, 0.0, -HZ), along: Vec3::X, length: 2.0 * HX, inward: Vec3::Z },
        Wall { start: vec3(HX, 0.0, HZ), along: -Vec3::X, length: 2.0 * HX, inward: -Vec3::Z },
        Wall { start: vec3(-HX, 0.0, HZ), along: -Vec3::Z, length: 2.0 * HZ, inward: Vec3::X },
        Wall { start: vec3(HX, 0.0, -HZ), along: Vec3::Z, length: 2.0 * HZ, inward: -Vec3::X },
    ]
}

/// A band of wall between two heights, from `a` to `b` along it (metres).
#[allow(clippy::too_many_arguments)]
pub(crate) fn band(
    mb: &mut MeshBuilder,
    w: &Wall,
    a: f32,
    b: f32,
    y0: f32,
    y1: f32,
    cell: f32,
    color: Color,
    uv: f32,
) {
    let len = b - a;
    mb.grid(
        w.start + w.along * a + Vec3::Y * y0,
        w.along * len,
        Vec3::Y * (y1 - y0),
        (len / cell).ceil().max(1.0) as u32,
        ((y1 - y0) / cell).ceil().max(1.0) as u32,
        color,
        uv,
    );
    // World-aligned texture coordinates, so bands of one wall line up.
    let first = mb.vertices.len()
        - ((len / cell).ceil().max(1.0) as usize + 1)
            * (((y1 - y0) / cell).ceil().max(1.0) as usize + 1);
    for v in &mut mb.vertices[first..] {
        let along = (v.position - w.start).dot(w.along);
        v.uv = vec2(along * uv, -v.position.y * uv);
    }
}

/// The floor and the ceiling.
pub(crate) fn floor_and_ceiling(
    floor: &mut MeshBuilder,
    ceiling: &mut MeshBuilder,
    cell: f32,
    floor_uv: f32,
    ceiling_uv: f32,
    ceiling_color: Color,
) {
    let (nx, nz) = ((2.0 * HX / cell) as u32, (2.0 * HZ / cell) as u32);
    floor.grid(
        vec3(-HX, 0.0, HZ),
        vec3(2.0 * HX, 0.0, 0.0),
        vec3(0.0, 0.0, -2.0 * HZ),
        nx,
        nz,
        WHITE,
        floor_uv,
    );
    ceiling.grid(
        vec3(-HX, HEIGHT, -HZ),
        vec3(2.0 * HX, 0.0, 0.0),
        vec3(0.0, 0.0, 2.0 * HZ),
        nx,
        nz,
        ceiling_color,
        ceiling_uv,
    );
}

/// A rug lying on the floor (the whole texture once).
pub(crate) fn rug(mb: &mut MeshBuilder, rx: f32, rz: f32) {
    let first = mb.len();
    mb.grid(
        vec3(-rx, 0.004, rz),
        vec3(2.0 * rx, 0.0, 0.0),
        vec3(0.0, 0.0, -2.0 * rz),
        24,
        18,
        WHITE,
        1.0,
    );
    for v in &mut mb.vertices[first..] {
        v.uv = vec2((v.position.x + rx) / (2.0 * rx), (v.position.z + rz) / (2.0 * rz));
    }
}

/// A picture seen through an opening: the whole texture on a big quad
/// `out` metres behind the wall.
/// `repeat`: how many times the picture goes across (it must tile then).
pub(crate) fn backdrop(
    mb: &mut MeshBuilder,
    w: &Wall,
    out: f32,
    y0: f32,
    y1: f32,
    flip: bool,
    repeat: f32,
) {
    let center = w.start + w.along * (w.length / 2.0) - w.inward * out + Vec3::Y * (y0 + y1) / 2.0;
    let right = if flip { -w.along } else { w.along };
    let first = mb.len();
    picture(mb, center, right, Vec3::Y, w.length + 2.0 * out + 2.0, y1 - y0, WHITE);
    for v in &mut mb.vertices[first..] {
        v.uv.x *= repeat;
    }
}

/// A soft cushion or seat: a box with rounded vertical edges.
pub(crate) fn cushion(mb: &mut MeshBuilder, center: Vec3, size: Vec3, yaw: f32, color: Color) {
    let r = (size.x.min(size.z) * 0.25).min(size.y * 0.5);
    mb.cuboid(center, size - vec3(2.0 * r, 0.0, 0.0), yaw, color, 1.0);
    mb.cuboid(center, size - vec3(0.0, 0.0, 2.0 * r), yaw, color, 1.0);
    let rot = Quat::from_rotation_y(yaw);
    for (sx, sz) in [(1.0f32, 1.0f32), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
        let c =
            center + rot * vec3(sx * (size.x / 2.0 - r), -size.y / 2.0, sz * (size.z / 2.0 - r));
        mb.lathe(c, &[(0.0, r), (size.y, r)], 10, color);
    }
}

/// A sofa: a base, a seat, a back and two rolled arms. `yaw` 0 faces +Z.
pub(crate) fn sofa(
    mb: &mut MeshBuilder,
    at: Vec3,
    yaw: f32,
    width: f32,
    color: Color,
    tufted: bool,
) {
    let rot = Quat::from_rotation_y(yaw);
    let p = |x: f32, y: f32, z: f32| at + rot * vec3(x, y, z);
    let dark = Color::new(color.r * 0.7, color.g * 0.7, color.b * 0.7, 1.0);
    mb.cuboid(p(0.0, 0.2, 0.0), vec3(width, 0.24, 0.86), yaw, dark, 1.0);
    // Seat cushions.
    let seats = (width / 0.7).round().max(1.0) as i32;
    let sw = (width - 0.3) / seats as f32;
    for k in 0..seats {
        let x = -width / 2.0 + 0.15 + sw * (k as f32 + 0.5);
        cushion(mb, p(x, 0.4, 0.06), vec3(sw - 0.02, 0.14, 0.66), yaw, color);
    }
    // The back, a little tilted, and the arms (rolled).
    mb.cuboid(p(0.0, 0.62, -0.34), vec3(width, 0.6, 0.2), yaw, color, 1.0);
    for sx in [-1.0f32, 1.0] {
        let x = sx * (width / 2.0 - 0.08);
        mb.cuboid(p(x, 0.42, 0.0), vec3(0.16, 0.4, 0.84), yaw, color, 1.0);
        tube(mb, p(x, 0.66, -0.42), p(x, 0.66, 0.42), 0.1, 14, color);
    }
    if tufted {
        // Buttons in a diamond pattern on the back.
        let button = Color::new(color.r * 0.45, color.g * 0.45, color.b * 0.45, 1.0);
        let n = (width / 0.16) as i32;
        for row in 0..3 {
            for k in 0..n {
                if (k + row) % 2 == 1 {
                    continue;
                }
                let x = -width / 2.0 + 0.2 + k as f32 * (width - 0.4) / (n - 1).max(1) as f32;
                let c = p(x, 0.5 + row as f32 * 0.14, -0.235);
                disc_facing(mb, c, rot * Vec3::Z, 0.012, 6, button);
            }
        }
    }
    // Short turned feet.
    for (sx, sz) in [(1.0f32, 1.0f32), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
        mb.lathe(
            p(sx * (width / 2.0 - 0.08), 0.0, sz * 0.36),
            &[(0.0, 0.025), (0.08, 0.035)],
            8,
            dark,
        );
    }
}

/// Bottles standing in a row from `at` along `along`.
pub(crate) fn bottles(mb: &mut MeshBuilder, at: Vec3, along: Vec3, count: usize, segs: u32) {
    let colors = [
        rgb(40, 110, 40),
        rgb(150, 85, 25),
        rgb(210, 200, 170),
        rgb(120, 25, 25),
        rgb(30, 60, 130),
        rgb(90, 60, 30),
    ];
    for k in 0..count {
        let h = 0.20 + (k % 3) as f32 * 0.04;
        let r = 0.034 - (k % 2) as f32 * 0.004;
        let profile = [
            (0.0, r * 0.92),
            (0.005, r),
            (h * 0.62, r),
            (h * 0.78, r * 0.55),
            (h * 0.84, r * 0.36),
            (h + 0.06, r * 0.34),
            (h + 0.065, r * 0.42),
            (h + 0.075, r * 0.38),
        ];
        mb.lathe(
            at + along * (k as f32 * 0.11),
            &profile,
            segs,
            colors[(k * 7 + 3) % colors.len()],
        );
    }
}

/// A plant in a pot: a trunk and fronds (`palm`) or a bushy crown of leaves.
pub(crate) fn potted_plant(
    leaves: &mut MeshBuilder,
    pots: &mut MeshBuilder,
    at: Vec3,
    height: f32,
    palm: bool,
    pot: Color,
) {
    pots.lathe(
        at,
        &[(0.0, 0.0), (0.0, 0.16), (0.36, 0.22), (0.38, 0.24), (0.42, 0.24), (0.42, 0.2)],
        16,
        pot,
    );
    pots.disc(at + Vec3::Y * 0.4, 0.2, 16, rgb(50, 36, 24));
    let trunk = rgb(110, 84, 56);
    let green = rgb(54, 110, 44);
    let top = at + vec3(0.06, height, 0.03);
    // The trunk, in rings.
    let rings = 8;
    for k in 0..rings {
        let (t0, t1) = (k as f32 / rings as f32, (k + 1) as f32 / rings as f32);
        let p = |t: f32| at.lerp(top, t) + Vec3::Y * 0.4 * (1.0 - t);
        let r = if palm { 0.045 - t0 * 0.015 } else { 0.02 };
        tube(pots, p(t0), p(t1), r, 8, if k % 2 == 0 { trunk } else { rgb(90, 68, 44) });
    }
    let fronds = if palm { 9 } else { 14 };
    for f in 0..fronds {
        let a = f as f32 / fronds as f32 * std::f32::consts::TAU + (f % 3) as f32 * 0.3;
        let dir = vec3(a.cos(), 0.0, a.sin());
        let side = vec3(-dir.z, 0.0, dir.x);
        let (len, width) = if palm { (0.9, 0.16) } else { (0.28, 0.1) };
        let lift = if palm { 0.35 } else { 0.5 + (f % 4) as f32 * 0.25 };
        let base = if palm { top } else { top - Vec3::Y * (f % 4) as f32 * 0.18 };
        let steps = 6;
        let point = |t: f32| base + dir * len * t + Vec3::Y * (lift * t - t * t * len * 0.9);
        for s in 0..steps {
            let (t0, t1) = (s as f32 / steps as f32, (s + 1) as f32 / steps as f32);
            let w =
                |t: f32| width * (1.0 - t).max(0.05) * if palm { 1.0 } else { (t * 3.0).min(1.0) };
            let color =
                Color::new(green.r * (0.8 + t0 * 0.4), green.g * (0.8 + t0 * 0.4), green.b, 1.0);
            leaves.quad(
                [
                    point(t0) - side * w(t0),
                    point(t1) - side * w(t1),
                    point(t1) + side * w(t1),
                    point(t0) + side * w(t0),
                ],
                [Vec2::ZERO; 4],
                color,
            );
        }
    }
}

/// A light shaft from an opening in a wall, falling across the floor.
pub(crate) fn shaft(mb: &mut MeshBuilder, corners: [Vec3; 4], floor: [Vec3; 4], color: Color) {
    // Bright at the window, fading to nothing on the floor.
    let fade = Color::new(color.r, color.g, color.b, 0.0);
    for k in 0..4 {
        let (a, b) = (corners[k], corners[(k + 1) % 4]);
        let (c, d) = (floor[(k + 1) % 4], floor[k]);
        let i = [
            mb.push(a, vec2(0.5, 0.5), Vec3::Y, color),
            mb.push(b, vec2(0.5, 0.5), Vec3::Y, color),
            mb.push(c, vec2(0.5, 0.5), Vec3::Y, fade),
            mb.push(d, vec2(0.5, 0.5), Vec3::Y, fade),
        ];
        mb.indices.extend_from_slice(&[i[0], i[1], i[2], i[0], i[2], i[3]]);
    }
}

/// A pool of light on a wall (the glow texture).
pub(crate) fn pool(mb: &mut MeshBuilder, center: Vec3, normal: Vec3, w: f32, h: f32, color: Color) {
    let right = Vec3::Y.cross(normal).normalize_or_zero();
    picture(mb, center + normal * 0.006, right, Vec3::Y, w, h, color);
}

/// A string of light bulbs hanging between two points (sagging `sag`).
pub(crate) fn string_lights(
    wire: &mut MeshBuilder,
    bulbs: &mut MeshBuilder,
    a: Vec3,
    b: Vec3,
    sag: f32,
    count: usize,
) -> Vec<Vec3> {
    let p = |t: f32| a.lerp(b, t) - Vec3::Y * sag * 4.0 * t * (1.0 - t);
    let mut at_bulbs = Vec::with_capacity(count);
    let steps = count * 2;
    for k in 0..steps {
        let (t0, t1) = (k as f32 / steps as f32, (k + 1) as f32 / steps as f32);
        tube(wire, p(t0), p(t1), 0.003, 4, rgb(30, 26, 22));
    }
    let warm = [rgb(255, 214, 140), rgb(255, 190, 110), rgb(255, 230, 170)];
    for k in 0..count {
        let t = (k as f32 + 0.5) / count as f32;
        let at = p(t) - Vec3::Y * 0.05;
        tube(wire, p(t), at + Vec3::Y * 0.02, 0.004, 4, rgb(30, 26, 22));
        bulbs.lathe(
            at - Vec3::Y * 0.035,
            &[(0.0, 0.0), (0.01, 0.02), (0.035, 0.024), (0.05, 0.012), (0.055, 0.0)],
            8,
            warm[k % 3],
        );
        at_bulbs.push(at - Vec3::Y * 0.01);
    }
    at_bulbs
}

// --- The beach bar -------------------------------------------------------------

/// A beach bar at sunset: a deck, open windows on the sea, a thatched roof,
/// string lights, palms and a bamboo bar.
fn beach(kit: &mut Kit, quality: Quality, table: Vec2) {
    let detailed = quality >= Quality::High;
    let medium = quality >= Quality::Medium;
    let big = quality == Quality::Ultra;
    let (deck_px, wall_px, sun_px, palms_px, thatch_px, bamboo_px, rug_px) =
        std::thread::scope(|s| {
            let deck = s.spawn(move || tx::deck(if big { 512 } else { 256 }));
            let wall = s.spawn(|| tx::planks([0.86, 0.82, 0.72], [0.46, 0.40, 0.33], 141));
            let sun = s.spawn(|| tx::sunset(Some(0.42), true, 7));
            let palms = s.spawn(|| tx::sunset(None, true, 11));
            let thatch = s.spawn(tx::thatch);
            let bamboo = s.spawn(crate::tables::bamboo);
            let rug = s.spawn(tx::rattan);
            (
                deck.join().expect("deck"),
                wall.join().expect("wall"),
                sun.join().expect("sun"),
                palms.join().expect("palms"),
                thatch.join().expect("thatch"),
                bamboo.join().expect("bamboo"),
                rug.join().expect("mat"),
            )
        });
    let deck_tex = Some(kit.texture(deck_px, true));
    let wall_tex = Some(kit.texture(wall_px, true));
    let sun_tex = Some(kit.texture(sun_px, false));
    let palms_tex = Some(kit.texture(palms_px, true));
    let thatch_tex = Some(kit.texture(thatch_px, true));
    let bamboo_tex = Some(kit.texture(bamboo_px, true));
    let mat_tex = Some(kit.texture(rug_px, true));
    let glow_tex = Some(kit.texture(textures::glow(), false));
    let teak_tex =
        Some(kit.texture(textures::fine_wood([0.55, 0.36, 0.19], [0.30, 0.17, 0.08], 29), true));

    // The sun sets beyond the left wall: warm light low through the windows.
    let mut lights = table_lights(vec3(1.0, 0.84, 0.62), table);
    for z in [-2.6f32, -0.9, 0.9, 2.6] {
        lights.push(light(vec3(-HX - 1.2, 1.5, z), vec3(1.0, 0.55, 0.28), 2.0, false));
    }
    for x in [-3.5f32, 0.0, 3.5] {
        lights.push(light(vec3(x, 1.6, -HZ - 1.2), vec3(0.95, 0.5, 0.42), 0.7, false));
    }
    for x in [-3.0f32, 0.0, 3.0] {
        for z in [-1.5f32, 1.5] {
            lights.push(light(vec3(x, 2.5, z), vec3(1.0, 0.78, 0.48), 0.22, false));
        }
    }
    lights.push(light(vec3(0.0, 2.2, -3.3), vec3(1.0, 0.75, 0.45), 0.6, true));
    let mut lit = Lighting::new(quality, lights, vec3(0.34, 0.24, 0.24), table, 2.4);
    // The bar casts a shadow.
    lit.occlude_box(vec3(0.0, 0.53, -3.35), vec3(4.1, 1.06, 0.74));
    let cell = cell(quality);

    let mut floor = MeshBuilder::default();
    let mut ceiling = MeshBuilder::default();
    floor_and_ceiling(&mut floor, &mut ceiling, cell, 0.45, 1.0 / 0.7, WHITE);
    lit.floor(kit, floor, deck_tex, 0.0, SATIN);
    lit.add(kit, ceiling, thatch_tex);
    if medium {
        // A woven mat under the table.
        let mut mat = MeshBuilder::default();
        let first = mat.len();
        rug(&mut mat, 2.3, 1.7);
        for v in &mut mat.vertices[first..] {
            v.uv *= vec2(9.0, 7.0);
            v.color = [235, 220, 200, 255];
        }
        lit.add(kit, mat, mat_tex);
    }

    // Walls: a plank wall to the sill, posts and open windows, a header.
    const SILL: f32 = 0.95;
    const HEAD: f32 = 2.45;
    let mut planks = MeshBuilder::default();
    let mut teak = MeshBuilder::default();
    let mut sky_sun = MeshBuilder::default();
    let mut sky = MeshBuilder::default();
    let mut shafts = MeshBuilder::default();
    let post = rgb(150, 110, 72);
    for (i, w) in walls().iter().enumerate() {
        band(&mut planks, w, 0.0, w.length, 0.0, SILL, cell, WHITE, 1.0 / 0.8);
        band(&mut planks, w, 0.0, w.length, HEAD, HEIGHT, cell, WHITE, 1.0 / 0.8);
        let bays = (w.length / 1.6).round().max(1.0) as i32;
        let bay = w.length / bays as f32;
        for k in 0..=bays {
            let at = w.start + w.along * (k as f32 * bay) + w.inward * 0.06;
            let size = w.along.abs() * 0.14 + Vec3::Y * (HEAD - SILL) + w.inward.abs() * 0.14;
            teak.cuboid(at + Vec3::Y * (SILL + HEAD) / 2.0, size, 0.0, post, 1.2);
        }
        // The sill and the lintel.
        let mid = w.start + w.along * (w.length / 2.0) + w.inward * 0.08;
        teak.cuboid(
            mid + Vec3::Y * SILL,
            w.along.abs() * w.length + Vec3::Y * 0.05 + w.inward.abs() * 0.22,
            0.0,
            post,
            1.2,
        );
        teak.cuboid(
            mid + Vec3::Y * HEAD,
            w.along.abs() * w.length + Vec3::Y * 0.1 + w.inward.abs() * 0.18,
            0.0,
            post,
            1.2,
        );
        // The sea and the sky beyond: the sun sets on the left.
        if i == 2 {
            backdrop(&mut sky_sun, w, 0.9, 0.0, 3.6, false, 1.0);
        } else {
            backdrop(&mut sky, w, 0.9, 0.0, 3.6, i == 1, (w.length / 4.5).round().max(1.0));
        }
        if i == 2 && medium {
            // Sunlight falling through the left windows onto the floor.
            for k in 0..bays {
                let a = k as f32 * bay + 0.15;
                let b = (k + 1) as f32 * bay - 0.15;
                let p =
                    |along: f32, y: f32| w.start + w.along * along + Vec3::Y * y + w.inward * 0.07;
                let corners =
                    [p(a, SILL + 0.05), p(b, SILL + 0.05), p(b, HEAD - 0.05), p(a, HEAD - 0.05)];
                let reach = |c: Vec3| vec3(c.x + (c.y - 0.02) * 1.6, 0.02, c.z + 0.25);
                let floor = corners.map(reach);
                shaft(&mut shafts, corners, floor, Color::new(1.0, 0.72, 0.45, 0.085));
            }
        }
    }
    lit.add(kit, planks, wall_tex);
    add_glow(kit, sky_sun, sun_tex);
    add_glow(kit, sky, palms_tex);

    // Roof beams: round poles across the room.
    let mut poles = MeshBuilder::default();
    let cane = rgb(196, 160, 104);
    for k in -3..=3 {
        let x = k as f32 * 1.6;
        tube(&mut poles, vec3(x, HEIGHT - 0.08, -HZ), vec3(x, HEIGHT - 0.08, HZ), 0.06, 10, cane);
    }
    tube(&mut poles, vec3(-HX, HEIGHT - 0.2, 0.0), vec3(HX, HEIGHT - 0.2, 0.0), 0.08, 12, cane);

    // The bar: a bamboo front, a teak top, bottles, stools.
    let bar_z = -3.35;
    let counter = 4.0;
    let mut front = MeshBuilder::default();
    front.grid(
        vec3(-counter / 2.0, 0.0, bar_z + 0.3),
        vec3(counter, 0.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        12,
        3,
        WHITE,
        1.0 / 0.28,
    );
    for sx in [-1.0f32, 1.0] {
        front.grid(
            vec3(sx * counter / 2.0, 0.0, bar_z + if sx > 0.0 { 0.3 } else { -0.3 }),
            vec3(0.0, 0.0, sx * 0.6),
            vec3(0.0, 1.0, 0.0),
            2,
            3,
            WHITE,
            1.0 / 0.28,
        );
    }
    lit.add(kit, front, bamboo_tex);
    teak.cuboid(
        vec3(0.0, 1.03, bar_z),
        vec3(counter + 0.12, 0.06, 0.74),
        0.0,
        rgb(200, 150, 100),
        1.0,
    );
    let mut glass = MeshBuilder::default();
    bottles(&mut glass, vec3(-1.6, 1.06, bar_z - 0.2), Vec3::X, 7, if detailed { 14 } else { 8 });
    bottles(&mut glass, vec3(0.9, 1.06, bar_z - 0.22), Vec3::X, 6, if detailed { 14 } else { 8 });
    // Coconut cups and a pineapple on the counter.
    let mut fruit = MeshBuilder::default();
    for (x, z) in [(-0.6f32, 0.1f32), (0.2, 0.15)] {
        fruit.lathe(
            vec3(x, 1.06, bar_z + z),
            &[(0.0, 0.0), (0.0, 0.04), (0.04, 0.06), (0.08, 0.055), (0.085, 0.0)],
            12,
            rgb(110, 70, 36),
        );
    }
    fruit.lathe(
        vec3(0.5, 1.06, bar_z - 0.1),
        &[(0.0, 0.0), (0.0, 0.05), (0.08, 0.065), (0.16, 0.05), (0.17, 0.0)],
        12,
        rgb(200, 150, 50),
    );
    for f in 0..6 {
        let a = f as f32 / 6.0 * std::f32::consts::TAU;
        let base = vec3(0.5, 1.22, bar_z - 0.1);
        tube(
            &mut fruit,
            base,
            base + vec3(a.cos() * 0.06, 0.12, a.sin() * 0.06),
            0.008,
            4,
            rgb(60, 120, 50),
        );
    }
    let mut stools = MeshBuilder::default();
    for k in 0..5 {
        let c = vec3(-1.6 + k as f32 * 0.8, 0.0, bar_z + 0.75);
        for a in 0..4 {
            let t = a as f32 / 4.0 * std::f32::consts::TAU + 0.4;
            tube(
                &mut stools,
                c + vec3(t.cos() * 0.16, 0.0, t.sin() * 0.16),
                c + vec3(t.cos() * 0.12, 0.7, t.sin() * 0.12),
                0.016,
                6,
                cane,
            );
        }
        hoop(&mut stools, c + Vec3::Y * 0.3, 0.145, 0.012, 14, cane);
        stools.lathe(
            c + Vec3::Y * 0.7,
            &[(0.0, 0.0), (0.0, 0.19), (0.04, 0.2), (0.06, 0.18), (0.065, 0.0)],
            18,
            rgb(120, 170, 165),
        );
    }

    // Palms in pots in the corners, a surfboard, a hammock-coloured flag.
    let mut leaves = MeshBuilder::default();
    let mut pots = MeshBuilder::default();
    if medium {
        for (x, z, h) in
            [(-4.9f32, -3.4f32, 1.9f32), (4.9, -3.4, 1.6), (-4.9, 3.4, 1.7), (4.9, 3.4, 2.0)]
        {
            potted_plant(&mut leaves, &mut pots, vec3(x, 0.0, z), h, true, rgb(176, 96, 60));
        }
    }
    let mut board = MeshBuilder::default();
    if detailed {
        // A surfboard leaning on the right wall.
        let base = vec3(HX - 0.25, 0.02, 1.6);
        let up = vec3(0.22, 1.0, 0.0).normalize();
        let across = Vec3::Z;
        let normal = up.cross(across).normalize();
        let n = 24;
        let outline = |t: f32| {
            let y = t * 2.1;
            let w = (std::f32::consts::PI * t).sin().powf(0.6)
                * 0.27
                * if t > 0.7 { 1.0 - (t - 0.7) * 1.4 } else { 1.0 };
            (y, w)
        };
        for k in 0..n {
            let (y0, w0) = outline(k as f32 / n as f32);
            let (y1, w1) = outline((k + 1) as f32 / n as f32);
            for side in [0.0f32, 1.0] {
                let off = normal * (side * 0.05 - 0.025);
                let p = |y: f32, x: f32| base + up * y + across * x + off;
                for (xa, xb) in [(-1.0f32, -0.13f32), (-0.13, 0.13), (0.13, 1.0)] {
                    let c = if side > 0.5 && xa.abs() < 0.2 {
                        rgb(244, 240, 228)
                    } else if side > 0.5 {
                        rgb(36, 168, 178)
                    } else {
                        rgb(232, 228, 214)
                    };
                    board.quad(
                        [p(y0, w0 * xa), p(y0, w0 * xb), p(y1, w1 * xb), p(y1, w1 * xa)],
                        [Vec2::ZERO; 4],
                        c,
                    );
                }
            }
        }
    }

    lit.shiny(kit, teak, teak_tex, VARNISH);
    lit.add(kit, poles, None);
    lit.shiny(kit, glass, None, GLASS);
    lit.add(kit, fruit, None);
    lit.add(kit, stools, None);
    lit.add(kit, leaves, None);
    lit.add(kit, pots, None);
    lit.add(kit, board, None);

    // String lights across the room, glowing by themselves.
    let mut wire = MeshBuilder::default();
    let mut bulbs = MeshBuilder::default();
    let strands = if medium { 4 } else { 2 };
    let mut lit_bulbs = Vec::new();
    for k in 0..strands {
        let z = -2.7 + k as f32 * 5.4 / (strands - 1) as f32;
        lit_bulbs.extend(string_lights(
            &mut wire,
            &mut bulbs,
            vec3(-HX + 0.1, 2.85, z),
            vec3(HX - 0.1, 2.85, z * 0.6),
            0.35,
            if detailed { 22 } else { 12 },
        ));
    }
    if medium {
        lit_bulbs.extend(string_lights(
            &mut wire,
            &mut bulbs,
            vec3(-HX + 0.1, 2.9, -HZ + 0.1),
            vec3(HX - 0.1, 2.9, HZ - 0.1),
            0.45,
            if detailed { 26 } else { 14 },
        ));
    }
    lit.add(kit, wire, None);
    add_glow(kit, bulbs, None);
    for at in lit_bulbs {
        kit.glow(at, 0.11, Color::new(1.0, 0.75, 0.42, 0.5), false);
    }
    if medium {
        kit.add(shafts, None, Surface::UNLIT, Layer::Blended);
        // Warm pools of light around the bulbs on the walls.
        let mut pools = MeshBuilder::default();
        for z in [-2.7f32, 2.7] {
            pool(
                &mut pools,
                vec3(-HX + 0.08, 2.4, z),
                Vec3::X,
                1.6,
                1.1,
                Color::new(1.0, 0.75, 0.45, 0.12),
            );
        }
        kit.add(pools, glow_tex, Surface::UNLIT, Layer::Blended);
    }
}

/// Three woven rattan domes on a driftwood bar.
fn rattan_lamp(kit: &mut Kit, quality: Quality, reach: f32) {
    let grow = reach - 0.45;
    let tex = kit.texture(tx::rattan(), true);
    let segs = if quality != Quality::Low { 32 } else { 18 };
    let mut shades = MeshBuilder::default();
    let mut wood = MeshBuilder::default();
    let mut glow = MeshBuilder::default();
    for x in lamp_xs(reach) {
        let at = vec3(x, LAMP_Y - 0.02, 0.0);
        let profile =
            [(0.0, 0.19), (0.04, 0.185), (0.09, 0.16), (0.14, 0.115), (0.18, 0.06), (0.2, 0.02)];
        let first = shades.len();
        shades.lathe(at, &profile, segs, WHITE);
        for v in &mut shades.vertices[first..] {
            v.uv = vec2(v.uv.x * 3.0, (v.position.y - at.y) * 6.0);
        }
        glow.frustum_at(at, 0.002, 0.18, 0.12, 0.12, segs, rgb(255, 205, 140));
        glow.disc(at + Vec3::Y * 0.12, 0.12, segs, rgb(255, 232, 190));
        glow.disc(at + Vec3::Y * 0.06, 0.04, 16, rgb(255, 250, 235));
        // The rope.
        tube(&mut wood, at + Vec3::Y * 0.2, at + Vec3::Y * 0.3, 0.007, 6, rgb(170, 140, 100));
    }
    // A bleached driftwood bar and ropes to the ceiling.
    tube(
        &mut wood,
        vec3(-0.7 - grow, LAMP_Y + 0.3, 0.0),
        vec3(0.72 + grow, LAMP_Y + 0.31, 0.01),
        0.035,
        10,
        rgb(180, 160, 130),
    );
    for x in [-0.6 - grow, 0.6 + grow] {
        tube(
            &mut wood,
            vec3(x, LAMP_Y + 0.3, 0.0),
            vec3(x, HEIGHT, 0.0),
            0.008,
            6,
            rgb(170, 140, 100),
        );
    }
    kit.add(shades, Some(tex), Surface::lit(0.3, 20.0, 0.25, 0.0), Layer::Lamp);
    kit.add(wood, None, Surface::lit(0.2, 20.0, 0.05, 0.0), Layer::Lamp);
    kit.add(glow, None, Surface::UNLIT, Layer::Lamp);
}

// --- The English club ----------------------------------------------------------

/// An English club: herringbone parquet, panelled walls, bookshelves, a
/// fireplace, leather sofas, paintings in gilt frames, a coffered ceiling.
fn club(kit: &mut Kit, quality: Quality, table: Vec2) {
    let detailed = quality >= Quality::High;
    let medium = quality >= Quality::Medium;
    let (parquet_px, books_px, rug_px, paint_px, silk_px) = std::thread::scope(|s| {
        let parquet = s.spawn(tx::parquet);
        let books = s.spawn(tx::books);
        let rug = s.spawn(tx::persian_rug);
        let paintings = s.spawn(|| [0, 1, 2, 3].map(tx::painting));
        let silk = s.spawn(|| tx::planks([0.20, 0.07, 0.07], [0.12, 0.035, 0.04], 151));
        (
            parquet.join().expect("parquet"),
            books.join().expect("books"),
            rug.join().expect("rug"),
            paintings.join().expect("paintings"),
            silk.join().expect("silk"),
        )
    });
    let parquet_tex = Some(kit.texture(parquet_px, true));
    let books_tex = Some(kit.texture(books_px, true));
    let rug_tex = Some(kit.texture(rug_px, false));
    let paint_tex: Vec<Option<usize>> =
        paint_px.into_iter().map(|p| Some(kit.texture(p, false))).collect();
    let wood_tex =
        Some(kit.texture(textures::fine_wood([0.26, 0.10, 0.05], [0.07, 0.025, 0.014], 47), true));
    let silk_tex = Some(kit.texture(silk_px, true));
    let glow_tex = Some(kit.texture(textures::glow(), false));

    let fire = vec3(-HX + 0.35, 0.35, 0.0);
    let mut lights = table_lights(vec3(1.0, 0.86, 0.66), table);
    lights.push(light(fire, vec3(1.0, 0.45, 0.15), 1.5, false));
    kit.glow(fire + vec3(0.1, 0.05, 0.0), 0.55, Color::new(1.0, 0.5, 0.18, 0.4), false);
    for z in [-2.4f32, 2.4] {
        lights.push(light(vec3(-HX + 0.2, 1.9, z), vec3(1.0, 0.72, 0.42), 0.5, false));
    }
    for z in [-1.6f32, 0.0, 1.6] {
        lights.push(light(vec3(HX - 0.3, 2.3, z), vec3(1.0, 0.8, 0.55), 0.45, true));
    }
    for x in [-3.6f32, 3.6] {
        lights.push(light(vec3(x, 1.0, HZ - 0.9), vec3(0.9, 0.85, 0.55), 0.35, false));
    }
    lights.push(light(vec3(0.0, 2.6, -HZ + 0.8), vec3(1.0, 0.78, 0.5), 0.5, true));
    let mut lit = Lighting::new(quality, lights, vec3(0.24, 0.18, 0.13), table, 3.1);
    // The fireplace's surround, the bookshelves and the sofa cast shadows.
    lit.occlude_box(vec3(-HX + 0.2, 0.62, 0.0), vec3(0.42, 1.24, 2.0));
    lit.occlude_box(vec3(0.0, 1.45, -HZ + 0.19), vec3(7.0, 2.9, 0.34));
    lit.occlude_box(vec3(0.0, 0.4, HZ - 0.6), vec3(2.3, 0.8, 0.9));
    let cell = cell(quality);

    let mut floor = MeshBuilder::default();
    let mut ceiling = MeshBuilder::default();
    floor_and_ceiling(&mut floor, &mut ceiling, cell, 0.5, 1.0, rgb(150, 132, 110));
    lit.floor(kit, floor, parquet_tex, 0.2, VARNISH);
    lit.add(kit, ceiling, None);
    if medium {
        let mut r = MeshBuilder::default();
        rug(&mut r, 2.6, 1.9);
        lit.add(kit, r, rug_tex);
    }

    // Walls: panelling to the chair rail, silk above, crown moulding.
    const DADO: f32 = 1.15;
    const CROWN: f32 = 2.95;
    let mut silk = MeshBuilder::default();
    let mut panels = MeshBuilder::default();
    let wood = WHITE;
    let light_wood = rgb(255, 240, 228);
    for w in walls() {
        band(&mut silk, &w, 0.0, w.length, DADO, CROWN, cell, WHITE, 1.0 / 0.5);
        band(&mut panels, &w, 0.0, w.length, 0.0, DADO, cell, wood, 0.9);
        band(&mut panels, &w, 0.0, w.length, CROWN, HEIGHT, cell, wood, 0.9);
        let a = w.start;
        let b = w.start + w.along * w.length;
        if medium {
            let count = (w.length / 0.8).floor().max(1.0) as u32;
            let pw = w.length / count as f32;
            for k in 0..count {
                let c0 = w.start + w.along * (k as f32 * pw) + w.inward * 0.012;
                let frame = 0.07;
                let depth = 0.02;
                let box_at =
                    |m: &mut MeshBuilder, along: f32, y: f32, len: f32, h: f32, c: Color| {
                        let center = c0 + w.along * along + Vec3::Y * y;
                        m.cuboid(
                            center,
                            w.along.abs() * len + Vec3::Y * h + w.inward.abs() * depth,
                            0.0,
                            c,
                            0.9,
                        );
                    };
                box_at(&mut panels, frame / 2.0, 0.6, frame, 0.92, wood);
                box_at(&mut panels, pw / 2.0, 0.18, pw, frame, wood);
                box_at(&mut panels, pw / 2.0, 1.03, pw, frame, wood);
                if detailed {
                    box_at(&mut panels, pw / 2.0, 0.6, pw - 2.6 * frame, 0.66, light_wood);
                }
            }
            let rail = [
                vec2(0.0, 0.0),
                vec2(0.03, 0.0),
                vec2(0.04, 0.015),
                vec2(0.04, 0.04),
                vec2(0.025, 0.06),
                vec2(0.0, 0.065),
            ];
            panels.extrude(
                a + Vec3::Y * (DADO - 0.03),
                b + Vec3::Y * (DADO - 0.03),
                w.inward,
                &rail,
                wood,
            );
            let crown: Vec<Vec2> = (0..=6)
                .map(|k| {
                    let t = k as f32 / 6.0 * std::f32::consts::FRAC_PI_2;
                    vec2(0.16 * (1.0 - t.sin()), 0.16 * (1.0 - t.cos()))
                })
                .chain([vec2(0.0, 0.22), vec2(0.0, 0.0)])
                .collect();
            panels.extrude(a + Vec3::Y * CROWN, b + Vec3::Y * CROWN, w.inward, &crown, wood);
            let base = [
                vec2(0.0, 0.0),
                vec2(0.03, 0.0),
                vec2(0.03, 0.16),
                vec2(0.015, 0.19),
                vec2(0.0, 0.2),
            ];
            panels.extrude(a, b, w.inward, &base, wood);
        }
    }
    lit.add(kit, silk, silk_tex);

    // The coffered ceiling: a grid of beams.
    let mut beams = MeshBuilder::default();
    if medium {
        for k in -4..=4 {
            let x = k as f32 * 1.2;
            beams.cuboid(vec3(x, HEIGHT - 0.07, 0.0), vec3(0.14, 0.14, 2.0 * HZ), 0.0, wood, 0.8);
        }
        for k in -3..=3 {
            let z = k as f32 * 1.2;
            beams.cuboid(vec3(0.0, HEIGHT - 0.07, z), vec3(2.0 * HX, 0.14, 0.14), 0.0, wood, 0.8);
        }
    }

    // Bookshelves along the back wall: uprights, shelves, books.
    let mut shelves = MeshBuilder::default();
    let mut book_rows = MeshBuilder::default();
    let (x0, x1) = (-4.6f32, 4.6f32);
    let bays = 8;
    let bw = (x1 - x0) / bays as f32;
    let back = -HZ + 0.02;
    let depth = 0.34;
    for k in 0..bays {
        let xa = x0 + k as f32 * bw;
        // Two shelves of books per tile (0.7 m), from 0.9 m up to 2.8 m;
        // cabinets with doors below.
        book_rows.grid(
            vec3(xa + 0.04, 0.95, back + depth - 0.05),
            vec3(bw - 0.08, 0.0, 0.0),
            vec3(0.0, 1.82, 0.0),
            4,
            6,
            WHITE,
            1.0,
        );
        let first = book_rows.len() - 35;
        for v in &mut book_rows.vertices[first..] {
            v.uv = vec2((v.position.x - x0) / 0.9, (2.77 - v.position.y) / 0.7);
        }
        shelves.cuboid(
            vec3(xa + bw / 2.0, 0.45, back + depth / 2.0 + 0.03),
            vec3(bw - 0.02, 0.9, depth + 0.06),
            0.0,
            wood,
            0.8,
        );
        if detailed {
            for d in [-1.0f32, 1.0] {
                shelves.cuboid(
                    vec3(xa + bw / 2.0 + d * bw / 4.0, 0.47, back + depth + 0.065),
                    vec3(bw / 2.0 - 0.07, 0.7, 0.012),
                    0.0,
                    light_wood,
                    0.8,
                );
            }
        }
    }
    for k in 0..=bays {
        let x = x0 + k as f32 * bw;
        shelves.cuboid(vec3(x, 1.4, back + depth / 2.0), vec3(0.05, 2.8, depth), 0.0, wood, 0.8);
    }
    for y in [0.93f32, 1.28, 1.63, 1.98, 2.33, 2.68] {
        shelves.cuboid(
            vec3(0.0, y, back + depth / 2.0),
            vec3(x1 - x0, 0.03, depth),
            0.0,
            wood,
            0.8,
        );
    }
    shelves.cuboid(
        vec3(0.0, 2.84, back + depth / 2.0 + 0.02),
        vec3(x1 - x0 + 0.1, 0.1, depth + 0.06),
        0.0,
        wood,
        0.8,
    );
    // A library ladder on a brass rail.
    let mut brass = MeshBuilder::default();
    let gold = rgb(200, 160, 80);
    if detailed {
        tube(
            &mut brass,
            vec3(x0, 2.55, back + depth + 0.08),
            vec3(x1, 2.55, back + depth + 0.08),
            0.012,
            8,
            gold,
        );
        let (lx, top, foot) = (1.7f32, back + depth + 0.08, back + depth + 0.75);
        for d in [-0.22f32, 0.22] {
            tube(&mut shelves, vec3(lx + d, 2.55, top), vec3(lx + d, 0.0, foot), 0.022, 8, wood);
        }
        for k in 1..9 {
            let t = k as f32 / 9.0;
            let z = top + (foot - top) * t;
            let y = 2.55 * (1.0 - t);
            tube(&mut shelves, vec3(lx - 0.22, y, z), vec3(lx + 0.22, y, z), 0.014, 6, wood);
        }
    }

    // The fireplace on the left wall: a marble surround, the fire, a
    // painting above.
    let mut marble = MeshBuilder::default();
    let mut dark = MeshBuilder::default();
    let mut flames = MeshBuilder::default();
    let fx = -HX + 0.02;
    let stone = rgb(188, 178, 162);
    // The surround: two jambs, a lintel and the mantel shelf, round an
    // opening 0.9 m wide; the firebox behind it is black.
    for sz in [-1.0f32, 1.0] {
        marble.cuboid(vec3(fx + 0.15, 0.6, sz * 0.66), vec3(0.3, 1.2, 0.42), 0.0, stone, 1.0);
        dark.cuboid(
            vec3(fx + 0.12, 0.44, sz * 0.46),
            vec3(0.24, 0.88, 0.02),
            0.0,
            rgb(16, 12, 10),
            1.0,
        );
    }
    marble.cuboid(vec3(fx + 0.15, 1.04, 0.0), vec3(0.3, 0.32, 0.9), 0.0, stone, 1.0);
    marble.cuboid(vec3(fx + 0.21, 1.235, 0.0), vec3(0.42, 0.07, 1.95), 0.0, stone, 1.0);
    marble.cuboid(vec3(fx + 0.4, 0.02, 0.0), vec3(0.8, 0.04, 2.0), 0.0, rgb(200, 192, 180), 1.0);
    dark.cuboid(vec3(fx + 0.01, 0.44, 0.0), vec3(0.02, 0.88, 0.92), 0.0, rgb(20, 14, 10), 1.0);
    // A grate with logs, and the flames.
    for dz in [-0.25f32, 0.25] {
        tube(
            &mut dark,
            vec3(fx + 0.24, 0.0, dz),
            vec3(fx + 0.24, 0.16, dz),
            0.012,
            6,
            rgb(30, 30, 30),
        );
    }
    tube(
        &mut dark,
        vec3(fx + 0.24, 0.12, -0.3),
        vec3(fx + 0.24, 0.12, 0.3),
        0.01,
        6,
        rgb(30, 30, 30),
    );
    for (dx, dy, dz) in [(0.14f32, 0.08f32, -0.1f32), (0.18, 0.08, 0.12), (0.15, 0.14, 0.0)] {
        tube(
            &mut dark,
            vec3(fx + dx, dy, dz - 0.24),
            vec3(fx + dx + 0.03, dy, dz + 0.24),
            0.04,
            8,
            rgb(52, 30, 16),
        );
    }
    let flame_colors = [rgb(255, 150, 40), rgb(255, 200, 90), rgb(255, 112, 32)];
    for k in 0..9 {
        let z = -0.32 + k as f32 * 0.08;
        let h = 0.16 + ((k * 37 % 5) as f32) * 0.05;
        let base = vec3(fx + 0.14 + (k % 2) as f32 * 0.05, 0.1, z);
        flames.quad(
            [
                base + vec3(0.0, 0.0, -0.055),
                base + vec3(0.0, 0.0, 0.055),
                base + vec3(0.0, h, 0.012),
                base + vec3(0.0, h, -0.012),
            ],
            [Vec2::ZERO; 4],
            flame_colors[k % 3],
        );
    }
    // Embers glowing under the logs.
    flames.quad(
        [
            vec3(fx + 0.06, 0.045, -0.3),
            vec3(fx + 0.06, 0.045, 0.3),
            vec3(fx + 0.26, 0.045, 0.3),
            vec3(fx + 0.26, 0.045, -0.3),
        ],
        [Vec2::ZERO; 4],
        rgb(220, 70, 20),
    );
    // A painting above the mantel, and a mirror-like gilt frame.
    let mut pictures: Vec<MeshBuilder> = (0..4).map(|_| MeshBuilder::default()).collect();
    let mut frames = MeshBuilder::default();
    let frame_gold = rgb(190, 150, 70);
    let hang = |pics: &mut Vec<MeshBuilder>,
                frames: &mut MeshBuilder,
                k: usize,
                c: Vec3,
                right: Vec3,
                normal: Vec3,
                w: f32,
                h: f32| {
        frames.cuboid(
            c - normal * 0.015,
            right.abs() * (w + 0.14) + Vec3::Y * (h + 0.14) + normal.abs() * 0.05,
            0.0,
            frame_gold,
            1.0,
        );
        picture(&mut pics[k], c + normal * 0.012, right, Vec3::Y, w, h, WHITE);
    };
    hang(&mut pictures, &mut frames, 0, vec3(fx + 0.06, 2.05, 0.0), -Vec3::Z, Vec3::X, 1.1, 0.8);
    // Portraits on the right wall with picture lights.
    for (k, z) in [(1usize, -1.6f32), (3, 0.0), (1, 1.6)] {
        hang(&mut pictures, &mut frames, k, vec3(HX - 0.05, 1.85, z), Vec3::Z, -Vec3::X, 0.6, 0.8);
        brass.cuboid(vec3(HX - 0.12, 2.35, z), vec3(0.08, 0.04, 0.5), 0.0, gold, 1.0);
    }
    hang(&mut pictures, &mut frames, 2, vec3(0.0, 1.9, HZ - 0.05), -Vec3::X, -Vec3::Z, 1.4, 0.9);
    // Brass sconces by the fireplace.
    let mut glow = MeshBuilder::default();
    for z in [-1.4f32, 1.4] {
        let at = vec3(fx + 0.05, 1.85, z);
        brass.cuboid(at, vec3(0.03, 0.16, 0.08), 0.0, gold, 1.0);
        tube(&mut brass, at, at + vec3(0.12, 0.05, 0.0), 0.007, 6, gold);
        glow.lathe(
            at + vec3(0.13, 0.04, 0.0),
            &[(0.0, 0.03), (0.08, 0.05), (0.1, 0.05)],
            14,
            rgb(255, 220, 170),
        );
        kit.glow(at + vec3(0.13, 0.09, 0.0), 0.22, Color::new(1.0, 0.72, 0.42, 0.5), false);
    }

    // Chesterfields and leather armchairs, side tables with banker's lamps.
    let mut leather = MeshBuilder::default();
    let mut tables = MeshBuilder::default();
    let oxblood = rgb(120, 32, 26);
    let tan = rgb(98, 54, 30);
    sofa(&mut leather, vec3(0.0, 0.0, HZ - 0.6), std::f32::consts::PI, 2.3, oxblood, detailed);
    sofa(
        &mut leather,
        vec3(-HX + 1.6, 0.0, -2.3),
        std::f32::consts::FRAC_PI_2 + 0.5,
        0.95,
        tan,
        detailed,
    );
    sofa(
        &mut leather,
        vec3(-HX + 1.6, 0.0, 2.3),
        std::f32::consts::FRAC_PI_2 - 0.5,
        0.95,
        tan,
        detailed,
    );
    for x in [-1.75f32, 1.75] {
        let c = vec3(x, 0.0, HZ - 0.55);
        tables.lathe(
            c,
            &[(0.0, 0.15), (0.03, 0.15), (0.05, 0.03), (0.55, 0.03), (0.56, 0.24), (0.6, 0.24)],
            18,
            rgb(80, 34, 20),
        );
        // A banker's lamp: a brass stem and a green glass shade.
        brass.lathe(
            c + Vec3::Y * 0.6,
            &[(0.0, 0.07), (0.02, 0.07), (0.03, 0.012), (0.3, 0.012)],
            12,
            gold,
        );
        let shade = c + vec3(0.0, 0.88, 0.0);
        glow.cuboid(shade - Vec3::Y * 0.035, vec3(0.24, 0.01, 0.09), 0.0, rgb(255, 236, 190), 1.0);
        kit.glow(shade - Vec3::Y * 0.05, 0.2, Color::new(1.0, 0.85, 0.6, 0.35), false);
        frames.cuboid(shade, vec3(0.26, 0.06, 0.12), 0.0, rgb(30, 110, 60), 1.0);
    }

    lit.shiny(kit, panels, wood_tex, SATIN);
    lit.add(kit, beams, wood_tex);
    lit.add(kit, shelves, wood_tex);
    lit.add(kit, book_rows, books_tex);
    lit.shiny(kit, tables, wood_tex, VARNISH);
    lit.shiny(kit, marble, None, (0.8, 90.0, 0.3));
    lit.add(kit, dark, None);
    lit.shiny(kit, brass, None, BRASS);
    lit.shiny(kit, frames, None, BRASS);
    lit.shiny(kit, leather, None, LEATHER);
    for (k, m) in pictures.into_iter().enumerate() {
        lit.add(kit, m, paint_tex[k]);
    }
    add_glow(kit, flames, None);
    add_glow(kit, glow, None);
    // The fire's glow on the floor and the hearth, the sconces on the wall.
    let mut pools = MeshBuilder::default();
    let mut floor_glow = MeshBuilder::default();
    floor_glow.quad(
        [
            vec3(fx + 0.2, 0.045, -1.1),
            vec3(fx + 0.2, 0.045, 1.1),
            vec3(fx + 1.8, 0.045, 1.1),
            vec3(fx + 1.8, 0.045, -1.1),
        ],
        [vec2(0.5, 0.0), vec2(0.5, 1.0), vec2(1.0, 1.0), vec2(1.0, 0.0)],
        Color::new(1.0, 0.5, 0.2, 0.22),
    );
    for z in [-1.4f32, 1.4] {
        pool(
            &mut pools,
            vec3(fx + 0.02, 2.0, z),
            Vec3::X,
            0.9,
            1.0,
            Color::new(1.0, 0.75, 0.45, 0.16),
        );
    }
    for z in [-1.6f32, 0.0, 1.6] {
        pool(
            &mut pools,
            vec3(HX - 0.02, 2.0, z),
            -Vec3::X,
            0.9,
            0.9,
            Color::new(1.0, 0.8, 0.55, 0.12),
        );
    }
    kit.add(pools, glow_tex, Surface::UNLIT, Layer::Blended);
    kit.add(floor_glow, glow_tex, Surface::UNLIT, Layer::Blended);
}

/// A long mahogany canopy over the table, lit from below, with a brass rim
/// and a fringe.
fn club_lamp(kit: &mut Kit, quality: Quality, reach: f32) {
    let wood = kit.texture(textures::fine_wood([0.26, 0.10, 0.05], [0.07, 0.025, 0.014], 47), true);
    let mut canopy = MeshBuilder::default();
    let mut brass = MeshBuilder::default();
    let mut glow = MeshBuilder::default();
    let (l, w, h) = (1.75 + 2.0 * (reach - 0.45), 0.62f32, 0.12f32);
    let y = LAMP_Y + 0.12;
    canopy.cuboid(vec3(0.0, y + h / 2.0, 0.0), vec3(l, h, w), 0.0, WHITE, 1.6);
    canopy.cuboid(vec3(0.0, y + h + 0.02, 0.0), vec3(l - 0.12, 0.04, w - 0.12), 0.0, WHITE, 1.6);
    // The diffuser under it.
    glow.quad(
        [
            vec3(-l / 2.0 + 0.05, y - 0.001, w / 2.0 - 0.05),
            vec3(l / 2.0 - 0.05, y - 0.001, w / 2.0 - 0.05),
            vec3(l / 2.0 - 0.05, y - 0.001, -w / 2.0 + 0.05),
            vec3(-l / 2.0 + 0.05, y - 0.001, -w / 2.0 + 0.05),
        ],
        [Vec2::ZERO; 4],
        rgb(255, 240, 205),
    );
    let gold = rgb(196, 156, 72);
    // A brass rim and a short fringe all round.
    for (a, b) in [
        (vec3(-l / 2.0, y, w / 2.0), vec3(l / 2.0, y, w / 2.0)),
        (vec3(l / 2.0, y, w / 2.0), vec3(l / 2.0, y, -w / 2.0)),
        (vec3(l / 2.0, y, -w / 2.0), vec3(-l / 2.0, y, -w / 2.0)),
        (vec3(-l / 2.0, y, -w / 2.0), vec3(-l / 2.0, y, w / 2.0)),
    ] {
        tube(&mut brass, a, b, 0.008, 6, gold);
        if quality != Quality::Low {
            let n = ((b - a).length() / 0.025) as i32;
            for k in 0..n {
                let p = a.lerp(b, (k as f32 + 0.5) / n as f32);
                let out = vec3(
                    p.x.signum() * (p.x.abs() >= l / 2.0 - 1e-3) as i32 as f32,
                    0.0,
                    p.z.signum() * (p.z.abs() >= w / 2.0 - 1e-3) as i32 as f32,
                ) * 0.01;
                tube(&mut brass, p + out, p + out - Vec3::Y * 0.05, 0.003, 3, rgb(150, 110, 50));
            }
        }
    }
    for x in [-l / 2.0 + 0.275, l / 2.0 - 0.275] {
        tube(&mut brass, vec3(x, y + h + 0.04, 0.0), vec3(x, HEIGHT, 0.0), 0.006, 6, gold);
    }
    kit.add(canopy, Some(wood), Surface::lit(0.6, 60.0, 0.06, 0.5), Layer::Lamp);
    kit.add(brass, None, Surface::lit(1.4, 80.0, 0.1, 0.6), Layer::Lamp);
    kit.add(glow, None, Surface::UNLIT, Layer::Lamp);
}

// --- The industrial loft -------------------------------------------------------

/// An industrial loft: brick walls, tall steel windows over the city at
/// dusk, a concrete floor, black beams and ducts, Edison bulbs, a neon sign.
fn loft(kit: &mut Kit, quality: Quality, table: Vec2) {
    let detailed = quality >= Quality::High;
    let medium = quality >= Quality::Medium;
    let (brick_px, concrete_px, city_px) = std::thread::scope(|s| {
        let brick = s.spawn(tx::brick);
        let concrete = s.spawn(tx::concrete);
        let city = s.spawn(|| tx::city(5));
        (
            brick.join().expect("brick"),
            concrete.join().expect("concrete"),
            city.join().expect("city"),
        )
    });
    let brick_tex = Some(kit.texture(brick_px, true));
    let concrete_tex = Some(kit.texture(concrete_px, true));
    let city_tex = Some(kit.texture(city_px, false));
    let glow_tex = Some(kit.texture(textures::glow(), false));
    let wood_tex =
        Some(kit.texture(textures::fine_wood([0.42, 0.28, 0.17], [0.20, 0.12, 0.07], 163), true));

    let mut lights = table_lights(vec3(1.0, 0.86, 0.66), table);
    for x in [-3.3f32, 0.0, 3.3] {
        lights.push(light(vec3(x, 1.9, -HZ - 0.8), vec3(0.45, 0.55, 0.95), 1.0, false));
    }
    for z in [-2.2f32, 0.0, 2.2] {
        lights.push(light(vec3(HX - 1.0, 2.2, z), vec3(1.0, 0.7, 0.4), 0.45, true));
    }
    lights.push(light(vec3(-HX + 0.2, 2.0, 0.6), vec3(0.3, 0.9, 1.0), 0.35, false));
    lights.push(light(vec3(-2.6, 1.2, HZ - 1.2), vec3(1.0, 0.72, 0.45), 0.4, false));
    let mut lit = Lighting::new(quality, lights, vec3(0.2, 0.2, 0.24), table, 2.6);
    // The bar along the right wall and the sofa cast shadows.
    lit.occlude_box(vec3(HX - 0.7, 0.53, 0.0), vec3(0.7, 1.06, 3.6));
    lit.occlude_box(vec3(-HX + 0.65, 0.4, -1.6), vec3(0.9, 0.8, 2.2));
    let cell = cell(quality);

    let mut floor = MeshBuilder::default();
    let mut ceiling = MeshBuilder::default();
    floor_and_ceiling(&mut floor, &mut ceiling, cell, 0.25, 0.4, rgb(110, 110, 114));
    lit.floor(kit, floor, concrete_tex, 0.24, (0.35, 40.0, 0.25));
    lit.add(kit, ceiling, concrete_tex);

    // Brick walls; tall windows in the back wall.
    let mut bricks = MeshBuilder::default();
    let mut steel = MeshBuilder::default();
    let mut sky = MeshBuilder::default();
    let black = rgb(28, 30, 32);
    let (sill, head) = (0.75f32, 2.85f32);
    let windows = [(-3.6f32, -1.4f32), (-1.0, 1.0), (1.4, 3.6)];
    for (i, w) in walls().iter().enumerate() {
        if i == 0 {
            band(&mut bricks, w, 0.0, w.length, 0.0, sill, cell, WHITE, 1.0 / 1.2);
            band(&mut bricks, w, 0.0, w.length, head, HEIGHT, cell, WHITE, 1.0 / 1.2);
            let mut x = 0.0;
            for (a, b) in windows {
                let (a, b) = (a + HX, b + HX);
                band(&mut bricks, w, x, a, sill, head, cell, WHITE, 1.0 / 1.2);
                x = b;
                // The steel frame: a grid of mullions and transoms.
                let p =
                    |along: f32, y: f32| w.start + w.along * along + Vec3::Y * y + w.inward * 0.03;
                for k in 0..=4 {
                    let along = a + (b - a) * k as f32 / 4.0;
                    steel.cuboid(
                        p(along, (sill + head) / 2.0),
                        vec3(0.04, head - sill, 0.06),
                        0.0,
                        black,
                        1.0,
                    );
                }
                for k in 0..=6 {
                    let y = sill + (head - sill) * k as f32 / 6.0;
                    steel.cuboid(p((a + b) / 2.0, y), vec3(b - a, 0.035, 0.06), 0.0, black, 1.0);
                }
                // A deep sill.
                steel.cuboid(
                    p((a + b) / 2.0, sill - 0.02) + w.inward * 0.06,
                    vec3(b - a + 0.1, 0.04, 0.2),
                    0.0,
                    rgb(90, 90, 92),
                    1.0,
                );
            }
            band(&mut bricks, w, x, w.length, sill, head, cell, WHITE, 1.0 / 1.2);
            backdrop(&mut sky, w, 1.2, -0.6, 4.6, false, 1.0);
        } else {
            band(&mut bricks, w, 0.0, w.length, 0.0, HEIGHT, cell, WHITE, 1.0 / 1.2);
        }
    }
    lit.add(kit, bricks, brick_tex);
    add_glow(kit, sky, city_tex);

    // Black steel beams and silver ducts under the ceiling.
    let mut ducts = MeshBuilder::default();
    if medium {
        for k in -2..=2 {
            let x = k as f32 * 2.3;
            steel.cuboid(vec3(x, HEIGHT - 0.2, 0.0), vec3(0.03, 0.36, 2.0 * HZ), 0.0, black, 1.0);
            for dy in [-0.37f32, -0.02] {
                steel.cuboid(
                    vec3(x, HEIGHT + dy, 0.0),
                    vec3(0.18, 0.025, 2.0 * HZ),
                    0.0,
                    black,
                    1.0,
                );
            }
        }
        for z in [-2.9f32, 2.9] {
            tube(
                &mut ducts,
                vec3(-HX, HEIGHT - 0.6, z),
                vec3(HX, HEIGHT - 0.6, z),
                0.22,
                18,
                rgb(170, 174, 180),
            );
            if detailed {
                for k in 0..10 {
                    let x = -HX + 0.5 + k as f32 * 1.1;
                    hoop_x(&mut ducts, vec3(x, HEIGHT - 0.6, z), 0.225, 0.012, rgb(120, 124, 130));
                }
            }
        }
    }

    // The bar along the right wall: a reclaimed-wood counter on black
    // steel, open shelves on the brick, stools, Edison bulbs.
    let mut wood = MeshBuilder::default();
    let mut glass = MeshBuilder::default();
    let mut glow = MeshBuilder::default();
    let bx = HX - 0.75;
    wood.cuboid(vec3(bx, 1.05, 0.0), vec3(0.7, 0.07, 3.6), 0.0, WHITE, 1.0);
    steel.cuboid(vec3(bx + 0.05, 0.51, 0.0), vec3(0.5, 1.0, 3.4), 0.0, rgb(40, 42, 44), 1.0);
    for y in [1.5f32, 1.95] {
        wood.cuboid(vec3(HX - 0.17, y, 0.0), vec3(0.3, 0.04, 3.2), 0.0, WHITE, 1.0);
        for z in [-1.5f32, 0.0, 1.5] {
            steel.cuboid(vec3(HX - 0.17, y - 0.08, z), vec3(0.28, 0.12, 0.02), 0.0, black, 1.0);
        }
    }
    for (y, n) in [(1.52f32, 12), (1.97, 10)] {
        bottles(&mut glass, vec3(HX - 0.17, y, -1.35), Vec3::Z, n, if detailed { 12 } else { 8 });
    }
    let mut stools = MeshBuilder::default();
    for k in 0..4 {
        let c = vec3(bx - 0.65, 0.0, -1.2 + k as f32 * 0.8);
        stools.lathe(
            c,
            &[
                (0.0, 0.18),
                (0.02, 0.18),
                (0.03, 0.03),
                (0.7, 0.025),
                (0.72, 0.17),
                (0.76, 0.17),
                (0.77, 0.0),
            ],
            16,
            if k % 2 == 0 { rgb(40, 40, 42) } else { rgb(110, 60, 30) },
        );
        hoop(&mut stools, c + Vec3::Y * 0.28, 0.14, 0.01, 14, rgb(50, 50, 52));
    }
    // Pendants over the bar: cloth cords and bare bulbs.
    for z in [-1.2f32, -0.4, 0.4, 1.2] {
        let at = vec3(bx, 2.1 + (z * 3.0).sin() * 0.06, z);
        tube(&mut steel, at + Vec3::Y * 0.12, vec3(at.x, HEIGHT, at.z), 0.005, 4, rgb(30, 28, 26));
        steel.lathe(at + Vec3::Y * 0.08, &[(0.0, 0.02), (0.05, 0.02)], 8, rgb(150, 120, 60));
        glow.lathe(
            at - Vec3::Y * 0.06,
            &[(0.0, 0.0), (0.02, 0.03), (0.09, 0.05), (0.13, 0.02), (0.14, 0.02)],
            12,
            rgb(255, 196, 110),
        );
        kit.glow(at - Vec3::Y * 0.01, 0.2, Color::new(1.0, 0.68, 0.35, 0.55), false);
    }

    // A worn leather sofa, a rug and a crate table on the left; a neon sign.
    let mut leather = MeshBuilder::default();
    sofa(
        &mut leather,
        vec3(-HX + 0.65, 0.0, -1.6),
        std::f32::consts::FRAC_PI_2,
        2.2,
        rgb(120, 70, 38),
        false,
    );
    wood.cuboid(vec3(-HX + 1.75, 0.2, -1.6), vec3(0.6, 0.4, 1.0), 0.0, WHITE, 1.0);
    let mut plants = MeshBuilder::default();
    let mut pots = MeshBuilder::default();
    if medium {
        potted_plant(
            &mut plants,
            &mut pots,
            vec3(-HX + 0.5, 0.0, -3.4),
            1.5,
            false,
            rgb(30, 30, 32),
        );
        potted_plant(&mut plants, &mut pots, vec3(HX - 0.5, 0.0, 3.3), 1.7, false, rgb(30, 30, 32));
        potted_plant(
            &mut plants,
            &mut pots,
            vec3(-HX + 0.5, 0.0, 3.3),
            1.2,
            true,
            rgb(160, 150, 140),
        );
    }
    let mut neon = MeshBuilder::default();
    let mut pools = MeshBuilder::default();
    if detailed {
        // A neon eight ball and an arrow on the brick.
        let nc = vec3(-HX + 0.06, 2.05, 0.9);
        let cyan = rgb(80, 230, 255);
        let pink = rgb(255, 90, 160);
        let ring = |m: &mut MeshBuilder, c: Vec3, r: f32, color: Color| {
            for k in 0..28 {
                let a0 = k as f32 / 28.0 * std::f32::consts::TAU;
                let a1 = (k + 1) as f32 / 28.0 * std::f32::consts::TAU;
                let p = |a: f32| c + vec3(0.0, a.sin() * r, a.cos() * r);
                tube(m, p(a0), p(a1), 0.01, 6, color);
            }
        };
        ring(&mut neon, nc, 0.32, cyan);
        kit.glow(nc + Vec3::X * 0.05, 0.7, Color::new(0.35, 0.85, 1.0, 0.3), false);
        ring(&mut neon, nc + Vec3::Y * 0.075, 0.065, pink);
        ring(&mut neon, nc - Vec3::Y * 0.065, 0.08, pink);
        for (a, b) in [
            (vec3(0.0, -0.55, -0.6), vec3(0.0, -0.55, 0.6)),
            (vec3(0.0, -0.55, 0.6), vec3(0.0, -0.45, 0.48)),
            (vec3(0.0, -0.55, 0.6), vec3(0.0, -0.65, 0.48)),
        ] {
            tube(&mut neon, nc + a, nc + b, 0.01, 6, cyan);
        }
        pool(&mut pools, nc, Vec3::X, 2.0, 2.0, Color::new(0.3, 0.85, 1.0, 0.16));
        // A big factory clock on the front wall.
        let cc = vec3(2.2, 2.05, HZ - 0.04);
        disc_facing(&mut steel, cc, -Vec3::Z, 0.45, 40, black);
        disc_facing(&mut leather, cc - Vec3::Z * 0.01, -Vec3::Z, 0.41, 40, rgb(226, 220, 204));
        for k in 0..12 {
            let a = k as f32 / 12.0 * std::f32::consts::TAU;
            let p = cc + vec3(a.cos() * 0.35, a.sin() * 0.35, -0.02);
            steel.cuboid(
                p,
                vec3(0.02, 0.02, 0.01) * if k % 3 == 0 { 1.8 } else { 1.0 },
                0.0,
                black,
                1.0,
            );
        }
        tube(&mut steel, cc - Vec3::Z * 0.025, cc + vec3(0.0, 0.3, -0.025), 0.008, 4, black);
        tube(&mut steel, cc - Vec3::Z * 0.025, cc + vec3(0.2, -0.08, -0.025), 0.01, 4, black);
    }
    // Light from the windows on the floor.
    let mut shafts = MeshBuilder::default();
    if medium {
        for (a, b) in windows {
            let corners = [
                vec3(a, sill, -HZ + 0.04),
                vec3(b, sill, -HZ + 0.04),
                vec3(b, head, -HZ + 0.04),
                vec3(a, head, -HZ + 0.04),
            ];
            let floor = corners.map(|c| vec3(c.x, 0.02, c.z + c.y * 0.9));
            shaft(&mut shafts, corners, floor, Color::new(0.55, 0.65, 1.0, 0.05));
        }
    }

    lit.shiny(kit, steel, None, (0.5, 40.0, 0.2));
    lit.shiny(kit, ducts, None, (0.9, 50.0, 0.4));
    lit.shiny(kit, wood, wood_tex, SATIN);
    lit.shiny(kit, glass, None, GLASS);
    lit.add(kit, stools, None);
    lit.shiny(kit, leather, None, LEATHER);
    lit.add(kit, plants, None);
    lit.add(kit, pots, None);
    add_glow(kit, glow, None);
    add_glow(kit, neon, None);
    kit.add(pools, glow_tex, Surface::UNLIT, Layer::Blended);
    kit.add(shafts, None, Surface::UNLIT, Layer::Blended);
}

/// A ring round a duct running along X.
pub(crate) fn hoop_x(mb: &mut MeshBuilder, center: Vec3, radius: f32, r: f32, color: Color) {
    for k in 0..16 {
        let a0 = k as f32 / 16.0 * std::f32::consts::TAU;
        let a1 = (k + 1) as f32 / 16.0 * std::f32::consts::TAU;
        let p = |a: f32| center + vec3(0.0, a.cos() * radius, a.sin() * radius);
        tube(mb, p(a0), p(a1), r, 5, color);
    }
}

/// Black enamelled factory shades with bare Edison bulbs.
fn loft_lamp(kit: &mut Kit, quality: Quality, reach: f32) {
    let grow = reach - 0.45;
    let segs = if quality != Quality::Low { 32 } else { 18 };
    let mut metal = MeshBuilder::default();
    let mut inside = MeshBuilder::default();
    let mut glow = MeshBuilder::default();
    for x in lamp_xs(reach + 0.05) {
        let at = vec3(x, LAMP_Y, 0.0);
        let profile = [
            (0.0, 0.2),
            (0.015, 0.19),
            (0.07, 0.13),
            (0.15, 0.07),
            (0.2, 0.05),
            (0.25, 0.04),
            (0.26, 0.0),
        ];
        metal.lathe(at, &profile, segs, rgb(24, 26, 28));
        inside.frustum_at(at, 0.003, 0.186, 0.14, 0.072, segs, rgb(240, 236, 226));
        glow.lathe(
            at + Vec3::Y * 0.02,
            &[(0.0, 0.0), (0.02, 0.025), (0.08, 0.04), (0.12, 0.016), (0.14, 0.016)],
            14,
            rgb(255, 196, 110),
        );
        metal.lathe(at + Vec3::Y * 0.26, &[(0.0, 0.014), (0.18, 0.014)], 8, rgb(20, 20, 20));
    }
    // A black pipe and chains to the ceiling.
    tube(
        &mut metal,
        vec3(-0.75 - grow, LAMP_Y + 0.44, 0.0),
        vec3(0.75 + grow, LAMP_Y + 0.44, 0.0),
        0.018,
        10,
        rgb(24, 24, 26),
    );
    for x in [-0.7 - grow, 0.7 + grow] {
        tube(
            &mut metal,
            vec3(x, LAMP_Y + 0.44, 0.0),
            vec3(x, HEIGHT, 0.0),
            0.006,
            4,
            rgb(30, 30, 30),
        );
    }
    kit.add(metal, None, Surface::lit(1.2, 70.0, 0.15, 0.5), Layer::Lamp);
    kit.add(inside, None, Surface::lit(0.4, 30.0, 0.0, 0.1), Layer::Lamp);
    kit.add(glow, None, Surface::UNLIT, Layer::Lamp);
}

/// The pool hall's lamp: three green glass shades on a brass bar.
fn pool_lamp(kit: &mut Kit, quality: Quality, reach: f32) {
    let grow = reach - 0.45;
    let mut glass = MeshBuilder::default();
    let mut brass = MeshBuilder::default();
    let mut glow = MeshBuilder::default();
    let shade_seg = if quality != Quality::Low { 32 } else { 18 };
    for x in lamp_xs(reach) {
        let at = vec3(x, LAMP_Y, 0.0);
        // Bell: wide at the bottom, curving in to a small neck.
        let profile = [(0.0, 0.165), (0.05, 0.150), (0.11, 0.115), (0.16, 0.070), (0.19, 0.040)];
        for w in profile.windows(2) {
            glass.frustum_at(at, w[0].0, w[0].1, w[1].0, w[1].1, shade_seg, rgb(44, 140, 82));
        }
        // Inside of the shade: bright near the bulb, warm towards the rim.
        glow.frustum_at(at, 0.001, 0.161, 0.06, 0.143, shade_seg, rgb(255, 214, 150));
        glow.frustum_at(at, 0.06, 0.143, 0.15, 0.073, shade_seg, rgb(255, 236, 196));
        glow.disc(at + Vec3::Y * 0.15, 0.073, shade_seg, rgb(255, 246, 225));
        glow.disc(at + Vec3::Y * 0.07, 0.042, 20, rgb(255, 255, 245));
        // Brass rim and neck.
        brass.frustum_at(at, -0.006, 0.168, 0.006, 0.168, shade_seg, rgb(176, 136, 62));
        brass.frustum_at(at, 0.19, 0.040, 0.24, 0.022, 16, rgb(176, 136, 62));
    }
    // The bar that holds the shades and the rods to the ceiling.
    brass.cuboid(
        vec3(0.0, LAMP_Y + 0.25, 0.0),
        vec3(1.24 + 2.0 * grow, 0.028, 0.04),
        0.0,
        rgb(150, 112, 50),
        1.0,
    );
    for x in [-0.55 - grow, 0.55 + grow] {
        brass.cuboid(
            vec3(x, LAMP_Y + 0.25 + 0.75, 0.0),
            vec3(0.008, 1.5, 0.008),
            0.0,
            rgb(60, 50, 40),
            1.0,
        );
    }
    kit.add(glass, None, Surface::lit(0.9, 90.0, 0.35, 0.8), Layer::Lamp);
    kit.add(brass, None, Surface::lit(1.0, 50.0, 0.1, 0.6), Layer::Lamp);
    kit.add(glow, None, Surface::UNLIT, Layer::Lamp);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every hall builds at every quality, stays within a vertex budget
    /// close to the pool hall's, and its parts fit a 16-bit mesh.
    #[test]
    fn every_hall_builds_within_budget() {
        let table = vec2(1.35, 0.8);
        let budget = |q: Quality| {
            let k = build(q, table, HallStyle::Pool);
            k.parts.iter().map(|p| p.mesh.len()).sum::<usize>()
        };
        for q in [Quality::Low, Quality::Medium, Quality::High, Quality::Ultra] {
            let pool = budget(q);
            for style in [
                HallStyle::Beach,
                HallStyle::Club,
                HallStyle::Loft,
                HallStyle::Lodge,
                HallStyle::Arcade,
                HallStyle::Deco,
                HallStyle::Penthouse,
            ] {
                let kit = build(q, table, style);
                let verts: usize = kit.parts.iter().map(|p| p.mesh.len()).sum();
                eprintln!("{style:?} {q:?}: {verts} vertices (pool hall {pool})");
                assert!(verts < pool * 2 + 20_000, "{style:?} {q:?}: {verts} vs {pool}");
                assert!(kit.parts.iter().any(|p| p.layer == Layer::Lamp), "a lamp");
                for p in &kit.parts {
                    if let Some(t) = p.tex {
                        assert!(t < kit.textures.len());
                    }
                }
            }
        }
    }
}
