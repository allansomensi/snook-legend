//! The staff API's inbox and moderation (0.12): messages to a player with
//! a read receipt, ratings put right, presents to many players at once,
//! games ended or called off, a player's timeline, a game shot by shot,
//! the banned words and the line pinned in the lounge's chat. Documented
//! in `docs/ADMIN-API.md` ("Inbox & moderation").
//!
//! Whatever notifies a player takes `"preview": true`: nothing is done,
//! and the answer is the notification the player would get, worded in
//! English, Portuguese and Spanish (see [`crate::inbox::preview`]).

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::{Value, json};
use snook_legend_proto::inbox::{Action, Kind};
use snook_legend_proto::{Role, Rules, clean_text};

use super::{ApiResult, err, find, log, private_audit, refresh, require_admin, staff};
use crate::inbox::{self, New};
use crate::store::User;
use crate::{Server, now};

pub fn routes() -> Router<Arc<Server>> {
    Router::new()
        .route("/admin/api/users/{name}/message", post(message))
        .route("/admin/api/users/{name}/notifications", get(notifications))
        .route("/admin/api/users/{name}/rating", post(adjust_rating))
        .route("/admin/api/users/{name}/timeline", get(timeline))
        .route("/admin/api/gifts/batch", post(gift_batch))
        .route("/admin/api/matches/{id}", get(match_detail))
        .route("/admin/api/matches/{id}/void", post(void_match))
        .route("/admin/api/live/{id}/end", post(end_live))
        .route("/admin/api/words", get(words).post(add_word))
        .route("/admin/api/words/remove", post(remove_word))
        .route("/admin/api/words/preview", post(preview_word))
        .route("/admin/api/chat/pinned", get(pinned))
        .route("/admin/api/chat/{id}/pin", post(pin))
        .route("/admin/api/chat/unpin", post(unpin))
}

/// A notification sent to a player, as the staff see it.
pub(super) fn notification_json(row: &crate::store::NotificationRow) -> Value {
    let n = inbox::notification(row);
    let (title, body) = inbox::wording(&n, "en");
    json!({
        "id": n.id, "kind": n.kind, "category": n.category(), "text": n.text, "from": n.from,
        "created_at": n.created_at, "read_at": row.read_at, "important": n.important,
        "pinned_until": n.pinned_until, "action": n.action, "deleted_at": row.deleted_at,
        "title": title, "body": body,
    })
}

/// Sends `n` to `target` (or, `preview`, only shows it): the answer of an
/// action that notifies a player.
async fn notify_or_preview(
    server: &Server,
    target: &User,
    n: New,
    preview: bool,
) -> Result<Option<i64>, Value> {
    if preview {
        return Err(inbox::preview(&n));
    }
    Ok(inbox::send(server, target.id, n).await)
}

/// The moderator may act on this player (not on staff, unless an
/// administrator; never on themselves).
fn may_act(s: &super::Staff, target: &User) -> Result<(), super::ApiError> {
    if target.id == s.user.id {
        return Err(err(StatusCode::BAD_REQUEST, "You can't do that to yourself."));
    }
    if !crate::social::may_moderate(s.user.role, target.role) {
        return Err(err(StatusCode::FORBIDDEN, "Only administrators can act on staff."));
    }
    Ok(())
}

#[derive(Deserialize)]
struct MessageBody {
    text: String,
    /// The player confirms they read it (the portal shows when).
    #[serde(default)]
    important: bool,
    /// Pinned at the top of their inbox for this many days (0: not pinned).
    #[serde(default)]
    pinned_days: u32,
    #[serde(default)]
    preview: bool,
}

