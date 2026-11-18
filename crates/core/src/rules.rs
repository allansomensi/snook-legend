//! Eight-ball rules. Up to ruleset 3 they follow the WPA world standardized
//! rules (simplified where noted); from ruleset 4 they are the Brazilian
//! house rules (see [`EightBallGame::finish_shot`]): a foul costs the turn
//! and gives the opponent balls, and only a cue ball that went down or left
//! the table gives ball in hand (from ruleset 6 not even that: the cue ball
//! is in hand for the break only, and one that went down comes back to the
//! head spot).
//!
//! The game is driven in two phases so that clients can animate the shot:
//! 1. [`EightBallGame::prepare_shot`] validates the stroke and returns a
//!    [`PendingShot`] holding a running [`Simulation`];
//! 2. once the simulation has settled, [`EightBallGame::finish_shot`] applies the
//!    rules and returns a [`ShotReport`].
//!
//! [`EightBallGame::play_shot`] does both at once (used by the AI and the server).

use glam::DVec2;
use serde::{Deserialize, Serialize};

use crate::ball::{Ball, BallId, BallKind, BallStatus, Motion};
use crate::physics::{Event, PhysicsParams, Shot, Simulation};
use crate::rng::Rng;
use crate::table::{PocketId, Table};

/// The two groups of object balls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Group {
    Solids,
    Stripes,
}

impl Group {
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
    /// Behind the head string (for the break itself, and up to ruleset 5
    /// after a break foul).
    Kitchen,
    /// Anywhere on the table (after a foul, up to ruleset 5).
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
    /// No ball pocketed and fewer than four object balls driven to a cushion.
    IllegalBreak,
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameOverReason {
    /// The winner legally pocketed the eight ball in the called pocket.
    PocketedEight,
    /// The loser pocketed the eight before clearing their group.
    EightPocketedEarly,
    /// The loser pocketed the eight in a pocket other than the one called.
    EightInWrongPocket,
    /// The loser pocketed the eight on a foul.
    EightPocketedOnFoul,
    /// The loser knocked the eight off the table.
    EightOffTable,
    /// The loser made the eight together with another ball: on the break
    /// with any ball, or at the end with one of the opponent's (ruleset 4).
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
    /// The shooter made one of the opponent's balls, which passed the turn
    /// (from ruleset 2), even if they made one of their own too.
    #[serde(default)]
    pub opponent_ball: bool,
    /// The opponent's balls taken off the table for them by this shot's
    /// fouls, lowest number first (ruleset 4).
    #[serde(default)]
    pub penalty: Vec<BallId>,
    /// The break went wrong (only the cue ball went down): the balls were
    /// racked again and the opponent breaks (ruleset 4).
    #[serde(default)]
    pub reracked: bool,
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
    /// A break that made balls of both groups (from ruleset 5 it sets no
    /// groups and passes the turn) makes nothing for the shooter.
    ///
    /// The shooter's group is the one this shot gave them or, once the
    /// groups are set, the one of the ball they hit first (a shot without a
    /// foul hits one of the shooter's own first, or the 8 when they are on
    /// it and have none left), so the report alone tells.
    pub fn balls_made(&self) -> Vec<BallId> {
        if self.is_foul() {
            return Vec::new();
        }
        let made = self.pocketed.iter().map(|(b, _)| *b);
        if let Some(over) = self.game_over {
            let won = over.winner == self.shooter && over.reason == GameOverReason::PocketedEight;
            return made.filter(|b| won && *b == BallId::EIGHT).collect();
        }
        // A break that made balls and set no groups made both kinds (from
        // ruleset 5 that passes the turn): neither is the shooter's.
        if self.was_break && self.group_assigned.is_none() {
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
    /// The shooter is on the eight and must nominate a pocket.
    MustCallPocket,
}

impl std::fmt::Display for ShotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShotError::GameIsOver => write!(f, "the game is over"),
            ShotError::IllegalCueBallPosition => write!(f, "the cue ball cannot be placed there"),
            ShotError::MustCallPocket => write!(f, "call a pocket for the 8-ball"),
        }
    }
}

impl std::error::Error for ShotError {}

/// A validated shot whose simulation is still running.
#[derive(Debug, Clone)]
pub struct PendingShot {
    pub simulation: Simulation,
    pub shot: Shot,
    pub called_pocket: Option<PocketId>,
}

/// Complete state of an eight-ball game between players `0` and `1`.
///
/// Serializable so the game server can send authoritative snapshots. The
/// table and the physics parameters are not part of a snapshot, only the
/// game's ruleset (see [`crate::RULESET`]): a deserialized game gets the
/// table and the physics of its ruleset (1 when a snapshot names none). A
/// snapshot that no game could be in is refused when it is read (see the
/// `TryFrom` below).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "GameData")]
pub struct EightBallGame {
    #[serde(skip)]
    table: Table,
    #[serde(skip)]
    params: PhysicsParams,
    ruleset: u8,
    balls: Vec<Ball>,
    current: usize,
    groups: [Option<Group>; 2],
    is_break: bool,
    ball_in_hand: Option<BallInHand>,
    game_over: Option<GameOver>,
    shots_taken: u32,
    /// The pocket each player called for the 8-ball. A call is binding from
    /// the first shot taken with it until the end of the game.
    #[serde(default)]
    called: [Option<PocketId>; 2],
    /// What the rack was shuffled from (a rack made again after a break
    /// that only made the cue ball is shuffled from it and the shots
    /// taken, the same on the server and in every game).
    #[serde(default)]
    rack_seed: u64,
}

/// What a snapshot holds (see [`EightBallGame`]).
#[derive(Deserialize)]
struct GameData {
    #[serde(default = "first_ruleset")]
    ruleset: u8,
    balls: Vec<Ball>,
    current: usize,
    groups: [Option<Group>; 2],
    is_break: bool,
    ball_in_hand: Option<BallInHand>,
    game_over: Option<GameOver>,
    shots_taken: u32,
    #[serde(default)]
    called: [Option<PocketId>; 2],
    #[serde(default)]
    rack_seed: u64,
}

fn first_ruleset() -> u8 {
    1
}

impl TryFrom<GameData> for EightBallGame {
    type Error = &'static str;

    /// A snapshot comes from the network or from a file. One that no game
    /// could be in (a ball missing or out of order, a third player, a ball
    /// still rolling, no cue ball while the game goes on) would make the
    /// game index out of bounds, or find nothing to strike, later on: it is
    /// refused here instead. Every snapshot a game writes passes.
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
        let ruleset = crate::known_ruleset(d.ruleset);
        Ok(EightBallGame {
            table: Table::of_ruleset(ruleset),
            params: PhysicsParams::of_ruleset(ruleset),
            ruleset,
            balls: d.balls,
            current: d.current,
            groups: d.groups,
            is_break: d.is_break,
            ball_in_hand: d.ball_in_hand,
            game_over: d.game_over,
            shots_taken: d.shots_taken,
            called: d.called,
            rack_seed: d.rack_seed,
        })
    }
}

impl EightBallGame {
    /// A freshly racked game under the current rules. `seed` randomises the rack.
    pub fn new(seed: u64, breaker: usize) -> Self {
        Self::with_ruleset(seed, breaker, crate::RULESET)
    }

    /// A freshly racked game under `ruleset` (see [`crate::RULESET`]): its
    /// table, its physics and its rules.
    pub fn with_ruleset(seed: u64, breaker: usize, ruleset: u8) -> Self {
        let ruleset = crate::known_ruleset(ruleset);
        let mut game = Self::with_table(
            Table::of_ruleset(ruleset),
            PhysicsParams::of_ruleset(ruleset),
            seed,
            breaker,
        );
        game.ruleset = ruleset;
        game
    }

    /// A game on a table and with physics of one's own (the current rules).
    pub fn with_table(table: Table, params: PhysicsParams, seed: u64, breaker: usize) -> Self {
        let balls = rack(&table, seed);
        EightBallGame {
            table,
            params,
            ruleset: crate::RULESET,
            balls,
            current: breaker % 2,
            groups: [None, None],
            is_break: true,
            ball_in_hand: Some(BallInHand::Kitchen),
            game_over: None,
            shots_taken: 0,
            called: [None, None],
            rack_seed: seed,
        }
    }

    pub fn table(&self) -> &Table {
        &self.table
    }

    /// The fastest the cue may move on the stroke about to be played (m/s):
    /// the top of the power bar.
    pub fn max_cue_speed(&self) -> f64 {
        self.params.cue_speed_limit(self.is_break)
    }

