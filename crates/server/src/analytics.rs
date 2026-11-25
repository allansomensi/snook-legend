//! The staff's analytics (see `admin/ops.rs` for the endpoints): where coins
//! come from and go (and whether there are too many), what sells, where the
//! players are, when they play, the VIP members, and a calendar of
//! everything scheduled. Read from the store as asked; nothing is kept.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Value, json};
use snook_legend_proto::shop::{self, ITEMS, Price};

use crate::store::{StoreResult, day_of};
use crate::{Server, now};

/// A time as a date, `YYYY-MM-DD` (UTC).
pub fn date(t: i64) -> String {
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = t.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year}-{month:02}-{day:02}")
}

/// The first second of month `m` (1–12) of `year` (UTC).
pub fn month_start(year: i64, m: i64) -> i64 {
    let (y, m) = if m <= 2 { (year - 1, m + 9) } else { (year, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) * 86_400
}

/// Where coins came from (a ledger entry that gave them), as the portal
/// groups it.
pub fn source_of(reason: &str) -> &'static str {
    let r = reason.to_ascii_lowercase();
    let first = r.split([':', ' ']).next().unwrap_or("");
    match first {
        "game" | "win" => "games",
        "daily" if r.starts_with("daily challenge") => "daily_challenge",
        "daily" => "daily_bonus",
        "mission" => "missions",
        "prize" => "tournament_prizes",
        "league" | "level" => "rewards",
        "staff" => "staff",
        "pack" | "vip" | "welcome" => "purchases",
        "refund" | "auction" => "refunds",
        _ => "other",
    }
}

/// Where coins went (a ledger entry that took them).
pub fn sink_of(reason: &str) -> String {
    let category = |code: &str| {
        shop::item(code).map_or("unknown".to_string(), |i| i.category.code().to_string())
    };
    if let Some(code) = reason.strip_prefix("buy ") {
        return format!("shop:{}", category(code.split(' ').next().unwrap_or("")));
    }
    if let Some(code) = reason.strip_prefix("gift ") {
        return format!("gifts:{}", category(code.split(' ').next().unwrap_or("")));
    }
    let r = reason.to_ascii_lowercase();
    let first = r.split([':', ' ']).next().unwrap_or("");
    match first {
        "tournament" => "tournament_fees".into(),
        "auction" => "auctions".into(),
        "name" => "name_changes".into(),
        "staff" => "staff".into(),
        // A payment refunded takes its coins back.
        "pack" | "vip" | "welcome" | "refund" => "payment_refunds".into(),
        other if other.is_empty() => "other".into(),
        _ => "other".into(),
    }
}

/// The health of the economy from the coins created over the last week as
/// a share of all coins: a little growth is healthy; a lot makes prices
/// feel cheap (inflation); shrinking means the sinks bite.
pub fn verdict(net_week: i64, supply: i64) -> (&'static str, f64) {
    let pct = if supply > 0 { net_week as f64 * 100.0 / supply as f64 } else { 0.0 };
    let word = if supply <= 0 {
        "unknown"
    } else if pct < -2.0 {
        "deflating"
    } else if pct <= 3.0 {
        "healthy"
    } else if pct <= 7.0 {
        "watch"
    } else {
        "inflating"
    };
    (word, (pct * 100.0).round() / 100.0)
}