/// A message from the staff to one player, in their inbox.
async fn message(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(body): Json<MessageBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    let target = find(&server, &name).await?;
    may_act(&s, &target)?;
    let text = clean_text(&body.text, snook_legend_proto::inbox::MAX_TEXT);
    if text.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "Write the message."));
    }
    let mut n = New::game(Kind::StaffMessage).text(&text).from(&s.user.username);
    if body.important {
        n = n.important();
    }
    if body.pinned_days > 0 {
        n = n.pinned_until(now() + body.pinned_days.min(366) as i64 * 86_400);
    }
    let id = match notify_or_preview(&server, &target, n, body.preview).await {
        Err(preview) => return Ok(Json(preview)),
        Ok(id) => id.ok_or_else(|| err(StatusCode::INTERNAL_SERVER_ERROR, "Database error."))?,
    };
    let what = if body.important { "important" } else { "message" };
    log(&server, &s, "message", &target.username, &format!("{what} #{id}: {text}")).await;
    Ok(Json(json!({ "ok": true, "id": id })))
}

#[derive(Deserialize)]
struct LimitQuery {
    #[serde(default)]
    limit: Option<u32>,
}

/// What was sent to a player, with when each was read (or taken out).
async fn notifications(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Query(q): Query<LimitQuery>,
) -> ApiResult {
    staff(&server, &headers).await?;
    let target = find(&server, &name).await?;
    let rows = server.store.notifications_sent(target.id, q.limit.unwrap_or(100).min(500)).await?;
    let list: Vec<Value> = rows.iter().map(notification_json).collect();
    Ok(Json(json!({ "notifications": list })))
}

#[derive(Deserialize)]
struct RatingBody {
    rules: Rules,
    /// Points to add (negative: take); or `rating`, the rating to put.
    #[serde(default)]
    delta: Option<i32>,
    #[serde(default)]
    rating: Option<i32>,
    reason: String,
    /// The game it is about (the player gets a button to watch it).
    #[serde(default)]
    match_id: Option<i64>,
    #[serde(default)]
    preview: bool,
}

/// The lowest and highest rating the staff may put.
const RATINGS: std::ops::RangeInclusive<i32> = 100..=4000;

/// Puts a player's rating right (administrators), with the reason in their
/// inbox and rating history.
async fn adjust_rating(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(body): Json<RatingBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    let target = find(&server, &name).await?;
    if target.id == s.user.id {
        return Err(err(StatusCode::BAD_REQUEST, "You can't do that to yourself."));
    }
    let reason = clean_text(&body.reason, 300);
    if reason.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "Give a reason."));
    }
    let now_rating = target.records.get(body.rules).rating;
    let delta = match (body.delta, body.rating) {
        (Some(d), None) => d,
        (None, Some(r)) => r - now_rating,
        _ => return Err(err(StatusCode::BAD_REQUEST, "Give either a delta or a rating.")),
    };
    if delta == 0 {
        return Err(err(StatusCode::BAD_REQUEST, "That is their rating already."));
    }
    if !RATINGS.contains(&(now_rating + delta)) {
        return Err(err(StatusCode::BAD_REQUEST, "Ratings stay between 100 and 4000."));
    }
    if let Some(id) = body.match_id
        && server.store.match_row(id).await?.is_none_or(|m| !m.players.contains(&target.id))
    {
        return Err(err(StatusCode::BAD_REQUEST, "That game isn't one of theirs."));
    }
    let match_id = body.match_id.unwrap_or(0);
    let kind =
        Kind::RatingAdjusted { rules: body.rules, delta, rating: now_rating + delta, match_id };
    let mut n = New::game(kind).text(&reason).from(&s.user.username).important();
    if match_id > 0 {
        n = n.action(Action::Replay { match_id });
    }
    if body.preview {
        return Ok(Json(inbox::preview(&n)));
    }
    let fresh = server
        .store
        .adjust_rating(target.id, body.rules, delta, &reason, body.match_id, s.user.id, now())
        .await?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "No such user."))?;
    refresh(&server, &target).await;
    server.invalidate_leaderboard();
    // What the player was told, with the rating as it really is now.
    if let Kind::RatingAdjusted { rating, .. } = &mut n.kind {
        *rating = fresh.records.get(body.rules).rating;
    }
    let id = inbox::send(&server, target.id, n).await;
    let details = format!("{} {delta:+} → {}: {reason}", body.rules.code(), now_rating + delta);
    log(&server, &s, "rating", &target.username, &details).await;
    Ok(Json(json!({
        "ok": true, "notification": id, "user": super::user_json(&fresh, false),
    })))
}

