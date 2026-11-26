//! The inbox as the game keeps it: the notifications the server sent (see
//! `snook_legend_proto::inbox`), each worded here in the player's language
//! from what happened; a toast when one arrives; and the important ones,
//! which wait in a dialog until the player says they read them. The
//! inbox's own screen is `crate::inbox`.
//!
//! Also what the moderation tells the player as facts: why a line was
//! turned away (muted, a word the chat doesn't allow), and a line that
//! mentions them.

use macroquad::prelude::*;
use snook_legend_proto::inbox::{Action, Category, Kind, Notification};
use snook_legend_proto::moderation::{TextPlace, TextRefusal};
use snook_legend_proto::{ClientMsg, Rules};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle};

use super::Online;
use crate::i18n::{t, tw};
use crate::ui::{ALERT, BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};

/// What the game keeps of the inbox.
#[derive(Default)]
pub(super) struct Inbox {
    /// Newest first, as the server last sent them (with what came since).
    pub(super) items: Vec<Notification>,
    pub(super) unread: u32,
    /// The server sent it since signing in.
    pub(super) loaded: bool,
    /// The important notification on screen, and since when (Enter takes
    /// it only after a moment).
    shown: Option<(i64, f64)>,
}

impl Inbox {
    fn recount(&mut self) {
        self.unread = self.items.iter().filter(|n| n.unread()).count() as u32;
    }
}

/// A notification's title and body in the player's language (what the
/// staff wrote comes after, as written).
pub fn wording(n: &Notification) -> (String, String) {
    let now = crate::game::unix_now();
    let item = |code: &str| {
        snook_legend_proto::shop::item(code)
            .map_or_else(|| code.to_string(), |i| t(i.name).to_string())
    };
    let rules = |r: Rules| crate::i18n::rules_name_inline(r);
    let (title, body) = match &n.kind {
        Kind::StaffMessage => (t("Message from the staff").to_string(), String::new()),
        Kind::Warning => (t("Warning from the staff").to_string(), String::new()),
        Kind::Muted { until } => (
            t("You were muted").to_string(),
            if *until > now {
                tw(
                    "You can't write to other players for {t} more.",
                    &[("t", &super::time_left(until - now))],
                )
            } else {
                t("The mute is over.").to_string()
            },
        ),
        Kind::Unmuted => {
            (t("You can chat again").to_string(), t("The staff lifted your mute.").to_string())
        }
        Kind::RatingAdjusted { rules: r, delta, rating, .. } => (
            t("Your rating was adjusted").to_string(),
            tw(
                "{delta} under the {rules}: now {rating}.",
                &[("delta", &format!("{delta:+}")), ("rules", &rules(*r)), ("rating", rating)],
            ),
        ),
        Kind::MatchVoided { opponent, delta, .. } => (
            t("A game was annulled").to_string(),
            tw(
                "Your game against {name} no longer counts ({delta} rating).",
                &[("name", opponent), ("delta", &format!("{:+}", -delta))],
            ),
        ),
        Kind::MatchEnded { opponent, won, .. } => (
            t("The staff ended a game").to_string(),
            match won {
                Some(true) => {
                    tw("Your game against {name} was given to you.", &[("name", opponent)])
                }
                Some(false) => tw(
                    "Your game against {name} was given to your opponent.",
                    &[("name", opponent)],
                ),
                None => {
                    tw("Your game against {name} ended without a result.", &[("name", opponent)])
                }
            },
        ),
        Kind::ReportActed { username } => (
            t("Thank you for your report").to_string(),
            tw("The staff took action about {name}.", &[("name", username)]),
        ),
        Kind::StaffGift { coins, item: code, vip_days } => {
            let mut parts = Vec::new();
            if *coins > 0 {
                parts.push(tw("{n} coins", &[("n", coins)]));
            }
            if !code.is_empty() {
                parts.push(item(code));
            }
            if *vip_days > 0 {
                parts.push(tw("{n} days of VIP", &[("n", vip_days)]));
            }
            (t("A gift from the staff").to_string(), parts.join(" · "))
        }
        Kind::ItemAwarded { code, .. } => (t("You won an item").to_string(), item(code)),
        Kind::VipItem { code, .. } => (t("VIP item of the month").to_string(), item(code)),
        Kind::Achievement { code } => {
            let title = snook_legend_proto::ACHIEVEMENTS
                .iter()
                .find(|a| a.code == code)
                .map_or_else(|| code.clone(), |a| t(a.title).to_string());
            (t("Achievement unlocked").to_string(), title)
        }
        Kind::TournamentStarting { name, starts_at, .. } => (
            t("A tournament is about to start").to_string(),
            if *starts_at > now {
                format!(
                    "{name} · {}",
                    tw("starts in {t}", &[("t", &super::time_left(starts_at - now))])
                )
            } else {
                name.clone()
            },
        ),
        Kind::TournamentResult { name, place, prize, .. } => (
            t("Tournament result").to_string(),
            tw(
                "{name}: place {place}, {prize} coins.",
                &[("name", name), ("place", place), ("prize", prize)],
            ),
        ),
        Kind::AuctionOutbid { code, bid, .. } => (
            t("You were outbid").to_string(),
            format!("{} · {}", item(code), tw("{n} coins", &[("n", bid)])),
        ),
        Kind::AuctionWon { code, price, .. } => (
            t("You won an auction").to_string(),
            format!("{} · {}", item(code), tw("{n} coins", &[("n", price)])),
        ),
        Kind::Maintenance { at, minutes } => (
            t("Maintenance").to_string(),
            if *at > now {
                tw(
                    "The server stops in {t} for about {n} min.",
                    &[("t", &super::time_left(at - now)), ("n", minutes)],
                )
            } else {
                tw("The server stops for about {n} min.", &[("n", minutes)])
            },
        ),
        Kind::Other => (t("Notification").to_string(), String::new()),
    };
    (title, body)
}

