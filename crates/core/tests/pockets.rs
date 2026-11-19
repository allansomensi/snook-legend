//! How the pockets play: the band of paths that still drop, measured with
//! the real physics. Guards against a change that would close the side
//! pockets again (players found them too tight).

use glam::DVec2;
use snook_legend_core::{
    Ball, BallId, BallStatus, PhysicsParams, PocketId, Shot, Simulation, Table, TableSpec,
};

/// The width (mm) of the band of paths, `deg` from straight in, that drop
/// at `speed` (m/s).
fn width(pocket: PocketId, deg: f64, speed: f64) -> f64 {
    let spec = TableSpec::default();
    let params = PhysicsParams::default();
    let table = Table::new(spec);
    let p = *table.pocket(pocket);
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

/// The table is the WPA's 8-foot tournament table, measure for
/// measure, and its pockets play like regulation ones: a side pocket takes
/// a wider band than a corner straight in, and little at a steep angle.
#[test]
fn the_table_is_a_regulation_eight_foot_table() {
    let inch = 0.0254;
    let s = TableSpec::default();
    assert_eq!(s, TableSpec::WPA_8FT);
    assert!((s.length - 92.0 * inch).abs() < 1e-9 && (s.width - 46.0 * inch).abs() < 1e-9);
    assert!((s.ball_radius - 1.125 * inch).abs() < 1e-9);
    assert!((0.156..=0.170).contains(&s.ball_mass), "5 1/2 to 6 oz");
    assert!((s.cushion_height / (2.0 * s.ball_radius) - 0.635).abs() < 0.01);
    assert!((4.5 * inch..=4.625 * inch).contains(&s.corner_mouth));
    assert!((5.0 * inch..=5.125 * inch).contains(&s.side_mouth));
    assert!((1.0 * inch..=2.25 * inch).contains(&s.corner_shelf));
    assert!((0.0..=0.375 * inch).contains(&s.side_shelf));
    // Facing angles: 142° at the corners (135° + 7°), 104° at the sides
    // (90° + 14°, turned in: the throat narrows).
    assert!((s.corner_facing_turn.to_degrees() - 7.0).abs() <= 1.0);
    assert!((s.side_facing_open.to_degrees() + 14.0).abs() <= 1.0);

    let side = width(PocketId::SideLeft, 0.0, 1.5);
    let corner = width(PocketId::FootLeft, 0.0, 1.5);
    assert!((75.0..=100.0).contains(&side), "side, straight in: {side} mm");
    assert!((55.0..=80.0).contains(&corner), "corner, straight in: {corner} mm");
    assert!(width(PocketId::SideLeft, 70.0, 1.5) < 30.0);
}

#[test]
fn corner_pockets_take_a_ball_straight_in() {
    assert!(width(PocketId::FootLeft, 0.0, 1.5) >= 60.0);
}
