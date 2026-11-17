//! The themed tables of the shop: the body, rails, legs, castings and
//! pockets around the cloth (the cloth and cushions are the scene's).
//!
//! Everything is built from code, away from the main thread (see
//! [`crate::gfx::Kit`]); only its owner sees a table, so the theme never
//! changes the game.

use ::glam::DVec2;
use macroquad::prelude::*;
use snook_legend_core::Table;
use snook_legend_proto::shop::TableStyle;

use crate::config::Quality;
use crate::coords::{TABLE_HEIGHT, surface};
use crate::gfx::{Kit, Layer, MeshBuilder, Surface};
use crate::scene::{
    CAST_H, CUSHION_DEPTH, RAIL_TOP, jaw_lines, jaw_wall, pocket_center_of, pocket_radius_of,
    rounded_loop,
};
use crate::textures::{self, Pixels, fbm, hash, image, lerp3, noise};

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba(r, g, b, 255)
}

/// The legs.
#[derive(Clone, Copy, PartialEq)]
enum Legs {
    /// Turned balusters under a square block (rings in `ring` if any).
    Turned,
    /// Tapered square posts on metal feet, tied by canes near the floor.
    Square,
    /// Heavy turned and fluted legs with a bun foot.
    Carved,
    /// Two slabs across the table on a metal plinth.
    Slab,
    /// Heavy square legs with a chrome band and levelling feet (pro tables).
    Block,
    /// Barley-twist legs: a double spiral turned between square blocks.
    Twist,
    /// A black steel frame: square tube legs braced with an X.
    Steel,
    /// Fluted columns with a gilt base and capital.
    Column,
}

/// What the side of the table below the rail is made of.
#[derive(Clone, Copy, PartialEq)]
enum Apron {
    /// Wood with a small lip at the bottom.
    Lip,
    /// Bamboo canes side by side.
    Bamboo,
    /// A deep apron with a bead moulding.
    Beaded,
    /// A slim slab with a metal strip and a light under it.
    Slim,
    /// Plain and deep, with a chrome band near the bottom.
    Plain,
    /// A riveted steel plate.
    Plate,
}

/// The metal (or leather) on the rail around the pockets.
#[derive(Clone, Copy, PartialEq)]
enum Pockets {
    /// Metal plates with a leather lip round the hole.
    Plates,
    /// Leather over the pockets and nets inside.
    Nets,
}

struct Style {
    /// Rail wood: light and dark tones, grain seed.
    rail: ([f32; 3], [f32; 3], u32),
    /// Legs (and the apron, unless bamboo).
    body: ([f32; 3], [f32; 3], u32),
    wood: Surface,
    /// Castings: colour and finish.
    cast: Color,
    cast_surface: Surface,
    /// The lip round each hole.
    lip: Color,
    lip_surface: Surface,
    sights: Color,
    /// Diamond-shaped sights (else round).
    diamonds: bool,
    /// A second metal for rings, feet and trims.
    trim: Color,
    legs: Legs,
    /// Metal rings on turned legs.
    leg_rings: bool,
    apron: Apron,
    apron_h: f32,
    pockets: Pockets,
}

