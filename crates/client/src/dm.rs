//! Direct messages between friends (see `snook_legend_proto::dm`): the
//! conversations, the panel that shows one (opened from the friends list),
//! the unread counts, and what the chat's settings do at the table (the
//! muted opponents, the quick phrases only).

use std::collections::HashMap;

use macroquad::prelude::*;

use snook_legend_proto::activity::{Activity, Pastime};
use snook_legend_proto::dm::{DirectMessage, DmThread, MAX_DM_LEN};
use snook_legend_proto::{ChatKind, ChatLine, ClientMsg, QUICK_CHAT};

use crate::config::Config;
use crate::game::Match;
use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::ui::{BORDER, CREAM, FeedLine, GOLD, MUTED, Ui, rrect, rrect_lines};

/// The conversation's text field and its scroll bar.
const FIELD: u64 = 9_310;
const LOG: u64 = 9_311;

/// One conversation, as far as it was read from the server.
#[derive(Debug, Default)]
struct Conversation {
    /// Oldest first.
    messages: Vec<DirectMessage>,
    /// Older messages are on the server.
    more: bool,
    /// Its newest messages came (a message arriving before that is kept
    /// until they do).
    loaded: bool,
}

/// The player's conversations with their friends.
#[derive(Debug, Default)]
pub struct Dms {
    /// Newest first.
    threads: Vec<DmThread>,
    conversations: HashMap<String, Conversation>,
    /// The friend whose conversation is open.
    pub open: Option<String>,
    input: String,
    scroll: usize,
    /// The quick phrases' list is open (the chat kept to them).
    phrases_open: bool,
    /// Older messages were asked for (one page at a time).
    asked_older: bool,
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

    /// A message to or from `me` came. Returns the friend's name when it is
    /// theirs and their conversation isn't open (the game tells the player).
    pub fn arrived(&mut self, message: DirectMessage, me: &str) -> Option<String> {
        let mine = message.from.eq_ignore_ascii_case(me);
        let with = if mine { message.to.clone() } else { message.from.clone() };
        let open = self.open.as_ref().is_some_and(|o| o.eq_ignore_ascii_case(&with));
        let c = self.conversations.entry(key(&with)).or_default();
        if !c.messages.iter().any(|m| m.id == message.id) {
            c.messages.push(message.clone());
            c.messages.sort_by_key(|m| m.id);
            if open {
                self.scroll = 0;
            }
        }
        let unread = match self.threads.iter().position(|t| t.with.eq_ignore_ascii_case(&with)) {
            Some(i) => self.threads.remove(i).unread,
            None => 0,
        };
        let unread = if open {
            0
        } else if mine {
            unread
        } else {
            unread + 1
        };
        self.threads.insert(0, DmThread { with: with.clone(), last: message, unread });
        (!mine && !open).then_some(with)
    }

    /// A page of a conversation came (the newest, or older ones).
    pub fn history(&mut self, with: &str, messages: Vec<DirectMessage>, more: bool) {
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
    }

