//! Numbers of the break and of plain strokes, to compare with a real table.
//! `cargo run --release -p snook_legend_core --example break_stats`.
use glam::DVec2;
use snook_legend_core::{
    Ball, BallId, EightBallGame, Event, Motion, PhysicsParams, RULESET, Shot, Simulation, Table,
};

fn main() {
    for ruleset in 1..=RULESET {
        let params = PhysicsParams::of_ruleset(ruleset);
        println!("== ruleset {ruleset}");
        for power in [1.0f64, 0.8] {
            let (mut t_rest, mut cushions, mut hops, mut maxz, mut potted, mut t90) =
                (0.0, 0.0, 0.0, 0.0f64, 0.0, 0.0);
            let n = 20;
            for seed in 0..n {
                let g = EightBallGame::with_ruleset(seed, 0, ruleset);
                let shot = Shot::new(g.default_aim(), params.max_cue_speed * power);
                let mut p = g.prepare_shot(&shot, None).unwrap();
                let r = p.simulation.table.spec.ball_radius;
                let mut last_fast = 0.0;
                while !p.simulation.is_settled() && p.simulation.time() < 60.0 {
                    p.simulation.step();
                    for b in &p.simulation.balls {
                        maxz = maxz.max(b.position.z - r);
                        if b.is_moving() && b.velocity.length() > 0.3 {
                            last_fast = p.simulation.time();
                        }
                    }
                }
                t_rest += p.simulation.time();
                t90 += last_fast;
                for e in p.simulation.events() {
                    match e {
                        Event::Cushion { .. } => cushions += 1.0,
                        Event::Slate { .. } => hops += 1.0,
                        Event::Pocketed { .. } => potted += 1.0,
                        _ => {}
                    }
                }
            }
            let n = n as f64;
            println!(
                "break power {power}: rest {:.1}s, last ball >0.3m/s at {:.1}s, cushion hits {:.0}, slate bounces {:.1}, max hop {:.0}mm, potted {:.2}",
                t_rest / n,
                t90 / n,
                cushions / n,
                hops / n,
                maxz * 1000.0,
                potted / n
            );
        }
        // One ball up and down the table.
        let table = Table::of_ruleset(ruleset);
        let r = table.spec.ball_radius;
        for speed in [1.0, 2.0, 4.0, 7.0] {
            let ball = Ball::at_rest(BallId::CUE, DVec2::new(-table.spec.length / 4.0, 0.0), r);
            let mut sim = Simulation::new(table.clone(), params, vec![ball]);
            sim.strike(BallId::CUE, &Shot::new(0.0, speed)).unwrap();
            let v0 = sim.balls[0].velocity.length();
            let mut dist = 0.0;
            let mut ratios = vec![];
            while !sim.is_settled() {
                let before = sim.balls[0].velocity.x;
                sim.step();
                let b = &sim.balls[0];
                dist += b.velocity.length() * params.time_step;
                if before.signum() != b.velocity.x.signum() && b.velocity.x != 0.0 && before != 0.0
                {
                    ratios.push((before, 0.0));
                }
                // Speed once rolling again after a cushion.
                if let Some(l) = ratios.last_mut()
                    && l.1 == 0.0
                    && b.motion == Motion::Rolling
                {
                    l.1 = b.velocity.x;
                }
            }
            let rs: Vec<String> = ratios
                .iter()
                .map(|(a, b)| format!("{:.2}->{:.2}({:.2})", a.abs(), b.abs(), (b / a).abs()))
                .collect();
            println!(
                "cue {speed} m/s: ball {v0:.2} m/s, travels {:.1} lengths in {:.1}s; cushions {}",
                dist / table.spec.length,
                sim.time(),
                rs.join(" ")
            );
        }
    }
}
