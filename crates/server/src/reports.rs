//! The game that goes with a report: the one being played between the two
//! players when it was sent or, failing that, their latest finished game
//! (within [`REPORT_GAME_WINDOW_SECS`]). A game in progress is linked to its
//! stored match once it has ended, the first time the staff look at it.

use serde_json::{Value, json};
use snookered_proto::REPORT_GAME_WINDOW_SECS;

use crate::Server;
use crate::store::{ReportRow, User};

/// After a report about a game in progress, how long its stored match is
/// looked for (games don't last longer).
const LIVE_LINK_WINDOW: i64 = 24 * 60 * 60;

/// The game a report from `reporter` about `target` at `at` goes with:
/// (stored match, room of the game in progress).
pub async fn game_for(
    server: &Server,
    reporter: &User,
    target: &User,
    at: i64,
) -> (Option<i64>, Option<i64>) {
    if let Some(id) = server.with(|l| l.live_game_between(&reporter.username, &target.username)) {
        return (None, Some(id as i64));
    }
    let recent = server
        .store
        .matches_between(reporter.id, target.id, at - REPORT_GAME_WINDOW_SECS, at, 1)
        .await
        .unwrap_or_default();
    (recent.first().map(|m| m.0), None)
}

/// The report's game for the staff panel: `{ kind, id, … }` with `kind`
/// `match` (a stored game: watch its replay), `live` (still being played)
/// or `gone` (it was being played but was never stored); `null` without one.
pub async fn game_json(server: &Server, r: &mut ReportRow) -> Value {
    if r.match_id.is_none()
        && let Some(live) = r.live_id
    {
        let room = server.with(|l| l.room_replay(live as u64));
        let live_json = |(players, rated, finished, replay): &([String; 2], bool, bool, _)| {
            let replay: &snookered_proto::replay::Replay = replay;
            json!({
                "kind": "live", "id": live, "players": players, "rated": rated,
                "finished": finished, "shots": replay.strokes.len(),
            })
        };
        if let Some(room) = room.as_ref().filter(|r| !r.2) {
            return live_json(room);
        }
        // It ended: the first game between them stored after the report.
        let found = server
            .store
            .matches_between(
                r.reporter_id,
                r.target_id,
                r.created_at,
                r.created_at + LIVE_LINK_WINDOW,
                50,
            )
            .await
            .unwrap_or_default();
        match found.last() {
            Some(&(id, _)) => {
                if let Err(e) = server.store.set_report_match(r.id, id).await {
                    tracing::warn!("linking report {} to match {id}: {e}", r.id);
                }
                r.match_id = Some(id);
            }
            // Just ended (not stored yet): still on this server.
            None => {
                return room
                    .as_ref()
                    .map(live_json)
                    .unwrap_or(json!({ "kind": "gone", "id": live }));
            }
        }
    }
    let Some(id) = r.match_id else { return Value::Null };
    match server.store.replay(id).await {
        Ok(Some(m)) => json!({
            "kind": "match", "id": id, "players": m.players, "rated": m.rated, "at": m.at,
            "winner": m.winner, "shots": m.replay.strokes.len(),
        }),
        _ => json!({ "kind": "gone", "id": id }),
    }
}
