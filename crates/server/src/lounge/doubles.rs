//! Doubles in the lounge: pairs of partners, their "Play now" and their
//! tables, and the four players of a doubles game (see
//! `snook_legend_proto::doubles` and `snook_legend_core::doubles`).
//!
//! A doubles game is a [`Room`] like any other: its game is played between
//! seat 0 and seat 1, each seat a team, `keys[team]` the team's first
//! player and [`Doubles`] the rest: the second players, who of each team
//! holds the cue, and each player's connection. The room's own
//! `disconnected[team]` is set only once both players of a team are away:
//! that is what the reconnection grace and the abandonment count, so a
//! team loses by leaving only when both of its players are gone for the
//! whole grace (or let the shot clock run out time after time). A player
//! whose partner is away plays the team's visits alone meanwhile.
//!
//! Doubles games are saved after every stroke and handed to the next
//! server like singles ones (`live_matches.partners` has the teams' second
//! players): the next server rebuilds the table with all four away, and
//! each is back at it as they sign in again.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

use rand::Rng;
use snook_legend_core::doubles::{designated, shooter, team_rating, team_result};
use snook_legend_core::rating::{EloConfig, Rating};
use snook_legend_core::{GameOverReason, PerRules, Rules};
use snook_legend_proto::doubles::{self, Lineup, MIN_PROTOCOL, Pair, Search};
use snook_legend_proto::{ChatLine, MatchResult, ServerMsg};

use super::{
    ABANDON_RATING, Effect, Game, Lounge, Offer, RECONNECT_CLOCK, Room, cooldown_message, key,
    queue_range,
};
use crate::now;
use crate::store::{DOUBLES_START, DoublesRecord, Record, User};

/// What the inviter hears when the other can't play doubles (an older game).
const OLD_GAME: &str = "That player's game is too old for doubles.";
const NO_PARTNER: &str = "Invite a friend to be your partner first.";

/// The four players of a doubles game beyond the room's own two (the
/// teams' first players, in `keys`).
#[derive(Debug, Clone)]
pub(super) struct Doubles {
    /// Each team's second player (lower-case names) and their account.
    pub partners: [String; 2],
    pub partner_users: [User; 2],
    /// Since when each player (`[team][player]`) has been away (their
    /// connection dropped).
    pub away: [[Option<Instant>; 2]; 2],
    /// Each player is still at the table (a game over: until they leave).
    pub present: [[bool; 2]; 2],
    /// For a player whose connection dropped: the table's chat count then.
    pub chat_missed: [[Option<u64>; 2]; 2],
    /// Each player's doubles record under the game's rules when it began.
    pub records: [[Record; 2]; 2],
    /// Who asked for another game (all four must).
    pub rematch: [[bool; 2]; 2],
    /// The rating changes of a rated game once over.
    pub deltas: Option<[[i32; 2]; 2]>,
}

/// A pair looking for another pair.
#[derive(Debug, Clone)]
struct Searching {
    keys: [String; 2],
    rated: bool,
    rules: Rules,
    since: Instant,
    last_status: Instant,
}

/// A doubles table waiting for a pair.
#[derive(Debug, Clone)]
struct PairTable {
    id: u64,
    keys: [String; 2],
    rated: bool,
    rules: Rules,
}

/// Pairs, their searches and tables.
#[derive(Default)]
pub(super) struct Lobby {
    /// Player -> partner (both ways).
    partners: HashMap<String, String>,
    /// (from, to): invitations to be partners.
    invites: HashSet<(String, String)>,
    queue: Vec<Searching>,
    tables: BTreeMap<u64, PairTable>,
    /// Each player's doubles records (from the store when they sign in;
    /// kept up to date by the results here).
    records: HashMap<String, PerRules<Record>>,
}

impl Room {
    /// The copy of this game saved for a restart (all four players of a
    /// doubles game).
    pub(super) fn live(&self) -> crate::store::LiveMatch {
        crate::store::LiveMatch {
            id: self.id as i64,
            players: [self.users[0].id, self.users[1].id],
            partners: self.doubles.as_ref().map(|d| d.partner_users.each_ref().map(|u| u.id)),
            rated: self.rated,
            replay: self.replay.clone(),
            updated_at: now(),
        }
    }

    /// Whether `k` plays in this room (either player of either team).
    pub(super) fn has(&self, k: &str) -> bool {
        self.keys.iter().any(|p| p == k)
            || self.doubles.as_ref().is_some_and(|d| d.partners.iter().any(|p| p == k))
    }

    /// Everyone who plays in this room.
    pub(super) fn all_keys(&self) -> Vec<String> {
        let mut out = self.keys.to_vec();
        if let Some(d) = &self.doubles {
            out.extend(d.partners.iter().cloned());
        }
        out
    }

    /// `k`'s team and their place in it (0: the first player).
    pub(super) fn place_of(&self, k: &str) -> (usize, usize) {
        match &self.doubles {
            Some(d) if d.partners[0] == k => (0, 1),
            Some(d) if d.partners[1] == k => (1, 1),
            _ => (self.seat_of(k), 0),
        }
    }

