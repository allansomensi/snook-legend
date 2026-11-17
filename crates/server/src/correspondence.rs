//! Correspondence ("daily") games: each player takes their turn whenever
//! they can, so a game goes on when the lounge is empty.
//!
//! The server is the authority and keeps nothing in memory that matters: a
//! game is its seed and strokes in the store, and the table is rebuilt by
//! replaying them (a small cache saves replaying on every shot). A turn is
//! a whole visit (ball in hand and calling the 8 included) with a deadline
//! ([`TURN_SECS`]); whoever runs out of it loses, applied by a timer and
//! whenever a game is looked at. Every change is saved against the number
//! of strokes it was made on, so two changes never both land.
//!
//! What a finished game counts for: it goes into the match history with its
//! replay and, when rated, moves the ratings (Elo, with the same limit on
//! games between the same two players as live games) and the lifetime
//! numbers achievements are made of. It earns no coins and no mission
//! progress, and its shots are left out of the aim review: with days per
//! turn and the table on screen for as long as one likes (the physics can
//! even be tried out offline), those would reward patience, not play, and
//! several slow games at once against a friend would make them easy to farm.
//! A turn that runs out is not counted as leaving a game.
//!
//! Only games of protocol [`SINCE_PROTOCOL`] and later get these messages.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rand::Rng;
use snook_legend_core::rating::{self, EloConfig};
use snook_legend_core::stats::ShotStats;
use snook_legend_core::{BallId, EightBallGame, GameOver, GameOverReason, PocketId, Shot};
use snook_legend_proto::correspondence::{
    CHALLENGE_SECS, CorrChallenge, CorrEvent, CorrGame, CorrResult, MAX_GAMES,
    MAX_RECEIVED_CHALLENGES, MAX_SENT_CHALLENGES, RECENT_FINISHED, SINCE_PROTOCOL, TURN_SECS,
    summary,
};
use snook_legend_proto::replay::{Replay, Stroke, apply};
use snook_legend_proto::{ClientMsg, PlayerInfo, PlayerStatus, ServerMsg};

use crate::limits::Bucket;
use crate::lounge::{key, player_info};
use crate::store::{AsyncGame, MatchRecord, NewAsyncGame, User};
use crate::{Server, now, parse_reason};

/// How often the server looks for turns that ran out.
const SWEEP_EVERY: Duration = Duration::from_secs(60);
/// Tables kept ready to play on (the rest are rebuilt from their strokes).
const CACHE_SIZE: usize = 256;

/// What the server keeps between messages: one lock per game being
/// changed, the latest tables and each player's challenge allowance.
#[derive(Default)]
pub struct State {
    locks: Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>,
    /// Game id -> (strokes, the table after them).
    tables: Mutex<HashMap<i64, (usize, EightBallGame)>>,
    /// User id -> challenges they may still send.
    challenges: Mutex<HashMap<i64, Bucket>>,
}

impl State {
    fn lock_for(&self, id: i64) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.locks.lock().unwrap_or_else(|e| e.into_inner());
        if locks.len() > 1000 {
            locks.retain(|_, l| Arc::strong_count(l) > 1);
        }
        locks.entry(id).or_default().clone()
    }

    fn cached(&self, id: i64, strokes: usize) -> Option<EightBallGame> {
        let tables = self.tables.lock().unwrap_or_else(|e| e.into_inner());
        tables.get(&id).filter(|(n, _)| *n == strokes).map(|(_, g)| g.clone())
    }

    fn remember(&self, id: i64, strokes: usize, game: &EightBallGame) {
        let mut tables = self.tables.lock().unwrap_or_else(|e| e.into_inner());
        if game.game_over().is_some() {
            tables.remove(&id);
            return;
        }
        if tables.len() >= CACHE_SIZE && !tables.contains_key(&id) {
            tables.clear();
        }
        tables.insert(id, (strokes, game.clone()));
    }

    /// Ten challenges at once, then one a minute.
    fn may_challenge(&self, user_id: i64) -> bool {
        let mut map = self.challenges.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() > 10_000 {
            map.clear();
        }
        map.entry(user_id).or_insert_with(|| Bucket::new(10.0, 1.0 / 60.0)).take()
    }
}

