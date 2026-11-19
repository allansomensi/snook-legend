//! Drills: short exercises on fixed positions, the same for everyone (a
//! long straight pot, a cut, a run of ten, leaving the cue ball in a zone,
//! escaping a snooker…). Each has a few tries and earns up to three stars;
//! the best score of each player makes a table per drill, and the stars of
//! every drill add up to the player's challenge rating.
//!
//! The game sends the strokes of a session and the server plays them again
//! ([`score`]): the score is the server's.

use glam::DVec2;
use serde::{Deserialize, Serialize};
use snook_legend_core::rng::Rng;
use snook_legend_core::{BallId, EightBallGame, Event, PocketId, Shot, ShotReport};

/// What a drill asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Goal {
    /// Make `ball` in `pocket`.
    Pot { ball: BallId, pocket: PocketId },
    /// Make `ball` in `pocket` off at least `cushions` cushions.
    Bank { ball: BallId, pocket: PocketId, cushions: u8 },
    /// Make `ball` in `pocket` and stop the cue ball within `radius` of
    /// `zone`.
    Position { ball: BallId, pocket: PocketId, zone: DVec2, radius: f64 },
    /// Hit `ball` first, without a foul.
    Escape { ball: BallId },
    /// A different pot each try, one after another: a miss ends the
    /// session; the score is the pots made in a row.
    Streak,
}

/// A drill.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drill {
    /// Stable identifier.
    pub code: &'static str,
    /// English, translated by the game.
    pub name: &'static str,
    pub description: &'static str,
    /// Tries in a session.
    pub tries: u32,
    /// The score for one, two and three stars.
    pub stars: [u32; 3],
    pub goal: Goal,
}

/// The ball every single-ball drill uses.
const ONE: BallId = BallId(1);

