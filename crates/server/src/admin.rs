//! The staff API under `/admin/api`, for the staff portal: the `/admin` pages
//! of the website (Next.js, the website's repository, at
//! `snooklegend.app/admin`), which call it from the browser. Only the
//! website's origin may (`SNOOK_LEGEND_ADMIN_ORIGIN`, checked by CORS).
//!
//! Staff are ordinary accounts with the moderator or administrator role
//! (the first administrators come from `SNOOK_LEGEND_ADMINS`). They sign in with
//! their game password and get a separate, short-lived admin session sent
//! as a bearer token (never a cookie, so there is no CSRF surface). Every
//! action is written to the audit log.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::{Value, json};
use snook_legend_proto::{
    MAX_PASSWORD_BYTES, Role, Rules, Tier, clean_text, valid_password, valid_username,
};

use crate::store::{SessionKind, User};
use crate::{Server, new_token, now, token_hash};

mod collection;
mod moderation;
mod modes;
mod ops;

const ADMIN_SESSION: i64 = 8 * 60 * 60;
pub fn router(server: Arc<Server>) -> Router<Arc<Server>> {
    Router::new()
        .route("/admin/api/login", post(login))
        .route("/admin/api/logout", post(logout))
        .route("/admin/api/me", get(me))
        .route("/admin/api/overview", get(overview))
        .route("/admin/api/live", get(live))
        .route("/admin/api/users", get(users))
        .route("/admin/api/users/{name}", get(user_detail))
        .route("/admin/api/users/{name}/{action}", post(user_action))
        .route("/admin/api/users/{name}/collection", get(collection::collection))
        .route("/admin/api/reports", get(reports))
        .route("/admin/api/reports/{id}", post(resolve_report))
        .route("/admin/api/audit", get(audit))
        .route("/admin/api/metrics", get(metrics))
        .route("/admin/api/aim", get(aim_review))
        .route("/admin/api/replay/{id}", get(replay))
        .route("/admin/api/replay/{id}/view", get(replay_view))
        .route("/admin/api/live/{id}/view", get(live_view))
        .route("/admin/api/announce", post(announce))
        .route("/admin/api/motd", post(motd))
        .route("/admin/api/chat/clear", post(clear_chat))
        .route("/admin/api/board", get(board).post(pin_note))
        .route("/admin/api/payments/{id}/test", post(payment_test))
        .route("/admin/api/msstore/{tracking}/retry", post(msstore_retry))
        .route("/admin/api/board/{id}", axum::routing::delete(unpin_note))
        .route("/admin/api/seeks", get(seeks))
        .route("/admin/api/seeks/{name}", axum::routing::delete(remove_seek))
        .route("/admin/api/notes/{id}", axum::routing::delete(remove_note))
        .route("/admin/api/chat", get(chat))
        .route("/admin/api/chat/{id}/delete", post(delete_chat))
        .route("/admin/api/tournaments/{id}/cancel", post(cancel_tournament))
        .route("/admin/api/economy", get(economy))
        .route("/admin/api/system", get(system))
        .route("/admin/api/events", get(list_events).post(create_event))
        .route("/admin/api/events/{id}/cancel", post(cancel_event))
        .merge(moderation::routes())
        .merge(modes::routes())
        .merge(ops::router())
        // The panel sends small JSON bodies (the longest a note on the
        // board); anyone can post to the sign-in.
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(middleware::from_fn_with_state(server, cors))
        .layer(middleware::from_fn(security_headers))
}

/// The largest request body the panel's API reads, in bytes.
const MAX_BODY: usize = 16 * 1024;

/// The staff portal calls the API from its own origin: the browser asks
/// first (a preflight `OPTIONS`) and is answered only for that origin. The
/// session is a bearer token, never a cookie, so no credentials are allowed.
async fn cors(State(server): State<Arc<Server>>, req: Request, next: Next) -> Response {
    let allowed = server.config.admin_origin.as_deref().and_then(|o| HeaderValue::from_str(o).ok());
    let from_portal = match (&allowed, req.headers().get(header::ORIGIN)) {
        (Some(a), Some(o)) => a.as_bytes().eq_ignore_ascii_case(o.as_bytes()),
        _ => false,
    };
    let preflight = req.method() == axum::http::Method::OPTIONS;
    let mut res = if preflight {
        if from_portal { StatusCode::NO_CONTENT } else { StatusCode::FORBIDDEN }.into_response()
    } else {
        next.run(req).await
    };
    let h = res.headers_mut();
    h.insert(header::VARY, HeaderValue::from_static("Origin"));
    if let (true, Some(origin)) = (from_portal, allowed) {
        h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        if preflight {
            h.insert(
                header::ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("GET, POST, DELETE"),
            );
            h.insert(
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("authorization, content-type"),
            );
            h.insert(header::ACCESS_CONTROL_MAX_AGE, HeaderValue::from_static("600"));
        }
    }
    res
}

async fn security_headers(req: Request, next: Next) -> Response {
    let is_admin = req.uri().path().starts_with("/admin");
    let mut res = next.run(req).await;
    if is_admin {
        let h = res.headers_mut();
        // JSON only: nothing here is ever a page.
        h.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
        );
        h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
        h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
        h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        // Never in a search engine.
        h.insert(
            header::HeaderName::from_static("x-robots-tag"),
            HeaderValue::from_static("noindex, nofollow"),
        );
        h.insert(header::STRICT_TRANSPORT_SECURITY, HeaderValue::from_static("max-age=31536000"));
    }
    res
}

pub struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

fn err(code: StatusCode, msg: impl Into<String>) -> ApiError {
    ApiError(code, msg.into())
}

impl From<crate::store::StoreError> for ApiError {
    fn from(e: crate::store::StoreError) -> Self {
        tracing::error!("admin api: {e}");
        err(StatusCode::INTERNAL_SERVER_ERROR, "Database error.")
    }
}

type ApiResult = Result<Json<Value>, ApiError>;

/// The signed-in staff member.
struct Staff {
    user: User,
}

async fn staff(server: &Server, headers: &HeaderMap) -> Result<Staff, ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| err(StatusCode::UNAUTHORIZED, "Sign in first."))?;
    let (user, _) = server
        .store
        .session_user(&token_hash(token.trim()), SessionKind::Admin, now())
        .await?
        .ok_or_else(|| err(StatusCode::UNAUTHORIZED, "Your session expired."))?;
    if !user.role.is_staff() || user.is_banned(now()) {
        return Err(err(StatusCode::FORBIDDEN, "Staff only."));
    }
    Ok(Staff { user })
}

fn require_admin(s: &Staff) -> Result<(), ApiError> {
    if s.user.role == Role::Admin {
        Ok(())
    } else {
        Err(err(StatusCode::FORBIDDEN, "Administrators only."))
    }
}

async fn log(server: &Server, s: &Staff, action: &str, target: &str, details: &str) {
    if let Err(e) = server.store.audit(s.user.id, action, target, details, now()).await {
        tracing::error!("audit log: {e}");
    }
    tracing::info!(staff = %s.user.username, action, target, details, "staff action");
}

/// A player as moderators see them: without their e-mail.
fn private_for(mut v: Value, admin: bool) -> Value {
    if !admin {
        v["recovery_email"] = Value::Null;
    }
    v
}

/// An audit entry as moderators see it: without the address a staff
/// member signed in from.
fn private_audit(mut e: crate::store::AuditRow, admin: bool) -> crate::store::AuditRow {
    if !admin && e.action == "login" {
        e.details.clear();
    }
    // Payments are for administrators: their ids too.
    if !admin && matches!(e.action.as_str(), "payment" | "msstore-retry") {
        e.target = "-".into();
    }
    e
}

fn user_json(u: &User, online: bool) -> Value {
    json!({
        "id": u.id,
        "username": u.username,
        "country": u.country,
        "role": u.role,
        "records": u.records.map(|_, r| json!({
            "rating": r.rating,
            "peak_rating": r.peak_rating,
            "tier": Tier::of(r.rating).name(),
            "games": r.games,
            "wins": r.wins,
        })),
        "games": u.rated_games(),
        "casual_games": u.casual_games,
        "created_at": u.created_at,
        "last_seen": u.last_seen,
        "banned_until": u.banned_until,
        "ban_reason": u.ban_reason,
        "muted_until": u.muted_until,
        "online": online,
        "recovery_email": u.recovery_email,
        "deleted_at": u.deleted_at,
        "abandons": u.abandons,
        "status_message": u.status_message,
        "name_changes": u.name_changes,
        "avatar": u.avatar,
    })
}