#[derive(Deserialize)]
struct TimelineQuery {
    /// Only what happened before this time (Unix seconds): the next page.
    #[serde(default)]
    before: Option<i64>,
    #[serde(default)]
    limit: Option<u32>,
}

/// Rows read from each source of a timeline (enough for several pages).
const TIMELINE_SCAN: u32 = 500;

/// A player's history in order, newest first: the account and the others
/// made from its address, games, purchases, reports sent and received,
/// the staff's actions, changes of name and country, notifications.
async fn timeline(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Query(q): Query<TimelineQuery>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    let admin = s.user.role == Role::Admin;
    let u = find(&server, &name).await?;
    let limit = q.limit.unwrap_or(100).clamp(1, 500) as usize;
    let before = q.before.unwrap_or(i64::MAX);
    let (games, payments, ledger, sent, received) = tokio::join!(
        server.store.history(u.id, TIMELINE_SCAN),
        server.store.payments_of(u.id, TIMELINE_SCAN),
        server.store.ledger(u.id, TIMELINE_SCAN),
        server.store.reports_by(u.id, TIMELINE_SCAN),
        server.store.reports_about(u.id, TIMELINE_SCAN),
    );
    let (changes, notes, linked) = tokio::join!(
        server.store.profile_changes(u.id, TIMELINE_SCAN),
        server.store.notifications_sent(u.id, TIMELINE_SCAN),
        server.store.linked_accounts(u.id),
    );
    let search =
        crate::store::AuditSearch { target: Some(u.username.clone()), ..Default::default() };
    let audit = server.store.audit_search(&search, TIMELINE_SCAN).await?;
    let mut items: Vec<(i64, Value)> = vec![(
        u.created_at,
        json!({ "type": "account", "at": u.created_at, "username": u.username, "country": u.country }),
    )];
    for a in linked? {
        items.push((
            a.created_at,
            json!({ "type": "linked_account", "at": a.created_at, "username": a.username }),
        ));
    }
    for h in games? {
        items.push((
            h.at,
            json!({
                "type": "match", "at": h.at, "match_id": h.match_id, "opponent": h.opponent,
                "won": h.won, "rated": h.rated, "rules": h.rules, "delta": h.delta,
                "reason": h.reason, "correspondence": h.correspondence,
            }),
        ));
    }
    if admin {
        for p in payments? {
            items.push((
                p.at,
                json!({
                    "type": "payment", "at": p.at, "kind": p.kind, "coins": p.coins,
                    "amount": p.amount, "currency": p.currency, "refunded": p.refunded,
                    "test": p.test,
                }),
            ));
        }
    }
    for l in ledger?.into_iter().filter(|l| l.reason.starts_with("buy") || l.delta < 0) {
        items.push((
            l.at,
            json!({ "type": "purchase", "at": l.at, "coins": l.delta, "reason": l.reason }),
        ));
    }
    for r in sent? {
        items.push((
            r.created_at,
            json!({ "type": "report_sent", "at": r.created_at, "id": r.id, "target": r.target,
                    "reason": r.reason, "status": r.status }),
        ));
    }
    for r in received? {
        items.push((
            r.created_at,
            json!({ "type": "report_received", "at": r.created_at, "id": r.id,
                    "reporter": r.reporter, "reason": r.reason, "status": r.status }),
        ));
    }
    for e in audit.into_iter().map(|e| private_audit(e, admin)) {
        items.push((
            e.at,
            json!({ "type": "staff_action", "at": e.at, "action": e.action, "staff": e.staff,
                    "details": e.details }),
        ));
    }
    for c in changes? {
        items.push((
            c.at,
            json!({ "type": "profile_change", "at": c.at, "kind": c.kind, "old": c.old,
                    "new": c.new, "by": c.by }),
        ));
    }
    for n in notes? {
        let mut v = notification_json(&n);
        v["type"] = json!("notification");
        v["at"] = json!(n.created_at);
        items.push((n.created_at, v));
    }
    items.retain(|(at, _)| *at < before);
    items.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    let more = items.len() > limit;
    items.truncate(limit);
    let next = more.then(|| items.last().map(|(at, _)| *at)).flatten();
    Ok(Json(json!({
        "items": items.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
        "next": next,
    })))
}

