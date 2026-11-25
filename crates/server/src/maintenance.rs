//! Maintenance scheduled from the portal: players are warned ahead (a
//! banner with a countdown in the game, and notices in the lounge 60, 30,
//! 15, 5 and 1 minutes before); at the time the server stops starting games
//! (the drain of a restart: games in progress go on and are handed over),
//! and the process ends cleanly so the host starts it again (`main.rs`; see
//! `docs/DEPLOY.md`).

use std::sync::{Mutex, RwLock};

use snook_legend_proto::ServerMsg;
use tokio::sync::Notify;

use crate::store::MaintenanceRow;
use crate::{Server, now};

/// The minutes before a maintenance when the lounge is told.
pub const NOTICES: [i64; 5] = [60, 30, 15, 5, 1];
/// How long games in progress may go on once it starts (seconds), unless
/// the host's own drain (`SNOOK_LEGEND_DRAIN_SECS`) says otherwise.
pub const DRAIN_SECS: u64 = 240;
/// The longest a maintenance may be announced to last (minutes).
pub const MAX_MINUTES: u32 = 24 * 60;
/// The longest message.
pub const MAX_MESSAGE: usize = 200;

/// The next maintenance, and what was said about it.
#[derive(Default)]
pub struct State {
    next: RwLock<Option<MaintenanceRow>>,
    /// What players were last told (id and time; None: nothing planned),
    /// and the notices given for it.
    told: Mutex<(Option<(i64, i64, u32, String)>, Vec<i64>)>,
    /// Wakes `main.rs` when it is time to go down.
    due: Notify,
}

impl State {
    /// Waits until a maintenance starts on this server.
    pub async fn due(&self) {
        self.due.notified().await
    }
}

/// Reads the maintenance planned again (after every change in the portal,
/// and every minute).
pub async fn reload(server: &Server) {
    if server.config.settings.lan {
        return;
    }
    match server.store.maintenances(20).await {
        Ok(rows) => {
            let next = rows
                .into_iter()
                .filter(|r| r.cancelled_at.is_none() && r.started_at.is_none())
                .min_by_key(|r| (r.at, r.id));
            *server.ops.maintenance.next.write().unwrap_or_else(|e| e.into_inner()) = next;
        }
        Err(e) => tracing::warn!("maintenance: {e}"),
    }
}

/// The maintenance planned (not started nor called off), if any.
pub fn next(server: &Server) -> Option<MaintenanceRow> {
    server.ops.maintenance.next.read().unwrap_or_else(|e| e.into_inner()).clone()
}

fn message_of(next: Option<&MaintenanceRow>) -> ServerMsg {
    match next {
        Some(m) => {
            ServerMsg::Maintenance { at: m.at, minutes: m.minutes, message: m.message.clone() }
        }
        None => ServerMsg::Maintenance { at: 0, minutes: 0, message: String::new() },
    }
}

/// What a player who signs in is told (nothing when none is planned).
pub fn message(server: &Server) -> Option<ServerMsg> {
    next(server).map(|m| message_of(Some(&m)))
}

/// Tells players when the plan changed, gives the notices, and starts the
/// maintenance at its time.
pub async fn tick(server: &Server) {
    if server.config.settings.lan {
        return;
    }
    let t = now();
    let next = next(server);
    let key = next.as_ref().map(|m| (m.id, m.at, m.minutes, m.message.clone()));
    let mut notice = None;
    {
        let mut told = server.ops.maintenance.told.lock().unwrap_or_else(|e| e.into_inner());
        if told.0 != key {
            let first = told.0.is_none() && key.is_none();
            told.0 = key.clone();
            told.1.clear();
            if !first {
                let msg = message_of(next.as_ref());
                server.with(|l| l.tell_everyone(&msg));
            }
        }
        if let Some(m) = &next {
            let left = (m.at - t + 59).div_euclid(60);
            // The nearest mark at or after the minutes left, once (one set
            // for in 20 minutes is said now, then at 15, 5 and 1).
            if let Some(&mark) = NOTICES.iter().rev().find(|&&n| left <= n)
                && !told.1.contains(&mark)
                && left > 0
            {
                told.1.extend(NOTICES.iter().filter(|&&n| n >= mark));
                notice = Some(left);
            }
        }
    }
    if let Some(left) = notice {
        let text = format!("The server stops for maintenance in {left} min.");
        server.with(|l| l.announce_text(&text));
    }
    let Some(m) = next.filter(|m| m.at <= t) else { return };
    // One server goes down for it (another one may share the database).
    match server.store.start_maintenance(m.id, t).await {
        Ok(true) => {}
        Ok(false) => {
            reload(server).await;
            return;
        }
        Err(e) => {
            tracing::error!("maintenance {}: {e}", m.id);
            return;
        }
    }
    tracing::warn!(id = m.id, minutes = m.minutes, "maintenance: stopping");
    server.with(|l| {
        l.announce_text("The server stops for maintenance now. Games in progress go on.");
        l.start_drain(DRAIN_SECS);
    });
    reload(server).await;
    server.ops.maintenance.due.notify_one();
}
