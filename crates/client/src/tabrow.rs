//! Rows of tabs and switches whose labels all have one size: each tab is as
//! wide as its label needs, the room left over shared out evenly, and when
//! the row is too narrow every label gets smaller together (rather than
//! the longest one alone shrinking, or being cut short).

use macroquad::prelude::*;

use crate::ui::{
    BORDER, CREAM, GOLD, MUTED, Ui, mix, motion, rrect, rrect_gradient, rrect_lines, shade,
};

/// The smallest a label gets before it is shortened.
const MIN_SIZE: f32 = 11.0;

/// The widths of the cells of a row `width` wide, `gap` apart, for labels
/// drawn at `size` (bold) with `pad` on each side and `extra` more room per
/// cell (a count beside the label); and the size the labels are drawn at.
pub fn layout(
    ui: &Ui,
    labels: &[&str],
    extra: &[f32],
    width: f32,
    gap: f32,
    size: f32,
    pad: f32,
) -> (Vec<f32>, f32) {
    let n = labels.len().max(1);
    let extra_at = |k: usize| extra.get(k).copied().unwrap_or(0.0);
    let text: Vec<f32> = labels.iter().map(|l| ui.text_size(l, size, true).x).collect();
    let fixed: f32 =
        (0..labels.len()).map(|k| 2.0 * pad + extra_at(k)).sum::<f32>() + gap * (n - 1) as f32;
    let natural: f32 = text.iter().sum();
    let room = (width - fixed).max(0.0);
    // Text widths follow the size: one factor for the whole row.
    let scale = if natural > room && natural > 0.0 { room / natural } else { 1.0 };
    let fitted = (size * scale).clamp(MIN_SIZE.min(size), size);
    let text: Vec<f32> = text.iter().map(|w| w * fitted / size).collect();
    let total: f32 = text.iter().sum();
    let widths = if total <= room {
        // Room to spare: shared out evenly.
        let spare = (room - total) / n as f32;
        text.iter().enumerate().map(|(i, w)| w + 2.0 * pad + extra_at(i) + spare).collect()
    } else {
        // Too narrow even at the smallest size: shares as the labels need,
        // the labels shortened to fit.
        text.iter()
            .enumerate()
            .map(|(i, w)| w * room / total.max(1.0) + 2.0 * pad + extra_at(i))
            .collect()
    };
    (widths, fitted)
}

/// `text` shortened with an ellipsis to fit `width` in bold at `size`.
fn shorten(ui: &Ui, text: &str, width: f32, size: f32) -> String {
    if ui.text_size(text, size, true).x <= width {
        return text.to_string();
    }
    let candidate = ui.start_that_fits(text, size, width, true).trim_end().to_string() + "…";
    if ui.text_size(&candidate, size, true).x <= width { candidate } else { String::new() }
}

/// A row of tabs in `r` (see [`crate::ui::Ui::tab_with_count`] for one):
/// the one clicked, if any. `counts` puts a small number at a tab's right.
pub fn tabs(
    ui: &mut Ui,
    r: Rect,
    labels: &[&str],
    counts: &[&str],
    selected: usize,
    gap: f32,
) -> (Option<usize>, Vec<Rect>) {
    let extra: Vec<f32> = labels
        .iter()
        .enumerate()
        .map(|(k, _)| match counts.get(k) {
            Some(c) if !c.is_empty() => ui.text_size(c, 11.0, false).x + 8.0,
            _ => 0.0,
        })
        .collect();
    let (widths, size) = layout(ui, labels, &extra, r.w, gap, 15.0, 10.0);
    let mut clicked = None;
    let mut rects = Vec::new();
    let mut x = r.x;
    for (k, label) in labels.iter().enumerate() {
        let cell = Rect::new(x, r.y, widths[k], r.h);
        x += widths[k] + gap;
        let count = counts.get(k).copied().unwrap_or("");
        if tab(ui, cell, label, count, k == selected, size, extra[k]) {
            clicked = Some(k);
        }
        rects.push(cell);
    }
    (clicked, rects)
}

