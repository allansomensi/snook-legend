//! The players' collections (see `snook_legend_proto::collection`): every
//! item with how and when it was got.
//!
//! Whatever gives a player an item records its origin with [`record`] (or
//! gives it and records it at once with [`give`]). The items that come with
//! achievements are not owned rows: they are the player's from the moment
//! the achievement was unlocked, and the collection says so.

use snook_legend_proto::collection::{Origin, OwnedItem};
use snook_legend_proto::reactions::{ITEM_PREFIX, REACTIONS};
use snook_legend_proto::shop::{self, Price};
use snook_legend_proto::{ClientMsg, ServerMsg};

use crate::store::{OwnedRow, User};
use crate::{Server, now};

/// Records how `user_id` got `code` (kept with the item; the first origin
/// stays when an item is got twice). An item the player doesn't have yet is
/// given with it.
pub async fn record(server: &Server, user_id: i64, code: &str, origin: Origin) {
    give(server, user_id, code, origin).await;
}

/// Gives `code` to `user_id` and records how they got it: true when they
/// didn't have it yet (false when they did, or it could not be stored: the
/// error is logged).
pub async fn give(server: &Server, user_id: i64, code: &str, origin: Origin) -> bool {
    let json = serde_json::to_string(&origin).unwrap_or_default();
    match server.store.record_origin(user_id, code, &json, now()).await {
        Ok(new) => new,
        Err(e) => {
            tracing::error!(user = user_id, item = code, "recording where an item came from: {e}");
            false
        }
    }
}

/// How an owned item was got: as recorded, or, for the items owned before
/// origins were kept, worked out from how it was added.
pub fn origin_of(row: &OwnedRow) -> Origin {
    if let Some(o) = row.origin.as_deref().and_then(|j| serde_json::from_str(j).ok()) {
        return o;
    }
    match row.source.as_str() {
        "coins" => Origin::Bought {
            price: shop::item(&row.code).and_then(|i| i.coins()).unwrap_or(0) as i64,
        },
        "welcome pack" => Origin::WelcomePack,
        "staff" => Origin::Staff { reason: String::new() },
        s if s.starts_with("gift:") => match &row.giver {
            Some(from) => Origin::Gift { from: from.clone() },
            None => Origin::Unknown,
        },
        _ => Origin::Unknown,
    }
}

/// Everything `user_id` has, with how and when: the items owned, the items
/// of the achievements unlocked, and the reactions those achievements bring
/// (`reaction_<code>`, also for reactions that are not shop items). Free
/// and VIP items are not listed (the game knows them).
pub async fn items_of(server: &Server, user_id: i64) -> Vec<OwnedItem> {
    let (owned, achievements) =
        tokio::join!(server.store.owned_items(user_id), server.store.achievements(user_id));
    let owned = owned.unwrap_or_else(|e| {
        tracing::warn!(user = user_id, "the collection: {e}");
        Vec::new()
    });
    let achievements = achievements.unwrap_or_default();
    let mut items: Vec<OwnedItem> = owned
        .iter()
        .map(|r| OwnedItem {
            code: r.code.clone(),
            origin: origin_of(r),
            acquired_at: r.acquired_at,
        })
        .collect();
    for (code, at) in &achievements {
        let from = |item: String| OwnedItem {
            code: item,
            origin: Origin::Achievement { code: code.clone() },
            acquired_at: *at,
        };
        for item in
            shop::ITEMS.iter().filter(|i| matches!(i.price, Price::Achievement(a) if a == code))
        {
            if !items.iter().any(|o| o.code == item.code) {
                items.push(from(item.code.to_string()));
            }
        }
        for r in REACTIONS.iter().filter(|r| r.achievement().is_some_and(|a| a.code == code)) {
            items.push(from(format!("{ITEM_PREFIX}{}", r.code)));
        }
    }
    items
}

pub fn is_msg(msg: &ClientMsg) -> bool {
    matches!(msg, ClientMsg::Collection)
}

pub async fn handle(server: &Server, me: &User, msg: ClientMsg, send: &impl Fn(ServerMsg)) {
    if let ClientMsg::Collection = msg {
        send(ServerMsg::Collection { items: items_of(server, me.id).await });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(code: &str, source: &str) -> OwnedRow {
        OwnedRow { code: code.into(), source: source.into(), acquired_at: 5, ..OwnedRow::default() }
    }

    #[test]
    fn origins_of_items_owned_before_they_were_kept() {
        assert_eq!(origin_of(&row("cloth_blue", "coins")), Origin::Bought { price: 1000 });
        assert_eq!(origin_of(&row("cue_welcome", "welcome pack")), Origin::WelcomePack);
        assert_eq!(origin_of(&row("cue_ocean", "staff")), Origin::Staff { reason: String::new() });
        let gift = OwnedRow { giver: Some("ana".into()), ..row("cloth_blue", "gift:7") };
        assert_eq!(origin_of(&gift), Origin::Gift { from: "ana".into() });
        assert_eq!(origin_of(&row("cloth_blue", "gift:7")), Origin::Unknown);
        assert_eq!(origin_of(&row("x", "promo")), Origin::Unknown);
        // What was recorded wins.
        let won = OwnedRow {
            origin: Some(r#"{"how":"tournament","name":"Winter Cup 2026","place":1}"#.into()),
            ..row("cue_season_champion", "staff")
        };
        assert_eq!(
            origin_of(&won),
            Origin::Tournament { name: "Winter Cup 2026".into(), place: 1 }
        );
    }
}
