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
/// - 2: 0.4.0. A 8-foot table with wider side pockets, balls that roll a little
///   more freely, and making an opponent's ball along with one's own
///   passes the turn.
/// - 3: 0.5.0. The WPA's 8-foot tournament table (92" × 46", regulation pockets),
///   and a tournament table's physics as measured: cushions that soak up a hard hit, ball friction and
///   restitution that depend on speed, and strokes at real speeds (an
///   ordinary stroke tops out well below a break).
/// - 4: 0.6.0. The Brazilian house rules (a foul gives the opponent balls,
///   only a cue ball that went down or left the table gives ball in hand,
///   the 8 alone on the break wins: see [`EightBallGame::finish_shot`]) on
///   the same table, with heavier-feeling balls: a slower cloth, a rack
///   that soaks up more of the break and a break that is not struck as hard.
pub const RULESET: u8 = 4;

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