    /// The player `place` of `team`.
    pub(super) fn key_at(&self, team: usize, place: usize) -> &str {
        match (&self.doubles, place) {
            (Some(d), 1) => &d.partners[team % 2],
            _ => &self.keys[team % 2],
        }
    }

    /// Who of `team` takes its current (or next) visit: 0 or 1.
    pub(super) fn shooter_slot(&self, team: usize) -> usize {
        let Some(d) = &self.doubles else { return 0 };
        let present = d.away[team % 2].map(|a| a.is_none());
        let visit = self.turns[team % 2];
        shooter(visit, present).unwrap_or(designated(visit))
    }

    /// Whether `k` holds the cue now (the game's current team, and their
    /// turn within it).
    pub(super) fn holds_cue(&self, k: &str) -> bool {
        let team = self.game.current_player();
        self.key_at(team, self.shooter_slot(team)) == k
    }

    /// Each player still at the table, with whether they are (a game over:
    /// until they leave it).
    pub(super) fn players_present(&self) -> Vec<(String, bool)> {
        match &self.doubles {
            Some(d) => (0..4)
                .map(|n| (self.key_at(n / 2, n % 2).to_string(), d.present[n / 2][n % 2]))
                .collect(),
            None => (0..2).map(|s| (self.keys[s].clone(), self.present[s])).collect(),
        }
    }
}

impl Lounge {
    /// The protocol of `k`'s game is known (see [`Lounge::set_protocol`]):
    /// a game that knows doubles is sent the doubles tables.
    pub(super) fn doubles_hello(&self, k: &str) {
        if self.can_double(k) {
            self.send(k, ServerMsg::DoublesTables { tables: self.doubles_tables() });
        }
    }

    /// `name`'s doubles records, as the store has them.
    pub fn set_doubles_records(&mut self, name: &str, records: PerRules<Record>) {
        self.doubles.records.insert(key(name), records);
    }

    /// `k`'s doubles record under `rules`.
    fn doubles_record(&self, k: &str, rules: Rules) -> Record {
        self.doubles.records.get(k).map(|r| *r.get(rules)).unwrap_or(Record::new(DOUBLES_START))
    }

    fn can_double(&self, k: &str) -> bool {
        self.sessions.get(k).is_some_and(|s| s.protocol >= MIN_PROTOCOL)
    }

    /// The pair of `k` (them first), if they have a partner.
    fn pair_of(&self, k: &str) -> Option<[String; 2]> {
        self.doubles.partners.get(k).map(|p| [k.to_string(), p.clone()])
    }

    /// The average doubles rating of a pair under `rules`.
    fn pair_rating(&self, pair: &[String; 2], rules: Rules) -> i32 {
        let r = |k: &str| {
            let rec = self.doubles_record(k, rules);
            Rating { rating: rec.rating, games: rec.games }
        };
        team_rating(r(&pair[0]), r(&pair[1])).rating
    }

    pub(super) fn doubles_tables(&self) -> Vec<doubles::Table> {
        self.doubles
            .tables
            .values()
            .filter_map(|t| {
                Some(doubles::Table {
                    id: t.id,
                    team: [self.info(&t.keys[0])?, self.info(&t.keys[1])?],
                    rated: t.rated,
                    rules: t.rules,
                    rating: self.pair_rating(&t.keys, t.rules),
                })
            })
            .collect()
    }

    fn broadcast_doubles_tables(&self) {
        let msg = ServerMsg::DoublesTables { tables: self.doubles_tables() };
        for k in self.sessions.keys().filter(|k| self.can_double(k)) {
            self.send(k, msg.clone());
        }
    }

    /// Tells both players of `k`'s pair how it stands (or `k` alone that
    /// they have none).
    fn send_pair(&self, k: &str) {
        let Some(pair) = self.pair_of(k) else {
            return self.send(k, ServerMsg::DoublesPair { pair: None });
        };
        let search = self.doubles.queue.iter().find(|q| q.keys.contains(&pair[0]));
        let table = self.doubles.tables.values().find(|t| t.keys.contains(&pair[0]));
        for (me, partner) in [(&pair[0], &pair[1]), (&pair[1], &pair[0])] {
            let Some(info) = self.info(partner) else { continue };
            let pair = Pair {
                partner: info,
                searching: search.map(|q| Search {
                    rated: q.rated,
                    rules: q.rules,
                    waited_secs: q.since.elapsed().as_secs() as u32,
                }),
                table: table.map(|t| t.id),
            };
            self.send(me, ServerMsg::DoublesPair { pair: Some(pair) });
        }
    }

