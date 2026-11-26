//! The match's introduction: for a few seconds when an online game starts,
//! the two players face each other ("VS") with their pictures, names,
//! flags, ratings and leagues, the rules, whether the game is rated and,
//! when it is, how much rating each wins with a win and loses with a loss.
//! A click (or Esc, Space, Enter) skips it; it isn't shown again when the
//! player comes back to the table after a dropped connection.
//!
//! A game against the computer starts the same way: the player and the
//! computer's character (portrait, what they are known as, their strength
//! and how they play), with where the game is played (the tour's stage, a
//! tournament). Nothing about rating: none is at stake.
//!
//! Each card is drawn by [`card`] on its own, so what is added to a
//! player's card later (the glow of a rare item) goes there.

use macroquad::prelude::*;

use snook_legend_proto::{PlayerInfo, Rules};

use crate::i18n::{t, tw};
use crate::paint::{draw_circle, draw_rectangle};
use crate::ui::{CREAM, GOLD, MUTED, Ui, rrect, rrect_lines, tier_color};

/// How long it shows (seconds): coming in, held, going.
pub const LENGTH: f32 = 3.4;
/// The cards' slide in.
const IN: f32 = 0.55;
/// When the "VS" lands (its impact), and how long it takes.
const VS_AT: f32 = 0.38;
const VS_IN: f32 = 0.32;
/// When the stakes and the rules come up.
const STAKES_AT: f32 = 0.62;
/// Everything is in (what a screenshot holds).
const SETTLED: f32 = 1.25;
/// The way out: the cards part a little as everything fades.
const OUT: f32 = 0.45;

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// Out with a small overshoot (`k`: how much), settling at 1.
fn ease_out_back(t: f32, k: f32) -> f32 {
    let t = t.clamp(0.0, 1.0) - 1.0;
    1.0 + (k + 1.0) * t * t * t + k * t * t
}

/// The introduction of a match about to start.
#[derive(Debug, Clone, PartialEq)]
pub struct Intro {
    pub players: [PlayerInfo; 2],
    /// This player's seat (a spectator's: the friend's).
    pub seat: usize,
    pub spectating: bool,
    pub rated: bool,
    /// Each seat's `[win, loss]` (rated games).
    pub stakes: Option<[[i32; 2]; 2]>,
    pub rules: Rules,
    /// A game against the computer: who it is (seat 1).
    pub computer: Option<crate::opponents::Opponent>,
    /// What the game is, under the cards, after the rules ("Against the
    /// computer", "The computer tour · Stage 3 of 15"); empty online.
    pub context: String,
    /// Seconds it has shown.
    age: f32,
    /// Held once in (screenshots: see [`Intro::sample`]).
    hold: bool,
}

impl Intro {
    pub fn new(
        players: &[PlayerInfo; 2],
        seat: usize,
        rated: bool,
        stakes: Option<[[i32; 2]; 2]>,
        rules: Rules,
        spectating: bool,
    ) -> Intro {
        Intro {
            players: players.clone(),
            seat: seat.min(1),
            spectating,
            rated,
            stakes: stakes.filter(|_| rated),
            rules,
            computer: None,
            context: String::new(),
            age: 0.0,
            hold: false,
        }
    }

    /// A game against the computer: `me` (seat 0) against `opponent`.
    pub fn vs_computer(
        me: PlayerInfo,
        opponent: &crate::opponents::Opponent,
        rules: Rules,
        context: String,
    ) -> Intro {
        let computer = PlayerInfo { username: opponent.display_name(), ..Default::default() };
        let mut intro = Intro::new(&[me, computer], 0, false, None, rules, false);
        intro.computer = Some(opponent.clone());
        intro.context = context;
        intro
    }

    /// Development aid (screenshots): held still once in.
    pub fn held(mut self) -> Intro {
        self.hold = true;
        self
    }

    /// Development aid (screenshots, SNOOK_LEGEND_INTRO): a rated game of
    /// two made-up players, held still once in.
    pub fn sample() -> Intro {
        let player = |name: &str, country: &str, rating: i32| PlayerInfo {
            username: name.into(),
            country: country.into(),
            standings: snook_legend_proto::PerRules::both(snook_legend_proto::Standing {
                rating,
                games: 120,
                wins: 70,
            }),
            vip: rating > 1700,
            ..Default::default()
        };
        let players = [player("Ana", "BR", 1684), player("Maximilian_Großmann", "DE", 1761)];
        let mut intro =
            Intro::new(&players, 0, true, Some([[9, -11], [7, -13]]), Rules::Official, false);
        intro.hold = true;
        intro
    }

