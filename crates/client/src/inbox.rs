//! The inbox's screen, laid out as the messaging apps people know: two
//! sections, the private messages and the notifications, each a list on the
//! left and the one picked on the right (or one at a time when there is no
//! room).
//!
//! The messages: the conversations (the other player's picture, whether
//! they are online, the newest message with how far it got, the unread
//! count), a search over them, a new message to a friend or anyone, and the
//! conversation picked (see `dm`). The notifications: what the staff and
//! the game told the player, newest first with the pinned ones on top,
//! filtered by kind with the unread count on each filter, and the one
//! picked in full with its buttons ("See the game", "Equip now", "See it
//! in your profile"…). The notifications are kept by `online::inbox`, the
//! messages by `dm`.

use macroquad::prelude::*;
use snook_legend_proto::ClientMsg;
use snook_legend_proto::dm::DmThread;
use snook_legend_proto::inbox::{Action, Category, Kind, Notification};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle};

use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::online::{Online, action_label, category_color, category_name, wording};
use crate::ranking::{HEADER_H, PAD, screen_header_until, text_y};
use crate::ui::{BORDER, CREAM, GOLD, HAIRLINE, MUTED, Ui, motion, rrect, rrect_lines};

/// What the inbox asks of the rest of the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboxAction {
    None,
    Back,
    Profile(String),
    /// This player's profile, on an achievement's card.
    Achievement(String),
    /// The shop (on an item when not empty).
    Shop(String),
    /// The shop's VIP page.
    Vip,
    /// The lounge, on its tournaments (the one picked is shown).
    Tournaments,
    /// The collection, on this item.
    Collection(String),
    /// The shop's auctions.
    Auctions,
}

/// The inbox's screen state.
#[derive(Default)]
pub struct InboxScreen {
    /// The private messages are shown (not the notifications).
    messages: bool,
    /// A new message: the name being typed.
    composing: Option<String>,
    /// The conversations' search.
    search: String,
    /// The first conversation of the list on screen.
    threads_first: crate::ui::Scroll,
    /// The friends to write to (a new message).
    friends_first: crate::ui::Scroll,
    /// The filter (None: all).
    filter: Option<Category>,
    /// The notification shown in full.
    selected: Option<i64>,
    /// The first row of the list on screen.
    first: crate::ui::Scroll,
    /// Development aid (screenshots): the first achievement's notice is
    /// picked once the list comes.
    pick_achievement: bool,
}

const THREAD_H: f32 = 70.0;
const NOTE_H: f32 = 66.0;
/// The new message's name field, and the search.
const TO_FIELD: u64 = 9_312;
const SEARCH_FIELD: u64 = 9_313;
/// Two panes side by side from this width.
const WIDE: f32 = 700.0;

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

/// The icon of a category's notifications.
fn category_icon(c: Category) -> Icon {
    match c {
        Category::Staff => Icon::Megaphone,
        Category::Punishment => Icon::Gavel,
        Category::Gift => Icon::Gift,
        Category::Tournament => Icon::Trophy,
        Category::Achievement => Icon::Medal,
        Category::Game => Icon::Bell,
    }
}

/// The achievement a notification is about.
fn achievement_of(n: &Notification) -> Option<&'static snook_legend_proto::Achievement> {
    match &n.kind {
        Kind::Achievement { code } => {
            snook_legend_proto::ACHIEVEMENTS.iter().find(|a| a.code == code)
        }
        _ => None,
    }
}

/// A notification as its row reads: what it is about first (the
/// achievement's name, the item won), then what kind of news it is.
fn headline(n: &Notification) -> (String, String) {
    let (title, body) = wording(n);
    let lead = matches!(
        n.kind,
        Kind::Achievement { .. } | Kind::ItemAwarded { .. } | Kind::VipItem { .. }
    );
    if lead && !body.is_empty() {
        (body, title)
    } else {
        let sub = if !body.is_empty() { body } else { n.text.clone() };
        (title, sub.replace('\n', " "))
    }
}

/// A round mark of a category: its icon on its colour, or the medal of an
/// achievement.
fn badge(n: &Notification, c: Vec2, r: f32) {
    let category = n.category();
    if let Some(a) = achievement_of(n) {
        let metal = crate::profile::medal_color(a.level);
        draw_circle(c.x, c.y, r, Color::new(metal.r, metal.g, metal.b, 0.14));
        crate::profile::medal_art(c, r * 1.75, a.level, false);
        return;
    }
    let color = category_color(category);
    draw_circle(c.x, c.y, r, Color::new(color.r, color.g, color.b, 0.16));
    crate::icons::draw(category_icon(category), c, r, color);
}

/// A small round count (unread), gold with dark figures, its right edge at
/// `right`; its width.
fn count_pill(ui: &Ui, n: u32, right: f32, mid: f32) -> f32 {
    let text = if n > 99 { "99+".to_string() } else { n.to_string() };
    let w = (ui.text_size(&text, 11.0, true).x + 10.0).max(20.0);
    let r = Rect::new(right - w, mid - 10.0, w, 20.0);
    rrect(r, 10.0, GOLD);
    ui.text_centered(&text, r.x + w / 2.0, text_y(mid, 11.0), 11.0, DARK, true);
    w
}