    /// Doubles messages from a player (true if it was one).
    pub(super) fn handle_doubles(&mut self, k: &str, msg: &snook_legend_proto::ClientMsg) -> bool {
        use snook_legend_proto::ClientMsg as C;
        match msg {
            C::DoublesInvite { to } => self.doubles_invite(k, &key(to)),
            C::DoublesAnswer { from, accept } => self.doubles_answer(k, &key(from), *accept),
            C::DoublesLeave => self.leave_pair(k, true),
            C::DoublesQueue { rated, rules } => {
                self.doubles_queue(k, Offer { rated: *rated, rules: *rules })
            }
            C::DoublesQueueLeave => {
                self.drop_search(k);
                self.send_pair(k);
            }
            C::DoublesOpenTable { rated, rules } => {
                self.open_doubles_table(k, Offer { rated: *rated, rules: *rules })
            }
            C::DoublesJoinTable { id } => self.join_doubles_table(k, *id),
            C::DoublesCloseTable => {
                if self.drop_doubles_table(k) {
                    self.send_pair(k);
                }
            }
            _ => return false,
        }
        true
    }

    /// Whether `k` may start a doubles game now (else they are told why).
    fn free_for_doubles(&self, k: &str, who: &str) -> bool {
        if self.refuse_if_draining(who) {
            return false;
        }
        if let Some(left) = self.abandon_cooldown(k) {
            self.error(who, cooldown_message(left));
            return false;
        }
        if !self.available(k) {
            self.error(who, "Finish your game first.");
            return false;
        }
        if self.tournament_busy(k) {
            self.error(who, "Players in a tournament can't start other games.");
            return false;
        }
        true
    }

    fn doubles_invite(&mut self, from: &str, to: &str) {
        let fail = |l: &Self, reason: &str| {
            l.send(from, ServerMsg::InviteFailed { reason: reason.into() })
        };
        if from == to {
            return fail(self, "You can't invite yourself.");
        }
        if !self.sessions.contains_key(to) {
            return fail(self, "That player is not online.");
        }
        if !self.can_double(to) {
            return fail(self, OLD_GAME);
        }
        // Friends (anyone on a local network, where there are none).
        let friends = self.sessions.get(from).is_some_and(|s| s.friends.contains(to));
        if !self.settings.lan && !friends {
            return fail(self, "You can only ask your friends to be your partner.");
        }
        if self.blocks(to, from) {
            return fail(self, "That player is not accepting invitations.");
        }
        if self.doubles.partners.contains_key(to) {
            return fail(self, "That player already has a partner.");
        }
        if self.doubles.partners.contains_key(from) {
            return fail(self, "Leave your pair first.");
        }
        if self.doubles.invites.contains(&(from.to_string(), to.to_string())) {
            return;
        }
        if !self.sessions.get_mut(from).is_some_and(|s| s.invite_bucket.take()) {
            return fail(self, "You're sending invitations too quickly.");
        }
        self.doubles.invites.insert((from.to_string(), to.to_string()));
        self.send(to, ServerMsg::DoublesInvited { from: self.display(from) });
        self.send(from, ServerMsg::DoublesInviteSent { to: self.display(to) });
    }

    fn doubles_answer(&mut self, me: &str, from: &str, accept: bool) {
        if !self.doubles.invites.remove(&(from.to_string(), me.to_string())) {
            return self.send(
                me,
                ServerMsg::InviteFailed { reason: "That invitation is no longer valid.".into() },
            );
        }
        if !accept {
            return self.send(from, ServerMsg::InviteDeclined { by: self.display(me) });
        }
        let taken = |l: &Self, k: &str| l.doubles.partners.contains_key(k);
        if !self.sessions.contains_key(from) || taken(self, from) || taken(self, me) {
            return self.send(
                me,
                ServerMsg::InviteFailed { reason: "That player is no longer available.".into() },
            );
        }
        // Other invitations of either to be partners are over.
        let gone: Vec<(String, String)> = self
            .doubles
            .invites
            .iter()
            .filter(|(a, b)| [a, b].iter().any(|x| *x == me || *x == from))
            .cloned()
            .collect();
        for (a, b) in gone {
            self.doubles.invites.remove(&(a.clone(), b.clone()));
            self.send(&a, ServerMsg::InviteDeclined { by: self.display(&b) });
        }
        self.doubles.partners.insert(me.to_string(), from.to_string());
        self.doubles.partners.insert(from.to_string(), me.to_string());
        self.send_pair(me);
    }

    /// `k` leaves their pair (or went offline): the search or table goes
    /// with it, and the partner is told. `asked`: they asked (else they
    /// left the lounge).
    pub(super) fn leave_pair(&mut self, k: &str, asked: bool) {
        // Invitations to be partners go too.
        let gone: Vec<(String, String)> =
            self.doubles.invites.iter().filter(|(a, b)| a == k || b == k).cloned().collect();
        for (a, b) in gone {
            self.doubles.invites.remove(&(a.clone(), b.clone()));
            if a == k {
                self.send(&b, ServerMsg::InviteCancelled { from: self.display(&a) });
            } else {
                self.send(&a, ServerMsg::InviteDeclined { by: self.display(&b) });
            }
        }
        self.drop_search(k);
        self.drop_doubles_table(k);
        let Some(partner) = self.doubles.partners.remove(k) else {
            if asked {
                self.send(k, ServerMsg::DoublesPair { pair: None });
            }
            return;
        };
        self.doubles.partners.remove(&partner);
        self.send(&partner, ServerMsg::DoublesPair { pair: None });
        self.send(
            &partner,
            ServerMsg::Notice { text: "Your doubles partner left the pair.".into() },
        );
        if asked {
            self.send(k, ServerMsg::DoublesPair { pair: None });
        }
    }