/// The name of an inbox filter.
pub fn category_name(c: Option<Category>) -> &'static str {
    t(match c {
        None => "All",
        Some(Category::Staff) => "Staff",
        Some(Category::Punishment) => "Punishments",
        Some(Category::Gift) => "Gifts",
        Some(Category::Tournament) => "Tournaments",
        Some(Category::Achievement) => "Achievements",
        Some(Category::Game) => "Other",
    })
}

/// The colour a category is marked with.
pub fn category_color(c: Category) -> Color {
    match c {
        Category::Staff => Color::new(0.42, 0.72, 1.0, 1.0),
        Category::Punishment => ALERT,
        Category::Gift => GOLD,
        Category::Tournament => Color::new(0.45, 0.86, 0.52, 1.0),
        Category::Achievement => Color::new(0.78, 0.56, 1.0, 1.0),
        Category::Game => MUTED,
    }
}

/// What a notification's button says (None: no button, or one this game
/// can't follow).
pub fn action_label(a: &Action) -> Option<&'static str> {
    Some(t(match a {
        Action::None | Action::Other => return None,
        Action::Replay { .. } => "See the game",
        Action::Equip { .. } => "Equip now",
        Action::Collection { .. } => "Open the collection",
        Action::Tournament { .. } => "Join",
        Action::Profile { .. } => "Profile",
        Action::Shop { .. } => "Shop",
        Action::Auctions => "Auctions",
        Action::Vip => "VIP",
    }))
}

/// Why something the player wrote was turned away, in their language.
pub fn refusal_text(refusal: &TextRefusal) -> String {
    match refusal {
        TextRefusal::Muted { until, reason } => {
            let left = super::time_left(until - crate::game::unix_now());
            if reason.is_empty() {
                tw("You are muted for {t} more.", &[("t", &left)])
            } else {
                tw("You are muted for {t} more: {reason}", &[("t", &left), ("reason", reason)])
            }
        }
        TextRefusal::BannedWord { word } => {
            tw("That word isn't allowed here: {word}", &[("word", word)])
        }
        TextRefusal::Other => t("That can't be sent.").to_string(),
    }
}

/// An envelope (the inbox), drawn: the icon font has none.
pub fn draw_envelope(c: Vec2, s: f32, color: Color) {
    let (w, h) = (s * 0.86, s * 0.62);
    let r = Rect::new(c.x - w / 2.0, c.y - h / 2.0, w, h);
    let th = (s * 0.09).max(1.4);
    rrect_lines(r, s * 0.08, th, color);
    draw_line(r.x + th, r.y + th, c.x, c.y + h * 0.08, th, color);
    draw_line(r.x + r.w - th, r.y + th, c.x, c.y + h * 0.08, th, color);
}

