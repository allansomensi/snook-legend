//! The secret achievements (see `snook_legend_proto::feats`): each rated
//! game that counts is read back once it ends, and the feats seen in it are
//! awarded to whoever did them.

use std::sync::Arc;

use snook_legend_proto::feats::detect;

use crate::Server;
use crate::store::MatchRecord;

/// Reads a finished game (on a blocking thread: it simulates every shot)
/// and awards the secret achievements its players earned in it.
pub async fn after_match(server: &Arc<Server>, record: &MatchRecord) {
    // Rated games only, and not two friends farming each other.
    if !record.rated || !record.counts {
        return;
    }
    let Some(replay) = record.replay.clone() else { return };
    if replay.strokes.is_empty() {
        return;
    }
    let players = record.players;
    let found = tokio::task::spawn_blocking(move || [detect(&replay, 0), detect(&replay, 1)]).await;
    let Ok(found) = found else { return };
    for (seat, feats) in found.into_iter().enumerate() {
        let feats = match feats {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("feats of {players:?}: the replay failed: {e}");
                return;
            }
        };
        if feats.is_empty() {
            continue;
        }
        let Ok(Some(user)) = server.store.user_by_id(players[seat]).await else { continue };
        for code in feats {
            tracing::info!(user = user.id, code, "secret achievement");
            server.award_achievement(&user, code).await;
        }
    }
}
