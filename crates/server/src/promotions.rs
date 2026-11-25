//! Promotions in the shop, scheduled by the staff from the portal: a sale
//! (a percent off items or whole categories) or a showcase (items shown
//! first). The price an item is sold (or given) for is the best running
//! discount's (see `snook_legend_proto::ops`); players are told about the
//! promotions running and coming when they sign in and whenever the list
//! changes (one is made, changed, starts or ends).

use std::sync::{Mutex, RwLock};

use snook_legend_proto::ServerMsg;
use snook_legend_proto::ops::{self, Promotion};
use snook_legend_proto::shop::Item;

use crate::store::PromotionRow;
use crate::{Server, now};

/// Promotions further ahead than this aren't shown to players yet (seconds).
pub const SHOWN_AHEAD: i64 = 3 * 86_400;
/// The longest a promotion may last (seconds): a month.
pub const MAX_LENGTH: i64 = 31 * 86_400;
/// The longest title.
pub const MAX_TITLE: usize = 60;
/// Promotions running or coming at once, at most.
pub const MAX_OPEN: usize = 50;

/// The promotions not over yet, and what players were last told.
#[derive(Default)]
pub struct Promotions {
    list: RwLock<Vec<Promotion>>,
    told: Mutex<Option<Vec<Promotion>>>,
}

pub fn to_proto(r: &PromotionRow) -> Promotion {
    Promotion {
        id: r.id,
        title: r.title.clone(),
        percent: if r.kind == "discount" { r.percent } else { 0 },
        codes: r.codes.clone(),
        categories: r.categories.clone(),
        featured: r.kind == "featured",
        starts_at: r.starts_at,
        ends_at: r.ends_at,
    }
}

/// Reads the promotions again (after every change in the portal, and every
/// minute) and tells players if what they see changed.
pub async fn reload(server: &Server) {
    if server.config.settings.lan {
        return;
    }
    let t = now();
    match server.store.promotions(500).await {
        Ok(rows) => {
            let list: Vec<Promotion> = rows
                .iter()
                .filter(|r| r.cancelled_at.is_none() && r.ends_at > t)
                .map(to_proto)
                .collect();
            *server.ops.promotions.list.write().unwrap_or_else(|e| e.into_inner()) = list;
        }
        Err(e) => {
            tracing::warn!("promotions: {e}");
            return;
        }
    }
    tick(server);
}

/// The promotions players are shown at `t`: running, or starting within
/// [`SHOWN_AHEAD`], soonest first.
pub fn shown(server: &Server, t: i64) -> Vec<Promotion> {
    let list = server.ops.promotions.list.read().unwrap_or_else(|e| e.into_inner());
    let mut out: Vec<Promotion> =
        list.iter().filter(|p| p.ends_at > t && p.starts_at <= t + SHOWN_AHEAD).cloned().collect();
    out.sort_by_key(|p| (p.starts_at, p.id));
    out
}

/// Tells everyone when the promotions shown changed (one started, ended,
/// was made or called off).
pub fn tick(server: &Server) {
    let list = shown(server, now());
    let changed = {
        let mut told = server.ops.promotions.told.lock().unwrap_or_else(|e| e.into_inner());
        let changed = told.as_ref() != Some(&list);
        *told = Some(list.clone());
        changed
    };
    if changed {
        server.with(|l| l.tell_everyone(&ServerMsg::Promotions { list }));
    }
}

/// What a player who signs in is told (nothing when there is none).
pub fn message(server: &Server) -> Option<ServerMsg> {
    let list = shown(server, now());
    (!list.is_empty()).then_some(ServerMsg::Promotions { list })
}

/// What `item` costs now in coins, with the best running discount (None:
/// not sold for coins).
pub fn price(server: &Server, item: &Item) -> Option<u32> {
    let list = server.ops.promotions.list.read().unwrap_or_else(|e| e.into_inner());
    ops::price_now(&list, item, now())
}