/// A count on a red badge at the top right of `r` (none at 0).
pub fn draw_badge(ui: &Ui, r: Rect, n: u32) {
    if n == 0 {
        return;
    }
    let text = if n > 99 { "99+".to_string() } else { n.to_string() };
    let bw = (ui.text_size(&text, 11.0, true).x + 10.0).max(18.0);
    let badge = Rect::new(r.x + r.w - bw + 6.0, r.y - 6.0, bw, 18.0);
    rrect(badge, 9.0, ALERT);
    ui.text_centered(&text, badge.x + bw / 2.0, badge.y + 3.0, 11.0, WHITE, true);
}

impl Online {
    /// Notifications not read yet (the badge on the inbox's buttons).
    pub fn inbox_unread(&self) -> u32 {
        self.inbox.unread
    }

    /// The inbox, newest first.
    pub fn inbox_items(&self) -> &[Notification] {
        &self.inbox.items
    }

    /// The server sent the inbox since signing in.
    pub fn inbox_loaded(&self) -> bool {
        self.inbox.loaded
    }

    /// The lounge's tournaments, on tournament `id` (a notification's
    /// "Join").
    pub fn open_tournament(&mut self, id: u64) {
        self.tab = super::Tab::Tournaments;
        self.selected_tournament = Some(id);
        self.send(ClientMsg::Tournaments);
    }

    /// Asks for the inbox again (the screen opens on what is known).
    pub fn request_inbox(&mut self) {
        self.send(ClientMsg::Inbox);
    }

    /// The player read these (an important one: confirmed reading it).
    pub fn inbox_read(&mut self, ids: Vec<i64>) {
        let now = crate::game::unix_now().max(1);
        let ids: Vec<i64> = ids
            .into_iter()
            .filter(|id| self.inbox.items.iter().any(|n| n.id == *id && n.unread()))
            .collect();
        if ids.is_empty() {
            return;
        }
        for n in self.inbox.items.iter_mut().filter(|n| ids.contains(&n.id)) {
            n.read_at = now;
        }
        self.inbox.recount();
        self.send(ClientMsg::InboxRead { ids });
    }

    /// Whether a notification may be taken out now: not while pinned, nor
    /// an important one not confirmed yet.
    pub fn inbox_deletable(n: &Notification) -> bool {
        !n.pinned(crate::game::unix_now()) && !(n.important && n.unread())
    }

    /// Takes notifications out of the inbox.
    pub fn inbox_delete(&mut self, ids: Vec<i64>) {
        let ids: Vec<i64> = ids
            .into_iter()
            .filter(|id| self.inbox.items.iter().any(|n| n.id == *id && Self::inbox_deletable(n)))
            .collect();
        if ids.is_empty() {
            return;
        }
        self.inbox.items.retain(|n| !ids.contains(&n.id));
        self.inbox.recount();
        self.send(ClientMsg::InboxDelete { ids });
    }

    /// The inbox came (after signing in, or as asked).
    pub(super) fn inbox_arrived(&mut self, items: Vec<Notification>, unread: u32) {
        self.inbox.items = items;
        self.inbox.unread = unread;
        self.inbox.loaded = true;
    }

    /// A new notification: on top, and said in a toast (an achievement has
    /// its own; an important one opens its dialog).
    pub(super) fn inbox_new(&mut self, item: Notification, unread: u32) {
        let (title, _) = wording(&item);
        let quiet = matches!(item.kind, Kind::Achievement { .. });
        if !quiet && !item.important {
            self.toast(tw("New in your inbox: {title}", &[("title", &title)]), true);
        }
        self.attention = Some(title);
        self.inbox.items.retain(|n| n.id != item.id);
        self.inbox.items.insert(0, item);
        self.inbox.unread = unread;
    }

    /// The oldest important notification not confirmed yet.
    fn important_waiting(&self) -> Option<&Notification> {
        // Development aid (screenshots): SNOOK_LEGEND_QUIET_INBOX keeps it
        // off the screens it would cover.
        if crate::dev::flag("SNOOK_LEGEND_QUIET_INBOX") {
            return None;
        }
        self.inbox.items.iter().rev().find(|n| n.important && n.unread())
    }

