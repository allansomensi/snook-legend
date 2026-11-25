//! A reward is opened, not just shown: an item won comes in a box that
//! shakes, bursts open in the colour of its rarity, and the item's card
//! turns over into view; coins earned count up in a little burst of coins
//! at the top of the screen.
//!
//! Everything is drawn from the time since the reward appeared (no state
//! per particle), like the winning effects.

use std::cell::RefCell;

use macroquad::prelude::*;
use snook_legend_proto::shop::{self, Rarity};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle, draw_triangle};

use crate::collection::{rarity_color, rarity_name};
use crate::icons::Icon;
use crate::ui::{GOLD, Ui, rrect, rrect_lines};

/// When the box bursts, and when the card has turned over.
const BURST: f32 = 0.75;
const SHOWN: f32 = 1.25;

fn hash(i: u32, k: u32) -> f32 {
    let mut h = i.wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B);
    h = (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D);
    h = (h ^ (h >> 12)).wrapping_mul(0x297A_2D39);
    (h ^ (h >> 15)) as f32 / u32::MAX as f32
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, a.clamp(0.0, 1.0))
}

/// The item `code` coming out of its box in `r`, `age` seconds after the
/// reward appeared (a present, an item won). An unknown code (VIP given as
/// a present): a crown comes out.
pub fn item(ui: &Ui, code: &str, r: Rect, age: f32) {
    let item = shop::item(code);
    let rarity = item.map(shop::rarity).unwrap_or(Rarity::Epic);
    let rc = rarity_color(rarity);
    let c = vec2(r.x + r.w / 2.0, r.y + r.h / 2.0);
    let s = r.h.min(r.w) * 0.42;
    // Rays turning slowly behind, from the burst on.
    let rays = smooth(BURST - 0.1, BURST + 0.4, age);
    if rays > 0.0 {
        let n = 14;
        for k in 0..n {
            let a = k as f32 / n as f32 * std::f32::consts::TAU + age * 0.35;
            let w = 0.09;
            let len = s * (1.6 + 0.2 * ((age * 2.0 + k as f32).sin()));
            let p1 = c + vec2((a - w).cos(), (a - w).sin()) * len;
            let p2 = c + vec2((a + w).cos(), (a + w).sin()) * len;
            draw_triangle(c, p1, p2, with_alpha(rc, 0.12 * rays));
        }
        draw_circle(c.x, c.y, s * 1.1, with_alpha(rc, 0.08 * rays));
    }
    if age < BURST {
        // The box: it grows in, then shakes harder and harder.
        let grow = smooth(0.0, 0.25, age);
        let shake = smooth(0.25, BURST, age);
        let jitter =
            vec2((age * 61.0).sin() * shake * s * 0.06, (age * 47.0).cos() * shake * s * 0.03);
        let tilt = (age * 40.0).sin() * shake * 0.08;
        let b = s * 0.95 * grow;
        let at = c + jitter;
        let body = Rect::new(at.x - b / 2.0, at.y - b * 0.3, b, b * 0.75);
        let dark = Color::new(rc.r * 0.45, rc.g * 0.45, rc.b * 0.45, 1.0);
        rrect(body, 4.0, dark);
        rrect(Rect::new(body.x, body.y, body.w, body.h * 0.5), 4.0, with_alpha(rc, 0.9));
        // The lid, a little wider, tipping as it shakes.
        let lid_y = body.y - b * 0.16 - tilt * b;
        rrect(Rect::new(at.x - b * 0.56, lid_y, b * 1.12, b * 0.2), 4.0, rc);
        // The ribbon, and its bow.
        let ribbon = Color::new(1.0, 0.92, 0.7, 1.0);
        draw_rectangle(at.x - b * 0.07, body.y, b * 0.14, body.h, ribbon);
        draw_rectangle(at.x - b * 0.07, lid_y, b * 0.14, b * 0.2, ribbon);
        for side in [-1.0f32, 1.0] {
            draw_triangle(
                vec2(at.x, lid_y),
                vec2(at.x + side * b * 0.28, lid_y - b * 0.18),
                vec2(at.x + side * b * 0.24, lid_y + b * 0.02),
                ribbon,
            );
        }
        // Light leaking out of it just before it opens.
        if shake > 0.5 {
            let k = (shake - 0.5) * 2.0;
            draw_line(
                body.x,
                lid_y + b * 0.2,
                body.x + body.w,
                lid_y + b * 0.2,
                3.0,
                with_alpha(WHITE, k),
            );
        }
        return;
    }
    // The burst: sparks flying out, and a flash.
    let t = age - BURST;
    let flash = (1.0 - t / 0.3).max(0.0);
    if flash > 0.0 {
        draw_circle(c.x, c.y, s * (0.6 + t * 3.0), with_alpha(WHITE, 0.5 * flash));
    }
    for i in 0..36u32 {
        let life = 0.6 + hash(i, 1) * 0.6;
        if t > life {
            continue;
        }
        let k = t / life;
        let a = hash(i, 2) * std::f32::consts::TAU;
        let d = s * (0.3 + 1.6 * k) * (0.6 + 0.6 * hash(i, 3));
        let p = c + vec2(a.cos(), a.sin()) * d + vec2(0.0, k * k * s * 0.6);
        let color = if i % 3 == 0 { with_alpha(WHITE, 1.0 - k) } else { with_alpha(rc, 1.0 - k) };
        draw_circle(p.x, p.y, 2.5 * (1.0 - k) + 0.8, color);
    }
    // The card turns over into view (narrow, then whole), with a little
    // overshoot.
    let flip = smooth(0.0, SHOWN - BURST, t);
    let width = (flip * std::f32::consts::FRAC_PI_2).sin();
    let pop = 1.0 + 0.06 * (1.0 - flip) * (flip * std::f32::consts::PI).sin();
    let card = Rect::new(
        c.x - r.w * 0.5 * width * pop,
        r.y + r.h * 0.5 * (1.0 - pop),
        r.w * width * pop,
        r.h * pop,
    );
    if card.w < 2.0 {
        return;
    }
    rrect(card, 8.0, Color::new(0.06, 0.045, 0.035, 0.95));
    match item {
        Some(item) if width > 0.6 => crate::shop::preview(ui, item, card),
        None if width > 0.6 => crate::icons::draw(
            Icon::Crown,
            vec2(card.x + card.w / 2.0, card.y + card.h / 2.0),
            card.h * 0.55,
            GOLD,
        ),
        _ => {}
    }
    rrect_lines(card, 8.0, if rarity >= Rarity::Epic { 2.0 } else { 1.2 }, rc);
    // The rarity's name under it, once the card is round.
    if flip > 0.9 && rarity > Rarity::Common {
        let name = rarity_name(rarity);
        let w = ui.text_size(name, 11.0, true).x + 16.0;
        let pill = Rect::new(c.x - w / 2.0, card.y + card.h - 9.0, w, 18.0);
        rrect(pill, 9.0, Color::new(0.0, 0.0, 0.0, 0.85));
        rrect_lines(pill, 9.0, 1.0, rc);
        ui.text_centered(name, c.x, pill.y + 3.0, 11.0, rc, true);
    }
    // Twinkles round the card, for good.
    for i in 0..8u32 {
        let ph = (age * 0.7 + hash(i, 7)).fract();
        let a = hash(i, 8) * std::f32::consts::TAU;
        let p = c + vec2(a.cos() * r.w * 0.55, a.sin() * r.h * 0.6);
        let k = (1.0 - (ph * 2.0 - 1.0).abs()) * flip;
        let z = 1.0 + 4.0 * k;
        let col = with_alpha(Color::new(1.0, 0.95, 0.8, 1.0), k);
        draw_line(p.x - z, p.y, p.x + z, p.y, 1.2, col);
        draw_line(p.x, p.y - z, p.x, p.y + z, 1.2, col);
    }
}

