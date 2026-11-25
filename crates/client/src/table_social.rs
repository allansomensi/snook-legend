//! What the table shows of the people at it, online: whether the game is
//! rated (always in view), the opponent who lost their connection (a big
//! countdown in the middle of the table), and muting the opponent's chat or
//! reactions from the game (remembered for the next games against them).

use macroquad::prelude::*;

use crate::game::{Match, Phase};
use crate::i18n::{t, tw};
use crate::paint::draw_rectangle;
use crate::ui::{ALERT, BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};

/// The rated or casual badge, at `x`, `y` (the left of the area under the
/// top bar). Its width.
pub fn rated_badge(ui: &mut Ui, m: &Match, x: f32, y: f32) -> f32 {
    let Some(o) = m.online.as_ref().filter(|o| o.corr.is_none()) else { return 0.0 };
    let (label, color) = if o.rated { (t("Rated"), GOLD) } else { (t("Casual"), MUTED) };
    let w = ui.text_size(label, 12.0, true).x + 26.0;
    let r = Rect::new(x, y, w, 22.0);
    rrect(r, 11.0, Color::new(0.0, 0.0, 0.0, 0.55));
    rrect_lines(r, 11.0, 1.0, Color::new(color.r, color.g, color.b, 0.7));
    crate::paint::draw_circle(r.x + 11.0, r.y + 11.0, 3.5, color);
    ui.text(label, r.x + 19.0, r.y + 4.0, 12.0, color, true);
    if ui.hovering(r) {
        let tip = match o.stakes.filter(|_| o.rated && !o.spectator) {
            Some(stakes) => {
                let [win, loss] = stakes[o.seat.min(1)];
                tw(
                    "This game counts for the rating: +{win} if you win, {loss} if you lose.",
                    &[("win", &win), ("loss", &loss)],
                )
            }
            None if o.rated => t("This game counts for the rating.").to_string(),
            None => t("A casual game: the rating doesn't change.").to_string(),
        };
        ui.tooltip(&tip, r);
    }
    w
}

/// The opponent lost their connection during the game: how long they have
/// to come back, big, in the middle of the table (the game is theirs to
/// lose after that).
pub fn reconnection(ui: &Ui, m: &Match) {
    let Some(o) = m.online.as_ref().filter(|o| !o.spectator && !o.opponent_connected) else {
        return;
    };
    if matches!(m.phase, Phase::GameOver) || m.game.game_over().is_some() {
        return;
    }
    let Some(left) = o.away_left() else { return };
    let sw = crate::ui::vw();
    let sh = crate::ui::vh();
    let name = &m.names[1 - o.seat.min(1)];
    let title = tw("{name} lost the connection", &[("name", &name)]);
    let clock = format!("{}:{:02}", left / 60, left % 60);
    let w = (ui.text_size(&title, 18.0, true).x + 60.0).clamp(300.0, sw - 24.0);
    let r = Rect::new((sw - w) / 2.0, sh / 2.0 - 80.0, w, 150.0);
    rrect(r, 16.0, Color::new(0.04, 0.03, 0.02, 0.86));
    rrect_lines(r, 16.0, 1.5, Color::new(ALERT.r, ALERT.g, ALERT.b, 0.6));
    let (title, size) = ui.fitted(&title, 18.0, w - 30.0, true);
    ui.text_centered(&title, sw / 2.0, r.y + 16.0, size, CREAM, true);
    // The seconds, large; the last ten in red, beating.
    let urgent = left <= 10;
    let pulse = if urgent { 1.0 + 0.06 * (get_time() as f32 * 6.0).sin() } else { 1.0 };
    let color = if urgent { ALERT } else { GOLD };
    ui.text_centered(&clock, sw / 2.0, r.y + 46.0, 48.0 * pulse, color, true);
    let note = t("If they don't come back in time, you win by abandonment.");
    let (note, size) = ui.fitted(note, 12.0, w - 30.0, false);
    ui.text_centered(&note, sw / 2.0, r.y + 116.0, size, MUTED, false);
    // A thin bar running out under it.
    let full = o.opponent_away.map_or(1, |(_, g)| g.max(1)) as f32;
    let bar = Rect::new(r.x + 20.0, r.y + r.h - 10.0, r.w - 40.0, 3.0);
    draw_rectangle(bar.x, bar.y, bar.w, bar.h, Color::new(1.0, 1.0, 1.0, 0.1));
    draw_rectangle(bar.x, bar.y, bar.w * (left as f32 / full).clamp(0.0, 1.0), bar.h, color);
}

/// The button to mute the opponent (in the chat's header) and the list it
/// opens: their chat, their reactions. `muted`: [chat, reactions] as they
/// are now. The new choice, when one was made.
pub fn mute_menu(
    ui: &mut Ui,
    b: Rect,
    name: &str,
    muted: [bool; 2],
    open: &mut bool,
) -> Option<[bool; 2]> {
    let any = muted.iter().any(|m| *m);
    let tip = tw("Mute {name}", &[("name", &name)]);
    if ui.icon_button_ex(b, crate::icons::Icon::Speaker, &tip, true, !(any || *open)) {
        *open = !*open;
        return None;
    }
    if !*open {
        return None;
    }
    let lines = [
        tw("Mute {name}'s chat", &[("name", &name)]),
        tw("Hide {name}'s reactions", &[("name", &name)]),
    ];
    let note = t("Remembered for your next games against them.");
    let w = lines
        .iter()
        .map(|l| ui.text_size(l, 15.0, false).x + 60.0)
        .fold(ui.text_size(note, 12.0, false).x + 24.0, f32::max)
        .min(crate::ui::vw() - 16.0);
    let h = 2.0 * 30.0 + 52.0;
    let r = Rect::new((b.x + b.w - w).max(8.0), (b.y - h - 6.0).max(8.0), w, h);
    ui.panel(r);
    rrect_lines(r, 10.0, 1.0, BORDER);
    let mut out = None;
    for (k, line) in lines.iter().enumerate() {
        let mut on = muted[k];
        if ui.checkbox(r.x + 12.0, r.y + 12.0 + k as f32 * 30.0, line, &mut on) {
            let mut next = muted;
            next[k] = on;
            out = Some(next);
        }
    }
    let (note, size) = ui.fitted(note, 12.0, r.w - 24.0, false);
    ui.text(&note, r.x + 12.0, r.y + r.h - 24.0, size, MUTED, false);
    if out.is_none() && ui.pressed_outside(&[r, b]) {
        *open = false;
    }
    out
}
