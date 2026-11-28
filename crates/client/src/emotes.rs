//! Reactions that act: a round face with two hands (the game's own
//! drawing, all vectors), each reaction a little performance of its own:
//! it waves, claps, gives a thumbs up, drops its jaw, laughs till it cries,
//! shivers, fumes, sleeps, salutes, shrugs, melts… The face pops in,
//! performs, and keeps going while it is shown.
//!
//! Everything is drawn in the face's own units (its radius is 1, `y` goes
//! down) through a [`Pen`] that places, turns, squashes and fades it.

use macroquad::prelude::*;

use crate::aa;

/// The reactions drawn as a character (the rest are pictures).
pub fn is_character(code: &str) -> bool {
    performance(code).is_some()
}

/// A performance: how the reaction acts at `t` seconds.
type Act = fn(&Pen, f32);

fn performance(code: &str) -> Option<Act> {
    Some(match code {
        "thumbs_up" => thumbs_up,
        "clap" => clap,
        "wow" => wow,
        "handshake" => handshake,
        "thinking" => thinking,
        "phew" => phew,
        "cool" => cool,
        "laugh" => laugh,
        "ice" => ice,
        "wave" => wave,
        "playful" => playful,
        "grin" => grin,
        "sob" => sob,
        "huff" => huff,
        "please" => please,
        "sleepy" => sleepy,
        "rofl" => rofl,
        "scream" => scream,
        "no_look" => no_look,
        "salute" => salute,
        "mind_blown" => mind_blown,
        "ok_hand" => ok_hand,
        "crossed_fingers" => crossed_fingers,
        "call_me" => call_me,
        "nerd" => nerd,
        "money_face" => money_face,
        "monocle" => monocle,
        "facepalm" => facepalm,
        "shrug" => shrug,
        "raised_hands" => raised_hands,
        "melting" => melting,
        "devil" => devil,
        "angel" => angel,
        "star_eyes" => star_eyes,
        "eyes" => eyes,
        "heart" => heart,
        _ => return None,
    })
}

/// Draws reaction `code` acting, centred on `at`, `size` across, `t`
/// seconds into its performance (the first half second it pops in). False
/// when it isn't one of the characters.
pub fn draw(code: &str, at: Vec2, size: f32, alpha: f32, t: f32) -> bool {
    let Some(act) = performance(code) else { return false };
    // Popping in: from small, past its size, and back.
    let pop = if t < 0.45 { ease_out_back(t / 0.45) } else { 1.0 };
    let pen = Pen { o: at, s: size * 0.34 * pop, rot: 0.0, sq: Vec2::ONE, a: alpha };
    act(&pen, t);
    true
}

// ------------------------------------------------------------------ pen

/// Where and how a part is drawn: its origin on screen, its scale (screen
/// units per face radius), its turn, its squash (x, y) and its opacity.
#[derive(Clone, Copy)]
pub struct Pen {
    o: Vec2,
    s: f32,
    rot: f32,
    sq: Vec2,
    a: f32,
}

impl Pen {
    fn p(&self, v: Vec2) -> Vec2 {
        let v = v * self.sq;
        let (sn, cs) = self.rot.sin_cos();
        self.o + vec2(v.x * cs - v.y * sn, v.x * sn + v.y * cs) * self.s
    }

    /// The same pen moved by `d` (in its own units), turned by `rot`,
    /// squashed by `sq` (about its new origin).
    fn moved(&self, d: Vec2, rot: f32, sq: Vec2) -> Pen {
        Pen { o: self.p(d), s: self.s, rot: self.rot + rot, sq: self.sq * sq, a: self.a }
    }

    fn faded(&self, a: f32) -> Pen {
        Pen { a: self.a * a.clamp(0.0, 1.0), ..*self }
    }

    fn col(&self, c: Color) -> Color {
        Color { a: c.a * self.a * crate::paint::alpha(), ..c }
    }

    fn fill(&self, pts: &[Vec2], c: Color) {
        let v: Vec<Vec2> = pts.iter().map(|p| self.p(*p)).collect();
        let c = self.col(c);
        aa::fill_convex(&v, |_| c);
    }

    /// A convex shape shaded point by point (`f` of the local point).
    fn fill_with(&self, pts: &[Vec2], f: impl Fn(Vec2) -> Color) {
        let v: Vec<Vec2> = pts.iter().map(|p| self.p(*p)).collect();
        aa::fill_convex(&v, |q| {
            // Back to the local point for the colour.
            let i = v.iter().position(|x| *x == q).unwrap_or(0);
            self.col(f(pts[i]))
        });
    }

    fn stroke(&self, pts: &[Vec2], w: f32, c: Color) {
        let v: Vec<Vec2> = pts.iter().map(|p| self.p(*p)).collect();
        aa::stroke(&v, false, w * self.s, self.col(c));
    }

    fn ring(&self, c: Vec2, rx: f32, ry: f32, w: f32, color: Color) {
        let pts = ellipse_pts(c, rx, ry, 0.0, 40);
        let v: Vec<Vec2> = pts.iter().map(|p| self.p(*p)).collect();
        aa::stroke(&v, true, w * self.s, self.col(color));
    }

    fn ellipse(&self, c: Vec2, rx: f32, ry: f32, rot: f32, color: Color) {
        self.fill(&ellipse_pts(c, rx, ry, rot, 36), color);
    }

    fn circle(&self, c: Vec2, r: f32, color: Color) {
        self.ellipse(c, r, r, 0.0, color);
    }

    fn capsule(&self, a: Vec2, b: Vec2, r: f32, color: Color) {
        self.fill(&capsule_pts(a, b, r), color);
    }

    /// An arc of the circle round `c` from angle `a0` to `a1`.
    fn arc(&self, c: Vec2, r: f32, a0: f32, a1: f32, w: f32, color: Color) {
        let n = 16;
        let pts: Vec<Vec2> = (0..=n)
            .map(|k| {
                let a = a0 + (a1 - a0) * k as f32 / n as f32;
                c + vec2(a.cos(), a.sin()) * r
            })
            .collect();
        self.stroke(&pts, w, color);
    }
}

fn ellipse_pts(c: Vec2, rx: f32, ry: f32, rot: f32, n: usize) -> Vec<Vec2> {
    let (sn, cs) = rot.sin_cos();
    (0..n)
        .map(|k| {
            let a = k as f32 / n as f32 * std::f32::consts::TAU;
            let v = vec2(a.cos() * rx, a.sin() * ry);
            c + vec2(v.x * cs - v.y * sn, v.x * sn + v.y * cs)
        })
        .collect()
}

fn capsule_pts(a: Vec2, b: Vec2, r: f32) -> Vec<Vec2> {
    let d = (b - a).try_normalize().unwrap_or(Vec2::Y);
    let base = d.y.atan2(d.x);
    let mut pts = Vec::with_capacity(26);
    for k in 0..=12 {
        let t = base + std::f32::consts::FRAC_PI_2 + k as f32 / 12.0 * std::f32::consts::PI;
        pts.push(a + vec2(t.cos(), t.sin()) * r);
    }
    for k in 0..=12 {
        let t = base - std::f32::consts::FRAC_PI_2 + k as f32 / 12.0 * std::f32::consts::PI;
        pts.push(b + vec2(t.cos(), t.sin()) * r);
    }
    pts
}

// --------------------------------------------------------------- timing

fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0) - 1.0;
    1.0 + t * t * (2.7 * t + 1.7)
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A bump from 0 up to 1 and back over `[a, b]`.
fn bump(a: f32, b: f32, x: f32) -> f32 {
    if x <= a || x >= b {
        return 0.0;
    }
    (((x - a) / (b - a)) * std::f32::consts::PI).sin()
}

/// How open the eyes are: a blink every few seconds.
fn blink(t: f32) -> f32 {
    let p = (t + 0.7) % 3.1;
    1.0 - bump(2.85, 3.05, p)
}

fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
}

fn mix(a: Color, b: Color, k: f32) -> Color {
    let k = k.clamp(0.0, 1.0);
    Color::new(
        a.r + (b.r - a.r) * k,
        a.g + (b.g - a.g) * k,
        a.b + (b.b - a.b) * k,
        a.a + (b.a - a.a) * k,
    )
}

