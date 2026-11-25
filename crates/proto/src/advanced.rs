//! Advanced statistics (a VIP feature): how a player pots, pocket by pocket
//! and by cut angle, how that changed week after week, and how it compares
//! with the players of their league. They explain the game; nothing here
//! plays for the player (no aim is suggested, nothing is drawn at the table).
//!
//! The server reads them from the shot analysis it keeps
//! ([`crate::analysis`]): every stored game is replayed when it ends and its
//! shots are added up, for the player and, week by week, for their
//! progress. A player who is not VIP is told so (`vip: false`) and gets no
//! numbers: the game shows a preview of the screen instead.

use serde::{Deserialize, Serialize};

use crate::analysis::{POCKETS, ShotAnalysis, ShotKind, Tally};
use crate::{Rules, Tier};

/// The cut-angle buckets, in the order they are shown (as the analysis's
/// kinds of shot: straight, up to 25°, up to 45°, thinner).
pub const CUTS: usize = 4;
pub const CUT_KINDS: [ShotKind; CUTS] =
    [ShotKind::Straight, ShotKind::Cut, ShotKind::WideCut, ShotKind::Thin];
/// Weeks of progress sent (the newest last).
pub const WEEKS: usize = 12;

/// Pot attempts in one week (weeks start on Monday, see [`week_of`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Week {
    /// The week's number (see [`week_of`]).
    pub week: i64,
    pub pots: Tally,
    pub games: u32,
}

/// The sums of a group of players (the league's average is their pooled
/// success: all their pots over all their attempts).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sums {
    pub pockets: [Tally; POCKETS],
    pub cuts: [Tally; CUTS],
    /// Every pot attempt.
    pub overall: Tally,
}

impl Sums {
    pub fn of(a: &ShotAnalysis) -> Sums {
        Sums {
            pockets: a.pockets,
            cuts: CUT_KINDS.map(|k| a.kinds[k.index()]),
            overall: a.attempts(),
        }
    }
}

/// The advanced statistics of a player under one of the rules (the league
/// is their tier under those rules).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvancedStats {
    pub rules: Rules,
    /// VIP: the numbers are theirs. Otherwise everything else is empty.
    pub vip: bool,
    /// Their league under `rules`.
    pub tier: Option<Tier>,
    /// Games counted.
    pub games: u32,
    pub mine: Sums,
    /// The players of the same league, pooled.
    pub league: Sums,
    /// How many players the league's numbers come from.
    pub league_players: u32,
    /// The last [`WEEKS`] weeks with games, oldest first.
    pub weeks: Vec<Week>,
}

/// The week a Unix time falls in: weeks start on Monday (the Unix epoch was
/// a Thursday, hence the three days).
pub fn week_of(unix: i64) -> i64 {
    (unix.div_euclid(86_400) + 3).div_euclid(7)
}

/// The Unix time a week starts at (Monday, 00:00 UTC).
pub fn week_start(week: i64) -> i64 {
    (week * 7 - 3) * 86_400
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weeks_start_on_monday() {
        // Monday 2026-11-23 00:00 UTC and the Sunday before it.
        let monday = 1_795_392_000;
        assert_eq!(week_of(monday), week_of(monday + 6 * 86_400 + 86_399));
        assert_eq!(week_of(monday) - 1, week_of(monday - 1));
        assert_eq!(week_start(week_of(monday)), monday);
    }

    #[test]
    fn sums_take_the_cuts_from_the_kinds() {
        let mut a = ShotAnalysis::default();
        a.kinds[ShotKind::WideCut.index()] = Tally { attempts: 4, made: 1 };
        a.kinds[ShotKind::Bank.index()] = Tally { attempts: 9, made: 9 };
        a.pockets[2] = Tally { attempts: 5, made: 3 };
        let s = Sums::of(&a);
        assert_eq!(s.cuts[2], Tally { attempts: 4, made: 1 });
        assert_eq!(s.cuts.iter().map(|t| t.attempts).sum::<u32>(), 4);
        assert_eq!(s.overall, Tally { attempts: 5, made: 3 });
    }
}
