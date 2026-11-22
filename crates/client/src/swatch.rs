//! The shop's small pictures of the table things, lit as on the table: the
//! cloth in its own woven texture (the one the table wears) under a lamp,
//! in wooden rails; balls shaded as spheres (lit from above, a rim of
//! shade, a glint, a soft shadow on the cloth under them).

use std::cell::RefCell;
use std::collections::HashMap;

use macroquad::prelude::*;

use crate::paint::{alpha as opacity, draw_circle, draw_triangle};
use crate::textures::Pixels;

/// Where the light comes from (the lamp above, a little to the front left).
const LIGHT: Vec3 = Vec3::new(-0.42, -0.58, 0.7);
const SIDE: u16 = 128;

struct Cache {
    shade: Option<Texture2D>,
    glint: Option<Texture2D>,
    lamp: Option<Texture2D>,
    shadow: Option<Texture2D>,
    felts: HashMap<[u8; 3], Texture2D>,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache {
        shade: None,
        glint: None,
        lamp: None,
        shadow: None,
        felts: HashMap::new(),
    });
}

fn texture(p: Pixels) -> Texture2D {
    let tex = Texture2D::from_rgba8(p.width, p.height, &p.rgba);
    tex.set_filter(FilterMode::Linear);
    tex
}

/// A disc's pixels, from what each point of the sphere gives (RGBA), with
/// its edge smoothed over a pixel.
fn sphere(f: impl Fn(Vec3) -> [f32; 4]) -> Pixels {
    let n = SIDE as usize;
    let mut rgba = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let px = (x as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let py = (y as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let d = (px * px + py * py).sqrt();
            let cover = ((1.0 - d) * n as f32 / 2.0 + 0.5).clamp(0.0, 1.0);
            let normal = vec3(px, py, (1.0 - (px * px + py * py).min(1.0)).sqrt());
            let [r, g, b, a] = f(normal);
            rgba.extend([r, g, b, a * cover].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    Pixels { width: SIDE, height: SIDE, rgba }
}

/// The shade over a ball's colour: dark where the light doesn't reach, and
/// a rim of shade all round.
fn shade_pixels() -> Pixels {
    let light = LIGHT.normalize();
    sphere(|n| {
        let lit = n.dot(light).max(0.0);
        let rim = (1.0 - n.z).powf(2.2);
        let dark = ((1.0 - lit) * 0.62 + rim * 0.35).min(0.85);
        [0.0, 0.0, 0.0, dark]
    })
}

/// The glint: the lamp seen in the varnish, and a faint sheen round it.
fn glint_pixels() -> Pixels {
    let light = LIGHT.normalize();
    let view = Vec3::Z;
    let half = (light + view).normalize();
    sphere(|n| {
        let spec = n.dot(half).max(0.0).powf(60.0);
        let sheen = n.dot(half).max(0.0).powf(8.0) * 0.18;
        [1.0, 1.0, 1.0, (spec * 0.95 + sheen).min(1.0)]
    })
}

/// The lamp's pool of light on the cloth, and the dark round it.
fn lamp_pixels() -> Pixels {
    let n = SIDE as usize;
    let mut rgba = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let px = (x as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let py = (y as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let d = (px * px * 0.8 + py * py).sqrt();
            // Light in the middle (white), shade towards the edges (black).
            let light = (1.0 - d * 1.3).max(0.0).powf(1.5) * 0.22;
            let dark = ((d - 0.55) / 0.75).clamp(0.0, 1.0).powf(1.4) * 0.55;
            if light > dark {
                rgba.extend([255, 255, 255, (light * 255.0) as u8]);
            } else {
                rgba.extend([0, 0, 0, (dark * 255.0) as u8]);
            }
        }
    }
    Pixels { width: SIDE, height: SIDE, rgba }
}

fn with<R>(f: impl FnOnce(&mut Cache) -> R) -> R {
    CACHE.with(|c| f(&mut c.borrow_mut()))
}

fn tint() -> Color {
    Color::new(1.0, 1.0, 1.0, opacity())
}

fn draw_tex(tex: &Texture2D, r: Rect, color: Color) {
    draw_texture_ex(
        tex,
        r.x,
        r.y,
        color,
        DrawTextureParams { dest_size: Some(vec2(r.w, r.h)), ..Default::default() },
    );
}

/// A ball of `base` colour (with a band of `stripe` round it for a striped
/// one, on `ivory`), shaded as a sphere, with its shadow on the cloth.
/// `spot`: the white disc where the number is.
pub fn ball(c: Vec2, rad: f32, base: Color, stripe: Option<Color>, ivory: Color, spot: bool) {
    let (shade, glint, shadow) = with(|cache| {
        (
            cache.shade.get_or_insert_with(|| texture(shade_pixels())).clone(),
            cache.glint.get_or_insert_with(|| texture(glint_pixels())).clone(),
            cache.shadow.get_or_insert_with(|| texture(crate::textures::shadow())).clone(),
        )
    });
    // The shadow, cast down and right of the lamp.
    let s = Rect::new(
        c.x - rad * 1.05 + rad * 0.35,
        c.y - rad * 0.8 + rad * 0.55,
        rad * 2.1,
        rad * 1.7,
    );
    draw_tex(&shadow, s, Color::new(1.0, 1.0, 1.0, 0.75 * opacity()));
    match stripe {
        None => draw_circle(c.x, c.y, rad, base),
        Some(band) => {
            draw_circle(c.x, c.y, rad, ivory);
            // The band: the part of the disc within ±55% of the radius of
            // its middle, built from thin slices that follow the edge.
            let half = rad * 0.55;
            let steps = 10;
            let at = |k: usize| {
                let y = -half + 2.0 * half * k as f32 / steps as f32;
                (y, (rad * rad - y * y).max(0.0).sqrt() - 0.3)
            };
            for k in 0..steps {
                let ((y0, w0), (y1, w1)) = (at(k), at(k + 1));
                let (a, b) = (vec2(c.x - w0, c.y + y0), vec2(c.x + w0, c.y + y0));
                let (d, e) = (vec2(c.x - w1, c.y + y1), vec2(c.x + w1, c.y + y1));
                draw_triangle(a, b, e, band);
                draw_triangle(a, e, d, band);
            }
        }
    }
    if spot {
        // The number's spot, turned a little towards the light.
        draw_circle(c.x - rad * 0.12, c.y - rad * 0.14, rad * 0.4, ivory);
    }
    let r = Rect::new(c.x - rad, c.y - rad, rad * 2.0, rad * 2.0);
    draw_tex(&shade, r, tint());
    draw_tex(&glint, r, tint());
}

/// A piece of cloth of colour `base`, woven, lit by the lamp over the
/// middle of `r` (the cushions' shade along its top and left edges).
pub fn felt(r: Rect, base: [u8; 3]) {
    let (felt, lamp) = with(|cache| {
        let felt = cache
            .felts
            .entry(base)
            .or_insert_with(|| texture(crate::textures::felt_sized(base, 128)))
            .clone();
        (felt, cache.lamp.get_or_insert_with(|| texture(lamp_pixels())).clone())
    });
    draw_tex(&felt, r, tint());
    draw_tex(&lamp, r, tint());
    // The cushions' shade on the cloth.
    let shade = Color::new(0.0, 0.0, 0.0, 0.28);
    crate::paint::draw_rectangle(r.x, r.y, r.w, 3.0, shade);
    crate::paint::draw_rectangle(r.x, r.y, 3.0, r.h, Color::new(0.0, 0.0, 0.0, 0.16));
}

/// A corner of a table dressed in a cloth: wooden rails, the cloth under
/// the lamp, a pocket, and two balls on it.
pub fn cloth(r: Rect, base: [u8; 3]) {
    let inner = table_corner(r, base);
    let rad = (inner.h * 0.11).clamp(6.0, 11.0);
    let ivory = Color::new(0.97, 0.95, 0.9, 1.0);
    ball(vec2(inner.x + inner.w * 0.66, inner.y + inner.h * 0.58), rad, ivory, None, ivory, false);
    ball(
        vec2(inner.x + inner.w * 0.34, inner.y + inner.h * 0.46),
        rad,
        Color::new(0.82, 0.1, 0.08, 1.0),
        None,
        ivory,
        false,
    );
}

/// Rails, a pocket and `base` cloth in `r`; returns the cloth's rectangle.
pub fn table_corner(r: Rect, base: [u8; 3]) -> Rect {
    // The rails: wood, lit from above.
    let wood_top = Color::new(0.36, 0.18, 0.09, 1.0);
    let wood_bottom = Color::new(0.17, 0.075, 0.035, 1.0);
    crate::ui::rrect_gradient(r, 9.0, wood_top, wood_bottom);
    crate::ui::top_light(r, 9.0, 0.14);
    let inner = Rect::new(r.x + 10.0, r.y + 10.0, r.w - 20.0, r.h - 20.0);
    felt(inner, base);
    // The diamonds on the rails.
    for k in 1..4 {
        let x = inner.x + inner.w * k as f32 / 4.0;
        draw_circle(x, r.y + 5.0, 1.4, Color::new(0.93, 0.87, 0.74, 0.8));
        draw_circle(x, r.y + r.h - 5.0, 1.4, Color::new(0.93, 0.87, 0.74, 0.8));
    }
    // A corner pocket, top left: its leather rim, then the dark hole.
    let pocket = vec2(inner.x + 1.5, inner.y + 1.5);
    draw_circle(pocket.x, pocket.y, 8.5, Color::new(0.11, 0.06, 0.035, 1.0));
    draw_circle(pocket.x, pocket.y, 6.5, Color::new(0.015, 0.01, 0.008, 1.0));
    inner
}