/// Coins created and spent per day over the last `days`, by source and by
/// sink, with the coins in wallets at the end of each day.
pub async fn coins(server: &Server, days: u32) -> StoreResult<Value> {
    let t = now();
    let today = day_of(t);
    let first = today - days.clamp(1, 400) as i64 + 1;
    let (rows, economy) =
        tokio::join!(server.store.coin_days(first, today), server.store.economy(t));
    let (rows, economy) = (rows?, economy?);
    #[derive(Default)]
    struct Day {
        created: BTreeMap<&'static str, i64>,
        spent: BTreeMap<String, i64>,
    }
    let mut by: BTreeMap<i64, Day> = (first..=today).map(|d| (d, Day::default())).collect();
    for r in &rows {
        let day = by.entry(r.day).or_default();
        if r.credited > 0 {
            *day.created.entry(source_of(&r.reason)).or_default() += r.credited;
        }
        if r.debited > 0 {
            *day.spent.entry(sink_of(&r.reason)).or_default() += r.debited;
        }
    }
    let nets: Vec<(i64, i64)> = by
        .iter()
        .map(|(d, day)| (*d, day.created.values().sum::<i64>() - day.spent.values().sum::<i64>()))
        .collect();
    // The coins in wallets at the end of each day: today's total less what
    // the days after it added.
    let mut supply = HashMap::new();
    let mut after = 0;
    for (d, net) in nets.iter().rev() {
        supply.insert(*d, economy.coins_total - after);
        after += net;
    }
    let mut totals_created: BTreeMap<&str, i64> = BTreeMap::new();
    let mut totals_spent: BTreeMap<String, i64> = BTreeMap::new();
    let list: Vec<Value> = by
        .iter()
        .map(|(d, day)| {
            for (k, v) in &day.created {
                *totals_created.entry(k).or_default() += v;
            }
            for (k, v) in &day.spent {
                *totals_spent.entry(k.clone()).or_default() += v;
            }
            let created: i64 = day.created.values().sum();
            let spent: i64 = day.spent.values().sum();
            json!({
                "day": d, "date": date(d * 86_400),
                "created": created, "spent": spent, "net": created - spent,
                "by_source": day.created, "by_sink": day.spent,
                "supply": supply.get(d),
            })
        })
        .collect();
    let week: i64 = nets.iter().filter(|(d, _)| *d > today - 7).map(|(_, n)| n).sum();
    let (health, pct) = verdict(week, economy.coins_total);
    Ok(json!({
        "days": list,
        "totals": { "created": totals_created, "spent": totals_spent },
        "supply": economy.coins_total,
        "wallets": economy.wallets,
        "net_7d": week,
        "net_7d_percent": pct,
        "health": health,
    }))
}

/// Sales per item and per category (bought and given) over the last 7 and
/// 30 days and all time, best first, and the items for coins never sold.
pub async fn sellers(server: &Server) -> StoreResult<Value> {
    let t = now();
    let (w, m, all) = tokio::join!(
        server.store.item_sales(t - 7 * 86_400),
        server.store.item_sales(t - 30 * 86_400),
        server.store.item_sales(0),
    );
    let (w, m, all) = (w?, m?, all?);
    let period = |list: &[crate::store::ItemSales], code: &str| {
        list.iter()
            .find(|s| s.code == code)
            .map(|s| json!({ "sales": s.sales, "coins": s.coins, "buyers": s.buyers }))
            .unwrap_or_else(|| json!({ "sales": 0, "coins": 0, "buyers": 0 }))
    };
    let mut items: Vec<(i64, Value)> = all
        .iter()
        .map(|s| {
            let item = shop::item(&s.code);
            (
                s.coins,
                json!({
                    "code": s.code,
                    "name": item.map(|i| i.name),
                    "category": item.map(|i| i.category.code()),
                    "rarity": item.map(|i| shop::rarity(i).code()),
                    "price": item.and_then(|i| i.coins()),
                    "last_sold_at": s.last_at,
                    "days_7": period(&w, &s.code),
                    "days_30": period(&m, &s.code),
                    "all_time": period(&all, &s.code),
                }),
            )
        })
        .collect();
    items.sort_by_key(|(coins, _)| std::cmp::Reverse(*coins));
    let mut categories: BTreeMap<&str, [i64; 3]> = BTreeMap::new();
    for s in &all {
        let c = shop::item(&s.code).map_or("unknown", |i| i.category.code());
        let e = categories.entry(c).or_default();
        e[0] += s.sales;
        e[1] += s.coins;
    }
    for s in &m {
        let c = shop::item(&s.code).map_or("unknown", |i| i.category.code());
        categories.entry(c).or_default()[2] += s.coins;
    }
    let never: Vec<Value> = ITEMS
        .iter()
        .filter(|i| matches!(i.price, Price::Coins(_)))
        .filter(|i| !all.iter().any(|s| s.code == i.code))
        .map(|i| json!({ "code": i.code, "name": i.name, "category": i.category.code(), "price": i.coins() }))
        .collect();
    Ok(json!({
        "items": items.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
        "categories": categories
            .into_iter()
            .map(|(c, [sales, coins, coins_30])| json!({
                "category": c, "sales": sales, "coins": coins, "coins_30d": coins_30,
            }))
            .collect::<Vec<_>>(),
        "never_sold": never,
    }))
}

