//! Who the computer is in a game against it: one of the three classic
//! levels, a named personality, or the ghost of a real player. Also how
//! they look: a small portrait drawn from shapes (no image files) and the
//! strength pips.

use macroquad::prelude::*;
use serde::{Deserialize, Serialize};
use snookered_core::Difficulty;
use snookered_core::ai::AiPlayer;
use snookered_core::personalities::{self, PERSONALITIES, Personality};
use snookered_proto::ghost::GhostStyle;

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_circle_lines, draw_line, draw_triangle};

use crate::i18n::{t, tw};
use crate::ui::{GOLD, MUTED};

/// The computer's side of a game against it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Opponent {
    /// A classic level (neutral style).
    Level { level: Difficulty },
    /// A named personality (its code).
    Personality { code: String },
    /// A real player's style, measured by the server.
    Ghost { name: String, style: GhostStyle },
}

impl Opponent {
    pub fn personality(code: &str) -> Opponent {
        Opponent::Personality { code: code.to_string() }
    }

    /// The personality, if this is one.
    pub fn as_personality(&self) -> Option<&'static Personality> {
        match self {
            Opponent::Personality { code } => personalities::personality(code),
            _ => None,
        }
    }

    /// How the computer plays.
    pub fn player(&self) -> AiPlayer {
        match self {
            Opponent::Level { level } => level.player(),
            Opponent::Personality { code } => personalities::personality(code)
                .map(|p| p.player)
                .unwrap_or_else(|| Difficulty::Medium.player()),
            Opponent::Ghost { style, .. } => style.player(),
        }
    }

    /// The name on the player card.
    pub fn display_name(&self) -> String {
        match self {
            Opponent::Level { level } => {
                tw("Computer ({level})", &[("level", &crate::i18n::difficulty(*level))])
            }
            Opponent::Personality { code } => match personalities::personality(code) {
                Some(p) => p.name.to_string(),
                None => t("Computer").to_string(),
            },
            Opponent::Ghost { name, .. } => ghost_name(name),
        }
    }

    /// The nearest classic level (kept in saved games for older versions).
    pub fn level(&self) -> Difficulty {
        match self {
            Opponent::Level { level } => *level,
            _ => match strength(self) {
                0..=2 => Difficulty::Easy,
                3..=4 => Difficulty::Medium,
                _ => Difficulty::Hard,
            },
        }
    }
}

/// "Ghost of <name>".
pub fn ghost_name(name: &str) -> String {
    tw("Ghost of {name}", &[("name", &name)])
}

/// How hard an opponent is to beat, 1 to 5.
pub fn strength(o: &Opponent) -> u8 {
    match o {
        Opponent::Level { level: Difficulty::Easy } => 1,
        Opponent::Level { level: Difficulty::Medium } => 3,
        Opponent::Level { level: Difficulty::Hard } => 4,
        Opponent::Personality { code } => {
            personalities::personality(code).map_or(3, |p| p.strength)
        }
        Opponent::Ghost { style, .. } => ghost_strength(style),
    }
}

/// A ghost's strength from how often the player pots what they go for.
pub fn ghost_strength(style: &GhostStyle) -> u8 {
    match style.sanitized().pot_rate {
        r if r < 0.4 => 1,
        r if r < 0.52 => 2,
        r if r < 0.64 => 3,
        r if r < 0.76 => 4,
        _ => 5,
    }
}

