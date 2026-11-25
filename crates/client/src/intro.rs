//! The match's introduction: for a few seconds when an online game starts,
//! the two players face each other ("VS") with their pictures, names,
//! flags, ratings and leagues, the rules, whether the game is rated and,
//! when it is, how much rating each wins with a win and loses with a loss.
//! A click (or Esc, Space, Enter) skips it; it isn't shown again when the
//! player comes back to the table after a dropped connection.
//!
//! Each card is drawn by [`card`] on its own, so what is added to a
//! player's card later (the glow of a rare item) goes there.

use macroquad::prelude::*;

use snook_legend_proto::{PlayerInfo, Rules};

use crate::i18n::{t, tw};
use crate::paint::{draw_circle, draw_rectangle};
use crate::ui::{CREAM, GOLD, MUTED, Ui, rrect, rrect_lines, tier_color};

/// How long it shows (seconds): sliding in, held, fading out.
pub const LENGTH: f32 = 3.2;
const IN: f32 = 0.45;
const OUT: f32 = 0.5;

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
    /// Seconds it has shown.
    age: f32,
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
            age: 0.0,
        }
    }

    /// Draws it over the table; false once it is over or skipped (the
    /// caller drops it).
    pub fn draw(&mut self, ui: &mut Ui, dt: f32) -> bool {
        self.age += dt;
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let keys = [KeyCode::Escape, KeyCode::Space, KeyCode::Enter];
        let skip = ui.clicked(Rect::new(0.0, 0.0, sw, sh))
            || (!ui.wants_keyboard && keys.iter().any(|k| is_key_pressed(*k)));
        // (Not the click that started the game.)
        if (skip && self.age > 0.15) || self.age >= LENGTH {
            return false;
        }
        // In, held, out.
        let enter = (self.age / IN).min(1.0);
        let ease = 1.0 - (1.0 - enter).powi(3);
        let fade = ((LENGTH - self.age) / OUT).clamp(0.0, 1.0);
        let saved = crate::paint::opacity();
        crate::paint::set_opacity(saved * fade);
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.02, 0.015, 0.01, 0.78 * ease));
        // The two cards come in from the sides; this player's on the left.
        let w = (sw * 0.34).clamp(220.0, 360.0);
        let h = 300.0f32.min(sh - 80.0);
        let gap = (sw * 0.06).clamp(56.0, 110.0);
        let y = (sh - h) / 2.0 - 10.0;
        let left_x = sw / 2.0 - gap / 2.0 - w;
        let right_x = sw / 2.0 + gap / 2.0;
        let slide = (1.0 - ease) * (sw / 2.0);
        let (first, second) = (self.seat, 1 - self.seat);
        card(ui, self, first, Rect::new(left_x - slide, y, w, h));
        card(ui, self, second, Rect::new(right_x + slide, y, w, h));
        // "VS" between them, landing with a little bounce.
        let pop = if enter < 1.0 { 0.0 } else { ((self.age - IN) / 0.25).min(1.0) };
        let scale = 1.0 + 0.35 * (1.0 - pop) * (pop * std::f32::consts::PI).sin().max(0.0);
        let vs_size = 46.0 * scale * pop.max(0.01);
        let c = vec2(sw / 2.0, y + h / 2.0);
        draw_circle(c.x, c.y, 34.0 * pop, Color::new(0.08, 0.05, 0.03, 0.95));
        crate::paint::draw_circle_lines(c.x, c.y, 34.0 * pop, 2.0, GOLD);
        if pop > 0.05 {
            ui.text_centered(
                &t("vs").to_uppercase(),
                c.x,
                c.y - vs_size * 0.55,
                vs_size,
                GOLD,
                true,
            );
        }
        // Under them: the rules, and whether it counts for the rating.
        let kind = if self.rated { t("Rated game") } else { t("Casual game") };
        let line = format!("{} · {}", crate::i18n::rules_name(self.rules), kind);
        let ly = y + h + 22.0;
        let lw = ui.text_size(&line, 16.0, true).x + 36.0;
        rrect(
            Rect::new((sw - lw) / 2.0, ly - 6.0, lw, 30.0),
            15.0,
            Color::new(0.0, 0.0, 0.0, 0.55),
        );
        ui.text_centered(&line, sw / 2.0, ly, 16.0, if self.rated { GOLD } else { CREAM }, true);
        let hint = t("Click to skip");
        ui.text_centered(hint, sw / 2.0, ly + 38.0, 12.0, MUTED, false);
        crate::paint::set_opacity(saved);
        // Nothing under it takes a click while it shows.
        ui.block(Rect::new(0.0, 0.0, sw, sh));
        true
    }
}

/// One player's card: picture, name and flag, rating and league, and in a
/// rated game what they stand to win and lose.
pub fn card(ui: &mut Ui, intro: &Intro, seat: usize, r: Rect) {
    let p = &intro.players[seat];
    let tier = p.tier_in(intro.rules);
    let accent = tier_color(tier);
    rrect(r, 14.0, Color::new(0.07, 0.045, 0.03, 0.96));
    rrect_lines(r, 14.0, 2.0, Color::new(accent.r, accent.g, accent.b, 0.85));
    // Picture.
    let c = vec2(r.x + r.w / 2.0, r.y + 70.0);
    draw_circle(c.x, c.y, 50.0, Color::new(accent.r, accent.g, accent.b, 0.25));
    crate::avatars::draw(&p.avatar, c, 44.0);
    // Name and flag, centred.
    let name_size = 22.0;
    let max = r.w - 28.0;
    let w = ui.text_size(&p.username, name_size, true).x.min(max) + 30.0;
    ui.player_name(p, r.x + (r.w - w.min(max)) / 2.0, r.y + 130.0, name_size, max);
    // Rating and league.
    let rating = p.rating_in(intro.rules);
    let line =
        if p.guest { t("Guest").to_string() } else { format!("{rating} · {}", t(tier.name())) };
    ui.text_centered(&line, c.x, r.y + 166.0, 17.0, accent, true);
    let who = if seat == intro.seat && !intro.spectating { t("You") } else { "" };
    if !who.is_empty() {
        ui.text_centered(who, c.x, r.y + 12.0, 12.0, MUTED, true);
    }
    // At stake.
    if let Some(stakes) = intro.stakes {
        let [win, loss] = stakes[seat];
        let y = r.y + r.h - 62.0;
        let half = (r.w - 36.0) / 2.0;
        let good = Color::new(0.45, 0.88, 0.5, 1.0);
        let bad = Color::new(1.0, 0.5, 0.42, 1.0);
        for (k, (label, value, color)) in
            [(t("Win"), format!("+{win}"), good), (t("Loss"), format!("{loss}"), bad)]
                .into_iter()
                .enumerate()
        {
            let b = Rect::new(r.x + 12.0 + k as f32 * (half + 12.0), y, half, 48.0);
            rrect(b, 9.0, Color::new(1.0, 1.0, 1.0, 0.05));
            ui.text_centered(label, b.x + b.w / 2.0, b.y + 5.0, 12.0, MUTED, true);
            ui.text_centered(&value, b.x + b.w / 2.0, b.y + 20.0, 20.0, color, true);
        }
        let hint = tw("{n} rating points at stake", &[("n", &win.max(-loss))]);
        ui.text_centered(&hint, c.x, y - 24.0, 12.0, CREAM, false);
    } else if !intro.rated {
        ui.text_centered(t("No rating at stake"), c.x, r.y + r.h - 40.0, 13.0, MUTED, false);
    }
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