    /// The server is going away: no pair looks for a game nor waits at a
    /// table any more (the pairs stay; each is told).
    pub(super) fn doubles_drain(&mut self) {
        let waiting: Vec<String> = self
            .doubles
            .queue
            .iter()
            .map(|q| q.keys[0].clone())
            .chain(self.doubles.tables.values().map(|t| t.keys[0].clone()))
            .collect();
        self.doubles.queue.clear();
        self.doubles.tables.clear();
        self.broadcast_doubles_tables();
        for k in waiting {
            self.send_pair(&k);
        }
    }

    /// A player left the lounge: their pair, and what it had going, end.
    pub(super) fn doubles_left(&mut self, k: &str) {
        self.leave_pair(k, false);
    }

    fn drop_search(&mut self, k: &str) -> bool {
        let before = self.doubles.queue.len();
        self.doubles.queue.retain(|q| !q.keys.iter().any(|p| p == k));
        before != self.doubles.queue.len()
    }

    fn drop_doubles_table(&mut self, k: &str) -> bool {
        let ids: Vec<u64> = self
            .doubles
            .tables
            .values()
            .filter(|t| t.keys.iter().any(|p| p == k))
            .map(|t| t.id)
            .collect();
        for id in &ids {
            self.doubles.tables.remove(id);
        }
        if !ids.is_empty() {
            self.broadcast_doubles_tables();
        }
        !ids.is_empty()
    }

    /// The pair of `k` if both may play a doubles game now (each told why
    /// not, otherwise).
    fn ready_pair(&self, k: &str) -> Option<[String; 2]> {
        let Some(pair) = self.pair_of(k) else {
            self.error(k, NO_PARTNER);
            return None;
        };
        if !self.can_double(&pair[1]) {
            self.error(k, OLD_GAME);
            return None;
        }
        pair.iter().all(|p| self.free_for_doubles(p, k)).then_some(pair)
    }

    fn doubles_queue(&mut self, k: &str, offer: Offer) {
        let Some(pair) = self.ready_pair(k) else { return };
        let guests = pair.iter().any(|p| self.is_guest(p));
        let rated = offer.rated && !guests && !self.settings.lan;
        for p in &pair {
            self.queue_leave(p, false);
            self.close_table_of(p);
        }
        self.drop_search(k);
        self.drop_doubles_table(k);
        let now = Instant::now();
        self.doubles.queue.push(Searching {
            keys: pair,
            rated,
            rules: offer.rules,
            since: now,
            last_status: now,
        });
        self.send_pair(k);
        self.match_doubles();
    }

    fn open_doubles_table(&mut self, k: &str, offer: Offer) {
        let Some(pair) = self.ready_pair(k) else { return };
        let guests = pair.iter().any(|p| self.is_guest(p));
        let rated = offer.rated && !guests && !self.settings.lan;
        self.drop_search(k);
        self.drop_doubles_table(k);
        for p in &pair {
            self.queue_leave(p, false);
            self.close_table_of(p);
        }
        let id = self.new_id();
        self.doubles.tables.insert(id, PairTable { id, keys: pair, rated, rules: offer.rules });
        self.broadcast_doubles_tables();
        self.send_pair(k);
    }

    fn join_doubles_table(&mut self, k: &str, id: u64) {
        let Some(t) = self.doubles.tables.get(&id).cloned() else {
            return self.error(k, "That table is no longer open.");
        };
        if t.keys.iter().any(|p| p == k) {
            return;
        }
        let Some(pair) = self.ready_pair(k) else { return };
        if t.keys.iter().any(|a| pair.iter().any(|b| self.blocks(a, b) || self.blocks(b, a))) {
            return self.error(k, "You can't join this table.");
        }
        if !t.keys.iter().all(|p| self.sessions.get(p).is_some_and(|s| s.room.is_none())) {
            self.doubles.tables.remove(&id);
            self.broadcast_doubles_tables();
            return self.error(k, "That table is no longer open.");
        }
        let guests = pair.iter().chain(&t.keys).any(|p| self.is_guest(p));
        let offer = Offer { rated: t.rated && !guests, rules: t.rules };
        self.start_doubles([t.keys.clone(), pair], offer);
    }

