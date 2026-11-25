//! Official tournaments: the weekly cup and the staff's (scheduled from the
//! portal as events, see `crate::events`). The lounge runs them like any
//! other; here they are remembered once over (the website shows their
//! brackets and standings, and the champions' trophies link to them), their
//! players are reminded before they start and told how they did, and the
//! champion gets the item prize.

use std::collections::HashSet;
use std::sync::Mutex;

use serde_json::{Value, json};
use snook_legend_proto::collection::Origin;
use snook_legend_proto::inbox::{Action, Kind};
use snook_legend_proto::{Rules, TournamentFormat};

use crate::inbox;
use crate::store::OfficialRecord;
use crate::{Server, now};

/// Players are reminded this long before a tournament they signed up for
/// starts (seconds).
pub const REMIND_BEFORE: i64 = 10 * 60;

/// A player of an official tournament, by seed.
#[derive(Debug, Clone, PartialEq)]
pub struct Entrant {
    pub user_id: i64,
    pub username: String,
    pub country: String,
    pub seed: u32,
}

/// An official tournament just crowned (see `Lounge::crown`).
#[derive(Debug, Clone, PartialEq)]
pub struct Finished {
    /// The lounge's id for it.
    pub lounge_id: u64,
    pub event_id: Option<i64>,
    pub name: String,
    pub rules: Rules,
    pub format: TournamentFormat,
    pub size: u32,
    pub rated: bool,
    pub entry_fee: u32,
    pub prize_pool: u64,
    /// Coins paid to the champion and the runner-up.
    pub prizes: [u32; 2],
    pub item_prize: String,
    pub starts_at: i64,
    pub ends_at: i64,
    pub players: Vec<Entrant>,
    /// A knockout's rounds, first first: (player, player, winner) by
    /// account (None: a bye, or nobody yet).
    pub rounds: Vec<Vec<[Option<i64>; 3]>>,
    /// An arena's standings, best first: (account, points, games, streak).
    pub standings: Vec<(i64, u32, u32, u32)>,
    /// Each player's place and coins won, the champion first.
    pub places: Vec<(i64, u32, i64)>,
    pub champion: Option<i64>,
    pub runner_up: Option<i64>,
}

/// Who was reminded of which tournament (lounge id, account).
#[derive(Default)]
pub struct Reminders {
    told: Mutex<HashSet<(u64, i64)>>,
}

fn format_code(f: TournamentFormat) -> &'static str {
    match f {
        TournamentFormat::Knockout => "knockout",
        TournamentFormat::Arena => "arena",
    }
}

