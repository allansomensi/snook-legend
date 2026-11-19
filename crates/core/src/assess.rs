//! How hard a shot is, for people watching: what the stroke as aimed does
//! (which ball it pots, how thin the cut, off how many cushions) and how
//! often a good player would make it, struck a little imperfectly.
//!
//! Only for display: it plays the shot on copies of the game.

use glam::DVec2;

use crate::rng::Rng;
use crate::{BallId, EightBallGame, Event, PocketId, Shot};

/// Aim error of a good club player (degrees, one standard deviation).
const AIM_NOISE_DEG: f64 = 0.3;
/// Speed error (a fraction of the speed, one standard deviation).
const SPEED_NOISE: f64 = 0.04;
/// Tip error (a fraction of the tip's radius, one standard deviation).
const TIP_NOISE: f64 = 0.04;

/// What a shot as aimed does, and how likely it is to work.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotAssessment {
    /// The first ball the cue ball hits, struck exactly as aimed.
    pub first_contact: Option<BallId>,
    /// The cut on that ball (0: full ball; near 90: the thinnest touch).
    pub cut_deg: Option<f64>,
    /// The object ball made legally for the shooter (one of their own, or
    /// the 8 that wins), struck exactly as aimed, and its pocket. One of
    /// the opponent's balls going down is not a pot: it passes the turn.
    pub pot: Option<(BallId, PocketId)>,
    /// Cushions that ball touched on its way (a bank: one or more).
    pub cushions: u32,
    /// How often the shooter keeps the table (or wins) when the stroke is
    /// struck with a good player's small errors (0 to 1).
    pub chance: f64,
}

/// Assesses `shot` from the position in `game` (None: on the break, with
/// the game over or when the shot can't be played as it is). `samples`
/// imperfect strokes estimate the chance.
pub fn assess(
    game: &EightBallGame,
    shot: &Shot,
    called: Option<PocketId>,
    samples: usize,
    seed: u64,
) -> Option<ShotAssessment> {
    if game.is_break() || game.game_over().is_some() {
        return None;
    }
    let me = game.current_player();
    let exact = game.prepare_shot(shot, called).ok()?;
    let mut trial = game.clone();
    let mut sim = exact.simulation;
    sim.run_to_rest();
    let events = sim.events().to_vec();
    let report = trial.finish_shot(crate::PendingShot { simulation: sim, ..exact });
    let first_contact = report.first_contact;
    let pot = report
        .balls_made()
        .first()
        .and_then(|made| report.pocketed.iter().find(|(b, _)| b == made).copied());
    let cushions = pot.map_or(0, |(ball, _)| {
        let dropped = events
            .iter()
            .find_map(|e| match e {
                Event::Pocketed { time, ball: b, .. } if *b == ball => Some(*time),
                _ => None,
            })
            .unwrap_or(f64::MAX);
        events
            .iter()
            .filter(|e| matches!(e, Event::Cushion { ball: b, time, .. } if *b == ball && *time < dropped))
            .count() as u32
    });
    let cut_deg = first_contact.and_then(|b| cut_angle(game, shot.yaw, b));

    let mut rng = Rng::new(seed);
    let mut made = 0usize;
    for _ in 0..samples {
        let mut s = *shot;
        s.yaw += rng.gaussian() * AIM_NOISE_DEG.to_radians();
        s.speed = (s.speed * (1.0 + rng.gaussian() * SPEED_NOISE)).max(0.0);
        s.tip += DVec2::new(rng.gaussian(), rng.gaussian()) * TIP_NOISE;
        if s.tip.length() > 1.0 {
            s.tip = s.tip.normalize();
        }
        let mut g = game.clone();
        let Ok(r) = g.play_shot(&s, called) else { continue };
        let won = r.game_over.is_some_and(|o| o.winner == me);
        if won || (r.continues && r.game_over.is_none()) {
            made += 1;
        }
    }
    let chance = if samples == 0 { 0.0 } else { made as f64 / samples as f64 };
    Some(ShotAssessment { first_contact, cut_deg, pot, cushions, chance })
}

