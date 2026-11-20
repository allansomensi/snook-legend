//! Eight-ball rules: the Brazilian rules of the bars and pool halls (see
//! [`Rules::Brazilian`] and `brazilian.rs`) and the official rules of the
//! World Pool-Billiard Association (see [`Rules::Official`] and
//! `official.rs`). Both are played on the same table with the same balls
//! and the same physics; a game is played under one of them from its break
//! to its end.
//!
//! The game is driven in two phases so that clients can animate the shot:
//! 1. [`EightBallGame::prepare_shot`] validates the stroke and returns a
//!    [`PendingShot`] holding a running [`Simulation`];
//! 2. once the simulation has settled, [`EightBallGame::finish_shot`] applies the
//!    rules and returns a [`ShotReport`].
//!
//! [`EightBallGame::play_shot`] does both at once (used by the AI and the server).

mod brazilian;
mod official;
#[cfg(test)]
mod official_tests;
#[cfg(test)]
mod tests;

use glam::DVec2;
use serde::{Deserialize, Serialize};

use crate::ball::{Ball, BallId, BallKind, BallStatus, Motion};
use crate::physics::{Event, PhysicsParams, Shot, Simulation};
use crate::rng::Rng;
use crate::table::{PocketId, Table};

/// Which rules a game is played under.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Rules {
    /// The Brazilian rules of the bars and pool halls, with penalty balls:
    /// a foul passes the turn and takes the lowest of the opponent's balls
    /// off the table for them; the cue ball is in hand on the break only.
    #[default]
    Brazilian,
    /// The World Pool-Billiard Association's rules (the Rules of Play of
    /// 2025-09-15, "8-Ball"): every shot but the break is called, and a
    /// foul gives the opponent the cue ball in hand anywhere on the table.
    Official,
}

impl Rules {
    pub const ALL: [Rules; 2] = [Rules::Brazilian, Rules::Official];

    /// The rules' name in code and in files ("brazilian", "official").
    pub fn code(self) -> &'static str {
        match self {
            Rules::Brazilian => "brazilian",
            Rules::Official => "official",
        }
    }

    pub fn parse(code: &str) -> Option<Rules> {
        Rules::ALL.into_iter().find(|r| r.code() == code)
    }

    /// The position of these rules in [`Self::ALL`] (for tables kept per rules).
    pub fn index(self) -> usize {
        self as usize
    }

    /// The other rules.
    pub fn other(self) -> Rules {
        match self {
            Rules::Brazilian => Rules::Official,
            Rules::Official => Rules::Brazilian,
        }
    }
}

impl std::fmt::Display for Rules {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

/// One value for each of the [`Rules`]: a player's rating, a list kept per
/// rules…
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct PerRules<T> {
    pub brazilian: T,
    pub official: T,
}

impl<T> PerRules<T> {
    pub fn new(brazilian: T, official: T) -> Self {
        PerRules { brazilian, official }
    }

    /// The same value for both.
    pub fn both(value: T) -> Self
    where
        T: Clone,
    {
        PerRules { brazilian: value.clone(), official: value }
    }

    pub fn get(&self, rules: Rules) -> &T {
        match rules {
            Rules::Brazilian => &self.brazilian,
            Rules::Official => &self.official,
        }
    }

    pub fn get_mut(&mut self, rules: Rules) -> &mut T {
        match rules {
            Rules::Brazilian => &mut self.brazilian,
            Rules::Official => &mut self.official,
        }
    }

    pub fn map<U>(self, mut f: impl FnMut(Rules, T) -> U) -> PerRules<U> {
        PerRules {
            brazilian: f(Rules::Brazilian, self.brazilian),
            official: f(Rules::Official, self.official),
        }
    }

    /// Both values, with their rules, Brazilian first.
    pub fn iter(&self) -> impl Iterator<Item = (Rules, &T)> {
        [(Rules::Brazilian, &self.brazilian), (Rules::Official, &self.official)].into_iter()
    }
}

/// The two groups of object balls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Group {
    Solids,
    Stripes,
}

impl Group {
    pub const ALL: [Group; 2] = [Group::Solids, Group::Stripes];

    pub fn contains(self, id: BallId) -> bool {
        matches!(
            (self, id.kind()),
            (Group::Solids, BallKind::Solid) | (Group::Stripes, BallKind::Stripe)
        )
    }

    pub fn of(id: BallId) -> Option<Group> {
        match id.kind() {
            BallKind::Solid => Some(Group::Solids),
            BallKind::Stripe => Some(Group::Stripes),
            _ => None,
        }
    }

    pub fn opposite(self) -> Group {
        match self {
            Group::Solids => Group::Stripes,
            Group::Stripes => Group::Solids,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Group::Solids => "solids",
            Group::Stripes => "stripes",
        }
    }
}

/// Where the incoming player may place the cue ball.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BallInHand {
    /// Behind the head string: for the break, and under the official rules
    /// after some fouls on it.
    Kitchen,
    /// Anywhere on the table (after a foul, under the official rules).
    Anywhere,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Foul {
    CueBallPocketed,
    CueBallOffTable,
    NoContact,
    WrongBallFirst(BallId),
    NoRailAfterContact,
    ObjectBallOffTable(BallId),
    /// The official break: no ball made and fewer than four object balls
    /// driven to a cushion.
    IllegalBreak,
    /// The official rules: with the cue ball in hand behind the head
    /// string, the first ball it hit lay behind the head string too and it
    /// had not crossed the head string before.
    BallBehindHeadString(BallId),
}

