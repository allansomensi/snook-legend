//! Random shots must never leave the table in an impossible state.

use glam::DVec2;
use snook_legend_core::rng::Rng;
use snook_legend_core::{EightBallGame, PocketId, Rules, Shot};

#[test]
fn random_shots_keep_the_table_physical() {
    let mut rng = Rng::new(7);
    let mut g = EightBallGame::new(3, 0, Rules::Brazilian);
    for i in 0..400 {
        if g.game_over().is_some() {
            g = EightBallGame::new(i, (i % 2) as usize, Rules::Brazilian);
        }
        if g.ball_in_hand().is_some() {
            for _ in 0..200 {
                let p = DVec2::new(rng.next_f64() * 1.9 - 0.95, rng.next_f64() * 0.9 - 0.45);
                if g.place_cue_ball(p) {
                    break;
                }
            }
        }
        let shot = Shot {
            yaw: rng.next_f64() * std::f64::consts::TAU,
            elevation: if rng.below(4) == 0 { rng.next_f64() * 1.4 } else { 0.0 },
            tip: DVec2::new(rng.next_f64() * 2.0 - 1.0, rng.next_f64() * 2.0 - 1.0),
            speed: rng.next_f64() * 8.0,
        };
        let called = g.needs_called_pocket().then(|| PocketId::ALL[rng.below(6)]);
        g.play_shot(&shot, called).expect("legal shot");
        // Every snapshot a game writes is read back as the same game (the
        // checks made when one is read refuse none of them).
        let json = serde_json::to_string(&g).expect("written");
        let back: EightBallGame = serde_json::from_str(&json).expect("read back");
        assert_eq!(back, g, "shot {i}: a snapshot changed on the way");

        let t = g.table();
        let r = t.spec.ball_radius;
        let dh = t.spec.cushion_height - r;
        let contact = (r * r - dh * dh).sqrt();
        let live: Vec<_> = g.balls().iter().filter(|b| b.in_play()).collect();
        for b in &live {
            assert!(b.position.is_finite(), "shot {i}: {:?} non-finite", b.id);
            assert!(!b.is_moving(), "shot {i}: {:?} still moving", b.id);
            assert!((b.position.z - r).abs() < 1e-6, "shot {i}: {:?} not on the slate", b.id);
            let nearest = t
                .cushions
                .iter()
                .map(|c| c.closest_point(b.xy()).distance(b.xy()))
                .fold(f64::MAX, f64::min);
            assert!(nearest > contact - 5e-4, "shot {i}: {:?} inside a cushion", b.id);
        }
        for a in 0..live.len() {
            for c in a + 1..live.len() {
                let d = live[a].xy().distance(live[c].xy());
                assert!(
                    d > 2.0 * r - 1e-5,
                    "shot {i}: {:?} and {:?} overlap",
                    live[a].id,
                    live[c].id
                );
            }
        }
    }
}

/// The same under the official rules (the cue ball in hand anywhere after
/// a foul, the choices after the break), where a ball that jumps onto a
/// rail stays there (off the table) and the cue ball in hand may go against
/// a cushion: every ball in play rests on the bed of the table or in a
/// pocket's mouth, never in a cushion or out on a rail.
#[test]
fn random_shots_keep_the_table_physical_under_the_official_rules() {
    let mut rng = Rng::new(11);
    let mut g = EightBallGame::new(3, 0, Rules::Official);
    for i in 0..400 {
        if g.game_over().is_some() {
            g = EightBallGame::new(i, (i % 2) as usize, Rules::Official);
        }
        if let Some(choice) = g.pending_choice() {
            let options = choice.options();
            g.choose(options[rng.below(options.len())]).expect("an option");
        }
        if g.ball_in_hand().is_some() {
            for _ in 0..200 {
                let p = DVec2::new(rng.next_f64() * 2.4 - 1.2, rng.next_f64() * 1.2 - 0.6);
                if g.place_cue_ball(p) {
                    break;
                }
            }
        }
        let shot = Shot {
            yaw: rng.next_f64() * std::f64::consts::TAU,
            elevation: if rng.below(3) == 0 { rng.next_f64() * 1.4 } else { 0.0 },
            tip: DVec2::new(rng.next_f64() * 2.0 - 1.0, rng.next_f64() * 2.0 - 1.0),
            speed: rng.next_f64() * 8.0,
        };
        let called = (g.calls_shots() && rng.below(4) > 0).then(|| PocketId::ALL[rng.below(6)]);
        g.play_shot(&shot, called).expect("legal shot");
        // Every snapshot a game writes is read back as the same game (the
        // checks made when one is read refuse none of them).
        let json = serde_json::to_string(&g).expect("written");
        let back: EightBallGame = serde_json::from_str(&json).expect("read back");
        assert_eq!(back, g, "shot {i}: a snapshot changed on the way");

        let t = g.table();
        let r = t.spec.ball_radius;
        let (hl, hw) = (t.spec.length / 2.0, t.spec.width / 2.0);
        let contact = t.cushion_contact();
        let live: Vec<_> = g.balls().iter().filter(|b| b.in_play()).collect();
        for b in &live {
            let p = b.xy();
            assert!(b.position.is_finite(), "shot {i}: {:?} non-finite", b.id);
            assert!(!b.is_moving(), "shot {i}: {:?} still moving", b.id);
            assert!((b.position.z - r).abs() < 1e-6, "shot {i}: {:?} not on the slate", b.id);
            let nearest =
                t.cushions.iter().map(|c| c.closest_point(p).distance(p)).fold(f64::MAX, f64::min);
            assert!(nearest > contact - 5e-4, "shot {i}: {:?} inside a cushion", b.id);
            let on_the_bed = p.x.abs() <= hl && p.y.abs() <= hw;
            let in_a_mouth = t.pockets.iter().any(|k| k.in_mouth(p));
            assert!(on_the_bed || in_a_mouth, "shot {i}: {:?} out on a rail at {p:?}", b.id);
        }
        for a in 0..live.len() {
            for c in a + 1..live.len() {
                let d = live[a].xy().distance(live[c].xy());
                assert!(
                    d > 2.0 * r - 1e-5,
                    "shot {i}: {:?} and {:?} overlap",
                    live[a].id,
                    live[c].id
                );
            }
        }
    }
}
