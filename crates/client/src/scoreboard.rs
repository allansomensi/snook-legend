//! The score panel at the top of a game: each player's picture and name in a
//! box at either side, and between them two rails, one per player, holding
//! the balls of their group they have made, drawn as spheres seen at an
//! angle, in the order they went down. A ball made rolls along its rail to
//! its place. The player at the table stands out; the other is dimmed.

use macroquad::prelude::*;
use snook_legend_core::{BallId, BallKind, Group};

use crate::game::{Match, Phase};
use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::paint::{draw_circle, fade};
use crate::textures::ball_color;
use crate::ui::{BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_gradient, rrect_lines};

/// Where the panel's parts go.
#[derive(Debug, Clone, Copy)]
pub struct Board {
    pub avatars: [Rect; 2],
    pub rails: [Rect; 2],
    /// The whole panel.
    pub all: Rect,
}

/// The height of an avatar box, and of the whole panel.
pub const AVATAR: f32 = 54.0;
/// The width of an avatar box (room for a name under the picture).
const AVATAR_W: f32 = 86.0;
const GAP: f32 = 8.0;
const RAIL_H: f32 = 24.0;

impl Board {
    /// The panel centred in a bar `sw` wide, never under `left` pixels of
    /// buttons on the left (nor the same on the right).
    pub fn layout(sw: f32, left: f32, top: f32) -> Board {
        let room = sw - 2.0 * left;
        // Just long enough for a group and its 8 (eight balls) plus the
        // clock at the far end: no empty stretch of rail.
        let step = RAIL_H - 2.0 + 2.5;
        let full = 8.0 + 8.0 * step + 40.0;
        let rail_w = (room - 2.0 * (AVATAR_W + GAP)).clamp(190.0, full);
        let total = rail_w + 2.0 * (AVATAR_W + GAP);
        let x0 = ((sw - total) / 2.0).max(left).min(sw - total - 8.0).max(4.0);
        let rx = x0 + AVATAR_W + GAP;
        let mid = top + AVATAR / 2.0;
        Board {
            avatars: [
                Rect::new(x0, top, AVATAR_W, AVATAR),
                Rect::new(x0 + total - AVATAR_W, top, AVATAR_W, AVATAR),
            ],
            rails: [
                Rect::new(rx, mid - RAIL_H - 2.0, rail_w, RAIL_H),
                Rect::new(rx, mid + 2.0, rail_w, RAIL_H),
            ],
            all: Rect::new(x0, top, total, AVATAR),
        }
    }

    /// Everything of seat `s` (to click or hover).
    pub fn seat(&self, s: usize) -> Rect {
        self.avatars[s].combine_with(self.rails[s])
    }
}

/// The balls made so far, in the order they went down, and when each was
/// seen going down (its arrival on the rail is animated from then).
#[derive(Default)]
pub struct Made {
    key: u64,
    order: Vec<(BallId, f64)>,
}

/// How long a ball takes to roll along a whole rail (s); shorter trips
/// take less.
const ROLL_TIME: f64 = 0.85;
/// Balls made on the same shot arrive one after the other.
const STAGGER: f64 = 0.14;

impl Made {
    /// Follows the table: balls that went down are added (animated), balls
    /// back on the table removed. A new game, or a new rack, starts over;
    /// what was down already when a game is first seen is placed without
    /// animation, in number order.
    pub fn update(&mut self, m: &Match, now: f64) {
        let key = match_key(m);
        let fresh = key != self.key;
        if fresh {
            self.key = key;
            self.order.clear();
        }
        let down: Vec<BallId> =
            m.balls().iter().filter(|b| b.id.is_object() && !b.in_play()).map(|b| b.id).collect();
        if down.is_empty() {
            self.order.clear();
            return;
        }
        self.order.retain(|(id, _)| down.contains(id));
        let mut next = now;
        for id in down {
            if !self.order.iter().any(|(b, _)| *b == id) {
                let at = if fresh { f64::NEG_INFINITY } else { next };
                self.order.push((id, at));
                next += STAGGER;
            }
        }
    }

    /// The balls of `group` made, in order, with when each arrived.
    fn of(&self, group: Group) -> impl Iterator<Item = (BallId, f64)> + '_ {
        self.order.iter().copied().filter(move |(id, _)| group.contains(*id))
    }
}

