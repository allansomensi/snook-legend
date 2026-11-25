//! Auctions of rare items, a sink for coins: the administrators put an item
//! up (its start, end, least bid and least raise); players bid with coins.
//! A bid is held (taken from the wallet); whoever is outbid gets their coins
//! back at once and is told; a bid in the last two minutes moves the end two
//! minutes later. At the end the leader gets the item for good and the
//! coins leave the economy.

use std::collections::HashMap;
use std::sync::Mutex;

use snook_legend_proto::ServerMsg;
use snook_legend_proto::collection::Origin;
use snook_legend_proto::inbox::{Action, Kind};
use snook_legend_proto::ops::{AuctionInfo, AuctionState, EXTENSION, SNIPE_WINDOW};
use snook_legend_proto::shop::{self, Price};

use crate::store::{AuctionRow, BidOutcome, User};
use crate::{Server, inbox, now};

/// Auctions coming further ahead than this aren't shown yet (seconds).
pub const SHOWN_AHEAD: i64 = 7 * 86_400;
/// Auctions over are still shown this long (seconds): who won.
pub const SHOWN_AFTER: i64 = 2 * 86_400;
/// The longest an auction may last (seconds).
pub const MAX_LENGTH: i64 = 14 * 86_400;
/// The largest bid (coins).
pub const MAX_BID: i64 = 10_000_000;

/// What players were last told of each auction (its state).
#[derive(Default)]
pub struct Told {
    states: Mutex<HashMap<i64, AuctionState>>,
}

/// Whether an item may be put up for auction: one that is sold for coins or
/// only won (not a free one, an achievement's, VIP's or the welcome pack's).
pub fn may_auction(item: &shop::Item) -> bool {
    matches!(item.price, Price::Coins(_) | Price::Earned)
}

pub fn state(r: &AuctionRow, t: i64) -> AuctionState {
    if r.cancelled_at.is_some() {
        AuctionState::Cancelled
    } else if r.settled_at.is_some() || t >= r.ends_at {
        AuctionState::Ended
    } else if t < r.starts_at {
        AuctionState::Coming
    } else {
        AuctionState::Open
    }
}

pub fn info(r: &AuctionRow, t: i64) -> AuctionInfo {
    AuctionInfo {
        id: r.id,
        code: r.code.clone(),
        starts_at: r.starts_at,
        ends_at: r.ends_at,
        min_bid: r.min_bid,
        min_increment: r.min_increment,
        bid: r.bid,
        leader: r.leader.clone(),
        bids: r.bids,
        state: state(r, t),
    }
}

/// The auctions players see: open, coming soon, and lately over.
pub async fn shown(server: &Server) -> Vec<AuctionInfo> {
    let t = now();
    let rows = match server.store.auctions(100).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("auctions: {e}");
            return Vec::new();
        }
    };
    let mut list: Vec<AuctionInfo> = rows
        .iter()
        .filter(|r| r.cancelled_at.is_none())
        .filter(|r| r.starts_at <= t + SHOWN_AHEAD && r.ends_at > t - SHOWN_AFTER)
        .map(|r| info(r, t))
        .collect();
    list.sort_by_key(|a| (a.state != AuctionState::Open, a.ends_at, a.id));
    list
}

/// A player signed in: the auctions, when one is open or coming.
pub async fn signed_in(server: &Server, user: &User) {
    let list = shown(server).await;
    if list.iter().any(|a| matches!(a.state, AuctionState::Open | AuctionState::Coming)) {
        server.with(|l| l.notify(&user.username, ServerMsg::Auctions { list }));
    }
}

fn updated(server: &Server, row: &AuctionRow) {
    let auction = info(row, now());
    server
        .ops
        .auctions
        .states
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(row.id, auction.state);
    server.with(|l| l.tell_everyone(&ServerMsg::AuctionUpdated { auction }));
}