const SKIN: Color = Color::new(1.0, 0.80, 0.30, 1.0);
const SKIN_LIGHT: Color = Color::new(1.0, 0.90, 0.55, 1.0);
const SKIN_DARK: Color = Color::new(0.94, 0.62, 0.16, 1.0);
const LINE: Color = Color::new(0.60, 0.36, 0.06, 1.0);
const INK: Color = Color::new(0.33, 0.20, 0.03, 1.0);
const MOUTH: Color = Color::new(0.42, 0.13, 0.10, 1.0);
const TONGUE: Color = Color::new(0.94, 0.40, 0.45, 1.0);
const WHITE: Color = Color::new(1.0, 1.0, 1.0, 1.0);
const TEAR: Color = Color::new(0.42, 0.74, 1.0, 1.0);
const GOLD: Color = Color::new(1.0, 0.80, 0.25, 1.0);

// ----------------------------------------------------------------- face

/// The face: a ball of skin (lighter on top, darker at the bottom, a soft
/// shine), outlined; `tint` mixes another colour in (blue with cold, red
/// with anger) by `k`.
fn face(p: &Pen, tint: Color, k: f32) {
    p.circle(Vec2::ZERO, 1.07, mix(LINE, tint, k * 0.6));
    let pts = ellipse_pts(Vec2::ZERO, 1.0, 1.0, 0.0, 48);
    p.fill_with(&pts, |v| {
        let base = if v.y < 0.0 {
            mix(SKIN, SKIN_LIGHT, -v.y * 0.8)
        } else {
            mix(SKIN, SKIN_DARK, v.y * v.y)
        };
        mix(base, tint, k)
    });
    p.ellipse(vec2(-0.32, -0.55), 0.36, 0.2, -0.5, Color::new(1.0, 1.0, 1.0, 0.32));
}

/// An eye open (`open` 0..1 for a blink) at `x`, looking `look`.
fn eye_open(p: &Pen, x: f32, look: Vec2, open: f32, size: f32) {
    let c = vec2(x, -0.14) + look * 0.06;
    let h = (0.17 * size * open).max(0.025);
    p.ellipse(c, 0.11 * size, h, 0.0, INK);
    if open > 0.5 {
        p.circle(c + vec2(-0.035, -0.06) * size, 0.04 * size, Color::new(1.0, 1.0, 1.0, 0.9));
    }
}

/// Two eyes open (`open` 0..1 for a blink), looking `look`.
fn eyes_open(p: &Pen, look: Vec2, open: f32, size: f32) {
    for x in [-0.36, 0.36] {
        eye_open(p, x, look, open, size);
    }
}

/// A wink: the left eye shut in a smile, the right one open.
fn wink(p: &Pen, t: f32) {
    p.arc(
        vec2(-0.36, -0.08),
        0.15,
        std::f32::consts::PI * 1.12,
        std::f32::consts::PI * 1.88,
        0.075,
        INK,
    );
    eye_open(p, 0.36, Vec2::ZERO, blink(t), 1.0);
}

/// Eyes shut in a smile (^ ^).
fn eyes_happy(p: &Pen, w: f32) {
    for x in [-0.36, 0.36] {
        p.arc(
            vec2(x, -0.08),
            0.15,
            std::f32::consts::PI * 1.12,
            std::f32::consts::PI * 1.88,
            w,
            INK,
        );
    }
}

/// Eyes shut tight, crying or laughing (> <).
fn eyes_squeezed(p: &Pen) {
    for (x, s) in [(-0.36, 1.0), (0.36, -1.0)] {
        let pts = [vec2(x - 0.13 * s, -0.24), vec2(x + 0.1 * s, -0.13), vec2(x - 0.13 * s, -0.02)];
        p.stroke(&pts, 0.075, INK);
    }
}

/// Eyes closed, asleep or at peace (gentle downward arcs).
fn eyes_closed(p: &Pen) {
    for x in [-0.36, 0.36] {
        p.arc(vec2(x, -0.2), 0.14, 0.35, std::f32::consts::PI - 0.35, 0.065, INK);
    }
}

/// Big round eyes, white with small pupils (shock, fear).
fn eyes_wide(p: &Pen, look: Vec2, size: f32) {
    for x in [-0.36, 0.36] {
        let c = vec2(x, -0.16);
        p.circle(c, 0.24 * size, INK);
        p.circle(c, 0.2 * size, WHITE);
        p.circle(c + look * 0.07, 0.085 * size, INK);
    }
}

/// Brows: lines over the eyes, raised by `up`, turned by `tilt` (angry:
/// inner ends down).
fn brows(p: &Pen, up: f32, tilt: f32) {
    for (x, s) in [(-0.36, 1.0), (0.36, -1.0)] {
        let y = -0.46 - up;
        let d = vec2(0.14, tilt * 0.12 * s);
        p.stroke(&[vec2(x, y) - d, vec2(x, y) + d], 0.07, INK);
    }
}

/// A smile, `w` wide, `curve` deep (a frown below 0).
fn smile(p: &Pen, w: f32, curve: f32, y: f32) {
    let n = 12;
    let pts: Vec<Vec2> = (0..=n)
        .map(|k| {
            let x = (k as f32 / n as f32 - 0.5) * 2.0 * w;
            vec2(x, y + curve * (1.0 - (x / w).powi(2)))
        })
        .collect();
    p.stroke(&pts, 0.08, INK);
}

/// A mouth open in a "D", `w` wide, `h` deep, with the tongue and,
/// optionally, the top teeth.
fn mouth_open(p: &Pen, y: f32, w: f32, h: f32, teeth: bool) {
    let n = 18;
    // (In order round the shape.)
    let mut shape = vec![vec2(-w, y)];
    shape.extend((0..=n).map(|k| {
        let a = std::f32::consts::PI - k as f32 / n as f32 * std::f32::consts::PI;
        vec2(a.cos() * w, y + a.sin() * h)
    }));
    p.fill(&shape, MOUTH);
    p.ellipse(vec2(0.0, y + h * 0.72), w * 0.55, h * 0.3, 0.0, TONGUE);
    if teeth {
        p.fill(
            &[
                vec2(-w * 0.86, y),
                vec2(w * 0.86, y),
                vec2(w * 0.8, y + h * 0.24),
                vec2(-w * 0.8, y + h * 0.24),
            ],
            WHITE,
        );
    }
    p.stroke(&[vec2(-w - 0.02, y), vec2(w + 0.02, y)], 0.06, INK);
}

/// A mouth open round ("O"), `w` wide and `h` tall.
fn mouth_o(p: &Pen, y: f32, w: f32, h: f32) {
    p.ellipse(vec2(0.0, y), w + 0.04, h + 0.04, 0.0, INK);
    p.ellipse(vec2(0.0, y), w, h, 0.0, MOUTH);
    p.ellipse(vec2(0.0, y + h * 0.5), w * 0.6, h * 0.35, 0.0, TONGUE);
}

/// A wobbly line of a mouth (worry, cold, melting).
fn mouth_wavy(p: &Pen, y: f32, w: f32, amp: f32, phase: f32) {
    let n = 16;
    let pts: Vec<Vec2> = (0..=n)
        .map(|k| {
            let x = (k as f32 / n as f32 - 0.5) * 2.0 * w;
            vec2(x, y + (x * 14.0 + phase).sin() * amp)
        })
        .collect();
    p.stroke(&pts, 0.07, INK);
}

fn blush(p: &Pen, a: f32) {
    for x in [-0.62, 0.62] {
        p.ellipse(vec2(x, 0.18), 0.17, 0.1, 0.0, Color::new(1.0, 0.45, 0.45, 0.35 * a));
    }
}

/// A drop (a tear, sweat) hanging from `at`, `r` round.
fn drop(p: &Pen, at: Vec2, r: f32, color: Color) {
    p.fill(
        &[at + vec2(0.0, -r * 2.0), at + vec2(r * 0.9, -r * 0.2), at + vec2(-r * 0.9, -r * 0.2)],
        color,
    );
    p.circle(at, r, color);
    p.circle(at + vec2(-r * 0.35, -r * 0.2), r * 0.3, Color::new(1.0, 1.0, 1.0, 0.7));
}

