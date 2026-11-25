//! Doubles and against the clock in memory (the same rules as the
//! database: see `store/postgres/modes.rs` and migration 0031).

use std::collections::{BTreeMap, HashMap};

use snook_legend_proto::{LeaderboardEntry, PerRules, Role, Rules};

use super::super::{
    AttemptFinish, AttemptRow, BestRow, DOUBLES_START, DoublesGame, DoublesRecord, Record,
    StoreResult, TimeAttempt,
};
use super::MemoryStore;

/// An attempt against the clock.
#[derive(Debug, Clone)]
struct Attempt {
    id: i64,
    user_id: i64,
    day: i64,
    seed: u64,
    started_ms: i64,
    finished: bool,
    finished_ms: Option<i64>,
    /// (ms, shots, fouls) of a cleared attempt.
    result: Option<(u32, u32, u32)>,
    /// (week, at) it counted in.
    counted: Option<(i64, i64)>,
    removed: Option<(i64, String)>,
}

/// A recorded doubles game.
#[derive(Debug, Clone)]
struct StoredDoubles {
    id: i64,
    record: DoublesRecord,
    voided_at: Option<i64>,
}

#[derive(Default)]
pub(super) struct State {
    /// (user, rules) -> doubles record.
    doubles: HashMap<(i64, Rules), Record>,
    doubles_games: Vec<StoredDoubles>,
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
        if record.room.is_some()
            && s.modes.doubles_games.iter().any(|g| g.record.room == record.room)
        {
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
        let id = s.modes.doubles_games.len() as i64 + 1;
        s.modes.doubles_games.push(StoredDoubles { id, record: record.clone(), voided_at: None });
        // Its copy saved for a restart is done with.
        if let Some(room) = record.room {
            s.live.remove(&room);
        }
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
        m.attempts.push(Attempt {
            id,
            user_id,
            day,
            seed,
            started_ms: now_ms,
            finished: false,
            finished_ms: None,
            result: None,
            counted: None,
            removed: None,
        });
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
        now_ms: i64,
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
        a.finished_ms = Some(now_ms);
        a.result = result;
        let Some((ms, _, _)) = result else { return Ok(AttemptFinish::Counted { best: false }) };
        a.counted = Some((week, at));
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

impl MemoryStore {
    fn doubles_game_of(&self, s: &super::Inner, g: &StoredDoubles) -> DoublesGame {
        let user = |id: i64| s.users.get(&id);
        let names = g
            .record
            .players
            .map(|t| t.map(|id| user(id).map(|u| u.username.clone()).unwrap_or_default()));
        let countries = g
            .record
            .players
            .map(|t| t.map(|id| user(id).map(|u| u.country.clone()).unwrap_or_default()));
        DoublesGame {
            id: g.id,
            record: g.record.clone(),
            names,
            countries,
            has_replay: g.record.replay.is_some(),
            voided_at: g.voided_at,
        }
    }

    pub fn doubles_games(
        &self,
        player: Option<i64>,
        before: Option<i64>,
        limit: u32,
    ) -> StoreResult<Vec<DoublesGame>> {
        let s = self.lock();
        Ok(s.modes
            .doubles_games
            .iter()
            .rev()
            .filter(|g| before.is_none_or(|b| g.id < b))
            .filter(|g| player.is_none_or(|p| g.record.players.iter().flatten().any(|&x| x == p)))
            .take(limit as usize)
            .map(|g| {
                let mut out = self.doubles_game_of(&s, g);
                out.record.replay = None;
                out
            })
            .collect())
    }

    pub fn doubles_game(&self, id: i64) -> StoreResult<Option<DoublesGame>> {
        let s = self.lock();
        Ok(s.modes.doubles_games.iter().find(|g| g.id == id).map(|g| self.doubles_game_of(&s, g)))
    }

    pub fn void_doubles(&self, id: i64, at: i64) -> StoreResult<bool> {
        let mut s = self.lock();
        let Some(k) = s.modes.doubles_games.iter().position(|g| g.id == id) else {
            return Ok(false);
        };
        let g = s.modes.doubles_games[k].clone();
        if !g.record.rated || g.voided_at.is_some() {
            return Ok(false);
        }
        for (team, players) in g.record.players.iter().enumerate() {
            for (p, &user) in players.iter().enumerate() {
                let delta = g.record.deltas[team][p];
                let mut r = s.modes.record(user, g.record.rules);
                if delta > 0 && r.peak_rating == r.rating {
                    r.peak_rating = (r.rating - delta).max(DOUBLES_START);
                }
                r.rating -= delta;
                r.peak_rating = r.peak_rating.max(r.rating);
                r.games = r.games.saturating_sub(1);
                r.wins = r.wins.saturating_sub(u32::from(team == g.record.winner));
                s.modes.doubles.insert((user, g.record.rules), r);
            }
        }
        s.modes.doubles_games[k].voided_at = Some(at);
        Ok(true)
    }

    fn attempt_of(s: &super::Inner, a: &Attempt) -> TimeAttempt {
        TimeAttempt {
            id: a.id,
            user_id: a.user_id,
            username: s.users.get(&a.user_id).map(|u| u.username.clone()).unwrap_or_default(),
            day: a.day,
            started_ms: a.started_ms,
            finished_ms: a.finished_ms,
            ms: a.result.map(|r| r.0),
            shots: a.result.map(|r| r.1),
            fouls: a.result.map(|r| r.2),
            week: a.counted.map(|c| c.0),
            at: a.counted.map(|c| c.1),
            removed_at: a.removed.as_ref().map(|r| r.0),
            removed_reason: a.removed.as_ref().map(|r| r.1.clone()),
        }
    }

    pub fn time_attack_user_attempts(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<TimeAttempt>> {
        let s = self.lock();
        Ok(s.modes
            .attempts
            .iter()
            .rev()
            .filter(|a| a.user_id == user_id)
            .take(limit as usize)
            .map(|a| Self::attempt_of(&s, a))
            .collect())
    }

    pub fn time_attack_remove(
        &self,
        id: i64,
        reason: &str,
        at: i64,
    ) -> StoreResult<Option<TimeAttempt>> {
        let mut s = self.lock();
        let m = &mut s.modes;
        let Some(a) = m.attempts.iter_mut().find(|a| a.id == id) else { return Ok(None) };
        let (Some((week, _)), None) = (a.counted, &a.removed) else { return Ok(None) };
        a.removed = Some((at, reason.to_string()));
        let user = a.user_id;
        // The best of the week from the attempts left (the earlier on a tie).
        let best = m
            .attempts
            .iter()
            .filter(|a| a.user_id == user && a.removed.is_none())
            .filter_map(|a| match (a.counted, a.result) {
                (Some((w, at)), Some((ms, ..))) if w == week => Some((ms, at)),
                _ => None,
            })
            .min();
        match best {
            Some(b) => m.bests.insert((week, user), b),
            None => m.bests.remove(&(week, user)),
        };
        let a = s.modes.attempts.iter().find(|a| a.id == id).expect("there");
        Ok(Some(Self::attempt_of(&s, a)))
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
