//! Reaction pictures from the bundled atlas (Twemoji, CC-BY 4.0). The
//! rarest ones move: they breathe, spin, sway or sparkle (see [`motion`]).

use std::cell::OnceCell;

use macroquad::prelude::*;
use snook_legend_proto::REACTIONS;

const SIZE: f32 = 72.0;
const COLUMNS: usize = 8;

thread_local! {
    static ATLAS: OnceCell<Texture2D> = const { OnceCell::new() };
}

fn atlas() -> Texture2D {
    ATLAS.with(|a| {
        a.get_or_init(|| {
            let tex =
                Texture2D::from_file_with_format(include_bytes!("../assets/reactions.png"), None);
            tex.set_filter(FilterMode::Linear);
            tex
        })
        .clone()
    })
}

/// How a shop reaction is had ("In the shop, for 300 coins").
pub fn shop_hint(r: &snook_legend_proto::Reaction) -> String {
    let coins = r.item().and_then(|i| i.coins()).unwrap_or(0);
    crate::i18n::tw("In the shop, for {n} coins", &[("n", &coins)])
}

/// How reaction `r` is had, under the pointer: the achievement that brings
/// it, the shop, VIP, or nothing (every player has it). Said of the
/// reactions had too (`have`), so a reaction tells how it was won.
pub fn how(r: &snook_legend_proto::Reaction, have: bool) -> String {
    use crate::i18n::{t, tw};
    if r.is_vip() {
        return t("For VIP members").to_string();
    }
    if r.is_shop() {
        // Bought or given by a friend: from the shop either way.
        return if have { t("From the shop").to_string() } else { shop_hint(r) };
    }
    if r.is_earned() {
        return t("Only won: events, cups and seasons").to_string();
    }
    match r.unlock.and_then(|a| snook_legend_proto::ACHIEVEMENTS.iter().find(|x| x.code == a)) {
        // A secret stays one until it is won.
        Some(a) if a.secret && !have => t("A secret achievement").to_string(),
        Some(a) => tw(
            "{title}: {how}",
            &[("title", &t(a.title)), ("how", &crate::profile::achievement_text(a))],
        ),
        None => t("Every player has it").to_string(),
    }
}

/// How a rare reaction moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    /// Grows and shrinks, with a glow behind.
    Pulse([u8; 3]),
    /// Turns round and round.
    Spin,
    /// Sways from side to side.
    Sway,
    /// Floats up and down, sparkling.
    Float([u8; 3]),
    /// Trembles, with a hot glow.
    Rumble([u8; 3]),
}

/// The reactions that move (the rarest), and how.
pub fn motion(code: &str) -> Option<Motion> {
    Some(match code {
        "dragon" => Motion::Sway,
        "volcano" => Motion::Rumble([255, 110, 30]),
        "tornado" => Motion::Spin,
        "crystal_ball" => Motion::Pulse([190, 130, 255]),
        "lion" => Motion::Pulse([255, 200, 80]),
        "unicorn" => Motion::Float([255, 170, 240]),
        "zombie" => Motion::Sway,
        "rainbow" => Motion::Pulse([140, 220, 255]),
        "comet" => Motion::Float([140, 200, 255]),
        "dizzy" => Motion::Spin,
        "season_medal" => Motion::Float([255, 214, 100]),
        _ => return None,
    })
}

/// Draws reaction `code` centred on `at`, `size` wide, with `alpha` (the
/// rare ones moving). False for an unknown code.
pub fn draw(code: &str, at: Vec2, size: f32, alpha: f32) -> bool {
    let Some(i) = REACTIONS.iter().position(|r| r.code == code) else { return false };
    // The characters act, each on its own beat (past their popping in).
    if crate::emotes::draw(code, at, size, alpha, 0.6 + get_time() as f32 + i as f32 * 0.37) {
        return true;
    }
    draw_picture(i, code, at, size, alpha, get_time() as f32)
}

/// Reaction `code` sent `age` seconds ago, in a game: a character performs
/// from the start (popping in); a picture hops in.
pub fn draw_sent(code: &str, at: Vec2, size: f32, alpha: f32, age: f32) -> bool {
    let Some(i) = REACTIONS.iter().position(|r| r.code == code) else { return false };
    if crate::emotes::draw(code, at, size, alpha, age) {
        return true;
    }
    draw_picture(i, code, at, size, alpha, age)
}

/// Whether reaction `code` is one of the characters that act (drawn
/// bigger in a game: they have a lot to show).
pub fn acts(code: &str) -> bool {
    crate::emotes::is_character(code)
}

