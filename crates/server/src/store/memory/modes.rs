//! Doubles and against the clock in memory (the same rules as the
//! database: see `store/postgres/modes.rs` and migration 0031).

use std::collections::{BTreeMap, HashMap};

use snook_legend_proto::{LeaderboardEntry, PerRules, Role, Rules};

use super::super::{
    AttemptFinish, AttemptRow, BestRow, DOUBLES_START, DoublesRecord, Record, StoreResult,
};
use super::MemoryStore;

/// An attempt against the clock.
#[derive(Debug, Clone, Copy)]
struct Attempt {
    id: i64,
    user_id: i64,
    day: i64,
    seed: u64,
    started_ms: i64,
    finished: bool,
}

#[derive(Default)]
pub(super) struct State {
    /// (user, rules) -> doubles record.
    doubles: HashMap<(i64, Rules), Record>,
    doubles_games: Vec<DoublesRecord>,
    attempts: Vec<Attempt>,
    next_attempt: i64,
    /// (week, user) -> (best ms, when)
    bests: BTreeMap<(i64, i64), (u32, i64)>,
    /// (week, place) -> user
    prizes: BTreeMap<(i64, u32), i64>,
}

impl State {
    fn record(&self, user_id: i64, rules: Rules) -> Record {
        self.doubles.get(&(user_id, rules)).copied().unwrap_or(Record::new(DOUBLES_START))
    }
}

impl MemoryStore {
    pub fn doubles_records(&self, user_id: i64) -> StoreResult<PerRules<Record>> {
        let s = self.lock();
        Ok(PerRules::new(
            s.modes.record(user_id, Rules::Brazilian),
            s.modes.record(user_id, Rules::Official),
        ))
    }

    pub fn record_doubles(&self, record: &DoublesRecord) -> StoreResult<bool> {
        let mut s = self.lock();
        if record.room.is_some() && s.modes.doubles_games.iter().any(|g| g.room == record.room) {
            return Ok(false);
        }
        if record.rated {
            for (team, players) in record.players.iter().enumerate() {
                for (k, &id) in players.iter().enumerate() {
                    let mut r = s.modes.record(id, record.rules);
                    r.rating += record.deltas[team][k];
                    r.peak_rating = r.peak_rating.max(r.rating);
                    r.games += 1;
                    r.wins += u32::from(team == record.winner);
                    s.modes.doubles.insert((id, record.rules), r);
                }
            }
        }
        s.modes.doubles_games.push(record.clone());
        Ok(true)
    }

    pub fn doubles_leaderboard(
        &self,
        rules: Rules,
        limit: u32,
        now: i64,
    ) -> StoreResult<Vec<LeaderboardEntry>> {
        let s = self.lock();
        let mut rows: Vec<(&super::User, Record)> = s
            .modes
            .doubles
            .iter()
            .filter(|((_, r), rec)| *r == rules && rec.games > 0)
            .filter_map(|((id, _), rec)| Some((s.users.get(id)?, *rec)))
            .filter(|(u, _)| !u.is_banned(now) && u.deleted_at.is_none())
            .collect();
        rows.sort_by(|(ua, a), (ub, b)| {
            b.rating.cmp(&a.rating).then(b.wins.cmp(&a.wins)).then(ua.username.cmp(&ub.username))
        });
        Ok(rows
            .into_iter()
            .take(limit as usize)
            .enumerate()
            .map(|(i, (u, r))| LeaderboardEntry {
                rank: i as u32 + 1,
                username: u.username.clone(),
                rating: r.rating,
                games: r.games,
                wins: r.wins,
                country: u.country.clone(),
                vip: s.wallets.get(&u.id).is_some_and(|w| w.vip_until > now),
                admin: u.role == Role::Admin,
            })
            .collect())
    }

