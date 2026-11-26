//! How the two rules look on every screen: Brazil's flag for the Brazilian
//! rules and a blue globe for the official (WPA) rules, the switch that
//! picks the rules the player's games are played under, the pill a screen
//! shows them in, and the small tag of a game's rules in a list.

use macroquad::prelude::*;
use snook_legend_core::{PerRules, Rules};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_rectangle};

use crate::i18n::t;
use crate::icons::Icon;
use crate::ui::{CREAM, GOLD, MUTED, Ui, rrect, rrect_gradient, rrect_lines, shade};

/// The official rules' colour: the blue of the globe.
pub const OFFICIAL: Color = Color::new(0.45, 0.67, 1.0, 1.0);

/// The colour that marks `rules`: the game's gold for the Brazilian rules,
/// blue for the official ones.
pub fn accent(rules: Rules) -> Color {
    match rules {
        Rules::Brazilian => GOLD,
        Rules::Official => OFFICIAL,
    }
}

/// The background of something chosen under `rules` (a switch's cell).
fn chosen_fill(rules: Rules) -> Color {
    match rules {
        Rules::Brazilian => Color::new(0.38, 0.25, 0.12, 1.0),
        Rules::Official => Color::new(0.12, 0.27, 0.58, 1.0),
    }
}

/// The mark of `rules`, `h` high, centred on `c`: Brazil's flag, or the
/// globe. It is `h` wide.
pub fn mark(rules: Rules, c: Vec2, h: f32) {
    match rules {
        Rules::Brazilian => {
            let fh = h * 0.75;
            crate::flags::draw("BR", c.x - crate::flags::width(fh) / 2.0, c.y - fh / 2.0, fh);
        }
        Rules::Official => crate::icons::draw(Icon::Globe, c, h * 1.05, OFFICIAL),
    }
}

/// The two rules side by side in `r`, `selected` highlighted, each with how
/// many players are online under it (when known); the rules clicked, if
/// they are not the selected ones.
pub fn switch(
    ui: &mut Ui,
    r: Rect,
    selected: Rules,
    counts: Option<PerRules<usize>>,
) -> Option<Rules> {
    ui.block(r);
    rrect(r, 8.0, Color::new(0.06, 0.04, 0.03, 0.92));
    let w = r.w / 2.0;
    let size = (r.h * 0.36).clamp(13.0, 18.0);
    let inner_of = |k: usize| Rect::new(r.x + k as f32 * w + 3.0, r.y + 3.0, w - 6.0, r.h - 6.0);
    // The chosen one's plate glides across, its colour turning to the
    // other rules' on the way.
    let at = Rules::ALL.iter().position(|x| *x == selected).unwrap_or(0);
    let key = crate::ui::motion::rect_key(crate::ui::motion::id("rules-switch"), r);
    let plate = ui.glide_rect(key, inner_of(at), 18.0);
    let k = ((plate.x - inner_of(0).x) / w).clamp(0.0, 1.0);
    let fill = crate::ui::mix(chosen_fill(Rules::ALL[0]), chosen_fill(Rules::ALL[1]), k);
    let mut clicked = None;
    for (k, rules) in Rules::ALL.into_iter().enumerate() {
        let cell = Rect::new(r.x + k as f32 * w, r.y, w, r.h);
        let on = rules == selected;
        let lit = ui.hover_anim(cell, ui.hovering(cell) && !on);
        if lit > 0.01 {
            rrect(inner_of(k), 6.0, Color::new(0.22, 0.15, 0.09, lit));
        }
    }
    rrect_gradient(plate, 6.0, shade(fill, 0.1), shade(fill, -0.12));
    for (k, rules) in Rules::ALL.into_iter().enumerate() {
        let cell = Rect::new(r.x + k as f32 * w, r.y, w, r.h);
        let hover = ui.hovering(cell);
        let on = rules == selected;
        // The mark, the name and the count, centred together.
        let label_color = match (on, rules) {
            (true, Rules::Brazilian) => GOLD,
            _ => CREAM,
        };
        let count = counts.map(|c| c.get(rules).to_string());
        let count_w = count
            .as_ref()
            .map_or(0.0, |n| 14.0 + size * 0.85 + 4.0 + ui.text_size(n, size * 0.85, true).x);
        let room = cell.w - 16.0 - size * 1.3 - 8.0 - count_w;
        let full = crate::i18n::rules_short(rules);
        let (label, lsize) = ui.fitted(full, size, room.max(20.0), true);
        let content = size * 1.3 + 8.0 + ui.text_size(&label, lsize, true).x + count_w;
        let mut x = cell.x + (cell.w - content) / 2.0;
        let cy = cell.y + cell.h / 2.0;
        mark(rules, vec2(x + size * 0.65, cy), size * 1.3);
        x += size * 1.3 + 8.0;
        ui.text(&label, x, crate::ranking::text_y(cy, lsize), lsize, label_color, true);
        x += ui.text_size(&label, lsize, true).x;
        if let Some(n) = count {
            let c = if on { CREAM } else { MUTED };
            let s = size * 0.85;
            x += 14.0;
            crate::icons::draw(Icon::Person, vec2(x + s / 2.0, cy), s, c);
            ui.text(&n, x + s + 4.0, crate::ranking::text_y(cy, s), s, c, true);
        }
        if hover && !on && ui.released_in(cell) {
            clicked = Some(rules);
        }
    }
    rrect_lines(r, crate::ui::BUTTON_RADIUS, 1.0, crate::ui::HAIRLINE);
    clicked
}

