//! In-memory storage (tests and LAN games): nothing survives a restart.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

use snookered_proto::{LeaderboardEntry, Role};

use snookered_proto::CareerNumbers;

mod correspondence;
mod msstore;

use super::{
    AimRow, AuditRow, BoardNote, BuyOutcome, FriendOutcome, FriendState, GiftRow, HistoryRow,
    LedgerRow, LiveMatch, MatchRecord, Metrics, NewGift, NewNote, NewReport, NewUser, Overview,
    Payment, RECENT_SCAN, RecentRow, Rename, ReportRow, SessionKind, StoreResult, StoredReplay,
    TournamentRecord, User, Wallet, day_of, hour_of,
};
use snookered_proto::analysis::ShotAnalysis;

#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    users: BTreeMap<i64, User>,
    next_user: i64,
    /// token hash -> (user, kind, created, expires)
    sessions: HashMap<String, (i64, SessionKind, i64, i64)>,
    matches: Vec<MatchRecord>,
    achievements: HashMap<i64, Vec<(String, i64)>>,
    /// (from, to) -> accepted
    friendships: HashMap<(i64, i64), bool>,
    blocks: HashSet<(i64, i64)>,
    reports: Vec<ReportRow>,
    report_ids: Vec<(i64, i64)>,
    audit: Vec<AuditRow>,
    tournaments: Vec<TournamentRecord>,
    wallets: HashMap<i64, Wallet>,
    /// (user, delta, reason, reference, at)
    ledger: Vec<(i64, i64, String, Option<String>, i64)>,
    payments: Vec<Payment>,
    live: BTreeMap<i64, LiveMatch>,
    /// (user, band) -> (attempts, made)
    shot_stats: HashMap<(i64, usize), (u32, u32)>,
    /// (day, user)
    activity: HashSet<(i64, i64)>,
    /// user -> (daily bonus day, earned day, earned that day)
    earnings: HashMap<i64, (i64, i64, i64)>,
    /// (user, period, code) -> (progress, period ends)
    missions: HashMap<(i64, String, String), (u32, i64)>,
    board: Vec<BoardNote>,
    next_note: i64,
    /// Correspondence games and challenges (see `correspondence.rs`).
    async_games: BTreeMap<i64, super::AsyncGame>,
    next_async_game: i64,
    async_challenges: BTreeMap<i64, super::AsyncChallenge>,
    next_async_challenge: i64,
    /// Ids of the matches that were correspondence games.
    correspondence_matches: HashSet<i64>,
    /// (user, kind, key) -> (attempts, made)
    analysis: HashMap<(i64, i16, i16), (u32, u32)>,
    /// hour -> (peak players, peak games, samples, players summed)
    online_hours: BTreeMap<i64, (i64, i64, i64, i64)>,
    /// (hour, user)
    active_hours: HashSet<(i64, i64)>,
    /// user -> (rack seed, strokes, claimed at)
    ladder_claims: HashMap<i64, (u64, u32, i64)>,
    /// Presents (names and countries filled in when read).
    gifts: Vec<GiftRow>,
    /// Microsoft Store purchases being fulfilled (see `msstore.rs`).
    ms_consumes: Vec<msstore::MsRow>,
    /// day -> the daily challenge's seed
    daily_seeds: HashMap<i64, u64>,
    /// (day, user) -> their day at the challenge
    daily: BTreeMap<(i64, i64), super::DailyRow>,
    /// (user, drill) -> (best, when)
    drills: BTreeMap<(i64, String), (u32, i64)>,
    /// user -> (challenge rating, stars, when)
    challenge: HashMap<i64, (u32, u32, i64)>,
}

impl Inner {
    /// A day's scores, best first (earlier first on a tie).
    fn daily_sorted(&self, day: i64) -> Vec<(i64, u32, i64)> {
        let mut rows: Vec<(i64, u32, i64)> = self
            .daily
            .range((day, i64::MIN)..=(day, i64::MAX))
            .filter_map(|(&(_, u), r)| r.best.map(|b| (u, b, r.best_at)))
            .collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)).then(a.0.cmp(&b.0)));
        rows
    }
}

impl Inner {
    fn by_name(&self, name: &str) -> Option<&User> {
        self.users.values().find(|u| u.username.eq_ignore_ascii_case(name))
    }

    fn name(&self, id: i64) -> String {
        self.users.get(&id).map(|u| u.username.clone()).unwrap_or_default()
    }

    fn record_match(&mut self, m: &MatchRecord) -> ([Option<User>; 2], i64) {
        for seat in 0..2 {
            if let Some(u) = self.users.get_mut(&m.players[seat]) {
                super::apply_match_numbers(u, m, seat);
            }
        }
        self.matches.push(m.clone());
        let id = self.matches.len() as i64;
        ([self.users.get(&m.players[0]).cloned(), self.users.get(&m.players[1]).cloned()], id)
    }

    /// A stored present with the players' names and countries as they are now.
    fn gift_row(&self, g: &GiftRow) -> GiftRow {
        let who = |id: Option<i64>| {
            id.and_then(|id| self.users.get(&id))
                .map(|u| (u.username.clone(), u.country.clone()))
                .unwrap_or_default()
        };
        let (from, from_country) = who(g.from_id);
        let (to, to_country) = who(Some(g.to_id));
        GiftRow { from, from_country, to, to_country, ..g.clone() }
    }

    fn add_gift(&mut self, gift: &NewGift<'_>) -> i64 {
        let id = self.gifts.len() as i64 + 1;
        self.gifts.push(GiftRow {
            id,
            from_id: Some(gift.from),
            from: String::new(),
            from_country: String::new(),
            to_id: gift.to,
            to: String::new(),
            to_country: String::new(),
            code: gift.code.to_string(),
            message: gift.message.to_string(),
            at: gift.at,
            seen: false,
            thanked: false,
        });
        id
    }
}

impl MemoryStore {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn create_user(&self, new: &NewUser<'_>) -> StoreResult<Option<User>> {
        let mut s = self.lock();
        if s.by_name(new.username).is_some() {
            return Ok(None);
        }
        s.next_user += 1;
        let now = crate::now();
        let user = User {
            id: s.next_user,
            username: new.username.to_string(),
            password_hash: new.password_hash.to_string(),
            country: new.country.to_string(),
            role: new.role,
            rating: new.rating,
            peak_rating: new.rating,
            games: 0,
            wins: 0,
            eight_wins: 0,
            casual_games: 0,
            streak: 0,
            best_streak: 0,
            runouts: 0,
            shutouts: 0,
            tournaments_played: 0,
            tournaments_won: 0,
            created_at: now,
            last_seen: now,
            banned_until: None,
            ban_reason: None,
            muted_until: None,
            recovery_email: None,
            deleted_at: None,
            abandons: 0,
            status_message: String::new(),
            xp: 0,
            name_changes: 0,
        };
        s.users.insert(user.id, user.clone());
        Ok(Some(user))
    }

