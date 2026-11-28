//! The cues as they are made: the turned shape, a lathe from the domed
//! leather tip to the rubber bumper (ferrule, a pro-taper shaft, the
//! collars of the joint, the forearm, the rings, the wrap, the butt sleeve
//! and its cap), and the skin laid on it, one texture per cue: the
//! shaft's maple, the forearm's wood or pattern with its veneers and
//! inlays, ring work, Irish linen or stitched leather, metal.
//!
//! The texture's alpha says what each texel is made of ([`Finish`]), and
//! the shader gives metal, lacquer, linen, pearl and rubber each their own
//! light ([`crate::gfx::Surface::CUE`]). The finer the cue (its rarity),
//! the richer its make: plain ivory collars and a single ring on the
//! cheapest, veneered points, steel and ring stacks above, gold, checks
//! and a stitched leather wrap on the finest. The shop's pictures read the
//! same skin ([`crate::cues::image`]).
//!
//! Along the cue, `y` runs in metres from the tip's face (0) to the
//! bumper (`LENGTH`); `u` runs round it (0 to 1).

use macroquad::prelude::{Color, Vec2, Vec3, vec2, vec3};
use snook_legend_proto::shop::{CueStyle, Rarity};

use crate::cues::{Design, Paint, hash, mix, paints_sleeve, pattern, shade, wrap_color};
use crate::gfx::MeshBuilder;
use crate::textures::Pixels;

/// Length of the cue, tip to butt.
pub const LENGTH: f32 = 1.48;

/// The apex of the tip's dome.
const TIP_END: f32 = -0.0046;
/// Where the dome meets the leather's side.
const TIP: f32 = -0.0027;
/// The fibre pad between the tip and the ferrule.
const PAD: f32 = 0.0007;
const SHAFT: f32 = 0.022;
const SHAFT_COLLAR: f32 = 0.785;
const JOINT: f32 = 0.80;
const FOREARM: f32 = 0.815;
const TRIM: f32 = 1.10;
const WRAP: f32 = 1.115;
const WRAP_END: f32 = 1.35;
const SLEEVE: f32 = 1.36;
const CAP: f32 = 1.455;
const BUMPER: f32 = 1.47;

/// Where along the texture (`v`, 0 to 1) each part lies: most of it for
/// what is worked (the forearm, the sleeve, the rings), little for the
/// plain shaft, whose grain runs along it.
const KNOTS: [(f32, f32); 12] = [
    (TIP_END - 0.0001, 0.0),
    (SHAFT, 0.03),
    // (Both ends of the shaft finely too: they meet the ivory of the
    // ferrule and the collar.)
    (SHAFT + 0.008, 0.0375),
    (SHAFT_COLLAR - 0.008, 0.1425),
    (SHAFT_COLLAR, 0.15),
    (FOREARM, 0.20),
    (TRIM, 0.58),
    (WRAP, 0.595),
    (WRAP_END, 0.74),
    (SLEEVE, 0.755),
    (CAP, 0.94),
    (LENGTH + 0.0001, 1.0),
];

fn v_of(y: f32) -> f32 {
    for w in KNOTS.windows(2) {
        let ((y0, v0), (y1, v1)) = (w[0], w[1]);
        if y <= y1 {
            return v0 + (v1 - v0) * ((y - y0) / (y1 - y0)).clamp(0.0, 1.0);
        }
    }
    1.0
}

fn y_of(v: f32) -> f32 {
    for w in KNOTS.windows(2) {
        let ((y0, v0), (y1, v1)) = (w[0], w[1]);
        if v <= v1 {
            return y0 + (y1 - y0) * ((v - v0) / (v1 - v0)).clamp(0.0, 1.0);
        }
    }
    LENGTH
}

/// The tip's dome: a cap of a sphere ("a nickel's curve") of this radius.
fn dome() -> (f32, f32) {
    let (r, h) = (0.0064f32, TIP - TIP_END);
    let sphere = (r * r + h * h) / (2.0 * h);
    (sphere, TIP_END + sphere)
}

