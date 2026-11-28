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
    scroll: crate::ui::Scroll,
    /// The rules whose ranking is shown (a tab each).
    rules: Rules,
    /// Only the player and their friends (else the world's top 100).
    friends: bool,
    /// The player's country's top 100 (by the flag on their profile).
    country: bool,
    /// The country's ranking asked for (rules, country): asked once the
    /// player's own card says which country is theirs.
    asked_country: Option<(Rules, String)>,
    /// The doubles ranking (each player's doubles rating).
    doubles: bool,
}

/// The country on the player's profile (its flag; empty: none chosen).
fn my_country(online: &Online) -> String {
    online.me.as_ref().map(|m| m.country.to_ascii_uppercase()).unwrap_or_default()
}

/// The player and their friends with rated games under `rules`, best first,
/// numbered (the player is always in it, to see their place).
pub fn friends_board(online: &Online, rules: Rules) -> Vec<snook_legend_proto::LeaderboardEntry> {
    let me = online.me.as_ref().filter(|m| !m.guest);
    let mut list: Vec<snook_legend_proto::LeaderboardEntry> = online
        .friends
        .friends
        .iter()
        .map(|f| &f.player)
        .filter(|p| p.standing(rules).games > 0)
        .filter(|p| me.is_none_or(|me| !me.username.eq_ignore_ascii_case(&p.username)))
        .chain(me)
        .map(|p| {
            let s = p.standing(rules);
            snook_legend_proto::LeaderboardEntry {
                rank: 0,
                username: p.username.clone(),
                rating: s.rating,
                games: s.games,
                wins: s.wins,
                country: p.country.clone(),
                vip: p.vip,
                admin: p.role == snook_legend_proto::Role::Admin,
            }
        })
        .collect();
    list.sort_by(|a, b| {
        b.rating.cmp(&a.rating).then(b.wins.cmp(&a.wins)).then(a.username.cmp(&b.username))
    });
    for (k, e) in list.iter_mut().enumerate() {
        e.rank = k as u32 + 1;
    }
    list
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

/// The server can't be reached: said plainly, what went wrong under it in
/// small type, and a button to try again (true when it is clicked).
pub(crate) fn offline_state(ui: &mut Ui, area: Rect, detail: &str) -> bool {
    let width = (area.w - 32.0).min(420.0);
    let lines = ui.wrap(detail, width, 13.0);
    let block = 56.0 + 14.0 + 22.0 + 8.0 + lines.len() as f32 * 18.0 + 18.0 + 38.0;
    let mut y = area.y + ((area.h - block) / 2.0).max(0.0);
    let cx = area.x + area.w / 2.0;
    draw_circle(cx, y + 28.0, 28.0, Color::new(1.0, 0.56, 0.44, 0.1));
    crate::icons::draw(Icon::WifiSlash, vec2(cx, y + 28.0), 28.0, crate::ui::ALERT);
    y += 56.0 + 14.0;
    ui.text_centered(t("Can't reach the server"), cx, y, 17.0, CREAM, true);
    y += 22.0 + 8.0;
    for line in &lines {
        ui.text_centered(line, cx, y, 13.0, MUTED, false);
        y += 18.0;
    }
    y += 18.0;
    let label = t("Try again");
    let bw = ui.text_size(label, 15.0, true).x + 48.0;
    ui.button(Rect::new(cx - bw / 2.0, y, bw, 38.0), label, true)
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
    /// Development aid (screenshots): `--screen ranking-friends` or
    /// `ranking-doubles` opens on that board.
    pub fn dev(&mut self, board: &str) {
        self.friends = board == "ranking-friends";
        self.doubles = board == "ranking-doubles";
        self.country = board == "ranking-country";
    }

    /// Opens on the ranking of the rules the player plays.
    pub fn open(&mut self, online: &mut Online) {
        self.scroll.reset();
        self.rules = online.rules;
        self.request(online, self.rules);
    }

    /// Asks for the ranking shown (singles, the country's or doubles) of
    /// `rules`.
    fn request(&self, online: &mut Online, rules: Rules) {
        if self.doubles {
            online.request_doubles_leaderboard(rules);
        } else if self.country {
            let country = my_country(online);
            if !country.is_empty() {
                online.request_country_leaderboard(rules, &country);
            }
        } else {
            online.request_leaderboard(rules);
        }
    }

    pub fn draw(&mut self, ui: &mut Ui, online: &mut Online) -> RankingAction {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        ui.screen_dim(0.5);
        // The world's top 100, the country's, or only the player and their
        // friends.
        let country = my_country(online);
        let country_loaded = online
            .country_board
            .as_ref()
            .is_some_and(|(r, c, _)| *r == self.rules && *c == country);
        if self.country
            && !country.is_empty()
            && !country_loaded
            && self.asked_country.as_ref() != Some(&(self.rules, country.clone()))
        {
            self.asked_country = Some((self.rules, country.clone()));
            online.request_country_leaderboard(self.rules, &country);
        }
        let entries = if self.doubles {
            online.doubles.leaderboard.get(self.rules).clone()
        } else if self.friends {
            friends_board(online, self.rules)
        } else if self.country {
            match &online.country_board {
                Some((_, _, list)) if country_loaded => list.clone(),
                _ => Vec::new(),
            }
        } else {
            online.leaderboard.get(self.rules).clone()
        };
        // The panel fits the list (up to the screen's height); an empty or
        // loading list gets room for its message. The tabs of the rules
        // under the header, a note on the ratings under the list.
        const TABS_H: f32 = 44.0;
        const NOTE_H: f32 = 30.0;
        let head = HEADER_H + TABS_H + 16.0 + 12.0 + 26.0;
        let room = ((sh - 40.0 - head - NOTE_H - PAD) / ROW_H).floor().max(3.0) as usize;
        let body_h =
            if entries.is_empty() { 200.0 } else { entries.len().min(room) as f32 * ROW_H };
        let w = (sw - 40.0).min(880.0);
        let h = (head + body_h + NOTE_H + PAD - 4.0).min(sh - 40.0);
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        ui.panel(r);
        let mut action = RankingAction::None;
        // World or friends, by the refresh button (signed in only: the
        // friends are the account's).
        // (Not signed in yet: the boards of an account wait for it.)
        let can_friends = online.account().is_some();
        if !can_friends && online.signed_in() {
            self.friends = false;
            self.country = false;
        }
        // World, the player's country and friends (signed in), doubles.
        let views: Vec<&str> = if can_friends {
            vec![t("World"), t("Country"), t("Friends"), t("Doubles")]
        } else {
            vec![t("World"), t("Doubles")]
        };
        let seg_w =
            views.iter().map(|v| ui.text_size(v, 13.0, true).x + 28.0).sum::<f32>().max(180.0);
        let seg = Rect::new(r.x + r.w - PAD - 36.0 - 12.0 - seg_w, r.y + 22.0, seg_w, 32.0);
        let (title, subtitle) = if self.doubles {
            (t("Doubles ranking"), t("Each player's doubles rating").to_string())
        } else if self.friends {
            (t("Friends ranking"), t("You and your friends").to_string())
        } else if self.country && !country.is_empty() {
            let name = crate::flags::name(&country);
            (t("Country ranking"), tw("The top 100 of {country}", &[("country", &name)]))
        } else if self.country {
            (t("Country ranking"), t("The best players of your country").to_string())
        } else {
            (t("World ranking"), t("The top 100 of each rules").to_string())
        };
        let right = seg.x - 12.0;
        let picking = crate::countries::is_open();
        if screen_header_until(ui, r, Icon::Trophy, title, Some(&subtitle), right)
            || (ui.escape() && !picking)
        {
            action = RankingAction::Back;
        }
        let picked = if self.doubles {
            views.len() - 1
        } else if !can_friends {
            0
        } else if self.friends {
            2
        } else {
            usize::from(self.country)
        };
        if let Some(k) = ui.segmented(seg, &views, picked) {
            self.doubles = k == views.len() - 1;
            self.country = can_friends && k == 1;
            self.friends = can_friends && k == 2;
            self.scroll.reset();
            if self.friends {
                online.send(snook_legend_proto::ClientMsg::Friends);
            } else {
                self.request(online, self.rules);
            }
        }
        if ui.icon_button(
            Rect::new(r.x + r.w - PAD - 36.0, r.y + 20.0, 36.0, 36.0),
            Icon::Refresh,
            t("Refresh"),
            true,
        ) {
            if self.friends {
                online.send(snook_legend_proto::ClientMsg::Friends);
            } else {
                self.request(online, self.rules);
            }
        }
        // A tab for each of the rules.
        let tabs = Rect::new(r.x + PAD, r.y + HEADER_H, r.w - PAD * 2.0, TABS_H);
        if let Some(rules) = crate::rulesui::switch(ui, tabs, self.rules, None) {
            self.rules = rules;
            self.scroll.reset();
            self.request(online, rules);
        }
        // Other rules or another board: the list slides in from that side.
        let board = if self.doubles {
            3
        } else if self.friends {
            2
        } else {
            i64::from(self.country)
        };
        let rules = i64::from(self.rules == snook_legend_core::Rules::Official);
        let _switch =
            ui.switch_scope(crate::ui::motion::id("ranking-board"), rules * 4 + board, 16.0);
        // Each of the rules has its own rating: said under the list.
        let note = online.me.as_ref().filter(|m| !m.guest).map(|me| {
            let other = self.rules.other();
            tw(
                "Each of the rules has its own rating: these games don't change your {rating} under the {other}.",
                &[
                    ("rating", &me.rating_in(other)),
                    ("other", &crate::i18n::rules_name_inline(other)),
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
        // Friends without rated games: only the player in the list.
        if self.friends && entries.len() <= 1 && online.friends.friends.is_empty() {
            let text = t("Add friends to see where you stand among them.");
            empty_state(ui, list, Icon::People, text);
            return action;
        }
        // The player's country: their flag first (it is chosen here too).
        if self.country && country.is_empty() {
            let text = t("Choose your country to see how you rank among its players.");
            if crate::online::empty_state(
                ui,
                list,
                crate::online::Glyph::Icon(Icon::Globe),
                text,
                Some(t("Choose my country")),
            ) {
                crate::countries::open();
            }
            if let Some(code) = crate::countries::draw_picker(ui) {
                online.send(snook_legend_proto::ClientMsg::SetCountry { country: code.clone() });
                if let Some(me) = online.me.as_mut() {
                    me.country = code.clone();
                }
                online.request_country_leaderboard(self.rules, &code);
            }
            return action;
        }
        if entries.is_empty() {
            if self.friends {
                empty_state(ui, list, Icon::People, t("No rated games yet. Be the first!"));
            } else if self.doubles && *online.doubles.leaderboard_loaded.get(self.rules) {
                empty_state(ui, list, Icon::Trophy, t("No rated doubles yet. Be the first pair!"));
            } else if self.country && country_loaded {
                let text = t("No rated games in your country yet. Be the first!");
                empty_state(ui, list, Icon::Globe, text);
            } else if !self.doubles && !self.country && *online.leaderboard_loaded.get(self.rules) {
                empty_state(ui, list, Icon::Trophy, t("No rated games yet. Be the first!"));
            } else if let Some(error) = unreachable(online) {
                if offline_state(ui, list, &error) {
                    online.error = None;
                    self.request(online, self.rules);
                }
            } else {
                loading(ui, list, t("Loading…"));
            }
            return action;
        }
        let me = online.me.as_ref().map(|m| m.username.clone()).unwrap_or_default();
        let mine = entries.iter().position(|e| e.username.eq_ignore_ascii_case(&me));
        // Your own row, when it is scrolled out of sight, stays pinned at
        // the bottom (under a thin line), so you always see where you are:
        // it takes the last place, where it slides into the list.
        let pin_h = ROW_H + 2.0;
        let offset = self.scroll.offset();
        let pinned = mine.filter(|&m| {
            let top = m as f32 * ROW_H - offset;
            scrolls && (top < -0.5 || top + ROW_H > list.h - pin_h + 0.5)
        });
        let view_r = if pinned.is_some() { Rect { h: list.h - pin_h, ..list } } else { list };
        let view = ui.list_begin(view_r, &mut self.scroll, entries.len(), ROW_H);
        let mut rows: Vec<(usize, f32)> = view.rows().map(|i| (i, view.y(i))).collect();
        let in_view = rows.len();
        if let Some(m) = pinned {
            rows.push((m, view_r.y + view_r.h + 2.0));
        }
        let mut view = Some(view);
        for (k, (i, y)) in rows.into_iter().enumerate() {
            if k == in_view
                && let Some(v) = view.take()
            {
                ui.scroll_end(v, &mut self.scroll);
                let y = view_r.y + view_r.h;
                draw_line(
                    x0 + 8.0,
                    y + 1.0,
                    x1 - 8.0,
                    y + 1.0,
                    1.0,
                    Color::new(GOLD.r, GOLD.g, GOLD.b, 0.35),
                );
            }
            let e = &entries[i];
            let row = Rect::new(x0, y + 2.0, x1 - x0, ROW_H - 4.0);
            let is_me = Some(i) == mine;
            let hover = ui.hovering(row);
            // Lights up under the pointer, easing in and out.
            let lit = ui.hover_anim(row, hover);
            if is_me {
                rrect(row, 6.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.15 + 0.07 * lit));
                rrect_lines(row, 6.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.55));
            } else {
                let rest = if i % 2 == 0 { 0.025 } else { 0.0 };
                let a = rest + (0.08 - rest) * lit;
                if a > 0.001 {
                    rrect(row, 6.0, Color::new(1.0, 1.0, 1.0, a));
                }
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
            if ui.activated(row, hover) {
                action = RankingAction::Profile(e.username.clone());
            }
        }
        if let Some(v) = view.take() {
            ui.scroll_end(v, &mut self.scroll);
        }
        action
    }
}