/// Messages handled here.
pub fn is_msg(msg: &ClientMsg) -> bool {
    matches!(
        msg,
        ClientMsg::CorrList
            | ClientMsg::CorrChallenge { .. }
            | ClientMsg::CorrAnswer { .. }
            | ClientMsg::CorrCancel { .. }
            | ClientMsg::CorrOpen { .. }
            | ClientMsg::CorrShoot { .. }
            | ClientMsg::CorrResign { .. }
    )
}

/// Handles a correspondence message from a signed-in player with an account.
pub async fn handle(server: &Arc<Server>, me: &User, msg: ClientMsg, send: &impl Fn(ServerMsg)) {
    if server.config.rules.lan {
        return send(ServerMsg::Error { message: "Daily games need an account.".into() });
    }
    match msg {
        ClientMsg::CorrList => {
            expire_due(server, me.id).await;
            if let Some(msg) = list_msg(server, me.id).await {
                send(msg);
            }
        }
        ClientMsg::CorrChallenge { to, rated } => challenge(server, me, &to, rated, send).await,
        ClientMsg::CorrAnswer { id, accept } => answer(server, me, id, accept, send).await,
        ClientMsg::CorrCancel { id } => cancel(server, me, id, send).await,
        ClientMsg::CorrOpen { id } => open(server, me, id, send).await,
        ClientMsg::CorrShoot { id, turn, shot, called_pocket, cue_ball } => {
            shoot(server, me, id, turn, shot, called_pocket, cue_ball, send).await
        }
        ClientMsg::CorrResign { id } => resign(server, me, id, send).await,
        _ => {}
    }
}

/// A player with a game that knows these messages signed in: their games
/// (turns that ran out are settled first).
pub async fn signed_in(server: &Arc<Server>, user: &User) {
    expire_due(server, user.id).await;
    push_list(server, user.id, &user.username).await;
}

/// Runs the timer: turns that ran out and challenges nobody answered.
pub async fn run(server: Arc<Server>) {
    let mut interval = tokio::time::interval(SWEEP_EVERY);
    loop {
        interval.tick().await;
        sweep(&server, now()).await;
    }
}

/// Settles every turn that ran out before `now` and withdraws challenges
/// older than [`CHALLENGE_SECS`].
pub async fn sweep(server: &Arc<Server>, now: i64) {
    // In batches (a game that can't be settled is tried again next time).
    for _ in 0..20 {
        let due = match server.store.expired_async_games(now, 50).await {
            Ok(list) => list,
            Err(e) => {
                tracing::error!("correspondence timeouts: {e}");
                break;
            }
        };
        let n = due.len();
        for g in due {
            time_out(server, g.id, now).await;
        }
        if n < 50 {
            break;
        }
    }
    match server.store.purge_async_challenges(now - CHALLENGE_SECS).await {
        Ok(old) => {
            for c in old {
                let (from, to) = (name_of(server, c.from).await, name_of(server, c.to).await);
                tell(server, &to, c.id, &from, CorrEvent::Withdrawn);
                push_list(server, c.from, &from).await;
                push_list(server, c.to, &to).await;
            }
        }
        Err(e) => tracing::error!("correspondence challenges: {e}"),
    }
}

/// Settles this player's games whose turn ran out ("on load").
async fn expire_due(server: &Arc<Server>, user_id: i64) {
    let t = now();
    let games = server.store.async_games_of(user_id, 0).await.unwrap_or_default();
    for g in games.into_iter().filter(|g| g.is_active() && g.deadline < t) {
        time_out(server, g.id, t).await;
    }
}

async fn name_of(server: &Server, id: i64) -> String {
    server.store.user_by_id(id).await.ok().flatten().map(|u| u.username).unwrap_or_default()
}

/// Sends a correspondence message to `name` if their game knows it.
fn send_to(server: &Server, name: &str, msg: ServerMsg) {
    server.with(|l| l.notify_since(name, SINCE_PROTOCOL, msg));
}

fn tell(server: &Server, name: &str, id: i64, opponent: &str, event: CorrEvent) {
    send_to(server, name, ServerMsg::CorrNotice { id, opponent: opponent.to_string(), event });
}

fn knows_corr(server: &Server, name: &str) -> bool {
    server.with(|l| l.protocol_of(name)).is_some_and(|v| v >= SINCE_PROTOCOL)
}

/// Sends a fresh list to `name` if they are online with a game that knows it.
async fn push_list(server: &Server, user_id: i64, name: &str) {
    if name.is_empty() || !knows_corr(server, name) {
        return;
    }
    if let Some(msg) = list_msg(server, user_id).await {
        send_to(server, name, msg);
    }
}

