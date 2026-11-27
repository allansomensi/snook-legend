//! Private messages (see `snook_legend_proto::dm`): the conversations, the
//! one open (in the inbox, or in a window over the lounge opened from the
//! friends list), the unread counts, and what the chat's settings do at the
//! table (the muted opponents, the quick phrases only).
//!
//! A conversation reads like a messaging app's: bubbles, this player's on
//! the right, each with its time and, for this player's own, how far it
//! got (a clock while it goes, one tick once the server has it, two once
//! the other's game has it, two in gold once read), the days between them,
//! whether the other is online (or when they last were) and when they are
//! typing.

use std::collections::HashMap;

use macroquad::prelude::*;

use snook_legend_proto::activity::{Activity, Pastime};
use snook_legend_proto::dm::{
    DirectMessage, DmThread, MAX_DM_LEN, TYPING_EVERY, TYPING_SHOWN, clean_dm,
};
use snook_legend_proto::{ClientMsg, PlayerInfo, QUICK_CHAT};

use crate::config::Config;
use crate::game::Match;
use crate::i18n::{t, tw};
use crate::icons::Icon;
#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_circle_lines, draw_line, draw_rectangle};
use crate::ranking::text_y;
use crate::ui::{ALERT, BORDER, CREAM, GOLD, HAIRLINE, MUTED, Ui, motion, rrect, rrect_lines};

/// The conversation's text field.
const FIELD: u64 = 9_310;

/// A message's type, its line and the room around it in its bubble.
const SIZE: f32 = 14.0;
const LINE: f32 = 19.0;
const PAD_X: f32 = 11.0;
const PAD_Y: f32 = 7.0;
/// The time (and the ticks) in a bubble.
const META: f32 = 10.5;
const META_H: f32 = 13.0;
/// The ticks' room after the time.
const TICKS_W: f32 = 17.0;
/// Messages from the same player this close together (seconds) sit under
/// each other as one group.
const GROUP_SECS: i64 = 5 * 60;
/// One to three emoji alone are drawn large, without a bubble.
const JUMBO: f32 = 34.0;
/// A message that hasn't come back from the server after this long
/// (seconds) didn't go.
const SEND_TIMEOUT: f64 = 12.0;

/// The colours of the bubbles: the other player's, this player's.
const THEIRS: Color = Color::new(0.165, 0.125, 0.095, 1.0);
const MINE_TOP: Color = Color::new(0.34, 0.25, 0.12, 1.0);
const MINE_BOTTOM: Color = Color::new(0.29, 0.21, 0.10, 1.0);
/// Online.
pub const ONLINE: Color = Color::new(0.35, 0.85, 0.45, 1.0);

/// A message this player sent that the server hasn't sent back yet.
#[derive(Debug, Clone)]
struct Pending {
    /// Its own number until then (negative: never a server's).
    local: i64,
    text: String,
    /// When it was sent (Unix seconds, and the game's clock).
    at: i64,
    sent: f64,
    /// The server refused it, or it never came back.
    failed: bool,
}

/// One conversation, as far as it was read from the server.
#[derive(Debug, Default)]
struct Conversation {
    /// Oldest first.
    messages: Vec<DirectMessage>,
    /// Sent and not back from the server yet, oldest first.
    pending: Vec<Pending>,
    /// Older messages are on the server.
    more: bool,
    /// Its newest messages came (a message arriving before that is kept
    /// until they do).
    loaded: bool,
    /// The other player was online, and last seen (friends), when the
    /// history came.
    online: bool,
    last_seen: i64,
}

/// How far a message of this player's got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    Sending,
    Failed,
    /// The server has it.
    Sent,
    /// The other player's game has it.
    Delivered,
    Read,
}

impl Tick {
    fn of(m: &DirectMessage) -> Tick {
        if m.read {
            Tick::Read
        } else if m.delivered {
            Tick::Delivered
        } else {
            Tick::Sent
        }
    }

    /// In words, for the hint over a message.
    fn words(self) -> &'static str {
        t(match self {
            Tick::Sending => "Sending…",
            Tick::Failed => "Not sent",
            Tick::Sent => "Sent",
            Tick::Delivered => "Delivered",
            Tick::Read => "Read",
        })
    }
}

/// What the game knows of the player on the other side of a conversation.
#[derive(Debug, Clone, Default)]
pub struct Peer {
    /// As they are now (picture, flag, name's colour), when known.
    pub player: Option<PlayerInfo>,
    /// Online now (None: not known).
    pub online: Option<bool>,
    /// When they were last online (friends; 0: not known).
    pub last_seen: i64,
    /// They can be invited to a game from here.
    pub can_invite: bool,
}

/// What the reader asked for from a conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DmAction {
    None,
    Profile(String),
    Invite(String),
}

/// One message as the conversation shows it (a server's, or one on its way).
struct Item<'a> {
    id: i64,
    mine: bool,
    text: &'a str,
    at: i64,
    tick: Option<Tick>,
}

/// A row of a conversation as laid out: the day, the unread line, or a
/// message (`item` in the conversation's list).
#[derive(Debug, Clone)]
enum Row {
    Day(String),
    Unread(String),
    Bubble(Bubble),
}

#[derive(Debug, Clone)]
struct Bubble {
    item: usize,
    mine: bool,
    lines: Vec<String>,
    /// The bubble's width.
    w: f32,
    /// The first and the last of a group from the same player.
    first: bool,
    last: bool,
    jumbo: bool,
    time: String,
}

/// The rows of a conversation laid out (top, height), and what they were
/// laid out for.
#[derive(Default)]
struct Layout {
    key: Option<(String, u64)>,
    rows: Vec<(f32, f32, Row)>,
    height: f32,
    /// The first message's id: older ones added above move what is read.
    first_id: i64,
}

/// The open conversation's view: where it is scrolled to, and what came
/// below while the reader was further up.
#[derive(Default)]
struct View {
    scroll: crate::ui::Scroll,
    layout: Layout,
    /// Taken to the newest (or the unread line) once its messages come.
    settle: bool,
    /// The view was at the newest last frame (new messages keep it there).
    at_end: bool,
    /// Messages from the other that came below while scrolled up.
    below: u32,
    /// Unread messages when it was opened (a line above the first of them).
    unread: u32,
}

/// The player's conversations.
#[derive(Default)]
pub struct Dms {
    /// Newest first.
    threads: Vec<DmThread>,
    conversations: HashMap<String, Conversation>,
    /// The player whose conversation is open (over the lounge, or in the
    /// inbox).
    pub open: Option<String>,
    input: String,
    view: View,
    emoji: crate::emoji_tray::EmojiTray,
    /// The quick phrases' list is open (the chat kept to them).
    phrases_open: bool,
    /// Older messages were asked for (one page at a time).
    asked_older: bool,
    /// When each player last said they were typing to this one (the
    /// game's clock).
    typing: HashMap<String, f64>,
    /// To whom and when this player last said they were typing, and what
    /// the field held then.
    typing_sent: Option<(String, f64)>,
    typed: String,
    next_local: i64,
}