/// How a computer player plays, as six levels from 0 to 1 (English
/// labels, translated by the game): what the style and skill numbers mean
/// at the table.
pub fn traits(p: &AiPlayer) -> [(&'static str, f32); 6] {
    use snookered_core::ai::Search;
    let (skill, style) = (p.skill.clamped(), p.style.clamped());
    let k = |v: f64| v.clamp(0.0, 1.0) as f32;
    let accuracy = k((1.8 - skill.aim_noise_deg) / 1.6);
    let power = k((style.power + 1.0) / 2.0);
    let spin = if skill.search == Search::Narrow { 0.0 } else { k(style.spin / 1.5) };
    let safeties = if skill.plays_safe {
        k((style.safety_threshold - 150.0) / 950.0 * 0.6 + style.safety_value / 2.5 * 0.4)
    } else {
        0.0
    };
    let banks = if style.banks > 0.25 {
        k(0.6 + 0.4 * style.banks)
    } else if style.banks < -0.25 || !skill.banks {
        0.0
    } else {
        0.4
    };
    let position = k(skill.position_weight * style.position / 900.0);
    [
        ("Accuracy", accuracy),
        ("Power", power),
        ("Spin", spin),
        ("Safeties", safeties),
        ("Banks", banks),
        ("Position", position),
    ]
}

/// A short bar of five segments for a level from 0 to 1.
pub fn draw_level(x: f32, cy: f32, w: f32, level: f32) {
    let n = 5;
    let gap = 3.0;
    let seg = (w - gap * (n - 1) as f32) / n as f32;
    let lit = (level * n as f32).round() as usize;
    for i in 0..n {
        let r = Rect::new(x + i as f32 * (seg + gap), cy - 3.0, seg, 6.0);
        let c = if i < lit { GOLD } else { Color::new(1.0, 1.0, 1.0, 0.1) };
        crate::ui::rrect(r, 2.0, c);
    }
}

/// The personality to start with when none was chosen yet.
pub const DEFAULT_PERSONALITY: &str = "lola";

/// The personality saved in the settings (or the default one).
pub fn chosen(code: &str) -> &'static Personality {
    personalities::personality(code)
        .or_else(|| personalities::personality(DEFAULT_PERSONALITY))
        .unwrap_or(&PERSONALITIES[0])
}

/// Five pips, `strength` of them lit. Returns the width used.
pub fn draw_strength(x: f32, cy: f32, strength: u8, size: f32) -> f32 {
    let r = size * 0.5;
    let gap = size * 0.45;
    for k in 0..5u8 {
        let cx = x + r + k as f32 * (size + gap);
        if k < strength {
            draw_circle(cx, cy, r, GOLD);
        } else {
            draw_circle(cx, cy, r, Color::new(1.0, 1.0, 1.0, 0.08));
            draw_circle_lines(cx, cy, r, 1.0, Color::new(MUTED.r, MUTED.g, MUTED.b, 0.6));
        }
    }
    5.0 * size + 4.0 * gap
}

/// "Strength 3/5", for tooltips and screen readers of the pips.
pub fn strength_label(strength: u8) -> String {
    tw("Strength {n}/5", &[("n", &strength)])
}

/// The width [`draw_strength`] takes.
pub fn strength_width(size: f32) -> f32 {
    5.0 * size + 4.0 * size * 0.45
}

// ---------------------------------------------------------------------------
// Portraits.

#[derive(Clone, Copy, PartialEq)]
enum Hair {
    Short,
    Long,
    Bun,
    Curly,
    Bald,
    Cap,
    Slick,
}

#[derive(Clone, Copy)]
struct Look {
    bg: [u8; 3],
    skin: [u8; 3],
    hair: [u8; 3],
    shirt: [u8; 3],
    style: Hair,
    glasses: bool,
    beard: bool,
    bowtie: bool,
}

const fn look(
    bg: [u8; 3],
    skin: [u8; 3],
    hair: [u8; 3],
    shirt: [u8; 3],
    style: Hair,
    extras: (bool, bool, bool),
) -> Look {
    Look { bg, skin, hair, shirt, style, glasses: extras.0, beard: extras.1, bowtie: extras.2 }
}