impl std::fmt::Display for Foul {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Foul::CueBallPocketed => write!(f, "Scratch: the cue ball was pocketed"),
            Foul::CueBallOffTable => write!(f, "The cue ball left the table"),
            Foul::NoContact => write!(f, "The cue ball didn't hit any ball"),
            Foul::WrongBallFirst(id) => write!(f, "Wrong ball hit first ({id})"),
            Foul::NoRailAfterContact => write!(f, "No ball reached a cushion after contact"),
            Foul::ObjectBallOffTable(id) => write!(f, "The {id} left the table"),
            Foul::IllegalBreak => write!(f, "Illegal break"),
            Foul::BallBehindHeadString(id) => {
                write!(f, "The {id} behind the head string was hit without crossing it first")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameOverReason {
    /// The winner legally pocketed the eight ball in the called pocket (or,
    /// under the Brazilian rules, the 8 alone on the break).
    PocketedEight,
    /// The loser pocketed the eight before clearing their group.
    EightPocketedEarly,
    /// The loser pocketed the eight in a pocket other than the one called
    /// (under the official rules, also on a shot called a safety).
    EightInWrongPocket,
    /// The loser pocketed the eight on a foul, or, under the Brazilian
    /// rules, fouled when the opponent had fewer balls left than the foul
    /// gave them: the 8 paid the rest.
    EightPocketedOnFoul,
    /// The loser knocked the eight off the table.
    EightOffTable,
    /// The Brazilian rules: the loser made the eight together with another
    /// ball, on the break with any ball, or at the end with one of the
    /// opponent's.
    EightWithAnotherBall,
    Resignation,
    /// The loser left an online game and did not come back in time.
    Abandoned,
}

impl GameOverReason {
    pub fn description(self) -> &'static str {
        match self {
            GameOverReason::PocketedEight => "pocketed the 8-ball",
            GameOverReason::EightPocketedEarly => "opponent pocketed the 8-ball too early",
            GameOverReason::EightInWrongPocket => "opponent sank the 8-ball in the wrong pocket",
            GameOverReason::EightPocketedOnFoul => "opponent fouled on the 8-ball",
            GameOverReason::EightOffTable => "opponent knocked the 8-ball off the table",
            GameOverReason::EightWithAnotherBall => {
                "opponent pocketed the 8-ball together with another ball"
            }
            GameOverReason::Resignation => "opponent resigned",
            GameOverReason::Abandoned => "opponent left the game",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameOver {
    pub winner: usize,
    pub reason: GameOverReason,
}

/// A decision the official rules leave to a player after some breaks
/// (WPA 4.3), before anyone shoots again. The player who decides is the
/// game's current player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakChoice {
    /// No ball was made and fewer than four object balls reached a cushion
    /// (4.3 d). The incoming player plays the table as it lies (with the
    /// cue ball in hand behind the head string if the breaker scratched),
    /// or the balls are racked again and they break, or the breaker breaks
    /// again. With an object ball off the table too (`ball_off`, 4.3 g)
    /// they may also take the cue ball in hand behind the head string.
    IllegalBreak {
        scratch: bool,
        #[serde(default)]
        ball_off: bool,
    },
    /// The 8 went down on a legal break (4.3 e). The breaker has it spotted
    /// and plays on, or breaks again.
    EightOnBreak,
    /// The 8 went down on a break with a foul (4.3 f). The incoming player
    /// has it spotted and plays with the cue ball in hand behind the head
    /// string, or the balls are racked again and they break.
    EightOnFoulBreak,
    /// An object ball went off the table on the break and the cue ball is
    /// still on it (4.3 g). The incoming player plays the table as it lies,
    /// or takes the cue ball in hand behind the head string.
    BallOffOnBreak,
}

/// What a player may answer to a [`BreakChoice`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakOption {
    /// Play on: the table as it lies (and the 8 spotted, when it went
    /// down), with the cue ball in hand behind the head string after a
    /// scratch or a ball made on a foul.
    PlayOn,
    /// The cue ball in hand behind the head string (after a ball off the
    /// table on the break).
    CueBallInHand,
    /// The balls are racked again, and the player choosing breaks.
    BreakAgain,
    /// The balls are racked again, and the other player breaks (after an
    /// illegal break: the breaker breaks again).
    OpponentBreaks,
}

impl BreakChoice {
    /// The answers this choice allows, the default first (the one taken
    /// when the shot clock runs out).
    pub fn options(self) -> &'static [BreakOption] {
        match self {
            BreakChoice::IllegalBreak { scratch: false, ball_off: true } => &[
                BreakOption::PlayOn,
                BreakOption::CueBallInHand,
                BreakOption::BreakAgain,
                BreakOption::OpponentBreaks,
            ],
            BreakChoice::IllegalBreak { .. } => {
                &[BreakOption::PlayOn, BreakOption::BreakAgain, BreakOption::OpponentBreaks]
            }
            BreakChoice::EightOnBreak | BreakChoice::EightOnFoulBreak => {
                &[BreakOption::PlayOn, BreakOption::BreakAgain]
            }
            BreakChoice::BallOffOnBreak => &[BreakOption::PlayOn, BreakOption::CueBallInHand],
        }
    }
}

/// Outcome of a shot, for display and for the AI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShotReport {
    pub shooter: usize,
    pub was_break: bool,
    pub first_contact: Option<BallId>,
    pub pocketed: Vec<(BallId, PocketId)>,
    pub fouls: Vec<Foul>,
    /// Balls returned to the table (object balls that left it, or the 8 on the break).
    pub spotted: Vec<BallId>,
    /// Set when this shot decided the groups; the group is the shooter's.
    pub group_assigned: Option<Group>,
    /// True if the shooter keeps the table.
    pub continues: bool,
    pub game_over: Option<GameOver>,
    /// The pocket the shot was called for: under the official rules every
    /// shot's (none: a safety), under the Brazilian rules the 8's.
    pub called: Option<PocketId>,
    /// The Brazilian rules: the shooter made one of the opponent's balls,
    /// which passed the turn, even if they made one of their own too.
    pub opponent_ball: bool,
    /// The Brazilian rules: the opponent's balls taken off the table for
    /// them by this shot's fouls, lowest number first, and the 8 last when it
    /// paid what they didn't have (the foul lost the game).
    pub penalty: Vec<BallId>,
    /// The Brazilian rules: the break hit no ball, or lost the cue ball
    /// without making one, and the opponent breaks, from where the balls
    /// lie, with the cue ball in hand behind the head string.
    pub break_again: bool,
    /// The official rules: a player must decide how play goes on after
    /// this break before anyone shoots (see [`EightBallGame::choose`]).
    pub choice: Option<BreakChoice>,
    /// The table was open (no groups) when the shot was played.
    #[serde(default)]
    pub open_table: bool,
}

