//! The professional tournament arena: the table alone on a blue stage under
//! a white light, the rest of the room in the dark — tiers of seats with
//! the crowd all round, banners hanging from the roof, the scoreboard over
//! the far stand, LED boards round the stage, TV cameras and a lighting
//! rig. Official tournaments are played in it; it is also an item only won.
//!
//! Built like every hall (see [`crate::halls`]): from code, with the light
//! baked into the vertices, the same size as the others so the cameras fit.

use macroquad::prelude::*;

use crate::config::Quality;
use crate::gfx::{Kit, Layer, MeshBuilder, Surface};
use crate::hall_textures as tx;
use crate::halls::{BRASS, GLASS, Lighting, add_glow, cell, floor_and_ceiling, light, rgb, walls};
use crate::room::{HEIGHT, HX, HZ, disc_facing, picture, tube};
use crate::scene::LAMP_Y;
use crate::textures::{self, Pixels, hash, image, lerp3};

/// How far the stage reaches from the middle (the LED boards stand on its
/// edge).
const STAGE: Vec2 = vec2(3.1, 2.15);
/// Where the first tier of seats begins, from the middle.
const STANDS: Vec2 = vec2(3.75, 2.75);
/// The height and depth of each tier.
const TIER: (f32, f32) = (0.26, 0.34);

/// The scoreboard's screen: two players' panels, blue and red, the frame
/// score between them in gold, the event's bar beneath (2:1).
fn scoreboard() -> Pixels {
    let size = 256u16;
    image(size, |u, v| {
        let v = v * 2.0;
        if v > 1.0 {
            return [6, 8, 14];
        }
        let border = !(0.02..=0.98).contains(&u) || !(0.04..=0.96).contains(&v);
        if border {
            return [40, 44, 56];
        }
        // The players' panels with their names as light bars.
        let panel = |x0: f32, base: [f32; 3]| -> Option<[u8; 3]> {
            if !(x0..x0 + 0.36).contains(&u) || !(0.12..0.62).contains(&v) {
                return None;
            }
            let lx = (u - x0) / 0.36;
            let ly = (v - 0.12) / 0.5;
            let name = (0.1..0.9).contains(&lx) && (0.2..0.38).contains(&ly);
            let flag = (0.1..0.28).contains(&lx) && (0.55..0.8).contains(&ly);
            let shade = 0.75 + 0.25 * (1.0 - ly);
            Some(if name {
                [235, 238, 245]
            } else if flag {
                [250, 210, 60]
            } else {
                lerp3([0.0; 3], base, shade)
            })
        };
        if let Some(c) = panel(0.06, [0.12, 0.36, 0.86]) {
            return c;
        }
        if let Some(c) = panel(0.58, [0.78, 0.14, 0.2]) {
            return c;
        }
        // The score in the middle: two gold blocks of digits.
        if (0.44..0.56).contains(&u) && (0.2..0.54).contains(&v) {
            let digit = (0.455..0.49).contains(&u) || (0.51..0.545).contains(&u);
            return if digit { [255, 214, 90] } else { [12, 14, 22] };
        }
        // The event's bar along the bottom, a gold line over it.
        if (0.72..0.74).contains(&v) {
            return [214, 170, 70];
        }
        if v > 0.76 {
            let stripe = ((u * 30.0).floor() as i32 % 2 == 0) as u8;
            return [18, 30 + stripe * 6, 70];
        }
        [10, 12, 20]
    })
}

/// A hanging banner: a deep colour with a gold edge, a stripe across and a
/// big ring for the emblem (`k`: which of the two colours).
fn banner(k: u32) -> Pixels {
    let base = if k == 0 { [0.07, 0.2, 0.55] } else { [0.5, 0.06, 0.1] };
    image(128, move |u, v| {
        let edge = !(0.06..=0.94).contains(&u);
        let gold = [0.86, 0.68, 0.28];
        if edge {
            return lerp3(gold, gold, 0.0);
        }
        let (x, y) = (u - 0.5, v - 0.32);
        let r = (x * x + y * y).sqrt();
        if (0.2..0.24).contains(&r) || ((0.62..0.66).contains(&v)) {
            return lerp3(gold, gold, 0.0);
        }
        if r < 0.13 {
            // The eight ball.
            return if (x * x + (y + 0.03) * (y + 0.03)).sqrt() < 0.06 {
                [240, 238, 230]
            } else {
                [14, 14, 16]
            };
        }
        let fold = 0.85 + 0.15 * (u * 9.0).sin();
        lerp3([0.0; 3], base, fold)
    })
}