/// The cut on `target` for a cue ball sent along `yaw` in a straight line
/// (None if that line misses it).
fn cut_angle(game: &EightBallGame, yaw: f64, target: BallId) -> Option<f64> {
    let r = game.table().spec.ball_radius;
    let cue = game.ball(BallId::CUE).xy();
    let t = game.ball(target).xy();
    let dir = DVec2::new(crate::math::cos(yaw), crate::math::sin(yaw));
    // Where along the line the cue ball touches the target: |cue + d·dir − t| = 2r.
    let rel = cue - t;
    let b = rel.dot(dir);
    let c = rel.length_squared() - 4.0 * r * r;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let d = -b - disc.sqrt();
    if d < 0.0 {
        return None;
    }
    let contact = cue + dir * d;
    let line = (t - contact).normalize_or_zero();
    let cos = dir.dot(line).clamp(-1.0, 1.0);
    Some(cos.acos().to_degrees())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A straight-in shot at a ball near a pocket: full ball, made almost
    /// every time; the same ball cut thin from far is harder.
    #[test]
    fn straight_shots_are_easier_than_thin_cuts() {
        let mut game = EightBallGame::new(1, 0, crate::Rules::Brazilian);
        let r = game.table().spec.ball_radius;
        let aim = game.table().pocket(PocketId::FootRight).aim_point();
        let solid = BallId(1);
        let ball = DVec2::new(0.75, -0.30);
        let eight = (BallId(8), DVec2::new(-0.7, 0.35));
        let back = (ball - aim).normalize();
        // Straight in from half a metre.
        let cue = ball + back * 0.5;
        game.set_up_test_position(&[(BallId::CUE, cue), (solid, ball), eight]);
        let to = (ball - cue).normalize();
        let a = assess(&game, &Shot::new(to.y.atan2(to.x), 2.0), None, 24, 1).expect("assessed");
        assert_eq!(a.first_contact, Some(solid));
        assert!(a.cut_deg.unwrap() < 2.0, "{a:?}");
        assert_eq!(a.pot, Some((solid, PocketId::FootRight)), "{a:?}");
        assert_eq!(a.cushions, 0);
        assert!(a.chance > 0.8, "{a:?}");

        // A cut of about 45 degrees from the far end of the table.
        let cue = DVec2::new(-0.6, -0.45);
        game.set_up_test_position(&[(BallId::CUE, cue), (solid, ball), eight]);
        let ghost = ball + back * 2.0 * r;
        let to = (ghost - cue).normalize();
        let b = assess(&game, &Shot::new(to.y.atan2(to.x), 3.0), None, 24, 1).expect("assessed");
        assert!((35.0..60.0).contains(&b.cut_deg.unwrap()), "{b:?}");
        assert!(b.chance < a.chance, "{a:?} {b:?}");
    }

    /// A combination that sends the opponent's ball in is no pot for the
    /// shooter: it passes the turn. On an open table the same ball gives
    /// them its group, and is theirs.
    #[test]
    fn the_opponents_ball_is_not_a_pot() {
        let mut game = EightBallGame::new(1, 0, crate::Rules::Brazilian);
        let r = game.table().spec.ball_radius;
        let aim = game.table().pocket(PocketId::FootRight).aim_point();
        let back = (DVec2::ZERO - aim).normalize();
        let stripe = aim + back * 0.15;
        let solid = stripe + back * (2.0 * r + 0.02);
        let cue = solid + back * 0.15;
        game.set_up_test_position(&[
            (BallId::CUE, cue),
            (BallId(1), solid),
            (BallId(12), stripe),
            (BallId(8), DVec2::new(-0.7, 0.35)),
        ]);
        let to = (solid - cue).normalize();
        let shot = Shot::new(to.y.atan2(to.x), 2.0);
        let open = assess(&game, &shot, None, 0, 1).expect("assessed");
        assert_eq!(open.first_contact, Some(BallId(1)));
        assert_eq!(open.pot, Some((BallId(12), PocketId::FootRight)), "{open:?}");
        // Player 0 on solids: the 12 is the opponent's.
        let mut v = serde_json::to_value(&game).unwrap();
        v["groups"] = serde_json::json!(["Solids", "Stripes"]);
        let solids: EightBallGame = serde_json::from_value(v).unwrap();
        let mut trial = solids.clone();
        let report = trial.play_shot(&shot, None).unwrap();
        assert!(!report.is_foul() && report.pocketed.iter().any(|(b, _)| b.0 == 12), "{report:?}");
        assert_eq!(assess(&solids, &shot, None, 0, 1).expect("assessed").pot, None);
    }

    #[test]
    fn no_assessment_on_the_break() {
        let game = EightBallGame::new(1, 0, crate::Rules::Brazilian);
        assert!(assess(&game, &Shot::new(0.0, 6.0), None, 4, 1).is_none());
    }
}
