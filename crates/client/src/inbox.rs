//! The inbox's screen: the private messages (the conversations, the one
//! picked with the field to answer, and a new message to anyone), and what
//! the staff and the game told the player, newest first with the pinned
//! ones on top (a warning, a mute running), filtered by kind with the
//! unread count on each filter, and the one picked shown in full with its
//! button ("See the game", "Equip now"…). The notifications themselves are
//! kept by `online::inbox`, the messages by `dm`.

use macroquad::prelude::*;
use snook_legend_proto::ClientMsg;
use snook_legend_proto::dm::DmThread;
use snook_legend_proto::inbox::{Action, Category, Notification};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle};

use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::online::{Online, action_label, category_color, category_name, wording};
use crate::ranking::{HEADER_H, PAD, empty_state, loading, screen_header_until};
use crate::ui::{BORDER, CREAM, GOLD, MUTED, Ui, motion, rrect, rrect_lines};

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
    /// The private messages are shown (not the notifications).
    messages: bool,
    /// A new message: the name being typed.
    composing: Option<String>,
    /// The first conversation of the list on screen.
    threads_first: crate::ui::Scroll,
    /// The filter (None: all).
    filter: Option<Category>,
    /// The notification shown in full.
    selected: Option<i64>,
    /// The first row of the list on screen.
    first: crate::ui::Scroll,
}