fn style(s: TableStyle) -> Style {
    let brass = Surface::lit(1.6, 90.0, 0.12, 0.7);
    let chrome = Surface::lit(1.3, 120.0, 0.1, 0.55);
    let leather = Surface::lit(0.5, 30.0, 0.0, 0.2);
    match s {
        TableStyle::Classic => Style {
            rail: ([0.17, 0.075, 0.045], [0.05, 0.022, 0.014], 11),
            body: ([0.13, 0.06, 0.035], [0.04, 0.018, 0.012], 17),
            wood: Surface::WOOD,
            cast: Color::new(0.47, 0.35, 0.17, 1.0),
            cast_surface: brass,
            lip: rgb(46, 28, 18),
            lip_surface: leather,
            sights: rgb(235, 228, 205),
            diamonds: false,
            trim: Color::new(0.47, 0.35, 0.17, 1.0),
            legs: Legs::Turned,
            leg_rings: false,
            apron: Apron::Lip,
            apron_h: 0.2,
            pockets: Pockets::Plates,
        },
        TableStyle::Beach => Style {
            rail: ([0.62, 0.40, 0.21], [0.36, 0.20, 0.09], 23),
            body: ([0.55, 0.36, 0.19], [0.30, 0.17, 0.08], 29),
            wood: Surface::lit(0.3, 30.0, 0.05, 0.25),
            cast: Color::new(0.44, 0.46, 0.50, 1.0),
            cast_surface: chrome,
            lip: Color::new(0.40, 0.42, 0.46, 1.0),
            lip_surface: chrome,
            sights: rgb(225, 238, 245),
            diamonds: false,
            trim: Color::new(0.80, 0.82, 0.86, 1.0),
            legs: Legs::Square,
            leg_rings: false,
            apron: Apron::Bamboo,
            apron_h: 0.19,
            pockets: Pockets::Plates,
        },
        TableStyle::Club => Style {
            rail: ([0.30, 0.10, 0.05], [0.085, 0.028, 0.016], 41),
            body: ([0.24, 0.085, 0.045], [0.07, 0.024, 0.014], 43),
            wood: Surface::lit(0.55, 60.0, 0.06, 0.55),
            cast: rgb(58, 34, 20),
            cast_surface: leather,
            lip: rgb(52, 30, 17),
            lip_surface: leather,
            sights: rgb(236, 226, 196),
            diamonds: true,
            trim: Color::new(0.55, 0.42, 0.20, 1.0),
            legs: Legs::Carved,
            leg_rings: false,
            apron: Apron::Beaded,
            apron_h: 0.25,
            pockets: Pockets::Nets,
        },
        TableStyle::Modern => Style {
            rail: ([0.055, 0.055, 0.06], [0.03, 0.03, 0.034], 53),
            body: ([0.05, 0.05, 0.055], [0.028, 0.028, 0.032], 59),
            wood: Surface::lit(1.2, 120.0, 0.08, 0.8),
            cast: Color::new(0.62, 0.64, 0.67, 1.0),
            cast_surface: Surface::lit(1.4, 60.0, 0.15, 0.5),
            lip: rgb(18, 18, 20),
            lip_surface: Surface::lit(0.3, 20.0, 0.0, 0.1),
            sights: Color::new(0.75, 0.77, 0.80, 1.0),
            diamonds: false,
            trim: Color::new(0.62, 0.64, 0.67, 1.0),
            legs: Legs::Slab,
            leg_rings: false,
            apron: Apron::Slim,
            apron_h: 0.12,
            pockets: Pockets::Plates,
        },
        TableStyle::Royal => Style {
            rail: ([0.075, 0.05, 0.04], [0.02, 0.014, 0.012], 61),
            body: ([0.07, 0.048, 0.038], [0.018, 0.013, 0.011], 67),
            wood: Surface::lit(0.9, 90.0, 0.08, 0.7),
            cast: Color::new(0.80, 0.62, 0.26, 1.0),
            cast_surface: Surface::lit(2.2, 120.0, 0.15, 0.9),
            lip: rgb(92, 18, 26),
            lip_surface: leather,
            sights: Color::new(0.88, 0.70, 0.32, 1.0),
            diamonds: true,
            trim: Color::new(0.80, 0.62, 0.26, 1.0),
            legs: Legs::Turned,
            leg_rings: true,
            apron: Apron::Beaded,
            apron_h: 0.2,
            pockets: Pockets::Plates,
        },
        TableStyle::Industrial => Style {
            rail: ([0.47, 0.38, 0.29], [0.20, 0.15, 0.11], 71),
            body: ([0.47, 0.38, 0.29], [0.20, 0.15, 0.11], 73),
            wood: Surface::lit(0.25, 24.0, 0.05, 0.15),
            cast: Color::new(0.10, 0.10, 0.11, 1.0),
            cast_surface: Surface::lit(0.9, 40.0, 0.12, 0.3),
            lip: rgb(14, 14, 15),
            lip_surface: Surface::lit(0.3, 16.0, 0.0, 0.05),
            sights: Color::new(0.70, 0.72, 0.74, 1.0),
            diamonds: false,
            trim: Color::new(0.11, 0.11, 0.12, 1.0),
            legs: Legs::Steel,
            leg_rings: false,
            apron: Apron::Plate,
            apron_h: 0.2,
            pockets: Pockets::Plates,
        },
        TableStyle::Tournament => Style {
            rail: ([0.115, 0.115, 0.125], [0.06, 0.06, 0.066], 79),
            body: ([0.10, 0.10, 0.11], [0.05, 0.05, 0.056], 83),
            wood: Surface::lit(0.6, 70.0, 0.06, 0.4),
            cast: Color::new(0.42, 0.44, 0.47, 1.0),
            cast_surface: chrome,
            lip: rgb(16, 16, 18),
            lip_surface: Surface::lit(0.3, 20.0, 0.0, 0.1),
            sights: rgb(238, 240, 244),
            diamonds: true,
            trim: Color::new(0.74, 0.76, 0.79, 1.0),
            legs: Legs::Block,
            leg_rings: false,
            apron: Apron::Plain,
            apron_h: 0.25,
            pockets: Pockets::Plates,
        },
        TableStyle::Heritage => Style {
            rail: ([0.40, 0.22, 0.10], [0.13, 0.065, 0.03], 89),
            body: ([0.36, 0.20, 0.09], [0.12, 0.06, 0.028], 97),
            wood: Surface::lit(0.8, 80.0, 0.07, 0.65),
            cast: Color::new(0.62, 0.46, 0.20, 1.0),
            cast_surface: brass,
            lip: rgb(60, 34, 18),
            lip_surface: leather,
            sights: rgb(240, 232, 214),
            diamonds: true,
            trim: Color::new(0.62, 0.46, 0.20, 1.0),
            legs: Legs::Twist,
            leg_rings: false,
            apron: Apron::Beaded,
            apron_h: 0.24,
            pockets: Pockets::Nets,
        },
        TableStyle::Deco => Style {
            rail: ([0.88, 0.84, 0.76], [0.74, 0.69, 0.60], 101),
            body: ([0.86, 0.82, 0.74], [0.72, 0.67, 0.58], 103),
            wood: Surface::lit(1.3, 140.0, 0.08, 0.85),
            cast: Color::new(0.85, 0.65, 0.27, 1.0),
            cast_surface: Surface::lit(2.2, 120.0, 0.15, 0.9),
            lip: rgb(20, 18, 18),
            lip_surface: leather,
            sights: Color::new(0.88, 0.70, 0.32, 1.0),
            diamonds: true,
            trim: Color::new(0.85, 0.65, 0.27, 1.0),
            legs: Legs::Column,
            leg_rings: true,
            apron: Apron::Beaded,
            apron_h: 0.21,
            pockets: Pockets::Plates,
        },
    }
}