fn key(name: &str) -> String {
    name.to_lowercase()
}

impl Dms {
    /// Unread messages from `name`.
    pub fn unread(&self, name: &str) -> u32 {
        self.threads.iter().find(|t| t.with.eq_ignore_ascii_case(name)).map_or(0, |t| t.unread)
    }

    /// Unread messages from everyone.
    pub fn total_unread(&self) -> u32 {
        self.threads.iter().map(|t| t.unread).sum()
    }

    /// The list of conversations came.
    pub fn set_threads(&mut self, mut threads: Vec<DmThread>) {
        // The open conversation is being read: nothing in it is unread.
        if let Some(open) = &self.open {
            for t in threads.iter_mut().filter(|t| t.with.eq_ignore_ascii_case(open)) {
                t.unread = 0;
            }
        }
        self.threads = threads;
    }

    /// A message to or from `me` came. Returns the sender's name when it is
    /// theirs and their conversation isn't open (the game tells the player).
    pub fn arrived(&mut self, message: DirectMessage, me: &str) -> Option<String> {
        let mine = message.from.eq_ignore_ascii_case(me);
        let with = if mine { message.to.clone() } else { message.from.clone() };
        let open = self.open.as_ref().is_some_and(|o| o.eq_ignore_ascii_case(&with));
        let c = self.conversations.entry(key(&with)).or_default();
        if !c.messages.iter().any(|m| m.id == message.id) {
            if mine {
                // The one on its way came back.
                if let Some(i) = c.pending.iter().position(|p| p.text == message.text) {
                    c.pending.remove(i);
                }
            }
            c.messages.push(message.clone());
            c.messages.sort_by_key(|m| m.id);
            if open && !mine && !self.view.at_end {
                self.view.below += 1;
            }
        }
        if !mine {
            // A message is the end of their typing.
            self.typing.remove(&key(&with));
        }
        let (unread, player, online) =
            match self.threads.iter().position(|t| t.with.eq_ignore_ascii_case(&with)) {
                Some(i) => {
                    let t = self.threads.remove(i);
                    (t.unread, t.player, t.online)
                }
                None => (0, None, !mine),
            };
        let unread = if open {
            0
        } else if mine {
            unread
        } else {
            unread + 1
        };
        // Someone who writes is online.
        let online = online || !mine;
        self.threads
            .insert(0, DmThread { with: with.clone(), last: message, unread, player, online });
        (!mine && !open).then_some(with)
    }

    /// A page of a conversation came (the newest, or older ones), with
    /// whether the other is online and when they last were.
    pub fn history(
        &mut self,
        with: &str,
        messages: Vec<DirectMessage>,
        more: bool,
        online: bool,
        last_seen: i64,
    ) {
        self.asked_older = false;
        let c = self.conversations.entry(key(with)).or_default();
        for m in messages {
            if !c.messages.iter().any(|x| x.id == m.id) {
                c.messages.push(m);
            }
        }
        c.messages.sort_by_key(|m| m.id);
        c.more = more;
        c.loaded = true;
        c.online = online;
        c.last_seen = last_seen;
    }

    /// The messages this player sent to `with`, up to `up_to`, reached
    /// their game (or were read).
    pub fn status(&mut self, with: &str, up_to: i64, read: bool) {
        let mine = |m: &DirectMessage| m.to.eq_ignore_ascii_case(with) && m.id <= up_to;
        if let Some(c) = self.conversations.get_mut(&key(with)) {
            for m in c.messages.iter_mut().filter(|m| mine(m)) {
                m.delivered = true;
                m.read |= read;
            }
        }
        for t in self.threads.iter_mut().filter(|t| t.with.eq_ignore_ascii_case(with)) {
            if mine(&t.last) {
                t.last.delivered = true;
                t.last.read |= read;
            }
        }
    }

    /// `from` is typing to this player.
    pub fn typing_from(&mut self, from: &str) {
        self.typing.insert(key(from), clock());
    }

    /// `name` is typing to this player now.
    pub fn is_typing(&self, name: &str) -> bool {
        self.typing.get(&key(name)).is_some_and(|at| clock() - at < TYPING_SHOWN)
    }

    /// The server refused something just now: the message just sent, if
    /// one is on its way, didn't go (the notice says why).
    pub fn refused(&mut self) {
        let now = clock();
        let newest = self
            .conversations
            .values_mut()
            .flat_map(|c| c.pending.iter_mut())
            .filter(|p| !p.failed && now - p.sent < 8.0)
            .max_by(|a, b| a.sent.total_cmp(&b.sent));
        if let Some(p) = newest {
            p.failed = true;
        }
    }

    /// Whether the conversation with `name` was online when its history came.
    pub fn snapshot(&self, name: &str) -> Option<(bool, i64)> {
        self.conversations.get(&key(name)).filter(|c| c.loaded).map(|c| (c.online, c.last_seen))
    }

    /// Opens the conversation with `name`: what to ask the server.
    pub fn open_with(&mut self, name: &str) -> Vec<ClientMsg> {
        let unread = self.unread(name);
        self.open = Some(name.to_string());
        self.view = View { settle: true, at_end: true, unread, ..View::default() };
        self.input.clear();
        self.emoji.reset();
        self.phrases_open = false;
        let mut out = vec![ClientMsg::DmHistory { with: name.to_string(), before: None }];
        if unread > 0 {
            out.push(ClientMsg::DmRead { with: name.to_string() });
        }
        for t in self.threads.iter_mut().filter(|t| t.with.eq_ignore_ascii_case(name)) {
            t.unread = 0;
        }
        out
    }

    /// Signed out or in as someone else: nothing of these is theirs.
    pub fn clear(&mut self) {
        *self = Dms::default();
    }

    /// The conversations, newest first.
    pub fn threads(&self) -> &[DmThread] {
        &self.threads
    }

    /// The newest message of the conversation with `name` is one on its
    /// way (for the list of conversations).
    pub fn sending_to(&self, name: &str) -> Option<Tick> {
        let p = self.conversations.get(&key(name))?.pending.last()?;
        Some(if p.failed { Tick::Failed } else { Tick::Sending })
    }

    /// How far this player's message `m` got.
    pub fn tick(m: &DirectMessage) -> Tick {
        Tick::of(m)
    }