/// Every drill, easiest first.
pub const DRILLS: &[Drill] = &[
    Drill {
        code: "straight",
        name: "Straight in",
        description: "A long straight pot into the corner. Five tries.",
        tries: 5,
        stars: [2, 4, 5],
        goal: Goal::Pot { ball: ONE, pocket: PocketId::FootRight },
    },
    Drill {
        code: "cut",
        name: "The cut",
        description: "A half-ball cut into the corner. Five tries.",
        tries: 5,
        stars: [2, 3, 5],
        goal: Goal::Pot { ball: ONE, pocket: PocketId::FootLeft },
    },
    Drill {
        code: "draw",
        name: "Draw it back",
        description: "Pot the ball and draw the cue ball back into the circle. Five tries.",
        tries: 5,
        stars: [1, 3, 5],
        goal: Goal::Position {
            ball: ONE,
            pocket: PocketId::FootRight,
            zone: DVec2::new(0.05, 0.02),
            radius: 0.16,
        },
    },
    Drill {
        code: "follow",
        name: "Follow through",
        description: "Pot the ball and leave the cue ball in the circle past it. Five tries.",
        tries: 5,
        stars: [1, 3, 5],
        goal: Goal::Position {
            ball: ONE,
            pocket: PocketId::SideLeft,
            zone: DVec2::new(0.55, 0.30),
            radius: 0.17,
        },
    },
    Drill {
        code: "bank",
        name: "Bank shot",
        description: "Off one cushion into the side pocket. Five tries.",
        tries: 5,
        stars: [1, 2, 4],
        goal: Goal::Bank { ball: ONE, pocket: PocketId::SideRight, cushions: 1 },
    },
    Drill {
        code: "escape",
        name: "Snooker escape",
        description: "Hit the 1 first: it hides behind other balls. Five tries.",
        tries: 5,
        stars: [1, 3, 5],
        goal: Goal::Escape { ball: ONE },
    },
    Drill {
        code: "side",
        name: "Side pocket",
        description: "A cut into the side pocket, where the angle is tight. Five tries.",
        tries: 5,
        stars: [2, 3, 5],
        goal: Goal::Pot { ball: ONE, pocket: PocketId::SideRight },
    },
    Drill {
        code: "long",
        name: "The long cut",
        description: "A cut from the far end of the table into the corner. Five tries.",
        tries: 5,
        stars: [1, 3, 5],
        goal: Goal::Pot { ball: ONE, pocket: PocketId::FootRight },
    },
    Drill {
        code: "thin",
        name: "Thin cut",
        description: "Barely graze the ball: a thin cut into the corner. Five tries.",
        tries: 5,
        stars: [1, 2, 4],
        goal: Goal::Pot { ball: ONE, pocket: PocketId::FootLeft },
    },
    Drill {
        code: "rail",
        name: "Along the cushion",
        description: "The ball sits by the cushion: run it along into the corner. Five tries.",
        tries: 5,
        stars: [1, 3, 5],
        goal: Goal::Pot { ball: ONE, pocket: PocketId::FootLeft },
    },
    Drill {
        code: "stop",
        name: "Stop shot",
        description: "Pot the ball and stop the cue ball dead where the ball was. Five tries.",
        tries: 5,
        stars: [1, 3, 5],
        goal: Goal::Position {
            ball: ONE,
            pocket: PocketId::FootLeft,
            zone: DVec2::new(0.45, 0.20),
            radius: 0.07,
        },
    },
    Drill {
        code: "combo",
        name: "The combination",
        description: "Pot the 1 by playing the 2 into it. Five tries.",
        tries: 5,
        stars: [1, 2, 4],
        goal: Goal::Pot { ball: ONE, pocket: PocketId::FootRight },
    },
    Drill {
        code: "cross",
        name: "Cross-corner bank",
        description: "Off the long cushion and across the table into the corner. Five tries.",
        tries: 5,
        stars: [1, 2, 3],
        goal: Goal::Bank { ball: ONE, pocket: PocketId::FootRight, cushions: 1 },
    },
    Drill {
        code: "kick",
        name: "Kick shot",
        description: "The 1 is walled off: send the cue ball off a cushion to hit it first. Five tries.",
        tries: 5,
        stars: [1, 3, 5],
        goal: Goal::Escape { ball: ONE },
    },
    Drill {
        code: "ten",
        name: "Ten in a row",
        description: "Ten different pots, one after another: a miss ends the session.",
        tries: 10,
        stars: [3, 6, 10],
        goal: Goal::Streak,
    },
    Drill {
        code: "back",
        name: "The far corner",
        description: "A cut back up the table into the corner behind you. Five tries.",
        tries: 5,
        stars: [1, 3, 5],
        goal: Goal::Pot { ball: ONE, pocket: PocketId::HeadLeft },
    },
    Drill {
        code: "side_long",
        name: "Long into the side",
        description: "From the far end of the table into the side pocket. Five tries.",
        tries: 5,
        stars: [1, 3, 5],
        goal: Goal::Pot { ball: ONE, pocket: PocketId::SideRight },
    },
    Drill {
        code: "frozen",
        name: "Cue ball on the cushion",
        description: "The cue ball touches the cushion: pot the ball into the corner all the same. Five tries.",
        tries: 5,
        stars: [1, 3, 5],
        goal: Goal::Pot { ball: ONE, pocket: PocketId::FootRight },
    },
    Drill {
        code: "plant",
        name: "The plant",
        description: "The 2 touches the 1 and points at the pocket: hit the 2 and the 1 goes in. Five tries.",
        tries: 5,
        stars: [2, 4, 5],
        goal: Goal::Pot { ball: ONE, pocket: PocketId::FootLeft },
    },
    Drill {
        code: "carom",
        name: "The carom",
        description: "The 2 is in the way: glance off it and send the 1 into the corner. Five tries.",
        tries: 5,
        stars: [1, 2, 4],
        goal: Goal::Pot { ball: ONE, pocket: PocketId::FootRight },
    },
    Drill {
        code: "long_draw",
        name: "The long draw",
        description: "Pot the ball and draw the cue ball all the way back into the circle. Five tries.",
        tries: 5,
        stars: [1, 2, 4],
        goal: Goal::Position {
            ball: ONE,
            pocket: PocketId::FootLeft,
            zone: DVec2::new(-0.30, -0.10),
            radius: 0.18,
        },
    },
    Drill {
        code: "tangent",
        name: "On the tangent line",
        description: "Pot the ball in the side and let the cue ball run on into the circle. Five tries.",
        tries: 5,
        stars: [1, 3, 5],
        goal: Goal::Position {
            ball: ONE,
            pocket: PocketId::SideLeft,
            zone: DVec2::new(-0.60, 0.20),
            radius: 0.18,
        },
    },
    Drill {
        code: "double_bank",
        name: "Two-cushion bank",
        description: "Off two cushions and into the corner. Five tries.",
        tries: 5,
        stars: [1, 2, 3],
        goal: Goal::Bank { ball: ONE, pocket: PocketId::FootLeft, cushions: 2 },
    },
    Drill {
        code: "fifteen",
        name: "Fifteen in a row",
        description: "Fifteen different pots, one after another: a miss ends the session.",
        tries: 15,
        stars: [5, 10, 15],
        goal: Goal::Streak,
    },
];

