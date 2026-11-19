//! Who the computer is in a game against it: one of the three classic
//! levels, a named personality, or the ghost of a real player. Also how
//! they look: a small portrait drawn from shapes (no image files) and the
//! strength pips.

use std::f32::consts::{PI, TAU};

use macroquad::prelude::*;
use serde::{Deserialize, Serialize};
use snook_legend_core::Difficulty;
use snook_legend_core::ai::AiPlayer;
use snook_legend_core::personalities::{self, PERSONALITIES, Personality};
use snook_legend_proto::ghost::GhostStyle;

use crate::paint::{draw_circle, draw_circle_lines, draw_ellipse, draw_triangle};

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
    use snook_legend_core::ai::Search;
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
// Portraits: little illustrated characters built from shapes, in units of
// the disc's radius (the disc is centred on 0,0 and has radius 1; y grows
// downwards). Details that would be smaller than a pixel or two are left
// out at small sizes, so the same drawing reads from 9 px to 100 px.

fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgba(c[0], c[1], c[2], 255)
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

fn shade(c: Color, k: f32) -> Color {
    crate::ui::shade(c, k)
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

const INK: Color = Color::new(0.13, 0.08, 0.07, 1.0);
const WHITE: Color = Color::new(0.98, 0.97, 0.95, 1.0);
const SHIRT_WHITE: Color = Color::new(0.94, 0.94, 0.92, 1.0);
const GOLD_METAL: Color = Color::new(0.95, 0.75, 0.25, 1.0);
const MOUTH: Color = Color::new(0.42, 0.1, 0.12, 1.0);
const TONGUE: Color = Color::new(0.9, 0.42, 0.45, 1.0);
const BLUSH: Color = Color::new(0.96, 0.42, 0.42, 0.26);

/// A regular polygon approximating a circle (or an ellipse).
fn ellipse(c: Vec2, rx: f32, ry: f32, n: usize) -> Vec<Vec2> {
    (0..n)
        .map(|k| {
            let a = TAU * k as f32 / n as f32;
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

/// The part of convex `subject` inside convex `clip` (both built the same
/// way round as [`ellipse`]: right, bottom, left, top).
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

/// Fills a polygon that is star-shaped around its centroid.
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

/// How much larger than the disc's units a character is drawn.
const FIGURE_ZOOM: f32 = 1.3;

/// Draws in a portrait's disc, in units of its radius.
struct Pen {
    c: Vec2,
    r: f32,
    /// The size of a unit: `r`, times the figure's zoom.
    u: f32,
    alpha: f32,
    disc: Vec<Vec2>,
}

impl Pen {
    fn new(c: Vec2, r: f32) -> Pen {
        let n = ((r * 1.2) as usize).clamp(24, 72);
        Pen { c, r, u: r, alpha: crate::paint::opacity(), disc: ellipse(c, r, r, n) }
    }

    /// A pen for the character itself: drawn a little larger than the
    /// disc's units, with a shorter neck.
    fn figure(&self) -> Pen {
        Pen { u: self.r * FIGURE_ZOOM, disc: self.disc.clone(), ..*self }
    }

    /// Whether details are drawn: a unit is at least `px` pixels.
    fn shows(&self, px: f32) -> bool {
        self.u >= px
    }

    fn p(&self, x: f32, y: f32) -> Vec2 {
        if self.u == self.r {
            return self.c + vec2(x, y) * self.r;
        }
        // Pull everything below the chin up a little: a shorter neck.
        let t = ((y - 0.24) / 0.24).clamp(0.0, 1.0);
        let y = y - 0.1 * t * t * (3.0 - 2.0 * t);
        self.c + vec2(x, y) * self.u + vec2(0.0, 0.05 * self.r)
    }

    fn col(&self, c: Color) -> Color {
        with_alpha(c, self.alpha)
    }

    fn segs(&self, size: f32) -> usize {
        ((size * self.u * 0.9) as usize).clamp(10, 44)
    }

    fn oval_poly(&self, x: f32, y: f32, rx: f32, ry: f32) -> Vec<Vec2> {
        ellipse(self.p(x, y), rx * self.u, ry * self.u, self.segs(rx.max(ry)))
    }

    /// An ellipse turned by `angle` (radians, clockwise on screen).
    fn oval_rot(&self, x: f32, y: f32, rx: f32, ry: f32, angle: f32) -> Vec<Vec2> {
        let (s, co) = angle.sin_cos();
        let n = self.segs(rx.max(ry));
        (0..n)
            .map(|k| {
                let a = TAU * k as f32 / n as f32;
                let (u, v) = (a.cos() * rx, a.sin() * ry);
                self.p(x + u * co - v * s, y + u * s + v * co)
            })
            .collect()
    }

    fn poly(&self, pts: &[(f32, f32)]) -> Vec<Vec2> {
        pts.iter().map(|&(x, y)| self.p(x, y)).collect()
    }

    /// A rectangle, right side first (the same way round as [`ellipse`]).
    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Vec2> {
        self.poly(&[(x1, y0), (x1, y1), (x0, y1), (x0, y0)])
    }

    /// A rectangle with rounded corners of radius `k`, convex.
    fn round_rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, k: f32) -> Vec<Vec2> {
        let n = 5;
        let corners = [
            (x1 - k, y1 - k, 0.0),
            (x0 + k, y1 - k, 0.5 * PI),
            (x0 + k, y0 + k, PI),
            (x1 - k, y0 + k, 1.5 * PI),
        ];
        let mut out = Vec::new();
        for (cx, cy, a0) in corners {
            for i in 0..=n {
                let a = a0 + 0.5 * PI * i as f32 / n as f32;
                out.push(self.p(cx + a.cos() * k, cy + a.sin() * k));
            }
        }
        out
    }

    /// Fills a polygon (cut to the disc).
    fn fill(&self, poly: Vec<Vec2>, c: Color) {
        fill(&clip_convex(poly, &self.disc), self.col(c));
    }

    fn oval(&self, x: f32, y: f32, rx: f32, ry: f32, c: Color) {
        self.fill(self.oval_poly(x, y, rx, ry), c);
    }

    /// `base` in a darker tone, its lit part (itself moved up and left by
    /// `shift`) in `c`.
    fn shaded(&self, base: Vec<Vec2>, shift: (f32, f32), dark: f32, c: Color) {
        let base = clip_convex(base, &self.disc);
        fill(&base, self.col(shade(c, -dark)));
        let off = vec2(shift.0, shift.1) * self.u;
        let lit: Vec<Vec2> = base.iter().map(|&q| q + off).collect();
        fill(&clip_convex(lit, &base), self.col(c));
    }

    /// Points of a quadratic curve a → b pulled towards `k`.
    fn quad(&self, a: (f32, f32), k: (f32, f32), b: (f32, f32)) -> Vec<Vec2> {
        let n = ((self.u / 5.0) as usize).clamp(4, 16);
        (0..=n)
            .map(|i| {
                let t = i as f32 / n as f32;
                let u = 1.0 - t;
                let x = u * u * a.0 + 2.0 * u * t * k.0 + t * t * b.0;
                let y = u * u * a.1 + 2.0 * u * t * k.1 + t * t * b.1;
                self.p(x, y)
            })
            .collect()
    }

    /// Points of an elliptic arc from angle `a0` to `a1`.
    fn arc(&self, x: f32, y: f32, rx: f32, ry: f32, a0: f32, a1: f32) -> Vec<Vec2> {
        let n = ((self.u * (a1 - a0).abs() * rx.max(ry) / 3.0) as usize).clamp(4, 24);
        (0..=n)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / n as f32;
                self.p(x + a.cos() * rx, y + a.sin() * ry)
            })
            .collect()
    }

    /// A stroke along `pts`, `w(t)` wide (in units) from start to end,
    /// never thinner than a pixel.
    fn ribbon(&self, pts: &[Vec2], w: impl Fn(f32) -> f32, c: Color) {
        let n = pts.len();
        if n < 2 {
            return;
        }
        let c = self.col(c);
        let mut left = Vec::with_capacity(n);
        let mut right = Vec::with_capacity(n);
        for i in 0..n {
            let d = (pts[(i + 1).min(n - 1)] - pts[i.saturating_sub(1)]).normalize_or_zero();
            let nrm = vec2(-d.y, d.x);
            let t = i as f32 / (n - 1) as f32;
            let hw = (w(t) * self.u).max(1.0) * 0.5;
            left.push(pts[i] + nrm * hw);
            right.push(pts[i] - nrm * hw);
        }
        let inside = |q: Vec2| q.distance(self.c) <= self.r;
        for i in 0..n - 1 {
            let quad = [left[i], left[i + 1], right[i + 1], right[i]];
            if quad.iter().all(|&q| inside(q)) {
                draw_triangle(left[i], right[i], left[i + 1], c);
                draw_triangle(right[i], right[i + 1], left[i + 1], c);
            } else {
                // Cut to the disc (either way round: a strip can bend).
                let mut q = clip_convex(quad.to_vec(), &self.disc);
                if q.len() < 3 {
                    q = clip_convex(quad.iter().rev().copied().collect(), &self.disc);
                }
                fill(&q, c);
            }
        }
    }

    /// A stroke of even width.
    fn stroke(&self, pts: &[Vec2], w: f32, c: Color) {
        self.ribbon(pts, |_| w, c);
    }

    /// A stroke thinning at both ends.
    fn taper(&self, pts: &[Vec2], w: f32, c: Color) {
        self.ribbon(pts, |t| w * (0.25 + 0.75 * (t * PI).sin()), c);
    }

    fn line(&self, a: (f32, f32), b: (f32, f32), w: f32, c: Color) {
        self.stroke(&[self.p(a.0, a.1), self.p(b.0, b.1)], w, c);
    }

    fn ring(&self, x: f32, y: f32, rad: f32, w: f32, c: Color) {
        let q = self.p(x, y);
        draw_circle_lines(q.x, q.y, rad * self.u, (w * self.u).max(1.0), self.col(c));
    }

    /// A five-pointed (or `n`-pointed) star.
    fn star(&self, (x, y): (f32, f32), outer: f32, inner: f32, n: usize, turn: f32, c: Color) {
        let pts: Vec<(f32, f32)> = (0..2 * n)
            .map(|k| {
                let a = turn - 0.5 * PI + PI * k as f32 / n as f32;
                let rad = if k % 2 == 0 { outer } else { inner };
                (x + a.cos() * rad, y + a.sin() * rad)
            })
            .collect();
        self.fill(self.poly(&pts), c);
    }
}

/// The part of `poly` below the line through `a` and `b` (`a` on the left).
fn below(p: &Pen, poly: Vec<Vec2>, a: (f32, f32), b: (f32, f32)) -> Vec<Vec2> {
    clip_edge(&poly, p.p(a.0, a.1), p.p(b.0, b.1))
}

/// The part of `poly` above the line through `a` and `b` (`a` on the left).
fn above(p: &Pen, poly: Vec<Vec2>, a: (f32, f32), b: (f32, f32)) -> Vec<Vec2> {
    clip_edge(&poly, p.p(b.0, b.1), p.p(a.0, a.1))
}

/// The soft disc behind a character: lighter towards the top left.
fn backdrop(p: &Pen, bg: Color) {
    fill(&p.disc, p.col(shade(bg, -0.42)));
    let steps = ((p.r / 2.5) as usize).clamp(5, 28);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let rad = 1.0 - 0.7 * t;
        let off = vec2(-0.12, -0.3) * t;
        let col = mix(shade(bg, -0.36), shade(bg, 0.16), t.powf(0.7));
        p.oval(off.x * (1.0 - rad) * 2.0, off.y * (1.0 - rad) * 2.0, rad, rad, col);
    }
}

