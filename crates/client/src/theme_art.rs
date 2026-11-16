//! Small pictures of the themed tables and halls for the shop's cards (2D,
//! like the other cards' pictures): the table from the front, and a corner
//! of each hall with its light.

use macroquad::prelude::*;
use snookered_proto::shop::{HallStyle, TableStyle};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_ellipse, draw_line, draw_rectangle, draw_triangle};
use crate::ui::{rrect, rrect_gradient};

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba(r, g, b, 255)
}

fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
}

fn quad(a: Vec2, b: Vec2, c: Vec2, d: Vec2, color: Color) {
    draw_triangle(a, b, c, color);
    draw_triangle(a, c, d, color);
}

/// A vertical gradient in `r` (no rounding), in bands.
fn gradient(r: Rect, top: Color, bottom: Color, bands: u32) {
    for k in 0..bands {
        let t = (k as f32 + 0.5) / bands as f32;
        let c = Color::new(
            top.r + (bottom.r - top.r) * t,
            top.g + (bottom.g - top.g) * t,
            top.b + (bottom.b - top.b) * t,
            1.0,
        );
        let y0 = r.y + r.h * k as f32 / bands as f32;
        draw_rectangle(r.x, y0, r.w, r.h / bands as f32 + 0.5, c);
    }
}

struct TableLook {
    rail: Color,
    apron: Color,
    legs: Color,
    metal: Color,
    /// The apron's depth (share of the picture's height).
    depth: f32,
}

fn table_look(s: TableStyle) -> TableLook {
    match s {
        TableStyle::Classic => TableLook {
            rail: rgb(92, 40, 22),
            apron: rgb(70, 32, 18),
            legs: rgb(60, 28, 16),
            metal: rgb(176, 136, 64),
            depth: 0.13,
        },
        TableStyle::Beach => TableLook {
            rail: rgb(178, 122, 70),
            apron: rgb(200, 162, 100),
            legs: rgb(150, 100, 56),
            metal: rgb(214, 218, 226),
            depth: 0.12,
        },
        TableStyle::Club => TableLook {
            rail: rgb(104, 34, 18),
            apron: rgb(84, 28, 16),
            legs: rgb(76, 26, 14),
            metal: rgb(70, 42, 24),
            depth: 0.17,
        },
        TableStyle::Modern => TableLook {
            rail: rgb(24, 24, 27),
            apron: rgb(18, 18, 20),
            legs: rgb(20, 20, 23),
            metal: rgb(172, 176, 182),
            depth: 0.08,
        },
        TableStyle::Royal => TableLook {
            rail: rgb(40, 28, 22),
            apron: rgb(32, 22, 17),
            legs: rgb(46, 32, 25),
            metal: rgb(224, 178, 80),
            depth: 0.13,
        },
    }
}

