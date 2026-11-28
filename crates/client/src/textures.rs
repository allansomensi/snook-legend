//! Procedurally generated textures (CPU side, RGBA8).
//!
//! The game ships without image files. Patterns are kept smooth (low
//! frequency) and the GPU textures are mipmapped, which avoids the shimmering
//! "grain" that fine noise produces when the camera moves.

use macroquad::math::Vec3;
use snook_legend_core::{BallId, BallKind};

/// RGBA8 image data.
pub struct Pixels {
    pub width: u16,
    pub height: u16,
    pub rgba: Vec<u8>,
}

/// Maps texture coordinates to a direction on the unit sphere (y-up).
pub fn uv_to_dir(u: f32, v: f32) -> Vec3 {
    let phi = u * std::f32::consts::TAU;
    let theta = v * std::f32::consts::PI;
    Vec3::new(theta.sin() * phi.cos(), theta.cos(), theta.sin() * phi.sin())
}

/// The colours of a ball set (see `snook_legend_proto::shop`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub colors: [[u8; 3]; 7],
    pub eight: [u8; 3],
    pub ivory: [u8; 3],
    /// Marble veins and galaxy stars are painted into the balls.
    pub finish: snook_legend_proto::shop::BallFinish,
    /// How the balls are drawn: dots, bands, a chequer, no circles…
    pub design: snook_legend_proto::shop::BallDesign,
}

impl Palette {
    /// The palette of a shop ball set (the classic one if unknown).
    pub fn of(code: &str) -> Palette {
        let item = snook_legend_proto::shop::item(code)
            .or_else(|| snook_legend_proto::shop::item("balls_classic"));
        match item.map(|i| i.look) {
            Some(snook_legend_proto::shop::Look::Balls {
                colors,
                eight,
                ivory,
                finish,
                design,
            }) => Palette { colors, eight, ivory, finish, design },
            _ => Palette {
                colors: [[255, 204, 10]; 7],
                eight: [22, 22, 24],
                ivory: IVORY,
                finish: snook_legend_proto::shop::BallFinish::Gloss,
                design: snook_legend_proto::shop::BallDesign::Standard,
            },
        }
    }
}

/// Ball colour in sRGB (classic set).
pub fn ball_color(id: BallId) -> [u8; 3] {
    ball_color_in(id, &Palette::of("balls_classic"))
}

pub fn ball_color_in(id: BallId, p: &Palette) -> [u8; 3] {
    match id.0 {
        // (A set on a dark ground keeps a white cue ball.)
        0 if !light(p.ivory) => IVORY,
        0 => p.ivory,
        8 => p.eight,
        n => p.colors[((n - 1) % 8).min(6) as usize],
    }
}

const IVORY: [u8; 3] = [246, 242, 228];
const INK: [u8; 3] = [20, 20, 20];

/// A light colour (dark digits read on it).
fn light(c: [u8; 3]) -> bool {
    0.3 * c[0] as f32 + 0.59 * c[1] as f32 + 0.11 * c[2] as f32 > 128.0
}

/// A ball's number, rasterised from the game's bold font with real
/// anti-aliasing (a 5x7 bitmap font looked blocky up close): coverage 0..1
/// on a canvas centred on the number circle.
struct Label {
    w: usize,
    h: usize,
    cov: Vec<f32>,
    /// Size of a canvas pixel in spot-radius units.
    scale: f32,
}

impl Label {
    fn new(number: u8) -> Label {
        const PX: f32 = 192.0;
        let font = fontdue::Font::from_bytes(
            &include_bytes!("../assets/DejaVuSans-Bold.ttf")[..],
            fontdue::FontSettings::default(),
        )
        .expect("bundled font");
        let text = number.to_string();
        // Lay the digits out on a baseline, a little tighter than the font.
        let mut glyphs = Vec::new();
        let mut pen = 0.0f32;
        for c in text.chars() {
            let (m, bitmap) = font.rasterize(c, PX);
            glyphs.push((pen.round() as i32 + m.xmin, m, bitmap));
            pen += m.advance_width * 0.94;
        }
        let x0 = glyphs.iter().map(|(x, _, _)| *x).min().unwrap_or(0);
        let x1 = glyphs.iter().map(|(x, m, _)| x + m.width as i32).max().unwrap_or(1);
        let top = glyphs.iter().map(|(_, m, _)| m.ymin + m.height as i32).max().unwrap_or(1);
        let bottom = glyphs.iter().map(|(_, m, _)| m.ymin).min().unwrap_or(0);
        let (gw, gh) = ((x1 - x0) as usize, (top - bottom) as usize);
        // 6 and 9 are underlined, as on real balls, so they can't be confused.
        let underline = matches!(number, 6 | 9);
        let (bar_gap, bar_h) = ((gh as f32 * 0.07) as usize, (gh as f32 * 0.10).max(2.0) as usize);
        let extra = if underline { bar_gap + bar_h } else { 0 };
        let pad = 4;
        let (w, h) = (gw + pad * 2, gh + extra + pad * 2);
        let mut cov = vec![0.0f32; w * h];
        for (x, m, bitmap) in &glyphs {
            let ox = (x - x0) as usize + pad;
            let oy = (top - (m.ymin + m.height as i32)) as usize + pad;
            for r in 0..m.height {
                for c in 0..m.width {
                    let v = bitmap[r * m.width + c] as f32 / 255.0;
                    let cell = &mut cov[(oy + r) * w + ox + c];
                    *cell = cell.max(v);
                }
            }
        }
        if underline {
            let bw = (gw as f32 * 0.8) as usize;
            let bx = pad + (gw - bw) / 2;
            for r in 0..bar_h {
                for c in 0..bw {
                    cov[(pad + gh + bar_gap + r) * w + bx + c] = 1.0;
                }
            }
        }
        // The digits fill about 60% of the circle's height (narrower for two).
        let height = (gh + extra) as f32;
        let scale = (1.16 / height).min(1.52 / gw as f32);
        Label { w, h, cov, scale }
    }