/// An LED board: a bright gradient with moving-looking stripes (1:8).
fn led_board() -> Pixels {
    image(128, |u, v| {
        let band = ((u * 16.0 + v * 2.0).fract() < 0.5) as u8 as f32;
        let t = 0.5 + 0.5 * (u * std::f32::consts::TAU * 2.0).sin();
        let c = lerp3([0.02, 0.16, 0.5], [0.06, 0.5, 0.8], t);
        let k = 0.8 + 0.2 * band;
        [(c[0] as f32 * k) as u8, (c[1] as f32 * k) as u8, (c[2] as f32 * k) as u8]
    })
}

/// The arena around a table of half size `table` (with its rails).
pub fn arena(kit: &mut Kit, quality: Quality, table: Vec2) {
    let medium = quality >= Quality::Medium;
    let detailed = quality >= Quality::High;
    let carpet_tex = Some(kit.texture(tx::fabric([0.08, 0.2, 0.5], 211), true));
    let floor_tex = Some(kit.texture(tx::concrete(), true));
    let seat_tex = Some(kit.texture(tx::fabric([0.5, 0.52, 0.58], 213), true));
    let board_tex = Some(kit.texture(scoreboard(), false));
    let banner_tex = [Some(kit.texture(banner(0), false)), Some(kit.texture(banner(1), false))];
    let led_tex = Some(kit.texture(led_board(), true));
    let glow_tex = Some(kit.texture(textures::glow(), false));

    // TV light: a strong white light on the table, a cool wash from the
    // rig, the stands left in a dim blue.
    let mut lights = crate::halls::table_lights(vec3(1.0, 0.98, 0.94), table);
    for (x, z) in [(-2.2f32, -1.6f32), (2.2, -1.6), (-2.2, 1.6), (2.2, 1.6)] {
        lights.push(light(vec3(x, 2.95, z), vec3(0.75, 0.8, 0.95), 0.55, true));
    }
    for (x, z) in [(0.0f32, -HZ + 0.6), (0.0, HZ - 0.6), (-HX + 0.6, 0.0), (HX - 0.6, 0.0)] {
        lights.push(light(vec3(x, 2.6, z), vec3(0.18, 0.24, 0.45), 0.5, false));
    }
    lights.push(light(vec3(0.0, 2.6, -HZ + 0.4), vec3(0.4, 0.45, 0.7), 0.4, false));
    let mut lit = Lighting::new(quality, lights, vec3(0.03, 0.035, 0.06), table, 1.9);
    let cell = cell(quality);

    // The floor: black, with the blue stage carpet round the table.
    let mut floor = MeshBuilder::default();
    let mut ceiling = MeshBuilder::default();
    floor_and_ceiling(&mut floor, &mut ceiling, cell, 0.4, 1.0, rgb(8, 8, 12));
    for v in &mut floor.vertices {
        v.color = [34, 34, 40, 255];
    }
    lit.floor(kit, floor, floor_tex, 0.25, (0.6, 60.0, 0.3));
    lit.add(kit, ceiling, None);
    let mut carpet = MeshBuilder::default();
    let (sx, sz) = (STAGE.x - 0.05, STAGE.y - 0.05);
    carpet.grid(
        vec3(-sx, 0.006, sz),
        vec3(2.0 * sx, 0.0, 0.0),
        vec3(0.0, 0.0, -2.0 * sz),
        (2.0 * sx / cell).ceil() as u32,
        (2.0 * sz / cell).ceil() as u32,
        WHITE,
        1.0 / 1.2,
    );
    lit.add(kit, carpet, carpet_tex);

    // The walls, black, and the stands rising to them on every side.
    let mut walls_mb = MeshBuilder::default();
    for w in walls() {
        crate::halls::band(
            &mut walls_mb,
            &w,
            0.0,
            w.length,
            0.0,
            HEIGHT,
            cell,
            rgb(16, 16, 22),
            1.0,
        );
    }
    lit.add(kit, walls_mb, None);
    let mut steps = MeshBuilder::default();
    let mut seats = MeshBuilder::default();
    let mut crowd = MeshBuilder::default();
    let tiers = ((HZ - STANDS.y) / TIER.1).floor() as i32;
    // The crowd in the dark: muted clothes, faces in the half light.
    let clothes = [
        rgb(24, 26, 36),
        rgb(44, 16, 20),
        rgb(18, 32, 26),
        rgb(50, 50, 56),
        rgb(14, 14, 16),
        rgb(22, 36, 66),
    ];
    let skin = [rgb(120, 94, 76), rgb(96, 68, 48), rgb(64, 44, 30), rgb(132, 110, 94)];
    let seat = rgb(40, 52, 96);
    for k in 0..tiers {
        let h = (k + 1) as f32 * TIER.0;
        let (ox, oz) = (STANDS.x + k as f32 * TIER.1, STANDS.y + k as f32 * TIER.1);
        // Each tier is a ring of four slabs (front and back, then the ends
        // between them).
        for side in [-1.0f32, 1.0] {
            let z = side * (oz + (HZ - oz) / 2.0);
            steps.cuboid(
                vec3(0.0, h / 2.0, z),
                vec3(2.0 * HX, h, HZ - oz),
                0.0,
                rgb(30, 32, 40),
                1.0,
            );
            let x = side * (ox + (HX - ox) / 2.0);
            steps.cuboid(
                vec3(x, h / 2.0, 0.0),
                vec3(HX - ox, h, 2.0 * oz),
                0.0,
                rgb(30, 32, 40),
                1.0,
            );
        }
        lit.occlude(vec3(-HX, 0.0, -HZ), vec3(HX, h, -oz));
        lit.occlude(vec3(-HX, 0.0, oz), vec3(HX, h, HZ));
        if !medium {
            continue;
        }
        // The seats on the tier, and the crowd in most of them.
        let spacing = 0.55;
        let row = |from: Vec3, along: Vec3, length: f32, facing: Vec3, seed: i32| {
            let n = (length / spacing) as i32;
            (0..n)
                .map(move |i| {
                    let p = from + along * ((i as f32 + 0.5) * spacing) + facing * 0.12;
                    (p, facing, seed * 1000 + i)
                })
                .collect::<Vec<_>>()
        };
        let mut places = Vec::new();
        for side in [-1.0f32, 1.0] {
            let z = side * (oz + TIER.1 * 0.5);
            places.extend(row(
                vec3(-HX + 0.4, h, z),
                Vec3::X,
                2.0 * HX - 0.8,
                vec3(0.0, 0.0, -side),
                k * 4 + (side > 0.0) as i32,
            ));
            let x = side * (ox + TIER.1 * 0.5);
            places.extend(row(
                vec3(x, h, -oz + 0.3),
                Vec3::Z,
                2.0 * oz - 0.6,
                vec3(-side, 0.0, 0.0),
                k * 4 + 2 + (side > 0.0) as i32,
            ));
        }
        for (p, facing, seed) in places {
            let yaw = facing.x.atan2(facing.z);
            seats.cuboid(
                p + vec3(0.0, 0.2, 0.0) - facing * 0.08,
                vec3(0.44, 0.04, 0.38),
                yaw,
                seat,
                1.0,
            );
            seats.cuboid(
                p + vec3(0.0, 0.42, 0.0) - facing * 0.26,
                vec3(0.44, 0.42, 0.04),
                yaw,
                seat,
                1.0,
            );
            if hash(seed, 3, 17) < 0.82 && detailed {
                let body =
                    clothes[(hash(seed, 5, 19) * clothes.len() as f32) as usize % clothes.len()];
                let face = skin[(hash(seed, 7, 23) * skin.len() as f32) as usize % skin.len()];
                crowd.cuboid(
                    p + vec3(0.0, 0.46, 0.0) - facing * 0.1,
                    vec3(0.32, 0.44, 0.2),
                    yaw,
                    body,
                    1.0,
                );
                let head = p + vec3(0.0, 0.8, 0.0) - facing * 0.08;
                crowd.lathe(
                    head - Vec3::Y * 0.08,
                    &[(0.0, 0.0), (0.03, 0.06), (0.09, 0.075), (0.15, 0.055), (0.18, 0.0)],
                    8,
                    face,
                );
            }
        }
    }
    lit.add(kit, steps, None);
    if medium {
        lit.add(kit, seats, seat_tex);
        lit.add(kit, crowd, None);
    }

    // The LED boards round the stage (an opening at each corner).
    let mut boards = MeshBuilder::default();
    let mut board_backs = MeshBuilder::default();
    let mut halos: Vec<(Vec3, f32, Color)> = Vec::new();
    let board_h = 0.34;
    for side in [-1.0f32, 1.0] {
        for (a, b) in [(-STAGE.x + 0.5, -0.4), (0.4, STAGE.x - 0.5)] {
            let z = side * STAGE.y;
            let c = vec3((a + b) / 2.0, board_h / 2.0 + 0.05, z);
            board_backs.cuboid(
                c + vec3(0.0, 0.0, side * 0.04),
                vec3(b - a, board_h + 0.1, 0.07),
                0.0,
                rgb(14, 14, 18),
                1.0,
            );
            picture(
                &mut boards,
                c - vec3(0.0, 0.0, side * 0.002),
                Vec3::X * -side,
                Vec3::Y,
                b - a - 0.04,
                board_h,
                WHITE,
            );
            if detailed {
                halos.push((c - vec3(0.0, 0.0, side * 0.3), 1.0, Color::new(0.2, 0.6, 1.0, 0.1)));
            }
        }
        let x = side * STAGE.x;
        let (a, b) = (-STAGE.y + 0.5, STAGE.y - 0.5);
        let c = vec3(x, board_h / 2.0 + 0.05, 0.0);
        board_backs.cuboid(
            c + vec3(side * 0.04, 0.0, 0.0),
            vec3(0.07, board_h + 0.1, b - a),
            0.0,
            rgb(14, 14, 18),
            1.0,
        );
        picture(
            &mut boards,
            c - vec3(side * 0.002, 0.0, 0.0),
            Vec3::Z * side,
            Vec3::Y,
            b - a - 0.04,
            board_h,
            WHITE,
        );
    }
    lit.shiny(kit, board_backs, None, (0.6, 50.0, 0.3));
    add_glow(kit, boards, led_tex);

    // The scoreboard over the far stand, in a black frame with a gold rim.
    let mut frame = MeshBuilder::default();
    let mut screen = MeshBuilder::default();
    let sc = vec3(0.0, 2.45, -HZ + 0.12);
    frame.cuboid(sc - Vec3::Z * 0.04, vec3(3.3, 1.05, 0.08), 0.0, rgb(12, 12, 16), 1.0);
    picture(&mut screen, sc + Vec3::Z * 0.002, Vec3::X, Vec3::Y, 3.1, 0.9, WHITE);
    halos.push((sc + Vec3::Z * 0.3, 2.2, Color::new(0.4, 0.5, 1.0, 0.14)));
    let mut gold = MeshBuilder::default();
    for (a, b) in [
        (vec3(-1.66, 1.93, 0.0), vec3(1.66, 1.93, 0.0)),
        (vec3(-1.66, 2.97, 0.0), vec3(1.66, 2.97, 0.0)),
    ] {
        tube(
            &mut gold,
            a + Vec3::Z * (-HZ + 0.17),
            b + Vec3::Z * (-HZ + 0.17),
            0.012,
            6,
            rgb(214, 170, 70),
        );
    }
    add_glow(kit, screen, board_tex);

    // Banners hanging from the roof along the long sides.
    let mut banners = [MeshBuilder::default(), MeshBuilder::default()];
    let mut poles = MeshBuilder::default();
    if medium {
        for (i, x) in [-4.2f32, -2.1, 0.0, 2.1, 4.2].into_iter().enumerate() {
            for side in [-1.0f32, 1.0] {
                if side < 0.0 && x.abs() < 2.0 {
                    continue; // The scoreboard is there.
                }
                let z = side * (HZ - 0.25);
                let c = vec3(x, 2.55, z);
                picture(
                    &mut banners[(i + (side > 0.0) as usize) % 2],
                    c,
                    Vec3::X * -side,
                    Vec3::Y,
                    0.8,
                    1.2,
                    WHITE,
                );
                tube(
                    &mut poles,
                    c + vec3(-0.45, 0.62, 0.0),
                    c + vec3(0.45, 0.62, 0.0),
                    0.012,
                    6,
                    rgb(180, 182, 190),
                );
                tube(
                    &mut poles,
                    c + vec3(0.0, 0.62, 0.0),
                    vec3(x, HEIGHT, z),
                    0.004,
                    4,
                    rgb(60, 60, 64),
                );
            }
        }
    }
    for (k, b) in banners.into_iter().enumerate() {
        lit.add(kit, b, banner_tex[k]);
    }

    // The lighting rig: a square truss over the table with floodlights
    // aimed at it.
    let mut truss = MeshBuilder::default();
    let mut lamps = MeshBuilder::default();
    let mut lenses = MeshBuilder::default();
    let (tx_, tz) = (2.6f32, 1.9f32);
    let ty = 2.95;
    let corners = [vec3(-tx_, ty, -tz), vec3(tx_, ty, -tz), vec3(tx_, ty, tz), vec3(-tx_, ty, tz)];
    for k in 0..4 {
        let (a, b) = (corners[k], corners[(k + 1) % 4]);
        for dy in [0.0f32, 0.22] {
            tube(&mut truss, a + Vec3::Y * dy, b + Vec3::Y * dy, 0.018, 6, rgb(150, 152, 160));
        }
        if detailed {
            let n = (a.distance(b) / 0.3) as i32;
            for i in 0..n {
                let p = a.lerp(b, i as f32 / n as f32);
                let q = a.lerp(b, (i + 1) as f32 / n as f32);
                tube(&mut truss, p, q + Vec3::Y * 0.22, 0.007, 4, rgb(150, 152, 160));
            }
        }
        tube(&mut truss, a + Vec3::Y * 0.22, vec3(a.x, HEIGHT, a.z), 0.01, 4, rgb(70, 70, 76));
        // Two floodlights on each side, aimed down at the table.
        for f in [0.3f32, 0.7] {
            let p = a.lerp(b, f) - Vec3::Y * 0.08;
            let aim = (vec3(0.0, crate::coords::TABLE_HEIGHT, 0.0) - p).normalize();
            lamps.lathe(
                p,
                &[(0.0, 0.0), (0.0, 0.07), (0.16, 0.09), (0.18, 0.0)],
                12,
                rgb(24, 24, 28),
            );
            disc_facing(&mut lenses, p + aim * 0.1, aim, 0.07, 14, rgb(255, 252, 240));
            halos.push((p + aim * 0.14, 0.45, Color::new(1.0, 0.98, 0.9, 0.3)));
        }
    }
    lit.shiny(kit, truss, None, BRASS);
    lit.shiny(kit, lamps, None, (0.6, 40.0, 0.2));
    add_glow(kit, lenses, None);
    lit.shiny(kit, gold, None, BRASS);
    lit.add(kit, poles, None);

    // TV cameras on tripods at two corners of the stage.
    if medium {
        let mut cams = MeshBuilder::default();
        let mut glass = MeshBuilder::default();
        for (x, z) in [(-STAGE.x + 0.25, STAGE.y - 0.25), (STAGE.x - 0.25, -STAGE.y + 0.25)] {
            let foot = vec3(x, 0.0, z);
            let head = foot + Vec3::Y * 1.45;
            for a in 0..3 {
                let ang = a as f32 / 3.0 * std::f32::consts::TAU;
                let leg = foot + vec3(ang.cos() * 0.35, 0.0, ang.sin() * 0.35);
                tube(&mut cams, leg, head - Vec3::Y * 0.1, 0.012, 5, rgb(30, 30, 34));
            }
            let to_table = vec3(-x, 0.0, -z).normalize();
            let yaw = to_table.x.atan2(to_table.z);
            cams.cuboid(head + Vec3::Y * 0.1, vec3(0.2, 0.22, 0.48), yaw, rgb(36, 38, 44), 1.0);
            let lens = head + Vec3::Y * 0.1 + to_table * 0.3;
            tube(&mut cams, head + Vec3::Y * 0.1 + to_table * 0.2, lens, 0.07, 12, rgb(20, 20, 22));
            disc_facing(&mut glass, lens + to_table * 0.002, to_table, 0.06, 14, rgb(40, 60, 90));
            // The red light that says it is live.
            halos.push((head + vec3(0.0, 0.24, 0.0), 0.1, Color::new(1.0, 0.1, 0.1, 0.6)));
        }
        lit.shiny(kit, cams, None, (0.7, 50.0, 0.3));
        lit.shiny(kit, glass, None, GLASS);
    }

    if medium {
        // The light of the stage washing over the first tiers, and the
        // LED boards' glow on the carpet.
        let mut pools = MeshBuilder::default();
        for side in [-1.0f32, 1.0] {
            crate::halls::pool(
                &mut pools,
                vec3(0.0, 0.01, side * (STAGE.y - 0.4)),
                Vec3::Y,
                5.0,
                0.8,
                Color::new(0.2, 0.55, 1.0, 0.12),
            );
        }
        kit.add(pools, glow_tex, Surface::UNLIT, Layer::Blended);
    }
    for (at, size, color) in halos {
        kit.glow(at, size, color, false);
    }
}

