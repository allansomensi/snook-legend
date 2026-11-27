//! A forgotten password, and the recovery e-mail confirmed.
//!
//! The website's pages call these from the browser (CORS: only the
//! website's origin, [`crate::Config::site_url`], and the portal's):
//!
//! | Request | What it does |
//! |---|---|
//! | `POST /api/account/forgot` `{login, lang}` | e-mails a link to choose a new password to the account's confirmed recovery e-mail (`login`: a username or an e-mail). Always "ok": whether an account matches is never told. |
//! | `POST /api/account/reset` `{token, password}` | the link's token and the new password: the password changes, every session ends (the game's, the portal's, the devices kept signed in), and the address is told |
//! | `POST /api/account/confirm-email` `{token}` | confirms the recovery e-mail the link was sent to |
//!
//! Errors are `{error, code}`, `code` one of `bad_request`, `weak_password`,
//! `invalid_token` (expired, used, or for an e-mail no longer the
//! account's), `rate_limited`, `unavailable` (no e-mail on this server, or
//! the database). Links work once, and only an account's latest of each
//! kind: a password link for [`RESET_MINUTES`], a confirmation for
//! [`CONFIRM_DAYS`]. A token is 32 random bytes; only its hash is kept.
//!
//! The game asks for the confirmation itself (setting the recovery e-mail,
//! or "Send again", see [`send_confirmation`]); its "Forgot your password?"
//! opens the website's page (another server's `/forgot-password` leads to
//! its website's).

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, DefaultBodyLimit, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use snook_legend_proto::{MAX_PASSWORD_BYTES, valid_password};

use crate::mail::{self, Lang};
use crate::store::{TokenPurpose, User};
use crate::{Server, new_token, now, token_hash};

/// How long a link to choose a new password works.
pub const RESET_MINUTES: i64 = 60;
/// How long a link to confirm the recovery e-mail works.
pub const CONFIRM_DAYS: i64 = 7;
/// Accounts one e-mail can recover at a time (one link each).
pub const MAX_ACCOUNTS_PER_EMAIL: u32 = 5;
/// The largest body these requests read.
const MAX_BODY: usize = 4 * 1024;

pub fn router(server: Arc<Server>) -> Router<Arc<Server>> {
    Router::new()
        .route("/api/account/forgot", post(forgot))
        .route("/api/account/reset", post(reset))
        .route("/api/account/confirm-email", post(confirm))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(middleware::from_fn_with_state(server, cors))
        .route("/forgot-password", get(forgot_page))
}

/// The origin (`scheme://host[:port]`) of an address.
fn origin_of(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once("://")?;
    let host_len = rest.find('/').unwrap_or(rest.len());
    (!scheme.is_empty() && host_len > 0).then(|| &url[..scheme.len() + 3 + host_len])
}