impl ShotReport {
    pub fn is_foul(&self) -> bool {
        !self.fouls.is_empty()
    }

    /// The object balls this shot made for the shooter, in the order they
    /// went down: those of their own group on a shot without a foul, and
    /// the 8 when it won the game. One of the opponent's balls made on the
    /// same shot went down for the opponent; a shot that loses the game, or
    /// that wins it, makes nothing else.
    ///
    /// The shooter's group is the one this shot gave them or, once the
    /// groups are set, the one of the ball they hit first (a shot without a
    /// foul hits one of the shooter's own first, or the 8 when they are on
    /// it and have none left), so the report alone tells. A break that set
    /// no groups made nothing for the shooter (under the official rules no
    /// break does).
    pub fn balls_made(&self) -> Vec<BallId> {
        if self.is_foul() {
            return Vec::new();
        }
        let made = self.pocketed.iter().map(|(b, _)| *b);
        if let Some(over) = self.game_over {
            let won = over.winner == self.shooter && over.reason == GameOverReason::PocketedEight;
            return made.filter(|b| won && *b == BallId::EIGHT).collect();
        }
        // A break that set no groups, or a shot on an open table that gave
        // none (the official rules: a ball made off its call, or on a
        // safety), made nothing for the shooter.
        if (self.was_break || self.open_table) && self.group_assigned.is_none() {
            return Vec::new();
        }
        let own = self.group_assigned.or_else(|| self.first_contact.and_then(Group::of));
        made.filter(|b| own.is_some_and(|g| g.contains(*b))).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShotError {
    GameIsOver,
    /// The cue ball is in hand and its current position is not allowed.
    IllegalCueBallPosition,
    /// The Brazilian rules: the shooter is on the eight and must nominate a
    /// pocket.
    MustCallPocket,
    /// The official rules: a player has to decide how play goes on after
    /// the break first (see [`EightBallGame::choose`]).
    ChoicePending,
}

impl std::fmt::Display for ShotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShotError::GameIsOver => write!(f, "the game is over"),
            ShotError::IllegalCueBallPosition => write!(f, "the cue ball cannot be placed there"),
            ShotError::MustCallPocket => write!(f, "call a pocket for the 8-ball"),
            ShotError::ChoicePending => write!(f, "how play goes on after the break is undecided"),
        }
    }
}

impl std::error::Error for ShotError {}

/// Why [`EightBallGame::choose`] refused an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChoiceError {
    /// There is nothing to decide.
    NoChoice,
    /// The answer is not one this choice allows.
    NotAnOption,
}

/// A validated shot whose simulation is still running.
#[derive(Debug, Clone)]
pub struct PendingShot {
    pub simulation: Simulation,
    pub shot: Shot,
    pub called_pocket: Option<PocketId>,
}

/// Complete state of an eight-ball game between players `0` and `1`.
///
/// Serializable so the game server can send authoritative snapshots (the
/// table and the physics are the same for every game and are not part of
/// one). A snapshot that no game could be in is refused when it is read
/// (see the `TryFrom` below).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "GameData")]
pub struct EightBallGame {
    #[serde(skip)]
    table: Table,
    #[serde(skip)]
    params: PhysicsParams,
    rules: Rules,
    balls: Vec<Ball>,
    current: usize,
    groups: [Option<Group>; 2],
    is_break: bool,
    ball_in_hand: Option<BallInHand>,
    game_over: Option<GameOver>,
    shots_taken: u32,
    /// The Brazilian rules: the pocket each player called for the 8-ball. A
    /// call is binding from the first shot taken with it until the end of
    /// the game.
    called: [Option<PocketId>; 2],
    /// What the rack was shuffled from (a rack made again after a break is
    /// shuffled from it and the shots taken, the same on the server and in
    /// every game).
    rack_seed: u64,
    /// The official rules: a decision the current player has to make
    /// before anyone shoots (after some breaks).
    choice: Option<BreakChoice>,
}

