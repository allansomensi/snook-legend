//! Games against the computer, as far as the server is concerned: the
//! ladder's reward and the style of a player's ghost.
//!
//! The ladder is played offline; the reward is only given here, after the
//! final game has been played again stroke by stroke ([`ladder::verify`]).
//! A ghost's style is measured on the player's latest recorded games and
//! only the aggregated numbers are sent. Both mean seconds of simulation,
//! so they run on blocking threads, a few at a time, and are rate-limited
//! per account.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use snookered_proto::ghost::{self, GhostAnswer, GhostStyle, Tally};
use snookered_proto::ladder::{self, ClaimFailure, ClaimResult};
use snookered_proto::replay::Replay;
use snookered_proto::{ServerMsg, valid_username};
use tokio::sync::Semaphore;

use crate::store::User;
use crate::{Server, now};

/// How long a claim waits for its turn before the player is asked to try
/// again.
const QUEUE_WAIT: Duration = Duration::from_secs(20);
/// Claims of one account at most this often (a refused one included).
const CLAIM_EVERY: Duration = Duration::from_secs(30);
/// Ghost styles one account may ask for: a burst, then one every few seconds.
const GHOST_BURST: f64 = 4.0;
const GHOST_RATE: f64 = 1.0 / 8.0;
/// A measured style is kept this long (unless the player plays again).
const GHOST_CACHE: Duration = Duration::from_secs(30 * 60);
const GHOST_CACHE_SIZE: usize = 500;

/// Shared state: the queues, the per-account limits and the measured styles.
pub struct Computer {
    claims: Semaphore,
    styles: Semaphore,
    last_claim: Mutex<HashMap<i64, Instant>>,
    ghost_limits: Mutex<HashMap<i64, crate::limits::Bucket>>,
    /// user -> (when, their latest match then, the style).
    cache: Mutex<HashMap<i64, (Instant, i64, GhostStyle)>>,
}

