//! Operations and the economy in memory (the same rules as the database:
//! see `store/postgres/ops.rs` and migration 0028).

use std::collections::{BTreeMap, HashMap, HashSet};

use snook_legend_proto::shop::{VIP_GIFT, VIP_MONTH_SECS};

use super::super::{
    AuctionRow, BidOutcome, CoinDay, CountryRow, Export, HourRow, ItemSales, MaintenanceRow,
    NewAuction, NewPromotion, OfficialRecord, PromotionRow, SettingRow, StoreResult, StreakClaim,
    SupporterRow, VipMember, day_of, hour_of, streak_coins,
};
use super::MemoryStore;

#[derive(Default)]
pub(super) struct State {
    /// key -> (value, by, at)
    settings: BTreeMap<String, (String, i64, i64)>,
    /// With their author by id.
    promotions: Vec<(PromotionRow, i64)>,
    officials: Vec<OfficialRecord>,
    maintenance: Vec<(MaintenanceRow, i64)>,
    auctions: Vec<(AuctionRow, i64)>,
    /// (auction, user, amount, at)
    bids: Vec<(i64, i64, i64, i64)>,
    /// user -> (last day, streak)
    streaks: HashMap<i64, (i64, u32)>,
    credits: HashSet<i64>,
}

impl MemoryStore {
    pub fn live_settings(&self) -> StoreResult<Vec<SettingRow>> {
        let s = self.lock();
        Ok(s.ops
            .settings
            .iter()
            .map(|(k, (v, by, at))| SettingRow {
                key: k.clone(),
                value: v.clone(),
                updated_by: s.name(*by),
                updated_at: *at,
            })
            .collect())
    }

    pub fn set_live_setting(
        &self,
        key: &str,
        value: Option<&str>,
        by: i64,
        at: i64,
    ) -> StoreResult<()> {
        let mut s = self.lock();
        match value {
            Some(v) => s.ops.settings.insert(key.to_string(), (v.to_string(), by, at)),
            None => s.ops.settings.remove(key),
        };
        Ok(())
    }

    fn promotion_row(p: &NewPromotion<'_>, id: i64) -> PromotionRow {
        PromotionRow {
            id,
            title: p.title.to_string(),
            kind: p.kind.to_string(),
            percent: p.percent,
            codes: p.codes.to_vec(),
            categories: p.categories.to_vec(),
            starts_at: p.starts_at,
            ends_at: p.ends_at,
            created_by: String::new(),
            created_at: p.at,
            cancelled_at: None,
        }
    }

    pub fn add_promotion(&self, p: &NewPromotion<'_>) -> StoreResult<i64> {
        let mut s = self.lock();
        let id = s.ops.promotions.len() as i64 + 1;
        s.ops.promotions.push((Self::promotion_row(p, id), p.by));
        Ok(id)
    }

    pub fn update_promotion(&self, id: i64, p: &NewPromotion<'_>) -> StoreResult<bool> {
        let mut s = self.lock();
        let Some((row, _)) =
            s.ops.promotions.iter_mut().find(|(r, _)| r.id == id && r.cancelled_at.is_none())
        else {
            return Ok(false);
        };
        let created_at = row.created_at;
        *row = PromotionRow { created_at, ..Self::promotion_row(p, id) };
        Ok(true)
    }

    pub fn promotions(&self, limit: u32) -> StoreResult<Vec<PromotionRow>> {
        let s = self.lock();
        let mut out: Vec<PromotionRow> = s
            .ops
            .promotions
            .iter()
            .map(|(r, by)| PromotionRow { created_by: s.name(*by), ..r.clone() })
            .collect();
        out.sort_by_key(|r| std::cmp::Reverse((r.ends_at, r.id)));
        out.truncate(limit as usize);
        Ok(out)
    }