/// The radius of the cue at `y`: a 13 mm tip, a pro taper (the first
/// 30 cm of the shaft barely thicken, then it swells to the joint), the
/// forearm and the butt thickening to 29.6 mm.
pub fn radius(y: f32) -> f32 {
    let lerp = |y0: f32, r0: f32, y1: f32, r1: f32| {
        r0 + (r1 - r0) * ((y - y0) / (y1 - y0)).clamp(0.0, 1.0)
    };
    if y < TIP {
        let (sphere, centre) = dome();
        (sphere * sphere - (centre - y).powi(2)).max(0.0).sqrt()
    } else if y < 0.0 {
        lerp(TIP, 0.0064, 0.0, 0.0065)
    } else if y < SHAFT {
        lerp(0.0, 0.0065, SHAFT, 0.0066)
    } else if y < 0.33 {
        lerp(SHAFT, 0.0066, 0.33, 0.00675)
    } else if y < SHAFT_COLLAR {
        let t = (y - 0.33) / (SHAFT_COLLAR - 0.33);
        0.00675 + (0.0105 - 0.00675) * t.powf(1.35)
    } else if y < JOINT {
        0.0105
    } else if y < FOREARM {
        lerp(JOINT, 0.0105, FOREARM, 0.0106)
    } else if y < TRIM {
        lerp(FOREARM, 0.0106, TRIM, 0.0125)
    } else if y < WRAP {
        lerp(TRIM, 0.0125, WRAP, 0.01255)
    } else if y < WRAP_END {
        lerp(WRAP, 0.01255, WRAP_END, 0.0138)
    } else if y < SLEEVE {
        lerp(WRAP_END, 0.0138, SLEEVE, 0.01385)
    } else if y < CAP {
        lerp(SLEEVE, 0.01385, CAP, 0.0147)
    } else if y < BUMPER {
        lerp(CAP, 0.0147, BUMPER, 0.0148)
    } else {
        0.0141
    }
}

/// The outline of the cue, tip to butt: (y, radius) at each turn of the
/// lathe, with steps (the bumper's) as two stations at one `y`.
fn stations() -> Vec<(f32, f32)> {
    let mut s = Vec::new();
    // The dome, from its apex.
    let (sphere, centre) = dome();
    let top = (0.0064 / sphere).asin();
    let n = 7;
    for k in 0..=n {
        let a = top * k as f32 / n as f32;
        s.push((centre - sphere * a.cos(), sphere * a.sin()));
    }
    s.push((0.0, radius(0.0)));
    s.push((PAD, radius(PAD)));
    s.push((SHAFT, radius(SHAFT)));
    // The shaft, with a station at each of the texture's knots on it.
    let (a, b) = (SHAFT + 0.008, SHAFT_COLLAR - 0.008);
    let n = 28;
    for k in 0..=n {
        let y = a + (b - a) * k as f32 / n as f32;
        s.push((y, radius(y)));
    }
    s.push((SHAFT_COLLAR, radius(SHAFT_COLLAR)));
    // The joint: the shaft's collar meeting the butt's, a fine bevel
    // on either face.
    let r = radius(JOINT);
    s.push((JOINT - 0.0005, r));
    s.push((JOINT - 0.00015, r - 0.0003));
    s.push((JOINT + 0.00015, r - 0.0003));
    s.push((JOINT + 0.0005, r));
    s.push((FOREARM, radius(FOREARM)));
    for k in 1..=6 {
        let y = FOREARM + (TRIM - FOREARM) * k as f32 / 6.0;
        s.push((y, radius(y)));
    }
    s.push((WRAP, radius(WRAP)));
    // The wrap sits a hair below the rings either side of it.
    let inset = 0.00012;
    s.push((WRAP + 0.0005, radius(WRAP + 0.0005) - inset));
    for k in 1..=4 {
        let y = WRAP + (WRAP_END - WRAP) * k as f32 / 5.0;
        s.push((y, radius(y) - inset));
    }
    s.push((WRAP_END - 0.0005, radius(WRAP_END - 0.0005) - inset));
    s.push((WRAP_END, radius(WRAP_END)));
    s.push((SLEEVE, radius(SLEEVE)));
    for k in 1..=3 {
        let y = SLEEVE + (CAP - SLEEVE) * k as f32 / 3.0;
        s.push((y, radius(y)));
    }
    // The cap's edge rounded off, then the rubber bumper.
    s.push((BUMPER - 0.0012, 0.0148));
    s.push((BUMPER - 0.0004, 0.01474));
    s.push((BUMPER, 0.01462));
    s.push((BUMPER, 0.0141));
    s.push((1.4765, 0.0141));
    s.push((1.4778, 0.0137));
    s.push((1.4789, 0.0130));
    s.push((1.4796, 0.0120));
    s.push((LENGTH, 0.0104));
    s.push((LENGTH, 0.0));
    s
}

