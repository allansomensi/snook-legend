//! Signing in to the staff portal, in two steps, and staying signed in.
//!
//! Every moderator and administrator signs in with their password and then
//! a code from an authenticator app (TOTP, see [`crate::totp`]); the first
//! time, the portal shows the secret (and its QR code) to add to the app,
//! and the code that proves it was added turns two-step verification on,
//! with ten single-use recovery codes for a lost phone.
//!
//! 1. `POST /admin/api/login` `{username, password, remember}` →
//!    `{step: "code", ticket}`, or the first time
//!    `{step: "setup", ticket, secret, uri}`. The ticket lasts
//!    [`PENDING_SECS`] and takes [`TICKET_TRIES`] wrong codes.
//! 2. `POST /admin/api/login/code` `{ticket, code}` (six digits, or a
//!    recovery code) → `{token, user}` (a portal session of eight hours),
//!    with `device` and `device_id` when `remember` was asked (a token that
//!    keeps this device signed in: see below), `recovery_codes` on the
//!    first time, and `codes_left` when a recovery code was used.
//!
//! A device kept signed in trades its token for a fresh portal session
//! (`POST /admin/api/login/device` `{device}` → `{token, user, device_id}`)
//! without the password or the code; each trade moves its end
//! [`DEVICE_DAYS`] days on, so only a device unused that long asks again.
//! The staff member sees their devices and signs them out in the portal's
//! settings (`/admin/api/security`); a new password (in the game or by
//! e-mail), a ban, or two-step verification turned off by an administrator
//! signs every one of them out.
//!
//! In the game, a staff member with two-step verification signs in with
//! the code too ([`game_sign_in`]); before they turned it on (in the
//! portal) the password alone still lets them play.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Json;
use axum::Router;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::{delete, get, post};
use serde::Deserialize;
use serde_json::{Value, json};
use snook_legend_proto::{AUTH_CODE_REQUIRED, AUTH_CODE_WRONG};

use super::{ApiError, ApiResult, Staff, err, log, require_admin, staff, user_json};
use crate::store::{Device, Totp, User};
use crate::{Server, new_token, now, token_hash, totp};

/// How long a sign-in waits for its code.
pub const PENDING_SECS: u64 = 10 * 60;
/// Wrong codes a sign-in takes before the password is asked again.
pub const TICKET_TRIES: u8 = 5;
/// Sign-ins waiting for their code, at most (more are turned away).
const MAX_PENDING: usize = 2_000;
/// A device kept signed in stays so this long after it was last used.
pub const DEVICE_DAYS: i64 = 30;
/// How much of a device's token hash is its id (shown to its owner).
const DEVICE_ID_LEN: usize = 16;

/// A sign-in between the password and the code.
struct Pending {
    user_id: i64,
    /// The first time: the secret being added to the app.
    setup_secret: Option<String>,
    remember: bool,
    label: String,
    ip: String,
    expires: Instant,
    tries: u8,
}

/// The sign-ins waiting for their code, by the hash of their ticket. Kept
/// in memory: a server that restarts meanwhile asks for the password again.
#[derive(Default)]
pub struct PendingLogins(Mutex<HashMap<String, Pending>>);

impl PendingLogins {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Pending>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Keeps a sign-in; its ticket, or `None` when too many are waiting.
    fn add(&self, p: Pending) -> Option<String> {
        let mut map = self.lock();
        let now = Instant::now();
        map.retain(|_, p| p.expires > now);
        if map.len() >= MAX_PENDING {
            return None;
        }
        // One at a time per account: a new sign-in replaces the last.
        map.retain(|_, q| q.user_id != p.user_id);
        let ticket = new_token();
        map.insert(token_hash(&ticket), p);
        Some(ticket)
    }
}

pub fn routes() -> Router<Arc<Server>> {
    Router::new()
        .route("/admin/api/login/code", post(login_code))
        .route("/admin/api/login/device", post(login_device))
        .route("/admin/api/security", get(security))
        .route("/admin/api/security/recovery-codes", post(new_recovery_codes))
        .route("/admin/api/security/devices/{id}", delete(sign_device_out))
        .route("/admin/api/security/sign-out-everywhere", post(sign_out_everywhere))
        .route("/admin/api/users/{name}/two-step/reset", post(reset_two_step))
}

