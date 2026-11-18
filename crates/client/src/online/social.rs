//! The lounge's social side: the people played recently (the Friends tab's
//! "Recent" view), the "looking for a game" board (at the top of the
//! Players tab) and status messages in the player rows.

use macroquad::prelude::*;
use snook_legend_proto::{ClientMsg, PlayerInfo, PlayerStatus, RecentOpponent, Relation, SeekNote};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle};

use super::{
    Glyph, Online, OnlineAction, Tab, button_w, empty_state, fit_text, loading, name_width,
    row_card, time_left,
};
use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::ui::{ALERT, BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};

/// A status message in a list row: lighter than the muted line it follows.
pub const STATUS_COLOR: Color = Color::new(0.84, 0.78, 0.68, 0.92);
const WIN: Color = Color::new(0.45, 0.95, 0.55, 1.0);
/// The text field of a new note.
const SEEK_FIELD: u64 = 63;

/// What the lounge keeps for the social features.
#[derive(Default)]
pub(super) struct Social {
    /// The Friends tab shows the people played recently.
    pub(super) recent_view: bool,
    /// The people played recently, as the server last sent them (None:
    /// not yet).
    pub(super) recent: Option<Vec<RecentOpponent>>,
    /// Ask for them again when they are next shown (a game ended).
    pub(super) recent_stale: bool,
    /// The notes on the "looking for a game" board, newest first.
    pub(super) seeks: Vec<SeekNote>,
    /// A note being written (None: not writing one).
    pub(super) composing: Option<String>,
}

impl Social {
    /// A note was put up (or replaced the player's earlier one).
    pub(super) fn posted(&mut self, note: SeekNote) {
        self.seeks.retain(|n| !n.username.eq_ignore_ascii_case(&note.username));
        self.seeks.push(note);
        self.seeks.sort_by_key(|n| std::cmp::Reverse((n.at, n.username.to_lowercase())));
    }

    pub(super) fn removed(&mut self, username: &str) {
        self.seeks.retain(|n| !n.username.eq_ignore_ascii_case(username));
    }

    /// The notes still up at `now`.
    pub(super) fn live_seeks(&self, now: i64) -> impl Iterator<Item = &SeekNote> {
        self.seeks.iter().filter(move |n| n.expires_at > now)
    }

    /// `name`'s note, if it is up.
    pub(super) fn note_of(&self, name: &str, now: i64) -> Option<&SeekNote> {
        self.live_seeks(now).find(|n| n.username.eq_ignore_ascii_case(name))
    }
}

/// How long ago something happened, as it is said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ago {
    JustNow,
    Minutes(i64),
    Hours(i64),
    Yesterday,
    Days(i64),
    /// Long ago: the date.
    Date,
}

fn ago_of(at: i64, now: i64) -> Ago {
    let s = (now - at).max(0);
    match s {
        0..60 => Ago::JustNow,
        60..3600 => Ago::Minutes(s / 60),
        3600..86_400 => Ago::Hours(s / 3600),
        86_400..172_800 => Ago::Yesterday,
        172_800..2_592_000 => Ago::Days(s / 86_400),
        _ => Ago::Date,
    }
}

/// "just now", "5 min ago", "3 h ago", "yesterday", "4 days ago", or the date.
pub fn ago(at: i64, now: i64) -> String {
    match ago_of(at, now) {
        Ago::JustNow => t("just now").to_string(),
        Ago::Minutes(n) => tw("{n} min ago", &[("n", &n)]),
        Ago::Hours(n) => tw("{n} h ago", &[("n", &n)]),
        Ago::Yesterday => t("yesterday").to_string(),
        Ago::Days(n) => tw("{n} days ago", &[("n", &n)]),
        Ago::Date => crate::profile::date(at),
    }
}

/// A note's text as shown (an empty note says what it is for).
pub fn note_text(n: &SeekNote) -> String {
    if n.text.is_empty() {
        t("Looking for a game").to_string()
    } else {
        format!("“{}”", n.text)
    }
}

