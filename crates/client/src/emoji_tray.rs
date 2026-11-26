//! The emoji tray of the chats (the table's and the lounge's): a 😊 button
//! by the field, and a small tray above the field with forty emoji and the
//! ones used lately (shared by every chat). An emoji goes in where the
//! caret was, the field keeps the keyboard, and the tray stays open for
//! more; Esc or a click elsewhere closes it.

use std::cell::RefCell;

use macroquad::prelude::*;

use crate::paint::draw_line;
use crate::ui::{BORDER, CREAM, GOLD, Ui};

/// The button's side.
pub const BUTTON: f32 = 28.0;

/// The tray's emoji (all in the bundled atlas), eight to a row: faces,
/// hands, then the game's own.
pub const EMOJI: [&str; 40] = [
    "😀", "😂", "🤣", "😅", "😊", "😉", "😎", "😍", //
    "🥳", "🤩", "😏", "😜", "🤔", "😮", "😱", "🤯", //
    "😬", "😢", "😭", "😡", "🙄", "😴", "👍", "👎", //
    "👏", "🙌", "🙏", "🤝", "👋", "💪", "✌️", "💯", //
    "🔥", "✨", "🎯", "🎱", "🏆", "🍀", "🎉", "❤️", //
];

const COLS: usize = 8;
const CELL: f32 = 28.0;
const PAD: f32 = 6.0;