/// A pill with the rules' mark and `label`, `h` high from (`x`, `y`): the
/// rules of a screen or a game. With `swap`, it switches them when clicked
/// (an arrow says so). Its rectangle, and whether it was clicked.
pub fn pill(
    ui: &mut Ui,
    x: f32,
    y: f32,
    h: f32,
    rules: Rules,
    label: &str,
    swap: bool,
) -> (Rect, bool) {
    let size = (h * 0.42).clamp(12.0, 17.0);
    let swap_w = if swap { size + 10.0 } else { 0.0 };
    let w = 10.0 + h * 0.62 + 8.0 + ui.text_size(label, size, true).x + swap_w + 12.0;
    let r = Rect::new(x, y, w, h);
    let lit = ui.hover_anim(r, swap && ui.hovering(r));
    let a = accent(rules);
    let fill = 0.16 + 0.1 * lit;
    rrect(r, h / 2.0, Color::new(a.r * 0.4, a.g * 0.4, a.b * 0.4, fill + 0.55));
    rrect_lines(r, h / 2.0, 1.2, Color::new(a.r, a.g, a.b, 0.6 + 0.35 * lit));
    let cy = y + h / 2.0;
    mark(rules, vec2(x + 10.0 + h * 0.31, cy), h * 0.62);
    let tx = x + 10.0 + h * 0.62 + 8.0;
    let color = if rules == Rules::Brazilian { GOLD } else { CREAM };
    ui.text(label, tx, crate::ranking::text_y(cy, size), size, color, true);
    if swap {
        let sx = tx + ui.text_size(label, size, true).x + 8.0 + size / 2.0;
        crate::icons::draw(Icon::Swap, vec2(sx, cy), size, Color::new(a.r, a.g, a.b, 0.9));
    }
    // Part of the interface (a click on it is not one on the table under it).
    ui.block(r);
    (r, swap && ui.clicked(r))
}