    /// The open conversation in a window over the lounge (opened from the
    /// friends list): what to send to the server, and what the reader
    /// asked for. `me`: this player; `phrases_only`: the chat kept to the
    /// quick phrases.
    pub fn draw(
        &mut self,
        ui: &mut Ui,
        me: &str,
        phrases_only: bool,
        peer: &Peer,
    ) -> (Vec<ClientMsg>, DmAction) {
        let Some(_) = self.open.clone() else { return (Vec::new(), DmAction::None) };
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let w = 480.0f32.min(sw - 24.0);
        let h = (sh * 0.78).clamp(320.0, 620.0).min(sh - 24.0);
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        let _modal = ui.modal_scope(motion::id("dm"), 0.5);
        ui.panel(r);
        let inner = Rect::new(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0);
        let (out, action, close) = self.conversation_with(ui, inner, me, phrases_only, peer, true);
        let busy = self.phrases_open || self.emoji.open;
        let closing = close || (!busy && (ui.escape() || ui.dismissed(r)));
        // Nothing under it takes a click.
        ui.block(Rect::new(0.0, 0.0, sw, sh));
        if closing {
            self.close(ui);
        }
        (out, action)
    }

    /// Closes the open conversation.
    pub fn close(&mut self, ui: &mut Ui) {
        self.open = None;
        self.phrases_open = false;
        self.emoji.reset();
        if ui.focus == Some(FIELD) {
            ui.focus = None;
        }
    }

    /// The open conversation in `r`, as the inbox shows it (no Close: the
    /// inbox has its own way back).
    pub fn conversation(
        &mut self,
        ui: &mut Ui,
        r: Rect,
        me: &str,
        phrases_only: bool,
        peer: &Peer,
    ) -> (Vec<ClientMsg>, DmAction) {
        let (out, action, _) = self.conversation_with(ui, r, me, phrases_only, peer, false);
        (out, action)
    }

    /// The open conversation in `r`: who with, the messages, and the field
    /// to answer under them. What to send, what the reader asked for, and
    /// whether they closed it (`close`: there is a Close button).
    fn conversation_with(
        &mut self,
        ui: &mut Ui,
        r: Rect,
        me: &str,
        phrases_only: bool,
        peer: &Peer,
        close: bool,
    ) -> (Vec<ClientMsg>, DmAction, bool) {
        let mut out = Vec::new();
        let Some(with) = self.open.clone() else { return (out, DmAction::None, false) };
        self.expire();
        let header_h = 60.0;
        let (action, closed) =
            self.header(ui, Rect::new(r.x, r.y, r.w, header_h), &with, peer, close);
        // The field grows with what is typed (up to five lines).
        let field_w = r.w - 24.0 - 2.0 * (38.0 + 6.0);
        let field_h = if phrases_only {
            38.0
        } else {
            ui.text_area_height(&self.input, field_w, 5).max(38.0)
        };
        let composer_h = field_h + 24.0;
        let log = Rect::new(r.x, r.y + header_h, r.w, r.h - header_h - composer_h);
        draw_rectangle(log.x, log.y, log.w, log.h, Color::new(0.0, 0.0, 0.0, 0.22));
        out.extend(self.messages(ui, log, &with, me));
        let composer = Rect::new(r.x, log.y + log.h, r.w, composer_h);
        out.extend(self.composer(ui, composer, field_w, field_h, &with, phrases_only));
        (out, action, closed)
    }

    /// Messages that never came back fail.
    fn expire(&mut self) {
        let now = clock();
        for c in self.conversations.values_mut() {
            for p in c.pending.iter_mut().filter(|p| now - p.sent > SEND_TIMEOUT) {
                p.failed = true;
            }
        }
    }

    /// The conversation's header: the other player's picture (with a dot
    /// when online), their name, whether they are online, typing or when
    /// they were last seen, and the buttons (invite, profile, close).
    fn header(
        &mut self,
        ui: &mut Ui,
        r: Rect,
        with: &str,
        peer: &Peer,
        close: bool,
    ) -> (DmAction, bool) {
        let mut action = DmAction::None;
        let mut closed = false;
        let mid = r.y + r.h / 2.0;
        // The buttons, from the right.
        let mut right = r.x + r.w - 12.0;
        let b = 32.0;
        if close {
            let br = Rect::new(right - b, mid - b / 2.0, b, b);
            if ui.icon_button_ex(br, Icon::Close, t("Close"), true, true) {
                closed = true;
            }
            right -= b + 6.0;
        }
        let br = Rect::new(right - b, mid - b / 2.0, b, b);
        if ui.icon_button_ex(br, Icon::Person, t("Profile"), true, true) {
            action = DmAction::Profile(with.to_string());
        }
        right -= b + 6.0;
        if peer.can_invite {
            let br = Rect::new(right - b, mid - b / 2.0, b, b);
            if ui.icon_button_ex(br, Icon::Ball, t("Invite to a game"), true, true) {
                action = DmAction::Invite(with.to_string());
            }
            right -= b + 6.0;
        }
        // The picture, a click away from the profile too.
        let pic_r = 19.0;
        let pic = vec2(r.x + 16.0 + pic_r, mid);
        let avatar = peer.player.as_ref().map_or("", |p| p.avatar.as_str());
        crate::avatars::draw(avatar, pic, pic_r);
        if peer.online == Some(true) {
            presence_dot(pic + vec2(pic_r * 0.72, pic_r * 0.72));
        }
        let x = pic.x + pic_r + 12.0;
        let name_w = right - 8.0 - x;
        let typing = self.is_typing(with);
        let status = if typing {
            Some((t("typing…").to_string(), GOLD))
        } else {
            match peer.online {
                Some(true) => Some((t("online").to_string(), ONLINE)),
                _ if peer.last_seen > 0 => {
                    Some((crate::localtime::last_seen(peer.last_seen, now()), MUTED))
                }
                Some(false) => Some((t("offline").to_string(), MUTED)),
                None => None,
            }
        };
        let name_y = if status.is_some() { mid - 18.0 } else { mid - 9.0 };
        match &peer.player {
            Some(p) => {
                ui.player_name(p, x, name_y, 16.0, name_w);
            }
            None => {
                let (name, size) = ui.fitted(with, 16.0, name_w, true);
                ui.text(&name, x, name_y, size, CREAM, true);
            }
        }
        if let Some((text, color)) = status {
            let text = ui.ellipsize(&text, name_w, 12.5);
            ui.text(&text, x, mid + 3.0, 12.5, color, false);
        }
        let name_r = Rect::new(r.x + 10.0, r.y + 8.0, (x + name_w).min(right) - r.x - 10.0, 44.0);
        if ui.hovering(name_r) && action == DmAction::None && ui.clicked(name_r) {
            action = DmAction::Profile(with.to_string());
        }
        draw_line(r.x, r.y + r.h - 0.5, r.x + r.w, r.y + r.h - 0.5, 1.0, BORDER);
        (action, closed)
    }