/// The cue's shape, turned: `segments` round it. Neighbouring bands share
/// a smooth normal unless they meet at an edge (the steps, the joint).
pub fn shape(segments: u32) -> MeshBuilder {
    let st = stations();
    let mut m = MeshBuilder::default();
    // Each band's normal in (radial, along): perpendicular to its slope.
    let normal = |a: (f32, f32), b: (f32, f32)| -> Option<Vec2> {
        let (dy, dr) = (b.0 - a.0, b.1 - a.1);
        (dy.abs() + dr.abs() > 1e-7).then(|| vec2(dy, -dr).normalize())
    };
    let bands: Vec<Option<Vec2>> = st.windows(2).map(|w| normal(w[0], w[1])).collect();
    let smooth = |a: Vec2, b: Option<Vec2>| match b {
        Some(b) if a.dot(b) > 0.92 => (a + b).normalize(),
        _ => a,
    };
    for (i, band) in bands.iter().enumerate() {
        let Some(n) = *band else { continue };
        let before = if i > 0 { bands[i - 1] } else { None };
        let after = bands.get(i + 1).copied().flatten();
        let ends = [(st[i], smooth(n, before)), (st[i + 1], smooth(n, after))];
        let first = m.vertices.len() as u16;
        for ((y, r), n) in ends {
            let v = v_of(y);
            for k in 0..=segments {
                let u = k as f32 / segments as f32;
                let a = u * std::f32::consts::TAU;
                let d = vec3(a.cos(), 0.0, a.sin());
                let normal = d * n.x + Vec3::Y * n.y;
                m.push(d * r + Vec3::Y * y, vec2(u, v), normal, Color::new(1.0, 1.0, 1.0, 1.0));
            }
        }
        let stride = segments as u16 + 1;
        for k in 0..segments as u16 {
            let a = first + k;
            let b = a + stride;
            m.indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    m
}

/// What a part of the cue is made of: the texture's alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finish {
    /// Polished metal: it mirrors the room in its own colour.
    Metal,
    /// Leather, linen, rubber, chalk: no gloss.
    Matte,
    /// An oiled maple shaft, a leather wrap: a soft sheen.
    Satin,
    /// Wood, paint and stone under a clear coat.
    Lacquer,
    /// Mother-of-pearl, cut stones, ice: lacquer whose colours shift with
    /// the angle.
    Pearl,
}

impl Finish {
    /// (Ordered so that what lies side by side on a cue blends through
    /// nothing odd as the texture is filtered: lacquer next to pearl and
    /// to satin, metal at the far end, past a sliver of matte.)
    pub fn alpha(self) -> u8 {
        match self {
            Finish::Metal => 0,
            Finish::Matte => 84,
            Finish::Satin => 140,
            Finish::Lacquer => 191,
            Finish::Pearl => 255,
        }
    }
}

/// Everything a cue's skin is made from.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub design: Design,
    /// The chalk on the tip.
    pub chalk: Color,
    /// The cue's level: its marks (metal bands, pearl, engraved gold).
    pub level: u8,
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba(r, g, b, 255)
}

fn luma(c: Color) -> f32 {
    0.3 * c.r + 0.59 * c.g + 0.11 * c.b
}

/// Hue (degrees), saturation and value.
fn hsv(c: Color) -> (f32, f32, f32) {
    let max = c.r.max(c.g).max(c.b);
    let min = c.r.min(c.g).min(c.b);
    let d = max - min;
    let hue = if d <= 1e-5 {
        0.0
    } else if max == c.r {
        60.0 * ((c.g - c.b) / d).rem_euclid(6.0)
    } else if max == c.g {
        60.0 * ((c.b - c.r) / d + 2.0)
    } else {
        60.0 * ((c.r - c.g) / d + 4.0)
    };
    (hue, if max > 0.0 { d / max } else { 0.0 }, max)
}

/// A gold (or brass) colour: a warm yellow, saturated and bright.
fn golden(c: Color) -> bool {
    let (h, s, v) = hsv(c);
    (25.0..=58.0).contains(&h) && s >= 0.3 && v >= 0.65
}

/// A silver colour: a cool or neutral light grey (not ivory's warm white).
fn silvery(c: Color) -> bool {
    let (_, s, v) = hsv(c);
    s < 0.13 && c.b >= c.r - 0.005 && v >= 0.68
}

fn metallic(c: Color) -> bool {
    golden(c) || silvery(c)
}

const STEEL: Color = Color::new(0.80, 0.82, 0.85, 1.0);
const GOLD: Color = Color::new(0.93, 0.77, 0.41, 1.0);
const EBONY: Color = Color::new(0.07, 0.065, 0.06, 1.0);
const IVORY: Color = Color::new(0.93, 0.91, 0.85, 1.0);

/// The metal of a cue's collars and pins, if it has any: the finer the
/// cue, the more metal (gold on the finest, unless it is a silver cue).
fn metal_of(d: &Design) -> Option<Color> {
    let ring = d.paint.ring;
    match d.tier {
        Rarity::Common => None,
        Rarity::Rare => metallic(ring).then_some(ring),
        Rarity::Epic => Some(if golden(ring) { ring } else { STEEL }),
        Rarity::Legendary => Some(if metallic(ring) { ring } else { GOLD }),
    }
}

/// The ring colour's finish: metal when it looks like one.
fn ring_finish(p: Paint) -> Finish {
    if metallic(p.ring) { Finish::Metal } else { Finish::Lacquer }
}