fn sparkle(p: &Pen, at: Vec2, r: f32, color: Color) {
    if r <= 0.001 {
        return;
    }
    p.fill(
        &[
            at + vec2(0.0, -r),
            at + vec2(r * 0.25, 0.0),
            at + vec2(0.0, r),
            at + vec2(-r * 0.25, 0.0),
        ],
        color,
    );
    p.fill(
        &[
            at + vec2(-r, 0.0),
            at + vec2(0.0, -r * 0.25),
            at + vec2(r, 0.0),
            at + vec2(0.0, r * 0.25),
        ],
        color,
    );
}

fn star(p: &Pen, at: Vec2, r: f32, rot: f32, color: Color) {
    let pt = |k: usize, rr: f32| {
        let a = rot - std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::TAU / 10.0;
        at + vec2(a.cos(), a.sin()) * rr
    };
    let inner: Vec<Vec2> = (0..5).map(|k| pt(2 * k + 1, r * 0.45)).collect();
    p.fill(&inner, color);
    for k in 0..5 {
        p.fill(&[pt(2 * k, r), pt(2 * k + 1, r * 0.45), pt((2 * k + 9) % 10, r * 0.45)], color);
    }
}

fn heart_shape(p: &Pen, at: Vec2, r: f32, color: Color) {
    p.circle(at + vec2(-r * 0.5, -r * 0.2), r * 0.55, color);
    p.circle(at + vec2(r * 0.5, -r * 0.2), r * 0.55, color);
    p.fill(
        &[
            at + vec2(-r * 1.03, -r * 0.05),
            at + vec2(r * 1.03, -r * 0.05),
            at + vec2(0.0, r * 1.05),
        ],
        color,
    );
}

/// A puff of steam or smoke.
fn puff(p: &Pen, at: Vec2, r: f32, color: Color) {
    for (d, k) in
        [(vec2(0.0, 0.0), 1.0), (vec2(r * 0.7, r * 0.2), 0.75), (vec2(-r * 0.65, r * 0.25), 0.7)]
    {
        p.circle(at + d, r * k, color);
    }
}

/// A "Z" (sleep), `r` tall.
fn zed(p: &Pen, at: Vec2, r: f32, color: Color) {
    let pts = [
        at + vec2(-r * 0.5, -r * 0.5),
        at + vec2(r * 0.5, -r * 0.5),
        at + vec2(-r * 0.5, r * 0.5),
        at + vec2(r * 0.5, r * 0.5),
    ];
    p.stroke(&pts, r * 0.22, color);
}

/// A question mark, `r` tall.
fn question(p: &Pen, at: Vec2, r: f32, color: Color) {
    p.arc(
        at + vec2(0.0, -r * 0.3),
        r * 0.3,
        std::f32::consts::PI * 1.0,
        std::f32::consts::PI * 2.4,
        r * 0.16,
        color,
    );
    p.stroke(&[at + vec2(0.05 * r, -r * 0.05), at + vec2(0.0, r * 0.2)], r * 0.16, color);
    p.circle(at + vec2(0.0, r * 0.5), r * 0.1, color);
}

// ---------------------------------------------------------------- hands

#[derive(Clone, Copy, PartialEq)]
enum Hand {
    /// Flat, the fingers together or `spread` apart.
    Open(f32),
    Fist,
    Thumb,
    Point,
    Ok,
    Shaka,
    Crossed,
}

/// A hand at `at` (face units), turned `rot` (0: fingers up), `size`
/// times the usual, `mirror` for the left one.
fn hand(p: &Pen, at: Vec2, rot: f32, size: f32, mirror: bool, kind: Hand) {
    let sx = if mirror { -1.0 } else { 1.0 };
    let h = p.moved(at, rot, vec2(sx * size, size));
    // Its parts as capsules and ellipses: outlined together, then filled.
    let mut caps: Vec<(Vec2, Vec2, f32)> = Vec::new();
    let mut blobs: Vec<(Vec2, f32, f32)> = Vec::new();
    let palm = |blobs: &mut Vec<(Vec2, f32, f32)>| blobs.push((vec2(0.0, 0.05), 0.3, 0.27));
    match kind {
        Hand::Open(spread) => {
            palm(&mut blobs);
            for (k, x) in [-0.18f32, -0.06, 0.06, 0.18].into_iter().enumerate() {
                let fan = (k as f32 - 1.5) * spread * 0.12;
                let tip = vec2(x + fan * 0.9, -0.44 + (k as f32 - 1.5).abs() * 0.04);
                caps.push((vec2(x, -0.08), tip, 0.075));
            }
            caps.push((vec2(-0.22, 0.08), vec2(-0.44, -0.12 - spread * 0.05), 0.08));
        }
        Hand::Fist | Hand::Thumb | Hand::Point | Hand::Ok | Hand::Shaka | Hand::Crossed => {
            blobs.push((vec2(0.0, 0.02), 0.3, 0.26));
            for x in [-0.16f32, -0.05, 0.06, 0.17] {
                blobs.push((vec2(x, -0.2), 0.075, 0.075));
            }
            match kind {
                Hand::Thumb => caps.push((vec2(-0.12, -0.18), vec2(-0.12, -0.56), 0.085)),
                Hand::Point => caps.push((vec2(-0.16, -0.2), vec2(-0.16, -0.6), 0.075)),
                Hand::Shaka => {
                    caps.push((vec2(-0.2, -0.05), vec2(-0.5, -0.3), 0.08));
                    caps.push((vec2(0.17, -0.18), vec2(0.36, -0.52), 0.07));
                }
                Hand::Crossed => {
                    caps.push((vec2(-0.12, -0.2), vec2(0.02, -0.62), 0.072));
                    caps.push((vec2(0.0, -0.2), vec2(-0.16, -0.58), 0.072));
                }
                Hand::Ok => {
                    caps.push((vec2(0.06, -0.2), vec2(0.06, -0.58), 0.07));
                    caps.push((vec2(0.17, -0.18), vec2(0.2, -0.52), 0.068));
                }
                _ => {
                    caps.push((vec2(-0.24, 0.02), vec2(0.02, -0.06), 0.075));
                }
            }
        }
    }
    let grow = 0.045;
    for (a, b, r) in &caps {
        h.capsule(*a, *b, r + grow, LINE);
    }
    for (c, rx, ry) in &blobs {
        h.ellipse(*c, rx + grow, ry + grow, 0.0, LINE);
    }
    for (c, rx, ry) in &blobs {
        h.ellipse(*c, *rx, *ry, 0.0, SKIN);
    }
    for (a, b, r) in &caps {
        h.capsule(*a, *b, *r, SKIN);
    }
    if kind == Hand::Ok {
        // The ring of the thumb and the finger.
        h.ring(vec2(-0.2, -0.28), 0.14, 0.14, 0.13, LINE);
        h.ring(vec2(-0.2, -0.28), 0.14, 0.14, 0.07, SKIN);
    }
    // A soft shine on the back of the hand.
    h.ellipse(vec2(-0.08, -0.02), 0.12, 0.07, -0.4, Color::new(1.0, 1.0, 1.0, 0.25));
}

// ---------------------------------------------------------- performances

/// The body's bob: a jump every `period` seconds, squashed on landing.
fn hop(p: &Pen, t: f32, period: f32, height: f32) -> Pen {
    let k = (t % period) / period;
    let up = bump(0.0, 0.5, k) * height;
    let land = bump(0.5, 0.65, k);
    p.moved(vec2(0.0, -up), 0.0, vec2(1.0 + 0.1 * land, 1.0 - 0.1 * land))
}

fn thumbs_up(p: &Pen, t: f32) {
    let b = hop(p, t, 1.4, 0.08);
    face(&b, SKIN, 0.0);
    wink(&b, t);
    smile(&b, 0.42, 0.18, 0.3);
    blush(&b, 1.0);
    // The thumb comes up and pumps.
    let rise = ease_out_back(((t - 0.1) / 0.45).clamp(0.0, 1.0));
    let pump = (t * 5.0).sin().max(0.0) * 0.06 * smooth(0.6, 0.9, t);
    let at = vec2(1.0, 1.2 - 0.85 * rise - pump);
    hand(p, at, -0.15, 1.15, false, Hand::Thumb);
    if t > 0.5 {
        let k = (t * 1.2).fract();
        sparkle(p, vec2(1.25, at.y - 0.85), 0.16 * bump(0.0, 0.5, k), GOLD);
    }
}