    /// The messages in `log`, newest at the bottom: what to ask the server
    /// (older messages, the conversation read).
    fn messages(&mut self, ui: &mut Ui, log: Rect, with: &str, me: &str) -> Vec<ClientMsg> {
        let mut out = Vec::new();
        let c = self.conversations.get(&key(with));
        let loaded = c.is_some_and(|c| c.loaded);
        let more = c.is_some_and(|c| c.more);
        let empty = c.is_none_or(|c| c.messages.is_empty() && c.pending.is_empty());
        if !loaded && empty {
            crate::online::loading(ui, log);
            return out;
        }
        if empty {
            let text = tw("No messages with {name} yet. Say hello!", &[("name", &with)]);
            crate::online::empty_state(ui, log, crate::online::Glyph::Chat, &text, None);
            return out;
        }
        let Some(c) = self.conversations.get(&key(with)) else { return out };
        let items = items(c, me);
        let inner_w = log.w - 28.0;
        let today = crate::localtime::day_of(now());
        let lay_key = {
            let first = items.first().map_or(0, |i| i.id);
            let last = items.last().map_or(0, |i| i.id);
            let h = (items.len() as u64)
                ^ (first as u64).rotate_left(17)
                ^ (last as u64).rotate_left(34)
                ^ ((inner_w * 4.0) as u64).rotate_left(48)
                ^ (today as u64).rotate_left(8)
                ^ (self.view.unread as u64).rotate_left(56)
                ^ crate::i18n::lang() as u64;
            (key(with), h)
        };
        if self.view.layout.key.as_ref() != Some(&lay_key) {
            let old_first = self.view.layout.first_id;
            let old_h = self.view.layout.height;
            let had = self.view.layout.key.as_ref().is_some_and(|(w, _)| *w == lay_key.0);
            self.view.layout = lay_out(ui, &items, inner_w, self.view.unread);
            self.view.layout.key = Some(lay_key);
            // Older messages came above: what was read stays where it was.
            let first = self.view.layout.first_id;
            if had && first < old_first && old_first != 0 && !self.view.at_end {
                let grew = self.view.layout.height - old_h;
                let at = self.view.scroll.offset() + grew;
                self.view.scroll.jump_to(at);
            }
        }
        let pad = 10.0;
        let content_h = self.view.layout.height + 2.0 * pad;
        let max = (content_h - log.h).max(0.0);
        // Opened (or its messages just came): at the newest, or with the
        // unread line near the top when there are more than fit.
        if self.view.settle && loaded {
            self.view.settle = false;
            let unread_y = self.view.layout.rows.iter().find_map(|(y, _, r)| match r {
                Row::Unread(_) => Some(*y),
                _ => None,
            });
            let at = match unread_y {
                Some(y) if y + pad < max => y + pad - 12.0,
                _ => max,
            };
            self.view.scroll.jump_to(at.max(0.0));
            self.view.at_end = at >= max - 1.0;
        } else if self.view.at_end {
            // At the newest: what comes keeps it there.
            if self.view.scroll.offset() < max - 0.5 {
                self.view.scroll.scroll_to(max);
            }
        }
        let view = ui.scroll_begin(log, &mut self.view.scroll, content_h);
        // Fewer messages than fit sit at the bottom, against the field.
        let top = view.top() + pad + (log.h - content_h).max(0.0);
        let x = log.x + 14.0;
        let mut hint: Option<(String, Rect)> = None;
        let mut retry = None;
        for (y, h, row) in &self.view.layout.rows {
            let y = top + y;
            if !view.visible(y, *h) {
                continue;
            }
            match row {
                Row::Day(label) => day_pill(ui, label, log.x + log.w / 2.0, y + 4.0),
                Row::Unread(label) => {
                    let band = Rect::new(log.x, y + 3.0, log.w, 22.0);
                    draw_rectangle(
                        band.x,
                        band.y,
                        band.w,
                        band.h,
                        Color::new(1.0, 0.86, 0.6, 0.07),
                    );
                    let lw = ui.text_size(label, 11.5, true).x;
                    ui.text(
                        label,
                        log.x + (log.w - lw) / 2.0,
                        text_y(band.y + band.h / 2.0, 11.5),
                        11.5,
                        GOLD,
                        true,
                    );
                }
                Row::Bubble(b) => {
                    let Some(item) = items.get(b.item) else { continue };
                    let bx = if b.mine { x + inner_w - b.w } else { x };
                    let r = Rect::new(bx, y, b.w, *h - if b.last { 0.0 } else { 2.0 });
                    let meta = draw_bubble(ui, b, item, r);
                    if ui.hovering(meta) {
                        let when = crate::localtime::when_full(item.at, now());
                        let text = match item.tick {
                            Some(tick) => format!("{} · {when}", tick.words()),
                            None => when,
                        };
                        hint = Some((text, meta));
                    }
                    if item.tick == Some(Tick::Failed) {
                        let mark = Rect::new(r.x - 26.0, r.y + r.h / 2.0 - 11.0, 22.0, 22.0);
                        failed_mark(mark);
                        if ui.hovering(mark) {
                            hint = Some((t("Not sent. Click to try again.").to_string(), mark));
                        }
                        if ui.clicked(mark) {
                            retry = Some(item.id);
                        }
                    }
                }
            }
        }
        let offset = self.view.scroll.offset();
        ui.scroll_end(view, &mut self.view.scroll);
        if let Some((text, at)) = hint {
            ui.tooltip(&text, at);
        }
        // Where the view is now.
        let max_now = (content_h - log.h).max(0.0);
        self.view.at_end = offset >= max_now - 24.0;
        if self.view.at_end {
            self.view.below = 0;
        }
        // Up at the oldest shown: the page before it.
        if more && offset < 60.0 && !self.asked_older && max_now > 0.0 {
            let before = c.messages.first().map(|m| m.id);
            self.asked_older = true;
            out.push(ClientMsg::DmHistory { with: with.to_string(), before });
        }
        if self.asked_older && more {
            crate::ui::spinner(vec2(log.x + log.w / 2.0, log.y + 18.0), 7.0, GOLD);
        }
        // Further up: a way down to the newest (with what came since).
        if !self.view.at_end {
            let br = Rect::new(log.x + log.w - 52.0, log.y + log.h - 50.0, 38.0, 38.0);
            ui.block(br);
            let hover = ui.hovering(br);
            let lit = ui.hover_anim(br, hover);
            crate::ui::soft_shadow(br, 19.0, 0.5);
            rrect(br, 19.0, crate::ui::mix(Color::new(0.16, 0.12, 0.09, 0.98), THEIRS, lit));
            rrect_lines(br, 19.0, 1.0, crate::ui::mix(BORDER, GOLD, lit));
            crate::icons::draw(Icon::ChevronDown, vec2(br.x + 19.0, br.y + 20.0), 16.0, CREAM);
            if self.view.below > 0 {
                let n =
                    if self.view.below > 99 { "99+".into() } else { self.view.below.to_string() };
                let bw = (ui.text_size(&n, 10.0, true).x + 9.0).max(18.0);
                let badge = Rect::new(br.x + br.w - bw + 4.0, br.y - 6.0, bw, 18.0);
                rrect(badge, 9.0, GOLD);
                ui.text_centered(&n, badge.x + bw / 2.0, badge.y + 3.0, 10.0, DARK, true);
            }
            if hover {
                ui.tooltip(t("Newest messages"), br);
            }
            if ui.activated(br, hover) {
                self.view.scroll.scroll_to(max_now);
                self.view.at_end = true;
                self.view.below = 0;
            }
        }
        if let Some(local) = retry {
            out.extend(self.retry(with, local));
        }
        out
    }