/// Smooth noise in 0..1, seamless round the cue: `x` (round it) repeats
/// every `period`.
fn noise(x: f32, y: f32, period: i32, seed: i32) -> f32 {
    let (xf, yf) = (x.floor(), y.floor());
    let (fx, fy) = (x - xf, y - yf);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (x0, y0) = (xf as i32, yf as i32);
    let w = |i: i32| i.rem_euclid(period.max(1));
    let h = |i: i32, j: i32| hash(w(i) * 7919 + seed * 104_729, j);
    let (a, b) = (h(x0, y0), h(x0 + 1, y0));
    let (c, d) = (h(x0, y0 + 1), h(x0 + 1, y0 + 1));
    let top = a + (b - a) * sx;
    let bottom = c + (d - c) * sx;
    top + (bottom - top) * sy
}

/// Wood cut along the cue: long streaks of its growth, darker grain lines
/// wandering through them, and fine pores.
fn wood(base: Color, u: f32, y: f32, seed: i32, strength: f32) -> Color {
    let wander = noise(u * 5.0, y * 3.0, 5, seed) * 2.2;
    let streak = noise(u * 28.0 + wander, y * 2.2, 28, seed + 1) * 0.6
        + noise(u * 70.0 + wander * 2.0, y * 6.0, 70, seed + 2) * 0.4;
    let lines = (streak * 7.0 + wander).fract();
    let line = 1.0 - ((lines - 0.5).abs() * 2.0).powf(0.35);
    let pore = noise(u * 420.0, y * 160.0, 420, seed + 3);
    let k = (streak - 0.5) * 0.34 - line * 0.16 - (pore - 0.75).max(0.0) * 0.5;
    shade(base, k * strength)
}

/// The tip: chalk on its domed face, layered leather on its side, the
/// chalk rubbed down it.
fn tip(l: &Look, y: f32, u: f32) -> (Color, Finish) {
    let chalk = l.chalk;
    let speck = noise(u * 36.0, y * 9000.0, 36, 11);
    if y < TIP {
        return (shade(chalk, (speck - 0.5) * 0.3), Finish::Matte);
    }
    let t = (y - TIP) / -TIP; // 0 at the face, 1 at the ferrule
    let leather = rgb(104, 68, 42);
    let layered = l.design.tier >= Rarity::Rare && (t * 7.0).fract() < 0.14;
    let leather = if layered { shade(leather, -0.35) } else { leather };
    let rubbed = (1.0 - t).powf(1.4) * (0.55 + 0.45 * speck);
    (mix(leather, chalk, rubbed), Finish::Matte)
}

/// The shaft: maple, oiled (a faint chalk blue near the ferrule, where the
/// hand slides), or carbon fibre's fine twill.
fn shaft(l: &Look, y: f32, u: f32) -> (Color, Finish) {
    let d = &l.design;
    if d.carbon {
        // Carbon fibre under a clear coat: fine strands along the shaft.
        let strands = noise(u * 260.0, y * 3.0, 260, 24) - 0.5;
        let tow = noise(u * 36.0, y * 1.5, 36, 25) - 0.5;
        return (shade(d.shaft, strands * 0.3 + tow * 0.2), Finish::Lacquer);
    }
    let streak = noise(u * 90.0, y * 2.5, 90, 21) - 0.5;
    let broad = noise(u * 20.0, y * 0.7, 20, 22) - 0.5;
    let ray = (noise(u * 160.0, y * 40.0, 160, 23) - 0.8).max(0.0);
    let c = shade(d.shaft, streak * 0.08 + broad * 0.07 + ray * 0.25);
    let near = (1.0 - (y - SHAFT) / 0.06).clamp(0.0, 1.0);
    (mix(c, l.chalk, near * near * 0.07), Finish::Satin)
}

/// The two collars of the joint (the shaft's and the butt's): ivory on a
/// plain cue, metal on a finer one, engraved gold on the finest.
fn collar(l: &Look, y: f32, u: f32) -> (Color, Finish) {
    let d = &l.design;
    let from = (y - JOINT).abs() * 1000.0; // mm from the joint's face
    let engraved_gold = l.level >= 6 || d.tier == Rarity::Legendary;
    let (base, finish) = match metal_of(d) {
        _ if l.level >= 6 => (GOLD, Finish::Metal),
        Some(m) => (m, Finish::Metal),
        None => (IVORY, Finish::Lacquer),
    };
    // A fine dark line near the outer edge of each collar.
    if (12.6..13.3).contains(&from) {
        return (EBONY, Finish::Lacquer);
    }
    if engraved_gold && (2.0..11.5).contains(&from) {
        // A running scroll cut into the metal.
        let x = u * std::f32::consts::TAU;
        let t = (from - 2.0) / 9.5;
        let wave = ((x * 9.0).sin() * 0.32 + 0.5 - t).abs();
        let curl = (((x * 18.0).sin() * 0.5 + 0.5) - t).abs();
        if wave < 0.06 || (curl < 0.04 && t > 0.15 && t < 0.85) {
            return (shade(base, -0.45), Finish::Metal);
        }
    }
    (base, finish)
}

