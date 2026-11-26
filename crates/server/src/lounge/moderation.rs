//! The lounge's side of the moderation: the banned words (checked on every
//! line), why a line was turned away (worded by the game), the line the
//! staff pin at the top of the chat, and who a line mentions.

use std::collections::HashMap;

use snook_legend_proto::moderation::{MAX_MENTIONS, TextPlace, TextRefusal, mention_candidates};
use snook_legend_proto::{ChatKind, ChatLine, ServerMsg};

use super::{Lounge, key};
use crate::now;
use crate::wordfilter::{Filter, Hit};

/// The first protocol whose games word a refusal themselves
/// ([`ServerMsg::TextRefused`]) and have an inbox.
pub const INBOX_PROTOCOL: u32 = 25;

/// What the lounge keeps for the moderation.
pub struct Moderation {
    /// The banned words (the store has them; reloaded when the staff change
    /// the list).
    pub words: Filter,
    /// The line pinned at the top of the chat (a copy: it stays when older
    /// lines scroll away).
    pinned: Option<ChatLine>,
    /// Why each muted player (by account) was muted, as the staff wrote it.
    mute_reasons: HashMap<i64, String>,
}

impl Default for Moderation {
    fn default() -> Self {
        Moderation { words: Filter::seed(), pinned: None, mute_reasons: HashMap::new() }
    }
}

/// What a game older than [`INBOX_PROTOCOL`] is told instead (in English,
/// as the server's errors were).
pub fn refusal_text(refusal: &TextRefusal) -> String {
    match refusal {
        TextRefusal::Muted { until, .. } => {
            let mins = ((until - now()).max(0) + 59) / 60;
            format!("You are muted for {} more minutes.", mins.max(1))
        }
        TextRefusal::BannedWord { word } => format!("That word isn't allowed here: {word}."),
        TextRefusal::Other => "That can't be sent.".into(),
    }
}

impl Lounge {
    /// `name`'s game has an inbox and words refusals itself.
    pub fn has_inbox(&self, name: &str) -> bool {
        self.sessions.get(&key(name)).is_some_and(|s| s.protocol >= INBOX_PROTOCOL)
    }

    /// The protocol of `name`'s game (none: not signed in).
    pub fn protocol_of(&self, name: &str) -> Option<u32> {
        self.sessions.get(&key(name)).map(|s| s.protocol)
    }

    /// The banned words changed.
    pub fn set_words(&mut self, words: Filter) {
        self.moderation.words = words;
    }

    /// The banned word `text` has, if any.
    pub fn banned_word(&self, text: &str) -> Option<Hit> {
        self.moderation.words.check(text)
    }

    /// The banned word a username has, if any.
    pub fn banned_name(&self, name: &str) -> Option<Hit> {
        self.moderation.words.check_name(name)
    }

    /// Why `user_id` was muted (as the staff wrote it).
    pub fn set_mute_reason(&mut self, user_id: i64, reason: &str) {
        if reason.is_empty() {
            self.moderation.mute_reasons.remove(&user_id);
        } else {
            self.moderation.mute_reasons.insert(user_id, reason.to_string());
        }
    }

    /// The refusal for a player muted until `until`.
    pub fn muted(&self, user_id: i64, until: i64) -> TextRefusal {
        let reason = self.moderation.mute_reasons.get(&user_id).cloned().unwrap_or_default();
        TextRefusal::Muted { until, reason }
    }

    /// What tells `name`'s game that what they wrote (`place`) was turned
    /// away: worded by the game, or in English for an older one.
    pub fn refusal_msg(&self, name: &str, place: TextPlace, refusal: TextRefusal) -> ServerMsg {
        if self.has_inbox(name) {
            ServerMsg::TextRefused { place, refusal }
        } else {
            ServerMsg::Error { message: refusal_text(&refusal) }
        }
    }

