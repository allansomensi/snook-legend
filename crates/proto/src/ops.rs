//! The live side of the game run from the staff portal: sales in the shop
//! (promotions), auctions of rare items, a maintenance announced ahead, the
//! daily bonus's streak and the supporters who appear in the credits.
//!
//! The server sends the facts; the game words them in the player's
//! language (a promotion's title is the staff's, as written).

use serde::{Deserialize, Serialize};

use crate::shop::Item;

/// A sale or a showcase in the shop, scheduled by the staff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Promotion {
    pub id: i64,
    /// Written by the staff ("Weekend sale").
    pub title: String,
    /// Percent off the coin price (0: none, a showcase only).
    #[serde(default)]
    pub percent: u32,
    /// The items it is about (shop codes)…
    #[serde(default)]
    pub codes: Vec<String>,
    /// …and whole categories (category codes: `cue`, `cloth`…).
    #[serde(default)]
    pub categories: Vec<String>,
    /// Its items are shown first and highlighted.
    #[serde(default)]
    pub featured: bool,
    /// Unix seconds.
    pub starts_at: i64,
    pub ends_at: i64,
}

/// The largest discount a promotion may give.
pub const MAX_DISCOUNT: u32 = 90;

impl Promotion {
    /// Running at `now`.
    pub fn is_on(&self, now: i64) -> bool {
        self.starts_at <= now && now < self.ends_at
    }

    /// Whether it is about `item` (by code or by category).
    pub fn covers(&self, item: &Item) -> bool {
        self.codes.iter().any(|c| c == item.code)
            || self.categories.iter().any(|c| c == item.category.code())
    }
}

/// The best discount (percent) on `item` among the promotions running at
/// `now` (discounts don't add up). Only items sold for coins have one.
pub fn discount(promotions: &[Promotion], item: &Item, now: i64) -> u32 {
    if item.coins().is_none() {
        return 0;
    }
    promotions
        .iter()
        .filter(|p| p.is_on(now) && p.covers(item))
        .map(|p| p.percent.min(MAX_DISCOUNT))
        .max()
        .unwrap_or(0)
}

/// `price` with `percent` off (rounded down to whole coins, never below 1).
pub fn discounted(price: u32, percent: u32) -> u32 {
    if percent == 0 {
        return price;
    }
    let off = price as u64 * percent.min(MAX_DISCOUNT) as u64 / 100;
    (price as u64 - off).max(1) as u32
}

/// What `item` costs at `now` in coins with the running promotions (None:
/// not sold for coins).
pub fn price_now(promotions: &[Promotion], item: &Item, now: i64) -> Option<u32> {
    item.coins().map(|p| discounted(p, discount(promotions, item, now)))
}

/// Whether a running promotion features `item`.
pub fn featured(promotions: &[Promotion], item: &Item, now: i64) -> bool {
    promotions.iter().any(|p| p.featured && p.is_on(now) && p.covers(item))
}

/// How an auction stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuctionState {
    /// Not open for bids yet.
    Coming,
    Open,
    /// Over: the leader (if any) won.
    Ended,
    /// Called off by the staff (the leader's coins went back).
    Cancelled,
    #[serde(other)]
    Unknown,
}

/// A bid in the last [`SNIPE_WINDOW`] seconds moves the end this much later.
pub const EXTENSION: i64 = 120;
/// See [`EXTENSION`].
pub const SNIPE_WINDOW: i64 = 120;

/// An auction of a rare item, as players see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuctionInfo {
    pub id: i64,
    /// The item (shop code).
    pub code: String,
    pub starts_at: i64,
    /// Moves later when someone bids at the last moment (see [`EXTENSION`]).
    pub ends_at: i64,
    /// The first bid is at least this.
    pub min_bid: i64,
    /// Each bid beats the one before by at least this.
    pub min_increment: i64,
    /// The highest bid so far (0: none).
    #[serde(default)]
    pub bid: i64,
    /// Who made it (empty: nobody yet).
    #[serde(default)]
    pub leader: String,
    /// Bids made so far.
    #[serde(default)]
    pub bids: u32,
    pub state: AuctionState,
}