/// The rules of a game in the top bar, compactly: the mark, the short name
/// and, when known, a green dot and how many are online who play them.
/// Returns its rectangle.
pub fn badge(ui: &mut Ui, x: f32, y: f32, h: f32, rules: Rules, online: Option<usize>) -> Rect {
    let w = badge_w(ui, h, rules, online);
    let r = Rect::new(x, y, w, h);
    let lit = ui.hover_anim(r, ui.hovering(r));
    let a = accent(rules);
    rrect(r, h / 2.0, Color::new(a.r * 0.4, a.g * 0.4, a.b * 0.4, 0.7 + 0.15 * lit));
    rrect_lines(r, h / 2.0, 1.2, Color::new(a.r, a.g, a.b, 0.55 + 0.4 * lit));
    let cy = y + h / 2.0;
    let size = 12.5;
    mark(rules, vec2(x + 9.0 + h * 0.28, cy), h * 0.56);
    let label = crate::i18n::rules_short(rules);
    let tx = x + 9.0 + h * 0.56 + 7.0;
    let color = if rules == Rules::Brazilian { GOLD } else { CREAM };
    ui.text(label, tx, crate::ranking::text_y(cy, size), size, color, true);
    if let Some(n) = online {
        let nx = tx + ui.text_size(label, size, true).x + 9.0;
        // A thin divider, then the dot and the number.
        crate::paint::draw_line(nx, cy - 7.0, nx, cy + 7.0, 1.0, Color::new(a.r, a.g, a.b, 0.35));
        let dot = vec2(nx + 10.0, cy);
        crate::paint::draw_circle(dot.x, dot.y, 3.5, Color::new(0.35, 0.85, 0.45, 1.0));
        let n = n.to_string();
        ui.text(&n, dot.x + 7.0, crate::ranking::text_y(cy, 12.0), 12.0, CREAM, true);
    }
    ui.block(r);
    r
}

/// The width of [`badge`].
pub fn badge_w(ui: &Ui, h: f32, rules: Rules, online: Option<usize>) -> f32 {
    let label = crate::i18n::rules_short(rules);
    let count =
        online.map_or(0.0, |n| 9.0 + 10.0 + 7.0 + ui.text_size(&n.to_string(), 12.0, true).x);
    9.0 + h * 0.56 + 7.0 + ui.text_size(label, 12.5, true).x + count + 12.0
}

/// A game's rules in a line of text: the mark and the short name, `size`
/// high, its middle at `cy`, from `x`; the width it took.
pub fn tag(ui: &Ui, rules: Rules, x: f32, cy: f32, size: f32) -> f32 {
    let label = crate::i18n::rules_short_inline(rules);
    mark(rules, vec2(x + size * 0.6, cy), size * 1.2);
    let tx = x + size * 1.2 + 5.0;
    ui.text(&label, tx, crate::ranking::text_y(cy, size), size, accent(rules), false);
    size * 1.2 + 5.0 + ui.text_size(&label, size, false).x
}

/// The width of [`tag`].
pub fn tag_w(ui: &Ui, rules: Rules, size: f32) -> f32 {
    let label = crate::i18n::rules_short_inline(rules);
    size * 1.2 + 5.0 + ui.text_size(&label, size, false).x
}

/// What the rules are about, in a sentence or two (under their name on
/// the screens where a game is set up).
pub fn summary(rules: Rules) -> &'static str {
    t(SUMMARY[rules.index()])
}

/// The rules in four short points (the cards that ask the player to choose).
pub fn points(rules: Rules) -> [&'static str; 4] {
    POINTS[rules.index()].map(t)
}

/// [`summary`] in English, by [`Rules::index`].
const SUMMARY: [&str; 2] = [
    "A foul gives the opponent balls of their group. The cue ball is in hand on the break only, and only the 8's pocket is called.",
    "A foul gives the opponent the cue ball in hand, anywhere on the table. Only the 8's pocket is called, on each shot at it, and the break must drive four balls to a cushion.",
];

/// [`points`] in English, by [`Rules::index`].
const POINTS: [[&str; 4]; 2] = [
    [
        "A foul gives balls to the opponent",
        "Ball in hand on the break only",
        "Only the 8's pocket is called",
        "The most played in Brazil",
    ],
    [
        "A foul gives ball in hand",
        "The break drives four balls to a cushion",
        "Only the 8 is called, shot by shot",
        "The same all over the world",
    ],
];