/// The drill of `code`.
pub fn drill(code: &str) -> Option<&'static Drill> {
    DRILLS.iter().find(|d| d.code == code)
}

/// Stars for a score (0 to 3).
pub fn stars(d: &Drill, score: u32) -> u32 {
    d.stars.iter().filter(|&&s| score >= s).count() as u32
}

/// The most stars there are.
pub fn max_stars() -> u32 {
    DRILLS.len() as u32 * 3
}

/// A player's challenge rating: a hundred for each star, plus their best
/// scores (which tell apart players with the same stars).
pub fn challenge_rating(best: &[(String, u32)]) -> u32 {
    best.iter()
        .filter_map(|(code, score)| drill(code).map(|d| stars(d, *score) * 100 + score))
        .sum()
}

fn aim(pocket: PocketId) -> DVec2 {
    snook_legend_core::Table::default().pocket(pocket).aim_point()
}

/// The balls of a drill's position.
pub fn position(d: &Drill) -> Vec<(BallId, DVec2)> {
    let r = snook_legend_core::Table::default().spec.ball_radius;
    let eight = (BallId::EIGHT, DVec2::new(-0.80, 0.40));
    // For drills played up the table: the 8 at the other end.
    let eight_far = (BallId::EIGHT, DVec2::new(0.95, -0.40));
    match (d.code, d.goal) {
        ("straight", Goal::Pot { pocket, .. }) => {
            // Straight in from across the table.
            let ball = DVec2::new(0.55, -0.26);
            let back = (ball - aim(pocket)).normalize();
            vec![(BallId::CUE, ball + back * 1.25), (ONE, ball), eight]
        }
        ("cut", Goal::Pot { pocket, .. }) => {
            // About a 30-degree cut.
            let ball = DVec2::new(0.62, 0.22);
            let line = (aim(pocket) - ball).normalize();
            let ghost = ball - line * 2.0 * r;
            let side = DVec2::new(line.y, -line.x);
            let from = (line * 0.87 + side * 0.5).normalize();
            vec![(BallId::CUE, ghost - from * 0.75), (ONE, ball), eight]
        }
        ("draw", Goal::Position { pocket, .. }) => {
            // Straight in, the cue ball close behind: draw brings it back.
            let ball = DVec2::new(0.62, -0.30);
            let back = (ball - aim(pocket)).normalize();
            vec![(BallId::CUE, ball + back * 0.4), (ONE, ball), eight]
        }
        ("follow", Goal::Position { pocket, .. }) => {
            // Into the side pocket, the cue ball rolling on towards the circle.
            let ball = DVec2::new(0.22, 0.36);
            let back = (ball - aim(pocket)).normalize();
            vec![(BallId::CUE, ball + back * 0.45), (ONE, ball), eight]
        }
        ("bank", _) => {
            // The ball near the far cushion, opposite the side pocket.
            vec![(BallId::CUE, DVec2::new(-0.55, -0.05)), (ONE, DVec2::new(0.05, 0.30)), eight]
        }
        ("escape", _) => {
            // The 1 behind a wall of two balls: a cushion first.
            let cue = DVec2::new(-0.45, 0.0);
            let target = DVec2::new(0.35, 0.0);
            vec![
                (BallId::CUE, cue),
                (ONE, target),
                (BallId(2), DVec2::new(0.0, 0.0)),
                (BallId(3), DVec2::new(0.0, 2.2 * r)),
                (BallId(4), DVec2::new(0.0, -2.2 * r)),
                eight,
            ]
        }
        ("side", Goal::Pot { pocket, .. }) => {
            // About a 25-degree cut into the side pocket.
            cut_position(DVec2::new(0.30, -0.24), pocket, 25.0, 0.6, eight)
        }
        ("long", Goal::Pot { pocket, .. }) => {
            // A gentle cut from the other end of the table.
            cut_position(DVec2::new(0.80, -0.36), pocket, 14.0, 1.3, eight)
        }
        ("thin", Goal::Pot { pocket, .. }) => {
            // A cut of about 58 degrees.
            cut_position(DVec2::new(0.74, 0.26), pocket, 58.0, 0.55, eight)
        }
        ("rail", Goal::Pot { pocket, .. }) => {
            // The ball a finger from the long cushion, the cue ball a
            // little further out: along the cushion into the corner.
            let hw = snook_legend_core::Table::default().spec.width / 2.0;
            cut_position(DVec2::new(0.35, hw - r - 0.012), pocket, 9.0, 0.55, eight)
        }
        ("stop", Goal::Position { pocket, zone, .. }) => {
            // Straight in: struck low and firm, the cue ball takes the
            // ball's place.
            let back = (zone - aim(pocket)).normalize();
            vec![(BallId::CUE, zone + back * 0.55), (ONE, zone), eight]
        }
        ("combo", Goal::Pot { pocket, .. }) => {
            // The cue ball, the 2 and the 1 in a line with the pocket.
            let ball = DVec2::new(0.78, -0.33);
            let back = (ball - aim(pocket)).normalize();
            let two = ball + back * 0.22;
            vec![(BallId::CUE, two + back * 0.45), (ONE, ball), (BallId(2), two), eight]
        }
        ("cross", _) => {
            // The ball goes to the long cushion beside it and across the
            // table into the far corner.
            vec![(BallId::CUE, DVec2::new(0.22, -0.20)), (ONE, DVec2::new(0.46, 0.30)), eight]
        }
        ("kick", _) => {
            // The 1 behind a wall: off the cushion beside it, or the far one.
            let cue = DVec2::new(-0.50, 0.30);
            vec![
                (BallId::CUE, cue),
                (ONE, DVec2::new(0.50, 0.30)),
                (BallId(2), DVec2::new(0.0, 0.30)),
                (BallId(3), DVec2::new(0.0, 0.30 + 2.2 * r)),
                (BallId(4), DVec2::new(0.0, 0.30 - 2.2 * r)),
                (BallId(5), DVec2::new(0.0, 0.30 - 4.4 * r)),
                eight,
            ]
        }
        ("back", Goal::Pot { pocket, .. }) => {
            // A cut back towards the head of the table, the cue ball
            // further up it than the ball.
            cut_position(DVec2::new(-0.78, 0.30), pocket, 35.0, 0.6, eight_far)
        }
        ("side_long", Goal::Pot { pocket, .. }) => {
            // Nearly straight, from well down the table.
            cut_position(DVec2::new(0.12, -0.42), pocket, 6.0, 0.95, eight)
        }
        ("frozen", _) => {
            // The cue ball against the long cushion, the 1 out in the table.
            let hw = snook_legend_core::Table::default().spec.width / 2.0;
            vec![
                (BallId::CUE, DVec2::new(0.05, -(hw - r - 0.001))),
                (ONE, DVec2::new(0.62, -0.40)),
                eight,
            ]
        }
        ("plant", Goal::Pot { pocket, .. }) => {
            // The 2 frozen to the 1, the line of their centres at the pocket.
            let ball = DVec2::new(0.70, 0.22);
            let line = (aim(pocket) - ball).normalize();
            let two = ball - line * (2.0 * r + 0.0005);
            let from = snook_legend_core::math::from_angle(20f64.to_radians()).rotate(line);
            vec![
                (BallId::CUE, two - line * 2.0 * r - from * 0.55),
                (ONE, ball),
                (BallId(2), two),
                eight,
            ]
        }
        ("carom", Goal::Pot { pocket, .. }) => {
            // The 1 near the corner, the 2 half across the way to it.
            let ball = DVec2::new(0.95, -0.40);
            let line = (aim(pocket) - ball).normalize();
            let ghost = ball - line * 2.0 * r;
            let cue = ghost - line * 0.9;
            let side = DVec2::new(-line.y, line.x);
            let two = (cue + ghost) / 2.0 + side * 1.2 * r;
            vec![(BallId::CUE, cue), (ONE, ball), (BallId(2), two), eight]
        }
        ("long_draw", Goal::Position { pocket, .. }) => {
            // Straight in, the cue ball some way back.
            let ball = DVec2::new(0.70, 0.28);
            let back = (ball - aim(pocket)).normalize();
            vec![(BallId::CUE, ball + back * 0.45), (ONE, ball), eight_far]
        }
        ("tangent", Goal::Position { pocket, .. }) => {
            // A cut into the side; the cue ball runs off along the tangent.
            cut_position(DVec2::new(0.20, 0.38), pocket, 35.0, 0.45, eight_far)
        }
        ("double_bank", _) => {
            // The 1 in the middle of the table: two cushions to the corner.
            vec![(BallId::CUE, DVec2::new(-0.55, -0.20)), (ONE, DVec2::new(-0.05, -0.30)), eight]
        }
        (_, Goal::Streak) => streak_position(0),
        _ => vec![(BallId::CUE, DVec2::new(-0.5, 0.0)), (ONE, DVec2::new(0.5, 0.0)), eight],
    }
}

