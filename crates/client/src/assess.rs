//! For spectators: how hard the shot being lined up is (the cut, cushions
//! and the chance a good player makes it), worked out on a worker thread
//! once the shooter's aim settles.

use std::sync::{Arc, Mutex};

use snookered_core::assess::{ShotAssessment, assess};
use snookered_core::{EightBallGame, PocketId, Shot};

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
            Some((k, t)) if *k == key => *t += dt,
            _ => self.aimed = Some((key, 0.0)),
        }
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
        let fresh = self.shown.is_some_and(|(k, _)| k == key)
            || self.running.as_ref().is_some_and(|(k, _)| *k == key);
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

    /// The answer for the stroke as it is aimed now (an older one while the
    /// aim moves would mislead).
    pub fn current(&self) -> Option<&ShotAssessment> {
        let (aimed, _) = self.aimed?;
        self.shown.as_ref().filter(|(k, _)| *k == aimed).map(|(_, a)| a)
    }
}