/// The rim around the disc, drawn last.
fn rim(p: &Pen, bg: Color) {
    let w = (0.035 * p.r).max(1.2);
    draw_circle_lines(p.c.x, p.c.y, p.r - w * 0.5, w, p.col(shade(bg, 0.3)));
    if p.shows(30.0) {
        let w2 = 0.02 * p.r;
        draw_circle_lines(
            p.c.x,
            p.c.y,
            p.r - w - w2 * 0.5,
            w2,
            p.col(with_alpha(shade(bg, -0.5), 0.35)),
        );
    }
}

/// The shape of a head.
#[derive(Clone, Copy)]
struct Face {
    skin: Color,
    /// Centre height.
    y: f32,
    /// Half width and half height.
    w: f32,
    h: f32,
    /// How much narrower the chin is than the cheeks (0 to 0.35).
    jaw: f32,
    /// Ear size (1 = normal, 0 = hidden under hair).
    ears: f32,
}

impl Face {
    fn new(skin: [u8; 3]) -> Face {
        Face { skin: rgb(skin), y: -0.06, w: 0.3, h: 0.35, jaw: 0.22, ears: 1.0 }
    }

    fn poly(&self, p: &Pen, dx: f32, dy: f32, grow: f32) -> Vec<Vec2> {
        let n = p.segs(self.h).max(28);
        (0..n)
            .map(|k| {
                let a = TAU * k as f32 / n as f32;
                let (s, co) = a.sin_cos();
                let narrow = if s > 0.0 { 1.0 - self.jaw * s * s } else { 1.0 };
                let x = co * (self.w + grow) * narrow;
                let y = s * (self.h + grow) * if s > 0.0 { 1.0 } else { 1.04 };
                p.p(dx + x, self.y + dy + y)
            })
            .collect()
    }

    fn chin(&self) -> f32 {
        self.y + self.h
    }
}

/// The neck, with the shadow under the chin.
fn neck(p: &Pen, f: &Face, half_w: f32) {
    let skin = f.skin;
    p.fill(p.rect(-half_w, f.chin() - 0.15, half_w, 0.58), shade(skin, -0.14));
    p.oval(0.0, 0.58, half_w, 0.05, shade(skin, -0.14));
    p.oval(0.0, f.chin() - 0.01, half_w * 1.05, 0.07, shade(skin, -0.32));
}

/// Ears, face and its shading.
fn head(p: &Pen, f: &Face) {
    let skin = f.skin;
    if f.ears > 0.0 {
        for s in [-1.0f32, 1.0] {
            let ex = s * (f.w + 0.005);
            let ey = f.y + 0.07;
            p.oval(
                ex,
                ey,
                0.065 * f.ears,
                0.09 * f.ears,
                shade(skin, if s > 0.0 { -0.2 } else { -0.06 }),
            );
            if p.shows(22.0) {
                p.oval(ex + s * 0.018, ey + 0.005, 0.03 * f.ears, 0.05 * f.ears, shade(skin, -0.3));
            }
        }
    }
    let base = f.poly(p, 0.0, 0.0, 0.0);
    fill(&base, p.col(shade(skin, -0.17)));
    let lit = f.poly(p, -0.035, -0.03, 0.0);
    fill(&clip_convex(lit, &base), p.col(skin));
    if p.shows(26.0) {
        // A soft sheen on the forehead and the lit cheek.
        let sheen = with_alpha(shade(skin, 0.3), 0.45);
        p.oval(-0.08, f.y - 0.17, 0.12, 0.07, sheen);
        p.oval(-0.17, f.y + 0.13, 0.05, 0.035, with_alpha(sheen, 0.6));
    }
}

fn blush(p: &Pen, f: &Face, strength: f32) {
    if p.shows(14.0) {
        for s in [-1.0, 1.0] {
            p.oval(s * 0.175, f.y + 0.15, 0.065, 0.038, with_alpha(BLUSH, strength));
        }
    }
}

/// How an eye looks.
#[derive(Clone, Copy)]
struct Eye {
    iris: Color,
    /// 1 wide open, 0.4 narrowed.
    open: f32,
    /// The lid's slant: positive lowers the inner corner (stern, focused).
    slant: f32,
    /// Where the eye looks (-1 to 1 of its size).
    look: (f32, f32),
    /// Lashes at the outer corner.
    lashes: bool,
    size: f32,
}

impl Eye {
    fn new(iris: [u8; 3]) -> Eye {
        Eye { iris: rgb(iris), open: 1.0, slant: 0.0, look: (0.0, 0.0), lashes: false, size: 1.0 }
    }
}

/// One eye at `x, y`; `side` is -1 for the left one (on screen), 1 for the right.
fn eye(p: &Pen, x: f32, y: f32, side: f32, e: &Eye) {
    // An almond, wider than it is tall, the iris filling most of its
    // height: a person's eye. (They were round, taller than wide, with a
    // small iris in a lot of white and a large glint: a doll's stare.)
    let (w, h) = (0.062 * e.size, 0.043 * e.size);
    if !p.shows(19.0) {
        // Small: a dark oval, shorter when narrowed.
        let q = p.p(x, y + h * (1.0 - e.open) * 0.6);
        let rx = (0.045 * p.u).max(0.9);
        let ry = (0.055 * e.open.max(0.5) * p.u).max(1.0);
        draw_ellipse(q.x, q.y, rx, ry, p.col(INK));
        return;
    }
    let white = p.oval_poly(x, y, w, h);
    // The upper lid cuts across the eye.
    let lid_y = y - h + 2.0 * h * (1.0 - e.open);
    let inner = (x - side * w * 1.2, lid_y + e.slant * h);
    let outer = (x + side * w * 1.2, lid_y - e.slant * h);
    let (a, b) = if side < 0.0 { (outer, inner) } else { (inner, outer) };
    let white = below(p, white, a, b);
    fill(&white, p.col(WHITE));
    let ic = (x + e.look.0 * w * 0.35, y + 0.008 + e.look.1 * h * 0.3);
    let ir = h * 0.98;
    let iris = clip_convex(p.oval_poly(ic.0, ic.1, ir * 0.95, ir), &white);
    fill(&iris, p.col(e.iris));
    if p.shows(30.0) {
        let ring = clip_convex(p.oval_poly(ic.0, ic.1 + ir * 0.35, ir * 0.7, ir * 0.55), &white);
        fill(&ring, p.col(shade(e.iris, 0.25)));
    }
    let pupil = clip_convex(p.oval_poly(ic.0, ic.1, ir * 0.48, ir * 0.52), &white);
    fill(&pupil, p.col(INK));
    // The shadow of the lid on the eye.
    let mut top: Vec<Vec2> = white.iter().copied().filter(|q| q.y <= p.p(0.0, y).y + 0.2).collect();
    top.sort_by(|a, b| a.x.total_cmp(&b.x));
    // Highlight.
    let hl = (ic.0 - ir * 0.32, ic.1 - ir * 0.38);
    let hq = p.p(hl.0, hl.1);
    if white.iter().any(|q| q.y < hq.y - 0.5) || e.open > 0.7 {
        draw_circle(hq.x, hq.y, (ir * 0.2 * p.u).max(0.6), p.col(with_alpha(WHITE, 0.9)));
    }
    // The lash line along the top.
    if top.len() >= 2 {
        p.ribbon(&top, |t| 0.017 * (0.6 + 0.4 * (t * PI).sin()), INK);
    }
    if e.lashes {
        let o = p.p(x + side * w * 1.02, y - h * 0.15 + (1.0 - e.open) * h * 0.6);
        let tip = o + vec2(side * 0.05, -0.035 - e.slant * 0.02) * p.u;
        p.taper(&[o, (o + tip) * 0.5 + vec2(0.0, 0.004 * p.u), tip], 0.022, INK);
    }
}

/// A pair of eyes.
fn eyes(p: &Pen, f: &Face, e: &Eye) {
    for s in [-1.0, 1.0] {
        eye(p, s * 0.125, f.y + 0.06, s, e);
    }
}

/// A closed, smiling eye (an arc bowing upwards), or a wink.
fn happy_eye(p: &Pen, x: f32, y: f32, side: f32, lashes: bool) {
    let pts = p.quad((x - 0.065, y + 0.02), (x, y - 0.05), (x + 0.065, y + 0.02));
    p.ribbon(&pts, |t| 0.028 * (0.5 + 0.5 * (t * PI).sin()), INK);
    if lashes && p.shows(24.0) {
        let o = p.p(x + side * 0.062, y + 0.015);
        let tip = o + vec2(side * 0.04, -0.012) * p.u;
        p.taper(&[o, tip], 0.018, INK);
    }
}

/// An eyebrow: `inner` and `outer` heights relative to `y`, `arch` lifts the middle.
#[allow(clippy::too_many_arguments)]
fn brow(
    p: &Pen,
    x: f32,
    y: f32,
    side: f32,
    inner: f32,
    outer: f32,
    arch: f32,
    thick: f32,
    c: Color,
) {
    let a = (x - side * 0.06, y + inner);
    let b = (x + side * 0.065, y + outer);
    let k = (x + side * 0.005, y + (inner + outer) * 0.5 - arch);
    let pts = p.quad(a, k, b);
    p.ribbon(&pts, |t| thick * (1.0 - 0.55 * t), c);
}

/// Both eyebrows, mirrored.
fn brows(p: &Pen, f: &Face, inner: f32, outer: f32, arch: f32, thick: f32, c: Color) {
    for s in [-1.0, 1.0] {
        brow(p, s * 0.125, f.y - 0.045, s, inner, outer, arch, thick, c);
    }
}

