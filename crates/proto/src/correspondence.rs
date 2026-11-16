//! Correspondence ("daily") games: each player takes their turn whenever
//! they can, like correspondence chess. The server keeps the game (its seed
//! and strokes, see [`crate::replay`]) and rebuilds the table by replaying
//! it; a turn is a whole visit, with a deadline.
//!
//! Only games of protocol version [`SINCE_PROTOCOL`] and later know these
//! messages: the server never sends them to older games.

use serde::{Deserialize, Serialize};
use snookered_core::{EightBallGame, GameOverReason, Group};

use crate::PlayerInfo;

/// The first protocol version with correspondence games.
pub const SINCE_PROTOCOL: u32 = 13;

/// Time for a turn (a whole visit): whoever runs out of it loses the game.
pub const TURN_SECS: i64 = 72 * 3600;

/// A challenge nobody answered is withdrawn after this long.
pub const CHALLENGE_SECS: i64 = 7 * 24 * 3600;

/// Games a player may have going at once.
pub const MAX_GAMES: usize = 10;

/// Challenges a player may have sent and still waiting for an answer.
pub const MAX_SENT_CHALLENGES: usize = 5;

/// Challenges waiting for one player's answer (more are refused).
pub const MAX_RECEIVED_CHALLENGES: usize = 10;

/// Finished games listed with the active ones.
pub const RECENT_FINISHED: usize = 8;

/// A correspondence game as one of its players sees it in the list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorrGame {
    pub id: i64,
    pub players: [PlayerInfo; 2],
    /// The seat of the player this was sent to.
    pub seat: usize,
    pub rated: bool,
    /// The seat whose turn it is.
    pub turn: usize,
    /// When the turn runs out (Unix seconds).
    pub deadline: i64,
    /// Strokes played so far.
    pub strokes: u32,
    /// Strokes this player has not watched yet.
    pub unseen: u32,
    /// Each seat's group, once the table is no longer open.
    pub groups: [Option<Group>; 2],
    /// Balls of their group each seat still has to pocket (7 while open).
    pub left: [u8; 2],
    pub started_at: i64,
    pub updated_at: i64,
    /// Set once the game is over.
    #[serde(default)]
    pub result: Option<CorrResult>,
}

impl CorrGame {
    /// Waiting for this player's move.
    pub fn my_turn(&self) -> bool {
        self.result.is_none() && self.turn == self.seat
    }

    pub fn opponent(&self) -> &PlayerInfo {
        &self.players[1 - self.seat.min(1)]
    }
}

/// How a correspondence game ended.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorrResult {
    pub winner: usize,
    pub reason: GameOverReason,
    /// The loser ran out of time (`reason` is then `Abandoned`).
    pub timeout: bool,
    /// Rating change of each seat (zero for casual games).
    pub delta: [i32; 2],
    /// Ratings after the game.
    pub ratings: [i32; 2],
    /// Unix seconds.
    pub at: i64,
    /// The stored match (its replay), 0 if unknown.
    pub match_id: i64,
}

/// A challenge to a correspondence game, waiting for an answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorrChallenge {
    pub id: i64,
    pub from: PlayerInfo,
    pub to: PlayerInfo,
    pub rated: bool,
    /// Sent at (Unix seconds).
    pub at: i64,
    /// Withdrawn at (Unix seconds) if nobody answers.
    pub expires_at: i64,
}

/// Something happened in a correspondence game (a toast, and a desktop
/// notification for a player in another window).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrEvent {
    /// `opponent` challenged this player.
    Challenged {
        rated: bool,
    },
    /// `opponent` accepted this player's challenge: the game is on.
    Accepted,
    Declined,
    /// `opponent` withdrew their challenge (or it expired).
    Withdrawn,
    /// `opponent` finished their visit: this player's turn.
    YourTurn,
    /// The game is over.
    Ended {
        won: bool,
        reason: GameOverReason,
        timeout: bool,
    },
}

/// The groups and the balls left of each seat, for the list.
pub fn summary(game: &EightBallGame) -> ([Option<Group>; 2], [u8; 2]) {
    let groups = [game.group_of(0), game.group_of(1)];
    let left = groups.map(|g| match g {
        Some(g) => game.remaining(g).len() as u8,
        None => 7,
    });
    (groups, left)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_rack_is_open_with_seven_balls_each() {
        let game = EightBallGame::new(5, 0);
        assert_eq!(summary(&game), ([None, None], [7, 7]));
    }

    #[test]
    fn messages_round_trip() {
        use crate::{ClientMsg, ServerMsg, parse_client, parse_server};
        let c = ClientMsg::CorrShoot {
            id: 9,
            turn: 4,
            shot: snookered_core::Shot::new(0.5, 2.0),
            called_pocket: None,
            cue_ball: Some(glam::DVec2::new(0.1, -0.2)),
        };
        assert_eq!(parse_client(&c.to_json()).unwrap(), c);
        assert!(c.to_json().contains("\"type\":\"corr_shoot\""));
        let s = ServerMsg::CorrNotice {
            id: 9,
            opponent: "Ana".into(),
            event: CorrEvent::Challenged { rated: true },
        };
        assert_eq!(parse_server(&s.to_json()).unwrap(), s);
        let s = ServerMsg::CorrList { games: Vec::new(), challenges: Vec::new() };
        assert_eq!(parse_server(&s.to_json()).unwrap(), s);
    }

    #[test]
    fn events_round_trip() {
        let e = CorrEvent::Ended { won: true, reason: GameOverReason::Abandoned, timeout: true };
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(serde_json::from_str::<CorrEvent>(&json).unwrap(), e);
        assert_eq!(serde_json::to_string(&CorrEvent::YourTurn).unwrap(), "\"your_turn\"");
    }
}
