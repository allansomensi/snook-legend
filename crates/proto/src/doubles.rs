//! Doubles: two teams of two at one table (see `snook_legend_core::doubles`
//! for who of a team takes each visit and how the ratings move).
//!
//! In the lounge a player invites a friend to be their partner; the pair
//! then plays together: "Play now" for doubles (pairs are matched by the
//! average of their doubles ratings), or a doubles table another pair
//! joins. A doubles game is played with the usual match messages, a team
//! per seat ([`crate::ServerMsg::MatchStarted`]'s `seat` is the player's
//! team, its `players` the teams' first players), and its line-up comes
//! before it and whenever it changes ([`crate::ServerMsg::DoublesLineup`]).
//!
//! Only games of protocol 25 or later are paired or seated in doubles.

use serde::{Deserialize, Serialize};
use snook_legend_core::Rules;

use crate::PlayerInfo;

/// The oldest protocol a doubles player may have (older games would take
/// their partner's turns for their own).
pub const MIN_PROTOCOL: u32 = 25;

/// A pair in the lounge, as each of its players sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pair {
    pub partner: PlayerInfo,
    /// The pair is waiting for another pair ("Play now"): rated or not,
    /// under which rules, and for how long.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub searching: Option<Search>,
    /// The doubles table the pair opened (waiting for another pair).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<u64>,
}

/// A pair's search for another pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Search {
    pub rated: bool,
    pub rules: Rules,
    pub waited_secs: u32,
}

/// A doubles table waiting in the lounge for a second pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub id: u64,
    /// The pair that opened it.
    pub team: [PlayerInfo; 2],
    pub rated: bool,
    pub rules: Rules,
    /// The pair's average doubles rating under the table's rules.
    pub rating: i32,
}

/// One player of a doubles game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Player {
    pub username: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub avatar: String,
    /// Their doubles rating under the game's rules (after the game: the
    /// new one).
    pub rating: i32,
    /// At the table (false while their connection is down, or once they
    /// left).
    pub connected: bool,
    #[serde(default)]
    pub admin: bool,
}

/// Who plays a doubles game and who holds the cue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lineup {
    pub match_id: u64,
    /// The teams in seat order (team 0 is seat 0), each with its first and
    /// second player.
    pub teams: [[Player; 2]; 2],
    /// Who of each team takes the team's current visit (the team at the
    /// table) or its next one (the other team): 0 or 1.
    pub shooter: [usize; 2],
    /// The rating change of each player (`[team][player]`) once a rated
    /// game is over.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deltas: Option<[[i32; 2]; 2]>,
}

impl Lineup {
    /// The team and the place in it of `name` (case-insensitive).
    pub fn place_of(&self, name: &str) -> Option<(usize, usize)> {
        (0..4)
            .map(|k| (k / 2, k % 2))
            .find(|&(t, p)| self.teams[t][p].username.eq_ignore_ascii_case(name))
    }

    /// The player holding (or about to hold) the cue for `team`.
    pub fn shooter_of(&self, team: usize) -> &Player {
        &self.teams[team % 2][self.shooter[team % 2] % 2]
    }

    /// A team's name: its two players.
    pub fn team_name(&self, team: usize) -> String {
        let t = &self.teams[team % 2];
        format!("{} & {}", t[0].username, t[1].username)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(name: &str) -> Player {
        Player {
            username: name.into(),
            country: String::new(),
            avatar: String::new(),
            rating: 1200,
            connected: true,
            admin: false,
        }
    }

    #[test]
    fn a_lineup_finds_its_players() {
        let l = Lineup {
            match_id: 1,
            teams: [[player("Ana"), player("bia")], [player("caio"), player("Duda")]],
            shooter: [1, 0],
            deltas: None,
        };
        assert_eq!(l.place_of("BIA"), Some((0, 1)));
        assert_eq!(l.place_of("duda"), Some((1, 1)));
        assert_eq!(l.place_of("eve"), None);
        assert_eq!(l.shooter_of(0).username, "bia");
        assert_eq!(l.team_name(1), "caio & Duda");
        let json = serde_json::to_string(&l).unwrap();
        assert!(!json.contains("deltas"));
        assert_eq!(serde_json::from_str::<Lineup>(&json).unwrap(), l);
    }
}