    /// Sends a message that didn't go again.
    fn retry(&mut self, with: &str, local: i64) -> Option<ClientMsg> {
        let c = self.conversations.get_mut(&key(with))?;
        let p = c.pending.iter_mut().find(|p| p.local == local)?;
        p.failed = false;
        p.sent = clock();
        p.at = now();
        Some(ClientMsg::DmSend { to: with.to_string(), text: p.text.clone() })
    }

    /// The field to write in (or the quick phrases), the emoji and Send.
    fn composer(
        &mut self,
        ui: &mut Ui,
        r: Rect,
        field_w: f32,
        field_h: f32,
        with: &str,
        phrases_only: bool,
    ) -> Vec<ClientMsg> {
        let mut out = Vec::new();
        draw_line(r.x, r.y + 0.5, r.x + r.w, r.y + 0.5, 1.0, BORDER);
        let field = Rect::new(r.x + 12.0, r.y + 12.0, field_w, field_h);
        let button = 38.0;
        let send_r =
            Rect::new(r.x + r.w - 12.0 - button, field.y + field.h - button, button, button);
        let emoji_r = Rect::new(send_r.x - 6.0 - button, send_r.y, button, button);
        let text = if phrases_only {
            self.input.clear();
            let wide = Rect::new(field.x, field.y, send_r.x - 6.0 - field.x, field.h);
            phrase_picker(ui, wide, &mut self.phrases_open).map(str::to_string)
        } else {
            let enter = ui.text_area(FIELD, field, &mut self.input, MAX_DM_LEN);
            crate::ui::chat_counter(ui, &self.input, field);
            self.emoji.track(ui, FIELD);
            let bounds = Rect::new(r.x, r.y - 300.0, r.w, 300.0 + r.h);
            self.emoji.draw(ui, emoji_r, field, bounds, FIELD, &mut self.input, MAX_DM_LEN);
            crate::online::placeholder(ui, FIELD, field, &self.input, t("Write a message…"));
            let can = !self.input.trim().is_empty();
            let pressed = send_button(ui, send_r, can);
            // Typing: the other player sees it (now and then, while it goes on).
            if self.input != self.typed {
                self.typed = self.input.clone();
                let due = self.typing_sent.as_ref().is_none_or(|(to, at)| {
                    !to.eq_ignore_ascii_case(with) || clock() - at >= TYPING_EVERY
                });
                if due && can {
                    self.typing_sent = Some((with.to_string(), clock()));
                    out.push(ClientMsg::DmTyping { to: with.to_string() });
                }
            }
            ((enter || pressed) && can).then(|| std::mem::take(&mut self.input))
        };
        if let Some(text) = text {
            let text = clean_dm(&text);
            if !text.is_empty() {
                self.next_local -= 1;
                let c = self.conversations.entry(key(with)).or_default();
                c.pending.push(Pending {
                    local: self.next_local,
                    text: text.clone(),
                    at: now(),
                    sent: clock(),
                    failed: false,
                });
                out.push(ClientMsg::DmSend { to: with.to_string(), text });
                // Down to it; the unread line has served.
                self.view.at_end = true;
                self.view.unread = 0;
                self.typing_sent = None;
                self.typed.clear();
            }
            self.emoji.reset();
            ui.focus = (!phrases_only).then_some(FIELD);
        }
        out
    }
}

/// The colour of text on gold.
const DARK: Color = Color::new(0.1, 0.06, 0.03, 1.0);

fn now() -> i64 {
    crate::game::unix_now()
}

/// Seconds on a clock that only goes forward (since the game started):
/// how long a message has been on its way, how long since "typing…".
fn clock() -> f64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
}

/// The conversation's messages, the server's then those on their way.
fn items<'a>(c: &'a Conversation, me: &str) -> Vec<Item<'a>> {
    let mut out: Vec<Item> = c
        .messages
        .iter()
        .map(|m| {
            let mine = m.from.eq_ignore_ascii_case(me);
            Item { id: m.id, mine, text: &m.text, at: m.at, tick: mine.then(|| Tick::of(m)) }
        })
        .collect();
    out.extend(c.pending.iter().map(|p| Item {
        id: p.local,
        mine: true,
        text: &p.text,
        at: p.at,
        tick: Some(if p.failed { Tick::Failed } else { Tick::Sending }),
    }));
    out
}

/// A message's words as the reader sees them (a quick phrase in their
/// language).
fn shown(text: &str) -> &str {
    if snook_legend_proto::is_quick_phrase(text) { t(text) } else { text }
}

/// One to three emoji and nothing else.
fn jumbo(text: &str) -> bool {
    let text = text.trim();
    crate::emoji::pieces(text).is_some_and(|p| {
        (1..=3).contains(&p.len()) && p.iter().all(|x| matches!(x, crate::emoji::Piece::Emoji(_)))
    })
}