    /// Tells `k` that what they wrote was turned away.
    pub(super) fn refuse(&self, k: &str, place: TextPlace, refusal: TextRefusal) {
        self.send(k, self.refusal_msg(k, place, refusal));
    }

    /// Why `k` may not write now (muted, or a banned word in `text`).
    pub(super) fn text_refusal(&self, k: &str, text: &str) -> Option<TextRefusal> {
        let s = self.sessions.get(k)?;
        let t = now();
        if s.user.is_muted(t) {
            return Some(self.muted(s.user.id, s.user.muted_until.unwrap_or(t)));
        }
        self.banned_word(text).map(|h| h.refusal())
    }

    /// The pinned line as players see it (none once it was deleted).
    fn pinned_line(&self) -> Option<ChatLine> {
        let line = self.moderation.pinned.as_ref()?;
        let gone = self.chat_removed.contains(&line.id) || self.chat_deleted.contains_key(&line.id);
        (!gone).then(|| line.clone())
    }

    /// Tells `k` which line is pinned (after signing in).
    pub(super) fn send_pinned(&self, k: &str) {
        let Some(s) = self.sessions.get(k) else { return };
        // With the chat off, a player's line isn't shown (the staff's are).
        let line = self.pinned_line().filter(|l| {
            !(self.chat_off(s) && l.kind == ChatKind::Player)
                && !self.chat_authors.get(&l.id).is_some_and(|a| s.blocked.contains(a))
        });
        if line.is_some() || s.protocol >= INBOX_PROTOCOL {
            self.send(k, ServerMsg::ChatPinned { line });
        }
    }

    fn broadcast_pinned(&self) {
        let keys: Vec<String> = self.sessions.keys().cloned().collect();
        for k in keys {
            self.send_pinned(&k);
        }
    }

    /// Pins lounge chat line `id` for everyone (the staff); the line, or
    /// None if there is no such line (or it was deleted).
    pub fn pin_chat(&mut self, id: u64) -> Option<ChatLine> {
        let line = self
            .chat
            .iter()
            .find(|l| l.id == id && l.id != 0)
            .filter(|l| {
                !self.chat_removed.contains(&l.id) && !self.chat_deleted.contains_key(&l.id)
            })
            .cloned()?;
        self.moderation.pinned = Some(line.clone());
        self.broadcast_pinned();
        Some(line)
    }

    /// Takes the pinned line down; false if none was pinned.
    pub fn unpin_chat(&mut self) -> bool {
        if self.moderation.pinned.take().is_none() {
            return false;
        }
        self.broadcast_pinned();
        true
    }

    /// The pinned line, as the staff see it.
    pub fn pinned_chat(&self) -> Option<ChatLine> {
        self.moderation.pinned.clone()
    }

    /// Line `id` was deleted (or every line was cleared, `None`): if it was
    /// the pinned one, it is taken down.
    pub(super) fn unpin_deleted(&mut self, id: Option<u64>) {
        let pinned = self.moderation.pinned.as_ref().map(|l| l.id);
        if pinned.is_some() && (id.is_none() || id == pinned) {
            self.moderation.pinned = None;
            self.broadcast_pinned();
        }
    }

    /// The players `text` mentions: real usernames as they are written,
    /// those online and those `known` (looked up in the store).
    pub fn mentions_in(&self, text: &str, known: &[String]) -> Vec<String> {
        mention_candidates(text)
            .into_iter()
            .filter_map(|c| {
                self.sessions
                    .get(&c)
                    .filter(|s| !s.guest)
                    .map(|s| s.user.username.clone())
                    .or_else(|| known.iter().find(|k| k.eq_ignore_ascii_case(&c)).cloned())
            })
            .take(MAX_MENTIONS)
            .collect()
    }

    /// Whether `name`'s next chat line would be within the chat's limit
    /// (nothing is counted): the mentions in it are looked up only then.
    pub fn may_chat_now(&self, name: &str) -> bool {
        self.sessions.get(&key(name)).is_some_and(|s| s.chat_bucket.has_token())
    }