fn clap(p: &Pen, t: f32) {
    let beat = (t * 3.2).fract();
    let shut = 1.0 - bump(0.0, 1.0, beat);
    let b =
        p.moved(vec2(0.0, -0.06 * (1.0 - shut)), 0.0, vec2(1.0 + 0.05 * shut, 1.0 - 0.05 * shut));
    face(&b, SKIN, 0.0);
    eyes_happy(&b, 0.08);
    mouth_open(&b, 0.22, 0.38, 0.3, true);
    blush(&b, 1.0);
    let gap = 0.12 + 0.42 * (1.0 - shut);
    hand(p, vec2(-gap, 0.95), 0.35 + 0.3 * (1.0 - shut), 1.05, true, Hand::Open(0.0));
    hand(p, vec2(gap, 0.95), -0.35 - 0.3 * (1.0 - shut), 1.05, false, Hand::Open(0.0));
    // The smack: lines out from the hands.
    if shut > 0.85 {
        for k in 0..5 {
            let a = -std::f32::consts::FRAC_PI_2 + (k as f32 - 2.0) * 0.45;
            let d = vec2(a.cos(), a.sin());
            let c = vec2(0.0, 0.55);
            p.stroke(&[c + d * 0.45, c + d * 0.65], 0.06, GOLD);
        }
    }
}

fn wow(p: &Pen, t: f32) {
    let k = smooth(0.0, 0.35, t);
    let jolt = bump(0.0, 0.35, t) * 0.15;
    let b = p.moved(vec2(0.0, -jolt), 0.0, vec2(1.0 - 0.06 * k, 1.0 + 0.08 * k));
    face(&b, SKIN, 0.0);
    brows(&b, 0.12 * k, -0.3);
    let shake = vec2((t * 31.0).sin(), (t * 27.0).cos()) * 0.25;
    eyes_wide(&b, shake * smooth(0.6, 1.0, t), 0.75 + 0.35 * k);
    mouth_o(&b, 0.45, 0.14 + 0.08 * k, 0.1 + 0.22 * k);
    // Hands up to the cheeks.
    let up = ease_out_back(((t - 0.1) / 0.4).clamp(0.0, 1.0));
    hand(p, vec2(-1.02, 1.1 - 0.75 * up), 0.35, 0.95, true, Hand::Open(0.3));
    hand(p, vec2(1.02, 1.1 - 0.75 * up), -0.35, 0.95, false, Hand::Open(0.3));
    // "!" over it.
    let e = bump(0.2, 1.4, t % 1.6);
    if e > 0.0 {
        let a = vec2(0.0, -1.45 - 0.1 * e);
        p.faded(e).capsule(a + vec2(0.0, -0.25), a + vec2(0.0, 0.02), 0.07, GOLD);
        p.faded(e).circle(a + vec2(0.0, 0.18), 0.075, GOLD);
    }
}

fn handshake(p: &Pen, t: f32) {
    let y = (t * 7.0).sin() * 0.14;
    // Sleeves from either side.
    p.capsule(vec2(-1.6, 0.6 + y * 0.4), vec2(-0.5, 0.1 + y), 0.26, rgba(70, 110, 200, 1.0));
    p.capsule(vec2(1.6, 0.6 + y * 0.4), vec2(0.5, 0.1 + y), 0.26, rgba(220, 110, 60, 1.0));
    hand(p, vec2(-0.28, y), 1.3, 1.3, true, Hand::Fist);
    hand(p, vec2(0.28, y + 0.02), -1.3, 1.3, false, Hand::Thumb);
    let k = (t * 1.5).fract();
    sparkle(p, vec2(0.0, -0.75), 0.2 * bump(0.0, 0.5, k), GOLD);
    sparkle(p, vec2(-0.7, -0.55), 0.13 * bump(0.3, 0.8, k), GOLD);
    sparkle(p, vec2(0.7, -0.6), 0.13 * bump(0.5, 1.0, k), GOLD);
}

fn thinking(p: &Pen, t: f32) {
    let tilt = (t * 1.1).sin() * 0.06;
    let b = p.moved(Vec2::ZERO, tilt, Vec2::ONE);
    face(&b, SKIN, 0.0);
    brows(&b, 0.0, 0.0);
    b.stroke(&[vec2(0.22, -0.56), vec2(0.5, -0.64)], 0.07, INK);
    eyes_open(&b, vec2(0.7, -0.8), blink(t), 1.0);
    b.stroke(&[vec2(-0.2, 0.42), vec2(0.18, 0.36)], 0.08, INK);
    // A finger stroking the chin.
    let stroke = (t * 4.0).sin() * 0.05;
    hand(p, vec2(0.28 + stroke, 1.05), -0.25, 1.0, false, Hand::Point);
    // Question marks rising, one after the other.
    for k in 0..3 {
        let f = ((t * 0.7 + k as f32 / 3.0) % 1.0).max(0.0);
        let a = bump(0.0, 1.0, f);
        question(
            &p.faded(a),
            vec2(0.95 + k as f32 * 0.18, -0.9 - f * 0.6),
            0.3 + 0.08 * k as f32,
            INK,
        );
    }
}

fn phew(p: &Pen, t: f32) {
    let cycle = t % 2.2;
    face(p, SKIN, 0.0);
    eyes_happy(p, 0.07);
    brows(p, 0.06, -0.25);
    smile(p, 0.4, 0.06, 0.38);
    // The sweat runs down the brow, the hand wipes it off.
    let wipe = smooth(0.9, 1.5, cycle);
    let fall = smooth(0.0, 0.9, cycle);
    if cycle < 1.25 {
        drop(p, vec2(0.62, -0.7 + 0.25 * fall), 0.1, TEAR);
    } else {
        let f = (cycle - 1.25) / 0.6;
        drop(&p.faded(1.0 - f), vec2(0.9 + f * 0.7, -0.6 + f * f * 0.9), 0.09, TEAR);
    }
    let x = -0.9 + 1.9 * wipe;
    let lift = if cycle > 0.8 && cycle < 1.6 { 0.0 } else { 0.5 };
    hand(p, vec2(x, -0.55 + lift), 1.45, 0.9, false, Hand::Open(0.0));
}

fn cool(p: &Pen, t: f32) {
    let nod = (t * 2.2).sin() * 0.04 * smooth(0.6, 1.0, t);
    let b = p.moved(vec2(0.0, nod), nod * 0.6, Vec2::ONE);
    face(&b, SKIN, 0.0);
    // The smirk.
    let pts: Vec<Vec2> = (0..=10)
        .map(|k| {
            let x = -0.3 + 0.62 * k as f32 / 10.0;
            vec2(x, 0.4 - (x + 0.3) * 0.18 + ((x - 0.1) * 6.0).sin().abs() * 0.0)
        })
        .collect();
    b.stroke(&pts, 0.08, INK);
    // The glasses slide down and land.
    let y = -1.4 + 1.26 * ease_out_back((t / 0.55).clamp(0.0, 1.0));
    let g = b.moved(vec2(0.0, y), 0.0, Vec2::ONE);
    for x in [-0.37f32, 0.37] {
        let lens =
            [vec2(x - 0.3, -0.1), vec2(x + 0.3, -0.1), vec2(x + 0.24, 0.12), vec2(x - 0.24, 0.12)];
        g.fill(&lens, rgba(20, 20, 30, 1.0));
        g.fill(
            &[vec2(x - 0.22, -0.06), vec2(x - 0.08, -0.06), vec2(x - 0.16, 0.06)],
            Color::new(1.0, 1.0, 1.0, 0.18),
        );
    }
    g.stroke(&[vec2(-0.7, -0.08), vec2(0.7, -0.08)], 0.08, rgba(20, 20, 30, 1.0));
    // A glint across them now and then.
    let k = (t * 0.8) % 1.0;
    if t > 0.6 {
        sparkle(&g, vec2(0.6, -0.12), 0.22 * bump(0.0, 0.25, k), WHITE);
    }
}