#[derive(Deserialize)]
struct BatchBody {
    /// The usernames (also pasted as one text: commas, spaces, new lines).
    #[serde(default)]
    usernames: Vec<String>,
    #[serde(default)]
    text: String,
    #[serde(default)]
    coins: i64,
    #[serde(default)]
    item: String,
    #[serde(default)]
    vip_days: u32,
    reason: String,
    /// Shown to each player with the present.
    #[serde(default)]
    message: String,
    /// Only check (who is unknown, who has the item already).
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    preview: bool,
}

/// Most players one batch of presents goes to.
pub const MAX_BATCH: usize = 500;

/// Presents from the staff to many players at once (administrators):
/// coins, an item and days of VIP, each in the player's inbox. A dry run
/// first says which names are unknown and who has the item already.
async fn gift_batch(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(body): Json<BatchBody>,
) -> ApiResult {
    use snook_legend_proto::collection::Origin;
    use snook_legend_proto::shop::Price;
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    let bad = |m: &str| err(StatusCode::BAD_REQUEST, m);
    let reason = clean_text(&body.reason, 300);
    if reason.is_empty() {
        return Err(bad("Give a reason."));
    }
    if body.coins < 0 || body.coins > 1_000_000 {
        return Err(bad("Give 0 to 1,000,000 coins."));
    }
    if body.vip_days > 366 {
        return Err(bad("Give 0 to 366 days of VIP."));
    }
    let item = body.item.trim();
    if !item.is_empty() {
        match snook_legend_proto::shop::item(item).map(|i| i.price) {
            None => return Err(bad("No such item.")),
            Some(Price::Coins(_) | Price::Pack | Price::Earned) => {}
            Some(_) => {
                return Err(bad(
                    "That item comes with VIP, an achievement, or to everyone: it can't be given.",
                ));
            }
        }
    }
    if body.coins == 0 && item.is_empty() && body.vip_days == 0 {
        return Err(bad("Give coins, an item or days of VIP."));
    }
    // The names, each once, in the order given.
    let mut seen = BTreeSet::new();
    let names: Vec<String> = body
        .usernames
        .iter()
        .flat_map(|n| n.split([',', ';', ' ', '\n', '\t', '\r']))
        .chain(body.text.split([',', ';', ' ', '\n', '\t', '\r']))
        .map(|n| n.trim().trim_start_matches('@').to_string())
        .filter(|n| !n.is_empty() && seen.insert(n.to_lowercase()))
        .collect();
    if names.is_empty() {
        return Err(bad("List the players."));
    }
    if names.len() > MAX_BATCH {
        return Err(bad("500 players at most at a time."));
    }
    let message = clean_text(&body.message, snook_legend_proto::inbox::MAX_TEXT);
    let kind =
        Kind::StaffGift { coins: body.coins, item: item.to_string(), vip_days: body.vip_days };
    let action =
        if item.is_empty() { Action::None } else { Action::Equip { code: item.to_string() } };
    let n = New::game(kind).text(&message).from(&s.user.username).action(action);
    if body.preview {
        return Ok(Json(inbox::preview(&n)));
    }
    let mut unknown = Vec::new();
    let mut owners = Vec::new();
    let mut players: Vec<User> = Vec::new();
    for name in &names {
        match server.store.user_by_name(name).await? {
            Some(u) if u.deleted_at.is_none() => {
                if !item.is_empty()
                    && server.store.wallet(u.id).await?.owned.iter().any(|c| c == item)
                {
                    owners.push(u.username.clone());
                }
                players.push(u);
            }
            _ => unknown.push(name.clone()),
        }
    }
    if body.dry_run {
        return Ok(Json(json!({
            "dry_run": true, "players": players.len(), "unknown": unknown,
            "already_own": owners, "notification": n.notification(),
        })));
    }
    let t = now();
    let mut given = Vec::new();
    for u in &players {
        let mut parts = Vec::new();
        if body.coins > 0 {
            server.store.credit(u.id, body.coins, &format!("staff: {reason}"), None, t).await?;
            parts.push(format!("{} coins", body.coins));
        }
        // Whoever has the item already gets the rest.
        let mut n = n.clone();
        if !item.is_empty() && !owners.contains(&u.username) {
            server.store.grant_item(u.id, item, "staff", t).await?;
            crate::collection::record(
                &server,
                u.id,
                item,
                Origin::Staff { reason: reason.clone() },
            )
            .await;
            parts.push(item.to_string());
        } else if let Kind::StaffGift { item: i, .. } = &mut n.kind {
            i.clear();
            n.action = Action::None;
        }
        if body.vip_days > 0 {
            server.store.add_vip(u.id, t, body.vip_days as i64 * 86_400).await?;
            parts.push(format!("+{} days of VIP", body.vip_days));
        }
        if parts.is_empty() {
            continue;
        }
        crate::shop::push_wallet(&server, u).await;
        inbox::send(&server, u.id, n).await;
        log(&server, &s, "gift", &u.username, &format!("{}: {reason}", parts.join(", "))).await;
        given.push(u.username.clone());
    }
    let summary = format!(
        "{} players; coins {}, item {}, VIP {} days: {reason}",
        given.len(),
        body.coins,
        if item.is_empty() { "-" } else { item },
        body.vip_days
    );
    log(&server, &s, "gift-batch", "batch", &summary).await;
    Ok(Json(json!({ "ok": true, "given": given, "unknown": unknown, "already_own": owners })))
}

