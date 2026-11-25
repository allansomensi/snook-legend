//! The inbox's screen: what the staff and the game told the player, newest
//! first with the pinned ones on top (a warning, a mute running), filtered
//! by kind with the unread count on each filter, and the one picked shown
//! in full with its button ("See the game", "Equip now"…). The
//! notifications themselves are kept by `online::inbox`.

use macroquad::prelude::*;
use snook_legend_proto::ClientMsg;
use snook_legend_proto::inbox::{Action, Category, Notification};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle};

use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::online::{Online, action_label, category_color, category_name, wording};
use crate::ranking::{HEADER_H, PAD, empty_state, loading, screen_header_until};
use crate::ui::{BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};

/// What the inbox asks of the rest of the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboxAction {
    None,
    Back,
    Profile(String),
    /// The shop (on an item when not empty).
    Shop(String),
    /// The shop's VIP page.
    Vip,
    /// The lounge, on its tournaments (the one picked is shown).
    Tournaments,
    /// The collection, on this item. Its screen comes in another part of
    /// 0.12: until it is wired (main.rs, `Screen::Inbox`), the shop opens
    /// on the item.
    Collection(String),
    /// The auctions: the shop until they have a screen of their own (to
    /// re-point in main.rs, `Screen::Inbox`).
    Auctions,
}

/// The inbox's screen state.
#[derive(Default)]
pub struct InboxScreen {
    /// The filter (None: all).
    filter: Option<Category>,
    /// The notification shown in full.
    selected: Option<i64>,
    /// The first row of the list on screen.
    first: usize,
}

const ROW_H: f32 = 62.0;
const TABS_H: f32 = 44.0;

/// The filters, in order.
const FILTERS: [Option<Category>; 7] = [
    None,
    Some(Category::Staff),
    Some(Category::Punishment),
    Some(Category::Gift),
    Some(Category::Tournament),
    Some(Category::Achievement),
    Some(Category::Game),
];

/// Pinned ones first (until they expire), then the newest.
fn ordered(items: &[Notification], filter: Option<Category>, now: i64) -> Vec<Notification> {
    let mut list: Vec<Notification> =
        items.iter().filter(|n| filter.is_none_or(|c| n.category() == c)).cloned().collect();
    list.sort_by_key(|n| {
        (!n.pinned(now), std::cmp::Reverse(n.created_at), std::cmp::Reverse(n.id))
    });
    list
}

impl InboxScreen {
    /// Opens on everything, asking the server for the newest.
    pub fn open(&mut self, online: &mut Online) {
        self.filter = None;
        self.selected = None;
        self.first = 0;
        online.request_inbox();
    }

