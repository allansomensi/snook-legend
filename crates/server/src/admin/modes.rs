//! The staff API for 0.12's modes (documented in `docs/ADMIN-API.md`,
//! "Modes"): the doubles games (being played and stored), annulling one,
//! and against the clock: a week's table, a player's attempts, an attempt
//! taken off the ranking.

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::{Value, json};
use snook_legend_proto::inbox::{Action, Kind};
use snook_legend_proto::time_attack::{self, week_of};
use snook_legend_proto::{PerRules, clean_text};

use super::{ApiResult, err, find, log, refresh, require_admin, staff};
use crate::doubles_history::team_name;
use crate::inbox::{self, New};
use crate::store::{DoublesGame, Record, TimeAttempt};
use crate::{Server, now};

pub fn routes() -> Router<Arc<Server>> {
    Router::new()
        .route("/admin/api/doubles", get(doubles_list))
        .route("/admin/api/doubles/{id}", get(doubles_detail))
        .route("/admin/api/doubles/{id}/void", post(doubles_void))
        .route("/admin/api/time-attack", get(time_attack_week))
        .route("/admin/api/users/{name}/time-attack", get(time_attack_attempts))
        .route("/admin/api/time-attack/attempts/{id}/remove", post(time_attack_remove))
}

/// A player's doubles records (the player detail's `doubles_ratings`).
pub(super) fn records_json(r: &PerRules<Record>) -> Value {
    let one = |r: &Record| json!({ "rating": r.rating, "peak_rating": r.peak_rating, "games": r.games, "wins": r.wins });
    json!({ "brazilian": one(&r.brazilian), "official": one(&r.official) })
}

/// A stored doubles game as the staff see it.
fn game_json(g: &DoublesGame) -> Value {
    let r = &g.record;
    json!({
        "id": g.id,
        "room": r.room,
        "teams": g.names,
        "countries": g.countries,
        "player_ids": r.players,
        "winner": r.winner,
        "reason": crate::reason_code(r.reason),
        "rated": r.rated,
        "rules": r.rules,
        "deltas": r.deltas,
        "shots": r.shots,
        "ended_at": r.ended_at,
        "replay": g.has_replay,
        "voided_at": g.voided_at,
    })
}

#[derive(Deserialize)]
struct DoublesQuery {
    /// Only this player's games.
    #[serde(default)]
    player: Option<String>,
    /// Stored games with a smaller id (the previous answer's `next`).
    #[serde(default)]
    before: Option<i64>,
    #[serde(default)]
    limit: Option<u32>,
}

/// The doubles games being played (and just over) in this server, and the
/// stored ones, newest first.
async fn doubles_list(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Query(q): Query<DoublesQuery>,
) -> ApiResult {
    staff(&server, &headers).await?;
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let player = match q.player.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(name) => Some(find(&server, name).await?),
        None => None,
    };
    let mut live = server.with(|l| l.live_doubles());
    if let Some(p) = &player {
        live.retain(|g| g.teams.iter().flatten().any(|n| n.eq_ignore_ascii_case(&p.username)));
    }
    let games = server.store.doubles_games(player.as_ref().map(|p| p.id), q.before, limit).await?;
    let next = (games.len() as u32 == limit).then(|| games.last().map(|g| g.id)).flatten();
    let games: Vec<Value> = games.iter().map(game_json).collect();
    Ok(Json(json!({ "live": live, "games": games, "next": next })))
}

/// A stored doubles game with its replay.
async fn doubles_detail(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    staff(&server, &headers).await?;
    let g = server
        .store
        .doubles_game(id)
        .await?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "No such doubles game."))?;
    let mut v = game_json(&g);
    v["strokes"] = json!(g.record.replay.as_ref().map(|r| r.strokes.len()));
    v["replay_data"] = json!(g.record.replay);
    Ok(Json(json!({ "game": v })))
}

#[derive(Deserialize)]
struct ReasonBody {
    #[serde(default)]
    reason: String,
    #[serde(default)]
    preview: bool,
}