/// Players per country: accounts, new in the last 7 days, online now.
pub async fn players_map(server: &Server) -> StoreResult<Value> {
    let rows = server.store.countries(now() - 7 * 86_400).await?;
    let online = server.with(|l| l.online_countries());
    let mut list: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "country": r.country, "players": r.players, "new_7d": r.new_players,
                "online": online.get(&r.country).copied().unwrap_or(0),
            })
        })
        .collect();
    // Online from a country no account of this store has (a guest).
    for (c, n) in &online {
        if !rows.iter().any(|r| &r.country == c) {
            list.push(json!({ "country": c, "players": 0, "new_7d": 0, "online": n }));
        }
    }
    list.sort_by_key(|v| std::cmp::Reverse(v["players"].as_i64().unwrap_or(0)));
    Ok(json!({ "countries": list, "online": online.values().sum::<usize>() }))
}

/// Players online by weekday and hour over the last `weeks`, in the time
/// zone `offset` minutes from UTC: the average and the most at once.
/// Weekdays: 0 = Sunday.
pub async fn peak_hours(server: &Server, weeks: u32, offset: i32) -> StoreResult<Value> {
    let weeks = weeks.clamp(1, 52);
    let from = crate::store::hour_of(now()) - weeks as i64 * 7 * 24;
    let rows = server.store.hour_samples(from).await?;
    // (weekday, hour) -> (sum of averages, hours, max)
    let mut grid = [[(0.0f64, 0u32, 0i64); 24]; 7];
    for r in &rows {
        let local = r.hour * 3600 + offset as i64 * 60;
        let day = local.div_euclid(86_400);
        let weekday = (day + 4).rem_euclid(7) as usize;
        let hour = (local.rem_euclid(86_400) / 3600) as usize;
        let cell = &mut grid[weekday][hour];
        cell.0 += r.avg_players;
        cell.1 += 1;
        cell.2 = cell.2.max(r.peak_players);
    }
    let cells: Vec<Value> = grid
        .iter()
        .enumerate()
        .flat_map(|(weekday, hours)| {
            hours.iter().enumerate().map(move |(hour, (sum, n, max))| {
                let avg = if *n > 0 { sum / *n as f64 } else { 0.0 };
                json!({
                    "weekday": weekday, "hour": hour,
                    "avg": (avg * 10.0).round() / 10.0, "max": max, "samples": n,
                })
            })
        })
        .collect();
    Ok(json!({ "weeks": weeks, "utc_offset_minutes": offset, "cells": cells }))
}