/// Figures on gold.
const DARK: Color = Color::new(0.1, 0.06, 0.03, 1.0);

impl InboxScreen {
    /// Opens on everything, asking the server for the newest (on the
    /// messages when some are unread, or when no notification is).
    pub fn open(&mut self, online: &mut Online) {
        self.messages = online.dms.total_unread() > 0 || online.inbox_unread() == 0;
        self.composing = None;
        self.search.clear();
        self.filter = None;
        self.selected = None;
        self.first.reset();
        self.threads_first.reset();
        // Development aid (screenshots): SNOOK_LEGEND_INBOX=notes opens on
        // the notifications, =achievement with an achievement's notice picked.
        match crate::dev::var("SNOOK_LEGEND_INBOX").as_deref() {
            Some("notes") => self.messages = false,
            Some("achievement") => {
                self.messages = false;
                self.pick_achievement = true;
            }
            _ => {}
        }
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
        let w = (sw - 40.0).min(1040.0);
        let h = (sh - 40.0).min(780.0);
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        ui.panel(r);
        let now = crate::game::unix_now();
        let notes_unread = online.inbox_unread();
        let dm_unread = online.dms.total_unread();
        let unread = notes_unread + dm_unread;
        let subtitle = if unread > 0 {
            tw("{n} unread", &[("n", &unread)])
        } else {
            t("Everything read").to_string()
        };
        // The two sections, on the right of the header.
        let sections = [
            (Icon::Chat, t("Messages"), dm_unread),
            (Icon::Bell, t("Notifications"), notes_unread),
        ];
        let cell_w: Vec<f32> = sections
            .iter()
            .map(|(_, label, n)| {
                let count = if *n > 0 {
                    ui.text_size(&(*n).min(99).to_string(), 11.0, true).x + 18.0
                } else {
                    0.0
                };
                44.0 + ui.text_size(label, 14.0, true).x + count + 14.0
            })
            .collect();
        let switch_w: f32 = cell_w.iter().sum::<f32>() + 8.0;
        let switch = Rect::new(r.x + r.w - PAD - switch_w, r.y + 18.0, switch_w, 40.0);
        let mut action = InboxAction::None;
        // (Esc with a field focused, or the emoji open, is theirs.)
        let narrow = w - 2.0 * PAD < WIDE;
        if screen_header_until(ui, r, Icon::Envelope, t("Inbox"), Some(&subtitle), switch.x - 16.0)
            || (ui.escape() && ui.focus.is_none())
        {
            let reading = online.dms.open.is_some() || self.composing.is_some();
            if self.messages && reading && narrow {
                self.composing = None;
                online.dms.close(ui);
            } else if !self.messages && self.selected.is_some() && narrow {
                self.selected = None;
            } else {
                action = InboxAction::Back;
            }
        }
        if let Some(k) = self.section_switch(ui, switch, &sections, &cell_w) {
            self.messages = k == 0;
        }
        let body = Rect::new(r.x + PAD, r.y + HEADER_H, r.w - PAD * 2.0, r.h - HEADER_H - PAD);
        // The other section slides in from its side.
        let _switch = ui.switch_scope(motion::id("inbox-section"), self.messages as i64, 16.0);
        let a = if self.messages {
            self.draw_messages(ui, online, body)
        } else {
            self.draw_notifications(ui, online, body, now)
        };
        if a != InboxAction::None {
            action = a;
        }
        action
    }

    /// The two sections: an icon, the name and the unread count each, the
    /// one shown lit. The one clicked, if any.
    fn section_switch(
        &mut self,
        ui: &mut Ui,
        r: Rect,
        sections: &[(Icon, &str, u32); 2],
        widths: &[f32],
    ) -> Option<usize> {
        ui.block(r);
        rrect(r, 20.0, Color::new(0.05, 0.035, 0.025, 1.0));
        rrect_lines(r, 20.0, 1.0, HAIRLINE);
        let selected = if self.messages { 0 } else { 1 };
        let mut x = r.x + 4.0;
        let cells: Vec<Rect> = widths
            .iter()
            .map(|w| {
                let c = Rect::new(x, r.y + 4.0, *w, r.h - 8.0);
                x += w;
                c
            })
            .collect();
        let plate = ui.glide_rect(motion::id("inbox-section-plate"), cells[selected], 22.0);
        let bg = Color::new(0.36, 0.245, 0.12, 1.0);
        crate::ui::rrect_gradient(
            plate,
            plate.h / 2.0,
            crate::ui::shade(bg, 0.1),
            crate::ui::shade(bg, -0.15),
        );
        let mut clicked = None;
        let group = motion::id("inbox-sections");
        for (k, ((icon, label, n), cell)) in sections.iter().zip(&cells).enumerate() {
            let hover = ui.hovering(*cell);
            let on = k == selected;
            let lit = ui.hover_anim(*cell, hover && !on);
            if lit > 0.01 {
                rrect(*cell, cell.h / 2.0, Color::new(1.0, 0.9, 0.75, 0.06 * lit));
            }
            let color = if on { GOLD } else { crate::ui::mix(MUTED, CREAM, lit) };
            let mid = cell.y + cell.h / 2.0;
            let ic = vec2(cell.x + 22.0, mid);
            if on {
                crate::icons::draw_filled(*icon, ic, 18.0, color);
            } else {
                crate::icons::draw(*icon, ic, 18.0, color);
            }
            ui.text(label, cell.x + 38.0, text_y(mid, 14.0), 14.0, color, true);
            if *n > 0 {
                count_pill(ui, *n, cell.x + cell.w - 10.0, mid);
            }
            let hit = ui.in_group(group, false, on, |ui| ui.activated(*cell, hover));
            if hit && !on {
                ui.note_click();
                clicked = Some(k);
            }
        }
        clicked
    }

