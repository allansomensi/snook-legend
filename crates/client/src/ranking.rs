//! The world ranking: the top 100 rated players.
//!
//! Also the few drawing helpers the shop, ranking and profile screens share
//! (the screen header, loading and empty states).

use macroquad::prelude::*;

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle};

use snook_legend_core::Rules;

use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::online::Online;
use crate::ui::{BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines, tier_color};

pub enum RankingAction {
    None,
    Back,
    Profile(String),
}

#[derive(Default)]
pub struct Ranking {
    first: usize,
    /// The rules whose ranking is shown (a tab each).
    rules: Rules,
}

/// Inner padding of the screens' panels.
pub(crate) const PAD: f32 = 24.0;
/// Where the content starts under a screen header (from the panel's top).
pub(crate) const HEADER_H: f32 = 76.0;

/// The `y` that centres text of `size` on `center` (the middle of its
/// capitals and lower case, as the toolkit's buttons do).
pub(crate) fn text_y(center: f32, size: f32) -> f32 {
    center - size * 0.5
}

/// The `y` that centres capitals and digits of `size` on `center` (a big
/// name or number next to an icon or a flag).
pub(crate) fn caps_y(center: f32, size: f32) -> f32 {
    center - size * 0.435
}

/// A screen's header: the back button, the icon and title, and a line of
/// muted text under the title. True when the back button was clicked.
pub(crate) fn screen_header(
    ui: &mut Ui,
    r: Rect,
    icon: Icon,
    title: &str,
    subtitle: Option<&str>,
) -> bool {
    screen_header_until(ui, r, icon, title, subtitle, r.x + r.w - PAD)
}

/// [`screen_header`] with its title and subtitle kept left of `right`
/// (tabs on the right of the header): smaller, or shortened, to fit.
pub(crate) fn screen_header_until(
    ui: &mut Ui,
    r: Rect,
    icon: Icon,
    title: &str,
    subtitle: Option<&str>,
    right: f32,
) -> bool {
    let back =
        ui.icon_button(Rect::new(r.x + PAD, r.y + 20.0, 36.0, 36.0), Icon::Back, t("Back"), true);
    let mid = r.y + 38.0;
    let x = r.x + PAD + 36.0 + 16.0;
    crate::icons::draw(icon, vec2(x + 12.0, mid), 24.0, GOLD);
    let room = (right - (x + 34.0)).max(60.0);
    let (title, size) = ui.fitted(title, 24.0, room, true);
    match subtitle {
        Some(sub) => {
            ui.text(&title, x + 34.0, r.y + 15.0 + (24.0 - size) / 2.0, size, GOLD, true);
            let (sub, sub_size) = ui.fitted(sub, 13.0, room, false);
            ui.text(&sub, x + 34.0, r.y + 44.0, sub_size, MUTED, false);
        }
        None => ui.text(&title, x + 34.0, caps_y(mid, size), size, GOLD, true),
    }
    back
}

/// A spinner and a line of text, centred in `area`.
pub(crate) fn loading(ui: &Ui, area: Rect, text: &str) {
    let cy = area.y + area.h / 2.0;
    crate::ui::spinner(vec2(area.x + area.w / 2.0, cy - 14.0), 11.0, GOLD);
    ui.text_centered(text, area.x + area.w / 2.0, cy + 10.0, 14.0, MUTED, false);
}

/// Nothing to show yet: a muted icon and a line of text (wrapped), centred
/// in `area`.
pub(crate) fn empty_state(ui: &Ui, area: Rect, icon: Icon, text: &str) {
    let lines = ui.wrap(text, (area.w - 32.0).min(420.0), 14.0);
    let block = 44.0 + 12.0 + lines.len() as f32 * 19.0;
    let mut y = area.y + (area.h - block) / 2.0;
    let cx = area.x + area.w / 2.0;
    draw_circle(cx, y + 22.0, 26.0, Color::new(1.0, 1.0, 1.0, 0.04));
    crate::icons::draw(icon, vec2(cx, y + 22.0), 26.0, Color::new(MUTED.r, MUTED.g, MUTED.b, 0.7));
    y += 56.0;
    for line in &lines {
        ui.text_centered(line, cx, y, 14.0, MUTED, false);
        y += 19.0;
    }
}

/// Why the server can't be asked (the connection failed), if it can't:
/// better said than a spinner turning for ever.
pub(crate) fn unreachable(online: &Online) -> Option<String> {
    let closed = matches!(online.stage, crate::online::Stage::SignIn | crate::online::Stage::Idle);
    if closed && !online.signed_in() { online.error.clone() } else { None }
}

