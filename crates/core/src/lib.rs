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
/// - 5: 0.7.0. Ruleset 4 with what it got wrong put right. A break that makes
///   balls of both groups passes the turn with the table still open. The
///   cue ball in hand may be placed against a
///   cushion, never past a pocket's mouth; a table still open when no ball
///   of either group is left puts both players on the 8; a shot clock that
///   runs out leaves the cue ball in hand as it was; a tip too far out to
///   measure is struck at the edge of the ball; a ball that jumps onto the
///   rail beside a pocket is off the table, not made. The computer chooses
///   only among the strokes it has checked, checks them slipping either
///   way, breaks and strokes the ball at speeds that fit this cue, counts
///   the opponent's balls it makes as given away, and plays for the pocket
///   it called for the 8 (calling it, on a safety, for where the 8 stops).
pub const RULESET: u8 = 5;

/// The newest ruleset this version can play. It is ahead of [`RULESET`]
/// while a ruleset is being prepared: a game is played under it only when
/// it is asked for by number ([`EightBallGame::with_ruleset`], or a
/// snapshot or a replay that names it), and nothing asks for it yet.
/// Making it the default means raising [`RULESET`] to it, with a new
/// protocol version (an older game would play its shots differently).
pub const NEWEST_RULESET: u8 = 5;

/// A ruleset this version knows (unknown ones play as the newest it knows).
pub fn known_ruleset(ruleset: u8) -> u8 {
    ruleset.clamp(1, NEWEST_RULESET)
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