    /// The private messages: the conversations, and the one picked (or a
    /// new message) beside them, or in their place when there is no room.
    fn draw_messages(&mut self, ui: &mut Ui, online: &mut Online, body: Rect) -> InboxAction {
        let threads: Vec<DmThread> = online.dms.threads().to_vec();
        let open = online.dms.open.clone();
        if !online.signed_in() {
            let text = t("Sign in to send and read messages.");
            crate::online::empty_state(ui, body, crate::online::Glyph::Chat, text, None);
            return InboxAction::None;
        }
        let showing = open.is_some() || self.composing.is_some();
        let (list_r, detail_r) = if body.w >= WIDE {
            let lw = (body.w * 0.36).clamp(290.0, 370.0);
            (
                Some(Rect::new(body.x, body.y, lw, body.h)),
                Some(Rect::new(body.x + lw + 14.0, body.y, body.w - lw - 14.0, body.h)),
            )
        } else if showing {
            (None, Some(body))
        } else {
            (Some(body), None)
        };
        if let Some(lr) = list_r {
            match self.draw_threads(ui, online, lr, &threads, open.as_deref()) {
                Some(Pick::Thread(name)) => {
                    self.composing = None;
                    online.open_dm(&name);
                }
                Some(Pick::Compose) => self.compose(ui, online),
                None => {}
            }
        }
        let Some(dr) = detail_r else { return InboxAction::None };
        rrect(dr, 10.0, Color::new(1.0, 1.0, 1.0, 0.025));
        rrect_lines(dr, 10.0, 1.0, BORDER);
        if self.composing.is_some() {
            return self.draw_compose(ui, online, dr);
        }
        let Some(with) = open else {
            let text = if threads.is_empty() {
                t("No messages yet. Write to a player from their profile, or start one here.")
            } else {
                t("Pick a conversation to read it, or start a new one.")
            };
            let label = t("New message");
            if crate::online::empty_state(ui, dr, crate::online::Glyph::Chat, text, Some(label)) {
                self.compose(ui, online);
            }
            return InboxAction::None;
        };
        // The conversation picked comes in.
        let key = motion::mix(motion::id("inbox-dm"), motion::id(&with.to_lowercase()));
        let fx = ui.come_in(key, vec2(12.0, 0.0));
        let inner = Rect::new(dr.x + 1.0, dr.y + 1.0, dr.w - 2.0, dr.h - 2.0);
        let mut action = InboxAction::None;
        if let crate::dm::DmAction::Profile(name) = online.draw_conversation(ui, inner) {
            action = InboxAction::Profile(name);
        }
        ui.pop_fx(fx);
        action
    }

    /// Starts a new message (the conversation open, if any, closes).
    fn compose(&mut self, ui: &mut Ui, online: &mut Online) {
        online.dms.close(ui);
        self.composing = Some(String::new());
        self.friends_first.reset();
        ui.focus = Some(TO_FIELD);
    }