fn look_of(code: &str) -> Look {
    const NONE: (bool, bool, bool) = (false, false, false);
    match code {
        "leo" => {
            look([70, 140, 90], [232, 190, 150], [200, 60, 40], [40, 110, 200], Hair::Cap, NONE)
        }
        "nina" => {
            look([150, 110, 170], [245, 205, 175], [120, 70, 40], [230, 150, 60], Hair::Long, NONE)
        }
        "lola" => {
            look([60, 120, 150], [200, 150, 115], [30, 22, 20], [200, 50, 80], Hair::Bun, NONE)
        }
        "bruno" => look(
            [170, 70, 40],
            [215, 160, 120],
            [40, 30, 25],
            [60, 60, 66],
            Hair::Short,
            (false, true, false),
        ),
        "marta" => look(
            [80, 90, 110],
            [235, 200, 170],
            [90, 90, 96],
            [70, 90, 70],
            Hair::Short,
            (true, false, false),
        ),
        "otto" => look(
            [110, 80, 50],
            [230, 190, 160],
            [225, 225, 225],
            [120, 40, 40],
            Hair::Bald,
            (false, true, false),
        ),
        "rosa" => {
            look([150, 40, 70], [170, 115, 85], [20, 16, 16], [240, 200, 80], Hair::Curly, NONE)
        }
        "kenji" => look(
            [50, 100, 110],
            [235, 205, 165],
            [25, 25, 30],
            [60, 70, 120],
            Hair::Short,
            (true, false, false),
        ),
        "sofia" => {
            look([40, 60, 120], [240, 210, 185], [230, 190, 90], [40, 40, 44], Hair::Long, NONE)
        }
        "viktor" => look(
            [92, 70, 120],
            [225, 195, 170],
            [30, 30, 34],
            [20, 20, 24],
            Hair::Slick,
            (false, false, true),
        ),
        // The classic levels: a robot-grey stranger.
        _ => look([70, 70, 80], [200, 200, 205], [90, 90, 100], [60, 60, 70], Hair::Short, NONE),
    }
}

fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgba(c[0], c[1], c[2], 255)
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

/// A regular polygon approximating a circle (or an ellipse).
fn ellipse(c: Vec2, rx: f32, ry: f32, n: usize) -> Vec<Vec2> {
    (0..n)
        .map(|k| {
            let a = std::f32::consts::TAU * k as f32 / n as f32;
            c + vec2(a.cos() * rx, a.sin() * ry)
        })
        .collect()
}

/// Keeps the part of the convex polygon `subject` inside the half-plane
/// left of `a → b` (Sutherland–Hodgman, one edge).
fn clip_edge(subject: &[Vec2], a: Vec2, b: Vec2) -> Vec<Vec2> {
    let inside = |p: Vec2| (b - a).perp_dot(p - a) >= 0.0;
    let mut out = Vec::with_capacity(subject.len() + 2);
    for i in 0..subject.len() {
        let cur = subject[i];
        let prev = subject[(i + subject.len() - 1) % subject.len()];
        match (inside(cur), inside(prev)) {
            (true, true) => out.push(cur),
            (true, false) => {
                out.push(intersect(prev, cur, a, b));
                out.push(cur);
            }
            (false, true) => out.push(intersect(prev, cur, a, b)),
            (false, false) => {}
        }
    }
    out
}

fn intersect(p: Vec2, q: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let d = q - p;
    let e = b - a;
    let denom = e.perp_dot(d);
    if denom.abs() < 1e-6 {
        return p;
    }
    let t = e.perp_dot(p - a) / -denom;
    p + d * t
}

/// The part of convex `subject` inside convex `clip` (counter-clockwise in
/// screen space: both are built the same way, so the orientation matches).
fn clip_convex(subject: Vec<Vec2>, clip: &[Vec2]) -> Vec<Vec2> {
    let mut out = subject;
    for i in 0..clip.len() {
        if out.len() < 3 {
            break;
        }
        out = clip_edge(&out, clip[i], clip[(i + 1) % clip.len()]);
    }
    out
}

fn fill(poly: &[Vec2], color: Color) {
    if poly.len() < 3 {
        return;
    }
    let c = poly.iter().copied().sum::<Vec2>() / poly.len() as f32;
    for i in 0..poly.len() {
        draw_triangle(c, poly[i], poly[(i + 1) % poly.len()], color);
    }
}