    /// Pairs the pairs that can be paired: the closest averages first,
    /// within what both have waited for (as the singles queue).
    pub(super) fn match_doubles(&mut self) {
        let now = Instant::now();
        // Pairs that can no longer play (broken up, in a game) stop.
        let stale: Vec<String> = self
            .doubles
            .queue
            .iter()
            .filter(|q| {
                q.keys.iter().any(|p| {
                    self.sessions.get(p).is_none_or(|s| s.room.is_some())
                        || !self.doubles.partners.contains_key(p)
                })
            })
            .map(|q| q.keys[0].clone())
            .collect();
        for k in stale {
            self.drop_search(&k);
            self.send_pair(&k);
        }
        let stale: Vec<String> = self
            .doubles
            .tables
            .values()
            .filter(|t| {
                t.keys.iter().any(|p| self.sessions.get(p).is_none_or(|s| s.room.is_some()))
            })
            .map(|t| t.keys[0].clone())
            .collect();
        for k in stale {
            self.drop_doubles_table(&k);
            self.send_pair(&k);
        }
        loop {
            let mut best: Option<(usize, usize, u32)> = None;
            let q = &self.doubles.queue;
            for i in 0..q.len() {
                for j in (i + 1)..q.len() {
                    let (a, b) = (&q[i], &q[j]);
                    if (a.rated, a.rules) != (b.rated, b.rules) {
                        continue;
                    }
                    let blocked = a
                        .keys
                        .iter()
                        .any(|x| b.keys.iter().any(|y| self.blocks(x, y) || self.blocks(y, x)));
                    if blocked {
                        continue;
                    }
                    let waited = |s: &Searching| now.duration_since(s.since).as_secs_f32();
                    let gap = (self.pair_rating(&a.keys, a.rules)
                        - self.pair_rating(&b.keys, b.rules))
                    .unsigned_abs();
                    let allowed = queue_range(waited(a)).min(queue_range(waited(b)));
                    if gap <= allowed && best.is_none_or(|(_, _, g)| gap < g) {
                        best = Some((i, j, gap));
                    }
                }
            }
            let Some((i, j, _)) = best else { break };
            let b = self.doubles.queue.remove(j);
            let a = self.doubles.queue.remove(i);
            let offer = Offer { rated: a.rated, rules: a.rules };
            self.start_doubles([a.keys, b.keys], offer);
        }
        // The pairs still waiting hear how long (every few seconds).
        let due: Vec<String> = self
            .doubles
            .queue
            .iter_mut()
            .filter(|q| now.duration_since(q.last_status) >= Duration::from_secs(3))
            .map(|q| {
                q.last_status = now;
                q.keys[0].clone()
            })
            .collect();
        for k in due {
            self.send_pair(&k);
        }
    }