/// How players look in the lists: as in the lounge when online.
async fn infos(server: &Server, ids: impl IntoIterator<Item = i64>) -> HashMap<i64, PlayerInfo> {
    let mut out = HashMap::new();
    for id in ids {
        if out.contains_key(&id) {
            continue;
        }
        let Ok(Some(u)) = server.store.user_by_id(id).await else { continue };
        let info = server
            .with(|l| l.player(&u.username))
            .unwrap_or_else(|| player_info(&u, false, PlayerStatus::Available));
        out.insert(id, info);
    }
    out
}

fn unknown_player() -> PlayerInfo {
    PlayerInfo {
        username: "?".into(),
        rating: 0,
        games: 0,
        wins: 0,
        status: PlayerStatus::Available,
        country: String::new(),
        role: Default::default(),
        guest: false,
        vip: false,
        cue: String::new(),
        chalk: String::new(),
        frame: String::new(),
        name_style: String::new(),
        effect: String::new(),
        status_message: String::new(),
        avatar: String::new(),
        vs_computer: false,
        away: false,
        level: 0,
        cue_level: 0,
    }
}

fn corr_game(g: &AsyncGame, seat: usize, infos: &HashMap<i64, PlayerInfo>) -> CorrGame {
    let info = |k: usize| infos.get(&g.players[k]).cloned().unwrap_or_else(unknown_player);
    let strokes = g.replay.strokes.len() as u32;
    let result = g.finished_at.map(|at| CorrResult {
        winner: g.winner.unwrap_or(0),
        reason: parse_reason(g.reason.as_deref().unwrap_or("")),
        timeout: g.timeout,
        delta: g.delta,
        ratings: g.ratings,
        at,
        match_id: g.match_id.unwrap_or(0),
    });
    CorrGame {
        id: g.id,
        players: [info(0), info(1)],
        seat,
        rated: g.rated,
        turn: g.turn,
        deadline: g.deadline,
        strokes,
        unseen: strokes.saturating_sub(g.seen[seat]),
        groups: g.groups,
        left: g.left,
        started_at: g.started_at,
        updated_at: g.updated_at,
        result,
    }
}

/// This player's games and challenges.
async fn list_msg(server: &Server, user_id: i64) -> Option<ServerMsg> {
    let games = server.store.async_games_of(user_id, RECENT_FINISHED as u32).await.ok()?;
    let challenges = server.store.async_challenges_of(user_id).await.ok()?;
    let ids = games
        .iter()
        .flat_map(|g| g.players)
        .chain(challenges.iter().flat_map(|c| [c.from, c.to]))
        .collect::<Vec<_>>();
    let infos = infos(server, ids).await;
    let games =
        games.iter().filter_map(|g| Some(corr_game(g, g.seat_of(user_id)?, &infos))).collect();
    let info = |id: i64| infos.get(&id).cloned().unwrap_or_else(unknown_player);
    let challenges = challenges
        .iter()
        .map(|c| CorrChallenge {
            id: c.id,
            from: info(c.from),
            to: info(c.to),
            rated: c.rated,
            at: c.created_at,
            expires_at: c.created_at + CHALLENGE_SECS,
        })
        .collect();
    Some(ServerMsg::CorrList { games, challenges })
}

fn error(send: &impl Fn(ServerMsg), message: &str) {
    send(ServerMsg::Error { message: message.into() });
}