/// A pattern of the shop's styles on the forearm or the sleeve (`v`: 0 at
/// its joint end, 1 at the butt end), with what it is made of: the wood
/// of a wooden ground, veneers along the points of a fine cue, metal wire
/// and settings, pearl inlays and stones.
fn worked(l: &Look, y: f32, u: f32, v: f32, length_mm: f32) -> (Color, Finish) {
    let d = &l.design;
    let p = d.paint;
    let style = d.style;
    let c = pattern(style, p, u, v);
    let finish = match style {
        CueStyle::Chrome => {
            if c == p.accent {
                Finish::Lacquer
            } else {
                Finish::Metal
            }
        }
        CueStyle::Damascus => Finish::Metal,
        CueStyle::Filigree if luma(c) > 0.22 => Finish::Metal,
        CueStyle::Gems if c == p.ring => Finish::Metal,
        CueStyle::Gems if c != p.butt => Finish::Pearl,
        CueStyle::Inlay if c != p.butt && c != shade(p.ring, -0.35) => Finish::Pearl,
        CueStyle::Crystal => Finish::Pearl,
        CueStyle::Scales | CueStyle::Stripes | CueStyle::Web | CueStyle::Confetti
            if c == p.ring =>
        {
            ring_finish(p)
        }
        _ => Finish::Lacquer,
    };
    let wooden = matches!(
        style,
        CueStyle::Plain
            | CueStyle::Points
            | CueStyle::Spiral
            | CueStyle::Flame
            | CueStyle::Inlay
            | CueStyle::Gems
    );
    if wooden && c == p.butt {
        return (wood(c, u, y, 31, 1.0), finish);
    }
    if style == CueStyle::Points && c == p.accent {
        // Veneers along each point's edges (two, three, four of them, the
        // finer the cue), then the point's own wood.
        let veneers: &[Color] = match d.tier {
            Rarity::Common => &[],
            Rarity::Rare => &[EBONY, IVORY],
            Rarity::Epic => &[EBONY, p.ring, EBONY],
            Rarity::Legendary => &[EBONY, p.ring, shade(p.accent, 0.45), EBONY],
        };
        let circ = std::f32::consts::TAU * radius(y) * 1000.0;
        let side = ((u * 4.0).fract() - 0.5).abs() * 2.0;
        let edge = 0.12 + 0.88 * side;
        let slope = 0.88 * 8.0 * length_mm / circ;
        let inside_mm = (v - edge) * length_mm / (1.0 + slope * slope).sqrt();
        let k = (inside_mm / 0.45) as usize;
        if let Some(&veneer) = veneers.get(k) {
            return (veneer, Finish::Lacquer);
        }
        return (wood(c, u, y, 37, 0.9), Finish::Lacquer);
    }
    (c, finish)
}

/// A stack of rings (above and below the wrap): ring work, the richer
/// the finer the cue: a single ring, then ebony lines and an accent, then
/// metal pins and checks, then gold and checks twice over.
fn rings(l: &Look, t: f32, u: f32, y: f32) -> (Color, Finish) {
    #[derive(Clone, Copy)]
    enum Band {
        Ring,
        Black,
        Metal,
        Accent,
        Checks,
    }
    use Band::*;
    let d = &l.design;
    let stack: &[(f32, Band)] = match d.tier {
        Rarity::Common => &[(0.6, Black), (13.8, Ring), (0.6, Black)],
        Rarity::Rare => &[
            (0.6, Black),
            (4.0, Ring),
            (0.6, Black),
            (3.6, Accent),
            (0.6, Black),
            (4.0, Ring),
            (0.6, Black),
        ],
        Rarity::Epic => &[
            (1.0, Metal),
            (0.5, Black),
            (3.0, Ring),
            (0.5, Black),
            (3.0, Checks),
            (0.5, Black),
            (3.0, Ring),
            (0.5, Black),
            (1.0, Metal),
        ],
        Rarity::Legendary => &[
            (1.2, Metal),
            (0.5, Black),
            (2.0, Ring),
            (0.5, Black),
            (2.4, Checks),
            (0.5, Black),
            (1.6, Accent),
            (0.5, Black),
            (2.4, Checks),
            (0.5, Black),
            (2.0, Ring),
            (0.5, Black),
            (1.2, Metal),
        ],
    };
    let total: f32 = stack.iter().map(|b| b.0).sum();
    let mut at = t * total;
    let mut band = (Ring, 0.0, 1.0);
    for &(w, b) in stack {
        if at < w {
            band = (b, at / w, w);
            break;
        }
        at -= w;
    }
    let p = d.paint;
    let metal = metal_of(d).unwrap_or(p.ring);
    match band.0 {
        Ring => (p.ring, ring_finish(p)),
        Black => (EBONY, Finish::Lacquer),
        Metal => (metal, Finish::Metal),
        Accent => (wood(p.accent, u, y, 41, 0.5), Finish::Lacquer),
        Checks => {
            // Little squares of ivory (or the ring's colour) and ebony.
            let circ = std::f32::consts::TAU * radius(y) * 1000.0;
            let n = (circ / (band.2 / 2.0)).round().max(8.0);
            let col = (u * n).floor() as i32;
            let row = (band.1 * 2.0).floor() as i32;
            if (col + row).rem_euclid(2) == 0 {
                (if metallic(p.ring) { IVORY } else { p.ring }, Finish::Lacquer)
            } else {
                (EBONY, Finish::Lacquer)
            }
        }
    }
}

