//! Choosing a country: a field showing the flag, and a searchable list.

use std::cell::RefCell;

use macroquad::prelude::*;
use snook_legend_proto::COUNTRIES;

use crate::i18n::t;
use crate::icons::Icon;
use crate::ranking::{PAD, empty_state, text_y};
use crate::ui::{BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};

#[derive(Default)]
struct Picker {
    open: bool,
    query: String,
    first: crate::ui::Scroll,
    /// The row Enter picks (moved with the arrow keys).
    sel: usize,
    /// The query `sel` belongs to (a new search starts at the top).
    sel_query: String,
    /// The country chosen now (shown ticked, and in view when the list opens).
    current: String,
    /// The list was just opened: bring `current` into view.
    fresh: bool,
    /// The arrow keys have moved the highlight.
    keyed: bool,
    /// A click has started since the list opened. The click that opened it
    /// ends (is released) over the list, and must not pick anything.
    armed: bool,
    /// Opened this frame: a press seen now belongs to the click that opened
    /// it (a quick tap presses and releases within one frame).
    just_opened: bool,
}

thread_local! {
    static PICKER: RefCell<Picker> = RefCell::new(Picker::default());
}

const ROW_H: f32 = 32.0;

/// Lower case without accents, so "arabia" finds "Arábia" and the list
/// sorts "Á" with the "A"s.
fn fold(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

/// A field that shows the chosen country; true when clicked.
pub fn field(ui: &mut Ui, r: Rect, code: &str, label: &str) -> bool {
    PICKER.with(|p| {
        let mut p = p.borrow_mut();
        if !p.open && p.current != code {
            p.current = code.to_string();
        }
    });
    let clicked = ui.button(r, "", true);
    let mid = r.y + r.h / 2.0;
    let x =
        if crate::flags::draw(code, r.x + 10.0, mid - 8.0, 16.0) { r.x + 36.0 } else { r.x + 10.0 };
    ui.text(label, x, text_y(mid, 15.0), 15.0, CREAM, false);
    chevron_down(vec2(r.x + r.w - 16.0, mid), MUTED);
    clicked
}

/// A small "▾", drawn (the same on every system).
fn chevron_down(c: Vec2, color: Color) {
    crate::paint::draw_line(c.x - 4.5, c.y - 2.0, c.x, c.y + 2.5, 1.8, color);
    crate::paint::draw_line(c.x, c.y + 2.5, c.x + 4.5, c.y - 2.0, 1.8, color);
}

pub fn open() {
    PICKER.with(|p| {
        let mut p = p.borrow_mut();
        p.open = true;
        p.query.clear();
        p.first.reset();
        p.sel = 0;
        p.sel_query.clear();
        p.fresh = true;
        p.keyed = false;
        p.armed = false;
        p.just_opened = true;
    });
}

/// Opens the list with `current` ticked and in view.
pub fn open_at(current: &str) {
    PICKER.with(|p| p.borrow_mut().current = current.to_string());
    open();
}

pub fn is_open() -> bool {
    PICKER.with(|p| p.borrow().open)
}

/// Draws the list when open; returns the code chosen ("" for none).
pub fn draw_picker(ui: &mut Ui) -> Option<String> {
    if !is_open() {
        return None;
    }
    let sw = crate::ui::vw();
    let sh = crate::ui::vh();
    let _modal = ui.modal_scope(crate::ui::motion::id("countries"), 0.55);
    let w = 400.0f32.min(sw - 20.0);
    // Header 24 + 28 + 16, search 36 + 12, list, 16 + button 36 + 24.
    let chrome = 24.0 + 28.0 + 16.0 + 36.0 + 12.0 + 16.0 + 36.0 + 24.0;
    let rows = (((sh - 40.0).min(600.0) - chrome - 8.0) / ROW_H).floor().max(4.0);
    let h = chrome + rows * ROW_H + 8.0;
    let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
    ui.panel(r);
    ui.block(Rect::new(0.0, 0.0, sw, sh));
    let x = r.x + PAD;
    let head_mid = r.y + 24.0 + 14.0;
    crate::icons::draw(Icon::Globe, vec2(x + 11.0, head_mid), 22.0, GOLD);
    ui.text(t("Country"), x + 32.0, text_y(head_mid, 22.0), 22.0, GOLD, true);
    let mut chosen = None;
    let close = ui.icon_button_ex(
        Rect::new(r.x + r.w - PAD - 32.0, head_mid - 16.0, 32.0, 32.0),
        Icon::Close,
        t("Close"),
        true,
        true,
    );
    PICKER.with(|p| {
        let mut p = p.borrow_mut();
        if is_mouse_button_pressed(MouseButton::Left) && !p.just_opened {
            p.armed = true;
        }
        p.just_opened = false;
        let armed = p.armed;
        let input = Rect::new(x, r.y + 24.0 + 28.0 + 16.0, r.w - 2.0 * PAD, 36.0);
        if ui.focus.is_none() {
            ui.focus = Some(60);
        }
        ui.text_input(60, input, &mut p.query, 30);
        if p.query.is_empty() {
            ui.text(t("Search…"), input.x + 10.0, text_y(input.y + 18.0, 14.0), 14.0, MUTED, false);
        }
        let q = fold(p.query.trim());
        let mut list: Vec<(&str, &str, String)> = COUNTRIES
            .iter()
            .map(|(c, _, _)| {
                let name = crate::flags::name(c);
                (*c, name, fold(name))
            })
            .filter(|(c, _, folded)| q.is_empty() || folded.contains(&q) || c.to_lowercase() == q)
            .collect();
        list.sort_by(|a, b| a.2.cmp(&b.2));
        if p.sel_query != p.query {
            p.sel_query = p.query.clone();
            p.sel = 0;
            p.first.reset();
        }
        // The list sits in a box as wide as the search field above it.
        let frame = Rect::new(x, input.y + input.h + 12.0, input.w, rows * ROW_H + 8.0);
        let area = Rect::new(frame.x + 4.0, frame.y + 4.0, frame.w - 8.0, frame.h - 8.0);
        let visible = (area.h / ROW_H).floor() as usize;
        if p.fresh {
            p.fresh = false;
            if let Some(i) = list.iter().position(|(c, _, _)| *c == p.current) {
                p.sel = i;
                p.first.jump_to(i.saturating_sub(visible / 2) as f32 * ROW_H);
            }
        }
        // The arrow keys move the highlight (and the list with it).
        if !list.is_empty() {
            if is_key_pressed(KeyCode::Down) {
                p.sel = (p.sel + 1).min(list.len() - 1);
            }
            if is_key_pressed(KeyCode::Up) {
                p.sel = p.sel.saturating_sub(1);
            }
            p.sel = p.sel.min(list.len() - 1);
            if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::Up) {
                p.keyed = true;
                let sel = p.sel;
                p.first.reveal_row(sel, ROW_H);
            }
        }
        // (The arrows move the highlight here, not the list.)
        p.first.no_keys = true;
        let view = ui.list_begin(area, &mut p.first, list.len(), ROW_H);
        let scrolls = view.scrolls();
        if list.is_empty() {
            empty_state(ui, area, Icon::Globe, t("No country found."));
        }
        for k in view.rows() {
            let (code, name, _) = &list[k];
            let row_w = area.w - if scrolls { 12.0 } else { 0.0 };
            let row = Rect::new(area.x, view.y(k), row_w, ROW_H - 2.0);
            let hover = ui.hovering(row);
            let current = *code == p.current;
            if current {
                rrect(row, 6.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.14));
            }
            // The row Enter would pick, once there is a search or a key press.
            let lit = ui.hover_anim(row, hover || (k == p.sel && (!q.is_empty() || p.keyed)));
            if lit > 0.01 {
                rrect(row, 6.0, Color::new(1.0, 1.0, 1.0, 0.07 * lit));
            }
            let mid = row.y + row.h / 2.0;
            crate::flags::draw(code, row.x + 10.0, mid - 7.5, 15.0);
            let color = if current { GOLD } else { CREAM };
            ui.text(name, row.x + 38.0, text_y(mid, 14.0), 14.0, color, current);
            if current {
                crate::icons::draw(Icon::Check, vec2(row.x + row.w - 16.0, mid), 14.0, GOLD);
            }
            if armed && ui.activated(row, hover) {
                chosen = Some(code.to_string());
            }
        }
        ui.scroll_end(view, &mut p.first);
        if ui.enter()
            && let Some((code, _, _)) = list.get(p.sel)
        {
            chosen = Some(code.to_string());
        }
        rrect_lines(frame, 6.0, 1.0, BORDER);
        let label = t("Don't show a flag");
        let bw = ui.text_size(label, 15.0, true).x + 32.0;
        let none = Rect::new(x, r.y + r.h - PAD - 36.0, bw, 36.0);
        if ui.button(none, label, !p.current.is_empty()) && armed {
            chosen = Some(String::new());
        }
        // Closed also by a press on the dimmed screen around it (not by
        // the click that opened it).
        if chosen.is_some()
            || (close && armed)
            || crate::ui::esc_pressed()
            || (armed && ui.dismissed(r))
        {
            p.open = false;
            ui.focus = None;
        }
    });
    chosen
}