/// A stored game shot by shot, to look into a report: for each stroke who
/// played it, how (power, spin, aim, the cue ball's placement), what it
/// made, its fouls, and how long it was thought over (games from 0.12 on).
async fn match_detail(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    staff(&server, &headers).await?;
    let m = server
        .store
        .match_row(id)
        .await?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "No such match."))?;
    let replay = server.store.replay(id).await?;
    let times = match m.room {
        Some(room) => server.store.shot_times(room).await?,
        None => None,
    };
    let strokes = match replay {
        Some(r) => {
            let replay = r.replay.clone();
            let names = m.names.clone();
            tokio::task::spawn_blocking(move || strokes_json(&replay, &names, times.as_deref()))
                .await
                .map_err(|_| {
                    err(StatusCode::INTERNAL_SERVER_ERROR, "The game could not be rebuilt.")
                })?
        }
        None => Value::Null,
    };
    Ok(Json(json!({ "match": m, "strokes": strokes })))
}

/// Every stroke of `replay` with what it did (see [`match_detail`]).
pub fn strokes_json(
    replay: &snook_legend_proto::replay::Replay,
    names: &[String; 2],
    times: Option<&[Option<u32>]>,
) -> Value {
    use snook_legend_proto::replay::{Stroke, apply};
    let mut game = replay.start();
    let mut out = Vec::with_capacity(replay.strokes.len());
    for (k, stroke) in replay.strokes.iter().enumerate() {
        let seat = game.current_player();
        let think = times.and_then(|t| t.get(k).copied().flatten()).map(|t| t as f64 / 10.0);
        let report = match apply(&mut game, stroke) {
            Ok(r) => r,
            Err(e) => {
                out.push(json!({ "index": k, "error": e }));
                break;
            }
        };
        let mut v = match stroke {
            Stroke::Shot { shot, called, cue_ball, timeout } => json!({
                "kind": "shot",
                "power": shot.speed,
                "aim_degrees": shot.yaw.to_degrees().rem_euclid(360.0),
                "elevation_degrees": shot.elevation.to_degrees(),
                "spin": { "side": shot.tip.x, "top": shot.tip.y },
                "cue_ball": cue_ball.map(|p| [p.x * 1000.0, p.y * 1000.0]),
                "called": called.map(|p| p.name()),
                "timeout": timeout,
            }),
            Stroke::TimeFoul => json!({ "kind": "time_foul" }),
            Stroke::Choice { option } => json!({ "kind": "choice", "option": option }),
            Stroke::Resign { seat } => json!({ "kind": "resign", "seat": seat }),
            Stroke::Abandon { seat } => json!({ "kind": "abandon", "seat": seat }),
        };
        v["index"] = json!(k);
        v["seat"] = json!(seat);
        v["player"] = json!(names[seat % 2]);
        v["think_secs"] = json!(think);
        if let Some(r) = report {
            v["potted"] = json!(
                r.pocketed
                    .iter()
                    .map(|(b, p)| json!({ "ball": b.index(), "pocket": p.name() }))
                    .collect::<Vec<_>>()
            );
            v["fouls"] = json!(r.fouls.iter().map(|f| f.to_string()).collect::<Vec<_>>());
            v["first_contact"] = json!(r.first_contact.map(|b| b.index()));
            v["break"] = json!(r.was_break);
            v["continues"] = json!(r.continues);
        }
        out.push(v);
    }
    Value::Array(out)
}

