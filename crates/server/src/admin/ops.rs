//! The staff API for operations and the economy (see `docs/ADMIN-API.md`,
//! "Operations & economy"): live settings, promotions, official
//! tournaments, maintenance, auctions, the analytics, the CSV exports and
//! the calendar. Administrators change; moderators read (not money).

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::{Value, json};
use snook_legend_proto::clean_text;
use snook_legend_proto::shop::{self, Category};

use super::{ApiResult, EventBody, err, log, require_admin, staff, utc};
use crate::store::{EXPORT_PAGE, Export, NewAuction, NewPromotion};
use crate::{Server, analytics, auction, maintenance, now, promotions, settings};

pub fn router() -> Router<Arc<Server>> {
    Router::new()
        .route("/admin/api/settings", get(list_settings))
        .route("/admin/api/settings/{key}", post(set_setting))
        .route("/admin/api/settings/{key}/reset", post(reset_setting))
        .route("/admin/api/promotions", get(list_promotions).post(create_promotion))
        .route("/admin/api/promotions/{id}", post(edit_promotion))
        .route("/admin/api/promotions/{id}/cancel", post(cancel_promotion))
        .route("/admin/api/tournaments/official", get(list_official).post(create_official))
        .route("/admin/api/maintenance", get(list_maintenance).post(schedule_maintenance))
        .route("/admin/api/maintenance/{id}/cancel", post(cancel_maintenance))
        .route("/admin/api/auctions", get(list_auctions).post(create_auction))
        .route("/admin/api/auctions/{id}/cancel", post(cancel_auction))
        .route("/admin/api/analytics/coins", get(coins))
        .route("/admin/api/analytics/sellers", get(sellers))
        .route("/admin/api/analytics/countries", get(countries))
        .route("/admin/api/analytics/hours", get(hours))
        .route("/admin/api/analytics/vip", get(vip))
        .route("/admin/api/export/{kind}", get(export))
        .route("/admin/api/calendar", get(calendar))
}

fn bad(m: impl Into<String>) -> super::ApiError {
    err(StatusCode::BAD_REQUEST, m)
}

// Live settings.

async fn list_settings(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    Ok(Json(json!({ "settings": server.ops.settings.list(), "now": now() })))
}

#[derive(Deserialize)]
struct SettingBody {
    value: Value,
}

async fn set_setting(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(key): Path<String>,
    Json(body): Json<SettingBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    let before = server.ops.settings.get(&key);
    let value = settings::set(&server, &key, Some(&body.value), s.user.id).await.map_err(bad)?;
    log(&server, &s, "setting", &key, &format!("{before} → {value}")).await;
    Ok(Json(json!({ "ok": true, "key": key, "value": value })))
}

async fn reset_setting(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    let before = server.ops.settings.get(&key);
    let value = settings::set(&server, &key, None, s.user.id).await.map_err(bad)?;
    log(&server, &s, "setting-reset", &key, &format!("{before} → {value} (default)")).await;
    Ok(Json(json!({ "ok": true, "key": key, "value": value })))
}

// Promotions.

async fn list_promotions(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    let t = now();
    let list: Vec<Value> = server
        .store
        .promotions(200)
        .await?
        .into_iter()
        .map(|p| {
            let status = if p.cancelled_at.is_some() {
                "cancelled"
            } else if p.ends_at <= t {
                "ended"
            } else if p.starts_at <= t {
                "live"
            } else {
                "scheduled"
            };
            let mut v = serde_json::to_value(&p).unwrap_or_default();
            v["status"] = json!(status);
            v
        })
        .collect();
    Ok(Json(json!({
        "promotions": list,
        "categories": Category::ALL.iter().map(|c| c.code()).collect::<Vec<_>>(),
        "limits": {
            "max_percent": snook_legend_proto::ops::MAX_DISCOUNT,
            "max_days": promotions::MAX_LENGTH / 86_400,
            "max_title": promotions::MAX_TITLE,
        },
        "now": t,
    })))
}

#[derive(Deserialize)]
struct PromotionBody {
    title: String,
    /// "discount" or "featured".
    kind: String,
    #[serde(default)]
    percent: u32,
    #[serde(default)]
    codes: Vec<String>,
    #[serde(default)]
    categories: Vec<String>,
    starts_at: i64,
    ends_at: i64,
}

/// A promotion's terms once checked: title, percent, items, categories,
/// start.
type Checked = (String, u32, Vec<String>, Vec<String>, i64);

