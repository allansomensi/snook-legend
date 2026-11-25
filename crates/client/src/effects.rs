//! Winning effects (sold in the shop): what happens over the pocket when a
//! player wins the game on the 8. Drawn in 2D over the table, from a seed and
//! the age of the effect (no state per particle).

use macroquad::prelude::*;
use snook_legend_proto::shop::{EffectKind, Look};

/// How long an effect plays, in seconds.
pub const LENGTH: f32 = 2.6;

fn hash(i: u32, k: u32) -> f32 {
    let mut h = i.wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B);
    h = (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D);
    h = (h ^ (h >> 12)).wrapping_mul(0x297A_2D39);
    (h ^ (h >> 15)) as f32 / u32::MAX as f32
}

fn rgb(c: [u8; 3], a: f32) -> Color {
    Color::from_rgba(c[0], c[1], c[2], (a.clamp(0.0, 1.0) * 255.0) as u8)
}

/// Draws an effect started `age` seconds ago at `origin` (the pocket on
/// screen), `scale` pixels per unit (bigger on a bigger screen).
pub fn draw_at(kind: EffectKind, color: [u8; 3], origin: Vec2, age: f32, scale: f32) {
    draw_clipped(kind, color, origin, age, scale, None)
}

/// [`draw_at`], keeping every particle inside `clip`.
fn draw_clipped(
    kind: EffectKind,
    color: [u8; 3],
    origin: Vec2,
    age: f32,
    scale: f32,
    clip: Option<Rect>,
) {
    let inside = |p: Vec2| clip.is_none_or(|r| r.contains(p));
    let draw_circle = |x: f32, y: f32, r: f32, c: Color| {
        if inside(vec2(x, y)) {
            crate::paint::draw_circle(x, y, r, c)
        }
    };
    let draw_line = |x1: f32, y1: f32, x2: f32, y2: f32, t: f32, c: Color| {
        if inside(vec2(x1, y1)) && inside(vec2(x2, y2)) {
            crate::paint::draw_line(x1, y1, x2, y2, t, c)
        }
    };
    let draw_rectangle = |x: f32, y: f32, w: f32, h: f32, c: Color| {
        if inside(vec2(x, y)) && inside(vec2(x + w, y + h)) {
            crate::paint::draw_rectangle(x, y, w, h, c)
        }
    };
    let draw_circle_lines = |x: f32, y: f32, r: f32, t: f32, c: Color| {
        let ok = clip.is_none_or(|cr| {
            x - r >= cr.x && x + r <= cr.x + cr.w && y - r >= cr.y && y + r <= cr.y + cr.h
        });
        if ok {
            crate::paint::draw_circle_lines(x, y, r, t, c)
        }
    };
    let draw_ellipse = |x: f32, y: f32, w: f32, h: f32, c: Color| {
        if inside(vec2(x, y)) {
            crate::paint::draw_ellipse(x, y, w, h, c)
        }
    };
    let draw_triangle = |a: Vec2, b: Vec2, c: Vec2, color: Color| {
        if inside(a) && inside(b) && inside(c) {
            crate::paint::draw_triangle(a, b, c, color)
        }
    };
    let t = age / LENGTH;
    if !(0.0..1.0).contains(&t) {
        return;
    }
    let fade = 1.0 - smooth(0.7, 1.0, t);
    match kind {
        EffectKind::None => {}
        EffectKind::Sparks => {
            for i in 0..48 {
                let a = hash(i, 1) * std::f32::consts::TAU;
                let speed = (0.6 + hash(i, 2) * 1.2) * scale;
                let life = 0.5 + hash(i, 3) * 0.5;
                if t > life {
                    continue;
                }
                let k = t / life;
                let d = vec2(a.cos(), a.sin() - 0.6) * speed * k + vec2(0.0, 0.9 * scale * k * k);
                let p = origin + d;
                let tail = p - vec2(a.cos(), a.sin() - 0.6) * scale * 0.08;
                let c = rgb(color, (1.0 - k) * 1.2);
                draw_line(tail.x, tail.y, p.x, p.y, 2.0, c);
                draw_circle(p.x, p.y, 1.8, Color::new(1.0, 1.0, 1.0, (1.0 - k) * 0.9));
            }
        }
        EffectKind::Confetti => {
            let palette = [
                [255, 90, 90],
                [255, 210, 80],
                [90, 200, 255],
                [120, 230, 140],
                [220, 120, 255],
                color,
            ];
            for i in 0..70 {
                let x0 = (hash(i, 1) - 0.5) * 3.0 * scale;
                let fall = (0.4 + hash(i, 2) * 0.8) * scale;
                let sway = (age * (2.0 + hash(i, 3) * 3.0) + hash(i, 4) * 6.0).sin() * 0.12 * scale;
                let p = origin + vec2(x0 + sway, -1.4 * scale + fall * t * 2.2);
                let spin = (age * 6.0 + hash(i, 5) * 6.0).sin();
                let c = rgb(palette[i as usize % palette.len()], fade);
                let (w, h) = (0.05 * scale * spin.abs().max(0.2), 0.03 * scale);
                draw_rectangle(p.x - w / 2.0, p.y - h / 2.0, w, h, c);
            }
        }
        EffectKind::Shockwave => {
            for k in 0..3 {
                let tk = t * 1.4 - k as f32 * 0.18;
                if !(0.0..1.0).contains(&tk) {
                    continue;
                }
                let r = tk * 1.1 * scale;
                draw_circle_lines(
                    origin.x,
                    origin.y,
                    r,
                    3.0 * (1.0 - tk) + 1.0,
                    rgb(color, 1.0 - tk),
                );
                draw_circle_lines(
                    origin.x,
                    origin.y,
                    r * 0.96,
                    1.5,
                    Color::new(1.0, 1.0, 1.0, (1.0 - tk) * 0.6),
                );
            }
        }
        EffectKind::Flames => {
            for i in 0..60 {
                let cycle = 0.6 + hash(i, 1) * 0.5;
                let k = ((age + hash(i, 2) * cycle) / cycle).fract();
                let x =
                    (hash(i, 3) - 0.5) * 0.35 * scale + (age * 9.0 + i as f32).sin() * 0.03 * scale;
                let p = origin + vec2(x * (1.0 - k * 0.6), -k * 0.75 * scale);
                let size = (1.0 - k) * 0.07 * scale + 1.0;
                let hot = [255, (230.0 - k * 160.0) as u8, (120.0 - k * 120.0).max(0.0) as u8];
                let c = rgb(if k < 0.3 { [255, 240, 180] } else { hot }, (1.0 - k) * fade);
                draw_circle(p.x, p.y, size, c);
            }
            let _ = color;
        }
        EffectKind::Coins => {
            for i in 0..26 {
                let a = (hash(i, 1) - 0.5) * 1.6;
                let speed = (1.3 + hash(i, 2) * 0.9) * scale;
                let k = t * 1.2;
                let p =
                    origin + vec2(a.sin() * speed * k, -a.cos() * speed * k + 1.8 * scale * k * k);
                let squash = (age * 8.0 + hash(i, 3) * 6.0).cos().abs().max(0.15);
                let r = 0.05 * scale;
                let c = rgb(color, fade);
                draw_ellipse(p.x, p.y, r * squash, r, c);
                draw_ellipse(p.x, p.y, r * squash * 0.6, r * 0.6, rgb([255, 240, 160], fade));
            }
        }
        EffectKind::Fireworks => {
            for burst in 0..3u32 {
                let start = burst as f32 * 0.35;
                let local = age - start;
                if local < 0.0 {
                    continue;
                }
                let target = origin
                    + vec2(
                        (hash(burst, 9) - 0.5) * 1.6 * scale,
                        -(0.9 + hash(burst, 8) * 0.5) * scale,
                    );
                let rise = 0.45;
                if local < rise {
                    let k = local / rise;
                    let p = origin.lerp(target, k);
                    draw_circle(p.x, p.y, 2.2, Color::new(1.0, 0.95, 0.8, 1.0));
                    continue;
                }
                let k = ((local - rise) / 1.3).min(1.0);
                let hue = [[255, 120, 120], [130, 220, 255], color][burst as usize];
                for i in 0..36 {
                    let a = i as f32 / 36.0 * std::f32::consts::TAU + hash(i, burst) * 0.2;
                    let d =
                        vec2(a.cos(), a.sin()) * k * 0.45 * scale + vec2(0.0, k * k * 0.25 * scale);
                    let p = target + d;
                    draw_circle(p.x, p.y, 2.0 * (1.0 - k) + 0.6, rgb(hue, (1.0 - k) * 1.3));
                }
            }
        }
        EffectKind::Lightning => {
            // Three strikes down onto the pocket, each a jagged bolt with a
            // fork, a white flash and a glow where it lands.
            for strike in 0..3u32 {
                let start = strike as f32 * 0.55;
                let local = age - start;
                if !(0.0..0.5).contains(&local) {
                    continue;
                }
                let k = local / 0.5;
                let alpha = (1.0 - k).powf(0.6) * fade;
                let top = origin
                    + vec2(
                        (hash(strike, 1) - 0.5) * 0.9 * scale,
                        -(1.1 + hash(strike, 2) * 0.3) * scale,
                    );
                let mut prev = top;
                let steps = 9;
                for i in 1..=steps {
                    let f = i as f32 / steps as f32;
                    let jag = if i == steps {
                        0.0
                    } else {
                        (hash(strike * 31 + i, 3) - 0.5) * 0.22 * scale
                    };
                    let p = top.lerp(origin, f) + vec2(jag, 0.0);
                    draw_line(prev.x, prev.y, p.x, p.y, 7.0, rgb(color, alpha * 0.25));
                    draw_line(prev.x, prev.y, p.x, p.y, 3.0, rgb(color, alpha));
                    draw_line(prev.x, prev.y, p.x, p.y, 1.2, Color::new(1.0, 1.0, 1.0, alpha));
                    if i == 4 {
                        // A fork off to one side.
                        let side = if hash(strike, 5) > 0.5 { 1.0 } else { -1.0 };
                        let end = p + vec2(side * 0.3 * scale, 0.28 * scale);
                        let mid = p.lerp(end, 0.5) + vec2(-side * 0.05 * scale, 0.0);
                        draw_line(p.x, p.y, mid.x, mid.y, 1.6, rgb(color, alpha * 0.8));
                        draw_line(mid.x, mid.y, end.x, end.y, 1.2, rgb(color, alpha * 0.6));
                    }
                    prev = p;
                }
                draw_circle(origin.x, origin.y, 0.18 * scale * (1.0 - k), rgb(color, alpha * 0.35));
                draw_circle(
                    origin.x,
                    origin.y,
                    0.08 * scale * (1.0 - k),
                    Color::new(1.0, 1.0, 1.0, alpha),
                );
            }
        }
        EffectKind::Snow => {
            // Flakes drifting down from above, swaying, over the whole table.
            for i in 0..80 {
                let x0 = (hash(i, 1) - 0.5) * 3.0 * scale;
                let speed = (0.35 + hash(i, 2) * 0.35) * scale;
                let delay = hash(i, 3) * 0.5;
                let k = (t - delay * 0.3).max(0.0);
                let sway = (age * (1.2 + hash(i, 4) * 1.5) + hash(i, 5) * 6.0).sin() * 0.08 * scale;
                let p = origin + vec2(x0 + sway, -1.3 * scale + speed * k * 3.2);
                let size = 1.2 + hash(i, 6) * 2.4;
                let c = rgb(color, fade * (0.6 + hash(i, 7) * 0.4));
                draw_circle(p.x, p.y, size, c);
                if size > 2.6 {
                    // The big ones show their six arms.
                    for a in 0..3 {
                        let ang = a as f32 * std::f32::consts::PI / 3.0 + age;
                        let d = vec2(ang.cos(), ang.sin()) * size * 1.8;
                        draw_line(p.x - d.x, p.y - d.y, p.x + d.x, p.y + d.y, 0.8, c);
                    }
                }
            }
        }
        EffectKind::Hearts => {
            // Hearts floating up and swaying, growing as they rise.
            for i in 0..26 {
                let cycle = 1.2 + hash(i, 1) * 0.8;
                let k = ((age + hash(i, 2) * cycle) / cycle).min(1.0);
                if age < hash(i, 2) * 0.8 {
                    continue;
                }
                let x =
                    (hash(i, 3) - 0.5) * 1.2 * scale + (age * 3.0 + i as f32).sin() * 0.08 * scale;
                let p = origin + vec2(x * k, -k * 1.1 * scale);
                let s = (0.03 + 0.05 * k) * scale * (0.7 + hash(i, 4) * 0.6);
                let c =
                    rgb(if i % 4 == 0 { [255, 255, 255] } else { color }, (1.0 - k) * 1.4 * fade);
                draw_circle(p.x - s * 0.5, p.y - s * 0.2, s * 0.55, c);
                draw_circle(p.x + s * 0.5, p.y - s * 0.2, s * 0.55, c);
                draw_triangle(
                    vec2(p.x - s * 1.02, p.y - s * 0.05),
                    vec2(p.x + s * 1.02, p.y - s * 0.05),
                    vec2(p.x, p.y + s * 1.1),
                    c,
                );
            }
        }
        EffectKind::Vortex => {
            // Streaks of light spiralling into the pocket, faster as they go
            // in, and a dark heart that swells and closes.
            for i in 0..70 {
                let a0 = hash(i, 1) * std::f32::consts::TAU;
                let r0 = (0.4 + hash(i, 2) * 0.8) * scale;
                let k = ((t * 1.6 + hash(i, 3)) % 1.0).powf(1.4);
                let r = r0 * (1.0 - k);
                let a = a0 + k * 7.0;
                let p = origin + vec2(a.cos() * r, a.sin() * r * 0.55);
                let back = origin + vec2((a - 0.25).cos() * r * 1.06, (a - 0.25).sin() * r * 0.58);
                let mix = if i % 3 == 0 { [255, 255, 255] } else { color };
                draw_line(back.x, back.y, p.x, p.y, 1.6, rgb(mix, k * fade * 1.2));
            }
            let heart = (t * 3.0).min(1.0) * (1.0 - smooth(0.75, 1.0, t)) * 0.13 * scale;
            draw_ellipse(origin.x, origin.y, heart * 1.6, heart, rgb(color, 0.35 * fade));
            draw_ellipse(
                origin.x,
                origin.y,
                heart,
                heart * 0.6,
                Color::new(0.0, 0.0, 0.0, 0.9 * fade),
            );
        }
        EffectKind::Stars => {
            // Shooting stars streaking out of the pocket, with long tails
            // and a twinkle where they fade.
            for i in 0..14 {
                let start = hash(i, 1) * 0.6;
                let local = (age - start) / 1.2;
                if !(0.0..1.0).contains(&local) {
                    continue;
                }
                let a = -std::f32::consts::FRAC_PI_2 + (hash(i, 2) - 0.5) * 2.2;
                let dir = vec2(a.cos(), a.sin());
                let reach = (0.8 + hash(i, 3) * 0.6) * scale;
                let k = 1.0 - (1.0 - local).powi(3);
                let head = origin + dir * reach * k;
                for seg in 0..8 {
                    let f0 = seg as f32 / 8.0;
                    let f1 = (seg + 1) as f32 / 8.0;
                    let p0 = head - dir * reach * 0.35 * f0;
                    let p1 = head - dir * reach * 0.35 * f1;
                    let alpha = (1.0 - f0) * (1.0 - local) * fade;
                    draw_line(p0.x, p0.y, p1.x, p1.y, 2.6 * (1.0 - f0) + 0.5, rgb(color, alpha));
                }
                let tw = (age * 20.0 + i as f32).sin().abs();
                draw_circle(
                    head.x,
                    head.y,
                    2.2 + tw * 1.2,
                    Color::new(1.0, 1.0, 1.0, (1.0 - local) * fade),
                );
            }
        }
        EffectKind::Crown => {
            // VIP's own: rays burst from the pocket, then a golden crown
            // rises out of it and hangs there, set with jewels, glinting,
            // in a ring of sparkles.
            let rise = smooth(0.05, 0.4, t);
            let center = origin + vec2(0.0, -0.55 * scale * rise);
            let spin = age * 0.5;
            for i in 0..16 {
                let a = i as f32 / 16.0 * std::f32::consts::TAU + spin;
                let len = (0.5 + 0.15 * (age * 3.0 + i as f32).sin()) * scale * rise.max(0.15);
                let d = vec2(a.cos(), a.sin());
                let alpha = 0.22 * fade * (1.0 - smooth(0.0, 1.0, (t - 0.5).max(0.0) * 2.0) * 0.5);
                draw_triangle(
                    center + vec2(-d.y, d.x) * 0.03 * scale,
                    center - vec2(-d.y, d.x) * 0.03 * scale,
                    center + d * len,
                    rgb(color, alpha),
                );
            }
            let w = 0.3 * scale * (0.6 + 0.4 * rise);
            let h = w * 0.62;
            let gold = rgb(color, fade);
            let deep = rgb([color[0] / 2 + 40, color[1] / 2 + 10, color[2] / 3], fade);
            let base_y = center.y + h * 0.35;
            draw_rectangle(center.x - w / 2.0, base_y - h * 0.22, w, h * 0.22, deep);
            draw_rectangle(center.x - w / 2.0, base_y - h * 0.3, w, h * 0.1, gold);
            for k in 0..5 {
                let x = center.x - w / 2.0 + w * k as f32 / 4.0;
                let tip = vec2(x, center.y - h * if k % 2 == 0 { 0.65 } else { 0.4 });
                draw_triangle(
                    vec2(x - w * 0.14, base_y - h * 0.28),
                    vec2(x + w * 0.14, base_y - h * 0.28),
                    tip,
                    gold,
                );
                draw_circle(tip.x, tip.y - 2.0, w * 0.035 + 1.0, Color::new(1.0, 0.98, 0.9, fade));
            }
            let jewels = [[230, 30, 60], [40, 120, 255], [30, 200, 120]];
            for (k, j) in jewels.iter().enumerate() {
                let x = center.x + (k as f32 - 1.0) * w * 0.3;
                draw_circle(x, base_y - h * 0.11, w * 0.05, rgb(*j, fade));
                draw_circle(
                    x - w * 0.015,
                    base_y - h * 0.13,
                    w * 0.015,
                    Color::new(1.0, 1.0, 1.0, 0.8 * fade),
                );
            }
            // A glint sweeping across the crown.
            let sweep = (age * 0.8).fract();
            let gx = center.x - w / 2.0 + w * sweep;
            draw_line(
                gx,
                base_y - h * 0.3,
                gx + w * 0.08,
                center.y - h * 0.5,
                2.0,
                Color::new(1.0, 1.0, 1.0, 0.5 * fade),
            );
            for i in 0..24 {
                let a = hash(i, 1) * std::f32::consts::TAU + age * (0.4 + hash(i, 2));
                let r = (0.35 + hash(i, 3) * 0.25) * scale * rise;
                let p = center + vec2(a.cos() * r, a.sin() * r * 0.7);
                let tw = ((age * 6.0 + hash(i, 4) * 6.0).sin() * 0.5 + 0.5) * fade;
                let s = 1.0 + tw * 2.5;
                draw_line(p.x - s, p.y, p.x + s, p.y, 1.0, Color::new(1.0, 0.95, 0.8, tw));
                draw_line(p.x, p.y - s, p.x, p.y + s, 1.0, Color::new(1.0, 0.95, 0.8, tw));
            }
        }
    }
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A looping preview in the shop.
pub fn draw_preview(kind: EffectKind, color: [u8; 3], r: Rect, time: f32) {
    let age = time % (LENGTH + 0.4);
    let origin = vec2(r.x + r.w / 2.0, r.y + r.h * 0.82);
    // A pocket for the effect to come out of.
    crate::paint::draw_circle(origin.x, origin.y, r.h * 0.11, Color::new(0.02, 0.02, 0.02, 1.0));
    let scale = if kind == EffectKind::Confetti { r.h * 0.55 } else { r.h * 0.75 };
    draw_clipped(kind, color, origin, age, scale, Some(r));
}

/// An effect playing over the table.
pub struct WinEffect {
    kind: EffectKind,
    color: [u8; 3],
    /// The pocket, in the 3D world.
    pub at: Vec3,
    age: f32,
}

impl WinEffect {
    /// The effect of a shop item (`None` for no effect).
    pub fn of(code: &str, at: Vec3) -> Option<WinEffect> {
        match snook_legend_proto::shop::item(code)?.look {
            Look::Effect { kind, color } if kind != EffectKind::None => {
                Some(WinEffect { kind, color, at, age: 0.0 })
            }
            _ => None,
        }
    }

    /// Draws at the pocket's place on screen; false once it is over.
    pub fn draw(&mut self, dt: f32, screen: Option<Vec2>, scale: f32) -> bool {
        self.age += dt;
        if let Some(p) = screen {
            draw_at(self.kind, self.color, p, self.age, scale);
        }
        self.age < LENGTH
    }
}