    /// Ink coverage at (`a`, `b`) (right and up, in spot-radius units), bilinear.
    fn sample(&self, a: f32, b: f32) -> f32 {
        let x = a / self.scale + self.w as f32 / 2.0 - 0.5;
        let y = -b / self.scale + self.h as f32 / 2.0 - 0.5;
        let (xf, yf) = (x.floor(), y.floor());
        let (fx, fy) = (x - xf, y - yf);
        let at = |cx: f32, cy: f32| -> f32 {
            if cx < 0.0 || cy < 0.0 || cx >= self.w as f32 || cy >= self.h as f32 {
                0.0
            } else {
                self.cov[cy as usize * self.w + cx as usize]
            }
        };
        let top = at(xf, yf) + (at(xf + 1.0, yf) - at(xf, yf)) * fx;
        let bottom = at(xf, yf + 1.0) + (at(xf + 1.0, yf + 1.0) - at(xf, yf + 1.0)) * fx;
        top + (bottom - top) * fy
    }
}

/// How much of a pixel `px` wide lies on the inside of an edge `d` away
/// (positive inside): a smooth, alias-free boundary.
fn coverage(d: f32, px: f32) -> f32 {
    (d / px.max(1e-6) + 0.5).clamp(0.0, 1.0)
}

fn mix(a: [f32; 3], b: [u8; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] as f32 - a[i]) * t)
}

/// Colour of a ball's surface in direction `p` (mesh-local, unit length),
/// for a texel covering `px` radians. Numbers sit on local ±Z; stripes run
/// around the band |x| < 0.5.
fn ball_surface_f(id: BallId, p: Vec3, pal: &Palette, label: Option<&Label>, px: f32) -> [f32; 3] {
    use snook_legend_proto::shop::BallDesign;
    let color = ball_color_in(id, pal);
    let ground = pal.ivory;
    let f = |c: [u8; 3]| c.map(|v| v as f32);
    if id == BallId::CUE {
        // Six small red dots (they show the spin).
        let dots = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z];
        let near = dots.iter().map(|d| p.dot(*d).clamp(-1.0, 1.0).acos()).fold(9.0, f32::min);
        return mix(f(color), [190, 30, 30], coverage(0.0895 - near, px));
    }
    // The body: the colour (in the set's design), or the ground outside a
    // stripe's band.
    let band = match id.kind() {
        BallKind::Stripe => coverage(0.5 - p.x.abs(), px * (1.0 - p.x * p.x).sqrt().max(0.2)),
        _ => 1.0,
    };
    let body = design_pattern(pal.design, p, f(color), ground, px);
    let mut c = [0, 1, 2].map(|i| ground[i] as f32 + (body[i] - ground[i] as f32) * band);
    c = finish_pattern(pal.finish, p, c, px);
    // A large number circle with big, bold digits: readable from across the
    // table (on a dark ground, light digits; with no circle, light digits
    // outlined in dark straight on the colour).
    let spot_angle: f32 = 0.56;
    let bold = pal.design == BallDesign::Bold;
    let digits = if light(ground) { INK } else { IVORY };
    for (s, e1) in [(Vec3::Z, Vec3::X), (-Vec3::Z, -Vec3::X)] {
        let angle = p.dot(s).clamp(-1.0, 1.0).acos();
        if angle < spot_angle + px {
            let inside = coverage(spot_angle - angle, px);
            let scale = spot_angle.sin();
            let a = p.dot(e1) / scale;
            let b = p.dot(Vec3::Y) / scale;
            let ink = label.map_or(0.0, |l| l.sample(a, b));
            if bold {
                // The outline: the digits grown a little all round.
                let k = 0.075;
                let grown = label.map_or(0.0, |l| {
                    [
                        (k, 0.0),
                        (-k, 0.0),
                        (0.0, k),
                        (0.0, -k),
                        (0.7 * k, 0.7 * k),
                        (-0.7 * k, 0.7 * k),
                        (0.7 * k, -0.7 * k),
                        (-0.7 * k, -0.7 * k),
                    ]
                    .iter()
                    .map(|(da, db)| l.sample(a + da, b + db))
                    .fold(ink, f32::max)
                });
                let fill = if light(ground) { ground } else { IVORY };
                let edged = mix(c, INK, grown * inside);
                c = mix(edged, fill, ink * inside);
            } else {
                let face = mix(f(ground), digits, ink);
                c = [0, 1, 2].map(|i| c[i] + (face[i] - c[i]) * inside);
            }
        }
    }
    c
}

/// Evenly spread points over the sphere (a Fibonacci lattice).
fn spread(n: usize) -> Vec<Vec3> {
    let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    (0..n)
        .map(|i| {
            let y = 1.0 - 2.0 * (i as f32 + 0.5) / n as f32;
            let r = (1.0 - y * y).sqrt();
            let a = golden * i as f32;
            Vec3::new(r * a.cos(), y, r * a.sin())
        })
        .collect()
}

/// The two nearest of `points` to `p` (angles) and the nearest's index.
fn nearest_two(points: &[Vec3], p: Vec3) -> (f32, f32, usize) {
    let (mut d1, mut d2, mut k) = (9.0f32, 9.0f32, 0);
    for (i, q) in points.iter().enumerate() {
        let d = p.dot(*q).clamp(-1.0, 1.0).acos();
        if d < d1 {
            d2 = d1;
            d1 = d;
            k = i;
        } else if d < d2 {
            d2 = d;
        }
    }
    (d1, d2, k)
}

fn tone(c: [f32; 3], k: f32) -> [f32; 3] {
    c.map(|v| (v * k).clamp(0.0, 255.0))
}

fn toward(c: [f32; 3], to: [f32; 3], k: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| c[i] + (to[i] - c[i]) * k.clamp(0.0, 1.0))
}