/// The promotion as it will be kept, or why it can't be.
fn checked(body: &PromotionBody, t: i64, editing: bool) -> Result<Checked, super::ApiError> {
    let title = clean_text(&body.title, promotions::MAX_TITLE);
    if title.is_empty() {
        return Err(bad("Give the promotion a title."));
    }
    let percent = match body.kind.as_str() {
        "discount" => {
            if !(1..=snook_legend_proto::ops::MAX_DISCOUNT).contains(&body.percent) {
                return Err(bad("A discount is 1% to 90% off."));
            }
            body.percent
        }
        "featured" => 0,
        _ => return Err(bad("Choose discount or featured.")),
    };
    let mut codes = Vec::new();
    for c in &body.codes {
        let Some(item) = shop::item(c.trim()) else {
            return Err(bad(format!("No such item: {c}.")));
        };
        if item.coins().is_none() && body.kind == "discount" {
            return Err(bad(format!("{c} isn't sold for coins.")));
        }
        codes.push(item.code.to_string());
    }
    let mut categories = Vec::new();
    for c in &body.categories {
        let Some(cat) = Category::parse(c.trim()) else {
            return Err(bad(format!("No such category: {c}.")));
        };
        categories.push(cat.code().to_string());
    }
    codes.dedup();
    categories.dedup();
    if codes.is_empty() && categories.is_empty() {
        return Err(bad("Choose items or categories."));
    }
    // "Now" in the form is a moment ago here.
    let starts_at =
        if body.starts_at >= t - 600 && body.starts_at < t { t } else { body.starts_at };
    if body.ends_at <= starts_at {
        return Err(bad("The promotion must end after it starts."));
    }
    if body.ends_at <= t || (!editing && starts_at < t) {
        return Err(bad("That time is already past."));
    }
    if body.ends_at - starts_at > promotions::MAX_LENGTH {
        return Err(bad("A promotion lasts a month at most."));
    }
    if starts_at > t + 365 * 86_400 {
        return Err(bad("Schedule promotions within a year."));
    }
    Ok((title, percent, codes, categories, starts_at))
}

async fn create_promotion(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(body): Json<PromotionBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    let t = now();
    let (title, percent, codes, categories, starts_at) = checked(&body, t, false)?;
    let open = server
        .store
        .promotions(500)
        .await?
        .iter()
        .filter(|p| p.cancelled_at.is_none() && p.ends_at > t)
        .count();
    if open >= promotions::MAX_OPEN {
        return Err(bad("Too many promotions are running or coming. Call some off first."));
    }
    let new = NewPromotion {
        title: &title,
        kind: &body.kind,
        percent,
        codes: &codes,
        categories: &categories,
        starts_at,
        ends_at: body.ends_at,
        by: s.user.id,
        at: t,
    };
    let id = server.store.add_promotion(&new).await?;
    promotions::reload(&server).await;
    let what = if percent > 0 { format!("{percent}% off") } else { "featured".into() };
    let on = [codes.join(" "), categories.join(" ")].join(" ");
    let when = format!("{} – {}", utc(starts_at), utc(body.ends_at));
    log(&server, &s, "promotion", &title, &format!("{what} on {}, {when}", on.trim())).await;
    Ok(Json(json!({ "ok": true, "id": id })))
}