/// A bid from a player.
pub async fn bid(server: &Server, me: &User, id: i64, amount: i64, send: &impl Fn(ServerMsg)) {
    let error = |text: &str| send(ServerMsg::Error { message: text.to_string() });
    let Ok(Some(a)) = server.store.auction(id).await else {
        return error("This auction is not open.");
    };
    if !(1..=MAX_BID).contains(&amount) {
        return error("Not enough coins.");
    }
    let Ok(w) = server.store.wallet(me.id).await else {
        return error("Something went wrong. Please try again.");
    };
    if w.owned.contains(&a.code) {
        return error("You already have this item.");
    }
    let t = now();
    match server.store.bid(id, me.id, amount, t, SNIPE_WINDOW, EXTENSION).await {
        Ok(BidOutcome::Placed { outbid, .. }) => {
            tracing::info!(user = me.id, auction = id, amount, "bid");
            crate::shop::push_wallet(server, me).await;
            if let Some((leader, held)) = outbid.filter(|(l, _)| *l != me.id) {
                let kind = Kind::AuctionOutbid { auction: id, code: a.code.clone(), bid: amount };
                inbox::send(server, leader, inbox::New::game(kind).action(Action::Auctions)).await;
                if let Ok(Some(u)) = server.store.user_by_id(leader).await {
                    crate::shop::push_wallet(server, &u).await;
                }
                tracing::info!(user = leader, auction = id, held, "outbid: coins back");
            }
            if let Ok(Some(row)) = server.store.auction(id).await {
                updated(server, &row);
            }
        }
        Ok(BidOutcome::TooLow(least)) => error(&format!("Bid at least {least} coins.")),
        Ok(BidOutcome::NotOpen) => error("This auction is not open."),
        Ok(BidOutcome::NotEnoughCoins) => error("Not enough coins."),
        Err(e) => {
            tracing::error!("bid on {id} by {}: {e}", me.username);
            error("Something went wrong. Please try again.")
        }
    }
}

/// Settles the auctions that ended (the winner gets the item) and tells
/// everyone when one opens or ends.
pub async fn tick(server: &Server) {
    if server.config.settings.lan {
        return;
    }
    let t = now();
    let Ok(rows) = server.store.auctions(100).await else { return };
    for r in rows.iter().filter(|r| r.ends_at > t - SHOWN_AFTER) {
        if r.cancelled_at.is_none() && r.settled_at.is_none() && r.ends_at <= t {
            settle(server, r.id).await;
            continue;
        }
        let now_state = state(r, t);
        let known = server
            .ops
            .auctions
            .states
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&r.id)
            .copied();
        match known {
            Some(s) if s == now_state => {}
            // First seen by this server: nothing to tell.
            None => {
                server
                    .ops
                    .auctions
                    .states
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(r.id, now_state);
            }
            Some(_) => updated(server, r),
        }
    }
}

/// Settles auction `id` (over): the winner gets the item and is told.
pub async fn settle(server: &Server, id: i64) {
    let row = match server.store.settle_auction(id, now()).await {
        Ok(Some(row)) => row,
        Ok(None) => return,
        Err(e) => {
            tracing::error!("settling auction {id}: {e}");
            return;
        }
    };
    if let Some(winner) = row.leader_id {
        tracing::info!(user = winner, auction = id, price = row.bid, item = %row.code, "auction won");
        let origin = Origin::Auction { price: row.bid };
        crate::collection::record(server, winner, &row.code, origin).await;
        let kind = Kind::AuctionWon { auction: id, code: row.code.clone(), price: row.bid };
        let n = inbox::New::game(kind).action(Action::Equip { code: row.code.clone() });
        inbox::send(server, winner, n).await;
        if let Ok(Some(u)) = server.store.user_by_id(winner).await {
            crate::shop::push_wallet(server, &u).await;
        }
    }
    updated(server, &row);
}

/// The staff call an auction off: the leader's coins go back.
pub async fn cancel(server: &Server, id: i64) -> Result<Option<(i64, i64)>, &'static str> {
    match server.store.cancel_auction(id, now()).await {
        Ok(Some(back)) => {
            if let Some((leader, _)) = back
                && let Ok(Some(u)) = server.store.user_by_id(leader).await
            {
                crate::shop::push_wallet(server, &u).await;
            }
            if let Ok(Some(row)) = server.store.auction(id).await {
                updated(server, &row);
            }
            Ok(back)
        }
        Ok(None) => Err("That auction is over or was called off already."),
        Err(e) => {
            tracing::error!("cancelling auction {id}: {e}");
            Err("Database error.")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_items_sold_or_won_go_up() {
        assert!(may_auction(shop::item("cloth_blue").unwrap()));
        assert!(!may_auction(shop::item("cloth_green").unwrap()), "free");
        assert!(!may_auction(shop::item("cue_royal").unwrap()), "VIP's");
        assert!(!may_auction(shop::item(snook_legend_proto::ladder::REWARD_ITEM).unwrap()));
    }
}