/// Everything of the table but the cloth, cushions and balls.
pub fn build(table: &Table, quality: Quality, theme: TableStyle) -> Kit {
    let st = style(theme);
    let mut kit = Kit::default();
    let rail_tex = kit.texture(textures::fine_wood(st.rail.0, st.rail.1, st.rail.2), true);
    let body_tex = kit.texture(textures::fine_wood(st.body.0, st.body.1, st.body.2), true);
    let apron_tex = match st.apron {
        Apron::Bamboo => kit.texture(bamboo(), true),
        _ => rail_tex,
    };
    let spec = table.spec;
    let (l, w) = (spec.length as f32, spec.width as f32);
    let y = TABLE_HEIGHT;
    let rail_outer = spec.rail_width as f32 + 0.05;
    let (bi, bo) = (CUSHION_DEPTH, rail_outer);
    let apron = st.apron_h;
    let bottom = y - apron;
    let outer = rounded_loop(l / 2.0 + bo, w / 2.0 + bo, 0.075);
    let inner = rounded_loop(l / 2.0 + bi, w / 2.0 + bi, 0.0);
    let low = quality == Quality::Low;

    // --- Body: the rail ring on top and the apron below ----------------------
    let mut rail = MeshBuilder::default();
    let mut skirt = MeshBuilder::default();
    let mut cast = MeshBuilder::default();
    let mut trim = MeshBuilder::default();
    let mut glow = MeshBuilder::default();
    let mut plate = MeshBuilder::default();
    let n = outer.len();
    let mut along = vec![0.0f32; n + 1];
    for k in 0..n {
        along[k + 1] = along[k] + outer[k].distance(outer[(k + 1) % n]);
    }
    let in_casting = |p: Vec2| {
        let corner = p.x.abs() > l / 2.0 - 0.10 && p.y.abs() > w / 2.0 - 0.10;
        let side = p.x.abs() < 0.11 && p.y.abs() > w / 2.0;
        corner || side
    };
    let top = y + RAIL_TOP;
    // Leather over the pockets stands a little prouder than metal.
    let cast_h = if st.pockets == Pockets::Nets { CAST_H + 0.002 } else { CAST_H };
    let ctop = top + cast_h;
    let wood = WHITE;
    let quad = |mb: &mut MeshBuilder, p: [Vec3; 4], uv: [Vec2; 4], nrm: Vec3, c: Color| {
        let idx: Vec<u16> = (0..4).map(|q| mb.push(p[q], uv[q], nrm, c)).collect();
        mb.indices.extend_from_slice(&[idx[0], idx[1], idx[2], idx[0], idx[2], idx[3]]);
    };
    // Where the rail band ends and the apron begins.
    let split = y - 0.012;
    // The apron's texture scale (bamboo canes ~3.5 cm wide).
    let apron_uv = if st.apron == Apron::Bamboo { 1.0 / 0.28 } else { 0.7 };
    for k in 0..n {
        let k1 = (k + 1) % n;
        let (o0, o1, i0, i1) = (outer[k], outer[k1], inner[k], inner[k1]);
        let (u0, u1) = (along[k] * 0.7, along[k + 1] * 0.7);
        let (a0, a1) = (along[k] * apron_uv, along[k + 1] * apron_uv);
        let v3 = |p: Vec2, h: f32| vec3(p.x, h, p.y);
        let normal_out = {
            let t = o1 - o0;
            vec3(t.y, 0.0, -t.x).normalize_or_zero()
        };
        let normal_out = if normal_out.dot(v3(o0, 0.0)) < 0.0 { -normal_out } else { normal_out };
        let out2 = |p: Vec2, d: f32| p + vec2(normal_out.x, normal_out.z) * d;
        let across = (bo - bi) * 1.2;
        // The outer edge of the rail is rounded (a quarter circle).
        let round = if low { 0.0 } else { 0.014 };
        let (oi0, oi1) = (out2(o0, -round), out2(o1, -round));
        quad(
            &mut rail,
            [v3(i0, top), v3(i1, top), v3(oi1, top), v3(oi0, top)],
            [vec2(u0, 0.0), vec2(u1, 0.0), vec2(u1, across), vec2(u0, across)],
            Vec3::Y,
            wood,
        );
        if round > 0.0 {
            let steps = if quality == Quality::Ultra { 5 } else { 3 };
            for s in 0..steps {
                let b0 = s as f32 / steps as f32 * std::f32::consts::FRAC_PI_2;
                let b1 = (s + 1) as f32 / steps as f32 * std::f32::consts::FRAC_PI_2;
                let at = |p: Vec2, a: f32| {
                    let q = out2(p, -round + round * a.sin());
                    vec3(q.x, top - round + round * a.cos(), q.y)
                };
                let nr = |a: f32| (Vec3::Y * a.cos() + normal_out * a.sin()).normalize_or_zero();
                let v = |a: f32| across + a * round;
                let idx = [
                    rail.push(at(o0, b0), vec2(u0, v(b0)), nr(b0), wood),
                    rail.push(at(o1, b0), vec2(u1, v(b0)), nr(b0), wood),
                    rail.push(at(o1, b1), vec2(u1, v(b1)), nr(b1), wood),
                    rail.push(at(o0, b1), vec2(u0, v(b1)), nr(b1), wood),
                ];
                rail.indices.extend_from_slice(&[idx[0], idx[1], idx[2], idx[0], idx[2], idx[3]]);
            }
        }
        // The rail band, down to the apron.
        quad(
            &mut rail,
            [v3(o0, split), v3(o1, split), v3(o1, top - round), v3(o0, top - round)],
            [vec2(u0, 0.0), vec2(u1, 0.0), vec2(u1, 0.06), vec2(u0, 0.06)],
            normal_out,
            wood,
        );
        // The apron, set back a little under a bamboo or slim rail.
        let inset = match st.apron {
            Apron::Bamboo => 0.012,
            Apron::Slim => 0.02,
            _ => 0.0,
        };
        let (s0, s1) = (out2(o0, -inset), out2(o1, -inset));
        if inset > 0.0 {
            // The underside of the rail band over the set-back apron.
            quad(
                &mut rail,
                [v3(s0, split), v3(s1, split), v3(o1, split), v3(o0, split)],
                [Vec2::ZERO; 4],
                -Vec3::Y,
                wood,
            );
        }
        let lip = match st.apron {
            Apron::Lip | Apron::Beaded if !low => 0.035,
            _ => 0.0,
        };
        let apron_mesh = match st.apron {
            Apron::Bamboo => &mut skirt,
            Apron::Plate => &mut plate,
            _ => &mut rail,
        };
        let vh = |h: f32| (split - h) * apron_uv;
        quad(
            apron_mesh,
            [v3(s0, bottom + lip), v3(s1, bottom + lip), v3(s1, split), v3(s0, split)],
            [vec2(a0, vh(bottom + lip)), vec2(a1, vh(bottom + lip)), vec2(a1, 0.0), vec2(a0, 0.0)],
            normal_out,
            wood,
        );
        // The underside of the apron.
        quad(
            apron_mesh,
            [
                v3(s0, bottom),
                v3(s1, bottom),
                v3(out2(i1, 0.05), bottom),
                v3(out2(i0, 0.05), bottom),
            ],
            [Vec2::ZERO; 4],
            -Vec3::Y,
            wood,
        );
        if lip > 0.0 {
            // A rounded lip standing a little proud at the bottom of the apron.
            let (l0, l1) = (out2(o0, 0.012), out2(o1, 0.012));
            quad(
                &mut rail,
                [
                    v3(o0, bottom + lip),
                    v3(o1, bottom + lip),
                    v3(l1, bottom + lip - 0.008),
                    v3(l0, bottom + lip - 0.008),
                ],
                [vec2(u0, 0.0), vec2(u1, 0.0), vec2(u1, 0.02), vec2(u0, 0.02)],
                (Vec3::Y + normal_out).normalize_or_zero(),
                wood,
            );
            quad(
                &mut rail,
                [
                    v3(l0, bottom),
                    v3(l1, bottom),
                    v3(l1, bottom + lip - 0.008),
                    v3(l0, bottom + lip - 0.008),
                ],
                [vec2(u0, 0.0), vec2(u1, 0.0), vec2(u1, 0.04), vec2(u0, 0.04)],
                normal_out,
                wood,
            );
            quad(
                &mut rail,
                [v3(o0, bottom), v3(o1, bottom), v3(l1, bottom), v3(l0, bottom)],
                [Vec2::ZERO; 4],
                -Vec3::Y,
                wood,
            );
        }
        if st.apron == Apron::Beaded && !low {
            // A half-round bead along the apron, under the rail band.
            let bead_y = split - 0.03;
            let r = 0.009;
            let steps = 4;
            let gilt = matches!(theme, TableStyle::Royal | TableStyle::Deco | TableStyle::Heritage);
            let target = if gilt { &mut trim } else { &mut rail };
            for s in 0..steps {
                let b0 =
                    -std::f32::consts::FRAC_PI_2 + s as f32 / steps as f32 * std::f32::consts::PI;
                let b1 = -std::f32::consts::FRAC_PI_2
                    + (s + 1) as f32 / steps as f32 * std::f32::consts::PI;
                let at = |p: Vec2, a: f32| {
                    let q = out2(p, r * a.cos());
                    vec3(q.x, bead_y + r * a.sin(), q.y)
                };
                let nr = |a: f32| (normal_out * a.cos() + Vec3::Y * a.sin()).normalize_or_zero();
                let idx = [
                    target.push(at(o0, b0), vec2(u0, 0.0), nr(b0), wood),
                    target.push(at(o1, b0), vec2(u1, 0.0), nr(b0), wood),
                    target.push(at(o1, b1), vec2(u1, 0.02), nr(b1), wood),
                    target.push(at(o0, b1), vec2(u0, 0.02), nr(b1), wood),
                ];
                target.indices.extend_from_slice(&[idx[0], idx[1], idx[2], idx[0], idx[2], idx[3]]);
            }
        }
        match st.apron {
            Apron::Slim => {
                // A brushed metal strip where the rail meets the apron, and a
                // line of light under the apron's edge.
                let (m0, m1) = (out2(o0, 0.002), out2(o1, 0.002));
                quad(
                    &mut trim,
                    [
                        v3(m0, split - 0.002),
                        v3(m1, split - 0.002),
                        v3(m1, split + 0.01),
                        v3(m0, split + 0.01),
                    ],
                    [Vec2::ZERO; 4],
                    normal_out,
                    st.trim,
                );
                if !low {
                    let (g0, g1) = (out2(o0, -inset + 0.002), out2(o1, -inset + 0.002));
                    quad(
                        &mut glow,
                        [
                            v3(g0, bottom + 0.004),
                            v3(g1, bottom + 0.004),
                            v3(g1, bottom + 0.011),
                            v3(g0, bottom + 0.011),
                        ],
                        [Vec2::ZERO; 4],
                        normal_out,
                        Color::new(0.62, 0.82, 1.0, 1.0),
                    );
                }
            }
            Apron::Plain => {
                // A chrome band a hand above the bottom of the apron.
                let (m0, m1) = (out2(o0, 0.0015), out2(o1, 0.0015));
                quad(
                    &mut trim,
                    [
                        v3(m0, bottom + 0.035),
                        v3(m1, bottom + 0.035),
                        v3(m1, bottom + 0.05),
                        v3(m0, bottom + 0.05),
                    ],
                    [Vec2::ZERO; 4],
                    normal_out,
                    st.trim,
                );
            }
            Apron::Plate if !low => {
                // Rivets along the top and the bottom of the plate, every
                // 12 cm or so on the straight runs.
                let len = o0.distance(o1);
                let count = (len / 0.12).floor() as i32;
                for r in 0..count {
                    let t = (r as f32 + 0.5) / count as f32;
                    let p = o0.lerp(o1, t);
                    for h in [split - 0.025, bottom + 0.025] {
                        let c = v3(out2(p, 0.0012), h);
                        crate::room::disc_facing(&mut trim, c, normal_out, 0.0055, 6, st.trim);
                    }
                }
            }
            Apron::Bamboo if !low => {
                // A teak lath along the bottom of the canes.
                let (m0, m1) = (out2(o0, -inset + 0.006), out2(o1, -inset + 0.006));
                quad(
                    &mut rail,
                    [v3(m0, bottom), v3(m1, bottom), v3(m1, bottom + 0.03), v3(m0, bottom + 0.03)],
                    [vec2(u0, 0.0), vec2(u1, 0.0), vec2(u1, 0.04), vec2(u0, 0.04)],
                    normal_out,
                    wood,
                );
                quad(
                    &mut rail,
                    [
                        v3(s0, bottom + 0.03),
                        v3(s1, bottom + 0.03),
                        v3(m1, bottom + 0.03),
                        v3(m0, bottom + 0.03),
                    ],
                    [Vec2::ZERO; 4],
                    Vec3::Y,
                    wood,
                );
            }
            _ => {}
        }
        // Inner side of the rail (behind the cushions; open at the pockets).
        quad(
            &mut rail,
            [v3(i0, y), v3(i1, y), v3(i1, top), v3(i0, top)],
            [vec2(u0, 0.0), vec2(u1, 0.0), vec2(u1, 0.05), vec2(u0, 0.05)],
            -normal_out,
            wood,
        );
        // Castings (or leathers) on the rail around the pockets.
        let mid = (o0 + o1) / 2.0;
        if in_casting(mid) {
            let z = [Vec2::ZERO; 4];
            let c = st.cast;
            quad(
                &mut cast,
                [v3(i0, ctop), v3(i1, ctop), v3(o1, ctop), v3(o0, ctop)],
                z,
                Vec3::Y,
                c,
            );
            quad(
                &mut cast,
                [v3(o0, top), v3(o1, top), v3(o1, ctop), v3(o0, ctop)],
                z,
                normal_out,
                c,
            );
            quad(
                &mut cast,
                [v3(i0, top), v3(i1, top), v3(i1, ctop), v3(i0, ctop)],
                z,
                -normal_out,
                c,
            );
            let prev = (outer[(k + n - 1) % n] + o0) / 2.0;
            let next = (o1 + outer[(k + 2) % n]) / 2.0;
            let tangent = (o1 - o0).normalize_or_zero();
            let tn = vec3(tangent.x, 0.0, tangent.y);
            if !in_casting(prev) {
                quad(&mut cast, [v3(i0, top), v3(o0, top), v3(o0, ctop), v3(i0, ctop)], z, -tn, c);
            }
            if !in_casting(next) {
                quad(&mut cast, [v3(i1, top), v3(o1, top), v3(o1, ctop), v3(i1, ctop)], z, tn, c);
            }
            if st.pockets == Pockets::Nets && !low {
                // The leather folds down over the rounded edge.
                let (d0, d1) = (out2(o0, 0.001), out2(o1, 0.001));
                quad(
                    &mut cast,
                    [v3(d0, top - 0.035), v3(d1, top - 0.035), v3(d1, ctop), v3(d0, ctop)],
                    z,
                    normal_out,
                    c,
                );
            }
        }
    }
    kit.add(rail, Some(rail_tex), st.wood.cut(2.0), Layer::Solid);
    kit.add(skirt, Some(apron_tex), Surface::lit(0.35, 30.0, 0.06, 0.3), Layer::Solid);
    if !plate.is_empty() {
        // Blackened steel, brushed: the grain of the steel texture.
        let steel =
            kit.texture(textures::fine_wood([0.17, 0.17, 0.18], [0.09, 0.09, 0.10], 107), true);
        kit.add(plate, Some(steel), Surface::lit(0.9, 50.0, 0.1, 0.35), Layer::Solid);
    }
    kit.add(cast, None, st.cast_surface.cut(2.0), Layer::Solid);
    kit.add(trim, None, Surface::lit(1.8, 90.0, 0.12, 0.7), Layer::Solid);
    kit.add(glow, None, Surface::UNLIT, Layer::Solid);

    // The jaws cut through the rail: their faces are the casting.
    let mut jaw_rails = MeshBuilder::default();
    for (a, end, across) in jaw_lines(table) {
        jaw_wall(&mut MeshBuilder::default(), &mut jaw_rails, table, a, end, across, st.cast);
    }
    kit.add(jaw_rails, None, Surface::lit(0.6, 50.0, 0.1, 0.3), Layer::Solid);

    legs(&mut kit, &st, quality, l, w, bottom, body_tex);
    sights(&mut kit, &st, table);
    pockets(&mut kit, &st, table);
    kit
}