/// Active players, games, new accounts, retention, the funnel, revenue
/// and the queue: what to build next is a decision made on these.
async fn metrics(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    let m = server.store.metrics(now()).await?;
    let countries = server.store.active_countries(now() - 30 * 86_400).await?;
    let waits: Vec<u32> = server.with(|l| l.queue_waits.iter().copied().collect());
    let avg_wait =
        if waits.is_empty() { 0.0 } else { waits.iter().sum::<u32>() as f64 / waits.len() as f64 };
    let mut sorted = waits.clone();
    sorted.sort_unstable();
    let median = sorted.get(sorted.len() / 2).copied().unwrap_or(0);
    let (online, online_by_rules, games, waiting) =
        server.with(|l| (l.online_count(), l.online_by_rules(), l.running_games(), l.queue_now()));
    // The latest waits, oldest first (each pairing counts both players).
    let recent: Vec<u32> = waits.iter().rev().take(60).rev().copied().collect();
    Ok(Json(json!({
        "metrics": m,
        "countries": countries,
        "queue": {
            "pairs": waits.len() / 2, "avg_wait_secs": avg_wait, "median_wait_secs": median,
            "size": waiting.len(),
            "waiting": waiting
                .iter()
                .map(|(offer, secs)| json!({ "rated": offer.rated, "rules": offer.rules, "secs": secs }))
                .collect::<Vec<_>>(),
            "recent": recent,
        },
        "online": online,
        "online_by_rules": online_by_rules,
        "games": games,
        "now": now(),
        "draining": server.with(|l| l.is_draining()),
    })))
}

/// The aim-assistance review queue: players whose potting on hard shots
/// (thin cuts over long distances) is far above the players of their tier.
/// Nobody is banned by this: it is a list to look at, with the replays.
async fn aim_review(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    let hard: Vec<usize> = (0..snook_legend_proto::replay::BANDS)
        .filter(|b| snook_legend_proto::replay::is_hard_band(*b))
        .collect();
    let rows = server.store.aim_rows(&hard, AIM_MIN_ATTEMPTS).await?;
    Ok(Json(json!({
        "min_attempts": AIM_MIN_ATTEMPTS,
        "flagged": flag_outliers(&rows),
        "players": rows.len(),
    })))
}

/// Hard shots a player must have taken before being compared.
const AIM_MIN_ATTEMPTS: u32 = 40;

/// Players above the 99th percentile of their tier on hard shots (and with
/// a rate no honest player keeps up), most suspicious first.
pub fn flag_outliers(rows: &[crate::store::AimRow]) -> Vec<Value> {
    use std::collections::BTreeMap;
    let rate = |r: &crate::store::AimRow| r.made as f64 / r.attempts.max(1) as f64;
    let mut by_tier: BTreeMap<Tier, Vec<f64>> = BTreeMap::new();
    for r in rows {
        by_tier.entry(Tier::of(r.rating)).or_default().push(rate(r));
    }
    for v in by_tier.values_mut() {
        v.sort_by(f64::total_cmp);
    }
    let p99 = |v: &Vec<f64>| {
        v[((v.len() as f64 * 0.99).ceil() as usize).saturating_sub(1).min(v.len() - 1)]
    };
    let mut out: Vec<(f64, Value)> = rows
        .iter()
        .filter_map(|r| {
            let tier = Tier::of(r.rating);
            let rates = &by_tier[&tier];
            let mean = rates.iter().sum::<f64>() / rates.len() as f64;
            let sd =
                (rates.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / rates.len() as f64).sqrt();
            let x = rate(r);
            // With few players in a tier the percentile is the player
            // themself: compare with the tier's spread and a hard floor.
            let threshold = if rates.len() >= 100 { p99(rates) } else { mean + 3.0 * sd.max(0.05) };
            (x >= threshold.max(0.45) && x > mean).then(|| {
                let z = (x - mean) / sd.max(0.02);
                (
                    z,
                    json!({
                        "username": r.username, "rating": r.rating, "tier": tier.name(),
                        "attempts": r.attempts, "made": r.made, "rate": x,
                        "tier_mean": mean, "tier_players": rates.len(), "z": z,
                    }),
                )
            })
        })
        .collect();
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    out.into_iter().map(|(_, v)| v).collect()
}

/// A stored game (to download, or to watch in the game: staff can open any
/// replay from the player's profile).
async fn replay(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    staff(&server, &headers).await?;
    let r = server
        .store
        .replay(id)
        .await?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "No replay for that match."))?;
    Ok(Json(json!({
        "id": r.id, "players": r.players, "ratings": r.ratings, "winner": r.winner,
        "rated": r.rated, "rules": r.replay.rules, "at": r.at, "seed": r.replay.seed,
        "breaker": r.replay.breaker, "strokes": r.replay.strokes,
    })))
}

/// A stored game as frames for the panel's replay viewer.
async fn replay_view(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    staff(&server, &headers).await?;
    let r = server
        .store
        .replay(id)
        .await?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "No replay for that match."))?;
    let replay = r.replay.clone();
    let mut v = tokio::task::spawn_blocking(move || crate::replay_view::view(&replay))
        .await
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "The replay could not be rebuilt."))?;
    v["match"] = json!({
        "id": r.id, "players": r.players, "countries": r.countries, "ratings": r.ratings,
        "winner": r.winner, "rated": r.rated, "at": r.at, "live": false,
    });
    Ok(Json(v))
}

/// A game being played on this server, so far, for the replay viewer.
async fn live_view(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<u64>,
) -> ApiResult {
    staff(&server, &headers).await?;
    let (players, rated, finished, replay) = server
        .with(|l| l.room_replay(id))
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "That game is no longer on this server."))?;
    let mut v = tokio::task::spawn_blocking(move || crate::replay_view::view(&replay))
        .await
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "The game could not be rebuilt."))?;
    v["match"] = json!({
        "id": id, "players": players, "rated": rated, "live": !finished, "at": now(),
    });
    Ok(Json(v))
}

#[derive(Deserialize)]
struct LoginBody {
    username: String,
    password: String,
}

async fn login(
    State(server): State<Arc<Server>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<LoginBody>,
) -> ApiResult {
    let ip = server.client_ip(peer, &headers).to_string();
    // The name as it is looked up: " admin" and "admin" share one limit.
    let username = body.username.trim();
    let wrong = || err(StatusCode::UNAUTHORIZED, "Wrong username or password.");
    // A name no account can have, or a password longer than any can be:
    // turned away before it costs a lookup, a hash or room in the limits.
    if !valid_username(username) || body.password.len() > MAX_PASSWORD_BYTES {
        return Err(wrong());
    }
    // Counted as a failure until the password proves right.
    if !server.auth().try_password(&ip, username) {
        return Err(err(StatusCode::TOO_MANY_REQUESTS, "Too many attempts. Wait a few minutes."));
    }
    let busy = || {
        server.auth().not_tried(&ip, username);
        err(StatusCode::SERVICE_UNAVAILABLE, "The server is busy. Try again in a moment.")
    };
    let user = match server.store.user_by_name(username).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("admin login: {e}");
            return Err(busy());
        }
    };
    let ok = match &user {
        Some(u) => server.verify_password(&body.password, &u.password_hash).await,
        None => server.verify_password(&body.password, crate::DUMMY_HASH).await.map(|_| false),
    };
    let Some(ok) = ok else { return Err(busy()) };
    let user = match user {
        Some(u) if ok => u,
        _ => return Err(wrong()),
    };
    let user = crate::promote_if_admin(&server, user).await;
    if !user.role.is_staff() || user.is_banned(now()) {
        // Stays counted as a failure.
        return Err(err(StatusCode::FORBIDDEN, "This account has no access to the panel."));
    }
    server.auth().succeeded(&ip, username);
    let token = new_token();
    let t = now();
    server
        .store
        .create_session(&token_hash(&token), user.id, SessionKind::Admin, t, t + ADMIN_SESSION)
        .await?;
    let s = Staff { user };
    log(&server, &s, "login", &s.user.username, &ip).await;
    Ok(Json(json!({ "token": token, "user": user_json(&s.user, false) })))
}