    pub fn draw(&mut self, ui: &mut Ui, online: &mut Online) -> InboxAction {
        let (sw, sh) = (crate::ui::vw(), crate::ui::vh());
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.5));
        let w = (sw - 40.0).min(980.0);
        let h = (sh - 40.0).min(760.0);
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        ui.panel(r);
        let now = crate::game::unix_now();
        let unread = online.inbox_unread();
        let subtitle = if unread > 0 {
            tw("{n} unread", &[("n", &unread)])
        } else {
            t("Everything read").to_string()
        };
        // "Mark all as read" on the right of the header.
        let all_label = t("Mark all as read");
        let bw = ui.text_size(all_label, 13.0, true).x + 24.0;
        let all_r = Rect::new(r.x + r.w - PAD - bw, r.y + 22.0, bw, 32.0);
        let mut action = InboxAction::None;
        if screen_header_until(ui, r, Icon::List, t("Inbox"), Some(&subtitle), all_r.x - 12.0)
            || (ui.escape() && ui.focus.is_none())
        {
            if self.selected.is_some() && w < 760.0 {
                self.selected = None;
            } else {
                action = InboxAction::Back;
            }
        }
        // Everything read at once (not the important ones: each is confirmed).
        let plain: Vec<i64> = online
            .inbox_items()
            .iter()
            .filter(|n| n.unread() && !n.important)
            .map(|n| n.id)
            .collect();
        if ui.button(all_r, all_label, !plain.is_empty()) {
            online.inbox_read(plain);
        }
        // The filters, each with what isn't read in it.
        let row = Rect::new(r.x + PAD, r.y + HEADER_H, r.w - PAD * 2.0, 34.0);
        let labels: Vec<&str> = FILTERS.iter().map(|c| category_name(*c)).collect();
        let counts: Vec<String> = FILTERS
            .iter()
            .map(|c| {
                let n = online
                    .inbox_items()
                    .iter()
                    .filter(|x| x.unread() && c.is_none_or(|c| x.category() == c))
                    .count();
                if n > 0 { n.to_string() } else { String::new() }
            })
            .collect();
        let counts: Vec<&str> = counts.iter().map(String::as_str).collect();
        let picked = FILTERS.iter().position(|c| *c == self.filter).unwrap_or(0);
        let (clicked, _) = crate::tabrow::tabs(ui, row, &labels, &counts, picked, 6.0);
        if let Some(k) = clicked {
            self.filter = FILTERS[k];
            self.first = 0;
        }
        let body = Rect::new(
            r.x + PAD,
            r.y + HEADER_H + TABS_H,
            r.w - PAD * 2.0,
            r.h - HEADER_H - TABS_H - PAD,
        );
        let list = ordered(online.inbox_items(), self.filter, now);
        if !online.inbox_loaded() && list.is_empty() {
            loading(ui, body, t("Loading…"));
            return action;
        }
        if list.is_empty() {
            let text = if self.filter.is_none() {
                t("No notifications yet. What the staff and the game tell you comes here.")
            } else {
                t("Nothing here.")
            };
            empty_state(ui, body, Icon::List, text);
            return action;
        }
        // Side by side when there is room; else the list, or the one picked.
        let wide = body.w >= 700.0;
        let selected = self.selected.and_then(|id| list.iter().find(|n| n.id == id)).cloned();
        let (list_r, detail_r) = if wide {
            let lw = (body.w * 0.42).max(300.0);
            (
                Some(Rect::new(body.x, body.y, lw, body.h)),
                Some(Rect::new(body.x + lw + 16.0, body.y, body.w - lw - 16.0, body.h)),
            )
        } else if selected.is_some() {
            (None, Some(body))
        } else {
            (Some(body), None)
        };
        if let Some(lr) = list_r
            && let Some(id) = self.draw_list(ui, lr, &list, now)
        {
            self.selected = Some(id);
            // Reading a plain one is opening it; an important one asks.
            if list.iter().any(|n| n.id == id && n.unread() && !n.important) {
                online.inbox_read(vec![id]);
            }
        }
        if let Some(dr) = detail_r {
            match &selected {
                Some(n) => {
                    if let Some(a) = self.draw_detail(ui, online, dr, n, now) {
                        action = a;
                    }
                }
                None => {
                    rrect(dr, 8.0, Color::new(1.0, 1.0, 1.0, 0.025));
                    let msg = t("Pick a notification to read it.");
                    ui.text_centered(
                        msg,
                        dr.x + dr.w / 2.0,
                        dr.y + dr.h / 2.0 - 8.0,
                        14.0,
                        MUTED,
                        false,
                    );
                }
            }
        }
        action
    }

    /// The list; the notification clicked, if any.
    fn draw_list(
        &mut self,
        ui: &mut Ui,
        area: Rect,
        list: &[Notification],
        now: i64,
    ) -> Option<i64> {
        let visible = (area.h / ROW_H).floor().max(1.0) as usize;
        self.first = ui.scroll_list(area, self.first, list.len(), visible);
        let scrolls = list.len() > visible;
        let mut picked = None;
        for (k, n) in list.iter().skip(self.first).take(visible).enumerate() {
            let w = area.w - if scrolls { 12.0 } else { 0.0 };
            let r = Rect::new(area.x, area.y + k as f32 * ROW_H, w, ROW_H - 6.0);
            let hover = ui.hovering(r);
            let chosen = self.selected == Some(n.id);
            let alpha = if chosen {
                0.10
            } else if hover {
                0.07
            } else if n.unread() {
                0.055
            } else {
                0.03
            };
            rrect(r, 6.0, Color::new(1.0, 1.0, 1.0, alpha));
            let color = category_color(n.category());
            if chosen {
                rrect_lines(r, 6.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.7));
            }
            // The category's colour down the left edge.
            rrect(Rect::new(r.x, r.y + 6.0, 3.0, r.h - 12.0), 1.5, color);
            let (title, body) = wording(n);
            let x = r.x + 16.0;
            // The date on the right, a pin or "important" left of it.
            let d = crate::profile::date(n.created_at);
            let dw = ui.text_size(&d, 12.0, false).x;
            let mut right = r.x + r.w - 10.0 - dw;
            ui.text(&d, right, r.y + 9.0, 12.0, MUTED, false);
            right -= 8.0;
            if n.pinned(now) {
                let label = t("Pinned");
                let pw = ui.text_size(label, 11.0, true).x + 12.0;
                right -= pw;
                let p = Rect::new(right, r.y + 8.0, pw, 17.0);
                rrect(p, 8.5, Color::new(color.r, color.g, color.b, 0.22));
                ui.text_centered(label, p.x + pw / 2.0, p.y + 2.5, 11.0, color, true);
                right -= 6.0;
            }
            if n.unread() {
                draw_circle(
                    x + 4.0,
                    r.y + 17.0,
                    4.0,
                    if n.important { crate::ui::ALERT } else { GOLD },
                );
            }
            let tx = if n.unread() { x + 14.0 } else { x };
            let (title, size) = ui.fitted(&title, 15.0, right - tx - 4.0, n.unread());
            ui.text(
                &title,
                tx,
                r.y + 9.0,
                size,
                if n.unread() { CREAM } else { MUTED },
                n.unread(),
            );
            // What it says, on one line: the game's words, else the staff's.
            let sub = if !body.is_empty() { body } else { n.text.clone() };
            let sub = ui.ellipsize(&sub.replace('\n', " "), r.w - 26.0, 13.0);
            ui.text(&sub, x, r.y + 32.0, 13.0, MUTED, false);
            if hover && ui.clicked(r) {
                ui.note_click();
                picked = Some(n.id);
            }
        }
        picked
    }

    /// A notification in full, with its buttons; what its button asks for.
    fn draw_detail(
        &mut self,
        ui: &mut Ui,
        online: &mut Online,
        area: Rect,
        n: &Notification,
        now: i64,
    ) -> Option<InboxAction> {
        rrect(area, 8.0, Color::new(1.0, 1.0, 1.0, 0.03));
        rrect_lines(area, 8.0, 1.0, BORDER);
        let pad = 18.0;
        let inner = area.w - pad * 2.0;
        let color = category_color(n.category());
        let (title, body) = wording(n);
        let mut y = area.y + pad;
        let (title, size) = ui.fitted(&title, 20.0, inner, true);
        ui.text(&title, area.x + pad, y, size, color, true);
        y += 30.0;
        // The kind, who sent it and when.
        let mut meta = vec![category_name(Some(n.category())).to_string()];
        if !n.from.is_empty() {
            meta.push(tw("From {name}", &[("name", &n.from)]));
        }
        meta.push(crate::profile::date(n.created_at));
        let meta = ui.ellipsize(&meta.join(" · "), inner, 13.0);
        ui.text(&meta, area.x + pad, y, 13.0, MUTED, false);
        y += 22.0;
        if n.pinned(now) {
            let left = if n.pinned_until >= i64::MAX / 2 {
                t("Pinned").to_string()
            } else {
                tw("Pinned for {t} more", &[("t", &crate::online::time_left(n.pinned_until - now))])
            };
            ui.text(&left, area.x + pad, y, 12.0, color, true);
            y += 20.0;
        }
        y += 6.0;
        // The buttons at the bottom; the text above them.
        let buttons_y = area.y + area.h - pad - 38.0;
        let text_room = buttons_y - 12.0 - y;
        let mut lines: Vec<(String, bool)> = Vec::new();
        for l in ui.wrap(&body, inner, 15.0) {
            lines.push((l, false));
        }
        if !n.text.is_empty() {
            if !lines.is_empty() {
                lines.push((String::new(), false));
            }
            for l in ui.wrap(&n.text, inner - 20.0, 15.0) {
                lines.push((l, true));
            }
        }
        let fit = ((text_room / 21.0).floor().max(1.0)) as usize;
        if lines.len() > fit {
            lines.truncate(fit);
            if let Some((last, _)) = lines.last_mut() {
                *last = ui.ellipsize(&format!("{last} …"), inner - 20.0, 15.0);
            }
        }
        // What the staff wrote: in a quote, as written.
        let quoted = lines.iter().filter(|(_, q)| *q).count();
        if quoted > 0 {
            let first = lines.iter().position(|(_, q)| *q).unwrap_or(0);
            let qr = Rect::new(
                area.x + pad,
                y + first as f32 * 21.0 - 6.0,
                inner,
                quoted as f32 * 21.0 + 12.0,
            );
            rrect(qr, 6.0, Color::new(1.0, 1.0, 1.0, 0.04));
            rrect(
                Rect::new(qr.x, qr.y + 4.0, 3.0, qr.h - 8.0),
                1.5,
                Color::new(color.r, color.g, color.b, 0.7),
            );
        }
        for (k, (l, quote)) in lines.iter().enumerate() {
            let x = area.x + pad + if *quote { 12.0 } else { 0.0 };
            let c = if *quote { crate::online::STATUS_COLOR } else { CREAM };
            ui.text(l, x, y + k as f32 * 21.0, 15.0, c, false);
        }
        // The buttons: what it leads to, "I read it" for an important one
        // not confirmed, and delete.
        let mut x = area.x + pad;
        let mut action = None;
        if let Some(label) = action_label(&n.action) {
            let bw = ui.text_size(label, 15.0, true).x + 36.0;
            if ui.primary_button(Rect::new(x, buttons_y, bw, 38.0), label) {
                action = self.follow(online, &n.action);
                online.inbox_read(vec![n.id]);
            }
            x += bw + 10.0;
        }
        if n.important && n.unread() {
            let label = t("I read it");
            let bw = ui.text_size(label, 15.0, true).x + 36.0;
            if ui.button(Rect::new(x, buttons_y, bw, 38.0), label, true) {
                online.inbox_read(vec![n.id]);
            }
        }
        let del = t("Delete");
        let dw = ui.text_size(del, 14.0, true).x + 28.0;
        let dr = Rect::new(area.x + area.w - pad - dw, buttons_y, dw, 38.0);
        let can = Online::inbox_deletable(n);
        if ui.button(dr, del, can) {
            online.inbox_delete(vec![n.id]);
            self.selected = None;
        }
        if !can && ui.hovering(dr) {
            let why = if n.important && n.unread() {
                t("Confirm you read it first.")
            } else {
                t("It stays while it is pinned.")
            };
            ui.tooltip(why, dr);
        }
        action
    }

    /// Follows a notification's button: what the game can do here (watch a
    /// game, equip an item), or where it goes.
    fn follow(&mut self, online: &mut Online, a: &Action) -> Option<InboxAction> {
        match a {
            Action::Replay { match_id } => {
                online.send(ClientMsg::Replay { match_id: *match_id });
                None
            }
            Action::Equip { code } => {
                online.send(ClientMsg::Equip { code: code.clone() });
                online.toast(t("Equipped."), true);
                None
            }
            Action::Collection { code } => Some(InboxAction::Collection(code.clone())),
            Action::Tournament { id } => {
                online.open_tournament(*id);
                Some(InboxAction::Tournaments)
            }
            Action::Profile { username } => Some(InboxAction::Profile(username.clone())),
            Action::Shop { code } => Some(InboxAction::Shop(code.clone())),
            Action::Auctions => Some(InboxAction::Auctions),
            Action::Vip => Some(InboxAction::Vip),
            Action::None | Action::Other => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snook_legend_proto::inbox::Kind;

    fn n(id: i64, kind: Kind, created_at: i64, pinned_until: i64) -> Notification {
        Notification {
            id,
            kind,
            text: String::new(),
            from: String::new(),
            created_at,
            read_at: 0,
            important: false,
            pinned_until,
            action: Action::None,
        }
    }

    #[test]
    fn pinned_first_then_newest_and_filtered() {
        let items = vec![
            n(1, Kind::Warning, 10, 1000),
            n(2, Kind::Achievement { code: "x".into() }, 30, 0),
            n(3, Kind::Unmuted, 20, 0),
            n(4, Kind::Warning, 5, 50),
        ];
        let ids = |l: Vec<Notification>| l.iter().map(|n| n.id).collect::<Vec<_>>();
        assert_eq!(ids(ordered(&items, None, 100)), [1, 2, 3, 4], "the pin of 4 is over");
        assert_eq!(ids(ordered(&items, None, 40)), [1, 4, 2, 3]);
        assert_eq!(ids(ordered(&items, Some(Category::Staff), 100)), [1, 4]);
        assert_eq!(ids(ordered(&items, Some(Category::Punishment), 100)), [3]);
    }
}