fn laugh(p: &Pen, t: f32) {
    let shake = (t * 22.0).sin() * 0.05;
    let b = p.moved(
        vec2(0.0, shake),
        (t * 6.0).sin() * 0.08,
        vec2(1.0 + shake * 0.6, 1.0 - shake * 0.6),
    );
    face(&b, SKIN, 0.0);
    eyes_squeezed(&b);
    mouth_open(&b, 0.16, 0.48, 0.42 + shake, true);
    // Tears of joy spurting out.
    for (s, phase) in [(-1.0f32, 0.0f32), (1.0, 0.35)] {
        let f = (t * 1.6 + phase).fract();
        let at = vec2(s * (0.58 + f * 0.6), -0.05 - f * 0.35 + f * f * 0.9);
        drop(&b.faded(1.0 - f), at, 0.09, TEAR);
    }
    // Holding its belly.
    hand(p, vec2(-0.72, 1.05 + shake), 0.9, 0.9, true, Hand::Fist);
    hand(p, vec2(0.72, 1.05 - shake), -0.9, 0.9, false, Hand::Fist);
}

fn ice(p: &Pen, t: f32) {
    let shiver = vec2((t * 40.0).sin() * 0.025, 0.0);
    let b = p.moved(shiver, 0.0, Vec2::ONE);
    let blue = rgba(150, 210, 255, 1.0);
    face(&b, blue, 0.7);
    eyes_open(&b, vec2(0.0, 0.4), blink(t) * 0.8, 0.9);
    brows(&b, 0.05, -0.35);
    // Chattering teeth.
    let open = 0.06 + 0.05 * (t * 30.0).sin().abs();
    b.fill(
        &[
            vec2(-0.34, 0.33),
            vec2(0.34, 0.33),
            vec2(0.34, 0.33 + open + 0.12),
            vec2(-0.34, 0.33 + open + 0.12),
        ],
        MOUTH,
    );
    b.fill(&[vec2(-0.32, 0.33), vec2(0.32, 0.33), vec2(0.32, 0.4), vec2(-0.32, 0.4)], WHITE);
    b.fill(
        &[
            vec2(-0.32, 0.38 + open),
            vec2(0.32, 0.38 + open),
            vec2(0.32, 0.45 + open),
            vec2(-0.32, 0.45 + open),
        ],
        WHITE,
    );
    for x in [-0.16f32, 0.0, 0.16] {
        b.stroke(&[vec2(x, 0.33), vec2(x, 0.45 + open)], 0.025, rgba(150, 170, 190, 1.0));
    }
    // Icicles on its chin, frost round it.
    for (x, l) in [(-0.4f32, 0.3f32), (-0.1, 0.42), (0.22, 0.34), (0.5, 0.22)] {
        b.fill(
            &[vec2(x - 0.07, 0.84), vec2(x + 0.07, 0.84), vec2(x, 0.84 + l)],
            rgba(210, 240, 255, 0.95),
        );
    }
    for k in 0..6 {
        let f = (t * 0.5 + k as f32 / 6.0).fract();
        let a = k as f32 * 1.7;
        sparkle(
            p,
            vec2(a.cos() * 1.35, -1.1 + f * 2.4),
            0.12 * bump(0.0, 1.0, f),
            rgba(220, 245, 255, 1.0),
        );
    }
}

fn wave(p: &Pen, t: f32) {
    let sway = (t * 2.5).sin() * 0.05;
    let b = p.moved(Vec2::ZERO, sway, Vec2::ONE);
    face(&b, SKIN, 0.0);
    eyes_happy(&b, 0.075);
    mouth_open(&b, 0.24, 0.32, 0.24, false);
    blush(&b, 1.0);
    let up = ease_out_back((t / 0.4).clamp(0.0, 1.0));
    let swing = (t * 9.0).sin() * 0.45;
    hand(p, vec2(1.05, 0.9 - 1.05 * up), swing - 0.2, 1.1, false, Hand::Open(0.6));
}

fn playful(p: &Pen, t: f32) {
    let tilt = (t * 3.0).sin() * 0.14;
    let b = p.moved(vec2(0.0, -(t * 6.0).sin().abs() * 0.05), tilt, Vec2::ONE);
    face(&b, SKIN, 0.0);
    wink(&b, t);
    smile(&b, 0.4, 0.14, 0.3);
    // The tongue out, wagging.
    let wag = (t * 8.0).sin() * 0.12;
    b.ellipse(vec2(0.06 + wag * 0.3, 0.55), 0.17, 0.2, wag, TONGUE);
    b.stroke(
        &[vec2(0.06 + wag * 0.3, 0.45), vec2(0.06 + wag * 0.3, 0.62)],
        0.03,
        rgba(200, 60, 70, 1.0),
    );
    blush(&b, 0.8);
}

fn grin(p: &Pen, t: f32) {
    let b = hop(p, t, 0.9, 0.12);
    face(&b, SKIN, 0.0);
    eyes_happy(&b, 0.08);
    // The grin, all teeth.
    let w = 0.5;
    let shape: Vec<Vec2> = (0..=16)
        .map(|k| {
            let a = std::f32::consts::PI - k as f32 / 16.0 * std::f32::consts::PI;
            vec2(a.cos() * w, 0.2 + a.sin() * 0.36)
        })
        .collect();
    b.fill(&shape, INK);
    let teeth: Vec<Vec2> = (0..=16)
        .map(|k| {
            let a = std::f32::consts::PI - k as f32 / 16.0 * std::f32::consts::PI;
            vec2(a.cos() * (w - 0.05), 0.23 + a.sin() * 0.3)
        })
        .collect();
    b.fill(&teeth, WHITE);
    b.stroke(&[vec2(-0.43, 0.36), vec2(0.43, 0.36)], 0.035, rgba(190, 190, 190, 1.0));
    for x in [-0.25f32, 0.0, 0.25] {
        b.stroke(&[vec2(x, 0.24), vec2(x, 0.5)], 0.03, rgba(190, 190, 190, 1.0));
    }
    sparkle(&b, vec2(0.38, 0.3), 0.2 * bump(0.0, 0.3, t % 0.9), WHITE);
}

fn sob(p: &Pen, t: f32) {
    let heave = (t * 6.0).sin() * 0.04;
    let b = p.moved(vec2(0.0, heave), 0.0, vec2(1.0 - heave, 1.0 + heave));
    face(&b, SKIN, 0.0);
    brows(&b, 0.02, -0.4);
    eyes_closed(&b);
    // The wail.
    let shape: Vec<Vec2> = (0..=16)
        .map(|k| {
            let a = k as f32 / 16.0 * std::f32::consts::PI;
            vec2(a.cos() * 0.34, 0.62 - a.sin() * 0.3)
        })
        .collect();
    b.fill(&shape, MOUTH);
    // Two streams of tears.
    for s in [-1.0f32, 1.0] {
        for k in 0..5 {
            let f = (t * 2.2 + k as f32 / 5.0).fract();
            let at = vec2(s * (0.45 + f * 0.25), -0.02 + f * 1.4);
            b.faded(1.0 - f * 0.6).circle(at, 0.1 - f * 0.03, TEAR);
        }
        b.capsule(vec2(s * 0.4, 0.0), vec2(s * 0.5, 0.5), 0.07, TEAR);
    }
}

fn huff(p: &Pen, t: f32) {
    let rage = smooth(0.0, 1.2, t);
    let tremble = vec2((t * 37.0).sin(), (t * 29.0).cos()) * 0.02 * rage;
    let b = p.moved(tremble, 0.0, Vec2::ONE);
    face(&b, rgba(255, 90, 70, 1.0), 0.55 * rage);
    brows(&b, -0.04, 0.9);
    eyes_open(&b, Vec2::ZERO, 0.55, 0.9);
    smile(&b, 0.3, -0.12, 0.48);
    // Steam from both sides.
    for s in [-1.0f32, 1.0] {
        for k in 0..3 {
            let f = (t * 1.4 + k as f32 / 3.0).fract();
            let at = vec2(s * (1.05 + f * 0.45), -0.2 - f * 0.8);
            puff(&p.faded((1.0 - f) * rage), at, 0.12 + f * 0.18, rgba(235, 235, 235, 0.9));
        }
    }
}

