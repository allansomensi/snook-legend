//! The achievements' medals, rendered like small pieces of jewellery: a
//! struck metal disc (bronze, silver, gold; platinum set with a diamond;
//! gold set with a ruby) on a striped grosgrain ribbon.
//!
//! Each medal is ray-cast on the CPU once per size it is shown at (a height
//! field for the disc: a rounded, milled rim, a guilloché field with a ring
//! of beads, an embossed faceted star or a cut stone in a bezel; normals from
//! it, lit by a studio environment: a soft box at the top left, a dark
//! horizon band, a rim light; ambient occlusion in the recesses), with the
//! ribbon's satin sheen and woven ribs, the ring that holds the disc and a
//! soft drop shadow. The pictures are kept as textures, one per metal, lock
//! and size step, so drawing one is a single quad. Fine detail (the milled
//! edge, the beads, the guilloché, the weave) is left out at small sizes,
//! where it would only shimmer.

use std::cell::RefCell;
use std::collections::HashMap;

use macroquad::prelude::*;

/// The sizes (pixels) a medal is rendered at; the smallest that covers the
/// size on screen is used.
const STEPS: [u32; 8] = [24, 32, 48, 64, 96, 128, 192, 288];

thread_local! {
    static CACHE: RefCell<HashMap<(u8, bool, u32), Texture2D>> = RefCell::new(HashMap::new());
}

/// Draws the medal of `level` (0 bronze … 4 ruby) `size` high (the picture
/// is square), centred on `c`. `locked`: in grey, faint.
pub fn draw(c: Vec2, size: f32, level: u8, locked: bool) {
    draw_faded(c, size, level, locked, 1.0);
}

/// [`draw`], `alpha` opaque (times the layer's opacity and a screen's fade):
/// a medal fades out with the card it is on, at its size.
pub fn draw_faded(c: Vec2, size: f32, level: u8, locked: bool, alpha: f32) {
    let pixels = size * crate::ui::ui_scale();
    let step = STEPS.iter().copied().find(|s| *s as f32 >= pixels * 0.95).unwrap_or(STEPS[7]);
    let level = level.min(4);
    let texture = CACHE.with(|cache| {
        cache
            .borrow_mut()
            .entry((level, locked, step))
            .or_insert_with(|| {
                let rgba = render(level, locked, step);
                let t = Texture2D::from_rgba8(step as u16, step as u16, &rgba);
                t.set_filter(FilterMode::Linear);
                t
            })
            .clone()
    });
    draw_texture_ex(
        &texture,
        c.x - size / 2.0,
        c.y - size / 2.0,
        Color::new(1.0, 1.0, 1.0, (alpha * crate::paint::alpha()).clamp(0.0, 1.0)),
        DrawTextureParams { dest_size: Some(vec2(size, size)), ..Default::default() },
    );
}

/// The metal's base reflectance.
fn metal(level: u8) -> Vec3 {
    match level {
        0 => vec3(0.86, 0.52, 0.30),
        1 => vec3(0.90, 0.91, 0.93),
        2 => vec3(1.00, 0.80, 0.38),
        // Platinum, a little cooler than silver.
        3 => vec3(0.80, 0.86, 0.92),
        _ => vec3(1.00, 0.75, 0.32),
    }
}

/// The ribbon's stripes across its width (0 to 1, symmetric): the base
/// colour, then (from, to, colour) bands.
/// A stripe of a ribbon: from, to (across it, 0 to 1) and its colour.
type Band = (f32, f32, [f32; 3]);

