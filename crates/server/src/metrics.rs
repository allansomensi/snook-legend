//! Who is online, sampled every minute into hourly buckets in the database
//! (a restart loses nothing): the staff panel charts active players, peak
//! players and games per hour from them.

use std::sync::Arc;
use std::time::Duration;

use crate::store::hour_of;
use crate::{Server, now};

pub const SAMPLE_EVERY: Duration = Duration::from_secs(60);
/// Who was online in which hour is kept this long (the peaks stay).
const KEEP_ACTIVE_HOURS: i64 = 7 * 24;

/// Samples now and then every [`SAMPLE_EVERY`].
pub fn spawn(server: Arc<Server>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(SAMPLE_EVERY);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut purged_hour = 0;
        loop {
            interval.tick().await;
            let t = now();
            sample(&server, t).await;
            if hour_of(t) != purged_hour {
                purged_hour = hour_of(t);
                let _ = server.store.purge_active_hours(purged_hour - KEEP_ACTIVE_HOURS).await;
            }
        }
    });
}

/// One sample: the accounts online, the players (guests too) and the games.
pub async fn sample(server: &Server, t: i64) {
    let (users, players, games) =
        server.with(|l| (l.online_user_ids(), l.online_count() as u32, l.running_games() as u32));
    if let Err(e) = server.store.sample_online(hour_of(t), &users, players, games).await {
        tracing::warn!("sampling who is online: {e}");
    }
}