/// A row's second line: `base` in muted text, then the player's status
/// message (if any) after a dot, cut to `width`.
pub fn sub_line(ui: &Ui, base: &str, status: &str, x: f32, y: f32, width: f32) {
    let base = ui.ellipsize(base, width, 12.0);
    ui.text(&base, x, y, 12.0, MUTED, false);
    if status.is_empty() {
        return;
    }
    let used = ui.text_size(&base, 12.0, false).x;
    let sep = "  ·  ";
    let sep_w = ui.text_size(sep, 12.0, false).x;
    let room = width - used - sep_w;
    // Not worth a few letters: leave it for the profile.
    if room < 40.0 {
        return;
    }
    ui.text(sep, x + used, y, 12.0, MUTED, false);
    let status = ui.ellipsize(status, room, 12.0);
    ui.text(&status, x + used + sep_w, y, 12.0, STATUS_COLOR, false);
}

impl Online {
    /// How this player relates to `name` (None where there are no friends:
    /// a local network, or not signed in).
    pub fn relation_to(&self, name: &str) -> Option<Relation> {
        let me = self.me.as_ref()?;
        if self.lan || me.guest {
            return None;
        }
        let is = |list: &[String]| list.iter().any(|n| n.eq_ignore_ascii_case(name));
        Some(if me.username.eq_ignore_ascii_case(name) {
            Relation::Me
        } else if self.friends.friends.iter().any(|f| f.player.username.eq_ignore_ascii_case(name))
        {
            Relation::Friend
        } else if is(&self.friends.blocked) {
            Relation::Blocked
        } else if is(&self.friends.outgoing) {
            Relation::RequestSent
        } else if is(&self.friends.incoming) {
            Relation::RequestReceived
        } else {
            Relation::None
        })
    }

    /// Asks `name` to be friends, or accepts their request.
    pub fn befriend(&mut self, name: &str) {
        match self.relation_to(name) {
            Some(Relation::None) => {
                self.send(ClientMsg::FriendRequest { username: name.to_string() });
                // At once: the server's list follows.
                self.friends.outgoing.push(name.to_string());
            }
            Some(Relation::RequestReceived) => {
                self.send(ClientMsg::FriendAnswer { username: name.to_string(), accept: true });
            }
            _ => {}
        }
    }

    /// Asks for the people played recently (shown on the Friends tab).
    pub(super) fn request_recent(&mut self) {
        self.social.recent_stale = false;
        // Development aid (screenshots): SNOOK_LEGEND_RECENT=loading never asks.
        if crate::dev::var("SNOOK_LEGEND_RECENT").as_deref() == Some("loading") {
            return;
        }
        self.send(ClientMsg::RecentOpponents);
    }

    /// Friends | Recent, over the Friends tab. Returns the area under it.
    pub(super) fn friends_switch(&mut self, ui: &mut Ui, area: Rect) -> Rect {
        let options = [t("Friends"), t("Recent")];
        let r = Rect::new(area.x, area.y, area.w.min(320.0), 30.0);
        if let Some(i) = ui.segmented(r, &options, self.social.recent_view as usize) {
            self.social.recent_view = i == 1;
            self.list_first = 0;
            if self.social.recent_view {
                self.request_recent();
            }
        }
        let top = r.y + r.h + 12.0;
        Rect::new(area.x, top, area.w, area.y + area.h - top)
    }