/// The wrap: Irish linen wound round (two-toned, flecked), or on the
/// finest cues stitched leather with a seam down the back.
fn wrap(l: &Look, y: f32, u: f32) -> (Color, Finish) {
    let d = &l.design;
    let p = d.paint;
    let base = wrap_color(d.style, p);
    let mm = (y - WRAP) * 1000.0;
    let circ = std::f32::consts::TAU * radius(y) * 1000.0;
    if crate::cues::leather_wrap(d.style) {
        let pebble = noise(u * circ / 0.7, mm / 0.7, (circ / 0.7) as i32, 51) - 0.5;
        let c = shade(base, pebble * 0.12);
        let seam = (u - 0.75).abs();
        if seam < 0.0015 {
            return (shade(base, -0.5), Finish::Satin);
        }
        // A row of stitches either side of the seam.
        if (0.006..0.011).contains(&seam) && (mm / 3.2).fract() < 0.55 {
            return (shade(p.ring, -0.1), Finish::Satin);
        }
        let edge = mm.min((WRAP_END - y) * 1000.0);
        return (shade(c, if edge < 0.8 { -0.25 } else { 0.0 }), Finish::Satin);
    }
    // The cord, wound round in a fine helix.
    let phase = mm / 0.95 + u;
    let across = ((phase.fract() - 0.5).abs() * 2.0).powi(2);
    let c = shade(base, -0.28 * across);
    let fibre = hash((u * circ / 0.35) as i32, (phase * 3.0) as i32);
    let c = if fibre > 0.9 {
        mix(c, mix(base, p.ring, 0.5), 0.35)
    } else if fibre < 0.06 {
        shade(c, -0.12)
    } else {
        c
    };
    (c, Finish::Matte)
}

/// The butt cap: black phenolic, with a fine ring on a better cue and a
/// band of metal on the finer ones; the rubber bumper after it.
fn cap(l: &Look, y: f32, u: f32) -> (Color, Finish) {
    let d = &l.design;
    if y >= BUMPER {
        let grain = noise(u * 120.0, y * 4000.0, 120, 61) - 0.5;
        let c = rgb(22, 21, 20);
        // Moulded rings on its end.
        let ring = if y > LENGTH - 0.0003 { 0.12 * (y * 30_000.0).sin().abs() } else { 0.0 };
        return (shade(c, grain * 0.1 + ring), Finish::Matte);
    }
    let mm = (y - CAP) * 1000.0;
    if d.tier >= Rarity::Epic
        && (10.5..13.0).contains(&mm)
        && let Some(m) = metal_of(d)
    {
        return (m, Finish::Metal);
    }
    if d.tier >= Rarity::Rare && (3.0..3.8).contains(&mm) {
        return (d.paint.ring, ring_finish(d.paint));
    }
    (rgb(18, 17, 17), Finish::Lacquer)
}

/// The marks of the cue's level, over what is there: metal bands below
/// the joint (silver, then gold), pearl diamonds, gold rings on the butt
/// sleeve (the top level's engraved gold is in the collars).
fn marks(l: &Look, y: f32, u: f32, under: (Color, Finish)) -> (Color, Finish) {
    let level = l.level;
    if level == 0 {
        return under;
    }
    let metal = if level >= 4 { GOLD } else { STEEL };
    let pearl = rgb(244, 240, 228);
    let bands = match level {
        1 => 1,
        2..=4 => 2,
        _ => 3,
    };
    for k in 0..bands {
        let y0 = FOREARM + 0.011 + k as f32 * 0.012;
        if (y0..y0 + 0.005).contains(&y) {
            return (metal, Finish::Metal);
        }
    }
    let diamonds = |y0: f32, y1: f32, count: f32| {
        if !(y0..y1).contains(&y) {
            return false;
        }
        let v = (y - y0) / (y1 - y0);
        ((u * count).fract() - 0.5).abs() * 2.0 + (v - 0.5).abs() * 2.0 < 0.8
    };
    if level >= 3 {
        let y0 = FOREARM + 0.011 + bands as f32 * 0.012 + 0.004;
        if diamonds(y0, y0 + 0.03, 6.0) {
            return (pearl, Finish::Pearl);
        }
    }
    if level >= 4 && (SLEEVE + 0.012..SLEEVE + 0.018).contains(&y) {
        return (GOLD, Finish::Metal);
    }
    if level >= 5 {
        if diamonds(SLEEVE + 0.024, SLEEVE + 0.054, 8.0) {
            return (pearl, Finish::Pearl);
        }
        if (SLEEVE + 0.06..SLEEVE + 0.066).contains(&y) {
            return (GOLD, Finish::Metal);
        }
    }
    under
}