async fn logout(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    if let Some(token) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        server.store.delete_session(&token_hash(token.trim())).await?;
    }
    Ok(Json(json!({ "ok": true })))
}

async fn me(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    let s = staff(&server, &headers).await?;
    Ok(Json(user_json(&s.user, false)))
}

async fn overview(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    let o = server.store.overview(now()).await?;
    let live = server.with(|l| l.live());
    Ok(Json(json!({
        "stored": o,
        "online": live.players.len(),
        "addresses": server.with(|l| l.distinct_addresses()),
        "online_by_rules": server.with(|l| l.online_by_rules()),
        "matches_live": live.rooms.iter().filter(|r| !r.finished).count(),
        "tables_open": live.tables,
        "tournaments": live.tournaments.len(),
        "uptime_secs": server.started.elapsed().as_secs(),
        "version": env!("CARGO_PKG_VERSION"),
        "persistent": server.store.is_persistent(),
        "motd": server.with(|l| l.settings.motd.clone()),
    })))
}

async fn live(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    let live = server.with(|l| l.live());
    Ok(Json(serde_json::to_value(live).unwrap_or_default()))
}

#[derive(Deserialize)]
struct UsersQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    page: u32,
    /// `all`, `online`, `staff`, `banned`, `muted` or `deleted`.
    #[serde(default)]
    filter: String,
    /// `newest`, `name`, `rating` or `last_seen`.
    #[serde(default)]
    sort: String,
}

/// Players on a page of the list.
const USERS_PER_PAGE: u32 = 50;

async fn users(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Query(q): Query<UsersQuery>,
) -> ApiResult {
    use crate::store::{UserFilter, UserSearch, UserSort};
    let s = staff(&server, &headers).await?;
    let admin = s.user.role == Role::Admin;
    let per_page = USERS_PER_PAGE;
    let page = q.page.min(10_000);
    // A whole e-mail finds its account, for administrators only (names
    // have no "@"): a moderator can't learn whose an address is.
    if !admin && q.q.contains('@') {
        return Ok(Json(json!({ "users": [], "total": 0, "page": page, "per_page": per_page })));
    }
    let search = UserSearch {
        q: q.q.trim().chars().take(64).collect(),
        filter: UserFilter::from_code(&q.filter),
        sort: UserSort::from_code(&q.sort),
        now: now(),
    };
    let (list, total) = if q.filter == "online" {
        // Who is online is known here, not in the database.
        let ids = server.with(|l| l.online_user_ids());
        let mut all = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(u) = server.store.user_by_id(id).await?
                && search.matches(&u)
            {
                all.push(u);
            }
        }
        match search.sort {
            UserSort::Newest => all.sort_by_key(|u| std::cmp::Reverse(u.id)),
            UserSort::Name => all.sort_by_key(|u| u.username.to_lowercase()),
            UserSort::Rating(rules) => {
                all.sort_by_key(|u| std::cmp::Reverse(u.records.get(rules).rating))
            }
            UserSort::LastSeen => all.sort_by_key(|u| std::cmp::Reverse(u.last_seen)),
        }
        let total = all.len() as i64;
        let start = (page * per_page) as usize;
        (all.into_iter().skip(start).take(per_page as usize).collect::<Vec<_>>(), total)
    } else {
        server.store.search_users(&search, page * per_page, per_page).await?
    };
    let rows: Vec<Value> = server.with(|l| {
        list.iter().map(|u| private_for(user_json(u, l.is_online(&u.username)), admin)).collect()
    });
    Ok(Json(json!({ "users": rows, "total": total, "page": page, "per_page": per_page })))
}

async fn find(server: &Server, name: &str) -> Result<User, ApiError> {
    server
        .store
        .user_by_name(name)
        .await?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "No such user."))
}

async fn user_detail(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    let admin = s.user.role == Role::Admin;
    let u = find(&server, &name).await?;
    let (career, recovery) =
        tokio::join!(server.store.career(u.id), server.store.payment_email(u.id));
    let (history, reports, achievements, wallet, ledger, payments) = tokio::join!(
        server.store.history(u.id, 50),
        server.store.reports_about(u.id, 50),
        server.store.achievements(u.id),
        server.store.wallet(u.id),
        server.store.ledger(u.id, 50),
        server.store.payments_of(u.id, 50),
    );
    let msstore = server.store.ms_consumes_of(u.id, 50).await?;
    let doubles = server.store.doubles_records(u.id).await?;
    let notes = server.store.staff_notes(u.id, 100).await?;
    let computer = computer_record(&server, u.id).await?;
    let search =
        crate::store::AuditSearch { target: Some(u.username.clone()), ..Default::default() };
    let moderation = server.store.audit_search(&search, 100).await?;
    let (renames, sent, adjustments) = tokio::join!(
        server.store.profile_changes(u.id, 100),
        server.store.notifications_sent(u.id, 50),
        server.store.rating_adjustments(u.id, 50),
    );
    let sent: Vec<Value> = sent?.iter().map(moderation::notification_json).collect();
    let online = server.with(|l| l.is_online(&u.username));
    let presence = server
        .with(|l| l.player(&u.username).map(|p| json!({ "status": p.status, "rules": p.rules })));
    let history: Vec<Value> = history?
        .into_iter()
        .map(|h| {
            json!({
                "opponent": h.opponent, "won": h.won, "reason": h.reason, "rated": h.rated,
                "rules": h.rules, "delta": h.delta, "rating_after": h.rating_after, "at": h.at,
                "match_id": h.match_id, "replay": h.replay,
            })
        })
        .collect();
    // E-mails, payments and sign-in addresses are for administrators: so
    // are the wallet's Stripe customer and the payment references in the
    // ledger.
    let moderation: Vec<crate::store::AuditRow> =
        moderation.into_iter().map(|e| private_audit(e, admin)).collect();
    let wallet = wallet.map(|mut w| {
        if !admin {
            w.stripe_customer = None;
        }
        w
    });
    let ledger = ledger.map(|rows| {
        rows.into_iter()
            .map(|mut l| {
                if !admin {
                    l.reference = None;
                }
                l
            })
            .collect::<Vec<_>>()
    });
    Ok(Json(json!({
        "user": private_for(user_json(&u, online), admin),
        "history": history,
        "reports": with_games(&server, reports?).await,
        "achievements": achievements?,
        "wallet": wallet?,
        "ledger": ledger?,
        "payments": if admin { json!(payments?) } else { json!([]) },
        "msstore": if admin { json!(msstore) } else { json!([]) },
        "career": career?,
        "payment_email": if admin { json!(recovery?) } else { Value::Null },
        "notes": notes,
        "moderation": moderation,
        "presence": presence,
        "computer": computer,
        "renames": renames?,
        "notifications": sent,
        "rating_adjustments": adjustments?,
        "doubles_ratings": modes::records_json(&doubles),
        "private_hidden": !admin,
    })))
}

/// What a player did against the computer, as far as the server knows it:
/// the tour (when its final was won, how many finals were dealt), the
/// medals of the tournaments against the computer (kept once the player is
/// online), the drills' best scores and stars, and the daily challenges they
/// topped. A game's progress on the tour is kept by the game, not here.
async fn computer_record(server: &Server, user_id: i64) -> Result<Value, ApiError> {
    use snook_legend_proto::drills;
    let today = snook_legend_proto::daily::day_of(now());
    let (medals, recent, tour, finals, bests, daily) = tokio::join!(
        server.store.cup_medals(user_id),
        server.store.cup_medal_list(user_id, 30),
        server.store.ladder_claim(user_id),
        server.store.ladder_finals(user_id),
        server.store.drill_bests(user_id),
        server.store.daily_wins(user_id, today),
    );
    let recent: Vec<Value> = recent?
        .into_iter()
        .map(|(m, at)| {
            // The tournament's number as text: it is larger than a script's
            // numbers keep exactly.
            json!({ "cup": m.cup.to_string(), "format": m.format, "place": m.place, "at": at })
        })
        .collect();
    let mut stars = 0;
    let drills: Vec<Value> = bests?
        .into_iter()
        .map(|(code, best)| {
            let drill = drills::drill(&code);
            let earned = drill.map_or(0, |d| drills::stars(d, best));
            stars += earned;
            json!({ "code": code, "name": drill.map(|d| d.name), "best": best, "stars": earned })
        })
        .collect();
    let (finals, last_final) = finals?;
    Ok(json!({
        "tour_won_at": tour?,
        "finals": finals,
        "finals_kept": crate::store::LADDER_TICKETS_KEPT,
        "last_final": last_final,
        "medals": medals?,
        "recent_medals": recent,
        "drills": drills,
        "drill_stars": stars,
        "drill_stars_max": drills::max_stars(),
        "daily_wins": daily?.len(),
    }))
}