impl AuctionInfo {
    /// The least the next bid may be.
    pub fn next_min(&self) -> i64 {
        if self.bid == 0 { self.min_bid } else { self.bid + self.min_increment.max(1) }
    }
}

/// Someone who supported the game (VIP now or before) and chose to appear
/// in the credits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Supporter {
    pub username: String,
    #[serde(default)]
    pub country: String,
    /// Their first month of VIP (Unix seconds).
    pub since: i64,
    /// Months of VIP so far.
    pub months: u32,
    /// VIP now.
    pub vip: bool,
}

/// The daily bonus's coins on day `day` in a row: `base` on the first day,
/// `step` more each day after, up to the day `days` (then it stays there).
pub fn streak_coins(day: u32, base: u32, step: u32, days: u32) -> u32 {
    let d = day.clamp(1, days.max(1));
    base.saturating_add(step.saturating_mul(d - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shop::item;

    fn promo(percent: u32, codes: &[&str], cats: &[&str], at: (i64, i64)) -> Promotion {
        Promotion {
            id: 1,
            title: "Sale".into(),
            percent,
            codes: codes.iter().map(|c| c.to_string()).collect(),
            categories: cats.iter().map(|c| c.to_string()).collect(),
            featured: percent == 0,
            starts_at: at.0,
            ends_at: at.1,
        }
    }

    #[test]
    fn the_best_running_discount_counts() {
        let blue = item("cloth_blue").unwrap();
        let price = blue.coins().unwrap();
        let list = [
            promo(30, &[], &["cloth"], (100, 200)),
            promo(50, &["cloth_blue"], &[], (150, 160)),
            promo(70, &["cue_royal"], &[], (100, 200)),
            promo(0, &["cloth_blue"], &[], (100, 300)),
        ];
        assert_eq!(discount(&list, blue, 99), 0, "not yet");
        assert_eq!(discount(&list, blue, 100), 30);
        assert_eq!(discount(&list, blue, 155), 50, "the best, not the sum");
        assert_eq!(discount(&list, blue, 200), 0, "over at the end");
        assert_eq!(price_now(&list, blue, 120), Some(discounted(price, 30)));
        assert!(featured(&list, blue, 250));
        assert!(!featured(&list, blue, 300));
        // VIP's items are never on sale (they aren't sold for coins).
        let vip = item("cue_royal").unwrap();
        assert_eq!(discount(&list, vip, 150), 0);
        assert_eq!(price_now(&list, vip, 150), None);
    }

    #[test]
    fn discounts_round_down_and_keep_a_price() {
        assert_eq!(discounted(1000, 30), 700);
        assert_eq!(discounted(999, 10), 900);
        assert_eq!(discounted(1, 90), 1);
        assert_eq!(discounted(1000, 100), 100, "at most 90% off");
        assert_eq!(discounted(450, 0), 450);
    }

    #[test]
    fn streaks_climb_then_stay() {
        assert_eq!(streak_coins(1, 5, 5, 7), 5);
        assert_eq!(streak_coins(4, 5, 5, 7), 20);
        assert_eq!(streak_coins(7, 5, 5, 7), 35);
        assert_eq!(streak_coins(30, 5, 5, 7), 35);
        assert_eq!(streak_coins(0, 5, 5, 7), 5);
    }

    #[test]
    fn auctions_round_trip_and_tolerate_the_unknown() {
        let a = AuctionInfo {
            id: 3,
            code: "cue_royal".into(),
            starts_at: 1,
            ends_at: 2,
            min_bid: 500,
            min_increment: 50,
            bid: 0,
            leader: String::new(),
            bids: 0,
            state: AuctionState::Open,
        };
        assert_eq!(a.next_min(), 500);
        let b = AuctionInfo { bid: 600, ..a.clone() };
        assert_eq!(b.next_min(), 650);
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(serde_json::from_str::<AuctionInfo>(&json).unwrap(), a);
        let later: AuctionState = serde_json::from_str(r#""paused""#).unwrap();
        assert_eq!(later, AuctionState::Unknown);
    }
}