    pub fn user_by_name(&self, name: &str) -> StoreResult<Option<User>> {
        Ok(self.lock().by_name(name).cloned())
    }

    pub fn user_by_id(&self, id: i64) -> StoreResult<Option<User>> {
        Ok(self.lock().users.get(&id).cloned())
    }

    pub fn create_session(
        &self,
        hash: &str,
        user_id: i64,
        kind: SessionKind,
        created: i64,
        expires: i64,
    ) -> StoreResult<()> {
        self.lock().sessions.insert(hash.to_string(), (user_id, kind, created, expires));
        Ok(())
    }

    pub fn session_user(
        &self,
        hash: &str,
        kind: SessionKind,
        now: i64,
    ) -> StoreResult<Option<(User, i64)>> {
        let s = self.lock();
        Ok(s.sessions
            .get(hash)
            .filter(|(_, k, _, exp)| *k == kind && *exp > now)
            .and_then(|(id, _, created, _)| s.users.get(id).map(|u| (u.clone(), *created))))
    }

    pub fn delete_session(&self, hash: &str) -> StoreResult<()> {
        self.lock().sessions.remove(hash);
        Ok(())
    }

    pub fn delete_user_sessions(&self, user_id: i64) -> StoreResult<()> {
        self.lock().sessions.retain(|_, (id, ..)| *id != user_id);
        Ok(())
    }

    pub fn mission_progress(
        &self,
        user_id: i64,
        periods: &[String],
    ) -> StoreResult<Vec<(String, String, u32)>> {
        let s = self.lock();
        Ok(s.missions
            .iter()
            .filter(|((u, p, _), _)| *u == user_id && periods.contains(p))
            .map(|((_, p, c), (n, _))| (p.clone(), c.clone(), *n))
            .collect())
    }

    pub fn advance_mission(
        &self,
        user_id: i64,
        period: &str,
        code: &str,
        amount: u32,
        target: u32,
        ends: i64,
    ) -> StoreResult<u32> {
        let mut s = self.lock();
        let e =
            s.missions.entry((user_id, period.to_string(), code.to_string())).or_insert((0, ends));
        e.0 = (e.0 + amount).min(target);
        Ok(e.0)
    }

    pub fn purge_missions(&self, now: i64) -> StoreResult<u64> {
        let mut s = self.lock();
        let before = s.missions.len();
        s.missions.retain(|_, (_, ends)| *ends > now);
        Ok((before - s.missions.len()) as u64)
    }

    pub fn board_notes(&self, now: i64) -> StoreResult<Vec<BoardNote>> {
        let s = self.lock();
        let mut notes: Vec<BoardNote> =
            s.board.iter().filter(|n| n.expires_at.is_none_or(|e| e > now)).cloned().collect();
        notes.sort_by_key(|n| std::cmp::Reverse((n.pinned, n.created_at, n.id)));
        Ok(notes)
    }

    pub fn add_board_note(&self, n: &NewNote<'_>) -> StoreResult<i64> {
        let mut s = self.lock();
        s.next_note += 1;
        let note = BoardNote {
            id: s.next_note,
            text: n.text.to_string(),
            author: s.name(n.author_id),
            created_at: n.at,
            expires_at: n.expires_at,
            kind: n.kind.to_string(),
            pinned: n.pinned,
            action: n.action.map(str::to_string),
        };
        s.board.push(note);
        Ok(s.next_note)
    }

    pub fn remove_board_note(&self, id: i64) -> StoreResult<bool> {
        let mut s = self.lock();
        let before = s.board.len();
        s.board.retain(|n| n.id != id);
        Ok(s.board.len() < before)
    }

    pub fn purge_sessions(&self, now: i64) -> StoreResult<u64> {
        let mut s = self.lock();
        let before = s.sessions.len();
        s.sessions.retain(|_, (.., exp)| *exp > now);
        Ok((before - s.sessions.len()) as u64)
    }

    pub fn set_country(&self, user_id: i64, country: &str) -> StoreResult<()> {
        if let Some(u) = self.lock().users.get_mut(&user_id) {
            u.country = country.to_string();
        }
        Ok(())
    }

    pub fn touch(&self, user_id: i64, now: i64) -> StoreResult<()> {
        if let Some(u) = self.lock().users.get_mut(&user_id) {
            u.last_seen = now;
        }
        Ok(())
    }

    pub fn set_status_message(&self, user_id: i64, text: &str) -> StoreResult<()> {
        if let Some(u) = self.lock().users.get_mut(&user_id) {
            u.status_message = text.to_string();
        }
        Ok(())
    }

    pub fn recent_opponents(&self, user_id: i64, limit: u32) -> StoreResult<Vec<RecentRow>> {
        let s = self.lock();
        // The player's latest games, as the database orders them.
        let mut games: Vec<(usize, &MatchRecord)> =
            s.matches.iter().enumerate().filter(|(_, m)| m.players.contains(&user_id)).collect();
        games.sort_by_key(|(i, m)| std::cmp::Reverse((m.ended_at, *i)));
        games.truncate(RECENT_SCAN as usize);
        let mut out: Vec<RecentRow> = Vec::new();
        for (_, m) in games {
            let seat = if m.players[0] == user_id { 0 } else { 1 };
            let other = m.players[1 - seat];
            if let Some(row) = out.iter_mut().find(|r| r.user.id == other) {
                row.games += 1;
                continue;
            }
            let Some(u) = s.users.get(&other).filter(|u| u.deleted_at.is_none()) else {
                continue;
            };
            out.push(RecentRow {
                user: u.clone(),
                at: m.ended_at,
                won: m.winner == seat,
                rated: m.rated,
                games: 1,
            });
        }
        out.truncate(limit as usize);
        Ok(out)
    }

    pub fn record_match(&self, m: &MatchRecord) -> StoreResult<([Option<User>; 2], i64)> {
        Ok(self.lock().record_match(m))
    }

    pub fn career(&self, user_id: i64) -> StoreResult<CareerNumbers> {
        let s = self.lock();
        let mut c = CareerNumbers::default();
        for m in &s.matches {
            let Some(seat) = m.players.iter().position(|p| *p == user_id) else { continue };
            let st = &m.stats[seat];
            if m.replay.is_none() && st.shots == 0 {
                continue;
            }
            c.games += 1;
            c.shots += st.shots;
            c.potting_shots += st.potting_shots;
            c.balls += st.balls;
            c.fouls += st.fouls;
            c.best_run = c.best_run.max(st.best_run);
        }
        Ok(c)
    }

