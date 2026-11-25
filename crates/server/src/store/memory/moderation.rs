//! The inbox and the moderation in memory (the same rules as the database).

use std::collections::{BTreeMap, HashMap};

use snook_legend_proto::Rules;

use super::super::{
    BannedWord, LinkedAccount, MatchRow, NewNotification, NotificationRow, ProfileChangeRow,
    RatingAdjustment, ReportRow, StoreResult, User,
};
use super::{Inner, MemoryStore};

/// What this store keeps for the inbox and the moderation.
pub(super) struct State {
    notifications: BTreeMap<i64, NotificationRow>,
    next_notification: i64,
    /// (lang, word) -> (added by, when)
    words: BTreeMap<(String, String), (Option<i64>, i64)>,
    /// (user, kind, old, new, at, by), oldest first.
    changes: Vec<(i64, String, String, String, i64, Option<i64>)>,
    /// (adjustment, staff), oldest first.
    adjustments: Vec<(RatingAdjustment, i64)>,
    /// match id -> called off at
    voided: HashMap<i64, i64>,
    /// room -> shot times
    shot_times: HashMap<i64, Vec<Option<u32>>>,
}

impl Default for State {
    fn default() -> Self {
        State {
            notifications: BTreeMap::new(),
            next_notification: 0,
            // The starting list, as the database's migration has it.
            words: crate::wordfilter::SEED
                .iter()
                .map(|(w, l)| ((l.to_string(), w.to_string()), (None, 0)))
                .collect(),
            changes: Vec::new(),
            adjustments: Vec::new(),
            voided: HashMap::new(),
            shot_times: HashMap::new(),
        }
    }
}

impl Inner {
    /// A notification with its sender's name as it is now.
    fn named(&self, n: &NotificationRow) -> NotificationRow {
        let mut n = n.clone();
        n.from = n.from_id.map(|id| self.name(id)).unwrap_or_default();
        n
    }

    /// The highest rating `user_id` had under `rules` after a rated game
    /// not called off.
    fn best_rating(&self, user_id: i64, rules: Rules) -> i32 {
        let games = self.matches.iter().enumerate().filter_map(|(k, m)| {
            let seat = m.players.iter().position(|p| *p == user_id)?;
            (m.rated && m.rules == rules && !self.moderation.voided.contains_key(&(k as i64 + 1)))
                .then_some(m.ratings_after[seat])
        });
        games.max().unwrap_or(i32::MIN)
    }

    /// The ratings put right of `user_id` under `rules` as (when, rating).
    pub(super) fn adjusted_ratings(&self, user_id: i64, rules: Rules) -> Vec<(i64, i32)> {
        self.moderation
            .adjustments
            .iter()
            .filter(|(a, _)| a.user_id == user_id && a.rules == rules)
            .map(|(a, _)| (a.at, a.rating_after))
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn adjust(
        &mut self,
        user_id: i64,
        rules: Rules,
        delta: i32,
        reason: &str,
        match_id: Option<i64>,
        staff_id: i64,
        at: i64,
    ) -> Option<User> {
        let user = self.users.get_mut(&user_id)?;
        let r = user.records.get_mut(rules);
        r.rating += delta;
        r.peak_rating = if delta >= 0 {
            r.peak_rating.max(r.rating)
        } else {
            (r.peak_rating + delta).max(r.rating)
        };
        let rating_after = r.rating;
        let fresh = user.clone();
        let id = self.moderation.adjustments.len() as i64 + 1;
        let adjustment = RatingAdjustment {
            id,
            user_id,
            rules,
            delta,
            rating_after,
            reason: reason.to_string(),
            match_id,
            staff: String::new(),
            at,
        };
        self.moderation.adjustments.push((adjustment, staff_id));
        Some(fresh)
    }
}

impl MemoryStore {
    pub fn add_notification(&self, n: &NewNotification<'_>, keep: usize) -> StoreResult<i64> {
        let mut s = self.lock();
        let m = &mut s.moderation;
        m.next_notification += 1;
        let id = m.next_notification;
        m.notifications.insert(
            id,
            NotificationRow {
                id,
                user_id: n.user_id,
                kind: n.kind.to_string(),
                text: n.text.to_string(),
                from: String::new(),
                from_id: n.from_id,
                created_at: n.created_at,
                read_at: None,
                important: n.important,
                pinned_until: n.pinned_until,
                action: n.action.to_string(),
                deleted_at: None,
            },
        );
        // The oldest read ones that aren't pinned go, past `keep`.
        let mine: Vec<&NotificationRow> = m
            .notifications
            .values()
            .filter(|r| r.user_id == n.user_id && r.deleted_at.is_none())
            .collect();
        let extra = mine.len().saturating_sub(keep);
        let drop: Vec<i64> = mine
            .iter()
            .filter(|r| r.read_at.is_some() && r.pinned_until <= n.created_at)
            .take(extra)
            .map(|r| r.id)
            .collect();
        for id in drop {
            m.notifications.remove(&id);
        }
        Ok(id)
    }