async fn challenge(
    server: &Arc<Server>,
    me: &User,
    to: &str,
    rated: bool,
    send: &impl Fn(ServerMsg),
) {
    let other = match server.store.user_by_name(to.trim()).await {
        Ok(Some(u)) if u.deleted_at.is_none() => u,
        _ => return error(send, "No such player."),
    };
    if other.id == me.id {
        return error(send, "You can't challenge yourself.");
    }
    let blocked_by_them =
        server.store.blocked(other.id).await.unwrap_or_default().iter().any(|u| u.id == me.id);
    if blocked_by_them || other.is_banned(now()) {
        return error(send, "That player is not accepting challenges.");
    }
    if server.store.blocked(me.id).await.unwrap_or_default().iter().any(|u| u.id == other.id) {
        return error(send, "You blocked that player.");
    }
    let (Ok(mine), Ok(theirs)) = (
        server.store.async_games_of(me.id, 0).await,
        server.store.async_games_of(other.id, 0).await,
    ) else {
        return error(send, "The server is busy. Try again in a moment.");
    };
    if mine.iter().any(|g| g.players.contains(&other.id)) {
        return error(send, "You already have a game going on against that player.");
    }
    if mine.len() >= MAX_GAMES {
        return error(send, "You have as many daily games as you can play at once.");
    }
    if theirs.len() >= MAX_GAMES {
        return error(send, "That player has too many daily games going on.");
    }
    let (Ok(sent), Ok(received)) = (
        server.store.async_challenges_of(me.id).await,
        server.store.async_challenges_of(other.id).await,
    ) else {
        return error(send, "The server is busy. Try again in a moment.");
    };
    if sent.iter().any(|c| c.from == other.id) {
        return error(send, "That player already challenged you: answer their challenge.");
    }
    if sent.iter().filter(|c| c.from == me.id).count() >= MAX_SENT_CHALLENGES {
        return error(send, "You have too many challenges waiting for an answer.");
    }
    if received.iter().filter(|c| c.to == other.id).count() >= MAX_RECEIVED_CHALLENGES {
        return error(send, "That player has too many challenges waiting.");
    }
    // Only challenges that would be sent count (withdrawing and sending
    // again is how one would spam).
    if !server.corr.may_challenge(me.id) {
        return error(send, "You're sending challenges too quickly.");
    }
    match server.store.create_async_challenge(me.id, other.id, rated, now()).await {
        Ok(Some(id)) => {
            tracing::info!(id, "daily game challenge: {} -> {}", me.username, other.username);
            send(ServerMsg::Notice { text: "Challenge sent.".into() });
            tell(server, &other.username, id, &me.username, CorrEvent::Challenged { rated });
            push_list(server, me.id, &me.username).await;
            push_list(server, other.id, &other.username).await;
        }
        Ok(None) => error(send, "You already challenged that player."),
        Err(e) => {
            tracing::error!("correspondence challenge: {e}");
            error(send, "The server is busy. Try again in a moment.")
        }
    }
}