/// The rows of the conversation `width` wide: the days, the unread line
/// (above the `unread` newest messages from the other), the bubbles.
fn lay_out(ui: &Ui, items: &[Item], width: f32, unread: u32) -> Layout {
    let max_w = (width * 0.78).min(440.0);
    let mut rows: Vec<(f32, f32, Row)> = Vec::new();
    let mut y = 0.0;
    // Where the unread line goes: before the unread-th newest from them.
    let unread_at = (unread > 0)
        .then(|| {
            let mut n = 0;
            items.iter().rposition(|i| {
                if !i.mine && i.id > 0 {
                    n += 1;
                }
                n >= unread
            })
        })
        .flatten();
    let mut last_day = None;
    for (k, item) in items.iter().enumerate() {
        let day = crate::localtime::day_of(item.at);
        let new_day = last_day != Some(day);
        if new_day {
            last_day = Some(day);
            let label = crate::localtime::day_label(item.at, now());
            y += if rows.is_empty() { 0.0 } else { 6.0 };
            rows.push((y, 30.0, Row::Day(label)));
            y += 30.0;
        }
        let mut unread_line = false;
        if unread_at == Some(k) {
            let label = if unread == 1 {
                t("1 unread message").to_string()
            } else {
                tw("{n} unread messages", &[("n", &unread)])
            };
            rows.push((y + 2.0, 30.0, Row::Unread(label)));
            y += 32.0;
            unread_line = true;
        }
        let prev = k.checked_sub(1).map(|i| &items[i]);
        let next = items.get(k + 1);
        let together = |a: &Item, b: &Item| {
            a.mine == b.mine
                && (b.at - a.at).abs() <= GROUP_SECS
                && crate::localtime::day_of(a.at) == crate::localtime::day_of(b.at)
        };
        let first = new_day || unread_line || prev.is_none_or(|p| !together(p, item));
        let last = next.is_none_or(|n| !together(item, n) || unread_at == Some(k + 1));
        if first && !rows.is_empty() && !new_day && !unread_line {
            y += 8.0;
        }
        let text = shown(item.text);
        let time = crate::localtime::time_of_day(item.at);
        let meta_w = ui.text_size(&time, META, false).x + if item.mine { TICKS_W } else { 0.0 };
        let big = jumbo(text);
        let (lines, w, h) = if big {
            let w = ui.text_size(text.trim(), JUMBO, false).x.max(meta_w) + 4.0;
            (vec![text.trim().to_string()], w, JUMBO + 8.0 + META_H)
        } else {
            let room = max_w - 2.0 * PAD_X;
            let lines = ui.wrap(text, room, SIZE);
            let lines = if lines.is_empty() { vec![String::new()] } else { lines };
            let widest = lines.iter().map(|l| ui.text_size(l, SIZE, false).x).fold(0.0, f32::max);
            let last_w = lines.last().map_or(0.0, |l| ui.text_size(l, SIZE, false).x);
            // The time after the last line when it fits there, else under it.
            let inline = last_w + 10.0 + meta_w <= room;
            let w = if inline { widest.max(last_w + 10.0 + meta_w) } else { widest.max(meta_w) };
            let h = lines.len() as f32 * LINE + 2.0 * PAD_Y + if inline { 0.0 } else { META_H };
            (lines, w + 2.0 * PAD_X, h)
        };
        let gap = if last { 0.0 } else { 2.0 };
        rows.push((
            y,
            h + gap,
            Row::Bubble(Bubble {
                item: k,
                mine: item.mine,
                lines,
                w: w.max(54.0),
                first,
                last,
                jumbo: big,
                time,
            }),
        ));
        y += h + gap;
    }
    let first_id = items.iter().find(|i| i.id > 0).map_or(0, |i| i.id);
    Layout { key: None, rows, height: y, first_id }
}

/// The day between messages: a small pill in the middle.
fn day_pill(ui: &Ui, label: &str, cx: f32, y: f32) {
    let w = ui.text_size(label, 11.5, true).x + 20.0;
    let r = Rect::new(cx - w / 2.0, y, w, 21.0);
    rrect(r, 10.5, Color::new(0.13, 0.10, 0.075, 0.96));
    rrect_lines(r, 10.5, 1.0, HAIRLINE);
    ui.text(label, r.x + 10.0, text_y(r.y + r.h / 2.0, 11.5), 11.5, MUTED, true);
}

/// A message's bubble in `r` (its time, and its ticks when this player's).
/// Returns where its time is (a hint there says more).
fn draw_bubble(ui: &Ui, b: &Bubble, item: &Item, r: Rect) -> Rect {
    let failed = item.tick == Some(Tick::Failed);
    let opacity = crate::paint::opacity();
    if item.tick == Some(Tick::Sending) {
        crate::paint::set_opacity(opacity * 0.75);
    }
    let meta_color = Color::new(CREAM.r, CREAM.g, CREAM.b, 0.55);
    let meta_w = ui.text_size(&b.time, META, false).x + if b.mine { TICKS_W } else { 0.0 };
    let meta = if b.jumbo {
        let y = r.y + JUMBO + 8.0;
        let x = if b.mine { r.x + r.w - meta_w - 2.0 } else { r.x + 2.0 };
        Rect::new(x, y, meta_w, META_H)
    } else {
        Rect::new(r.x + r.w - PAD_X - meta_w, r.y + r.h - PAD_Y - META_H + 1.0, meta_w, META_H)
    };
    if b.jumbo {
        let text = &b.lines[0];
        let x = if b.mine { r.x + r.w - ui.text_size(text, JUMBO, false).x - 2.0 } else { r.x };
        ui.text(text, x, r.y + 2.0, JUMBO, CREAM, false);
    } else {
        // The corner on the sender's side is sharp on a group's first.
        let radius = 13.0;
        if b.mine {
            crate::ui::rrect_gradient(r, radius, MINE_TOP, MINE_BOTTOM);
            if b.first {
                let c = Rect::new(r.x + r.w - radius, r.y, radius, radius);
                crate::ui::rrect_gradient(
                    Rect::new(c.x, c.y, c.w, c.h + 4.0),
                    3.0,
                    MINE_TOP,
                    crate::ui::mix(MINE_TOP, MINE_BOTTOM, (radius + 4.0) / r.h.max(1.0)),
                );
            }
            rrect_lines(r, radius, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.16));
        } else {
            rrect(r, radius, THEIRS);
            if b.first {
                rrect(Rect::new(r.x, r.y, radius, radius + 4.0), 3.0, THEIRS);
            }
            rrect_lines(r, radius, 1.0, HAIRLINE);
        }
        if failed {
            rrect_lines(r, radius, 1.2, Color::new(ALERT.r, ALERT.g, ALERT.b, 0.7));
        }
        let mut y = r.y + PAD_Y;
        for line in &b.lines {
            ui.text(line, r.x + PAD_X, y, SIZE, CREAM, false);
            y += LINE;
        }
    }
    ui.text(&b.time, meta.x, meta.y, META, meta_color, false);
    if let Some(tick) = item.tick {
        let at = vec2(meta.x + meta.w - TICKS_W + 4.0, meta.y + 1.0);
        draw_tick(tick, at);
    }
    crate::paint::set_opacity(opacity);
    meta
}