/// A nose: the shadow under its tip, the tip and a highlight.
fn nose(p: &Pen, f: &Face, size: f32) {
    let skin = f.skin;
    let y = f.y + 0.15;
    if !p.shows(16.0) {
        p.oval(0.01, y + 0.01, 0.035 * size, 0.02 * size, shade(skin, -0.25));
        return;
    }
    if p.shows(26.0) {
        let bridge = p.quad((0.02, y - 0.09 * size), (0.04 * size, y - 0.02), (0.025, y + 0.01));
        p.taper(&bridge, 0.016 * size, shade(skin, -0.2));
    }
    p.oval(0.006, y + 0.018 * size, 0.045 * size, 0.022 * size, shade(skin, -0.28));
    p.oval(0.0, y, 0.04 * size, 0.03 * size, shade(skin, 0.02));
    if p.shows(26.0) {
        p.oval(-0.012 * size, y - 0.01 * size, 0.014 * size, 0.009 * size, shade(skin, 0.4));
    }
}

/// A closed mouth: a curve from corner to corner. `curve` > 0 smiles;
/// `tilt` > 0 lifts the right corner.
#[allow(clippy::too_many_arguments)]
fn smile(p: &Pen, x: f32, y: f32, w: f32, curve: f32, tilt: f32, thick: f32, c: Color) {
    let a = (x - w, y - curve + tilt);
    let b = (x + w, y - curve - tilt);
    let pts = p.quad(a, (x, y + curve), b);
    p.ribbon(&pts, |t| thick * (0.5 + 0.5 * (t * PI).sin()), c);
}

/// An open mouth: a straight top, a curved bottom; teeth and tongue.
#[allow(clippy::too_many_arguments)]
fn grin(
    p: &Pen,
    f: &Face,
    x: f32,
    y: f32,
    w: f32,
    depth: f32,
    tilt: f32,
    teeth: f32,
    tongue: bool,
) {
    let l = (x - w, y + tilt);
    let r = (x + w, y - tilt);
    let k = (x + tilt * 0.3, y + depth * 2.0);
    // The bottom curve from corner to corner; the polygon closes along the top.
    let mouth = p.quad(r, k, l);
    fill(&mouth, p.col(MOUTH));
    if !p.shows(16.0) {
        return;
    }
    if teeth > 0.0 {
        let t = p.rect(x - w, y - 0.1, x + w, y + depth * teeth);
        fill(&clip_convex(t, &mouth), p.col(WHITE));
    }
    if tongue {
        let tg = p.oval_poly(x + 0.01, y + depth * 1.65, w * 0.55, depth * 0.6);
        fill(&clip_convex(tg, &mouth), p.col(TONGUE));
    }
    if p.shows(26.0) {
        // The lower lip's shadow and the corners.
        let under = p.quad(
            (x - w * 0.45, y + depth * 1.75 + 0.03),
            (x, y + depth * 2.0 + 0.045),
            (x + w * 0.45, y + depth * 1.75 + 0.03),
        );
        p.taper(&under, 0.018, shade(f.skin, -0.18));
        for (s, c) in [(-1.0, l), (1.0, r)] {
            let o = p.p(c.0, c.1);
            let q = o + vec2(s * 0.022, -0.02) * p.u;
            p.taper(&[o, q], 0.014, shade(f.skin, -0.3));
        }
    }
}

/// The outline of the shoulders, `w` wide.
fn torso_poly(p: &Pen, w: f32) -> Vec<Vec2> {
    clip_convex(p.oval_poly(0.0, 1.05, w, 0.56), &p.disc)
}

/// A shirt or top: the shoulders, shaded.
fn torso(p: &Pen, w: f32, c: Color) {
    p.shaded(torso_poly(p, w), (-0.08, -0.02), 0.2, c);
}

/// A waistcoat or vest over the shirt: two panels either side of a V
/// down to `depth`, cut to the shoulders.
fn waistcoat(p: &Pen, w: f32, top: f32, depth: f32, c: Color, lit: Color) {
    let body = torso_poly(p, w);
    for s in [-1.0f32, 1.0] {
        let panel =
            [(s * top, 0.5), (s * 0.01, depth), (s * 0.01, 1.2), (s * 1.0, 1.2), (s * 1.0, 0.3)];
        let panel =
            if s > 0.0 { panel } else { [panel[4], panel[3], panel[2], panel[1], panel[0]] };
        fill(&clip_convex(p.poly(&panel), &body), p.col(if s < 0.0 { lit } else { c }));
    }
}

/// Smiling eyes: the cheeks push up under them.
fn smiling_lids(p: &Pen, f: &Face) {
    if p.shows(19.0) {
        for s in [-1.0f32, 1.0] {
            let x = s * 0.125;
            let y = f.y + 0.06 + 0.074;
            let lid = p.quad((x - 0.1, y + 0.01), (x, y - 0.06), (x + 0.1, y + 0.01));
            p.ribbon(&lid, |_| 0.05, if s < 0.0 { f.skin } else { shade(f.skin, -0.05) });
            let crease = p.quad((x - 0.06, y - 0.005), (x, y - 0.035), (x + 0.06, y - 0.005));
            p.taper(&crease, 0.012, shade(f.skin, -0.25));
        }
    }
}

/// Pointed shirt collars either side of the neck.
fn collar(p: &Pen, c: Color, spread: f32) {
    let line = shade(c, -0.25);
    for s in [-1.0f32, 1.0] {
        let pts = [(s * 0.02, 0.6), (s * (0.15 + spread), 0.68), (s * 0.13, 0.49)];
        let pts = if s < 0.0 { [pts[0], pts[2], pts[1]] } else { pts };
        p.fill(p.poly(&pts), c);
        if p.shows(26.0) {
            let (a, b) = ((s * 0.02, 0.6), (s * (0.15 + spread), 0.68));
            p.line(a, b, 0.012, line);
        }
    }
}

/// Short hair: a cap down to the ears, with the forehead below
/// `hairline` left bare (so the temples show).
fn short_hair(p: &Pen, f: &Face, hair: Color, hairline: f32, sides: f32) {
    let cap = f.poly(p, 0.0, -0.012, 0.022);
    let cap = above(p, cap, (-0.5, f.y + sides), (0.5, f.y + sides));
    p.shaded(cap, (-0.035, -0.03), 0.2, hair);
    let fy = f.y + 0.06;
    let forehead = p.oval_poly(0.0, fy, f.w * 0.78, fy - hairline);
    let face = f.poly(p, 0.0, 0.0, 0.0);
    fill(&clip_convex(forehead, &face), p.col(f.skin));
}

/// Sideburns down the sides of the face, between angles `a0` and `a1` of
/// its outline (measured from the side, downwards).
fn sideburns(p: &Pen, f: &Face, hair: Color, a0: f32, a1: f32, w: f32) {
    for s in [-1.0f32, 1.0] {
        let pts: Vec<Vec2> = (0..=6)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / 6.0;
                let x = s * a.cos() * (f.w - w * 0.4);
                p.p(x, f.y + a.sin() * f.h)
            })
            .collect();
        p.stroke(&pts, w, hair);
    }
}

/// A highlight streak on hair.
fn shine(p: &Pen, pts: &[Vec2], w: f32, c: Color) {
    if p.shows(18.0) {
        p.taper(pts, w, with_alpha(shade(c, 0.45), 0.75));
    }
}

/// The portrait of personality `code` (a friendly robot for any other).
pub fn draw_person(code: &str, c: Vec2, r: f32) {
    let p = Pen::new(c, r);
    let bg = rgb(match code {
        "leo" => [70, 140, 90],
        "nina" => [150, 110, 170],
        "tito" => [40, 120, 80],
        "lola" => [60, 120, 150],
        "bruno" => [170, 70, 40],
        "ines" => [170, 120, 140],
        "marta" => [80, 90, 110],
        "otto" => [110, 80, 50],
        "diego" => [150, 30, 40],
        "rosa" => [150, 40, 70],
        "kenji" => [50, 100, 110],
        "amara" => [30, 110, 100],
        "sofia" => [40, 60, 120],
        "yuri" => [60, 70, 80],
        "viktor" => [92, 70, 120],
        _ => [70, 74, 90],
    });
    backdrop(&p, bg);
    let f = p.figure();
    match code {
        "leo" => leo(&f),
        "nina" => nina(&f),
        "tito" => tito(&f),
        "lola" => lola(&f),
        "bruno" => bruno(&f),
        "ines" => ines(&f),
        "marta" => marta(&f),
        "otto" => otto(&f),
        "diego" => diego(&f),
        "rosa" => rosa(&f),
        "kenji" => kenji(&f),
        "amara" => amara(&f),
        "sofia" => sofia(&f),
        "yuri" => yuri(&f),
        "viktor" => viktor(&f),
        _ => robot(&f),
    }
    rim(&p, bg);
}

/// Tito, the gambler: a tipped-back fedora, a loud palm-print shirt open
/// at the collar, a toothpick and one eyebrow up: "double or nothing?"
fn tito(p: &Pen) {
    let f = Face { jaw: 0.18, ..Face::new([222, 172, 130]) };
    let hair = rgb([58, 40, 30]);
    let shirt = rgb([235, 120, 60]);
    torso(p, 0.8, shirt);
    if p.shows(22.0) {
        // Palm leaves on the shirt.
        for (x, y, a) in
            [(-0.45, 0.75, 0.6), (0.4, 0.8, -0.5), (-0.2, 0.95, 0.2), (0.15, 0.68, -0.9)]
        {
            p.fill(p.oval_rot(x, y, 0.11, 0.04, a), rgb([40, 150, 90]));
            p.fill(p.oval_rot(x + 0.04, y + 0.03, 0.09, 0.03, a + 0.7), rgb([30, 120, 70]));
        }
    }
    neck(p, &f, 0.11);
    // The open collar.
    for s in [-1.0f32, 1.0] {
        p.fill(p.poly(&[(s * 0.04, 0.55), (s * 0.24, 0.5), (s * 0.18, 0.72)]), shade(shirt, 0.15));
    }
    head(p, &f);
    sideburns(p, &f, hair, -0.1, 0.25, 0.05);
    // The fedora, pushed back: the crown, a black band, the brim.
    let felt = rgb([200, 180, 140]);
    let crown = p.round_rect(-0.25, -0.58, 0.25, -0.27, 0.1);
    p.shaded(crown, (-0.03, -0.02), 0.15, felt);
    p.fill(p.rect(-0.25, -0.36, 0.25, -0.28), rgb([40, 34, 30]));
    p.oval(0.0, -0.265, 0.43, 0.068, shade(felt, -0.2));
    p.oval(0.0, -0.28, 0.42, 0.06, shade(felt, -0.05));
    if p.shows(24.0) {
        p.line((0.12, -0.36), (0.19, -0.44), 0.03, rgb([220, 60, 50]));
    }
    // One brow up, one eye narrowed: the bluff.
    eyes(p, &f, &Eye { open: 0.8, look: (0.35, 0.0), ..Eye::new([70, 110, 60]) });
    brow(p, -0.125, f.y - 0.06, -1.0, 0.0, 0.02, 0.02, 0.035, hair);
    brow(p, 0.125, f.y - 0.12, 1.0, 0.0, -0.05, 0.06, 0.035, hair);
    nose(p, &f, 1.05);
    // A lopsided smile with a toothpick.
    smile(p, 0.03, f.y + 0.24, 0.1, 0.03, -0.04, 0.03, rgb([140, 70, 60]));
    p.line((0.1, f.y + 0.23), (0.28, f.y + 0.17), 0.018, rgb([230, 205, 160]));
    blush(p, &f, 0.4);
}