    /// The people played recently: when, the result, and what can be done
    /// with them now (befriend, invite, watch).
    pub(super) fn draw_recent(&mut self, ui: &mut Ui, area: Rect) -> Option<OnlineAction> {
        if self.social.recent_stale {
            self.request_recent();
        }
        let Some(recent) = self.social.recent.clone() else {
            loading(ui, area);
            return None;
        };
        if recent.is_empty() {
            let text = t("Nobody yet. The people you play online will show up here.");
            if empty_state(ui, area, Glyph::Icon(Icon::Replay), text, Some(t("Play now"))) {
                self.tab = Tab::Players;
                self.list_first = 0;
                if self.queue.is_none() {
                    self.send(ClientMsg::QueueJoin { rated: self.rated_games });
                }
            }
            return None;
        }
        let now = crate::game::unix_now();
        let row_h = 48.0;
        let visible = (area.h / row_h).floor().max(1.0) as usize;
        self.list_first = ui.scroll_list(area, self.list_first, recent.len(), visible);
        let row_w = area.w - if recent.len() > visible { 12.0 } else { 0.0 };
        let invite_label = t("Invite");
        let watch_label = t("Watch");
        let bw = button_w(ui, invite_label, 30.0).max(button_w(ui, watch_label, 30.0)).max(96.0);
        let mut action = None;
        let mut send = None;
        let mut befriend = None;
        let mut watch = None;
        for (k, r_) in recent.iter().skip(self.list_first).take(visible).enumerate() {
            let r = Rect::new(area.x, area.y + k as f32 * row_h, row_w, row_h - 6.0);
            let mid = r.y + r.h / 2.0;
            // As they are now, if they are here.
            let live = self.players.iter().find(|p| p.username == r_.player.username);
            let p: &PlayerInfo = live.unwrap_or(&r_.player);
            let button = Rect::new(r.x + r.w - bw - 6.0, r.y + 6.0, bw, r.h - 12.0);
            let rel = Rect::new(button.x - 8.0 - 30.0, r.y + 6.0, 30.0, r.h - 12.0);
            let hover = row_card(ui, r, true);
            // Won or lost, as in the profile's games.
            let (mark, color) = if r_.won { (t("W"), WIN) } else { (t("L"), ALERT) };
            let badge = Rect::new(r.x + 8.0, mid - 12.0, 24.0, 24.0);
            rrect(badge, 5.0, Color::new(color.r, color.g, color.b, 0.18));
            ui.text_centered(
                mark,
                badge.x + 12.0,
                crate::ranking::text_y(mid, 14.0),
                14.0,
                color,
                true,
            );
            let x = r.x + 42.0;
            let room = rel.x - 10.0 - x;
            let used = ui.player_name(p, x, r.y + 5.0, 15.0, room - 14.0);
            // Here now (coloured as in the lounge) or not (grey).
            let dot = vec2(x + used + 9.0, r.y + 5.0 + 8.5);
            if live.is_some() {
                Self::status_dot(p, dot.x, dot.y);
            } else {
                draw_circle(dot.x, dot.y, 4.0, Color::new(0.4, 0.4, 0.4, 1.0));
            }
            let mode = if r_.rated { t("rated") } else { t("casual") };
            let mut sub = format!("{} · {} · {}", t(p.tier().name()), ago(r_.at, now), mode);
            if r_.games > 1 {
                sub = format!("{sub} · {}", tw("{n} games", &[("n", &r_.games)]));
            }
            let (sub, size) = fit_text(ui, &sub, 12.0, room, false);
            ui.text(&sub, x, r.y + 24.0, size, MUTED, false);
            if hover && ui.clicked(r) && !button.contains(ui.mouse()) && !rel.contains(ui.mouse()) {
                action = Some(OnlineAction::Profile(p.username.clone()));
            }
            // Friends already, asked, asked by them, or not yet.
            let name = p.username.clone();
            let relation = self.relation_to(&name).unwrap_or_default();
            let friend = relation == Relation::Friend;
            let rc = vec2(rel.x + rel.w / 2.0, rel.y + rel.h / 2.0);
            match relation {
                Relation::Friend => {
                    let gold = Color::new(GOLD.r, GOLD.g, GOLD.b, 0.8);
                    crate::icons::draw(Icon::People, rc, 17.0, gold);
                    if ui.hovering(rel) {
                        ui.tooltip(t("Friend"), rel);
                    }
                }
                Relation::RequestSent => {
                    crate::icons::draw(Icon::Check, rc, 15.0, MUTED);
                    if ui.hovering(rel) {
                        ui.tooltip(t("Request sent"), rel);
                    }
                }
                Relation::RequestReceived => {
                    if ui.icon_button(rel, Icon::Check, t("Accept friend"), true) {
                        befriend = Some(name.clone());
                    }
                }
                Relation::None => {
                    if ui.icon_button(rel, Icon::AddFriend, t("Add friend"), true) {
                        befriend = Some(name.clone());
                    }
                }
                Relation::Me | Relation::Blocked => {}
            }
            if live.is_some() && p.status == PlayerStatus::Playing && friend {
                if ui.button(button, watch_label, true) {
                    watch = Some(name);
                }
            } else {
                let can = live.is_some_and(|p| p.status == PlayerStatus::Available)
                    && self.outgoing.is_none();
                if ui.button(button, invite_label, can) {
                    send = Some(ClientMsg::Invite { to: name, rated: self.rated_games });
                }
            }
        }
        if let Some(msg) = send {
            self.send(msg);
        }
        if let Some(name) = befriend {
            self.befriend(&name);
        }
        if let Some(name) = watch {
            self.spectate(&name);
        }
        action
    }