    /// An important notification waits for the player's "I read it".
    pub(super) fn important_open(&self) -> bool {
        self.important_waiting().is_some()
    }

    /// The oldest important notification, over whatever page is up (not a
    /// game), until the player confirms they read it. Esc doesn't close it.
    pub(super) fn draw_important(&mut self, ui: &mut Ui) {
        let Some(n) = self.important_waiting().cloned() else {
            self.inbox.shown = None;
            return;
        };
        let now = get_time();
        let shown = match self.inbox.shown {
            Some((id, at)) if id == n.id => at,
            _ => {
                self.inbox.shown = Some((n.id, now));
                now
            }
        };
        let (title, body) = wording(&n);
        let more = self.inbox.items.iter().filter(|m| m.important && m.unread()).count() - 1;
        let (sw, sh) = (crate::ui::vw(), crate::ui::vh());
        let w = 520.0f32.min(sw - 32.0);
        let pad = 24.0;
        let inner = w - pad * 2.0;
        let mut size = 15.0;
        let wrap = |ui: &Ui, size: f32| {
            let mut lines = Vec::new();
            if !body.is_empty() {
                lines.extend(ui.wrap(&body, inner, size));
            }
            if !n.text.is_empty() {
                if !lines.is_empty() {
                    lines.push(String::new());
                }
                lines.extend(ui.wrap(&n.text, inner, size));
            }
            lines
        };
        let fixed = pad + 30.0 + 22.0 + 14.0 + 20.0 + 40.0 + pad;
        let room = sh - 16.0 - fixed;
        let mut lines = wrap(ui, size);
        if lines.len() as f32 * (size + 5.0) > room {
            size = 13.0;
            lines = wrap(ui, size);
        }
        let line_h = size + 5.0;
        let fit = ((room / line_h).floor() as usize).max(1);
        if lines.len() > fit {
            lines.truncate(fit);
            if let Some(last) = lines.last_mut() {
                *last = ui.ellipsize(&format!("{last} …"), inner, size);
            }
        }
        let h = fixed + lines.len() as f32 * line_h;
        let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
        let color = category_color(n.category());
        let _modal = ui.modal_scope(crate::ui::motion::id("inbox-message"), 0.6);
        ui.panel(r);
        rrect_lines(r, 8.0, 2.0, color);
        ui.block(Rect::new(0.0, 0.0, sw, sh));
        let mut y = r.y + pad;
        let (title, title_size) = ui.fitted(&title, 22.0, inner, true);
        ui.text_centered(&title, sw / 2.0, y + 2.0, title_size, color, true);
        y += 30.0;
        // From whom and when.
        let from = if n.from.is_empty() {
            crate::profile::date(n.created_at)
        } else {
            format!(
                "{} · {}",
                tw("From {name}", &[("name", &n.from)]),
                crate::profile::date(n.created_at)
            )
        };
        let (from, from_size) = ui.fitted(&from, 13.0, inner, false);
        ui.text_centered(&from, sw / 2.0, y, from_size, MUTED, false);
        y += 22.0 + 14.0;
        for line in &lines {
            ui.text(line, r.x + pad, y, size, CREAM, false);
            y += line_h;
        }
        let label = if more > 0 {
            format!("{} ({})", t("I read it"), tw("{n} more", &[("n", &more)]))
        } else {
            t("I read it").to_string()
        };
        let bw = super::button_w(ui, &label, 40.0).max(160.0).min(inner);
        let b = Rect::new(sw / 2.0 - bw / 2.0, r.y + r.h - pad - 40.0, bw, 40.0);
        let enter = (is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter))
            && now - shown > 1.5;
        if ui.primary_button(b, &label) || enter {
            self.inbox_read(vec![n.id]);
            self.inbox.shown = None;
        }
    }

    /// Something the player wrote was turned away: the line leaves the
    /// chat, and a notice says why.
    pub(super) fn text_refused(&mut self, place: TextPlace, refusal: TextRefusal) {
        if place == TextPlace::Chat
            && let Some(text) = self.echoes.pop()
            && let Some(i) = self.chat.iter().rposition(|l| l.pending && l.text == text)
        {
            self.chat.remove(i);
        }
        self.toast(refusal_text(&refusal), false);
    }

    /// A lounge chat line mentions this player: it is lit, and a soft
    /// sound and a notice say so (the game's window elsewhere too).
    pub(super) fn mentioned(&mut self, line: &snook_legend_proto::ChatLine) {
        let me = self.me.as_ref().map(|m| m.username.clone()).unwrap_or_default();
        let mine = line.from.eq_ignore_ascii_case(&me);
        if mine || !line.mentions.iter().any(|m| m.eq_ignore_ascii_case(&me)) {
            return;
        }
        let text = tw("{name} mentioned you", &[("name", &line.from)]);
        self.attention = Some(format!("{text}: {}", line.text));
        self.toast(text, true);
        self.invite_sound = true;
    }
}