/// Changes when another game is shown (another match, the next rack).
fn match_key(m: &Match) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    m.names.hash(&mut h);
    m.games_played.hash(&mut h);
    m.identity().hash(&mut h);
    h.finish()
}

/// Draws the panel. Returns nothing: clicks are the caller's (see [`Board::seat`]).
pub fn draw(ui: &mut Ui, m: &Match, made: &Made, board: &Board, now: f64) {
    let over = matches!(m.phase, Phase::GameOver);
    let current = m.game.current_player();
    for seat in 0..2 {
        let active = seat == current && !over;
        avatar(ui, m, seat, board.avatars[seat], active);
        rail(ui, m, made, seat, board.rails[seat], active, now);
        // The player who isn't at the table is dimmed a little.
        if !active && !over {
            let dim = Color::new(0.0, 0.0, 0.0, 0.22);
            rrect(board.avatars[seat], 9.0, dim);
            rrect(board.rails[seat], RAIL_H / 2.0, dim);
        }
    }
}

/// A player's box: their picture, their name under it, the games won.
fn avatar(ui: &mut Ui, m: &Match, seat: usize, r: Rect, active: bool) {
    rrect_gradient(r, 9.0, Color::new(0.20, 0.15, 0.10, 0.96), Color::new(0.08, 0.06, 0.04, 0.96));
    rrect_lines(r, 9.0, if active { 2.0 } else { 1.0 }, if active { GOLD } else { BORDER });
    let face = vec2(r.x + r.w / 2.0, r.y + 19.0);
    match m.opponent.as_ref().filter(|_| seat == 1 && m.online.is_none()) {
        // The computer's own portrait.
        Some(o) => crate::opponents::draw_portrait(o, face, 15.0),
        None => {
            let code = m.avatars[seat].as_str();
            crate::avatars::draw(code, face, 15.0);
        }
    }
    // The name, under the picture.
    let color = match &m.online {
        Some(o) => crate::ui::name_color_of(o.admins[seat], o.ratings[seat]),
        None if active => GOLD,
        None => CREAM,
    };
    // Whole when it fits (a little smaller if it must), else cut short.
    let (name, size) = ui.fitted(&m.names[seat], 11.0, r.w - 10.0, true);
    let w = ui.text_size(&name, size, true).x;
    ui.text(&name, r.x + (r.w - w) / 2.0, r.y + r.h - size - 6.5, size, color, true);
    // Online: the country's flag in the top left corner.
    if let Some(o) = &m.online {
        crate::flags::draw(&o.countries[seat], r.x + 4.0, r.y + 4.0, 10.0);
    }
    // Games won, in a corner badge (from the second game on).
    if m.games_played > 0 || m.wins[seat] > 0 {
        let n = m.wins[seat].to_string();
        let bw = ui.text_size(&n, 11.0, true).x + 9.0;
        let b = Rect::new(r.x + r.w - bw - 2.0, r.y + 2.0, bw.max(16.0), 15.0);
        rrect(b, 7.5, Color::new(0.0, 0.0, 0.0, 0.55));
        ui.text(
            &n,
            b.x + (b.w - ui.text_size(&n, 11.0, true).x) / 2.0,
            b.y + 1.5,
            11.0,
            GOLD,
            true,
        );
    }
    if ui.hovering(r) {
        let mut tip = m.names[seat].clone();
        if let Some(o) = &m.online {
            tip = tw("{name} · rating {r}", &[("name", &tip), ("r", &o.ratings[seat])]);
        }
        ui.tooltip(&tip, r);
    }
}

