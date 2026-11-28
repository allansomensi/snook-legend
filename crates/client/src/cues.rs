//! The patterns of the cues' forearms and butt sleeves, shared by the 3D
//! cue at the table and its picture in the shop (so the shop shows what
//! the table will).
//!
//! `u` runs round the cue (0 to 1), `v` along the part painted (0 at the
//! joint end, 1 at the butt end).

use macroquad::prelude::Color;
use snook_legend_proto::shop::CueStyle;

/// The colours a cue is painted with (from its shop look).
#[derive(Debug, Clone, Copy)]
pub struct Paint {
    pub butt: Color,
    pub accent: Color,
    pub ring: Color,
}

impl Paint {
    pub fn of(butt: [u8; 3], accent: [u8; 3], ring: [u8; 3]) -> Paint {
        let c = |v: [u8; 3]| Color::from_rgba(v[0], v[1], v[2], 255);
        Paint { butt: c(butt), accent: c(accent), ring: c(ring) }
    }
}

pub(crate) fn mix(a: Color, b: Color, k: f32) -> Color {
    let k = k.clamp(0.0, 1.0);
    Color::new(a.r + (b.r - a.r) * k, a.g + (b.g - a.g) * k, a.b + (b.b - a.b) * k, 1.0)
}

pub(crate) fn shade(c: Color, k: f32) -> Color {
    if k >= 0.0 {
        mix(c, Color::new(1.0, 1.0, 1.0, 1.0), k)
    } else {
        mix(c, Color::new(0.0, 0.0, 0.0, 1.0), -k)
    }
}

