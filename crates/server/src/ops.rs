//! Operations and the economy, as a player meets them: what the server
//! keeps for them (live settings, promotions, maintenance, reminders), the
//! background work that keeps it current, and the player's own messages
//! (auctions, the supporters' credits). The daily bonus's streak and VIP's
//! item of the month are given here too.

use std::sync::Arc;
use std::time::Duration;

use snook_legend_proto::collection::Origin;
use snook_legend_proto::ops::Supporter;
use snook_legend_proto::{ClientMsg, ServerMsg};

use crate::store::{User, day_of};
use crate::{Config, Server, auction, maintenance, now, official, promotions, settings};

/// What the server keeps for operations (one per server).
pub struct Ops {
    pub settings: settings::Live,
    pub promotions: promotions::Promotions,
    pub maintenance: maintenance::State,
    pub reminders: official::Reminders,
    pub auctions: auction::Told,
}

impl Ops {
    pub fn new(config: &Config) -> Ops {
        Ops {
            settings: settings::Live::new(config),
            promotions: Default::default(),
            maintenance: Default::default(),
            reminders: Default::default(),
            auctions: Default::default(),
        }
    }
}

/// How often promotions, maintenance, auctions and reminders are looked at.
const TICK: Duration = Duration::from_secs(10);

/// Reads what the portal set (at start-up, before anyone signs in).
pub async fn init(server: &Server) {
    settings::reload(server).await;
    if server.config.settings.lan {
        return;
    }
    promotions::reload(server).await;
    maintenance::reload(server).await;
}

/// Keeps promotions, maintenance, auctions and reminders current while the
/// server runs.
pub async fn run(server: Arc<Server>) {
    let mut interval = tokio::time::interval(TICK);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut ticks: u64 = 0;
    loop {
        interval.tick().await;
        ticks += 1;
        // What the portal changed on another server (or in the database)
        // comes in within a minute.
        if ticks % 6 == 0 {
            settings::reload(&server).await;
            promotions::reload(&server).await;
            maintenance::reload(&server).await;
        }
        promotions::tick(&server);
        maintenance::tick(&server).await;
        auction::tick(&server).await;
        official::remind(&server).await;
    }
}

/// A player signed in: the promotions, a maintenance planned, the auctions
/// open, and VIP's item of the month if it is theirs now.
pub async fn signed_in(server: &Server, user: &User) {
    if server.config.settings.lan {
        return;
    }
    let msgs = [promotions::message(server), maintenance::message(server)];
    server.with(|l| {
        for m in msgs.into_iter().flatten() {
            l.notify(&user.username, m);
        }
    });
    auction::signed_in(server, user).await;
    vip_month_item(server, user).await;
}

/// The month `YYYY-MM` (UTC) at `t`.
pub fn month_of(t: i64) -> String {
    crate::analytics::date(t)[..7].to_string()
}

/// VIP's item of this month (the portal's choice, else the catalogue's
/// `vip_month_YYYY_MM`): every member VIP this month gets it, for good.
pub fn vip_item_now(server: &Server, t: i64) -> Option<&'static str> {
    let chosen = server.ops.settings.text(settings::VIP_MONTH_ITEM);
    if let Some(item) = snook_legend_proto::shop::item(&chosen) {
        return Some(item.code);
    }
    snook_legend_proto::shop::vip_month_item(&month_of(t)).map(|i| i.code)
}

/// Gives `user` VIP's item of the month when they are VIP and don't have it
/// yet (signing in, a renewal).
pub async fn vip_month_item(server: &Server, user: &User) {
    let t = now();
    let Some(code) = vip_item_now(server, t) else { return };
    let Ok(w) = server.store.wallet(user.id).await else { return };
    if !crate::shop::is_vip(&w, t) || w.owned.iter().any(|o| o == code) {
        return;
    }
    let origin = Origin::VipMonth { month: month_of(t) };
    if official::award_item(server, user.id, code, origin).await {
        tracing::info!(user = user.id, item = code, "VIP's item of the month");
    }
}

/// The daily bonus, once a day, with its streak: more for each day in a
/// row (see `settings::DAILY_*`). The player is told the day, the coins and
/// what tomorrow brings.
pub async fn daily_bonus(server: &Server, user: &User) {
    let live = &server.ops.settings;
    let (base, step) = (live.int(settings::DAILY_BASE), live.int(settings::DAILY_STEP));
    let days = live.int(settings::DAILY_DAYS).clamp(1, 1000) as u32;
    let day = day_of(now());
    match server.store.claim_daily_streak(user.id, day, base, step, days).await {
        Ok(Some(claim)) => {
            let tomorrow = crate::store::streak_coins(claim.day + 1, base, step, days);
            server.with(|l| {
                l.notify(
                    &user.username,
                    ServerMsg::CoinsEarned { amount: claim.coins as u32, reason: "daily".into() },
                );
                l.notify(
                    &user.username,
                    ServerMsg::DailyStreak {
                        day: claim.day,
                        coins: claim.coins as u32,
                        tomorrow: tomorrow as u32,
                    },
                );
            });
        }
        Ok(None) => {}
        Err(e) => tracing::error!("daily bonus of {}: {e}", user.username),
    }
}

/// The supporters in the credits, as players see them.
pub async fn supporters(server: &Server) -> Vec<Supporter> {
    let t = now();
    match server.store.supporters(t).await {
        Ok(rows) => rows
            .into_iter()
            .map(|r| Supporter {
                username: r.username,
                country: r.country,
                since: r.since,
                months: r.months,
                vip: r.vip_until > t,
            })
            .collect(),
        Err(e) => {
            tracing::error!("supporters: {e}");
            Vec::new()
        }
    }
}

pub fn is_msg(msg: &ClientMsg) -> bool {
    matches!(
        msg,
        ClientMsg::Auctions
            | ClientMsg::Bid { .. }
            | ClientMsg::SetCredits { .. }
            | ClientMsg::Supporters
    )
}

/// A player's message about auctions or the credits.
pub async fn handle(server: &Server, me: &User, msg: ClientMsg, send: &impl Fn(ServerMsg)) {
    let error = |text: &str| send(ServerMsg::Error { message: text.to_string() });
    match msg {
        ClientMsg::Auctions => send(ServerMsg::Auctions { list: auction::shown(server).await }),
        ClientMsg::Bid { id, amount } => auction::bid(server, me, id, amount, send).await,
        ClientMsg::SetCredits { show } => {
            let Ok(w) = server.store.wallet(me.id).await else {
                return error("Something went wrong. Please try again.");
            };
            if show && w.vip_until == 0 {
                return error("The credits are for VIP members, now or before.");
            }
            if server.store.set_credits(me.id, show, now()).await.is_err() {
                return error("Something went wrong. Please try again.");
            }
            let payment = server.store.payment_email(me.id).await.ok().flatten();
            let fresh = server.store.user_by_id(me.id).await.ok().flatten();
            send(crate::account::account_msg(server, fresh.as_ref().unwrap_or(me), payment).await);
        }
        ClientMsg::Supporters => send(ServerMsg::Supporters { list: supporters(server).await }),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn months_are_named() {
        assert_eq!(super::month_of(1_795_291_200), "2026-11");
        assert_eq!(super::month_of(0), "1970-01");
    }
}