    pub fn rating_history(&self, user_id: i64, limit: u32) -> StoreResult<Vec<(i64, i32)>> {
        let s = self.lock();
        let mut list: Vec<(i64, i32)> = s
            .matches
            .iter()
            .rev()
            .filter(|m| m.rated)
            .filter_map(|m| {
                let seat = m.players.iter().position(|p| *p == user_id)?;
                Some((m.ended_at, m.ratings_after[seat]))
            })
            .take(limit as usize)
            .collect();
        list.reverse();
        Ok(list)
    }

    pub fn replay(&self, match_id: i64) -> StoreResult<Option<StoredReplay>> {
        let s = self.lock();
        let Some(m) = usize::try_from(match_id - 1).ok().and_then(|i| s.matches.get(i)) else {
            return Ok(None);
        };
        let Some(replay) = m.replay.clone() else { return Ok(None) };
        let user = |seat: usize| s.users.get(&m.players[seat]);
        Ok(Some(StoredReplay {
            id: match_id,
            players: [0, 1].map(|k| user(k).map(|u| u.username.clone()).unwrap_or_default()),
            countries: [0, 1].map(|k| user(k).map(|u| u.country.clone()).unwrap_or_default()),
            ratings: [0, 1].map(|k| m.ratings_after[k] - m.delta[k]),
            winner: m.winner,
            rated: m.rated,
            at: m.ended_at,
            replay,
        }))
    }