/// The table, seen from the front and a little above, on a soft spotlight.
pub fn table(r: Rect, style: TableStyle) {
    rrect_gradient(r, 8.0, rgb(30, 22, 17), rgb(14, 10, 8));
    let lk = table_look(style);
    let c = r.x + r.w / 2.0;
    let tw = (r.w * 0.82).min(r.h * 2.2);
    let floor = r.y + r.h * 0.9;
    // A pool of light and the shadow on the floor.
    draw_ellipse(c, floor, tw * 0.6, r.h * 0.08, rgba(0, 0, 0, 0.45));
    let top0 = r.y + r.h * 0.12;
    let top1 = top0 + r.h * 0.36;
    let (w0, w1) = (tw * 0.74, tw);
    let rail = r.h * 0.045;
    // The rail (a frame) and the cloth.
    quad(
        vec2(c - w0 / 2.0, top0),
        vec2(c + w0 / 2.0, top0),
        vec2(c + w1 / 2.0, top1),
        vec2(c - w1 / 2.0, top1),
        lk.rail,
    );
    let inset = |k: f32| rail * k;
    quad(
        vec2(c - w0 / 2.0 + inset(1.0), top0 + inset(0.5)),
        vec2(c + w0 / 2.0 - inset(1.0), top0 + inset(0.5)),
        vec2(c + w1 / 2.0 - inset(1.3), top1 - inset(0.6)),
        vec2(c - w1 / 2.0 + inset(1.3), top1 - inset(0.6)),
        rgb(28, 112, 56),
    );
    // Castings at the corners and the middle pockets (or leathers).
    let plate = rail * 1.1;
    for (x, y) in
        [(c - w0 / 2.0, top0), (c + w0 / 2.0, top0), (c - w1 / 2.0, top1), (c + w1 / 2.0, top1)]
    {
        let k = if y > top0 { 1.0 } else { 0.8 };
        draw_circle(
            x + (c - x).signum() * plate * 0.45,
            y + (top0 + top1 - 2.0 * y).signum() * plate * 0.2,
            plate * 0.75 * k,
            lk.metal,
        );
    }
    for (y, k) in [(top0, 0.8f32), (top1, 1.0)] {
        draw_rectangle(
            c - plate * 0.8 * k,
            y - plate * 0.3,
            plate * 1.6 * k,
            plate * 0.6,
            lk.metal,
        );
    }
    for (x, y, k) in [
        (c - w0 / 2.0 + inset(1.0), top0 + inset(0.5), 0.38),
        (c + w0 / 2.0 - inset(1.0), top0 + inset(0.5), 0.38),
        (c, top0 + inset(0.45), 0.34),
        (c - w1 / 2.0 + inset(1.3), top1 - inset(0.6), 0.5),
        (c + w1 / 2.0 - inset(1.3), top1 - inset(0.6), 0.5),
        (c, top1 - inset(0.55), 0.46),
    ] {
        draw_circle(x, y, rail * k * 1.4, rgb(8, 6, 5));
    }
    // A few balls in a triangle.
    for (k, (dx, dy)) in
        [(0.0f32, 0.0f32), (-0.03, 0.03), (0.03, 0.03), (-0.06, 0.06), (0.0, 0.06), (0.06, 0.06)]
            .into_iter()
            .enumerate()
    {
        let colors = [
            rgb(240, 200, 20),
            rgb(30, 60, 200),
            rgb(220, 30, 30),
            rgb(20, 20, 22),
            rgb(120, 40, 170),
            rgb(240, 120, 20),
        ];
        draw_circle(
            c + tw * 0.16 + dx * tw * 0.7,
            top0 + (top1 - top0) * 0.38 + dy * r.h * 0.9,
            r.h * 0.022,
            colors[k],
        );
    }
    draw_circle(c - tw * 0.2, top0 + (top1 - top0) * 0.55, r.h * 0.024, rgb(245, 242, 230));
    // The apron.
    let depth = r.h * lk.depth;
    let apron = Rect::new(c - w1 / 2.0, top1, w1, depth);
    draw_rectangle(apron.x, apron.y, apron.w, apron.h, lk.apron);
    draw_rectangle(apron.x, apron.y, apron.w, (r.h * 0.025).max(1.5), lk.rail);
    match style {
        TableStyle::Beach => {
            // Bamboo canes.
            let n = (apron.w / 4.0).max(8.0) as i32;
            for k in 0..n {
                let x = apron.x + apron.w * (k as f32 + 0.5) / n as f32;
                draw_line(x, apron.y + 2.0, x, apron.y + apron.h, 1.0, rgba(90, 60, 30, 0.6));
            }
            draw_line(
                apron.x,
                apron.y + apron.h * 0.5,
                apron.x + apron.w,
                apron.y + apron.h * 0.5,
                1.0,
                rgba(120, 84, 40, 0.5),
            );
        }
        TableStyle::Club | TableStyle::Royal => {
            let y = apron.y + apron.h * 0.55;
            let color = if style == TableStyle::Royal {
                lk.metal
            } else {
                crate::ui::shade(lk.apron, 0.12)
            };
            draw_line(apron.x, y, apron.x + apron.w, y, 1.6, color);
            draw_rectangle(apron.x - 1.5, apron.y + apron.h - 2.0, apron.w + 3.0, 2.5, lk.rail);
        }
        TableStyle::Modern => {
            draw_line(apron.x, apron.y + 1.0, apron.x + apron.w, apron.y + 1.0, 1.4, lk.metal);
            // The light under the edge.
            draw_rectangle(
                apron.x + 2.0,
                apron.y + apron.h - 1.5,
                apron.w - 4.0,
                1.5,
                rgb(150, 210, 255),
            );
            draw_ellipse(
                c,
                apron.y + apron.h + 3.0,
                apron.w * 0.48,
                4.0,
                rgba(120, 190, 255, 0.18),
            );
        }
        TableStyle::Classic => {
            draw_rectangle(apron.x - 1.0, apron.y + apron.h - 2.5, apron.w + 2.0, 2.5, lk.rail);
        }
    }
    // The legs.
    let leg_top = apron.y + apron.h;
    let leg_h = floor - leg_top;
    match style {
        TableStyle::Modern => {
            for sx in [-1.0f32, 1.0] {
                let x = c + sx * w1 * 0.3;
                let lw = w1 * 0.06;
                draw_rectangle(x - lw / 2.0, leg_top, lw, leg_h, lk.legs);
                draw_line(x - lw / 2.0, leg_top, x - lw / 2.0, floor, 1.0, lk.metal);
                draw_rectangle(x - lw * 0.9, floor - 2.5, lw * 1.8, 2.5, lk.metal);
            }
        }
        TableStyle::Beach => {
            for sx in [-1.0f32, 1.0] {
                let x = c + sx * (w1 / 2.0 - w1 * 0.06);
                let (a, b) = (w1 * 0.035, w1 * 0.026);
                quad(
                    vec2(x - a, leg_top),
                    vec2(x + a, leg_top),
                    vec2(x + b, floor - 3.0),
                    vec2(x - b, floor - 3.0),
                    lk.legs,
                );
                draw_rectangle(x - b - 1.0, floor - 3.0, 2.0 * b + 2.0, 3.0, lk.metal);
            }
            let y = floor - leg_h * 0.2;
            draw_line(
                c - w1 / 2.0 + w1 * 0.06,
                y,
                c + w1 / 2.0 - w1 * 0.06,
                y,
                2.0,
                rgb(200, 164, 104),
            );
        }
        _ => {
            let carved = style == TableStyle::Club;
            for sx in [-1.0f32, 1.0] {
                let x = c + sx * (w1 / 2.0 - w1 * 0.07);
                let lw = w1 * if carved { 0.07 } else { 0.055 };
                // A block, a swelling turned body, a narrow neck, a foot.
                draw_rectangle(x - lw / 2.0, leg_top, lw, leg_h * 0.12, lk.legs);
                draw_ellipse(x, leg_top + leg_h * 0.38, lw * 0.62, leg_h * 0.24, lk.legs);
                draw_rectangle(
                    x - lw * 0.22,
                    leg_top + leg_h * 0.1,
                    lw * 0.44,
                    leg_h * 0.8,
                    lk.legs,
                );
                draw_ellipse(x, floor - leg_h * 0.1, lw * 0.5, leg_h * 0.1, lk.legs);
                if carved {
                    for k in -1..=1 {
                        let fx = x + k as f32 * lw * 0.22;
                        draw_line(
                            fx,
                            leg_top + leg_h * 0.22,
                            fx,
                            leg_top + leg_h * 0.52,
                            1.0,
                            rgba(0, 0, 0, 0.35),
                        );
                    }
                }
                if style == TableStyle::Royal {
                    for y in [leg_top + leg_h * 0.12, floor - leg_h * 0.18] {
                        draw_rectangle(x - lw * 0.4, y, lw * 0.8, 1.6, lk.metal);
                    }
                }
            }
        }
    }
    // A soft shine on the rail.
    draw_line(
        c - w0 / 2.0 + 2.0,
        top0 + 1.0,
        c + w0 / 2.0 - 2.0,
        top0 + 1.0,
        1.0,
        rgba(255, 255, 255, 0.18),
    );
}