    /// Starts a doubles game between two pairs (each `[first, second]`).
    fn start_doubles(&mut self, mut teams: [[String; 2]; 2], offer: Offer) {
        let all: Vec<String> = teams.iter().flatten().cloned().collect();
        if !all.iter().all(|k| self.sessions.contains_key(k)) {
            return;
        }
        if self.draining.is_some() {
            for k in &all {
                self.error(k, "The server is restarting: new games start in a moment.");
            }
            return;
        }
        if rand::thread_rng().gen_bool(0.5) {
            teams.swap(0, 1);
        }
        for k in &all {
            self.drop_search(k);
            self.drop_doubles_table(k);
            self.drop_invites_of(k);
            self.close_table_of(k);
            self.queue_leave(k, false);
            self.leave_finished(k);
        }
        let user = |l: &Self, k: &str| l.sessions[k].user.clone();
        let users = [user(self, &teams[0][0]), user(self, &teams[1][0])];
        let partner_users = [user(self, &teams[0][1]), user(self, &teams[1][1])];
        let guests = [0, 1].map(|t| teams[t].iter().any(|k| self.is_guest(k)));
        let rules = offer.rules;
        let rated = offer.rated && !guests[0] && !guests[1] && !self.settings.lan;
        let records = teams.clone().map(|t| t.map(|k| self.doubles_record(&k, rules)));
        let d = Doubles {
            partners: [teams[0][1].clone(), teams[1][1].clone()],
            partner_users,
            away: [[None; 2]; 2],
            present: [[true; 2]; 2],
            chat_missed: [[None; 2]; 2],
            records,
            rematch: [[false; 2]; 2],
            deltas: None,
        };
        let keys = [teams[0][0].clone(), teams[1][0].clone()];
        let host = if self.pair_rating(&teams[0], rules) >= self.pair_rating(&teams[1], rules) {
            &keys[0]
        } else {
            &keys[1]
        };
        let look = self.table_look_of(host);
        let id = self.new_id();
        let game = Game { seed: rand::thread_rng().r#gen(), breaker: 0, rules };
        self.create_room(
            id,
            keys,
            users,
            guests,
            rated,
            game,
            None,
            look,
            false,
            Some(Box::new(d)),
        );
        tracing::info!(room = id, rated, "doubles started: {all:?}");
    }

    /// What a rated doubles game puts at stake for each team (the first
    /// player's points: partners of a like record move alike).
    pub(super) fn doubles_stakes(d: &Doubles) -> [[i32; 2]; 2] {
        let config = EloConfig::default();
        let r = |rec: Record| Rating { rating: rec.rating, games: rec.games };
        let teams = d.records.map(|t| t.map(r));
        [0, 1].map(|team| {
            let win = team_result(&config, teams[team], teams[1 - team])[0][0];
            let loss = team_result(&config, teams[1 - team], teams[team])[1][0];
            [win, loss]
        })
    }

    /// The line-up of doubles game `id` as it stands.
    pub(super) fn lineup(&self, id: u64) -> Option<Lineup> {
        let room = self.rooms.get(&id)?;
        let d = room.doubles.as_ref()?;
        let rules = room.game.rules();
        let player = |team: usize, place: usize| {
            let k = room.key_at(team, place);
            let user = if place == 0 { &room.users[team] } else { &d.partner_users[team] };
            let at = self.sessions.get(k).is_some_and(|s| s.room == Some(id));
            doubles::Player {
                username: self
                    .sessions
                    .get(k)
                    .map_or(user.username.clone(), |s| s.user.username.clone()),
                country: user.country.clone(),
                avatar: user.avatar.clone(),
                rating: self.doubles_record(k, rules).rating,
                connected: at && d.away[team][place].is_none() && d.present[team][place],
                admin: user.role == snook_legend_proto::Role::Admin,
            }
        };
        Some(Lineup {
            match_id: id,
            teams: [[player(0, 0), player(0, 1)], [player(1, 0), player(1, 1)]],
            shooter: [room.shooter_slot(0), room.shooter_slot(1)],
            deltas: d.deltas,
        })
    }

    /// Tells everyone at doubles game `id` (players and spectators) its
    /// line-up (nothing for a singles game).
    pub(super) fn doubles_changed(&self, id: u64) {
        if let Some(lineup) = self.lineup(id) {
            self.send_room(id, &ServerMsg::DoublesLineup { lineup });
        }
    }

    /// The line-up for one player (before their table), if a doubles game.
    pub(super) fn send_lineup(&self, k: &str, id: u64) {
        if let Some(lineup) = self.lineup(id) {
            self.send(k, ServerMsg::DoublesLineup { lineup });
        }
    }

    /// `k`'s connection to doubles game `id` dropped (the game goes on).
    pub(super) fn doubles_dropped(&mut self, k: &str, id: u64) {
        let grace = self.settings.reconnect_grace.as_secs() as u32;
        let Some(room) = self.rooms.get_mut(&id) else { return };
        let (team, place) = room.place_of(k);
        let was_shooter = !room.finished && room.busy.is_none() && room.holds_cue(k);
        let total = room.chat_total;
        let Some(d) = room.doubles.as_mut() else { return };
        if room.finished {
            d.present[team][place] = false;
            room.present[team] = d.present[team].iter().any(|p| *p);
        } else {
            d.away[team][place] = Some(Instant::now());
            d.chat_missed[team][place] = Some(total);
            // Both players of the team gone: the team is away (the grace
            // runs, and the other team is told).
            if d.away[team].iter().all(Option::is_some) {
                room.disconnected[team] = Some(Instant::now());
                let others = [room.key_at(1 - team, 0), room.key_at(1 - team, 1)].map(String::from);
                let msg = ServerMsg::OpponentConnection {
                    connected: false,
                    grace_secs: grace,
                    clock_left: None,
                };
                for o in others {
                    self.send(&o, msg.clone());
                }
            } else if was_shooter {
                // The partner takes the visit, with a fair moment to shoot.
                let least = Instant::now() + RECONNECT_CLOCK.min(self.settings.shot_clock);
                room.deadline = room.deadline.max(least);
            }
        }
        self.doubles_changed(id);
        self.close_room_if_empty(id);
    }

    /// `k` is back at doubles game `id` (signed in again).
    pub(super) fn doubles_back(&mut self, k: &str, id: u64) {
        let clock = self.settings.shot_clock.as_secs() as u32;
        let Some(room) = self.rooms.get_mut(&id) else { return };
        let (team, place) = room.place_of(k);
        let Some(d) = room.doubles.as_mut() else { return };
        let was_away = d.away[team][place].take().is_some();
        let missed = d.chat_missed[team][place].take();
        let team_back = room.disconnected[team].take().is_some() && !room.finished;
        // Rebuilt after a restart: once both teams are back, the clock
        // starts now.
        if room.grace_until.is_some() && room.disconnected.iter().all(Option::is_none) {
            room.grace_until = None;
            room.deadline = Instant::now() + self.settings.shot_clock;
            room.clock = self.settings.shot_clock;
            self.featured_dirty = true;
        }
        let clock_left = room.deadline.saturating_duration_since(Instant::now()).as_secs_f32();
        let lines: Vec<ChatLine> = match missed {
            Some(mark) => {
                let n = room.chat_total.saturating_sub(mark).min(room.chat.len() as u64) as usize;
                room.chat.iter().skip(room.chat.len() - n).cloned().collect()
            }
            None => {
                let since = now() - super::RECENT_CHAT_SECS;
                room.chat.iter().filter(|l| l.at >= since).cloned().collect()
            }
        };
        let msg = room.started_msg(team, &self.sessions, clock, false);
        let result = room.result.clone();
        let others: Vec<String> = room.all_keys().into_iter().filter(|p| p != k).collect();
        let other_team = [room.key_at(1 - team, 0), room.key_at(1 - team, 1)].map(String::from);
        let ids: HashMap<String, i64> = {
            let d = room.doubles.as_ref().expect("doubles");
            (0..4)
                .map(|n| {
                    let user =
                        if n % 2 == 0 { &room.users[n / 2] } else { &d.partner_users[n / 2] };
                    (room.key_at(n / 2, n % 2).to_string(), user.id)
                })
                .collect()
        };
        self.send_lineup(k, id);
        self.send(k, msg);
        if let Some(result) = result {
            self.send(k, ServerMsg::MatchEnded { result });
        }
        let blocked = self.sessions.get(k).map(|s| s.blocked.clone()).unwrap_or_default();
        for line in lines {
            let from = key(&line.from);
            if from != k && !ids.get(&from).is_some_and(|i| blocked.contains(i)) {
                self.send(k, ServerMsg::MatchChat { line });
            }
        }
        let count = self.spectators(id).len() as u32;
        if count > 0 {
            self.send(k, ServerMsg::Spectators { count });
            self.send(k, ServerMsg::Support { counts: self.support(id) });
        }
        if team_back {
            let msg = ServerMsg::OpponentConnection {
                connected: true,
                grace_secs: 0,
                clock_left: Some(clock_left),
            };
            for o in &other_team {
                if self.sessions.get(o).is_some_and(|s| s.room == Some(id)) {
                    self.send(o, msg.clone());
                }
            }
        }
        if was_away {
            for o in others {
                self.send_lineup(&o, id);
            }
        }
    }

    /// `k` left the finished doubles game `id` (back to the lounge).
    pub(super) fn doubles_leave_finished(&mut self, k: &str, id: u64) {
        let Some(room) = self.rooms.get_mut(&id) else { return };
        let (team, place) = room.place_of(k);
        let Some(d) = room.doubles.as_mut() else { return };
        d.present[team][place] = false;
        room.present[team] = d.present[team].iter().any(|p| *p);
        self.doubles_changed(id);
    }

    /// A doubles game is over: the ratings (rated), the result for all
    /// four, the record.
    pub(super) fn finish_doubles(&mut self, id: u64) {
        let Some(room) = self.rooms.get(&id) else { return };
        let Some(over) = room.game.game_over() else { return };
        if room.doubles.is_none() {
            return;
        }
        if room.finished {
            return;
        }
        let rules = room.game.rules();
        let rated = room.rated;
        let (w, l) = (over.winner % 2, 1 - over.winner % 2);
        let teams: [[String; 2]; 2] =
            [0, 1].map(|t| [room.key_at(t, 0).to_string(), room.key_at(t, 1).to_string()]);
        // The freshest records (a game between these just recorded).
        let records = teams.clone().map(|t| t.map(|k| self.doubles_record(&k, rules)));
        let mut deltas = [[0; 2]; 2];
        if rated {
            let r = |rec: Record| Rating { rating: rec.rating, games: rec.games };
            let change = team_result(&EloConfig::default(), records[w].map(r), records[l].map(r));
            deltas[w] = change[0];
            deltas[l] = change[1];
        }
        let abandoned = over.reason == GameOverReason::Abandoned;
        if abandoned {
            for k in &teams[l] {
                self.record_abandon(k);
            }
            if rated {
                for p in 0..2 {
                    let floor = EloConfig::default().floor.min(records[l][p].rating);
                    let after = (records[l][p].rating + deltas[l][p] - ABANDON_RATING).max(floor);
                    deltas[l][p] = after - records[l][p].rating;
                }
            }
        }
        let room = &self.rooms[&id];
        let d = room.doubles.as_ref().expect("doubles");
        let ids =
            [[room.users[0].id, d.partner_users[0].id], [room.users[1].id, d.partner_users[1].id]];
        let guests = room.guests;
        let record = DoublesRecord {
            players: ids,
            winner: w,
            reason: over.reason,
            rated,
            rules,
            deltas,
            shots: room.game.shots_taken(),
            ended_at: now(),
            replay: Some(room.replay.clone()),
            room: Some(id as i64),
        };
        let average = |t: usize| {
            let after = |p: usize| {
                let rec = records[t][p];
                Rating { rating: rec.rating + deltas[t][p], games: rec.games }
            };
            team_rating(after(0), after(1)).rating
        };
        let result = MatchResult {
            winner: w,
            reason: over.reason,
            rated,
            rating_delta: [0, 1].map(|t| (deltas[t][0] + deltas[t][1]) / 2),
            new_ratings: [average(0), average(1)],
        };
        // The new numbers at once; the database catches up.
        if rated {
            for t in 0..2 {
                for p in 0..2 {
                    let k = &teams[t][p];
                    let mut rec = self
                        .doubles
                        .records
                        .get(k)
                        .copied()
                        .unwrap_or(PerRules::both(Record::new(DOUBLES_START)));
                    let r = rec.get_mut(rules);
                    r.rating += deltas[t][p];
                    r.peak_rating = r.peak_rating.max(r.rating);
                    r.games += 1;
                    r.wins += u32::from(t == w);
                    self.doubles.records.insert(k.clone(), rec);
                }
            }
        }
        let room = self.rooms.get_mut(&id).expect("room");
        room.finished = true;
        room.result = Some(result.clone());
        room.ended = Some(Instant::now());
        room.rematch = [false, false];
        if let Some(d) = room.doubles.as_mut() {
            d.deltas = rated.then_some(deltas);
            d.rematch = [[false; 2]; 2];
        }
        self.finished_rooms.insert(id, Instant::now());
        self.featured_dirty = true;
        self.send_room(id, &ServerMsg::MatchEnded { result });
        self.doubles_changed(id);
        for k in teams.iter().flatten() {
            self.announce(k);
        }
        if !guests[0] && !guests[1] && !self.settings.lan {
            let keys = teams.clone();
            self.effect(Effect::RecordDoubles { record: Box::new(record), keys });
        }
        tracing::info!(room = id, "doubles finished: {:?}", over);
    }

    /// `k` asks for another doubles game with the same four: once all four
    /// asked, it starts (the other team breaks).
    pub(super) fn doubles_rematch(&mut self, k: &str, id: u64) {
        let Some(room) = self.rooms.get(&id) else { return };
        let all = room.all_keys();
        let present = |l: &Self, p: &String| l.sessions.get(p).is_some_and(|s| s.room == Some(id));
        if !all.iter().all(|p| present(self, p))
            || room.doubles.as_ref().is_some_and(|d| d.present.iter().flatten().any(|p| !p))
        {
            return self.error(k, "Not all four players are at the table.");
        }
        if self.refuse_if_draining(k) {
            return;
        }
        for p in &all {
            if let Some(left) = self.abandon_cooldown(p) {
                return self.error(k, cooldown_message(left));
            }
            if self.tournament_busy(p) {
                return self.error(k, "Players in a tournament can't start other games.");
            }
        }
        let Some(room) = self.rooms.get_mut(&id) else { return };
        let (team, place) = room.place_of(k);
        let Some(d) = room.doubles.as_mut() else { return };
        d.rematch[team][place] = true;
        if !d.rematch.iter().flatten().all(|r| *r) {
            // (The others see who asked on the line-up's players.)
            let others: Vec<String> = all.into_iter().filter(|p| p != k).collect();
            for o in others {
                self.send(&o, ServerMsg::RematchOffered);
            }
            return;
        }
        let d = (**d).clone();
        let keys = room.keys.clone();
        let (rated, guests, look, chat) =
            (room.rated, room.guests, room.table_look.clone(), room.chat.clone());
        let game = Game {
            seed: rand::thread_rng().r#gen(),
            breaker: 1 - room.breaker,
            rules: room.game.rules(),
        };
        self.rooms.remove(&id);
        self.featured_dirty = true;
        for p in &all {
            if let Some(s) = self.sessions.get_mut(p) {
                s.room = None;
            }
        }
        let rules = game.rules;
        let teams =
            [[keys[0].clone(), d.partners[0].clone()], [keys[1].clone(), d.partners[1].clone()]];
        let fresh = Doubles {
            away: [[None; 2]; 2],
            present: [[true; 2]; 2],
            chat_missed: [[None; 2]; 2],
            records: teams.clone().map(|t| t.map(|k| self.doubles_record(&k, rules))),
            rematch: [[false; 2]; 2],
            deltas: None,
            partner_users: [0, 1].map(|t| self.sessions[&d.partners[t]].user.clone()),
            ..d
        };
        let users = [0, 1].map(|t| self.sessions[&keys[t]].user.clone());
        let new_id = self.new_id();
        self.create_room(
            new_id,
            keys,
            users,
            guests,
            rated,
            game,
            None,
            look,
            false,
            Some(Box::new(fresh)),
        );
        if let Some(r) = self.rooms.get_mut(&new_id) {
            r.chat = chat;
        }
    }

    /// Who is online and may play doubles (the staff panel: pairs and
    /// what they are doing).
    pub fn doubles_now(&self) -> (usize, usize, usize) {
        let pairs = self.doubles.partners.len() / 2;
        let games = self.rooms.values().filter(|r| r.doubles.is_some() && !r.finished).count();
        (pairs, self.doubles.queue.len(), games)
    }
}
