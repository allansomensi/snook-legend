//! # snook_legend_core
//!
//! Engine-agnostic core of **Snook Legend**: ball physics, table geometry, the
//! eight-ball rules, the computer opponent and the rating system.
//!
//! This crate has no rendering or networking dependencies so that the desktop
//! client and the game server run exactly the same simulation.

/// The version of the rules, the physics and the table a game is played
/// with. Every game records its own (see [`EightBallGame::ruleset`]), so a
/// replay, a saved game or a correspondence game that started under older
/// rules plays on under them, shot for shot.
///
/// - 1: 0.3.0. A 9-foot table, the measured "heavy" physics, the first ball
///   made on a legal break sets the groups.
/// - 2: a 8-foot table with wider side pockets, balls that roll a little
///   more freely, and making an opponent's ball along with one's own
///   passes the turn.
pub const RULESET: u8 = 2;

/// A ruleset this version knows (unknown ones play as the newest).
pub fn known_ruleset(ruleset: u8) -> u8 {
    ruleset.clamp(1, RULESET)
}

pub mod ball;
pub mod math;
pub mod physics;
pub mod table;

pub use ball::{Ball, BallId, BallKind, BallStatus, Motion};
pub use physics::{Event, PhysicsParams, Shot, Simulation, StrikeError};
pub use table::{Pocket, PocketId, Table, TableSpec};
pub mod rng;
pub mod rules;

pub use rules::{
    BallInHand, EightBallGame, Foul, GameOver, GameOverReason, Group, PendingShot, ShotError,
    ShotReport,
};
pub mod ai;
pub mod assess;
pub mod personalities;
pub mod rating;
pub mod stats;

pub use ai::{AiDecision, AiPlayer, Difficulty, Skill, Style};
