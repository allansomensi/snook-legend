//! The shot analysis on profiles (see [`snook_legend_proto::analysis`]): each
//! stored game is replayed once when it ends and its shots are added to both
//! players' sums.

use std::sync::Arc;

use snook_legend_proto::ServerMsg;
use snook_legend_proto::analysis::analyse;

use crate::Server;
use crate::store::MatchRecord;

/// Replays a finished game (on a blocking thread: it simulates every shot)
/// and adds each player's shots to their analysis.
pub async fn after_match(server: &Arc<Server>, record: &MatchRecord) {
    let Some(replay) = record.replay.clone() else { return };
    if replay.strokes.is_empty() {
        return;
    }
    let players = record.players;
    let sums = match tokio::task::spawn_blocking(move || analyse(&replay)).await {
        Ok(Ok(sums)) => sums,
        Ok(Err(e)) => {
            tracing::warn!("shot analysis of {players:?}: the replay failed: {e}");
            return;
        }
        Err(e) => {
            tracing::error!("shot analysis of {players:?}: {e}");
            return;
        }
    };
    let week = snook_legend_proto::advanced::week_of(record.ended_at);
    for (seat, sums) in sums.iter().enumerate() {
        if let Err(e) = server.store.add_shot_analysis(players[seat], sums).await {
            tracing::warn!("shot analysis of {}: {e}", players[seat]);
        }
        // The week's pots, for the advanced statistics' progress.
        let pots = sums.attempts();
        if let Err(e) =
            server.store.add_shot_week(players[seat], week, pots.attempts, pots.made).await
        {
            tracing::warn!("shot week of {}: {e}", players[seat]);
        }
    }
}

/// Adds the analysis to a profile.
pub async fn attach(server: &Server, msg: &mut ServerMsg) {
    let ServerMsg::Profile { profile } = msg else { return };
    if profile.player.guest {
        return;
    }
    let Ok(Some(user)) = server.store.user_by_name(&profile.player.username).await else {
        return;
    };
    match server.store.shot_analysis(user.id).await {
        Ok(a) => profile.analysis = Some(Box::new(a)),
        Err(e) => tracing::warn!("shot analysis of {}: {e}", user.id),
    }
}
