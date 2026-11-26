//! The shot analysis, a VIP feature (see `snook_legend_proto::advanced`):
//! the player's whole analysis, their pots week by week, and the same
//! numbers for the players of their league. Only VIP members get it, and
//! only their own: profiles don't carry it.
//!
//! The weeks are added up when a game's analysis is (see
//! [`crate::analysis::after_match`]); a league's numbers sum many players'
//! analyses, so they are kept for a while (they change slowly).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use snook_legend_proto::advanced::{AdvancedStats, Sums, WEEKS, Week};
use snook_legend_proto::analysis::Tally;
use snook_legend_proto::{Rules, ServerMsg, Tier};

use crate::store::User;
use crate::{Server, now, shop};

/// How long a league's numbers are kept before they are read again.
const LEAGUE_KEPT: Duration = Duration::from_secs(15 * 60);

/// A league's numbers as read: when, the sums, and how many players.
type League = (Instant, Sums, u32);

/// The leagues' numbers read lately, by rules and tier.
#[derive(Default)]
pub struct LeagueCache {
    leagues: Mutex<HashMap<(Rules, Tier), League>>,
}

/// The ratings of a tier: from its own minimum to the next one's.
fn range(tier: Tier) -> (i32, i32) {
    let next = Tier::ALL.into_iter().find(|t| *t > tier).map_or(i32::MAX, |t| t.min_rating());
    (tier.min_rating(), next)
}

/// The players of `tier` under `rules`, pooled (read again every
/// [`LEAGUE_KEPT`]).
async fn league(server: &Server, rules: Rules, tier: Tier) -> (Sums, u32) {
    let cache = &server.leagues;
    if let Some((at, sums, players)) =
        cache.leagues.lock().unwrap_or_else(|e| e.into_inner()).get(&(rules, tier))
        && at.elapsed() < LEAGUE_KEPT
    {
        return (sums.clone(), *players);
    }
    let (min, max) = range(tier);
    match server.store.league_analysis(rules, min, max).await {
        Ok((analysis, players)) => {
            let sums = Sums::of(&analysis);
            cache
                .leagues
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert((rules, tier), (Instant::now(), sums.clone(), players));
            (sums, players)
        }
        Err(e) => {
            tracing::warn!("the league's analysis ({rules:?}, {tier:?}): {e}");
            (Sums::default(), 0)
        }
    }
}

/// `me`'s advanced statistics under `rules` (only the league and the VIP
/// flag for a player who is not VIP: the game shows a preview).
pub async fn handle(server: &Server, me: &User, rules: Rules, send: &impl Fn(ServerMsg)) {
    let record = me.records.get(rules);
    let tier = (record.games > 0).then(|| Tier::of(record.rating));
    let wallet = server.store.wallet(me.id).await.unwrap_or_default();
    let vip = shop::is_vip(&wallet, now());
    let mut stats = AdvancedStats { rules, vip, tier, ..Default::default() };
    if vip {
        let (analysis, weeks) = tokio::join!(
            server.store.shot_analysis(me.id),
            server.store.shot_weeks(me.id, WEEKS as u32)
        );
        let analysis = analysis.unwrap_or_default();
        stats.games = analysis.games;
        stats.mine = Sums::of(&analysis);
        stats.analysis = Some(Box::new(analysis));
        stats.weeks = weeks
            .unwrap_or_default()
            .into_iter()
            .rev()
            .map(|w| Week {
                week: w.week,
                pots: Tally { attempts: w.attempts, made: w.made.min(w.attempts) },
                games: w.games,
            })
            .collect();
        // The league of a player without rated games yet: the one they
        // start in.
        let (league, players) =
            league(server, rules, tier.unwrap_or(Tier::of(record.rating))).await;
        stats.league = league;
        stats.league_players = players;
    }
    send(ServerMsg::AdvancedStats { stats: Box::new(stats) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_cover_every_rating_once() {
        assert_eq!(range(Tier::Rookie), (i32::MIN, 1250));
        assert_eq!(range(Tier::Expert), (1500, 1700));
        assert_eq!(range(Tier::Legend), (2100, i32::MAX));
        for pair in Tier::ALL.windows(2) {
            assert_eq!(range(pair[0]).1, range(pair[1]).0);
        }
    }
}