/// Inês, the soft touch: dark hair in a low bun, pearl earrings, a lilac
/// cardigan over a white blouse and a calm, quiet smile.
fn ines(p: &Pen) {
    let f = Face { jaw: 0.27, ears: 0.9, ..Face::new([236, 196, 168]) };
    let hair = rgb([46, 30, 26]);
    let cardigan = rgb([170, 140, 200]);
    // The bun, low at the back.
    p.oval(0.0, 0.18, 0.2, 0.15, shade(hair, -0.2));
    torso(p, 0.7, cardigan);
    p.fill(p.poly(&[(-0.16, 0.5), (0.16, 0.5), (0.0, 0.95)]), SHIRT_WHITE);
    if p.shows(20.0) {
        for y in [0.68, 0.82, 0.96] {
            p.oval(0.13, y, 0.018, 0.018, shade(cardigan, 0.3));
        }
    }
    neck(p, &f, 0.09);
    head(p, &f);
    // Hair parted in the middle, smoothed back.
    let top = p.oval_poly(0.0, -0.14, 0.34, 0.31);
    let top = above(p, top, (-0.4, -0.04), (0.4, -0.04));
    p.shaded(top, (-0.03, -0.03), 0.15, hair);
    for s in [-1.0f32, 1.0] {
        p.fill(p.oval_rot(s * 0.27, -0.06, 0.07, 0.17, s * 0.2), hair);
    }
    if p.shows(22.0) {
        p.line((0.0, -0.44), (0.0, -0.26), 0.012, shade(hair, 0.25));
        shine(p, &p.arc(0.0, -0.15, 0.26, 0.24, 1.15 * PI, 1.4 * PI), 0.025, hair);
    }
    // Pearls.
    for s in [-1.0f32, 1.0] {
        p.oval(s * 0.3, f.y + 0.1, 0.025, 0.025, rgb([245, 240, 230]));
    }
    eyes(p, &f, &Eye { open: 0.75, lashes: true, look: (0.0, 0.25), ..Eye::new([90, 60, 40]) });
    brows(p, &f, -0.065, -0.07, 0.03, 0.022, shade(hair, 0.1));
    nose(p, &f, 0.8);
    smile(p, 0.0, f.y + 0.24, 0.08, 0.025, 0.0, 0.025, rgb([170, 80, 90]));
    blush(p, &f, 0.9);
}

/// Diego, the showman: a high glossy quiff, a red stage jacket with a bow
/// tie, a wink and the widest grin in the hall.
fn diego(p: &Pen) {
    let f = Face { jaw: 0.2, ..Face::new([206, 150, 108]) };
    let hair = rgb([24, 20, 22]);
    let jacket = rgb([190, 30, 45]);
    torso(p, 0.85, jacket);
    p.fill(p.poly(&[(-0.17, 0.5), (0.17, 0.5), (0.0, 0.9)]), SHIRT_WHITE);
    for s in [-1.0f32, 1.0] {
        // Satin lapels.
        p.fill(p.poly(&[(s * 0.17, 0.5), (s * 0.3, 0.6), (s * 0.05, 0.98)]), shade(jacket, -0.25));
    }
    if p.shows(22.0) {
        for (x, y) in [(-0.5, 0.8), (0.48, 0.85), (-0.35, 0.95), (0.33, 0.7)] {
            p.star((x, y), 0.025, 0.01, 4, 0.0, rgb([255, 220, 150]));
        }
    }
    neck(p, &f, 0.11);
    // The bow tie.
    let tie = rgb([20, 20, 24]);
    p.fill(p.poly(&[(0.0, 0.57), (-0.12, 0.51), (-0.12, 0.63)]), tie);
    p.fill(p.poly(&[(0.0, 0.57), (0.12, 0.51), (0.12, 0.63)]), tie);
    p.oval(0.0, 0.57, 0.03, 0.03, shade(tie, 0.2));
    head(p, &f);
    // The quiff: swept up and forward.
    short_hair(p, &f, hair, f.y - 0.22, 0.02);
    p.shaded(p.oval_rot(0.05, -0.43, 0.27, 0.13, -0.25), (-0.03, -0.03), 0.2, hair);
    if p.shows(20.0) {
        shine(p, &p.quad((-0.15, -0.45), (0.05, -0.55), (0.25, -0.45)), 0.03, hair);
    }
    // A wink.
    eye(p, -0.125, f.y + 0.07, -1.0, &Eye { look: (0.3, 0.0), ..Eye::new([60, 40, 30]) });
    happy_eye(p, 0.125, f.y + 0.07, 1.0, false);
    brows(p, &f, -0.08, -0.1, 0.06, 0.035, hair);
    nose(p, &f, 1.0);
    grin(p, &f, 0.0, f.y + 0.23, 0.15, 0.065, -0.02, 0.4, true);
    // A thin moustache.
    let mo = p.quad((-0.1, f.y + 0.205), (0.0, f.y + 0.18), (0.1, f.y + 0.205));
    p.stroke(&mo, 0.018, hair);
}

/// Amara, the tactician: a patterned head wrap, a teal blazer with a chess
/// knight pin, and a steady, calculating look.
fn amara(p: &Pen) {
    let f = Face { jaw: 0.25, ears: 0.0, ..Face::new([128, 84, 58]) };
    let wrap = rgb([235, 170, 50]);
    let blazer = rgb([30, 120, 120]);
    torso(p, 0.76, blazer);
    p.fill(p.poly(&[(-0.15, 0.5), (0.15, 0.5), (0.0, 0.85)]), rgb([30, 30, 36]));
    for s in [-1.0f32, 1.0] {
        p.fill(p.poly(&[(s * 0.15, 0.5), (s * 0.28, 0.62), (s * 0.06, 0.95)]), shade(blazer, -0.2));
    }
    if p.shows(20.0) {
        // The knight: a little silver horse head.
        let (x, y) = (-0.3, 0.72);
        p.fill(
            p.poly(&[
                (x - 0.04, y + 0.06),
                (x - 0.03, y - 0.04),
                (x + 0.03, y - 0.06),
                (x + 0.05, y),
            ]),
            rgb([215, 215, 225]),
        );
        p.fill(p.rect(x - 0.05, y + 0.05, x + 0.05, y + 0.08), rgb([215, 215, 225]));
    }
    neck(p, &f, 0.1);
    head(p, &f);
    // The wrap, tall at the back, with a pattern.
    let wrap_poly = p.oval_poly(0.0, -0.3, 0.36, 0.26);
    let wrap_poly = above(p, wrap_poly, (-0.42, -0.1), (0.42, -0.14));
    p.shaded(wrap_poly, (-0.03, -0.03), 0.18, wrap);
    p.oval(0.12, -0.5, 0.15, 0.1, shade(wrap, -0.1));
    if p.shows(22.0) {
        for (x, y) in [(-0.2, -0.3), (-0.05, -0.38), (0.12, -0.3), (0.25, -0.22), (-0.1, -0.2)] {
            p.oval(x, y, 0.028, 0.028, rgb([170, 40, 50]));
        }
    }
    // Gold hoops.
    for s in [-1.0f32, 1.0] {
        p.ring(s * 0.31, f.y + 0.13, 0.04, 0.012, GOLD_METAL);
    }
    eyes(p, &f, &Eye { open: 0.82, slant: 0.2, lashes: true, ..Eye::new([60, 36, 24]) });
    brows(p, &f, -0.04, -0.07, 0.02, 0.03, rgb([30, 20, 18]));
    nose(p, &f, 1.0);
    smile(p, 0.0, f.y + 0.245, 0.075, 0.01, 0.0, 0.03, rgb([120, 50, 50]));
}

/// Yuri, the machine: a platinum buzz cut, a black turtleneck, an earpiece
/// and a face that shows nothing at all.
fn yuri(p: &Pen) {
    let f = Face { w: 0.29, h: 0.36, jaw: 0.14, ..Face::new([238, 214, 200]) };
    let hair = rgb([225, 222, 210]);
    let sweater = rgb([30, 30, 34]);
    torso(p, 0.72, sweater);
    // The turtleneck.
    p.fill(p.round_rect(-0.15, 0.4, 0.15, 0.6, 0.05), shade(sweater, 0.12));
    if p.shows(20.0) {
        for y in [0.45, 0.5, 0.55] {
            p.line((-0.14, y), (0.14, y), 0.008, shade(sweater, 0.25));
        }
    }
    head(p, &f);
    short_hair(p, &f, hair, f.y - 0.24, 0.0);
    // The earpiece.
    p.oval(0.31, f.y + 0.05, 0.03, 0.045, rgb([40, 40, 46]));
    p.line((0.31, f.y + 0.09), (0.26, f.y + 0.3), 0.012, rgb([60, 60, 66]));
    eyes(p, &f, &Eye { open: 0.7, slant: 0.1, size: 0.9, ..Eye::new([120, 140, 150]) });
    brows(p, &f, -0.08, -0.08, 0.0, 0.025, shade(hair, -0.25));
    nose(p, &f, 0.9);
    // A straight line of a mouth.
    p.line((-0.07, f.y + 0.245), (0.07, f.y + 0.245), 0.022, rgb([150, 100, 100]));
}

