//! The shop's pictures of the cloths and the ball sets (the table rendered,
//! see [`crate::studio`], and drawn in flat colours by the same camera
//! while it is made), and balls drawn shaded as spheres (lit from above, a
//! rim of shade, a glint, a soft shadow on the cloth under them).

use std::cell::RefCell;

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
}

thread_local! {
    static CACHE: RefCell<Cache> = const {
        RefCell::new(Cache { shade: None, glint: None, lamp: None, shadow: None })
    };
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

/// A corner of a table dressed in a cloth: the game's own table, rendered
/// (see [`crate::studio`]), fading in over a drawn one while it is made.
pub fn cloth(r: Rect, base: [u8; 3]) {
    let layers = crate::studio::cloth(r, base);
    show(r, layers, |r, radius| {
        let g = crate::studio::geometry();
        let ivory = Color::new(0.97, 0.95, 0.9, 1.0);
        let balls: Vec<Drawn> = g
            .balls()
            .iter()
            .map(|&(n, at)| {
                let base = match n {
                    0 => ivory,
                    8 => Color::new(0.05, 0.05, 0.06, 1.0),
                    _ => Color::new(0.98, 0.76, 0.08, 1.0),
                };
                Drawn { at, base, stripe: None, ivory, spot: n != 0 }
            })
            .collect();
        drawn(r, radius, base, &g.camera(r.w / r.h.max(1.0)), &balls);
    });
}

/// A ball set racked on the tournament cloth: rendered (see
/// [`crate::studio`]), fading in over a drawn one while it is made.
/// `colors`, `eight`, `ivory`: the set's (for the drawn one).
pub fn ball_set(r: Rect, code: &'static str, colors: [[u8; 3]; 7], eight: [u8; 3], ivory: [u8; 3]) {
    let layers = crate::studio::balls(r, code);
    show(r, layers, |r, radius| {
        let g = crate::studio::geometry();
        let rgb = |c: [u8; 3]| Color::from_rgba(c[0], c[1], c[2], 255);
        // (On a dark ground, the cue ball stays white.)
        let cue = if 0.3 * ivory[0] as f32 + 0.59 * ivory[1] as f32 + 0.11 * ivory[2] as f32 > 128.0
        {
            rgb(ivory)
        } else {
            rgb([246, 242, 228])
        };
        let ivory = rgb(ivory);
        let balls: Vec<Drawn> = g
            .rack()
            .iter()
            .map(|&(n, at)| {
                let n = n as usize;
                let (base, stripe) = match n {
                    0 => (cue, None),
                    8 => (rgb(eight), None),
                    1..=7 => (rgb(colors[n - 1]), None),
                    _ => (ivory, Some(rgb(colors[n - 9]))),
                };
                Drawn { at, base, stripe, ivory, spot: n != 0 }
            })
            .collect();
        drawn(r, radius, [24, 96, 48], &g.rack_camera(r.w / r.h.max(1.0)), &balls);
    });
}

/// The rendered picture's layers in `r`, its corners rounded; what
/// `stand_in` draws under it while it fades in (or for good where there
/// is none).
fn show(r: Rect, layers: crate::studio::Layers, stand_in: impl FnOnce(Rect, f32)) {
    let radius = (r.w.min(r.h) * 0.08).clamp(4.0, 9.0);
    let shown = layers.over.as_ref().map_or(0.0, |o| o.2);
    if shown < 1.0 {
        match &layers.under {
            Some((tex, src)) => picture(tex, r, *src, radius, 1.0),
            None => stand_in(r, radius),
        }
    }
    if let Some((tex, src, fade)) = &layers.over {
        picture(tex, r, *src, radius, *fade);
    }
}

/// `src` (pixels) of the picture `tex` in `r`, its corners rounded.
fn picture(tex: &Texture2D, r: Rect, src: Rect, radius: f32, fade: f32) {
    let size = tex.size();
    let color = Color::new(1.0, 1.0, 1.0, fade * crate::paint::alpha());
    let at = |p: Vec2| {
        let k = (p - vec2(r.x, r.y)) / vec2(r.w, r.h);
        let uv = vec2((src.x + src.w * k.x) / size.x, (src.y + src.h * k.y) / size.y);
        Vertex::new(p.x, p.y, 0.0, uv.x, uv.y, color)
    };
    let mut vertices = vec![at(vec2(r.x + r.w / 2.0, r.y + r.h / 2.0))];
    vertices.extend(rounded(r, radius).into_iter().map(at));
    let n = vertices.len() as u16;
    let indices = (1..n).flat_map(|k| [0, k, if k + 1 < n { k + 1 } else { 1 }]).collect();
    draw_mesh(&Mesh { vertices, indices, texture: Some(tex.clone()) });
}

/// A ball of the drawn stand-in: its centre (world) and colours (see
/// [`ball`]).
struct Drawn {
    at: Vec3,
    base: Color,
    stripe: Option<Color>,
    ivory: Color,
    spot: bool,
}

/// The four sides of the table: along it, out from its middle, and its
/// half length and the half width of the table across it.
fn sides(half: Vec2) -> [(Vec2, Vec2, f32, f32); 4] {
    [
        (Vec2::X, Vec2::Y, half.x, half.y),
        (Vec2::X, -Vec2::Y, half.x, half.y),
        (Vec2::Y, Vec2::X, half.y, half.x),
        (Vec2::Y, -Vec2::X, half.y, half.x),
    ]
}

/// The drawn stand-in for a rendered picture (while it is made, and where
/// pictures can't be drawn into): the same table seen by the same camera,
/// in flat colours under a pool of light: the dark room, the wooden rails
/// and the cushions, the pockets, the cloth `base`, and `balls` shaded as
/// spheres.
fn drawn(r: Rect, radius: f32, base: [u8; 3], cam: &crate::gfx::SceneCamera, balls: &[Drawn]) {
    use crate::scene::{CUSHION_DEPTH, CUSHION_TOP, RAIL_TOP};
    use crate::studio::on_table;
    let g = crate::studio::geometry();
    let view = Mat4::perspective_rh_gl(cam.fovy, r.w / r.h.max(1.0), 0.03, 40.0)
        * Mat4::look_at_rh(cam.eye, cam.target, cam.up);
    let edge = rounded(r, radius);
    // A flat shape on the table (table coordinates at a height).
    let fill = |points: &[(Vec2, f32)], color: Color| {
        let clip: Vec<Vec4> =
            points.iter().map(|&(p, h)| view * on_table(p, h).extend(1.0)).collect();
        let flat = clipped(&near(&clip), &edge, r);
        for k in 1..flat.len().saturating_sub(1) {
            draw_triangle(flat[0], flat[k], flat[k + 1], color);
        }
    };
    let rail = g.rail;
    // A band along each side of the table, from `a` to `b` out from the
    // cushion's nose, at `height`.
    let bands = |a: f32, b: f32, height: f32, color: Color| {
        for (along, out, len, across) in sides(g.half) {
            let len = len + rail;
            let (p, q) = (out * (across + a), out * (across + b));
            let quad = [p - along * len, p + along * len, q + along * len, q - along * len];
            fill(&quad.map(|v| (v, height)), color);
        }
    };
    // The cloth under the lamps: warmer and lighter than its dye.
    let lit = |k: f32| {
        let c = base.map(|v| v as f32 / 255.0);
        let warm = [1.0, 0.9, 0.78];
        let f = |i: usize| ((c[i] * 1.35 + 0.035) * warm[i] * k).min(1.0);
        Color::new(f(0), f(1), f(2), 1.0)
    };
    let dark = Color::new(0.014, 0.012, 0.011, 1.0);
    // The dark room, the cloth, round it the cushions and the rails.
    crate::ui::rrect(r, radius, dark);
    let h = g.half;
    let bed = [vec2(-h.x, -h.y), vec2(h.x, -h.y), vec2(h.x, h.y), vec2(-h.x, h.y)];
    fill(&bed.map(|v| (v, 0.0)), lit(1.0));
    bands(CUSHION_DEPTH, rail, RAIL_TOP, Color::new(0.30, 0.12, 0.07, 1.0));
    bands(0.0, CUSHION_DEPTH, CUSHION_TOP, lit(0.92));
    // The cushions' noses that face the camera, in their shade.
    let eye = vec2(cam.eye.x, -cam.eye.z);
    for (along, out, len, across) in sides(h) {
        let p = out * across;
        if (eye - p).dot(out) < 0.0 {
            let (a, b) = (p - along * len, p + along * len);
            fill(&[(a, 0.0), (b, 0.0), (b, CUSHION_TOP), (a, CUSHION_TOP)], lit(0.62));
        }
    }
    // The pockets: their plates on the rails, the leather lips, the holes.
    let disc = |c: Vec2, rad: f32, height: f32, color: Color| {
        let ring: Vec<(Vec2, f32)> = (0..28)
            .map(|k| {
                let a = k as f32 / 28.0 * std::f32::consts::TAU;
                (c + vec2(a.cos(), a.sin()) * rad, height)
            })
            .collect();
        fill(&ring, color);
    };
    for &(c, rad) in &g.holes {
        let out = c.normalize_or_zero() * 0.012;
        disc(c + out, rad * 1.9, RAIL_TOP, Color::new(0.55, 0.40, 0.25, 1.0));
        disc(c, rad * 1.16, RAIL_TOP + 0.001, Color::new(0.16, 0.09, 0.05, 1.0));
        disc(c, rad, 0.0, Color::new(0.02, 0.015, 0.012, 1.0));
    }
    // The room beyond the rails.
    bands(rail, 6.0, RAIL_TOP, dark);
    // The balls, the farthest first.
    let mut order: Vec<&Drawn> = balls.iter().collect();
    order.sort_by(|a, b| b.at.distance(cam.eye).total_cmp(&a.at.distance(cam.eye)));
    let right = (cam.target - cam.eye).cross(cam.up).normalize_or_zero();
    let to_rect = |p: Vec3| {
        let c = view * p.extend(1.0);
        let n = c.truncate() / c.w.max(1e-4);
        vec2(r.x + (n.x + 1.0) / 2.0 * r.w, r.y + (1.0 - n.y) / 2.0 * r.h)
    };
    for b in order {
        let c = to_rect(b.at);
        let rad = (to_rect(b.at + right * g.ball_radius) - c).length();
        if r.contains(c) && rad >= 1.0 {
            ball(c, rad, b.base, b.stripe, b.ivory, b.spot && rad > 3.0);
        }
    }
    // The pool of light, and the dark round it.
    let lamp = with(|cache| cache.lamp.get_or_insert_with(|| texture(lamp_pixels())).clone());
    draw_tex(&lamp, r, tint());
}

/// The corners of `r` rounded by `radius`: a convex outline.
fn rounded(r: Rect, radius: f32) -> Vec<Vec2> {
    let mut out = Vec::with_capacity(28);
    let corners = [
        (vec2(r.x + r.w - radius, r.y + r.h - radius), 0.0),
        (vec2(r.x + radius, r.y + r.h - radius), 0.5),
        (vec2(r.x + radius, r.y + radius), 1.0),
        (vec2(r.x + r.w - radius, r.y + radius), 1.5),
    ];
    for (o, start) in corners {
        for k in 0..=6 {
            let a = (start + 0.5 * k as f32 / 6.0) * std::f32::consts::PI;
            out.push(o + vec2(a.cos(), a.sin()) * radius);
        }
    }
    out
}

/// A shape in the camera's clip space, without what is behind the camera.
fn near(points: &[Vec4]) -> Vec<Vec4> {
    const W: f32 = 0.03;
    let mut out = Vec::with_capacity(points.len() + 2);
    for (k, &a) in points.iter().enumerate() {
        let b = points[(k + 1) % points.len()];
        if a.w >= W {
            out.push(a);
        }
        if (a.w >= W) != (b.w >= W) {
            out.push(a + (b - a) * ((W - a.w) / (b.w - a.w)));
        }
    }
    out
}

/// A shape in clip space, seen in `r` and cut to the convex outline `edge`
/// (counter-clockwise or clockwise, as [`rounded`] makes it).
fn clipped(points: &[Vec4], edge: &[Vec2], r: Rect) -> Vec<Vec2> {
    let mut poly: Vec<Vec2> = points
        .iter()
        .map(|c| {
            let n = c.truncate() / c.w;
            vec2(r.x + (n.x + 1.0) / 2.0 * r.w, r.y + (1.0 - n.y) / 2.0 * r.h)
        })
        .collect();
    let mid = vec2(r.x + r.w / 2.0, r.y + r.h / 2.0);
    for (k, &a) in edge.iter().enumerate() {
        if poly.len() < 3 {
            break;
        }
        let b = edge[(k + 1) % edge.len()];
        let d = b - a;
        // Which side of the edge is inside: the side the middle is on.
        let sign = d.perp_dot(mid - a).signum();
        let inside = |p: Vec2| d.perp_dot(p - a) * sign >= 0.0;
        let mut out = Vec::with_capacity(poly.len() + 2);
        for (j, &p) in poly.iter().enumerate() {
            let q = poly[(j + 1) % poly.len()];
            if inside(p) {
                out.push(p);
            }
            if inside(p) != inside(q) {
                let (dp, dq) = (d.perp_dot(p - a), d.perp_dot(q - a));
                out.push(p + (q - p) * (dp / (dp - dq)));
            }
        }
        poly = out;
    }
    poly
}