/// The website's pages (and the portal's) call these from the browser:
/// only their origins are answered. No cookies are involved.
async fn cors(State(server): State<Arc<Server>>, req: Request, next: Next) -> Response {
    let origin = req.headers().get(header::ORIGIN).cloned();
    let allowed = origin.as_ref().is_some_and(|o| {
        let o = o.as_bytes();
        let site = origin_of(&server.config.site_url);
        let portal = server.config.admin_origin.as_deref();
        [site, portal].into_iter().flatten().any(|a| a.as_bytes().eq_ignore_ascii_case(o))
    });
    let preflight = req.method() == Method::OPTIONS;
    let mut res = if preflight {
        if allowed { StatusCode::NO_CONTENT } else { StatusCode::FORBIDDEN }.into_response()
    } else {
        next.run(req).await
    };
    let h = res.headers_mut();
    h.insert(header::VARY, HeaderValue::from_static("Origin"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    if let (true, Some(origin)) = (allowed, origin) {
        h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        if preflight {
            h.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("POST"));
            h.insert(
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("content-type"),
            );
            h.insert(header::ACCESS_CONTROL_MAX_AGE, HeaderValue::from_static("600"));
        }
    }
    res
}

fn fail(status: StatusCode, code: &str, error: &str) -> Response {
    (status, Json(json!({ "error": error, "code": code }))).into_response()
}

fn unavailable() -> Response {
    fail(StatusCode::SERVICE_UNAVAILABLE, "unavailable", "Try again in a moment.")
}

fn invalid_token() -> Response {
    fail(StatusCode::BAD_REQUEST, "invalid_token", "This link expired or was already used.")
}

/// A token as the links carry it: 64 hexadecimal digits.
fn token_ok(t: &str) -> bool {
    t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The link a token is sent in: the website's page in the e-mail's
/// language, the token after `#` (never sent to any server by the browser,
/// nor kept in logs on the way).
fn link(server: &Server, path: &str, token: &str) -> String {
    format!("{}{path}#token={token}", server.config.site_url.trim_end_matches('/'))
}

#[derive(Deserialize)]
struct ForgotBody {
    login: String,
    #[serde(default)]
    lang: String,
}

/// `POST /api/account/forgot`: e-mails a link to choose a new password.
async fn forgot(
    State(server): State<Arc<Server>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<ForgotBody>,
) -> Response {
    if !server.mail.enabled() {
        return fail(
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "This server sends no e-mail.",
        );
    }
    let login = body.login.trim().to_lowercase();
    if login.is_empty() || login.len() > 254 {
        return fail(StatusCode::BAD_REQUEST, "bad_request", "Type your username or e-mail.");
    }
    let ip = server.client_ip(peer, &headers).to_string();
    if !server.auth().try_mail_request(&ip, &login) {
        return fail(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "Too many requests. Try again later.",
        );
    }
    let lang = Lang::parse(&body.lang);
    // Answered at once, the same way whether an account matches or not:
    // the lookup and the e-mail happen after (the time taken tells
    // nothing either).
    tokio::spawn(async move {
        let users = if login.contains('@') {
            match server.store.users_by_confirmed_email(&login, MAX_ACCOUNTS_PER_EMAIL).await {
                Ok(u) => u,
                Err(e) => return tracing::error!("password reset lookup: {e}"),
            }
        } else {
            match account_with_confirmed_email(&server, &login).await {
                Ok(u) => u.into_iter().collect(),
                Err(e) => return tracing::error!("password reset lookup: {e}"),
            }
        };
        for user in users {
            let Some(email) = user.recovery_email.clone() else { continue };
            if !server.auth().try_mail_to(user.id) {
                tracing::info!(user = user.id, "password reset: too many e-mails for this account");
                continue;
            }
            let token = new_token();
            let t = now();
            let stored = server
                .store
                .replace_account_token(
                    &token_hash(&token),
                    user.id,
                    TokenPurpose::PasswordReset,
                    &email,
                    t,
                    t + RESET_MINUTES * 60,
                )
                .await;
            if let Err(e) = stored {
                tracing::error!("password reset token: {e}");
                continue;
            }
            let url = link(&server, lang.reset_path(), &token);
            let mail = mail::password_reset(lang, &user.username, &url, RESET_MINUTES);
            tracing::info!(user = user.id, "password reset link sent");
            server.mail.send(&email, mail).await;
        }
    });
    (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
}

/// The account named `name`, if it can be recovered: not deleted, with a
/// recovery e-mail that was confirmed.
async fn account_with_confirmed_email(
    server: &Server,
    name: &str,
) -> crate::store::StoreResult<Option<User>> {
    let Some(user) = server.store.user_by_name(name).await? else { return Ok(None) };
    if user.deleted_at.is_some() {
        return Ok(None);
    }
    let confirmed = server.store.confirmed_email(user.id).await?;
    Ok(email_confirmed(&user, confirmed.as_deref()).then_some(user))
}

/// Whether the account's recovery e-mail is the one it confirmed.
pub fn email_confirmed(user: &User, confirmed: Option<&str>) -> bool {
    match (&user.recovery_email, confirmed) {
        (Some(e), Some(c)) => e.eq_ignore_ascii_case(c),
        _ => false,
    }
}

#[derive(Deserialize)]
struct ResetBody {
    token: String,
    password: String,
}

/// `POST /api/account/reset`: a new password, with the e-mailed link's token.
async fn reset(
    State(server): State<Arc<Server>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<ResetBody>,
) -> Response {
    let token = body.token.trim();
    if !token_ok(token) {
        return invalid_token();
    }
    // The new password first: a weak one must not use the link up.
    if !valid_password(&body.password) || body.password.len() > MAX_PASSWORD_BYTES {
        return fail(
            StatusCode::BAD_REQUEST,
            "weak_password",
            "Passwords need at least 8 characters (not only digits).",
        );
    }
    let ip = server.client_ip(peer, &headers).to_string();
    if !server.auth().try_token(&ip) {
        return fail(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "Too many requests. Try again later.",
        );
    }
    let t = now();
    let taken =
        server.store.take_account_token(&token_hash(token), TokenPurpose::PasswordReset, t).await;
    let (user_id, email) = match taken {
        Ok(Some(found)) => found,
        Ok(None) => return invalid_token(),
        Err(e) => {
            tracing::error!("password reset: {e}");
            return unavailable();
        }
    };
    let user = match server.store.user_by_id(user_id).await {
        Ok(Some(u)) if u.deleted_at.is_none() => u,
        Ok(_) => return invalid_token(),
        Err(e) => {
            tracing::error!("password reset: {e}");
            return unavailable();
        }
    };
    // Sent to the address that is still the account's, confirmed.
    let confirmed = server.store.confirmed_email(user.id).await.ok().flatten();
    if !email_confirmed(&user, confirmed.as_deref())
        || !user.recovery_email.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(&email))
    {
        return invalid_token();
    }
    let Some(hash) = server.hash_password(&body.password).await else { return unavailable() };
    if let Err(e) = server.store.set_password(user.id, &hash).await {
        tracing::error!("password reset: {e}");
        return unavailable();
    }
    signed_out_everywhere(&server, &user, "Your password was changed. Sign in again.").await;
    tracing::info!(user = user.id, "password reset by e-mail");
    // The password's own limits start over: the owner is back.
    server.auth().forget_failures(&user.username);
    let lang = lang_of(&headers);
    notify_password_changed(&server, &user, lang);
    (StatusCode::OK, Json(json!({ "ok": true, "username": user.username }))).into_response()
}

/// Ends every session of the account (the game's, the portal's, the
/// devices kept signed in) and takes it out of the lounge.
pub async fn signed_out_everywhere(server: &Server, user: &User, why: &str) {
    if let Err(e) = server.store.delete_user_sessions(user.id).await {
        tracing::error!("signing {} out: {e}", user.username);
    }
    server.with(|l| l.kick(&user.username, why));
}

/// Tells the account's confirmed recovery e-mail that its password changed
/// (sent in the background).
pub fn notify_password_changed(server: &Arc<Server>, user: &User, lang: Lang) {
    if !server.mail.enabled() {
        return;
    }
    let server = server.clone();
    let user = user.clone();
    tokio::spawn(async move {
        let confirmed = server.store.confirmed_email(user.id).await.ok().flatten();
        if !email_confirmed(&user, confirmed.as_deref()) {
            return;
        }
        let Some(email) = user.recovery_email.as_deref() else { return };
        server.mail.send(email, mail::password_changed(lang, &user.username)).await;
    });
}

/// The language of the page that asked (`Accept-Language`'s first).
fn lang_of(headers: &HeaderMap) -> Lang {
    let first = headers
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .unwrap_or("");
    Lang::parse(first.split(';').next().unwrap_or(""))
}

#[derive(Deserialize)]
struct ConfirmBody {
    token: String,
}

/// `POST /api/account/confirm-email`: the recovery e-mail is the player's.
async fn confirm(
    State(server): State<Arc<Server>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<ConfirmBody>,
) -> Response {
    let token = body.token.trim();
    if !token_ok(token) {
        return invalid_token();
    }
    let ip = server.client_ip(peer, &headers).to_string();
    if !server.auth().try_token(&ip) {
        return fail(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "Too many requests. Try again later.",
        );
    }
    let taken = server
        .store
        .take_account_token(&token_hash(token), TokenPurpose::ConfirmEmail, now())
        .await;
    let (user_id, email) = match taken {
        Ok(Some(found)) => found,
        Ok(None) => return invalid_token(),
        Err(e) => {
            tracing::error!("e-mail confirmation: {e}");
            return unavailable();
        }
    };
    let user = match server.store.user_by_id(user_id).await {
        Ok(Some(u)) if u.deleted_at.is_none() => u,
        Ok(_) => return invalid_token(),
        Err(e) => {
            tracing::error!("e-mail confirmation: {e}");
            return unavailable();
        }
    };
    // An address the player has since replaced is not confirmed.
    if !user.recovery_email.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(&email)) {
        return invalid_token();
    }
    if let Err(e) = server.store.confirm_email(user.id, &email).await {
        tracing::error!("e-mail confirmation: {e}");
        return unavailable();
    }
    tracing::info!(user = user.id, "recovery e-mail confirmed");
    // A game open on the account's settings shows it at once.
    let payment = server.store.payment_email(user.id).await.ok().flatten();
    let msg = crate::account::account_msg(&server, &user, payment).await;
    server.with(|l| l.notify(&user.username, msg));
    (StatusCode::OK, Json(json!({ "ok": true, "username": user.username, "email": email })))
        .into_response()
}

/// Sends `user` the link that confirms `email` as their recovery e-mail.
/// Returns why not, when it can't be sent (in the game's words).
pub async fn send_confirmation(
    server: &Server,
    user: &User,
    email: &str,
    lang: Lang,
) -> Result<(), &'static str> {
    if !server.mail.enabled() {
        return Err("This server doesn't send e-mail.");
    }
    if !server.auth().try_mail_to(user.id) {
        return Err("Too many e-mails. Try again in an hour.");
    }
    let token = new_token();
    let t = now();
    server
        .store
        .replace_account_token(
            &token_hash(&token),
            user.id,
            TokenPurpose::ConfirmEmail,
            email,
            t,
            t + CONFIRM_DAYS * 86_400,
        )
        .await
        .map_err(|e| {
            tracing::error!("e-mail confirmation token: {e}");
            "Something went wrong. Please try again."
        })?;
    let url = link(server, lang.confirm_path(), &token);
    let mail = mail::confirm_email(lang, &user.username, email, &url, CONFIRM_DAYS);
    if server.mail.send(email, mail).await {
        tracing::info!(user = user.id, "recovery e-mail confirmation sent");
        Ok(())
    } else {
        Err("The e-mail could not be sent. Try again in a moment.")
    }
}