/// Coins earned: a pill at the top of the screen that counts up, in a
/// spray of coins.
struct CoinPop {
    amount: i64,
    why: String,
    since: f64,
}

thread_local! {
    static COINS: RefCell<Vec<CoinPop>> = const { RefCell::new(Vec::new()) };
}

/// How long a pill of coins stays.
const COIN_TIME: f64 = 3.4;

/// Coins were earned (`why`: a few words, already translated).
pub fn coins(amount: i64, why: String) {
    COINS.with(|c| {
        let mut c = c.borrow_mut();
        c.push(CoinPop { amount, why, since: get_time() });
        while c.len() > 3 {
            c.remove(0);
        }
    });
}

/// Draws the coins earned lately (over everything, at the top).
pub fn draw_coins(ui: &Ui) {
    let now = get_time();
    COINS.with(|c| c.borrow_mut().retain(|p| now - p.since < COIN_TIME));
    COINS.with(|c| {
        let sw = crate::ui::vw();
        for (k, p) in c.borrow().iter().enumerate() {
            let age = (now - p.since) as f32;
            let fade = smooth(0.0, 0.2, age)
                * (1.0 - smooth(COIN_TIME as f32 - 0.5, COIN_TIME as f32, age));
            let count = (p.amount as f32 * smooth(0.15, 1.0, age)).round() as i64;
            let text = format!("+{}", crate::collection::coins_text(count));
            let tw_ = ui.text_size(&text, 20.0, true).x;
            let why_w =
                if p.why.is_empty() { 0.0 } else { ui.text_size(&p.why, 13.0, false).x + 12.0 };
            let w = 52.0 + tw_ + why_w + 18.0;
            let drop = (1.0 - smooth(0.0, 0.3, age)) * 18.0;
            let r = Rect::new((sw - w) / 2.0, 64.0 + k as f32 * 52.0 - drop, w, 42.0);
            rrect(r, 21.0, Color::new(0.08, 0.06, 0.03, 0.92 * fade));
            rrect_lines(r, 21.0, 1.5, with_alpha(GOLD, 0.9 * fade));
            // The coin turns as the count runs.
            let turn = if age < 1.0 { (age * 14.0).cos().abs().max(0.2) } else { 1.0 };
            let cc = vec2(r.x + 24.0, r.y + 21.0);
            crate::paint::draw_ellipse(cc.x, cc.y, 13.0 * turn, 13.0, with_alpha(GOLD, fade));
            crate::paint::draw_ellipse(
                cc.x,
                cc.y,
                9.0 * turn,
                9.0,
                with_alpha(Color::new(1.0, 0.9, 0.55, 1.0), fade),
            );
            ui.text(&text, r.x + 44.0, r.y + 10.0, 20.0, with_alpha(GOLD, fade), true);
            if !p.why.is_empty() {
                let x = r.x + 44.0 + tw_ + 12.0;
                ui.text(&p.why, x, r.y + 14.0, 13.0, with_alpha(crate::ui::CREAM, fade), false);
            }
            // Coins bursting out of the pill, then falling.
            if age < 1.2 {
                for i in 0..12u32 {
                    let a = -std::f32::consts::FRAC_PI_2 + (hash(i, 1) - 0.5) * 2.6;
                    let v = 70.0 + 60.0 * hash(i, 2);
                    let p0 = cc + vec2(a.cos(), a.sin()) * v * age + vec2(0.0, 160.0 * age * age);
                    let alpha = (1.0 - age / 1.2) * fade;
                    let spin = (age * 12.0 + hash(i, 3) * 6.0).cos().abs().max(0.2);
                    crate::paint::draw_ellipse(
                        p0.x,
                        p0.y,
                        4.5 * spin,
                        4.5,
                        with_alpha(GOLD, alpha),
                    );
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_box_bursts_before_the_card_shows() {
        assert!(BURST < SHOWN);
        assert!(smooth(0.0, 1.0, 0.5) > 0.4 && smooth(0.0, 1.0, 2.0) == 1.0);
    }
}