/// A tiny pool table in the middle of a hall picture.
fn little_table(c: Vec2, w: f32) {
    let h = w * 0.22;
    draw_ellipse(c.x, c.y + h * 1.9, w * 0.55, h * 0.35, rgba(0, 0, 0, 0.45));
    quad(
        vec2(c.x - w * 0.42, c.y),
        vec2(c.x + w * 0.42, c.y),
        vec2(c.x + w * 0.5, c.y + h * 0.7),
        vec2(c.x - w * 0.5, c.y + h * 0.7),
        rgb(60, 28, 16),
    );
    quad(
        vec2(c.x - w * 0.38, c.y + h * 0.1),
        vec2(c.x + w * 0.38, c.y + h * 0.1),
        vec2(c.x + w * 0.45, c.y + h * 0.58),
        vec2(c.x - w * 0.45, c.y + h * 0.58),
        rgb(30, 120, 60),
    );
    draw_rectangle(c.x - w * 0.5, c.y + h * 0.7, w, h * 0.35, rgb(46, 20, 12));
    for sx in [-1.0f32, 1.0] {
        draw_rectangle(
            c.x + sx * w * 0.42 - w * 0.03,
            c.y + h * 1.05,
            w * 0.06,
            h * 0.85,
            rgb(40, 18, 10),
        );
    }
}