/// The browser and system of a `User-Agent`, in a few words
/// ("Chrome · Android"), to tell the devices apart.
pub fn device_label(user_agent: &str) -> String {
    let ua = user_agent;
    let os = if ua.contains("iPhone") {
        "iPhone"
    } else if ua.contains("iPad") {
        "iPad"
    } else if ua.contains("Android") {
        "Android"
    } else if ua.contains("CrOS") {
        "ChromeOS"
    } else if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("Macintosh") || ua.contains("Mac OS") {
        "Mac"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        ""
    };
    let browser = if ua.contains("Edg/") || ua.contains("EdgA/") || ua.contains("EdgiOS/") {
        "Edge"
    } else if ua.contains("OPR/") || ua.contains("Opera") {
        "Opera"
    } else if ua.contains("SamsungBrowser/") {
        "Samsung Internet"
    } else if ua.contains("Firefox/") || ua.contains("FxiOS/") {
        "Firefox"
    } else if ua.contains("CriOS/") || ua.contains("Chrome/") {
        "Chrome"
    } else if ua.contains("Safari/") {
        "Safari"
    } else {
        ""
    };
    match (browser, os) {
        ("", "") => "Browser".into(),
        ("", os) => os.into(),
        (b, "") => b.into(),
        (b, os) => format!("{b} · {os}"),
    }
}

/// The second step of [`super::login`]: the password was right and the
/// account is staff. Answers with the ticket (and, the first time, the
/// secret to add to the app).
pub async fn after_password(
    server: &Server,
    user: &User,
    remember: bool,
    headers: &HeaderMap,
    ip: &str,
) -> ApiResult {
    let enabled = server.store.totp(user.id).await?.is_some();
    let secret = (!enabled).then(totp::new_secret);
    let label = device_label(
        headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or_default(),
    );
    let pending = Pending {
        user_id: user.id,
        setup_secret: secret.clone(),
        remember,
        label,
        ip: ip.to_string(),
        expires: Instant::now() + Duration::from_secs(PENDING_SECS),
        tries: 0,
    };
    let Some(ticket) = server.staff_logins.add(pending) else {
        return Err(err(
            StatusCode::SERVICE_UNAVAILABLE,
            "The server is busy. Try again in a moment.",
        ));
    };
    Ok(Json(match secret {
        None => json!({ "step": "code", "ticket": ticket }),
        Some(secret) => json!({
            "step": "setup",
            "ticket": ticket,
            "uri": totp::uri(&secret, &user.username),
            "secret": secret,
        }),
    }))
}

/// How a code was accepted.
enum CodeOk {
    App,
    /// A recovery code, with how many are left.
    Recovery(u32),
}

/// Checks a code of the account's app (or one of its recovery codes) and
/// uses it up. `Err` carries what to answer.
async fn check_code(
    server: &Server,
    user_id: i64,
    totp: &Totp,
    code: &str,
    recovery_allowed: bool,
) -> Result<CodeOk, &'static str> {
    if !server.auth().try_code(user_id) {
        return Err("Too many wrong codes. Wait a few minutes and try again.");
    }
    let t = now();
    if totp::looks_like_totp(code) {
        let Some(step) = totp::verify(&totp.secret, code, t) else { return Err(AUTH_CODE_WRONG) };
        match server.store.use_totp_step(user_id, step).await {
            Ok(true) => {}
            Ok(false) => return Err(AUTH_CODE_WRONG),
            Err(e) => {
                tracing::error!("two-step verification: {e}");
                return Err("The server is busy. Try again in a moment.");
            }
        }
        server.auth().code_ok(user_id);
        return Ok(CodeOk::App);
    }
    let Some(normal) = totp::normalize_recovery(code).filter(|_| recovery_allowed) else {
        return Err(AUTH_CODE_WRONG);
    };
    match server.store.use_recovery_code(user_id, &token_hash(&normal)).await {
        Ok(true) => {
            server.auth().code_ok(user_id);
            let left = server.store.recovery_codes_left(user_id).await.unwrap_or(0);
            tracing::warn!(user = user_id, left, "a recovery code was used");
            Ok(CodeOk::Recovery(left))
        }
        Ok(false) => Err(AUTH_CODE_WRONG),
        Err(e) => {
            tracing::error!("recovery code: {e}");
            Err("The server is busy. Try again in a moment.")
        }
    }
}