#[derive(Deserialize, Default)]
struct ActionBody {
    #[serde(default)]
    hours: Option<u32>,
    #[serde(default)]
    minutes: Option<u32>,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    role: Option<Role>,
    #[serde(default)]
    password: String,
    /// Coins to add (negative: remove), for "coins".
    #[serde(default)]
    coins: Option<i64>,
    /// Item code, for "item"; days, for "vip".
    #[serde(default)]
    item: String,
    #[serde(default)]
    days: Option<u32>,
    /// The new username, for "rename".
    #[serde(default)]
    name: String,
    /// Show what the player would be told (warn, mute, unmute, coins, item,
    /// vip) and do nothing.
    #[serde(default)]
    preview: bool,
}

async fn user_action(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path((name, action)): Path<(String, String)>,
    body: Option<Json<ActionBody>>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    let body = body.map(|b| b.0).unwrap_or_default();
    let target = find(&server, &name).await?;
    // Moderators can't act on other staff. Nobody bans, mutes or changes the
    // role of their own account; an administrator may change their own
    // wallet (coins, items, VIP), which is in the audit log like any other.
    // (A listed administrator who hasn't signed in since is staff already.)
    let staff_target = target.role.is_staff() || server.is_admin_name(&target.username);
    if staff_target && s.user.role != Role::Admin {
        return Err(err(StatusCode::FORBIDDEN, "Only administrators can act on staff."));
    }
    let own_wallet = matches!(action.as_str(), "coins" | "item" | "vip" | "revoke" | "note");
    if target.id == s.user.id && action != "logout-all" && !own_wallet {
        return Err(err(StatusCode::BAD_REQUEST, "You can't do that to yourself."));
    }
    let reason = clean_text(&body.reason, 300);
    let t = now();
    // What the player is told in their inbox (the portal can see it first).
    if let Some(n) = inbox_notice(&action, &body, &reason, &s, t) {
        if body.preview {
            return Ok(Json(crate::inbox::preview(&n)));
        }
    } else if body.preview {
        return Err(err(StatusCode::BAD_REQUEST, "Nothing to preview: the player isn't told."));
    }
    match action.as_str() {
        "ban" => {
            let until = match body.hours {
                Some(h) if h > 0 => t + h as i64 * 3600,
                _ => i64::MAX,
            };
            server.store.set_ban(target.id, Some(until), Some(&reason)).await?;
            server.store.delete_user_sessions(target.id).await?;
            let msg = match body.hours {
                Some(h) if h > 0 => format!("Your account was suspended for {h} hours: {reason}"),
                _ => format!("Your account was banned: {reason}"),
            };
            // Out of the tournaments (no prize or crown for them), and, for
            // longer than a turn lasts, out of their games by correspondence
            // (their opponents don't wait days on them).
            server.with(|l| {
                l.disqualify(&target.username);
                l.kick(&target.username, &msg);
            });
            let long = body.hours.is_none_or(|h| {
                h == 0 || h as i64 * 3600 > snook_legend_proto::correspondence::TURN_SECS
            });
            if long && !server.config.settings.lan {
                crate::correspondence::forfeit_all(&server, target.id).await;
            }
            let dur = match body.hours {
                Some(h) if h > 0 => format!("{h}h"),
                _ => "permanent".into(),
            };
            log(&server, &s, "ban", &target.username, &format!("{dur}: {reason}")).await;
        }
        "unban" => {
            server.store.set_ban(target.id, None, None).await?;
            log(&server, &s, "unban", &target.username, &reason).await;
        }
        "mute" => {
            let minutes = body.minutes.unwrap_or(60).clamp(1, 60 * 24 * 30);
            let until = t + minutes as i64 * 60;
            server.store.set_mute(target.id, Some(until)).await?;
            refresh(&server, &target).await;
            server.with(|l| {
                l.set_mute_reason(target.id, &reason);
                // A game older than the inbox is told as it was.
                if !l.has_inbox(&target.username) {
                    l.notify(
                        &target.username,
                        snook_legend_proto::ServerMsg::Notice {
                            text: format!("You were muted for {minutes} minutes: {reason}"),
                        },
                    )
                }
            });
            if let Some(n) = inbox_notice(&action, &body, &reason, &s, t) {
                crate::inbox::send(&server, target.id, n).await;
            }
            log(&server, &s, "mute", &target.username, &format!("{minutes} min: {reason}")).await;
        }
        "unmute" => {
            server.store.set_mute(target.id, None).await?;
            refresh(&server, &target).await;
            server.with(|l| l.set_mute_reason(target.id, ""));
            if let Some(n) = inbox_notice(&action, &body, &reason, &s, t) {
                crate::inbox::send(&server, target.id, n).await;
            }
            log(&server, &s, "unmute", &target.username, &reason).await;
        }
        "kick" => {
            let msg = if reason.is_empty() {
                "You were disconnected by a moderator.".to_string()
            } else {
                format!("You were disconnected by a moderator: {reason}")
            };
            let was = server.with(|l| l.kick(&target.username, &msg));
            if !was {
                return Err(err(StatusCode::BAD_REQUEST, "That player is not online."));
            }
            log(&server, &s, "kick", &target.username, &reason).await;
        }
        "clear-status" => {
            if target.status_message.is_empty() {
                return Err(err(StatusCode::BAD_REQUEST, "No status message to clear."));
            }
            server.store.set_status_message(target.id, "").await?;
            refresh(&server, &target).await;
            server.with(|l| {
                l.notify(
                    &target.username,
                    snook_legend_proto::ServerMsg::Notice {
                        text: "A moderator cleared your status message.".into(),
                    },
                )
            });
            log(&server, &s, "clear-status", &target.username, &target.status_message).await;
        }
        "logout-all" => {
            server.store.delete_user_sessions(target.id).await?;
            server.with(|l| l.kick(&target.username, "Please sign in again."));
            log(&server, &s, "logout-all", &target.username, "").await;
        }
        "role" => {
            require_admin(&s)?;
            let role = body.role.ok_or_else(|| err(StatusCode::BAD_REQUEST, "Missing role."))?;
            server.store.set_role(target.id, role).await?;
            server.store.delete_user_sessions(target.id).await?;
            refresh(&server, &target).await;
            // The connection keeps a copy of the account it signed in with
            // (and the powers that came with it): the player signs in again.
            server.with(|l| l.kick(&target.username, "Please sign in again."));
            log(&server, &s, "role", &target.username, &format!("{role:?}")).await;
        }
        "password" => {
            require_admin(&s)?;
            if !valid_password(&body.password) {
                return Err(err(StatusCode::BAD_REQUEST, "Passwords need at least 8 characters."));
            }
            let hash = server
                .hash_password(&body.password)
                .await
                .ok_or_else(|| err(StatusCode::INTERNAL_SERVER_ERROR, "Could not hash."))?;
            server.store.set_password(target.id, &hash).await?;
            server.store.delete_user_sessions(target.id).await?;
            server.with(|l| l.kick(&target.username, "Your password was changed. Sign in again."));
            log(&server, &s, "password-reset", &target.username, "").await;
        }
        // Shop support (refund goodwill, contest prizes…): administrators only,
        // always with a reason, always in the audit log.
        "coins" => {
            require_admin(&s)?;
            let coins = body.coins.filter(|c| *c != 0 && c.unsigned_abs() <= 1_000_000);
            let coins = coins.ok_or_else(|| err(StatusCode::BAD_REQUEST, "Give a coin amount."))?;
            if reason.is_empty() {
                return Err(err(StatusCode::BAD_REQUEST, "Give a reason."));
            }
            server.store.credit(target.id, coins, &format!("staff: {reason}"), None, t).await?;
            crate::shop::push_wallet(&server, &target).await;
            if let Some(n) = inbox_notice(&action, &body, &reason, &s, t) {
                crate::inbox::send(&server, target.id, n).await;
            }
            log(&server, &s, "coins", &target.username, &format!("{coins:+}: {reason}")).await;
        }
        "item" => {
            require_admin(&s)?;
            // Only what owning can make usable: not what comes with VIP or
            // an achievement, nor what everyone has.
            use snook_legend_proto::shop::Price;
            match snook_legend_proto::shop::item(&body.item).map(|i| i.price) {
                None => return Err(err(StatusCode::BAD_REQUEST, "No such item.")),
                // (Items only won, too: the staff award them for events.)
                Some(Price::Coins(_) | Price::Pack | Price::Earned) => {}
                Some(_) => {
                    return Err(err(
                        StatusCode::BAD_REQUEST,
                        "That item comes with VIP, an achievement, or to everyone: it can't be given.",
                    ));
                }
            }
            if server.store.wallet(target.id).await?.owned.contains(&body.item) {
                return Err(err(StatusCode::CONFLICT, "They already have it."));
            }
            server.store.grant_item(target.id, &body.item, "staff", t).await?;
            let origin =
                snook_legend_proto::collection::Origin::Staff { reason: reason.to_string() };
            crate::collection::record(&server, target.id, &body.item, origin).await;
            crate::shop::push_wallet(&server, &target).await;
            if let Some(n) = inbox_notice(&action, &body, &reason, &s, t) {
                crate::inbox::send(&server, target.id, n).await;
            }
            log(&server, &s, "item", &target.username, &format!("{}: {reason}", body.item)).await;
        }
        "vip" => {
            require_admin(&s)?;
            let days = body.days.filter(|d| *d > 0 && *d <= 366);
            let days = days.ok_or_else(|| err(StatusCode::BAD_REQUEST, "Give 1–366 days."))?;
            server.store.add_vip(target.id, t, days as i64 * 86_400).await?;
            crate::shop::push_wallet(&server, &target).await;
            if let Some(n) = inbox_notice(&action, &body, &reason, &s, t) {
                crate::inbox::send(&server, target.id, n).await;
            }
            log(&server, &s, "vip", &target.username, &format!("+{days} days: {reason}")).await;
        }
        // A note among the staff (the player never sees it).
        "note" => {
            let text = clean_text(&body.reason, crate::store::MAX_STAFF_NOTE);
            if text.is_empty() {
                return Err(err(StatusCode::BAD_REQUEST, "Write the note."));
            }
            server.store.add_staff_note(target.id, s.user.id, "note", &text, t).await?;
            log(&server, &s, "note", &target.username, &text).await;
        }
        // A warning the player is shown: now if online, else at their next
        // sign-in.
        "warn" => {
            let text = clean_text(&body.reason, crate::store::MAX_STAFF_NOTE);
            if text.is_empty() {
                return Err(err(StatusCode::BAD_REQUEST, "Write the warning."));
            }
            server.store.add_staff_note(target.id, s.user.id, "warning", &text, t).await?;
            if server.with(|l| l.is_online(&target.username)) {
                // (A game with an inbox has it there: it is marked shown.)
                crate::account::deliver_warnings(&server, &target).await;
            }
            if let Some(n) = inbox_notice(&action, &body, &reason, &s, t) {
                crate::inbox::send(&server, target.id, n).await;
            }
            log(&server, &s, "warn", &target.username, &text).await;
        }
        // An offensive name: changed by the staff, without using one of the
        // player's own changes. The player signs in again under it.
        "rename" => {
            require_admin(&s)?;
            let name = body.name.trim();
            if !valid_username(name) {
                return Err(err(
                    StatusCode::BAD_REQUEST,
                    "Usernames have 3–16 letters, digits or underscores.",
                ));
            }
            if name == target.username {
                return Err(err(StatusCode::BAD_REQUEST, "That is already their username."));
            }
            if server.is_reserved_name(name) && !name.eq_ignore_ascii_case(&target.username) {
                return Err(err(StatusCode::CONFLICT, "That username is taken."));
            }
            // Whoever registered the name freed could take their seat.
            let busy = server.with(|l| {
                let busy = l.is_busy(&target.username);
                if !busy {
                    l.drop_invites_named(&target.username);
                }
                busy
            });
            if busy {
                return Err(err(
                    StatusCode::CONFLICT,
                    "They are in a game, at a table, in the queue or in a tournament: rename them once they are out.",
                ));
            }
            match server.store.staff_rename(target.id, name).await? {
                crate::store::Rename::Done => {}
                crate::store::Rename::Taken => {
                    return Err(err(StatusCode::CONFLICT, "That username is taken."));
                }
                crate::store::Rename::NoChangesLeft => {
                    return Err(err(StatusCode::BAD_REQUEST, "A deleted account keeps its name."));
                }
            }
            server.with(|l| {
                l.kick(
                    &target.username,
                    &format!("The staff changed your username to {name}. Sign in again."),
                )
            });
            // Under the new name, where the rest of their history went (the
            // old one may be someone else's next).
            let old = &target.username;
            let detail = if reason.is_empty() {
                format!("from {old}")
            } else {
                format!("from {old}: {reason}")
            };
            log(&server, &s, "rename", name, &detail).await;
            let fresh = find(&server, name).await?;
            return Ok(Json(json!({ "ok": true, "user": user_json(&fresh, false) })));
        }
        // An item taken back (given by mistake, a refunded purchase).
        "revoke" => {
            require_admin(&s)?;
            if reason.is_empty() {
                return Err(err(StatusCode::BAD_REQUEST, "Give a reason."));
            }
            if server.store.revoke_items(target.id, &[body.item.as_str()]).await? == 0 {
                return Err(err(StatusCode::BAD_REQUEST, "The player doesn't have that item."));
            }
            crate::shop::push_wallet(&server, &target).await;
            log(&server, &s, "revoke", &target.username, &format!("{}: {reason}", body.item)).await;
        }
        _ => return Err(err(StatusCode::NOT_FOUND, "Unknown action.")),
    }
    let fresh = find(&server, &target.username).await?;
    let online = server.with(|l| l.is_online(&fresh.username));
    // The recovery e-mail is for administrators (as on the player's page).
    let user = private_for(user_json(&fresh, online), s.user.role == Role::Admin);
    Ok(Json(json!({ "ok": true, "user": user })))
}