    /// Draws it over the table; false once it is over or skipped (the
    /// caller drops it).
    pub fn draw(&mut self, ui: &mut Ui, dt: f32) -> bool {
        self.age = if self.hold { (self.age + dt).min(hold_at()) } else { self.age + dt };
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let keys = [KeyCode::Escape, KeyCode::Space, KeyCode::Enter];
        let skip = ui.clicked(Rect::new(0.0, 0.0, sw, sh))
            || (!ui.wants_keyboard && keys.iter().any(|k| is_key_pressed(*k)));
        // (Not the click that started the game.) Skipped, it still goes
        // out smoothly: straight to its way out.
        if skip && self.age > 0.15 && self.age < LENGTH - OUT {
            self.age = LENGTH - OUT;
        }
        if self.age >= LENGTH {
            return false;
        }
        let age = self.age;
        // How far out it has gone (0 in, 1 gone), eased.
        let going = smooth(LENGTH - OUT, LENGTH, age);
        let saved = crate::paint::opacity();
        let layer = |k: f32| crate::paint::set_opacity(saved * k * (1.0 - going));
        // The table dims behind.
        layer(1.0);
        let dim = smooth(0.0, 0.3, age);
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.02, 0.015, 0.01, 0.8 * dim));

        // The cards' size and place: side by side, the badge between them.
        let badge = 34.0f32;
        let gap = (sw * 0.05).clamp(badge * 2.0 + 8.0, 104.0);
        let w = ((sw - gap - 32.0) / 2.0).clamp(200.0, 360.0);
        let h = 304.0f32.min(sh - 120.0).max(260.0);
        let y = ((sh - h) / 2.0 - 22.0).max(12.0);
        let left_x = sw / 2.0 - gap / 2.0 - w;
        let right_x = sw / 2.0 + gap / 2.0;
        // Each slides in from its side (this player's from the left), the
        // second a beat later, with a hint of overshoot; on the way out
        // they part a little.
        let (first, second) = (self.seat, 1 - self.seat);
        let stakes = smooth(STAKES_AT, STAKES_AT + 0.35, age);
        let part = going * 28.0;
        for (k, (seat, x, side)) in
            [(first, left_x, -1.0f32), (second, right_x, 1.0f32)].into_iter().enumerate()
        {
            let t = ((age - k as f32 * 0.08) / IN).clamp(0.0, 1.0);
            let slide = (1.0 - ease_out_back(t, 0.9)) * (w * 0.9 + gap);
            layer(smooth(0.0, 0.6, t));
            card(ui, self, seat, Rect::new(x + side * (slide + part), y, w, h), stakes);
        }

        // "VS" lands between them: it comes down from large, settles with
        // a small bounce, and a ring of light spreads from the impact.
        let c = vec2(sw / 2.0, y + h / 2.0);
        let v = ((age - VS_AT) / VS_IN).clamp(0.0, 1.0);
        if v > 0.0 {
            let scale = 1.0 + 0.7 * (1.0 - ease_out_back(v, 1.6)) * (1.0 - going * 0.2);
            let scale = scale.max(0.6) * (1.0 - 0.12 * going);
            let r = badge * scale;
            layer(smooth(0.0, 0.35, v));
            // A soft glow behind it.
            for i in 0..4 {
                let g = r * (1.25 + 0.22 * i as f32);
                draw_circle(c.x, c.y, g, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.045));
            }
            crate::ui::soft_shadow(Rect::new(c.x - r, c.y - r, 2.0 * r, 2.0 * r), r, 0.9);
            draw_circle(c.x, c.y, r, Color::new(0.11, 0.07, 0.04, 1.0));
            draw_circle(c.x, c.y - r * 0.08, r * 0.86, Color::new(0.16, 0.105, 0.06, 1.0));
            crate::paint::draw_circle_lines(c.x, c.y, r, 2.5, GOLD);
            crate::paint::draw_circle_lines(
                c.x,
                c.y,
                r - 5.0 * scale,
                1.0,
                Color::new(GOLD.r, GOLD.g, GOLD.b, 0.35),
            );
            // The word fits inside the ring, whatever the language.
            let word = t("vs").to_uppercase();
            let room = r * 1.3;
            let size = (26.0 * scale)
                .min(26.0 * scale * room / ui.text_size(&word, 26.0 * scale, true).x.max(1.0));
            let ty = crate::ranking::caps_y(c.y, size);
            ui.text_centered(&word, c.x, ty + 1.5, size, Color::new(0.0, 0.0, 0.0, 0.5), true);
            ui.text_centered(&word, c.x, ty, size, GOLD, true);
            // The impact's ring of light.
            let ring = ((age - VS_AT - VS_IN * 0.55) / 0.5).clamp(0.0, 1.0);
            if ring > 0.0 && ring < 1.0 {
                let e = ease_out_cubic(ring);
                layer(1.0 - ring);
                crate::paint::draw_circle_lines(
                    c.x,
                    c.y,
                    badge * (1.0 + 1.3 * e),
                    2.0 * (1.0 - e) + 0.5,
                    Color::new(1.0, 0.9, 0.6, 0.8),
                );
            }
        }

        // Under them: the rules, and whether it counts for the rating.
        let rules = smooth(STAKES_AT + 0.1, STAKES_AT + 0.45, age);
        if rules > 0.0 {
            layer(rules);
            let kind = if self.computer.is_some() {
                self.context.as_str()
            } else if self.rated {
                t("Rated game")
            } else {
                t("Casual game")
            };
            let line = format!("{} · {}", crate::i18n::rules_name(self.rules), kind);
            let (line, size) = ui.fitted(&line, 16.0, sw - 64.0, true);
            let ly = y + h + 18.0 + (1.0 - ease_out_cubic(rules)) * 8.0;
            let lw = ui.text_size(&line, size, true).x + 36.0;
            let pill = Rect::new((sw - lw) / 2.0, ly, lw, 32.0);
            rrect(pill, 16.0, Color::new(0.0, 0.0, 0.0, 0.6));
            rrect_lines(pill, 16.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.3));
            let color = if self.rated { GOLD } else { CREAM };
            ui.text_centered(
                &line,
                sw / 2.0,
                crate::ranking::text_y(ly + 16.0, size),
                size,
                color,
                true,
            );
            let hint = t("Click to skip");
            ui.text_centered(hint, sw / 2.0, ly + 42.0, 12.0, MUTED, false);
        }
        crate::paint::set_opacity(saved);
        // Nothing under it takes a click while it shows.
        ui.block(Rect::new(0.0, 0.0, sw, sh));
        true
    }
}