/// A box that says what the rules are about, with a title ("Under these
/// rules"), in `r`; the height it needs at `r.w`.
pub fn summary_box(ui: &Ui, r: Rect, rules: Rules, draw: bool) -> f32 {
    let a = accent(rules);
    let lines = ui.wrap(summary(rules), r.w - 28.0, 14.0);
    let h = 14.0 + 20.0 + 6.0 + lines.len() as f32 * 19.0 + 12.0;
    if draw {
        let r = Rect::new(r.x, r.y, r.w, h);
        rrect(r, 8.0, Color::new(a.r * 0.12, a.g * 0.12, a.b * 0.14, 0.9));
        rrect_lines(r, 8.0, 1.0, Color::new(a.r, a.g, a.b, 0.55));
        mark(rules, vec2(r.x + 14.0 + 9.0, r.y + 14.0 + 10.0), 18.0);
        let title = crate::i18n::rules_name(rules);
        ui.text(title, r.x + 14.0 + 26.0, r.y + 14.0, 16.0, a, true);
        for (k, line) in lines.iter().enumerate() {
            ui.text(line, r.x + 14.0, r.y + 40.0 + k as f32 * 19.0, 14.0, CREAM, false);
        }
    }
    h
}

/// What the player did in [`ask`].
pub enum Asked {
    None,
    /// Chose the rules of their games.
    Chose(Rules),
    /// Wants to read both rules first.
    Read,
}

/// The first start of a version with the two rules: how does the player
/// like to play? A card for each of the rules (with how many play it now,
/// once known); the dialog says where to switch them later.
pub fn ask(ui: &mut Ui, counts: Option<PerRules<usize>>) -> Asked {
    let sw = crate::ui::vw();
    let sh = crate::ui::vh();
    let _modal = ui.modal_scope(crate::ui::motion::id("ask-rules"), 0.62);
    let w = 1000.0f32.min(sw - 24.0);
    let pad = (w * 0.05).clamp(18.0, 52.0);
    let inner = w - pad * 2.0;
    let stacked = inner < 560.0;
    let card_h = 300.0;
    let cards_h = if stacked { card_h * 2.0 + 16.0 } else { card_h };
    let lead = t("Choose the rules of your games. You can switch them any time, in the main menu.");
    let lead_lines = ui.wrap(lead, inner, 16.0);
    let h = pad
        + 40.0
        + 14.0
        + lead_lines.len() as f32 * 22.0
        + 26.0
        + cards_h
        + 26.0
        + 22.0
        + pad * 0.6;
    let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
    crate::ui::soft_shadow(r, 12.0, 1.0);
    ui.panel(r);
    ui.block(Rect::new(0.0, 0.0, sw, sh));
    let mut y = r.y + pad;
    let (title, size) = ui.fitted(t("How do you like to play?"), 34.0, inner, true);
    ui.text_centered(&title, sw / 2.0, y, size, GOLD, true);
    y += 40.0 + 14.0;
    for line in &lead_lines {
        ui.text_centered(line, sw / 2.0, y, 16.0, CREAM, false);
        y += 22.0;
    }
    y += 26.0;
    let mut out = Asked::None;
    let cw = if stacked { inner } else { (inner - 24.0) / 2.0 };
    for (k, rules) in Rules::ALL.into_iter().enumerate() {
        let c = if stacked {
            Rect::new(r.x + pad, y + k as f32 * (card_h + 16.0), cw, card_h)
        } else {
            Rect::new(r.x + pad + k as f32 * (cw + 24.0), y, cw, card_h)
        };
        if rules_card(ui, c, rules, counts.map(|n| *n.get(rules))) {
            out = Asked::Chose(rules);
        }
    }
    y += cards_h + 26.0;
    // Not sure: both rules, side by side in the book.
    let q = t("Not sure what changes?");
    let link = t("Read both rules");
    let qw = ui.text_size(q, 15.0, false).x;
    let lw = ui.text_size(link, 15.0, true).x;
    let x = sw / 2.0 - (24.0 + qw + 8.0 + lw) / 2.0;
    crate::icons::draw(Icon::Book, vec2(x + 10.0, y + 9.0), 18.0, MUTED);
    ui.text(q, x + 24.0, y, 15.0, MUTED, false);
    let lr = Rect::new(x + 24.0 + qw + 8.0, y - 4.0, lw, 26.0);
    let lit = ui.hover_anim(lr, ui.hovering(lr));
    ui.text(link, lr.x, y, 15.0, crate::ui::mix(GOLD, CREAM, lit), true);
    crate::paint::draw_line(lr.x, y + 19.0, lr.x + lw, y + 19.0, 1.0, GOLD);
    if ui.clicked(lr) {
        out = Asked::Read;
    }
    out
}