/// VIP members: active now, renewed this month, lapsed lately (when), each
/// member's period end and how they pay.
pub async fn vip(server: &Server) -> StoreResult<Value> {
    let t = now();
    let today = date(t);
    let (y, m): (i64, i64) = (today[..4].parse().unwrap_or(1970), today[5..7].parse().unwrap_or(1));
    let month_start = month_start(y, m);
    let members = server.store.vip_members(t - 90 * 86_400).await?;
    let source = |v: &crate::store::VipMember| {
        if v.subscriber && v.stripe_months > 0 {
            "stripe"
        } else if v.store_months > 0 {
            "microsoft_store"
        } else if v.gifted_months > 0 {
            "gift"
        } else {
            "staff"
        }
    };
    let active: Vec<&crate::store::VipMember> =
        members.iter().filter(|v| v.vip_until > t).collect();
    let renewed =
        active.iter().filter(|v| v.last_at >= month_start && v.first_at < month_start).count();
    let new = active.iter().filter(|v| v.first_at >= month_start).count();
    let lapsed: Vec<Value> = members
        .iter()
        .filter(|v| v.vip_until <= t)
        .map(|v| {
            json!({
                "username": v.username, "country": v.country, "ended_at": v.vip_until,
                "source": source(v), "months": v.stripe_months + v.store_months + v.gifted_months,
            })
        })
        .collect();
    let mut by_source: BTreeMap<&str, usize> = BTreeMap::new();
    for v in &active {
        *by_source.entry(source(v)).or_default() += 1;
    }
    Ok(json!({
        "active": active.len(),
        "renewed_this_month": renewed,
        "new_this_month": new,
        "lapsed_90d": lapsed.len(),
        "by_source": by_source,
        "members": active.iter().map(|v| json!({
            "username": v.username, "country": v.country, "until": v.vip_until,
            "source": source(v), "since": v.first_at, "last_payment": v.last_at,
            "months": v.stripe_months + v.store_months + v.gifted_months,
            "subscriber": v.subscriber,
        })).collect::<Vec<_>>(),
        "lapsed": lapsed,
        "now": t,
    }))
}

/// One CSV line: fields quoted when they hold a comma, a quote or a line
/// break (quotes doubled), and never read as a formula by a spreadsheet.
pub fn csv_line(fields: &[String]) -> String {
    let mut out = String::new();
    for (i, f) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        // A spreadsheet runs a cell that starts with these as a formula.
        let f = if f.starts_with(['=', '+', '-', '@', '\t', '\r']) && f.parse::<f64>().is_err() {
            format!("'{f}")
        } else {
            f.clone()
        };
        if f.contains([',', '"', '\n', '\r']) {
            out.push('"');
            out.push_str(&f.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(&f);
        }
    }
    out.push_str("\r\n");
    out
}