/// Annuls a rated doubles game (administrators): the four rating changes
/// are given back, each player is told (with a button to watch it).
async fn doubles_void(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<ReasonBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    let reason = clean_text(&body.reason, 300);
    if reason.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "Give a reason."));
    }
    let g = server
        .store
        .doubles_game(id)
        .await?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "No such doubles game."))?;
    if !g.record.rated {
        return Err(err(StatusCode::BAD_REQUEST, "Only a rated game changed ratings."));
    }
    if g.voided_at.is_some() {
        return Err(err(StatusCode::CONFLICT, "That game was called off already."));
    }
    // (The doubles game's id made negative: the replay button finds it.)
    let tell = |team: usize, place: usize| {
        let kind = Kind::MatchVoided {
            match_id: -id,
            opponent: team_name(&g.names[1 - team]),
            delta: g.record.deltas[team][place],
        };
        New::game(kind)
            .text(&reason)
            .from(&s.user.username)
            .action(Action::Replay { match_id: -id })
    };
    if body.preview {
        let mut v = inbox::preview(&tell(0, 0));
        v["players"] =
            json!((0..4).map(|n| inbox::preview(&tell(n / 2, n % 2))).collect::<Vec<_>>());
        return Ok(Json(v));
    }
    if !server.store.void_doubles(id, now()).await? {
        return Err(err(StatusCode::CONFLICT, "That game was called off already."));
    }
    for (team, players) in g.record.players.iter().enumerate() {
        for (place, &user_id) in players.iter().enumerate() {
            let Ok(Some(user)) = server.store.user_by_id(user_id).await else { continue };
            // The lounge's copy of their doubles records, as stored now.
            if let Ok(records) = server.store.doubles_records(user_id).await {
                server.with(|l| l.set_doubles_records(&user.username, records));
            }
            refresh(&server, &user).await;
            inbox::send(&server, user_id, tell(team, place)).await;
            let delta = g.record.deltas[team][place];
            let details = format!("doubles #{id} ({:+} given back): {reason}", -delta);
            log(&server, &s, "doubles-void", &user.username, &details).await;
        }
    }
    let g = server.store.doubles_game(id).await?.map(|g| game_json(&g));
    Ok(Json(json!({ "ok": true, "game": g })))
}

#[derive(Deserialize)]
struct WeekQuery {
    /// The week's number (`time_attack::week_of`); none: this week.
    #[serde(default)]
    week: Option<i64>,
    #[serde(default)]
    limit: Option<u32>,
}

/// A week's table against the clock (this week's by default).
async fn time_attack_week(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Query(q): Query<WeekQuery>,
) -> ApiResult {
    staff(&server, &headers).await?;
    let this_week = week_of(now());
    let week = q.week.unwrap_or(this_week);
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let (board, players) = server.store.time_attack_board(week, limit).await?;
    let rows: Vec<Value> = board
        .iter()
        .enumerate()
        .map(|(k, r)| {
            json!({ "place": k + 1, "user_id": r.user_id, "username": r.username,
                    "country": r.country, "ms": r.ms, "time": time_attack::clock(r.ms), "at": r.at })
        })
        .collect();
    Ok(Json(json!({
        "week": week,
        "this_week": this_week,
        "starts_at": time_attack::week_start(week),
        "ends_at": time_attack::week_start(week + 1),
        "players": players,
        "prizes": crate::time_attack::prizes(&server),
        "board": rows,
    })))
}

fn attempt_json(a: &TimeAttempt) -> Value {
    json!({
        "id": a.id, "username": a.username, "day": a.day, "started_ms": a.started_ms,
        "finished_ms": a.finished_ms, "ms": a.ms, "time": a.ms.map(time_attack::clock),
        "shots": a.shots, "fouls": a.fouls, "week": a.week, "at": a.at,
        "counted": a.week.is_some() && a.removed_at.is_none(),
        "removed_at": a.removed_at, "removed_reason": a.removed_reason,
    })
}

#[derive(Deserialize)]
struct LimitQuery {
    #[serde(default)]
    limit: Option<u32>,
}

/// A player's attempts against the clock, newest first.
async fn time_attack_attempts(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Query(q): Query<LimitQuery>,
) -> ApiResult {
    staff(&server, &headers).await?;
    let u = find(&server, &name).await?;
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let list = server.store.time_attack_user_attempts(u.id, limit).await?;
    let week = week_of(now());
    let best = server.store.time_attack_best(u.id, week).await?;
    Ok(Json(json!({
        "username": u.username,
        "this_week": { "week": week, "best_ms": best.map(|b| b.0), "place": best.and_then(|b| b.1) },
        "attempts": list.iter().map(attempt_json).collect::<Vec<_>>(),
    })))
}

/// Takes an attempt off the ranking (administrators: cheating): the
/// player's best of its week is worked out again from the others.
async fn time_attack_remove(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<ReasonBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    let reason = clean_text(&body.reason, 300);
    if reason.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "Give a reason."));
    }
    let a = server.store.time_attack_remove(id, &reason, now()).await?.ok_or_else(|| {
        err(StatusCode::CONFLICT, "No such attempt in a ranking (not counted, or taken off).")
    })?;
    let details = format!(
        "attempt #{id} ({}, week {}): {reason}",
        a.ms.map(time_attack::clock).unwrap_or_default(),
        a.week.unwrap_or_default()
    );
    log(&server, &s, "time-attack-remove", &a.username, &details).await;
    let best = match a.week {
        Some(w) => server.store.time_attack_best(a.user_id, w).await?.map(|b| b.0),
        None => None,
    };
    Ok(Json(json!({ "ok": true, "attempt": attempt_json(&a), "best_ms": best })))
}