/// A pot of `ball` into `pocket` with the cue ball `distance` behind the
/// ghost ball, turned `cut_deg` off the line to the pocket.
fn cut_position(
    ball: DVec2,
    pocket: PocketId,
    cut_deg: f64,
    distance: f64,
    eight: (BallId, DVec2),
) -> Vec<(BallId, DVec2)> {
    let r = snook_legend_core::Table::default().spec.ball_radius;
    let line = (aim(pocket) - ball).normalize();
    let ghost = ball - line * 2.0 * r;
    let from = snook_legend_core::math::from_angle(cut_deg.to_radians()).rotate(line);
    vec![(BallId::CUE, ghost - from * distance), (ONE, ball), eight]
}

/// The pot of try `k` of a streak: a pocket in turn, the 1 some way out
/// from it, the cue ball for a cut of up to about 35 degrees.
fn streak_position(k: u32) -> Vec<(BallId, DVec2)> {
    let table = snook_legend_core::Table::default();
    let r = table.spec.ball_radius;
    let mut rng = Rng::new(0x57EA_C000 + k as u64);
    let pocket = PocketId::ALL[k as usize % PocketId::ALL.len()];
    let p = table.pocket(pocket);
    let aim = p.aim_point();
    // Out from the pocket into the table, a little to either side.
    let into = -p.outward;
    let turn = (rng.next_f64() - 0.5) * if pocket.is_side() { 0.7 } else { 0.5 };
    let dir = snook_legend_core::math::from_angle(turn).rotate(into);
    let ball = aim + dir * (0.25 + 0.3 * rng.next_f64());
    let line = (aim - ball).normalize();
    let ghost = ball - line * 2.0 * r;
    let cut = (rng.next_f64() * 2.0 - 1.0) * 35f64.to_radians();
    let from = snook_legend_core::math::from_angle(cut).rotate(line);
    let cue = ghost - from * (0.45 + 0.4 * rng.next_f64());
    let (hl, hw) = (table.spec.length / 2.0 - 2.0 * r, table.spec.width / 2.0 - 2.0 * r);
    let cue = DVec2::new(cue.x.clamp(-hl, hl), cue.y.clamp(-hw, hw));
    // No 8 on the table: nothing in the way, nothing to lose the game on.
    vec![(BallId::CUE, cue), (ONE, ball)]
}