/// The ticks of a message of this player's, 13 units wide from `at` (the
/// top left): a clock while it goes, one tick, two, two in gold once read.
pub fn draw_tick(tick: Tick, at: Vec2) {
    let grey = Color::new(CREAM.r, CREAM.g, CREAM.b, 0.55);
    let check = |x: f32, color: Color| {
        let (a, b, c) = (vec2(x, at.y + 6.0), vec2(x + 3.0, at.y + 9.0), vec2(x + 8.5, at.y + 2.5));
        draw_line(a.x, a.y, b.x, b.y, 1.5, color);
        draw_line(b.x - 0.5, b.y + 0.4, c.x, c.y, 1.5, color);
    };
    match tick {
        Tick::Sending => {
            let c = at + vec2(5.5, 5.5);
            draw_circle_lines(c.x, c.y, 4.6, 1.2, grey);
            draw_line(c.x, c.y, c.x, c.y - 3.0, 1.1, grey);
            draw_line(c.x, c.y, c.x + 2.2, c.y, 1.1, grey);
        }
        Tick::Failed => {
            let c = at + vec2(5.5, 5.5);
            draw_circle(c.x, c.y, 5.5, ALERT);
            draw_line(c.x, c.y - 3.0, c.x, c.y + 0.8, 1.4, DARK);
            draw_circle(c.x, c.y + 2.7, 0.8, DARK);
        }
        Tick::Sent => check(at.x + 2.0, grey),
        Tick::Delivered => {
            check(at.x, grey);
            check(at.x + 4.5, grey);
        }
        Tick::Read => {
            let gold = Color::new(1.0, 0.80, 0.36, 1.0);
            check(at.x, gold);
            check(at.x + 4.5, gold);
        }
    }
}

/// The red mark by a message that didn't go (a click sends it again).
fn failed_mark(r: Rect) {
    let c = vec2(r.x + r.w / 2.0, r.y + r.h / 2.0);
    draw_circle(c.x, c.y, 9.0, ALERT);
    draw_line(c.x, c.y - 4.5, c.x, c.y + 1.5, 2.0, DARK);
    draw_circle(c.x, c.y + 4.3, 1.2, DARK);
}

/// The green dot of someone online, ringed by the panel's colour.
pub fn presence_dot(c: Vec2) {
    draw_circle(c.x, c.y, 6.5, Color::new(0.08, 0.05, 0.03, 1.0));
    draw_circle(c.x, c.y, 4.5, ONLINE);
}

/// The round gold Send button (a paper plane); pressed and enabled.
pub fn send_button(ui: &mut Ui, r: Rect, enabled: bool) -> bool {
    ui.block(r);
    let hover = enabled && ui.hovering(r);
    let lit = ui.hover_anim(r, hover);
    let radius = r.w.min(r.h) / 2.0;
    if enabled {
        let top = crate::ui::mix(
            Color::new(0.93, 0.78, 0.42, 1.0),
            Color::new(1.0, 0.86, 0.52, 1.0),
            lit,
        );
        let bottom = crate::ui::mix(
            Color::new(0.78, 0.60, 0.27, 1.0),
            Color::new(0.86, 0.68, 0.33, 1.0),
            lit,
        );
        crate::ui::rrect_gradient(r, radius, top, bottom);
    } else {
        rrect(r, radius, Color::new(1.0, 1.0, 1.0, 0.06));
        rrect_lines(r, radius, 1.0, HAIRLINE);
    }
    let color = if enabled { DARK } else { Color::new(MUTED.r, MUTED.g, MUTED.b, 0.6) };
    crate::icons::draw_filled(
        Icon::Send,
        vec2(r.x + r.w / 2.0 + 1.0, r.y + r.h / 2.0),
        17.0,
        color,
    );
    if hover {
        ui.tooltip(t("Send"), r);
    }
    enabled && ui.activated(r, hover)
}

/// The quick phrases in place of a text field (the chat kept to them): a
/// button the size of `field` that opens their list above it. The phrase
/// chosen, if any (in English: each reader sees it in their language).
pub fn phrase_picker(ui: &mut Ui, field: Rect, open: &mut bool) -> Option<&'static str> {
    let label = t("Choose a quick phrase…");
    let toggled = ui.button(field, label, true);
    if toggled {
        *open = !*open;
    }
    if !*open {
        return None;
    }
    let row_h = 26.0;
    let n = QUICK_CHAT.len() as f32;
    let w = QUICK_CHAT
        .iter()
        .map(|p| ui.text_size(t(p), 13.0, false).x + 24.0)
        .fold(field.w.min(260.0), f32::max)
        .min(crate::ui::vw() - 16.0);
    let h = n * row_h + 12.0;
    let y = (field.y - h - 6.0).max(8.0);
    let x = field.x.min(crate::ui::vw() - w - 8.0).max(8.0);
    let list = Rect::new(x, y, w, h);
    let mut chosen = None;
    ui.panel(list);
    for (k, p) in QUICK_CHAT.iter().enumerate() {
        let row =
            Rect::new(list.x + 4.0, list.y + 6.0 + k as f32 * row_h, list.w - 8.0, row_h - 2.0);
        let lit = ui.hover_anim(row, ui.hovering(row));
        if lit > 0.01 {
            rrect(row, 5.0, Color::new(1.0, 1.0, 1.0, 0.08 * lit));
        }
        let (text, size) = ui.fitted(t(p), 13.0, row.w - 16.0, false);
        ui.text(&text, row.x + 8.0, text_y(row.y + row.h / 2.0, size), size, CREAM, false);
        if !toggled && ui.clicked(row) {
            chosen = Some(*p);
        }
    }
    rrect_lines(list, 8.0, 1.0, BORDER);
    if chosen.is_some() || (!toggled && ui.pressed_outside(&[list, field])) {
        *open = false;
    }
    chosen
}

/// What the player does outside online games, for their friends.
pub fn pastime(m: &Match) -> Pastime {
    if m.is_online() || m.is_replay() {
        Pastime::None
    } else if m.daily.is_some() {
        Pastime::Daily
    } else if m.drill.is_some() {
        Pastime::Drills
    } else if m.ladder.is_some() {
        Pastime::Ladder
    } else if m.free_play {
        Pastime::Practice
    } else if m.is_against_computer() || m.practice {
        Pastime::Computer
    } else {
        Pastime::None
    }
}

/// The opponent of this player's own online game (not one watched).
fn opponent(m: &Match) -> Option<(usize, &str)> {
    let o = m.online.as_ref().filter(|o| !o.spectator && o.corr.is_none())?;
    let seat = 1 - o.seat.min(1);
    Some((seat, m.names[seat].as_str()))
}