/// A card of [`ask`]: the rules, their points, how many play them and the
/// button that chooses them; whether it was chosen.
fn rules_card(ui: &mut Ui, c: Rect, rules: Rules, playing: Option<usize>) -> bool {
    let a = accent(rules);
    let lit = ui.hover_anim(c, ui.hovering(c));
    rrect(c, 10.0, Color::new(a.r * 0.08, a.g * 0.08, a.b * 0.1, 0.95));
    rrect_lines(c, 10.0, 1.2 + 0.8 * lit, Color::new(a.r, a.g, a.b, 0.55 + 0.45 * lit));
    let x = c.x + 24.0;
    let mut y = c.y + 24.0;
    mark(rules, vec2(x + 26.0, y + 24.0), 52.0);
    let name = crate::i18n::rules_name(rules);
    let (name, size) = ui.fitted(name, 26.0, c.w - 24.0 - 76.0 - 16.0, true);
    ui.text(&name, x + 70.0, y + 4.0, size, CREAM, true);
    let tag = crate::i18n::rules_tagline(rules);
    let (tag, tsize) = ui.fitted(tag, 15.0, c.w - 24.0 - 76.0 - 16.0, false);
    ui.text(&tag, x + 70.0, y + 34.0, tsize, a, false);
    y += 64.0;
    crate::paint::draw_line(x, y, c.x + c.w - 24.0, y, 1.0, Color::new(a.r, a.g, a.b, 0.3));
    y += 16.0;
    for point in points(rules) {
        crate::icons::draw(
            Icon::Check,
            vec2(x + 8.0, y + 8.0),
            15.0,
            Color::new(0.4, 0.85, 0.5, 1.0),
        );
        let (point, psize) = ui.fitted(point, 15.0, c.w - 48.0 - 28.0, false);
        ui.text(&point, x + 26.0, y, psize, CREAM, false);
        y += 26.0;
    }
    // How many play them now, and the button.
    let by = c.y + c.h - 24.0 - 44.0;
    let bw = 150.0f32.min(c.w * 0.42);
    let b = Rect::new(c.x + c.w - 24.0 - bw, by, bw, 44.0);
    if let Some(n) = playing {
        let cy = by + 22.0;
        draw_circle(x + 4.0, cy, 3.5, Color::new(0.35, 0.85, 0.45, 0.95));
        crate::icons::draw(Icon::Person, vec2(x + 20.0, cy), 15.0, CREAM);
        let number = n.to_string();
        ui.text(&number, x + 32.0, crate::ranking::text_y(cy, 17.0), 17.0, CREAM, true);
        let nw = ui.text_size(&number, 17.0, true).x;
        let (now, nsize) =
            ui.fitted(t("playing now"), 14.0, b.x - (x + 32.0 + nw + 8.0) - 8.0, false);
        ui.text(&now, x + 32.0 + nw + 8.0, crate::ranking::text_y(cy, nsize), nsize, MUTED, false);
    }
    let chose = if rules == Rules::Brazilian {
        ui.primary_button(b, t("Choose"))
    } else {
        ui.button(b, t("Choose"), true)
    };
    chose || (ui.clicked(c) && !ui.hovering(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_text_is_translated() {
        use crate::i18n::{Lang, has_translation};
        for lang in Lang::translated() {
            for text in SUMMARY.iter().chain(POINTS.iter().flatten()) {
                assert!(has_translation(lang, text), "{lang:?}: {text}");
            }
        }
    }
}