    /// The bar over the players: post a note on the "looking for a game"
    /// board, write it, or see and take down one's own. Returns its height
    /// (with the gap under it).
    pub(super) fn draw_seek_bar(&mut self, ui: &mut Ui, area: Rect) -> f32 {
        if self.lan {
            return 0.0;
        }
        let h = 40.0;
        let r = Rect::new(area.x, area.y, area.w, h);
        let now = crate::game::unix_now();
        let me = self.me.as_ref().map(|m| m.username.clone()).unwrap_or_default();
        let mine = self.social.note_of(&me, now).cloned();
        if let Some(text) = self.social.composing.as_mut() {
            // Writing a note: the field, Post, and × (Esc) to cancel.
            let post = t("Post");
            // 36-high controls, like the lounge's other fields.
            let ch = 36.0;
            let cy = r.y + (h - ch) / 2.0;
            let pw = button_w(ui, post, ch).max(84.0);
            let close = Rect::new(r.x + r.w - ch, cy, ch, ch);
            let post_r = Rect::new(close.x - 8.0 - pw, cy, pw, ch);
            let field = Rect::new(r.x, cy, post_r.x - 8.0 - r.x, ch);
            let enter = ui.text_input(SEEK_FIELD, field, text, snook_legend_proto::MAX_SEEK_LEN);
            let text = text.clone();
            let kind = if self.rated_games { t("rated") } else { t("casual") };
            let hint = tw("What are you after? ({kind})", &[("kind", &kind)]);
            if text.is_empty() {
                // Shown while typing too: the field was just opened for it.
                let (hint, size) = fit_text(ui, &hint, 14.0, field.w - 24.0, false);
                let y = field.y + (field.h - size) / 2.0;
                ui.text(&hint, field.x + 12.0, y, size, MUTED, false);
            }
            if ui.primary_button(post_r, post) || enter {
                let text = snook_legend_proto::clean_seek(&text);
                self.send(ClientMsg::PostSeek { text, rated: self.rated_games });
                self.social.composing = None;
                ui.focus = None;
            } else if ui.icon_button(close, Icon::Close, t("Cancel"), true) || ui.escape() {
                self.social.composing = None;
            }
            return h + 12.0;
        }
        match mine {
            Some(note) => {
                // Up on the board: what it says, how long it stays, take it down.
                rrect(r, 8.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.10));
                rrect_lines(r, 8.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.55));
                let down = t("Take down");
                let bw = button_w(ui, down, 30.0).max(90.0);
                let b = Rect::new(r.x + r.w - bw - 5.0, r.y + 5.0, bw, h - 10.0);
                crate::icons::draw(Icon::Star, vec2(r.x + 18.0, r.y + h / 2.0), 14.0, GOLD);
                let x = r.x + 34.0;
                let room = b.x - 10.0 - x;
                let mode = if note.rated { t("rated") } else { t("casual") };
                let left = time_left(note.expires_at - now);
                let line1 = tw("Your note: {text}", &[("text", &note_text(&note))]);
                let (line1, s1) = fit_text(ui, &line1, 13.0, room, true);
                ui.text(&line1, x, r.y + 5.0, s1, CREAM, true);
                let line2 = tw("{mode} · {left} left", &[("mode", &mode), ("left", &left)]);
                let line2 = ui.ellipsize(&line2, room, 11.0);
                ui.text(&line2, x, r.y + 23.0, 11.0, MUTED, false);
                if ui.button(b, down, true) {
                    self.send(ClientMsg::RemoveSeek { username: String::new() });
                    self.social.removed(&me);
                }
            }
            None => {
                rrect(r, 8.0, Color::new(1.0, 1.0, 1.0, 0.03));
                rrect_lines(r, 8.0, 1.0, Color::new(BORDER.r, BORDER.g, BORDER.b, 0.6));
                let label = t("Post a note");
                let bw = button_w(ui, label, 30.0).max(110.0);
                let b = Rect::new(r.x + r.w - bw - 5.0, r.y + 5.0, bw, h - 10.0);
                let x = r.x + 12.0;
                let (text, size) =
                    fit_text(ui, t("Looking for a game?"), 14.0, b.x - 10.0 - x, false);
                ui.text(&text, x, r.y + (h - size) / 2.0, size, MUTED, false);
                if ui.button(b, label, true) {
                    self.social.composing = Some(String::new());
                    ui.focus = Some(SEEK_FIELD);
                }
                if ui.hovering(b) {
                    ui.tooltip(t("Everyone in the lounge sees it for 30 minutes"), b);
                }
            }
        }
        h + 12.0
    }

    /// A player row's second line when they have a note up: the note, and
    /// whether the game would be rated.
    pub(super) fn note_line(ui: &Ui, note: &SeekNote, x: f32, y: f32, width: f32) {
        let mode = if note.rated { t("rated") } else { t("casual") };
        let mode = format!("  ·  {mode}");
        let mode_w = ui.text_size(&mode, 12.0, false).x;
        let text = ui.ellipsize(&note_text(note), (width - mode_w).max(20.0), 12.0);
        ui.text(&text, x, y, 12.0, GOLD, false);
        let used = ui.text_size(&text, 12.0, false).x;
        if used + mode_w <= width {
            ui.text(&mode, x + used, y, 12.0, MUTED, false);
        }
    }

    /// The rating after a name in a row whose second line is a note.
    pub(super) fn rating_after_name(ui: &Ui, p: &PlayerInfo, x: f32, y: f32, room: f32) {
        if p.guest {
            return;
        }
        let used = name_width(ui, p, 15.0);
        let text = t(p.tier().name()).to_string();
        if used + 10.0 + ui.text_size(&text, 12.0, false).x <= room {
            ui.text(&text, x + used + 10.0, y + 2.5, 12.0, MUTED, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_ago() {
        let now = 1_700_000_000;
        assert_eq!(ago_of(now - 5, now), Ago::JustNow);
        assert_eq!(ago_of(now + 30, now), Ago::JustNow, "a clock a little ahead");
        assert_eq!(ago_of(now - 600, now), Ago::Minutes(10));
        assert_eq!(ago_of(now - 7200, now), Ago::Hours(2));
        assert_eq!(ago_of(now - 90_000, now), Ago::Yesterday);
        assert_eq!(ago_of(now - 5 * 86_400, now), Ago::Days(5));
        assert_eq!(ago_of(now - 60 * 86_400, now), Ago::Date);
    }

    #[test]
    fn one_note_per_player_newest_first() {
        let note = |name: &str, at: i64| SeekNote {
            username: name.into(),
            text: String::new(),
            rated: true,
            at,
            expires_at: at + 1800,
        };
        let mut s = Social::default();
        s.posted(note("ana", 10));
        s.posted(note("bia", 20));
        s.posted(note("ANA", 30));
        let names: Vec<&str> = s.seeks.iter().map(|n| n.username.as_str()).collect();
        assert_eq!(names, ["ANA", "bia"]);
        assert!(s.note_of("ana", 100).is_some());
        assert!(s.note_of("bia", 20 + 1800).is_none(), "expired");
        s.removed("Ana");
        assert_eq!(s.seeks.len(), 1);
    }
}