async fn edit_promotion(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<PromotionBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    let t = now();
    let (title, percent, codes, categories, starts_at) = checked(&body, t, true)?;
    let new = NewPromotion {
        title: &title,
        kind: &body.kind,
        percent,
        codes: &codes,
        categories: &categories,
        starts_at,
        ends_at: body.ends_at,
        by: s.user.id,
        at: t,
    };
    if !server.store.update_promotion(id, &new).await? {
        return Err(err(StatusCode::NOT_FOUND, "No such promotion (or it was called off)."));
    }
    promotions::reload(&server).await;
    let what = if percent > 0 { format!("{percent}% off") } else { "featured".into() };
    log(
        &server,
        &s,
        "promotion-edit",
        &title,
        &format!("#{id}: {what}, {} – {}", utc(starts_at), utc(body.ends_at)),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

async fn cancel_promotion(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    if !server.store.cancel_promotion(id, now()).await? {
        return Err(err(
            StatusCode::NOT_FOUND,
            "No such promotion (or it was called off already).",
        ));
    }
    promotions::reload(&server).await;
    log(&server, &s, "promotion-cancel", &format!("promotion {id}"), "").await;
    Ok(Json(json!({ "ok": true })))
}

// Official tournaments.

/// The official tournaments: scheduled (the staff's events, the weekly
/// cup's next), open or running now with their prize pool, and the latest
/// finished.
async fn list_official(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    let t = now();
    let scheduled: Vec<Value> = server
        .store
        .events_after(t)
        .await?
        .into_iter()
        .filter(|e| e.kind == "tournament")
        .map(|e| serde_json::to_value(&e).unwrap_or_default())
        .collect();
    let live: Vec<Value> = server.with(|l| {
        l.live()
            .tournaments
            .into_iter()
            .filter(|x| x.host == snook_legend_proto::GAME_NAME)
            .map(|x| {
                json!({
                    "id": x.id, "name": x.name, "state": x.state, "format": x.format,
                    "rules": x.rules, "players": x.players.len(), "size": x.size,
                    "entry_fee": x.entry_fee, "prize_pool": x.prize_pool, "prizes": x.prizes,
                    "item_prize": x.item_prize, "starts_at": x.starts_at, "ends_at": x.ends_at,
                })
            })
            .collect()
    });
    let finished: Vec<Value> = server
        .store
        .officials(100)
        .await?
        .into_iter()
        .map(|(id, o)| {
            json!({
                "id": id, "event_id": o.event_id, "name": o.name, "rules": o.rules,
                "format": o.format, "size": o.size, "players": o.players, "rated": o.rated,
                "entry_fee": o.entry_fee, "prize_pool": o.prize_pool, "prizes": o.prizes,
                "item_prize": o.item_prize, "champion_id": o.champion_id,
                "runner_up_id": o.runner_up_id, "starts_at": o.starts_at, "ends_at": o.ends_at,
            })
        })
        .collect();
    let cup = server.ops.settings.cup().map(|c| {
        json!({ "name": c.name, "next_start": c.next_start(t), "format": match c.format {
            snook_legend_proto::TournamentFormat::Arena => "arena",
            snook_legend_proto::TournamentFormat::Knockout => "knockout",
        }, "entry_fee": c.entry_fee, "house_prize": c.house_prize, "rules": c.rules })
    });
    let earned: Vec<&str> =
        shop::ITEMS.iter().filter(|i| i.price == shop::Price::Earned).map(|i| i.code).collect();
    Ok(Json(json!({
        "scheduled": scheduled,
        "live": live,
        "finished": finished,
        "weekly_cup": cup,
        "house_cut_percent": server.ops.settings.int(settings::HOUSE_CUT),
        "item_prizes": earned,
        "now": t,
    })))
}

/// Schedules an official tournament: an event of kind `tournament` (the
/// same body as `POST /admin/api/events`).
async fn create_official(
    state: State<Arc<Server>>,
    headers: HeaderMap,
    Json(mut body): Json<EventBody>,
) -> ApiResult {
    body.kind = "tournament".into();
    super::create_event(state, headers, Json(body)).await
}

// Maintenance.

async fn list_maintenance(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    let t = now();
    let list: Vec<Value> = server
        .store
        .maintenances(50)
        .await?
        .into_iter()
        .map(|m| {
            let status = if m.cancelled_at.is_some() {
                "cancelled"
            } else if m.started_at.is_some() {
                "started"
            } else if m.at <= t {
                "due"
            } else {
                "scheduled"
            };
            let mut v = serde_json::to_value(&m).unwrap_or_default();
            v["status"] = json!(status);
            v
        })
        .collect();
    Ok(Json(json!({
        "maintenance": list,
        "limits": { "max_minutes": maintenance::MAX_MINUTES, "max_message": maintenance::MAX_MESSAGE },
        "notices_minutes": maintenance::NOTICES,
        "draining": server.with(|l| l.is_draining()),
        "now": t,
    })))
}

#[derive(Deserialize)]
struct MaintenanceBody {
    at: i64,
    minutes: u32,
    #[serde(default)]
    message: String,
}

async fn schedule_maintenance(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(body): Json<MaintenanceBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    let t = now();
    if body.at < t + 60 {
        return Err(bad("A maintenance starts at least a minute from now."));
    }
    if body.at > t + 90 * 86_400 {
        return Err(bad("Schedule maintenance within 90 days."));
    }
    if !(1..=maintenance::MAX_MINUTES).contains(&body.minutes) {
        return Err(bad("A maintenance lasts from 1 minute to a day."));
    }
    let open = server.store.maintenances(50).await?;
    if open.iter().any(|m| m.cancelled_at.is_none() && m.started_at.is_none()) {
        return Err(bad("A maintenance is planned already: call it off first."));
    }
    let message = clean_text(&body.message, maintenance::MAX_MESSAGE);
    let id = server.store.add_maintenance(body.at, body.minutes, &message, s.user.id, t).await?;
    maintenance::reload(&server).await;
    maintenance::tick(&server).await;
    let details = format!("{}, {} min: {message}", utc(body.at), body.minutes);
    log(&server, &s, "maintenance", &format!("maintenance {id}"), &details).await;
    Ok(Json(json!({ "ok": true, "id": id })))
}

async fn cancel_maintenance(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    if !server.store.cancel_maintenance(id, now()).await? {
        return Err(bad("No such maintenance planned (or it started already)."));
    }
    maintenance::reload(&server).await;
    maintenance::tick(&server).await;
    log(&server, &s, "maintenance-cancel", &format!("maintenance {id}"), "").await;
    Ok(Json(json!({ "ok": true })))
}

// Auctions.

async fn list_auctions(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    let t = now();
    let mut list = Vec::new();
    for a in server.store.auctions(100).await? {
        let bids = server.store.auction_bids(a.id, 20).await?;
        let mut v = serde_json::to_value(&a).unwrap_or_default();
        v["state"] = json!(auction::state(&a, t));
        v["name"] = json!(shop::item(&a.code).map(|i| i.name));
        v["latest_bids"] = json!(
            bids.iter()
                .map(|(name, amount, at)| json!({ "username": name, "amount": amount, "at": at }))
                .collect::<Vec<_>>()
        );
        list.push(v);
    }
    let items: Vec<Value> = shop::ITEMS
        .iter()
        .filter(|i| auction::may_auction(i))
        .map(|i| json!({ "code": i.code, "name": i.name, "price": i.coins(), "rarity": shop::rarity(i).code() }))
        .collect();
    Ok(Json(json!({
        "auctions": list,
        "items": items,
        "limits": {
            "max_days": auction::MAX_LENGTH / 86_400,
            "max_bid": auction::MAX_BID,
            "extension_secs": snook_legend_proto::ops::EXTENSION,
            "snipe_window_secs": snook_legend_proto::ops::SNIPE_WINDOW,
        },
        "now": t,
    })))
}

#[derive(Deserialize)]
struct AuctionBody {
    code: String,
    starts_at: i64,
    ends_at: i64,
    min_bid: i64,
    #[serde(default = "one")]
    min_increment: i64,
}

fn one() -> i64 {
    1
}

async fn create_auction(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Json(body): Json<AuctionBody>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    let t = now();
    let Some(item) = shop::item(body.code.trim()) else { return Err(bad("No such item.")) };
    if !auction::may_auction(item) {
        return Err(bad("Only items sold for coins or only won can be put up for auction."));
    }
    let starts_at = body.starts_at.max(t);
    if body.starts_at < t - 600 {
        return Err(bad("That time is already past."));
    }
    if body.ends_at <= starts_at + 60 {
        return Err(bad("The auction must end after it starts."));
    }
    if body.ends_at - starts_at > auction::MAX_LENGTH {
        return Err(bad("An auction lasts two weeks at most."));
    }
    if !(1..=auction::MAX_BID).contains(&body.min_bid)
        || !(1..=auction::MAX_BID).contains(&body.min_increment)
    {
        return Err(bad("The least bid and raise are 1 coin or more."));
    }
    let new = NewAuction {
        code: item.code,
        starts_at,
        ends_at: body.ends_at,
        min_bid: body.min_bid,
        min_increment: body.min_increment,
        by: s.user.id,
        at: t,
    };
    let id = server.store.add_auction(&new).await?;
    if let Ok(Some(row)) = server.store.auction(id).await
        && starts_at <= t
    {
        let auction = auction::info(&row, t);
        server
            .with(|l| l.tell_everyone(&snook_legend_proto::ServerMsg::AuctionUpdated { auction }));
    }
    let details = format!(
        "{} – {}, from {} coins (+{})",
        utc(starts_at),
        utc(body.ends_at),
        body.min_bid,
        body.min_increment
    );
    log(&server, &s, "auction", item.code, &details).await;
    Ok(Json(json!({ "ok": true, "id": id })))
}

async fn cancel_auction(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    let back = auction::cancel(&server, id).await.map_err(bad)?;
    let details = match back {
        Some((user, coins)) => format!("{coins} coins back to {user}"),
        None => "no bids".into(),
    };
    log(&server, &s, "auction-cancel", &format!("auction {id}"), &details).await;
    Ok(Json(json!({ "ok": true, "refunded": back.map(|(_, c)| c).unwrap_or(0) })))
}

// Analytics.

#[derive(Deserialize)]
struct DaysQuery {
    #[serde(default)]
    days: Option<u32>,
}

async fn coins(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Query(q): Query<DaysQuery>,
) -> ApiResult {
    staff(&server, &headers).await?;
    Ok(Json(analytics::coins(&server, q.days.unwrap_or(30)).await?))
}

async fn sellers(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    Ok(Json(analytics::sellers(&server).await?))
}

async fn countries(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    staff(&server, &headers).await?;
    Ok(Json(analytics::players_map(&server).await?))
}

#[derive(Deserialize)]
struct HoursQuery {
    #[serde(default)]
    weeks: Option<u32>,
    /// Minutes from UTC (Brasília: -180).
    #[serde(default)]
    tz: Option<i32>,
}

async fn hours(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Query(q): Query<HoursQuery>,
) -> ApiResult {
    staff(&server, &headers).await?;
    let tz = q.tz.unwrap_or(0).clamp(-720, 840);
    Ok(Json(analytics::peak_hours(&server, q.weeks.unwrap_or(4), tz).await?))
}

async fn vip(State(server): State<Arc<Server>>, headers: HeaderMap) -> ApiResult {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    Ok(Json(analytics::vip(&server).await?))
}

#[derive(Deserialize)]
struct RangeQuery {
    #[serde(default)]
    from: Option<i64>,
    #[serde(default)]
    to: Option<i64>,
}

/// `GET /admin/api/export/{kind}.csv`: a CSV, read a page at a time while it
/// is sent (administrators).
async fn export(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(kind): Path<String>,
    Query(q): Query<RangeQuery>,
) -> Result<Response, super::ApiError> {
    let s = staff(&server, &headers).await?;
    require_admin(&s)?;
    let Some(kind) = Export::parse(kind.trim_end_matches(".csv")) else {
        return Err(err(StatusCode::NOT_FOUND, "No such export."));
    };
    let t = now();
    let to = q.to.unwrap_or(t + 1).min(t + 86_400);
    let from = q.from.unwrap_or(to - 30 * 86_400).max(0);
    if from >= to {
        return Err(bad("from must be before to."));
    }
    log(&server, &s, "export", kind.code(), &format!("{} – {}", utc(from), utc(to))).await;
    let header_line =
        analytics::csv_line(&kind.header().iter().map(|h| h.to_string()).collect::<Vec<_>>());
    // (cursor, done): pages until one comes back short.
    let pages = futures_util::stream::unfold(
        (server, Some(i64::MIN)),
        move |(server, cursor)| async move {
            let after = cursor?;
            let after = if after == i64::MIN {
                if kind == Export::Daily { i64::MIN / 2 } else { 0 }
            } else {
                after
            };
            match server.store.export_page(kind, from, to, after, EXPORT_PAGE).await {
                Ok(rows) => {
                    let next = if rows.len() < EXPORT_PAGE as usize {
                        None
                    } else {
                        rows.last().map(|(c, _)| *c)
                    };
                    if rows.is_empty() {
                        return None;
                    }
                    let text: String = rows.iter().map(|(_, f)| analytics::csv_line(f)).collect();
                    Some((Ok::<_, std::io::Error>(text), (server, next)))
                }
                Err(e) => {
                    tracing::error!("export {}: {e}", kind.code());
                    Some((Err(std::io::Error::other("database error")), (server, None)))
                }
            }
        },
    );
    let body = futures_util::stream::once(async move { Ok::<_, std::io::Error>(header_line) })
        .chain(pages);
    let mut res = Body::from_stream(body).into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/csv; charset=utf-8"));
    let name = format!(
        "attachment; filename=\"snook-legend-{}-{}-{}.csv\"",
        kind.code(),
        analytics::date(from),
        analytics::date(to - 1)
    );
    if let Ok(v) = HeaderValue::from_str(&name) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(res)
}

use futures_util::StreamExt;

async fn calendar(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Query(q): Query<RangeQuery>,
) -> ApiResult {
    staff(&server, &headers).await?;
    let t = now();
    let from = q.from.unwrap_or(t - 7 * 86_400);
    let to = q.to.unwrap_or(from + 42 * 86_400);
    if to <= from || to - from > 400 * 86_400 {
        return Err(bad("Ask for 400 days at most, from before to."));
    }
    Ok(Json(analytics::calendar(&server, from, to).await?))
}