/// Where a held introduction (screenshots) stops: all in, or the moment
/// in `SNOOK_LEGEND_INTRO` (`0.4s`: seconds) to see the animation midway.
fn hold_at() -> f32 {
    crate::dev::var("SNOOK_LEGEND_INTRO")
        .and_then(|v| v.strip_suffix('s').and_then(|v| v.parse::<f32>().ok()))
        .filter(|v| *v > 0.0 && *v < LENGTH)
        .unwrap_or(SETTLED)
}

/// One player's card: picture, name and flag, rating and league, and in a
/// rated game what they stand to win and lose (`stakes`: how far those
/// have come up, 0 to 1).
pub fn card(ui: &mut Ui, intro: &Intro, seat: usize, r: Rect, stakes: f32) {
    if let Some(o) = intro.computer.as_ref().filter(|_| seat == 1) {
        computer_card(ui, o, r, stakes);
        return;
    }
    let p = &intro.players[seat];
    let tier = p.tier_in(intro.rules);
    let accent = tier_color(tier);
    crate::ui::soft_shadow(r, 14.0, 0.8);
    crate::ui::rrect_gradient(
        r,
        14.0,
        Color::new(0.11, 0.075, 0.05, 0.98),
        Color::new(0.055, 0.035, 0.025, 0.98),
    );
    // A band of the league's colour along the top, and the edge in it.
    crate::ui::rrect_gradient(
        Rect::new(r.x, r.y, r.w, 64.0),
        14.0,
        Color::new(accent.r, accent.g, accent.b, 0.16),
        Color::new(accent.r, accent.g, accent.b, 0.0),
    );
    rrect_lines(r, 14.0, 1.5, Color::new(accent.r, accent.g, accent.b, 0.85));
    crate::ui::top_light(r, 14.0, 0.06);
    // "You" (this player's card), a small tag clear of the picture.
    let cx = r.x + r.w / 2.0;
    // Against the computer nothing is at stake under the player: what they
    // show (picture, name, rating) sits in the card's middle, level with
    // the computer's fuller card.
    let top = if intro.computer.is_some() { r.y + ((r.h - 210.0) / 2.0).max(0.0) } else { r.y };
    let who = if seat == intro.seat && !intro.spectating { t("You") } else { "" };
    if !who.is_empty() {
        let tw_ = ui.text_size(who, 11.0, true).x + 16.0;
        let tag = Rect::new(cx - tw_ / 2.0, r.y + 10.0, tw_, 18.0);
        rrect(tag, 9.0, Color::new(accent.r, accent.g, accent.b, 0.22));
        ui.text_centered(who, cx, crate::ranking::text_y(tag.y + 9.0, 11.0), 11.0, CREAM, true);
    }
    // Picture.
    let c = vec2(cx, top + 82.0);
    draw_circle(c.x, c.y, 49.0, Color::new(accent.r, accent.g, accent.b, 0.3));
    crate::paint::draw_circle_lines(
        c.x,
        c.y,
        49.0,
        2.0,
        Color::new(accent.r, accent.g, accent.b, 0.9),
    );
    crate::avatars::draw(&p.avatar, c, 44.0);
    // Name and flag, centred as a whole (the flag, the name, the crown).
    let max = r.w - 28.0;
    let crown = |size: f32| if p.vip { size + 4.0 } else { 0.0 };
    let flag = |size: f32| {
        if p.country.is_empty() { 0.0 } else { crate::flags::width((size * 0.78).round()) + 6.0 }
    };
    let mut size = 22.0f32;
    while size > 16.0 && ui.text_size(&p.username, size, true).x + crown(size) + flag(size) > max {
        size -= 1.0;
    }
    let total = (ui.text_size(&p.username, size, true).x + crown(size) + flag(size)).min(max);
    ui.player_name(p, cx - total / 2.0, top + 144.0, size, max);
    // Rating and league.
    let rating = p.rating_in(intro.rules);
    let line = if p.guest && intro.computer.is_some() {
        // Playing the computer without an account: no rating to speak of.
        String::new()
    } else if p.guest {
        t("Guest").to_string()
    } else {
        format!("{rating} · {}", t(tier.name()))
    };
    let (line, ls) = ui.fitted(&line, 17.0, max, true);
    ui.text_centered(&line, cx, top + 176.0, ls, accent, true);
    // At stake: they come up once the cards are in.
    if stakes <= 0.0 {
        return;
    }
    let saved = crate::paint::opacity();
    crate::paint::set_opacity(saved * stakes);
    let rise = (1.0 - ease_out_cubic(stakes)) * 10.0;
    if let Some(at_stake) = intro.stakes {
        let [win, loss] = at_stake[seat];
        let y = r.y + r.h - 62.0 + rise;
        let half = (r.w - 36.0) / 2.0;
        let good = Color::new(0.45, 0.88, 0.5, 1.0);
        let bad = Color::new(1.0, 0.5, 0.42, 1.0);
        for (k, (label, value, color)) in
            [(t("Win"), format!("+{win}"), good), (t("Loss"), format!("{loss}"), bad)]
                .into_iter()
                .enumerate()
        {
            let b = Rect::new(r.x + 12.0 + k as f32 * (half + 12.0), y, half, 50.0);
            rrect(b, 9.0, Color::new(1.0, 1.0, 1.0, 0.05));
            rrect_lines(b, 9.0, 1.0, Color::new(color.r, color.g, color.b, 0.25));
            let (label, size) = ui.fitted(label, 12.0, half - 12.0, true);
            ui.text_centered(&label, b.x + b.w / 2.0, b.y + 6.0, size, MUTED, true);
            ui.text_centered(&value, b.x + b.w / 2.0, b.y + 21.0, 20.0, color, true);
        }
        let hint = tw("{n} rating points at stake", &[("n", &win.max(-loss))]);
        let (hint, size) = ui.fitted(&hint, 12.0, max, false);
        ui.text_centered(&hint, cx, y - 22.0, size, CREAM, false);
    } else if !intro.rated && intro.computer.is_none() {
        let (text, size) = ui.fitted(t("No rating at stake"), 13.0, max, false);
        ui.text_centered(&text, cx, r.y + r.h - 40.0 + rise, size, MUTED, false);
    }
    crate::paint::set_opacity(saved);
}