/// What a user action tells the player in their inbox (None: nothing).
/// A warning is pinned for a week and asks the player to confirm it; a
/// mute stays pinned until it ends.
fn inbox_notice(
    action: &str,
    body: &ActionBody,
    reason: &str,
    s: &Staff,
    t: i64,
) -> Option<crate::inbox::New> {
    use snook_legend_proto::inbox::{Action, Kind};
    let from = s.user.username.clone();
    let n = match action {
        "warn" => {
            let text = clean_text(&body.reason, crate::store::MAX_STAFF_NOTE);
            crate::inbox::New::game(Kind::Warning)
                .text(text)
                .important()
                .pinned_until(t + WARNING_PINNED)
                .quiet()
        }
        "mute" => {
            let minutes = body.minutes.unwrap_or(60).clamp(1, 60 * 24 * 30);
            let until = t + minutes as i64 * 60;
            crate::inbox::New::game(Kind::Muted { until }).text(reason).pinned_until(until).quiet()
        }
        "unmute" => crate::inbox::New::game(Kind::Unmuted).text(reason),
        "coins" => {
            let coins = body.coins.filter(|c| *c > 0)?;
            crate::inbox::New::game(Kind::StaffGift { coins, item: String::new(), vip_days: 0 })
                .text(reason)
        }
        "item" => crate::inbox::New::game(Kind::StaffGift {
            coins: 0,
            item: body.item.clone(),
            vip_days: 0,
        })
        .text(reason)
        .action(Action::Equip { code: body.item.clone() }),
        "vip" => {
            let days = body.days.filter(|d| *d > 0 && *d <= 366)?;
            crate::inbox::New::game(Kind::StaffGift {
                coins: 0,
                item: String::new(),
                vip_days: days,
            })
            .text(reason)
            .action(Action::Vip)
        }
        _ => return None,
    };
    Some(n.from(from))
}

/// A warning stays at the top of the player's inbox this long.
const WARNING_PINNED: i64 = 7 * 86_400;

/// Reloads an online player's account into the lounge.
async fn refresh(server: &Server, target: &User) {
    if let Ok(Some(fresh)) = server.store.user_by_id(target.id).await {
        server.with(|l| l.refresh_user(fresh));
    }
}

#[derive(Deserialize)]
struct ReportsQuery {
    #[serde(default)]
    status: String,
}