/// Lamp shades over the table: `color`, with a warm glow under them.
fn shades(c: Vec2, w: f32, color: Color, top: f32) {
    for k in -1..=1 {
        let x = c.x + k as f32 * w * 0.22;
        draw_line(x, top, x, c.y, 1.0, rgba(20, 16, 12, 0.9));
        let s = w * 0.09;
        quad(
            vec2(x - s * 0.4, c.y - s * 0.6),
            vec2(x + s * 0.4, c.y - s * 0.6),
            vec2(x + s, c.y),
            vec2(x - s, c.y),
            color,
        );
        draw_ellipse(x, c.y + 1.0, s * 0.9, s * 0.25, rgb(255, 226, 170));
    }
}

/// A corner of the hall with a little table in front.
pub fn hall(r: Rect, style: HallStyle) {
    rrect(r, 8.0, rgb(12, 9, 7));
    let inner = Rect::new(r.x + 2.0, r.y + 2.0, r.w - 4.0, r.h - 4.0);
    let horizon = inner.y + inner.h * 0.62;
    let wall = Rect::new(inner.x, inner.y, inner.w, horizon - inner.y);
    let floor = Rect::new(inner.x, horizon, inner.w, inner.y + inner.h - horizon);
    let table_c = vec2(inner.x + inner.w / 2.0, horizon + floor.h * 0.12);
    let table_w = inner.w * 0.5;
    match style {
        HallStyle::Pool => {
            gradient(wall, rgb(22, 52, 38), rgb(14, 36, 26), 6);
            // Damask dots, the wainscot and a neon ring.
            for k in 0..8 {
                for j in 0..3 {
                    let x = wall.x + wall.w * (k as f32 + 0.5 + (j % 2) as f32 * 0.5) / 8.0;
                    let y = wall.y + wall.h * (0.15 + j as f32 * 0.18);
                    draw_circle(x, y, 2.2, rgba(60, 100, 70, 0.6));
                }
            }
            draw_rectangle(wall.x, wall.y + wall.h * 0.7, wall.w, wall.h * 0.3, rgb(64, 30, 18));
            draw_line(
                wall.x,
                wall.y + wall.h * 0.7,
                wall.x + wall.w,
                wall.y + wall.h * 0.7,
                1.5,
                rgb(110, 60, 36),
            );
            draw_circle(
                wall.x + wall.w * 0.82,
                wall.y + wall.h * 0.35,
                wall.h * 0.16,
                rgba(255, 80, 170, 0.18),
            );
            crate::paint::draw_circle_lines(
                wall.x + wall.w * 0.82,
                wall.y + wall.h * 0.35,
                wall.h * 0.12,
                1.6,
                rgb(255, 110, 190),
            );
            // Bottles on the back bar.
            for k in 0..6 {
                let x = wall.x + wall.w * (0.06 + k as f32 * 0.035);
                draw_rectangle(
                    x,
                    wall.y + wall.h * 0.5,
                    2.5,
                    wall.h * 0.16,
                    [rgb(40, 120, 50), rgb(160, 90, 30), rgb(130, 30, 30)][k % 3],
                );
            }
            gradient(floor, rgb(70, 40, 22), rgb(40, 22, 12), 5);
            draw_ellipse(
                table_c.x,
                table_c.y + floor.h * 0.35,
                table_w * 0.8,
                floor.h * 0.3,
                rgb(110, 26, 22),
            );
            shades(vec2(table_c.x, wall.y + wall.h * 0.32), table_w, rgb(40, 130, 76), wall.y);
        }
        HallStyle::Beach => {
            // The sunset through a wide window, the sea, a palm.
            let win = Rect::new(wall.x, wall.y + wall.h * 0.12, wall.w, wall.h * 0.62);
            gradient(
                Rect::new(win.x, win.y, win.w, win.h * 0.7),
                rgb(70, 50, 110),
                rgb(255, 150, 70),
                10,
            );
            gradient(
                Rect::new(win.x, win.y + win.h * 0.7, win.w, win.h * 0.3),
                rgb(140, 80, 80),
                rgb(40, 36, 70),
                4,
            );
            draw_circle(
                win.x + win.w * 0.3,
                win.y + win.h * 0.66,
                win.h * 0.12,
                rgb(255, 222, 150),
            );
            draw_rectangle(
                win.x + win.w * 0.3 - 3.0,
                win.y + win.h * 0.72,
                6.0,
                win.h * 0.2,
                rgba(255, 190, 110, 0.6),
            );
            let palm = vec2(win.x + win.w * 0.78, win.y + win.h * 0.18);
            draw_line(palm.x + 6.0, win.y + win.h, palm.x, palm.y, 2.5, rgb(30, 20, 30));
            for k in 0..6 {
                let a = k as f32 / 6.0 * std::f32::consts::TAU + 0.3;
                let tip = palm + vec2(a.cos() * win.h * 0.32, a.sin().abs() * win.h * 0.12 + 2.0);
                draw_line(palm.x, palm.y, tip.x, tip.y, 2.0, rgb(30, 20, 30));
            }
            // Posts, the header and the plank wall below.
            draw_rectangle(wall.x, wall.y, wall.w, win.y - wall.y, rgb(150, 120, 90));
            for k in 0..5 {
                let x = win.x + win.w * k as f32 / 4.0;
                draw_rectangle(x - 2.5, win.y, 5.0, win.h, rgb(110, 76, 46));
            }
            draw_rectangle(
                wall.x,
                win.y + win.h,
                wall.w,
                wall.y + wall.h - win.y - win.h,
                rgb(190, 172, 140),
            );
            draw_line(wall.x, win.y + win.h, wall.x + wall.w, win.y + win.h, 2.5, rgb(120, 84, 52));
            // String lights.
            for k in 0..14 {
                let t = k as f32 / 13.0;
                let x = wall.x + wall.w * t;
                let y = wall.y + wall.h * 0.08 + (t * (1.0 - t)) * wall.h * 0.25;
                draw_circle(x, y, 1.6, rgb(255, 214, 140));
            }
            gradient(floor, rgb(170, 130, 90), rgb(110, 80, 52), 5);
            for k in 0..5 {
                let y = floor.y + floor.h * (k as f32 + 1.0) / 6.0;
                draw_line(floor.x, y, floor.x + floor.w, y, 1.0, rgba(60, 40, 24, 0.5));
            }
            shades(vec2(table_c.x, wall.y + wall.h * 0.32), table_w, rgb(170, 130, 80), wall.y);
        }
        HallStyle::Club => {
            gradient(wall, rgb(52, 18, 16), rgb(38, 12, 12), 4);
            // Bookshelves on the right, a fireplace on the left.
            let shelf = Rect::new(
                wall.x + wall.w * 0.56,
                wall.y + wall.h * 0.06,
                wall.w * 0.44,
                wall.h * 0.94,
            );
            draw_rectangle(shelf.x, shelf.y, shelf.w, shelf.h, rgb(40, 18, 10));
            let colors = [
                rgb(110, 22, 22),
                rgb(24, 56, 36),
                rgb(30, 36, 80),
                rgb(120, 80, 36),
                rgb(70, 30, 18),
            ];
            for row in 0..4 {
                let y = shelf.y + 3.0 + row as f32 * shelf.h / 4.0;
                let mut x = shelf.x + 3.0;
                let mut k = row * 3;
                while x < shelf.x + shelf.w - 4.0 {
                    let bw = 2.0 + (k % 3) as f32;
                    let bh = shelf.h / 4.0 - 5.0 - (k % 4) as f32;
                    draw_rectangle(
                        x,
                        y + (shelf.h / 4.0 - 5.0 - bh),
                        bw,
                        bh,
                        colors[k % colors.len()],
                    );
                    x += bw + 0.6;
                    k += 1;
                }
                draw_rectangle(shelf.x, y + shelf.h / 4.0 - 4.0, shelf.w, 2.0, rgb(70, 34, 18));
            }
            draw_rectangle(
                wall.x,
                wall.y + wall.h * 0.66,
                wall.w * 0.56,
                wall.h * 0.34,
                rgb(58, 22, 12),
            );
            let fp = Rect::new(
                wall.x + wall.w * 0.12,
                wall.y + wall.h * 0.5,
                wall.w * 0.3,
                wall.h * 0.5,
            );
            draw_rectangle(fp.x, fp.y, fp.w, fp.h, rgb(190, 180, 164));
            let fire = Rect::new(fp.x + fp.w * 0.22, fp.y + fp.h * 0.35, fp.w * 0.56, fp.h * 0.65);
            draw_rectangle(fire.x, fire.y, fire.w, fire.h, rgb(14, 10, 8));
            draw_ellipse(
                fire.x + fire.w / 2.0,
                fire.y + fire.h * 0.8,
                fire.w * 0.36,
                fire.h * 0.3,
                rgb(255, 130, 40),
            );
            draw_ellipse(
                fire.x + fire.w / 2.0,
                fire.y + fire.h * 0.82,
                fire.w * 0.2,
                fire.h * 0.18,
                rgb(255, 210, 110),
            );
            // A painting in a gilt frame above the mantel.
            let p = Rect::new(fp.x + fp.w * 0.1, wall.y + wall.h * 0.14, fp.w * 0.8, wall.h * 0.26);
            draw_rectangle(p.x - 2.0, p.y - 2.0, p.w + 4.0, p.h + 4.0, rgb(196, 156, 72));
            gradient(p, rgb(120, 110, 80), rgb(40, 50, 30), 3);
            gradient(floor, rgb(50, 28, 16), rgb(26, 14, 8), 4);
            draw_ellipse(
                table_c.x,
                table_c.y + floor.h * 0.35,
                table_w * 0.85,
                floor.h * 0.32,
                rgb(110, 20, 20),
            );
            draw_ellipse(
                table_c.x,
                table_c.y + floor.h * 0.35,
                table_w * 0.5,
                floor.h * 0.18,
                rgb(26, 30, 70),
            );
            // The canopy lamp.
            let y = wall.y + wall.h * 0.3;
            draw_line(
                table_c.x - table_w * 0.3,
                wall.y,
                table_c.x - table_w * 0.3,
                y,
                1.0,
                rgb(160, 120, 60),
            );
            draw_line(
                table_c.x + table_w * 0.3,
                wall.y,
                table_c.x + table_w * 0.3,
                y,
                1.0,
                rgb(160, 120, 60),
            );
            draw_rectangle(table_c.x - table_w * 0.45, y, table_w * 0.9, 5.0, rgb(80, 30, 16));
            draw_rectangle(
                table_c.x - table_w * 0.43,
                y + 5.0,
                table_w * 0.86,
                2.0,
                rgb(255, 230, 180),
            );
        }
        HallStyle::Loft => {
            // Brick with a tall steel window over the city.
            draw_rectangle(wall.x, wall.y, wall.w, wall.h, rgb(110, 50, 32));
            let rows = 10;
            for row in 0..rows {
                let y = wall.y + wall.h * row as f32 / rows as f32;
                draw_line(wall.x, y, wall.x + wall.w, y, 1.0, rgba(190, 170, 150, 0.35));
                let shift = if row % 2 == 0 { 0.0 } else { 5.0 };
                let mut x = wall.x + shift;
                while x < wall.x + wall.w {
                    draw_line(x, y, x, y + wall.h / rows as f32, 1.0, rgba(190, 170, 150, 0.3));
                    x += 10.0;
                }
            }
            let win = Rect::new(
                wall.x + wall.w * 0.42,
                wall.y + wall.h * 0.1,
                wall.w * 0.5,
                wall.h * 0.8,
            );
            gradient(win, rgb(16, 22, 50), rgb(70, 62, 110), 6);
            for k in 0..9 {
                let bx = win.x + win.w * k as f32 / 9.0;
                let bh = win.h * (0.25 + ((k * 37) % 5) as f32 * 0.09);
                draw_rectangle(bx, win.y + win.h - bh, win.w / 9.0 - 1.0, bh, rgb(14, 16, 30));
                for j in 0..4 {
                    if (k + j) % 3 != 0 {
                        draw_rectangle(
                            bx + 2.0,
                            win.y + win.h - bh + 3.0 + j as f32 * 4.0,
                            1.5,
                            1.5,
                            rgb(255, 214, 140),
                        );
                    }
                }
            }
            for k in 0..=4 {
                let x = win.x + win.w * k as f32 / 4.0;
                draw_line(x, win.y, x, win.y + win.h, 1.6, rgb(24, 26, 28));
            }
            for k in 0..=5 {
                let y = win.y + win.h * k as f32 / 5.0;
                draw_line(win.x, y, win.x + win.w, y, 1.6, rgb(24, 26, 28));
            }
            // A neon ring on the brick.
            draw_circle(
                wall.x + wall.w * 0.18,
                wall.y + wall.h * 0.36,
                wall.h * 0.17,
                rgba(80, 230, 255, 0.16),
            );
            crate::paint::draw_circle_lines(
                wall.x + wall.w * 0.18,
                wall.y + wall.h * 0.36,
                wall.h * 0.12,
                1.6,
                rgb(90, 230, 255),
            );
            gradient(floor, rgb(90, 90, 94), rgb(54, 54, 58), 4);
            shades(vec2(table_c.x, wall.y + wall.h * 0.32), table_w, rgb(26, 28, 30), wall.y);
        }
    }
    little_table(table_c, table_w);
}