    /// Opens the conversation with `name`: what to ask the server.
    pub fn open_with(&mut self, name: &str) -> Vec<ClientMsg> {
        self.open = Some(name.to_string());
        self.scroll = 0;
        self.input.clear();
        self.phrases_open = false;
        let mut out = vec![ClientMsg::DmHistory { with: name.to_string(), before: None }];
        if self.unread(name) > 0 {
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

    /// The open conversation: its messages and the field to answer. What
    /// to send to the server. `me`: this player; `can_write`: still friends
    /// (a conversation stays readable after a friendship ends);
    /// `phrases_only`: the chat kept to the quick phrases.
    pub fn draw(
        &mut self,
        ui: &mut Ui,
        me: &str,
        can_write: bool,
        phrases_only: bool,
    ) -> Vec<ClientMsg> {
        let mut out = Vec::new();
        let Some(with) = self.open.clone() else { return out };
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let w = 460.0f32.min(sw - 24.0);
        let h = (sh * 0.7).clamp(300.0, 560.0).min(sh - 24.0);
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        let _modal = ui.modal_scope(crate::ui::motion::id("dm"), 0.5);
        ui.panel(r);
        let title = tw("Messages with {name}", &[("name", &with)]);
        let (title, size) = ui.fitted(&title, 18.0, r.w - 70.0, true);
        ui.text(&title, r.x + 16.0, r.y + 14.0, size, GOLD, true);
        let close = Rect::new(r.x + r.w - 40.0, r.y + 10.0, 28.0, 28.0);
        let mut closing = ui.icon_button_ex(close, Icon::Close, t("Close"), true, true);
        let field_h = if can_write { 40.0 } else { 0.0 };
        let log = Rect::new(r.x + 10.0, r.y + 48.0, r.w - 20.0, r.h - 48.0 - field_h - 20.0);
        rrect(log, 6.0, Color::new(0.0, 0.0, 0.0, 0.25));
        let c = self.conversations.get(&key(&with));
        let loaded = c.is_some_and(|c| c.loaded);
        let more = c.is_some_and(|c| c.more);
        let lines: Vec<FeedLine> =
            c.map(|c| c.messages.iter().map(line_of).collect()).unwrap_or_default();
        if !loaded && lines.is_empty() {
            crate::online::loading(ui, log);
        } else if lines.is_empty() {
            let text = tw("No messages with {name} yet. Say hello!", &[("name", &with)]);
            crate::online::empty_state(ui, log, crate::online::Glyph::Chat, &text, None);
        } else {
            self.scroll = ui.chat_log(LOG, log, &lines, Some(me), false, self.scroll).0;
            // Scrolled to the oldest shown: the page before it.
            let top = self.scroll + 1 >= lines.len().saturating_sub(4);
            if more && top && !self.asked_older && self.scroll > 0 {
                let before = c.and_then(|c| c.messages.first()).map(|m| m.id);
                self.asked_older = true;
                out.push(ClientMsg::DmHistory { with: with.clone(), before });
            }
        }
        if can_write {
            let field =
                Rect::new(r.x + 10.0, r.y + r.h - 10.0 - field_h, r.w - 20.0 - 46.0, field_h);
            let send_r = Rect::new(field.x + field.w + 6.0, field.y, 40.0, field_h);
            let text = if phrases_only {
                self.input.clear();
                phrase_picker(ui, field, &mut self.phrases_open).map(str::to_string)
            } else {
                let enter = ui.text_input(FIELD, field, &mut self.input, MAX_DM_LEN);
                crate::online::placeholder(
                    ui,
                    FIELD,
                    field,
                    &self.input,
                    t("Write a message… (Enter to send)"),
                );
                let can = !self.input.trim().is_empty();
                let pressed = ui.icon_button(send_r, Icon::Play, t("Send"), can);
                ((enter || pressed) && can).then(|| std::mem::take(&mut self.input))
            };
            if let Some(text) = text {
                out.push(ClientMsg::DmSend { to: with.clone(), text });
                self.scroll = 0;
                ui.focus = (!phrases_only).then_some(FIELD);
            }
        } else {
            let note = t("You can write to your friends only.");
            ui.text_centered(note, r.x + r.w / 2.0, r.y + r.h - 28.0, 13.0, MUTED, false);
        }
        if !self.phrases_open && (ui.escape() || ui.dismissed(r)) {
            closing = true;
        }
        // Nothing under it takes a click.
        ui.block(Rect::new(0.0, 0.0, sw, sh));
        if closing {
            self.open = None;
            self.phrases_open = false;
            if ui.focus == Some(FIELD) {
                ui.focus = None;
            }
        }
        out
    }
}

/// A message as a line of the chat panel (a quick phrase in the reader's
/// language).
fn line_of(m: &DirectMessage) -> FeedLine {
    FeedLine::chat(&ChatLine {
        from: m.from.clone(),
        text: m.text.clone(),
        at: m.at,
        kind: ChatKind::Player,
        ..Default::default()
    })
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
        if ui.hovering(row) {
            rrect(row, 5.0, Color::new(1.0, 1.0, 1.0, 0.08));
        }
        let (text, size) = ui.fitted(t(p), 13.0, row.w - 16.0, false);
        ui.text(
            &text,
            row.x + 8.0,
            crate::ranking::text_y(row.y + row.h / 2.0, size),
            size,
            CREAM,
            false,
        );
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
        assert_eq!(d.arrived(dm(4, "ana", "me"), "me"), None);
        assert_eq!(d.unread("ana"), 0);
        d.history("ana", vec![dm(1, "ana", "me"), dm(2, "ana", "me")], true);
        let c = &d.conversations["ana"];
        assert_eq!(c.messages.iter().map(|m| m.id).collect::<Vec<_>>(), [1, 2, 4]);
        assert!(c.more && c.loaded);
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