async fn answer(server: &Arc<Server>, me: &User, id: i64, accept: bool, send: &impl Fn(ServerMsg)) {
    let c = match server.store.async_challenge(id).await {
        Ok(Some(c)) if c.to == me.id => c,
        _ => {
            push_list(server, me.id, &me.username).await;
            return error(send, "That challenge is no longer valid.");
        }
    };
    let from = name_of(server, c.from).await;
    if !accept {
        if server.store.delete_async_challenge(id).await.unwrap_or(false) {
            tell(server, &from, id, &me.username, CorrEvent::Declined);
        }
        push_list(server, me.id, &me.username).await;
        push_list(server, c.from, &from).await;
        return;
    }
    let (Ok(mine), Ok(theirs)) =
        (server.store.async_games_of(me.id, 0).await, server.store.async_games_of(c.from, 0).await)
    else {
        return error(send, "The server is busy. Try again in a moment.");
    };
    if mine.iter().any(|g| g.players.contains(&c.from)) {
        return error(send, "You already have a game going on against that player.");
    }
    if mine.len() >= MAX_GAMES {
        return error(send, "You have as many daily games as you can play at once.");
    }
    if theirs.len() >= MAX_GAMES {
        return error(send, "That player has too many daily games going on.");
    }
    let t = now();
    let (seed, breaker) = {
        let mut rng = rand::thread_rng();
        (rng.r#gen::<u64>(), rng.gen_range(0..2usize))
    };
    let new = NewAsyncGame { seed, breaker, deadline: t + TURN_SECS, at: t };
    match server.store.accept_async_challenge(id, &new).await {
        Ok(Some(g)) => {
            tracing::info!(game = g.id, "daily game started: {} vs {}", from, me.username);
            tell(server, &from, g.id, &me.username, CorrEvent::Accepted);
            push_list(server, me.id, &me.username).await;
            push_list(server, c.from, &from).await;
        }
        Ok(None) => error(send, "That challenge is no longer valid."),
        Err(e) => {
            tracing::error!("correspondence accept: {e}");
            error(send, "The server is busy. Try again in a moment.")
        }
    }
}

async fn cancel(server: &Arc<Server>, me: &User, id: i64, send: &impl Fn(ServerMsg)) {
    match server.store.async_challenge(id).await {
        Ok(Some(c)) if c.from == me.id => {
            if server.store.delete_async_challenge(id).await.unwrap_or(false) {
                let to = name_of(server, c.to).await;
                tell(server, &to, id, &me.username, CorrEvent::Withdrawn);
                push_list(server, c.to, &to).await;
            }
        }
        _ => error(send, "That challenge is no longer valid."),
    }
    push_list(server, me.id, &me.username).await;
}

/// The table after every stroke of `g` (from the cache, or replayed).
async fn table_of(server: &Server, g: &AsyncGame) -> Result<EightBallGame, String> {
    let n = g.replay.strokes.len();
    if let Some(game) = server.corr.cached(g.id, n) {
        return Ok(game);
    }
    let replay = g.replay.clone();
    let game = tokio::task::spawn_blocking(move || replay.play(None).map(|(game, _)| game))
        .await
        .map_err(|e| format!("replaying panicked: {e}"))??;
    server.corr.remember(g.id, n, &game);
    Ok(game)
}

async fn open(server: &Arc<Server>, me: &User, id: i64, send: &impl Fn(ServerMsg)) {
    let Some((g, seat)) = load(server, me, id, send).await else { return };
    let g = if g.is_active() && g.deadline < now() {
        time_out(server, id, now()).await;
        match server.store.async_game(id).await {
            Ok(Some(g)) => g,
            _ => return error(send, "No such game."),
        }
    } else {
        g
    };
    let infos = infos(server, g.players).await;
    let seen = g.seen[seat].min(g.replay.strokes.len() as u32);
    send(ServerMsg::CorrState {
        game: corr_game(&g, seat, &infos),
        replay: g.replay.clone(),
        seen,
    });
    let n = g.replay.strokes.len() as u32;
    if seen < n {
        let _ = server.store.set_async_seen(id, seat, n).await;
    }
}

/// A game of `me` and their seat (an error is sent otherwise).
async fn load(
    server: &Server,
    me: &User,
    id: i64,
    send: &impl Fn(ServerMsg),
) -> Option<(AsyncGame, usize)> {
    match server.store.async_game(id).await {
        Ok(Some(g)) => match g.seat_of(me.id) {
            Some(seat) => Some((g, seat)),
            None => {
                error(send, "No such game.");
                None
            }
        },
        Ok(None) => {
            error(send, "No such game.");
            None
        }
        Err(e) => {
            tracing::error!("correspondence game {id}: {e}");
            error(send, "The server is busy. Try again in a moment.");
            None
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn shoot(
    server: &Arc<Server>,
    me: &User,
    id: i64,
    turn: u32,
    shot: Shot,
    called: Option<PocketId>,
    cue_ball: Option<glam::DVec2>,
    send: &impl Fn(ServerMsg),
) {
    let reject = |reason: &str, game: EightBallGame| {
        send(ServerMsg::CorrShotRejected { id, turn, reason: reason.into(), game })
    };
    let finite = shot.yaw.is_finite()
        && shot.elevation.is_finite()
        && shot.tip.is_finite()
        && shot.speed.is_finite()
        && cue_ball.is_none_or(|p| p.is_finite());
    let lock = server.corr.lock_for(id);
    let guard = lock.lock().await;
    let Some((mut g, seat)) = load(server, me, id, send).await else { return };
    let table = match table_of(server, &g).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(game = id, "correspondence game can't be rebuilt: {e}");
            return error(send, "This game can't be played right now.");
        }
    };
    if !finite {
        return reject("Invalid shot.", table);
    }
    if !g.is_active() || table.game_over().is_some() {
        return reject("This game is over.", table);
    }
    let t = now();
    if g.deadline < t {
        drop(guard);
        time_out(server, id, t).await;
        return reject("Your time for this turn ran out.", table);
    }
    if g.turn != seat || table.current_player() != seat {
        return reject("It's not your turn.", table);
    }
    if table.shots_taken() != turn {
        return reject("That shot was for an earlier turn.", table);
    }
    let before = table.clone();
    let played = tokio::task::spawn_blocking(move || {
        let mut game = before;
        if let Some(p) = cue_ball
            && game.ball_in_hand().is_some()
            && game.ball(BallId::CUE).xy() != p
            && !game.place_cue_ball(p)
        {
            return Err("The cue ball can't be placed there.".to_string());
        }
        let placed = game.ball_in_hand().map(|_| game.ball(BallId::CUE).xy());
        let mut pending =
            game.prepare_shot(&shot, called).map_err(|e| format!("Can't shoot: {e}."))?;
        pending.simulation.run_to_rest();
        let report = game.finish_shot(pending);
        Ok((game, report, placed))
    })
    .await
    .unwrap_or_else(|e| Err(format!("simulation panicked: {e}")));
    let (end, report, placed) = match played {
        Ok(p) => p,
        Err(reason) => return reject(&reason, table),
    };
    let expected = g.replay.strokes.len();
    g.replay.strokes.push(Stroke::Shot { shot, called, cue_ball: placed, timeout: false });
    let n = g.replay.strokes.len();
    g.seen[seat] = n as u32;
    g.updated_at = t;
    (g.groups, g.left) = summary(&end);
    let next = end.current_player();
    let passed = next != seat;
    let over = end.game_over();
    let finished = match over {
        Some(over) => match finish(server, g.clone(), expected, &end, over, false, t).await {
            Some(f) => Some(f),
            None => return reject("This game changed meanwhile: open it again.", table),
        },
        None => {
            if passed {
                g.turn = next;
                g.deadline = t + TURN_SECS;
            }
            match server.store.update_async_game(&g, expected).await {
                Ok(true) => {}
                Ok(false) => return reject("This game changed meanwhile: open it again.", table),
                Err(e) => {
                    tracing::error!(game = id, "saving a correspondence shot: {e}");
                    return reject("The server could not play that shot.", table);
                }
            }
            None
        }
    };
    server.corr.remember(id, n, &end);
    drop(guard);
    send(ServerMsg::CorrShot { id, turn, end, report });
    match finished {
        Some(f) => announce(server, f).await,
        None if passed => {
            let other = g.players[1 - seat];
            let other_name = name_of(server, other).await;
            tell(server, &other_name, id, &me.username, CorrEvent::YourTurn);
            push_list(server, me.id, &me.username).await;
            push_list(server, other, &other_name).await;
        }
        None => {}
    }
}

async fn resign(server: &Arc<Server>, me: &User, id: i64, send: &impl Fn(ServerMsg)) {
    let lock = server.corr.lock_for(id);
    let guard = lock.lock().await;
    let Some((mut g, seat)) = load(server, me, id, send).await else { return };
    if !g.is_active() {
        return error(send, "This game is over.");
    }
    let mut table = match table_of(server, &g).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(game = id, "correspondence game can't be rebuilt: {e}");
            return error(send, "This game can't be played right now.");
        }
    };
    let expected = g.replay.strokes.len();
    table.resign(seat);
    g.replay.strokes.push(Stroke::Resign { seat });
    g.seen[seat] = g.replay.strokes.len() as u32;
    let Some(over) = table.game_over() else { return };
    let finished = finish(server, g, expected, &table, over, false, now()).await;
    drop(guard);
    match finished {
        Some(f) => announce(server, f).await,
        None => error(send, "This game changed meanwhile: open it again."),
    }
}

