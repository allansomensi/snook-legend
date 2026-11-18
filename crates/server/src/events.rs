//! Events the staff schedule from the portal (see `snook_legend_proto::events`):
//! experience or coins that count more for a while, and themed tournaments
//! the server organises. The events not over yet are kept here (read again
//! every minute and after every change in the portal); players are told
//! about those running and coming when they sign in and whenever the list
//! changes.

use std::sync::Arc;
use std::time::Duration;

use snook_legend_proto::events::{self, EventInfo, EventKind};
use snook_legend_proto::{ServerMsg, TournamentFormat};

use crate::lounge::ScheduledTournament;
use crate::store::{GameEvent, User};
use crate::{Server, now};

/// The events not over yet, and what players were last told.
#[derive(Default)]
pub struct Events {
    list: std::sync::RwLock<Vec<GameEvent>>,
    told: std::sync::Mutex<Vec<EventInfo>>,
}

/// How often the events are read again (a boost starts or ends on time
/// anyway: the times are in the list).
const RELOAD: Duration = Duration::from_secs(60);

pub fn info(e: &GameEvent) -> EventInfo {
    EventInfo {
        id: e.id,
        kind: EventKind::from_code(&e.kind),
        title: e.title.clone(),
        multiplier: e.multiplier,
        starts_at: e.starts_at,
        ends_at: e.ends_at,
    }
}

impl Server {
    /// What experience (or coins from games) count now: 1, or the largest
    /// boost running.
    pub fn boost(&self, kind: EventKind) -> u32 {
        if self.config.rules.lan {
            return 1;
        }
        let list = self.events.list.read().unwrap_or_else(|e| e.into_inner());
        let infos: Vec<EventInfo> = list.iter().map(info).collect();
        events::boost(&infos, kind, now())
    }

    /// The events players are shown at `t`: running, or starting within
    /// [`events::SHOWN_AHEAD`], soonest first.
    pub fn shown_events(&self, t: i64) -> Vec<EventInfo> {
        let list = self.events.list.read().unwrap_or_else(|e| e.into_inner());
        list.iter()
            .filter(|e| {
                e.cancelled_at.is_none() && e.ends_at > t && e.starts_at <= t + events::SHOWN_AHEAD
            })
            .map(info)
            .collect()
    }
}

/// Reads the events again: the boosts follow, the lounge learns of the
/// themed tournaments, and players are told if what they see changed.
pub async fn reload(server: &Server) {
    if server.config.rules.lan {
        return;
    }
    let t = now();
    let list = match server.store.events_after(t).await {
        Ok(list) => list,
        Err(e) => {
            tracing::warn!("events: {e}");
            return;
        }
    };
    let tournaments: Vec<ScheduledTournament> = list
        .iter()
        .filter(|e| EventKind::from_code(&e.kind) == EventKind::Tournament)
        .map(|e| ScheduledTournament {
            event_id: e.id,
            name: e.title.clone(),
            format: if e.format == "arena" {
                TournamentFormat::Arena
            } else {
                TournamentFormat::Knockout
            },
            size: e.size,
            rated: e.rated,
            entry_fee: e.entry_fee,
            house_prize: e.prize,
            starts_at: e.starts_at,
            ends_at: e.ends_at,
        })
        .collect();
    *server.events.list.write().unwrap_or_else(|e| e.into_inner()) = list;
    server.with(|l| l.set_scheduled_tournaments(tournaments));
    let shown = server.shown_events(t);
    let changed = {
        let mut told = server.events.told.lock().unwrap_or_else(|e| e.into_inner());
        let changed = *told != shown;
        *told = shown.clone();
        changed
    };
    if changed {
        server.with(|l| l.tell_everyone(&ServerMsg::Events { events: shown }));
    }
}

/// Keeps the events current while the server runs.
pub async fn run(server: Arc<Server>) {
    loop {
        reload(&server).await;
        tokio::time::sleep(RELOAD).await;
    }
}

/// A player who signed in learns of the events running and coming.
pub fn signed_in(server: &Server, user: &User) {
    let events = server.shown_events(now());
    server.with(|l| l.notify(&user.username, ServerMsg::Events { events }));
}
