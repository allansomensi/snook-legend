//! The public API the website reads (`docs/ADMIN-API.md`, "Public API"):
//! a player's page, the official tournaments and the supporters' credits.
//! Open to any origin (CORS) like `/api/leaderboard`, cached a minute;
//! times are Unix seconds. A player who doesn't exist, was banned or
//! deleted (or a name they left) is not found.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::{Value, json};
use snook_legend_proto::shop::{self, Category};
use snook_legend_proto::{ACHIEVEMENTS, Role, Rules, Tier, TrophyKind};

use crate::store::User;
use crate::{Server, now};

pub fn router() -> Router<Arc<Server>> {
    Router::new()
        .route("/api/players/{name}", get(player))
        .route("/api/tournaments", get(tournaments))
        .route("/api/tournaments/{id}", get(tournament))
        .route("/api/supporters", get(supporters))
}

/// JSON anyone may read, cached a minute.
fn public(status: StatusCode, value: Value) -> Response {
    let mut res = (status, axum::Json(value)).into_response();
    let h = res.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=60"));
    res
}

fn not_found() -> Response {
    public(StatusCode::NOT_FOUND, json!({ "error": "not found" }))
}

fn unavailable() -> Response {
    let mut res = (StatusCode::SERVICE_UNAVAILABLE, axum::Json(json!({ "error": "try again" })))
        .into_response();
    res.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    res
}

/// The medal of an achievement's level.
pub fn medal(level: u8) -> &'static str {
    match level {
        0 => "bronze",
        1 => "silver",
        2 => "gold",
        3 => "diamond",
        _ => "ruby",
    }
}

fn role_code(role: Role) -> &'static str {
    match role {
        Role::Player => "player",
        Role::Moderator => "moderator",
        Role::Admin => "admin",
    }
}

fn tier_code(tier: Tier) -> Value {
    serde_json::to_value(tier).unwrap_or(Value::Null)
}

/// The account behind a name, if it may be shown.
async fn shown_user(server: &Server, name: &str) -> Result<Option<User>, ()> {
    let name = name.trim();
    if !snook_legend_proto::valid_username(name) {
        return Ok(None);
    }
    let user = server.store.user_by_name(name).await.map_err(|_| ())?;
    Ok(user.filter(|u| {
        u.deleted_at.is_none()
            && !u.is_banned(now())
            // The name it has now, not one it left (the lookup is by the
            // name now; this keeps it so).
            && u.username.eq_ignore_ascii_case(name)
    }))
}

