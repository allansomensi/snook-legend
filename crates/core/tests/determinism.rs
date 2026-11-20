//! The server re-simulates every shot a player's computer has already shown,
//! and their verdict must be the same: the simulation has to give identical
//! bits on every platform (Linux, Windows, macOS on Intel and ARM). CI runs
//! this on all of them against one fingerprint.

use glam::DVec2;
use snook_legend_core::{BallId, EightBallGame, Event, PocketId, Rules, Shot};

struct Fnv(u64);

impl Fnv {
    fn add(&mut self, v: u64) {
        for byte in v.to_le_bytes() {
            self.0 ^= byte as u64;
            self.0 = self.0.wrapping_mul(0x100_0000_01b3);
        }
    }
    fn f(&mut self, v: f64) {
        self.add(v.to_bits());
    }
}

fn fingerprint(rules: Rules) -> u64 {
    let mut h = Fnv(0xcbf2_9ce4_8422_2325);
    for seed in 0..6u64 {
        let mut g = EightBallGame::new(seed, (seed % 2) as usize, rules);
        // Breaks with spin and elevation, then the computer plays on.
        let tip = DVec2::new((seed as f64 - 2.5) * 0.15, 0.2);
        let shot = Shot::new(0.003 * seed as f64, 6.5).with_tip(tip).with_elevation(0.05);
        let mut turns = 0;
        let mut pending = g.prepare_shot(&shot, None).ok();
        while let Some(mut p) = pending.take() {
            p.simulation.run_to_rest();
            for e in p.simulation.events() {
                h.f(e.time());
                if let Event::BallBall { a, b, speed, .. } = *e {
                    h.add(a.0 as u64 * 31 + b.0 as u64);
                    h.f(speed);
                }
            }
            let report = g.finish_shot(p);
            h.add(report.fouls.len() as u64);
            for b in g.balls() {
                h.f(b.position.x);
                h.f(b.position.y);
                h.f(b.orientation.w);
            }
            turns += 1;
            if g.game_over().is_some() || turns > 12 {
                break;
            }
            // Shots from plain numbers, so that this pins the physics and
            // the rules alone (the computer's choices are pinned in
            // ai_determinism.rs). A break's choice: the first option.
            if let Some(choice) = g.pending_choice() {
                g.choose(choice.options()[0]).expect("an option");
            }
            if g.ball_in_hand().is_some() {
                let spots =
                    (0..40).map(|k| DVec2::new(-0.9 + 0.045 * k as f64, -0.35 + 0.0175 * k as f64));
                for p in spots {
                    if g.place_cue_ball(p) {
                        break;
                    }
                }
            }
            let k = seed * 13 + turns;
            let shot = Shot::new(0.37 * k as f64, 1.0 + (k % 6) as f64)
                .with_tip(DVec2::new(((k % 5) as f64 - 2.0) * 0.2, ((k % 3) as f64 - 1.0) * 0.4));
            let called = (g.needs_called_pocket() || g.calls_shots()).then_some(PocketId::FootLeft);
            pending = g.prepare_shot(&shot, called).ok();
        }
        h.f(g.ball(BallId::CUE).position.x);
    }
    h.0
}

#[test]
fn simulation_is_bit_identical_everywhere() {
    let got: Vec<(Rules, u64)> =
        EXPECTED.iter().map(|(rules, _)| (*rules, fingerprint(*rules))).collect();
    for (rules, value) in &got {
        println!("{rules}: fingerprint {value:#x}");
    }
    for ((rules, got), (_, expected)) in got.iter().zip(EXPECTED) {
        assert_eq!(*got, expected, "the physics gave different bits on this platform ({rules})");
    }
}

/// Update only on purpose (a change to the physics or the rules), from the
/// values printed above.
const EXPECTED: [(Rules, u64); 2] =
    [(Rules::Brazilian, 0x65f6_75cb_2fbd_67f2), (Rules::Official, 0xcb10_2195_dd34_1769)];
