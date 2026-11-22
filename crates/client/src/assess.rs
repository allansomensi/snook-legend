//! For spectators: how hard the shot being lined up is (the cut, cushions
//! and the chance a good player makes it), worked out on a worker thread
//! once the shooter's aim settles.

use std::sync::{Arc, Mutex};

use snook_legend_core::assess::{ShotAssessment, assess};
use snook_legend_core::{EightBallGame, PocketId, Shot};

/// The aim must stay put this long before it is assessed (seconds).
const SETTLE: f32 = 0.35;
/// Imperfect strokes tried for the chance.
const SAMPLES: usize = 24;

/// A stroke, rounded so small jitters of the aim don't start a new
/// assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Key {
    turn: u32,
    yaw: i64,
    speed: i64,
    tip: (i64, i64),
    elevation: i64,
    called: Option<PocketId>,
}

impl Key {
    fn of(turn: u32, shot: &Shot, called: Option<PocketId>) -> Key {
        let q = |x: f64, step: f64| (x / step).round() as i64;
        Key {
            turn,
            yaw: q(shot.yaw, 0.0015),
            speed: q(shot.speed, 0.08),
            tip: (q(shot.tip.x, 0.05), q(shot.tip.y, 0.05)),
            elevation: q(shot.elevation, 0.02),
            called,
        }
    }
}

impl Key {
    /// The same stroke, give or take a step of the rounding: the aim that
    /// arrives over the network wavers on a rounding boundary, and must not
    /// count as a new stroke each time it does.
    fn near(&self, other: &Key) -> bool {
        let close = |a: i64, b: i64| a.abs_diff(b) <= 1;
        self.turn == other.turn
            && self.called == other.called
            && close(self.yaw, other.yaw)
            && close(self.speed, other.speed)
            && close(self.tip.0, other.tip.0)
            && close(self.tip.1, other.tip.1)
            && close(self.elevation, other.elevation)
    }
}

/// Where a worker thread leaves its answer.
type Slot = Arc<Mutex<Option<Option<ShotAssessment>>>>;

#[derive(Default)]
pub struct Assessor {
    /// The stroke being aimed and how long it has stayed.
    aimed: Option<(Key, f32)>,
    /// The stroke on the worker thread, and where its answer lands.
    running: Option<(Key, Slot)>,
    /// The latest answer and the stroke it is for.
    shown: Option<(Key, ShotAssessment)>,
}

impl Assessor {
    /// Call every frame while watching a shot being lined up.
    pub fn update(
        &mut self,
        game: &EightBallGame,
        turn: u32,
        shot: Shot,
        called: Option<PocketId>,
        dt: f32,
    ) {
        let key = Key::of(turn, &shot, called);
        // A new turn: the old answer is about another position.
        if self.shown.is_some_and(|(k, _)| k.turn != turn) {
            self.shown = None;
        }
        match &mut self.aimed {
            Some((k, t)) if k.near(&key) => *t += dt,
            _ => self.aimed = Some((key, 0.0)),
        }
        // The stroke as it settled (a waver doesn't move it).
        let key = self.aimed.map_or(key, |(k, _)| k);
        // An answer came back.
        if let Some((k, slot)) = &self.running
            && let Some(answer) = slot.lock().ok().and_then(|mut s| s.take())
        {
            let k = *k;
            self.running = None;
            match answer {
                Some(a) if k.turn == turn => self.shown = Some((k, a)),
                _ => self.shown = None,
            }
        }
        let settled = self.aimed.is_some_and(|(_, t)| t >= SETTLE);
        let fresh = self.shown.is_some_and(|(k, _)| k.near(&key))
            || self.running.as_ref().is_some_and(|(k, _)| k.near(&key));
        if settled && !fresh && self.running.is_none() {
            let slot = Arc::new(Mutex::new(None));
            let out = slot.clone();
            let game = game.clone();
            let seed = (key.yaw as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ turn as u64;
            std::thread::spawn(move || {
                let a = assess(&game, &shot, called, SAMPLES, seed);
                if let Ok(mut s) = out.lock() {
                    *s = Some(a);
                }
            });
            self.running = Some((key, slot));
        }
    }

    /// Nothing being lined up (the balls roll, or it isn't a turn).
    pub fn clear(&mut self) {
        self.aimed = None;
        self.shown = None;
    }

    /// The latest answer for this turn, and whether it is for the stroke as
    /// it is aimed now (false while the aim moves and a new one is worked
    /// out: it is shown dimmed meanwhile, rather than blinking off and on).
    pub fn latest(&self) -> Option<(ShotAssessment, bool)> {
        let (aimed, _) = self.aimed?;
        self.shown.map(|(k, a)| (a, k.near(&aimed)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(chance: f64) -> ShotAssessment {
        ShotAssessment { first_contact: None, cut_deg: Some(20.0), pot: None, cushions: 0, chance }
    }

    fn shot(yaw: f64) -> Shot {
        Shot { yaw, speed: 3.0, tip: Default::default(), elevation: 0.0 }
    }

    /// The aim wavers on a rounding boundary (as the remote aim does): the
    /// answer stays shown, and stays current.
    #[test]
    fn a_wavering_aim_keeps_its_answer() {
        let game = EightBallGame::new(1, 0, Default::default());
        let mut a = Assessor::default();
        let k = Key::of(3, &shot(1.0), None);
        a.aimed = Some((k, 1.0));
        a.shown = Some((k, answer(0.7)));
        for yaw in [1.0007, 0.9993, 1.0008, 0.9992, 1.0] {
            a.update(&game, 3, shot(yaw), None, 0.05);
            let (got, fresh) = a.latest().expect("still shown");
            assert_eq!(got.chance, 0.7);
            assert!(fresh, "yaw {yaw}");
        }
    }

    /// A new aim keeps the last answer on screen, marked as not current,
    /// until the new one comes; a new turn drops it.
    #[test]
    fn a_new_aim_shows_the_last_answer_until_the_next() {
        let game = EightBallGame::new(1, 0, Default::default());
        let mut a = Assessor::default();
        let k = Key::of(3, &shot(1.0), None);
        a.aimed = Some((k, 1.0));
        a.shown = Some((k, answer(0.4)));
        a.update(&game, 3, shot(1.2), None, 0.05);
        let (got, fresh) = a.latest().expect("kept while the aim moves");
        assert_eq!(got.chance, 0.4);
        assert!(!fresh);
        a.update(&game, 4, shot(1.2), None, 0.05);
        assert!(a.latest().is_none());
    }
}