/// The TV light over the table: a long black canopy with a white panel
/// beneath and a silver frame, hung on cables.
pub fn arena_lamp(kit: &mut Kit, reach: f32) {
    let grow = reach - 0.45;
    let mut body = MeshBuilder::default();
    let mut glow = MeshBuilder::default();
    let (l, w) = (2.0 + 2.0 * grow, 0.9f32);
    let y = LAMP_Y + 0.08;
    body.cuboid(vec3(0.0, y + 0.05, 0.0), vec3(l, 0.1, w), 0.0, rgb(14, 14, 16), 1.0);
    let (hx, hz) = (l / 2.0 - 0.05, w / 2.0 - 0.05);
    glow.quad(
        [
            vec3(-hx, y - 0.001, hz),
            vec3(hx, y - 0.001, hz),
            vec3(hx, y - 0.001, -hz),
            vec3(-hx, y - 0.001, -hz),
        ],
        [Vec2::ZERO; 4],
        rgb(255, 255, 250),
    );
    let c = |x: f32, z: f32| vec3(x, y, z);
    let (ex, ez) = (l / 2.0 + 0.01, w / 2.0 + 0.01);
    let ring = [c(-ex, -ez), c(ex, -ez), c(ex, ez), c(-ex, ez), c(-ex, -ez)];
    for s in ring.windows(2) {
        tube(&mut body, s[0], s[1], 0.012, 6, rgb(190, 194, 202));
    }
    for (x, z) in [
        (-l / 2.0 + 0.1, -w / 2.0 + 0.1),
        (l / 2.0 - 0.1, w / 2.0 - 0.1),
        (-l / 2.0 + 0.1, w / 2.0 - 0.1),
        (l / 2.0 - 0.1, -w / 2.0 + 0.1),
    ] {
        tube(&mut body, vec3(x, y + 0.1, z), vec3(x, HEIGHT, z), 0.003, 4, rgb(30, 30, 30));
    }
    kit.add(body, None, Surface::lit(1.0, 80.0, 0.1, 0.5), Layer::Lamp);
    kit.add(glow, None, Surface::UNLIT, Layer::Lamp);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arena_builds_at_every_quality() {
        let table = vec2(1.27, 0.75);
        for q in [Quality::Low, Quality::Medium, Quality::High, Quality::Ultra] {
            let mut kit = Kit::default();
            arena(&mut kit, q, table);
            arena_lamp(&mut kit, 0.45);
            let verts: usize = kit.parts.iter().map(|p| p.mesh.len()).sum();
            assert!(verts > 1000 && verts < 400_000, "{q:?}: {verts} vertices");
            for p in &kit.parts {
                if let Some(t) = p.tex {
                    assert!(t < kit.textures.len());
                }
            }
        }
    }
}