/// A player's rail: the balls of their group made, in order, from their
/// own side (the first player's from the left, the second's from the
/// right), and at the far end their clock or rating.
fn rail(ui: &mut Ui, m: &Match, made: &Made, seat: usize, r: Rect, active: bool, now: f64) {
    // A glossy dark tube.
    rrect_gradient(
        r,
        r.h / 2.0,
        Color::new(0.16, 0.14, 0.13, 0.95),
        Color::new(0.04, 0.035, 0.03, 0.95),
    );
    rrect(
        Rect::new(r.x + r.h / 2.0, r.y + 2.0, r.w - r.h, 1.5),
        0.75,
        Color::new(1.0, 1.0, 1.0, 0.07),
    );
    rrect_lines(r, r.h / 2.0, if active { 1.8 } else { 1.0 }, if active { GOLD } else { BORDER });
    let size = r.h - 2.0;
    let step = size + 2.5;
    let pad = 4.0;
    let from_left = seat == 0;
    // Slot k's left edge.
    let slot = |k: usize| {
        if from_left {
            r.x + pad + k as f32 * step
        } else {
            r.x + r.w - pad - size - k as f32 * step
        }
    };
    // Where a ball comes in: the far end of the rail.
    let entry = if from_left { r.x + r.w - pad - size } else { r.x + pad };
    let cy = r.y + r.h / 2.0;
    let mut used = 0;
    match m.game.group_of(seat) {
        Some(group) => {
            for (k, (id, at)) in made.of(group).enumerate() {
                let end = slot(k);
                let x = arrival(entry, end, now - at);
                let alpha = ((now - at) / 0.12).clamp(0.0, 1.0) as f32;
                // Rolling: the ball turns by the distance still to go.
                let roll = (x - end) / (size / 2.0);
                ball(ui, id, vec2(x + size / 2.0, cy), size / 2.0, roll, alpha);
                used = k + 1;
            }
            // On the 8: it waits at the next place, with a soft gold ring.
            if m.game.remaining(group).is_empty() {
                let x = slot(used);
                let c = vec2(x + size / 2.0, cy);
                let pulse = 0.5 + 0.5 * (now as f32 * 3.0).sin();
                crate::paint::draw_circle_lines(
                    c.x,
                    c.y,
                    size / 2.0 + 2.0,
                    1.5,
                    with_alpha(GOLD, 0.4 + 0.4 * pulse),
                );
                ball(ui, BallId::EIGHT, c, size / 2.0, 0.0, 1.0);
                used += 1;
                let hit = Rect::new(x - 2.0, r.y, size + 4.0, r.h);
                if ui.hovering(hit) {
                    ui.tooltip(t("On the 8-ball"), hit);
                }
            }
        }
        None => {
            let text = t("Open table");
            let w = ui.text_size(text, 12.0, false).x;
            let x = if from_left { r.x + 12.0 } else { r.x + r.w - 12.0 - w };
            ui.text(text, x, cy - 7.0, 12.0, MUTED, false);
            used = ((w + 16.0) / step).ceil() as usize;
        }
    }
    // The far end: the shot clock while it runs, a daily game's time, or
    // the rating online.
    let far = |ui: &mut Ui, text: &str, color: Color| {
        let w = ui.text_size(text, 11.5, true).x;
        let x = if from_left { r.x + r.w - w - 10.0 } else { r.x + 10.0 };
        // Not over the balls.
        let clear = if from_left {
            x > slot(used.max(1) - 1) + size + 4.0
        } else {
            x + w < slot(used.max(1) - 1) - 4.0
        };
        if clear || used == 0 {
            ui.text(text, x, cy - 7.0, 11.5, color, true);
        }
    };
    if let Some((left, total)) = m.shot_clock()
        && active
        && matches!(m.phase, Phase::Turn { .. })
        && m.replay.is_none()
    {
        let frac = (left / total.max(1.0)).clamp(0.0, 1.0);
        let color = if left < 10.0 { crate::ui::ALERT } else { GOLD };
        let bar_w = (r.w - r.h) * frac;
        let bx = if from_left { r.x + r.h / 2.0 } else { r.x + r.w - r.h / 2.0 - bar_w };
        rrect(Rect::new(bx, r.y + r.h - 3.0, bar_w, 2.0), 1.0, color);
        far(ui, &format!("{:.0}s", left.ceil()), color);
    } else if let Some((left, color)) = crate::correspondence::card_clock(m, seat) {
        far(ui, &left, color);
    } else if let Some(o) = &m.online {
        far(ui, &o.ratings[seat].to_string(), MUTED);
    }
}