#[derive(Deserialize)]
struct PageQuery {
    #[serde(default)]
    lang: String,
}

/// `GET /forgot-password?lang=pt`: the website's page for a forgotten
/// password (the game of another server than the official one links here).
async fn forgot_page(State(server): State<Arc<Server>>, Query(q): Query<PageQuery>) -> Response {
    let site = server.config.site_url.trim_end_matches('/');
    Redirect::to(&format!("{site}{}", Lang::parse(&q.lang).reset_path())).into_response()
}

/// What the account's recovery e-mail looks like to the game: whether it
/// still waits for its link (only on a server that sends e-mail).
pub async fn needs_confirmation(server: &Server, user: &User) -> bool {
    if !server.mail.enabled() || user.recovery_email.is_none() {
        return false;
    }
    let confirmed = server.store.confirmed_email(user.id).await.ok().flatten();
    !email_confirmed(user, confirmed.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins() {
        assert_eq!(origin_of("https://snooklegend.app"), Some("https://snooklegend.app"));
        assert_eq!(origin_of("https://snooklegend.app/"), Some("https://snooklegend.app"));
        assert_eq!(origin_of("http://localhost:3000/x/y"), Some("http://localhost:3000"));
        assert_eq!(origin_of("snooklegend.app"), None);
        assert_eq!(origin_of("https:///x"), None);
    }

    #[test]
    fn tokens() {
        assert!(token_ok(&new_token()));
        assert!(!token_ok("abc"));
        assert!(!token_ok(&"g".repeat(64)));
    }
}