/// The player to move ran out of time: they lose the game.
async fn time_out(server: &Arc<Server>, id: i64, now: i64) {
    let lock = server.corr.lock_for(id);
    let guard = lock.lock().await;
    let Ok(Some(mut g)) = server.store.async_game(id).await else { return };
    if !g.is_active() || g.deadline >= now {
        return;
    }
    let mut table = match table_of(server, &g).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(game = id, "correspondence game can't be rebuilt: {e}");
            return;
        }
    };
    let seat = g.turn;
    let expected = g.replay.strokes.len();
    table.abandon(seat);
    g.replay.strokes.push(Stroke::Abandon { seat });
    let Some(over) = table.game_over() else { return };
    tracing::info!(game = id, seat, "correspondence turn ran out");
    let finished = finish(server, g, expected, &table, over, true, now).await;
    drop(guard);
    if let Some(f) = finished {
        announce(server, f).await;
    }
}

/// A game that was just recorded, to tell its players.
struct Finished {
    game: AsyncGame,
    users: [Option<User>; 2],
    over: GameOver,
    timeout: bool,
}

/// Each seat's numbers and turns over the whole game (for the record).
fn analyse(replay: &Replay) -> Result<([ShotStats; 2], [u32; 2]), String> {
    let mut game = replay.start();
    let mut stats = [ShotStats::default(); 2];
    let mut turns = [0u32; 2];
    turns[replay.breaker % 2] = 1;
    for stroke in &replay.strokes {
        let shooter = game.current_player();
        let in_hand = game.ball_in_hand().is_some();
        if let Some(r) = apply(&mut game, stroke)? {
            stats[shooter].add(&r, in_hand);
            if let Stroke::Shot { shot, .. } = stroke {
                stats[shooter].add_spin(shot, &r);
            }
        }
        let next = game.current_player();
        if next != shooter && game.game_over().is_none() {
            turns[next] += 1;
        }
    }
    Ok((stats, turns))
}