/// Everything scheduled between `from` and `to` (Unix seconds): events,
/// promotions, official tournaments (the weekly cup's coming ones too),
/// maintenance, auctions and VIP's items of the month, by start.
pub async fn calendar(server: &Server, from: i64, to: i64) -> StoreResult<Value> {
    let t = now();
    let (events, promotions, officials, maintenance, auctions) = tokio::join!(
        server.store.recent_events(2000),
        server.store.promotions(2000),
        server.store.officials(1000),
        server.store.maintenances(500),
        server.store.auctions(1000),
    );
    let overlaps = |s: i64, e: i64| s < to && e > from;
    let mut items: Vec<Value> = Vec::new();
    let mut push = |kind: &str, title: &str, s: i64, e: i64, id: Value, status: &str| {
        if overlaps(s, e) {
            items.push(json!({
                "kind": kind, "title": title, "starts_at": s, "ends_at": e, "id": id,
                "status": status,
            }));
        }
    };
    let status = |s: i64, e: i64, cancelled: bool| {
        if cancelled {
            "cancelled"
        } else if e <= t {
            "ended"
        } else if s <= t {
            "live"
        } else {
            "scheduled"
        }
    };
    for e in events? {
        let kind = if e.kind == "tournament" { "tournament" } else { "event" };
        let title = if e.kind == "tournament" {
            e.title.clone()
        } else {
            format!("{} ×{} {}", e.title, e.multiplier, e.kind)
        };
        let st = status(e.starts_at, e.ends_at, e.cancelled_at.is_some());
        push(kind, &title, e.starts_at, e.ends_at, json!(e.id), st);
    }
    for p in promotions? {
        let st = status(p.starts_at, p.ends_at, p.cancelled_at.is_some());
        push("promotion", &p.title, p.starts_at, p.ends_at, json!(p.id), st);
    }
    for (id, o) in officials? {
        if o.event_id.is_none() {
            push("weekly_cup", &o.name, o.starts_at, o.ends_at, json!(id), "ended");
        }
    }
    // The weekly cup's coming occurrences, as set now.
    if let Some(cup) = server.ops.settings.cup() {
        let length = if cup.format == snook_legend_proto::TournamentFormat::Arena {
            cup.minutes as i64 * 60
        } else {
            2 * 3600
        };
        let mut at = cup.next_start(from.max(t));
        while at < to {
            push("weekly_cup", &cup.name, at, at + length, Value::Null, "scheduled");
            at = cup.next_start(at + 1);
        }
    }
    for m in maintenance? {
        let e = m.at + m.minutes as i64 * 60;
        let st = if m.started_at.is_some() {
            "started"
        } else {
            status(m.at, e, m.cancelled_at.is_some())
        };
        let title =
            if m.message.is_empty() { "Maintenance".to_string() } else { m.message.clone() };
        push("maintenance", &title, m.at, e, json!(m.id), st);
    }
    for a in auctions? {
        let name = shop::item(&a.code).map_or(a.code.as_str(), |i| i.name);
        let st = status(a.starts_at, a.ends_at, a.cancelled_at.is_some());
        push("auction", name, a.starts_at, a.ends_at, json!(a.id), st);
    }
    // VIP's item of each month.
    let first = date(from);
    let (mut y, mut m): (i64, i64) =
        (first[..4].parse().unwrap_or(1970), first[5..7].parse().unwrap_or(1));
    loop {
        let start = month_start(y, m);
        if start >= to {
            break;
        }
        let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
        let end = month_start(ny, nm);
        let month = format!("{y}-{m:02}");
        let code = if crate::ops::month_of(t) == month {
            crate::ops::vip_item_now(server, t)
        } else {
            shop::vip_month_item(&month).map(|i| i.code)
        };
        if let Some(code) = code {
            let name = shop::item(code).map_or(code, |i| i.name);
            push("vip_item", name, start, end, json!(code), status(start, end, false));
        }
        (y, m) = (ny, nm);
    }
    items.sort_by_key(|v| {
        (v["starts_at"].as_i64().unwrap_or(0), v["kind"].as_str().unwrap_or("").to_string())
    });
    Ok(json!({ "from": from, "to": to, "items": items, "now": t }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_months() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(1_795_291_200), "2026-11-21");
        assert_eq!(month_start(2026, 11), 1_793_491_200);
        assert_eq!(date(month_start(2024, 2)), "2024-02-01");
        assert_eq!(date(month_start(2024, 3) - 1), "2024-02-29");
    }

    #[test]
    fn coins_are_sorted_by_where_they_come_from_and_go() {
        assert_eq!(source_of("game"), "games");
        assert_eq!(source_of("daily bonus"), "daily_bonus");
        assert_eq!(source_of("daily challenge"), "daily_challenge");
        assert_eq!(source_of("staff: goodwill"), "staff");
        assert_eq!(source_of("pack coins_500"), "purchases");
        assert_eq!(source_of("auction refund #3"), "refunds");
        assert_eq!(sink_of("buy cloth_blue"), "shop:cloth");
        assert_eq!(sink_of("gift cue_maple to 12"), "gifts:cue");
        assert_eq!(sink_of("tournament entry"), "tournament_fees");
        assert_eq!(sink_of("auction bid #3"), "auctions");
        assert_eq!(sink_of("name change"), "name_changes");
    }

    #[test]
    fn the_economy_has_a_verdict() {
        assert_eq!(verdict(10, 1000).0, "healthy");
        assert_eq!(verdict(50, 1000).0, "watch");
        assert_eq!(verdict(200, 1000).0, "inflating");
        assert_eq!(verdict(-100, 1000).0, "deflating");
        assert_eq!(verdict(5, 0).0, "unknown");
    }

    #[test]
    fn csv_fields_are_escaped() {
        let line = csv_line(&[
            "plain".into(),
            "a,b".into(),
            "say \"hi\"".into(),
            "=HYPERLINK(\"x\")".into(),
            "-12".into(),
        ]);
        assert_eq!(line, "plain,\"a,b\",\"say \"\"hi\"\"\",\"'=HYPERLINK(\"\"x\"\")\",-12\r\n");
    }
}
