//! Doubles: two teams of two at one table.
//!
//! The game itself doesn't change: an [`crate::EightBallGame`] is played
//! between seat 0 and seat 1, and in doubles each seat is a team. What
//! doubles adds is who of the team holds the cue, and how a team's rating
//! moves.
//!
//! # The rotation
//!
//! Every time the table comes to a team, a new *visit* of that team begins,
//! and the team's two players take the visits in turn: the team's first
//! player takes its first visit (the breaking team's first player breaks),
//! the second player its second visit, the first player its third, and so
//! on. Within a visit nothing changes: a player who keeps the table (a
//! legal pot, a choice that leaves them at it) shoots again. So with team A
//! (Ana, Bia) breaking against team B (Caio, Duda), the cue goes Ana… Caio…
//! Bia… Duda… Ana…, each name as long as their visit lasts.
//!
//! It is not scotch doubles (where partners alternate every stroke, also
//! within a visit). A time foul, a foul or a choice that hands the table
//! over ends the visit like a miss.
//!
//! A player whose connection dropped (or who left) is not waited for: the
//! visit that was theirs is taken by their partner, and the order goes on
//! from the visit count as before once they are back (see [`shooter`]).
//!
//! # The rating
//!
//! Each player has a doubles rating of their own under each of the rules,
//! apart from their singles one. A team plays at the average of its two
//! ratings: the expected score comes from the two teams' averages (the
//! same Elo as singles), and each player's change is that result weighted
//! by their own K-factor (a newcomer's rating settles quickly, a veteran's
//! moves little), so partners usually move by the same amount, and a
//! newcomer more (see [`team_result`]).

use crate::rating::{EloConfig, Rating, expected_score};

/// The player (0 or 1 within the team) whose turn it is to take a team's
/// `visit`-th visit to the table (the first is 1; before its first visit,
/// 0, the team's first player is the one to come).
pub fn designated(visit: u32) -> usize {
    (visit.saturating_sub(1) % 2) as usize
}

/// Who of a team takes its `visit`-th visit given who is at the table
/// (`present[slot]`): the designated player, or their partner when the
/// designated one isn't there. None when neither is.
pub fn shooter(visit: u32, present: [bool; 2]) -> Option<usize> {
    let slot = designated(visit);
    if present[slot] {
        Some(slot)
    } else if present[1 - slot] {
        Some(1 - slot)
    } else {
        None
    }
}

/// A team's rating: the average of its two players' (rounded to the
/// nearest point), with the fewer rated games of the two.
pub fn team_rating(a: Rating, b: Rating) -> Rating {
    let sum = a.rating as i64 + b.rating as i64;
    Rating { rating: (sum as f64 / 2.0).round() as i32, games: a.games.min(b.games) }
}

/// What a doubles result does to each player's rating: `[winners, losers]`,
/// each `[first player, second player]`, as the change in points.
///
/// The expected score is the winning team's against the losing team's
/// average ([`team_rating`]); each player then moves by their own K-factor
/// (see [`crate::rating::apply_result`]: the same floor too).
pub fn team_result(config: &EloConfig, winners: [Rating; 2], losers: [Rating; 2]) -> [[i32; 2]; 2] {
    let (w, l) = (team_rating(winners[0], winners[1]), team_rating(losers[0], losers[1]));
    let expected_w = expected_score(w.rating, l.rating);
    let gain = |r: Rating| (k_factor(config, r) * (1.0 - expected_w)).round() as i32;
    let loss = |r: Rating| {
        let points = (k_factor(config, r) * (1.0 - expected_w)).round() as i32;
        // The floor stops a fall; it never lifts a rating already below it.
        let floor = config.floor.min(r.rating);
        r.rating.saturating_sub(points).max(floor) - r.rating
    };
    [winners.map(gain), losers.map(loss)]
}

/// The K-factor of a player's record (as singles: provisional, standard or
/// elite).
fn k_factor(config: &EloConfig, r: Rating) -> f64 {
    if r.games < config.provisional_games {
        config.k_provisional
    } else if r.rating >= config.elite_threshold {
        config.k_elite
    } else {
        config.k_standard
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn established(rating: i32) -> Rating {
        Rating { rating, games: 100 }
    }

    /// The order of the example in the module's notes: each team's players
    /// take its visits in turn.
    #[test]
    fn each_team_visit_goes_to_the_next_player() {
        let both = [true, true];
        // A's visits 1, 2, 3 → Ana, Bia, Ana; B's the same.
        assert_eq!([1, 2, 3, 4].map(|v| shooter(v, both)), [Some(0), Some(1), Some(0), Some(1)]);
        // Before a team's first visit, its first player is the one to come.
        assert_eq!(designated(0), 0);
        assert_eq!(designated(1), 0);
        assert_eq!(designated(2), 1);
    }

    #[test]
    fn a_missing_player_is_covered_by_the_partner() {
        assert_eq!(shooter(2, [true, false]), Some(0));
        assert_eq!(shooter(1, [false, true]), Some(1));
        assert_eq!(shooter(3, [false, false]), None);
        // Back again, the order is the visit count's.
        assert_eq!(shooter(4, [true, true]), Some(1));
    }

    #[test]
    fn a_team_plays_at_its_average() {
        assert_eq!(team_rating(established(1400), established(1601)).rating, 1501);
        assert_eq!(team_rating(Rating { rating: 1200, games: 3 }, established(1500)).games, 3);
    }

    /// Two teams of the same average exchange what two singles players of
    /// that rating would; a newcomer moves more than a veteran partner.
    #[test]
    fn team_results_follow_singles_elo() {
        let c = EloConfig::default();
        let even = team_result(
            &c,
            [established(1500), established(1500)],
            [established(1400), established(1600)],
        );
        let single = crate::rating::apply_result(&c, established(1500), established(1500));
        assert_eq!(even[0], [single.winner_delta; 2]);
        assert_eq!(even[1], [single.loser_delta; 2]);
        let mixed = team_result(
            &c,
            [Rating { rating: 1200, games: 2 }, established(1800)],
            [established(1500), established(1500)],
        );
        assert!(mixed[0][0] > mixed[0][1], "{mixed:?}");
        assert!(mixed[0].iter().all(|d| *d > 0) && mixed[1].iter().all(|d| *d < 0));
    }

    #[test]
    fn the_floor_holds_in_doubles() {
        let c = EloConfig::default();
        let low = Rating { rating: c.floor, games: 50 };
        let r = team_result(&c, [established(1500), established(1500)], [low, low]);
        assert_eq!(r[1], [0, 0]);
    }
}