/// A repeatable pseudo-random number in 0..1 for a cell.
pub(crate) fn hash(i: i32, j: i32) -> f32 {
    let mut h = (i as u32).wrapping_mul(0x8DA6_B343) ^ (j as u32).wrapping_mul(0xD816_3841);
    h ^= h >> 13;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 16;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Whether a style also paints the butt sleeve (the plainer ones keep it
/// plain, as before).
pub fn paints_sleeve(style: CueStyle) -> bool {
    !matches!(
        style,
        CueStyle::Plain | CueStyle::Points | CueStyle::Spiral | CueStyle::Weave | CueStyle::Stripes
    )
}

/// The colour of the wrap (the grip): the accent's, or for the finest
/// styles a dark leather of the butt's tone, so that the gold or the
/// pearl stays for the inlays.
pub fn wrap_color(style: CueStyle, p: Paint) -> Color {
    if leather_wrap(style) { shade(p.butt, -0.55) } else { p.accent }
}

/// Whether a style's wrap is leather (stitched; the finest styles) rather
/// than Irish linen.
pub fn leather_wrap(style: CueStyle) -> bool {
    matches!(
        style,
        CueStyle::Filigree
            | CueStyle::Inlay
            | CueStyle::Damascus
            | CueStyle::Lightning
            | CueStyle::Aurora
            | CueStyle::Lava
            | CueStyle::Crystal
            | CueStyle::Gems
    )
}

/// The colour at `(u, v)` of a forearm (or sleeve) of `style`.
pub fn pattern(style: CueStyle, p: Paint, u: f32, v: f32) -> Color {
    use std::f32::consts::TAU;
    let (b, a, ring) = (p.butt, p.accent, p.ring);
    match style {
        CueStyle::Plain => b,
        CueStyle::Points => {
            // Four points rising from the butt end towards the tip.
            let side = ((u * 4.0).fract() - 0.5).abs() * 2.0;
            if v > 0.12 + 0.88 * side { a } else { b }
        }
        CueStyle::Spiral => {
            if (u * 2.0 + v * 3.0).fract() < 0.38 {
                a
            } else {
                b
            }
        }
        CueStyle::Weave => {
            let (i, j) = ((u * 12.0) as i32, (v * 15.0) as i32);
            if (i + j) % 2 == 0 { a } else { b }
        }
        CueStyle::Flame => {
            // Five tongues of fire licking up from the butt end, flickering
            // edges, hot at the base and pale at the tips.
            let k = u * 5.0;
            let phase = k.fract();
            let tongue = 1.0 - (phase * 2.0 - 1.0).abs();
            let wobble =
                (v * 22.0 + k.floor() * 1.7).sin() * 0.06 + (v * 41.0 + u * 30.0).sin() * 0.03;
            let height = 0.25 + 0.65 * tongue.powf(1.6) + wobble;
            let up = 1.0 - v; // 0 at the butt end
            if up < height {
                let t = up / height.max(0.01);
                // A dark outline round each tongue.
                if height - up < 0.035 { shade(a, -0.55) } else { mix(a, ring, t * t) }
            } else {
                b
            }
        }
        CueStyle::Marble => {
            // Veins winding along the stone, with a soft cloudiness.
            let x = u * TAU;
            let n = (x * 2.0
                + v * 9.0
                + (v * 23.0 + x * 3.0).sin() * 1.6
                + (x * 5.0 - v * 11.0).sin() * 0.7)
                .sin();
            let fine = (x * 7.0 - v * 31.0 + (v * 13.0).sin() * 2.0).sin();
            let cloud = ((x * 3.0 + v * 5.0).sin() * (v * 7.0 - x).cos()) * 0.06;
            if n.abs() < 0.05 {
                shade(a, -0.15)
            } else if n.abs() < 0.11 {
                mix(b, a, 0.75 - n.abs() * 3.0)
            } else if fine.abs() < 0.04 {
                mix(b, a, 0.45)
            } else {
                shade(b, cloud)
            }
        }
        CueStyle::Gems => {
            // Diamond-cut stones in rows, set in the ring's metal, with a
            // spark of light on each.
            let (cu, cv) = (u * 8.0, v * 6.0);
            let row = cv.floor() as i32;
            let cu = cu + if row % 2 == 0 { 0.0 } else { 0.5 };
            let (fu, fv) = (cu.fract() - 0.5, cv.fract() - 0.5);
            let d = fu.abs() * 2.0 + fv.abs() * 2.0;
            if d < 0.5 {
                // Facets: lighter on one side, a white spark near the top.
                if (fu + 0.12).abs() < 0.06 && (fv + 0.12).abs() < 0.06 {
                    Color::new(1.0, 1.0, 1.0, 1.0)
                } else if fu + fv < 0.0 {
                    shade(a, 0.25)
                } else {
                    shade(a, -0.2)
                }
            } else if d < 0.66 {
                ring
            } else {
                b
            }
        }
        CueStyle::Scales => {
            // Overlapping scales, each darker at its rim and lighter at its
            // heart, in offset rows.
            let rows = 9.0;
            let cv = v * rows;
            let row = cv.floor() as i32;
            let cu = u * 7.0 + if row % 2 == 0 { 0.0 } else { 0.5 };
            let (fu, fv) = (cu.fract() - 0.5, cv.fract());
            // Distance from the scale's root (at the top of its cell).
            let d = (fu * fu + (fv - 1.0) * (fv - 1.0) * 0.6).sqrt();
            let jitter = hash(cu.floor() as i32, row) * 0.12;
            if d > 0.52 {
                mix(b, a, 0.15 + jitter)
            } else if d > 0.46 {
                ring
            } else {
                mix(b, a, 0.35 + (1.0 - d / 0.46) * 0.6 + jitter)
            }
        }
        CueStyle::Chrome => {
            // Polished metal: a bright reflection running along the cue and
            // a darker side, with engraved bands of the accent colour.
            let x = u * TAU;
            let light = 0.5 + 0.5 * (x - 0.6).cos();
            let base = mix(shade(b, -0.45), shade(b, 0.55), light.powf(2.2));
            let band = (v * 6.0).fract();
            if band < 0.06 {
                a
            } else if (0.06..0.09).contains(&band) {
                ring
            } else {
                // Fine engraving: a scroll between the bands.
                let scroll = ((x * 6.0 + v * 60.0).sin() * (v * 40.0).sin()).abs();
                if scroll > 0.93 { shade(a, 0.1) } else { base }
            }
        }
        CueStyle::Nebula => {
            // Clouds of colour drifting over a night sky, and stars.
            let x = u * TAU;
            let cloud = ((x * 2.0 + v * 4.0).sin()
                + (x * 3.0 - v * 7.0).cos() * 0.6
                + (v * 13.0 + x).sin() * 0.3)
                / 1.9;
            let cloud = (cloud * 0.5 + 0.5).powf(2.5);
            let sky = mix(b, a, cloud * 0.9);
            let (i, j) = ((u * 96.0) as i32, (v * 80.0) as i32);
            let star = hash(i, j);
            if star > 0.985 {
                Color::new(1.0, 1.0, 1.0, 1.0)
            } else if star > 0.96 {
                mix(sky, ring, 0.7)
            } else {
                sky
            }
        }
        CueStyle::Filigree => {
            // Gold wire scrolls on lacquer: a running vine with curls on
            // either side, between two fine bands, and a soft glint of the
            // lacquer between.
            let x = u * TAU;
            let band = (v * 3.0).fract();
            let cell = (v * 3.0).floor();
            let stem = (x * 2.0 + cell * 1.3).sin() * 0.18 + 0.5;
            let d_stem = (band - stem).abs();
            // Curls: circles hung off the vine every eighth of a turn.
            let k = (u * 8.0).fract() - 0.5;
            let side = if ((u * 8.0) as i32 + cell as i32) % 2 == 0 { 0.2 } else { -0.2 };
            let (cx, cy) = (k * 0.9, band - stem - side);
            let ring_d = ((cx * cx + cy * cy).sqrt() - 0.12).abs();
            let rim = !(0.03..=0.97).contains(&band);
            let lacquer = shade(b, 0.08 * (x - 0.6).cos());
            if rim || d_stem < 0.025 || ring_d < 0.022 {
                // The wire: brighter where it turns to the light.
                let lit = 0.5 + 0.5 * (x * 3.0 + v * 20.0).sin();
                mix(a, ring, 0.35 * lit)
            } else if d_stem < 0.04 || ring_d < 0.034 {
                mix(lacquer, a, 0.35)
            } else {
                lacquer
            }
        }
        CueStyle::Inlay => {
            // Mother-of-pearl: long ovals and diamonds set in rows, each
            // shimmering from the accent to the ring colour across its face.
            let (cu, cv) = (u * 6.0, v * 4.0);
            let row = cv.floor() as i32;
            let cu = cu + if row % 2 == 0 { 0.0 } else { 0.5 };
            let (fu, fv) = (cu.fract() - 0.5, cv.fract() - 0.5);
            let oval = (fu * fu * 9.0 + fv * fv * 4.0).sqrt();
            let diamond = fu.abs() * 4.0 + fv.abs() * 6.0;
            let shape = if row % 2 == 0 { oval } else { diamond };
            if shape < 0.95 {
                let sheen = ((fu * 9.0 + fv * 7.0 + cu.floor() * 1.7).sin() * 0.5 + 0.5).powf(1.5);
                let pearl = mix(a, ring, sheen);
                shade(pearl, 0.2 * (1.0 - shape))
            } else if shape < 1.12 {
                shade(ring, -0.35)
            } else {
                b
            }
        }
        CueStyle::Damascus => {
            // Folded steel: many thin layers rippling along the cue, the
            // bright ones of the accent, the dark ones of the butt.
            let x = u * TAU;
            let wave = (v * 40.0
                + (x * 2.0 + v * 6.0).sin() * 2.2
                + (x * 5.0 - v * 13.0).sin() * 0.9
                + (v * 3.0 + x).cos() * 1.5)
                .sin();
            let layer = wave * 0.5 + 0.5;
            let steel = mix(shade(b, -0.25), a, layer.powf(0.8));
            // Polished: a bright streak where the light falls.
            let light = 0.5 + 0.5 * (x - 0.6).cos();
            let steel = shade(steel, 0.25 * light.powf(6.0));
            let band = (v * 5.0).fract();
            if band < 0.025 { ring } else { steel }
        }
        CueStyle::Lightning => {
            // Forked bolts zigzagging down the cue: white-hot at the core,
            // the accent colour round it, a glow fading into the dark.
            let mut best = 1.0f32;
            for bolt in 0..3 {
                let base = bolt as f32 / 3.0 + 0.08;
                let seg = (v * 7.0).floor();
                let t = (v * 7.0).fract();
                let jag = |s: f32| (hash(bolt, s as i32) - 0.5) * 0.14;
                let path = base + jag(seg) + (jag(seg + 1.0) - jag(seg)) * t;
                let du = ((u - path + 0.5).rem_euclid(1.0) - 0.5).abs();
                best = best.min(du);
                // A fork leaving the bolt halfway down.
                if (0.4..0.7).contains(&v) {
                    let fork = path + (v - 0.4) * 0.35;
                    best = best.min(((u - fork + 0.5).rem_euclid(1.0) - 0.5).abs() * 1.5);
                }
            }
            if best < 0.006 {
                Color::new(1.0, 1.0, 1.0, 1.0)
            } else if best < 0.016 {
                mix(ring, a, (best - 0.006) / 0.01)
            } else {
                mix(b, a, (0.35 - best * 5.0).max(0.0))
            }
        }
        CueStyle::Aurora => {
            // Curtains of light hanging down the cue, from the accent to the
            // ring colour, waving round it, over a night sky with stars.
            let x = u * TAU;
            let fold = (x * 3.0 + (v * 4.0 + x).sin() * 1.2).sin() * 0.5 + 0.5;
            let curtain = fold.powf(3.0) * (1.0 - v * 0.6);
            let hue = mix(a, ring, (v * 1.4 + fold * 0.3).min(1.0));
            let sky = mix(b, hue, curtain.min(1.0) * 0.95);
            let (i, j) = ((u * 90.0) as i32, (v * 60.0) as i32);
            if hash(i, j) > 0.988 { mix(sky, Color::new(1.0, 1.0, 1.0, 1.0), 0.8) } else { sky }
        }
        CueStyle::Lava => {
            // Cooled black rock in plates, split by cracks that glow from
            // the accent at their edge to the ring colour at the hottest.
            let (cu, cv) = (u * 6.0, v * 5.0);
            let (iu, iv) = (cu.floor() as i32, cv.floor() as i32);
            // How far from the edge between the nearest two cells (cracks).
            let mut d1 = 9.0f32;
            let mut d2 = 9.0f32;
            for du in -1..=1 {
                for dv in -1..=1 {
                    let (nu, nv) = (iu + du, iv + dv);
                    let pu = nu as f32 + 0.2 + 0.6 * hash(nu.rem_euclid(6), nv);
                    let pv = nv as f32 + 0.2 + 0.6 * hash(nv, nu.rem_euclid(6) + 17);
                    let d = ((cu - pu).powi(2) + (cv - pv).powi(2)).sqrt();
                    if d < d1 {
                        d2 = d1;
                        d1 = d;
                    } else if d < d2 {
                        d2 = d;
                    }
                }
            }
            let edge = d2 - d1;
            if edge < 0.05 {
                mix(ring, a, edge / 0.05)
            } else if edge < 0.14 {
                mix(a, shade(b, 0.05), ((edge - 0.05) / 0.09).sqrt())
            } else {
                shade(b, 0.06 * (hash(iu, iv) - 0.5))
            }
        }
        CueStyle::Crystal => {
            // Ice facets: triangles of light and shade, with bright edges.
            let (cu, cv) = (u * 8.0, v * 5.0);
            let (fu, fv) = (cu.fract(), cv.fract());
            let upper = fu + fv < 1.0;
            let cell = hash(cu.floor() as i32 * 2 + upper as i32, cv.floor() as i32);
            let facet = mix(shade(a, -0.3), shade(b, 0.2), cell);
            let edge = fu.min(fv).min((1.0 - fu - fv).abs());
            if edge < 0.03 {
                mix(ring, Color::new(1.0, 1.0, 1.0, 1.0), 0.5)
            } else if cell > 0.9 {
                mix(facet, ring, 0.6)
            } else {
                facet
            }
        }
        CueStyle::Stripes => {
            // Bold diagonal stripes, with a thin line of the ring colour
            // between them.
            let t = (u * 3.0 + v * 2.0).fract();
            if t < 0.36 {
                a
            } else if !(0.42..=0.94).contains(&t) {
                ring
            } else {
                b
            }
        }
        CueStyle::Confetti => {
            // Little discs and slivers of the accent, the ring colour and
            // white, scattered every which way.
            let (cu, cv) = (u * 14.0, v * 12.0);
            let (iu, iv) = (cu.floor() as i32, cv.floor() as i32);
            let h = hash(iu.rem_euclid(14), iv);
            let (pu, pv) = (0.3 + 0.4 * hash(iv, iu.rem_euclid(14)), 0.3 + 0.4 * h);
            let (du, dv) = (cu.fract() - pu, cv.fract() - pv);
            let r = 0.18 + 0.14 * hash(iu.rem_euclid(14) + 3, iv + 5);
            let inside = if h > 0.5 {
                du * du + dv * dv < r * r
            } else {
                // A sliver, turned.
                let (c, s) = ((h * 9.0).cos(), (h * 9.0).sin());
                let (x, y) = (du * c - dv * s, du * s + dv * c);
                x.abs() < r * 1.3 && y.abs() < r * 0.4
            };
            if !inside || hash(iu, iv + 99) < 0.25 {
                b
            } else {
                match (h * 3.0) as i32 {
                    0 => a,
                    1 => ring,
                    _ => Color::new(1.0, 1.0, 1.0, 1.0),
                }
            }
        }
        CueStyle::Snowflake => {
            // Six-pointed flakes of the accent colour, of two sizes, on a
            // frosty ground.
            let (cu, cv) = (u * 5.0, v * 4.0);
            let row = cv.floor() as i32;
            let cu = cu + if row % 2 == 0 { 0.0 } else { 0.5 };
            let (fu, fv) = ((cu.fract() - 0.5) * 1.2, cv.fract() - 0.5);
            let r = (fu * fu + fv * fv).sqrt();
            let ang = fv.atan2(fu);
            let arm = ((ang * 3.0).cos().abs() - 1.0).abs() * r;
            let sub = ((ang * 3.0 + 0.5).cos().abs() - 1.0).abs() * r;
            let small = hash(cu.floor() as i32, row) > 0.5;
            let reach = if small { 0.28 } else { 0.42 };
            let flake = r < reach
                && (arm < 0.022 || (sub < 0.016 && (r - reach * 0.6).abs() < reach * 0.25));
            let frost = mix(b, ring, 0.15 * ((u * 60.0).sin() * (v * 47.0).sin()).abs());
            if flake || r < 0.035 { a } else { frost }
        }
        CueStyle::Web => {
            // A spider's web: spokes round its centre and threads looping
            // between them, slightly sagging.
            let (cu, cv) = (u * 2.0, v * 1.5);
            let (fu, fv) = (cu.fract() - 0.5, cv.fract() - 0.5);
            let r = (fu * fu + fv * fv).sqrt();
            let ang = fv.atan2(fu);
            let spokes = 8.0;
            let spoke = ((ang / TAU * spokes).fract() - 0.5).abs() * r * TAU / spokes;
            let sag = (ang * spokes).sin().abs() * 0.012;
            let rings = ((r + sag) * 14.0).fract();
            let thread = spoke < 0.006 || (r < 0.48 && !(0.07..=0.93).contains(&rings));
            if thread {
                a
            } else if r < 0.04 {
                ring
            } else {
                b
            }
        }
        CueStyle::Bunting => {
            // Strings of little triangular flags looping round the cue, of
            // the accent, the ring colour and white in turn.
            let lines = 4.0;
            let cv = v * lines;
            let (row, fv) = (cv.floor() as i32, cv.fract());
            let flags = 9.0;
            let cu = u * flags + row as f32 * 0.5;
            let (k, fu) = (cu.floor() as i32, cu.fract());
            // The string sags a little between the flags.
            let string = 0.12 + (fu - 0.5).powi(2) * 0.12;
            let flag = fv > string && fv < string + 0.55 * (1.0 - (fu - 0.5).abs() * 2.0);
            if (fv - string).abs() < 0.02 {
                shade(b, -0.5)
            } else if flag {
                match k.rem_euclid(3) {
                    0 => a,
                    1 => ring,
                    _ => Color::new(0.98, 0.96, 0.9, 1.0),
                }
            } else {
                b
            }
        }
    }
}

/// A cue's look (the colours and pattern of its shop item).
#[derive(Debug, Clone, Copy)]
pub struct Design {
    pub paint: Paint,
    pub style: CueStyle,
    pub shaft: Color,
    /// A carbon-fibre shaft (else maple).
    pub carbon: bool,
    /// How fine its make is (its rarity in the shop): collars, rings,
    /// veneers, metal (see `cue_skin`).
    pub tier: snook_legend_proto::shop::Rarity,
}

impl Design {
    pub fn of(code: &str) -> Option<Design> {
        use snook_legend_proto::shop::{CARBON_SHAFT, Look, item, rarity};
        let item = item(code)?;
        match item.look {
            Look::Cue { butt, accent, ring, style, shaft } => Some(Design {
                paint: Paint::of(butt, accent, ring),
                style,
                shaft: Color::from_rgba(shaft[0], shaft[1], shaft[2], 255),
                carbon: shaft == CARBON_SHAFT,
                tier: rarity(item),
            }),
            _ => None,
        }
    }
}

/// A picture of the cue, `w` by `h` pixels, from its butt to `span` along
/// it (1: the whole cue): its skin (see `cue_skin`) lit as in a studio,
/// lacquer with a sharp streak of light, metal mirroring a bright band,
/// linen and rubber matte, pearl shifting its colours.
pub fn image(d: &Design, span: f32, w: u16, h: u16) -> macroquad::texture::Image {
    use crate::cue_skin::{Finish, LENGTH, Look, radius, texel};
    let look = Look { design: *d, chalk: Color::from_rgba(40, 80, 150, 255), level: 0 };
    let mut img = macroquad::texture::Image::gen_image_color(w, h, Color::new(0.0, 0.0, 0.0, 0.0));
    // The butt (29.6 mm) fills all but a pixel or two of the height.
    let unit = h as f32 / 2.0 / 0.0155;
    for px in 0..w {
        let k = (px as f32 + 0.5) / w as f32 * span;
        let y = (1.0 - k) * LENGTH;
        // (The bumper's rounded end is left square: a picture's edge.)
        let hw = radius(y.clamp(0.0, LENGTH - 0.002)) * unit;
        for py in 0..h {
            let dy = py as f32 + 0.5 - h as f32 / 2.0;
            // Coverage of the edge, for a smooth outline.
            let cover = (hw - dy.abs() + 0.5).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let n = (dy / hw).clamp(-1.0, 1.0); // -1 at the top, 1 at the bottom
            let u = 0.25 + 0.5 * (n + 1.0) / 2.0;
            let (base, finish) = texel(&look, y, u);
            let facing = (1.0 - n * n).sqrt();
            let light = 0.55 + 0.45 * facing - 0.18 * n;
            let streak = |width: f32, at: f32| (-((n - at) / width).powi(2)).exp();
            let lit = |k: f32| Color::new(base.r * k, base.g * k, base.b * k, 1.0);
            let c = match finish {
                Finish::Metal => {
                    // A bright band of the room above, dark below it, a
                    // pale rim: the metal's own colour throughout.
                    let band = 0.25 + 1.1 * streak(0.28, -0.45) + 0.35 * streak(0.2, 0.75);
                    let c = lit(band);
                    mix(c, Color::new(1.0, 1.0, 1.0, 1.0), 0.35 * streak(0.07, -0.47))
                }
                Finish::Matte => lit(0.5 + 0.42 * facing - 0.2 * n),
                Finish::Satin => {
                    let c = lit(light);
                    mix(c, Color::new(1.0, 1.0, 1.0, 1.0), 0.16 * streak(0.2, -0.42))
                }
                Finish::Lacquer | Finish::Pearl => {
                    let c = lit(light);
                    let c = mix(c, Color::new(1.0, 1.0, 1.0, 1.0), 0.42 * streak(0.1, -0.42));
                    if finish == Finish::Pearl {
                        let t = n * 3.0 + k * 40.0;
                        let tint = Color::new(
                            0.5 + 0.5 * t.cos(),
                            0.5 + 0.5 * (t - 2.1).cos(),
                            0.5 + 0.5 * (t + 2.1).cos(),
                            1.0,
                        );
                        mix(c, tint, 0.14)
                    } else {
                        c
                    }
                }
            };
            img.set_pixel(
                px as u32,
                py as u32,
                Color::new(c.r.min(1.0), c.g.min(1.0), c.b.min(1.0), cover),
            );
        }
    }
    img
}

thread_local! {
    static PICTURES: std::cell::RefCell<std::collections::HashMap<(String, u32), macroquad::texture::Texture2D>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The picture of `code` from its butt to `span` along it, made once.
pub fn picture(code: &str, span: f32) -> Option<macroquad::texture::Texture2D> {
    let key = (code.to_string(), (span * 1000.0) as u32);
    if let Some(t) = PICTURES.with(|p| p.borrow().get(&key).cloned()) {
        return Some(t);
    }
    let d = Design::of(code)?;
    let w = (720.0 * span) as u16;
    let tex = macroquad::texture::Texture2D::from_image(&image(&d, span, w.max(64), 64));
    tex.set_filter(macroquad::texture::FilterMode::Linear);
    PICTURES.with(|p| p.borrow_mut().insert(key, tex.clone()));
    Some(tex)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every style paints something other than plain wood somewhere (so no
    /// two cues of different styles look alike), and stays in range.
    #[test]
    fn every_style_shows_its_pattern() {
        let p = Paint::of([20, 20, 20], [230, 60, 30], [240, 220, 120]);
        for style in CueStyle::ALL {
            let mut differs = false;
            for i in 0..48 {
                for j in 0..64 {
                    let c = pattern(style, p, i as f32 / 48.0, j as f32 / 64.0);
                    assert!((0.0..=1.0).contains(&c.r) && (0.0..=1.0).contains(&c.g), "{style:?}");
                    differs |= (c.r - p.butt.r).abs() > 0.05;
                }
            }
            assert_eq!(differs, style != CueStyle::Plain, "{style:?}");
        }
    }

    /// Every cue's picture comes out, with its outline see-through and its
    /// middle solid.
    #[test]
    fn pictures_of_every_cue() {
        use snook_legend_proto::shop::{Category, ITEMS};
        for item in ITEMS.iter().filter(|i| i.category == Category::Cue) {
            let d = Design::of(item.code).expect("a cue");
            let img = image(&d, 0.62, 120, 32);
            assert_eq!(img.get_pixel(60, 0).a, 0.0, "{}", item.code);
            assert_eq!(img.get_pixel(20, 16).a, 1.0, "{}", item.code);
        }
    }
}