/// The inbox's button for a page's header: an envelope with the unread
/// count. True when clicked.
pub fn inbox_button(ui: &mut Ui, r: Rect, unread: u32) -> bool {
    let hover = ui.hovering(r);
    ui.block(r);
    rrect(r, 8.0, Color::new(1.0, 1.0, 1.0, if hover { 0.08 } else { 0.04 }));
    rrect_lines(r, 8.0, 1.0, if hover { GOLD } else { BORDER });
    draw_envelope(vec2(r.x + r.w / 2.0, r.y + r.h / 2.0), 20.0, if hover { GOLD } else { CREAM });
    draw_badge(ui, r, unread);
    if hover {
        let tip = if unread > 0 {
            tw("Inbox ({n} unread)", &[("n", &unread)])
        } else {
            t("Inbox").to_string()
        };
        ui.tooltip(&tip, r);
    }
    let clicked = hover && ui.clicked(r);
    if clicked {
        ui.note_click();
    }
    clicked
}

impl Online {
    /// The line the staff pinned, over the lounge's chat (`area`'s top):
    /// who wrote it and what, a click on a mention opens a profile; the
    /// staff can take it down. The height it took (with the gap under it).
    pub(super) fn draw_pinned(
        &mut self,
        ui: &mut Ui,
        line: &snook_legend_proto::ChatLine,
        area: Rect,
        staff: bool,
    ) -> f32 {
        let size = 13.0;
        let pad = 10.0;
        let label = t("Pinned");
        let lw = ui.text_size(label, 11.0, true).x + 12.0;
        let text = if line.kind == snook_legend_proto::ChatKind::Player {
            format!("{}: {}", line.from, line.text)
        } else {
            crate::i18n::server_text(&line.text)
        };
        let room = area.w - pad * 2.0 - lw - 8.0 - if staff { 28.0 } else { 0.0 };
        let mut lines = ui.wrap(&text, room, size);
        if lines.len() > 2 {
            lines.truncate(2);
            if let Some(last) = lines.last_mut() {
                *last = ui.ellipsize(&format!("{last} …"), room, size);
            }
        }
        let h = 12.0 + lines.len() as f32 * 18.0;
        let r = Rect::new(area.x, area.y, area.w, h);
        ui.block(r);
        rrect(r, 6.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.10));
        rrect_lines(r, 6.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.7));
        let pill = Rect::new(r.x + pad, r.y + 7.0, lw, 18.0);
        rrect(pill, 9.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.85));
        let dark = Color::new(0.1, 0.07, 0.04, 1.0);
        ui.text_centered(label, pill.x + lw / 2.0, pill.y + 3.0, 11.0, dark, true);
        let x = pill.x + lw + 8.0;
        for (k, l) in lines.iter().enumerate() {
            let y = r.y + 6.0 + k as f32 * 18.0;
            let mut cx = x;
            for (span, mention) in crate::ui::mention_spans(l, &line.mentions) {
                let w = ui.text_size(&span, size, mention).x;
                ui.text(&span, cx, y + 1.0, size, if mention { GOLD } else { CREAM }, mention);
                let hit = Rect::new(cx, y, w, 18.0);
                if mention && ui.hovering(hit) && ui.clicked(hit) {
                    let name = span.trim_start_matches('@').to_string();
                    self.request_profile(&name);
                }
                cx += w;
            }
        }
        if staff {
            let b = Rect::new(r.x + r.w - 26.0, r.y + 6.0, 20.0, 20.0);
            if ui.icon_button_ex(b, crate::icons::Icon::Close, t("Unpin"), true, true) {
                self.send(ClientMsg::UnpinChat);
            }
        }
        h + 8.0
    }
}