/// Leo, the rookie: a cap worn a little crooked, freckles, a plaster on
/// the nose and a big eager grin.
fn leo(p: &Pen) {
    let f = Face { ears: 1.15, ..Face::new([240, 196, 158]) };
    let hair = rgb([176, 96, 44]);
    let cap = rgb([206, 58, 42]);
    torso(p, 0.7, rgb([40, 110, 200]));
    neck(p, &f, 0.1);
    // The crew neck.
    let rib = p.arc(0.0, 0.53, 0.14, 0.075, 0.0, PI);
    p.stroke(&rib, 0.045, rgb([235, 240, 250]));
    head(p, &f);
    // Tufts of hair under the cap.
    for s in [-1.0f32, 1.0] {
        let t = [(s * 0.32, -0.2), (s * 0.35, -0.06), (s * 0.27, -0.13)];
        p.fill(p.poly(&if s < 0.0 { t } else { [t[0], t[2], t[1]] }), hair);
    }
    for (x, len) in [(-0.13, 0.08), (-0.06, 0.1), (0.02, 0.07)] {
        p.fill(p.poly(&[(x + 0.05, -0.2), (x + 0.01, -0.2 + len), (x - 0.04, -0.2)]), hair);
    }
    // The crown.
    let crown = p.oval_poly(0.0, -0.2, 0.34, 0.3);
    let crown = above(p, crown, (-0.4, -0.17), (0.4, -0.22));
    let crown = clip_convex(crown, &p.disc);
    fill(&crown, p.col(shade(cap, -0.18)));
    let lit: Vec<Vec2> = crown.iter().map(|&q| q + vec2(-0.05, -0.03) * p.u).collect();
    fill(&clip_convex(lit, &crown), p.col(cap));
    if p.shows(24.0) {
        for s in [-1.0, 1.0] {
            let seam = p.quad((0.0, -0.49), (s * 0.16, -0.42), (s * 0.2, -0.2));
            p.stroke(&seam, 0.012, shade(cap, -0.3));
        }
        shine(p, &p.arc(0.0, -0.2, 0.27, 0.22, 1.1 * PI, 1.35 * PI), 0.03, cap);
    }
    p.oval(0.0, -0.49, 0.035, 0.022, shade(cap, -0.25));
    // A white star on the front.
    p.star((0.0, -0.33), 0.085, 0.036, 5, 0.0, WHITE);
    // The brim, tipped up a little.
    let brim_l = (-0.36, -0.165);
    let brim_r = (0.38, -0.225);
    let mut brim = p.quad(brim_r, (0.03, -0.07), brim_l);
    brim.extend(p.quad(brim_l, (0.0, -0.21), brim_r).into_iter().skip(1));
    brim.pop();
    p.fill(brim.clone(), shade(cap, -0.08));
    if p.shows(24.0) {
        let edge = p.quad(brim_r, (0.03, -0.07), brim_l);
        p.stroke(&edge, 0.02, shade(cap, -0.35));
    }
    eyes(p, &f, &Eye { look: (0.0, -0.2), size: 1.12, ..Eye::new([70, 120, 60]) });
    brows(p, &f, -0.035, -0.015, 0.03, 0.035, shade(hair, -0.1));
    nose(p, &f, 0.9);
    // Freckles.
    if p.shows(24.0) {
        let fr = shade(f.skin, -0.32);
        for (x, y) in [
            (-0.2, 0.09),
            (-0.15, 0.12),
            (-0.22, 0.14),
            (-0.1, 0.1),
            (0.2, 0.09),
            (0.15, 0.12),
            (0.22, 0.14),
            (0.1, 0.1),
        ] {
            let q = p.p(x, y + f.y);
            draw_circle(q.x, q.y, (0.011 * p.u).max(0.7), p.col(fr));
        }
    }
    blush(p, &f, 0.9);
    grin(p, &f, 0.0, f.y + 0.235, 0.12, 0.058, 0.0, 0.38, true);
    // A plaster on the cheek.
    if p.shows(16.0) {
        let pl = p.oval_rot(0.19, f.y + 0.2, 0.055, 0.022, -0.5);
        p.fill(pl, rgb([236, 205, 160]));
        if p.shows(28.0) {
            for d in [-0.015, 0.015] {
                let q = p.p(0.19 + d, f.y + 0.2 - d * 0.5);
                draw_circle(q.x, q.y, 0.006 * p.u, p.col(rgb([200, 160, 120])));
            }
        }
    }
}

/// Nina, the weekend player: long wavy hair, a headband, a hoodie and a
/// happy, eyes-closed laugh.
fn nina(p: &Pen) {
    let f = Face { ears: 0.0, jaw: 0.26, ..Face::new([246, 206, 176]) };
    let hair = rgb([128, 74, 42]);
    let hoodie = rgb([232, 150, 60]);
    // Long hair behind.
    p.oval(0.0, 0.1, 0.43, 0.52, shade(hair, -0.1));
    for (x, y) in [(-0.4, 0.3), (-0.37, 0.48), (0.4, 0.3), (0.37, 0.48), (-0.32, 0.6), (0.32, 0.6)]
    {
        p.oval(x, y, 0.11, 0.12, shade(hair, -0.1));
    }
    p.oval(0.0, 0.25, 0.3, 0.38, shade(hair, -0.35));
    // The hood around the neck, then the hoodie.
    torso(p, 0.72, hoodie);
    p.oval(0.0, 0.53, 0.3, 0.12, shade(hoodie, -0.3));
    neck(p, &f, 0.1);
    p.fill(p.poly(&[(0.1, 0.56), (0.0, 0.66), (-0.1, 0.56), (0.0, 0.5)]), shade(f.skin, -0.14));
    if p.shows(18.0) {
        for s in [-1.0, 1.0] {
            p.line((s * 0.07, 0.62), (s * 0.08, 0.85), 0.018, WHITE);
            p.oval(s * 0.08, 0.86, 0.016, 0.025, shade(WHITE, -0.2));
        }
    }
    head(p, &f);
    // Wavy locks framing the face.
    for s in [-1.0f32, 1.0] {
        let lock = p.oval_rot(s * 0.29, 0.05, 0.075, 0.28, s * -0.12);
        p.fill(lock, hair);
    }
    // Hair on top, side-swept bangs.
    let top = p.oval_poly(0.0, -0.13, 0.35, 0.33);
    let top = above(p, top, (-0.4, -0.08), (0.4, -0.24));
    p.shaded(top, (-0.04, -0.03), 0.15, hair);
    p.fill(p.poly(&[(0.3, -0.25), (0.33, -0.05), (0.05, -0.22)]), hair);
    p.fill(p.poly(&[(-0.15, -0.16), (-0.31, -0.02), (-0.3, -0.14)]), hair);
    // The headband.
    let band = p.arc(0.0, -0.08, 0.34, 0.36, 1.08 * PI, 1.92 * PI);
    p.stroke(&band, 0.06, rgb([70, 196, 186]));
    if p.shows(24.0) {
        let band2 = p.arc(0.0, -0.08, 0.34, 0.36, 1.12 * PI, 1.4 * PI);
        p.taper(&band2, 0.02, rgb([170, 240, 230]));
        shine(p, &p.arc(0.0, -0.12, 0.28, 0.25, 1.15 * PI, 1.45 * PI), 0.03, hair);
    }
    for s in [-1.0, 1.0] {
        happy_eye(p, s * 0.125, f.y + 0.07, s, true);
    }
    brows(p, &f, -0.07, -0.06, 0.03, 0.025, shade(hair, -0.15));
    nose(p, &f, 0.8);
    blush(p, &f, 1.4);
    grin(p, &f, 0.0, f.y + 0.235, 0.115, 0.056, 0.0, 0.35, true);
}

/// Lola, the bank artist: a high bun held by a paintbrush, cat-eye liner,
/// a paint smudge on the cheek and a knotted neckerchief.
fn lola(p: &Pen) {
    let f = Face { jaw: 0.28, ..Face::new([204, 152, 116]) };
    let hair = rgb([34, 24, 22]);
    let top = rgb([200, 50, 80]);
    torso(p, 0.66, top);
    neck(p, &f, 0.09);
    // The neckerchief: a band and a knot with two ends.
    let scarf = rgb([240, 190, 60]);
    let band = p.arc(0.0, 0.5, 0.14, 0.07, 0.05 * PI, 0.95 * PI);
    p.stroke(&band, 0.06, scarf);
    p.fill(p.poly(&[(0.16, 0.56), (0.25, 0.74), (0.12, 0.66)]), shade(scarf, -0.15));
    p.fill(p.poly(&[(0.14, 0.57), (0.12, 0.8), (0.05, 0.66)]), scarf);
    p.oval(0.13, 0.57, 0.04, 0.035, shade(scarf, -0.1));
    if p.shows(26.0) {
        for (x, y) in [(-0.08, 0.57), (0.0, 0.59), (0.08, 0.58), (0.12, 0.7)] {
            p.oval(x, y, 0.012, 0.012, with_alpha(top, 0.8));
        }
    }
    head(p, &f);
    // Sleek hair pulled up, a centre parting swept to both sides.
    short_hair(p, &f, hair, f.y - 0.17, 0.02);
    for s in [-1.0f32, 1.0] {
        let wing = p.oval_rot(s * 0.15, -0.27, 0.17, 0.075, s * 0.42);
        p.fill(wing, hair);
    }
    if p.shows(22.0) {
        p.line((0.0, -0.42), (0.0, -0.33), 0.012, shade(hair, 0.3));
    }
    // The bun and the paintbrush through it.
    if p.shows(14.0) {
        p.line((-0.3, -0.62), (0.32, -0.36), 0.03, rgb([180, 40, 60]));
        p.line((0.17, -0.425), (0.25, -0.39), 0.035, rgb([200, 200, 210]));
        let tip = p.oval_rot(0.3, -0.37, 0.06, 0.022, 0.4);
        p.fill(tip, rgb([60, 200, 210]));
    }
    p.shaded(p.oval_poly(0.03, -0.47, 0.16, 0.13), (-0.03, -0.03), 0.2, hair);
    if p.shows(20.0) {
        shine(p, &p.arc(0.03, -0.47, 0.11, 0.08, 1.05 * PI, 1.6 * PI), 0.025, hair);
        shine(p, &p.arc(0.0, -0.08, 0.28, 0.3, 1.18 * PI, 1.42 * PI), 0.03, hair);
        shine(p, &p.arc(0.0, -0.08, 0.28, 0.3, 1.6 * PI, 1.8 * PI), 0.02, hair);
    }
    if p.shows(24.0) {
        p.line((-0.14, -0.55), (-0.09, -0.53), 0.03, rgb([180, 40, 60]));
        // A loose curl in front of the ear.
        for s in [-1.0f32, 1.0] {
            let curl = p.quad((s * 0.28, -0.08), (s * 0.34, 0.05), (s * 0.27, 0.15));
            p.taper(&curl, 0.025, hair);
        }
    }
    // Drop earrings.
    for s in [-1.0f32, 1.0] {
        p.oval(s * 0.315, f.y + 0.2, 0.025, 0.025, GOLD_METAL);
        p.oval(s * 0.315, f.y + 0.26, 0.03, 0.04, rgb([60, 200, 210]));
    }
    eyes(p, &f, &Eye { lashes: true, open: 0.85, look: (0.4, 0.0), ..Eye::new([90, 60, 40]) });
    brow(p, -0.125, f.y - 0.05, -1.0, -0.05, -0.04, 0.045, 0.028, hair);
    brow(p, 0.125, f.y - 0.045, 1.0, -0.035, -0.03, 0.025, 0.028, hair);
    nose(p, &f, 0.85);
    blush(p, &f, 0.8);
    // Lips: a playful smile.
    let lip = rgb([196, 40, 70]);
    smile(p, 0.0, f.y + 0.245, 0.085, 0.02, 0.012, 0.035, lip);
    if p.shows(26.0) {
        p.oval(0.0, f.y + 0.27, 0.035, 0.012, shade(lip, 0.3));
    }
    // A smudge of paint.
    if p.shows(18.0) {
        p.oval(-0.19, f.y + 0.2, 0.03, 0.02, rgb([60, 200, 210]));
        p.oval(-0.155, f.y + 0.215, 0.018, 0.014, rgb([250, 200, 60]));
    }
}