/// A box tapering from `w0` (bottom) to `w1` (top), square in plan.
fn tapered_post(mb: &mut MeshBuilder, at: Vec3, h: f32, w0: f32, w1: f32, color: Color) {
    let c = |sx: f32, sz: f32, y: f32| {
        let half = (w0 + (w1 - w0) * (y / h)) / 2.0;
        at + vec3(sx * half, y, sz * half)
    };
    let sides = [
        ([(-1.0, 1.0), (1.0, 1.0)], Vec3::Z),
        ([(1.0, 1.0), (1.0, -1.0)], Vec3::X),
        ([(1.0, -1.0), (-1.0, -1.0)], -Vec3::Z),
        ([(-1.0, -1.0), (-1.0, 1.0)], -Vec3::X),
    ];
    for ([(ax, az), (bx, bz)], _) in sides {
        mb.quad(
            [c(ax, az, 0.0), c(bx, bz, 0.0), c(bx, bz, h), c(ax, az, h)],
            [vec2(0.0, 0.0), vec2(w0 * 1.2, 0.0), vec2(w1 * 1.2, h * 1.2), vec2(0.0, h * 1.2)],
            color,
        );
    }
}

/// A turned shape whose middle (between heights `y0` and `y1`) is fluted:
/// `flutes` shallow grooves round it.
#[allow(clippy::too_many_arguments)]
fn fluted_lathe(
    mb: &mut MeshBuilder,
    at: Vec3,
    profile: &[(f32, f32)],
    segments: u32,
    flutes: u32,
    y0: f32,
    y1: f32,
    color: Color,
) {
    let first = mb.vertices.len();
    mb.lathe(at, profile, segments, color);
    for v in &mut mb.vertices[first..] {
        let p = v.position - at;
        if p.y <= y0 || p.y >= y1 {
            continue;
        }
        // Fade the grooves in and out at the ends.
        let fade = ((p.y - y0) / 0.03).min((y1 - p.y) / 0.03).clamp(0.0, 1.0);
        let a = p.z.atan2(p.x);
        let groove = (a * flutes as f32).cos().max(0.0).powf(3.0) * 0.12 * fade;
        let k = 1.0 - groove;
        v.position = at + vec3(p.x * k, p.y, p.z * k);
        // Tilt the normal across the groove so it catches the light.
        let tilt = -(a * flutes as f32).sin() * groove * 2.0;
        let n = v.normal.truncate();
        let side = vec3(-a.sin(), 0.0, a.cos());
        let n = (n + side * tilt).normalize_or_zero();
        v.normal = n.extend(0.0);
    }
}