/// A staff member signing in to the game: with two-step verification on,
/// the code of their app (or a recovery code) is asked too. `Err`: what
/// the game is told ([`AUTH_CODE_REQUIRED`] makes it show the code's field).
pub async fn game_sign_in(server: &Server, user: &User, code: Option<&str>) -> Result<(), String> {
    let totp = match server.store.totp(user.id).await {
        Ok(Some(t)) => t,
        // Not turned on yet (it is, the first time they open the portal).
        Ok(None) => return Ok(()),
        Err(e) => {
            tracing::error!("two-step verification: {e}");
            return Err("The server is busy. Try again in a moment.".into());
        }
    };
    let code = code.map(str::trim).unwrap_or_default();
    if code.is_empty() {
        return Err(AUTH_CODE_REQUIRED.into());
    }
    match check_code(server, user.id, &totp, code, true).await {
        Ok(_) => Ok(()),
        Err(why) => Err(why.into()),
    }
}

/// Recovery codes as the account keeps them (hashes).
fn recovery_hashes(codes: &[String]) -> Vec<String> {
    codes.iter().filter_map(|c| totp::normalize_recovery(c)).map(|c| token_hash(&c)).collect()
}

/// A device's id as its owner sees it.
fn device_id(token_hash: &str) -> String {
    token_hash.chars().take(DEVICE_ID_LEN).collect()
}

/// Keeps this device signed in: its token (given once) and id.
async fn remember_device(
    server: &Server,
    user_id: i64,
    label: &str,
) -> Result<(String, String), ApiError> {
    let token = new_token();
    let hash = token_hash(&token);
    let t = now();
    server
        .store
        .create_device(&Device {
            token_hash: hash.clone(),
            user_id,
            label: label.chars().take(60).collect(),
            created_at: t,
            last_used_at: t,
            expires_at: t + DEVICE_DAYS * 86_400,
        })
        .await?;
    Ok((token, device_id(&hash)))
}

#[derive(Deserialize)]
struct CodeBody {
    ticket: String,
    code: String,
}

