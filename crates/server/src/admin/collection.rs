//! A player's collection for the staff portal: every item they have, with
//! its rarity and how and when they got it (see `crate::collection`).

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde_json::{Value, json};
use snook_legend_proto::shop;

use super::{ApiResult, find, staff};
use crate::Server;

/// `GET /admin/api/users/{name}/collection` (moderators and administrators).
pub(super) async fn collection(
    State(server): State<Arc<Server>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> ApiResult {
    staff(&server, &headers).await?;
    let u = find(&server, &name).await?;
    let items: Vec<Value> = crate::collection::items_of(&server, u.id)
        .await
        .into_iter()
        .map(|o| {
            let item = shop::item(&o.code);
            json!({
                "code": o.code,
                "name": item.map(|i| i.name),
                "category": item.map(|i| i.category.code()).unwrap_or("reaction"),
                "rarity": item.map(shop::rarity),
                "bound": item.is_some_and(shop::bound),
                "origin": o.origin,
                "acquired_at": o.acquired_at,
            })
        })
        .collect();
    Ok(Json(json!({ "username": u.username, "items": items })))
}
