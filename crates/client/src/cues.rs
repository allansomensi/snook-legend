//! The patterns of the cues' forearms and butt sleeves, shared by the 3D
//! cue at the table and its picture in the shop (so the shop shows what
//! the table will).
//!
//! `u` runs round the cue (0 to 1), `v` along the part painted (0 at the
//! joint end, 1 at the butt end).

use macroquad::prelude::Color;
use snookered_proto::shop::CueStyle;

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

fn mix(a: Color, b: Color, k: f32) -> Color {
    let k = k.clamp(0.0, 1.0);
    Color::new(a.r + (b.r - a.r) * k, a.g + (b.g - a.g) * k, a.b + (b.b - a.b) * k, 1.0)
}

fn shade(c: Color, k: f32) -> Color {
    if k >= 0.0 {
        mix(c, Color::new(1.0, 1.0, 1.0, 1.0), k)
    } else {
        mix(c, Color::new(0.0, 0.0, 0.0, 1.0), -k)
    }
}

/// A repeatable pseudo-random number in 0..1 for a cell.
fn hash(i: i32, j: i32) -> f32 {
    let mut h = (i as u32).wrapping_mul(0x8DA6_B343) ^ (j as u32).wrapping_mul(0xD816_3841);
    h ^= h >> 13;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 16;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Whether a style also paints the butt sleeve (the plainer ones keep it
/// plain, as before).
pub fn paints_sleeve(style: CueStyle) -> bool {
    !matches!(style, CueStyle::Plain | CueStyle::Points | CueStyle::Spiral | CueStyle::Weave)
}

/// How finely a style must be painted on the 3D cue: (round, along).
pub fn resolution(style: CueStyle) -> (u32, u32) {
    match style {
        CueStyle::Plain => (16, 1),
        CueStyle::Points | CueStyle::Spiral => (32, 40),
        CueStyle::Weave => (24, 30),
        _ => (48, 64),
    }
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
    }
}

/// A cue's look (the colours and pattern of its shop item).
#[derive(Debug, Clone, Copy)]
pub struct Design {
    pub paint: Paint,
    pub style: CueStyle,
    pub shaft: Color,
}

impl Design {
    pub fn of(code: &str) -> Option<Design> {
        use snookered_proto::shop::{Look, item};
        match item(code)?.look {
            Look::Cue { butt, accent, ring, style, shaft } => Some(Design {
                paint: Paint::of(butt, accent, ring),
                style,
                shaft: Color::from_rgba(shaft[0], shaft[1], shaft[2], 255),
            }),
            _ => None,
        }
    }
}

/// Half the thickness of the cue at `k` along it (0: the butt end, 1: the
/// tip), in the units of the shop's picture (6 at the butt).
pub fn half_width(k: f32) -> f32 {
    let lerp = |k0: f32, w0: f32, k1: f32, w1: f32| w0 + (w1 - w0) * ((k - k0) / (k1 - k0));
    match k {
        k if k < 0.08 => lerp(0.0, 6.0, 0.08, 5.6),
        k if k < 0.26 => lerp(0.08, 5.6, 0.26, 5.0),
        k if k < 0.46 => lerp(0.26, 5.0, 0.46, 4.3),
        k if k < 0.97 => lerp(0.46, 4.3, 0.97, 2.4),
        _ => lerp(0.97, 2.4, 1.0, 2.2),
    }
}

/// The colour of the cue's surface at `k` along it and `u` round it (the
/// side facing the viewer runs from 0.25 to 0.75).
fn surface(d: &Design, k: f32, u: f32) -> Color {
    let p = d.paint;
    let sleeve = paints_sleeve(d.style);
    match k {
        k if k < 0.08 && sleeve => pattern(d.style, p, u, 1.0 - k / 0.08),
        k if k < 0.08 => p.butt,
        k if k < 0.26 => {
            // The wrap: a fine linen texture.
            let thread = ((u * 90.0).sin() * (k * 400.0).sin()).abs();
            shade(p.accent, -0.12 * thread)
        }
        k if k < 0.27 => p.ring,
        k if k < 0.45 => pattern(d.style, p, u, 1.0 - (k - 0.27) / 0.18),
        k if k < 0.46 => p.ring,
        k if k < 0.97 => d.shaft,
        k if k < 0.99 => Color::new(0.95, 0.94, 0.9, 1.0),
        _ => Color::new(0.16, 0.32, 0.6, 1.0),
    }
}

/// A picture of the cue, `w` by `h` pixels, from its butt to `span` along
/// it (1: the whole cue), lit like a varnished cylinder.
pub fn image(d: &Design, span: f32, w: u16, h: u16) -> macroquad::texture::Image {
    let mut img = macroquad::texture::Image::gen_image_color(w, h, Color::new(0.0, 0.0, 0.0, 0.0));
    // 6.3 units fill half the picture's height.
    let unit = h as f32 / 2.0 / 6.3;
    for px in 0..w {
        let k = (px as f32 + 0.5) / w as f32 * span;
        let hw = half_width(k) * unit;
        for py in 0..h {
            let dy = py as f32 + 0.5 - h as f32 / 2.0;
            // Coverage of the edge, for a smooth outline.
            let cover = (hw - dy.abs() + 0.5).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let n = (dy / hw).clamp(-1.0, 1.0); // -1 at the top, 1 at the bottom
            let u = 0.25 + 0.5 * (n + 1.0) / 2.0;
            let base = surface(d, k, u);
            // Lambert from above, a specular streak and a dark rim.
            let facing = (1.0 - n * n).sqrt();
            let light = 0.55 + 0.45 * facing - 0.18 * n;
            let spec = 0.38 * (-((n + 0.42) / 0.11).powi(2)).exp();
            let c = Color::new(
                (base.r * light + spec).min(1.0),
                (base.g * light + spec).min(1.0),
                (base.b * light + spec).min(1.0),
                cover,
            );
            img.set_pixel(px as u32, py as u32, c);
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
        use snookered_proto::shop::{Category, ITEMS};
        for item in ITEMS.iter().filter(|i| i.category == Category::Cue) {
            let d = Design::of(item.code).expect("a cue");
            let img = image(&d, 0.62, 120, 32);
            assert_eq!(img.get_pixel(60, 0).a, 0.0, "{}", item.code);
            assert_eq!(img.get_pixel(20, 16).a, 1.0, "{}", item.code);
        }
    }
}