/// Where a ball rolling in from `from` to `to` is `age` seconds after it
/// went down (ease-out: quick, then settling gently into place).
fn arrival(from: f32, to: f32, age: f64) -> f32 {
    if !age.is_finite() || age >= ROLL_TIME {
        return to;
    }
    let d = (from - to).abs();
    // A shorter trip takes less time.
    let length = (ROLL_TIME * (0.45 + 0.55 * (d / 380.0).min(1.0) as f64)).max(0.2);
    let k = (age.max(0.0) / length).min(1.0) as f32;
    let ease = 1.0 - (1.0 - k).powi(3);
    from + (to - from) * ease
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

/// The tilt of every ball's number towards the viewer: none, so the
/// number sits in the very middle of its ball.
const TILT: f32 = 0.0;
/// The number's place when the ball is at rest: facing the viewer squarely,
/// so it reads at a glance (turned aside, it was squashed to a few pixels).
const REST_TURN: f32 = 0.0;

/// A pool ball at `c` with radius `r`, seen at an angle, turned by `roll`
/// (radians about the vertical: a ball rolling sideways).
pub fn ball(ui: &Ui, id: BallId, c: Vec2, r: f32, roll: f32, alpha: f32) {
    let col = ball_color(id);
    let color = with_alpha(Color::from_rgba(col[0], col[1], col[2], 255), alpha);
    let ivory = with_alpha(Color::from_rgba(247, 243, 230, 255), alpha);
    // A soft shadow on the rail.
    crate::paint::draw_ellipse(
        c.x + r * 0.12,
        c.y + r * 0.82,
        r * 0.85,
        r * 0.28,
        with_alpha(BLACK, 0.35 * alpha),
    );
    let turn = REST_TURN + roll;
    if id.kind() == BallKind::Stripe {
        draw_circle(c.x, c.y, r, ivory);
        // The band around the ball, through the number.
        let centre = -r * TILT.sin() * 0.9;
        band(c, r, centre - r * 0.62, centre + r * 0.62, color);
    } else {
        draw_circle(c.x, c.y, r, color);
    }
    // The number circles: two, on opposite sides; the one facing the viewer
    // is drawn foreshortened.
    for side in [0.0, std::f32::consts::PI] {
        let a = turn + side;
        let d = vec3(a.sin() * TILT.cos(), -TILT.sin(), a.cos() * TILT.cos());
        if d.z <= 0.08 {
            continue;
        }
        let rho = r * 0.60;
        let p = vec2(c.x + d.x * r * 0.9, c.y + d.y * r * 0.9);
        let angle = d.y.atan2(d.x).to_degrees();
        // Squashed along the direction it faces, by how much it faces away.
        macroquad::shapes::draw_ellipse(p.x, p.y, rho * d.z, rho, angle, fade(ivory));
        let label = id.0.to_string();
        let fs = if id.0 >= 10 { r * 0.86 } else { r * 1.10 };
        let aspect = d.z.max(0.35);
        let w = ui.text_size(&label, fs, true).x * aspect;
        ui.text_aspect(
            &label,
            p.x - w / 2.0,
            // The digits stand 0.73 em tall on a baseline 0.8 em down: their
            // middle, not the line's, goes on the circle's centre.
            p.y - fs * 0.435,
            fs,
            with_alpha(BLACK, alpha),
            true,
            aspect,
        );
    }
    // Light: a darker rim and a soft highlight, from a shading picture.
    let tex = shading();
    draw_texture_ex(
        &tex,
        c.x - r,
        c.y - r,
        fade(Color::new(1.0, 1.0, 1.0, alpha)),
        DrawTextureParams { dest_size: Some(vec2(2.0 * r, 2.0 * r)), ..Default::default() },
    );
}

/// The part of the disc at `c` (radius `r`) between heights `lo` and `hi`
/// (relative to the centre, y down), filled with `color`.
fn band(c: Vec2, r: f32, lo: f32, hi: f32, color: Color) {
    let (lo, hi) = (lo.clamp(-r, r), hi.clamp(-r, r));
    let (a0, a1) = ((lo / r).asin(), (hi / r).asin());
    let mut pts: Vec<Vec2> = Vec::with_capacity(24);
    for k in 0..=10 {
        let a = a0 + (a1 - a0) * k as f32 / 10.0;
        pts.push(vec2(c.x + r * a.cos(), c.y + r * a.sin()));
    }
    for k in 0..=10 {
        let a = std::f32::consts::PI - a1 + (a1 - a0) * k as f32 / 10.0;
        pts.push(vec2(c.x + r * a.cos(), c.y + r * a.sin()));
    }
    let mid = vec2(c.x, c.y + (lo + hi) / 2.0);
    for i in 0..pts.len() {
        crate::paint::draw_triangle(mid, pts[i], pts[(i + 1) % pts.len()], color);
    }
}

/// A sphere's light as a picture laid over a flat ball: darker towards the
/// rim and the lower right, a soft highlight at the upper left.
fn shading() -> Texture2D {
    thread_local! {
        static TEX: std::cell::OnceCell<Texture2D> = const { std::cell::OnceCell::new() };
    }
    TEX.with(|t| {
        t.get_or_init(|| {
            const N: usize = 96;
            let mut px = vec![0u8; N * N * 4];
            for y in 0..N {
                for x in 0..N {
                    let u = (x as f32 + 0.5) / N as f32 * 2.0 - 1.0;
                    let v = (y as f32 + 0.5) / N as f32 * 2.0 - 1.0;
                    let rr = (u * u + v * v).sqrt();
                    let i = (y * N + x) * 4;
                    let edge = ((1.0 - rr) * N as f32 * 0.5).clamp(0.0, 1.0);
                    if edge <= 0.0 {
                        continue;
                    }
                    let z = (1.0 - (rr * rr).min(1.0)).sqrt();
                    let n = vec3(u, v, z);
                    let light = vec3(-0.5, -0.7, 0.75).normalize();
                    let diffuse = n.dot(light).max(0.0);
                    // Darkness to lay over (black with alpha) …
                    let dark = (0.5 - 0.5 * diffuse).clamp(0.0, 0.5) + 0.18 * (1.0 - z).powf(2.0);
                    // … and a highlight (white with alpha).
                    let h = (light + vec3(0.0, 0.0, 1.0)).normalize();
                    let spec = n.dot(h).max(0.0);
                    let shine = spec.powf(60.0) * 0.85 + spec.powf(8.0) * 0.10;
                    // White over black: the colour is white where it shines.
                    let a = (dark + shine).min(1.0);
                    let w = if a > 0.0 { shine / a } else { 0.0 };
                    let g = (w * 255.0) as u8;
                    px[i] = g;
                    px[i + 1] = g;
                    px[i + 2] = g;
                    px[i + 3] = (a * edge * 255.0) as u8;
                }
            }
            let tex = Texture2D::from_rgba8(N as u16, N as u16, &px);
            tex.set_filter(FilterMode::Linear);
            tex
        })
        .clone()
    })
}

/// A small person in a circle: the picture of a player who chose none.
pub fn silhouette(c: Vec2, r: f32) {
    draw_circle(c.x, c.y, r, Color::new(0.30, 0.25, 0.20, 1.0));
    crate::icons::draw(Icon::Person, c, r * 1.3, Color::new(0.85, 0.78, 0.66, 1.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ball_rolls_in_and_stops_in_its_place() {
        assert_eq!(arrival(300.0, 20.0, 0.0), 300.0);
        let mid = arrival(300.0, 20.0, 0.2);
        assert!(mid < 300.0 && mid > 20.0);
        assert_eq!(arrival(300.0, 20.0, 5.0), 20.0);
        assert_eq!(arrival(300.0, 20.0, f64::NEG_INFINITY), 20.0, "placed without animation");
        // Always closer as time goes on.
        let xs: Vec<f32> = (0..20).map(|k| arrival(300.0, 20.0, k as f64 * 0.05)).collect();
        assert!(xs.windows(2).all(|w| w[1] <= w[0]));
    }

    #[test]
    fn the_board_fits_and_keeps_clear_of_the_buttons() {
        for sw in [800.0, 1280.0, 1920.0] {
            let b = Board::layout(sw, 160.0, 4.0);
            assert!(b.all.x >= 4.0 && b.all.x + b.all.w <= sw - 4.0, "{sw}");
            assert!(b.rails[0].x > b.avatars[0].x + b.avatars[0].w);
            assert!(b.rails[1].y > b.rails[0].y + b.rails[0].h);
            assert!(b.avatars[1].x >= b.rails[0].x + b.rails[0].w);
        }
    }
}