/// A reaction's picture: the rare ones move their own way, the rest hop
/// now and then (squashed as they land), `time` seconds on.
fn draw_picture(i: usize, code: &str, at: Vec2, size: f32, alpha: f32, time: f32) -> bool {
    let (col, row) = (i % COLUMNS, i / COLUMNS);
    let src = Rect::new(col as f32 * SIZE, row as f32 * SIZE, SIZE, SIZE);
    let a = alpha;
    // Each reaction moves on its own beat.
    let phase = i as f32 * 0.7;
    let (mut at, mut size, mut rotation) = (at, size, 0.0);
    let mut squash = vec2(1.0, 1.0);
    let glow = |c: [u8; 3], k: f32, at: Vec2, size: f32| {
        let color = Color::from_rgba(c[0], c[1], c[2], 255);
        for (r, alpha) in [(0.62, 0.10), (0.5, 0.12), (0.38, 0.14)] {
            crate::paint::draw_circle(
                at.x,
                at.y,
                size * r * (1.0 + 0.1 * k),
                Color::new(color.r, color.g, color.b, alpha * a * (0.6 + 0.4 * k)),
            );
        }
    };
    let sparkles = |c: [u8; 3], at: Vec2, size: f32| {
        for k in 0..4 {
            let t = (time * 0.8 + k as f32 * 0.25 + phase).fract();
            let ang = k as f32 * 1.9 + phase;
            let p = at + vec2(ang.cos(), ang.sin()) * size * (0.38 + 0.18 * t);
            let s = size * 0.07 * (1.0 - (t * 2.0 - 1.0).abs());
            let color = Color::from_rgba(c[0], c[1], c[2], (220.0 * a) as u8);
            crate::paint::draw_line(p.x - s, p.y, p.x + s, p.y, 1.2, color);
            crate::paint::draw_line(p.x, p.y - s, p.x, p.y + s, 1.2, color);
        }
    };
    match motion(code) {
        None => {
            // A hop now and then: up, and squashed a little on landing.
            let k = ((time + phase) % 2.4) / 2.4;
            let up = if k < 0.18 { (k / 0.18 * std::f32::consts::PI).sin() } else { 0.0 };
            let land = if (0.18..0.26).contains(&k) {
                ((k - 0.18) / 0.08 * std::f32::consts::PI).sin()
            } else {
                0.0
            };
            at.y -= up * size * 0.12;
            squash = vec2(1.0 + 0.12 * land - 0.05 * up, 1.0 - 0.12 * land + 0.06 * up);
            at.y += (1.0 - squash.y) * size * 0.5;
        }
        Some(Motion::Pulse(c)) => {
            let k = 0.5 + 0.5 * (time * 3.0 + phase).sin();
            glow(c, k, at, size);
            size *= 1.0 + 0.07 * k;
        }
        Some(Motion::Spin) => rotation = (time * 2.2 + phase) % std::f32::consts::TAU,
        Some(Motion::Sway) => rotation = (time * 2.5 + phase).sin() * 0.16,
        Some(Motion::Float(c)) => {
            at.y += (time * 2.0 + phase).sin() * size * 0.06;
            sparkles(c, at, size);
        }
        Some(Motion::Rumble(c)) => {
            glow(c, 0.5 + 0.5 * (time * 7.0).sin(), at, size);
            at += vec2((time * 47.0).sin(), (time * 39.0).cos()) * size * 0.015;
        }
    }
    let dest = vec2(size, size) * squash;
    draw_texture_ex(
        &atlas(),
        at.x - dest.x / 2.0,
        at.y - dest.y / 2.0,
        Color::new(1.0, 1.0, 1.0, a),
        DrawTextureParams {
            dest_size: Some(dest),
            source: Some(src),
            rotation,
            ..Default::default()
        },
    );
    true
}

/// Like [`draw`], in grey (a reaction still locked).
pub fn draw_locked(code: &str, at: Vec2, size: f32) {
    let Some(i) = REACTIONS.iter().position(|r| r.code == code) else { return };
    let (col, row) = (i % COLUMNS, i / COLUMNS);
    let src = Rect::new(col as f32 * SIZE, row as f32 * SIZE, SIZE, SIZE);
    let a = crate::paint::alpha();
    draw_texture_ex(
        &atlas(),
        at.x - size / 2.0,
        at.y - size / 2.0,
        Color::new(0.55, 0.5, 0.45, 0.3 * a),
        DrawTextureParams {
            dest_size: Some(vec2(size, size)),
            source: Some(src),
            ..Default::default()
        },
    );
}