async fn reports(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Query(q): Query<ReportsQuery>,
) -> ApiResult {
    staff(&server, &headers).await?;
    let status = match q.status.as_str() {
        "all" => None,
        "actioned" => Some(1),
        "dismissed" => Some(2),
        _ => Some(0),
    };
    let list = server.store.reports(status, 200).await?;
    Ok(Json(json!({ "reports": with_games(&server, list).await })))
}

/// Reports with the game each one is about (see [`crate::reports::game_json`]).
async fn with_games(server: &Server, list: Vec<crate::store::ReportRow>) -> Vec<Value> {
    let mut out = Vec::with_capacity(list.len());
    for mut r in list {
        let game = crate::reports::game_json(server, &mut r).await;
        let mut v = serde_json::to_value(&r).unwrap_or_default();
        v["game"] = game;
        out.push(v);
    }
    out
}

#[derive(Deserialize)]
struct ResolveBody {
    status: String,
    #[serde(default)]
    note: String,
}

async fn resolve_report(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<ResolveBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    let status = match body.status.as_str() {
        "actioned" => 1,
        "dismissed" => 2,
        "open" => 0,
        _ => return Err(err(StatusCode::BAD_REQUEST, "Unknown status.")),
    };
    let note = clean_text(&body.note, 300);
    let report = server
        .store
        .report(id)
        .await?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "No such report."))?;
    // A moderator's reports are an administrator's to answer. An
    // administrator answers those about themselves (with one administrator
    // nobody else could, and they stayed open for ever); the answer goes in
    // the audit log like any other.
    if report.target_id == s.user.id && s.user.role != Role::Admin {
        return Err(err(StatusCode::FORBIDDEN, "An administrator answers reports about you."));
    }
    if let Some(target) = server.store.user_by_id(report.target_id).await?
        && !crate::social::may_moderate(s.user.role, target.role)
    {
        return Err(err(StatusCode::FORBIDDEN, "Only administrators can act on staff."));
    }
    if !server.store.resolve_report(id, s.user.id, status, &note, now()).await? {
        return Err(err(StatusCode::NOT_FOUND, "No such report."));
    }
    // Those who reported the player hear that the staff acted (never how);
    // a dismissal tells nobody.
    if status == 1 && report.status != 1 {
        moderation::thank_reporters(&server, &report, s.user.id).await;
    }
    log(&server, &s, "report", &format!("#{id}"), &format!("{}: {note}", body.status)).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct AuditQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    action: String,
    #[serde(default)]
    target: String,
    /// Entries older than this id (the next page).
    #[serde(default)]
    before: Option<i64>,
}

/// Entries on a page of the audit log.
const AUDIT_PAGE: u32 = 100;

async fn audit(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Query(q): Query<AuditQuery>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    let admin = s.user.role == Role::Admin;
    let some = |v: &str| Some(v.trim().to_string()).filter(|v| !v.is_empty());
    let search = crate::store::AuditSearch {
        q: q.q.trim().chars().take(64).collect(),
        action: some(&q.action),
        target: some(&q.target),
        before: q.before,
    };
    let entries = server.store.audit_search(&search, AUDIT_PAGE).await?;
    let next =
        (entries.len() == AUDIT_PAGE as usize).then(|| entries.last().map(|e| e.id)).flatten();
    // A moderator's words don't search the addresses of sign-ins (they
    // would spell them out letter by letter): such an entry is found only
    // by its staff member or target.
    // Nor the ids of payments, which they don't see (a search on them would
    // tell, letter by letter, which there are).
    let q = search.q.to_lowercase();
    let hidden_target =
        |e: &crate::store::AuditRow| matches!(e.action.as_str(), "payment" | "msstore-retry");
    let entries: Vec<crate::store::AuditRow> = entries
        .into_iter()
        .filter(|e| {
            admin
                || q.is_empty()
                || e.action != "login"
                || e.staff.to_lowercase().contains(&q)
                || e.target.to_lowercase().contains(&q)
        })
        .filter(|e| {
            admin
                || !hidden_target(e)
                || (search.target.is_none()
                    && (q.is_empty() || e.staff.to_lowercase().contains(&q)))
        })
        .map(|e| private_audit(e, admin))
        .collect();
    Ok(Json(json!({ "entries": entries, "next": next })))
}

/// The lounge chat as the staff see it (deleted lines with their text).
async fn chat(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    Ok(Json(json!({ "lines": server.with(|l| l.staff_chat()) })))
}

#[derive(Deserialize, Default)]
struct DeleteChatBody {
    /// Without a trace (administrators).
    #[serde(default)]
    silent: bool,
}

async fn delete_chat(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<u64>,
    body: Option<Json<DeleteChatBody>>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    let silent = body.map(|b| b.0.silent).unwrap_or(false);
    if silent {
        require_admin(&s)?;
    }
    let line = server
        .with(|l| l.staff_chat())
        .into_iter()
        .find(|l| l["id"] == id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "No such line."))?;
    let author = line["from"].as_str().unwrap_or_default().to_string();
    // A moderator's line stays out of another moderator's reach.
    if let Some(u) = server.store.user_by_name(&author).await?
        && !crate::social::may_moderate(s.user.role, u.role)
        && u.id != s.user.id
    {
        return Err(err(StatusCode::FORBIDDEN, "Only administrators can act on staff."));
    }
    if !server.with(|l| l.staff_delete_chat(id, silent)) {
        return Err(err(StatusCode::BAD_REQUEST, "That line is already deleted."));
    }
    let text = line["text"].as_str().unwrap_or_default();
    let what = if silent { "removed" } else { "deleted" };
    log(&server, &s, "chat-delete", &author, &format!("{what}: {text}")).await;
    Ok(Json(json!({ "ok": true })))
}

/// A tournament called off before it starts (fees go back).
async fn cancel_tournament(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<u64>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    let name = server
        .with(|l| l.staff_cancel_tournament(id))
        .map_err(|why| err(StatusCode::BAD_REQUEST, why))?;
    log(&server, &s, "tournament-cancel", &name, &format!("#{id}")).await;
    Ok(Json(json!({ "ok": true })))
}

/// Coins, VIP, the latest payments and the shop's catalogue.
async fn economy(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    let staff_role = staff(&server, &headers).await?.user.role;
    let t = now();
    let (economy, payments) =
        tokio::join!(server.store.economy(t), server.store.recent_payments(100));
    let admin = staff_role == Role::Admin;
    let payments: Vec<Value> = payments?
        .into_iter()
        .filter(|_| admin)
        .map(|(p, payer)| {
            json!({
                "id": p.id, "payer": payer, "kind": p.kind, "coins": p.coins,
                "amount": p.amount, "currency": p.currency, "at": p.at,
                "refunded": p.refunded, "test": p.test,
            })
        })
        .collect();
    use snook_legend_proto::shop::{ITEMS, Price};
    let catalog: Vec<Value> = ITEMS
        .iter()
        .map(|i| {
            let (how, coins) = match i.price {
                Price::Free => ("free", None),
                Price::Coins(n) => ("coins", Some(n)),
                Price::Achievement(_) => ("achievement", None),
                Price::Vip => ("vip", None),
                Price::Pack => ("pack", None),
                Price::Earned => ("earned", None),
            };
            json!({
                "code": i.code, "name": i.name, "category": format!("{:?}", i.category).to_lowercase(),
                "how": how, "coins": coins,
                "achievement": match i.price { Price::Achievement(a) => Some(a), _ => None },
                "rarity": snook_legend_proto::shop::rarity(i),
                "bound": snook_legend_proto::shop::bound(i),
            })
        })
        .collect();
    Ok(Json(json!({
        "economy": economy?,
        "payments": payments,
        "private_hidden": !admin,
        "catalog": catalog,
        "stripe": server.stripe.is_some(),
        "msstore": server.msstore.is_some(),
        "now": t,
    })))
}

