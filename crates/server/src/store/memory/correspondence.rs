//! Correspondence games in memory (the same rules as the database).

use snook_legend_proto::replay::Replay;

use super::super::{AsyncChallenge, AsyncGame, MatchRecord, NewAsyncGame, StoreResult, User};
use super::MemoryStore;

impl MemoryStore {
    pub fn create_async_challenge(
        &self,
        from: i64,
        to: i64,
        rated: bool,
        rules: snook_legend_proto::Rules,
        at: i64,
    ) -> StoreResult<Option<i64>> {
        let mut s = self.lock();
        if s.async_challenges.values().any(|c| c.from == from && c.to == to) {
            return Ok(None);
        }
        s.next_async_challenge += 1;
        let id = s.next_async_challenge;
        s.async_challenges
            .insert(id, AsyncChallenge { id, from, to, rated, rules, created_at: at });
        Ok(Some(id))
    }

    pub fn async_challenge(&self, id: i64) -> StoreResult<Option<AsyncChallenge>> {
        Ok(self.lock().async_challenges.get(&id).cloned())
    }

    pub fn async_challenges_of(&self, user_id: i64) -> StoreResult<Vec<AsyncChallenge>> {
        let s = self.lock();
        let mut list: Vec<AsyncChallenge> = s
            .async_challenges
            .values()
            .filter(|c| c.from == user_id || c.to == user_id)
            .cloned()
            .collect();
        list.sort_by_key(|c| (c.created_at, c.id));
        Ok(list)
    }

    pub fn delete_async_challenge(&self, id: i64) -> StoreResult<bool> {
        Ok(self.lock().async_challenges.remove(&id).is_some())
    }

    pub fn accept_async_challenge(
        &self,
        id: i64,
        new: &NewAsyncGame,
    ) -> StoreResult<Option<AsyncGame>> {
        let mut s = self.lock();
        let Some(c) = s.async_challenges.remove(&id) else { return Ok(None) };
        s.next_async_game += 1;
        let game = AsyncGame {
            id: s.next_async_game,
            players: [c.from, c.to],
            rated: c.rated,
            replay: Replay::new(new.seed, new.breaker % 2, c.rules),
            turn: new.breaker % 2,
            deadline: new.deadline,
            seen: [0, 0],
            groups: [None, None],
            left: [7, 7],
            started_at: new.at,
            updated_at: new.at,
            finished_at: None,
            winner: None,
            reason: None,
            timeout: false,
            delta: [0, 0],
            ratings: [0, 0],
            match_id: None,
        };
        s.async_games.insert(game.id, game.clone());
        Ok(Some(game))
    }

    pub fn purge_async_challenges(&self, before: i64) -> StoreResult<Vec<AsyncChallenge>> {
        let mut s = self.lock();
        let old: Vec<AsyncChallenge> =
            s.async_challenges.values().filter(|c| c.created_at < before).cloned().collect();
        for c in &old {
            s.async_challenges.remove(&c.id);
        }
        Ok(old)
    }

    pub fn async_game(&self, id: i64) -> StoreResult<Option<AsyncGame>> {
        Ok(self.lock().async_games.get(&id).cloned())
    }

    pub fn async_games_of(&self, user_id: i64, finished: u32) -> StoreResult<Vec<AsyncGame>> {
        let s = self.lock();
        let mine = || s.async_games.values().filter(|g| g.players.contains(&user_id));
        let mut active: Vec<AsyncGame> = mine().filter(|g| g.is_active()).cloned().collect();
        active.sort_by_key(|g| g.id);
        let mut done: Vec<AsyncGame> = mine().filter(|g| !g.is_active()).cloned().collect();
        done.sort_by_key(|g| std::cmp::Reverse((g.finished_at, g.id)));
        done.truncate(finished as usize);
        active.extend(done);
        Ok(active)
    }

    pub fn update_async_game(&self, game: &AsyncGame, expected: usize) -> StoreResult<bool> {
        let mut s = self.lock();
        let Some(g) = s.async_games.get_mut(&game.id) else { return Ok(false) };
        if !g.is_active() || g.replay.strokes.len() != expected {
            return Ok(false);
        }
        g.replay.strokes = game.replay.strokes.clone();
        g.turn = game.turn;
        g.deadline = game.deadline;
        g.seen = [0, 1].map(|k| g.seen[k].max(game.seen[k]));
        g.groups = game.groups;
        g.left = game.left;
        g.updated_at = game.updated_at;
        Ok(true)
    }

    pub fn set_async_seen(&self, id: i64, seat: usize, seen: u32) -> StoreResult<()> {
        if let Some(g) = self.lock().async_games.get_mut(&id) {
            let slot = &mut g.seen[seat % 2];
            *slot = (*slot).max(seen);
        }
        Ok(())
    }

    pub fn finish_async_game(
        &self,
        game: &AsyncGame,
        expected: usize,
        record: &MatchRecord,
    ) -> StoreResult<Option<([Option<User>; 2], i64)>> {
        let mut s = self.lock();
        match s.async_games.get(&game.id) {
            Some(g) if g.is_active() && g.replay.strokes.len() == expected => {}
            _ => return Ok(None),
        }
        let (users, match_id) = s.record_match(record);
        s.correspondence_matches.insert(match_id);
        let g = s.async_games.get_mut(&game.id).expect("checked");
        g.replay.strokes = game.replay.strokes.clone();
        g.turn = game.turn;
        g.seen = [0, 1].map(|k| g.seen[k].max(game.seen[k]));
        g.groups = game.groups;
        g.left = game.left;
        g.updated_at = record.ended_at;
        g.finished_at = Some(record.ended_at);
        g.winner = Some(record.winner);
        g.reason = Some(crate::reason_code(record.reason).to_string());
        g.timeout = game.timeout;
        g.delta = record.delta;
        g.ratings = [0, 1].map(|k| {
            users[k]
                .as_ref()
                .map(|u| u.records.get(record.rules).rating)
                .unwrap_or(record.ratings_after[k])
        });
        g.match_id = Some(match_id);
        Ok(Some((users, match_id)))
    }

    pub fn expired_async_games(&self, now: i64, limit: u32) -> StoreResult<Vec<AsyncGame>> {
        let s = self.lock();
        let mut list: Vec<AsyncGame> =
            s.async_games.values().filter(|g| g.is_active() && g.deadline < now).cloned().collect();
        list.sort_by_key(|g| (g.deadline, g.id));
        list.truncate(limit as usize);
        Ok(list)
    }
}