/// Records the end of `g` (with its last stroke already in it, made on
/// `expected` strokes): ratings, history, the replay. `None` if the game
/// changed meanwhile.
async fn finish(
    server: &Arc<Server>,
    mut g: AsyncGame,
    expected: usize,
    table: &EightBallGame,
    over: GameOver,
    timeout: bool,
    at: i64,
) -> Option<Finished> {
    let (a, b) =
        tokio::join!(server.store.user_by_id(g.players[0]), server.store.user_by_id(g.players[1]));
    let (Ok(Some(a)), Ok(Some(b))) = (a, b) else {
        tracing::error!(game = g.id, "correspondence game without its players");
        return None;
    };
    let users = [a, b];
    let replay = g.replay.clone();
    let (stats, turns) = tokio::task::spawn_blocking(move || analyse(&replay))
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    let (w, l) = (over.winner, 1 - over.winner);
    let mut delta = [0, 0];
    let mut ratings_after = [users[0].rating, users[1].rating];
    if g.rated {
        let keys = [key(&users[0].username), key(&users[1].username)];
        let repeats = server.with(|lounge| lounge.count_pair_game(&keys));
        let change =
            rating::apply_result(&EloConfig::default(), users[w].rating(), users[l].rating());
        delta[w] = change.winner_delta;
        delta[l] = change.loser_delta;
        // The same two players again and again: less and less at stake.
        let scale = match repeats {
            0..=2 => 1.0,
            3..=4 => 0.5,
            _ => 0.0,
        };
        for seat in 0..2 {
            delta[seat] = (delta[seat] as f64 * scale).round() as i32;
            ratings_after[seat] = users[seat].rating + delta[seat];
        }
    }
    let eight_win = over.reason == GameOverReason::PocketedEight;
    let record = MatchRecord {
        players: g.players,
        winner: w,
        reason: over.reason,
        rated: g.rated,
        delta,
        ratings_after,
        shots: table.shots_taken(),
        tournament: None,
        eight_win,
        runout: eight_win && turns[l] == 0 && g.replay.breaker == w,
        shutout: eight_win && table.group_of(l).is_some_and(|gr| table.remaining(gr).len() == 7),
        ended_at: at,
        replay: Some(g.replay.clone()),
        stats,
        abandoned: None,
    };
    (g.groups, g.left) = summary(table);
    g.timeout = timeout;
    g.delta = delta;
    g.ratings = ratings_after;
    match server.store.finish_async_game(&g, expected, &record).await {
        Ok(Some((users, match_id))) => {
            g.finished_at = Some(at);
            g.winner = Some(w);
            g.reason = Some(crate::reason_code(over.reason).to_string());
            g.match_id = Some(match_id);
            tracing::info!(game = g.id, match_id, "correspondence game over: {:?}", over);
            Some(Finished { game: g, users, over, timeout })
        }
        Ok(None) => None,
        Err(e) => {
            tracing::error!(game = g.id, "recording a correspondence game: {e}");
            None
        }
    }
}

/// Tells both players how a game ended, with their new numbers.
async fn announce(server: &Arc<Server>, f: Finished) {
    server.invalidate_leaderboard();
    let names: Vec<String> = f
        .users
        .iter()
        .map(|u| u.as_ref().map(|u| u.username.clone()).unwrap_or_default())
        .collect();
    for (seat, user) in f.users.iter().enumerate() {
        let Some(user) = user else { continue };
        server.with(|l| l.refresh_user(user.clone()));
        server.check_achievements_after(user, f.game.rated && f.over.winner == seat).await;
        let event = CorrEvent::Ended {
            won: f.over.winner == seat,
            reason: f.over.reason,
            timeout: f.timeout,
        };
        tell(server, &user.username, f.game.id, &names[1 - seat], event);
        push_list(server, user.id, &user.username).await;
    }
}
