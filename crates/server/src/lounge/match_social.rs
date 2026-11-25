//! The lounge's part of 0.12's match and social features: what friends see
//! each other doing, the rating at stake in a rated game, the spectators'
//! own chat and the chat kept to the quick phrases.

use std::collections::HashSet;

use snook_legend_core::Rules;
use snook_legend_core::rating::{self, EloConfig};
use snook_legend_proto::activity::{Activity, Pastime};
use snook_legend_proto::{ChatKind, ChatLine, ServerMsg};

use super::{Lounge, MATCH_CHAT_KEEP, key};
use crate::now;
use crate::store::User;

/// What a player whose chat is kept to the quick phrases is told when they
/// write anything else.
pub const PHRASES_ONLY: &str = "Your chat is set to the quick phrases only.";

/// Whether `user` may send only the quick phrases: a parent or guardian
/// chose so when they turned the chat of an account under 18 on.
pub fn phrases_only(user: &User) -> bool {
    user.minor == Some(true) && user.chat_phrases_only
}

impl Lounge {
    /// What a rated game between `keys` (with these accounts, under
    /// `rules`) puts at stake for each seat: `[win, loss]`, reckoned as the
    /// result will be (the same Elo, halved and then nothing when the two
    /// played each other often today). Leaving the game costs more (see
    /// [`super::ABANDON_RATING`]): that is not a result anyone plans.
    pub(super) fn stakes_for(
        &mut self,
        keys: &[String; 2],
        users: &[User; 2],
        rules: Rules,
    ) -> [[i32; 2]; 2] {
        let scale = match self.pair_games_today(keys, true) {
            0..=2 => 1.0,
            3..=4 => 0.5,
            _ => 0.0,
        };
        let config = EloConfig::default();
        let scaled = |d: i32| (d as f64 * scale).round() as i32;
        [0, 1].map(|seat| {
            let (me, other) = (users[seat].rating(rules), users[1 - seat].rating(rules));
            let win = rating::apply_result(&config, me, other).winner_delta;
            let loss = rating::apply_result(&config, other, me).loser_delta;
            [scaled(win), scaled(loss)]
        })
    }

    /// The online player `k` is doing (None: not online).
    pub fn activity(&self, k: &str) -> Option<Activity> {
        let s = self.sessions.get(k)?;
        if let Some(room) = s.room.and_then(|id| self.rooms.get(&id)).filter(|r| !r.finished) {
            let seat = room.seat_of(k);
            return Some(Activity::Playing {
                opponent: room.users[1 - seat].username.clone(),
                match_id: room.id,
                rules: room.game.rules(),
                rated: room.rated,
                tournament: room.tournament.is_some(),
            });
        }
        if let Some(player) = &s.watching {
            return Some(Activity::Watching { player: self.display(player) });
        }
        if let Some(q) = self.queue.iter().find(|q| q.key == k) {
            return Some(Activity::Searching { rules: q.rules });
        }
        if let Some(t) = s.table.and_then(|id| self.tables.get(&id)) {
            return Some(Activity::AtTable { rules: t.rules });
        }
        if let Some(t) = self.in_tournament(k).and_then(|id| self.tournaments.get(&id)) {
            return Some(Activity::Tournament { name: self.tournament_info(t).name });
        }
        if s.away {
            return Some(Activity::Away);
        }
        Some(match s.pastime {
            Pastime::Daily => Activity::DailyChallenge,
            Pastime::Drills => Activity::Drills,
            Pastime::Ladder => Activity::Ladder,
            Pastime::Practice => Activity::Practice,
            Pastime::Computer => Activity::VsComputer,
            Pastime::None | Pastime::Other if s.vs_computer => Activity::VsComputer,
            Pastime::None | Pastime::Other => Activity::Lounge,
        })
    }