fn legs(kit: &mut Kit, st: &Style, quality: Quality, l: f32, w: f32, bottom: f32, tex: usize) {
    let mut legs = MeshBuilder::default();
    let mut metal = MeshBuilder::default();
    let leg_h = bottom;
    let low = quality == Quality::Low;
    let segs = if quality == Quality::Ultra { 36 } else { 20 };
    let corners = [(1.0f32, 1.0f32), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)];
    match st.legs {
        Legs::Turned | Legs::Carved | Legs::Twist | Legs::Column if low => {
            for (sx, sz) in corners {
                let at = vec3(sx * (l / 2.0 - 0.02), 0.0, sz * (w / 2.0 - 0.02));
                let size = if st.legs == Legs::Carved { 0.24 } else { 0.19 };
                legs.cuboid(at + Vec3::Y * leg_h / 2.0, vec3(size, leg_h, size), 0.0, WHITE, 1.2);
            }
        }
        Legs::Turned => {
            for (sx, sz) in corners {
                let at = vec3(sx * (l / 2.0 - 0.02), 0.0, sz * (w / 2.0 - 0.02));
                let block = 0.16;
                legs.cuboid(
                    at + Vec3::Y * (leg_h - block / 2.0),
                    vec3(0.2, block, 0.2),
                    0.0,
                    WHITE,
                    1.2,
                );
                let h = leg_h - block;
                let profile = [
                    (0.0, 0.0),
                    (0.0, 0.105),
                    (0.025, 0.11),
                    (0.05, 0.1),
                    (0.07, 0.075),
                    (0.11, 0.07),
                    (0.16, 0.085),
                    (0.22, 0.1),
                    (0.30, 0.105),
                    (0.36, 0.095),
                    (0.40, 0.07),
                    (h * 0.75, 0.058),
                    (h * 0.8, 0.068),
                    (h * 0.84, 0.072),
                    (h * 0.88, 0.06),
                    (h - 0.015, 0.068),
                    (h, 0.085),
                ];
                legs.lathe(at, &profile, segs, WHITE);
                if st.leg_rings {
                    // Gold rings where the turning narrows (the ebony table).
                    for (yy, r) in [(0.055, 0.103), (h * 0.84, 0.074), (h - 0.012, 0.072)] {
                        metal.lathe(
                            at + Vec3::Y * yy,
                            &[(0.0, r), (0.006, r + 0.004), (0.012, r)],
                            segs,
                            st.trim,
                        );
                    }
                }
            }
        }
        Legs::Carved => {
            for (sx, sz) in corners {
                let at = vec3(sx * (l / 2.0 - 0.04), 0.0, sz * (w / 2.0 - 0.04));
                let block = 0.2;
                // A square block under the apron with a moulded cap.
                legs.cuboid(
                    at + Vec3::Y * (leg_h - block / 2.0),
                    vec3(0.27, block, 0.27),
                    0.0,
                    WHITE,
                    1.2,
                );
                legs.cuboid(
                    at + Vec3::Y * (leg_h - block - 0.012),
                    vec3(0.24, 0.024, 0.24),
                    0.0,
                    WHITE,
                    1.2,
                );
                let h = leg_h - block - 0.024;
                // Bun foot, a collar, the fluted "cup", a neck and a capital.
                let profile = [
                    (0.0, 0.0),
                    (0.0, 0.10),
                    (0.02, 0.118),
                    (0.06, 0.122),
                    (0.09, 0.105),
                    (0.11, 0.08),
                    (0.13, 0.088),
                    (0.15, 0.08),
                    (0.18, 0.105),
                    (0.26, 0.13),
                    (0.34, 0.132),
                    (0.42, 0.118),
                    (h * 0.72, 0.085),
                    (h * 0.78, 0.075),
                    (h * 0.82, 0.088),
                    (h * 0.86, 0.078),
                    (h * 0.93, 0.09),
                    (h - 0.01, 0.105),
                    (h, 0.11),
                ];
                fluted_lathe(&mut legs, at, &profile, segs.max(32), 12, 0.2, h * 0.7, WHITE);
            }
        }
        Legs::Square => {
            for (sx, sz) in corners {
                let at = vec3(sx * (l / 2.0 - 0.06), 0.0, sz * (w / 2.0 - 0.06));
                tapered_post(&mut legs, at + Vec3::Y * 0.035, leg_h - 0.035, 0.085, 0.12, WHITE);
                // Chrome shoe.
                metal.cuboid(at + Vec3::Y * 0.0175, vec3(0.095, 0.035, 0.095), 0.0, st.trim, 1.0);
            }
            if !low {
                // Bamboo stretchers between the legs, a hand's width up.
                let cane = Color::new(0.80, 0.66, 0.40, 1.0);
                let (x, z) = (l / 2.0 - 0.06, w / 2.0 - 0.06);
                for sz in [1.0f32, -1.0] {
                    crate::room::tube(
                        &mut legs,
                        vec3(-x, 0.16, sz * z),
                        vec3(x, 0.16, sz * z),
                        0.02,
                        10,
                        cane,
                    );
                }
                for sx in [1.0f32, -1.0] {
                    crate::room::tube(
                        &mut legs,
                        vec3(sx * x, 0.16, -z),
                        vec3(sx * x, 0.16, z),
                        0.02,
                        10,
                        cane,
                    );
                }
            }
        }
        Legs::Block | Legs::Twist | Legs::Steel | Legs::Column => {
            more_legs(&mut legs, &mut metal, st, quality, l, w, leg_h);
        }
        Legs::Slab => {
            for sx in [1.0f32, -1.0] {
                let at = vec3(sx * (l / 2.0 - 0.32), 0.0, 0.0);
                // A wide slab across the table, set on a metal plinth.
                legs.cuboid(
                    at + Vec3::Y * (leg_h / 2.0 + 0.03),
                    vec3(0.11, leg_h - 0.06, w * 0.86),
                    0.0,
                    WHITE,
                    1.2,
                );
                metal.cuboid(at + Vec3::Y * 0.015, vec3(0.16, 0.03, w * 0.92), 0.0, st.trim, 1.0);
                if !low {
                    // A metal reveal line up each face.
                    for sz in [1.0f32, -1.0] {
                        metal.cuboid(
                            at + vec3(0.0, leg_h / 2.0 + 0.03, sz * w * 0.43),
                            vec3(0.113, leg_h - 0.06, 0.008),
                            0.0,
                            st.trim,
                            1.0,
                        );
                    }
                }
            }
            // A beam between the slabs.
            legs.cuboid(vec3(0.0, leg_h * 0.5, 0.0), vec3(l - 0.7, 0.1, 0.08), 0.0, WHITE, 1.2);
        }
    }
    kit.add(legs, Some(tex), st.wood, Layer::Solid);
    kit.add(metal, None, Surface::lit(2.0, 110.0, 0.15, 0.8), Layer::Solid);
}