/// A ball set's design over the colour of a ball (`c`), at `p` (the stripe
/// band runs round the X axis; see [`ball_surface_f`]).
fn design_pattern(
    design: snook_legend_proto::shop::BallDesign,
    p: Vec3,
    c: [f32; 3],
    ground: [u8; 3],
    px: f32,
) -> [f32; 3] {
    use snook_legend_proto::shop::BallDesign;
    let white = [255.0, 255.0, 255.0];
    // Round the stripes' axis (X): the angle, and how far along it.
    let round = p.z.atan2(p.y);
    match design {
        BallDesign::Standard | BallDesign::Dark | BallDesign::Bold => c,
        BallDesign::Dots => {
            static DOTS: std::sync::OnceLock<Vec<Vec3>> = std::sync::OnceLock::new();
            let dots = DOTS.get_or_init(|| spread(26));
            let (d, _, _) = nearest_two(dots, p);
            let spot = if light(ground) { ground.map(|v| v as f32) } else { toward(c, white, 0.7) };
            toward(c, spot, coverage(0.17 - d, px))
        }
        BallDesign::Planet => {
            // Bands of tone round the ball, wavering, and a thin dark belt.
            let t = p.x * 9.0 + 0.5 * (round * 3.0 + p.x * 4.0).sin();
            let k = 0.86 + 0.2 * (t.sin() * 0.5 + 0.5) + 0.06 * (t * 2.7).sin();
            let belt = coverage(0.05 - (p.x - 0.18).abs(), px * 2.0);
            toward(tone(c, k), tone(c, 0.55), belt)
        }
        BallDesign::Waves => {
            // Crests of light rolling round the ball, deeper in the troughs.
            let w = (p.x * 7.0 + 0.45 * (round * 4.0).sin()) * std::f32::consts::PI;
            let s = w.sin();
            let crest = coverage(s - 0.82, px * 20.0);
            toward(tone(c, 0.82 + 0.18 * (s * 0.5 + 0.5)), white, crest * 0.5)
        }
        BallDesign::Halves => {
            // The upper half in the light, the lower one deeper.
            toward(tone(c, 0.6), c, coverage(p.y, px * 3.0))
        }
        BallDesign::Checker => {
            let i = ((round / std::f32::consts::PI + 1.0) * 6.0).floor() as i32;
            let j = ((p.x + 1.0) * 4.0).floor() as i32;
            if (i + j).rem_euclid(2) == 0 { c } else { tone(c, 0.5) }
        }
        BallDesign::Camo => {
            let n = (p.x * 4.3 + p.y * 2.1).sin()
                + (p.y * 5.1 - p.z * 3.3).sin() * 0.8
                + (p.z * 6.7 + p.x * 2.9).sin() * 0.6;
            let m = (p.x * 7.9 - p.z * 4.1).sin() + (p.y * 8.3 + p.x * 3.7).sin() * 0.7;
            if n > 0.9 {
                tone(c, 0.45)
            } else if m > 0.8 {
                tone(c, 1.18)
            } else if n < -0.7 {
                tone(c, 0.72)
            } else {
                c
            }
        }
        BallDesign::Swirl => {
            // A lighter ribbon winding round, twice.
            let s = (round / std::f32::consts::TAU * 2.0 + p.x * 1.7).rem_euclid(1.0);
            let edge = (s - 0.14).abs() - 0.14;
            toward(c, white, 0.5 * coverage(-edge, px * 3.0))
        }
        BallDesign::Facets => {
            // Facets of a cut stone: each its own light, bright edges.
            static FACETS: std::sync::OnceLock<Vec<Vec3>> = std::sync::OnceLock::new();
            let facets = FACETS.get_or_init(|| spread(44));
            let (d1, d2, k) = nearest_two(facets, p);
            let lit = tone(c, 0.72 + 0.5 * hash(k as i32, 3, 91));
            toward(lit, white, 0.45 * coverage(0.025 - (d2 - d1), px * 2.0))
        }
        BallDesign::Football => {
            // Dark patches round the twelve corners of an icosahedron.
            static CORNERS: std::sync::OnceLock<Vec<Vec3>> = std::sync::OnceLock::new();
            let corners = CORNERS.get_or_init(|| {
                let g = (1.0 + 5f32.sqrt()) / 2.0;
                let mut v = Vec::new();
                for (a, b) in [(1.0, g), (-1.0, g), (1.0, -g), (-1.0, -g)] {
                    v.push(Vec3::new(0.0, a, b).normalize());
                    v.push(Vec3::new(a, b, 0.0).normalize());
                    v.push(Vec3::new(b, 0.0, a).normalize());
                }
                v
            });
            let (d, _, _) = nearest_two(corners, p);
            toward(c, tone(c, 0.22), coverage(0.3 - d, px))
        }
    }
}

/// What a finish paints into a ball's body (`p`: the direction on the ball;
/// `c`: its colour there): veins through marble, stars in a galaxy. Kept
/// gentle, so that every ball keeps its own colour at a glance.
fn finish_pattern(
    finish: snook_legend_proto::shop::BallFinish,
    p: Vec3,
    c: [f32; 3],
    px: f32,
) -> [f32; 3] {
    use snook_legend_proto::shop::BallFinish;
    match finish {
        BallFinish::Marble => {
            // Veins winding through the stone: one bold, finer ones round it.
            let warp = (p.y * 3.1 + p.z * 2.3).sin() * 0.9 + (p.x * 4.7 - p.z * 1.9).sin() * 0.5;
            let vein = (p.x * 5.0 + p.y * 3.0 + warp * 1.6).sin();
            let fine = (p.z * 11.0 - p.y * 7.0 + warp * 3.0).sin();
            let bold = coverage(0.07 - vein.abs(), px * 6.0);
            let thin = coverage(0.03 - fine.abs(), px * 6.0) * 0.5;
            let cloud = 1.0 + 0.06 * ((p.x * 2.0 + p.y).sin() * (p.z * 3.0 - p.x).cos());
            let base = c.map(|v| (v * cloud).min(255.0));
            let light = base.map(|v| v + (255.0 - v) * 0.55);
            let k = (bold + thin).min(1.0);
            [0, 1, 2].map(|i| base[i] + (light[i] - base[i]) * k)
        }
        BallFinish::Galaxy => {
            // Starlight: tiny specks, a few bright ones, and a faint swirl of
            // lighter dust through the colour.
            let q = p * 34.0;
            let cell = (q.x.floor() as i32, q.y.floor() as i32, q.z.floor() as i32);
            let h = hash(cell.0, cell.1 * 7919 + cell.2, 77);
            let f = q - Vec3::new(q.x.floor(), q.y.floor(), q.z.floor()) - Vec3::splat(0.5);
            let d = f.length();
            let star =
                if h > 0.86 { coverage(0.16 - d, 0.12) * (0.5 + (h - 0.86) / 0.28) } else { 0.0 };
            let dust = ((p.x * 3.0 + p.y * 5.0).sin() * (p.z * 4.0 - p.y * 2.0).cos()).max(0.0);
            // A cool nebula drifting through the colour.
            let nebula = [150.0, 160.0, 255.0];
            let base: [f32; 3] =
                [0, 1, 2].map(|i| c[i] * (0.82 + 0.16 * dust) + (nebula[i] - c[i]) * 0.14 * dust);
            [0, 1, 2].map(|i| base[i] + (255.0 - base[i]) * star.min(1.0))
        }
        BallFinish::Glow => {
            // Brighter and more saturated, as if lit from inside.
            let mean = (c[0] + c[1] + c[2]) / 3.0;
            c.map(|v| (mean + (v - mean) * 1.15).clamp(0.0, 255.0))
        }
        _ => c,
    }
}