/// What a snapshot holds (see [`EightBallGame`]).
#[derive(Deserialize)]
struct GameData {
    rules: Rules,
    balls: Vec<Ball>,
    current: usize,
    groups: [Option<Group>; 2],
    is_break: bool,
    ball_in_hand: Option<BallInHand>,
    game_over: Option<GameOver>,
    shots_taken: u32,
    called: [Option<PocketId>; 2],
    rack_seed: u64,
    choice: Option<BreakChoice>,
}

impl TryFrom<GameData> for EightBallGame {
    type Error = &'static str;

    /// A snapshot comes from the network or from a file. One that no game
    /// could be in (a ball missing or out of order, a third player, a ball
    /// still rolling, no cue ball while the game goes on, a choice the
    /// Brazilian rules never ask for) would make the game index out of
    /// bounds, or find nothing to strike, or never end, later on: it is
    /// refused here
    /// instead. Every snapshot a game writes passes.
    fn try_from(d: GameData) -> Result<Self, Self::Error> {
        if d.balls.len() != 16 || d.balls.iter().enumerate().any(|(i, b)| b.id.index() != i) {
            return Err("a game has sixteen balls, in the order of their numbers");
        }
        if d.current > 1 || d.game_over.is_some_and(|o| o.winner > 1) {
            return Err("a game has two players, 0 and 1");
        }
        if d.balls.iter().any(|b| !b.position.is_finite()) {
            return Err("a ball is nowhere");
        }
        // (A ball at rest with a speed or a spin that isn't a number would
        // turn every ball it meets into nowhere.)
        let unreal = |b: &Ball| {
            !b.velocity.is_finite() || !b.angular_velocity.is_finite() || !b.orientation.is_finite()
        };
        if d.balls.iter().any(unreal) {
            return Err("a ball moves in a way that can't be");
        }
        // Between shots everything on the table is at rest (a ball left
        // spinning in place may stay so: it no longer moves).
        let moving = |b: &Ball| {
            b.in_play() && matches!(b.motion, Motion::Sliding | Motion::Rolling | Motion::Airborne)
        };
        if d.balls.iter().any(moving) {
            return Err("a ball is still moving");
        }
        if d.game_over.is_none() && !d.balls[BallId::CUE.index()].in_play() {
            return Err("the cue ball is off the table and the game goes on");
        }
        if d.choice.is_some() && (d.rules != Rules::Official || d.game_over.is_some()) {
            return Err("only an official game going on waits for a choice");
        }
        // The 8 off the table ends the game (or is spotted at once), but for
        // the official choice it went down on the break (spotted with it).
        let eight_waits =
            matches!(d.choice, Some(BreakChoice::EightOnBreak | BreakChoice::EightOnFoulBreak));
        if d.game_over.is_none() && !d.balls[BallId::EIGHT.index()].in_play() && !eight_waits {
            return Err("the 8 is off the table and the game goes on");
        }
        // The groups are decided for both players at once.
        match d.groups {
            [None, None] => {}
            [Some(a), Some(b)] if b == a.opposite() => {}
            _ => return Err("the players' groups don't go together"),
        }
        Ok(EightBallGame {
            table: Table::default(),
            params: PhysicsParams::default(),
            rules: d.rules,
            balls: d.balls,
            current: d.current,
            groups: d.groups,
            is_break: d.is_break,
            ball_in_hand: d.ball_in_hand,
            game_over: d.game_over,
            shots_taken: d.shots_taken,
            called: d.called,
            rack_seed: d.rack_seed,
            choice: d.choice,
        })
    }
}

/// What a settled shot did, read from its events.
struct Facts {
    /// The ball the cue ball hit first, and when.
    first_contact: Option<(f64, BallId)>,
    pocketed: Vec<(BallId, PocketId)>,
    off_table: Vec<BallId>,
}

impl Facts {
    fn of(events: &[Event]) -> Facts {
        // The earliest touch of the cue ball (the first one listed on a tie).
        let first_contact = events
            .iter()
            .filter_map(|e| match *e {
                Event::BallBall { time, a, b, .. } if a == BallId::CUE => Some((time, b)),
                Event::BallBall { time, a, b, .. } if b == BallId::CUE => Some((time, a)),
                _ => None,
            })
            .reduce(|first, e| if e.0 < first.0 { e } else { first });
        let pocketed = events
            .iter()
            .filter_map(|e| match *e {
                Event::Pocketed { ball, pocket, .. } => Some((ball, pocket)),
                _ => None,
            })
            .collect();
        let off_table = events
            .iter()
            .filter_map(|e| match *e {
                Event::OffTable { ball, .. } => Some(ball),
                _ => None,
            })
            .collect();
        Facts { first_contact, pocketed, off_table }
    }

    fn first_contact(&self) -> Option<BallId> {
        self.first_contact.map(|(_, id)| id)
    }

    /// The object balls made, in the order they went down.
    fn object_pocketed(&self) -> Vec<BallId> {
        self.pocketed.iter().map(|(b, _)| *b).filter(|b| b.is_object()).collect()
    }

    fn cue_pocketed(&self) -> bool {
        self.pocketed.iter().any(|(b, _)| *b == BallId::CUE)
    }

    fn cue_off(&self) -> bool {
        self.off_table.contains(&BallId::CUE)
    }

    /// The pocket `ball` went down, if it did.
    fn pocket_of(&self, ball: BallId) -> Option<PocketId> {
        self.pocketed.iter().find(|(b, _)| *b == ball).map(|(_, p)| *p)
    }
}