/// What is kept of a finished tournament: the bracket and standings with
/// the players' ids (their names now are looked up when it is read; the
/// names then stay as a fallback).
pub fn record_of(f: &Finished) -> OfficialRecord {
    let name_of = |id: i64| {
        f.players.iter().find(|p| p.user_id == id).map(|p| p.username.clone()).unwrap_or_default()
    };
    let detail = json!({
        "players": f.players.iter().map(|p| json!({
            "id": p.user_id, "username": p.username, "country": p.country, "seed": p.seed,
        })).collect::<Vec<_>>(),
        "rounds": f.rounds.iter().map(|round| round.iter().map(|[a, b, w]| json!({
            "a": a, "b": b, "winner": w,
        })).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "standings": f.standings.iter().map(|(id, points, games, streak)| json!({
            "id": id, "username": name_of(*id), "points": points, "games": games, "streak": streak,
        })).collect::<Vec<_>>(),
    });
    OfficialRecord {
        key: format!("{}:{}", f.lounge_id, f.starts_at),
        event_id: f.event_id,
        name: f.name.clone(),
        rules: f.rules,
        format: format_code(f.format).into(),
        size: f.size,
        players: f.players.len() as u32,
        rated: f.rated,
        entry_fee: f.entry_fee,
        prize_pool: f.prize_pool as i64,
        prizes: f.prizes.iter().map(|&c| c as i64).collect(),
        item_prize: f.item_prize.clone(),
        champion_id: f.champion,
        runner_up_id: f.runner_up,
        detail: detail.to_string(),
        starts_at: f.starts_at,
        ends_at: f.ends_at,
    }
}

/// Keeps a finished official tournament, gives the champion the item prize
/// and tells every player their place.
pub async fn finished(server: &Server, f: &Finished) {
    let record = record_of(f);
    let id = match crate::retrying("keeping an official tournament", || {
        server.store.record_official(&record)
    })
    .await
    {
        Ok(Some(id)) => id,
        // Kept already (a write tried again): its players were told then.
        Ok(None) => return,
        Err(e) => {
            tracing::error!("could not keep the official tournament {}: {e}", f.name);
            return;
        }
    };
    if let Some(champion) = f.champion
        && snook_legend_proto::shop::item(&f.item_prize).is_some()
    {
        let origin = Origin::Tournament { name: f.name.clone(), place: 1 };
        award_item(server, champion, &f.item_prize, origin).await;
    }
    for &(user_id, place, prize) in &f.places {
        let kind = Kind::TournamentResult { id: id as u64, name: f.name.clone(), place, prize };
        inbox::send(server, user_id, inbox::New::game(kind)).await;
    }
}

/// Gives `user_id` an item they won (the champion's prize, an auction, VIP's
/// item of the month), records how, and tells them (with a button to put
/// it on). False when they had it already.
pub async fn award_item(server: &Server, user_id: i64, code: &str, origin: Origin) -> bool {
    let source = match &origin {
        Origin::Tournament { .. } => "tournament",
        Origin::Auction { .. } => "auction",
        Origin::VipMonth { .. } => "vip-month",
        _ => "prize",
    };
    match server.store.grant_item(user_id, code, source, now()).await {
        Ok(true) => {}
        Ok(false) => return false,
        Err(e) => {
            tracing::error!("item {code} for {user_id}: {e}");
            return false;
        }
    }
    item_received(server, user_id, code, origin).await;
    true
}

/// An item `user_id` now has (given already): its origin is recorded, they
/// are told, and their wallet is sent again.
pub async fn item_received(server: &Server, user_id: i64, code: &str, origin: Origin) {
    crate::collection::record(server, user_id, code, origin.clone()).await;
    let kind = match &origin {
        Origin::VipMonth { month } => Kind::VipItem { code: code.into(), month: month.clone() },
        _ => Kind::ItemAwarded { code: code.into(), origin },
    };
    let n = inbox::New::game(kind).action(Action::Equip { code: code.into() });
    inbox::send(server, user_id, n).await;
    if let Ok(Some(user)) = server.store.user_by_id(user_id).await {
        crate::shop::push_wallet(server, &user).await;
    }
}

/// Reminds the players of the official tournaments starting within
/// [`REMIND_BEFORE`] (once each).
pub async fn remind(server: &Server) {
    let t = now();
    let soon = server.with(|l| l.official_starting(t + REMIND_BEFORE));
    let mut due = Vec::new();
    {
        let mut told = server.ops.reminders.told.lock().unwrap_or_else(|e| e.into_inner());
        // Those long started are forgotten.
        told.retain(|(id, _)| soon.iter().any(|s| s.id == *id));
        for s in &soon {
            for &user in &s.players {
                if told.insert((s.id, user)) {
                    due.push((user, s.clone()));
                }
            }
        }
    }
    for (user, s) in due {
        let kind =
            Kind::TournamentStarting { id: s.id, name: s.name.clone(), starts_at: s.starts_at };
        inbox::send(server, user, inbox::New::game(kind).action(Action::Tournament { id: s.id }))
            .await;
    }
}

/// A kept tournament with its players' names as they are now (an account
/// renamed shows its new name; a deleted one its anonymous name).
pub async fn detail_with_names(server: &Server, detail: &str) -> Value {
    let mut v: Value = serde_json::from_str(detail).unwrap_or_else(|_| json!({}));
    let mut ids: Vec<i64> = Vec::new();
    for key in ["players", "standings"] {
        if let Some(list) = v[key].as_array() {
            ids.extend(list.iter().filter_map(|p| p["id"].as_i64()));
        }
    }
    ids.sort_unstable();
    ids.dedup();
    let mut names = std::collections::HashMap::new();
    for id in ids {
        if let Ok(Some(u)) = server.store.user_by_id(id).await {
            names.insert(id, (u.username, u.country));
        }
    }
    for key in ["players", "standings"] {
        if let Some(list) = v[key].as_array_mut() {
            for p in list {
                if let Some((name, country)) = p["id"].as_i64().and_then(|id| names.get(&id)) {
                    p["username"] = json!(name);
                    if key == "players" {
                        p["country"] = json!(country);
                    }
                }
            }
        }
    }
    v["names"] = json!(
        names
            .iter()
            .map(|(id, (n, _))| (id.to_string(), n))
            .collect::<std::collections::BTreeMap<_, _>>()
    );
    v
}