/// The server itself: what runs, how it is set up, how busy it is.
async fn system(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    // As in force now (see `crate::settings`).
    let r = server.with(|l| l.settings.clone());
    let (online, addresses, games, live, queue, draining) = server.with(|l| {
        (
            l.online_count(),
            l.distinct_addresses(),
            l.running_games(),
            l.live(),
            l.queue_now().len(),
            l.is_draining(),
        )
    });
    let pending = server.with(|l| l.pending_effects()).load(std::sync::atomic::Ordering::Relaxed);
    let cup = r.cup.as_ref().map(|c| {
        json!({
            "name": c.name, "weekday": c.weekday, "hour": c.hour, "minute": c.minute,
            "utc_offset": c.utc_offset, "minutes": c.minutes, "entry_fee": c.entry_fee,
            "rules": c.rules,
        })
    });
    Ok(Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": snook_legend_proto::PROTOCOL_VERSION,
        "oldest_protocol": snook_legend_proto::OLDEST_PROTOCOL_VERSION,
        "uptime_secs": server.started.elapsed().as_secs(),
        "persistent": server.store.is_persistent(),
        "draining": draining,
        "memory_bytes": resident_memory(),
        "online": online,
        "addresses": addresses,
        "games": games,
        "tables": live.tables,
        "tournaments": live.tournaments.len(),
        "queue": queue,
        "pending_writes": pending,
        "stripe": server.stripe.is_some(),
        "msstore": server.msstore.is_some(),
        "online_by_rules": server.with(|l| l.online_by_rules()),
        "settings": {
            "shot_clock_secs": r.shot_clock.as_secs(),
            "reconnect_grace_secs": r.reconnect_grace.as_secs(),
            "chat_history": r.chat_history,
            "lan": r.lan,
            "max_connections_per_ip": r.max_connections_per_ip,
            "tournament_no_show_secs": r.tournament_no_show.as_secs(),
            "max_open_tournaments": r.max_open_tournaments,
            "seek_lifetime_secs": r.seek_lifetime.as_secs(),
            "cup": cup,
        },
        "now": now(),
    })))
}

/// A time as people read it, in UTC ("2026-11-21 20:00 UTC").
pub(crate) fn utc(t: i64) -> String {
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year}-{month:02}-{day:02} {:02}:{:02} UTC", secs / 3600, secs % 3600 / 60)
}

/// The memory the server holds (Linux; none elsewhere).
fn resident_memory() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

/// The staff's events: the latest (past, called off, running, coming), with
/// how each stands now.
async fn list_events(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    let t = now();
    let events: Vec<Value> = server
        .store
        .recent_events(200)
        .await?
        .into_iter()
        .map(|e| {
            let status = if e.cancelled_at.is_some() {
                "cancelled"
            } else if e.ends_at <= t {
                "ended"
            } else if e.starts_at <= t {
                "live"
            } else {
                "scheduled"
            };
            let mut v = serde_json::to_value(&e).unwrap_or_default();
            v["status"] = json!(status);
            v
        })
        .collect();
    Ok(Json(json!({
        "events": events,
        "now": t,
        "limits": {
            "max_multiplier": snook_legend_proto::events::MAX_MULTIPLIER,
            "max_boost_days": snook_legend_proto::events::MAX_BOOST_LENGTH / 86_400,
            "max_title": snook_legend_proto::events::MAX_TITLE,
            "knockout_sizes": crate::lounge::KNOCKOUT_SIZES,
            "arena_minutes": [*crate::lounge::ARENA_MINUTES.start(), *crate::lounge::ARENA_MINUTES.end()],
            "max_entry_fee": crate::lounge::tournaments_max_fee(),
            "max_prize": crate::lounge::MAX_HOUSE_PRIZE,
        },
    })))
}

#[derive(Deserialize)]
struct EventBody {
    /// Optional on `/tournaments/official`, which sets it.
    #[serde(default)]
    kind: String,
    title: String,
    #[serde(default)]
    multiplier: u32,
    starts_at: i64,
    ends_at: i64,
    #[serde(default)]
    format: String,
    #[serde(default)]
    size: u32,
    #[serde(default)]
    rated: bool,
    #[serde(default)]
    rules: Rules,
    #[serde(default)]
    entry_fee: u32,
    #[serde(default)]
    prize: u32,
    /// A tournament's: the champion's share of the prizes, percent (the
    /// runner-up has the rest; 0: the usual 70:20 of the fees, 70:30 of the
    /// house's prize).
    #[serde(default)]
    champion_share: u32,
    /// A tournament's: an item the champion wins too (an item that is only
    /// won; empty: none).
    #[serde(default)]
    item_prize: String,
}

/// Events at once (running or coming) at most: a mistake can't fill the
/// lounge.
const MAX_OPEN_EVENTS: usize = 50;

/// Schedules an event (administrators: it touches the economy).
async fn create_event(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(body): Json<EventBody>,
) -> ApiResult {
    use snook_legend_proto::events::{EventKind, MAX_BOOST_LENGTH, MAX_MULTIPLIER, MAX_TITLE};
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    let bad = |m: &str| err(StatusCode::BAD_REQUEST, m);
    let t = now();
    let kind = EventKind::from_code(&body.kind);
    if kind == EventKind::Unknown {
        return Err(bad("Unknown kind of event."));
    }
    let title = clean_text(&body.title, MAX_TITLE);
    if title.is_empty() {
        return Err(bad("Give the event a title."));
    }
    // "Now" in the form is a moment ago here.
    let starts_at = if kind.is_boost() && body.starts_at >= t - 600 {
        body.starts_at.max(t)
    } else {
        body.starts_at
    };
    let ends_at = body.ends_at;
    if ends_at <= starts_at {
        return Err(bad("The event must end after it starts."));
    }
    if ends_at <= t {
        return Err(bad("That time is already past."));
    }
    if starts_at > t + 365 * 86_400 {
        return Err(bad("Schedule events within a year."));
    }
    let (multiplier, format, size, rated, entry_fee, prize) = if kind.is_boost() {
        if starts_at < t {
            return Err(bad("That time is already past."));
        }
        if !(2..=MAX_MULTIPLIER).contains(&body.multiplier) {
            return Err(bad("Boosts are ×2 or ×3."));
        }
        if ends_at - starts_at > MAX_BOOST_LENGTH {
            return Err(bad("A boost lasts two weeks at most."));
        }
        (body.multiplier, "", 0, false, 0, 0)
    } else {
        if starts_at < t + 60 {
            return Err(bad("A tournament must start at least a minute from now."));
        }
        let length = ends_at - starts_at;
        let format = match body.format.as_str() {
            "arena" => {
                let range = crate::lounge::ARENA_MINUTES;
                if !(*range.start() as i64 * 60..=*range.end() as i64 * 60).contains(&length) {
                    return Err(bad("An arena lasts from 10 minutes to 3 hours."));
                }
                "arena"
            }
            "knockout" => {
                if !crate::lounge::KNOCKOUT_SIZES.contains(&body.size) {
                    return Err(bad("A knockout has 4, 8, 16 or 32 seats."));
                }
                if length > 24 * 3600 {
                    return Err(bad("A knockout ends within a day of its start."));
                }
                "knockout"
            }
            _ => return Err(bad("Choose knockout or arena.")),
        };
        if body.entry_fee > crate::lounge::tournaments_max_fee() {
            return Err(bad("The entry fee is 1,000 coins at most."));
        }
        if body.prize > crate::lounge::MAX_HOUSE_PRIZE {
            return Err(bad("The house adds 100,000 coins at most."));
        }
        if body.champion_share != 0 && !(50..=100).contains(&body.champion_share) {
            return Err(bad("The champion's share is 50% to 100% of the prizes."));
        }
        if !body.item_prize.is_empty()
            && !snook_legend_proto::shop::item(&body.item_prize)
                .is_some_and(|i| i.price == snook_legend_proto::shop::Price::Earned)
        {
            return Err(bad("The item prize must be an item that is only won."));
        }
        let size = if format == "knockout" { body.size } else { 0 };
        (1, format, size, body.rated, body.entry_fee, body.prize)
    };
    let tournament = !kind.is_boost();
    let champion_share = if tournament { body.champion_share } else { 0 };
    let item_prize = if tournament { body.item_prize.as_str() } else { "" };
    if server.store.events_after(t).await?.len() >= MAX_OPEN_EVENTS {
        return Err(bad("Too many events are running or coming. Call some off first."));
    }
    let new = crate::store::NewEvent {
        kind: kind.code(),
        title: &title,
        multiplier,
        starts_at,
        ends_at,
        format,
        size,
        rated,
        rules: body.rules,
        entry_fee,
        prize,
        champion_share,
        item_prize,
        created_by: s.user.id,
        at: t,
    };
    let id = server.store.add_event(&new).await?;
    crate::events::reload(&server).await;
    let what = if kind.is_boost() {
        format!("{} ×{multiplier}", kind.code())
    } else {
        format!("{format} tournament, {} rules", body.rules.code())
    };
    log(&server, &s, "event", &title, &format!("{what}, {} – {}", utc(starts_at), utc(ends_at)))
        .await;
    Ok(Json(json!({ "ok": true, "id": id })))
}