/// The legs of the newer tables (see [`Legs`]).
#[allow(clippy::too_many_arguments)]
fn more_legs(
    legs: &mut MeshBuilder,
    metal: &mut MeshBuilder,
    st: &Style,
    quality: Quality,
    l: f32,
    w: f32,
    leg_h: f32,
) {
    let segs = if quality == Quality::Ultra { 40 } else { 24 };
    let corners = [(1.0f32, 1.0f32), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)];
    match st.legs {
        Legs::Block => {
            for (sx, sz) in corners {
                let at = vec3(sx * (l / 2.0 - 0.05), 0.0, sz * (w / 2.0 - 0.05));
                tapered_post(legs, at + Vec3::Y * 0.05, leg_h - 0.05, 0.17, 0.2, WHITE);
                // A chrome band and a levelling foot.
                metal.cuboid(at + Vec3::Y * 0.16, vec3(0.178, 0.022, 0.178), 0.0, st.trim, 1.0);
                metal.lathe(
                    at,
                    &[(0.0, 0.07), (0.02, 0.07), (0.03, 0.03), (0.05, 0.03)],
                    20,
                    st.trim,
                );
            }
        }
        Legs::Twist => {
            for (sx, sz) in corners {
                let at = vec3(sx * (l / 2.0 - 0.05), 0.0, sz * (w / 2.0 - 0.05));
                // Square blocks at the top and the bottom, the twist between.
                let (foot, block) = (0.12, 0.17);
                legs.cuboid(at + Vec3::Y * (foot / 2.0), vec3(0.15, foot, 0.15), 0.0, WHITE, 1.2);
                legs.cuboid(
                    at + Vec3::Y * (leg_h - block / 2.0),
                    vec3(0.18, block, 0.18),
                    0.0,
                    WHITE,
                    1.2,
                );
                let h = leg_h - foot - block;
                let base = at + Vec3::Y * foot;
                let profile = [
                    (0.0, 0.055),
                    (0.02, 0.07),
                    (0.04, 0.06),
                    (0.06, 0.068),
                    (h - 0.06, 0.068),
                    (h - 0.04, 0.06),
                    (h - 0.02, 0.07),
                    (h, 0.055),
                ];
                twisted_lathe(legs, base, &profile, segs, 0.06, h - 0.06, 2.6 / h, WHITE);
                // Brass collars where the twist meets the blocks.
                for y in [foot - 0.004, leg_h - block - 0.004] {
                    metal.lathe(
                        at + Vec3::Y * y,
                        &[(0.0, 0.074), (0.004, 0.078), (0.008, 0.074)],
                        segs,
                        st.trim,
                    );
                }
            }
        }
        Legs::Steel => {
            let (x, z) = (l / 2.0 - 0.08, w / 2.0 - 0.08);
            let black = Color::new(0.13, 0.13, 0.14, 1.0);
            for (sx, sz) in corners {
                let at = vec3(sx * x, 0.0, sz * z);
                metal.cuboid(
                    at + Vec3::Y * (leg_h / 2.0),
                    vec3(0.08, leg_h, 0.08),
                    0.0,
                    black,
                    1.0,
                );
                metal.cuboid(at + Vec3::Y * 0.006, vec3(0.13, 0.012, 0.13), 0.0, black, 1.0);
            }
            // An X of flat bar across each long side and a stretcher
            // down the middle, a hand above the floor.
            for sz in [1.0f32, -1.0] {
                for (a, b) in [
                    (vec3(-x, 0.12, sz * z), vec3(0.0, leg_h - 0.1, sz * z)),
                    (vec3(0.0, leg_h - 0.1, sz * z), vec3(x, 0.12, sz * z)),
                    (vec3(-x, leg_h - 0.1, sz * z), vec3(0.0, 0.12, sz * z)),
                    (vec3(0.0, 0.12, sz * z), vec3(x, leg_h - 0.1, sz * z)),
                ] {
                    crate::room::tube(metal, a, b, 0.016, 6, black);
                }
            }
            for sx in [1.0f32, -1.0] {
                metal.cuboid(vec3(sx * x, 0.14, 0.0), vec3(0.05, 0.05, 2.0 * z), 0.0, black, 1.0);
            }
            metal.cuboid(vec3(0.0, 0.14, 0.0), vec3(2.0 * x, 0.05, 0.05), 0.0, black, 1.0);
        }
        Legs::Column => {
            for (sx, sz) in corners {
                let at = vec3(sx * (l / 2.0 - 0.06), 0.0, sz * (w / 2.0 - 0.06));
                // A stepped gilt base, the fluted shaft, a gilt capital.
                metal.cuboid(at + Vec3::Y * 0.02, vec3(0.2, 0.04, 0.2), 0.0, st.trim, 1.0);
                metal.lathe(
                    at + Vec3::Y * 0.04,
                    &[(0.0, 0.09), (0.03, 0.09), (0.045, 0.075), (0.06, 0.08)],
                    segs,
                    st.trim,
                );
                let h = leg_h - 0.16;
                let shaft = [(0.0, 0.072), (h * 0.5, 0.068), (h, 0.062)];
                fluted_lathe(
                    legs,
                    at + Vec3::Y * 0.1,
                    &shaft,
                    segs.max(36),
                    16,
                    0.02,
                    h - 0.02,
                    WHITE,
                );
                metal.lathe(
                    at + Vec3::Y * (0.1 + h),
                    &[(0.0, 0.066), (0.02, 0.08), (0.04, 0.095), (0.06, 0.1)],
                    segs,
                    st.trim,
                );
            }
        }
        _ => {}
    }
}