/// `GET /api/players/{name}`.
async fn player(State(server): State<Arc<Server>>, Path(name): Path<String>) -> Response {
    let user = match shown_user(&server, &name).await {
        Ok(Some(u)) => u,
        Ok(None) => return not_found(),
        Err(()) => return unavailable(),
    };
    let t = now();
    let (achievements, wallet, history, brazilian_rank, official_rank) = tokio::join!(
        server.store.achievements(user.id),
        server.store.wallet(user.id),
        server.store.history(user.id, 10),
        server.store.rank_of(user.id, Rules::Brazilian),
        server.store.rank_of(user.id, Rules::Official),
    );
    let (trophies, won, in_credits) = tokio::join!(
        crate::trophies_of(&server, user.id),
        server.store.officials_won(user.id),
        server.store.in_credits(user.id),
    );
    let achievements = achievements.unwrap_or_default();
    let wallet = wallet.unwrap_or_default();
    let codes: Vec<String> = achievements.iter().map(|(c, _)| c.clone()).collect();
    let frame = crate::shop::effective(&wallet, &codes, Category::Frame, t);
    let cue = crate::shop::effective(&wallet, &codes, Category::Cue, t);
    let ranks = [brazilian_rank.ok().flatten(), official_rank.ok().flatten()];
    let ratings: Vec<Value> = [Rules::Brazilian, Rules::Official]
        .iter()
        .zip(ranks)
        .map(|(&rules, rank)| {
            let r = user.records.get(rules);
            json!({
                "rules": rules.code(), "rating": r.rating, "peak": r.peak_rating,
                "tier": tier_code(Tier::of(r.rating)), "games": r.games, "wins": r.wins,
                "rank": if r.games > 0 { rank } else { None },
            })
        })
        .collect();
    let achievements: Vec<Value> = achievements
        .iter()
        .filter_map(|(code, at)| {
            let a = ACHIEVEMENTS.iter().find(|a| a.code == code)?;
            Some(json!({
                "code": a.code, "name": a.title, "description": a.description,
                "unlocked_at": at, "medal": medal(a.level), "secret": false,
            }))
        })
        .collect();
    let won = won.unwrap_or_default();
    let trophies: Vec<Value> = trophies
        .iter()
        .map(|tr| {
            let kind = serde_json::to_value(tr.kind).unwrap_or(Value::Null);
            // The official tournament it was (the same name, ended then).
            let id = (tr.kind == TrophyKind::Tournament)
                .then(|| {
                    won.iter()
                        .find(|(_, name, at)| *name == tr.name && (at - tr.at).abs() <= 600)
                        .map(|(id, _, _)| *id)
                })
                .flatten();
            json!({
                "kind": kind, "name": tr.name, "players": tr.players, "at": tr.at,
                "tournament_id": id,
            })
        })
        .collect();
    let recent: Vec<Value> = history
        .unwrap_or_default()
        .into_iter()
        .map(|h| {
            json!({
                "id": h.match_id, "at": h.at, "rules": h.rules.code(), "rated": h.rated,
                "opponent": h.opponent, "opponent_country": h.opponent_country, "won": h.won,
                "delta": h.delta,
                // As the game's protocol writes it.
                "reason": serde_json::to_value(crate::parse_reason(&h.reason)).unwrap_or(Value::Null),
            })
        })
        .collect();
    let online = server.with(|l| l.is_online(&user.username));
    let last_seen_days = if online { 0 } else { ((t - user.last_seen).max(0) / 86_400) as u32 };
    let stats = user.stats(t);
    let value = json!({
        "username": user.username,
        "country": user.country,
        "avatar": snook_legend_proto::shown_avatar(&user.avatar),
        "vip": crate::shop::is_vip(&wallet, t),
        "supporter": in_credits.unwrap_or(false) && wallet.vip_until > 0,
        "role": role_code(user.role),
        "created_at": user.created_at,
        "last_seen_days": last_seen_days,
        "level": snook_legend_proto::xp::level_of(user.xp),
        "frame": frame,
        "cue": cue,
        "cue_rarity": shop::item(cue).map(|i| shop::rarity(i).code()),
        "ratings": ratings,
        "achievements": achievements,
        "trophies": trophies,
        "recent": recent,
        "career": {
            "games": stats.games + stats.casual_games,
            "wins": stats.wins,
            "break_and_runs": stats.runouts,
            "best_streak": stats.best_streak,
        },
    });
    public(StatusCode::OK, value)
}

#[derive(serde::Deserialize)]
struct ListQuery {
    #[serde(default)]
    limit: Option<u32>,
}

/// Current names of accounts, by id (a deleted one keeps its anonymous
/// name; one gone entirely is left out).
async fn names(server: &Server, ids: impl IntoIterator<Item = i64>) -> HashMap<i64, String> {
    let mut out = HashMap::new();
    for id in ids {
        if out.contains_key(&id) {
            continue;
        }
        if let Ok(Some(u)) = server.store.user_by_id(id).await {
            out.insert(id, u.username);
        }
    }
    out
}