    pub fn time_attack_start(
        &self,
        user_id: i64,
        day: i64,
        now_ms: i64,
        seed: u64,
        max: u32,
    ) -> StoreResult<Option<i64>> {
        let mut s = self.lock();
        let m = &mut s.modes;
        let used = m.attempts.iter().filter(|a| a.user_id == user_id && a.day == day).count();
        if used as u32 >= max {
            return Ok(None);
        }
        // One attempt at a time: the one under way is given up.
        for a in m.attempts.iter_mut().filter(|a| a.user_id == user_id) {
            a.finished = true;
        }
        m.next_attempt += 1;
        let id = m.next_attempt;
        m.attempts.push(Attempt { id, user_id, day, seed, started_ms: now_ms, finished: false });
        Ok(Some(id))
    }

    pub fn time_attack_open(&self, user_id: i64) -> StoreResult<Option<AttemptRow>> {
        let s = self.lock();
        Ok(s.modes
            .attempts
            .iter()
            .rev()
            .find(|a| a.user_id == user_id && !a.finished)
            .map(|a| AttemptRow { id: a.id, seed: a.seed, started_ms: a.started_ms }))
    }

    pub fn time_attack_attempts(&self, user_id: i64, day: i64) -> StoreResult<u32> {
        let s = self.lock();
        Ok(s.modes.attempts.iter().filter(|a| a.user_id == user_id && a.day == day).count() as u32)
    }

    pub fn time_attack_finish(
        &self,
        user_id: i64,
        id: i64,
        _now_ms: i64,
        week: i64,
        at: i64,
        result: Option<(u32, u32, u32)>,
    ) -> StoreResult<AttemptFinish> {
        let mut s = self.lock();
        let m = &mut s.modes;
        let Some(a) =
            m.attempts.iter_mut().find(|a| a.id == id && a.user_id == user_id && !a.finished)
        else {
            return Ok(AttemptFinish::NotStarted);
        };
        a.finished = true;
        let Some((ms, _, _)) = result else { return Ok(AttemptFinish::Counted { best: false }) };
        let best = m.bests.get(&(week, user_id)).is_none_or(|(b, _)| ms < *b);
        if best {
            m.bests.insert((week, user_id), (ms, at));
        }
        Ok(AttemptFinish::Counted { best })
    }

    pub fn time_attack_board(&self, week: i64, limit: u32) -> StoreResult<(Vec<BestRow>, u32)> {
        let s = self.lock();
        let rows = sorted_bests(&s, week);
        let players = rows.len() as u32;
        let board = rows
            .into_iter()
            .take(limit as usize)
            .filter_map(|(user_id, ms, at)| {
                let u = s.users.get(&user_id)?;
                Some(BestRow {
                    user_id,
                    username: u.username.clone(),
                    country: u.country.clone(),
                    ms,
                    at,
                })
            })
            .collect();
        Ok((board, players))
    }

    pub fn time_attack_best(
        &self,
        user_id: i64,
        week: i64,
    ) -> StoreResult<Option<(u32, Option<u32>)>> {
        let s = self.lock();
        let Some(&(ms, _)) = s.modes.bests.get(&(week, user_id)) else { return Ok(None) };
        let rank = sorted_bests(&s, week).iter().position(|(u, ..)| *u == user_id);
        Ok(Some((ms, rank.map(|k| k as u32 + 1))))
    }

    pub fn time_attack_prize(
        &self,
        week: i64,
        place: u32,
        user_id: i64,
        _coins: i64,
        _ms: u32,
        _at: i64,
    ) -> StoreResult<bool> {
        let mut s = self.lock();
        if s.modes.prizes.contains_key(&(week, place)) {
            return Ok(false);
        }
        s.modes.prizes.insert((week, place), user_id);
        Ok(true)
    }
}

/// A week's bests, quickest first (the earlier on a tie), without the
/// banned and deleted: (user, ms, when).
fn sorted_bests(s: &super::Inner, week: i64) -> Vec<(i64, u32, i64)> {
    let mut rows: Vec<(i64, u32, i64)> = s
        .modes
        .bests
        .range((week, i64::MIN)..=(week, i64::MAX))
        .filter(|&(&(_, u), _)| !s.off_boards(u))
        .map(|(&(_, u), &(ms, at))| (u, ms, at))
        .collect();
    rows.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)).then(a.0.cmp(&b.0)));
    rows
}