/// Calls an event off: a boost stops, a tournament not started yet is
/// called off with every fee back.
async fn cancel_event(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    let event = server
        .store
        .recent_events(1000)
        .await?
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "No such event."))?;
    if event.ends_at <= now() {
        return Err(err(StatusCode::BAD_REQUEST, "That event is over."));
    }
    if !server.store.cancel_event(id, now()).await? {
        return Err(err(StatusCode::BAD_REQUEST, "That event was called off already."));
    }
    crate::events::reload(&server).await;
    log(&server, &s, "event-cancel", &event.title, &format!("#{id}")).await;
    Ok(Json(json!({ "ok": true })))
}

/// Removes a note about a player: one's own, or any (administrators).
async fn remove_note(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    let author = (s.user.role != Role::Admin).then_some(s.user.id);
    if !server.store.remove_staff_note(id, author).await? {
        return Err(err(StatusCode::NOT_FOUND, "No such note (or not yours)."));
    }
    log(&server, &s, "note-remove", &format!("note {id}"), "").await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct TextBody {
    text: String,
}

async fn announce(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(body): Json<TextBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    let text = clean_text(&body.text, snook_legend_proto::MAX_CHAT_LEN);
    if text.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "Write a message."));
    }
    // Players named with "@" are mentioned (their lines light up).
    let known = crate::moderation::known_mentions(&server, &text).await;
    server.with(|l| l.announce_mentioning(&text, &known));
    log(&server, &s, "announce", "lounge", &text).await;
    Ok(Json(json!({ "ok": true })))
}

async fn clear_chat(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    server.with(|l| l.clear_chat(&s.user.username));
    log(&server, &s, "clear-chat", "lounge", "").await;
    Ok(Json(json!({ "ok": true })))
}

async fn motd(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(body): Json<TextBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    // Kept like the other live settings (it survives a restart).
    let text = json!(clean_text(&body.text, 300));
    crate::settings::set(&server, crate::settings::MOTD, Some(&text), s.user.id)
        .await
        .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;
    log(&server, &s, "motd", "lounge", &clean_text(&body.text, 300)).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct TestBody {
    test: bool,
}

/// Marks a payment as made with the provider's test keys (no real money),
/// or back: test payments stay out of the revenue.
async fn payment_test(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<TestBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    if !server.store.set_payment_test(&id, body.test).await? {
        return Err(err(StatusCode::NOT_FOUND, "No such payment."));
    }
    let what = if body.test { "test" } else { "real" };
    log(&server, &s, "payment", &id, what).await;
    Ok(Json(json!({ "ok": true })))
}

/// A stuck Microsoft Store purchase is sent to Microsoft again (with the
/// same tracking id: if it was consumed, Microsoft says so and the coins are
/// credited; if not, it is consumed now). Never credits anything by itself.
async fn msstore_retry(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(tracking): Path<String>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    match server.store.ms_reopen(&tracking, now()).await {
        Ok(true) => {}
        Ok(false) => return Err(err(StatusCode::NOT_FOUND, "No stuck purchase with this id.")),
        Err(_) => {
            return Err(err(
                StatusCode::CONFLICT,
                "Another request is already receiving this purchase.",
            ));
        }
    }
    crate::msstore::retry_due(&server, now()).await;
    log(&server, &s, "msstore-retry", &tracking, "").await;
    Ok(Json(json!({ "ok": true })))
}

/// Longest note on the lounge's board.
pub const MAX_NOTE_LEN: usize = 400;

async fn board(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    let notes = server.store.board_notes(now()).await?;
    let notes: Vec<_> = notes
        .into_iter()
        .map(|n| {
            json!({ "id": n.id, "text": n.text, "author": n.author,
                    "created_at": n.created_at, "expires_at": n.expires_at,
                    "kind": n.kind, "pinned": n.pinned, "action": n.action })
        })
        .collect();
    Ok(Json(json!({ "notes": notes })))
}

#[derive(Deserialize)]
struct NoteBody {
    text: String,
    /// Taken down after this many days (none: until removed).
    #[serde(default)]
    days: Option<u32>,
    /// `notice`, `event` or `rule`.
    #[serde(default)]
    kind: String,
    #[serde(default)]
    pinned: bool,
    /// A button under the note (`tournaments`, `shop`, `vip`, `missions`).
    #[serde(default)]
    action: Option<String>,
}

async fn pin_note(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(body): Json<NoteBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    let text = clean_text(&body.text, MAX_NOTE_LEN);
    if text.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "Write a note."));
    }
    let t = now();
    let expires = body.days.filter(|d| (1..=366).contains(d)).map(|d| t + d as i64 * 86_400);
    let kind = snook_legend_proto::NoteKind::from_code(&body.kind).code();
    let action = body
        .action
        .as_deref()
        .filter(|a| !a.is_empty())
        .map(|a| {
            snook_legend_proto::NoteAction::from_code(a)
                .map(|a| a.code())
                .ok_or_else(|| err(StatusCode::BAD_REQUEST, "Unknown button."))
        })
        .transpose()?;
    let note = crate::store::NewNote {
        text: &text,
        author_id: s.user.id,
        at: t,
        expires_at: expires,
        kind,
        pinned: body.pinned,
        action,
    };
    let id = server.store.add_board_note(&note).await?;
    server.refresh_board().await;
    log(&server, &s, "pin", "board", &text).await;
    Ok(Json(json!({ "ok": true, "id": id })))
}

async fn unpin_note(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    if s.user.role != Role::Admin {
        let notes = server.store.board_notes(now()).await?;
        let author = notes.iter().find(|n| n.id == id).map(|n| n.author.clone());
        if let Some(author) = author
            && let Some(u) = server.store.user_by_name(&author).await?
            && !crate::social::may_moderate(s.user.role, u.role)
            && u.id != s.user.id
        {
            return Err(err(StatusCode::FORBIDDEN, "Only administrators can act on staff."));
        }
    }
    if !server.store.remove_board_note(id).await? {
        return Err(err(StatusCode::NOT_FOUND, "No such note."));
    }
    server.refresh_board().await;
    log(&server, &s, "unpin", "board", &format!("note {id}")).await;
    Ok(Json(json!({ "ok": true })))
}

/// The "looking for a game" board (notes of players online now).
async fn seeks(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    Ok(Json(json!({ "notes": server.with(|l| l.seeks()) })))
}

async fn remove_seek(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    let role = server.store.user_by_name(&name).await?.map(|u| u.role).unwrap_or_default();
    if !crate::social::may_moderate(s.user.role, role) {
        return Err(err(StatusCode::FORBIDDEN, "Only administrators can act on staff."));
    }
    if !server.with(|l| l.remove_seek_of(&name)) {
        return Err(err(StatusCode::NOT_FOUND, "That player has no note on the board."));
    }
    log(&server, &s, "remove-seek", &name, "").await;
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::flag_outliers;
    use crate::store::AimRow;

    #[test]
    fn times_read_as_dates() {
        assert_eq!(super::utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(super::utc(1_795_291_200), "2026-11-21 20:00 UTC");
        assert_eq!(super::utc(951_782_400), "2000-02-29 00:00 UTC");
    }

    #[test]
    fn only_far_outliers_are_flagged() {
        // A tier of honest players around 20–35% on hard shots, and one at 80%.
        let mut rows: Vec<AimRow> = (0..60)
            .map(|k| AimRow {
                user_id: k,
                username: format!("p{k}"),
                rating: 1550,
                attempts: 100,
                made: 20 + (k as u32 % 16),
            })
            .collect();
        rows.push(AimRow {
            user_id: 99,
            username: "aimbot".into(),
            rating: 1560,
            attempts: 120,
            made: 96,
        });
        // A strong player in a higher tier is compared with that tier, not this one.
        rows.push(AimRow {
            user_id: 100,
            username: "master".into(),
            rating: 1750,
            attempts: 80,
            made: 30,
        });
        let flagged = flag_outliers(&rows);
        assert_eq!(flagged.len(), 1, "{flagged:?}");
        assert_eq!(flagged[0]["username"], "aimbot");
    }
}
