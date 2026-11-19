//! How forgiving the pockets play: for paths at several angles from straight
//! in, the width of the band that still drops. `cargo run --release -p snook_legend_core --example pocket_angles`
use glam::DVec2;
use snook_legend_core::{
    Ball, BallId, BallStatus, PhysicsParams, PocketId, Shot, Simulation, Table, TableSpec,
};

/// The width (mm) of the band of paths, at `deg` from straight in, that
/// still drop at `speed`.
fn width(spec: TableSpec, params: PhysicsParams, pocket: PocketId, deg: f64, speed: f64) -> f64 {
    let table = Table::new(spec);
    let p = *table.pocket(pocket);
    let aim = p.aim_point();
    let a = deg.to_radians();
    let inward = -p.outward;
    let dir = DVec2::new(
        inward.x * a.cos() - inward.y * a.sin(),
        inward.x * a.sin() + inward.y * a.cos(),
    );
    let side = DVec2::new(-dir.y, dir.x);
    let mut n = 0;
    for k in -60..=60 {
        let off = k as f64 * 0.002;
        let start = aim + dir * 0.6 + side * off;
        if !table.is_inside_playing_area(start) {
            continue;
        }
        let ball = Ball::at_rest(BallId::CUE, start, spec.ball_radius);
        let mut sim = Simulation::new(table.clone(), params, vec![ball]);
        let yaw = (-dir).y.atan2((-dir).x);
        sim.strike(BallId::CUE, &Shot::new(yaw, speed)).unwrap();
        sim.run_to_rest();
        if matches!(sim.balls[0].status, BallStatus::Pocketed(_)) {
            n += 1;
        }
    }
    n as f64 * 2.0
}

fn main() {
    let rows: Vec<(String, TableSpec, PhysicsParams)> =
        vec![("WPA 8-foot".into(), TableSpec::default(), PhysicsParams::default())];
    for (name, spec, params) in rows {
        let s: Vec<String> = [0.0, 20.0, 35.0, 45.0]
            .iter()
            .map(|d| {
                format!(
                    "{d:>2}°: side {:>3} corner {:>3}",
                    width(spec, params, PocketId::SideLeft, *d, 1.5),
                    width(spec, params, PocketId::FootLeft, *d, 1.5)
                )
            })
            .collect();
        println!("{name:<20} {}", s.join(" | "));
    }
}