impl EightBallGame {
    /// A freshly racked game under `rules`. `seed` randomises the rack.
    pub fn new(seed: u64, breaker: usize, rules: Rules) -> Self {
        let table = Table::default();
        let balls = rack(&table, seed);
        EightBallGame {
            table,
            params: PhysicsParams::default(),
            rules,
            balls,
            current: breaker % 2,
            groups: [None, None],
            is_break: true,
            ball_in_hand: Some(BallInHand::Kitchen),
            game_over: None,
            shots_taken: 0,
            called: [None, None],
            rack_seed: seed,
            choice: None,
        }
    }

    pub fn table(&self) -> &Table {
        &self.table
    }

    /// The rules this game is played under.
    pub fn rules(&self) -> Rules {
        self.rules
    }

    /// The fastest the cue may move on the stroke about to be played (m/s):
    /// the top of the power bar.
    pub fn max_cue_speed(&self) -> f64 {
        self.params.cue_speed_limit(self.is_break)
    }

    pub fn params(&self) -> &PhysicsParams {
        &self.params
    }

    pub fn balls(&self) -> &[Ball] {
        &self.balls
    }

    pub fn ball(&self, id: BallId) -> &Ball {
        &self.balls[id.index()]
    }

    pub fn current_player(&self) -> usize {
        self.current
    }

    pub fn group_of(&self, player: usize) -> Option<Group> {
        self.groups[player]
    }

    pub fn is_break(&self) -> bool {
        self.is_break
    }

    pub fn is_open_table(&self) -> bool {
        self.groups[0].is_none()
    }

    pub fn ball_in_hand(&self) -> Option<BallInHand> {
        self.ball_in_hand
    }

    pub fn game_over(&self) -> Option<GameOver> {
        self.game_over
    }

    pub fn shots_taken(&self) -> u32 {
        self.shots_taken
    }

    /// The official rules: the decision the current player has to make
    /// before anyone shoots, if there is one (see [`Self::choose`]).
    pub fn pending_choice(&self) -> Option<BreakChoice> {
        self.choice
    }

    /// Object balls of `group` still on the table.
    pub fn remaining(&self, group: Group) -> Vec<BallId> {
        self.balls.iter().filter(|b| b.in_play() && group.contains(b.id)).map(|b| b.id).collect()
    }

    /// True if `player` may play for the 8.
    ///
    /// With the groups set, once their group is off the table. On an open
    /// table: under the Brazilian rules when no ball of either group is
    /// left on it (they all went down on fouls, which decide no group, and
    /// both players are on the 8); under the official rules once either
    /// group is off the table (4.4: the shooter may claim it and shoot the
    /// 8).
    pub fn is_on_eight(&self, player: usize) -> bool {
        match self.groups[player] {
            Some(g) => self.remaining(g).is_empty(),
            None => match self.rules {
                Rules::Brazilian => {
                    self.balls.iter().all(|b| !b.in_play() || Group::of(b.id).is_none())
                }
                Rules::Official => Group::ALL.iter().any(|g| self.remaining(*g).is_empty()),
            },
        }
    }

    /// Balls the current player may legally hit first.
    pub fn legal_targets(&self) -> Vec<BallId> {
        let player = self.current;
        let in_play = |id: BallId| self.ball(id).in_play();
        if self.is_break {
            return BallId::all().filter(|id| id.is_object() && in_play(*id)).collect();
        }
        let open_balls =
            || BallId::all().filter(|id| Group::of(*id).is_some() && in_play(*id)).collect();
        let targets: Vec<BallId> = match (self.groups[player], self.rules) {
            (Some(g), _) if self.remaining(g).is_empty() => vec![BallId::EIGHT],
            (Some(g), _) => self.remaining(g),
            (None, Rules::Brazilian) if self.is_on_eight(player) => vec![BallId::EIGHT],
            (None, Rules::Brazilian) => open_balls(),
            (None, Rules::Official) => {
                let mut balls: Vec<BallId> = open_balls();
                if self.is_on_eight(player) && in_play(BallId::EIGHT) {
                    balls.push(BallId::EIGHT);
                }
                balls
            }
        };
        if targets.is_empty() { vec![BallId::EIGHT] } else { targets }
    }

    /// A sensible aim when the player has not chosen one: the foot spot on
    /// the break, otherwise the nearest ball they may hit.
    pub fn default_aim(&self) -> f64 {
        let cue = self.ball(BallId::CUE).xy();
        if self.is_break {
            let d = self.table.foot_spot() - cue;
            return crate::math::atan2(d.y, d.x);
        }
        self.legal_targets()
            .into_iter()
            .map(|id| self.ball(id).xy() - cue)
            .min_by(|a, b| a.length().total_cmp(&b.length()))
            .map(|d| crate::math::atan2(d.y, d.x))
            .unwrap_or(0.0)
    }

    /// The pocket closest to `ball` (used as a default called pocket).
    pub fn nearest_pocket(&self, ball: BallId) -> PocketId {
        let p = self.ball(ball).xy();
        PocketId::ALL
            .into_iter()
            .min_by(|a, b| {
                let da = self.table.pocket(*a).mouth_center.distance(p);
                let db = self.table.pocket(*b).mouth_center.distance(p);
                da.total_cmp(&db)
            })
            .unwrap_or(PocketId::FootLeft)
    }

    /// The Brazilian rules: the pocket `player` called for the 8-ball, once
    /// it is binding.
    pub fn called_pocket_of(&self, player: usize) -> Option<PocketId> {
        self.called[player % 2]
    }

