//! How the pockets play: the band of paths that still drop, measured with
//! the real physics. Guards against a change that would close the side
//! pockets again (players found them too tight).

use glam::DVec2;
use snookered_core::{
    Ball, BallId, BallStatus, PhysicsParams, PocketId, RULESET, Shot, Simulation, Table, TableSpec,
};

/// The width (mm) of the band of paths, `deg` from straight in, that drop
/// at `speed` (m/s).
fn width(ruleset: u8, pocket: PocketId, deg: f64, speed: f64) -> f64 {
    let spec = TableSpec::of_ruleset(ruleset);
    let params = PhysicsParams::of_ruleset(ruleset);
    let table = Table::new(spec);
    let p = table.pocket(pocket).clone();
    let aim = p.aim_point();
    let a = deg.to_radians();
    let inward = -p.outward;
    let dir = DVec2::new(
        inward.x * a.cos() - inward.y * a.sin(),
        inward.x * a.sin() + inward.y * a.cos(),
    );
    let across = DVec2::new(-dir.y, dir.x);
    let dropped = (-60..=60)
        .filter(|k| {
            let start = aim + dir * 0.6 + across * (*k as f64 * 0.002);
            if !table.is_inside_playing_area(start) {
                return false;
            }
            let ball = Ball::at_rest(BallId::CUE, start, spec.ball_radius);
            let mut sim = Simulation::new(table.clone(), params, vec![ball]);
            sim.strike(BallId::CUE, &Shot::new((-dir).y.atan2((-dir).x), speed)).unwrap();
            sim.run_to_rest();
            matches!(sim.balls[0].status, BallStatus::Pocketed(_))
        })
        .count();
    dropped as f64 * 2.0
}

#[test]
fn side_pockets_forgive_more_than_the_nine_foot_table() {
    for deg in [0.0, 20.0, 35.0] {
        let now = width(RULESET, PocketId::SideLeft, deg, 1.5);
        let before = width(1, PocketId::SideLeft, deg, 1.5);
        assert!(now >= 110.0, "{deg}°: {now} mm");
        assert!(now > before + 15.0, "{deg}°: {now} mm (ruleset 1: {before} mm)");
    }
    // Still a pocket: steep paths rattle out.
    assert!(width(RULESET, PocketId::SideLeft, 70.0, 1.5) < 40.0);
}

#[test]
fn corner_pockets_take_a_ball_straight_in() {
    assert!(width(RULESET, PocketId::FootLeft, 0.0, 1.5) >= 60.0);
}