/// The part of a polygon above (`above`) or below the line `y`.
fn half(poly: Vec<Vec2>, y: f32, above: bool) -> Vec<Vec2> {
    // Screen y grows downwards; left of a→b is inside.
    if above {
        clip_edge(&poly, vec2(1.0e4, y), vec2(-1.0e4, y))
    } else {
        clip_edge(&poly, vec2(-1.0e4, y), vec2(1.0e4, y))
    }
}

/// The portrait of an opponent in a disc of radius `r` at `c`.
pub fn draw_portrait(o: &Opponent, c: Vec2, r: f32) {
    match o {
        Opponent::Ghost { name, .. } => draw_ghost(name, c, r),
        Opponent::Personality { code } => draw_person(code, c, r),
        Opponent::Level { .. } => draw_person("", c, r),
    }
}

/// The portrait of personality `code`.
pub fn draw_person(code: &str, c: Vec2, r: f32) {
    let l = look_of(code);
    let alpha = crate::paint::opacity();
    let a = |col: Color| with_alpha(col, alpha);
    let disc = ellipse(c, r, r, 40);
    let bg = rgb(l.bg);
    fill(&disc, a(crate::ui::shade(bg, -0.18)));
    fill(&ellipse(c - vec2(0.0, r * 0.25), r * 0.8, r * 0.7, 32), a(with_alpha(bg, 0.55)));
    let head_c = c + vec2(0.0, -0.08 * r);
    let head_r = 0.34 * r;
    let skin = rgb(l.skin);
    let hair = rgb(l.hair);
    // Long hair falls behind the head and shoulders.
    if l.style == Hair::Long {
        let back = ellipse(head_c + vec2(0.0, 0.2 * r), 0.44 * r, 0.52 * r, 32);
        fill(&clip_convex(back, &disc), a(hair));
    }
    // Shoulders: an ellipse cut by the disc.
    let shoulders = ellipse(c + vec2(0.0, 0.92 * r), 0.74 * r, 0.5 * r, 36);
    fill(&clip_convex(shoulders, &disc), a(rgb(l.shirt)));
    // Neck.
    fill(
        &ellipse(head_c + vec2(0.0, 0.33 * r), 0.12 * r, 0.12 * r, 16),
        a(crate::ui::shade(skin, -0.12)),
    );
    if l.bowtie {
        let k = head_c + vec2(0.0, 0.47 * r);
        let s = 0.11 * r;
        draw_triangle(
            k,
            k + vec2(-s * 1.4, -s),
            k + vec2(-s * 1.4, s),
            a(Color::new(0.75, 0.1, 0.12, 1.0)),
        );
        draw_triangle(
            k,
            k + vec2(s * 1.4, -s),
            k + vec2(s * 1.4, s),
            a(Color::new(0.75, 0.1, 0.12, 1.0)),
        );
        // A white collar under it.
        draw_triangle(
            k + vec2(-0.2 * r, -0.08 * r),
            k + vec2(-0.02 * r, 0.02 * r),
            k + vec2(-0.12 * r, 0.12 * r),
            a(Color::new(0.92, 0.92, 0.9, 1.0)),
        );
        draw_triangle(
            k + vec2(0.2 * r, -0.08 * r),
            k + vec2(0.02 * r, 0.02 * r),
            k + vec2(0.12 * r, 0.12 * r),
            a(Color::new(0.92, 0.92, 0.9, 1.0)),
        );
    }
    // Head and ears.
    fill(&ellipse(head_c + vec2(-head_r * 0.98, 0.02 * r), 0.07 * r, 0.09 * r, 12), a(skin));
    fill(&ellipse(head_c + vec2(head_r * 0.98, 0.02 * r), 0.07 * r, 0.09 * r, 12), a(skin));
    fill(&ellipse(head_c, head_r, head_r * 1.08, 32), a(skin));
    if l.beard {
        let beard = ellipse(head_c + vec2(0.0, 0.04 * r), head_r * 1.02, head_r * 1.1, 32);
        fill(&half(beard, head_c.y + 0.1 * r, false), a(hair));
        // The mouth shows through.
        draw_line(
            head_c.x - 0.07 * r,
            head_c.y + 0.2 * r,
            head_c.x + 0.07 * r,
            head_c.y + 0.2 * r,
            (0.025 * r).max(1.0),
            a(crate::ui::shade(skin, -0.3)),
        );
    } else {
        draw_line(
            head_c.x - 0.07 * r,
            head_c.y + 0.17 * r,
            head_c.x + 0.07 * r,
            head_c.y + 0.17 * r,
            (0.025 * r).max(1.0),
            a(Color::new(0.45, 0.2, 0.18, 1.0)),
        );
    }
    // Hair on top.
    let cap = ellipse(head_c + vec2(0.0, -0.03 * r), head_r * 1.06, head_r * 1.12, 32);
    match l.style {
        Hair::Short | Hair::Long | Hair::Bun | Hair::Slick => {
            fill(&half(cap, head_c.y - 0.12 * r, true), a(hair));
        }
        Hair::Curly => {
            for k in 0..7 {
                let ang = std::f32::consts::PI * (1.05 + 0.9 * k as f32 / 6.0);
                let p = head_c + vec2(ang.cos(), ang.sin() * 1.05) * head_r * 0.95;
                fill(&ellipse(p, 0.13 * r, 0.13 * r, 14), a(hair));
            }
        }
        Hair::Bald => {
            // A ring of hair over the ears.
            for side in [-1.0, 1.0] {
                fill(
                    &ellipse(head_c + vec2(side * head_r * 0.9, -0.06 * r), 0.07 * r, 0.12 * r, 12),
                    a(hair),
                );
            }
        }
        Hair::Cap => {
            let cap_col = rgb(l.hair);
            fill(&half(cap, head_c.y - 0.08 * r, true), a(cap_col));
            // The brim.
            let brim = ellipse(head_c + vec2(0.16 * r, -0.1 * r), 0.3 * r, 0.06 * r, 16);
            fill(&brim, a(crate::ui::shade(cap_col, -0.2)));
        }
    }
    if l.style == Hair::Bun {
        fill(&ellipse(head_c + vec2(0.0, -head_r * 1.1), 0.13 * r, 0.12 * r, 16), a(hair));
    }
    if l.style == Hair::Slick {
        // A parting.
        draw_line(
            head_c.x - 0.12 * r,
            head_c.y - 0.34 * r,
            head_c.x - 0.04 * r,
            head_c.y - 0.2 * r,
            (0.02 * r).max(1.0),
            a(crate::ui::shade(hair, 0.4)),
        );
    }
    // Eyes.
    let eye = Color::new(0.12, 0.09, 0.08, 1.0);
    for side in [-1.0, 1.0] {
        let p = head_c + vec2(side * 0.12 * r, 0.0);
        draw_circle(p.x, p.y, (0.035 * r).max(1.2), a(eye));
        if l.glasses {
            draw_circle_lines(
                p.x,
                p.y,
                0.085 * r,
                (0.025 * r).max(1.0),
                a(Color::new(0.1, 0.1, 0.12, 1.0)),
            );
        }
    }
    if l.glasses {
        draw_line(
            head_c.x - 0.035 * r,
            head_c.y,
            head_c.x + 0.035 * r,
            head_c.y,
            (0.025 * r).max(1.0),
            a(Color::new(0.1, 0.1, 0.12, 1.0)),
        );
    }
    draw_circle_lines(c.x, c.y, r, (0.04 * r).max(1.2), a(with_alpha(bg, 0.9)));
}