    /// Tells `k`'s friends online what `k` is doing now.
    pub(super) fn tell_friends(&self, k: &str) {
        let Some(s) = self.sessions.get(k) else { return };
        if s.friends.is_empty() {
            return;
        }
        let Some(activity) = self.activity(k) else { return };
        let msg = ServerMsg::FriendActivity { username: s.user.username.clone(), activity };
        for f in &s.friends {
            // (Only a friend who has this player as a friend too: a list
            // being changed is caught up by the next one.)
            if self.sessions.get(f).is_some_and(|x| x.friends.contains(k)) {
                self.send(f, msg.clone());
            }
        }
    }

    /// Tells `name`'s friends online what `name` is doing now.
    pub fn tell(&self, name: &str) {
        self.tell_friends(&key(name));
    }

    /// `name`'s friends (usernames), as the store has them now.
    pub fn set_friends(&mut self, name: &str, friends: impl IntoIterator<Item = String>) {
        let k = key(name);
        let Some(s) = self.sessions.get_mut(&k) else { return };
        s.friends = friends.into_iter().map(|f| key(&f)).collect::<HashSet<_>>();
    }

    /// A parent or guardian kept `name`'s chat to the quick phrases (or not).
    pub fn set_phrases_only(&mut self, name: &str, on: bool) {
        if let Some(s) = self.sessions.get_mut(&key(name)) {
            s.user.chat_phrases_only = on;
        }
    }

    /// The game `k` is watching (the friend may have dropped for a moment:
    /// their game goes on, and so does watching it).
    pub(super) fn watched_room(&self, k: &str) -> Option<u64> {
        let friend = self.sessions.get(k)?.watching.clone()?;
        if let Some(id) = self.sessions.get(&friend).and_then(|f| f.room) {
            return Some(id);
        }
        self.rooms.values().find(|r| !r.finished && r.keys.contains(&friend)).map(|r| r.id)
    }

    /// A spectator's line: to the others watching the game (never to its
    /// players, whose chat is their own), as the chat's rules allow.
    pub(super) fn spectator_chat(&mut self, k: &str, text: &str) {
        let Some(id) = self.watched_room(k) else {
            return self.error(k, "You aren't watching a game.");
        };
        let place = snook_legend_proto::moderation::TextPlace::SpectatorChat;
        let Some(text) = self.chat_allowed(k, text, place) else { return };
        let s = &self.sessions[k];
        let line = ChatLine {
            from: s.user.username.clone(),
            text,
            at: now(),
            kind: ChatKind::Player,
            rating: s.shown_rating(),
            admin: s.user.role == snook_legend_proto::Role::Admin,
            ..Default::default()
        };
        let sender = s.user.id;
        let Some(room) = self.rooms.get_mut(&id) else { return };
        room.spectator_chat.push_back(line.clone());
        while room.spectator_chat.len() > MATCH_CHAT_KEEP {
            room.spectator_chat.pop_front();
        }
        let msg = ServerMsg::SpectatorChat { line };
        for w in self.spectators(id) {
            let hidden = self
                .sessions
                .get(&w)
                .is_none_or(|s| s.blocked.contains(&sender) || self.chat_off(s));
            if !hidden {
                self.send(&w, msg.clone());
            }
        }
    }

    /// The spectators' chat so far, for `w` who just started watching room
    /// `id` (without the lines of those they blocked).
    pub(super) fn send_spectator_chat(&self, w: &str, id: u64) {
        let (Some(room), Some(s)) = (self.rooms.get(&id), self.sessions.get(w)) else { return };
        if self.chat_off(s) {
            return;
        }
        let blocked_names: HashSet<String> = self
            .sessions
            .values()
            .filter(|x| s.blocked.contains(&x.user.id))
            .map(|x| key(&x.user.username))
            .collect();
        for line in &room.spectator_chat {
            if !blocked_names.contains(&key(&line.from)) {
                self.send(w, ServerMsg::SpectatorChat { line: line.clone() });
            }
        }
    }
}