/// The game set up for a drill: an open table past the break.
pub fn game(d: &Drill) -> EightBallGame {
    game_for_try(d, 0)
}

/// The game for try `k` (only a streak changes from try to try).
pub fn game_for_try(d: &Drill, k: u32) -> EightBallGame {
    game_for_session_try(d, 0, k)
}

/// The table of try `k` of a session the server started (`session`, not 0):
/// the cue ball a little off its place (up to [`SESSION_JITTER`] each way),
/// the same for everyone in that session and different from try to try, so
/// one stroke that worked can't simply be sent for every try. Session 0 is
/// the drill as drawn (a session played signed out, for practice).
///
/// A drill is the same whatever the rules a player plays: its positions are
/// played under the Brazilian rules (a ball made keeps the table, with
/// nothing to call).
pub fn game_for_session_try(d: &Drill, session: u64, k: u32) -> EightBallGame {
    let mut g = EightBallGame::new(1, 0, snook_legend_core::Rules::Brazilian);
    let mut balls = if d.goal == Goal::Streak { streak_position(k) } else { position(d) };
    let cue_at = balls.iter().position(|(id, _)| *id == BallId::CUE);
    if session != 0
        && let Some(i) = cue_at
    {
        let mut rng = Rng::new(session ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let shift =
            DVec2::new(rng.next_f64() * 2.0 - 1.0, rng.next_f64() * 2.0 - 1.0) * SESSION_JITTER;
        let moved = balls[i].1 + shift;
        // Only where the ball may lie: on the cloth, clear of the other
        // balls (a table's ball-in-hand check would need a ball in hand).
        let table = g.table();
        let clear = 2.0 * table.spec.ball_radius + 1e-4;
        let free = table.can_place_ball(moved)
            && balls
                .iter()
                .filter(|(id, _)| *id != BallId::CUE)
                .all(|(_, p)| p.distance(moved) >= clear);
        if free {
            balls[i].1 = moved;
        }
    }
    g.set_up_test_position(&balls);
    g
}

/// How far a session moves a try's cue ball, each way (m).
pub const SESSION_JITTER: f64 = 0.012;

/// Plays `shot` from `g`; the report, the events and the table after.
fn play(g: &EightBallGame, shot: &Shot) -> Option<(ShotReport, Vec<Event>, EightBallGame)> {
    let finite = shot.yaw.is_finite()
        && shot.elevation.is_finite()
        && shot.tip.is_finite()
        && shot.speed.is_finite();
    if !finite {
        return None;
    }
    let pending = g.prepare_shot(shot, None).ok()?;
    let mut sim = pending.simulation;
    sim.run_to_rest();
    let events = sim.events().to_vec();
    let mut after = g.clone();
    let report = after.finish_shot(snook_legend_core::PendingShot { simulation: sim, ..pending });
    Some((report, events, after))
}

/// Whether a try of a single-shot drill worked.
pub fn success(d: &Drill, report: &ShotReport, events: &[Event], after: &EightBallGame) -> bool {
    let made = |ball: BallId, pocket: PocketId| {
        !report.is_foul() && report.pocketed.iter().any(|&(b, p)| b == ball && p == pocket)
    };
    match d.goal {
        Goal::Pot { ball, pocket } => made(ball, pocket),
        Goal::Bank { ball, pocket, cushions } => {
            let dropped = events.iter().find_map(|e| match e {
                Event::Pocketed { time, ball: b, .. } if *b == ball => Some(*time),
                _ => None,
            });
            made(ball, pocket)
                && dropped.is_some_and(|t| {
                    let hits = events
                        .iter()
                        .filter(|e| {
                            matches!(e, Event::Cushion { ball: b, time, .. } if *b == ball && *time < t)
                        })
                        .count();
                    hits >= cushions as usize
                })
        }
        Goal::Position { ball, pocket, zone, radius } => {
            let cue = after.ball(BallId::CUE);
            made(ball, pocket) && cue.in_play() && cue.xy().distance(zone) <= radius
        }
        Goal::Escape { ball } => !report.is_foul() && report.first_contact == Some(ball),
        Goal::Streak => !report.is_foul() && report.pocketed.iter().any(|&(b, _)| b == ONE),
    }
}

/// Why a session was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refused {
    UnknownDrill,
    /// More strokes than tries, a stroke after the run ended, or one that
    /// can't be played.
    BadStroke,
    /// No session the server started for this drill (or it ran out of
    /// time): sessions are started signed in.
    NoSession,
    #[serde(other)]
    Other,
}