/// The colour of a computer opponent's strength (1 to 5): green for the
/// gentle ones, through gold, to red for the strongest.
fn strength_color(strength: u8) -> Color {
    match strength {
        0..=1 => Color::new(0.45, 0.8, 0.5, 1.0),
        2 => Color::new(0.55, 0.78, 0.95, 1.0),
        3 => GOLD,
        4 => Color::new(1.0, 0.6, 0.3, 1.0),
        _ => Color::new(0.95, 0.38, 0.35, 1.0),
    }
}

/// The computer's card: its portrait, name, what it is known as and its
/// strength, then (once the cards are in, as the stakes would) how it
/// plays.
fn computer_card(ui: &mut Ui, o: &crate::opponents::Opponent, r: Rect, more: f32) {
    let strength = crate::opponents::strength(o);
    let accent = strength_color(strength);
    crate::ui::soft_shadow(r, 14.0, 0.8);
    crate::ui::rrect_gradient(
        r,
        14.0,
        Color::new(0.11, 0.075, 0.05, 0.98),
        Color::new(0.055, 0.035, 0.025, 0.98),
    );
    crate::ui::rrect_gradient(
        Rect::new(r.x, r.y, r.w, 64.0),
        14.0,
        Color::new(accent.r, accent.g, accent.b, 0.16),
        Color::new(accent.r, accent.g, accent.b, 0.0),
    );
    rrect_lines(r, 14.0, 1.5, Color::new(accent.r, accent.g, accent.b, 0.85));
    crate::ui::top_light(r, 14.0, 0.06);
    let cx = r.x + r.w / 2.0;
    let max = r.w - 28.0;
    // Portrait, in a ring of its strength's colour.
    let c = vec2(cx, r.y + 82.0);
    draw_circle(c.x, c.y, 49.0, Color::new(accent.r, accent.g, accent.b, 0.3));
    crate::paint::draw_circle_lines(
        c.x,
        c.y,
        49.0,
        2.0,
        Color::new(accent.r, accent.g, accent.b, 0.9),
    );
    crate::opponents::draw_portrait(o, c, 44.0);
    // Name.
    let name = o.display_name();
    let (name, size) = ui.fitted(&name, 22.0, max, true);
    ui.text_centered(&name, cx, r.y + 144.0, size, CREAM, true);
    // What they are known as (a personality), and the strength's pips.
    let personality = o.as_personality();
    let mut y = r.y + 176.0;
    if let Some(p) = personality {
        let (epithet, es) = ui.fitted(t(p.epithet), 15.0, max, true);
        ui.text_centered(&epithet, cx, y, es, accent, true);
        y += 26.0;
    }
    let pip = 7.0;
    let pw = crate::opponents::strength_width(pip);
    let label = crate::opponents::strength_label(strength);
    let lw = ui.text_size(&label, 12.0, false).x;
    let total = pw + 10.0 + lw;
    let x = cx - total / 2.0;
    crate::opponents::draw_strength(x, y + 7.0, strength, pip);
    ui.text(&label, x + pw + 10.0, y, 12.0, MUTED, false);
    // How they play, coming up with the rest.
    if more <= 0.0 {
        return;
    }
    let Some(p) = personality else { return };
    let saved = crate::paint::opacity();
    crate::paint::set_opacity(saved * more);
    let rise = (1.0 - ease_out_cubic(more)) * 10.0;
    let top = y + 30.0 + rise;
    let lines = ui.wrap(t(p.description), max, 12.5);
    let room = ((r.y + r.h - 14.0 - top) / 17.0).floor().max(0.0) as usize;
    for (k, line) in lines.iter().take(room).enumerate() {
        let line = if k + 1 == room && lines.len() > room {
            ui.ellipsize(&format!("{line}…"), max, 12.5)
        } else {
            line.clone()
        };
        ui.text_centered(&line, cx, top + k as f32 * 17.0, 12.5, MUTED, false);
    }
    crate::paint::set_opacity(saved);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stakes_show_only_in_rated_games() {
        let players: [PlayerInfo; 2] = Default::default();
        let casual =
            Intro::new(&players, 1, false, Some([[1, -1], [1, -1]]), Rules::Official, false);
        assert_eq!(casual.stakes, None);
        let rated =
            Intro::new(&players, 3, true, Some([[12, -8], [8, -12]]), Rules::Official, true);
        assert_eq!((rated.seat, rated.stakes), (1, Some([[12, -8], [8, -12]])));
    }
}