/// A turned shape twisted into a double spiral (barley twist) between
/// heights `y0` and `y1`: `turns` per metre.
#[allow(clippy::too_many_arguments)]
fn twisted_lathe(
    mb: &mut MeshBuilder,
    at: Vec3,
    profile: &[(f32, f32)],
    segments: u32,
    y0: f32,
    y1: f32,
    turns: f32,
    color: Color,
) {
    // Many rings along the twist, so the spiral is smooth.
    let mut fine: Vec<(f32, f32)> = Vec::new();
    for w in profile.windows(2) {
        let (a, b) = (w[0], w[1]);
        let steps = (((b.0 - a.0) / 0.012).ceil() as usize).max(1);
        for k in 0..steps {
            let t = k as f32 / steps as f32;
            fine.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        }
    }
    fine.push(*profile.last().unwrap_or(&(0.0, 0.0)));
    let first = mb.vertices.len();
    mb.lathe(at, &fine, segments, color);
    for v in &mut mb.vertices[first..] {
        let p = v.position - at;
        if p.y <= y0 || p.y >= y1 {
            continue;
        }
        let fade = ((p.y - y0) / 0.04).min((y1 - p.y) / 0.04).clamp(0.0, 1.0);
        let a = p.z.atan2(p.x);
        let phase = 2.0 * a - p.y * turns * std::f32::consts::TAU;
        let groove = (0.5 - 0.5 * phase.cos()) * 0.32 * fade;
        let k = 1.0 - groove;
        v.position = at + vec3(p.x * k, p.y, p.z * k);
        // The groove's slope tilts the normal round and along the leg.
        let slope = phase.sin() * 0.32 * fade;
        let side = vec3(-a.sin(), 0.0, a.cos());
        let n = v.normal.truncate() - side * slope * 0.9 + Vec3::Y * slope * 0.4;
        v.normal = n.normalize_or_zero().extend(0.0);
    }
}

fn sights(kit: &mut Kit, st: &Style, table: &Table) {
    let spec = table.spec;
    let rail_outer = spec.rail_width + 0.05;
    let rail_mid = CUSHION_DEPTH as f64 + (rail_outer - CUSHION_DEPTH as f64) / 2.0;
    let mut sights = MeshBuilder::default();
    let mut put = |at: DVec2, along_x: bool| {
        let c = surface(at, RAIL_TOP + 0.0006);
        if st.diamonds {
            // A diamond, long across the rail.
            let (a, b) = if along_x { (0.006, 0.011) } else { (0.011, 0.006) };
            sights.quad(
                [
                    c + vec3(-a, 0.0, 0.0),
                    c + vec3(0.0, 0.0, b),
                    c + vec3(a, 0.0, 0.0),
                    c + vec3(0.0, 0.0, -b),
                ],
                [Vec2::ZERO; 4],
                st.sights,
            );
        } else {
            sights.disc(c, 0.0065, 12, st.sights);
        }
    };
    for k in (1..8).filter(|k| *k != 4) {
        let x = -spec.length / 2.0 + k as f64 * spec.length / 8.0;
        for sy in [1.0, -1.0] {
            put(DVec2::new(x, sy * (spec.width / 2.0 + rail_mid)), true);
        }
    }
    for k in 1..4 {
        let yy = -spec.width / 2.0 + k as f64 * spec.width / 4.0;
        for sx in [1.0, -1.0] {
            put(DVec2::new(sx * (spec.length / 2.0 + rail_mid), yy), false);
        }
    }
    let shiny = st.sights.r > 0.7 && st.sights.b < 0.5;
    let surface =
        if shiny { Surface::lit(2.0, 100.0, 0.1, 0.7) } else { Surface::lit(0.8, 60.0, 0.0, 0.3) };
    kit.add(sights, None, surface, Layer::Detail);
}