    /// A new message: to whom (a friend from the list, or any name).
    fn draw_compose(&mut self, ui: &mut Ui, online: &mut Online, dr: Rect) -> InboxAction {
        let Some(name) = self.composing.as_mut() else { return InboxAction::None };
        let pad = 20.0;
        let x = dr.x + pad;
        let inner = dr.w - 2.0 * pad;
        let mid = dr.y + pad + 12.0;
        crate::icons::draw(Icon::Compose, vec2(x + 11.0, mid), 22.0, GOLD);
        ui.text(t("New message"), x + 32.0, text_y(mid, 18.0), 18.0, GOLD, true);
        let close = Rect::new(dr.x + dr.w - pad - 32.0, mid - 16.0, 32.0, 32.0);
        if ui.icon_button_ex(close, Icon::Close, t("Cancel"), true, true) {
            self.composing = None;
            return InboxAction::None;
        }
        let mut y = dr.y + pad + 44.0;
        let to = t("To:");
        let to_w = ui.text_size(to, 14.0, true).x;
        let field = Rect::new(x + to_w + 10.0, y, inner - to_w - 10.0, 40.0);
        ui.text(to, x, text_y(y + 20.0, 14.0), 14.0, MUTED, true);
        let enter = ui.text_input(TO_FIELD, field, name, snook_legend_proto::MAX_NAME_LEN);
        crate::online::placeholder(ui, TO_FIELD, field, name, t("Player's name"));
        y += 40.0 + 16.0;
        let typed = name.trim().to_string();
        let mut go: Option<String> = None;
        if enter && !typed.is_empty() {
            go = Some(typed.clone());
        }
        // The friends whose name has what was typed, online first.
        let query = typed.to_lowercase();
        let mut friends: Vec<&snook_legend_proto::FriendInfo> = online
            .friends
            .friends
            .iter()
            .filter(|f| query.is_empty() || f.player.username.to_lowercase().contains(&query))
            .collect();
        friends.sort_by_key(|f| (!f.online, f.player.username.to_lowercase()));
        let exact = friends.iter().any(|f| f.player.username.eq_ignore_ascii_case(&typed));
        // Anyone else, by the name typed.
        if !typed.is_empty() && !exact {
            let label = tw("Write to {name}", &[("name", &typed)]);
            let row = Rect::new(x - 6.0, y, inner + 12.0, 46.0);
            let hover = ui.hovering(row);
            let lit = ui.hover_anim(row, hover);
            rrect(row, 10.0, Color::new(1.0, 0.92, 0.8, 0.035 + 0.05 * lit));
            crate::icons::draw(Icon::Send, vec2(row.x + 24.0, row.y + 23.0), 17.0, GOLD);
            let (label, size) = ui.fitted(&label, 14.0, row.w - 60.0, true);
            ui.text(&label, row.x + 48.0, text_y(row.y + 23.0, size), size, CREAM, true);
            if ui.activated(row, hover) {
                go = Some(typed.clone());
            }
            y += 46.0 + 16.0;
        }
        if !friends.is_empty() {
            ui.text(&t("Friends").to_uppercase(), x, y, 11.5, MUTED, true);
            y += 22.0;
            let list = Rect::new(x - 6.0, y, inner + 12.0, dr.y + dr.h - pad - y);
            let row_h = 54.0;
            let view = ui.list_begin(list, &mut self.friends_first, friends.len(), row_h);
            let w = list.w - if view.scrolls() { 12.0 } else { 0.0 };
            let now = crate::game::unix_now();
            for k in view.rows() {
                let f = friends[k];
                let row = Rect::new(list.x, view.y(k), w, row_h - 4.0);
                let hover = ui.hovering(row);
                let lit = ui.hover_anim(row, hover);
                if lit > 0.01 {
                    rrect(row, 10.0, Color::new(1.0, 0.92, 0.8, 0.05 * lit));
                }
                let mid = row.y + row.h / 2.0;
                let pic = vec2(row.x + 10.0 + 18.0, mid);
                crate::avatars::draw(&f.player.avatar, pic, 18.0);
                if f.online {
                    crate::dm::presence_dot(pic + vec2(13.0, 13.0));
                }
                let nx = pic.x + 18.0 + 12.0;
                let room = row.w - (nx - row.x) - 12.0;
                ui.player_name(&f.player, nx, mid - 17.0, 14.5, room);
                let (sub, color) = if f.online {
                    (t("online").to_string(), crate::dm::ONLINE)
                } else if f.last_seen > 0 {
                    (crate::localtime::last_seen(f.last_seen, now), MUTED)
                } else {
                    (t("offline").to_string(), MUTED)
                };
                let sub = ui.ellipsize(&sub, room, 12.0);
                ui.text(&sub, nx, mid + 3.0, 12.0, color, false);
                if ui.activated(row, hover) {
                    go = Some(f.player.username.clone());
                }
            }
            ui.scroll_end(view, &mut self.friends_first);
        } else if typed.is_empty() {
            let hint = t("Write the name of the player to write to.");
            for (k, line) in ui.wrap(hint, inner, 13.0).iter().enumerate() {
                ui.text(line, x, y + k as f32 * 18.0, 13.0, MUTED, false);
            }
        }
        if let Some(name) = go {
            self.composing = None;
            if ui.focus == Some(TO_FIELD) {
                ui.focus = None;
            }
            online.open_dm(&name);
        }
        InboxAction::None
    }