#[derive(Deserialize)]
struct ReasonBody {
    #[serde(default)]
    reason: String,
    #[serde(default)]
    preview: bool,
}

/// Calls a finished rated game off (administrators): both players' rating
/// changes are given back, and each is told.
async fn void_match(
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
    let m = server
        .store
        .match_row(id)
        .await?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "No such match."))?;
    if !m.rated {
        return Err(err(StatusCode::BAD_REQUEST, "Only a rated game changed ratings."));
    }
    if m.voided_at.is_some() {
        return Err(err(StatusCode::CONFLICT, "That game was called off already."));
    }
    let tell = |seat: usize| {
        let kind = Kind::MatchVoided {
            match_id: id,
            opponent: m.names[1 - seat].clone(),
            delta: m.delta[seat],
        };
        New::game(kind).text(&reason).from(&s.user.username).action(Action::Replay { match_id: id })
    };
    if body.preview {
        let mut v = inbox::preview(&tell(0));
        v["players"] = json!([inbox::preview(&tell(0)), inbox::preview(&tell(1))]);
        return Ok(Json(v));
    }
    let users = server
        .store
        .void_match(id, s.user.id, &reason, now())
        .await?
        .ok_or_else(|| err(StatusCode::CONFLICT, "That game was called off already."))?;
    server.invalidate_leaderboard();
    for (seat, user) in users.iter().enumerate() {
        let Some(user) = user else { continue };
        refresh(&server, user).await;
        inbox::send(&server, user.id, tell(seat)).await;
        let details = format!("#{id} ({:+} given back): {reason}", -m.delta[seat]);
        log(&server, &s, "match-void", &user.username, &details).await;
    }
    Ok(Json(json!({ "ok": true, "match": server.store.match_row(id).await? })))
}

#[derive(Deserialize)]
struct EndBody {
    /// The seat (0 or 1) given the game; none: ended without a result.
    #[serde(default)]
    winner: Option<usize>,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    preview: bool,
}

/// Ends a stuck game being played (administrators): given to one player
/// (the other resigns it) or without a result (nothing recorded).
async fn end_live(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<u64>,
    Json(body): Json<EndBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    let reason = clean_text(&body.reason, 300);
    if reason.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "Give a reason."));
    }
    let (players, _, finished, _) = server
        .with(|l| l.room_replay(id))
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "That game is no longer on this server."))?;
    if finished {
        return Err(err(StatusCode::BAD_REQUEST, "That game is over already."));
    }
    let tell = |seat: usize, names: &[String; 2]| {
        let kind = Kind::MatchEnded {
            match_id: 0,
            opponent: names[1 - seat].clone(),
            won: body.winner.map(|w| w == seat),
        };
        New::game(kind).text(&reason).from(&s.user.username)
    };
    if body.preview {
        let mut v = inbox::preview(&tell(0, &players));
        v["players"] =
            json!([inbox::preview(&tell(0, &players)), inbox::preview(&tell(1, &players))]);
        return Ok(Json(v));
    }
    let ended = server
        .with(|l| l.staff_end_match(id, body.winner))
        .map_err(|why| err(StatusCode::BAD_REQUEST, why))?;
    let result = match body.winner {
        Some(w) => format!("given to {}", ended.names[w % 2]),
        None => "no result".to_string(),
    };
    for seat in 0..2 {
        if ended.guests[seat] {
            continue;
        }
        inbox::send(&server, ended.ids[seat], tell(seat, &ended.names)).await;
        let details = format!("live game {id}, {result}: {reason}");
        log(&server, &s, "match-end", &ended.names[seat], &details).await;
    }
    Ok(Json(json!({ "ok": true })))
}