    /// The Brazilian rules: the current player is on the 8 but hasn't
    /// called a pocket yet.
    pub fn may_call_pocket(&self) -> bool {
        self.needs_called_pocket() && self.called[self.current].is_none()
    }

    /// The Brazilian rules: the shooter must call a pocket for this shot
    /// (the 8's).
    pub fn needs_called_pocket(&self) -> bool {
        self.rules == Rules::Brazilian && !self.is_break && self.is_on_eight(self.current)
    }

    /// The official rules: this shot is called (every shot but the break;
    /// no call makes it a safety).
    pub fn calls_shots(&self) -> bool {
        self.rules == Rules::Official && !self.is_break && self.choice.is_none()
    }

    /// True if the cue ball may be placed (or left) at `p` given the
    /// ball-in-hand rule: on the bed of the table, against a cushion at
    /// most and never past a pocket's mouth ([`Table::can_place_ball`]),
    /// behind the head string when it must be, and clear of every ball.
    pub fn can_place_cue_ball(&self, p: DVec2) -> bool {
        let Some(bih) = self.ball_in_hand else {
            return false;
        };
        if !self.table.can_place_ball(p) {
            return false;
        }
        if bih == BallInHand::Kitchen && p.x > self.table.head_string_x() {
            return false;
        }
        let min_dist = 2.0 * self.table.spec.ball_radius + 1e-4;
        self.balls
            .iter()
            .filter(|b| b.id != BallId::CUE && b.in_play())
            .all(|b| b.xy().distance(p) >= min_dist)
    }

    /// Moves the cue ball while it is in hand.
    pub fn place_cue_ball(&mut self, p: DVec2) -> bool {
        if !self.can_place_cue_ball(p) {
            return false;
        }
        let r = self.table.spec.ball_radius;
        self.balls[0] = Ball::at_rest(BallId::CUE, p, r);
        true
    }

    /// Development and tests: an open table past the break with only these
    /// balls on it (the others are put away in a pocket).
    #[doc(hidden)]
    pub fn set_up_test_position(&mut self, balls: &[(BallId, DVec2)]) {
        let r = self.table.spec.ball_radius;
        for b in &mut self.balls {
            match balls.iter().find(|(id, _)| *id == b.id) {
                Some(&(id, p)) => *b = Ball::at_rest(id, p, r),
                None => b.status = crate::BallStatus::Pocketed(PocketId::HeadLeft),
            }
        }
        self.is_break = false;
        self.ball_in_hand = None;
    }

    /// Validates a shot and starts simulating it. `called_pocket` is the
    /// call: under the Brazilian rules the 8's (needed once on the 8, and
    /// binding from then on), under the official rules this shot's (none:
    /// a safety).
    pub fn prepare_shot(
        &self,
        shot: &Shot,
        called_pocket: Option<PocketId>,
    ) -> Result<PendingShot, ShotError> {
        if self.game_over.is_some() {
            return Err(ShotError::GameIsOver);
        }
        if self.choice.is_some() {
            return Err(ShotError::ChoicePending);
        }
        if self.ball_in_hand.is_some() && !self.can_place_cue_ball(self.ball(BallId::CUE).xy()) {
            return Err(ShotError::IllegalCueBallPosition);
        }
        let called_pocket = match self.rules {
            Rules::Brazilian => {
                // Once called, the pocket for the 8 can't change.
                let called = match self.called[self.current] {
                    Some(locked) if self.needs_called_pocket() => Some(locked),
                    _ => called_pocket,
                };
                if self.needs_called_pocket() && called.is_none() {
                    return Err(ShotError::MustCallPocket);
                }
                called
            }
            // Nothing is called on the break.
            Rules::Official => called_pocket.filter(|_| !self.is_break),
        };
        // A tip beyond the edge of the ball is brought back to the edge, in
        // its direction (one so far out that its length overflows an f64 is
        // scaled down first).
        let mut shot = *shot;
        if shot.tip.is_finite() && !shot.tip.length().is_finite() {
            let largest = shot.tip.x.abs().max(shot.tip.y.abs());
            shot.tip = (shot.tip / largest).normalize();
        }
        let shot = shot.sanitized_for(&self.params, self.is_break);
        let mut simulation = Simulation::new(self.table.clone(), self.params, self.balls.clone());
        if self.rules == Rules::Official
            && !self.is_break
            && self.ball_in_hand == Some(BallInHand::Kitchen)
        {
            simulation.watch_crossing(BallId::CUE, self.table.head_string_x());
        }
        // Between shots the table is at rest with the cue ball on it (a
        // snapshot that isn't is refused when it is read): only a table set
        // up by hand without a cue ball gets here, and has nothing to strike.
        simulation.strike(BallId::CUE, &shot).map_err(|_| ShotError::IllegalCueBallPosition)?;
        Ok(PendingShot { simulation, shot, called_pocket })
    }

    /// Validates, simulates and applies a shot in one call.
    pub fn play_shot(
        &mut self,
        shot: &Shot,
        called_pocket: Option<PocketId>,
    ) -> Result<ShotReport, ShotError> {
        let mut pending = self.prepare_shot(shot, called_pocket)?;
        pending.simulation.run_to_rest();
        Ok(self.finish_shot(pending))
    }

    /// Applies the rules to a settled shot and advances the game.
    pub fn finish_shot(&mut self, mut pending: PendingShot) -> ShotReport {
        if !pending.simulation.is_settled() {
            pending.simulation.run_to_rest();
        }
        match self.rules {
            Rules::Brazilian => self.finish_brazilian(pending),
            Rules::Official => self.finish_official(pending),
        }
    }