    /// The conversations, newest first, under the search and New message;
    /// what was picked.
    fn draw_threads(
        &mut self,
        ui: &mut Ui,
        online: &Online,
        area: Rect,
        threads: &[DmThread],
        open: Option<&str>,
    ) -> Option<Pick> {
        let mut picked = None;
        // The search, and a new message beside it.
        let compose = Rect::new(area.x + area.w - 40.0, area.y, 40.0, 40.0);
        let field = Rect::new(area.x, area.y, area.w - 48.0, 40.0);
        ui.text_input(SEARCH_FIELD, field, &mut self.search, 32);
        if self.search.is_empty() && ui.focus != Some(SEARCH_FIELD) {
            let c = vec2(field.x + 18.0, field.y + 20.0);
            crate::icons::draw(Icon::Search, c, 15.0, MUTED);
            let hint = ui.ellipsize(t("Search conversations"), field.w - 44.0, 14.0);
            ui.text(&hint, field.x + 34.0, text_y(field.y + 20.0, 14.0), 14.0, MUTED, false);
        }
        if ui.icon_button(compose, Icon::Compose, t("New message"), true) {
            picked = Some(Pick::Compose);
        }
        let list = Rect::new(area.x, area.y + 50.0, area.w, area.h - 50.0);
        let query = self.search.trim().to_lowercase();
        let shown: Vec<&DmThread> = threads
            .iter()
            .filter(|th| query.is_empty() || th.with.to_lowercase().contains(&query))
            .collect();
        if shown.is_empty() {
            let (glyph, text) = if threads.is_empty() {
                (crate::online::Glyph::Chat, t("No conversations yet."))
            } else {
                (crate::online::Glyph::Icon(Icon::Search), t("No conversation with that name."))
            };
            crate::online::empty_state(ui, list, glyph, text, None);
            return picked;
        }
        let me = online.me.as_ref().map(|m| m.username.clone()).unwrap_or_default();
        let now = crate::game::unix_now();
        let view = ui.list_begin(list, &mut self.threads_first, shown.len(), THREAD_H);
        let scrolls = view.scrolls();
        for k in view.rows() {
            let th = shown[k];
            let w = list.w - if scrolls { 12.0 } else { 0.0 };
            let r = Rect::new(list.x, view.y(k), w, THREAD_H - 4.0);
            let hover = ui.hovering(r);
            let chosen = open.is_some_and(|o| o.eq_ignore_ascii_case(&th.with));
            let unread = th.unread > 0;
            let lit = ui.hover_anim(r, hover);
            let alpha = if chosen { 0.085 } else { 0.05 * lit };
            if alpha > 0.002 {
                rrect(r, 10.0, Color::new(1.0, 0.92, 0.8, alpha));
            }
            if chosen {
                rrect(Rect::new(r.x, r.y + 12.0, 3.0, r.h - 24.0), 1.5, GOLD);
            }
            let mid = r.y + r.h / 2.0;
            // The picture, with a dot when they are online.
            let peer = online.peer(&th.with);
            let pic_r = 22.0;
            let pic = vec2(r.x + 12.0 + pic_r, mid);
            let avatar = peer.player.as_ref().map_or("", |p| p.avatar.as_str());
            crate::avatars::draw(avatar, pic, pic_r);
            if peer.online == Some(true) {
                crate::dm::presence_dot(pic + vec2(pic_r * 0.72, pic_r * 0.72));
            }
            let x = pic.x + pic_r + 12.0;
            // The time on the right (gold with something unread), the count under it.
            let when = crate::localtime::when_short(th.last.at, now);
            let when_w = ui.text_size(&when, 11.5, unread).x;
            let tc = if unread { GOLD } else { MUTED };
            ui.text(&when, r.x + r.w - 12.0 - when_w, mid - 17.0, 11.5, tc, unread);
            let mut right = r.x + r.w - 12.0;
            if unread {
                right -= count_pill(ui, th.unread, right, mid + 10.0) + 8.0;
            }
            let name_w = r.x + r.w - 12.0 - when_w - 8.0 - x;
            match &peer.player {
                Some(p) => {
                    ui.player_name(p, x, mid - 19.0, 15.0, name_w);
                }
                None => {
                    let (name, size) = ui.fitted(&th.with, 15.0, name_w, true);
                    ui.text(&name, x, mid - 19.0, size, CREAM, true);
                }
            }
            // The newest message (or "typing…"), with how far it got when
            // it is this player's.
            let mut lx = x;
            let line_y = mid + 3.0;
            if online.dms.is_typing(&th.with) {
                ui.text(t("typing…"), lx, line_y, 13.0, GOLD, false);
            } else {
                if th.last.from.eq_ignore_ascii_case(&me) {
                    let tick =
                        online.dms.sending_to(&th.with).unwrap_or(crate::dm::Dms::tick(&th.last));
                    crate::dm::draw_tick(tick, vec2(lx, line_y + 2.0));
                    lx += 18.0;
                }
                let text = if snook_legend_proto::is_quick_phrase(&th.last.text) {
                    t(&th.last.text).to_string()
                } else {
                    th.last.text.replace('\n', " ")
                };
                let line = ui.ellipsize(&text, right - lx - 4.0, 13.0);
                ui.text(&line, lx, line_y, 13.0, if unread { CREAM } else { MUTED }, false);
            }
            if ui.activated(r, hover) {
                ui.note_click();
                picked = Some(Pick::Thread(th.with.clone()));
            }
        }
        ui.scroll_end(view, &mut self.threads_first);
        picked
    }