/// The colour and finish of the cue at `y` along it and `u` round it.
pub fn texel(l: &Look, y: f32, u: f32) -> (Color, Finish) {
    let d = &l.design;
    let made = if y < 0.0 {
        tip(l, y, u)
    } else if y < PAD {
        (rgb(30, 26, 24), Finish::Satin)
    } else if y < SHAFT {
        let fibre = noise(u * 60.0, y * 400.0, 60, 71) - 0.5;
        (shade(IVORY, fibre * 0.04 + 0.03), Finish::Lacquer)
    } else if y < SHAFT_COLLAR {
        shaft(l, y, u)
    } else if y < FOREARM {
        collar(l, y, u)
    } else if y < TRIM {
        let v = (y - FOREARM) / (TRIM - FOREARM);
        worked(l, y, u, v, (TRIM - FOREARM) * 1000.0)
    } else if y < WRAP {
        rings(l, (y - TRIM) / (WRAP - TRIM), u, y)
    } else if y < WRAP_END {
        wrap(l, y, u)
    } else if y < SLEEVE {
        rings(l, (y - WRAP_END) / (SLEEVE - WRAP_END), u, y)
    } else if y < CAP {
        if paints_sleeve(d.style) {
            let v = (y - SLEEVE) / (CAP - SLEEVE);
            worked(l, y, u, v, (CAP - SLEEVE) * 1000.0)
        } else {
            (wood(d.paint.butt, u, y, 33, 1.0), Finish::Lacquer)
        }
    } else {
        cap(l, y, u)
    };
    marks(l, y, u, made)
}

/// Whether `y` is in a worked part (patterns, rings, inlays), where the
/// skin is sampled finer so that its edges come out clean.
fn worked_part(y: f32) -> bool {
    y >= SHAFT_COLLAR && !(WRAP..WRAP_END).contains(&y)
}