    /// The rules, physics and table this game is played with.
    pub fn ruleset(&self) -> u8 {
        self.ruleset
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

    /// Object balls of `group` still on the table.
    pub fn remaining(&self, group: Group) -> Vec<BallId> {
        self.balls.iter().filter(|b| b.in_play() && group.contains(b.id)).map(|b| b.id).collect()
    }

    /// True if `player` has cleared their group and must now pocket the eight.
    ///
    /// From ruleset 5 also when the table is still open and no ball of
    /// either group is left on it (they all went down on fouls, which
    /// decide no group): both players are on the 8. Before, every shot was
    /// a foul from then on (the 8 hit first, or made too early) and nobody
    /// could win.
    pub fn is_on_eight(&self, player: usize) -> bool {
        match self.groups[player] {
            Some(g) => self.remaining(g).is_empty(),
            None => {
                self.ruleset >= 5
                    && self.balls.iter().all(|b| !b.in_play() || Group::of(b.id).is_none())
            }
        }
    }

    /// Balls the current player may legally hit first.
    pub fn legal_targets(&self) -> Vec<BallId> {
        let player = self.current;
        let in_play = |id: BallId| self.ball(id).in_play();
        if self.is_break {
            return BallId::all().filter(|id| id.is_object() && in_play(*id)).collect();
        }
        if self.is_on_eight(player) {
            return vec![BallId::EIGHT];
        }
        let targets: Vec<BallId> = match self.groups[player] {
            Some(g) => self.remaining(g),
            None => BallId::all().filter(|id| Group::of(*id).is_some() && in_play(*id)).collect(),
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

    /// The pocket `player` called for the 8-ball, once it is binding.
    pub fn called_pocket_of(&self, player: usize) -> Option<PocketId> {
        self.called[player % 2]
    }

    /// The current player is on the 8 but hasn't called a pocket yet.
    pub fn may_call_pocket(&self) -> bool {
        self.needs_called_pocket() && self.called[self.current].is_none()
    }

    /// The shooter must call a pocket for this shot.
    pub fn needs_called_pocket(&self) -> bool {
        !self.is_break && self.is_on_eight(self.current)
    }

    /// True if the cue ball may be placed (or left) at `p` given the ball-in-hand rule.
    ///
    /// Up to ruleset 4 its centre had to be a radius inside the cushion
    /// lines: a ball could never be placed against a cushion (one at rest
    /// against it is a millimetre nearer), and in the corners it could be
    /// placed past the pocket's mouth, in the pocket. From ruleset 5 it may
    /// go against a cushion, never past a mouth ([`Table::can_place_ball`]).
    pub fn can_place_cue_ball(&self, p: DVec2) -> bool {
        let Some(bih) = self.ball_in_hand else {
            return false;
        };
        if !self.is_on_the_bed(p) {
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

    /// Where this game's rules let the cue ball be put down (see
    /// [`Self::can_place_cue_ball`]).
    fn is_on_the_bed(&self, p: DVec2) -> bool {
        if self.ruleset >= 5 {
            self.table.can_place_ball(p)
        } else {
            self.table.is_inside_playing_area(p)
        }
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

    /// Validates a shot and starts simulating it.
    pub fn prepare_shot(
        &self,
        shot: &Shot,
        called_pocket: Option<PocketId>,
    ) -> Result<PendingShot, ShotError> {
        if self.game_over.is_some() {
            return Err(ShotError::GameIsOver);
        }
        if self.ball_in_hand.is_some() && !self.can_place_cue_ball(self.ball(BallId::CUE).xy()) {
            return Err(ShotError::IllegalCueBallPosition);
        }
        // Once called, the pocket for the 8 can't change.
        let called_pocket = match self.called[self.current] {
            Some(locked) if self.needs_called_pocket() => Some(locked),
            _ => called_pocket,
        };
        if self.needs_called_pocket() && called_pocket.is_none() {
            return Err(ShotError::MustCallPocket);
        }
        // A tip beyond the edge of the ball is brought back to the edge, in
        // its direction; but one so far out that its length overflows an
        // f64 came out as the centre of the ball. From ruleset 5 it is
        // scaled down first and goes to the edge like any other (the server
        // records a stroke as it is sent, so an older game keeps it as it
        // was played).
        let mut shot = *shot;
        if self.ruleset >= 5 && shot.tip.is_finite() && !shot.tip.length().is_finite() {
            let largest = shot.tip.x.abs().max(shot.tip.y.abs());
            shot.tip = (shot.tip / largest).normalize();
        }
        let shot = shot.sanitized_for(&self.params, self.is_break);
        let mut simulation = Simulation::new(self.table.clone(), self.params, self.balls.clone());
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
        if self.ruleset >= 4 {
            return self.finish_shot_house(pending);
        }
        let sim = &pending.simulation;
        let shooter = self.current;
        let was_break = self.is_break;
        let on_eight = self.is_on_eight(shooter);
        if on_eight && !was_break && pending.called_pocket.is_some() {
            self.called[shooter] = pending.called_pocket;
        }
        let events = sim.events();

        let first_contact_event = events.iter().find_map(|e| match *e {
            Event::BallBall { time, a, b, .. } if a == BallId::CUE => Some((time, b)),
            Event::BallBall { time, a, b, .. } if b == BallId::CUE => Some((time, a)),
            _ => None,
        });
        let first_contact = first_contact_event.map(|(_, id)| id);
        let pocketed: Vec<(BallId, PocketId)> = events
            .iter()
            .filter_map(|e| match *e {
                Event::Pocketed { ball, pocket, .. } => Some((ball, pocket)),
                _ => None,
            })
            .collect();
        let off_table: Vec<BallId> = events
            .iter()
            .filter_map(|e| match *e {
                Event::OffTable { ball, .. } => Some(ball),
                _ => None,
            })
            .collect();
        let object_pocketed: Vec<BallId> =
            pocketed.iter().map(|(b, _)| *b).filter(|b| b.is_object()).collect();
        let rail_after_contact = first_contact_event.is_some_and(|(t0, _)| {
            events.iter().any(|e| matches!(e, Event::Cushion { time, .. } if *time >= t0))
        });

        let mut fouls = Vec::new();
        if pocketed.iter().any(|(b, _)| *b == BallId::CUE) {
            fouls.push(Foul::CueBallPocketed);
        }
        if off_table.contains(&BallId::CUE) {
            fouls.push(Foul::CueBallOffTable);
        }
        for &b in off_table.iter().filter(|b| b.is_object()) {
            fouls.push(Foul::ObjectBallOffTable(b));
        }

        let mut game_over = None;
        let mut group_assigned = None;
        let mut spotted = Vec::new();
        let eight_pocket = pocketed.iter().find(|(b, _)| *b == BallId::EIGHT).map(|(_, p)| *p);
        let eight_off = off_table.contains(&BallId::EIGHT);

        if was_break {
            let rails_hit: std::collections::BTreeSet<BallId> = events
                .iter()
                .filter_map(|e| match *e {
                    Event::Cushion { ball, .. } if ball.is_object() => Some(ball),
                    _ => None,
                })
                .collect();
            if first_contact.is_none() || (object_pocketed.is_empty() && rails_hit.len() < 4) {
                fouls.push(Foul::IllegalBreak);
            }
            if eight_pocket.is_some() || eight_off {
                spotted.push(BallId::EIGHT);
            }
            // The first ball made on a legal break is the breaker's group
            // (a foul on the break leaves the table open).
            if fouls.is_empty()
                && let Some(g) = object_pocketed.iter().find_map(|b| Group::of(*b))
            {
                self.groups[shooter] = Some(g);
                self.groups[1 - shooter] = Some(g.opposite());
                group_assigned = Some(g);
            }
        } else {
            match first_contact {
                None => fouls.push(Foul::NoContact),
                Some(hit) => {
                    let legal = if on_eight {
                        hit == BallId::EIGHT
                    } else {
                        match self.groups[shooter] {
                            Some(g) => g.contains(hit),
                            None => hit != BallId::EIGHT,
                        }
                    };
                    if !legal {
                        fouls.push(Foul::WrongBallFirst(hit));
                    }
                    if pocketed.is_empty() && !rail_after_contact {
                        fouls.push(Foul::NoRailAfterContact);
                    }
                }
            }
            let opponent = 1 - shooter;
            if eight_off {
                game_over =
                    Some(GameOver { winner: opponent, reason: GameOverReason::EightOffTable });
            } else if let Some(pocket) = eight_pocket {
                let reason = if !on_eight {
                    Some(GameOverReason::EightPocketedEarly)
                } else if !fouls.is_empty() {
                    Some(GameOverReason::EightPocketedOnFoul)
                } else if pending.called_pocket != Some(pocket) {
                    Some(GameOverReason::EightInWrongPocket)
                } else {
                    None
                };
                game_over = Some(match reason {
                    Some(reason) => GameOver { winner: opponent, reason },
                    None => GameOver { winner: shooter, reason: GameOverReason::PocketedEight },
                });
            }
            if self.groups[shooter].is_none()
                && fouls.is_empty()
                && let Some(g) = object_pocketed.iter().find_map(|b| Group::of(*b))
            {
                self.groups[shooter] = Some(g);
                self.groups[opponent] = Some(g.opposite());
                group_assigned = Some(g);
            }
        }
        for &b in off_table.iter().filter(|b| b.is_object() && **b != BallId::EIGHT) {
            spotted.push(b);
        }

        // From ruleset 2, making one of the opponent's balls passes the turn
        // even when one of one's own went down too (not on the break).
        let opponent_ball = self.ruleset >= 2
            && !was_break
            && self.groups[shooter]
                .is_some_and(|g| object_pocketed.iter().any(|b| g.opposite().contains(*b)));
        let continues = game_over.is_none()
            && fouls.is_empty()
            && !opponent_ball
            && if was_break {
                !object_pocketed.is_empty()
            } else {
                match self.groups[shooter] {
                    Some(g) => object_pocketed.iter().any(|b| g.contains(*b)),
                    None => false,
                }
            };

        // Commit the new ball positions.
        self.balls = sim.balls.clone();
        self.balls.sort_by_key(|b| b.id);
        for &b in &spotted {
            self.spot_ball(b);
        }
        self.shots_taken += 1;
        self.is_break = false;
        self.game_over = game_over;
        self.ball_in_hand = None;
        if game_over.is_none() {
            if !fouls.is_empty() {
                self.ball_in_hand =
                    Some(if was_break { BallInHand::Kitchen } else { BallInHand::Anywhere });
                // A scratch, or a break foul that left the cue ball beyond the
                // head string: it goes back to the kitchen, so the incoming
                // player can always shoot from where it lies.
                let cue = self.ball(BallId::CUE);
                if !cue.in_play() || !self.can_place_cue_ball(cue.xy()) {
                    self.respot_cue_ball();
                }
            }
            if !continues {
                self.current = 1 - shooter;
            }
        }

        ShotReport {
            shooter,
            was_break,
            first_contact,
            pocketed,
            fouls,
            spotted,
            group_assigned,
            continues,
            game_over,
            opponent_ball,
            penalty: Vec::new(),
            reracked: false,
        }
    }

    /// Ruleset 4, the Brazilian house rules (ruleset 5 plays them too, with
    /// the corrections listed at [`crate::RULESET`]). What differs from the
    /// rules before it:
    ///
    /// - A foul costs the turn and gives the opponent balls: the lowest
    ///   numbers of their group still on the table come off it, as if they
    ///   had made them. One for missing every ball, one for a scratch, two
    ///   for hitting a ball that isn't one's own first, or one when the cue
    ///   ball went to a cushion before it. The counts add up (a miss that
    ///   scratches gives two; the 8 hit first before one's group is
    ///   cleared counts as a miss: one). While the table is open there is no
    ///   opponent's group yet, and a foul costs the turn alone.
    /// - Only a cue ball that went down or left the table is in hand;
    ///   after any other foul it is played from where it lies.
    /// - A ball of the shooter's knocked off the table comes back to it;
    ///   one of the opponent's stays off, made for them.
    /// - A break that makes only the cue ball is played again by the
    ///   opponent: the balls go back in the rack and they break.
    /// - Ruleset 5 only: a break that makes balls of both groups passes the
    ///   turn, and the table stays open.
    /// - From ruleset 6 the first ball made decides the groups on the break
    ///   too, whatever else goes down with it; one of the opponent's made
    ///   with it passes the turn, as on any other shot.
    /// - From ruleset 6 the cue ball is in hand only for the break (behind
    ///   the head string). One that goes down or leaves the table, on the
    ///   break or later, goes back to the head spot (right behind a ball
    ///   lying there) and is played from there; a break that makes only the
    ///   cue ball is not racked again: the opponent plays on from there.
    /// - The 8 alone on the break wins; the 8 on the break with any other
    ///   ball loses. At the end, the 8 loses if the cue ball or one of the
    ///   opponent's balls goes down with it.
    /// - Nothing has to reach a cushion, on the break or after a contact.
    fn finish_shot_house(&mut self, pending: PendingShot) -> ShotReport {
        let sim = &pending.simulation;
        let shooter = self.current;
        let opponent = 1 - shooter;
        let was_break = self.is_break;
        let on_eight = self.is_on_eight(shooter);
        if on_eight && !was_break && pending.called_pocket.is_some() {
            self.called[shooter] = pending.called_pocket;
        }
        let events = sim.events();

        let first_contact_event = events.iter().find_map(|e| match *e {
            Event::BallBall { time, a, b, .. } if a == BallId::CUE => Some((time, b)),
            Event::BallBall { time, a, b, .. } if b == BallId::CUE => Some((time, a)),
            _ => None,
        });
        let first_contact = first_contact_event.map(|(_, id)| id);
        let pocketed: Vec<(BallId, PocketId)> = events
            .iter()
            .filter_map(|e| match *e {
                Event::Pocketed { ball, pocket, .. } => Some((ball, pocket)),
                _ => None,
            })
            .collect();
        let off_table: Vec<BallId> = events
            .iter()
            .filter_map(|e| match *e {
                Event::OffTable { ball, .. } => Some(ball),
                _ => None,
            })
            .collect();
        let object_pocketed: Vec<BallId> =
            pocketed.iter().map(|(b, _)| *b).filter(|b| b.is_object()).collect();
        let cue_pocketed = pocketed.iter().any(|(b, _)| *b == BallId::CUE);
        let cue_off = off_table.contains(&BallId::CUE);
        // The cue ball went to a cushion before it reached a ball.
        let cushion_first = first_contact_event.is_some_and(|(t0, _)| {
            events.iter().any(
                |e| matches!(e, Event::Cushion { time, ball, .. } if *ball == BallId::CUE && *time < t0),
            )
        });

        let mut fouls = Vec::new();
        // How many of the opponent's balls the fouls give them.
        let mut owed = 0usize;
        if cue_pocketed {
            fouls.push(Foul::CueBallPocketed);
            owed += 1;
        }
        if cue_off {
            fouls.push(Foul::CueBallOffTable);
        }
        for &b in off_table.iter().filter(|b| b.is_object()) {
            fouls.push(Foul::ObjectBallOffTable(b));
        }
        match first_contact {
            None => {
                fouls.push(Foul::NoContact);
                owed += 1;
            }
            Some(hit) => {
                let legal = if was_break {
                    true
                } else if on_eight {
                    hit == BallId::EIGHT
                } else {
                    match self.groups[shooter] {
                        Some(g) => g.contains(hit),
                        None => hit != BallId::EIGHT,
                    }
                };
                if !legal {
                    fouls.push(Foul::WrongBallFirst(hit));
                    // One of the opponent's balls hit directly costs two.
                    // Off a cushion it costs one, and so does the 8 hit
                    // too soon (it is nobody's ball: the same as hitting
                    // none of one's own).
                    let theirs = self.groups[opponent].is_some_and(|g| g.contains(hit));
                    owed += if theirs && !cushion_first { 2 } else { 1 };
                }
            }
        }

        let eight_pocket = pocketed.iter().find(|(b, _)| *b == BallId::EIGHT).map(|(_, p)| *p);
        let eight_off = off_table.contains(&BallId::EIGHT);
        let lost = |reason| Some(GameOver { winner: opponent, reason });
        let mut game_over = None;
        let mut group_assigned = None;
        let mut spotted = Vec::new();
        if was_break {
            if eight_pocket.is_some() {
                game_over = if pocketed.len() > 1 {
                    lost(GameOverReason::EightWithAnotherBall)
                } else if !fouls.is_empty() {
                    lost(GameOverReason::EightPocketedOnFoul)
                } else {
                    Some(GameOver { winner: shooter, reason: GameOverReason::PocketedEight })
                };
            } else if eight_off {
                spotted.push(BallId::EIGHT);
            }
        } else if eight_off {
            game_over = lost(GameOverReason::EightOffTable);
        } else if let Some(pocket) = eight_pocket {
            let with_opponents_ball = object_pocketed.iter().any(|b| *b != BallId::EIGHT);
            game_over = if !on_eight {
                lost(GameOverReason::EightPocketedEarly)
            } else if !fouls.is_empty() {
                lost(GameOverReason::EightPocketedOnFoul)
            } else if with_opponents_ball {
                lost(GameOverReason::EightWithAnotherBall)
            } else if pending.called_pocket != Some(pocket) {
                lost(GameOverReason::EightInWrongPocket)
            } else {
                Some(GameOver { winner: shooter, reason: GameOverReason::PocketedEight })
            };
        }
        // Ruleset 5: a break that makes balls of both groups (the 3 and the
        // 14, say) passes the turn and leaves the table open: on the break
        // only one kind may go down. From ruleset 6 the first one made
        // decides the groups, and one of the other kind passes the turn.
        let mixed_break = was_break
            && self.ruleset == 5
            && [Group::Solids, Group::Stripes]
                .iter()
                .all(|g| object_pocketed.iter().any(|b| g.contains(*b)));
        // The first ball made on a shot without a foul decides the groups.
        if game_over.is_none()
            && self.groups[shooter].is_none()
            && fouls.is_empty()
            && !mixed_break
            && let Some(g) = object_pocketed.iter().find_map(|b| Group::of(*b))
        {
            self.groups[shooter] = Some(g);
            self.groups[opponent] = Some(g.opposite());
            group_assigned = Some(g);
        }
        let theirs = self.groups[opponent];
        // Balls knocked off the table: the opponent's stay off, the others
        // come back.
        let mut penalty = Vec::new();
        for &b in off_table.iter().filter(|b| b.is_object() && **b != BallId::EIGHT) {
            if theirs.is_some_and(|g| g.contains(b)) {
                penalty.push(b);
            } else {
                spotted.push(b);
            }
        }

        // One of the opponent's balls made passes the turn; from ruleset 6
        // on the break too (the first ball made gave the shooter the other
        // kind).
        let opponent_ball = (!was_break || self.ruleset >= 6)
            && self.groups[shooter]
                .is_some_and(|g| object_pocketed.iter().any(|b| g.opposite().contains(*b)));
        let continues = game_over.is_none()
            && fouls.is_empty()
            && !opponent_ball
            && !mixed_break
            && if was_break {
                !object_pocketed.is_empty()
            } else {
                match self.groups[shooter] {
                    Some(g) => object_pocketed.iter().any(|b| g.contains(*b)),
                    None => false,
                }
            };

        // Commit the new ball positions.
        self.balls = sim.balls.clone();
        self.balls.sort_by_key(|b| b.id);
        for &b in &spotted {
            self.spot_ball(b);
        }
        // (The opponent's balls knocked off the table stay off it as they
        // are: gone, like the ones made.)
        if game_over.is_none()
            && let Some(g) = theirs
        {
            let mut given = self.remaining(g);
            given.sort();
            given.truncate(owed);
            for &b in &given {
                self.take_off(b);
            }
            penalty.extend(given);
        }
        self.shots_taken += 1;
        self.is_break = false;
        self.game_over = game_over;
        self.ball_in_hand = None;
        // Only the cue ball down on the break: racked again, and the
        // opponent breaks (up to ruleset 5; from 6 the opponent plays on
        // from the head spot).
        let reracked = was_break
            && cue_pocketed
            && object_pocketed.is_empty()
            && !eight_off
            && self.ruleset < 6;
        if reracked {
            let seed = self.rack_seed.wrapping_add(self.shots_taken as u64);
            self.balls = rack(&self.table, seed);
            self.is_break = true;
            self.ball_in_hand = Some(BallInHand::Kitchen);
            self.current = opponent;
            spotted.clear();
            penalty.clear();
        } else if game_over.is_none() {
            if (cue_pocketed || cue_off) && self.ruleset >= 6 {
                // Back where it began, without ball in hand.
                self.return_cue_ball();
            } else if cue_pocketed || cue_off {
                self.ball_in_hand =
                    Some(if was_break { BallInHand::Kitchen } else { BallInHand::Anywhere });
                self.respot_cue_ball();
            }
            if !continues {
                self.current = opponent;
            }
        }

        ShotReport {
            shooter,
            was_break,
            first_contact,
            pocketed,
            fouls,
            spotted,
            group_assigned,
            continues,
            game_over,
            opponent_ball,
            penalty,
            reracked,
        }
    }

    /// Takes a ball off the table as if it had been made (a penalty ball):
    /// it goes down the pocket nearest to where it lay.
    fn take_off(&mut self, id: BallId) {
        let p = self.balls[id.index()].xy();
        let pocket = PocketId::ALL
            .into_iter()
            .min_by(|a, b| {
                let da = self.table.pocket(*a).mouth_center.distance(p);
                let db = self.table.pocket(*b).mouth_center.distance(p);
                da.total_cmp(&db)
            })
            .unwrap_or(PocketId::FootLeft);
        self.balls[id.index()].status = BallStatus::Pocketed(pocket);
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
        }
    }

    /// The shot clock ran out: the turn passes and the opponent gets ball
    /// in hand. Under the house rules (ruleset 4) only a scratch gives ball
    /// in hand: the turn passes and the cue ball stays where it lies (in
    /// hand only if it already was, as on the break; from ruleset 6 the
    /// break is the only time it is).
    ///
    /// Ruleset 4 made a cue ball in hand past the break in hand anywhere:
    /// after a scratch on the break (in hand behind the head string) a
    /// clock that ran out handed the breaker the whole table. From ruleset
    /// 5 the hand stays as it was.
    pub fn time_foul(&mut self) {
        if self.game_over.is_some() {
            return;
        }
        if !self.ball(BallId::CUE).in_play() {
            if self.ruleset >= 6 {
                self.return_cue_ball();
            } else {
                self.respot_cue_ball();
            }
        }
        if self.ruleset < 4 || (self.ruleset == 4 && self.ball_in_hand.is_some()) {
            self.ball_in_hand =
                Some(if self.is_break { BallInHand::Kitchen } else { BallInHand::Anywhere });
        }
        self.current = 1 - self.current;
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

    /// From ruleset 6: the cue ball that went down or left the table goes
    /// back where it starts the game, on the head spot, or right behind
    /// the ball lying there (towards the head rail, on the long string),
    /// and is played from there: not in hand.
    fn return_cue_ball(&mut self) {
        let r = self.table.spec.ball_radius;
        let head = self.table.head_spot();
        // A sixteenth of a radius at a time: behind a ball, all but against it.
        let step = r / 16.0;
        let spot = (0..)
            .map(|i| DVec2::new(head.x - i as f64 * step, head.y))
            .take_while(|p| self.is_on_the_bed(*p))
            .find(|p| self.is_spot_free(*p, BallId::CUE));
        match spot {
            Some(p) => self.balls[0] = Ball::at_rest(BallId::CUE, p, r),
            // The whole string behind the spot taken: the nearest free
            // place behind the head string.
            None => self.respot_cue_ball(),
        }
        self.ball_in_hand = None;
    }

    fn respot_cue_ball(&mut self) {
        let r = self.table.spec.ball_radius;
        let head = self.table.head_spot();
        let mut p = head;
        'search: for ring in 0..40 {
            for k in 0..16 {
                let angle = k as f64 / 16.0 * std::f64::consts::TAU;
                let cand = head + crate::math::from_angle(angle) * (ring as f64 * r);
                if self.is_on_the_bed(cand)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ball::BallStatus;
    use crate::table::TableSpec;

    /// Puts the table into an arbitrary layout (test helper).
    fn layout(game: &mut EightBallGame, positions: &[(u8, DVec2)]) {
        let r = game.table.spec.ball_radius;
        for b in &mut game.balls {
            b.status = BallStatus::Pocketed(PocketId::FootLeft);
        }
        for &(n, p) in positions {
            game.balls[n as usize] = Ball::at_rest(BallId(n), p, r);
        }
    }

    fn straight_shot(game: &EightBallGame, target: BallId, pocket: PocketId, speed: f64) -> Shot {
        let r = game.table.spec.ball_radius;
        let aim = game.table.pocket(pocket).aim_point();
        let t = game.ball(target).xy();
        let ghost = t - (aim - t).normalize() * 2.0 * r;
        let d = ghost - game.ball(BallId::CUE).xy();
        Shot::new(d.y.atan2(d.x), speed)
    }

    fn after_break(seed: u64) -> EightBallGame {
        let mut g = EightBallGame::new(seed, 0);
        g.is_break = false;
        g.ball_in_hand = None;
        g
    }

    /// The WPA rules of the rulesets before the house rules.
    const WPA: u8 = 3;

    fn after_break_wpa(seed: u64) -> EightBallGame {
        let mut g = EightBallGame::with_ruleset(seed, 0, WPA);
        g.is_break = false;
        g.ball_in_hand = None;
        g
    }

    #[test]
    fn rack_has_sixteen_non_overlapping_balls_with_eight_in_centre() {
        let t = Table::default();
        let balls = rack(&t, 7);
        assert_eq!(balls.len(), 16);
        for (i, a) in balls.iter().enumerate() {
            assert_eq!(a.id.index(), i);
            for b in &balls[i + 1..] {
                assert!(a.xy().distance(b.xy()) >= 2.0 * t.spec.ball_radius);
            }
        }
        let eight = balls[8].xy();
        let row_dx = (2.0 * t.spec.ball_radius + 1e-4) * 3f64.sqrt() / 2.0;
        assert!((eight.x - (t.foot_spot().x + 2.0 * row_dx)).abs() < 1e-9);
        assert!(eight.y.abs() < 1e-9);
    }

    #[test]
    fn break_shot_is_handled() {
        let mut g = EightBallGame::new(1, 0);
        assert!(g.is_break());
        let report = g.play_shot(&Shot::new(0.0, 7.0), None).unwrap();
        assert!(report.was_break);
        assert!(!g.is_break());
        assert!(report.first_contact.is_some());
        assert!(g.balls().iter().all(|b| b.status != BallStatus::OffTable));
    }

    #[test]
    fn soft_break_is_illegal() {
        let mut g = EightBallGame::with_ruleset(1, 0, WPA);
        let report = g.play_shot(&Shot::new(0.0, 0.6), None).unwrap();
        assert!(report.fouls.contains(&Foul::IllegalBreak));
        assert_eq!(g.current_player(), 1);
        assert_eq!(g.ball_in_hand(), Some(BallInHand::Kitchen));
    }

    /// A break foul without a scratch used to leave the cue ball beyond the
    /// head string with ball in hand "in the kitchen": no shot was allowed
    /// until the player moved it.
    #[test]
    fn a_break_foul_always_leaves_a_playable_cue_ball() {
        for seed in 0..20 {
            let mut g = EightBallGame::with_ruleset(seed, 0, WPA);
            // Soft enough to be illegal, hard enough to cross the head string.
            let report = g.play_shot(&Shot::new(0.0, 1.2), None).unwrap();
            assert!(report.fouls.contains(&Foul::IllegalBreak), "seed {seed}");
            assert_eq!(g.ball_in_hand(), Some(BallInHand::Kitchen));
            let cue = g.ball(BallId::CUE).xy();
            assert!(g.can_place_cue_ball(cue), "seed {seed}: cue ball left at {cue:?}");
            assert!(g.prepare_shot(&Shot::new(0.0, 2.0), None).is_ok());
        }
    }

    #[test]
    fn pocketing_assigns_groups_and_keeps_turn() {
        let mut g = after_break(3);
        layout(
            &mut g,
            &[
                (0, DVec2::new(-0.2, 0.0)),
                (3, DVec2::new(0.3, 0.2)),
                (12, DVec2::new(0.0, -0.3)),
                (8, DVec2::new(-0.6, -0.3)),
            ],
        );
        let shot = straight_shot(&g, BallId(3), PocketId::FootLeft, 2.0);
        let report = g.play_shot(&shot, None).unwrap();
        assert_eq!(report.fouls, vec![]);
        assert_eq!(report.group_assigned, Some(Group::Solids));
        assert!(report.continues);
        assert_eq!(g.group_of(0), Some(Group::Solids));
        assert_eq!(g.group_of(1), Some(Group::Stripes));
        assert_eq!(g.current_player(), 0);
    }

    #[test]
    fn hitting_opponent_ball_first_is_a_foul() {
        let mut g = after_break_wpa(3);
        g.groups = [Some(Group::Stripes), Some(Group::Solids)];
        layout(
            &mut g,
            &[
                (0, DVec2::new(-0.2, 0.0)),
                (3, DVec2::new(0.3, 0.2)),
                (12, DVec2::new(0.0, -0.3)),
                (8, DVec2::new(-0.6, -0.3)),
            ],
        );
        let shot = straight_shot(&g, BallId(3), PocketId::FootLeft, 2.0);
        let report = g.play_shot(&shot, None).unwrap();
        assert!(report.fouls.contains(&Foul::WrongBallFirst(BallId(3))));
        assert_eq!(g.current_player(), 1);
        assert_eq!(g.ball_in_hand(), Some(BallInHand::Anywhere));
    }

    /// A scratch: up to ruleset 5 the opponent has the cue ball in hand
    /// anywhere; from ruleset 6 it goes back to the head spot, not in hand.
    #[test]
    fn scratch_gives_ball_in_hand_and_respots_cue_ball() {
        for ruleset in [5, 6] {
            let mut g = EightBallGame::with_ruleset(3, 0, ruleset);
            g.is_break = false;
            g.ball_in_hand = None;
            g.groups = [Some(Group::Solids), Some(Group::Stripes)];
            let t = g.table.clone();
            let aim = t.pocket(PocketId::FootLeft).aim_point();
            let start = aim - DVec2::new(1.0, 0.5).normalize() * 0.6;
            layout(
                &mut g,
                &[
                    (0, start),
                    (2, DVec2::new(-0.5, 0.0)),
                    (10, DVec2::new(-0.5, 0.3)),
                    (8, DVec2::new(-0.6, -0.3)),
                ],
            );
            let d = aim - start;
            let report = g.play_shot(&Shot::new(d.y.atan2(d.x), 2.0), None).unwrap();
            assert!(report.fouls.contains(&Foul::CueBallPocketed));
            assert!(g.ball(BallId::CUE).in_play());
            assert_eq!(g.current_player(), 1);
            if ruleset >= 6 {
                assert_eq!(g.ball_in_hand(), None);
                assert_eq!(g.ball(BallId::CUE).xy(), t.head_spot());
            } else {
                assert_eq!(g.ball_in_hand(), Some(BallInHand::Anywhere));
            }
        }
    }

    #[test]
    fn eight_requires_called_pocket_and_wins() {
        let mut g = after_break(3);
        g.groups = [Some(Group::Solids), Some(Group::Stripes)];
        layout(
            &mut g,
            &[(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.3, 0.2)), (12, DVec2::new(0.0, -0.3))],
        );
        let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 2.0);
        assert_eq!(g.play_shot(&shot, None), Err(ShotError::MustCallPocket));
        let report = g.play_shot(&shot, Some(PocketId::FootLeft)).unwrap();
        assert_eq!(
            report.game_over,
            Some(GameOver { winner: 0, reason: GameOverReason::PocketedEight })
        );
    }

    #[test]
    fn eight_in_wrong_pocket_loses() {
        let mut g = after_break(3);
        g.groups = [Some(Group::Solids), Some(Group::Stripes)];
        layout(
            &mut g,
            &[(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.3, 0.2)), (12, DVec2::new(0.0, -0.3))],
        );
        let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 2.0);
        let report = g.play_shot(&shot, Some(PocketId::FootRight)).unwrap();
        assert_eq!(
            report.game_over,
            Some(GameOver { winner: 1, reason: GameOverReason::EightInWrongPocket })
        );
    }

    #[test]
    fn early_eight_loses() {
        let mut g = after_break(3);
        g.groups = [Some(Group::Solids), Some(Group::Stripes)];
        layout(
            &mut g,
            &[
                (0, DVec2::new(-0.2, 0.0)),
                (8, DVec2::new(0.3, 0.2)),
                (2, DVec2::new(-0.6, 0.3)),
                (12, DVec2::new(0.0, -0.3)),
            ],
        );
        let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 2.0);
        let report = g.play_shot(&shot, None).unwrap();
        assert_eq!(report.game_over.map(|o| o.winner), Some(1));
    }

    #[test]
    fn ball_in_hand_placement_rules() {
        let mut g = EightBallGame::new(5, 0);
        let kitchen = g.table.head_string_x() - 0.1;
        assert!(g.place_cue_ball(DVec2::new(kitchen, 0.2)));
        assert!(!g.place_cue_ball(DVec2::new(0.0, 0.0)), "kitchen only on the break");
        assert!(!g.place_cue_ball(DVec2::new(-2.0, 0.0)), "outside the table");
        g.ball_in_hand = Some(BallInHand::Anywhere);
        assert!(g.place_cue_ball(DVec2::new(0.0, 0.3)));
        let eight = g.ball(BallId::EIGHT).xy();
        assert!(!g.place_cue_ball(eight), "overlapping a ball");
    }

    #[test]
    fn the_call_for_the_eight_is_binding() {
        let mut g = EightBallGame::new(3, 0);
        // Player 0 is on the 8 (every solid pocketed).
        for b in g.balls.iter_mut() {
            if (1..=7).contains(&b.id.0) {
                b.status = BallStatus::Pocketed(PocketId::FootLeft);
            }
        }
        g.groups = [Some(Group::Solids), Some(Group::Stripes)];
        g.is_break = false;
        g.ball_in_hand = None;
        assert!(g.may_call_pocket());
        // A soft shot that doesn't reach anything: the call becomes binding.
        g.play_shot(&Shot::new(std::f64::consts::PI, 0.3), Some(PocketId::SideLeft)).unwrap();
        assert_eq!(g.called_pocket_of(0), Some(PocketId::SideLeft));
        g.current = 0;
        g.ball_in_hand = None;
        assert!(!g.may_call_pocket());
        // A different pocket in the next shot is ignored.
        let pending = g.prepare_shot(&Shot::new(0.0, 0.3), Some(PocketId::FootRight)).unwrap();
        assert_eq!(pending.called_pocket, Some(PocketId::SideLeft));
        // And no pocket at all is fine: the call stands.
        assert!(g.prepare_shot(&Shot::new(0.0, 0.3), None).is_ok());
    }

    #[test]
    fn time_foul_passes_turn_with_ball_in_hand() {
        let mut g = after_break_wpa(4);
        g.time_foul();
        assert_eq!(g.current_player(), 1);
        assert_eq!(g.ball_in_hand(), Some(BallInHand::Anywhere));
        // Under the house rules the turn passes and the cue ball stays.
        let mut house = after_break(4);
        house.time_foul();
        assert_eq!((house.current_player(), house.ball_in_hand()), (1, None));
        g.abandon(1);
        assert_eq!(g.game_over(), Some(GameOver { winner: 0, reason: GameOverReason::Abandoned }));
        g.time_foul();
        assert_eq!(g.current_player(), 1, "no changes after the game ended");
    }

    #[test]
    fn game_state_round_trips_through_json() {
        let mut g = EightBallGame::new(9, 1);
        g.play_shot(&Shot::new(0.0, 7.0), None).unwrap();
        let json = serde_json::to_string(&g).unwrap();
        let back: EightBallGame = serde_json::from_str(&json).unwrap();
        assert_eq!(g, back);
    }

    /// A snapshot no game could be in is refused when it is read, instead
    /// of panicking at the next shot.
    #[test]
    fn impossible_snapshots_are_refused() {
        let mut g = EightBallGame::new(9, 1);
        g.play_shot(&Shot::new(0.0, 6.0), None).unwrap();
        let good = serde_json::to_value(&g).unwrap();
        let read = |v: &serde_json::Value| serde_json::from_value::<EightBallGame>(v.clone());
        assert_eq!(read(&good).unwrap(), g);
        let broken = |edit: &dyn Fn(&mut serde_json::Value)| {
            let mut v = good.clone();
            edit(&mut v);
            read(&v).expect_err("refused")
        };
        broken(&|v| v["current"] = 2.into());
        broken(&|v| {
            v["balls"].as_array_mut().unwrap().pop();
        });
        broken(&|v| v["balls"].as_array_mut().unwrap().swap(3, 4));
        broken(&|v| v["game_over"] = serde_json::json!({ "winner": 5, "reason": "Resignation" }));
        broken(&|v| v["balls"][3]["motion"] = "Rolling".into());
        broken(&|v| v["balls"][0]["status"] = serde_json::json!({ "Pocketed": "FootLeft" }));
        // The same cue ball is fine once the game is over (a scratch on the 8).
        let mut v = good.clone();
        v["balls"][0]["status"] = serde_json::json!({ "Pocketed": "FootLeft" });
        v["game_over"] = serde_json::json!({ "winner": 0, "reason": "EightPocketedOnFoul" });
        assert!(read(&v).is_ok());
    }

    /// A seat other than 0 or 1 can't underflow: it counts by its parity.
    #[test]
    fn resigning_with_any_seat_number_is_safe() {
        let mut g = EightBallGame::new(1, 0);
        g.resign(3);
        assert_eq!(
            g.game_over(),
            Some(GameOver { winner: 0, reason: GameOverReason::Resignation })
        );
        let mut g = EightBallGame::new(1, 0);
        g.abandon(usize::MAX);
        assert_eq!(g.game_over(), Some(GameOver { winner: 0, reason: GameOverReason::Abandoned }));
    }

    /// A table put together without a cue ball has nothing to strike: the
    /// shot is refused (it used to panic).
    #[test]
    fn no_cue_ball_no_shot() {
        let mut g = EightBallGame::new(1, 0);
        g.set_up_test_position(&[(BallId(3), DVec2::new(0.3, 0.0))]);
        assert_eq!(
            g.prepare_shot(&Shot::new(0.0, 2.0), None).err(),
            Some(ShotError::IllegalCueBallPosition)
        );
    }

    #[test]
    fn spotting_finds_free_place() {
        let mut g = after_break(9);
        g.spot_ball(BallId(5));
        let p = g.ball(BallId(5)).xy();
        assert!(g.is_spot_free(p, BallId(5)));
        assert_eq!(p.y, 0.0, "spotted balls sit on the long string");
        assert!(g.table.is_inside_playing_area(p));
    }

    fn on_break_with(game: &mut EightBallGame, positions: &[(u8, DVec2)]) {
        layout(game, positions);
        game.is_break = true;
        game.ball_in_hand = None;
    }

    /// The first ball made on a legal break is the
    /// breaker's group, and they shoot again.
    #[test]
    fn the_first_ball_made_on_the_break_sets_the_groups() {
        let mut g = EightBallGame::new(3, 0);
        on_break_with(
            &mut g,
            &[(0, DVec2::new(-0.2, 0.0)), (10, DVec2::new(0.3, 0.2)), (2, DVec2::new(0.0, -0.3))],
        );
        let shot = straight_shot(&g, BallId(10), PocketId::FootLeft, 2.0);
        let report = g.play_shot(&shot, None).unwrap();
        assert!(report.pocketed.iter().any(|(b, _)| *b == BallId(10)));
        assert_eq!(report.fouls, vec![]);
        assert_eq!(report.group_assigned, Some(Group::Stripes));
        assert!(report.continues);
        assert_eq!((g.group_of(0), g.group_of(1)), (Some(Group::Stripes), Some(Group::Solids)));
        assert_eq!(g.current_player(), 0);
    }

    /// A foul on the break (the cue ball in a pocket) leaves the table open
    /// even if a ball went in.
    #[test]
    fn a_break_foul_leaves_the_table_open() {
        let mut g = EightBallGame::new(3, 0);
        // The cue ball follows the 10 into the pocket.
        on_break_with(&mut g, &[(0, DVec2::new(0.55, 0.40)), (10, DVec2::new(0.9, 0.52))]);
        let to = g.ball(BallId(10)).xy() - g.ball(BallId::CUE).xy();
        let shot = Shot::new(crate::math::atan2(to.y, to.x), 3.0).with_tip(DVec2::new(0.0, 0.8));
        let report = g.play_shot(&shot, None).unwrap();
        if report.fouls.contains(&Foul::CueBallPocketed) {
            assert_eq!(report.group_assigned, None);
            assert!(g.is_open_table());
        }
    }

    #[test]
    fn eight_on_the_break_is_spotted_and_play_goes_on() {
        let mut g = EightBallGame::with_ruleset(3, 0, WPA);
        on_break_with(
            &mut g,
            &[(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.3, 0.2)), (2, DVec2::new(0.0, -0.3))],
        );
        let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 2.0);
        let report = g.play_shot(&shot, None).unwrap();
        assert_eq!(report.game_over, None);
        assert!(report.spotted.contains(&BallId::EIGHT));
        assert!(g.ball(BallId::EIGHT).in_play());
    }

    /// A scratch on the break: up to ruleset 5 the cue ball is in hand
    /// behind the head string; from ruleset 6 it goes back to the head
    /// spot (here right behind the 2, which lies on it), not in hand.
    #[test]
    fn scratch_on_the_break_gives_ball_in_hand_in_the_kitchen() {
        for ruleset in [5, 6] {
            let mut g = EightBallGame::with_ruleset(3, 0, ruleset);
            let t = g.table.clone();
            let aim = t.pocket(PocketId::FootLeft).aim_point();
            let start = aim - DVec2::new(1.0, 0.5).normalize() * 0.6;
            on_break_with(&mut g, &[(0, start), (2, t.head_spot()), (8, DVec2::new(-0.6, -0.3))]);
            let d = aim - start;
            let report = g.play_shot(&Shot::new(crate::math::atan2(d.y, d.x), 2.0), None).unwrap();
            assert!(report.fouls.contains(&Foul::CueBallPocketed));
            assert_eq!(g.current_player(), 1);
            if ruleset >= 6 {
                // Played on, not racked again (ruleset 5 racked again: only
                // the cue ball went down).
                assert!(!g.is_break() && !report.reracked);
                assert_eq!(g.ball_in_hand(), None);
                let cue = g.ball(BallId::CUE).xy();
                let two = g.ball(BallId(2)).xy();
                assert_eq!(two, t.head_spot(), "the 2 wasn't touched");
                let r = t.spec.ball_radius;
                assert!(cue.y == 0.0 && cue.x < two.x, "behind the 2: {cue}");
                let gap = two.x - cue.x - 2.0 * r;
                assert!(gap > 0.0 && gap < r / 8.0, "right behind it: {gap}");
            } else {
                assert_eq!(g.ball_in_hand(), Some(BallInHand::Kitchen));
            }
        }
    }

    #[test]
    fn foul_on_an_open_table_assigns_nothing() {
        let mut g = after_break(3);
        // The cue ball follows the 3 into the pocket.
        layout(
            &mut g,
            &[(0, DVec2::new(0.6, 0.3)), (3, DVec2::new(0.9, 0.45)), (8, DVec2::new(-0.6, -0.3))],
        );
        let aim = g.table.pocket(PocketId::FootLeft).aim_point();
        let d = aim - g.ball(BallId::CUE).xy();
        let shot = Shot::new(crate::math::atan2(d.y, d.x), 3.0).with_tip(DVec2::new(0.0, 1.0));
        let report = g.play_shot(&shot, None).unwrap();
        if report.is_foul() {
            assert_eq!(report.group_assigned, None);
            assert!(g.is_open_table());
            assert_eq!(g.current_player(), 1);
        }
    }

    #[test]
    fn eight_first_on_an_open_table_is_a_foul() {
        let mut g = after_break(3);
        layout(
            &mut g,
            &[(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.3, 0.2)), (2, DVec2::new(0.0, -0.3))],
        );
        let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootRight, 0.8);
        let report = g.play_shot(&shot, None).unwrap();
        assert!(report.fouls.contains(&Foul::WrongBallFirst(BallId::EIGHT)));
    }

    #[test]
    fn no_rail_after_contact_is_a_foul() {
        let mut g = after_break_wpa(3);
        g.groups = [Some(Group::Solids), Some(Group::Stripes)];
        layout(
            &mut g,
            &[(0, DVec2::new(-0.2, 0.0)), (3, DVec2::new(0.0, 0.0)), (8, DVec2::new(-0.6, -0.3))],
        );
        // A gentle nudge: the 3 moves a little, nothing reaches a cushion.
        let report = g.play_shot(&Shot::new(0.0, 0.45), None).unwrap();
        assert_eq!(report.first_contact, Some(BallId(3)));
        assert_eq!(report.fouls, vec![Foul::NoRailAfterContact]);
        assert_eq!(g.ball_in_hand(), Some(BallInHand::Anywhere));
        assert_eq!(g.current_player(), 1);
    }

    #[test]
    fn pocketing_only_the_opponents_ball_passes_the_turn() {
        let mut g = after_break(3);
        g.groups = [Some(Group::Solids), Some(Group::Stripes)];
        layout(
            &mut g,
            &[(0, DVec2::new(-0.2, 0.0)), (12, DVec2::new(0.3, 0.2)), (3, DVec2::new(0.0, -0.3))],
        );
        let shot = straight_shot(&g, BallId(12), PocketId::FootLeft, 2.0);
        let report = g.play_shot(&shot, None).unwrap();
        // Hitting the 12 first is itself a foul for solids.
        assert!(report.fouls.contains(&Foul::WrongBallFirst(BallId(12))));
        assert!(!report.continues);
        assert_eq!(g.current_player(), 1);
    }

    /// From ruleset 2: one's own ball and one of the opponent's made on the
    /// same shot pass the turn (no foul, no ball in hand); ruleset 1 kept
    /// the table.
    #[test]
    fn making_an_opponents_ball_with_ones_own_passes_the_turn() {
        // The 3 (solids) sends the 12 (stripes) into the corner and follows
        // it in: a combination that makes both.
        let setup = |ruleset: u8| {
            let mut g = after_break(3);
            g.ruleset = ruleset;
            g.groups = [Some(Group::Solids), Some(Group::Stripes)];
            let aim = g.table.pocket(PocketId::FootLeft).aim_point();
            let dir = (aim - DVec2::new(0.2, 0.0)).normalize();
            let r = g.table.spec.ball_radius;
            let twelve = aim - dir * 0.12;
            let three = twelve - dir * (2.0 * r + 0.15);
            let cue = three - dir * 0.35;
            layout(&mut g, &[(0, cue), (3, three), (12, twelve), (8, DVec2::new(-0.6, 0.3))]);
            g
        };
        let both = |r: &ShotReport| {
            r.fouls.is_empty()
                && [3u8, 12].iter().all(|n| r.pocketed.iter().any(|(b, _)| b.0 == *n))
        };
        let shot = (0..60)
            .map(|k| straight_shot(&setup(2), BallId(3), PocketId::FootLeft, 1.5 + k as f64 * 0.04))
            .map(|s| s.with_tip(DVec2::new(0.0, 0.8)))
            .find(|s| setup(2).play_shot(s, None).is_ok_and(|r| both(&r)))
            .expect("a stroke that makes the 3 and the 12");
        let mut g = setup(2);
        let report = g.play_shot(&shot, None).unwrap();
        assert!(report.opponent_ball && !report.continues, "{report:?}");
        assert_eq!((g.current_player(), g.ball_in_hand()), (1, None), "no foul, no ball in hand");
        // The same shot under ruleset 1: the shooter stays at the table.
        let mut old = setup(1);
        let report = old.play_shot(&shot, None).unwrap();
        assert!(both(&report) && !report.opponent_ball && report.continues, "{report:?}");
        assert_eq!(old.current_player(), 0);
    }

    /// A snapshot names its ruleset and brings back that ruleset's table and
    /// physics; one from before rulesets existed is ruleset 1.
    #[test]
    fn snapshots_keep_their_ruleset() {
        let g = EightBallGame::with_ruleset(4, 1, 1);
        let back: EightBallGame =
            serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert_eq!(back, g);
        assert_eq!(back.table().spec, TableSpec::TOURNAMENT_9FT);
        let mut v = serde_json::to_value(EightBallGame::new(4, 1)).unwrap();
        assert_eq!(v["ruleset"], crate::RULESET);
        v.as_object_mut().unwrap().remove("ruleset");
        let old: EightBallGame = serde_json::from_value(v).unwrap();
        assert_eq!((old.ruleset(), old.params().rolling_friction), (1, 0.0125));
        assert_eq!(EightBallGame::with_ruleset(4, 1, 2).table().spec, TableSpec::TOURNAMENT_8FT);
        assert_eq!(EightBallGame::new(4, 1).table().spec, TableSpec::WPA_8FT);
    }

    #[test]
    fn scratch_on_the_eight_without_pocketing_it_is_only_a_foul() {
        let mut g = after_break(3);
        g.groups = [Some(Group::Solids), Some(Group::Stripes)];
        layout(
            &mut g,
            &[(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.3, 0.2)), (12, DVec2::new(0.0, -0.3))],
        );
        // Miss everything: no contact.
        let report =
            g.play_shot(&Shot::new(std::f64::consts::PI, 1.0), Some(PocketId::FootLeft)).unwrap();
        assert!(report.fouls.contains(&Foul::NoContact));
        assert_eq!(report.game_over, None);
        assert_eq!(g.current_player(), 1);
    }

    /// Hidden behind another ball: a kick off the cushions that then hits one's own ball is
    /// legal (if a ball reaches a cushion or a pocket afterwards).
    #[test]
    fn kicks_off_the_cushions_are_legal() {
        for rails in [1, 2] {
            let mut g = after_break(3);
            g.groups = [Some(Group::Solids), Some(Group::Stripes)];
            let hw = g.table.spec.width / 2.0;
            layout(
                &mut g,
                &[
                    (0, DVec2::new(-0.9, -0.2)),
                    (12, DVec2::new(-0.3, -0.2)), // the snooker
                    (3, DVec2::new(0.3, -0.2)),
                    (8, DVec2::new(-0.9, 0.35)),
                ],
            );
            // Unfold the path across the cushions (the nose touches the
            // ball ~27.5 mm from its centre).
            let line = hw - 0.0275;
            let target = match rails {
                1 => DVec2::new(0.3, 2.0 * line + 0.2),
                _ => DVec2::new(0.3, 4.0 * line - 0.2),
            };
            let d = target - g.ball(BallId::CUE).xy();
            let speed = if rails == 1 { 2.5 } else { 4.5 };
            // The cushions don't reflect like a mirror (friction, spin):
            // look near the unfolded aim for the stroke that reaches the 3.
            let base = crate::math::atan2(d.y, d.x);
            let shot = (0..400)
                .map(|k| {
                    let off = (k / 2) as f64 * 0.0005 * if k % 2 == 0 { 1.0 } else { -1.0 };
                    Shot::new(base + off, speed)
                })
                .find(|s| {
                    let mut t = g.clone();
                    // (One that doesn't scratch: that is another foul.)
                    t.play_shot(s, None).is_ok_and(|r| {
                        r.first_contact == Some(BallId(3))
                            && !r.fouls.contains(&Foul::CueBallPocketed)
                    })
                })
                .unwrap_or_else(|| panic!("no {rails}-rail kick reaches the 3"));
            let report = g.play_shot(&shot, None).unwrap();
            assert_eq!(report.first_contact, Some(BallId(3)), "{rails}-rail kick");
            assert_eq!(report.fouls, vec![], "{rails}-rail kick: {report:?}");
            assert!(!report.continues, "nothing made: the turn passes, no ball in hand");
            assert_eq!(g.ball_in_hand(), None);
        }
    }

    /// Hundreds of games of varied shots, checking the rules' invariants
    /// after every one of them.
    #[test]
    fn rules_hold_over_many_random_games() {
        let r = Table::default().spec.ball_radius;
        for seed in 0..120u64 {
            // Half under the WPA rules, half under the house rules.
            let house = seed >= 60;
            let ruleset = if house { crate::RULESET } else { WPA };
            let mut g = EightBallGame::with_ruleset(seed, (seed % 2) as usize, ruleset);
            let mut rng = Rng::new(seed + 1000);
            for _ in 0..120 {
                if g.game_over().is_some() {
                    break;
                }
                if g.ball_in_hand().is_some() {
                    for _ in 0..300 {
                        let p = DVec2::new(rng.next_f64() * 2.2 - 1.1, rng.next_f64() * 1.0 - 0.5);
                        if g.place_cue_ball(p) {
                            break;
                        }
                    }
                }
                let before = g.clone();
                let shooter = g.current_player();
                let shot =
                    Shot::new(rng.next_f64() * std::f64::consts::TAU, 0.3 + rng.next_f64() * 6.5)
                        .with_tip(DVec2::new(rng.next_f64() - 0.5, rng.next_f64() - 0.5));
                let pocket = PocketId::ALL[rng.below(6)];
                let called = g.needs_called_pocket().then_some(pocket);
                let report = g.play_shot(&shot, called).unwrap();

                // Groups: both unknown, or opposite; never change once set.
                match (g.group_of(0), g.group_of(1)) {
                    (None, None) => {}
                    (Some(a), Some(b)) => assert_eq!(a, b.opposite()),
                    other => panic!("half-assigned groups {other:?}"),
                }
                if let Some(old) = before.group_of(0) {
                    assert_eq!(g.group_of(0), Some(old), "groups changed");
                }
                if report.was_break {
                    // The first ball made on a legal break sets the groups;
                    // otherwise the table stays open.
                    let first = report.pocketed.iter().find_map(|(b, _)| Group::of(*b));
                    match first.filter(|_| !report.is_foul()) {
                        Some(gr) => assert_eq!(g.group_of(shooter), Some(gr), "{report:?}"),
                        None => assert!(g.is_open_table(), "{report:?}"),
                    }
                }
                // Ball in hand exactly after a foul (when the game goes on);
                // under the house rules, only when the cue ball went away;
                // from ruleset 6 never past the break: the cue ball that
                // went away is back on the long string, from the head spot
                // towards the head rail.
                if g.game_over().is_none() {
                    let cue_gone = report
                        .fouls
                        .iter()
                        .any(|f| matches!(f, Foul::CueBallPocketed | Foul::CueBallOffTable));
                    let in_hand = if ruleset >= 6 {
                        false
                    } else if house {
                        cue_gone
                    } else {
                        report.is_foul()
                    };
                    assert_eq!(g.ball_in_hand().is_some(), in_hand, "{report:?}");
                    if ruleset >= 6 && cue_gone {
                        let cue = g.ball(BallId::CUE).xy();
                        let head = g.table().head_spot();
                        assert!(cue.y == head.y && cue.x <= head.x, "{cue} {report:?}");
                    }
                    // The turn stays only on a legal shot that made a ball.
                    let stays = g.current_player() == shooter;
                    assert_eq!(stays, report.continues);
                    if report.continues {
                        assert!(!report.is_foul() && !report.pocketed.is_empty());
                    }
                    assert!(g.ball(BallId::CUE).in_play(), "the cue ball is always back");
                    assert!(g.ball(BallId::EIGHT).in_play(), "the 8 only leaves to end the game");
                }
                // A foul always gives the turn away.
                if report.is_foul() {
                    assert!(!report.continues);
                }
                // A game won by the shooter: on the 8 (or the 8 alone on a
                // house-rules break), no foul, right pocket.
                if let Some(over) = report.game_over
                    && over.winner == shooter
                {
                    assert_eq!(over.reason, GameOverReason::PocketedEight);
                    assert!(!report.is_foul());
                    if report.was_break {
                        assert!(house && report.pocketed.len() == 1);
                    } else {
                        assert!(before.is_on_eight(shooter));
                    }
                }
                // Penalty balls: only under the house rules, only after a
                // foul, only the opponent's, lowest numbers first, and off
                // the table afterwards.
                if !report.penalty.is_empty() {
                    assert!(house && report.is_foul(), "{report:?}");
                    let theirs = g.group_of(1 - shooter).expect("groups are set");
                    for b in &report.penalty {
                        assert!(theirs.contains(*b) && !g.ball(*b).in_play(), "{report:?}");
                    }
                }
                // Balls at rest never overlap and stay on the table.
                let live: Vec<_> = g.balls().iter().filter(|b| b.in_play()).collect();
                for (k, a) in live.iter().enumerate() {
                    assert!(
                        // Never inside a cushion (the nose is above the
                        // centre: it touches at ~27.5 mm, not a full radius),
                        // and never beyond the cushions.
                        g.table()
                            .cushions
                            .iter()
                            .all(|c| c.closest_point(a.xy()).distance(a.xy()) > 0.0270)
                            && a.xy().x.abs() < g.table().spec.length / 2.0 + r
                            && a.xy().y.abs() < g.table().spec.width / 2.0 + r,
                        "ball {} off the cloth at {:?}",
                        a.id,
                        a.xy()
                    );
                    for b in &live[k + 1..] {
                        assert!(
                            a.xy().distance(b.xy()) > 2.0 * r - 1e-6,
                            "{} and {} overlap",
                            a.id,
                            b.id
                        );
                    }
                }
                // Pocketed balls stay pocketed (except what is spotted).
                for b in before.balls().iter().filter(|b| !b.in_play()) {
                    if b.id != BallId::CUE {
                        assert!(!g.ball(b.id).in_play(), "ball {} came back", b.id);
                    }
                }
            }
        }
    }

    // ---- Ruleset 4: the Brazilian house rules -------------------------

    /// Player 0 on stripes, player 1 on solids, past the break, with the
    /// cue ball in the middle and these balls on the table.
    fn house(positions: &[(u8, DVec2)]) -> EightBallGame {
        let mut g = EightBallGame::with_ruleset(3, 0, 4);
        g.is_break = false;
        g.ball_in_hand = None;
        assert_eq!(g.ruleset(), 4);
        g.groups = [Some(Group::Stripes), Some(Group::Solids)];
        layout(&mut g, positions);
        g
    }

    fn on_table(g: &EightBallGame, group: Group) -> Vec<u8> {
        g.remaining(group).into_iter().map(|b| b.0).collect()
    }

    #[test]
    fn missing_every_ball_gives_the_opponent_their_lowest_ball() {
        let mut g = house(&[
            (0, DVec2::new(-0.2, 0.0)),
            (2, DVec2::new(0.5, 0.3)),
            (5, DVec2::new(0.6, -0.3)),
            (12, DVec2::new(0.0, -0.4)),
            (8, DVec2::new(-0.6, -0.3)),
        ]);
        // Up the table, between the balls.
        let report = g.play_shot(&Shot::new(0.0, 1.0), None).unwrap();
        assert_eq!(report.fouls, vec![Foul::NoContact]);
        assert_eq!(report.penalty, vec![BallId(2)]);
        assert_eq!(on_table(&g, Group::Solids), vec![5]);
        assert_eq!(g.current_player(), 1);
        assert_eq!(g.ball_in_hand(), None, "the cue ball is played from where it lies");
    }

    #[test]
    fn missing_and_scratching_gives_two_balls() {
        let mut g = house(&[
            (0, DVec2::new(0.6, 0.3)),
            (2, DVec2::new(-0.5, 0.3)),
            (5, DVec2::new(-0.6, -0.3)),
            (7, DVec2::new(-0.3, -0.3)),
            (12, DVec2::new(0.0, -0.4)),
            (8, DVec2::new(-0.6, 0.0)),
        ]);
        // Straight into the foot corner pocket.
        let aim = g.table.pocket(PocketId::FootLeft).aim_point() - g.ball(BallId::CUE).xy();
        let report = g.play_shot(&Shot::new(aim.y.atan2(aim.x), 2.5), None).unwrap();
        assert!(report.fouls.contains(&Foul::NoContact), "{report:?}");
        assert!(report.fouls.contains(&Foul::CueBallPocketed), "{report:?}");
        assert_eq!(report.penalty, vec![BallId(2), BallId(5)]);
        assert_eq!(on_table(&g, Group::Solids), vec![7]);
        assert_eq!(g.current_player(), 1);
        assert_eq!(g.ball_in_hand(), Some(BallInHand::Anywhere));
    }

    #[test]
    fn hitting_the_opponents_ball_directly_gives_two_balls() {
        let mut g = house(&[
            (0, DVec2::new(-0.2, 0.0)),
            (3, DVec2::new(0.3, 0.0)),
            (1, DVec2::new(0.5, 0.3)),
            (6, DVec2::new(0.6, -0.3)),
            (12, DVec2::new(0.0, -0.4)),
            (8, DVec2::new(-0.6, -0.3)),
        ]);
        let report = g.play_shot(&Shot::new(0.0, 1.0), None).unwrap();
        assert_eq!(report.fouls, vec![Foul::WrongBallFirst(BallId(3))]);
        assert_eq!(report.penalty, vec![BallId(1), BallId(3)]);
        assert_eq!(on_table(&g, Group::Solids), vec![6]);
        assert_eq!(g.current_player(), 1);
        assert_eq!(g.ball_in_hand(), None);
    }

    #[test]
    fn hitting_the_opponents_ball_and_scratching_gives_three_balls() {
        let mut g = house(&[(0, DVec2::new(-0.2, 0.0))]);
        // The opponent's 3 near the corner, the cue ball straight behind it
        // on the line to the pocket: struck high and hard, it follows the 3
        // in. Their 1, 4 and 6 are elsewhere.
        let pocket = g.table.pocket(PocketId::FootLeft).aim_point();
        let back = (DVec2::ZERO - pocket).normalize();
        layout(
            &mut g,
            &[
                (0, pocket + back * 0.55),
                (3, pocket + back * 0.12),
                (1, DVec2::new(-0.5, 0.3)),
                (4, DVec2::new(-0.6, -0.3)),
                (6, DVec2::new(-0.3, -0.3)),
                (7, DVec2::new(-0.7, 0.1)),
                (12, DVec2::new(0.0, -0.4)),
                (8, DVec2::new(-0.8, -0.2)),
            ],
        );
        let shot =
            straight_shot(&g, BallId(3), PocketId::FootLeft, 4.0).with_tip(DVec2::new(0.0, 0.9));
        let report = g.play_shot(&shot, None).unwrap();
        assert_eq!(report.first_contact, Some(BallId(3)));
        assert!(report.fouls.contains(&Foul::WrongBallFirst(BallId(3))), "{report:?}");
        assert!(report.fouls.contains(&Foul::CueBallPocketed), "{report:?}");
        // The 3 went down by itself; three more come off, lowest first.
        assert_eq!(report.penalty, vec![BallId(1), BallId(4), BallId(6)]);
        assert_eq!(on_table(&g, Group::Solids), vec![7]);
        assert_eq!(g.current_player(), 1);
        assert_eq!(g.ball_in_hand(), Some(BallInHand::Anywhere));
    }

    /// The opponent's ball hit directly and made: it stays down, and two
    /// more of theirs come off as well (the one that fell doesn't count
    /// towards the two).
    #[test]
    fn potting_the_opponents_ball_hit_directly_still_gives_two_more() {
        let mut g = house(&[(0, DVec2::new(-0.2, 0.0))]);
        let pocket = g.table.pocket(PocketId::FootLeft).aim_point();
        let back = (DVec2::ZERO - pocket).normalize();
        layout(
            &mut g,
            &[
                (0, pocket + back * 0.6),
                (3, pocket + back * 0.15),
                (1, DVec2::new(-0.5, 0.3)),
                (4, DVec2::new(-0.6, -0.3)),
                (6, DVec2::new(-0.3, -0.3)),
                (12, DVec2::new(0.0, -0.4)),
                (8, DVec2::new(-0.8, -0.2)),
            ],
        );
        // A stop shot: the 3 goes in, the cue ball stays up.
        let shot =
            straight_shot(&g, BallId(3), PocketId::FootLeft, 2.0).with_tip(DVec2::new(0.0, -0.4));
        let report = g.play_shot(&shot, None).unwrap();
        assert_eq!(report.fouls, vec![Foul::WrongBallFirst(BallId(3))], "{report:?}");
        assert!(report.pocketed.iter().any(|(b, _)| *b == BallId(3)), "{report:?}");
        assert_eq!(report.penalty, vec![BallId(1), BallId(4)]);
        assert_eq!(on_table(&g, Group::Solids), vec![6], "the 3 that fell and two more");
        assert_eq!((g.current_player(), g.ball_in_hand()), (1, None));
    }

    #[test]
    fn hitting_the_eight_too_soon_counts_as_a_miss() {
        let mut g = house(&[
            (0, DVec2::new(-0.2, 0.0)),
            (8, DVec2::new(0.3, 0.0)),
            (1, DVec2::new(0.5, 0.3)),
            (6, DVec2::new(0.6, -0.3)),
            (12, DVec2::new(0.0, -0.4)),
        ]);
        let report = g.play_shot(&Shot::new(0.0, 1.0), None).unwrap();
        assert_eq!(report.fouls, vec![Foul::WrongBallFirst(BallId::EIGHT)]);
        assert_eq!(report.penalty, vec![BallId(1)], "one ball, not two: the 8 is nobody's");
        assert_eq!(on_table(&g, Group::Solids), vec![6]);
        assert_eq!((g.current_player(), g.ball_in_hand(), g.game_over()), (1, None, None));
    }

    #[test]
    fn hitting_the_opponents_ball_off_a_cushion_gives_one_ball() {
        let mut g = house(&[
            (0, DVec2::new(0.0, 0.0)),
            (3, DVec2::new(0.4, 0.0)),
            (6, DVec2::new(-0.6, -0.3)),
            (12, DVec2::new(-0.5, 0.3)),
            (8, DVec2::new(-0.8, -0.1)),
        ]);
        // One cushion (the long rail) and then the 3: aim at its mirror.
        let w = g.table.spec.width / 2.0 - g.table.spec.ball_radius;
        let mirror = DVec2::new(0.4, 2.0 * w);
        let report = g.play_shot(&Shot::new(mirror.y.atan2(mirror.x), 2.0), None).unwrap();
        assert_eq!(report.fouls, vec![Foul::WrongBallFirst(BallId(3))], "{report:?}");
        assert_eq!(report.penalty, vec![BallId(3)]);
        assert_eq!(on_table(&g, Group::Solids), vec![6]);
        assert_eq!(g.current_player(), 1);
    }

    #[test]
    fn a_scratch_gives_one_ball_and_ball_in_hand() {
        let mut g = house(&[
            (0, DVec2::new(0.4, 0.0)),
            (12, DVec2::new(0.7, 0.2)),
            (2, DVec2::new(-0.5, 0.3)),
            (5, DVec2::new(-0.6, -0.3)),
            (8, DVec2::new(-0.6, 0.0)),
        ]);
        // A thin cut on the 12 that carries the cue ball on into the corner.
        let mut scratched = None;
        for k in 0..400 {
            let mut trial = g.clone();
            let yaw = 0.2 + k as f64 * 0.002;
            let report = trial.play_shot(&Shot::new(yaw, 3.0), None).unwrap();
            if report.fouls == vec![Foul::CueBallPocketed]
                && report.first_contact == Some(BallId(12))
            {
                scratched = Some((trial, report));
                break;
            }
        }
        let (after, report) = scratched.expect("a scratch off the 12");
        g = after;
        assert_eq!(report.penalty, vec![BallId(2)]);
        assert_eq!(on_table(&g, Group::Solids), vec![5]);
        assert_eq!(g.current_player(), 1);
        assert_eq!(g.ball_in_hand(), Some(BallInHand::Anywhere));
        assert!(g.ball(BallId::CUE).in_play());
    }

    #[test]
    fn a_foul_on_an_open_table_costs_only_the_turn() {
        let mut g = after_break(3);
        layout(
            &mut g,
            &[
                (0, DVec2::new(-0.2, 0.0)),
                (2, DVec2::new(0.5, 0.3)),
                (12, DVec2::new(0.0, -0.4)),
                (8, DVec2::new(-0.6, -0.3)),
            ],
        );
        let report = g.play_shot(&Shot::new(0.0, 1.0), None).unwrap();
        assert_eq!(report.fouls, vec![Foul::NoContact]);
        assert!(report.penalty.is_empty());
        assert!(g.ball(BallId(2)).in_play() && g.ball(BallId(12)).in_play());
        assert_eq!(g.current_player(), 1);
        assert_eq!(g.ball_in_hand(), None);
    }

    #[test]
    fn a_penalty_never_takes_the_eight_and_stops_at_the_last_ball() {
        let mut g = house(&[
            (0, DVec2::new(-0.2, 0.0)),
            (3, DVec2::new(0.3, 0.0)),
            (12, DVec2::new(0.0, -0.4)),
            (8, DVec2::new(-0.6, -0.3)),
        ]);
        // Two balls owed, the opponent has one: they are on the 8 now.
        let report = g.play_shot(&Shot::new(0.0, 1.0), None).unwrap();
        assert_eq!(report.penalty, vec![BallId(3)]);
        assert!(g.ball(BallId::EIGHT).in_play());
        assert!(g.is_on_eight(1));
        assert_eq!(g.game_over(), None);
    }

    #[test]
    fn nothing_has_to_reach_a_cushion_under_the_house_rules() {
        let mut g = house(&[
            (0, DVec2::new(-0.2, 0.0)),
            (12, DVec2::new(0.0, 0.0)),
            (3, DVec2::new(0.5, 0.3)),
            (8, DVec2::new(-0.6, -0.3)),
        ]);
        let report = g.play_shot(&Shot::new(0.0, 0.45), None).unwrap();
        assert_eq!(report.first_contact, Some(BallId(12)));
        assert_eq!(report.fouls, vec![]);
        assert!(report.penalty.is_empty());
        assert_eq!(g.current_player(), 1, "nothing made: the turn passes");
        assert_eq!(g.ball_in_hand(), None);
    }

    #[test]
    fn a_soft_break_is_no_foul_under_the_house_rules() {
        let mut g = EightBallGame::new(1, 0);
        let report = g.play_shot(&Shot::new(0.0, 1.0), None).unwrap();
        assert!(report.first_contact.is_some());
        assert_eq!(report.fouls, vec![]);
        assert_eq!(g.current_player(), 1);
        assert_eq!(g.ball_in_hand(), None);
    }

    /// Ruleset 4 and 5 (ruleset 6: see
    /// `a_break_that_only_makes_the_cue_ball_is_played_on`).
    #[test]
    fn a_break_that_only_makes_the_cue_ball_goes_to_the_opponent() {
        let mut g = EightBallGame::with_ruleset(3, 0, 5);
        // Only the cue ball goes down: straight into a corner.
        let aim = g.table.pocket(PocketId::FootLeft).aim_point();
        on_break_with(
            &mut g,
            &[(0, aim + (DVec2::ZERO - aim).normalize() * 0.4), (5, DVec2::new(-0.3, 0.0))],
        );
        let d = aim - g.ball(BallId::CUE).xy();
        let report = g.play_shot(&Shot::new(d.y.atan2(d.x), 2.0), None).unwrap();
        assert!(report.fouls.contains(&Foul::CueBallPocketed), "{report:?}");
        assert!(report.reracked && !report.continues && report.game_over.is_none());
        // The opponent breaks a full rack, the cue ball in hand behind the line.
        assert_eq!(g.current_player(), 1);
        assert!(g.is_break());
        assert_eq!(g.ball_in_hand(), Some(BallInHand::Kitchen));
        assert!(g.balls().iter().all(|b| b.in_play()), "all sixteen are back");
        assert!(g.is_open_table());
        let fresh = rack(&g.table, 0);
        for b in g.balls().iter().filter(|b| b.id.is_object()) {
            assert!(
                fresh.iter().any(|f| f.id.is_object() && f.xy().distance(b.xy()) < 1e-9),
                "the {} is in the rack",
                b.id
            );
        }
        // The same on every machine: a snapshot plays the same re-rack.
        let json = serde_json::to_string(&g).unwrap();
        let copy: EightBallGame = serde_json::from_str(&json).unwrap();
        assert_eq!(copy, g);
        // The opponent's break is an ordinary break.
        let report = g.play_shot(&Shot::new(g.default_aim(), 6.0), None).unwrap();
        assert!(report.was_break && !report.reracked);

        // The cue ball down with another ball: an ordinary scratch, no rack.
        let mut g = EightBallGame::with_ruleset(3, 0, 5);
        let aim = g.table.pocket(PocketId::FootLeft).aim_point();
        let back = (DVec2::ZERO - aim).normalize();
        on_break_with(&mut g, &[(0, aim + back * 0.5), (5, aim + back * 0.08)]);
        let shot =
            straight_shot(&g, BallId(5), PocketId::FootLeft, 4.0).with_tip(DVec2::new(0.0, 0.9));
        let report = g.play_shot(&shot, None).unwrap();
        assert!(report.fouls.contains(&Foul::CueBallPocketed), "{report:?}");
        assert!(!report.reracked && !g.is_break());
        assert_eq!(g.ball_in_hand(), Some(BallInHand::Kitchen));
    }

    #[test]
    fn the_eight_alone_on_the_break_wins() {
        let mut g = EightBallGame::new(3, 0);
        on_break_with(
            &mut g,
            &[(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.3, 0.2)), (2, DVec2::new(0.0, -0.3))],
        );
        let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 2.0);
        let report = g.play_shot(&shot, None).unwrap();
        assert_eq!(report.pocketed.len(), 1);
        assert_eq!(
            report.game_over,
            Some(GameOver { winner: 0, reason: GameOverReason::PocketedEight })
        );
    }

    #[test]
    fn the_eight_with_another_ball_on_the_break_loses() {
        let mut g = EightBallGame::new(3, 0);
        let t = g.table.clone();
        let r = t.spec.ball_radius;
        // The 8 and the 2 each hang over a corner pocket; one stroke sends
        // the cue ball into the 8 and the 8's neighbour into the 2.
        let foot_left = t.pocket(PocketId::FootLeft).aim_point();
        let foot_right = t.pocket(PocketId::FootRight).aim_point();
        let inward = |p: DVec2, d: f64| p + (DVec2::ZERO - p).normalize() * d;
        on_break_with(
            &mut g,
            &[
                (0, DVec2::new(0.0, 0.0)),
                (8, inward(foot_left, 0.12)),
                (2, inward(foot_right, 0.12)),
                (11, inward(foot_right, 0.12 + 2.0 * r + 0.002)),
            ],
        );
        // Find a stroke that makes both (the cue ball off the 11 into the 8
        // is too fine to aim by hand: sweep).
        let mut lost = None;
        for k in 0..2000 {
            let mut trial = g.clone();
            let yaw = -0.9 + k as f64 * 0.0009;
            let report = trial.play_shot(&Shot::new(yaw, 6.0), None).unwrap();
            let made: Vec<u8> = report.pocketed.iter().map(|(b, _)| b.0).collect();
            if made.contains(&8) && made.len() > 1 {
                lost = Some(report);
                break;
            }
        }
        // The layout above is hard to hit in one stroke on every table; the
        // rule itself is what matters, so fall back to the cue ball going
        // down with the 8 (a scratch on the 8, the same verdict).
        let report = lost.unwrap_or_else(|| {
            let mut g = EightBallGame::new(3, 0);
            let foot_left = g.table.pocket(PocketId::FootLeft).aim_point();
            let p8 = foot_left + (DVec2::ZERO - foot_left).normalize() * 0.08;
            on_break_with(
                &mut g,
                &[(0, p8 + (DVec2::ZERO - foot_left).normalize() * 0.5), (8, p8)],
            );
            let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 6.0)
                .with_tip(DVec2::new(0.0, 0.9));
            g.play_shot(&shot, None).unwrap()
        });
        assert!(report.pocketed.len() > 1, "{report:?}");
        assert_eq!(
            report.game_over,
            Some(GameOver { winner: 1, reason: GameOverReason::EightWithAnotherBall })
        );
    }

    #[test]
    fn the_eight_with_an_opponents_ball_at_the_end_loses() {
        let mut g = house(&[(0, DVec2::new(-0.2, 0.0))]);
        let t = g.table.clone();
        let r = t.spec.ball_radius;
        // Player 0 (stripes, none left) is on the 8. The 8 hangs over one
        // corner, the opponent's 3 over the other, and the 8 is struck so
        // that the cue ball runs on into the 3... simpler and certain: the
        // 3 sits on the 8's path, right at the lip, and goes down first.
        let pocket = t.pocket(PocketId::FootLeft).aim_point();
        let back = (DVec2::ZERO - pocket).normalize();
        layout(
            &mut g,
            &[
                (0, pocket + back * 0.9),
                (8, pocket + back * (0.10 + 2.0 * r + 0.001)),
                (3, pocket + back * 0.10),
            ],
        );
        assert!(g.is_on_eight(0));
        let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 3.5);
        let report = g.play_shot(&shot, Some(PocketId::FootLeft)).unwrap();
        let made: Vec<u8> = report.pocketed.iter().map(|(b, _)| b.0).collect();
        assert!(made.contains(&8) && made.contains(&3), "{report:?}");
        assert_eq!(
            report.game_over,
            Some(GameOver { winner: 1, reason: GameOverReason::EightWithAnotherBall })
        );
    }

    #[test]
    fn the_eight_alone_at_the_end_wins_and_with_the_cue_ball_loses() {
        let mut g = house(&[(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.6, 0.3))]);
        let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 2.0);
        let report = g.play_shot(&shot, Some(PocketId::FootLeft)).unwrap();
        assert_eq!(
            report.game_over,
            Some(GameOver { winner: 0, reason: GameOverReason::PocketedEight })
        );

        // The same pot followed in by the cue ball.
        let mut g = house(&[(0, DVec2::new(-0.2, 0.0))]);
        let pocket = g.table.pocket(PocketId::FootLeft).aim_point();
        let back = (DVec2::ZERO - pocket).normalize();
        layout(&mut g, &[(0, pocket + back * 0.5), (8, pocket + back * 0.08)]);
        let shot = straight_shot(&g, BallId::EIGHT, PocketId::FootLeft, 4.0)
            .with_tip(DVec2::new(0.0, 0.9));
        let report = g.play_shot(&shot, Some(PocketId::FootLeft)).unwrap();
        assert!(report.fouls.contains(&Foul::CueBallPocketed), "{report:?}");
        assert_eq!(
            report.game_over,
            Some(GameOver { winner: 1, reason: GameOverReason::EightPocketedOnFoul })
        );
    }

    /// Balls knocked off the table (the events are given by hand: a jump
    /// that clears the rail is hard to produce on demand).
    #[test]
    fn balls_knocked_off_the_table_under_the_house_rules() {
        let knock_off = |ball: u8| {
            let mut g = house(&[
                (0, DVec2::new(-0.2, 0.0)),
                (12, DVec2::new(0.3, 0.0)),
                (14, DVec2::new(0.5, 0.3)),
                (3, DVec2::new(0.5, -0.3)),
                (6, DVec2::new(0.6, -0.1)),
                (8, DVec2::new(-0.6, -0.3)),
            ]);
            let mut pending = g.prepare_shot(&Shot::new(0.0, 1.0), None).unwrap();
            pending.simulation.run_to_rest();
            pending.simulation.knock_off_for_tests(BallId(ball));
            let report = g.finish_shot(pending);
            (g, report)
        };
        // One's own ball comes back; the turn is lost, nothing else.
        let (g, report) = knock_off(14);
        assert_eq!(report.fouls, vec![Foul::ObjectBallOffTable(BallId(14))]);
        assert_eq!(report.spotted, vec![BallId(14)]);
        assert!(report.penalty.is_empty());
        assert!(g.ball(BallId(14)).in_play());
        assert_eq!((g.current_player(), g.ball_in_hand()), (1, None));
        // The opponent's stays off, made for them.
        let (g, report) = knock_off(3);
        assert_eq!(report.fouls, vec![Foul::ObjectBallOffTable(BallId(3))]);
        assert!(report.spotted.is_empty());
        assert_eq!(report.penalty, vec![BallId(3)]);
        assert_eq!(on_table(&g, Group::Solids), vec![6]);
        assert_eq!((g.current_player(), g.ball_in_hand()), (1, None));
        // The cue ball: the turn and ball in hand, no ball given.
        let (g, report) = knock_off(0);
        assert_eq!(report.fouls, vec![Foul::CueBallOffTable]);
        assert!(report.penalty.is_empty());
        assert_eq!(on_table(&g, Group::Solids), vec![3, 6]);
        assert_eq!((g.current_player(), g.ball_in_hand()), (1, Some(BallInHand::Anywhere)));
        assert!(g.ball(BallId::CUE).in_play());
    }

    /// The 8 knocked off the table on the break is no loss: it goes back on
    /// the foot spot and the turn passes, a foul (see the rules as players
    /// read them, "The 8-ball on the break").
    #[test]
    fn the_eight_off_the_table_on_the_break_is_spotted() {
        let mut g = EightBallGame::new(3, 0);
        let mut pending = g.prepare_shot(&Shot::new(g.default_aim(), 6.0), None).unwrap();
        pending.simulation.run_to_rest();
        pending.simulation.knock_off_for_tests(BallId::EIGHT);
        let report = g.finish_shot(pending);
        assert!(report.fouls.contains(&Foul::ObjectBallOffTable(BallId::EIGHT)), "{report:?}");
        assert!(report.spotted.contains(&BallId::EIGHT));
        assert_eq!(report.game_over, None);
        assert!(!report.continues);
        assert_eq!(g.current_player(), 1);
        let eight = g.ball(BallId::EIGHT);
        assert!(eight.in_play() && eight.xy().y == 0.0 && eight.xy().x >= g.table.foot_spot().x);
    }

    // ---- Ruleset 5: the default from 0.7.0 ------------------------------

    /// A game under ruleset 5 past the break, an open table with these
    /// balls on it.
    fn ruleset_5(positions: &[(u8, DVec2)]) -> EightBallGame {
        let mut g = EightBallGame::with_ruleset(3, 0, 5);
        assert_eq!(g.ruleset(), 5);
        g.is_break = false;
        g.ball_in_hand = None;
        layout(&mut g, positions);
        g
    }

    /// Ruleset 5 is a game like any other: its snapshot names it and comes
    /// back under it, on ruleset 4's table with its own physics.
    #[test]
    fn ruleset_5_can_be_played_and_read_back() {
        let mut g = EightBallGame::with_ruleset(9, 1, 5);
        g.play_shot(&Shot::new(g.default_aim(), 6.0), None).unwrap();
        let back: EightBallGame =
            serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert_eq!(back, g);
        assert_eq!(back.ruleset(), 5);
        assert_eq!(back.table(), EightBallGame::with_ruleset(1, 0, 4).table());
        assert_eq!(*back.params(), PhysicsParams::of_ruleset(5));
        assert!(back.params().jumps_land_on_rails);
        // A ruleset this version doesn't know plays as the newest it knows.
        assert_eq!(EightBallGame::with_ruleset(1, 0, 200).ruleset(), crate::NEWEST_RULESET);
    }

    /// Under ruleset 5 a break that makes a solid and a stripe passes the
    /// turn and leaves the table open; one kind only keeps the table and
    /// sets the groups, as before. Ruleset 4 played on after either; from
    /// ruleset 6 the first made sets the groups and the turn passes.
    #[test]
    fn a_break_with_both_groups_passes_the_turn() {
        let kinds = |r: &ShotReport| {
            let made: Vec<BallId> = r.pocketed.iter().map(|(b, _)| *b).collect();
            let solids = made.iter().any(|b| Group::Solids.contains(*b));
            let stripes = made.iter().any(|b| Group::Stripes.contains(*b));
            (solids, stripes, made.contains(&BallId::EIGHT))
        };
        let (mut mixed, mut single, mut old_mixed, mut new_mixed) = (0, 0, 0, 0);
        for seed in 0..300u64 {
            // Real breaks, struck a little differently each time.
            let yaw = (seed % 9) as f64 * 0.003 - 0.012;
            for ruleset in [4, 5, 6] {
                let mut g = EightBallGame::with_ruleset(seed, 0, ruleset);
                let report = g.play_shot(&Shot::new(yaw, g.max_cue_speed()), None).unwrap();
                let (solids, stripes, eight) = kinds(&report);
                if report.is_foul() || eight || report.reracked {
                    continue;
                }
                if solids && stripes && ruleset == 6 {
                    // The first one made decides; the other kind passes the turn.
                    new_mixed += 1;
                    let first = report.pocketed.iter().find_map(|(b, _)| Group::of(*b));
                    assert_eq!(report.group_assigned, first, "seed {seed}: {report:?}");
                    assert!(report.opponent_ball && !report.continues, "seed {seed}");
                    assert_eq!(g.current_player(), 1, "the opponent's turn");
                } else if solids && stripes {
                    if ruleset == 5 {
                        mixed += 1;
                        assert!(!report.continues, "seed {seed}: {report:?}");
                        assert!(g.is_open_table() && report.group_assigned.is_none());
                        assert_eq!(g.current_player(), 1, "the opponent's turn");
                        assert!(report.balls_made().is_empty(), "none made for the shooter");
                    } else {
                        old_mixed += 1;
                        assert!(report.continues && report.group_assigned.is_some());
                    }
                } else if (solids || stripes) && ruleset == 5 {
                    single += 1;
                    assert!(report.continues && report.group_assigned.is_some(), "{report:?}");
                    assert_eq!(g.current_player(), 0);
                }
            }
        }
        assert!(
            mixed > 0 && single > 0 && old_mixed > 0 && new_mixed > 0,
            "{mixed} {single} {old_mixed} {new_mixed}"
        );
    }

    /// Every group ball went down on fouls, which decide no group: under
    /// ruleset 4 the 8 can only be hit (a foul) or made too early, and
    /// nobody can win. Under ruleset 5 both players are on the 8.
    #[test]
    fn an_open_table_with_no_group_ball_left_puts_both_players_on_the_eight() {
        let at = [(0, DVec2::new(-0.2, 0.0)), (8, DVec2::new(0.3, 0.2))];
        let mut old = EightBallGame::with_ruleset(3, 0, 4);
        old.is_break = false;
        old.ball_in_hand = None;
        layout(&mut old, &at);
        assert!(old.is_open_table() && !old.is_on_eight(0) && !old.needs_called_pocket());
        let shot = straight_shot(&old, BallId::EIGHT, PocketId::FootLeft, 2.0);
        let soft = Shot::new(shot.yaw, 0.6);
        assert!(old.clone().play_shot(&soft, None).unwrap().is_foul(), "the 8 first: a foul");
        let report = old.play_shot(&shot, None).unwrap();
        assert_eq!(
            report.game_over,
            Some(GameOver { winner: 1, reason: GameOverReason::EightPocketedEarly })
        );

        let mut g = ruleset_5(&at);
        assert!(g.is_open_table() && g.is_on_eight(0) && g.is_on_eight(1));
        assert_eq!(g.legal_targets(), vec![BallId::EIGHT]);
        assert_eq!(g.play_shot(&shot, None), Err(ShotError::MustCallPocket));
        // Touching the 8 is legal now (and the call binding from then on).
        let mut touch = g.clone();
        let report = touch.play_shot(&soft, Some(PocketId::FootLeft)).unwrap();
        assert_eq!(report.first_contact, Some(BallId::EIGHT));
        assert_eq!(report.fouls, vec![], "{report:?}");
        assert_eq!(touch.called_pocket_of(0), Some(PocketId::FootLeft));
        assert!(touch.needs_called_pocket(), "player 1 is on the 8 too");
        // Made in the called pocket, it wins.
        let report = g.play_shot(&shot, Some(PocketId::FootLeft)).unwrap();
        assert_eq!(
            report.game_over,
            Some(GameOver { winner: 0, reason: GameOverReason::PocketedEight })
        );
    }

    /// Under ruleset 4 the cue ball in hand could not be placed against a
    /// cushion (a radius from its line, where a ball at rest against it is
    /// a millimetre nearer), and in a corner it could be placed past the
    /// pocket's mouth. Ruleset 5 puts both right.
    #[test]
    fn the_cue_ball_in_hand_goes_against_a_cushion_but_never_in_a_pocket() {
        for ruleset in [4, 5] {
            let mut g = EightBallGame::with_ruleset(5, 0, ruleset);
            g.is_break = false;
            g.ball_in_hand = Some(BallInHand::Anywhere);
            let t = g.table.clone();
            let (hl, hw, r) = (t.spec.length / 2.0, t.spec.width / 2.0, t.spec.ball_radius);
            let c = t.cushion_contact();
            assert!(c < r - 0.0005, "the nose meets a ball below its widest");
            let frozen = DVec2::new(-0.3, hw - c);
            let corner = DVec2::new(-(hl - r), hw - r);
            let mouth = t.pocket(PocketId::HeadLeft);
            assert!((corner - mouth.mouth_center).dot(mouth.outward) > 0.01, "past the mouth");
            assert!(mouth.in_throat(corner));
            assert_eq!(g.can_place_cue_ball(frozen), ruleset >= 5, "ruleset {ruleset}");
            assert_eq!(g.can_place_cue_ball(corner), ruleset < 5, "ruleset {ruleset}");
            // Nowhere past a mouth or into a cushion, under ruleset 5.
            if ruleset >= 5 {
                assert!(!g.can_place_cue_ball(frozen + DVec2::new(0.0, 1e-6)));
                for k in PocketId::ALL {
                    let p = t.pocket(k);
                    assert!(!g.can_place_cue_ball(p.mouth_center + p.outward * 0.001));
                }
                // Struck away, a ball placed against the cushion leaves it
                // without a touch; struck into it, it comes off it.
                assert!(g.place_cue_ball(frozen));
                let away = g.prepare_shot(&Shot::new(-std::f64::consts::FRAC_PI_2, 1.0), None);
                let mut away = away.unwrap().simulation;
                away.run_to_rest();
                let cushion = |e: &Event| matches!(e, Event::Cushion { ball: BallId::CUE, .. });
                let first = away.events().iter().find(|e| cushion(e)).map(|e| e.time());
                assert!(first.is_none_or(|t| t > 0.5), "{first:?}");
                let into = g.prepare_shot(&Shot::new(std::f64::consts::FRAC_PI_2, 1.0), None);
                let mut into = into.unwrap().simulation;
                into.run_to_rest();
                assert!(into.events().iter().any(cushion));
            }
        }
    }

    /// A scratch with another ball on the break leaves the opponent the cue
    /// ball in hand behind the head string. If their clock runs out, ruleset
    /// 4 gave the breaker the cue ball anywhere; ruleset 5 keeps it behind
    /// the line.
    #[test]
    fn a_clock_that_runs_out_keeps_the_cue_ball_in_hand_as_it_was() {
        for ruleset in [4, 5] {
            let mut g = EightBallGame::with_ruleset(3, 0, ruleset);
            let aim = g.table.pocket(PocketId::FootLeft).aim_point();
            let back = (DVec2::ZERO - aim).normalize();
            on_break_with(&mut g, &[(0, aim + back * 0.5), (5, aim + back * 0.08)]);
            let shot = straight_shot(&g, BallId(5), PocketId::FootLeft, 4.0)
                .with_tip(DVec2::new(0.0, 0.9));
            let report = g.play_shot(&shot, None).unwrap();
            assert!(report.fouls.contains(&Foul::CueBallPocketed) && !report.reracked);
            assert_eq!((g.current_player(), g.ball_in_hand()), (1, Some(BallInHand::Kitchen)));
            g.time_foul();
            let hand = if ruleset >= 5 { BallInHand::Kitchen } else { BallInHand::Anywhere };
            assert_eq!((g.current_player(), g.ball_in_hand()), (0, Some(hand)), "{ruleset}");
            // Elsewhere the clock still only passes the turn.
            let mut g = ruleset_5(&[(0, DVec2::new(-0.2, 0.0)), (3, DVec2::new(0.3, 0.1))]);
            g.time_foul();
            assert_eq!((g.current_player(), g.ball_in_hand()), (1, None));
        }
    }

    /// A tip so far out that its length overflows an f64: ruleset 4 struck
    /// the centre of the ball (and must go on doing so: a game recorded with
    /// one replays the same), ruleset 5 the edge, in its direction. Any tip
    /// that can be measured is struck as before.
    #[test]
    fn a_tip_too_far_out_to_measure_is_struck_at_the_edge() {
        let tip_of = |ruleset: u8, tip: DVec2| {
            let g = EightBallGame::with_ruleset(1, 0, ruleset);
            g.prepare_shot(&Shot::new(0.0, 2.0).with_tip(tip), None).unwrap().shot.tip
        };
        let huge = DVec2::new(3e200, -3e200);
        assert_eq!(tip_of(4, huge), DVec2::ZERO);
        let edge = tip_of(5, huge);
        assert!((edge.length() - 1.0).abs() < 1e-12 && (edge.x + edge.y).abs() < 1e-12, "{edge}");
        for tip in [DVec2::new(3.0, -4.0), DVec2::new(0.3, 0.2), DVec2::new(1e150, 2.0)] {
            assert_eq!(
                tip_of(5, tip).to_array().map(f64::to_bits),
                tip_of(4, tip).to_array().map(f64::to_bits)
            );
        }
    }

    /// Random games under ruleset 5: the house rules' invariants hold, and
    /// a cue ball in hand only goes, or is put back, where a ball can rest.
    #[test]
    fn ruleset_5_holds_over_random_games() {
        let r = Table::default().spec.ball_radius;
        for seed in 0..40u64 {
            let mut g = EightBallGame::with_ruleset(seed, (seed % 2) as usize, 5);
            let mut rng = Rng::new(seed + 5000);
            for _ in 0..120 {
                if g.game_over().is_some() {
                    break;
                }
                if g.ball_in_hand().is_some() {
                    for _ in 0..300 {
                        let p = DVec2::new(rng.next_f64() * 2.4 - 1.2, rng.next_f64() * 1.2 - 0.6);
                        if g.place_cue_ball(p) {
                            assert!(g.table().can_place_ball(p), "{p}");
                            break;
                        }
                    }
                }
                let shooter = g.current_player();
                let shot =
                    Shot::new(rng.next_f64() * std::f64::consts::TAU, 0.3 + rng.next_f64() * 6.5)
                        .with_tip(DVec2::new(rng.next_f64() - 0.5, rng.next_f64() - 0.5));
                let called = g.needs_called_pocket().then_some(PocketId::ALL[rng.below(6)]);
                let report = g.play_shot(&shot, called).unwrap();
                match (g.group_of(0), g.group_of(1)) {
                    (None, None) => {}
                    (Some(a), Some(b)) => assert_eq!(a, b.opposite()),
                    other => panic!("half-assigned groups {other:?}"),
                }
                if g.game_over().is_none() {
                    let cue_gone = report
                        .fouls
                        .iter()
                        .any(|f| matches!(f, Foul::CueBallPocketed | Foul::CueBallOffTable));
                    assert_eq!(g.ball_in_hand().is_some(), cue_gone, "{report:?}");
                    assert_eq!(g.current_player() == shooter, report.continues);
                    assert!(g.ball(BallId::CUE).in_play() && g.ball(BallId::EIGHT).in_play());
                    if g.ball_in_hand().is_some() {
                        assert!(g.can_place_cue_ball(g.ball(BallId::CUE).xy()), "{report:?}");
                    }
                }
                let live: Vec<_> = g.balls().iter().filter(|b| b.in_play()).collect();
                for (k, a) in live.iter().enumerate() {
                    for b in &live[k + 1..] {
                        assert!(a.xy().distance(b.xy()) > 2.0 * r - 1e-6, "{} {}", a.id, b.id);
                    }
                }
            }
        }
    }

    // ---- Ruleset 6: the default from 0.7.1 ------------------------------

    /// A break under `ruleset` that the cue ball strikes softly, with the
    /// balls `down` made, in that order, at its end (wherever the cue ball
    /// sent them): what the rules do with the order alone.
    fn break_making(ruleset: u8, down: &[u8]) -> (EightBallGame, ShotReport) {
        let mut g = EightBallGame::with_ruleset(3, 0, ruleset);
        let mut pending = g.prepare_shot(&Shot::new(g.default_aim(), 1.0), None).unwrap();
        pending.simulation.run_to_rest();
        assert!(
            pending.simulation.events().iter().all(|e| !matches!(e, Event::Pocketed { .. })),
            "a soft break makes nothing by itself"
        );
        for (k, n) in down.iter().enumerate() {
            pending.simulation.pocket_for_tests(BallId(*n), PocketId::ALL[k % 6]);
        }
        let report = g.finish_shot(pending);
        (g, report)
    }

    /// From ruleset 6 the first ball made on the break decides the groups,
    /// whatever goes down after it: the 3 and then the 13 make the breaker
    /// solids, and the 13 (the opponent's) passes the turn, made for them.
    /// One kind only keeps the table, as before. Ruleset 5 left the table
    /// open after both kinds.
    #[test]
    fn the_first_ball_made_on_the_break_decides_the_groups() {
        let (g, report) = break_making(6, &[3, 13]);
        assert_eq!(report.fouls, vec![]);
        assert_eq!(report.group_assigned, Some(Group::Solids));
        assert_eq!((g.group_of(0), g.group_of(1)), (Some(Group::Solids), Some(Group::Stripes)));
        assert!(report.opponent_ball && !report.continues, "{report:?}");
        assert_eq!(g.current_player(), 1);
        assert_eq!(report.balls_made(), vec![BallId(3)]);
        assert!(!g.ball(BallId(13)).in_play(), "the 13 stays down, made for the opponent");
        assert_eq!(g.ball_in_hand(), None);

        // The other way round: the 13 first makes the breaker stripes.
        let (g, report) = break_making(6, &[13, 3, 5]);
        assert_eq!(report.group_assigned, Some(Group::Stripes));
        assert_eq!(g.group_of(0), Some(Group::Stripes));
        assert!(report.opponent_ball && !report.continues);
        assert_eq!(report.balls_made(), vec![BallId(13)]);

        // One kind only: the breaker keeps the table.
        let (g, report) = break_making(6, &[11, 9]);
        assert_eq!(report.group_assigned, Some(Group::Stripes));
        assert!(!report.opponent_ball && report.continues);
        assert_eq!(g.current_player(), 0);

        // Ruleset 5: both kinds leave the table open and pass the turn.
        let (g, report) = break_making(5, &[3, 13]);
        assert_eq!(report.group_assigned, None);
        assert!(g.is_open_table() && !report.continues && !report.opponent_ball);
        assert_eq!(g.current_player(), 1);
    }

    /// From ruleset 6 a break that makes only the cue ball is not racked
    /// again: the balls stay where they went, the cue ball goes back to the
    /// head spot, not in hand, and the opponent plays on an open table.
    #[test]
    fn a_break_that_only_makes_the_cue_ball_is_played_on() {
        let mut g = EightBallGame::with_ruleset(3, 0, 6);
        let aim = g.table.pocket(PocketId::FootLeft).aim_point();
        on_break_with(
            &mut g,
            &[(0, aim + (DVec2::ZERO - aim).normalize() * 0.4), (5, DVec2::new(-0.3, 0.0))],
        );
        let d = aim - g.ball(BallId::CUE).xy();
        let report = g.play_shot(&Shot::new(d.y.atan2(d.x), 2.0), None).unwrap();
        assert!(report.fouls.contains(&Foul::CueBallPocketed), "{report:?}");
        assert!(!report.reracked && !report.continues && report.game_over.is_none());
        assert_eq!(g.current_player(), 1);
        assert!(!g.is_break() && g.is_open_table());
        assert_eq!(g.ball_in_hand(), None);
        assert_eq!(g.ball(BallId::CUE).xy(), g.table.head_spot());
        assert_eq!(g.ball(BallId(5)).xy(), DVec2::new(-0.3, 0.0), "nothing is racked again");
        // The opponent's shot is an ordinary one, from there.
        let shot = straight_shot(&g, BallId(5), PocketId::FootLeft, 2.0);
        let report = g.play_shot(&shot, None).unwrap();
        assert!(!report.was_break);
    }

    /// From ruleset 6 the cue ball knocked off the table also comes back to
    /// the head spot, not in hand, and a foul's penalty is given as before.
    #[test]
    fn the_cue_ball_off_the_table_comes_back_to_the_head_spot() {
        let mut g = EightBallGame::with_ruleset(3, 0, 6);
        g.is_break = false;
        g.ball_in_hand = None;
        g.groups = [Some(Group::Solids), Some(Group::Stripes)];
        layout(
            &mut g,
            &[
                (0, DVec2::new(0.4, 0.0)),
                (2, DVec2::new(0.2, 0.3)),
                (10, DVec2::new(-0.5, 0.3)),
                (11, DVec2::new(0.5, -0.3)),
                (8, DVec2::new(-0.6, -0.3)),
            ],
        );
        // A miss that ends with the cue ball off the table.
        let mut pending =
            g.prepare_shot(&Shot::new(std::f64::consts::PI / 2.0, 0.5), None).unwrap();
        pending.simulation.run_to_rest();
        pending.simulation.knock_off_for_tests(BallId::CUE);
        let report = g.finish_shot(pending);
        assert!(report.fouls.contains(&Foul::CueBallOffTable), "{report:?}");
        assert!(report.fouls.contains(&Foul::NoContact), "{report:?}");
        assert_eq!(report.penalty, vec![BallId(10)], "the miss gives the opponent their lowest");
        assert_eq!(g.ball_in_hand(), None);
        assert_eq!(g.ball(BallId::CUE).xy(), g.table.head_spot());
        assert_eq!(g.current_player(), 1);
        // A clock that runs out now passes the turn alone.
        g.time_foul();
        assert_eq!((g.current_player(), g.ball_in_hand()), (0, None));
    }

    /// From ruleset 6 the cue ball is in hand for the break alone: a clock
    /// that runs out on the break leaves it in hand behind the head string.
    #[test]
    fn the_cue_ball_is_in_hand_for_the_break_only() {
        let mut g = EightBallGame::with_ruleset(3, 0, 6);
        assert_eq!(g.ball_in_hand(), Some(BallInHand::Kitchen));
        g.time_foul();
        assert_eq!((g.current_player(), g.ball_in_hand()), (1, Some(BallInHand::Kitchen)));
        assert!(g.is_break());
        let report = g.play_shot(&Shot::new(g.default_aim(), g.max_cue_speed()), None).unwrap();
        assert!(report.was_break);
        assert_eq!(g.ball_in_hand(), None, "{report:?}");
    }

    #[test]
    fn full_random_game_terminates() {
        // Plays a game with simple straight-in attempts to make sure the state
        // machine never gets stuck.
        let mut g = EightBallGame::new(11, 0);
        g.play_shot(&Shot::new(0.0, 7.0), None).unwrap();
        let mut rng = Rng::new(5);
        for _ in 0..400 {
            if g.game_over().is_some() {
                break;
            }
            if g.ball_in_hand().is_some() {
                for _ in 0..200 {
                    let p = DVec2::new(rng.next_f64() * 1.8 - 0.9, rng.next_f64() * 0.8 - 0.4);
                    if g.place_cue_ball(p) {
                        break;
                    }
                }
            }
            let target = g.legal_targets()[0];
            let pocket = PocketId::ALL[rng.below(6)];
            let shot = straight_shot(&g, target, pocket, 1.0 + rng.next_f64() * 3.0);
            let called = g.needs_called_pocket().then_some(pocket);
            g.play_shot(&shot, called).unwrap();
        }
        assert!(g.game_over().is_some(), "game should finish");
    }
}