/// Bruno, the cannon: broad shoulders, a buzz cut, a full beard, a scar
/// through the eyebrow and an explosive grin.
fn bruno(p: &Pen) {
    let f = Face { w: 0.32, h: 0.34, jaw: 0.08, ears: 1.05, ..Face::new([214, 158, 118]) };
    let hair = rgb([44, 32, 26]);
    let shirt = rgb([62, 62, 70]);
    torso(p, 0.92, shirt);
    neck(p, &f, 0.17);
    // A crew neck and an orange burst on the chest.
    let rib = p.arc(0.0, 0.52, 0.2, 0.085, 0.0, PI);
    p.stroke(&rib, 0.04, shade(shirt, -0.3));
    p.star((-0.3, 0.72), 0.11, 0.055, 8, 0.2, rgb([250, 150, 40]));
    p.star((-0.3, 0.72), 0.06, 0.032, 8, 0.0, rgb([255, 225, 90]));
    head(p, &f);
    // The buzz cut: a short dark cap.
    let buzz = mix(hair, f.skin, 0.22);
    short_hair(p, &f, buzz, f.y - 0.21, 0.0);
    if p.shows(26.0) {
        for i in 0..14 {
            let a = PI * (1.1 + 0.8 * i as f32 / 13.0);
            let q = p.p(a.cos() * 0.25, -0.24 + a.sin() * 0.14);
            draw_circle(q.x, q.y, 0.008 * p.u, p.col(shade(hair, 0.15)));
        }
    }
    // The beard.
    let beard = f.poly(p, 0.0, 0.01, 0.02);
    let beard = below(p, beard, (-0.5, f.y + 0.1), (0.5, f.y + 0.1));
    p.shaded(beard, (-0.04, -0.03), 0.18, hair);
    sideburns(p, &f, hair, -0.05, 0.4, 0.06);
    // The cheeks above the beard line.
    for s in [-1.0f32, 1.0] {
        p.oval(
            s * 0.15,
            f.y + 0.13,
            0.12,
            0.06,
            if s < 0.0 { f.skin } else { shade(f.skin, -0.05) },
        );
    }
    if p.shows(28.0) {
        shine(p, &p.arc(0.0, f.y + 0.05, 0.27, 0.27, 0.6 * PI, 0.9 * PI), 0.025, hair);
    }
    eyes(p, &f, &Eye { open: 0.8, slant: 0.35, size: 0.9, ..Eye::new([90, 60, 35]) });
    brows(p, &f, 0.0, -0.055, 0.0, 0.05, hair);
    // A scar through the left eyebrow.
    if p.shows(22.0) {
        p.line((-0.165, f.y - 0.1), (-0.145, f.y - 0.04), 0.016, shade(f.skin, 0.1));
    }
    nose(p, &f, 1.25);
    // Moustache and the grin.
    grin(p, &f, 0.0, f.y + 0.245, 0.14, 0.045, 0.0, 0.42, false);
    let mo = p.quad((-0.17, f.y + 0.27), (0.0, f.y + 0.17), (0.17, f.y + 0.27));
    p.ribbon(&mo, |t| 0.06 * (0.35 + 0.65 * (t * PI).sin()), shade(hair, 0.08));
}

/// Marta, the wall: a neat grey bob with a blunt fringe, reading glasses
/// on a chain, a cardigan and a mouth that gives nothing away.
fn marta(p: &Pen) {
    let f = Face { jaw: 0.24, ears: 0.0, ..Face::new([236, 200, 172]) };
    let hair = rgb([150, 150, 158]);
    let cardigan = rgb([72, 96, 72]);
    // The bob behind the face.
    let bob = p.oval_poly(0.0, -0.06, 0.4, 0.42);
    let bob = half(bob, p.p(0.0, 0.26).y, true);
    p.shaded(bob, (-0.03, -0.02), 0.25, hair);
    torso(p, 0.7, cardigan);
    neck(p, &f, 0.09);
    // A blouse with a round collar inside the cardigan's V.
    p.fill(p.poly(&[(0.2, 0.5), (0.0, 0.95), (-0.2, 0.5)]), SHIRT_WHITE);
    for s in [-1.0f32, 1.0] {
        p.oval(s * 0.08, 0.57, 0.1, 0.06, shade(SHIRT_WHITE, -0.04));
    }
    if p.shows(20.0) {
        for s in [-1.0f32, 1.0] {
            p.line((s * 0.2, 0.5), (s * 0.02, 0.97), 0.03, shade(cardigan, 0.15));
        }
        for y in [0.72, 0.84] {
            p.oval(0.06, y, 0.02, 0.02, rgb([210, 190, 150]));
        }
    }
    head(p, &f);
    // The blunt fringe and the sides.
    let top = p.oval_poly(0.0, -0.1, 0.36, 0.35);
    let top = half(top, p.p(0.0, -0.15).y, true);
    p.shaded(top, (-0.04, -0.03), 0.12, hair);
    for s in [-1.0f32, 1.0] {
        let side = p.oval_rot(s * 0.3, 0.0, 0.08, 0.26, s * -0.08);
        p.fill(side, if s < 0.0 { hair } else { shade(hair, -0.12) });
    }
    if p.shows(20.0) {
        shine(p, &p.arc(0.0, -0.12, 0.28, 0.25, 1.15 * PI, 1.5 * PI), 0.035, hair);
        // Pearl earrings below the hair.
        for s in [-1.0f32, 1.0] {
            p.oval(s * 0.31, f.y + 0.24, 0.022, 0.022, rgb([245, 240, 230]));
        }
    }
    eyes(
        p,
        &f,
        &Eye { open: 0.62, slant: 0.1, size: 0.9, look: (0.0, 0.2), ..Eye::new([80, 100, 120]) },
    );
    brows(p, &f, 0.01, -0.01, 0.0, 0.03, shade(hair, -0.35));
    // Glasses: thin rectangles, and their chain.
    let frame = rgb([110, 40, 50]);
    for s in [-1.0f32, 1.0] {
        let x = s * 0.125;
        let y = f.y + 0.065;
        let lens = p.round_rect(x - 0.085, y - 0.055, x + 0.085, y + 0.055, 0.02);
        let mut closed = lens.clone();
        closed.push(lens[0]);
        fill(&lens, p.col(Color::new(1.0, 1.0, 1.0, 0.1)));
        p.stroke(&closed, 0.02, frame);
        if p.shows(20.0) {
            let chain = p.quad((s * 0.21, y), (s * 0.3, 0.35), (s * 0.24, 0.62));
            for q in chain.iter() {
                draw_circle(q.x, q.y, (0.009 * p.u).max(0.6), p.col(GOLD_METAL));
            }
        }
    }
    p.line((-0.04, f.y + 0.06), (0.04, f.y + 0.06), 0.018, frame);
    nose(p, &f, 0.9);
    if p.shows(26.0) {
        for s in [-1.0f32, 1.0] {
            let fold =
                p.quad((s * 0.08, f.y + 0.19), (s * 0.12, f.y + 0.24), (s * 0.1, f.y + 0.28));
            p.taper(&fold, 0.012, shade(f.skin, -0.2));
        }
    }
    smile(p, 0.0, f.y + 0.255, 0.07, -0.008, 0.0, 0.028, rgb([170, 90, 90]));
    blush(p, &f, 0.4);
}

/// Otto, the old pro: a tweed flat cap, white tufts and a magnificent
/// moustache, smiling eyes, a waistcoat and a watch chain.
fn otto(p: &Pen) {
    let f = Face { jaw: 0.18, ears: 1.2, ..Face::new([232, 188, 158]) };
    let white = rgb([236, 236, 232]);
    let tweed = rgb([128, 104, 78]);
    let waistcoat_c = rgb([124, 42, 42]);
    torso(p, 0.72, SHIRT_WHITE);
    neck(p, &f, 0.1);
    // The waistcoat: two panels leaving a V for the shirt and tie.
    waistcoat(p, 0.72, 0.17, 0.88, shade(waistcoat_c, -0.18), waistcoat_c);
    p.fill(p.poly(&[(0.04, 0.55), (0.0, 0.85), (-0.04, 0.55)]), rgb([40, 70, 50]));
    p.oval(0.0, 0.55, 0.04, 0.03, rgb([40, 70, 50]));
    collar(p, SHIRT_WHITE, 0.0);
    if p.shows(20.0) {
        for y in [0.9, 1.0] {
            p.oval(-0.06, y - 0.05, 0.018, 0.018, GOLD_METAL);
        }
        let chain = p.quad((-0.06, 0.92), (-0.2, 1.0), (-0.32, 0.88));
        p.stroke(&chain, 0.012, GOLD_METAL);
    }
    // White tufts behind the ears.
    for s in [-1.0f32, 1.0] {
        p.oval(s * 0.3, -0.1, 0.075, 0.09, shade(white, -0.08));
        p.oval(s * 0.33, -0.02, 0.05, 0.07, shade(white, -0.15));
    }
    head(p, &f);
    // The flat cap: a wide crown sloping to a short brim.
    let crown = p.oval_poly(0.02, -0.31, 0.37, 0.165);
    p.shaded(crown, (-0.05, -0.04), 0.2, tweed);
    if p.shows(30.0) {
        for i in 0..9 {
            let x = -0.3 + i as f32 * 0.075;
            let a = (x, -0.42 + (x * 2.0).powi(2) * 0.04);
            p.line(a, (a.0 + 0.04, a.1 + 0.12), 0.01, with_alpha(shade(tweed, -0.25), 0.6));
        }
    }
    let mut brim = p.quad((0.36, -0.23), (0.03, -0.09), (-0.33, -0.22));
    brim.extend(p.quad((-0.33, -0.22), (0.03, -0.2), (0.36, -0.23)).into_iter().skip(1));
    brim.pop();
    p.fill(brim, shade(tweed, -0.25));
    p.oval(0.02, -0.465, 0.03, 0.018, shade(tweed, -0.2));
    if p.shows(22.0) {
        shine(p, &p.arc(0.02, -0.31, 0.3, 0.12, 1.1 * PI, 1.45 * PI), 0.03, tweed);
    }
    // Smiling eyes with crow's feet, under bushy white brows.
    eyes(p, &f, &Eye { open: 0.8, size: 0.9, ..Eye::new([90, 120, 150]) });
    smiling_lids(p, &f);
    if p.shows(26.0) {
        for s in [-1.0f32, 1.0] {
            for k in [-1.0, 0.0, 1.0] {
                let o = (s * 0.21, f.y + 0.06 + k * 0.025);
                p.line(o, (o.0 + s * 0.04, o.1 + k * 0.02), 0.01, shade(f.skin, -0.25));
            }
        }
    }
    for s in [-1.0f32, 1.0] {
        let b = p.quad((s * 0.06, f.y - 0.01), (s * 0.13, f.y - 0.075), (s * 0.23, f.y + 0.0));
        p.ribbon(&b, |t| 0.055 * (1.0 - 0.3 * t), white);
    }
    nose(p, &f, 1.3);
    p.oval(0.0, f.y + 0.15, 0.04, 0.025, with_alpha(BLUSH, 1.2));
    blush(p, &f, 1.0);
    // A small smile under the moustache.
    smile(p, 0.0, f.y + 0.275, 0.05, 0.015, 0.0, 0.022, rgb([150, 70, 70]));
    // The moustache: two sweeping halves curling up at the ends.
    for s in [-1.0f32, 1.0] {
        let m = p.quad((s * 0.01, f.y + 0.19), (s * 0.18, f.y + 0.28), (s * 0.24, f.y + 0.17));
        p.ribbon(&m, |t| 0.085 * (1.0 - t * 0.85), white);
        if p.shows(26.0) {
            let m2 = p.quad((s * 0.03, f.y + 0.2), (s * 0.15, f.y + 0.26), (s * 0.2, f.y + 0.2));
            p.taper(&m2, 0.012, shade(white, -0.15));
        }
    }
}