/// `GET /api/tournaments?limit=50`: the official tournaments, newest first.
async fn tournaments(State(server): State<Arc<Server>>, Query(q): Query<ListQuery>) -> Response {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let Ok(list) = server.store.officials(limit).await else { return unavailable() };
    let ids = list.iter().flat_map(|(_, o)| [o.champion_id, o.runner_up_id]).flatten();
    let names = names(&server, ids.collect::<Vec<_>>()).await;
    let name = |id: Option<i64>| id.and_then(|id| names.get(&id).cloned());
    let out: Vec<Value> = list
        .iter()
        .map(|(id, o)| {
            json!({
                "id": id, "name": o.name, "rules": o.rules.code(), "format": o.format,
                "size": o.size, "players": o.players, "state": "finished",
                "starts_at": o.starts_at, "ends_at": o.ends_at,
                "champion": name(o.champion_id), "runner_up": name(o.runner_up_id),
                "prize_pool": o.prize_pool, "entry_fee": o.entry_fee, "rated": o.rated,
            })
        })
        .collect();
    public(StatusCode::OK, Value::Array(out))
}

/// `GET /api/tournaments/{id}`: one with its bracket or standings.
async fn tournament(State(server): State<Arc<Server>>, Path(id): Path<String>) -> Response {
    let Ok(id) = id.parse::<i64>() else { return not_found() };
    let o = match server.store.official(id).await {
        Ok(Some((_, o))) => o,
        Ok(None) => return not_found(),
        Err(_) => return unavailable(),
    };
    let detail = crate::official::detail_with_names(&server, &o.detail).await;
    let known: HashMap<i64, String> = detail["names"]
        .as_object()
        .map(|m| {
            m.iter().filter_map(|(k, v)| Some((k.parse().ok()?, v.as_str()?.to_string()))).collect()
        })
        .unwrap_or_default();
    let name = |id: &Value| id.as_i64().and_then(|id| known.get(&id).cloned());
    let players: Vec<Value> = detail["players"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|p| json!({ "username": p["username"], "country": p["country"], "seed": p["seed"] }))
                .collect()
        })
        .unwrap_or_default();
    let rounds: Vec<Value> = detail["rounds"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|round| {
                    Value::Array(
                        round
                            .as_array()
                            .map(|r| {
                                r.iter()
                                    .map(|m| {
                                        json!({
                                            "a": name(&m["a"]), "b": name(&m["b"]),
                                            "winner": name(&m["winner"]),
                                            "match_id": Value::Null, "score": "",
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let standings: Vec<Value> = detail["standings"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|s| {
                    json!({
                        "username": s["username"], "points": s["points"], "games": s["games"],
                        "streak": s["streak"],
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let champion = o.champion_id.and_then(|i| known.get(&i).cloned());
    let runner_up = o.runner_up_id.and_then(|i| known.get(&i).cloned());
    let (champion, runner_up) = if champion.is_none() || runner_up.is_none() {
        let more = names(&server, [o.champion_id, o.runner_up_id].into_iter().flatten()).await;
        (
            champion.or_else(|| o.champion_id.and_then(|i| more.get(&i).cloned())),
            runner_up.or_else(|| o.runner_up_id.and_then(|i| more.get(&i).cloned())),
        )
    } else {
        (champion, runner_up)
    };
    let value = json!({
        "id": id, "name": o.name, "rules": o.rules.code(), "format": o.format, "size": o.size,
        "state": "finished", "starts_at": o.starts_at, "ends_at": o.ends_at, "rated": o.rated,
        "entry_fee": o.entry_fee, "prize_pool": o.prize_pool, "prizes": o.prizes,
        "item_prize": if o.item_prize.is_empty() { Value::Null } else { json!(o.item_prize) },
        "champion": champion, "runner_up": runner_up,
        "players": players, "rounds": rounds, "standings": standings,
    });
    public(StatusCode::OK, value)
}

/// `GET /api/supporters`.
async fn supporters(State(server): State<Arc<Server>>) -> Response {
    let list = crate::ops::supporters(&server).await;
    public(StatusCode::OK, serde_json::to_value(list).unwrap_or_default())
}