/// `POST /admin/api/login/code`: the second step.
async fn login_code(State(server): State<Arc<Server>>, Json(body): Json<CodeBody>) -> ApiResult {
    let expired = || err(StatusCode::UNAUTHORIZED, "The sign-in took too long. Sign in again.");
    let key = token_hash(body.ticket.trim());
    // The sign-in, taken out while its code is checked (a second request
    // with the same ticket meanwhile finds nothing).
    let mut pending = {
        let mut map = server.staff_logins.lock();
        match map.remove(&key) {
            Some(p) if p.expires > Instant::now() => p,
            _ => return Err(expired()),
        }
    };
    let user = match server.store.user_by_id(pending.user_id).await? {
        Some(u) if u.role.is_staff() && !u.is_banned(now()) && u.deleted_at.is_none() => u,
        _ => return Err(err(StatusCode::FORBIDDEN, "This account has no access to the panel.")),
    };
    let code = body.code.trim();
    let mut first_time = None;
    let checked = match &pending.setup_secret {
        // The first time: the code proves the app has the secret.
        Some(secret) => {
            if !server.auth().try_code(user.id) {
                Err("Too many wrong codes. Wait a few minutes and try again.")
            } else {
                match totp::verify(secret, code, now()) {
                    Some(step) => {
                        server.auth().code_ok(user.id);
                        let codes = totp::new_recovery_codes();
                        server
                            .store
                            .enable_totp(user.id, secret, step, now(), &recovery_hashes(&codes))
                            .await?;
                        // Whatever was signed in before starts over.
                        server.store.delete_staff_sessions(user.id).await?;
                        first_time = Some(codes);
                        Ok(CodeOk::App)
                    }
                    None => Err(AUTH_CODE_WRONG),
                }
            }
        }
        None => match server.store.totp(user.id).await? {
            Some(t) => check_code(&server, user.id, &t, code, true).await,
            // Turned off by an administrator meanwhile.
            None => return Err(expired()),
        },
    };
    let ok = match checked {
        Ok(ok) => ok,
        Err(why) => {
            pending.tries += 1;
            let status = if why == AUTH_CODE_WRONG {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::TOO_MANY_REQUESTS
            };
            if pending.tries < TICKET_TRIES && why == AUTH_CODE_WRONG {
                server.staff_logins.lock().insert(key, pending);
            }
            return Err(err(status, why));
        }
    };
    let token = new_token();
    let t = now();
    server
        .store
        .create_session(
            &token_hash(&token),
            user.id,
            crate::store::SessionKind::Admin,
            t,
            t + super::ADMIN_SESSION,
        )
        .await?;
    let mut answer = json!({ "token": token, "user": user_json(&user, false) });
    if pending.remember {
        let (device, id) = remember_device(&server, user.id, &pending.label).await?;
        answer["device"] = json!(device);
        answer["device_id"] = json!(id);
    }
    let how = match (&first_time, &ok) {
        (Some(_), _) => " · two-step verification turned on",
        (None, CodeOk::Recovery(_)) => " · recovery code",
        _ => "",
    };
    if let CodeOk::Recovery(left) = ok {
        answer["codes_left"] = json!(left);
    }
    if let Some(codes) = first_time {
        answer["recovery_codes"] = json!(codes);
    }
    let s = Staff { user };
    let remembered = if pending.remember { " · kept signed in" } else { "" };
    log(&server, &s, "login", &s.user.username, &format!("{}{how}{remembered}", pending.ip)).await;
    Ok(Json(answer))
}

#[derive(Deserialize)]
struct DeviceBody {
    device: String,
}

/// `POST /admin/api/login/device`: a device kept signed in gets a fresh
/// portal session.
async fn login_device(
    State(server): State<Arc<Server>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<DeviceBody>,
) -> ApiResult {
    let gone = || err(StatusCode::UNAUTHORIZED, "Sign in again.");
    let device = body.device.trim();
    if device.len() != 64 || !device.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(gone());
    }
    let ip = server.client_ip(peer, &headers).to_string();
    if !server.auth().try_token(&ip) {
        return Err(err(StatusCode::TOO_MANY_REQUESTS, "Too many attempts. Wait a few minutes."));
    }
    let hash = token_hash(device);
    let t = now();
    let Some((user, _)) = server.store.device_user(&hash, t).await? else { return Err(gone()) };
    if !user.role.is_staff() || user.is_banned(t) || user.deleted_at.is_some() {
        let _ = server.store.delete_device(user.id, &hash).await;
        return Err(err(StatusCode::FORBIDDEN, "This account has no access to the panel."));
    }
    // A device is only kept by two-step verification.
    if server.store.totp(user.id).await?.is_none() {
        let _ = server.store.delete_device(user.id, &hash).await;
        return Err(gone());
    }
    server.store.touch_device(&hash, t, t + DEVICE_DAYS * 86_400).await?;
    let token = new_token();
    server
        .store
        .create_session(
            &token_hash(&token),
            user.id,
            crate::store::SessionKind::Admin,
            t,
            t + super::ADMIN_SESSION,
        )
        .await?;
    let answer =
        json!({ "token": token, "user": user_json(&user, false), "device_id": device_id(&hash) });
    let s = Staff { user };
    log(&server, &s, "login", &s.user.username, &format!("{ip} · device kept signed in")).await;
    Ok(Json(answer))
}