/// Rosa, the hustler: a cloud of curls, gold hoops, a wink and a smirk,
/// and a mustard jacket.
fn rosa(p: &Pen) {
    let f = Face { jaw: 0.25, ears: 0.9, ..Face::new([168, 112, 82]) };
    let hair = rgb([28, 20, 20]);
    let jacket = rgb([232, 186, 66]);
    // The curls behind.
    let curls = [
        (-0.38, -0.25),
        (-0.27, -0.42),
        (-0.1, -0.5),
        (0.1, -0.5),
        (0.27, -0.42),
        (0.38, -0.25),
        (-0.45, -0.05),
        (0.45, -0.05),
        (-0.43, 0.15),
        (0.43, 0.15),
        (-0.38, 0.32),
        (0.38, 0.32),
    ];
    p.oval(0.0, -0.08, 0.46, 0.44, hair);
    for (x, y) in curls {
        p.oval(x, y, 0.15, 0.15, hair);
    }
    torso(p, 0.7, jacket);
    // A black top in the jacket's V, the lapels.
    p.fill(p.poly(&[(0.18, 0.5), (0.0, 1.0), (-0.18, 0.5)]), rgb([30, 26, 30]));
    for s in [-1.0f32, 1.0] {
        let lap =
            [(s * 0.18, 0.5), (s * 0.02, 1.0), (s * 0.12, 1.0), (s * 0.3, 0.62), (s * 0.24, 0.5)];
        let lap = if s > 0.0 { [lap[4], lap[3], lap[2], lap[1], lap[0]] } else { lap };
        p.fill(p.poly(&lap), shade(jacket, if s < 0.0 { 0.1 } else { -0.15 }));
    }
    neck(p, &f, 0.09);
    if p.shows(20.0) {
        let chain = p.arc(0.0, 0.5, 0.11, 0.12, 0.1 * PI, 0.9 * PI);
        p.stroke(&chain, 0.012, GOLD_METAL);
    }
    head(p, &f);
    // Curls over the forehead.
    for (x, y, rr) in [
        (-0.22, -0.3, 0.12),
        (-0.08, -0.36, 0.12),
        (0.08, -0.36, 0.12),
        (0.22, -0.3, 0.12),
        (-0.3, -0.18, 0.08),
        (0.3, -0.18, 0.08),
    ] {
        p.oval(x, y, rr, rr, hair);
    }
    if p.shows(22.0) {
        for (x, y) in [
            (-0.3, -0.42),
            (-0.1, -0.53),
            (0.15, -0.5),
            (-0.44, -0.12),
            (0.4, -0.3),
            (-0.2, -0.33),
            (0.05, -0.38),
            (0.25, -0.33),
        ] {
            let a = p.arc(x, y, 0.06, 0.05, 1.1 * PI, 1.7 * PI);
            p.taper(&a, 0.02, rgb([95, 70, 66]));
        }
    }
    // Gold hoops.
    for s in [-1.0f32, 1.0] {
        p.ring(s * 0.31, f.y + 0.23, 0.065, 0.02, GOLD_METAL);
    }
    // One sly eye, one wink.
    let e =
        Eye { open: 0.75, slant: -0.1, lashes: true, look: (-0.3, 0.0), ..Eye::new([70, 45, 30]) };
    eye(p, -0.125, f.y + 0.06, -1.0, &e);
    if p.shows(19.0) {
        let w = p.quad((0.065, f.y + 0.06), (0.125, f.y + 0.1), (0.19, f.y + 0.055));
        p.ribbon(&w, |t| 0.028 * (0.5 + 0.5 * (t * PI).sin()), INK);
        let o = p.p(0.19, f.y + 0.055);
        let lash = o + vec2(0.035, -0.015) * p.u;
        p.taper(&[o, lash], 0.018, INK);
    } else {
        p.line((0.08, f.y + 0.07), (0.17, f.y + 0.07), 0.03, INK);
    }
    brow(p, -0.125, f.y - 0.06, -1.0, -0.03, -0.045, 0.04, 0.032, hair);
    brow(p, 0.125, f.y - 0.03, 1.0, 0.0, -0.01, 0.015, 0.032, hair);
    nose(p, &f, 0.9);
    blush(p, &f, 0.6);
    // The smirk.
    let lip = rgb([150, 30, 50]);
    let sm = p.quad((-0.08, f.y + 0.255), (0.02, f.y + 0.28), (0.1, f.y + 0.215));
    p.ribbon(&sm, |t| 0.035 * (0.45 + 0.55 * (t * PI).sin()), lip);
    if p.shows(24.0) {
        let d = p.quad((0.1, f.y + 0.215), (0.125, f.y + 0.22), (0.12, f.y + 0.245));
        p.taper(&d, 0.012, shade(f.skin, -0.3));
        let q = p.p(0.15, f.y + 0.19);
        draw_circle(q.x, q.y, (0.01 * p.u).max(0.8), p.col(shade(f.skin, -0.5)));
    }
}

/// Kenji, the professor: a neat side parting, round glasses with a glint,
/// one eyebrow raised, a pencil behind the ear and a sweater vest with a tie.
fn kenji(p: &Pen) {
    let f = Face { jaw: 0.24, ..Face::new([236, 204, 166]) };
    let hair = rgb([28, 28, 34]);
    let vest = rgb([62, 72, 124]);
    torso(p, 0.7, SHIRT_WHITE);
    neck(p, &f, 0.1);
    // The vest's V-neck with a diamond pattern.
    waistcoat(p, 0.7, 0.2, 0.9, shade(vest, -0.18), vest);
    if p.shows(26.0) {
        for (x, y) in [(-0.4, 0.8), (-0.25, 0.95), (0.35, 0.82), (0.2, 0.97), (-0.55, 0.95)] {
            p.fill(
                p.poly(&[(x + 0.06, y), (x, y + 0.08), (x - 0.06, y), (x, y - 0.08)]),
                shade(vest, 0.15),
            );
        }
        for s in [-1.0f32, 1.0] {
            p.line((s * 0.2, 0.55), (s * 0.0, 0.9), 0.025, shade(vest, -0.3));
        }
    }
    // The tie.
    let tie = rgb([150, 40, 50]);
    p.fill(
        p.poly(&[(0.035, 0.57), (0.055, 0.82), (0.0, 0.9), (-0.055, 0.82), (-0.035, 0.57)]),
        tie,
    );
    p.oval(0.0, 0.56, 0.04, 0.03, shade(tie, -0.1));
    collar(p, SHIRT_WHITE, 0.0);
    head(p, &f);
    // Neat hair with a side parting and a swept fringe.
    short_hair(p, &f, hair, f.y - 0.2, -0.02);
    p.shaded(p.oval_rot(0.06, -0.33, 0.27, 0.1, 0.22), (-0.02, -0.02), 0.2, hair);
    p.fill(p.oval_rot(-0.2, -0.31, 0.12, 0.07, -0.5), hair);
    if p.shows(20.0) {
        shine(p, &p.quad((-0.08, -0.4), (0.1, -0.42), (0.27, -0.3)), 0.03, hair);
    }
    // A pencil behind the right ear.
    if p.shows(18.0) {
        p.line((0.27, -0.2), (0.42, 0.05), 0.04, rgb([245, 200, 60]));
        p.line((0.27, -0.2), (0.295, -0.158), 0.04, rgb([240, 140, 150]));
        p.fill(p.poly(&[(0.435, 0.03), (0.45, 0.1), (0.4, 0.06)]), rgb([235, 210, 170]));
    }
    // Eyes looking up, thinking, behind round glasses.
    eyes(p, &f, &Eye { look: (0.5, -0.6), open: 0.9, size: 0.85, ..Eye::new([60, 45, 35]) });
    brow(p, -0.125, f.y - 0.04, -1.0, 0.0, 0.0, 0.01, 0.03, hair);
    brow(p, 0.125, f.y - 0.08, 1.0, 0.0, -0.03, 0.05, 0.03, hair);
    let frame = rgb([40, 36, 40]);
    for s in [-1.0f32, 1.0] {
        let (x, y) = (s * 0.125, f.y + 0.065);
        p.oval(x, y, 0.088, 0.088, Color::new(0.8, 0.9, 1.0, 0.12));
        p.ring(x, y, 0.088, 0.02, frame);
        if p.shows(26.0) {
            p.line((x - 0.04, y - 0.01), (x - 0.01, y - 0.05), 0.014, with_alpha(WHITE, 0.7));
        }
        p.line((s * 0.213, y - 0.01), (s * 0.29, y - 0.03), 0.018, frame);
    }
    let bridge = p.quad((-0.04, f.y + 0.06), (0.0, f.y + 0.035), (0.04, f.y + 0.06));
    p.stroke(&bridge, 0.018, frame);
    nose(p, &f, 0.9);
    // A knowing half-smile.
    smile(p, 0.01, f.y + 0.25, 0.07, 0.012, 0.012, 0.026, rgb([150, 80, 70]));
    blush(p, &f, 0.5);
}