    /// The official rules: the current player's answer to the pending
    /// [`BreakChoice`] (see [`BreakChoice::options`]).
    pub fn choose(&mut self, option: BreakOption) -> Result<(), ChoiceError> {
        let choice = self.choice.ok_or(ChoiceError::NoChoice)?;
        if !choice.options().contains(&option) {
            return Err(ChoiceError::NotAnOption);
        }
        self.apply_choice(choice, option);
        Ok(())
    }

    /// The balls are still racked as the game began (the break to come is
    /// on a full rack, not on balls a break left lying about).
    pub fn rack_untouched(&self) -> bool {
        let racked = rack(&self.table, self.rack_seed);
        racked.iter().filter(|b| b.id.is_object()).all(|b| {
            let now = self.ball(b.id);
            now.in_play() && now.xy().distance(b.xy()) < 1e-3
        })
    }

    /// `player` concedes (a seat number: only its parity counts, as in
    /// [`Self::conclude`], so a wrong one can't underflow).
    pub fn resign(&mut self, player: usize) {
        self.end_with(1 - player % 2, GameOverReason::Resignation);
    }

    /// `player` abandoned an online game (the seat as in [`Self::resign`]).
    pub fn abandon(&mut self, player: usize) {
        self.end_with(1 - player % 2, GameOverReason::Abandoned);
    }

    /// Ends the game with a result decided elsewhere (a client applying the
    /// server's verdict). Does nothing if the game is already over.
    pub fn conclude(&mut self, winner: usize, reason: GameOverReason) {
        self.end_with(winner % 2, reason);
    }

    fn end_with(&mut self, winner: usize, reason: GameOverReason) {
        if self.game_over.is_none() {
            self.game_over = Some(GameOver { winner, reason });
            self.choice = None;
        }
    }

    /// The shot clock ran out. Returns the balls it gave the opponent (the
    /// Brazilian rules' penalty, the 8 last when it paid).
    ///
    /// The Brazilian rules: a foul that gives the opponent a ball (their
    /// lowest; with none left, the 8 pays it and the game is lost), and the
    /// cue ball goes back to the head spot, not in hand; with the table
    /// still open nothing is owed. On the break, the break passes to the
    /// opponent, the cue ball in hand behind the head string.
    ///
    /// The official rules: slow play is a standard foul (3.14), and the
    /// opponent has the cue ball in hand anywhere on the table; on the
    /// break they break instead. A player who had to decide how play goes
    /// on after a break gets the first of the options (see
    /// [`BreakChoice::options`]).
    pub fn time_foul(&mut self) -> Vec<BallId> {
        if self.game_over.is_some() {
            return Vec::new();
        }
        if let Some(choice) = self.choice {
            self.apply_choice(choice, choice.options()[0]);
            return Vec::new();
        }
        let shooter = self.current;
        let opponent = 1 - shooter;
        let mut paid = Vec::new();
        match self.rules {
            Rules::Brazilian if self.is_break => {
                if !self.ball(BallId::CUE).in_play() {
                    self.respot_cue_ball();
                }
                self.ball_in_hand = Some(BallInHand::Kitchen);
            }
            Rules::Brazilian => {
                // The ball paid first: the cue ball goes to the head spot
                // it may have left free.
                if let Some(g) = self.groups[opponent] {
                    paid = self.pay_penalty(g, 1);
                } else if self.only_the_eight_left() {
                    // An open table with only the 8: both are on it, and the
                    // ball owed is the 8.
                    self.take_off(BallId::EIGHT);
                    paid.push(BallId::EIGHT);
                }
                if paid.contains(&BallId::EIGHT) {
                    self.end_with(opponent, GameOverReason::EightPocketedOnFoul);
                    return paid;
                }
                self.return_cue_ball();
            }
            Rules::Official if self.is_break => {
                self.respot_cue_ball();
                self.ball_in_hand = Some(BallInHand::Kitchen);
            }
            Rules::Official => {
                // In hand for the opponent, from a place it may be played
                // from (as after any foul).
                self.current = opponent;
                self.grant_hand(BallInHand::Anywhere);
                return paid;
            }
        }
        self.current = opponent;
        paid
    }

    /// No object ball on the table but the 8.
    pub(super) fn only_the_eight_left(&self) -> bool {
        self.ball(BallId::EIGHT).in_play()
            && self.balls.iter().all(|b| !b.id.is_object() || b.id == BallId::EIGHT || !b.in_play())
    }

    /// The Brazilian rules: `owed` balls of `group` come off the table for
    /// its player, lowest number first; with fewer left, the 8 pays the
    /// rest (it comes off last, and the foul loses the game). Returns them.
    pub(super) fn pay_penalty(&mut self, group: Group, owed: usize) -> Vec<BallId> {
        let mut given = self.remaining(group);
        given.sort();
        let eight_pays = owed > given.len();
        given.truncate(owed);
        if eight_pays {
            given.push(BallId::EIGHT);
        }
        for &b in &given {
            self.take_off(b);
        }
        given
    }

    /// Takes a ball off the table as if it had been made (a penalty ball):
    /// it goes down the pocket nearest to where it lay.
    fn take_off(&mut self, id: BallId) {
        let pocket = self.nearest_pocket(id);
        self.balls[id.index()].status = BallStatus::Pocketed(pocket);
    }

