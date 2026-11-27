//! Settings, direct messages, phrases-only chat and weekly pot attempts in
//! memory (the same rules as the database, see `store/match_social.rs`).

use snook_legend_proto::Rules;
use snook_legend_proto::analysis::ShotAnalysis;

use super::super::{DM_KEPT, DM_THREADS, DmRow, DmThreadRow, StoreResult, WeekRow};
use super::MemoryStore;

/// Whether `m` is between `a` and `b` (either way).
fn between(m: &DmRow, a: i64, b: i64) -> bool {
    (m.from == a && m.to == b) || (m.from == b && m.to == a)
}

impl MemoryStore {
    pub fn account_settings(&self, user_id: i64) -> StoreResult<Option<(String, i64)>> {
        Ok(self.lock().account_settings.get(&user_id).cloned())
    }

    pub fn save_account_settings(&self, user_id: i64, data: &str, at: i64) -> StoreResult<()> {
        self.lock().account_settings.insert(user_id, (data.to_string(), at));
        Ok(())
    }

    pub fn dm_send(&self, from: i64, to: i64, text: &str, at: i64) -> StoreResult<DmRow> {
        let mut s = self.lock();
        s.next_dm += 1;
        let row = DmRow {
            id: s.next_dm,
            from,
            to,
            text: text.to_string(),
            at,
            read_at: None,
            delivered_at: None,
        };
        s.dms.push(row.clone());
        let count = s.dms.iter().filter(|m| between(m, from, to)).count();
        if count > DM_KEPT {
            let mut extra = count - DM_KEPT;
            s.dms.retain(|m| {
                if extra > 0 && between(m, from, to) {
                    extra -= 1;
                    false
                } else {
                    true
                }
            });
        }
        Ok(row)
    }

    pub fn dm_history(
        &self,
        a: i64,
        b: i64,
        before: Option<i64>,
        limit: u32,
    ) -> StoreResult<Vec<DmRow>> {
        let s = self.lock();
        Ok(s.dms
            .iter()
            .rev()
            .filter(|m| between(m, a, b) && before.is_none_or(|id| m.id < id))
            .take(limit as usize)
            .cloned()
            .collect())
    }

    pub fn dm_delivered(&self, id: i64, at: i64) -> StoreResult<bool> {
        let mut s = self.lock();
        let Some(m) = s.dms.iter_mut().find(|m| m.id == id && m.delivered_at.is_none()) else {
            return Ok(false);
        };
        m.delivered_at = Some(at);
        Ok(true)
    }

    pub fn dm_delivered_to(&self, to: i64, at: i64) -> StoreResult<Vec<(i64, i64)>> {
        let mut s = self.lock();
        let mut newest: Vec<(i64, i64)> = Vec::new();
        for m in s.dms.iter_mut().filter(|m| m.to == to && m.delivered_at.is_none()) {
            m.delivered_at = Some(at);
            match newest.iter_mut().find(|(from, _)| *from == m.from) {
                Some((_, up_to)) => *up_to = (*up_to).max(m.id),
                None => newest.push((m.from, m.id)),
            }
        }
        Ok(newest)
    }

    pub fn dm_read(&self, me: i64, with: i64, at: i64) -> StoreResult<Option<i64>> {
        let mut s = self.lock();
        let mut newest = None;
        for m in s.dms.iter_mut().filter(|m| m.from == with && m.to == me && m.read_at.is_none()) {
            m.read_at = Some(at);
            m.delivered_at.get_or_insert(at);
            newest = newest.max(Some(m.id));
        }
        Ok(newest)
    }

    pub fn dm_threads(&self, me: i64) -> StoreResult<Vec<DmThreadRow>> {
        let s = self.lock();
        let mut threads: Vec<DmThreadRow> = Vec::new();
        // Newest first: the first message seen of each conversation is its last.
        for m in s.dms.iter().rev().filter(|m| m.from == me || m.to == me) {
            let other = if m.from == me { m.to } else { m.from };
            let unread = (m.to == me && m.read_at.is_none()) as u32;
            match threads.iter_mut().find(|t| t.other == other) {
                Some(t) => t.unread += unread,
                None => threads.push(DmThreadRow { other, last: m.clone(), unread }),
            }
        }
        threads.truncate(DM_THREADS as usize);
        Ok(threads)
    }

    pub fn dm_waiting(&self, from: i64, to: i64) -> StoreResult<u32> {
        let s = self.lock();
        let answered =
            s.dms.iter().filter(|m| m.from == to && m.to == from).map(|m| m.id).max().unwrap_or(0);
        Ok(s.dms.iter().filter(|m| m.from == from && m.to == to && m.id > answered).count() as u32)
    }

    pub fn dm_started(&self, from: i64, since: i64) -> StoreResult<u32> {
        let s = self.lock();
        let mut seen: Vec<i64> = Vec::new();
        let mut started = 0;
        // Oldest first: the first message seen of each conversation is its first.
        for m in s.dms.iter().filter(|m| m.from == from || m.to == from) {
            let other = if m.from == from { m.to } else { m.from };
            if seen.contains(&other) {
                continue;
            }
            seen.push(other);
            if m.from == from && m.at >= since {
                started += 1;
            }
        }
        Ok(started)
    }

    pub fn set_phrases_only(&self, user_id: i64, on: bool) -> StoreResult<()> {
        if let Some(u) = self.lock().users.get_mut(&user_id) {
            u.chat_phrases_only = on;
        }
        Ok(())
    }

    pub fn add_shot_week(
        &self,
        user_id: i64,
        week: i64,
        attempts: u32,
        made: u32,
    ) -> StoreResult<()> {
        let mut s = self.lock();
        let e = s.shot_weeks.entry((user_id, week)).or_default();
        e.0 = e.0.saturating_add(1);
        e.1 = e.1.saturating_add(attempts);
        e.2 = e.2.saturating_add(made.min(attempts));
        Ok(())
    }

    pub fn shot_weeks(&self, user_id: i64, limit: u32) -> StoreResult<Vec<WeekRow>> {
        let s = self.lock();
        Ok(s.shot_weeks
            .range((user_id, i64::MIN)..=(user_id, i64::MAX))
            .rev()
            .take(limit as usize)
            .map(|(&(_, week), &(games, attempts, made))| WeekRow { week, games, attempts, made })
            .collect())
    }

    pub fn league_analysis(
        &self,
        rules: Rules,
        min: i32,
        max: i32,
    ) -> StoreResult<(ShotAnalysis, u32)> {
        let s = self.lock();
        let players: std::collections::HashSet<i64> = s
            .users
            .values()
            .filter(|u| u.deleted_at.is_none())
            .filter(|u| {
                let r = u.records.get(rules);
                r.games > 0 && (min..max).contains(&r.rating)
            })
            .map(|u| u.id)
            .collect();
        let rows = s
            .analysis
            .iter()
            .filter(|((u, ..), _)| players.contains(u))
            .map(|(&(_, kind, key), &(attempts, made))| (kind, key, attempts, made));
        let sums = ShotAnalysis::from_rows(rows.collect::<Vec<_>>());
        let counted =
            players.iter().filter(|u| s.analysis.keys().any(|(id, ..)| id == *u)).count() as u32;
        Ok((sums, counted))
    }
}
