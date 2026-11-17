//! The effects of spin and of the cue's elevation, as numbers to compare
//! with a real table. `cargo run --release -p snook_legend_core --example spin_stats`.
use glam::DVec2;
use snook_legend_core::physics::cue_impulse;
use snook_legend_core::{Ball, BallId, Event, PhysicsParams, Shot, Simulation, Table};

fn sim(balls: &[(u8, DVec2)]) -> Simulation {
    let table = Table::default();
    let r = table.spec.ball_radius;
    let balls = balls.iter().map(|&(n, p)| Ball::at_rest(BallId(n), p, r)).collect();
    Simulation::new(table, PhysicsParams::default(), balls)
}
fn deg(v: DVec2) -> f64 {
    v.y.atan2(v.x).to_degrees()
}

fn main() {
    let p = PhysicsParams::default();
    let t = Table::default();
    let r = t.spec.ball_radius;
    // 1. Spin given by the tip.
    for (name, tip) in [("centre", DVec2::ZERO), ("max top", DVec2::Y), ("max side", DVec2::X)] {
        let (v, w) = cue_impulse(&p, &t, &Shot::new(0.0, 2.0).with_tip(tip));
        println!(
            "tip {name}: ball {:.2} m/s, spin ratio Rw/v {:.2}, direction {:.2}°",
            v.length(),
            w.length() * r / v.length(),
            deg(v.truncate())
        );
    }
    // 2. Cue ball deflection off a half-ball hit: stun, rolling, draw.
    for (name, tipy, dist, speed) in [
        ("stun (close, firm)", 0.0, 0.15, 3.0),
        ("rolling", 0.0, 1.0, 1.0),
        ("rolling 3/4-ball", 0.0, 1.0, 1.0),
        ("draw", -1.0, 0.3, 2.5),
    ] {
        let off = if name.contains("3/4") { r * 0.5 } else { r };
        let mut s = sim(&[(0, DVec2::new(-dist, off)), (1, DVec2::ZERO)]);
        s.strike(BallId::CUE, &Shot::new(0.0, speed).with_tip(DVec2::new(0.0, tipy))).unwrap();
        while !s.events().iter().any(|e| matches!(e, Event::BallBall { .. })) && s.time() < 20.0 {
            s.step();
        }
        let first = deg(s.balls[0].velocity.truncate());
        let ob = deg(s.balls[1].velocity.truncate());
        let hit = s.balls[0].xy();
        for _ in 0..1500 {
            s.step();
        }
        let fin = deg(s.balls[0].xy() - hit);
        let _ = fin;
        let v = s.balls[0].velocity.truncate();
        println!(
            "half-ball {name}: cue leaves at {first:.1}°, settles to {:.1}°; object ball {ob:.1}° (line of centres {:.1}°)",
            deg(v),
            -(off / (2.0 * r)).asin().to_degrees()
        );
    }
    // 3. Draw and follow distances, straight in.
    for (name, tipy, speed) in [
        ("max draw", -1.0, 2.5),
        ("max draw soft", -1.0, 1.2),
        ("max follow", 1.0, 2.5),
        ("stop (centre, firm)", 0.0, 3.0),
    ] {
        let mut s = sim(&[(0, DVec2::new(-0.5, 0.3)), (1, DVec2::new(0.0, 0.3))]);
        s.strike(BallId::CUE, &Shot::new(0.0, speed).with_tip(DVec2::new(0.0, tipy))).unwrap();
        s.run_to_rest();
        println!(
            "straight in from 0.5 m, cue {speed} m/s, {name}: cue ball ends {:+.2} m from the contact point",
            s.balls[0].position.x + 2.0 * r
        );
    }
    // 4. Side spin off a cushion, straight in.
    for (speed, side) in [(1.0, 1.0), (2.0, 1.0), (3.5, 1.0), (2.0, 0.5)] {
        let mut s = sim(&[(0, DVec2::new(0.5, 0.0))]);
        s.strike(
            BallId::CUE,
            &Shot::new(std::f64::consts::FRAC_PI_2, speed).with_tip(DVec2::new(side, 0.0)),
        )
        .unwrap();
        while !s.events().iter().any(|e| matches!(e, Event::Cushion { .. })) && s.time() < 20.0 {
            s.step();
        }
        for _ in 0..300 {
            s.step();
        }
        let v = s.balls[0].velocity.truncate();
        println!(
            "into a cushion square, cue {speed} m/s, side {side}: comes off {:.1}° from square",
            (v.x / -v.y).atan().to_degrees()
        );
    }
    // 5. Elevation: swerve, massé, jump.
    for (name, elev, side, speed) in [
        ("level", 0.0, 1.0, 2.0),
        ("5°", 5.0, 1.0, 2.0),
        ("15°", 15.0, 1.0, 2.0),
        ("30°", 30.0, 1.0, 2.5),
        ("massé 70°", 70.0, 1.0, 3.0),
        ("massé 70° left", 70.0, -1.0, 3.0),
    ] {
        let mut s = sim(&[(0, DVec2::new(-0.9, 0.0))]);
        s.strike(
            BallId::CUE,
            &Shot::new(0.0, speed)
                .with_tip(DVec2::new(side, 0.0))
                .with_elevation(f64::to_radians(elev)),
        )
        .unwrap();
        let d0 = deg(s.balls[0].velocity.truncate());
        let mut maxz = 0.0f64;
        let mut y_at_1m = f64::NAN;
        while !s.is_settled() {
            s.step();
            maxz = maxz.max(s.balls[0].position.z - r);
            if y_at_1m.is_nan() && s.balls[0].position.x > 0.1 {
                y_at_1m = s.balls[0].position.y;
            }
            if s.events().iter().any(|e| matches!(e, Event::Cushion { .. })) {
                break;
            }
        }
        let b = &s.balls[0];
        println!(
            "side {side:+}, elevation {name}, cue {speed}: leaves at {d0:.2}°, y after 1 m {:.1} mm, heading then {:.1}°, hop {:.0} mm, at ({:.2},{:.2})",
            y_at_1m * 1000.0,
            deg(b.velocity.truncate()),
            maxz * 1000.0,
            b.position.x,
            b.position.y
        );
    }
    for (elev, speed) in [(20.0f64, 3.0), (35.0, 3.5), (45.0, 4.0), (60.0, 4.0)] {
        let mut s = sim(&[(0, DVec2::new(-0.9, 0.0))]);
        s.strike(BallId::CUE, &Shot::new(0.0, speed).with_elevation(elev.to_radians())).unwrap();
        let (mut maxz, mut land) = (0.0f64, f64::NAN);
        while !s.is_settled() {
            s.step();
            maxz = maxz.max(s.balls[0].position.z - r);
            if land.is_nan() && maxz > 0.001 && s.balls[0].position.z - r < 1e-6 {
                land = s.balls[0].position.x + 0.9;
            }
        }
        println!(
            "jump: elevation {elev}°, cue {speed}: clears {:.0} mm, first flight {:.2} m",
            maxz * 1000.0,
            land
        );
    }
    // 6. Throw: a slow stun cut, and side spin on a straight shot.
    for (name, off, side, speed) in [
        ("half-ball stun, slow", r, 0.0, 0.8),
        ("half-ball stun, fast", r, 0.0, 3.5),
        ("straight, max side, slow", 0.0, 1.0, 0.8),
        ("straight, max side, fast", 0.0, 1.0, 3.5),
    ] {
        let mut s = sim(&[(0, DVec2::new(-0.12, off)), (1, DVec2::ZERO)]);
        s.strike(BallId::CUE, &Shot::new(0.0, speed).with_tip(DVec2::new(side, 0.0))).unwrap();
        while !s.events().iter().any(|e| matches!(e, Event::BallBall { .. })) && s.time() < 20.0 {
            s.step();
        }
        let c = s.balls[0].xy();
        let line = deg(-c);
        let ob = deg(s.balls[1].velocity.truncate());
        println!("throw, {name}: {:.2}° off the line of centres", ob - line);
    }
}