    fn is_spot_free(&self, p: DVec2, ignore: BallId) -> bool {
        let min_dist = 2.0 * self.table.spec.ball_radius + 1e-4;
        self.balls
            .iter()
            .filter(|b| b.id != ignore && b.in_play())
            .all(|b| b.xy().distance(p) >= min_dist)
    }

    /// Places a ball on the foot spot, or the nearest free point on the long
    /// string towards the foot rail (then towards the head rail).
    fn spot_ball(&mut self, id: BallId) {
        let r = self.table.spec.ball_radius;
        let start = self.table.foot_spot();
        let limit = self.table.spec.length / 2.0 - r;
        let step = r / 4.0;
        let mut candidates = (0..).map(|i| start.x + i as f64 * step).take_while(|x| *x <= limit);
        let mut pos = candidates.find(|x| self.is_spot_free(DVec2::new(*x, 0.0), id));
        if pos.is_none() {
            pos = (0..)
                .map(|i| start.x - i as f64 * step)
                .take_while(|x| *x >= -limit)
                .find(|x| self.is_spot_free(DVec2::new(*x, 0.0), id));
        }
        let x = pos.unwrap_or(start.x);
        self.balls[id.index()] = Ball::at_rest(id, DVec2::new(x, 0.0), r);
    }

    /// The cue ball that went down or left the table goes back where it
    /// starts the game, on the head spot, or right behind the ball lying
    /// there (towards the head rail, on the long string), and is played
    /// from there: not in hand.
    fn return_cue_ball(&mut self) {
        let r = self.table.spec.ball_radius;
        let head = self.table.head_spot();
        // A sixteenth of a radius at a time: behind a ball, all but against it.
        let step = r / 16.0;
        let spot = (0..)
            .map(|i| DVec2::new(head.x - i as f64 * step, head.y))
            .take_while(|p| self.table.can_place_ball(*p))
            .find(|p| self.is_spot_free(*p, BallId::CUE));
        match spot {
            Some(p) => self.balls[0] = Ball::at_rest(BallId::CUE, p, r),
            // The whole string behind the spot taken: the nearest free
            // place behind the head string.
            None => self.respot_cue_ball(),
        }
        self.ball_in_hand = None;
    }

    /// Puts the cue ball on the head spot, or the nearest free place
    /// behind the head string.
    fn respot_cue_ball(&mut self) {
        let r = self.table.spec.ball_radius;
        let head = self.table.head_spot();
        let mut p = head;
        'search: for ring in 0..40 {
            for k in 0..16 {
                let angle = k as f64 / 16.0 * std::f64::consts::TAU;
                let cand = head + crate::math::from_angle(angle) * (ring as f64 * r);
                if self.table.can_place_ball(cand)
                    && cand.x <= self.table.head_string_x()
                    && self.is_spot_free(cand, BallId::CUE)
                {
                    p = cand;
                    break 'search;
                }
            }
        }
        self.balls[0] = Ball::at_rest(BallId::CUE, p, r);
    }

    /// A new rack for a break again, shuffled from the first rack's seed
    /// and the shots taken (the same on the server and in every game),
    /// broken by `breaker` with the cue ball in hand behind the head string.
    fn rerack(&mut self, breaker: usize) {
        let seed = self.rack_seed.wrapping_add(0x9e37_79b9 * (self.shots_taken as u64 + 1));
        self.balls = rack(&self.table, seed);
        self.rack_seed = seed;
        self.groups = [None, None];
        self.called = [None, None];
        self.is_break = true;
        self.ball_in_hand = Some(BallInHand::Kitchen);
        self.current = breaker % 2;
    }
}

/// Builds a standard eight-ball rack: apex on the foot spot, the 8 in the
/// centre, one solid and one stripe on the back corners, the rest shuffled.
pub fn rack(table: &Table, seed: u64) -> Vec<Ball> {
    let r = table.spec.ball_radius;
    let gap = 1e-4;
    let spacing = 2.0 * r + gap;
    let row_dx = spacing * (3f64).sqrt() / 2.0;
    let apex = table.foot_spot();

    let mut rng = Rng::new(seed);
    let mut solids: Vec<u8> = (1..=7).collect();
    let mut stripes: Vec<u8> = (9..=15).collect();
    rng.shuffle(&mut solids);
    rng.shuffle(&mut stripes);
    let (corner_a, corner_b) = if rng.below(2) == 0 {
        (solids.pop().unwrap(), stripes.pop().unwrap())
    } else {
        (stripes.pop().unwrap(), solids.pop().unwrap())
    };
    let mut rest: Vec<u8> = solids.into_iter().chain(stripes).collect();
    rng.shuffle(&mut rest);

    let mut balls = vec![Ball::at_rest(BallId::CUE, table.head_spot(), r)];
    let mut slots = Vec::with_capacity(15);
    for row in 0..5 {
        for k in 0..=row {
            let x = apex.x + row as f64 * row_dx;
            let y = (k as f64 - row as f64 / 2.0) * spacing;
            slots.push((row, k, DVec2::new(x, y)));
        }
    }
    let mut rest = rest.into_iter();
    for (row, k, p) in slots {
        let number = match (row, k) {
            (2, 1) => 8,
            (4, 0) => corner_a,
            (4, 4) => corner_b,
            _ => rest.next().expect("fifteen balls"),
        };
        balls.push(Ball::at_rest(BallId(number), p, r));
    }
    balls.sort_by_key(|b| b.id);
    debug_assert!(balls.iter().all(|b| b.status == BallStatus::InPlay));
    balls
}