/// The cue's skin, `width` round and `height` along: RGB, and what each
/// texel is made of in the alpha. `fine`: the worked parts sampled
/// four times a texel (for the background, sharp-edged).
pub fn skin(l: &Look, width: u16, height: u16, fine: bool) -> Pixels {
    let (w, h) = (width as usize, height as usize);
    let mut rgba = vec![0u8; w * h * 4];
    for j in 0..h {
        let v = (j as f32 + 0.5) / h as f32;
        let y = y_of(v);
        let samples: &[(f32, f32)] = if fine && worked_part(y) {
            &[(-0.25, -0.25), (0.25, -0.25), (-0.25, 0.25), (0.25, 0.25)]
        } else {
            &[(0.0, 0.0)]
        };
        let ys: Vec<f32> = samples.iter().map(|&(_, dv)| y_of(v + dv / h as f32)).collect();
        for i in 0..w {
            let mut sum = [0.0f32; 4];
            for (&(du, _), &y) in samples.iter().zip(&ys) {
                let u = ((i as f32 + 0.5 + du) / w as f32).rem_euclid(1.0);
                let (c, f) = texel(l, y, u);
                sum[0] += c.r;
                sum[1] += c.g;
                sum[2] += c.b;
                sum[3] += f.alpha() as f32 / 255.0;
            }
            let n = samples.len() as f32;
            let at = (j * w + i) * 4;
            for k in 0..4 {
                rgba[at + k] = ((sum[k] / n).clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
    Pixels { width, height, rgba }
}

/// The marks' glow at the top level: the engraved gold of the collars
/// lit from within, and a faint haze round the joint.
pub fn glow(level: u8) -> Option<MeshBuilder> {
    if level < 6 {
        return None;
    }
    let mut g = MeshBuilder::default();
    let (y0, y1) = (SHAFT_COLLAR, FOREARM);
    let lift = 0.0005;
    g.frustum(y0, radius(y0) + lift, y1, radius(y1) + lift, 32, Color::new(1.0, 0.86, 0.5, 0.22));
    let haze = [(0.76f32, 0.0f32), (0.79, 0.09), (0.81, 0.09), (0.85, 0.0)];
    for w in haze.windows(2) {
        let ((ya, aa), (yb, ab)) = (w[0], w[1]);
        let (ra, rb) = (radius(ya) * 1.3, radius(yb) * 1.3);
        let n = 24;
        for k in 0..n {
            let (t0, t1) = (
                k as f32 / n as f32 * std::f32::consts::TAU,
                (k + 1) as f32 / n as f32 * std::f32::consts::TAU,
            );
            let d = |t: f32| vec3(t.cos(), 0.0, t.sin());
            let ca = Color::new(1.0, 0.8, 0.4, aa);
            let cb = Color::new(1.0, 0.8, 0.4, ab);
            let i = [
                g.push(d(t0) * ra + Vec3::Y * ya, Vec2::ZERO, d(t0), ca),
                g.push(d(t1) * ra + Vec3::Y * ya, Vec2::ZERO, d(t1), ca),
                g.push(d(t1) * rb + Vec3::Y * yb, Vec2::ZERO, d(t1), cb),
                g.push(d(t0) * rb + Vec3::Y * yb, Vec2::ZERO, d(t0), cb),
            ];
            g.indices.extend_from_slice(&[i[0], i[1], i[2], i[0], i[2], i[3]]);
        }
    }
    Some(g)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The texture's map along the cue runs one way and back exactly.
    #[test]
    fn the_map_along_the_cue_inverts() {
        for k in 0..=400 {
            let y = TIP_END + (LENGTH - TIP_END) * k as f32 / 400.0;
            assert!((y_of(v_of(y)) - y).abs() < 1e-4, "{y}");
        }
        assert!(KNOTS.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1));
    }

    /// The outline runs from the tip's apex to the bumper's end, never
    /// backwards, and the shape fits in one mesh.
    #[test]
    fn the_shape_is_whole() {
        let s = stations();
        assert_eq!(s[0], (TIP_END, 0.0));
        assert_eq!(*s.last().unwrap(), (LENGTH, 0.0));
        assert!(s.windows(2).all(|w| w[1].0 >= w[0].0), "{s:?}");
        assert!((radius(TIP) - 0.0064).abs() < 1e-5);
        let m = shape(64);
        assert!(m.vertices.len() < u16::MAX as usize);
        // Every normal points outwards (away from the axis, or along it at
        // the ends).
        for v in &m.vertices {
            let p = vec3(v.position.x, v.position.y, v.position.z);
            let n = vec3(v.normal.x, v.normal.y, v.normal.z);
            let out = vec3(p.x, 0.0, p.z);
            assert!(n.dot(out) >= -1e-6, "{p} {n}");
        }
    }

    /// What lies where: chalk on the tip, the shaft's satin, metal on the
    /// finest cues' collars and ivory on the plainest, the rubber bumper.
    #[test]
    fn each_part_is_made_of_what_it_should() {
        let look = |code: &str, level: u8| Look {
            design: Design::of(code).expect(code),
            chalk: Color::new(0.2, 0.4, 0.8, 1.0),
            level,
        };
        let classic = look("cue_classic", 0);
        assert_eq!(texel(&classic, TIP - 0.001, 0.3).1, Finish::Matte);
        assert_eq!(texel(&classic, 0.4, 0.3).1, Finish::Satin);
        assert_eq!(texel(&classic, JOINT + 0.005, 0.3).1, Finish::Lacquer);
        assert_eq!(texel(&classic, LENGTH - 0.002, 0.3).1, Finish::Matte);
        let finest = snook_legend_proto::shop::ITEMS
            .iter()
            .find(|i| {
                i.category == snook_legend_proto::shop::Category::Cue
                    && snook_legend_proto::shop::rarity(i) == Rarity::Legendary
            })
            .expect("a legendary cue");
        let finest = look(finest.code, 0);
        assert_eq!(texel(&finest, JOINT + 0.0135, 0.3).1, Finish::Metal);
        // The marks of a cue's level.
        assert_eq!(texel(&look("cue_classic", 1), FOREARM + 0.013, 0.3).1, Finish::Metal);
    }

    /// Every cue's skin comes out, at every level.
    #[test]
    fn every_cue_has_a_skin() {
        use snook_legend_proto::shop::{Category, ITEMS};
        for item in ITEMS.iter().filter(|i| i.category == Category::Cue) {
            for level in [0, 6] {
                let l = Look {
                    design: Design::of(item.code).expect("a cue"),
                    chalk: Color::new(0.2, 0.4, 0.8, 1.0),
                    level,
                };
                let p = skin(&l, 32, 128, false);
                assert_eq!(p.rgba.len(), 32 * 128 * 4, "{}", item.code);
            }
        }
    }
}