/// One tab, as [`crate::ui::Ui::tab_with_count`] draws it, its label at `size`.
fn tab(
    ui: &mut Ui,
    r: Rect,
    label: &str,
    count: &str,
    selected: bool,
    size: f32,
    reserve: f32,
) -> bool {
    ui.block(r);
    let hover = ui.hovering(r);
    let a = ui.hover_anim(r, hover && !selected);
    let sel = ui.anim(motion::rect_key(motion::id("tab-on"), r), selected as u8 as f32, 22.0);
    let idle = mix(Color::new(0.12, 0.08, 0.05, 1.0), Color::new(0.25, 0.16, 0.09, 1.0), a);
    let bg = mix(idle, Color::new(0.38, 0.25, 0.12, 1.0), sel);
    if sel > 0.01 || a > 0.01 {
        rrect_gradient(r, 6.0, shade(bg, 0.06), shade(bg, -0.15));
    } else {
        rrect(r, 6.0, bg);
    }
    // The underline glides to the tab chosen.
    ui.tab_underline(r, selected, 8.0, 1.5);
    if !count.is_empty() {
        let cw = ui.text_size(count, 11.0, false).x;
        ui.text(count, r.x + r.w - cw - 10.0, r.y + (r.h - 11.0) / 2.0, 11.0, MUTED, false);
    }
    // Shortened only when even the smallest size doesn't fit.
    let label = shorten(ui, label, r.w - 16.0 - reserve, size);
    let width = ui.text_size(&label, size, true).x;
    let centre = (r.x + r.w / 2.0).min(r.x + r.w - reserve - 6.0 - width / 2.0);
    let color = mix(CREAM, GOLD, sel);
    ui.text_centered(&label, centre, r.y + (r.h - size) / 2.0, size, color, true);
    let clicked = !selected && hover && ui.released_in(r);
    if clicked {
        ui.note_click();
    }
    clicked
}

/// A switch of several options (as [`crate::ui::Ui::segmented`]), each as
/// wide as its label needs, all labels at one size.
pub fn segmented(ui: &mut Ui, r: Rect, options: &[&str], selected: usize) -> Option<usize> {
    ui.block(r);
    let size = (r.h * 0.42).clamp(12.0, 16.0);
    let (widths, size) = layout(ui, options, &[], r.w, 0.0, size, 6.0);
    rrect(r, 7.0, Color::new(0.06, 0.04, 0.03, 1.0));
    let mut cells = Vec::with_capacity(options.len());
    let mut x = r.x;
    for w in &widths {
        cells.push(Rect::new(x, r.y, *w, r.h));
        x += w;
    }
    let inset = |c: Rect| Rect::new(c.x + 2.0, c.y + 2.0, c.w - 4.0, c.h - 4.0);
    // The chosen one's plate glides to the option clicked.
    if let Some(cell) = cells.get(selected) {
        let key = motion::rect_key(motion::id("segmented"), r);
        let plate = ui.glide_rect(key, inset(*cell), 20.0);
        let bg = Color::new(0.38, 0.25, 0.12, 1.0);
        rrect_gradient(plate, 6.0, shade(bg, 0.08), shade(bg, -0.15));
    }
    let mut clicked = None;
    for (i, label) in options.iter().enumerate() {
        let cell = cells[i];
        let hover = ui.hovering(cell);
        let a = ui.hover_anim(cell, hover && i != selected);
        if a > 0.01 {
            rrect(inset(cell), 6.0, Color::new(0.22, 0.15, 0.09, a));
        }
        let sel = ui.anim(
            motion::rect_key(motion::id("seg-on"), cell),
            (i == selected) as u8 as f32,
            20.0,
        );
        let label = shorten(ui, label, cell.w - 8.0, size);
        let color = mix(CREAM, GOLD, sel);
        ui.text_centered(
            &label,
            cell.x + cell.w / 2.0,
            cell.y + (r.h - size) / 2.0,
            size,
            color,
            true,
        );
        if i != selected && hover && ui.released_in(cell) {
            clicked = Some(i);
            ui.note_click();
        }
    }
    rrect_lines(r, 7.0, 1.2, BORDER);
    clicked
}