/// A ghost: a pale floating sheet, its tint from the player's name.
pub fn draw_ghost(name: &str, c: Vec2, r: f32) {
    let alpha = crate::paint::opacity();
    let a = |col: Color| with_alpha(col, alpha);
    let disc = ellipse(c, r, r, 40);
    let hash = name.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    let hue = (hash % 360) as f32 / 360.0;
    let tint = hsl_to_rgb(hue, 0.35, 0.32);
    fill(&disc, a(Color::new(tint.r * 0.5, tint.g * 0.5, tint.b * 0.6, 1.0)));
    fill(&ellipse(c - vec2(0.0, 0.25 * r), 0.8 * r, 0.7 * r, 32), a(with_alpha(tint, 0.5)));
    let sheet = Color::new(0.88, 0.93, 0.98, 0.92);
    let top = c + vec2(0.0, -0.18 * r);
    let w = 0.42 * r;
    fill(&ellipse(top, w, w, 28), a(sheet));
    // The body down to a wavy hem.
    let hem = c.y + 0.5 * r;
    fill(
        &[
            vec2(top.x - w, top.y),
            vec2(top.x + w, top.y),
            vec2(top.x + w, hem),
            vec2(top.x - w, hem),
        ],
        a(sheet),
    );
    for k in 0..3 {
        let x = top.x - w + w / 3.0 + k as f32 * (2.0 * w / 3.0);
        fill(&ellipse(vec2(x, hem), w / 3.0, 0.1 * r, 14), a(sheet));
    }
    let eye = Color::new(0.15, 0.18, 0.28, 1.0);
    for side in [-1.0, 1.0] {
        fill(&ellipse(top + vec2(side * 0.14 * r, 0.02 * r), 0.06 * r, 0.09 * r, 12), a(eye));
    }
    fill(&ellipse(top + vec2(0.0, 0.2 * r), 0.06 * r, 0.05 * r, 12), a(eye));
    draw_circle_lines(c.x, c.y, r, (0.04 * r).max(1.2), a(with_alpha(tint, 0.9)));
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> Color {
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |t: f32| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    Color::new(f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0), 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opponents_round_trip_and_play() {
        let all = [
            Opponent::Level { level: Difficulty::Hard },
            Opponent::personality("kenji"),
            Opponent::Ghost {
                name: "Ana".into(),
                style: GhostStyle { shots: 80, pot_rate: 0.7, speed: 3.0, ..Default::default() },
            },
        ];
        for o in &all {
            let text = serde_json::to_string(o).unwrap();
            assert_eq!(&serde_json::from_str::<Opponent>(&text).unwrap(), o);
            assert!((1..=5).contains(&strength(o)));
        }
        assert_eq!(all[1].player(), personalities::personality("kenji").unwrap().player);
        assert_eq!(all[0].player(), Difficulty::Hard.player());
        // An unknown personality (a newer version's) still plays.
        assert_eq!(Opponent::personality("zed").player(), Difficulty::Medium.player());
        assert_eq!(chosen("zed").code, DEFAULT_PERSONALITY);
    }

    #[test]
    fn clipping_keeps_the_inside() {
        let square = vec![vec2(0.0, 0.0), vec2(0.0, 2.0), vec2(2.0, 2.0), vec2(2.0, 0.0)];
        let top = half(square.clone(), 1.0, true);
        assert!(top.iter().all(|p| p.y <= 1.0 + 1e-4), "{top:?}");
        assert!(!top.is_empty());
        let bottom = half(square, 1.0, false);
        assert!(bottom.iter().all(|p| p.y >= 1.0 - 1e-4), "{bottom:?}");
        // An ellipse cut by a disc stays inside the disc.
        let disc = ellipse(vec2(0.0, 0.0), 1.0, 1.0, 40);
        let cut = clip_convex(ellipse(vec2(0.0, 0.9), 0.7, 0.5, 30), &disc);
        assert!(cut.len() >= 3);
        assert!(cut.iter().all(|p| p.length() <= 1.0 + 1e-3), "{cut:?}");
    }
}