    /// The notifications: the filters, the list and the one picked.
    fn draw_notifications(
        &mut self,
        ui: &mut Ui,
        online: &mut Online,
        body: Rect,
        now: i64,
    ) -> InboxAction {
        let mut action = InboxAction::None;
        // "Mark all as read" (not the important ones: each is confirmed).
        let plain: Vec<i64> = online
            .inbox_items()
            .iter()
            .filter(|n| n.unread() && !n.important)
            .map(|n| n.id)
            .collect();
        let all = t("Mark all as read");
        let aw = ui.text_size(all, 13.0, true).x + 46.0;
        let all_r = Rect::new(body.x + body.w - aw, body.y, aw, 34.0);
        if !plain.is_empty() {
            ui.block(all_r);
            let hover = ui.hovering(all_r);
            let lit = ui.hover_anim(all_r, hover);
            rrect(all_r, 17.0, Color::new(1.0, 0.9, 0.75, 0.04 + 0.06 * lit));
            let color = crate::ui::mix(MUTED, CREAM, lit);
            crate::icons::draw(Icon::Check, vec2(all_r.x + 18.0, all_r.y + 17.0), 15.0, color);
            ui.text(all, all_r.x + 32.0, text_y(all_r.y + 17.0, 13.0), 13.0, color, true);
            if ui.activated(all_r, hover) {
                online.inbox_read(plain);
            }
        }
        // The filters, each with what isn't read in it.
        let row = Rect::new(body.x, body.y, body.w - aw - 12.0, 34.0);
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
        let tab = FILTERS.iter().position(|c| *c == self.filter).unwrap_or(0);
        let (clicked, _) = crate::tabrow::tabs(ui, row, &labels, &counts, tab, 6.0);
        if let Some(k) = clicked {
            self.filter = FILTERS[k];
            self.first.reset();
        }
        let _switch = ui.switch_scope(motion::id("inbox-filter"), tab as i64, 16.0);
        let body = Rect::new(body.x, body.y + 46.0, body.w, body.h - 46.0);
        let list = ordered(online.inbox_items(), self.filter, now);
        if self.pick_achievement
            && let Some(n) = list.iter().find(|n| achievement_of(n).is_some())
        {
            self.pick_achievement = false;
            self.selected = Some(n.id);
        }
        if !online.inbox_loaded() && list.is_empty() {
            crate::online::loading(ui, body);
            return action;
        }
        if list.is_empty() {
            let text = if self.filter.is_none() {
                t("No notifications yet. What the staff and the game tell you comes here.")
            } else {
                t("Nothing here.")
            };
            let glyph = crate::online::Glyph::Icon(Icon::Bell);
            crate::online::empty_state(ui, body, glyph, text, None);
            return action;
        }
        // Side by side when there is room; else the list, or the one picked.
        let selected = self.selected.and_then(|id| list.iter().find(|n| n.id == id)).cloned();
        let (list_r, detail_r) = if body.w >= WIDE {
            let lw = (body.w * 0.42).clamp(300.0, 430.0);
            (
                Some(Rect::new(body.x, body.y, lw, body.h)),
                Some(Rect::new(body.x + lw + 14.0, body.y, body.w - lw - 14.0, body.h)),
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
            rrect(dr, 10.0, Color::new(1.0, 1.0, 1.0, 0.025));
            rrect_lines(dr, 10.0, 1.0, BORDER);
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
                    let glyph = crate::online::Glyph::Icon(Icon::Bell);
                    let text = t("Pick a notification to read it.");
                    crate::online::empty_state(ui, dr, glyph, text, None);
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
        let view = ui.list_begin(area, &mut self.first, list.len(), NOTE_H);
        let scrolls = view.scrolls();
        let mut picked = None;
        for k in view.rows() {
            let n = &list[k];
            let w = area.w - if scrolls { 12.0 } else { 0.0 };
            let r = Rect::new(area.x, view.y(k), w, NOTE_H - 4.0);
            let hover = ui.hovering(r);
            let chosen = self.selected == Some(n.id);
            let lit = ui.hover_anim(r, hover);
            let alpha = if chosen { 0.085 } else { 0.05 * lit };
            if alpha > 0.002 {
                rrect(r, 10.0, Color::new(1.0, 0.92, 0.8, alpha));
            }
            if chosen {
                rrect(Rect::new(r.x, r.y + 12.0, 3.0, r.h - 24.0), 1.5, GOLD);
            }
            let mid = r.y + r.h / 2.0;
            badge(n, vec2(r.x + 12.0 + 20.0, mid), 20.0);
            let x = r.x + 12.0 + 40.0 + 12.0;
            let unread = n.unread();
            // The time on the right; under it, unread (or pinned).
            let when = crate::localtime::when_short(n.created_at, now);
            let when_w = ui.text_size(&when, 11.5, unread).x;
            let tc = if unread { GOLD } else { MUTED };
            ui.text(&when, r.x + r.w - 12.0 - when_w, mid - 17.0, 11.5, tc, unread);
            let mut right = r.x + r.w - 12.0;
            if unread {
                let color = if n.important { crate::ui::ALERT } else { GOLD };
                draw_circle(right - 5.0, mid + 10.0, 5.0, color);
                right -= 18.0;
            } else if n.pinned(now) {
                let label = t("Pinned");
                let pw = ui.text_size(label, 10.5, true).x + 12.0;
                let p = Rect::new(right - pw, mid + 2.0, pw, 17.0);
                let color = category_color(n.category());
                rrect(p, 8.5, Color::new(color.r, color.g, color.b, 0.2));
                ui.text_centered(label, p.x + pw / 2.0, text_y(p.y + 8.5, 10.5), 10.5, color, true);
                right -= pw + 8.0;
            }
            let (title, sub) = headline(n);
            let title_room = r.x + r.w - 12.0 - when_w - 8.0 - x;
            let (title, size) = ui.fitted(&title, 14.5, title_room, true);
            let title_color = if unread { CREAM } else { crate::ui::mix(MUTED, CREAM, 0.55) };
            ui.text(&title, x, mid - 19.0, size, title_color, true);
            let sub = ui.ellipsize(&sub, right - x - 4.0, 12.5);
            ui.text(&sub, x, mid + 3.0, 12.5, MUTED, false);
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
        let pad = 22.0;
        let inner = area.w - pad * 2.0;
        let color = category_color(n.category());
        let (title, body) = wording(n);
        let achievement = achievement_of(n);
        if let Some(a) = achievement {
            return self.draw_achievement(ui, online, area, n, a, now);
        }
        let mut y = area.y + pad;
        // The mark, the title and when; the delete button at the right.
        let mark = vec2(area.x + pad + 28.0, y + 28.0);
        badge(n, mark, 28.0);
        let del = Rect::new(area.x + area.w - pad - 34.0, y + 11.0, 34.0, 34.0);
        let can = Online::inbox_deletable(n);
        if ui.icon_button_ex(del, Icon::Trash, t("Delete"), can, true) {
            online.inbox_delete(vec![n.id]);
            self.selected = None;
        }
        if !can && ui.hovering(del) {
            let why = if n.important && n.unread() {
                t("Confirm you read it first.")
            } else {
                t("It stays while it is pinned.")
            };
            ui.tooltip(why, del);
        }
        let tx = mark.x + 28.0 + 16.0;
        let room = del.x - 10.0 - tx;
        let (head, kind) = (title.clone(), category_name(Some(n.category())).to_string());
        let (head, size) = ui.fitted(&head, 20.0, room, true);
        ui.text(&head, tx, y + 7.0, size, CREAM, true);
        let mut meta = vec![kind];
        if !n.from.is_empty() {
            meta.push(tw("From {name}", &[("name", &n.from)]));
        }
        meta.push(crate::localtime::when_full(n.created_at, now));
        let meta = ui.ellipsize(&meta.join(" · "), room, 12.5);
        ui.text(&meta, tx, y + 34.0, 12.5, MUTED, false);
        y += 56.0 + 14.0;
        if n.pinned(now) {
            let left = if n.pinned_until >= i64::MAX / 2 {
                t("Pinned").to_string()
            } else {
                tw("Pinned for {t} more", &[("t", &crate::online::time_left(n.pinned_until - now))])
            };
            let w = ui.text_size(&left, 11.5, true).x + 16.0;
            let p = Rect::new(tx, y - 6.0, w, 20.0);
            rrect(p, 10.0, Color::new(color.r, color.g, color.b, 0.18));
            ui.text(&left, p.x + 8.0, text_y(p.y + 10.0, 11.5), 11.5, color, true);
            y += 24.0;
        }
        draw_line(area.x + pad, y, area.x + area.w - pad, y, 1.0, BORDER);
        y += 16.0;
        // The buttons at the bottom; the text above them.
        let buttons_y = area.y + area.h - pad - 40.0;
        let text_room = buttons_y - 14.0 - y;
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
        let fit = ((text_room / 22.0).floor().max(1.0)) as usize;
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
                y + first as f32 * 22.0 - 6.0,
                inner,
                quoted as f32 * 22.0 + 12.0,
            );
            rrect(qr, 8.0, Color::new(1.0, 1.0, 1.0, 0.04));
            rrect(
                Rect::new(qr.x, qr.y + 4.0, 3.0, qr.h - 8.0),
                1.5,
                Color::new(color.r, color.g, color.b, 0.7),
            );
        }
        for (k, (l, quote)) in lines.iter().enumerate() {
            let x = area.x + pad + if *quote { 12.0 } else { 0.0 };
            let c = if *quote { crate::online::STATUS_COLOR } else { CREAM };
            ui.text(l, x, y + k as f32 * 22.0, 15.0, c, false);
        }
        // The buttons: what it leads to, and "I read it" for an important
        // one not confirmed.
        let mut x = area.x + pad;
        let mut action = None;
        if let Some(label) = action_label(&n.action) {
            let bw = ui.text_size(label, 15.0, true).x + 40.0;
            if ui.primary_button(Rect::new(x, buttons_y, bw, 40.0), label) {
                action = self.follow(online, &n.action);
                online.inbox_read(vec![n.id]);
            }
            x += bw + 10.0;
        }
        if n.important && n.unread() {
            let label = t("I read it");
            let bw = ui.text_size(label, 15.0, true).x + 40.0;
            if ui.button(Rect::new(x, buttons_y, bw, 40.0), label, true) {
                online.inbox_read(vec![n.id]);
            }
        }
        action
    }

    /// An achievement's notice in full: its medal large, its name, its
    /// metal and when, what it took, and the way to it in the profile.
    fn draw_achievement(
        &mut self,
        ui: &mut Ui,
        online: &mut Online,
        area: Rect,
        n: &Notification,
        a: &'static snook_legend_proto::Achievement,
        now: i64,
    ) -> Option<InboxAction> {
        let pad = 22.0;
        let inner = area.w - pad * 2.0;
        let cx = area.x + area.w / 2.0;
        let del = Rect::new(area.x + area.w - pad - 34.0, area.y + pad - 4.0, 34.0, 34.0);
        let can = Online::inbox_deletable(n);
        if ui.icon_button_ex(del, Icon::Trash, t("Delete"), can, true) {
            online.inbox_delete(vec![n.id]);
            self.selected = None;
        }
        // The buttons at the bottom; the rest centred in the room above.
        let buttons_y = area.y + area.h - pad - 42.0;
        let desc = ui.wrap(&crate::profile::achievement_text(a), inner.min(420.0), 15.0);
        let medal = (area.h * 0.26).clamp(90.0, 150.0);
        let block = medal + 22.0 + 30.0 + 26.0 + 18.0 + desc.len() as f32 * 21.0;
        let top = area.y + ((buttons_y - area.y - block) / 2.0).max(pad);
        // The medal on a soft glow of its metal.
        let metal = crate::profile::medal_color(a.level);
        let c = vec2(cx, top + medal / 2.0);
        for (k, alpha) in [(1.0, 0.05), (0.78, 0.07), (0.58, 0.10)] {
            draw_circle(c.x, c.y, medal * 0.62 * k, Color::new(metal.r, metal.g, metal.b, alpha));
        }
        crate::profile::medal_art(c, medal, a.level, false);
        let mut y = top + medal + 22.0;
        let (name, size) = ui.fitted(t(a.title), 24.0, inner, true);
        ui.text_centered(&name, cx, y, size, CREAM, true);
        y += 34.0;
        // Its metal, and when it was won.
        let tier = crate::profile::medal_tier(a.level).to_uppercase();
        let when = crate::localtime::when_full(n.created_at, now);
        let tier_w = ui.text_size(&tier, 11.0, true).x + 16.0;
        let when_w = ui.text_size(&when, 12.5, false).x;
        let row_w = tier_w + 10.0 + when_w;
        let pill = Rect::new(cx - row_w / 2.0, y - 2.0, tier_w, 20.0);
        rrect(pill, 10.0, Color::new(metal.r, metal.g, metal.b, 0.2));
        ui.text(&tier, pill.x + 8.0, text_y(pill.y + 10.0, 11.0), 11.0, metal, true);
        ui.text(&when, pill.x + tier_w + 10.0, text_y(pill.y + 10.0, 12.5), 12.5, MUTED, false);
        y += 26.0 + 18.0;
        for line in &desc {
            ui.text_centered(line, cx, y, 15.0, CREAM, false);
            y += 21.0;
        }
        // To the profile, on this medal (and what the notice offers besides).
        let see = t("See it in your profile");
        let see_w = ui.text_size(see, 15.0, true).x + 44.0;
        let other = action_label(&n.action);
        let other_w = other.map_or(0.0, |l| ui.text_size(l, 15.0, true).x + 40.0 + 10.0);
        let mut x = cx - (see_w + other_w) / 2.0;
        let mut action = None;
        if ui.primary_button(Rect::new(x, buttons_y, see_w, 42.0), see) {
            if let Kind::Achievement { code } = &n.kind {
                action = Some(InboxAction::Achievement(code.clone()));
            }
            online.inbox_read(vec![n.id]);
        }
        x += see_w + 10.0;
        if let Some(label) = other
            && ui.button(Rect::new(x, buttons_y, other_w - 10.0, 42.0), label, true)
        {
            action = self.follow(online, &n.action);
            online.inbox_read(vec![n.id]);
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

/// What was picked in the list of conversations.
enum Pick {
    Thread(String),
    Compose,
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn an_achievement_reads_by_its_name_first() {
        let _lang = crate::i18n::lang_lock();
        crate::i18n::set_lang(crate::i18n::Lang::English);
        let a = &snook_legend_proto::ACHIEVEMENTS[0];
        let n1 = n(1, Kind::Achievement { code: a.code.into() }, 10, 0);
        let (title, sub) = headline(&n1);
        assert_eq!(title, a.title);
        assert_eq!(sub, "Achievement unlocked");
        assert!(achievement_of(&n1).is_some());
        assert_eq!(headline(&n(2, Kind::Warning, 10, 0)).0, "Warning from the staff");
    }
}