thread_local! {
    /// The emoji used lately, the latest first (every chat's).
    static RECENT: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

/// A chat's tray: open or not, how far it has faded in, and where the
/// caret was in its field.
#[derive(Default)]
pub struct EmojiTray {
    pub open: bool,
    fade: f32,
    caret: Option<usize>,
}

impl EmojiTray {
    pub fn new(open: bool) -> EmojiTray {
        EmojiTray { open, ..EmojiTray::default() }
    }

    /// Call after the field is drawn, each frame: the caret is kept for
    /// when the tray takes the click.
    pub fn track(&mut self, ui: &Ui, field: u64) {
        if let Some(c) = ui.caret_in(field) {
            self.caret = Some(c);
        }
    }

    /// The field was sent or emptied: the tray closes, the caret is gone.
    pub fn reset(&mut self) {
        self.open = false;
        self.caret = None;
    }

    /// The button at `button` and, when open, the tray above `field_rect`
    /// (kept inside `bounds`). Returns whether the button was clicked (to
    /// close the chat's other lists) and the tray's rectangle.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        ui: &mut Ui,
        button: Rect,
        field_rect: Rect,
        bounds: Rect,
        field: u64,
        value: &mut String,
        max_len: usize,
    ) -> (bool, Option<Rect>) {
        ui.block(button);
        let hover = ui.hovering(button);
        let lit = hover || self.open;
        crate::ui::rrect(
            button,
            7.0,
            if lit {
                Color::new(0.38, 0.25, 0.12, 0.95)
            } else {
                Color::new(0.07, 0.045, 0.03, 0.85)
            },
        );
        crate::ui::rrect_lines(button, 7.0, 1.2, if lit { GOLD } else { BORDER });
        let face = if lit { 19.0 } else { 17.0 };
        let opacity = crate::paint::opacity();
        crate::paint::set_opacity(opacity * if lit { 1.0 } else { 0.8 });
        ui.text_centered(
            "😊",
            button.x + button.w / 2.0,
            button.y + (button.h - face) / 2.0,
            face,
            CREAM,
            false,
        );
        crate::paint::set_opacity(opacity);
        let clicked = hover && ui.released_in(button);
        if clicked {
            self.open = !self.open;
            if self.open {
                // The field keeps the keyboard, the caret where it was.
                ui.focus = Some(field);
            }
        }
        if !self.open {
            self.fade = 0.0;
            return (clicked, None);
        }
        let recent = RECENT.with(|r| r.borrow().clone());
        let rows = EMOJI.len().div_ceil(COLS);
        let recent_h = if recent.is_empty() { 0.0 } else { CELL + 7.0 };
        let w = COLS as f32 * CELL + 2.0 * PAD;
        let h = recent_h + rows as f32 * CELL + 2.0 * PAD;
        let x = crate::ui::clamp_soft(
            button.x + button.w - w,
            bounds.x + 6.0,
            (bounds.x + bounds.w - w - 6.0).min(crate::ui::vw() - w - 8.0),
        );
        let y = (field_rect.y - h - 4.0).max(8.0);
        let p = Rect::new(x, y, w, h);
        // Grows out of its button.
        let anchor = vec2(button.x + button.w / 2.0, button.y);
        let _pop = ui.popover_scope(crate::ui::motion::id("emoji-tray"), anchor);
        // Fades in, a small tray: the chat stays readable around it.
        self.fade = (self.fade + get_frame_time() / 0.12).min(1.0);
        crate::paint::set_opacity(opacity * self.fade);
        crate::ui::rrect(
            Rect::new(p.x + 1.0, p.y + 2.0, p.w, p.h),
            9.0,
            Color::new(0.0, 0.0, 0.0, 0.35),
        );
        crate::ui::rrect(p, 9.0, Color::new(0.06, 0.045, 0.035, 0.97));
        crate::ui::rrect_lines(p, 9.0, 1.0, Color::new(BORDER.r, BORDER.g, BORDER.b, 0.8));
        ui.block(p);
        let mut chosen = None;
        let mut cell = |ui: &mut Ui, e: &'static str, c: Rect| {
            let hover = ui.hovering(c);
            if hover {
                crate::ui::rrect(
                    Rect::new(c.x + 1.0, c.y + 1.0, c.w - 2.0, c.h - 2.0),
                    6.0,
                    Color::new(1.0, 1.0, 1.0, 0.09),
                );
            }
            let size = if hover { 20.0 } else { 17.0 };
            ui.text_centered(e, c.x + c.w / 2.0, c.y + (c.h - size) / 2.0, size, CREAM, false);
            if hover && ui.released_in(c) {
                chosen = Some(e);
            }
        };
        let mut top = p.y + PAD;
        if !recent.is_empty() {
            for (k, e) in recent.iter().enumerate() {
                cell(ui, e, Rect::new(p.x + PAD + k as f32 * CELL, top, CELL, CELL));
            }
            top += CELL + 3.0;
            draw_line(
                p.x + PAD + 4.0,
                top,
                p.x + p.w - PAD - 4.0,
                top,
                1.0,
                Color::new(BORDER.r, BORDER.g, BORDER.b, 0.6),
            );
            top += 4.0;
        }
        for (k, e) in EMOJI.iter().enumerate() {
            let c = Rect::new(
                p.x + PAD + (k % COLS) as f32 * CELL,
                top + (k / COLS) as f32 * CELL,
                CELL,
                CELL,
            );
            cell(ui, e, c);
        }
        if let Some(e) = chosen {
            let at = ui.insert_into(field, value, e, max_len, self.caret);
            self.caret = Some(at);
            RECENT.with(|r| {
                let mut r = r.borrow_mut();
                r.retain(|x| *x != e);
                r.insert(0, e);
                r.truncate(COLS);
            });
        } else if is_key_pressed(KeyCode::Escape)
            || (is_mouse_button_released(MouseButton::Left)
                && !ui.hovering(p)
                && !ui.hovering(button)
                && !clicked)
        {
            self.open = false;
        }
        crate::paint::set_opacity(opacity);
        (clicked, Some(p))
    }
}

#[cfg(test)]
mod tests {
    /// Every emoji of the tray is drawn from the atlas (none falls back to
    /// the font, which has no emoji).
    #[test]
    fn the_tray_emoji_are_all_in_the_atlas() {
        for e in super::EMOJI {
            let pieces = crate::emoji::pieces(e).unwrap_or_default();
            assert!(
                matches!(pieces.as_slice(), [crate::emoji::Piece::Emoji(_)]),
                "{e}: {pieces:?}"
            );
        }
    }
}