/// The banned words, by language.
async fn words(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    let words = server.store.banned_words().await?;
    Ok(Json(json!({ "words": words, "max_word": crate::wordfilter::MAX_WORD })))
}

#[derive(Deserialize)]
struct WordBody {
    word: String,
    /// A language code (`pt`, `en`, `es`, `pt-BR`…) or `*` for all.
    lang: String,
}

fn word_of(body: &WordBody) -> Result<(String, String), super::ApiError> {
    let word = clean_text(&body.word, crate::wordfilter::MAX_WORD).to_lowercase();
    let lang = body.lang.trim().to_string();
    if crate::wordfilter::tokens(&word).is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "Write the word."));
    }
    if !crate::wordfilter::valid_lang(&lang) {
        return Err(err(StatusCode::BAD_REQUEST, "Give a language code (pt, en, es…) or *."));
    }
    Ok((word, lang))
}

async fn add_word(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(body): Json<WordBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    let (word, lang) = word_of(&body)?;
    if !server.store.add_banned_word(&word, &lang, s.user.id, now()).await? {
        return Err(err(StatusCode::CONFLICT, "That word is on the list already."));
    }
    crate::moderation::reload_words(&server).await;
    log(&server, &s, "banned-word", &lang, &word).await;
    Ok(Json(json!({ "ok": true })))
}

async fn remove_word(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(body): Json<WordBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    let (word, lang) = word_of(&body)?;
    if !server.store.remove_banned_word(&word, &lang).await? {
        return Err(err(StatusCode::NOT_FOUND, "That word isn't on the list."));
    }
    crate::moderation::reload_words(&server).await;
    log(&server, &s, "banned-word-remove", &lang, &word).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct PreviewBody {
    text: String,
}

/// Would this text be turned away, and by which word?
async fn preview_word(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(body): Json<PreviewBody>,
) -> ApiResult {
    staff(&server, &headers).await?;
    let text = clean_text(&body.text, 2000);
    let hit = server.with(|l| l.banned_word(&text));
    Ok(Json(json!({
        "blocked": hit.is_some(),
        "word": hit.as_ref().map(|h| h.word.clone()),
        "lang": hit.as_ref().map(|h| h.lang.clone()),
        "tokens": crate::wordfilter::tokens(&text),
    })))
}

/// The line pinned at the top of the lounge's chat.
async fn pinned(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    Ok(Json(json!({ "line": server.with(|l| l.pinned_chat()) })))
}

async fn pin(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<u64>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    let line = server
        .with(|l| l.pin_chat(id))
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "No such line (or it was deleted)."))?;
    log(&server, &s, "chat-pin", "lounge", &format!("{}: {}", line.from, line.text)).await;
    Ok(Json(json!({ "ok": true, "line": line })))
}

async fn unpin(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    let s = staff(&server, &headers).await?;
    if !server.with(|l| l.unpin_chat()) {
        return Err(err(StatusCode::BAD_REQUEST, "No line is pinned."));
    }
    log(&server, &s, "chat-unpin", "lounge", "").await;
    Ok(Json(json!({ "ok": true })))
}

/// The players who reported `target` (open reports, and report `id`
/// itself), each told that the staff acted (never how). Not the target, nor
/// the staff member who acted.
pub(super) async fn thank_reporters(server: &Server, report: &crate::store::ReportRow, by: i64) {
    let mut told = BTreeSet::new();
    let others = server.store.reports_about(report.target_id, 200).await.unwrap_or_default();
    let reporters = std::iter::once(report.reporter_id)
        .chain(others.iter().filter(|r| r.status == 0).map(|r| r.reporter_id));
    for reporter in reporters {
        if reporter == report.target_id || reporter == by || !told.insert(reporter) {
            continue;
        }
        let kind = Kind::ReportActed { username: report.target.clone() };
        inbox::send(server, reporter, New::game(kind)).await;
    }
}