const ROW_H: f32 = 62.0;
const TABS_H: f32 = 44.0;
/// The new message's name field.
const TO_FIELD: u64 = 9_312;

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
    /// Opens on everything, asking the server for the newest (on the
    /// messages when some are unread).
    pub fn open(&mut self, online: &mut Online) {
        self.messages = online.dms.total_unread() > 0;
        self.composing = None;
        self.filter = None;
        self.selected = None;
        self.first.reset();
        self.threads_first.reset();
        online.request_inbox();
    }

    /// Opens on the messages, with the conversation with `name` when given.
    pub fn open_messages(&mut self, online: &mut Online, name: Option<&str>) {
        self.open(online);
        self.messages = true;
        if let Some(name) = name {
            online.open_dm(name);
        }
    }

    pub fn draw(&mut self, ui: &mut Ui, online: &mut Online) -> InboxAction {
        let (sw, sh) = (crate::ui::vw(), crate::ui::vh());
        ui.screen_dim(0.5);
        let w = (sw - 40.0).min(980.0);
        let h = (sh - 40.0).min(760.0);
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        ui.panel(r);
        let now = crate::game::unix_now();
        let unread = online.inbox_unread() + online.dms.total_unread();
        let subtitle = if unread > 0 {
            tw("{n} unread", &[("n", &unread)])
        } else {
            t("Everything read").to_string()
        };
        // On the right of the header: "New message" over the messages,
        // "Mark all as read" over the notifications.
        let right_label = if self.messages { t("New message") } else { t("Mark all as read") };
        let bw = ui.text_size(right_label, 13.0, true).x + 24.0;
        let all_r = Rect::new(r.x + r.w - PAD - bw, r.y + 22.0, bw, 32.0);
        let mut action = InboxAction::None;
        // (Esc with a field focused is the field's.)
        if screen_header_until(ui, r, Icon::List, t("Inbox"), Some(&subtitle), all_r.x - 12.0)
            || (ui.escape() && ui.focus.is_none())
        {
            let reading = online.dms.open.is_some() || self.composing.is_some();
            if self.messages && reading && w < 760.0 {
                self.composing = None;
                online.dms.close(ui);
            } else if !self.messages && self.selected.is_some() && w < 760.0 {
                self.selected = None;
            } else {
                action = InboxAction::Back;
            }
        }
        if self.messages {
            if ui.button(all_r, right_label, online.signed_in()) {
                online.dms.close(ui);
                self.composing = Some(String::new());
                ui.focus = Some(TO_FIELD);
            }
        } else {
            // Everything read at once (not the important ones: each is
            // confirmed).
            let plain: Vec<i64> = online
                .inbox_items()
                .iter()
                .filter(|n| n.unread() && !n.important)
                .map(|n| n.id)
                .collect();
            if ui.button(all_r, right_label, !plain.is_empty()) {
                online.inbox_read(plain);
            }
        }
        // The messages, then the filters, each with what isn't read in it.
        let row = Rect::new(r.x + PAD, r.y + HEADER_H, r.w - PAD * 2.0, 34.0);
        let mut labels: Vec<&str> = vec![t("Messages")];
        labels.extend(FILTERS.iter().map(|c| category_name(*c)));
        let dm_unread = online.dms.total_unread();
        let mut counts: Vec<String> =
            vec![if dm_unread > 0 { dm_unread.to_string() } else { String::new() }];
        counts.extend(FILTERS.iter().map(|c| {
            let n = online
                .inbox_items()
                .iter()
                .filter(|x| x.unread() && c.is_none_or(|c| x.category() == c))
                .count();
            if n > 0 { n.to_string() } else { String::new() }
        }));
        let counts: Vec<&str> = counts.iter().map(String::as_str).collect();
        let tab = |s: &Self| {
            if s.messages {
                0
            } else {
                1 + FILTERS.iter().position(|c| *c == s.filter).unwrap_or(0)
            }
        };
        let (clicked, _) = crate::tabrow::tabs(ui, row, &labels, &counts, tab(self), 6.0);
        if let Some(k) = clicked {
            self.messages = k == 0;
            if k > 0 {
                self.filter = FILTERS[k - 1];
                self.first.reset();
            }
        }
        // Another tab: the list slides in from its side.
        let _switch = ui.switch_scope(motion::id("inbox-filter"), tab(self) as i64, 16.0);
        let body = Rect::new(
            r.x + PAD,
            r.y + HEADER_H + TABS_H,
            r.w - PAD * 2.0,
            r.h - HEADER_H - TABS_H - PAD,
        );
        if self.messages {
            return match self.draw_messages(ui, online, body) {
                InboxAction::None => action,
                a => a,
            };
        }
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
                    // Each notification picked comes in.
                    let key = motion::mix(motion::id("inbox-detail"), n.id as u64);
                    let fx = ui.come_in(key, vec2(12.0, 0.0));
                    if let Some(a) = self.draw_detail(ui, online, dr, n, now) {
                        action = a;
                    }
                    ui.pop_fx(fx);
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

    /// The private messages: the conversations, and the one picked (or a
    /// new message) beside them, or in their place when there is no room.
    fn draw_messages(&mut self, ui: &mut Ui, online: &mut Online, body: Rect) -> InboxAction {
        let threads: Vec<DmThread> = online.dms.threads().to_vec();
        let open = online.dms.open.clone();
        if !online.signed_in() {
            empty_state(ui, body, Icon::List, t("Sign in to send and read messages."));
            return InboxAction::None;
        }
        if threads.is_empty() && open.is_none() && self.composing.is_none() {
            let text = t(
                "No messages yet. Write to a player from their profile, or start one with New message.",
            );
            empty_state(ui, body, Icon::List, text);
            return InboxAction::None;
        }
        let showing = open.is_some() || self.composing.is_some();
        let (list_r, detail_r) = if body.w >= 700.0 {
            let lw = (body.w * 0.38).max(280.0);
            (
                Some(Rect::new(body.x, body.y, lw, body.h)),
                Some(Rect::new(body.x + lw + 16.0, body.y, body.w - lw - 16.0, body.h)),
            )
        } else if showing {
            (None, Some(body))
        } else {
            (Some(body), None)
        };
        if let Some(lr) = list_r
            && let Some(name) = self.draw_threads(ui, online, lr, &threads, open.as_deref())
        {
            self.composing = None;
            online.open_dm(&name);
        }
        let Some(dr) = detail_r else { return InboxAction::None };
        if let Some(name) = self.composing.as_mut() {
            // A new message: to whom.
            rrect(dr, 8.0, Color::new(1.0, 1.0, 1.0, 0.03));
            rrect_lines(dr, 8.0, 1.0, BORDER);
            let pad = 18.0;
            ui.text(t("New message"), dr.x + pad, dr.y + pad, 18.0, GOLD, true);
            let hint = t("Write the name of the player to write to.");
            let hint = ui.ellipsize(hint, dr.w - pad * 2.0, 13.0);
            ui.text(&hint, dr.x + pad, dr.y + pad + 30.0, 13.0, MUTED, false);
            let field =
                Rect::new(dr.x + pad, dr.y + pad + 56.0, (dr.w - pad * 2.0).min(360.0), 40.0);
            let enter = ui.text_input(TO_FIELD, field, name, snook_legend_proto::MAX_NAME_LEN);
            crate::online::placeholder(ui, TO_FIELD, field, name, t("Player's name"));
            let go = t("Write");
            let gw = ui.text_size(go, 15.0, true).x + 36.0;
            let can = !name.trim().is_empty();
            let go_r = Rect::new(dr.x + pad, field.y + field.h + 12.0, gw, 38.0);
            let cancel = t("Cancel");
            let cw = ui.text_size(cancel, 14.0, true).x + 28.0;
            let cancel_r = Rect::new(go_r.x + gw + 10.0, go_r.y, cw, 38.0);
            if (ui.primary_button(go_r, go) || enter) && can {
                let name = name.trim().to_string();
                self.composing = None;
                if ui.focus == Some(TO_FIELD) {
                    ui.focus = None;
                }
                online.open_dm(&name);
            } else if ui.button(cancel_r, cancel, true) {
                self.composing = None;
            }
            return InboxAction::None;
        }
        let Some(with) = open else {
            rrect(dr, 8.0, Color::new(1.0, 1.0, 1.0, 0.025));
            let msg = t("Pick a conversation to read it.");
            ui.text_centered(msg, dr.x + dr.w / 2.0, dr.y + dr.h / 2.0 - 8.0, 14.0, MUTED, false);
            return InboxAction::None;
        };
        // The conversation: who with (their profile a click away), then
        // the messages and the field.
        let key = motion::mix(motion::id("inbox-dm"), motion::id(&with.to_lowercase()));
        let fx = ui.come_in(key, vec2(12.0, 0.0));
        rrect(dr, 8.0, Color::new(1.0, 1.0, 1.0, 0.03));
        rrect_lines(dr, 8.0, 1.0, BORDER);
        let mut action = InboxAction::None;
        let profile = t("Profile");
        let pw = ui.text_size(profile, 13.0, true).x + 24.0;
        let pr = Rect::new(dr.x + dr.w - 12.0 - pw, dr.y + 10.0, pw, 30.0);
        let (title, size) = ui.fitted(&with, 18.0, pr.x - dr.x - 28.0, true);
        ui.text(&title, dr.x + 14.0, dr.y + 15.0, size, CREAM, true);
        if ui.button(pr, profile, true) {
            action = InboxAction::Profile(with.clone());
        }
        let inner = Rect::new(dr.x + 10.0, dr.y + 50.0, dr.w - 20.0, dr.h - 60.0);
        online.draw_conversation(ui, inner);
        ui.pop_fx(fx);
        action
    }

    /// The conversations, newest first; the player picked, if any.
    fn draw_threads(
        &mut self,
        ui: &mut Ui,
        online: &Online,
        area: Rect,
        threads: &[DmThread],
        open: Option<&str>,
    ) -> Option<String> {
        if threads.is_empty() {
            rrect(area, 8.0, Color::new(1.0, 1.0, 1.0, 0.025));
            let msg = t("No conversations yet.");
            ui.text_centered(msg, area.x + area.w / 2.0, area.y + 24.0, 13.0, MUTED, false);
            return None;
        }
        let me = online.me.as_ref().map(|m| m.username.clone()).unwrap_or_default();
        let view = ui.list_begin(area, &mut self.threads_first, threads.len(), ROW_H);
        let scrolls = view.scrolls();
        let mut picked = None;
        for k in view.rows() {
            let th = &threads[k];
            let w = area.w - if scrolls { 12.0 } else { 0.0 };
            let r = Rect::new(area.x, view.y(k), w, ROW_H - 6.0);
            let hover = ui.hovering(r);
            let chosen = open.is_some_and(|o| o.eq_ignore_ascii_case(&th.with));
            let unread = th.unread > 0;
            let rest = if unread { 0.055 } else { 0.03 };
            let lit = ui.hover_anim(r, hover);
            let alpha = if chosen { 0.10 } else { rest + (0.07 - rest) * lit };
            rrect(r, 6.0, Color::new(1.0, 1.0, 1.0, alpha));
            if chosen {
                rrect_lines(r, 6.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.7));
            }
            let x = r.x + 12.0;
            // The date on the right; the count of unread under it.
            let d = crate::profile::date(th.last.at);
            let dw = ui.text_size(&d, 12.0, false).x;
            ui.text(&d, r.x + r.w - 10.0 - dw, r.y + 9.0, 12.0, MUTED, false);
            if unread {
                let n = if th.unread > 9 { "9+".to_string() } else { th.unread.to_string() };
                let bw = (ui.text_size(&n, 10.0, true).x + 8.0).max(16.0);
                let b = Rect::new(r.x + r.w - 10.0 - bw, r.y + 30.0, bw, 16.0);
                rrect(b, 8.0, crate::ui::ALERT);
                ui.text_centered(
                    &n,
                    b.x + bw / 2.0,
                    b.y + 2.0,
                    10.0,
                    Color::new(0.1, 0.05, 0.03, 1.0),
                    true,
                );
            }
            let (name, size) = ui.fitted(&th.with, 15.0, r.w - dw - 40.0, true);
            ui.text(&name, x, r.y + 9.0, size, if unread { CREAM } else { MUTED }, true);
            // The newest message, on one line (a quick phrase in this
            // player's language).
            let text = if snook_legend_proto::is_quick_phrase(&th.last.text) {
                t(&th.last.text).to_string()
            } else {
                th.last.text.replace('\n', " ")
            };
            let line = if th.last.from.eq_ignore_ascii_case(&me) {
                tw("You: {text}", &[("text", &text)])
            } else {
                text
            };
            let line = ui.ellipsize(&line, r.w - 60.0, 13.0);
            ui.text(&line, x, r.y + 32.0, 13.0, MUTED, false);
            if ui.activated(r, hover) {
                ui.note_click();
                picked = Some(th.with.clone());
            }
        }
        ui.scroll_end(view, &mut self.threads_first);
        picked
    }

    /// The list; the notification clicked, if any.
    fn draw_list(
        &mut self,
        ui: &mut Ui,
        area: Rect,
        list: &[Notification],
        now: i64,
    ) -> Option<i64> {
        let view = ui.list_begin(area, &mut self.first, list.len(), ROW_H);
        let scrolls = view.scrolls();
        let mut picked = None;
        for k in view.rows() {
            let n = &list[k];
            let w = area.w - if scrolls { 12.0 } else { 0.0 };
            let r = Rect::new(area.x, view.y(k), w, ROW_H - 6.0);
            let hover = ui.hovering(r);
            let chosen = self.selected == Some(n.id);
            let rest = if n.unread() { 0.055 } else { 0.03 };
            let lit = ui.hover_anim(r, hover);
            let alpha = if chosen { 0.10 } else { rest + (0.07 - rest) * lit };
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
            if ui.activated(r, hover) {
                ui.note_click();
                picked = Some(n.id);
            }
        }
        ui.scroll_end(view, &mut self.first);
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