/// The columns, from the right edge of the rows inwards: (width, header,
/// right-aligned).
const COLUMNS: [(f32, &str, bool); 4] =
    [(60.0, "Win %", true), (76.0, "Games", true), (112.0, "Tier", false), (64.0, "Rating", true)];
const GAP: f32 = 20.0;
const ROW_H: f32 = 36.0;

impl Ranking {
    /// Opens on the ranking of the rules the player plays.
    pub fn open(&mut self, online: &mut Online) {
        self.first = 0;
        self.rules = online.rules;
        online.request_leaderboard(self.rules);
    }

    pub fn draw(&mut self, ui: &mut Ui, online: &mut Online) -> RankingAction {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.5));
        let entries = online.leaderboard.get(self.rules).clone();
        // The panel fits the list (up to the screen's height); an empty or
        // loading list gets room for its message. The tabs of the rules
        // under the header, a note on the ratings under the list.
        const TABS_H: f32 = 44.0;
        const NOTE_H: f32 = 30.0;
        let head = HEADER_H + TABS_H + 16.0 + 12.0 + 26.0;
        let room = ((sh - 40.0 - head - NOTE_H - PAD) / ROW_H).floor().max(3.0) as usize;
        let body_h =
            if entries.is_empty() { 200.0 } else { entries.len().min(room) as f32 * ROW_H };
        let w = (sw - 40.0).min(760.0);
        let h = (head + body_h + NOTE_H + PAD - 4.0).min(sh - 40.0);
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        ui.panel(r);
        let mut action = RankingAction::None;
        let subtitle = t("The top 100 of each rules · rated games");
        if screen_header(ui, r, Icon::Trophy, t("World ranking"), Some(subtitle)) || ui.escape() {
            action = RankingAction::Back;
        }
        if ui.icon_button(
            Rect::new(r.x + r.w - PAD - 36.0, r.y + 20.0, 36.0, 36.0),
            Icon::Refresh,
            t("Refresh"),
            true,
        ) {
            online.request_leaderboard(self.rules);
        }
        // A tab for each of the rules, with how many play it now.
        let tabs = Rect::new(r.x + PAD, r.y + HEADER_H, r.w - PAD * 2.0, TABS_H);
        let players = online.counts().map(|c| c.map(|_, n| n.players));
        if let Some(rules) = crate::rulesui::switch(ui, tabs, self.rules, players) {
            self.rules = rules;
            self.first = 0;
            online.request_leaderboard(rules);
        }
        // Each of the rules has its own rating: said under the list.
        let note = online.me.as_ref().filter(|m| !m.guest).map(|me| {
            let other = self.rules.other();
            tw(
                "Each of the rules has its own rating: these games don't change your {rating} under the {other}.",
                &[
                    ("rating", &me.rating_in(other)),
                    ("other", &crate::i18n::rules_name(other).to_lowercase()),
                ],
            )
        });
        if let Some(note) = &note {
            let (note, size) = ui.fitted(note, 13.0, r.w - PAD * 2.0, false);
            ui.text(&note, r.x + PAD, r.y + r.h - PAD - 8.0, size, MUTED, false);
        }
        // The rows run from x0 to x1; a scroll bar, when there is one, sits
        // to the right of x1.
        let scrolls = entries.len() > room;
        let x0 = r.x + PAD - 8.0;
        let x1 = r.x + r.w - PAD + 8.0 - if scrolls { 12.0 } else { 0.0 };
        // Column positions: (left edge, width, right-aligned). A column is
        // never narrower than its heading (long in some languages).
        let mut cols = Vec::new();
        let mut right = x1 - 12.0;
        for (cw, label, ra) in COLUMNS {
            let cw = cw.max(ui.text_size(t(label), 12.0, true).x);
            cols.push((right - cw, cw, ra));
            right -= cw + GAP;
        }
        let name_x = x0 + 52.0;
        let name_w = right + GAP - 12.0 - name_x;
        let head_y = r.y + HEADER_H + TABS_H + 16.0 + 12.0;
        ui.text("#", x0 + 20.0 - ui.text_size("#", 12.0, true).x / 2.0, head_y, 12.0, MUTED, true);
        ui.text(t("Player"), name_x, head_y, 12.0, MUTED, true);
        for (k, (_, label, _)) in COLUMNS.iter().enumerate() {
            let (cx, cw, ra) = cols[k];
            let label = t(label);
            let lw = ui.text_size(label, 12.0, true).x;
            ui.text(label, if ra { cx + cw - lw } else { cx }, head_y, 12.0, MUTED, true);
        }
        let line_y = head_y + 21.0;
        draw_line(r.x + PAD - 8.0, line_y, r.x + r.w - PAD + 8.0, line_y, 1.0, BORDER);
        let list = Rect::new(x0, line_y + 5.0, r.x + r.w - PAD + 8.0 - x0, body_h);
        if entries.is_empty() {
            if *online.leaderboard_loaded.get(self.rules) {
                empty_state(ui, list, Icon::Trophy, t("No rated games yet. Be the first!"));
            } else if let Some(error) = unreachable(online) {
                empty_state(ui, list, Icon::Network, &error);
            } else {
                loading(ui, list, t("Loading…"));
            }
            return action;
        }
        let me = online.me.as_ref().map(|m| m.username.clone()).unwrap_or_default();
        let mine = entries.iter().position(|e| e.username.eq_ignore_ascii_case(&me));
        let visible = entries.len().min(room);
        self.first = ui.scroll_list(list, self.first, entries.len(), visible);
        // Your own row, when it is scrolled out of sight, stays pinned at
        // the bottom (under a thin line), so you always see where you are.
        let pinned = mine.filter(|&m| m < self.first || m >= self.first + visible);
        let shown = if pinned.is_some() { visible - 1 } else { visible };
        let mut rows: Vec<(usize, f32)> = (self.first..(self.first + shown).min(entries.len()))
            .enumerate()
            .map(|(k, i)| (i, list.y + k as f32 * ROW_H))
            .collect();
        if let Some(m) = pinned {
            let y = list.y + shown as f32 * ROW_H;
            draw_line(
                x0 + 8.0,
                y + 1.0,
                x1 - 8.0,
                y + 1.0,
                1.0,
                Color::new(GOLD.r, GOLD.g, GOLD.b, 0.35),
            );
            rows.push((m, y + 2.0));
        }
        for (i, y) in rows {
            let e = &entries[i];
            let row = Rect::new(x0, y + 2.0, x1 - x0, ROW_H - 4.0);
            let is_me = Some(i) == mine;
            let hover = ui.hovering(row);
            if is_me {
                rrect(
                    row,
                    6.0,
                    Color::new(GOLD.r, GOLD.g, GOLD.b, if hover { 0.22 } else { 0.15 }),
                );
                rrect_lines(row, 6.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.55));
            } else if hover {
                rrect(row, 6.0, Color::new(1.0, 1.0, 1.0, 0.08));
            } else if i % 2 == 0 {
                rrect(row, 6.0, Color::new(1.0, 1.0, 1.0, 0.025));
            }
            let mid = row.y + row.h / 2.0;
            let medal = match e.rank {
                1 => Some(GOLD),
                2 => Some(Color::new(0.82, 0.82, 0.86, 1.0)),
                3 => Some(Color::new(0.80, 0.55, 0.33, 1.0)),
                _ => None,
            };
            let rank = e.rank.to_string();
            match medal {
                Some(c) => {
                    draw_circle(x0 + 20.0, mid, 11.0, c);
                    ui.text_centered(
                        &rank,
                        x0 + 20.0,
                        text_y(mid, 13.0),
                        13.0,
                        Color::new(0.1, 0.07, 0.04, 1.0),
                        true,
                    );
                }
                None => ui.text_centered(&rank, x0 + 20.0, text_y(mid, 14.0), 14.0, MUTED, true),
            }
            let tier = snook_legend_proto::Tier::of(e.rating);
            let color = if e.admin {
                crate::ui::ADMIN_RED
            } else if is_me {
                GOLD
            } else {
                tier_color(tier)
            };
            let crown = if e.vip { 22.0 } else { 0.0 };
            let ty = text_y(mid, 15.0);
            let used =
                ui.name_with_flag(&e.username, &e.country, color, name_x, ty, 15.0, name_w - crown);
            if e.vip {
                // VIP members: the crown, as everywhere else.
                crate::icons::draw(Icon::Crown, vec2(name_x + used + 12.0, mid), 14.0, GOLD);
            }
            let pct = (e.wins * 100).checked_div(e.games).unwrap_or(0);
            let cells: [(String, f32, Color, bool); 4] = [
                (format!("{pct}%"), 14.0, MUTED, false),
                (e.games.to_string(), 14.0, MUTED, false),
                (t(tier.name()).to_string(), 14.0, tier_color(tier), false),
                (e.rating.to_string(), 15.0, CREAM, true),
            ];
            for (k, (text, size, c, bold)) in cells.iter().enumerate() {
                let (cx, cw, ra) = cols[k];
                let (text, size) = ui.fitted(text, *size, cw, *bold);
                let tw = ui.text_size(&text, size, *bold).x;
                let x = if ra { cx + cw - tw } else { cx };
                ui.text(&text, x, text_y(mid, size), size, *c, *bold);
            }
            if hover && ui.clicked(row) {
                action = RankingAction::Profile(e.username.clone());
            }
        }
        action
    }
}