/// `GET /admin/api/security`: the staff member's own two-step verification
/// and devices.
async fn security(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    let s = staff(&server, &headers).await?;
    let t = now();
    let totp = server.store.totp(s.user.id).await?;
    let left = server.store.recovery_codes_left(s.user.id).await?;
    let devices: Vec<Value> = server
        .store
        .devices(s.user.id, t)
        .await?
        .iter()
        .map(|d| {
            json!({
                "id": device_id(&d.token_hash),
                "label": d.label,
                "created_at": d.created_at,
                "last_used_at": d.last_used_at,
                "expires_at": d.expires_at,
            })
        })
        .collect();
    Ok(Json(json!({
        "two_step_at": totp.map(|t| t.enabled_at),
        "recovery_codes_left": left,
        "devices": devices,
        "device_days": DEVICE_DAYS,
    })))
}

#[derive(Deserialize)]
struct NewCodesBody {
    code: String,
}

/// `POST /admin/api/security/recovery-codes`: ten new recovery codes (the
/// old ones stop working), with a code of the app.
async fn new_recovery_codes(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(body): Json<NewCodesBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    let Some(t) = server.store.totp(s.user.id).await? else {
        return Err(err(StatusCode::BAD_REQUEST, "Two-step verification is off."));
    };
    match check_code(&server, s.user.id, &t, body.code.trim(), false).await {
        Ok(_) => {}
        Err(why) if why == AUTH_CODE_WRONG => return Err(err(StatusCode::BAD_REQUEST, why)),
        Err(why) => return Err(err(StatusCode::TOO_MANY_REQUESTS, why)),
    }
    let codes = totp::new_recovery_codes();
    server.store.set_recovery_codes(s.user.id, &recovery_hashes(&codes)).await?;
    log(&server, &s, "recovery-codes", &s.user.username, "").await;
    Ok(Json(json!({ "recovery_codes": codes })))
}

/// `DELETE /admin/api/security/devices/{id}`: signs one of one's devices out.
async fn sign_device_out(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    let devices = server.store.devices(s.user.id, now()).await?;
    let Some(d) =
        devices.iter().find(|d| id.len() == DEVICE_ID_LEN && d.token_hash.starts_with(&id))
    else {
        return Err(err(StatusCode::NOT_FOUND, "No such device."));
    };
    server.store.delete_device(s.user.id, &d.token_hash).await?;
    log(&server, &s, "device-signed-out", &s.user.username, &d.label).await;
    Ok(Json(json!({ "ok": true })))
}

/// `POST /admin/api/security/sign-out-everywhere`: every portal session and
/// device of the staff member ends (this one too).
async fn sign_out_everywhere(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    let s = staff(&server, &headers).await?;
    server.store.delete_staff_sessions(s.user.id).await?;
    log(&server, &s, "signed-out-everywhere", &s.user.username, "").await;
    Ok(Json(json!({ "ok": true })))
}

/// `POST /admin/api/users/{name}/two-step/reset` (administrators): turns a
/// staff member's two-step verification off (a lost phone, no recovery
/// codes left) and signs them out of the portal everywhere; they set it up
/// again at their next sign-in.
async fn reset_two_step(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    let target = super::find(&server, &name).await?;
    if server.store.totp(target.id).await?.is_none() {
        return Err(err(StatusCode::BAD_REQUEST, "Two-step verification is off for this account."));
    }
    server.store.disable_totp(target.id).await?;
    server.store.delete_staff_sessions(target.id).await?;
    log(&server, &s, "two-step-reset", &target.username, "").await;
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::device_label;

    #[test]
    fn devices_are_named_by_browser_and_system() {
        let android = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Mobile Safari/537.36";
        assert_eq!(device_label(android), "Chrome · Android");
        let iphone = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Mobile/15E148 Safari/604.1";
        assert_eq!(device_label(iphone), "Safari · iPhone");
        let edge = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0";
        assert_eq!(device_label(edge), "Edge · Windows");
        let firefox =
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 14.1; rv:132.0) Gecko/20100101 Firefox/132.0";
        assert_eq!(device_label(firefox), "Firefox · Mac");
        assert_eq!(device_label(""), "Browser");
    }
}