/// Plays a session's strokes, each try from its position. The score: the
/// tries that worked (a streak: the pots in a row, a miss ending it).
pub fn score(code: &str, strokes: &[Shot]) -> Result<u32, Refused> {
    score_session(code, 0, strokes)
}

/// [`score`] for the tries of `session` (see [`game_for_session_try`]).
pub fn score_session(code: &str, session: u64, strokes: &[Shot]) -> Result<u32, Refused> {
    let d = drill(code).ok_or(Refused::UnknownDrill)?;
    if strokes.len() > d.tries as usize {
        return Err(Refused::BadStroke);
    }
    let mut good = 0;
    for (k, shot) in strokes.iter().enumerate() {
        let g = game_for_session_try(d, session, k as u32);
        let (report, events, after) = play(&g, shot).ok_or(Refused::BadStroke)?;
        let ok = success(d, &report, &events, &after);
        good += ok as u32;
        if d.goal == Goal::Streak && !ok {
            // The streak is over: nothing may follow.
            if k + 1 != strokes.len() {
                return Err(Refused::BadStroke);
            }
            break;
        }
    }
    Ok(good)
}

/// A try played at the table (the game shows the result straight away;
/// the server's count is the one that stays).
pub fn try_shot(d: &Drill, g: &EightBallGame, shot: &Shot) -> Option<bool> {
    let (report, events, after) = play(g, shot)?;
    Some(success(d, &report, &events, &after))
}