fn please(p: &Pen, t: f32) {
    let bob = (t * 4.0).sin() * 0.04;
    let b = p.moved(vec2(0.0, bob), 0.0, Vec2::ONE);
    face(&b, SKIN, 0.0);
    brows(&b, 0.08, -0.45);
    // Big shining eyes, brimming.
    for x in [-0.36f32, 0.36] {
        let c = vec2(x, -0.12);
        b.ellipse(c, 0.2, 0.23, 0.0, INK);
        b.circle(c + vec2(-0.06, -0.08), 0.08, WHITE);
        b.circle(c + vec2(0.07, 0.07), 0.04, WHITE);
        b.arc(c, 0.21, 0.4, std::f32::consts::PI - 0.4, 0.05, rgba(120, 190, 255, 0.9));
    }
    mouth_wavy(&b, 0.45, 0.16, 0.02, t * 10.0);
    blush(&b, 1.0);
    // Hands pressed together.
    hand(p, vec2(-0.1, 1.0 - bob), 0.15, 1.0, true, Hand::Open(0.0));
    hand(p, vec2(0.1, 1.0 - bob), -0.15, 1.0, false, Hand::Open(0.0));
    let k = (t * 1.3).fract();
    sparkle(p, vec2(-1.05, -0.6), 0.16 * bump(0.0, 0.5, k), GOLD);
    sparkle(p, vec2(1.1, -0.4), 0.14 * bump(0.4, 0.9, k), GOLD);
}

fn sleepy(p: &Pen, t: f32) {
    let nod = (t * 1.2).sin();
    let b = p.moved(vec2(0.0, nod * 0.04), nod * 0.12, Vec2::ONE);
    face(&b, SKIN, 0.0);
    eyes_closed(&b);
    b.ellipse(vec2(0.05, 0.45), 0.1, 0.07, 0.0, MOUTH);
    // A bubble from the nose, growing and popping.
    let f = (t * 0.6).fract();
    if f < 0.85 {
        let r = 0.05 + f * 0.25;
        b.circle(vec2(0.28 + r * 0.6, 0.22), r, Color::new(0.7, 0.88, 1.0, 0.45));
        b.ring(vec2(0.28 + r * 0.6, 0.22), r, r, 0.025, Color::new(0.7, 0.88, 1.0, 0.8));
    }
    for k in 0..3 {
        let g = (t * 0.5 + k as f32 / 3.0).fract();
        zed(
            &p.faded(bump(0.0, 1.0, g)),
            vec2(0.8 + g * 0.6, -0.7 - g * 0.9),
            0.18 + g * 0.18,
            rgba(140, 170, 255, 1.0),
        );
    }
}

fn rofl(p: &Pen, t: f32) {
    let b = p.moved(vec2(0.0, -(t * 4.0).sin().abs() * 0.1), t * 4.5, Vec2::ONE);
    face(&b, SKIN, 0.0);
    eyes_squeezed(&b);
    mouth_open(&b, 0.16, 0.48, 0.4, true);
    for s in [-1.0f32, 1.0] {
        let f = (t * 2.0 + s * 0.25).fract();
        drop(&b.faded(1.0 - f), vec2(s * (0.6 + f * 0.5), -0.1 + f * 0.3), 0.09, TEAR);
    }
}

fn scream(p: &Pen, t: f32) {
    let k = smooth(0.0, 0.3, t);
    let shake = vec2((t * 41.0).sin(), (t * 35.0).cos()) * 0.03;
    let b = p.moved(shake, 0.0, vec2(1.0 - 0.1 * k, 1.0 + 0.22 * k));
    face(&b, rgba(190, 220, 255, 1.0), 0.45 * k);
    brows(&b, 0.12 * k, -0.5);
    eyes_wide(&b, Vec2::ZERO, 0.85 + 0.15 * k);
    mouth_o(&b, 0.5, 0.16, 0.12 + 0.3 * k);
    // Hands on its cheeks, squeezing.
    let squeeze = (t * 6.0).sin() * 0.04;
    hand(p, vec2(-0.88 + squeeze, 0.3), 0.25, 0.95, true, Hand::Open(0.1));
    hand(p, vec2(0.88 - squeeze, 0.3), -0.25, 0.95, false, Hand::Open(0.1));
}

fn no_look(p: &Pen, t: f32) {
    face(p, SKIN, 0.0);
    // Peeking now and then through the fingers of one hand.
    let peek = bump(0.8, 1.8, t % 2.4);
    eyes_open(p, vec2(0.6, 0.0), 1.0, 1.0);
    mouth_wavy(p, 0.42, 0.2, 0.025, t * 8.0);
    blush(p, 1.0);
    hand(p, vec2(-0.36, -0.12), 0.25, 1.1, true, Hand::Open(0.0));
    hand(
        p,
        vec2(0.36 + peek * 0.5, -0.12 + peek * 0.25),
        -0.25 - peek * 0.5,
        1.1,
        false,
        Hand::Open(peek),
    );
}

fn salute(p: &Pen, t: f32) {
    let snap = ease_out_back(((t - 0.15) / 0.3).clamp(0.0, 1.0));
    let b = p.moved(Vec2::ZERO, 0.0, vec2(1.0, 1.0 + 0.04 * snap));
    face(&b, SKIN, 0.0);
    brows(&b, -0.02, 0.35);
    eyes_open(&b, Vec2::ZERO, blink(t), 1.0);
    b.stroke(&[vec2(-0.22, 0.42), vec2(0.22, 0.42)], 0.08, INK);
    let at = vec2(0.95 - 0.3 * snap, 0.9 - 1.55 * snap);
    hand(p, at, -1.05 * snap, 1.05, false, Hand::Open(0.0));
}

fn mind_blown(p: &Pen, t: f32) {
    let c = t % 2.6;
    let build = smooth(0.0, 0.9, c);
    let shake =
        vec2((t * 45.0).sin(), (t * 39.0).cos()) * 0.03 * build * if c < 0.9 { 1.0 } else { 0.0 };
    let b = p.moved(shake, 0.0, Vec2::ONE);
    face(&b, SKIN, 0.0);
    if c < 0.9 {
        eyes_wide(&b, Vec2::ZERO, 0.8 + 0.2 * build);
        mouth_o(&b, 0.5, 0.12, 0.08 + 0.12 * build);
    } else {
        // Spirals for eyes, a smile of wonder.
        for x in [-0.36f32, 0.36] {
            let pts: Vec<Vec2> = (0..30)
                .map(|k| {
                    let a = k as f32 * 0.5 + t * 6.0;
                    vec2(x, -0.14) + vec2(a.cos(), a.sin()) * (0.02 + k as f32 * 0.0065)
                })
                .collect();
            b.stroke(&pts, 0.04, INK);
        }
        mouth_o(&b, 0.48, 0.18, 0.2);
        // The burst over its head.
        let f = smooth(0.9, 1.6, c);
        let fade = 1.0 - smooth(1.8, 2.5, c);
        for k in 0..7 {
            let a = -std::f32::consts::FRAC_PI_2 + (k as f32 - 3.0) * 0.4;
            let d = vec2(a.cos(), a.sin());
            puff(
                &p.faded(fade),
                vec2(0.0, -0.9) + d * (0.2 + 0.7 * f),
                0.18 + 0.12 * f,
                rgba(250, 240, 220, 0.95),
            );
        }
        for k in 0..8 {
            let a = k as f32 * 0.8 + 0.3;
            let d = vec2(a.cos(), a.sin() * 0.7 - 0.5);
            sparkle(
                &p.faded(fade),
                vec2(0.0, -1.0) + d * (0.4 + 1.1 * f),
                0.16,
                rgba(255, 170, 60, 1.0),
            );
        }
    }
}

fn ok_hand(p: &Pen, t: f32) {
    let b = hop(p, t, 1.6, 0.05);
    face(&b, SKIN, 0.0);
    wink(&b, t);
    smile(&b, 0.36, 0.16, 0.32);
    let up = ease_out_back((t / 0.4).clamp(0.0, 1.0));
    let wiggle = (t * 6.0).sin() * 0.08;
    hand(p, vec2(1.0, 1.0 - 0.95 * up), -0.2 + wiggle, 1.1, false, Hand::Ok);
    sparkle(p, vec2(0.82, -0.55), 0.18 * bump(0.0, 0.4, t % 1.2), GOLD);
}