/// The inside of the pockets and their lips (or nets).
fn pockets(kit: &mut Kit, st: &Style, table: &Table) {
    let mut inside = MeshBuilder::default();
    let mut lips = MeshBuilder::default();
    let mut net = MeshBuilder::default();
    let nets = st.pockets == Pockets::Nets;
    let net_tex = if nets { Some(kit.texture(net_pixels(), true)) } else { None };
    let cast_h = if nets { CAST_H + 0.002 } else { CAST_H };
    for pocket in &table.pockets {
        let id = pocket.id;
        let r = pocket_radius_of(table, id);
        let center = surface(pocket_center_of(table, id), 0.0);
        let top = RAIL_TOP + cast_h + 0.0008;
        if nets {
            // A short well, then the net: a basket of cord seen from above.
            inside.tube_inside(center, r, top, 48, rgb(30, 18, 11));
            let depth = 0.12;
            let first = net.len();
            let rings = 6;
            for j in 0..rings {
                let (t0, t1) = (j as f32 / rings as f32, (j + 1) as f32 / rings as f32);
                let ra = r * (1.0 - 0.35 * t0 * t0);
                let rb = r * (1.0 - 0.35 * t1 * t1);
                for k in 0..32 {
                    let (a0, a1) = (
                        k as f32 / 32.0 * std::f32::consts::TAU,
                        (k + 1) as f32 / 32.0 * std::f32::consts::TAU,
                    );
                    let p = |a: f32, rr: f32, t: f32| {
                        center + vec3(a.cos() * rr, -depth * t, a.sin() * rr)
                    };
                    let uv = |a: f32, t: f32| vec2(a / std::f32::consts::TAU * 6.0, t * 1.4);
                    let n = |a: f32| -vec3(a.cos(), -0.4, a.sin()).normalize();
                    let i = [
                        net.push(p(a0, ra, t0), uv(a0, t0), n(a0), WHITE),
                        net.push(p(a1, ra, t0), uv(a1, t0), n(a1), WHITE),
                        net.push(p(a1, rb, t1), uv(a1, t1), n(a1), WHITE),
                        net.push(p(a0, rb, t1), uv(a0, t1), n(a0), WHITE),
                    ];
                    net.indices.extend_from_slice(&[i[0], i[1], i[2], i[0], i[2], i[3]]);
                }
            }
            let _ = first;
            inside.disc(center - Vec3::Y * depth, r * 0.66, 32, rgb(6, 4, 3));
            // A wide leather lip.
            lips.ring(center + Vec3::Y * (top + 0.0004), r, r + 0.016, 48, st.lip);
        } else {
            let bands = [
                (top, rgb(36, 24, 17)),
                (0.0, rgb(24, 16, 11)),
                (-0.04, rgb(9, 7, 6)),
                (-0.11, rgb(4, 3, 3)),
            ];
            for w in bands.windows(2) {
                let (y0, c0) = w[0];
                let (y1, _) = w[1];
                inside.tube_inside(center + Vec3::Y * y1, r, y0 - y1, 48, c0);
            }
            inside.disc(center - Vec3::Y * 0.11, r, 48, rgb(3, 3, 3));
            lips.ring(center + Vec3::Y * (top + 0.0004), r, r + 0.011, 48, st.lip);
        }
    }
    kit.add(inside, None, Surface::UNLIT.cut(3.0), Layer::Inside);
    if let Some(tex) = net_tex {
        kit.add(net, Some(tex), Surface::lit(0.1, 10.0, 0.0, 0.0).cut(3.0), Layer::Inside);
    }
    kit.add(lips, None, st.lip_surface.cut(2.0), Layer::Detail);
}

/// Bamboo canes side by side, standing up (8 canes a tile), with their nodes.
pub(crate) fn bamboo() -> Pixels {
    image(256, |u, v| {
        let canes = 8.0;
        let k = (u * canes).floor();
        let x = u * canes - k;
        // Round: lit from the left, dark at the seams.
        let round = (x * std::f32::consts::PI).sin();
        let tone = (hash(k as i32, 0, 5) - 0.5) * 0.16;
        let offset = hash(k as i32, 1, 7);
        let along = (v + offset).fract();
        // Nodes: a ridge every half tile.
        let node = ((along * 2.0).fract() - 0.5).abs();
        let ridge = (1.0 - (node - 0.47).abs() / 0.03).clamp(0.0, 1.0);
        let streak = noise(x * 6.0 + k * 3.1, v * 40.0, 40, 9) - 0.5;
        let light = [0.86, 0.72, 0.44];
        let dark = [0.42, 0.29, 0.13];
        let mut t = 0.75 - round * 0.55 + tone + streak * 0.12 + (1.0 - x) * 0.08;
        t += ridge * 0.35;
        if !(0.03..=0.97).contains(&x) {
            t = 1.1;
        }
        let _ = fbm;
        lerp3(light, dark, t)
    })
}

/// A net of cord (diamonds of knotted string) over the dark of the pocket.
fn net_pixels() -> Pixels {
    image(128, |u, v| {
        let (a, b) = ((u + v) * 4.0, (u - v + 1.0) * 4.0);
        let d = |x: f32| ((x.fract() - 0.5).abs() * 2.0 - 0.86).max(0.0) / 0.14;
        let cord = d(a).max(d(b));
        let knot = d(a).min(d(b));
        let c = cord * 0.8 + knot * 0.4;
        lerp3([0.02, 0.015, 0.012], [0.55, 0.40, 0.24], c)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every table builds at every quality, with the same pockets cut.
    #[test]
    fn every_table_builds() {
        let table = Table::default();
        for theme in [
            TableStyle::Classic,
            TableStyle::Beach,
            TableStyle::Club,
            TableStyle::Modern,
            TableStyle::Royal,
            TableStyle::Industrial,
            TableStyle::Tournament,
            TableStyle::Heritage,
            TableStyle::Deco,
        ] {
            for q in [Quality::Low, Quality::Ultra] {
                let kit = build(&table, q, theme);
                assert!(kit.parts.iter().any(|p| p.layer == Layer::Inside), "{q:?}");
                assert!(kit.parts.iter().any(|p| p.layer == Layer::Detail));
                let verts: usize = kit.parts.iter().map(|p| p.mesh.len()).sum();
                assert!(verts < 60_000, "{verts} vertices");
                for p in &kit.parts {
                    if let Some(t) = p.tex {
                        assert!(t < kit.textures.len());
                    }
                }
            }
        }
    }
}