fn ribbon(level: u8) -> (Vec3, &'static [Band]) {
    match level {
        0 => (
            vec3(0.46, 0.11, 0.10),
            &[(0.16, 0.24, [0.86, 0.58, 0.34]), (0.76, 0.84, [0.86, 0.58, 0.34])],
        ),
        1 => (vec3(0.10, 0.18, 0.42), &[(0.42, 0.58, [0.90, 0.90, 0.92])]),
        2 => (
            vec3(0.62, 0.06, 0.09),
            &[
                (0.10, 0.18, [0.98, 0.78, 0.32]),
                (0.44, 0.56, [0.96, 0.94, 0.90]),
                (0.82, 0.90, [0.98, 0.78, 0.32]),
            ],
        ),
        3 => (
            vec3(0.05, 0.30, 0.46),
            &[(0.26, 0.33, [0.86, 0.92, 0.96]), (0.67, 0.74, [0.86, 0.92, 0.96])],
        ),
        _ => (
            vec3(0.09, 0.07, 0.07),
            &[
                (0.34, 0.38, [0.98, 0.78, 0.32]),
                (0.38, 0.62, [0.70, 0.04, 0.10]),
                (0.62, 0.66, [0.98, 0.78, 0.32]),
            ],
        ),
    }
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash(a: i32, b: i32) -> f32 {
    let mut h = (a as u32).wrapping_mul(0x9E37_79B1) ^ (b as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Signed distance to a five-pointed star (point up), `r` its outer radius,
/// `rf` the inner corners' share of it (Inigo Quilez's formula).
fn star(p: Vec2, r: f32, rf: f32) -> f32 {
    let k1 = vec2(0.809_017, -0.587_785);
    let k2 = vec2(-k1.x, k1.y);
    let mut p = vec2(p.x.abs(), p.y);
    p -= 2.0 * k1.dot(p).max(0.0) * k1;
    p -= 2.0 * k2.dot(p).max(0.0) * k2;
    p.x = p.x.abs();
    p.y -= r;
    let ba = rf * vec2(-k1.y, k1.x) - vec2(0.0, 1.0);
    let h = (p.dot(ba) / ba.dot(ba)).clamp(0.0, r);
    (p - ba * h).length() * (p.y * ba.x - p.x * ba.y).signum()
}

/// Where the disc's design changes, in disc radii.
const RIM_IN: f32 = 0.82;
const BEZEL: f32 = 0.52;

/// What a point of the disc is.
#[derive(Clone, Copy, PartialEq)]
enum Part {
    Metal,
    /// The cut stone (diamond, ruby).
    Stone,
}

/// The disc's height at `p` (disc radii, y down), and what it is there.
/// `fine`: the small detail is drawn (large sizes).
fn height(p: Vec2, gem: bool, fine: bool) -> (f32, Part) {
    let rho = p.length();
    let theta = p.y.atan2(p.x);
    if rho >= RIM_IN {
        // The rim: rounded, raised, its outer face milled.
        let u = ((rho - (RIM_IN + 1.0) / 2.0) / ((1.0 - RIM_IN) / 2.0)).clamp(-1.0, 1.0);
        let mut h = 0.07 + 0.09 * (1.0 - u * u).max(0.0).sqrt();
        if fine && u > 0.2 {
            h += 0.008 * (theta * 140.0).cos() * (u - 0.2);
        }
        return (h, Part::Metal);
    }
    // The field: gently domed, guilloché rays, a ring of beads inside the rim.
    let mut h = 0.03 + 0.035 * (1.0 - rho * rho);
    // The rim's inner wall.
    h += 0.04 * smooth(RIM_IN - 0.05, RIM_IN, rho);
    if fine {
        h += 0.0035 * (theta * 56.0).sin() * smooth(0.2, 0.3, rho) * (1.0 - smooth(0.66, 0.7, rho));
        // Beads.
        let ring = 0.74;
        let n = 40.0;
        let a = (theta / std::f32::consts::TAU * n).round() / n * std::f32::consts::TAU;
        let bead = vec2(a.cos(), a.sin()) * ring;
        let d = (p - bead).length() / 0.03;
        if d < 1.0 {
            h += 0.03 * (1.0 - d * d).sqrt();
        }
    } else {
        h += 0.018 * (1.0 - ((rho - 0.74) / 0.025).abs()).max(0.0);
    }
    if gem {
        if rho < BEZEL {
            return (h + 0.06, Part::Stone);
        }
        // The bezel holding the stone: a raised collar.
        let u = ((rho - BEZEL - 0.05) / 0.05).clamp(-1.0, 1.0);
        if rho < BEZEL + 0.1 {
            h += 0.09 * (1.0 - u * u).sqrt();
        }
        return (h, Part::Metal);
    }
    // The star: a faceted pyramid (its ridges run to the points).
    let d = star(vec2(p.x, -p.y), 0.56, 0.46);
    if d < 0.0 {
        h += (-d).min(0.2) * 0.75 + 0.02;
    }
    (h, Part::Metal)
}

/// The studio around the medal, seen in direction `r` (z towards the viewer):
/// (overall light, the soft box's highlight).
fn studio(r: Vec3) -> (f32, f32) {
    let up = -r.y * 0.8 - r.x * 0.35;
    let mut sky = 0.22 + 0.78 * smooth(-0.5, 0.9, up);
    // The dark band of the horizon that makes polished metal read as metal.
    sky *= 1.0 - 0.55 * (-((r.y - 0.25) / 0.22).powi(2)).exp();
    let key = vec3(-0.45, -0.62, 0.64).normalize();
    let spec = r.dot(key).max(0.0).powf(28.0) * 1.8;
    let rim = vec3(0.62, 0.5, 0.6).normalize();
    let back = r.dot(rim).max(0.0).powf(12.0) * 0.35;
    (sky + back, spec)
}

/// A colour in [0, 1] from light that may go past it (soft shoulder).
fn tone(c: Vec3) -> Vec3 {
    vec3(1.0 - (-c.x * 1.15).exp(), 1.0 - (-c.y * 1.15).exp(), 1.0 - (-c.z * 1.15).exp())
}

/// The cut stone's colour at `p` (disc radii): a brilliant seen from
/// above, a table, crown facets and star facets catching the studio.
fn stone(p: Vec2, level: u8) -> Vec3 {
    let rho = p.length() / BEZEL;
    let theta = p.y.atan2(p.x) + std::f32::consts::PI;
    let (tilt, bins, offset, id_base): (f32, f32, f32, i32) = if rho < 0.42 {
        (0.0, 1.0, 0.0, 0)
    } else if rho < 0.7 {
        (0.42, 8.0, 0.0, 10)
    } else {
        (0.78, 16.0, 0.5, 30)
    };
    let sector = ((theta / std::f32::consts::TAU * bins) + offset).floor();
    let a = (sector + 0.5 - offset) / bins * std::f32::consts::TAU - std::f32::consts::PI;
    let mut n = vec3(a.cos() * tilt.sin(), a.sin() * tilt.sin(), tilt.cos()).normalize();
    if tilt == 0.0 {
        // The table, a hair towards the light: it catches the soft box.
        n = vec3(-0.18, -0.24, 1.0).normalize();
    }
    let r = vec3(2.0 * n.z * n.x, 2.0 * n.z * n.y, 2.0 * n.z * n.z - 1.0);
    let (sky, spec) = studio(r);
    let id = id_base + sector as i32;
    // Light that went in and came back out of the stone: some facets dark,
    // some bright.
    let inner = 0.25 + 0.9 * hash(id, level as i32);
    if level == 3 {
        diamond(p / BEZEL)
    } else {
        let deep = vec3(0.62, 0.02, 0.07);
        deep * (0.25 + 1.1 * sky * inner)
            + vec3(1.0, 0.45, 0.5) * 0.25 * inner * inner
            + Vec3::splat(spec * 1.2)
    }
}

/// A round brilliant seen from above at `p` (stone radii): the octagonal
/// table, eight star facets, eight kites and sixteen upper girdle facets,
/// each either lit (near white, a few with fire) or dark (the black of the
/// studio seen through the stone), arranged so the light comes from the
/// top left. That contrast, not a pale tint, is what reads as a diamond at
/// every size.
fn diamond(p: Vec2) -> Vec3 {
    use std::f32::consts::{PI, TAU};
    let rho = p.length();
    // Angle from the top, clockwise; eight table vertices at k·45°.
    let theta = (p.x.atan2(-p.y) + TAU) % TAU;
    let s = theta / TAU * 8.0;
    let f = s.fract();
    let tri = (f - 0.5).abs() * 2.0; // 1 at a table vertex, 0 between
    // The octagon's own radius for the table.
    let oct = rho * ((f - 0.5) * PI / 4.0).cos() / (PI / 8.0).cos();
    let star_tip = 0.5 + 0.3 * (1.0 - tri);
    let kite_foot = 1.0 - 0.2 * (1.0 - tri);
    // The facet's kind (tilt from facing the viewer) and its middle angle.
    let vertex = s.round();
    let (kind, tilt, mid) = if oct < 0.5 {
        (0, 0.0, 0.0)
    } else if rho < star_tip {
        (1, 0.42, (s.floor() + 0.5) / 8.0 * TAU)
    } else if rho < kite_foot {
        (2, 0.62, vertex / 8.0 * TAU)
    } else {
        let half = (s * 2.0).floor();
        (3, 0.85, (half + 0.5) / 16.0 * TAU)
    };
    // Lit or dark: the light comes from the top left (an angle of 315°),
    // the facets facing it or facing straight away light up (the light
    // goes in through one side and out through the other), those across
    // stay dark; a hash breaks the symmetry like a real stone's.
    let id = kind * 100
        + match kind {
            1 => s.floor() as i32,
            2 => vertex as i32 % 8,
            3 => (s * 2.0).floor() as i32,
            _ => 0,
        };
    let toward = (mid - 1.75 * PI).cos(); // 1 facing the light
    // (Light past 1: `tone` gives it a soft shoulder, so white needs ~2.)
    let mut v = if kind == 0 {
        // The table: a sheet of light at the top left fading to silver.
        let k = (-p.x * 0.6 - p.y * 0.8) / 0.5;
        // Flat glass: straight-edged reflections, not a gradient (that
        // would read as a dome).
        let v = if k > 0.15 {
            2.4
        } else if k > -0.35 {
            1.25
        } else {
            0.75
        };
        // Its octagon outlined by a darker line.
        v * (1.0 - 0.45 * smooth(0.44, 0.5, oct))
    } else {
        // Neighbours alternate, as the cut's arrows do; the side towards
        // the light is a little brighter; a few break the pattern.
        let parity = (id + kind) % 2 == 0;
        let dark = !parity && hash(id, 3) < 0.6 + 0.25 * toward.min(0.0);
        if !dark {
            1.5 + 0.5 * toward + 0.4 * hash(id, 5)
        } else {
            0.06 + 0.16 * hash(id, 7) + 0.1 * toward.max(0.0)
        }
    };
    // The facets' edges catch a thin line of light.
    let edge = [
        (oct - 0.5).abs(),
        (rho - star_tip).abs() * if kind == 0 { 9.0 } else { 1.0 },
        (rho - kite_foot).abs(),
        (f - 0.5).abs().min(f).min(1.0 - f) * rho * 0.8,
    ]
    .into_iter()
    .fold(f32::MAX, f32::min);
    if kind > 0 {
        v += 0.35 * (1.0 - smooth(0.0, 0.035, edge));
    }
    // The girdle: a dark line where the stone meets the setting.
    v *= 1.0 - 0.7 * smooth(0.93, 1.0, rho);
    let _ = tilt;
    let white = vec3(0.93, 0.97, 1.0);
    let mut c = white * v;
    // Fire: a few lit facets throw a spectral colour.
    if kind > 1 && v > 1.0 && hash(id, 11) > 0.72 {
        let fire = [vec3(1.0, 0.55, 0.12), vec3(0.25, 0.55, 1.0), vec3(0.35, 1.0, 0.55)]
            [(hash(id, 13) * 2.99) as usize];
        c = c.lerp(fire * v * 0.75, 0.75);
    }
    // The dark facets are a deep steel blue rather than flat black.
    if v < 0.5 {
        c += vec3(0.02, 0.05, 0.12);
    }
    c
}

/// A star-shaped glint (a long cross and a short one), centred on `d` = 0,
/// `len` its arms' length; the light it adds.
fn glint(d: Vec2, len: f32) -> f32 {
    let arm = |a: f32, b: f32| {
        (1.0 - (a.abs() / len)).max(0.0).powi(2) * (-(b / (len * 0.07)).powi(2)).exp()
    };
    let diag = vec2(d.x + d.y, d.x - d.y) * std::f32::consts::FRAC_1_SQRT_2;
    let core = (-(d.length() / (len * 0.16)).powi(2)).exp();
    (arm(d.x, d.y) + arm(d.y, d.x))
        .max(0.6 * (arm(diag.x * 1.8, diag.y) + arm(diag.y * 1.8, diag.x)))
        + core
}

/// One sample of the picture at `q` (0 to 1, y down): premultiplied colour
/// and coverage. `px`: a pixel in picture units.
fn sample(q: Vec2, level: u8, px: f32, fine: bool) -> Vec4 {
    let centre = vec2(0.5, 0.635);
    let radius = 0.335;
    let mut out = Vec4::ZERO;
    let mut over = |c: Vec3, a: f32| {
        if a <= 0.0 {
            return;
        }
        let a = a.min(1.0);
        out = vec4(c.x, c.y, c.z, 1.0) * a + out * (1.0 - a);
    };

    // The disc's soft shadow.
    let d = (q - centre - vec2(0.012, 0.03)).length() / radius;
    over(Vec3::ZERO, 0.45 * (1.0 - smooth(0.88, 1.13, d)));

    // The ribbon: two straps crossing in a V behind the ring.
    let (base, bands) = ribbon(level);
    let (top, bottom) = (0.0, 0.43);
    let width = 0.27;
    for side in [-1.0f32, 1.0] {
        if q.y > bottom + px {
            continue;
        }
        // The strap's left edge at this height; u across it (0 to 1).
        let t = ((q.y - top) / (bottom - top)).clamp(0.0, 1.0);
        let x_mid = 0.5 + side * (0.25 - 0.27 * t);
        let u = (q.x - (x_mid - width / 2.0)) / width;
        let inside_x = smooth(-px, px, (0.5 - (u - 0.5).abs()) * width);
        let inside_y = smooth(-px, px, bottom - q.y);
        let cover = inside_x * inside_y;
        if cover <= 0.0 {
            continue;
        }
        let uu = if side < 0.0 { u } else { 1.0 - u };
        let mut c = base;
        for (a, b, col) in bands {
            let k = smooth(*a - 0.006, *a + 0.006, uu) * (1.0 - smooth(*b - 0.006, *b + 0.006, uu));
            c = c.lerp(Vec3::from(*col), k);
        }
        // A rounded strap with a satin sheen, lit from the top left, darker
        // where it goes behind the disc.
        let mut light = 0.72 + 0.28 * (std::f32::consts::PI * u).sin();
        light += 0.16 * (-((u - if side < 0.0 { 0.32 } else { 0.5 }) / 0.16).powi(2)).exp();
        light *= 1.0 - 0.4 * smooth(0.12, 0.43, q.y);
        if side > 0.0 {
            light *= 0.93;
        }
        // The weave's ribs (where they are big enough to be seen).
        let period = 0.016;
        if fine && period > px * 2.2 {
            light *= 0.93 + 0.07 * (q.y / period * std::f32::consts::TAU).sin();
        }
        // The hems.
        light *= 1.0 - 0.3 * (1.0 - smooth(0.0, 0.05, u.min(1.0 - u)));
        // The left strap is in the right one's shadow where they cross.
        if side < 0.0 {
            let mid_r = 0.5 + 0.25 - 0.27 * t;
            let ur = (q.x + 0.012 - (mid_r - width / 2.0)) / width;
            if (0.0..=1.0).contains(&ur) {
                light *= 0.62;
            }
        }
        over(c * light, cover);
    }

    // The ring that holds the disc.
    let m = metal(level);
    let ring_c = vec2(0.5, centre.y - radius - 0.012);
    let (rr, tube) = (0.042, 0.015);
    let off = (q - ring_c).length() - rr;
    if off.abs() < tube + px {
        let k = (off / tube).clamp(-1.0, 1.0);
        let dir = (q - ring_c).normalize_or_zero();
        let n = vec3(dir.x * k, dir.y * k, (1.0 - k * k).max(0.0).sqrt()).normalize();
        let r = vec3(2.0 * n.z * n.x, 2.0 * n.z * n.y, 2.0 * n.z * n.z - 1.0);
        let (sky, spec) = studio(r);
        let c = tone(m * (0.1 + sky) + Vec3::splat(spec));
        over(c, smooth(-px, px, tube - off.abs()));
    }

    // The disc.
    let p = (q - centre) / radius;
    let rho = p.length();
    let edge = smooth(-px / radius, px / radius, 1.0 - rho);
    if edge > 0.0 {
        let gem = level >= 3;
        let (h, part) = height(p, gem, fine);
        let c = if part == Part::Stone {
            tone(stone(p, level))
        } else {
            let e = (px / radius) * 0.75;
            let hx = (height(p + vec2(e, 0.0), gem, fine).0
                - height(p - vec2(e, 0.0), gem, fine).0)
                / (2.0 * e);
            let hy = (height(p + vec2(0.0, e), gem, fine).0
                - height(p - vec2(0.0, e), gem, fine).0)
                / (2.0 * e);
            let n = vec3(-hx, -hy, 1.0).normalize();
            let r = vec3(2.0 * n.z * n.x, 2.0 * n.z * n.y, 2.0 * n.z * n.z - 1.0);
            let (sky, spec) = studio(r);
            // Recesses (the field by the rim, around the star) get less light.
            let mut ao = 1.0 - 0.3 * (-((rho - RIM_IN + 0.02) / 0.05).powi(2)).exp();
            if !gem {
                let d = star(vec2(p.x, -p.y), 0.56, 0.46);
                if d > 0.0 {
                    ao *= 1.0 - 0.28 * (-(d / 0.05).powi(2)).exp();
                }
            } else if rho > BEZEL && rho < BEZEL + 0.03 {
                ao *= 0.6;
            }
            // A slightly worn, brushed surface.
            let grain = if fine {
                0.97 + 0.06 * hash((p.y.atan2(p.x) * 400.0) as i32, (rho * 30.0) as i32)
            } else {
                1.0
            };
            let _ = h;
            tone((m * (0.13 + sky * 1.05) * grain + m.lerp(Vec3::ONE, 0.45) * spec) * ao)
        };
        over(c, edge);
    }
    // The diamond's glint, at the top left of the stone: never thinner
    // than about a pixel, so it sparkles at small sizes too.
    if level == 3 {
        let at = centre + vec2(-0.3, -0.34) * BEZEL * radius;
        let len = (0.12f32).max(px * 4.0);
        let g = glint((q - at) / len * 0.12, 0.12);
        if g > 0.002 {
            let a = (g * 0.95).min(1.0);
            out = vec4(1.0, 1.0, 1.0, 1.0) * a + out * (1.0 - a);
        }
    }
    out
}

/// The medal of `level`, `locked` or not, as `size`×`size` RGBA pixels
/// (straight alpha).
pub fn render(level: u8, locked: bool, size: u32) -> Vec<u8> {
    let ss = if size <= 64 {
        3
    } else if size <= 160 {
        2
    } else {
        1
    };
    let fine = size >= 90;
    let px = 1.0 / size as f32;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let mut acc = Vec4::ZERO;
            for sy in 0..ss {
                for sx in 0..ss {
                    let q = vec2(
                        (x as f32 + (sx as f32 + 0.5) / ss as f32) * px,
                        (y as f32 + (sy as f32 + 0.5) / ss as f32) * px,
                    );
                    acc += sample(q, level, px, fine);
                }
            }
            acc /= (ss * ss) as f32;
            let a = acc.w;
            let mut c = if a > 0.0 { acc.truncate() / a } else { Vec3::ZERO };
            let mut alpha = a;
            if locked {
                let l = c.dot(vec3(0.3, 0.59, 0.11));
                c = Vec3::splat(l * 0.42 + 0.07);
                alpha *= 0.72;
            }
            let to = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            out.extend_from_slice(&[to(c.x), to(c.y), to(c.z), to(alpha)]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medals_are_opaque_in_the_middle_and_clear_in_the_corners() {
        for level in 0..5 {
            let px = render(level, false, 32);
            let at = |x: usize, y: usize| px[(y * 32 + x) * 4 + 3];
            assert_eq!(at(0, 31), 0);
            assert_eq!(at(31, 31), 0);
            assert!(at(16, 20) > 250, "level {level}");
        }
    }

    /// Writes every medal as PNG files to look at:
    /// `cargo test -p snook-legend export_medals -- --ignored`
    /// (into `$SNOOK_LEGEND_MEDALS_OUT`, by default the system's temporary folder).
    #[test]
    #[ignore]
    fn export_medals() {
        let dir = std::env::var("SNOOK_LEGEND_MEDALS_OUT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir());
        for size in [32u32, 64, 128, 288] {
            // All five side by side, then locked.
            let w = size * 5;
            let mut sheet = vec![0u8; (w * size * 2 * 4) as usize];
            for level in 0..5u8 {
                for (row, locked) in [false, true].into_iter().enumerate() {
                    let px = render(level, locked, size);
                    for y in 0..size {
                        for x in 0..size {
                            let s = ((y * size + x) * 4) as usize;
                            let d = (((y + row as u32 * size) * w + level as u32 * size + x) * 4)
                                as usize;
                            // Over a panel's brown, as in the game.
                            let a = px[s + 3] as f32 / 255.0;
                            let bg = [24.0, 15.0, 10.0];
                            for k in 0..3 {
                                sheet[d + k] = (px[s + k] as f32 * a + bg[k] * (1.0 - a)) as u8;
                            }
                            sheet[d + 3] = 255;
                        }
                    }
                }
            }
            let file = std::fs::File::create(dir.join(format!("medals-{size}.png"))).unwrap();
            let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, size * 2);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&sheet).unwrap();
        }
    }
}