fn crossed_fingers(p: &Pen, t: f32) {
    let hope = (t * 7.0).sin() * 0.04;
    face(p, SKIN, 0.0);
    brows(p, 0.1, -0.5);
    eyes_open(p, vec2(0.3, -0.6), blink(t), 1.05);
    mouth_wavy(p, 0.42, 0.18, 0.025, t * 12.0);
    hand(p, vec2(1.0 + hope, 0.0), -0.15, 1.1, false, Hand::Crossed);
    let k = (t * 1.1).fract();
    sparkle(p, vec2(1.35, -0.75), 0.15 * bump(0.0, 0.5, k), GOLD);
}

fn call_me(p: &Pen, t: f32) {
    let tilt = 0.12 + (t * 2.0).sin() * 0.04;
    let b = p.moved(Vec2::ZERO, tilt, Vec2::ONE);
    face(&b, SKIN, 0.0);
    wink(&b, t);
    smile(&b, 0.34, 0.16, 0.32);
    let wiggle = (t * 9.0).sin() * 0.15;
    hand(p, vec2(1.05, 0.0), 0.5 + wiggle, 1.1, false, Hand::Shaka);
}

fn nerd(p: &Pen, t: f32) {
    let push = bump(0.0, 0.5, t % 2.2);
    face(p, SKIN, 0.0);
    eyes_open(p, Vec2::ZERO, blink(t), 0.8);
    let up = -push * 0.08;
    for x in [-0.37f32, 0.37] {
        p.ring(vec2(x, -0.13 + up), 0.25, 0.23, 0.07, rgba(40, 30, 20, 1.0));
        p.ellipse(vec2(x, -0.13 + up), 0.22, 0.2, 0.0, Color::new(0.8, 0.9, 1.0, 0.18));
    }
    p.stroke(&[vec2(-0.12, -0.15 + up), vec2(0.12, -0.15 + up)], 0.06, rgba(40, 30, 20, 1.0));
    smile(p, 0.34, 0.12, 0.33);
    // Buck teeth.
    p.fill(&[vec2(-0.12, 0.4), vec2(0.0, 0.4), vec2(0.0, 0.56), vec2(-0.12, 0.56)], WHITE);
    p.fill(&[vec2(0.01, 0.4), vec2(0.13, 0.4), vec2(0.13, 0.56), vec2(0.01, 0.56)], WHITE);
    hand(p, vec2(0.05, 0.75 - push * 0.6), 0.0, 0.9, false, Hand::Point);
    sparkle(p, vec2(0.55, -0.3 + up), 0.18 * bump(0.5, 0.8, t % 2.2), WHITE);
}

fn money_face(p: &Pen, t: f32) {
    let b = hop(p, t, 1.0, 0.07);
    face(&b, SKIN, 0.0);
    let green = rgba(40, 170, 80, 1.0);
    for x in [-0.36f32, 0.36] {
        let c = vec2(x, -0.14);
        let pts: Vec<Vec2> = (0..=14)
            .map(|k| {
                let s = k as f32 / 14.0;
                c + vec2((s * std::f32::consts::TAU * 1.0).sin() * -0.1, (s - 0.5) * 0.34)
            })
            .collect();
        b.stroke(&pts, 0.06, green);
        b.stroke(&[c + vec2(0.0, -0.22), c + vec2(0.0, 0.22)], 0.04, green);
    }
    smile(&b, 0.36, 0.15, 0.3);
    b.fill(
        &[vec2(-0.16, 0.42), vec2(0.16, 0.42), vec2(0.18, 0.78), vec2(-0.18, 0.78)],
        rgba(120, 200, 120, 1.0),
    );
    b.stroke(&[vec2(-0.1, 0.6), vec2(0.1, 0.6)], 0.04, rgba(40, 120, 60, 1.0));
    // Coins flying up.
    for k in 0..4 {
        let f = (t * 1.2 + k as f32 / 4.0).fract();
        let x = (k as f32 - 1.5) * 0.55 * (0.5 + f);
        let at = vec2(x, -0.9 - f * 1.0 + f * f * 0.8);
        p.faded(1.0 - f).circle(at, 0.14, rgba(255, 200, 60, 1.0));
        p.faded(1.0 - f).circle(at, 0.09, rgba(255, 225, 120, 1.0));
    }
}

fn monocle(p: &Pen, t: f32) {
    let tilt = (t * 0.9).sin() * 0.08;
    let b = p.moved(Vec2::ZERO, tilt, Vec2::ONE);
    face(&b, SKIN, 0.0);
    b.stroke(&[vec2(-0.5, -0.36), vec2(-0.22, -0.34)], 0.07, INK);
    let raise = 0.08 + 0.06 * (t * 2.0).sin().abs();
    b.arc(
        vec2(0.36, -0.42 - raise),
        0.16,
        std::f32::consts::PI * 1.15,
        std::f32::consts::PI * 1.85,
        0.07,
        INK,
    );
    b.stroke(&[vec2(-0.5, -0.12), vec2(-0.22, -0.12)], 0.07, INK);
    b.ellipse(vec2(0.36, -0.12), 0.11, 0.15 * blink(t), 0.0, INK);
    b.ring(vec2(0.36, -0.12), 0.24, 0.24, 0.07, GOLD);
    b.circle(vec2(0.36, -0.12), 0.21, Color::new(0.8, 0.9, 1.0, 0.2));
    b.stroke(&[vec2(0.6, -0.08), vec2(0.66, 0.35), vec2(0.5, 0.8)], 0.03, GOLD);
    b.stroke(&[vec2(-0.2, 0.4), vec2(0.2, 0.4)], 0.08, INK);
    sparkle(&b, vec2(0.45, -0.25), 0.16 * bump(0.0, 0.3, t % 1.5), WHITE);
}

fn facepalm(p: &Pen, t: f32) {
    let c = t % 2.6;
    let slap = smooth(0.15, 0.4, c);
    let slide = smooth(0.8, 2.2, c) * 0.25;
    let b = p.moved(vec2(0.0, 0.05 * bump(0.38, 0.6, c)), -0.1 * slap, Vec2::ONE);
    face(&b, SKIN, 0.0);
    brows(&b, -0.03, -0.3);
    eyes_closed(&b);
    smile(&b, 0.25, -0.06, 0.48);
    let at = vec2(1.35 - 1.2 * slap, -0.2 + 0.1 * slap + slide);
    hand(p, at, -0.35 - 0.5 * slap, 1.25, false, Hand::Open(0.15));
    if c > 0.38 && c < 0.7 {
        let f = (c - 0.38) / 0.32;
        for k in 0..4 {
            let a = -2.4 + k as f32 * 0.5;
            let d = vec2(a.cos(), a.sin());
            p.faded(1.0 - f).stroke(
                &[
                    vec2(-0.2, -0.5) + d * (0.95 + f * 0.2),
                    vec2(-0.2, -0.5) + d * (1.15 + f * 0.25),
                ],
                0.06,
                GOLD,
            );
        }
    }
}

fn shrug(p: &Pen, t: f32) {
    let up = bump(0.0, 1.2, t % 1.6);
    let b = p.moved(vec2(0.0, -0.05 * up), 0.12 * up, Vec2::ONE);
    face(&b, SKIN, 0.0);
    brows(&b, 0.1 * up, -0.2);
    eyes_open(&b, vec2(-0.5, 0.0), blink(t), 1.0);
    b.stroke(&[vec2(-0.22, 0.42), vec2(0.0, 0.42), vec2(0.24, 0.36)], 0.08, INK);
    hand(p, vec2(-1.15, 0.65 - 0.3 * up), -1.3 + 0.2 * up, 1.0, true, Hand::Open(0.4));
    hand(p, vec2(1.15, 0.65 - 0.3 * up), 1.3 - 0.2 * up, 1.0, false, Hand::Open(0.4));
}