/// Sofia, the sharpshooter: a swinging blonde ponytail, one eye narrowed
/// to aim, a high-collared top with a crosshair pin.
fn sofia(p: &Pen) {
    let f = Face { jaw: 0.27, ears: 0.95, ..Face::new([242, 212, 188]) };
    let hair = rgb([232, 190, 92]);
    let top = rgb([40, 40, 46]);
    // The ponytail swinging out to the right.
    let tail = p.oval_rot(0.4, -0.05, 0.13, 0.32, -0.45);
    p.shaded(tail, (-0.03, -0.02), 0.2, shade(hair, -0.05));
    p.fill(p.oval_rot(0.53, 0.2, 0.06, 0.14, -0.6), shade(hair, -0.15));
    shine(p, &p.quad((0.33, -0.25), (0.48, -0.1), (0.5, 0.15)), 0.03, hair);
    torso(p, 0.66, top);
    neck(p, &f, 0.09);
    // A high zipped collar.
    p.fill(p.round_rect(-0.14, 0.42, 0.14, 0.6, 0.04), shade(top, 0.08));
    if p.shows(20.0) {
        p.line((0.0, 0.47), (0.0, 1.0), 0.015, rgb([170, 170, 180]));
        p.oval(0.0, 0.5, 0.015, 0.02, rgb([200, 200, 210]));
        // The crosshair pin.
        p.ring(-0.3, 0.72, 0.055, 0.016, GOLD_METAL);
        p.line((-0.37, 0.72), (-0.23, 0.72), 0.012, GOLD_METAL);
        p.line((-0.3, 0.65), (-0.3, 0.79), 0.012, GOLD_METAL);
    } else {
        p.oval(-0.3, 0.72, 0.05, 0.05, GOLD_METAL);
    }
    head(p, &f);
    // Hair pulled back tight, a hair tie, a sweep of bangs to one side.
    let cap = p.oval_poly(0.0, -0.11, 0.33, 0.33);
    let cap = above(p, cap, (-0.4, -0.13), (0.4, -0.2));
    p.shaded(cap, (-0.04, -0.03), 0.15, hair);
    p.oval(0.31, -0.28, 0.04, 0.05, rgb([60, 120, 220]));
    p.fill(p.poly(&[(0.05, -0.3), (-0.3, -0.05), (-0.33, -0.15), (-0.2, -0.33)]), hair);
    if p.shows(20.0) {
        shine(p, &p.quad((-0.22, -0.32), (-0.05, -0.42), (0.18, -0.36)), 0.03, hair);
        shine(p, &p.quad((-0.25, -0.2), (-0.15, -0.25), (-0.05, -0.27)), 0.02, hair);
        // A stud earring.
        p.oval(-0.315, f.y + 0.14, 0.018, 0.018, GOLD_METAL);
    }
    // One eye open, the other narrowed to aim.
    let e =
        Eye { slant: 0.3, size: 0.9, lashes: true, look: (0.0, 0.0), ..Eye::new([60, 140, 90]) };
    eye(p, -0.125, f.y + 0.06, -1.0, &Eye { open: 0.75, ..e });
    eye(p, 0.125, f.y + 0.06, 1.0, &Eye { open: 0.42, slant: 0.1, ..e });
    brow(p, -0.125, f.y - 0.04, -1.0, 0.02, -0.025, 0.01, 0.028, shade(hair, -0.3));
    brow(p, 0.125, f.y - 0.02, 1.0, 0.015, -0.0, 0.0, 0.028, shade(hair, -0.3));
    nose(p, &f, 0.8);
    blush(p, &f, 0.5);
    // Lips pressed, a corner tucked.
    smile(p, -0.01, f.y + 0.25, 0.06, 0.0, -0.008, 0.026, rgb([190, 100, 100]));
}

/// Viktor, the champion: slicked-back hair with silver temples, a
/// confident smile, a waistcoat, a red bow tie and a gold medal.
fn viktor(p: &Pen) {
    let f = Face { jaw: 0.2, w: 0.3, ..Face::new([226, 194, 168]) };
    let hair = rgb([30, 30, 36]);
    let waistcoat_c = rgb([28, 28, 34]);
    torso(p, 0.72, SHIRT_WHITE);
    neck(p, &f, 0.1);
    waistcoat(p, 0.72, 0.17, 0.88, waistcoat_c, shade(waistcoat_c, 0.12));
    if p.shows(20.0) {
        for y in [0.92, 1.02] {
            p.oval(0.02, y - 0.04, 0.016, 0.016, GOLD_METAL);
        }
    }
    collar(p, SHIRT_WHITE, 0.04);
    // The medal on its ribbon, on the waistcoat.
    let (mx, my) = (-0.27, 0.71);
    p.fill(
        p.poly(&[(mx + 0.06, my - 0.15), (mx, my - 0.03), (mx - 0.06, my - 0.15)]),
        rgb([200, 40, 50]),
    );
    p.oval(mx, my, 0.07, 0.07, shade(GOLD_METAL, -0.2));
    p.oval(mx - 0.005, my - 0.005, 0.055, 0.055, GOLD_METAL);
    if p.shows(22.0) {
        p.star((mx - 0.005, my), 0.035, 0.015, 5, 0.0, shade(GOLD_METAL, -0.3));
    }
    // The bow tie.
    let bow = rgb([200, 30, 40]);
    for s in [-1.0f32, 1.0] {
        let wing = [(0.0, 0.565), (s * 0.13, 0.5), (s * 0.13, 0.63)];
        let wing = if s < 0.0 { [wing[0], wing[2], wing[1]] } else { wing };
        p.fill(p.poly(&wing), if s < 0.0 { bow } else { shade(bow, -0.18) });
    }
    p.oval(0.0, 0.565, 0.03, 0.035, shade(bow, -0.25));
    head(p, &f);
    // Slicked-back hair with a little height, silver at the temples.
    short_hair(p, &f, hair, f.y - 0.23, -0.03);
    // A swept-back quiff, higher on the left.
    p.shaded(p.oval_rot(-0.02, -0.4, 0.27, 0.12, -0.12), (-0.02, -0.03), 0.25, hair);
    if p.shows(18.0) {
        // Comb lines sweeping back.
        for (x, w) in [(-0.17, 0.03), (-0.06, 0.035), (0.06, 0.025), (0.17, 0.02)] {
            let s = p.quad((x, -0.31), (x + 0.02, -0.42), (x + 0.1, -0.47));
            shine(p, &s, w, hair);
        }
    }
    eyes(p, &f, &Eye { open: 0.7, slant: 0.05, size: 0.88, ..Eye::new([90, 70, 50]) });
    brow(p, -0.125, f.y - 0.06, -1.0, 0.0, -0.01, 0.025, 0.032, hair);
    brow(p, 0.125, f.y - 0.075, 1.0, 0.0, -0.025, 0.035, 0.032, hair);
    nose(p, &f, 1.0);
    // A winner's smile, a dimple and a cleft chin.
    grin(p, &f, 0.005, f.y + 0.228, 0.1, 0.04, 0.008, 0.55, false);
    if p.shows(24.0) {
        p.taper(
            &p.quad((0.125, f.y + 0.19), (0.14, f.y + 0.215), (0.13, f.y + 0.24)),
            0.012,
            shade(f.skin, -0.3),
        );
        p.line((0.0, f.y + 0.32), (0.0, f.y + 0.34), 0.012, shade(f.skin, -0.25));
    }
}

/// The classic levels: a friendly robot with a red ball for an antenna.
fn robot(p: &Pen) {
    let metal = rgb([196, 202, 214]);
    let glow = rgb([90, 230, 230]);
    // Body and a chest panel with three lights.
    p.shaded(p.round_rect(-0.62, 0.52, 0.62, 1.3, 0.2), (-0.06, -0.03), 0.18, metal);
    p.fill(p.round_rect(-0.22, 0.66, 0.22, 0.86, 0.05), rgb([50, 58, 72]));
    for (k, c) in [[230, 70, 60], [250, 200, 60], [80, 200, 110]].iter().enumerate() {
        p.oval(-0.12 + k as f32 * 0.12, 0.76, 0.04, 0.04, rgb(*c));
    }
    // Neck rings.
    p.fill(p.rect(-0.1, 0.3, 0.1, 0.56), shade(metal, -0.35));
    for y in [0.4, 0.48] {
        p.fill(p.rect(-0.12, y - 0.02, 0.12, y + 0.02), shade(metal, -0.15));
    }
    // The antenna.
    p.line((0.0, -0.4), (0.0, -0.58), 0.03, shade(metal, -0.3));
    p.shaded(p.oval_poly(0.0, -0.64, 0.075, 0.075), (-0.02, -0.02), 0.3, rgb([220, 40, 40]));
    if p.shows(20.0) {
        p.oval(-0.025, -0.665, 0.022, 0.018, with_alpha(WHITE, 0.85));
    }
    // Ears.
    for s in [-1.0f32, 1.0] {
        p.fill(
            p.round_rect(s * 0.38 - 0.06, -0.15, s * 0.38 + 0.06, 0.12, 0.04),
            shade(metal, -0.25),
        );
        p.oval(s * 0.41, -0.015, 0.035, 0.035, glow);
    }
    // The head, the screen, the face on it.
    p.shaded(p.round_rect(-0.38, -0.42, 0.38, 0.33, 0.14), (-0.05, -0.04), 0.18, metal);
    p.fill(p.round_rect(-0.3, -0.31, 0.3, 0.22, 0.09), rgb([28, 36, 50]));
    if p.shows(20.0) {
        p.fill(p.round_rect(-0.27, -0.29, -0.05, -0.24, 0.02), Color::new(1.0, 1.0, 1.0, 0.08));
        for s in [-1.0f32, 1.0] {
            p.oval(s * 0.13, -0.08, 0.075, 0.085, with_alpha(glow, 0.25));
        }
    }
    for s in [-1.0f32, 1.0] {
        p.fill(p.round_rect(s * 0.13 - 0.045, -0.14, s * 0.13 + 0.045, -0.02, 0.04), glow);
        if p.shows(22.0) {
            p.oval(s * 0.13 - 0.015, -0.11, 0.015, 0.018, WHITE);
        }
    }
    let mouth = p.quad((-0.11, 0.06), (0.0, 0.16), (0.11, 0.06));
    p.ribbon(&mouth, |_| 0.035, glow);
    for s in [-1.0f32, 1.0] {
        p.oval(s * 0.22, 0.06, 0.04, 0.025, Color::new(1.0, 0.45, 0.6, 0.45));
    }
    if p.shows(26.0) {
        for (x, y) in [(-0.32, -0.36), (0.32, -0.36), (-0.32, 0.27), (0.32, 0.27)] {
            p.oval(x, y, 0.014, 0.014, shade(metal, -0.35));
        }
    }
}

/// A ghost: a pale floating sheet, its tint from the player's name.
pub fn draw_ghost(name: &str, c: Vec2, r: f32) {
    let alpha = crate::paint::opacity();
    let a = |col: Color| with_alpha(col, alpha);
    let hash = name.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    let hue = (hash % 360) as f32 / 360.0;
    let tint = hsl_to_rgb(hue, 0.35, 0.32);
    backdrop(&Pen::new(c, r), tint);
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
