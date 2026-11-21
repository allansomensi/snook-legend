//! Choosing the interface language: a field showing the one in use, and a
//! list of them all, each in its own name.

use std::cell::RefCell;

use macroquad::prelude::*;

use crate::i18n::{Lang, t};
use crate::icons::Icon;
use crate::ranking::{PAD, text_y};
use crate::ui::{BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};

#[derive(Default)]
struct Picker {
    open: bool,
    first: usize,
    /// The row Enter picks (moved with the arrow keys).
    sel: usize,
    /// The arrow keys have moved the highlight.
    keyed: bool,
    /// A click has started since the list opened (the click that opened it
    /// ends over the list and must not pick anything).
    armed: bool,
    /// Opened this frame.
    just_opened: bool,
    /// The list was just opened: bring the language in use into view.
    fresh: bool,
}

thread_local! {
    static PICKER: RefCell<Picker> = RefCell::new(Picker::default());
}

const ROW_H: f32 = 34.0;

/// A field that shows `lang`; true when clicked.
pub fn field(ui: &mut Ui, r: Rect, lang: Lang) -> bool {
    let clicked = ui.button(r, "", true);
    let mid = r.y + r.h / 2.0;
    crate::icons::draw(Icon::Globe, vec2(r.x + 18.0, mid), 16.0, MUTED);
    let (label, size) = ui.fitted(lang.name(), 15.0, r.w - 34.0 - 28.0, false);
    ui.text(&label, r.x + 34.0, text_y(mid, size), size, CREAM, false);
    let c = vec2(r.x + r.w - 16.0, mid);
    crate::paint::draw_line(c.x - 4.5, c.y - 2.0, c.x, c.y + 2.5, 1.8, MUTED);
    crate::paint::draw_line(c.x, c.y + 2.5, c.x + 4.5, c.y - 2.0, 1.8, MUTED);
    clicked
}

pub fn open() {
    PICKER.with(|p| {
        *p.borrow_mut() =
            Picker { open: true, just_opened: true, fresh: true, ..Picker::default() };
    });
}

pub fn is_open() -> bool {
    PICKER.with(|p| p.borrow().open)
}

/// Draws the list when open (`current` ticked); returns the language chosen.
pub fn draw_picker(ui: &mut Ui, current: Lang) -> Option<Lang> {
    if !is_open() {
        return None;
    }
    let sw = crate::ui::vw();
    let sh = crate::ui::vh();
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.55));
    let w = 380.0f32.min(sw - 20.0);
    // Header 24 + 28 + 16, list, 24.
    let chrome = 24.0 + 28.0 + 16.0 + 24.0;
    let rows = (((sh - 40.0).min(640.0) - chrome - 8.0) / ROW_H).floor().max(4.0);
    let rows = rows.min(Lang::ALL.len() as f32);
    let h = chrome + rows * ROW_H + 8.0;
    let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
    ui.panel(r);
    ui.block(Rect::new(0.0, 0.0, sw, sh));
    let x = r.x + PAD;
    let head_mid = r.y + 24.0 + 14.0;
    crate::icons::draw(Icon::Globe, vec2(x + 11.0, head_mid), 22.0, GOLD);
    ui.text(t("Language"), x + 32.0, text_y(head_mid, 22.0), 22.0, GOLD, true);
    let close = ui.icon_button_ex(
        Rect::new(r.x + r.w - PAD - 32.0, head_mid - 16.0, 32.0, 32.0),
        Icon::Close,
        t("Close"),
        true,
        true,
    );
    let mut chosen = None;
    PICKER.with(|p| {
        let mut p = p.borrow_mut();
        if is_mouse_button_pressed(MouseButton::Left) && !p.just_opened {
            p.armed = true;
        }
        p.just_opened = false;
        let armed = p.armed;
        let frame = Rect::new(x, r.y + 24.0 + 28.0 + 16.0, r.w - 2.0 * PAD, rows * ROW_H + 8.0);
        let area = Rect::new(frame.x + 4.0, frame.y + 4.0, frame.w - 8.0, frame.h - 8.0);
        let visible = (area.h / ROW_H).floor() as usize;
        let count = Lang::ALL.len();
        if p.fresh {
            p.fresh = false;
            p.sel = Lang::ALL.iter().position(|l| *l == current).unwrap_or(0);
            p.first = p.sel.saturating_sub(visible / 2);
        }
        let down = is_key_pressed(KeyCode::Down);
        let up = is_key_pressed(KeyCode::Up);
        if down {
            p.sel = (p.sel + 1).min(count - 1);
        }
        if up {
            p.sel = p.sel.saturating_sub(1);
        }
        if down || up {
            p.keyed = true;
            if p.sel < p.first {
                p.first = p.sel;
            } else if p.sel >= p.first + visible {
                p.first = p.sel + 1 - visible;
            }
        }
        p.first = ui.scroll_list(area, p.first, count, visible);
        let scrolls = count > visible;
        for (k, lang) in Lang::ALL.iter().enumerate().skip(p.first).take(visible) {
            let row_w = area.w - if scrolls { 12.0 } else { 0.0 };
            let row = Rect::new(area.x, area.y + (k - p.first) as f32 * ROW_H, row_w, ROW_H - 2.0);
            let hover = ui.hovering(row);
            let selected = *lang == current;
            if selected {
                rrect(row, 6.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.14));
            }
            if hover || (k == p.sel && p.keyed) {
                rrect(row, 6.0, Color::new(1.0, 1.0, 1.0, 0.07));
            }
            let mid = row.y + row.h / 2.0;
            let color = if selected { GOLD } else { CREAM };
            ui.text(lang.name(), row.x + 12.0, text_y(mid, 15.0), 15.0, color, selected);
            if selected {
                crate::icons::draw(Icon::Check, vec2(row.x + row.w - 16.0, mid), 14.0, GOLD);
            }
            if armed && hover && ui.clicked(row) {
                chosen = Some(*lang);
            }
        }
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
            chosen = Some(Lang::ALL[p.sel]);
        }
        rrect_lines(frame, 6.0, 1.0, BORDER);
        if chosen.is_some()
            || (close && armed)
            || is_key_pressed(KeyCode::Escape)
            || (armed && ui.dismissed(r))
        {
            p.open = false;
        }
    });
    chosen
}