    pub fn notifications(&self, user_id: i64, limit: u32) -> StoreResult<Vec<NotificationRow>> {
        let s = self.lock();
        Ok(s.moderation
            .notifications
            .values()
            .rev()
            .filter(|n| n.user_id == user_id && n.deleted_at.is_none())
            .take(limit as usize)
            .map(|n| s.named(n))
            .collect())
    }

    pub fn notifications_sent(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<NotificationRow>> {
        let s = self.lock();
        Ok(s.moderation
            .notifications
            .values()
            .rev()
            .filter(|n| n.user_id == user_id)
            .take(limit as usize)
            .map(|n| s.named(n))
            .collect())
    }

    pub fn read_notifications(&self, user_id: i64, ids: &[i64], at: i64) -> StoreResult<u64> {
        let mut s = self.lock();
        let mut n = 0;
        for id in ids {
            if let Some(row) = s.moderation.notifications.get_mut(id)
                && row.user_id == user_id
                && row.read_at.is_none()
                && row.deleted_at.is_none()
            {
                row.read_at = Some(at);
                n += 1;
            }
        }
        Ok(n)
    }

    pub fn delete_notifications(&self, user_id: i64, ids: &[i64], now: i64) -> StoreResult<u64> {
        let mut s = self.lock();
        let mut n = 0;
        for id in ids {
            if let Some(row) = s.moderation.notifications.get_mut(id)
                && row.user_id == user_id
                && row.deleted_at.is_none()
                && row.pinned_until <= now
                && !(row.important && row.read_at.is_none())
            {
                row.deleted_at = Some(now);
                n += 1;
            }
        }
        Ok(n)
    }

    pub fn banned_words(&self) -> StoreResult<Vec<BannedWord>> {
        let s = self.lock();
        Ok(s.moderation
            .words
            .iter()
            .map(|((lang, word), (by, at))| BannedWord {
                word: word.clone(),
                lang: lang.clone(),
                added_by: by.map(|id| s.name(id)).unwrap_or_default(),
                added_at: *at,
            })
            .collect())
    }

    pub fn add_banned_word(&self, word: &str, lang: &str, by: i64, at: i64) -> StoreResult<bool> {
        let mut s = self.lock();
        let key = (lang.to_string(), word.to_string());
        if s.moderation.words.contains_key(&key) {
            return Ok(false);
        }
        s.moderation.words.insert(key, (Some(by), at));
        Ok(true)
    }

    pub fn remove_banned_word(&self, word: &str, lang: &str) -> StoreResult<bool> {
        let mut s = self.lock();
        Ok(s.moderation.words.remove(&(lang.to_string(), word.to_string())).is_some())
    }

    pub fn add_profile_change(
        &self,
        user_id: i64,
        kind: &str,
        old: &str,
        new: &str,
        at: i64,
        by: Option<i64>,
    ) -> StoreResult<()> {
        let mut s = self.lock();
        s.moderation.changes.push((
            user_id,
            kind.to_string(),
            old.to_string(),
            new.to_string(),
            at,
            by,
        ));
        Ok(())
    }

    pub fn profile_changes(&self, user_id: i64, limit: u32) -> StoreResult<Vec<ProfileChangeRow>> {
        let s = self.lock();
        let mut list: Vec<ProfileChangeRow> = s
            .moderation
            .changes
            .iter()
            .filter(|c| c.0 == user_id)
            .map(|(user_id, kind, old, new, at, by)| ProfileChangeRow {
                user_id: *user_id,
                kind: kind.clone(),
                old: old.clone(),
                new: new.clone(),
                at: *at,
                by: by.map(|id| s.name(id)).unwrap_or_default(),
            })
            .collect();
        list.reverse();
        list.sort_by_key(|c| std::cmp::Reverse(c.at));
        list.truncate(limit as usize);
        Ok(list)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn adjust_rating(
        &self,
        user_id: i64,
        rules: Rules,
        delta: i32,
        reason: &str,
        match_id: Option<i64>,
        staff_id: i64,
        at: i64,
    ) -> StoreResult<Option<User>> {
        let mut s = self.lock();
        Ok(s.adjust(user_id, rules, delta, reason, match_id, staff_id, at))
    }

    pub fn rating_adjustments(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<RatingAdjustment>> {
        let s = self.lock();
        Ok(s.moderation
            .adjustments
            .iter()
            .rev()
            .filter(|(a, _)| a.user_id == user_id)
            .take(limit as usize)
            .map(|(a, staff)| RatingAdjustment { staff: s.name(*staff), ..a.clone() })
            .collect())
    }

    pub fn match_row(&self, id: i64) -> StoreResult<Option<MatchRow>> {
        let s = self.lock();
        let Some(m) =
            id.checked_sub(1).and_then(|i| usize::try_from(i).ok()).and_then(|i| s.matches.get(i))
        else {
            return Ok(None);
        };
        Ok(Some(MatchRow {
            id,
            players: m.players,
            names: m.players.map(|p| s.name(p)),
            winner: m.winner,
            reason: crate::reason_code(m.reason).to_string(),
            rated: m.rated,
            rules: m.rules,
            delta: m.delta,
            ratings_after: m.ratings_after,
            shots: m.shots,
            ended_at: m.ended_at,
            tournament: m.tournament,
            room: m.room,
            voided_at: s.moderation.voided.get(&id).copied(),
        }))
    }

    pub fn void_match(
        &self,
        id: i64,
        staff_id: i64,
        reason: &str,
        at: i64,
    ) -> StoreResult<Option<[Option<User>; 2]>> {
        let mut s = self.lock();
        let Some(m) = id
            .checked_sub(1)
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| s.matches.get(i))
            .cloned()
        else {
            return Ok(None);
        };
        if !m.rated || s.moderation.voided.contains_key(&id) {
            return Ok(None);
        }
        s.moderation.voided.insert(id, at);
        let mut out = [None, None];
        for (seat, slot) in out.iter_mut().enumerate() {
            let user_id = m.players[seat];
            let delta = m.delta[seat];
            let Some(old_peak) = s.users.get(&user_id).map(|u| u.records.get(m.rules).peak_rating)
            else {
                continue;
            };
            let Some(mut fresh) =
                s.adjust(user_id, m.rules, -delta, reason, Some(id), staff_id, at)
            else {
                continue;
            };
            // Out of the rated games (and the wins) it counted in; the peak
            // it brought is worked out again.
            let best = s.best_rating(user_id, m.rules);
            if let Some(user) = s.users.get_mut(&user_id) {
                let r = user.records.get_mut(m.rules);
                r.games = r.games.saturating_sub(1);
                if m.winner == seat {
                    r.wins = r.wins.saturating_sub(1);
                }
                // The game made the peak: it is what the player had before
                // it, or after another game, never above what it was.
                r.peak_rating = if old_peak == m.ratings_after[seat] {
                    best.max(m.ratings_after[seat] - delta).max(r.rating).min(old_peak)
                } else {
                    old_peak.max(r.rating)
                };
                fresh = user.clone();
            }
            *slot = Some(fresh);
        }
        Ok(Some(out))
    }

    pub fn save_shot_times(&self, room: i64, times: &[Option<u32>], _at: i64) -> StoreResult<()> {
        self.lock().moderation.shot_times.insert(room, times.to_vec());
        Ok(())
    }

    pub fn shot_times(&self, room: i64) -> StoreResult<Option<Vec<Option<u32>>>> {
        Ok(self.lock().moderation.shot_times.get(&room).cloned())
    }

    pub fn reports_by(&self, user_id: i64, limit: u32) -> StoreResult<Vec<ReportRow>> {
        let s = self.lock();
        Ok(s.reports
            .iter()
            .zip(&s.report_ids)
            .rev()
            .filter(|(_, (r, _))| *r == user_id)
            .map(|(r, _)| r.clone())
            .take(limit as usize)
            .collect())
    }

    /// Nothing: this store doesn't keep the addresses accounts were made from.
    pub fn linked_accounts(&self, _user_id: i64) -> StoreResult<Vec<LinkedAccount>> {
        Ok(Vec::new())
    }
}