/// [`ball_surface_f`] at the centre of a texel of a 1024-wide texture.
#[cfg(test)]
fn ball_surface(id: BallId, p: Vec3, pal: &Palette) -> [u8; 3] {
    let label = (id.0 > 0).then(|| Label::new(id.0));
    ball_surface_f(id, p, pal, label.as_ref(), std::f32::consts::TAU / 1024.0)
        .map(|v| v.round().clamp(0.0, 255.0) as u8)
}

/// Equirectangular texture for a numbered ball, `width` pixels wide.
pub fn ball(id: BallId, width: u16, palette: &Palette) -> Pixels {
    let w_px = width;
    let h_px = width / 2;
    let label = (id.0 > 0).then(|| Label::new(id.0));
    // Edges are anti-aliased analytically; two samples a side smooth the
    // digits further and the poles, where texels are narrow.
    const SS: u32 = 2;
    let px = std::f32::consts::TAU / w_px as f32;
    let mut rgba = Vec::with_capacity(w_px as usize * h_px as usize * 4);
    for y in 0..h_px as u32 {
        for x in 0..w_px as u32 {
            let mut acc = [0f32; 3];
            for sy in 0..SS {
                for sx in 0..SS {
                    let u = (x as f32 + (sx as f32 + 0.5) / SS as f32) / w_px as f32;
                    let v = (y as f32 + (sy as f32 + 0.5) / SS as f32) / h_px as f32;
                    let c = ball_surface_f(id, uv_to_dir(u, v), palette, label.as_ref(), px);
                    for k in 0..3 {
                        acc[k] += c[k];
                    }
                }
            }
            let n = (SS * SS) as f32;
            let px8 = acc.map(|v| (v / n).round().clamp(0.0, 255.0) as u8);
            rgba.extend_from_slice(&[px8[0], px8[1], px8[2], 255]);
        }
    }
    Pixels { width: w_px, height: h_px, rgba }
}

pub(crate) fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374_761_393)
        ^ (y as u32).wrapping_mul(668_265_263)
        ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    (h ^ (h >> 16)) as f32 / u32::MAX as f32
}

/// Smooth value noise, tileable with period `period` cells.
pub(crate) fn noise(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let w = |v: i32| v.rem_euclid(period);
    let h = |xx: i32, yy: i32| hash(w(xx), w(yy), seed);
    let top = h(x0, y0) + (h(x0 + 1, y0) - h(x0, y0)) * s(fx);
    let bottom = h(x0, y0 + 1) + (h(x0 + 1, y0 + 1) - h(x0, y0 + 1)) * s(fx);
    top + (bottom - top) * s(fy)
}

pub(crate) fn fbm(x: f32, y: f32, period: i32, seed: u32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 0.5, 1.0, 0.0);
    for o in 0..octaves {
        sum += amp * noise(x * freq, y * freq, period * freq as i32, seed + o);
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

pub(crate) fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| ((a[i] + (b[i] - a[i]) * t) * 255.0).round().clamp(0.0, 255.0) as u8)
}

/// How much sharper the textures of the halls and tables are made (Ultra:
/// twice as many pixels each way).
static DETAIL: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(1);

/// Sets the texture detail for the quality preset (see [`detailed`]).
pub fn set_detail(ultra: bool) {
    DETAIL.store(if ultra { 2 } else { 1 }, std::sync::atomic::Ordering::Relaxed);
}