    pub fn cancel_promotion(&self, id: i64, at: i64) -> StoreResult<bool> {
        let mut s = self.lock();
        match s.ops.promotions.iter_mut().find(|(r, _)| r.id == id && r.cancelled_at.is_none()) {
            Some((r, _)) => {
                r.cancelled_at = Some(at);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn record_official(&self, r: &OfficialRecord) -> StoreResult<Option<i64>> {
        let mut s = self.lock();
        if s.ops.officials.iter().any(|o| o.key == r.key) {
            return Ok(None);
        }
        s.ops.officials.push(r.clone());
        Ok(Some(s.ops.officials.len() as i64))
    }

    pub fn officials(&self, limit: u32) -> StoreResult<Vec<(i64, OfficialRecord)>> {
        let s = self.lock();
        let mut out: Vec<(i64, OfficialRecord)> =
            s.ops.officials.iter().enumerate().map(|(i, r)| (i as i64 + 1, r.clone())).collect();
        out.sort_by_key(|(id, r)| std::cmp::Reverse((r.ends_at, *id)));
        out.truncate(limit as usize);
        Ok(out)
    }

    pub fn official(&self, id: i64) -> StoreResult<Option<(i64, OfficialRecord)>> {
        let s = self.lock();
        let r = usize::try_from(id - 1).ok().and_then(|i| s.ops.officials.get(i));
        Ok(r.map(|r| (id, r.clone())))
    }

    pub fn officials_won(&self, user_id: i64) -> StoreResult<Vec<(i64, String, i64)>> {
        let s = self.lock();
        Ok(s.ops
            .officials
            .iter()
            .enumerate()
            .filter(|(_, r)| r.champion_id == Some(user_id))
            .map(|(i, r)| (i as i64 + 1, r.name.clone(), r.ends_at))
            .collect())
    }

    pub fn add_maintenance(
        &self,
        at: i64,
        minutes: u32,
        message: &str,
        by: i64,
        created_at: i64,
    ) -> StoreResult<i64> {
        let mut s = self.lock();
        let id = s.ops.maintenance.len() as i64 + 1;
        let row = MaintenanceRow {
            id,
            at,
            minutes,
            message: message.to_string(),
            created_by: String::new(),
            created_at,
            cancelled_at: None,
            started_at: None,
        };
        s.ops.maintenance.push((row, by));
        Ok(id)
    }

    pub fn maintenances(&self, limit: u32) -> StoreResult<Vec<MaintenanceRow>> {
        let s = self.lock();
        let mut out: Vec<MaintenanceRow> = s
            .ops
            .maintenance
            .iter()
            .map(|(r, by)| MaintenanceRow { created_by: s.name(*by), ..r.clone() })
            .collect();
        out.sort_by_key(|r| std::cmp::Reverse((r.at, r.id)));
        out.truncate(limit as usize);
        Ok(out)
    }

    pub fn cancel_maintenance(&self, id: i64, at: i64) -> StoreResult<bool> {
        let mut s = self.lock();
        let row = s
            .ops
            .maintenance
            .iter_mut()
            .find(|(r, _)| r.id == id && r.cancelled_at.is_none() && r.started_at.is_none());
        match row {
            Some((r, _)) => {
                r.cancelled_at = Some(at);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn start_maintenance(&self, id: i64, at: i64) -> StoreResult<bool> {
        let mut s = self.lock();
        let row = s
            .ops
            .maintenance
            .iter_mut()
            .find(|(r, _)| r.id == id && r.cancelled_at.is_none() && r.started_at.is_none());
        match row {
            Some((r, _)) => {
                r.started_at = Some(at);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn add_auction(&self, a: &NewAuction<'_>) -> StoreResult<i64> {
        let mut s = self.lock();
        let id = s.ops.auctions.len() as i64 + 1;
        let row = AuctionRow {
            id,
            code: a.code.to_string(),
            starts_at: a.starts_at,
            ends_at: a.ends_at,
            min_bid: a.min_bid,
            min_increment: a.min_increment,
            leader_id: None,
            leader: String::new(),
            bid: 0,
            bids: 0,
            created_by: String::new(),
            created_at: a.at,
            cancelled_at: None,
            settled_at: None,
        };
        s.ops.auctions.push((row, a.by));
        Ok(id)
    }

    fn auction_row(s: &super::Inner, r: &AuctionRow, by: i64) -> AuctionRow {
        AuctionRow {
            leader: r.leader_id.map(|l| s.name(l)).unwrap_or_default(),
            created_by: s.name(by),
            ..r.clone()
        }
    }

    pub fn auctions(&self, limit: u32) -> StoreResult<Vec<AuctionRow>> {
        let s = self.lock();
        let mut out: Vec<AuctionRow> =
            s.ops.auctions.iter().map(|(r, by)| Self::auction_row(&s, r, *by)).collect();
        out.sort_by_key(|r| std::cmp::Reverse((r.ends_at, r.id)));
        out.truncate(limit as usize);
        Ok(out)
    }

    pub fn auction(&self, id: i64) -> StoreResult<Option<AuctionRow>> {
        let s = self.lock();
        Ok(s.ops
            .auctions
            .iter()
            .find(|(r, _)| r.id == id)
            .map(|(r, by)| Self::auction_row(&s, r, *by)))
    }

    pub fn bid(
        &self,
        id: i64,
        user_id: i64,
        amount: i64,
        now: i64,
        window: i64,
        extension: i64,
    ) -> StoreResult<BidOutcome> {
        let mut s = self.lock();
        let Some(k) = s.ops.auctions.iter().position(|(r, _)| r.id == id) else {
            return Ok(BidOutcome::NotOpen);
        };
        let a = s.ops.auctions[k].0.clone();
        if a.cancelled_at.is_some()
            || a.settled_at.is_some()
            || now < a.starts_at
            || now >= a.ends_at
        {
            return Ok(BidOutcome::NotOpen);
        }
        let least = if a.bid == 0 { a.min_bid } else { a.bid + a.min_increment.max(1) };
        if amount < least {
            return Ok(BidOutcome::TooLow(least));
        }
        // The leader raising their own bid only puts in the difference.
        let back = if a.leader_id == Some(user_id) { a.bid } else { 0 };
        if s.wallets.get(&user_id).map_or(0, |w| w.coins) + back < amount {
            return Ok(BidOutcome::NotEnoughCoins);
        }
        s.ops.bids.push((id, user_id, amount, now));
        let n = s.ops.bids.len();
        let outbid = a.leader_id.map(|l| (l, a.bid));
        if let Some((leader, held)) = outbid {
            s.wallets.entry(leader).or_default().coins += held;
            s.ledger.push((
                leader,
                held,
                format!("auction refund #{id}"),
                Some(format!("auction-refund:{id}:{n}")),
                now,
            ));
        }
        s.wallets.entry(user_id).or_default().coins -= amount;
        s.ledger.push((
            user_id,
            -amount,
            format!("auction bid #{id}"),
            Some(format!("auction:{id}:{n}")),
            now,
        ));
        let row = &mut s.ops.auctions[k].0;
        row.leader_id = Some(user_id);
        row.bid = amount;
        row.bids += 1;
        if row.ends_at - now <= window {
            row.ends_at = now + extension;
        }
        Ok(BidOutcome::Placed { outbid, ends_at: row.ends_at })
    }

    pub fn cancel_auction(&self, id: i64, at: i64) -> StoreResult<Option<Option<(i64, i64)>>> {
        let mut s = self.lock();
        let Some(k) =
            s.ops.auctions.iter().position(|(r, _)| {
                r.id == id && r.cancelled_at.is_none() && r.settled_at.is_none()
            })
        else {
            return Ok(None);
        };
        let row = &mut s.ops.auctions[k].0;
        row.cancelled_at = Some(at);
        let back = row.leader_id.map(|l| (l, row.bid));
        if let Some((leader, held)) = back {
            s.wallets.entry(leader).or_default().coins += held;
            s.ledger.push((
                leader,
                held,
                format!("auction refund #{id}"),
                Some(format!("auction-cancel:{id}")),
                at,
            ));
        }
        Ok(Some(back))
    }

    pub fn settle_auction(&self, id: i64, at: i64) -> StoreResult<Option<(AuctionRow, bool)>> {
        let mut s = self.lock();
        let Some(k) = s.ops.auctions.iter().position(|(r, _)| {
            r.id == id && r.cancelled_at.is_none() && r.settled_at.is_none() && r.ends_at <= at
        }) else {
            return Ok(None);
        };
        s.ops.auctions[k].0.settled_at = Some(at);
        let (row, by) = s.ops.auctions[k].clone();
        let mut given = false;
        if let Some(winner) = row.leader_id {
            let w = s.wallets.entry(winner).or_default();
            if w.owned.contains(&row.code) {
                // Got meanwhile (bought, a present): the bid goes back.
                w.coins += row.bid;
                s.ledger.push((
                    winner,
                    row.bid,
                    format!("auction refund #{id}"),
                    Some(format!("auction-owned:{id}")),
                    at,
                ));
            } else {
                w.owned.push(row.code.clone());
                s.acquired.insert(
                    (winner, row.code.clone()),
                    super::collection::acquired(at, &format!("auction:{id}")),
                );
                given = true;
            }
        }
        Ok(Some((Self::auction_row(&s, &row, by), given)))
    }

    pub fn auction_bids(&self, id: i64, limit: u32) -> StoreResult<Vec<(String, i64, i64)>> {
        let s = self.lock();
        Ok(s.ops
            .bids
            .iter()
            .rev()
            .filter(|b| b.0 == id)
            .take(limit as usize)
            .map(|b| (s.name(b.1), b.2, b.3))
            .collect())
    }

    pub fn claim_daily_streak(
        &self,
        user_id: i64,
        day: i64,
        base: i64,
        step: i64,
        days: u32,
    ) -> StoreResult<Option<StreakClaim>> {
        let mut s = self.lock();
        let e = s.earnings.entry(user_id).or_default();
        if e.0 >= day {
            return Ok(None);
        }
        e.0 = day;
        let (last, streak) = s.ops.streaks.get(&user_id).copied().unwrap_or((i64::MIN, 0));
        let streak = if last == day - 1 { streak + 1 } else { 1 };
        s.ops.streaks.insert(user_id, (day, streak));
        let coins = streak_coins(streak, base, step, days);
        s.wallets.entry(user_id).or_default().coins += coins;
        s.ledger.push((user_id, coins, "daily bonus".into(), None, day * 86_400));
        Ok(Some(StreakClaim { day: streak, coins }))
    }

    pub fn set_credits(&self, user_id: i64, show: bool, _at: i64) -> StoreResult<()> {
        let mut s = self.lock();
        if show {
            s.ops.credits.insert(user_id);
        } else {
            s.ops.credits.remove(&user_id);
        }
        Ok(())
    }

    pub fn in_credits(&self, user_id: i64) -> StoreResult<bool> {
        Ok(self.lock().ops.credits.contains(&user_id))
    }

    pub fn supporters(&self, now: i64) -> StoreResult<Vec<SupporterRow>> {
        let s = self.lock();
        let mut out = Vec::new();
        for &id in &s.ops.credits {
            let Some(u) = s.users.get(&id) else { continue };
            let vip_until = s.wallets.get(&id).map_or(0, |w| w.vip_until);
            if u.deleted_at.is_some() || u.is_banned(now) || vip_until == 0 {
                continue;
            }
            let mut times: Vec<i64> = s
                .payments
                .iter()
                .filter(|p| p.user_id == id && p.kind == "vip" && !p.refunded)
                .map(|p| p.at)
                .collect();
            times.extend(
                s.gifts.iter().filter(|g| g.to_id == id && g.code == VIP_GIFT).map(|g| g.at),
            );
            let since = times.iter().min().copied().unwrap_or(vip_until - VIP_MONTH_SECS).max(0);
            out.push(SupporterRow {
                user_id: id,
                username: u.username.clone(),
                country: u.country.clone(),
                since,
                months: (times.len() as u32).max(1),
                vip_until,
            });
        }
        out.sort_by(|a, b| a.since.cmp(&b.since).then(a.username.cmp(&b.username)));
        Ok(out)
    }

    pub fn coin_days(&self, from: i64, to: i64) -> StoreResult<Vec<CoinDay>> {
        let s = self.lock();
        let mut by: BTreeMap<(i64, String), CoinDay> = BTreeMap::new();
        for (_, delta, reason, _, at) in &s.ledger {
            let day = day_of(*at);
            if day < from || day > to {
                continue;
            }
            let row = by.entry((day, reason.clone())).or_insert(CoinDay {
                day,
                reason: reason.clone(),
                credited: 0,
                debited: 0,
                entries: 0,
            });
            if *delta >= 0 {
                row.credited += delta;
            } else {
                row.debited -= delta;
            }
            row.entries += 1;
        }
        Ok(by.into_values().collect())
    }

    pub fn item_sales(&self, since: i64) -> StoreResult<Vec<ItemSales>> {
        let s = self.lock();
        let mut by: BTreeMap<String, (ItemSales, HashSet<i64>)> = BTreeMap::new();
        for (user, delta, reason, _, at) in s.ledger.iter().filter(|l| l.4 >= since && l.1 <= 0) {
            let Some(code) = super::super::sold_item(reason) else { continue };
            let (row, buyers) = by.entry(code.to_string()).or_insert_with(|| {
                (
                    ItemSales { code: code.to_string(), sales: 0, coins: 0, buyers: 0, last_at: 0 },
                    HashSet::new(),
                )
            });
            row.sales += 1;
            row.coins -= delta;
            row.last_at = row.last_at.max(*at);
            buyers.insert(*user);
        }
        Ok(by
            .into_values()
            .map(|(mut row, buyers)| {
                row.buyers = buyers.len() as i64;
                row
            })
            .collect())
    }

    pub fn countries(&self, since: i64) -> StoreResult<Vec<CountryRow>> {
        let s = self.lock();
        let mut by: BTreeMap<String, CountryRow> = BTreeMap::new();
        for u in s.users.values().filter(|u| u.deleted_at.is_none()) {
            let row = by.entry(u.country.clone()).or_insert(CountryRow {
                country: u.country.clone(),
                players: 0,
                new_players: 0,
            });
            row.players += 1;
            if u.created_at >= since {
                row.new_players += 1;
            }
        }
        Ok(by.into_values().collect())
    }

    pub fn hour_samples(&self, from: i64) -> StoreResult<Vec<HourRow>> {
        let s = self.lock();
        Ok(s.online_hours
            .range(from..)
            .map(|(&hour, &(peak_players, peak_games, samples, sum))| HourRow {
                hour,
                active: s.active_hours.iter().filter(|(h, _)| *h == hour).count() as i64,
                peak_players,
                peak_games,
                avg_players: if samples > 0 { sum as f64 / samples as f64 } else { 0.0 },
                games: s.matches.iter().filter(|m| hour_of(m.ended_at) == hour).count() as i64,
            })
            .collect())
    }

    pub fn vip_members(&self, since: i64) -> StoreResult<Vec<VipMember>> {
        let s = self.lock();
        let mut out = Vec::new();
        for (&id, w) in &s.wallets {
            if w.vip_until == 0 || w.vip_until < since {
                continue;
            }
            let Some(u) = s.users.get(&id) else { continue };
            let paid: Vec<&crate::store::Payment> =
                s.payments.iter().filter(|p| p.user_id == id && p.kind == "vip").collect();
            let store_months =
                paid.iter().filter(|p| p.currency == crate::store::MS_CURRENCY).count() as i64;
            let gifts: Vec<i64> = s
                .gifts
                .iter()
                .filter(|g| g.to_id == id && g.code == VIP_GIFT)
                .map(|g| g.at)
                .collect();
            let times: Vec<i64> = paid.iter().map(|p| p.at).chain(gifts.iter().copied()).collect();
            out.push(VipMember {
                user_id: id,
                username: u.username.clone(),
                country: u.country.clone(),
                vip_until: w.vip_until,
                stripe_months: paid.len() as i64 - store_months,
                store_months,
                gifted_months: gifts.len() as i64,
                first_at: times.iter().min().copied().unwrap_or(0),
                last_at: times.iter().max().copied().unwrap_or(0),
                subscriber: w.stripe_customer.is_some(),
            });
        }
        out.sort_by_key(|m| std::cmp::Reverse((m.vip_until, m.user_id)));
        Ok(out)
    }

    pub fn export_page(
        &self,
        kind: Export,
        from: i64,
        to: i64,
        after: i64,
        limit: u32,
    ) -> StoreResult<Vec<(i64, Vec<String>)>> {
        let s = self.lock();
        let b = |v: bool| if v { "true" } else { "false" }.to_string();
        let opt = |v: Option<i64>| v.map(|v| v.to_string()).unwrap_or_default();
        let rows: Vec<(i64, Vec<String>)> = match kind {
            Export::Players => s
                .users
                .values()
                .filter(|u| u.id > after && u.created_at >= from && u.created_at < to)
                .map(|u| {
                    let w = s.wallets.get(&u.id).cloned().unwrap_or_default();
                    let (br, of) = (u.records.brazilian, u.records.official);
                    (
                        u.id,
                        vec![
                            u.id.to_string(),
                            u.username.clone(),
                            u.country.clone(),
                            format!("{:?}", u.role).to_lowercase(),
                            u.created_at.to_string(),
                            u.last_seen.to_string(),
                            br.rating.to_string(),
                            br.games.to_string(),
                            of.rating.to_string(),
                            of.games.to_string(),
                            u.casual_games.to_string(),
                            u.xp.to_string(),
                            w.coins.to_string(),
                            w.vip_until.to_string(),
                            opt(u.banned_until),
                            opt(u.deleted_at),
                        ],
                    )
                })
                .collect(),
            Export::Matches => s
                .matches
                .iter()
                .enumerate()
                .map(|(i, m)| (i as i64 + 1, m))
                .filter(|(id, m)| *id > after && m.ended_at >= from && m.ended_at < to)
                .map(|(id, m)| {
                    (
                        id,
                        vec![
                            id.to_string(),
                            m.ended_at.to_string(),
                            m.rules.code().to_string(),
                            b(m.rated),
                            s.name(m.players[0]),
                            s.name(m.players[1]),
                            m.winner.to_string(),
                            crate::reason_code(m.reason).to_string(),
                            m.delta[0].to_string(),
                            m.delta[1].to_string(),
                            m.shots.to_string(),
                            opt(m.tournament),
                        ],
                    )
                })
                .collect(),
            Export::Purchases => s
                .payments
                .iter()
                .enumerate()
                .map(|(i, p)| (i as i64 + 1, p))
                .filter(|(id, p)| *id > after && p.at >= from && p.at < to)
                .map(|(id, p)| {
                    (
                        id,
                        vec![
                            p.id.clone(),
                            p.at.to_string(),
                            p.user_id.to_string(),
                            s.name(p.user_id),
                            p.kind.clone(),
                            p.coins.to_string(),
                            p.amount.to_string(),
                            p.currency.clone(),
                            b(p.refunded),
                            b(p.test),
                        ],
                    )
                })
                .collect(),
            Export::Ledger => s
                .ledger
                .iter()
                .enumerate()
                .map(|(i, l)| (i as i64 + 1, l))
                .filter(|(id, l)| *id > after && l.4 >= from && l.4 < to)
                .map(|(id, (user, delta, reason, reference, at))| {
                    (
                        id,
                        vec![
                            id.to_string(),
                            at.to_string(),
                            user.to_string(),
                            s.name(*user),
                            delta.to_string(),
                            reason.clone(),
                            reference.clone().unwrap_or_default(),
                        ],
                    )
                })
                .collect(),
            Export::Daily => {
                let (first, last) = (day_of(from).max(after + 1), day_of(to - 1));
                (first..=last)
                    .map(|day| {
                        let active = s.activity.iter().filter(|(d, _)| *d == day).count();
                        let games = s.matches.iter().filter(|m| day_of(m.ended_at) == day).count();
                        let new = s.users.values().filter(|u| day_of(u.created_at) == day).count();
                        let (mut made, mut spent) = (0i64, 0i64);
                        for l in s.ledger.iter().filter(|l| day_of(l.4) == day) {
                            if l.1 >= 0 { made += l.1 } else { spent -= l.1 }
                        }
                        (
                            day,
                            vec![
                                day.to_string(),
                                crate::analytics::date(day * 86_400),
                                active.to_string(),
                                games.to_string(),
                                new.to_string(),
                                made.to_string(),
                                spent.to_string(),
                            ],
                        )
                    })
                    .collect()
            }
        };
        Ok(rows.into_iter().take(limit as usize).collect())
    }
}