    pub fn recent_replays(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<(i64, usize, snookered_proto::replay::Replay)>> {
        let s = self.lock();
        Ok(s.matches
            .iter()
            .enumerate()
            .rev()
            .filter_map(|(index, m)| {
                let seat = m.players.iter().position(|p| *p == user_id)?;
                Some((index as i64 + 1, seat, m.replay.clone()?))
            })
            .take(limit as usize)
            .collect())
    }

    pub fn record_ladder_claim(
        &self,
        user_id: i64,
        rack_seed: u64,
        strokes: u32,
        at: i64,
    ) -> StoreResult<bool> {
        let mut s = self.lock();
        if s.ladder_claims.contains_key(&user_id) {
            return Ok(false);
        }
        s.ladder_claims.insert(user_id, (rack_seed, strokes, at));
        Ok(true)
    }

    pub fn ladder_claim(&self, user_id: i64) -> StoreResult<Option<i64>> {
        Ok(self.lock().ladder_claims.get(&user_id).map(|c| c.2))
    }

    pub fn save_live(&self, m: &LiveMatch) -> StoreResult<()> {
        self.lock().live.insert(m.id, m.clone());
        Ok(())
    }

    pub fn delete_live(&self, id: i64) -> StoreResult<()> {
        self.lock().live.remove(&id);
        Ok(())
    }

    pub fn live_match_of(&self, user_id: i64) -> StoreResult<Option<LiveMatch>> {
        Ok(self.lock().live.values().rev().find(|m| m.players.contains(&user_id)).cloned())
    }

    pub fn purge_live(&self, before: i64) -> StoreResult<u64> {
        let mut s = self.lock();
        let n = s.live.len();
        s.live.retain(|_, m| m.updated_at >= before);
        Ok((n - s.live.len()) as u64)
    }

    pub fn spend(
        &self,
        user_id: i64,
        amount: i64,
        reason: &str,
        reference: Option<&str>,
        at: i64,
    ) -> StoreResult<bool> {
        let mut s = self.lock();
        let w = s.wallets.entry(user_id).or_default();
        if w.coins < amount {
            return Ok(false);
        }
        w.coins -= amount;
        s.ledger.push((user_id, -amount, reason.to_string(), reference.map(str::to_string), at));
        Ok(true)
    }

    pub fn add_shot_stats(&self, user_id: i64, bands: &[(usize, u32, u32)]) -> StoreResult<()> {
        let mut s = self.lock();
        for &(band, attempts, made) in bands {
            let e = s.shot_stats.entry((user_id, band)).or_default();
            e.0 += attempts;
            e.1 += made;
        }
        Ok(())
    }

    pub fn aim_rows(&self, bands: &[usize], min: u32) -> StoreResult<Vec<AimRow>> {
        let s = self.lock();
        let mut rows: BTreeMap<i64, (u32, u32)> = BTreeMap::new();
        for (&(user, band), &(a, m)) in &s.shot_stats {
            if bands.contains(&band) {
                let e = rows.entry(user).or_default();
                e.0 += a;
                e.1 += m;
            }
        }
        Ok(rows
            .into_iter()
            .filter(|(_, (a, _))| *a >= min)
            .filter_map(|(id, (attempts, made))| {
                let u = s.users.get(&id).filter(|u| u.deleted_at.is_none())?;
                Some(AimRow {
                    user_id: id,
                    username: u.username.clone(),
                    rating: u.rating,
                    attempts,
                    made,
                })
            })
            .collect())
    }

    pub fn mark_active(&self, user_id: i64, day: i64) -> StoreResult<()> {
        self.lock().activity.insert((day, user_id));
        Ok(())
    }

    pub fn metrics(&self, now: i64) -> StoreResult<Metrics> {
        let s = self.lock();
        let today = day_of(now);
        let days = (today - 29..=today)
            .map(|d| {
                let active = s.activity.iter().filter(|(day, _)| *day == d).count() as i64;
                let matches = s.matches.iter().filter(|m| day_of(m.ended_at) == d).count() as i64;
                let new = s.users.values().filter(|u| day_of(u.created_at) == d).count() as i64;
                (d, active, matches, new)
            })
            .collect();
        let recent: Vec<&User> =
            s.users.values().filter(|u| day_of(u.created_at) >= today - 30).collect();
        // Accounts created from `from` to `to` (days) that came back
        // `after` days later, among those for which that day has come.
        let back_between = |from: i64, to: i64, after: i64| {
            let eligible: Vec<&User> = s
                .users
                .values()
                .filter(|u| (from..=to).contains(&day_of(u.created_at)))
                .filter(|u| day_of(u.created_at) + after <= today)
                .collect();
            let came = eligible
                .iter()
                .filter(|u| s.activity.contains(&(day_of(u.created_at) + after, u.id)))
                .count();
            super::percent(came as i64, eligible.len() as i64)
        };
        let back =
            |after: i64| back_between(today - after - 29, today - after, after).unwrap_or(0.0);
        let cohorts = (0..super::COHORT_WEEKS)
            .map(|w| {
                let first_day = today - 6 - 7 * w;
                let last = first_day + 6;
                super::Cohort {
                    first_day,
                    accounts: s
                        .users
                        .values()
                        .filter(|u| (first_day..=last).contains(&day_of(u.created_at)))
                        .count() as i64,
                    d1: back_between(first_day, last, 1),
                    d7: back_between(first_day, last, 7),
                    d30: back_between(first_day, last, 30),
                }
            })
            .collect();
        let games = |u: &User| u.games + u.casual_games;
        let mut revenue: BTreeMap<String, (i64, i64)> = BTreeMap::new();
        let mut revenue_days: BTreeMap<(i64, String), (i64, i64)> = BTreeMap::new();
        for p in s.payments.iter().filter(|p| !p.refunded && !p.test && p.at > now - 30 * 86_400) {
            let e = revenue.entry(p.currency.clone()).or_default();
            e.0 += p.amount;
            e.1 += 1;
            let e = revenue_days.entry((day_of(p.at), p.currency.clone())).or_default();
            e.0 += p.amount;
            e.1 += 1;
        }
        let this_hour = hour_of(now);
        let hours = (this_hour - 47..=this_hour)
            .map(|h| {
                let (peak_players, peak_games, samples, sum) =
                    s.online_hours.get(&h).copied().unwrap_or_default();
                super::HourRow {
                    hour: h,
                    active: s.active_hours.iter().filter(|(x, _)| *x == h).count() as i64,
                    peak_players,
                    peak_games,
                    avg_players: if samples > 0 { sum as f64 / samples as f64 } else { 0.0 },
                    games: s.matches.iter().filter(|m| hour_of(m.ended_at) == h).count() as i64,
                }
            })
            .collect();
        Ok(Metrics {
            days,
            retention_d1: back(1),
            retention_d7: back(7),
            retention_d30: back(30),
            cohorts,
            funnel: [
                recent.len() as i64,
                recent.iter().filter(|u| games(u) >= 1).count() as i64,
                recent.iter().filter(|u| games(u) >= 5).count() as i64,
            ],
            revenue: revenue.into_iter().map(|(c, (a, n))| (c, a, n)).collect(),
            hours,
            revenue_days: revenue_days.into_iter().map(|((d, c), (a, n))| (d, c, a, n)).collect(),
        })
    }

    pub fn set_recovery_email(&self, user_id: i64, email: Option<&str>) -> StoreResult<()> {
        if let Some(u) = self.lock().users.get_mut(&user_id) {
            u.recovery_email = email.map(str::to_string);
        }
        Ok(())
    }

    pub fn change_username(&self, user_id: i64, name: &str, limit: u32) -> StoreResult<Rename> {
        let mut s = self.lock();
        if s.by_name(name).is_some_and(|u| u.id != user_id) {
            return Ok(Rename::Taken);
        }
        let Some(u) = s.users.get_mut(&user_id).filter(|u| u.deleted_at.is_none()) else {
            return Ok(Rename::NoChangesLeft);
        };
        if u.name_changes >= limit {
            return Ok(Rename::NoChangesLeft);
        }
        let old = std::mem::replace(&mut u.username, name.to_string());
        u.name_changes += 1;
        // The board keeps its authors by name here (the database looks them up).
        for n in s.board.iter_mut().filter(|n| n.author == old) {
            n.author = name.to_string();
        }
        Ok(Rename::Done)
    }

    pub fn payment_email(&self, user_id: i64) -> StoreResult<Option<String>> {
        let s = self.lock();
        Ok(s.payments.iter().rev().filter(|p| p.user_id == user_id).find_map(|p| p.email.clone()))
    }

    pub fn delete_account(&self, user_id: i64, now: i64) -> StoreResult<()> {
        let mut s = self.lock();
        if let Some(u) = s.users.get_mut(&user_id) {
            u.username = super::deleted_name(user_id);
            u.password_hash = String::new();
            u.country = String::new();
            u.recovery_email = None;
            u.status_message = String::new();
            u.deleted_at = Some(now);
        }
        s.sessions.retain(|_, v| v.0 != user_id);
        s.friendships.retain(|(a, b), _| *a != user_id && *b != user_id);
        s.blocks.retain(|(a, b)| *a != user_id && *b != user_id);
        s.achievements.remove(&user_id);
        s.ladder_claims.remove(&user_id);
        // What they wrote with their presents goes with them.
        for g in s.gifts.iter_mut().filter(|g| g.from_id == Some(user_id)) {
            g.message.clear();
        }
        s.shot_stats.retain(|(u, _), _| *u != user_id);
        s.live.retain(|_, m| !m.players.contains(&user_id));
        s.async_challenges.retain(|_, c| c.from != user_id && c.to != user_id);
        if let Some(w) = s.wallets.get_mut(&user_id) {
            w.stripe_customer = None;
            w.equipped.clear();
        }
        for p in s.payments.iter_mut().filter(|p| p.user_id == user_id) {
            p.email = None;
        }
        Ok(())
    }

    pub fn add_achievements(
        &self,
        user_id: i64,
        codes: &[&str],
        at: i64,
    ) -> StoreResult<Vec<String>> {
        let mut s = self.lock();
        let list = s.achievements.entry(user_id).or_default();
        let mut new = Vec::new();
        for code in codes {
            if !list.iter().any(|(c, _)| c == code) {
                list.push((code.to_string(), at));
                new.push(code.to_string());
            }
        }
        Ok(new)
    }

    pub fn achievements(&self, user_id: i64) -> StoreResult<Vec<(String, i64)>> {
        Ok(self.lock().achievements.get(&user_id).cloned().unwrap_or_default())
    }

    pub fn history(&self, user_id: i64, limit: u32) -> StoreResult<Vec<HistoryRow>> {
        let s = self.lock();
        Ok(s.matches
            .iter()
            .enumerate()
            .rev()
            .filter_map(|(index, m)| {
                let seat = m.players.iter().position(|p| *p == user_id)?;
                let other = s.users.get(&m.players[1 - seat]);
                Some(HistoryRow {
                    opponent: other.map(|u| u.username.clone()).unwrap_or_default(),
                    opponent_country: other.map(|u| u.country.clone()).unwrap_or_default(),
                    opponent_admin: other.is_some_and(|u| u.role == Role::Admin),
                    opponent_rating: m.ratings_after[1 - seat],
                    won: m.winner == seat,
                    reason: crate::reason_code(m.reason).to_string(),
                    rated: m.rated,
                    delta: m.delta[seat],
                    rating_after: m.ratings_after[seat],
                    tournament: m.tournament.is_some(),
                    at: m.ended_at,
                    match_id: index as i64 + 1,
                    replay: m.replay.is_some(),
                    correspondence: s.correspondence_matches.contains(&(index as i64 + 1)),
                })
            })
            .take(limit as usize)
            .collect())
    }

    pub fn leaderboard(&self, limit: u32, now: i64) -> StoreResult<Vec<LeaderboardEntry>> {
        let s = self.lock();
        // Banned players are left out while the ban lasts.
        let mut users: Vec<&User> = s
            .users
            .values()
            .filter(|u| u.games > 0 && !u.is_banned(now) && u.deleted_at.is_none())
            .collect();
        users.sort_by(|a, b| {
            b.rating.cmp(&a.rating).then(b.wins.cmp(&a.wins)).then(a.username.cmp(&b.username))
        });
        Ok(users
            .into_iter()
            .take(limit as usize)
            .enumerate()
            .map(|(i, u)| LeaderboardEntry {
                rank: i as u32 + 1,
                username: u.username.clone(),
                rating: u.rating,
                games: u.games,
                wins: u.wins,
                country: u.country.clone(),
                vip: s.wallets.get(&u.id).is_some_and(|w| w.vip_until > now),
                admin: u.role == Role::Admin,
            })
            .collect())
    }

    pub fn rank_of(&self, user_id: i64) -> StoreResult<Option<u32>> {
        let s = self.lock();
        let Some(me) = s.users.get(&user_id).filter(|u| u.games > 0) else { return Ok(None) };
        let now = crate::now();
        let above = s
            .users
            .values()
            .filter(|u| {
                u.games > 0 && !u.is_banned(now) && u.deleted_at.is_none() && u.rating > me.rating
            })
            .count();
        Ok(Some(above as u32 + 1))
    }

    pub fn friends(&self, user_id: i64) -> StoreResult<Vec<(User, FriendState)>> {
        let s = self.lock();
        let mut out = Vec::new();
        for ((a, b), accepted) in &s.friendships {
            let (other, state) = if *a == user_id {
                (*b, if *accepted { FriendState::Friends } else { FriendState::Outgoing })
            } else if *b == user_id && !*accepted {
                (*a, FriendState::Incoming)
            } else {
                continue;
            };
            if let Some(u) = s.users.get(&other) {
                out.push((u.clone(), state));
            }
        }
        out.sort_by_key(|x| x.0.username.to_lowercase());
        Ok(out)
    }

    pub fn friend_request(&self, from: i64, to: i64, _at: i64) -> StoreResult<FriendOutcome> {
        let mut s = self.lock();
        if s.friendships.get(&(from, to)) == Some(&true) {
            return Ok(FriendOutcome::AlreadyFriends);
        }
        if s.friendships.get(&(to, from)) == Some(&false) {
            s.friendships.insert((to, from), true);
            s.friendships.insert((from, to), true);
            return Ok(FriendOutcome::Accepted);
        }
        if s.friendships.contains_key(&(from, to)) {
            return Ok(FriendOutcome::AlreadySent);
        }
        s.friendships.insert((from, to), false);
        Ok(FriendOutcome::Sent)
    }

    pub fn friend_accept(&self, me: i64, from: i64) -> StoreResult<bool> {
        let mut s = self.lock();
        if s.friendships.get(&(from, me)) != Some(&false) {
            return Ok(false);
        }
        s.friendships.insert((from, me), true);
        s.friendships.insert((me, from), true);
        Ok(true)
    }

    pub fn friend_remove(&self, a: i64, b: i64) -> StoreResult<()> {
        let mut s = self.lock();
        s.friendships.remove(&(a, b));
        s.friendships.remove(&(b, a));
        Ok(())
    }

    pub fn friend_count(&self, user_id: i64) -> StoreResult<u32> {
        let s = self.lock();
        Ok(s.friendships.iter().filter(|((a, _), ok)| *a == user_id && **ok).count() as u32)
    }

    pub fn block(&self, user_id: i64, blocked: i64) -> StoreResult<()> {
        self.lock().blocks.insert((user_id, blocked));
        Ok(())
    }

    pub fn unblock(&self, user_id: i64, blocked: i64) -> StoreResult<()> {
        self.lock().blocks.remove(&(user_id, blocked));
        Ok(())
    }

    pub fn blocked(&self, user_id: i64) -> StoreResult<Vec<User>> {
        let s = self.lock();
        Ok(s.blocks
            .iter()
            .filter(|(a, _)| *a == user_id)
            .filter_map(|(_, b)| s.users.get(b).cloned())
            .collect())
    }

    pub fn create_report_with_game(
        &self,
        r: &NewReport,
        match_id: Option<i64>,
        live_id: Option<i64>,
    ) -> StoreResult<i64> {
        let mut s = self.lock();
        let id = s.reports.len() as i64 + 1;
        let row = ReportRow {
            id,
            reporter: s.name(r.reporter),
            target: s.name(r.target),
            reason: r.reason.clone(),
            details: r.details.clone(),
            context: r.context.clone(),
            status: 0,
            created_at: r.at,
            resolved_by: None,
            resolution: None,
            reporter_id: r.reporter,
            target_id: r.target,
            match_id: match_id.filter(|m| *m >= 1 && *m as usize <= s.matches.len()),
            live_id,
        };
        s.reports.push(row);
        s.report_ids.push((r.reporter, r.target));
        Ok(id)
    }

    pub fn report(&self, id: i64) -> StoreResult<Option<ReportRow>> {
        Ok(self.lock().reports.iter().find(|r| r.id == id).cloned())
    }

    pub fn set_report_match(&self, id: i64, match_id: i64) -> StoreResult<()> {
        let mut s = self.lock();
        let known = match_id >= 1 && match_id as usize <= s.matches.len();
        if let Some(r) = s.reports.iter_mut().find(|r| r.id == id).filter(|_| known) {
            r.match_id = Some(match_id);
        }
        Ok(())
    }

    pub fn matches_between(
        &self,
        a: i64,
        b: i64,
        from: i64,
        to: i64,
        limit: u32,
    ) -> StoreResult<Vec<(i64, i64)>> {
        let s = self.lock();
        let mut list: Vec<(i64, i64)> = s
            .matches
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                m.replay.is_some()
                    && (m.players == [a, b] || m.players == [b, a])
                    && (from..=to).contains(&m.ended_at)
            })
            .map(|(k, m)| (k as i64 + 1, m.ended_at))
            .collect();
        list.sort_by(|x, y| y.1.cmp(&x.1).then(y.0.cmp(&x.0)));
        list.truncate(limit as usize);
        Ok(list)
    }

    pub fn add_shot_analysis(&self, user_id: i64, analysis: &ShotAnalysis) -> StoreResult<()> {
        let mut s = self.lock();
        for (kind, key, attempts, made) in analysis.rows() {
            let e = s.analysis.entry((user_id, kind, key)).or_default();
            e.0 = e.0.saturating_add(attempts);
            e.1 = e.1.saturating_add(made);
        }
        Ok(())
    }

    pub fn shot_analysis(&self, user_id: i64) -> StoreResult<ShotAnalysis> {
        let s = self.lock();
        Ok(ShotAnalysis::from_rows(
            s.analysis
                .iter()
                .filter(|((u, _, _), _)| *u == user_id)
                .map(|(&(_, kind, key), &(a, m))| (kind, key, a, m)),
        ))
    }

    pub fn sample_online(
        &self,
        hour: i64,
        users: &[i64],
        players: u32,
        games: u32,
    ) -> StoreResult<()> {
        let mut s = self.lock();
        let e = s.online_hours.entry(hour).or_default();
        e.0 = e.0.max(players as i64);
        e.1 = e.1.max(games as i64);
        e.2 += 1;
        e.3 += players as i64;
        for u in users {
            if s.users.contains_key(u) {
                s.active_hours.insert((hour, *u));
            }
        }
        Ok(())
    }

    pub fn purge_active_hours(&self, hour: i64) -> StoreResult<u64> {
        let mut s = self.lock();
        let before = s.active_hours.len();
        s.active_hours.retain(|(h, _)| *h >= hour);
        Ok((before - s.active_hours.len()) as u64)
    }

    pub fn record_tournament(&self, t: &TournamentRecord) -> StoreResult<()> {
        let mut s = self.lock();
        for id in &t.players {
            if let Some(u) = s.users.get_mut(id) {
                u.tournaments_played += 1;
                if t.winner == Some(*id) {
                    u.tournaments_won += 1;
                }
            }
        }
        s.tournaments.push(t.clone());
        Ok(())
    }

    pub fn drill_bests(&self, user_id: i64) -> StoreResult<Vec<(String, u32)>> {
        let s = self.lock();
        Ok(s.drills
            .iter()
            .filter(|((u, _), _)| *u == user_id)
            .map(|((_, c), (b, _))| (c.clone(), *b))
            .collect())
    }

    pub fn drill_record(
        &self,
        user_id: i64,
        code: &str,
        score: u32,
        at: i64,
    ) -> StoreResult<Option<u32>> {
        let mut s = self.lock();
        let key = (user_id, code.to_string());
        let before = s.drills.get(&key).map(|(b, _)| *b);
        if before.is_none_or(|b| score > b) {
            s.drills.insert(key, (score, at));
        }
        Ok(before)
    }

    pub fn drill_board(&self, code: &str, limit: u32) -> StoreResult<Vec<super::BoardRow>> {
        let s = self.lock();
        let mut rows: Vec<(u32, i64, i64)> = s
            .drills
            .iter()
            .filter(|((_, c), _)| c == code)
            .map(|((u, _), (b, at))| (*b, *at, *u))
            .collect();
        rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        Ok(rows
            .into_iter()
            .take(limit as usize)
            .filter_map(|(score, _, u)| {
                let user = s.users.get(&u)?;
                Some(super::BoardRow {
                    username: user.username.clone(),
                    country: user.country.clone(),
                    score,
                    stars: 0,
                })
            })
            .collect())
    }

    pub fn set_challenge_rating(
        &self,
        user_id: i64,
        rating: u32,
        stars: u32,
        at: i64,
    ) -> StoreResult<()> {
        self.lock().challenge.insert(user_id, (rating, stars, at));
        Ok(())
    }

    pub fn challenge_board(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<(Vec<super::BoardRow>, Option<u32>)> {
        let s = self.lock();
        let mut rows: Vec<(u32, i64, i64, u32)> =
            s.challenge.iter().map(|(u, (r, st, at))| (*r, *at, *u, *st)).collect();
        rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        let rank = rows.iter().position(|r| r.2 == user_id).map(|k| k as u32 + 1);
        let board = rows
            .into_iter()
            .take(limit as usize)
            .filter_map(|(rating, _, u, stars)| {
                let user = s.users.get(&u)?;
                Some(super::BoardRow {
                    username: user.username.clone(),
                    country: user.country.clone(),
                    score: rating,
                    stars,
                })
            })
            .collect();
        Ok((board, rank))
    }

    pub fn add_xp(&self, user_id: i64, amount: u32) -> StoreResult<i64> {
        let mut s = self.lock();
        Ok(s.users.get_mut(&user_id).map_or(0, |u| {
            u.xp = u.xp.saturating_add(amount as i64);
            u.xp
        }))
    }

    pub fn daily_seed(&self, day: i64, fresh: u64) -> StoreResult<u64> {
        Ok(*self.lock().daily_seeds.entry(day).or_insert(fresh))
    }

    pub fn daily_start(&self, user_id: i64, day: i64, now: i64, max: u32) -> StoreResult<bool> {
        let mut s = self.lock();
        let row = s.daily.entry((day, user_id)).or_default();
        if row.attempts >= max {
            return Ok(false);
        }
        row.attempts += 1;
        row.started_at = now;
        Ok(true)
    }

    pub fn daily_finish(
        &self,
        user_id: i64,
        day: i64,
        now: i64,
        balls: u32,
        cleared: bool,
        limit: i64,
    ) -> StoreResult<super::DailyFinish> {
        let mut s = self.lock();
        let Some(row) = s.daily.get_mut(&(day, user_id)) else {
            return Ok(super::DailyFinish::NotStarted);
        };
        if row.started_at == 0 || now - row.started_at > limit {
            return Ok(super::DailyFinish::NotStarted);
        }
        row.started_at = 0;
        let improved = row.best.is_none_or(|b| balls > b);
        if improved {
            row.best = Some(balls);
            row.best_at = now;
        }
        let first_clear = cleared && !row.cleared;
        row.cleared |= cleared;
        Ok(super::DailyFinish::Counted { improved, first_clear })
    }

    pub fn daily_row(&self, user_id: i64, day: i64) -> StoreResult<super::DailyRow> {
        Ok(self.lock().daily.get(&(day, user_id)).copied().unwrap_or_default())
    }

    pub fn daily_board(
        &self,
        day: i64,
        limit: u32,
    ) -> StoreResult<(Vec<super::DailyBoardRow>, u32)> {
        let s = self.lock();
        let rows = s.daily_sorted(day);
        let players = rows.len() as u32;
        let board = rows
            .into_iter()
            .take(limit as usize)
            .filter_map(|(u, balls, at)| {
                let user = s.users.get(&u)?;
                Some(super::DailyBoardRow {
                    user_id: u,
                    username: user.username.clone(),
                    country: user.country.clone(),
                    balls,
                    at,
                })
            })
            .collect();
        Ok((board, players))
    }

    pub fn daily_rank(&self, user_id: i64, day: i64) -> StoreResult<Option<u32>> {
        let s = self.lock();
        Ok(s.daily_sorted(day).iter().position(|r| r.0 == user_id).map(|k| k as u32 + 1))
    }

    pub fn daily_wins(&self, user_id: i64, before: i64) -> StoreResult<Vec<(i64, u32)>> {
        let s = self.lock();
        let days: std::collections::BTreeSet<i64> =
            s.daily.keys().map(|&(d, _)| d).filter(|&d| d < before).collect();
        Ok(days
            .into_iter()
            .rev()
            .filter_map(|d| {
                let rows = s.daily_sorted(d);
                (rows.first()?.0 == user_id).then_some((d, rows.len() as u32))
            })
            .collect())
    }

    pub fn tournaments_won(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<(String, u32, i64)>> {
        let s = self.lock();
        let mut won: Vec<_> = s
            .tournaments
            .iter()
            .filter(|t| t.winner == Some(user_id))
            .map(|t| (t.name.clone(), t.players.len() as u32, t.finished_at))
            .collect();
        won.sort_by_key(|t| std::cmp::Reverse(t.2));
        won.truncate(limit as usize);
        Ok(won)
    }

    pub fn search_users(
        &self,
        query: &str,
        offset: u32,
        limit: u32,
    ) -> StoreResult<(Vec<User>, i64)> {
        let s = self.lock();
        let q = query.to_lowercase();
        let all: Vec<User> = s
            .users
            .values()
            .rev()
            .filter(|u| {
                q.is_empty()
                    || u.username.to_lowercase().contains(&q)
                    || u.recovery_email.as_deref().is_some_and(|e| e.to_lowercase() == q)
            })
            .cloned()
            .collect();
        let total = all.len() as i64;
        Ok((all.into_iter().skip(offset as usize).take(limit as usize).collect(), total))
    }

    pub fn overview(&self, now: i64) -> StoreResult<Overview> {
        let s = self.lock();
        let week = now - 7 * 86_400;
        Ok(Overview {
            users: s.users.len() as i64,
            new_users_7d: s.users.values().filter(|u| u.created_at > week).count() as i64,
            active_users_7d: s.users.values().filter(|u| u.last_seen > week).count() as i64,
            matches_total: s.matches.len() as i64,
            matches_24h: s.matches.iter().filter(|m| m.ended_at > now - 86_400).count() as i64,
            open_reports: s.reports.iter().filter(|r| r.status == 0).count() as i64,
            banned: s.users.values().filter(|u| u.is_banned(now)).count() as i64,
        })
    }

    pub fn set_ban(
        &self,
        user_id: i64,
        until: Option<i64>,
        reason: Option<&str>,
    ) -> StoreResult<()> {
        if let Some(u) = self.lock().users.get_mut(&user_id) {
            u.banned_until = until;
            u.ban_reason = reason.map(str::to_string);
        }
        Ok(())
    }

    pub fn set_mute(&self, user_id: i64, until: Option<i64>) -> StoreResult<()> {
        if let Some(u) = self.lock().users.get_mut(&user_id) {
            u.muted_until = until;
        }
        Ok(())
    }

    pub fn set_role(&self, user_id: i64, role: Role) -> StoreResult<()> {
        if let Some(u) = self.lock().users.get_mut(&user_id) {
            u.role = role;
        }
        Ok(())
    }

    pub fn set_password(&self, user_id: i64, hash: &str) -> StoreResult<()> {
        if let Some(u) = self.lock().users.get_mut(&user_id) {
            u.password_hash = hash.to_string();
        }
        Ok(())
    }

    pub fn reports(&self, status: Option<i16>, limit: u32) -> StoreResult<Vec<ReportRow>> {
        let s = self.lock();
        Ok(s.reports
            .iter()
            .rev()
            .filter(|r| status.is_none_or(|st| r.status == st))
            .take(limit as usize)
            .cloned()
            .collect())
    }

    pub fn reports_about(&self, user_id: i64, limit: u32) -> StoreResult<Vec<ReportRow>> {
        let s = self.lock();
        Ok(s.reports
            .iter()
            .zip(&s.report_ids)
            .rev()
            .filter(|(_, (_, t))| *t == user_id)
            .map(|(r, _)| r.clone())
            .take(limit as usize)
            .collect())
    }

    pub fn resolve_report(
        &self,
        id: i64,
        staff: i64,
        status: i16,
        note: &str,
        _at: i64,
    ) -> StoreResult<bool> {
        let mut s = self.lock();
        let name = s.name(staff);
        match s.reports.iter_mut().find(|r| r.id == id) {
            Some(r) => {
                r.status = status;
                r.resolved_by = Some(name);
                r.resolution = Some(note.to_string());
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn audit(
        &self,
        staff: i64,
        action: &str,
        target: &str,
        details: &str,
        at: i64,
    ) -> StoreResult<()> {
        let mut s = self.lock();
        let id = s.audit.len() as i64 + 1;
        let row = AuditRow {
            id,
            staff: s.name(staff),
            action: action.to_string(),
            target: target.to_string(),
            details: details.to_string(),
            at,
        };
        s.audit.push(row);
        Ok(())
    }

    pub fn audit_log(&self, limit: u32) -> StoreResult<Vec<AuditRow>> {
        Ok(self.lock().audit.iter().rev().take(limit as usize).cloned().collect())
    }

    // The shop.

    pub fn wallet(&self, user_id: i64) -> StoreResult<Wallet> {
        Ok(self.lock().wallets.get(&user_id).cloned().unwrap_or_default())
    }

    pub fn buy_item(
        &self,
        user_id: i64,
        code: &str,
        price: i64,
        at: i64,
    ) -> StoreResult<BuyOutcome> {
        let mut s = self.lock();
        let w = s.wallets.entry(user_id).or_default();
        if w.owned.iter().any(|o| o == code) {
            return Ok(BuyOutcome::AlreadyOwned);
        }
        if w.coins < price {
            return Ok(BuyOutcome::NotEnoughCoins);
        }
        w.coins -= price;
        w.owned.push(code.to_string());
        s.ledger.push((user_id, -price, format!("buy {code}"), None, at));
        Ok(BuyOutcome::Bought)
    }

    pub fn grant_item(
        &self,
        user_id: i64,
        code: &str,
        _source: &str,
        _at: i64,
    ) -> StoreResult<bool> {
        let mut s = self.lock();
        let w = s.wallets.entry(user_id).or_default();
        if w.owned.iter().any(|o| o == code) {
            return Ok(false);
        }
        w.owned.push(code.to_string());
        Ok(true)
    }

    pub fn equip(&self, user_id: i64, category: &str, code: &str) -> StoreResult<()> {
        let mut s = self.lock();
        s.wallets
            .entry(user_id)
            .or_default()
            .equipped
            .insert(category.to_string(), code.to_string());
        Ok(())
    }

    pub fn add_cue_game(
        &self,
        user_id: i64,
        code: &str,
        won: bool,
        potted: u32,
    ) -> StoreResult<u32> {
        let mut s = self.lock();
        let w = s.wallets.entry(user_id).or_default();
        let games = w.cue_games.entry(code.to_string()).or_default();
        games.0 = games.0.saturating_add(1);
        games.1 = games.1.saturating_add(potted);
        let wins = w.cue_wins.entry(code.to_string()).or_default();
        if won {
            *wins = wins.saturating_add(1);
        }
        Ok(*wins)
    }

    pub fn credit(
        &self,
        user_id: i64,
        delta: i64,
        reason: &str,
        reference: Option<&str>,
        at: i64,
    ) -> StoreResult<bool> {
        let mut s = self.lock();
        if let Some(r) = reference
            && s.ledger.iter().any(|l| l.3.as_deref() == Some(r))
        {
            return Ok(false);
        }
        s.wallets.entry(user_id).or_default().coins += delta;
        s.ledger.push((user_id, delta, reason.to_string(), reference.map(str::to_string), at));
        Ok(true)
    }

    pub fn set_vip(&self, user_id: i64, until: i64, force: bool) -> StoreResult<()> {
        let mut s = self.lock();
        let w = s.wallets.entry(user_id).or_default();
        w.vip_until = if force { until } else { w.vip_until.max(until) };
        Ok(())
    }

    pub fn set_stripe_customer(&self, user_id: i64, customer: &str) -> StoreResult<()> {
        let mut s = self.lock();
        s.wallets.entry(user_id).or_default().stripe_customer = Some(customer.to_string());
        Ok(())
    }

    pub fn user_by_stripe_customer(&self, customer: &str) -> StoreResult<Option<i64>> {
        let s = self.lock();
        Ok(s.wallets
            .iter()
            .find(|(_, w)| w.stripe_customer.as_deref() == Some(customer))
            .map(|(id, _)| *id))
    }

    pub fn record_payment(&self, p: &Payment) -> StoreResult<bool> {
        let mut s = self.lock();
        if s.payments.iter().any(|x| x.id == p.id) {
            return Ok(false);
        }
        s.payments.push(p.clone());
        Ok(true)
    }

    pub fn payment(&self, id: &str) -> StoreResult<Option<Payment>> {
        Ok(self.lock().payments.iter().find(|p| p.id == id).cloned())
    }

    pub fn set_payment_test(&self, id: &str, test: bool) -> StoreResult<bool> {
        let mut s = self.lock();
        match s.payments.iter_mut().find(|p| p.id == id) {
            Some(p) => {
                p.test = test;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn refund_payment(&self, id: &str) -> StoreResult<bool> {
        let mut s = self.lock();
        match s.payments.iter_mut().find(|p| p.id == id) {
            Some(p) if !p.refunded => {
                p.refunded = true;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn ledger(&self, user_id: i64, limit: u32) -> StoreResult<Vec<LedgerRow>> {
        let s = self.lock();
        Ok(s.ledger
            .iter()
            .rev()
            .filter(|l| l.0 == user_id)
            .take(limit as usize)
            .map(|l| LedgerRow { delta: l.1, reason: l.2.clone(), reference: l.3.clone(), at: l.4 })
            .collect())
    }

    pub fn payments_of(&self, user_id: i64, limit: u32) -> StoreResult<Vec<Payment>> {
        let s = self.lock();
        Ok(s.payments
            .iter()
            .rev()
            .filter(|p| p.user_id == user_id)
            .take(limit as usize)
            .cloned()
            .collect())
    }

    pub fn claim_daily(&self, user_id: i64, day: i64, coins: i64) -> StoreResult<bool> {
        let mut s = self.lock();
        let e = s.earnings.entry(user_id).or_default();
        if e.0 >= day {
            return Ok(false);
        }
        e.0 = day;
        s.wallets.entry(user_id).or_default().coins += coins;
        s.ledger.push((user_id, coins, "daily bonus".into(), None, day * 86_400));
        Ok(true)
    }

    pub fn earn(
        &self,
        user_id: i64,
        day: i64,
        amount: i64,
        cap: i64,
        reason: &str,
        at: i64,
    ) -> StoreResult<i64> {
        let mut s = self.lock();
        let e = s.earnings.entry(user_id).or_default();
        if e.1 != day {
            e.1 = day;
            e.2 = 0;
        }
        let given = amount.min(cap - e.2).max(0);
        if given == 0 {
            return Ok(0);
        }
        e.2 += given;
        s.wallets.entry(user_id).or_default().coins += given;
        s.ledger.push((user_id, given, reason.to_string(), None, at));
        Ok(given)
    }

    pub fn give_item(
        &self,
        gift: &NewGift<'_>,
        price: i64,
    ) -> StoreResult<(BuyOutcome, Option<i64>)> {
        let (from, to, code, at) = (gift.from, gift.to, gift.code, gift.at);
        let mut s = self.lock();
        if s.wallets.get(&to).is_some_and(|w| w.owned.iter().any(|o| o == code)) {
            return Ok((BuyOutcome::AlreadyOwned, None));
        }
        let payer = s.wallets.entry(from).or_default();
        if payer.coins < price {
            return Ok((BuyOutcome::NotEnoughCoins, None));
        }
        payer.coins -= price;
        s.wallets.entry(to).or_default().owned.push(code.to_string());
        s.ledger.push((from, -price, format!("gift {code} to {to}"), None, at));
        let id = s.add_gift(gift);
        Ok((BuyOutcome::Bought, Some(id)))
    }

    pub fn record_gift(&self, gift: &NewGift<'_>) -> StoreResult<i64> {
        Ok(self.lock().add_gift(gift))
    }

    pub fn gift(&self, id: i64) -> StoreResult<Option<GiftRow>> {
        let s = self.lock();
        Ok(s.gifts.iter().find(|g| g.id == id).map(|g| s.gift_row(g)))
    }

    pub fn gifts_of(&self, user_id: i64, limit: u32) -> StoreResult<Vec<GiftRow>> {
        let s = self.lock();
        let mut list: Vec<GiftRow> = s
            .gifts
            .iter()
            .filter(|g| g.to_id == user_id || g.from_id == Some(user_id))
            .map(|g| s.gift_row(g))
            .collect();
        list.sort_by_key(|g| std::cmp::Reverse((g.at, g.id)));
        list.truncate(limit as usize);
        Ok(list)
    }

    pub fn unseen_gifts(&self, user_id: i64) -> StoreResult<Vec<GiftRow>> {
        let s = self.lock();
        let mut list: Vec<GiftRow> = s
            .gifts
            .iter()
            .filter(|g| g.to_id == user_id && !g.seen)
            .map(|g| s.gift_row(g))
            .collect();
        list.sort_by_key(|g| (g.at, g.id));
        Ok(list)
    }

    pub fn mark_gifts_seen(&self, user_id: i64, ids: &[i64]) -> StoreResult<u64> {
        let mut s = self.lock();
        let mut n = 0;
        for g in s.gifts.iter_mut() {
            if g.to_id == user_id && !g.seen && ids.contains(&g.id) {
                g.seen = true;
                n += 1;
            }
        }
        Ok(n)
    }

    pub fn thank_gift(&self, user_id: i64, id: i64) -> StoreResult<Option<GiftRow>> {
        let mut s = self.lock();
        let Some(g) = s.gifts.iter_mut().find(|g| g.id == id && g.to_id == user_id && !g.thanked)
        else {
            return Ok(None);
        };
        g.thanked = true;
        g.seen = true;
        let g = g.clone();
        Ok(Some(s.gift_row(&g)))
    }
}