/// A line of the game's chat that isn't shown: the opponent's, muted.
pub fn muted_line(m: &Match, from: &str, config: &Config) -> bool {
    opponent(m).is_some_and(|(_, name)| name.eq_ignore_ascii_case(from) && config.muted(name, true))
}

/// A reaction that isn't shown: the opponent's, hidden.
pub fn muted_reaction(m: &Match, seat: usize, config: &Config) -> bool {
    opponent(m).is_some_and(|(s, name)| s == seat && config.muted(name, false))
}

/// What a friend is doing, in words ("Playing against Ana").
pub fn activity_text(a: &Activity) -> Option<String> {
    let rules = |r| crate::i18n::rules_name(r);
    Some(match a {
        Activity::Lounge | Activity::Other => return None,
        Activity::Searching { rules: r } => {
            tw("Looking for a game ({rules})", &[("rules", &rules(*r))])
        }
        Activity::AtTable { rules: r } => {
            tw("At a table, waiting ({rules})", &[("rules", &rules(*r))])
        }
        Activity::Playing { opponent, tournament: true, .. } => {
            tw("Playing a tournament game against {name}", &[("name", opponent)])
        }
        Activity::Playing { opponent, .. } => tw("Playing against {name}", &[("name", opponent)]),
        Activity::Watching { player } => tw("Watching {name}'s game", &[("name", player)]),
        Activity::Tournament { name } => {
            tw("In {name}", &[("name", &crate::i18n::tournament_name(name))])
        }
        Activity::VsComputer => t("Playing the computer").to_string(),
        Activity::Practice => t("Practising").to_string(),
        Activity::DailyChallenge => t("In the daily challenge").to_string(),
        Activity::Drills => t("Doing drills").to_string(),
        Activity::Ladder => t("On the tour").to_string(),
        Activity::Away => t("Away").to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dm(id: i64, from: &str, to: &str) -> DirectMessage {
        DirectMessage {
            id,
            from: from.into(),
            to: to.into(),
            text: format!("m{id}"),
            at: id,
            read: false,
            delivered: false,
        }
    }

    #[test]
    fn conversations_count_what_is_unread() {
        let mut d = Dms::default();
        assert_eq!(d.arrived(dm(1, "ana", "me"), "me"), Some("ana".into()));
        d.arrived(dm(2, "ana", "me"), "me");
        d.arrived(dm(3, "me", "bob"), "me");
        assert_eq!((d.unread("Ana"), d.unread("bob"), d.total_unread()), (2, 0, 2));
        assert_eq!(d.threads[0].with, "bob", "newest first");
        // Opening it reads it, asks for it, and what comes is shown at once.
        let asked = d.open_with("ana");
        assert!(asked.iter().any(|m| matches!(m, ClientMsg::DmRead { .. })));
        assert_eq!(d.unread("ana"), 0);
        assert_eq!(d.view.unread, 2, "the unread line goes above the two");
        assert_eq!(d.arrived(dm(4, "ana", "me"), "me"), None);
        assert_eq!(d.unread("ana"), 0);
        d.history("ana", vec![dm(1, "ana", "me"), dm(2, "ana", "me")], true, true, 0);
        let c = &d.conversations["ana"];
        assert_eq!(c.messages.iter().map(|m| m.id).collect::<Vec<_>>(), [1, 2, 4]);
        assert!(c.more && c.loaded && c.online);
    }

    #[test]
    fn a_message_goes_then_is_delivered_then_read() {
        let mut d = Dms::default();
        d.open_with("ana");
        d.next_local -= 1;
        d.conversations.entry("ana".into()).or_default().pending.push(Pending {
            local: d.next_local,
            text: "hi".into(),
            at: 5,
            sent: clock(),
            failed: false,
        });
        assert_eq!(d.sending_to("ana"), Some(Tick::Sending));
        let mut back = dm(9, "me", "ana");
        back.text = "hi".into();
        d.arrived(back, "me");
        let c = &d.conversations["ana"];
        assert!(c.pending.is_empty(), "the message came back from the server");
        assert_eq!(Tick::of(&c.messages[0]), Tick::Sent);
        d.status("Ana", 9, false);
        assert_eq!(Tick::of(&d.conversations["ana"].messages[0]), Tick::Delivered);
        d.status("ana", 9, true);
        assert_eq!(Tick::of(&d.conversations["ana"].messages[0]), Tick::Read);
        assert!(d.threads[0].last.read, "the list of conversations shows it too");
        // A later message of this player's isn't read by an older receipt.
        d.arrived(dm(12, "me", "ana"), "me");
        d.status("ana", 9, true);
        assert_eq!(Tick::of(&d.conversations["ana"].messages[1]), Tick::Sent);
    }

    #[test]
    fn a_refusal_fails_the_message_just_sent() {
        let mut d = Dms::default();
        d.conversations.entry("ana".into()).or_default().pending.push(Pending {
            local: -1,
            text: "hi".into(),
            at: 5,
            sent: clock(),
            failed: false,
        });
        d.refused();
        assert_eq!(d.sending_to("ana"), Some(Tick::Failed));
        // Sent again, it is on its way once more.
        let again = d.retry("ana", -1);
        assert!(matches!(again, Some(ClientMsg::DmSend { ref text, .. }) if text == "hi"));
        assert_eq!(d.sending_to("ana"), Some(Tick::Sending));
    }

    #[test]
    fn typing_shows_until_their_message_comes() {
        let mut d = Dms::default();
        d.typing_from("Ana");
        assert!(d.is_typing("ana"));
        d.arrived(dm(1, "ana", "me"), "me");
        assert!(!d.is_typing("ana"));
    }

    #[test]
    fn emoji_alone_are_drawn_large() {
        assert!(jumbo("👍"));
        assert!(jumbo(" 🎱🔥 "));
        assert!(!jumbo("ok 👍"));
        assert!(!jumbo("👍👍👍👍"));
        assert!(!jumbo("hello"));
    }

    #[test]
    fn every_activity_has_words() {
        let r = snook_legend_proto::Rules::Brazilian;
        for a in [
            Activity::Searching { rules: r },
            Activity::AtTable { rules: r },
            Activity::Playing {
                opponent: "a".into(),
                match_id: 1,
                rules: r,
                rated: true,
                tournament: false,
            },
            Activity::Watching { player: "b".into() },
            Activity::Tournament { name: "Cup".into() },
            Activity::VsComputer,
            Activity::Practice,
            Activity::DailyChallenge,
            Activity::Drills,
            Activity::Ladder,
            Activity::Away,
        ] {
            assert!(activity_text(&a).is_some_and(|s| !s.is_empty()), "{a:?}");
        }
        assert_eq!(activity_text(&Activity::Lounge), None);
    }
}