fn raised_hands(p: &Pen, t: f32) {
    let b = hop(p, t, 0.8, 0.1);
    face(&b, SKIN, 0.0);
    eyes_happy(&b, 0.08);
    mouth_open(&b, 0.22, 0.38, 0.32, true);
    let up = ease_out_back((t / 0.45).clamp(0.0, 1.0));
    let wav = (t * 8.0).sin() * 0.25;
    hand(p, vec2(-1.0, 1.0 - 1.95 * up), -0.35 + wav, 1.0, true, Hand::Open(0.5));
    hand(p, vec2(1.0, 1.0 - 1.95 * up), 0.35 - wav, 1.0, false, Hand::Open(0.5));
    for k in 0..3 {
        let f = (t * 1.3 + k as f32 / 3.0).fract();
        sparkle(p, vec2((k as f32 - 1.0) * 0.7, -1.4 - f * 0.4), 0.15 * bump(0.0, 1.0, f), GOLD);
    }
}

fn melting(p: &Pen, t: f32) {
    let c = t % 3.2;
    let melt = smooth(0.2, 1.6, c) * (1.0 - smooth(2.4, 3.1, c));
    let b = p.moved(vec2(0.0, 0.35 * melt), 0.0, vec2(1.0 + 0.18 * melt, 1.0 - 0.32 * melt));
    // The puddle under it.
    p.ellipse(vec2(0.0, 1.0), 1.0 + 0.5 * melt, 0.12 + 0.12 * melt, 0.0, SKIN_DARK);
    face(&b, SKIN, 0.0);
    // Drips down its sides.
    for (x, l) in [(-0.75f32, 0.5f32), (0.55, 0.65), (0.15, 0.4)] {
        b.capsule(vec2(x, 0.6), vec2(x, 0.6 + l * melt), 0.12, SKIN_DARK);
    }
    eyes_open(&b, vec2(0.0, 0.5), blink(t) * (1.0 - 0.5 * melt), 1.0);
    mouth_wavy(&b, 0.4, 0.32, 0.04, t * 3.0);
}

fn devil(p: &Pen, t: f32) {
    let rock = (t * 2.0).sin() * 0.08;
    let b = p.moved(Vec2::ZERO, rock, Vec2::ONE);
    let purple = rgba(160, 80, 220, 1.0);
    // Horns first (under the outline).
    for s in [-1.0f32, 1.0] {
        b.fill(
            &[vec2(s * 0.45, -0.72), vec2(s * 0.78, -0.48), vec2(s * 0.85, -1.25)],
            rgba(110, 40, 160, 1.0),
        );
    }
    face(&b, purple, 0.85);
    brows(&b, -0.02, 0.8);
    eyes_open(&b, vec2((t * 1.3).sin(), 0.0), blink(t), 0.9);
    // The wicked grin.
    let pts: Vec<Vec2> = (0..=14)
        .map(|k| {
            let x = (k as f32 / 14.0 - 0.5) * 1.0;
            vec2(
                x,
                0.28 + 0.2 * (1.0 - (x / 0.5).powi(2)) - if x.abs() > 0.42 { 0.08 } else { 0.0 },
            )
        })
        .collect();
    b.stroke(&pts, 0.08, rgba(60, 10, 80, 1.0));
    let f = (t * 2.0).fract();
    sparkle(p, vec2(1.0, -0.9), 0.14 * bump(0.0, 0.5, f), rgba(255, 120, 60, 1.0));
}

fn angel(p: &Pen, t: f32) {
    let float = (t * 2.0).sin() * 0.06;
    let b = p.moved(vec2(0.0, float), 0.0, Vec2::ONE);
    face(&b, SKIN, 0.0);
    eyes_closed(&b);
    smile(&b, 0.3, 0.12, 0.36);
    blush(&b, 1.0);
    // The halo, bobbing a little above.
    let h = -1.3 + (t * 2.6).sin() * 0.05;
    b.ring(vec2(0.0, h), 0.6, 0.16, 0.16, rgba(255, 230, 120, 0.35));
    b.ring(vec2(0.0, h), 0.6, 0.16, 0.08, GOLD);
    for k in 0..3 {
        let f = (t * 0.9 + k as f32 / 3.0).fract();
        let a = k as f32 * 2.1 + t;
        sparkle(
            p,
            vec2(a.cos() * 1.3, -0.4 + a.sin() * 0.7),
            0.13 * bump(0.0, 1.0, f),
            rgba(255, 240, 180, 1.0),
        );
    }
}

fn star_eyes(p: &Pen, t: f32) {
    let b = hop(p, t, 0.9, 0.08);
    face(&b, SKIN, 0.0);
    let pulse = 1.0 + 0.15 * (t * 8.0).sin();
    for x in [-0.36f32, 0.36] {
        star(&b, vec2(x, -0.14), 0.26 * pulse, t * 1.5, rgba(255, 170, 30, 1.0));
        star(&b, vec2(x, -0.14), 0.15 * pulse, t * 1.5, rgba(255, 230, 120, 1.0));
    }
    mouth_open(&b, 0.22, 0.36, 0.3, true);
    for k in 0..4 {
        let f = (t * 1.4 + k as f32 / 4.0).fract();
        let a = k as f32 * 1.6;
        sparkle(p, vec2(a.cos(), a.sin()) * (1.2 + 0.2 * f), 0.16 * bump(0.0, 1.0, f), GOLD);
    }
}

fn eyes(p: &Pen, t: f32) {
    // Two eyes, darting left, right, and a long side-look.
    let c = t % 3.0;
    let look = if c < 0.6 {
        -1.0
    } else if c < 1.2 {
        1.0
    } else if c < 2.4 {
        -1.0
    } else {
        0.0
    };
    let x = look * 0.12;
    let open = blink(t);
    for s in [-0.5f32, 0.5] {
        let c = vec2(s, 0.0);
        p.ellipse(c, 0.42, 0.56 * open.max(0.08), 0.0, INK);
        p.ellipse(c, 0.36, 0.5 * open.max(0.05), 0.0, WHITE);
        if open > 0.3 {
            p.circle(c + vec2(x * 1.5, 0.08), 0.17, INK);
            p.circle(c + vec2(x * 1.5 - 0.05, 0.03), 0.05, WHITE);
        }
    }
}

fn heart(p: &Pen, t: f32) {
    // A double beat.
    let k = t % 1.0;
    let beat = 1.0 + 0.16 * bump(0.0, 0.16, k) + 0.1 * bump(0.2, 0.34, k);
    let red = rgba(235, 40, 70, 1.0);
    p.faded(0.35).circle(vec2(0.0, 0.0), 1.05 * beat, Color::new(1.0, 0.4, 0.5, 0.25));
    heart_shape(p, vec2(0.0, 0.05), 0.95 * beat, rgba(180, 20, 50, 1.0));
    heart_shape(p, vec2(0.0, 0.02), 0.86 * beat, red);
    p.ellipse(vec2(-0.42 * beat, -0.32 * beat), 0.2, 0.12, -0.6, Color::new(1.0, 1.0, 1.0, 0.45));
    for j in 0..3 {
        let f = (t * 0.8 + j as f32 / 3.0).fract();
        let at = vec2((j as f32 - 1.0) * 0.8 + (f * 9.0).sin() * 0.12, -0.6 - f * 1.2);
        heart_shape(&p.faded(1.0 - f), at, 0.18, rgba(255, 110, 140, 1.0));
    }
}

/// Development aid (screenshots): every character acting, `t` seconds in.
pub fn sheet(t: f32) {
    let codes: Vec<&str> =
        snook_legend_proto::REACTIONS.iter().map(|r| r.code).filter(|c| is_character(c)).collect();
    let (w, h) = (crate::ui::vw(), crate::ui::vh());
    clear_background(Color::new(0.1, 0.07, 0.05, 1.0));
    let cols = 9;
    let rows = codes.len().div_ceil(cols);
    let cell = (w / cols as f32).min(h / rows as f32);
    for (k, code) in codes.iter().enumerate() {
        let c = vec2((k % cols) as f32 + 0.5, (k / cols) as f32 + 0.5) * cell;
        draw(code, c, cell * 0.8, 1.0, t + k as f32 * 0.37);
    }
}
