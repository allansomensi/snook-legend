//! Polls in the lounge chat: a player asks a question with a few options,
//! and everyone votes (and may change or take back their vote) until its
//! author or the staff close it. A poll is a chat line (its question is
//! the line's text) and lives as long as the line is kept; the votes are
//! counted by account.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use snook_legend_proto::moderation::TextPlace;
use snook_legend_proto::{ChatKind, ChatLine, Poll, ServerMsg, clean_text};

use super::{Lounge, now};

/// A player starts a poll at most this often.
const POLL_EVERY: Duration = Duration::from_secs(5 * 60);

impl Lounge {
    /// `k` asks the lounge `question`, with `options`.
    pub(super) fn create_poll(&mut self, k: &str, question: &str, options: &[String]) {
        let Some(s) = self.sessions.get(k) else { return };
        let author = s.user.id;
        // The options as they read, each once.
        let mut kept: Vec<String> = Vec::new();
        for o in options {
            let o = clean_text(o, Poll::OPTION_MAX);
            if !o.is_empty() && !kept.iter().any(|x| x.eq_ignore_ascii_case(&o)) {
                kept.push(o);
            }
        }
        if !Poll::OPTIONS.contains(&kept.len()) {
            return self.error(k, "A poll needs 2 to 6 different options.");
        }
        if self.last_poll.get(&author).is_some_and(|at| at.elapsed() < POLL_EVERY) {
            return self.error(k, "You can start another poll in a few minutes.");
        }
        // As the chat allows: the options' words, then the question (which
        // goes through the chat's own checks: muted, words, pace).
        for o in &kept {
            if let Some(refusal) = self.text_refusal(k, o) {
                return self.refuse(k, TextPlace::Chat, refusal);
            }
        }
        let question = clean_text(question, Poll::QUESTION_MAX);
        let Some(question) = self.chat_allowed(k, &question, TextPlace::Chat) else { return };
        let s = &self.sessions[k];
        let line = ChatLine {
            from: s.user.username.clone(),
            text: question,
            at: now(),
            kind: ChatKind::Player,
            rating: s.shown_rating(),
            admin: s.user.role == snook_legend_proto::Role::Admin,
            styles: s.styles(),
            poll: Some(Poll { votes: vec![0; kept.len()], options: kept, closed: false }),
            ..Default::default()
        };
        let line = self.push_chat(line);
        self.chat_authors.insert(line.id, author);
        self.last_poll.insert(author, Instant::now());
        self.poll_votes.insert(line.id, HashMap::new());
        tracing::info!(by = k, line = line.id, "poll started");
        let msg = ServerMsg::Chat { line };
        for (other, session) in &self.sessions {
            if !session.blocked.contains(&author) && !self.chat_off(session) {
                self.send(other, msg.clone());
            }
        }
    }

    /// `k` votes `option` in poll `id` (none: takes the vote back).
    pub(super) fn vote_poll(&mut self, k: &str, id: u64, option: Option<u8>) {
        let Some(s) = self.sessions.get_mut(k) else { return };
        if !s.action_bucket.take() {
            return;
        }
        let voter = s.user.id;
        if self.chat_removed.contains(&id) || self.chat_deleted.contains_key(&id) {
            return;
        }
        let Some(at) = self.chat.iter().position(|l| l.id == id && l.poll.is_some()) else {
            return;
        };
        let Some(poll) = self.chat[at].poll.as_ref() else { return };
        let n = poll.options.len();
        if poll.closed || option.is_some_and(|o| o as usize >= n) {
            return;
        }
        let votes = self.poll_votes.entry(id).or_default();
        let before = votes.get(&voter).copied();
        match option {
            Some(o) => votes.insert(voter, o),
            None => votes.remove(&voter),
        };
        let mut counts = vec![0u32; n];
        for &o in votes.values() {
            counts[o as usize] += 1;
        }
        if let Some(poll) = self.chat[at].poll.as_mut() {
            poll.votes = counts.clone();
        }
        self.send(k, ServerMsg::PollVoted { id, option });
        if before != option {
            self.broadcast(&ServerMsg::PollVotes { id, votes: counts, closed: false });
        }
    }

    /// Poll `id` takes no more votes: closed by its author or the staff.
    pub(super) fn close_poll(&mut self, k: &str, id: u64) {
        let Some(s) = self.sessions.get(k) else { return };
        let staff = s.user.role != snook_legend_proto::Role::Player;
        let own = self.chat_authors.get(&id).is_some_and(|a| *a == s.user.id);
        if !(own || staff) {
            return;
        }
        let Some(poll) = self.chat.iter_mut().find(|l| l.id == id).and_then(|l| l.poll.as_mut())
        else {
            return;
        };
        if poll.closed {
            return;
        }
        poll.closed = true;
        let votes = poll.votes.clone();
        self.broadcast(&ServerMsg::PollVotes { id, votes, closed: true });
    }

    /// `k`'s votes in the polls of the chat kept: (line, option).
    pub(super) fn my_votes(&self, k: &str) -> Vec<(u64, u8)> {
        let Some(s) = self.sessions.get(k) else { return Vec::new() };
        let mut out: Vec<(u64, u8)> = self
            .poll_votes
            .iter()
            .filter_map(|(&id, votes)| votes.get(&s.user.id).map(|&o| (id, o)))
            .collect();
        out.sort_unstable();
        out
    }
}