/// The size a texture of `size` pixels is made at: textures are drawn from
/// code in 0..1 coordinates, so any size gives the same picture, sharper.
pub(crate) fn detailed(size: u16) -> u16 {
    if size > 512 {
        return size;
    }
    size * DETAIL.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) fn image(size: u16, f: impl Fn(f32, f32) -> [u8; 3]) -> Pixels {
    let size = detailed(size);
    let mut rgba = Vec::with_capacity(size as usize * size as usize * 4);
    for y in 0..size {
        for x in 0..size {
            let c = f(x as f32 / size as f32, y as f32 / size as f32);
            rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    Pixels { width: size, height: size, rgba }
}

/// Cloth at a given size; the larger one (Ultra) has a finer, woven nap.
pub fn felt_sized(base: [u8; 3], size: u16) -> Pixels {
    // The classic green [24, 96, 48] maps to the original tones.
    let b = base.map(|c| c as f32 / 255.0);
    let dark = [b[0] * 0.90, b[1] * 0.95, b[2] * 0.90];
    let light = [b[0] * 1.06, b[1] * 1.06, b[2] * 1.02];
    let fine = size >= 1024;
    image(size, move |u, v| {
        let n = fbm(u * 4.0, v * 4.0, 4, 3, 3);
        let nap = fbm(u * 96.0, v * 96.0, 96, 7, 2) - 0.5;
        let c = lerp3(dark, light, n);
        let mut k = 1.0 + nap * 0.07;
        if fine {
            // The weave: a very faint cross-hatch that only shows up close.
            let weave = ((u * 640.0 * std::f32::consts::TAU).sin()
                * (v * 640.0 * std::f32::consts::TAU).sin())
                * 0.012;
            k += weave + (noise(u * 256.0, v * 256.0, 256, 19) - 0.5) * 0.03;
        }
        c.map(|x| (x as f32 * k).round().clamp(0.0, 255.0) as u8)
    })
}

/// Fine, realistic wood: long thin grain lines running along `u`, gentle
/// figure, small dark pores, low contrast. Tileable.
pub fn fine_wood(light: [f32; 3], dark: [f32; 3], seed: u32) -> Pixels {
    const N: u16 = 512;
    let mut rgba = Vec::with_capacity(N as usize * N as usize * 4);
    for y in 0..N {
        for x in 0..N {
            let (u, v) = (x as f32 / N as f32, y as f32 / N as f32);
            // Slow, wide figure that bends the grain.
            let warp = fbm(u * 4.0, v * 4.0, 4, seed, 3);
            // Growth rings seen from the side: thin lines, many of them.
            let ring = ((v * 22.0 + warp * 1.6) * std::f32::consts::TAU).sin() * 0.5 + 0.5;
            let ring = ring.powf(6.0);
            // Very fine streaks that run the whole length.
            let streak = noise(0.0, v * 160.0 + warp * 6.0, 160, seed + 5) * 0.7
                + noise(0.0, v * 48.0, 48, seed + 11) * 0.3;
            // Broad, soft colour variation.
            let tone = fbm(u * 2.0, v * 2.0, 2, seed + 21, 2);
            let mut t = 0.30 + 0.28 * ring + 0.30 * (streak - 0.5) + 0.25 * (tone - 0.5);
            // Pores: tiny dark specks stretched along the grain.
            if hash(x as i32 / 3, y as i32, seed + 31) > 0.985 {
                t += 0.25;
            }
            let c = lerp3(light, dark, t);
            rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    Pixels { width: N, height: N, rgba }
}

/// Plank floor: staggered boards of slightly different tones, fine grain,
/// dark seams and end joints. Tileable; `size` 256 or 512.
pub fn floor_sized(size: u16) -> Pixels {
    image(size, |u, v| {
        let planks = 6.0;
        let plank = (v * planks).floor();
        let within = v * planks - plank;
        // Each board starts somewhere else along its length.
        let offset = hash(plank as i32, 3, 41);
        let along = (u + offset).fract() * 2.0;
        let board = along.floor();
        let x = along - board;
        let id = (plank as i32) * 7 + board as i32;
        let tone = (hash(id, 0, 7) - 0.5) * 0.35;
        let grain = fbm(u * 2.0 + plank * 0.37, within * 4.0 + hash(id, 1, 9), 4, 11, 4) - 0.5;
        let streak = noise(u * 64.0, within * 3.0 + plank, 64, 23) - 0.5;
        let mut t = 0.42 + tone + grain * 0.55 + streak * 0.18;
        // Seams between boards and end joints.
        let seam = !(0.018..=0.985).contains(&within) as u8 as f32;
        let joint = !(0.004..=0.996).contains(&x) as u8 as f32;
        t += (seam + joint).min(1.0) * 0.45;
        lerp3([0.42, 0.26, 0.14], [0.11, 0.065, 0.035], t)
    })
}

/// Damask wallpaper: deep green with a slightly lighter, satin pattern of
/// medallions and fine gold stripes. Tileable.
pub fn wallpaper() -> Pixels {
    let base = [0.085, 0.16, 0.12];
    let satin = [0.14, 0.24, 0.17];
    let gold = [0.45, 0.36, 0.17];
    image(512, move |u, v| {
        // Two medallions per tile, offset like a brick pattern.
        let cell = |u: f32, v: f32| -> f32 {
            let (x, y) = (u * 2.0 - 1.0, v * 2.0 - 1.0);
            let r = (x * x * 1.6 + y * y).sqrt();
            let a = y.atan2(x);
            let petals = 0.36 + 0.10 * (a * 4.0).cos() + 0.04 * (a * 8.0).cos();
            let outline = 1.0 - ((r - petals).abs() / 0.025).clamp(0.0, 1.0);
            let heart = 1.0 - (r / 0.12).clamp(0.0, 1.0);
            let leaf = {
                let ly = (y.abs() - 0.62).abs();
                let lx = x.abs();
                (1.0 - (lx * lx * 30.0 + ly * ly * 90.0).sqrt().clamp(0.0, 1.0)) * 0.8
            };
            outline.max(heart * 0.9).max(leaf)
        };
        let m1 = cell(u.fract(), (v * 2.0).fract());
        let m2 = cell((u + 0.5).fract(), (v * 2.0 + 0.5).fract()) * 0.55;
        let pattern = m1.max(m2);
        let mut c = lerp3(base, satin, pattern * 0.85 + fbm(u * 8.0, v * 8.0, 8, 5, 2) * 0.15);
        // Fine stripes at the edges of the tile.
        let stripe = ((u - 0.02).abs() < 0.004) || ((u - 0.98).abs() < 0.004);
        if stripe {
            c = lerp3(gold, gold, 0.0);
        }
        c
    })
}

/// Pressed tin ceiling: raised squares with a round boss, catching the light
/// from one side. Tileable.
pub fn tin_ceiling() -> Pixels {
    image(256, |u, v| {
        let height = |u: f32, v: f32| -> f32 {
            let (x, y) = ((u.fract() - 0.5).abs(), (v.fract() - 0.5).abs());
            let square = 1.0 - ((x.max(y) - 0.38) / 0.04).clamp(0.0, 1.0);
            let r = (x * x + y * y).sqrt();
            let boss = 1.0 - ((r - 0.16) / 0.05).clamp(0.0, 1.0);
            let ring = 1.0 - ((r - 0.27).abs() / 0.015).clamp(0.0, 1.0);
            square * 0.6 + boss * 0.5 + ring * 0.3
        };
        let e = 1.0 / 256.0;
        let dx = height(u + e, v) - height(u - e, v);
        let dy = height(u, v + e) - height(u, v - e);
        let shade = (0.5 + (dx + dy) * 6.0).clamp(0.0, 1.0);
        lerp3([0.20, 0.15, 0.11], [0.38, 0.30, 0.22], shade)
    })
}

/// A rug for under the table (not tiled: the whole rug, 2:1): a dark red
/// border with a gold line, a deep green field and a medallion.
pub fn rug() -> Pixels {
    // Four samples a pixel: the motifs and the fine gold lines get smooth
    // outlines (sampled once, they were stepped, and the lines of the
    // corners broke up into dots seen from above).
    crate::hall_textures::image_wh_smooth(1024, 512, |u, v| {
        // Distance to the edge, in rug units (the rug is 2:1).
        let edge = (u.min(1.0 - u) * 2.0).min(v.min(1.0 - v));
        let field = [0.07, 0.17, 0.12];
        let border = [0.32, 0.06, 0.05];
        let gold = [0.52, 0.40, 0.18];
        let wool = fbm(u * 64.0, v * 32.0, 32, 77, 2) - 0.5;
        let c = if edge < 0.012 {
            // Fringe-ish dark edge.
            [0.18, 0.05, 0.04]
        } else if edge < 0.09 {
            // Border with a running diamond pattern.
            let t = ((u + v) * 60.0).sin() * ((u - v) * 60.0).sin();
            let k = (t > 0.3) as u8 as f32;
            [0, 1, 2].map(|i| border[i] + (gold[i] - border[i]) * k * 0.45)
        } else if (edge - 0.095).abs() < 0.006 || (edge - 0.13).abs() < 0.004 {
            gold
        } else {
            // The field, with a medallion in the middle.
            let (cx, cy) = ((u - 0.5) * 2.0, v - 0.5);
            let r = (cx * cx + cy * cy).sqrt();
            let a = cy.atan2(cx);
            let star = 0.17 + 0.025 * (a * 8.0).cos() + 0.01 * (a * 16.0).cos();
            if r < star {
                let inner = r / star;
                if (inner - 0.55).abs() < 0.05 || inner < 0.18 {
                    gold
                } else if inner < 0.55 {
                    field
                } else {
                    border
                }
            } else if (r - star - 0.02).abs() < 0.006 {
                gold
            } else {
                // A few gold lines round each corner of the field (many
                // fine ones shimmered).
                let corner =
                    ((u - 0.5).abs() * 2.0 - 0.72).max(0.0) + ((v - 0.5).abs() - 0.28).max(0.0);
                if corner > 0.0 && (corner * 22.0).fract() < 0.2 { gold } else { field }
            }
        };
        c.map(|x| ((x * (1.0 + wool * 0.25)).clamp(0.0, 1.0) * 255.0) as u8)
    })
}

/// A chalkboard with the scores of the night (tally marks and lines).
pub fn chalkboard() -> Pixels {
    image(256, |u, v| {
        let board = [0.10, 0.13, 0.11];
        let chalk = [0.78, 0.80, 0.76];
        let dust = fbm(u * 6.0, v * 6.0, 6, 3, 3);
        let mut c = [0, 1, 2].map(|i| board[i] + dust * 0.06);
        // Wiped arcs and lines of chalk.
        let line = |y: f32| (v - y).abs() < 0.006 && u > 0.1 && u < 0.9;
        let tally = |row: f32, n: u32| -> bool {
            (0..n).any(|k| {
                let x = 0.42 + k as f32 * 0.045;
                if k % 5 == 4 {
                    let t = (u - (x - 0.18)) / 0.2;
                    (0.0..=1.0).contains(&t) && (v - (row + 0.06 - t * 0.12)).abs() < 0.007
                } else {
                    (u - x).abs() < 0.006 && (v - row).abs() < 0.06
                }
            })
        };
        let name_block = |row: f32| {
            u > 0.12
                && u < 0.32
                && (v - row).abs() < 0.03
                && noise(u * 60.0, v * 60.0, 60, 9) > 0.45
        };
        if line(0.16) || line(0.5) || line(0.84) || (u - 0.37).abs() < 0.005 && v > 0.2 && v < 0.8 {
            c = chalk;
        }
        for (row, n) in [(0.32, 9), (0.66, 6)] {
            if tally(row, n) || name_block(row) {
                c = chalk;
            }
        }
        let px = [0, 1, 2].map(|i| c[i]);
        lerp3(px, px, 0.0)
    })
}

/// A dartboard seen face on (sectors, rings, bull), for a disc with polar UVs.
pub fn dartboard() -> Pixels {
    image(256, |u, v| {
        let (x, y) = (u * 2.0 - 1.0, v * 2.0 - 1.0);
        let r = (x * x + y * y).sqrt();
        let a = y.atan2(x) + std::f32::consts::PI / 20.0;
        let sector = ((a / std::f32::consts::TAU * 20.0).floor() as i32).rem_euclid(2);
        let black = [0.06, 0.06, 0.06];
        let cream = [0.86, 0.80, 0.66];
        let red = [0.70, 0.08, 0.08];
        let green = [0.05, 0.42, 0.18];
        let ring = |c0: [f32; 3], c1: [f32; 3]| if sector == 0 { c0 } else { c1 };
        let c = if r > 0.98 {
            [0.15, 0.10, 0.07]
        } else if r > 0.80 {
            black
        } else if r > 0.74 {
            ring(red, green)
        } else if r > 0.48 {
            ring(black, cream)
        } else if r > 0.43 {
            ring(red, green)
        } else if r > 0.08 {
            ring(black, cream)
        } else if r > 0.04 {
            green
        } else {
            red
        };
        let wire = ((a / std::f32::consts::TAU * 20.0).fract() < 0.02 && r > 0.08 && r < 0.80) as u8
            as f32;
        lerp3(c, [0.75, 0.75, 0.72], wire * 0.7)
    })
}

/// A vintage poster: aged paper, a big numbered ball and title bands.
pub fn poster(k: u32) -> Pixels {
    let papers = [[0.80, 0.72, 0.55], [0.70, 0.76, 0.68], [0.82, 0.70, 0.50]];
    let inks = [[0.48, 0.13, 0.10], [0.10, 0.26, 0.20], [0.14, 0.15, 0.30]];
    let balls = [[0.85, 0.65, 0.05], [0.60, 0.10, 0.10], [0.10, 0.20, 0.55]];
    let (paper, ink, ball) = (papers[k as usize % 3], inks[k as usize % 3], balls[k as usize % 3]);
    image(256, move |u, v| {
        let age = fbm(u * 5.0, v * 5.0, 5, 13 + k, 3);
        let mut c = [0, 1, 2].map(|i| paper[i] * (0.85 + age * 0.2));
        let edge = u.min(1.0 - u).min(v.min(1.0 - v));
        if edge < 0.05 {
            c = [0, 1, 2].map(|i| c[i] * 0.8);
        }
        if (edge - 0.07).abs() < 0.006 {
            c = ink;
        }
        // The ball, with its number circle.
        let (bx, by) = (u - 0.5, (v - 0.42) * 1.0);
        let r = (bx * bx + by * by).sqrt();
        if r < 0.24 {
            let light = 1.0 - ((bx + 0.08).powi(2) + (by + 0.1).powi(2)).sqrt() * 1.6;
            c = [0, 1, 2].map(|i| ball[i] * (0.55 + light.max(0.0) * 0.6));
            if r < 0.09 {
                c = [0.95, 0.93, 0.86];
                // An 8: two rings, one on the other.
                let ring =
                    |cy: f32, r: f32| ((bx * bx + (by - cy).powi(2)).sqrt() - r).abs() < 0.008;
                let ink_mark = ring(-0.026, 0.02) || ring(0.024, 0.025);
                if ink_mark {
                    c = [0.08, 0.08, 0.08];
                }
            }
        }
        // Title and subtitle bands.
        if (v - 0.78).abs() < 0.035
            && u > 0.15
            && u < 0.85
            && noise(u * 40.0, 1.0, 40, 3 + k) > 0.35
        {
            c = ink;
        }
        if (v - 0.87).abs() < 0.015 && u > 0.25 && u < 0.75 && noise(u * 70.0, 2.0, 70, 7 + k) > 0.4
        {
            c = ink;
        }
        lerp3(c, c, 0.0)
    })
}

/// A bottle label: paper with a coloured band and a dark title.
pub fn label() -> Pixels {
    image(64, |u, v| {
        let band = (v - 0.5).abs() < 0.12;
        let paper = [0.86, 0.80, 0.64];
        let c = if band { [0.55, 0.12, 0.10] } else { paper };
        let text =
            (v - 0.25).abs() < 0.04 && u > 0.2 && u < 0.8 && noise(u * 20.0, 0.0, 20, 4) > 0.4;
        if text { lerp3([0.1, 0.08, 0.06], [0.1, 0.08, 0.06], 0.0) } else { lerp3(c, c, 0.0) }
    })
}

/// A line of bold text rasterised from the game's font: coverage 0..1,
/// `width` × `height` pixels, the text `px` pixels high.
struct Lettering {
    width: usize,
    height: usize,
    cov: Vec<f32>,
}

impl Lettering {
    fn new(text: &str, px: f32, spacing: f32) -> Lettering {
        let font = fontdue::Font::from_bytes(
            &include_bytes!("../assets/DejaVuSans-Bold.ttf")[..],
            fontdue::FontSettings::default(),
        )
        .expect("bundled font");
        let mut glyphs = Vec::new();
        let mut pen = 0.0f32;
        for c in text.chars() {
            let (m, bitmap) = font.rasterize(c, px);
            glyphs.push((pen.round() as i32 + m.xmin, m, bitmap));
            pen += m.advance_width + spacing;
        }
        let x0 = glyphs.iter().map(|(x, _, _)| *x).min().unwrap_or(0);
        let x1 = glyphs.iter().map(|(x, m, _)| x + m.width as i32).max().unwrap_or(1);
        let top = glyphs.iter().map(|(_, m, _)| m.ymin + m.height as i32).max().unwrap_or(1);
        let bottom = glyphs.iter().map(|(_, m, _)| m.ymin).min().unwrap_or(0);
        let (width, height) = ((x1 - x0).max(1) as usize, (top - bottom).max(1) as usize);
        let mut cov = vec![0.0f32; width * height];
        for (x, m, bitmap) in &glyphs {
            let ox = (x - x0) as usize;
            let oy = (top - (m.ymin + m.height as i32)) as usize;
            for r in 0..m.height {
                for c in 0..m.width {
                    let v = bitmap[r * m.width + c] as f32 / 255.0;
                    let at = (oy + r) * width + ox + c;
                    if at < cov.len() {
                        cov[at] = cov[at].max(v);
                    }
                }
            }
        }
        Lettering { width, height, cov }
    }

    /// Coverage at pixel (`x`, `y`) of a picture where the text's top left
    /// corner is at (`left`, `top`).
    fn at(&self, x: i32, y: i32, left: i32, top: i32) -> f32 {
        let (cx, cy) = (x - left, y - top);
        if cx < 0 || cy < 0 || cx >= self.width as i32 || cy >= self.height as i32 {
            0.0
        } else {
            self.cov[cy as usize * self.width + cx as usize]
        }
    }
}

/// The paper wrapped round the four sides of a cube of chalk of `color`:
/// the chalk's own colour, a fine band at the top and the bottom, and the
/// game's name printed across it (pale on a dark chalk, dark on a light
/// one), with a little grain and a little chalk dust towards the top.
pub fn chalk_side(color: [u8; 3]) -> Pixels {
    let size: usize = 256;
    let base = color.map(|v| v as f32 / 255.0);
    let luma = 0.3 * base[0] + 0.6 * base[1] + 0.1 * base[2];
    let ink = if luma > 0.55 { [0.10, 0.09, 0.08] } else { [0.96, 0.93, 0.86] };
    let big = Lettering::new("SNOOK", 58.0, 1.0);
    let small = Lettering::new("LEGEND", 30.0, 5.0);
    let big_at = ((size as i32 - big.width as i32) / 2, 86);
    let small_at = ((size as i32 - small.width as i32) / 2, 86 + big.height as i32 + 12);
    let mut rgba = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let (u, v) = (x as f32 / size as f32, y as f32 / size as f32);
            // Paper grain and a slightly lighter middle (the light on it).
            let grain = (noise(u * 64.0, v * 64.0, 64, 71) - 0.5) * 0.05;
            let shade = 1.0 - 0.10 * ((u - 0.5) * 2.0).powi(2) + grain;
            let mut c = base.map(|b| b * shade);
            // The bands.
            let band = |at: f32| ((v - at).abs() < 0.012) as u8 as f32;
            let lines = band(0.10).max(band(0.90));
            let text = big
                .at(x as i32, y as i32, big_at.0, big_at.1)
                .max(small.at(x as i32, y as i32, small_at.0, small_at.1));
            let k = lines.max(text * 0.92);
            c = [0, 1, 2].map(|i| c[i] + (ink[i] - c[i]) * k);
            // Chalk dust towards the top edge, where the cue brushes it.
            let dust = (1.0 - v / 0.12).clamp(0.0, 1.0) * noise(u * 40.0, v * 40.0, 40, 9) * 0.25;
            c = [0, 1, 2].map(|i| c[i] + (base[i] * 0.4 + 0.6 - c[i]) * dust);
            let px = lerp3(c, c, 0.0);
            rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
    }
    Pixels { width: size as u16, height: size as u16, rgba }
}

/// The top of a cube of chalk of `color`: the chalk itself, powdery, with
/// the hollow a cue's tip wears into it (darker where it is packed hard,
/// with a lighter, dusty rim).
pub fn chalk_top(color: [u8; 3]) -> Pixels {
    let base = color.map(|v| v as f32 / 255.0);
    image(128, |u, v| {
        let (dx, dy) = (u - 0.5, v - 0.5);
        let d = (dx * dx + dy * dy).sqrt();
        let powder = (fbm(u * 24.0, v * 24.0, 24, 33, 3) - 0.5) * 0.18;
        // The worn hollow: deeper (darker) in the middle, a dusty rim.
        let hollow = (1.0 - (d / 0.30)).clamp(0.0, 1.0);
        let rim = (1.0 - ((d - 0.31) / 0.05).abs()).clamp(0.0, 1.0);
        let k = 1.0 + powder - 0.28 * hollow * hollow + 0.18 * rim;
        // A paper edge round the top.
        let edge = (dx.abs().max(dy.abs()) > 0.47) as u8 as f32;
        let c = base.map(|b| (b * k).min(1.0) * (1.0 - 0.25 * edge));
        lerp3(c, c, 0.0)
    })
}

/// Soft round shadow: black with alpha falling off towards the edge.
/// A soft white disc fading out (light on a wall).
/// The halo round a bright light: a hot core in a wide, soft fall-off.
pub fn halo() -> Pixels {
    let size: u16 = 128;
    let mut rgba = Vec::with_capacity(size as usize * size as usize * 4);
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let dy = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let d2 = dx * dx + dy * dy;
            let edge = (1.0 - d2.sqrt()).clamp(0.0, 1.0);
            let a = (edge * edge * 0.55 + (-d2 * 28.0).exp() * 0.45).min(1.0);
            rgba.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    Pixels { width: size, height: size, rgba }
}

pub fn glow() -> Pixels {
    let size: u16 = 64;
    let mut rgba = Vec::with_capacity(size as usize * size as usize * 4);
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let dy = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let d = (dx * dx + dy * dy).sqrt();
            let a = (1.0 - d).clamp(0.0, 1.0).powf(2.0);
            rgba.extend_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
        }
    }
    Pixels { width: size, height: size, rgba }
}

pub fn shadow() -> Pixels {
    let size: u16 = 64;
    let mut rgba = Vec::with_capacity(size as usize * size as usize * 4);
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let dy = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let d = (dx * dx + dy * dy).sqrt();
            let a = (1.0 - d).clamp(0.0, 1.0).powf(1.6);
            rgba.extend_from_slice(&[0, 0, 0, (a * 255.0) as u8]);
        }
    }
    Pixels { width: size, height: size, rgba }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_render() {
        let one = Label::new(1);
        assert!(one.sample(0.0, 0.0) > 0.9, "the 1's stem is in the middle");
        assert!(one.sample(-0.45, -0.1) < 0.1);
        assert!(Label::new(15).sample(0.0, 0.9) < 0.01, "nothing above the digits");
        // 6 and 9 are underlined.
        assert!(Label::new(6).h > Label::new(8).h);
    }

    #[test]
    fn ball_layout() {
        let classic = Palette::of("balls_classic");
        assert_eq!(
            ball_surface(BallId(3), Vec3::new(0.33, 0.33, 0.88).normalize(), &classic),
            IVORY
        );
        assert_eq!(
            ball_surface(BallId(3), Vec3::Z, &Palette::of("balls_classic")),
            INK,
            "the number is in the middle of the spot"
        );
        assert_eq!(ball_surface(BallId(11), Vec3::X, &Palette::of("balls_classic")), IVORY);
        assert_eq!(
            ball_surface(BallId(11), Vec3::Y, &Palette::of("balls_classic")),
            ball_color(BallId(11))
        );
        assert_eq!(
            ball_surface(BallId(3), Vec3::X, &Palette::of("balls_classic")),
            ball_color(BallId(3))
        );
    }

    #[test]
    fn textures_have_expected_size_and_tile() {
        let f = felt_sized([24, 96, 48], 512);
        assert_eq!(f.rgba.len(), 512 * 512 * 4);
        // Tileable noise: first and last columns are close.
        let px = |x: usize, y: usize| f.rgba[(y * 512 + x) * 4 + 1] as i32;
        assert!((px(0, 10) - px(511, 10)).abs() <= 6);
        let s = shadow();
        assert_eq!(s.rgba[3], 0, "corner is transparent");
        assert!(s.rgba[(32 * 64 + 32) * 4 + 3] > 200, "centre is dark");
    }
}
