//! The lounge's social side: the "looking for a game" board, and sending
//! messages only to games new enough to understand them.
//!
//! A note on the board belongs to an online player: one each, taken down
//! when they start a game, leave, take it down themselves or after
//! [`SEEK_LIFETIME`]; the staff can take any down.

use std::collections::HashMap;
use std::time::Duration;

use snook_legend_proto::{
    OLDEST_PROTOCOL_VERSION, SOCIAL_PROTOCOL, SeekNote, ServerMsg, clean_seek,
};

use super::{Lounge, key};
use crate::now;

/// How long a note stays on the board (by default; see [`super::Rules`]).
pub const SEEK_LIFETIME: Duration = Duration::from_secs(30 * 60);
/// Most notes on the board at once (one per player; this only bounds the
/// board's size if very many people are online).
const MAX_SEEKS: usize = 200;
/// Posting a note: a few at once, then one every 20 seconds.
pub(super) const SEEK_BURST: f64 = 3.0;
pub(super) const SEEK_RATE: f64 = 1.0 / 20.0;

/// The notes on the board, by the poster's lower-case name.
#[derive(Default)]
pub(super) struct Seeks {
    notes: HashMap<String, SeekNote>,
}

/// The protocol version a session starts with until its game's version is
/// known (nothing new is sent to it).
pub(super) const UNKNOWN_PROTOCOL: u32 = OLDEST_PROTOCOL_VERSION;

impl Lounge {
    /// Records which protocol version a signed-in player's game speaks, and
    /// sends what only newer games understand (the "looking for a game"
    /// board). Call right after [`Lounge::join`].
    pub fn set_protocol(&mut self, name: &str, version: u32) {
        let k = key(name);
        let Some(s) = self.sessions.get_mut(&k) else { return };
        s.protocol = version;
        if version >= SOCIAL_PROTOCOL && !self.rules.lan {
            let blocked = &self.sessions[&k].blocked;
            let mut notes: Vec<SeekNote> = self
                .seeks
                .notes
                .iter()
                .filter(|(poster, _)| {
                    !self.sessions.get(*poster).is_some_and(|p| blocked.contains(&p.user.id))
                })
                .map(|(_, n)| n.clone())
                .collect();
            notes.sort_by_key(|n| std::cmp::Reverse((n.at, n.username.to_lowercase())));
            self.send(&k, ServerMsg::SeekBoard { notes });
        }
    }

    /// The protocol version of an online player's game.
    pub fn protocol_of(&self, name: &str) -> Option<u32> {
        self.sessions.get(&key(name)).map(|s| s.protocol)
    }

    /// Sends to everyone whose game speaks protocol `min` or newer.
    pub(super) fn broadcast_from(&self, min: u32, msg: &ServerMsg) {
        for (k, s) in &self.sessions {
            if s.protocol >= min {
                self.send(k, msg.clone());
            }
        }
    }

    /// The notes on the board (newest first), for the staff panel.
    pub fn seeks(&self) -> Vec<SeekNote> {
        let mut notes: Vec<SeekNote> = self.seeks.notes.values().cloned().collect();
        notes.sort_by_key(|n| std::cmp::Reverse((n.at, n.username.to_lowercase())));
        notes
    }

    /// A player puts (or replaces) their note on the board.
    pub(super) fn post_seek(&mut self, k: &str, text: &str, rated: bool) {
        if self.rules.lan {
            return;
        }
        let nowt = now();
        let Some(s) = self.sessions.get(k) else { return };
        if s.user.is_muted(nowt) {
            let mins = (s.user.muted_until.unwrap_or(nowt) - nowt + 59) / 60;
            return self.error(k, format!("You are muted for {mins} more minutes."));
        }
        if s.room.is_some() {
            return self.error(k, "Finish your game first.");
        }
        if self.tournament_busy(k) {
            return self.error(k, "Players in a tournament can't start other games.");
        }
        if !self.seeks.notes.contains_key(k) && self.seeks.notes.len() >= MAX_SEEKS {
            return self
                .error(k, "There are too many notes right now. Try again in a few minutes.");
        }
        if !self.sessions.get_mut(k).is_some_and(|s| s.seek_bucket.take()) {
            return self.error(k, "Slow down a little.");
        }
        let s = &self.sessions[k];
        // With the chat off: the note says nothing but the wish.
        let text = if self.chat_off(s) { String::new() } else { clean_seek(text) };
        let note = SeekNote {
            username: s.user.username.clone(),
            text,
            rated: rated && !s.guest,
            at: nowt,
            expires_at: nowt + self.rules.seek_lifetime.as_secs().max(1) as i64,
        };
        let poster = s.user.id;
        self.seeks.notes.insert(k.to_string(), note.clone());
        tracing::info!(player = k, rated = note.rated, "looking for a game: {}", note.text);
        let msg = ServerMsg::SeekPosted { note };
        for (other, session) in &self.sessions {
            if session.protocol >= SOCIAL_PROTOCOL && !session.blocked.contains(&poster) {
                self.send(other, msg.clone());
            }
        }
    }

    /// Takes `k`'s note off the board (if there is one) and tells everyone.
    pub(super) fn drop_seek(&mut self, k: &str) -> bool {
        match self.seeks.notes.remove(k) {
            Some(note) => {
                let msg = ServerMsg::SeekRemoved { username: note.username };
                self.broadcast_from(SOCIAL_PROTOCOL, &msg);
                true
            }
            None => false,
        }
    }

    /// The staff take a player's note off the board; false if there was none.
    pub fn remove_seek_of(&mut self, name: &str) -> bool {
        let k = key(name);
        let removed = self.drop_seek(&k);
        if removed {
            let text = "A moderator took down your note.";
            self.send(&k, ServerMsg::Notice { text: text.into() });
        }
        removed
    }

    /// Takes expired notes off the board.
    pub(super) fn expire_seeks(&mut self) {
        if self.seeks.notes.is_empty() {
            return;
        }
        let nowt = now();
        let expired: Vec<String> = self
            .seeks
            .notes
            .iter()
            .filter(|(_, n)| n.expires_at <= nowt)
            .map(|(k, _)| k.clone())
            .collect();
        for k in expired {
            self.drop_seek(&k);
        }
    }
}