    /// The lounge chat with the mentions of offline players looked up
    /// (`known`: the usernames found).
    pub fn chat_with_mentions(&mut self, name: &str, text: &str, known: Vec<String>) {
        let k = key(name);
        self.chat_mentioning(&k, text, &known);
    }

    /// An announcement of the staff in the lounge chat, its mentions looked
    /// up (`known`).
    pub fn announce_mentioning(&mut self, text: &str, known: &[String]) {
        let mentions = self.mentions_in(text, known);
        self.announce_line(text, mentions);
    }
}

/// The think times of a game's strokes (`think`: (stroke, tenths)), one per
/// stroke, None for what isn't a shot or wasn't timed.
pub fn shot_times(think: &[(usize, u32)], strokes: usize) -> Vec<Option<u32>> {
    let mut out = vec![None; strokes];
    for &(stroke, tenths) in think {
        if let Some(slot) = out.get_mut(stroke) {
            *slot = Some(tenths);
        }
    }
    out
}

/// A live game the staff ended (see [`Lounge::staff_end_match`]).
#[derive(Debug, Clone, PartialEq)]
pub struct EndedByStaff {
    /// The players' usernames and accounts.
    pub names: [String; 2],
    pub ids: [i64; 2],
    pub guests: [bool; 2],
}

impl Lounge {
    /// The username of account `user_id` if they are online (not a guest).
    pub fn online_name(&self, user_id: i64) -> Option<String> {
        self.sessions
            .values()
            .find(|s| !s.guest && s.user.id == user_id)
            .map(|s| s.user.username.clone())
    }

    /// How long each stroke of live game `id` was thought over so far.
    pub fn room_shot_times(&self, id: u64) -> Option<Vec<Option<u32>>> {
        let room = self.rooms.get(&id)?;
        Some(shot_times(&room.think, room.replay.strokes.len()))
    }

    /// The staff end a stuck live game: given to seat `winner` (the other
    /// resigns it, the rating as for any resignation), or (None) ended
    /// without a result: nothing is recorded, and the players, sent back to
    /// sign in, find themselves in the lounge.
    pub fn staff_end_match(
        &mut self,
        id: u64,
        winner: Option<usize>,
    ) -> Result<EndedByStaff, &'static str> {
        let room = self.rooms.get_mut(&id).ok_or("That game is no longer on this server.")?;
        if room.finished {
            return Err("That game is over already.");
        }
        let ended = EndedByStaff {
            names: [room.users[0].username.clone(), room.users[1].username.clone()],
            ids: [room.users[0].id, room.users[1].id],
            guests: room.guests,
        };
        match winner {
            Some(seat) => {
                if seat > 1 {
                    return Err("Give the game to seat 0 or 1.");
                }
                if room.busy.is_some() {
                    return Err("A shot is being played: try again in a moment.");
                }
                let loser = 1 - seat;
                room.game.resign(loser);
                room.replay
                    .strokes
                    .push(snook_legend_proto::replay::Stroke::Resign { seat: loser });
                self.finish(id);
            }
            None => {
                if room.tournament.is_some() {
                    return Err(
                        "A tournament's game needs a winner: give it to one of the players.",
                    );
                }
                let room = self.rooms.remove(&id).expect("room");
                self.finished_rooms.insert(id, std::time::Instant::now());
                self.featured_dirty = true;
                self.effect(super::Effect::DeleteLive(id as i64));
                for k in &room.keys {
                    self.end_watching_of(k, id);
                    if let Some(s) = self.sessions.get_mut(k)
                        && s.room == Some(id)
                    {
                        s.room = None;
                    }
                    self.kick(k, "The staff ended your game without a result. Sign in again.");
                }
                tracing::info!(room = id, "game ended by the staff without a result");
            }
        }
        Ok(ended)
    }
}