impl Default for Computer {
    fn default() -> Self {
        Computer {
            // One game checked at a time: a claim is seconds of CPU time.
            claims: Semaphore::new(1),
            styles: Semaphore::new(2),
            last_claim: Mutex::new(HashMap::new()),
            ghost_limits: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Computer {
    /// True (and remembered) if `user` may claim now.
    fn may_claim(&self, user: i64) -> bool {
        let mut last = lock(&self.last_claim);
        let now = Instant::now();
        last.retain(|_, t| now.duration_since(*t) < CLAIM_EVERY);
        if last.contains_key(&user) {
            return false;
        }
        last.insert(user, now);
        true
    }

    fn may_ask_ghost(&self, user: i64) -> bool {
        let mut limits = lock(&self.ghost_limits);
        if limits.len() > 10_000 {
            limits.clear();
        }
        limits
            .entry(user)
            .or_insert_with(|| crate::limits::Bucket::new(GHOST_BURST, GHOST_RATE))
            .take()
    }

    fn cached(&self, user: i64, latest: i64) -> Option<GhostStyle> {
        let cache = lock(&self.cache);
        let (at, at_match, style) = cache.get(&user)?;
        (*at_match == latest && at.elapsed() < GHOST_CACHE).then_some(*style)
    }

    fn remember(&self, user: i64, latest: i64, style: GhostStyle) {
        let mut cache = lock(&self.cache);
        if cache.len() >= GHOST_CACHE_SIZE {
            cache.retain(|_, (at, _, _)| at.elapsed() < GHOST_CACHE);
            if cache.len() >= GHOST_CACHE_SIZE {
                cache.clear();
            }
        }
        cache.insert(user, (Instant::now(), latest, style));
    }
}

/// The ladder's reward for `me`, if the game checks out.
pub async fn claim(server: &Server, me: &User, replay: Replay) -> ClaimResult {
    let fail = |reason| ClaimResult::Failed { reason };
    // Once per account: if they have it, there is nothing to check.
    match server.store.ladder_claim(me.id).await {
        Ok(Some(_)) => {
            // The achievement too (should the first grant have stopped half way).
            award(server, me).await;
            return ClaimResult::Granted { already: true };
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("ladder claim of {}: {e}", me.username);
            return fail(ClaimFailure::ServerError);
        }
    }
    if replay.strokes.len() > ladder::MAX_STROKES {
        return fail(ClaimFailure::TooLong);
    }
    if !server.computer.may_claim(me.id) {
        return fail(ClaimFailure::TooSoon);
    }
    let Ok(Ok(_permit)) = tokio::time::timeout(QUEUE_WAIT, server.computer.claims.acquire()).await
    else {
        return fail(ClaimFailure::Busy);
    };
    let started = Instant::now();
    let checked = replay.clone();
    let verdict = tokio::task::spawn_blocking(move || ladder::verify(&checked))
        .await
        .unwrap_or(Err(ClaimFailure::ServerError));
    tracing::info!(
        user = %me.username,
        strokes = replay.strokes.len(),
        ms = started.elapsed().as_millis() as u64,
        "ladder claim: {:?}",
        verdict.as_ref().err()
    );
    if let Err(reason) = verdict {
        return fail(reason);
    }
    match server
        .store
        .record_ladder_claim(me.id, replay.seed, replay.strokes.len() as u32, now())
        .await
    {
        Ok(fresh) => {
            award(server, me).await;
            ClaimResult::Granted { already: !fresh }
        }
        Err(e) => {
            tracing::error!("recording the ladder claim of {}: {e}", me.username);
            fail(ClaimFailure::ServerError)
        }
    }
}

/// Unlocks the ladder's achievement (which brings the item) and tells the
/// player, as [`Server::check_achievements`] does.
async fn award(server: &Server, me: &User) {
    match server.store.add_achievements(me.id, &[ladder::ACHIEVEMENT], now()).await {
        Ok(new) => {
            let all = server.store.achievements(me.id).await.unwrap_or_default();
            let codes: Vec<String> = all.into_iter().map(|(code, _)| code).collect();
            server.with(|l| {
                for code in new {
                    l.notify(&me.username, ServerMsg::AchievementUnlocked { code });
                }
                l.set_achievements(&me.username, codes);
            });
            crate::shop::push_wallet(server, me).await;
        }
        Err(e) => tracing::error!("the ladder achievement of {}: {e}", me.username),
    }
}

/// How `username` plays, for `me` to play against their ghost.
pub async fn ghost_style(server: &Server, me: &User, username: &str) -> GhostAnswer {
    let username = username.trim();
    if !valid_username(username) {
        return GhostAnswer::NoSuchPlayer;
    }
    if !server.computer.may_ask_ghost(me.id) {
        return GhostAnswer::TooSoon;
    }
    let target = match server.store.user_by_name(username).await {
        Ok(Some(u)) if u.deleted_at.is_none() => u,
        Ok(_) => return GhostAnswer::NoSuchPlayer,
        Err(e) => {
            tracing::error!("ghost of {username}: {e}");
            return GhostAnswer::Busy;
        }
    };
    let games = match server.store.recent_replays(target.id, ghost::MAX_GAMES).await {
        Ok(g) => g,
        Err(e) => {
            tracing::error!("ghost of {username}: {e}");
            return GhostAnswer::Busy;
        }
    };
    let latest = games.first().map_or(0, |(id, _, _)| *id);
    let style = match server.computer.cached(target.id, latest) {
        Some(style) => style,
        None => {
            let Ok(Ok(_permit)) =
                tokio::time::timeout(QUEUE_WAIT, server.computer.styles.acquire()).await
            else {
                return GhostAnswer::Busy;
            };
            let measured = tokio::task::spawn_blocking(move || {
                let mut tally = Tally::default();
                for (_, seat, replay) in &games {
                    tally.add_game(replay, *seat);
                }
                tally.style()
            })
            .await;
            let Ok(style) = measured else { return GhostAnswer::Busy };
            server.computer.remember(target.id, latest, style);
            style
        }
    };
    if style.shots < ghost::MIN_SHOTS {
        return GhostAnswer::NotEnough { shots: style.shots, needed: ghost::MIN_SHOTS };
    }
    GhostAnswer::Ready { style }
}

/// Answers a claim on its own task (it can take seconds), so the player's
/// other messages are not held up.
pub fn spawn_claim(
    server: std::sync::Arc<Server>,
    me: User,
    replay: Replay,
    tx: tokio::sync::mpsc::Sender<ServerMsg>,
) {
    tokio::spawn(async move {
        let result = claim(&server, &me, replay).await;
        let _ = tx.send(ServerMsg::LadderClaim { result }).await;
    });
}

/// Answers a request for a ghost's style on its own task.
pub fn spawn_ghost(
    server: std::sync::Arc<Server>,
    me: User,
    username: String,
    tx: tokio::sync::mpsc::Sender<ServerMsg>,
) {
    tokio::spawn(async move {
        let answer = ghost_style(&server, &me, &username).await;
        let username = username.trim().to_string();
        let _ = tx.send(ServerMsg::GhostStyle { username, answer }).await;
    });
}