/// A line of a drill's table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub username: String,
    #[serde(default)]
    pub country: String,
    pub score: u32,
}

/// A drill as one player sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrillInfo {
    pub code: String,
    /// Their best score (None: not tried).
    pub best: Option<u32>,
    /// The best scores, best first (earlier first on a tie).
    pub board: Vec<Entry>,
}

/// The challenge rating's table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RatingEntry {
    pub username: String,
    #[serde(default)]
    pub country: String,
    pub rating: u32,
    pub stars: u32,
}

/// Every drill for one player, with the challenge rating.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Info {
    pub drills: Vec<DrillInfo>,
    pub rating: u32,
    pub stars: u32,
    /// Their place by challenge rating (None: no drill done).
    pub rank: Option<u32>,
    pub leaders: Vec<RatingEntry>,
}

#[cfg(test)]
mod session_tests {
    use super::*;

    fn cue(g: &EightBallGame) -> DVec2 {
        g.ball(BallId::CUE).xy()
    }

    /// A session moves each try's cue ball: away from the drawn spot,
    /// differently from session to session and from try to try, never
    /// further than the jitter, and only where a ball may lie.
    #[test]
    fn sessions_move_the_cue_ball() {
        let d = drill("straight").unwrap();
        let drawn = cue(&game_for_try(d, 0));
        let a = cue(&game_for_session_try(d, 0x1234_5678_9abc, 0));
        let b = cue(&game_for_session_try(d, 0x0fed_cba9_8765, 0));
        let a2 = cue(&game_for_session_try(d, 0x1234_5678_9abc, 1));
        assert_ne!(a, drawn, "moved");
        assert_ne!(a, b, "per session");
        assert_ne!(a, a2, "per try");
        assert_eq!(
            a,
            cue(&game_for_session_try(d, 0x1234_5678_9abc, 0)),
            "the same for the same session"
        );
        let mut moved = 0;
        for d in DRILLS {
            for session in 1..40u64 {
                let g = game_for_session_try(d, session.wrapping_mul(0x9E37_79B9), 0);
                let p = cue(&g);
                let from = cue(&game_for_try(d, 0));
                assert!((p - from).abs().max_element() <= SESSION_JITTER + 1e-12, "{}", d.code);
                assert!(g.table().can_place_ball(p), "{}", d.code);
                let r = 2.0 * g.table().spec.ball_radius;
                assert!(
                    g.balls()
                        .iter()
                        .filter(|b| b.id != BallId::CUE && b.in_play())
                        .all(|b| b.xy().distance(p) >= r),
                    "{}",
                    d.code
                );
                moved += usize::from(p != from);
            }
        }
        assert!(moved > DRILLS.len() * 30, "most sessions move the ball ({moved})");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stroke straight at `to` from the cue ball of `g`.
    fn at(g: &EightBallGame, to: DVec2, speed: f64) -> Shot {
        let d = to - g.ball(BallId::CUE).xy();
        Shot::new(d.y.atan2(d.x), speed)
    }

    /// A stroke that works for try `k` of `d`, from a small search over aim,
    /// speed and spin (so no drill, and no pot of the streak, is impossible).
    fn a_way(d: &Drill, k: u32) -> Option<Shot> {
        let r = snook_legend_core::Table::default().spec.ball_radius;
        let g = game_for_try(d, k);
        let t = g.ball(ONE).xy();
        let pocket = match d.goal {
            Goal::Pot { pocket, .. } | Goal::Position { pocket, .. } => Some(pocket),
            // The streak's pocket: the nearest to the 1.
            Goal::Streak => PocketId::ALL
                .iter()
                .copied()
                .min_by(|a, b| aim(*a).distance(t).total_cmp(&aim(*b).distance(t))),
            _ => None,
        };
        for i in 0..720 {
            let yaw = match pocket {
                Some(p) => {
                    let ghost = t - (aim(p) - t).normalize() * 2.0 * r;
                    at(&g, ghost, 1.0).yaw + (i as f64 - 360.0) * 0.0002
                }
                None => i as f64 * std::f64::consts::TAU / 720.0,
            };
            for speed in [1.2, 2.0, 2.5, 3.0, 3.5, 4.1] {
                for tip_y in [-0.9, -0.6, 0.0, 0.5] {
                    let shot = Shot::new(yaw, speed).with_tip(DVec2::new(0.0, tip_y));
                    if try_shot(d, &g, &shot) == Some(true) {
                        return Some(shot);
                    }
                }
            }
        }
        None
    }

    #[test]
    fn every_drill_can_be_done() {
        for d in DRILLS {
            if d.goal == Goal::Streak {
                let strokes: Vec<Shot> = (0..d.tries)
                    .map(|k| a_way(d, k).unwrap_or_else(|| panic!("pot {k} of the streak")))
                    .collect();
                assert_eq!(score(d.code, &strokes), Ok(d.tries));
                continue;
            }
            let shot = a_way(d, 0).unwrap_or_else(|| panic!("{} can't be done", d.code));
            assert_eq!(score(d.code, &[shot, shot]), Ok(2), "{}", d.code);
        }
    }

    #[test]
    fn every_position_is_on_the_table_without_overlaps() {
        let table = snook_legend_core::Table::default();
        let r = table.spec.ball_radius;
        for d in DRILLS {
            let balls = position(d);
            for (k, (id, p)) in balls.iter().enumerate() {
                assert!(table.is_inside_playing_area(*p), "{}: {id} off the cloth", d.code);
                for (other, q) in &balls[k + 1..] {
                    assert!(p.distance(*q) > 2.0 * r, "{}: {id} and {other} overlap", d.code);
                }
            }
        }
        let mut codes: Vec<&str> = DRILLS.iter().map(|d| d.code).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), DRILLS.len(), "codes are unique");
    }

    #[test]
    fn scores_and_refusals() {
        assert_eq!(score("nope", &[]), Err(Refused::UnknownDrill));
        let d = drill("straight").unwrap();
        let g = game(d);
        let miss = at(&g, DVec2::new(-0.9, 0.45), 1.0);
        assert_eq!(score("straight", &[miss]), Ok(0));
        assert_eq!(score("straight", &[miss; 6]), Err(Refused::BadStroke));
        assert_eq!(stars(d, 0), 0);
        assert_eq!(stars(d, 4), 2);
        assert_eq!(stars(d, 5), 3);
        assert_eq!(challenge_rating(&[("straight".into(), 5), ("cut".into(), 2)]), 305 + 102);
        // A streak ends at the first miss.
        let ten = drill("ten").unwrap();
        let g = game_for_try(ten, 0);
        let away =
            at(&g, g.ball(BallId::CUE).xy() - (g.ball(ONE).xy() - g.ball(BallId::CUE).xy()), 0.8);
        assert_eq!(score("ten", &[away]), Ok(0));
        assert_eq!(score("ten", &[away, away]), Err(Refused::BadStroke));
        // Every try of the streak is a different pot.
        assert_ne!(streak_position(0), streak_position(1));
    }
}
