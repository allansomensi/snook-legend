//! Profile pictures: one of the predefined portraits
//! ([`snook_legend_proto::AVATARS`]), chosen in the profile (or in the
//! settings, offline) and shown in the score panel, the profile and the
//! lounge.

use std::cell::RefCell;

use macroquad::prelude::*;

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_circle_lines, draw_rectangle};

use crate::i18n::t;
use crate::icons::Icon;
use crate::ui::{BORDER, GOLD, Ui, rrect, rrect_lines};

/// Draws picture `code` in a disc of radius `r` at `c` (a plain silhouette
/// for none, or for a code this version doesn't know).
pub fn draw(code: &str, c: Vec2, r: f32) {
    if !crate::opponents::draw_avatar(code, c, r) {
        crate::scoreboard::silhouette(c, r);
    }
}

#[derive(Default)]
struct State {
    /// This player's picture (from the settings, or from their account).
    mine: String,
    open: bool,
    /// The click that opened the picker doesn't choose in it.
    armed: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// This player's picture.
pub fn mine() -> String {
    STATE.with(|s| s.borrow().mine.clone())
}

pub fn set_mine(code: &str) {
    if snook_legend_proto::valid_avatar(code) {
        STATE.with(|s| s.borrow_mut().mine = code.to_string());
    }
}

/// Opens the picker.
pub fn open() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.open = true;
        s.armed = false;
    });
}

pub fn is_open() -> bool {
    STATE.with(|s| s.borrow().open)
}

/// Draws the picker when open; returns the picture chosen ("" for none),
/// which is then this player's.
pub fn draw_picker(ui: &mut Ui) -> Option<String> {
    if !is_open() {
        return None;
    }
    let sw = crate::ui::vw();
    let sh = crate::ui::vh();
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.55));
    const PAD: f32 = 24.0;
    const GAP: f32 = 10.0;
    let count = snook_legend_proto::AVATARS.len() + 1;
    // As many columns as fit (11 at most: three full rows), cells as large
    // as fit.
    let cols = (((sw - 40.0 - 2.0 * PAD + GAP) / (60.0 + GAP)).floor() as usize).clamp(4, 11);
    let rows = count.div_ceil(cols);
    let head = 64.0;
    let room_h = (sh - 40.0 - head - 2.0 * PAD).max(160.0);
    let cell = ((room_h + GAP) / rows as f32 - GAP).clamp(38.0, 72.0);
    let w = 2.0 * PAD + cols as f32 * (cell + GAP) - GAP;
    let h = head + rows as f32 * (cell + GAP) - GAP + PAD;
    let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
    ui.panel(r);
    ui.block(Rect::new(0.0, 0.0, sw, sh));
    let head_mid = r.y + PAD + 14.0;
    crate::icons::draw(Icon::Person, vec2(r.x + PAD + 11.0, head_mid), 22.0, GOLD);
    ui.text(t("Profile picture"), r.x + PAD + 32.0, head_mid - 12.0, 22.0, GOLD, true);
    let close = ui.icon_button_ex(
        Rect::new(r.x + r.w - PAD - 32.0, head_mid - 16.0, 32.0, 32.0),
        Icon::Close,
        t("Close"),
        true,
        true,
    );
    let (mine, armed) = STATE.with(|s| {
        let mut s = s.borrow_mut();
        if is_mouse_button_pressed(MouseButton::Left) {
            s.armed = true;
        }
        (s.mine.clone(), s.armed)
    });
    let mut chosen = None;
    let codes = std::iter::once("").chain(snook_legend_proto::AVATARS);
    for (k, code) in codes.enumerate() {
        let (col, row) = (k % cols, k / cols);
        let cr = Rect::new(
            r.x + PAD + col as f32 * (cell + GAP),
            r.y + head + row as f32 * (cell + GAP),
            cell,
            cell,
        );
        let hover = ui.hovering(cr);
        let selected = code == mine;
        if hover || selected {
            rrect(cr, 10.0, Color::new(1.0, 1.0, 1.0, if hover { 0.08 } else { 0.04 }));
        }
        draw(code, vec2(cr.x + cell / 2.0, cr.y + cell / 2.0), cell / 2.0 - 4.0);
        if selected {
            rrect_lines(cr, 10.0, 2.0, GOLD);
        } else if hover {
            rrect_lines(cr, 10.0, 1.0, BORDER);
        }
        if code.is_empty() && hover {
            ui.tooltip(t("No picture"), cr);
        }
        if hover && armed && ui.clicked(cr) {
            chosen = Some(code.to_string());
        }
    }
    if chosen.is_some() || close || is_key_pressed(KeyCode::Escape) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.open = false;
            if let Some(c) = &chosen {
                s.mine = c.clone();
            }
        });
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_pictures_are_kept() {
        set_mine("zuri");
        assert_eq!(mine(), "zuri");
        set_mine("no-such-picture");
        assert_eq!(mine(), "zuri");
        set_mine("");
        assert_eq!(mine(), "");
    }
}
