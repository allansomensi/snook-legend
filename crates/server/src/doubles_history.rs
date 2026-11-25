//! Doubles games in a player's history and their replays (0.12.1).
//!
//! A doubles game is a [`HistoryEntry`] like a singles one, with the
//! partner and both opponents in `doubles`; its `match_id` is the doubles
//! game's id made negative (a singles game's is positive), so that a
//! `replay { match_id }` for it finds it. A 0.12.0 game, which doesn't know
//! `doubles`, shows it as a game against "caio & duda" with no button to
//! watch it (it only offers one for a positive id).

use snook_legend_proto::{DoublesSide, HistoryEntry, ReplayInfo};

use crate::store::{DoublesGame, User};
use crate::{Server, now};

/// How a team is named where one name goes: "ana & bia".
pub fn team_name(names: &[String; 2]) -> String {
    format!("{} & {}", names[0], names[1])
}

/// `user_id`'s line in their history for doubles game `g` (none: not
/// theirs).
pub fn entry(g: &DoublesGame, user_id: i64) -> Option<HistoryEntry> {
    let (team, place) = g.place_of(user_id)?;
    let other = 1 - team;
    let rated = g.record.rated && g.voided_at.is_none();
    Some(HistoryEntry {
        opponent: team_name(&g.names[other]),
        opponent_country: g.countries[other][0].clone(),
        opponent_rating: 0,
        opponent_admin: false,
        won: g.record.winner == team,
        reason: g.record.reason,
        rated,
        rules: g.record.rules,
        delta: if rated { g.record.deltas[team][place] } else { 0 },
        rating_after: 0,
        tournament: false,
        at: g.record.ended_at,
        match_id: -g.id,
        replay: g.has_replay,
        correspondence: false,
        doubles: Some(DoublesSide {
            partner: g.names[team][1 - place].clone(),
            opponents: g.names[other].clone(),
        }),
    })
}

/// `history` (singles, newest first) with `user`'s latest doubles games in
/// it by date, `limit` lines at most.
pub async fn merged(
    server: &Server,
    user_id: i64,
    mut history: Vec<HistoryEntry>,
    limit: usize,
) -> Vec<HistoryEntry> {
    let games = server.store.doubles_games(Some(user_id), None, limit as u32).await;
    let games = games.unwrap_or_else(|e| {
        tracing::warn!("doubles games of {user_id}: {e}");
        Vec::new()
    });
    history.extend(games.iter().filter_map(|g| entry(g, user_id)));
    history.sort_by(|a, b| b.at.cmp(&a.at));
    history.truncate(limit);
    history
}

/// The replay of doubles game `id`, for one of its four players or staff:
/// the teams as its two sides ("ana & bia" against "caio & duda").
pub async fn replay(server: &Server, me: &User, id: i64) -> Option<ReplayInfo> {
    let g = server.store.doubles_game(id).await.ok().flatten()?;
    if !me.role.is_staff() && g.place_of(me.id).is_none() {
        return None;
    }
    let replay = g.record.replay.clone()?;
    // The teams' doubles ratings now (what is kept of them).
    let mut ratings = [0, 0];
    for (team, players) in g.record.players.iter().enumerate() {
        let mut sum = 0;
        for id in players {
            let r = server.store.doubles_records(*id).await.ok()?;
            sum += r.get(g.record.rules).rating;
        }
        ratings[team] = sum / 2;
    }
    Some(ReplayInfo {
        match_id: -g.id,
        players: [team_name(&g.names[0]), team_name(&g.names[1])],
        countries: [g.countries[0][0].clone(), g.countries[1][0].clone()],
        ratings,
        winner: g.record.winner,
        rated: g.record.rated && g.voided_at.is_none(),
        at: g.record.ended_at.min(now()),
        replay,
    })
}
