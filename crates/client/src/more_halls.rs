//! The newer halls of the shop: the mountain lodge, the neon arcade, the
//! art deco lounge and the penthouse (the first four are in
//! [`crate::halls`] and [`crate::room`]).
//!
//! Built like the others: from code, with the light baked into the vertices
//! (see [`Lighting`]), the same size as every hall so the cameras fit.

use macroquad::prelude::*;

use crate::config::Quality;
use crate::gfx::{Kit, Layer, MeshBuilder, Surface};
use crate::hall_textures as tx;
use crate::halls::{
    BRASS, GLASS, LEATHER, Lighting, SATIN, Wall, add_glow, backdrop, band, bottles, cell, cushion,
    floor_and_ceiling, lamp_xs, light, potted_plant, rgb, rug, sofa, table_lights, walls,
};
use crate::room::{HEIGHT, HX, HZ, disc_facing, hoop, picture, tube};
use crate::scene::LAMP_Y;
use crate::textures;

/// The two textures of each hall made side by side (they take a while).
fn both<A: Send, B: Send>(a: impl FnOnce() -> A + Send, b: impl FnOnce() -> B + Send) -> (A, B) {
    std::thread::scope(|s| {
        let a = s.spawn(a);
        let b = s.spawn(b);
        (a.join().expect("texture"), b.join().expect("texture"))
    })
}

/// A box lying along a wall: `along` metres along it, `y` up, `out` from
/// it into the room, of size (along, up, out).
fn wall_box(
    mb: &mut MeshBuilder,
    w: &Wall,
    along: f32,
    y: f32,
    out: f32,
    size: Vec3,
    color: Color,
) {
    let center = w.start + w.along * along + Vec3::Y * y + w.inward * out;
    let size = w.along.abs() * size.x + Vec3::Y * size.y + w.inward.abs() * size.z;
    mb.cuboid(center, size, 0.0, color, 1.0);
}

/// A wall between heights `y0` and `y1` with rectangular openings: `holes`
/// are (from, to, bottom, top) along the wall, in order.
#[allow(clippy::too_many_arguments)]
fn wall_with_holes(
    mb: &mut MeshBuilder,
    w: &Wall,
    holes: &[(f32, f32, f32, f32)],
    y0: f32,
    y1: f32,
    cell: f32,
    color: Color,
    uv: f32,
) {
    let mut at = 0.0;
    for &(a, b, lo, hi) in holes {
        if a > at {
            band(mb, w, at, a, y0, y1, cell, color, uv);
        }
        if lo > y0 {
            band(mb, w, a, b, y0, lo, cell, color, uv);
        }
        if hi < y1 {
            band(mb, w, a, b, hi, y1, cell, color, uv);
        }
        at = b;
    }
    if at < w.length {
        band(mb, w, at, w.length, y0, y1, cell, color, uv);
    }
}

/// The frame of a window in a wall: the reveals round the opening (the
/// wall is `depth` thick) and the bars of the glazing, `cols` × `rows`
/// panes.
#[allow(clippy::too_many_arguments)]
fn window_frame(
    mb: &mut MeshBuilder,
    w: &Wall,
    (a, b, lo, hi): (f32, f32, f32, f32),
    depth: f32,
    (cols, rows): (u32, u32),
    bar: f32,
    color: Color,
) {
    let (mid, wide, tall) = ((a + b) / 2.0, b - a, hi - lo);
    // The reveals: two jambs, the sill and the head, set into the wall.
    for x in [a + bar / 2.0, b - bar / 2.0] {
        wall_box(mb, w, x, (lo + hi) / 2.0, -depth / 2.0, vec3(bar, tall, depth), color);
    }
    wall_box(mb, w, mid, lo, -depth / 2.0 + 0.03, vec3(wide + 0.08, 0.05, depth + 0.06), color);
    wall_box(mb, w, mid, hi - bar / 2.0, -depth / 2.0, vec3(wide, bar, depth), color);
    // The glazing bars, a little behind the wall's face.
    for k in 1..cols {
        let x = a + wide * k as f32 / cols as f32;
        wall_box(mb, w, x, (lo + hi) / 2.0, -depth * 0.6, vec3(bar * 0.5, tall, bar * 0.6), color);
    }
    for k in 1..rows {
        let y = lo + tall * k as f32 / rows as f32;
        wall_box(mb, w, mid, y, -depth * 0.6, vec3(wide, bar * 0.5, bar * 0.6), color);
    }
}

/// A fire in a firebox facing `out`: logs on a grate, flames and embers.
fn fire(dark: &mut MeshBuilder, flames: &mut MeshBuilder, at: Vec3, out: Vec3, width: f32) {
    let side = Vec3::Y.cross(out).normalize_or_zero();
    for (d, h, s) in [(0.0f32, 0.08f32, -0.35f32), (0.08, 0.08, 0.3), (0.03, 0.15, 0.0)] {
        let c = at + out * d + Vec3::Y * h + side * s * width * 0.2;
        tube(dark, c - side * width * 0.42, c + side * width * 0.42, 0.045, 8, rgb(56, 32, 18));
    }
    let colors = [rgb(255, 150, 40), rgb(255, 205, 95), rgb(255, 110, 30)];
    let n = (width / 0.07) as i32;
    for k in 0..n {
        let s = (k as f32 + 0.5) / n as f32 - 0.5;
        let h = 0.2 + ((k * 37 % 5) as f32) * 0.06 - s.abs() * 0.2;
        let base = at + side * s * width * 0.9 + out * ((k % 2) as f32 * 0.05) + Vec3::Y * 0.1;
        // Two crossed tongues, so the fire has body from any side.
        for tilt in [0.0f32, 0.6] {
            let across = (side + out * tilt).normalize_or_zero();
            flames.quad(
                [
                    base - across * 0.05,
                    base + across * 0.05,
                    base + Vec3::Y * h + across * 0.01,
                    base + Vec3::Y * h - across * 0.01,
                ],
                [Vec2::ZERO; 4],
                colors[(k as usize) % 3],
            );
        }
    }
    let e = side * width * 0.45;
    let o = out * 0.12;
    flames.quad(
        [
            at - e - o + Vec3::Y * 0.045,
            at + e - o + Vec3::Y * 0.045,
            at + e + o + Vec3::Y * 0.045,
            at - e + o + Vec3::Y * 0.045,
        ],
        [Vec2::ZERO; 4],
        rgb(225, 75, 22),
    );
}

/// Three shades over the table on a bar: the shape of each (a lathe
/// profile from its rim up), their colour and the inner glow.
#[allow(clippy::too_many_arguments)]
fn shades_on_bar(
    kit: &mut Kit,
    quality: Quality,
    reach: f32,
    profile: &[(f32, f32)],
    shell: (Color, Surface),
    inner: Color,
    bar: (Color, Surface),
) {
    let segs = if quality != Quality::Low { 32 } else { 18 };
    let mut shades = MeshBuilder::default();
    let mut glow = MeshBuilder::default();
    let mut rods = MeshBuilder::default();
    let rim = profile[0].1;
    let grow = reach - 0.45;
    for x in lamp_xs(reach) {
        let at = vec3(x, LAMP_Y, 0.0);
        shades.lathe(at, profile, segs, shell.0);
        glow.frustum_at(at, 0.002, rim * 0.97, 0.08, rim * 0.7, segs, inner);
        glow.disc(at + Vec3::Y * 0.08, rim * 0.7, segs, inner);
        glow.disc(at + Vec3::Y * 0.05, 0.035, 16, rgb(255, 252, 240));
        let top = profile.last().map_or(0.2, |p| p.0);
        tube(&mut rods, at + Vec3::Y * top, vec3(x, LAMP_Y + 0.25, 0.0), 0.006, 6, bar.0);
    }
    rods.cuboid(vec3(0.0, LAMP_Y + 0.25, 0.0), vec3(1.2 + 2.0 * grow, 0.03, 0.04), 0.0, bar.0, 1.0);
    for x in [-0.55 - grow, 0.55 + grow] {
        tube(&mut rods, vec3(x, LAMP_Y + 0.25, 0.0), vec3(x, HEIGHT, 0.0), 0.004, 6, bar.0);
    }
    kit.add(shades, None, shell.1, Layer::Lamp);
    kit.add(rods, None, bar.1, Layer::Lamp);
    kit.add(glow, None, Surface::UNLIT, Layer::Lamp);
}

// --- The mountain lodge --------------------------------------------------------

/// A mountain lodge at dusk: walls of round logs, a ceiling of pine boards on
/// heavy beams, a fieldstone chimney with a roaring fire, tall windows on
/// snowy peaks, a wide plank floor, a tartan rug, leather and wool.
pub fn lodge(kit: &mut Kit, quality: Quality, table: Vec2) {
    let medium = quality >= Quality::Medium;
    let detailed = quality >= Quality::High;
    let big = quality == Quality::Ultra;
    let ((logs_px, stone_px), (sky_px, floor_px)) = both(
        || both(tx::logs, tx::fieldstone),
        || both(|| tx::mountains(5), || textures::floor_sized(if big { 512 } else { 256 })),
    );
    let logs_tex = Some(kit.texture(logs_px, true));
    let stone_tex = Some(kit.texture(stone_px, true));
    let sky_tex = Some(kit.texture(sky_px, false));
    let floor_tex = Some(kit.texture(floor_px, true));
    let pine_tex = Some(kit.texture(tx::planks([0.78, 0.56, 0.34], [0.42, 0.26, 0.14], 167), true));
    let rug_tex = Some(kit.texture(tartan(), true));
    let wool_tex = Some(kit.texture(tx::fabric([0.42, 0.36, 0.30], 169), true));
    let glow_tex = Some(kit.texture(textures::glow(), false));

    // The fire on the left wall, lanterns, and the cold light of the snow.
    let fire_at = vec3(-HX + 0.42, 0.3, 0.0);
    let mut lights = table_lights(vec3(1.0, 0.82, 0.58), table);
    lights.push(light(fire_at + Vec3::X * 0.3, vec3(1.0, 0.45, 0.14), 1.9, false));
    for (x, z) in [(-2.6f32, HZ - 0.15), (2.6, HZ - 0.15), (HX - 0.15, -1.6), (HX - 0.15, 1.6)] {
        lights.push(light(vec3(x, 1.95, z), vec3(1.0, 0.68, 0.36), 0.45, false));
    }
    for x in [-3.0f32, 0.0, 3.0] {
        lights.push(light(vec3(x, 1.7, -HZ - 1.0), vec3(0.55, 0.62, 0.9), 0.55, false));
    }
    let mut lit = Lighting::new(quality, lights, vec3(0.26, 0.21, 0.18), table, 3.3);
    let cell = cell(quality);
    // The chimney breast, the sofa and the armchairs cast shadows.
    lit.occlude_box(vec3(-HX + 0.3, 1.6, 0.0), vec3(0.6, 3.2, 2.4));
    lit.occlude_box(vec3(-2.9, 0.4, 0.0), vec3(0.9, 0.8, 2.3));

    // Floor (wide oak, oiled) and the ceiling of pine boards.
    let mut floor = MeshBuilder::default();
    let mut ceiling = MeshBuilder::default();
    floor_and_ceiling(&mut floor, &mut ceiling, cell, 0.24, 1.0 / 1.1, WHITE);
    for v in &mut floor.vertices {
        v.color = [255, 226, 200, 255];
    }
    lit.floor(kit, floor, floor_tex, 0.12, (0.35, 40.0, 0.2));
    lit.add(kit, ceiling, pine_tex);

    // Log walls; three tall windows in the back wall, two in the right.
    const SILL: f32 = 0.75;
    const HEAD: f32 = 2.55;
    let mut logwall = MeshBuilder::default();
    let mut timber = MeshBuilder::default();
    let mut sky = MeshBuilder::default();
    let frame_wood = rgb(120, 78, 44);
    for (i, w) in walls().iter().enumerate() {
        let holes: Vec<(f32, f32, f32, f32)> = match i {
            0 => [-3.2f32, 0.0, 3.2]
                .iter()
                .map(|x| (x + HX - 0.75, x + HX + 0.75, SILL, HEAD))
                .collect(),
            3 if medium => vec![(1.4, 3.2, SILL, HEAD), (4.8, 6.6, SILL, HEAD)],
            _ => Vec::new(),
        };
        wall_with_holes(&mut logwall, w, &holes, 0.0, HEIGHT, cell, WHITE, 1.0);
        for &h in &holes {
            window_frame(&mut timber, w, h, 0.28, (2, 3), 0.07, frame_wood);
        }
        if !holes.is_empty() {
            backdrop(&mut sky, w, 1.2, -1.0, 3.4, false, (w.length / 6.0).round().max(1.0));
        }
        // Corner posts: the logs' ends meet in a notched corner.
        tube(
            &mut timber,
            w.start + w.inward * 0.12 + w.along * 0.12,
            w.start + w.inward * 0.12 + w.along * 0.12 + Vec3::Y * HEIGHT,
            0.16,
            12,
            rgb(150, 100, 60),
        );
    }
    lit.add(kit, logwall, logs_tex);
    add_glow(kit, sky, sky_tex);

    // Heavy beams across the ceiling and a ridge beam along it.
    let beam = rgb(140, 92, 54);
    for k in -3..=3 {
        let x = k as f32 * 1.6;
        tube(&mut timber, vec3(x, HEIGHT - 0.14, -HZ), vec3(x, HEIGHT - 0.14, HZ), 0.13, 12, beam);
    }
    tube(&mut timber, vec3(-HX, HEIGHT - 0.3, 0.0), vec3(HX, HEIGHT - 0.3, 0.0), 0.16, 14, beam);

    // The chimney breast: fieldstone from floor to ceiling, a firebox, a
    // log mantel, a raised stone hearth.
    let mut stone = MeshBuilder::default();
    let mut dark = MeshBuilder::default();
    let mut flames = MeshBuilder::default();
    let fx = -HX;
    let grey = rgb(235, 228, 220);
    stone.cuboid(vec3(fx + 0.3, 1.35, -0.85), vec3(0.6, 2.7, 0.7), 0.0, grey, 1.0 / 0.9);
    stone.cuboid(vec3(fx + 0.3, 1.35, 0.85), vec3(0.6, 2.7, 0.7), 0.0, grey, 1.0 / 0.9);
    stone.cuboid(vec3(fx + 0.3, 1.25, 0.0), vec3(0.6, 0.5, 1.0), 0.0, grey, 1.0 / 0.9);
    stone.cuboid(vec3(fx + 0.25, 2.95, 0.0), vec3(0.5, 0.5, 2.2), 0.0, grey, 1.0 / 0.9);
    stone.cuboid(
        vec3(fx + 0.55, 0.12, 0.0),
        vec3(1.1, 0.24, 2.6),
        0.0,
        rgb(210, 204, 196),
        1.0 / 0.9,
    );
    // The firebox, black with soot.
    dark.cuboid(vec3(fx + 0.06, 0.62, 0.0), vec3(0.12, 0.76, 1.0), 0.0, rgb(18, 13, 10), 1.0);
    dark.cuboid(vec3(fx + 0.3, 0.995, 0.0), vec3(0.5, 0.01, 1.0), 0.0, rgb(24, 18, 14), 1.0);
    for sz in [-1.0f32, 1.0] {
        dark.cuboid(
            vec3(fx + 0.3, 0.62, sz * 0.495),
            vec3(0.5, 0.76, 0.01),
            0.0,
            rgb(30, 22, 16),
            1.0,
        );
    }
    fire(&mut dark, &mut flames, vec3(fx + 0.32, 0.24, 0.0), Vec3::X, 0.8);
    // The mantel: a split log on stone corbels.
    timber.cuboid(vec3(fx + 0.68, 1.56, 0.0), vec3(0.26, 0.16, 2.3), 0.0, rgb(150, 98, 56), 1.0);
    // A pair of antlers over the mantel.
    let antler = rgb(220, 205, 180);
    if detailed {
        let base = vec3(fx + 0.62, 2.2, 0.0);
        for sz in [-1.0f32, 1.0] {
            let mut p = base + vec3(0.0, 0.0, sz * 0.04);
            for k in 0..5 {
                let q = p + vec3(0.03, 0.12 - k as f32 * 0.01, sz * (0.12 - k as f32 * 0.01));
                tube(&mut stone, p, q, 0.016 - k as f32 * 0.002, 6, antler);
                // A tine up from each joint.
                tube(&mut stone, q, q + vec3(0.02, 0.12, sz * 0.01), 0.008, 5, antler);
                p = q;
            }
        }
    }
    // Firewood stacked beside the hearth.
    if medium {
        for row in 0..4 {
            for k in 0..(5 - row) {
                let z = 1.45 + (k as f32 + row as f32 * 0.5) * 0.15;
                let y = 0.3 + row as f32 * 0.13;
                tube(
                    &mut timber,
                    vec3(fx + 0.15, y, z),
                    vec3(fx + 0.6, y, z),
                    0.065,
                    8,
                    rgb(130, 90, 56),
                );
                disc_facing(
                    &mut stone,
                    vec3(fx + 0.601, y, z),
                    Vec3::X,
                    0.065,
                    8,
                    rgb(196, 160, 110),
                );
            }
        }
    }
    let mut halos: Vec<(Vec3, f32, Color)> =
        vec![(fire_at + vec3(0.2, 0.15, 0.0), 0.6, Color::new(1.0, 0.5, 0.18, 0.45))];

    // Iron lanterns on the walls.
    let mut iron = MeshBuilder::default();
    let mut glow = MeshBuilder::default();
    let walls = walls();
    for (wi, along) in [(1usize, HX - 2.6), (1, HX + 2.6), (3, HZ - 1.6), (3, HZ + 1.6)] {
        let w = &walls[wi];
        let at = w.start + w.along * along + Vec3::Y * 1.95 + w.inward * 0.18;
        iron.cuboid(
            at - w.inward * 0.12 + Vec3::Y * 0.1,
            vec3(0.04, 0.04, 0.04) + w.inward.abs() * 0.2,
            0.0,
            rgb(30, 28, 26),
            1.0,
        );
        iron.lathe(
            at + Vec3::Y * 0.12,
            &[(0.0, 0.08), (0.06, 0.04), (0.08, 0.0)],
            4,
            rgb(36, 32, 28),
        );
        iron.lathe(
            at - Vec3::Y * 0.14,
            &[(0.0, 0.0), (0.0, 0.07), (0.03, 0.07), (0.03, 0.0)],
            4,
            rgb(36, 32, 28),
        );
        glow.lathe(at - Vec3::Y * 0.11, &[(0.0, 0.06), (0.23, 0.06)], 4, rgb(255, 196, 120));
        halos.push((at, 0.24, Color::new(1.0, 0.68, 0.36, 0.5)));
    }

    // Before the fire: a leather sofa, two wool armchairs, a log table and
    // a tartan rug; skis and poles by the door.
    let mut leather = MeshBuilder::default();
    let mut wool = MeshBuilder::default();
    let mut wood = MeshBuilder::default();
    sofa(
        &mut leather,
        vec3(-2.9, 0.0, 0.0),
        -std::f32::consts::FRAC_PI_2,
        2.2,
        rgb(110, 62, 34),
        false,
    );
    if medium {
        for (z, yaw) in [(-1.75f32, 0.5f32), (1.75, std::f32::consts::PI - 0.5)] {
            sofa(&mut wool, vec3(-4.0, 0.0, z), yaw, 0.95, rgb(200, 180, 160), false);
        }
        let mut r = MeshBuilder::default();
        let first = r.len();
        rug(&mut r, 1.0, 1.35);
        for v in &mut r.vertices[first..] {
            v.position.x -= 4.0;
            v.uv *= vec2(2.5, 3.0);
        }
        lit.add(kit, r, rug_tex);
        // A coffee table: a slab of a trunk on two log legs.
        wood.cuboid(vec3(-4.0, 0.36, 0.0), vec3(0.6, 0.08, 1.2), 0.0, rgb(170, 116, 70), 1.0);
        for z in [-0.4f32, 0.4] {
            tube(&mut wood, vec3(-4.0, 0.0, z), vec3(-4.0, 0.32, z), 0.09, 10, rgb(120, 80, 48));
        }
        // A blanket over the sofa's back.
        cushion(&mut wool, vec3(-2.62, 0.82, 0.3), vec3(0.24, 0.05, 0.7), 0.0, rgb(150, 40, 36));
    }
    if detailed {
        // Two pairs of skis and poles leaning by the right-hand window.
        for (k, z) in [0.15f32, 0.45].iter().enumerate() {
            let foot = vec3(HX - 0.35, 0.0, -3.4 + z);
            let top = vec3(HX - 0.12, 1.75, -3.35 + z);
            for d in [-0.05f32, 0.05] {
                let c = [rgb(190, 40, 36), rgb(40, 80, 160)][k];
                let off = vec3(0.0, 0.0, d);
                let across = vec3(0.0, 0.0, 0.035);
                wood.quad(
                    [
                        foot + off - across,
                        foot + off + across,
                        top + off + across,
                        top + off - across,
                    ],
                    [Vec2::ZERO; 4],
                    c,
                );
            }
            tube(
                &mut iron,
                foot + vec3(0.05, 0.0, 0.2),
                top + vec3(-0.05, -0.15, 0.25),
                0.007,
                5,
                rgb(160, 160, 166),
            );
        }
        // A cue rack on the front wall.
        let w = &walls[1];
        wall_box(&mut wood, w, HX + 0.0, 0.85, 0.04, vec3(1.4, 1.5, 0.05), rgb(150, 100, 60));
        for k in 0..7 {
            let at =
                w.start + w.along * (HX - 0.6 + k as f32 * 0.2) + w.inward * 0.09 + Vec3::Y * 0.15;
            leather.lathe(
                at,
                &[(0.0, 0.0148), (0.6, 0.012), (1.45, 0.0062)],
                8,
                [rgb(60, 30, 18), rgb(30, 30, 34)][k % 2],
            );
        }
    }
    lit.add(kit, timber, pine_tex);
    lit.add(kit, stone, stone_tex);
    lit.add(kit, dark, None);
    lit.add(kit, iron, None);
    lit.shiny(kit, leather, None, LEATHER);
    lit.add(kit, wool, wool_tex);
    lit.shiny(kit, wood, None, SATIN);
    add_glow(kit, flames, None);
    add_glow(kit, glow, None);
    // The fire's glow on the hearth and the floor.
    if medium {
        let mut pools = MeshBuilder::default();
        let c = vec3(-HX + 1.2, 0.006, 0.0);
        pools.quad(
            [
                c + vec3(-0.9, 0.0, 1.6),
                c + vec3(0.9, 0.0, 1.6),
                c + vec3(0.9, 0.0, -1.6),
                c + vec3(-0.9, 0.0, -1.6),
            ],
            [vec2(0.0, 1.0), vec2(1.0, 1.0), vec2(1.0, 0.0), vec2(0.0, 0.0)],
            Color::new(1.0, 0.5, 0.2, 0.2),
        );
        kit.add(pools, glow_tex, Surface::UNLIT, Layer::Blended);
    }
    for (at, size, color) in halos {
        kit.glow(at, size, color, false);
    }
}

/// A tartan (red with green and black stripes): a blanket, a rug.
fn tartan() -> textures::Pixels {
    textures::image(128, |u, v| {
        let band = |x: f32| {
            let x = (x * 4.0).fract();
            if x < 0.1 {
                2
            } else if x < 0.3 {
                1
            } else if (x - 0.62).abs() < 0.02 {
                3
            } else {
                0
            }
        };
        let colors =
            [[0.55, 0.08, 0.08], [0.08, 0.22, 0.12], [0.04, 0.04, 0.05], [0.85, 0.75, 0.3]];
        let (a, b) = (colors[band(u)], colors[band(v)]);
        // Twill: the warp and the weft alternate on a diagonal.
        let twill = (((u + v) * 128.0).floor() as i32) % 2 == 0;
        let c = if twill { a } else { b };
        let mixc = [0, 1, 2].map(|i| c[i] * 0.8 + (a[i] + b[i]) * 0.1);
        tx::to8(mixc)
    })
}

/// Iron lanterns on a log beam, an antler crown above.
pub fn lodge_lamp(kit: &mut Kit, quality: Quality, reach: f32) {
    let grow = reach - 0.45;
    let profile =
        [(0.0, 0.17), (0.02, 0.165), (0.07, 0.13), (0.13, 0.08), (0.17, 0.05), (0.2, 0.04)];
    shades_on_bar(
        kit,
        quality,
        reach,
        &profile,
        (rgb(40, 34, 30), Surface::lit(0.5, 30.0, 0.15, 0.2)),
        rgb(255, 214, 150),
        (rgb(36, 32, 28), Surface::lit(0.4, 20.0, 0.05, 0.1)),
    );
    // The beam is a log; antlers stand on it.
    let mut wood = MeshBuilder::default();
    tube(
        &mut wood,
        vec3(-0.75 - grow, LAMP_Y + 0.27, 0.0),
        vec3(0.75 + grow, LAMP_Y + 0.27, 0.0),
        0.05,
        12,
        rgb(140, 92, 54),
    );
    if quality >= Quality::Medium {
        let antler = rgb(220, 205, 180);
        for (x, s) in [(-0.35 - grow, 1.0f32), (0.35 + grow, -1.0)] {
            let mut p = vec3(x, LAMP_Y + 0.31, 0.0);
            for k in 0..4 {
                let q = p + vec3(s * 0.06, 0.07, if k % 2 == 0 { 0.03 } else { -0.03 });
                tube(&mut wood, p, q, 0.012, 6, antler);
                tube(&mut wood, q, q + vec3(-s * 0.02, 0.08, 0.0), 0.007, 5, antler);
                p = q;
            }
        }
    }
    kit.add(wood, None, Surface::lit(0.25, 20.0, 0.05, 0.1), Layer::Lamp);
}

// --- The neon arcade -----------------------------------------------------------

/// A neon tube between points (it shines by itself).
fn neon(mb: &mut MeshBuilder, path: &[Vec3], color: Color) {
    for w in path.windows(2) {
        tube(mb, w[0], w[1], 0.011, 6, color);
    }
}

/// The strokes of a few capital letters on a 0..1 square (x right, y up).
fn letter(c: char) -> &'static [(f32, f32)] {
    match c {
        'P' => &[
            (0.0, 0.0),
            (0.0, 1.0),
            (0.7, 1.0),
            (0.85, 0.85),
            (0.85, 0.6),
            (0.7, 0.45),
            (0.0, 0.45),
        ],
        'L' => &[(0.0, 1.0), (0.0, 0.0), (0.8, 0.0)],
        'A' => &[(0.0, 0.0), (0.42, 1.0), (0.84, 0.0), (0.65, 0.42), (0.19, 0.42)],
        'Y' => &[(0.0, 1.0), (0.42, 0.5), (0.84, 1.0), (0.42, 0.5), (0.42, 0.0)],
        _ => &[],
    }
}

/// A black room lit by neon: a mirror-black floor, neon tubes, arcade
/// cabinets with glowing screens, a bar with a lit shelf, a carpet runner.
pub fn arcade(kit: &mut Kit, quality: Quality, table: Vec2) {
    let medium = quality >= Quality::Medium;
    let detailed = quality >= Quality::High;
    let (brick_px, carpet_px) = both(tx::brick, tx::arcade_carpet);
    let brick_tex = Some(kit.texture(brick_px, true));
    let carpet_tex = Some(kit.texture(carpet_px, true));
    let floor_tex = Some(kit.texture(tx::concrete(), true));
    let screens: Vec<Option<usize>> =
        (0..3).map(|k| Some(kit.texture(tx::arcade_screen(k), false))).collect();
    let glow_tex = Some(kit.texture(textures::glow(), false));

    let pink = Color::new(1.0, 0.22, 0.68, 1.0);
    let cyan = Color::new(0.2, 0.85, 1.0, 1.0);
    let violet = Color::new(0.6, 0.3, 1.0, 1.0);
    let mut lights = table_lights(vec3(1.0, 0.92, 0.8), table);
    // The neon along the walls tints the room.
    for (p, c) in [
        (vec3(0.0, 2.7, -HZ + 0.1), vec3(1.0, 0.25, 0.7)),
        (vec3(-3.0, 2.7, HZ - 0.1), vec3(0.25, 0.8, 1.0)),
        (vec3(3.0, 2.7, HZ - 0.1), vec3(1.0, 0.25, 0.7)),
        (vec3(-HX + 0.1, 2.7, 0.0), vec3(0.25, 0.8, 1.0)),
        (vec3(HX - 0.1, 2.7, 0.0), vec3(0.6, 0.3, 1.0)),
    ] {
        lights.push(light(p, c, 0.7, false));
    }
    // The screens of the cabinets.
    for k in 0..6 {
        lights.push(light(
            vec3(-3.4 + k as f32 * 1.0, 1.4, -HZ + 1.1),
            vec3(0.4, 0.5, 1.0),
            0.18,
            false,
        ));
    }
    let mut lit = Lighting::new(quality, lights, vec3(0.08, 0.07, 0.1), table, 2.6);
    let cell = cell(quality);
    lit.occlude_box(vec3(0.0 - 0.9, 0.9, -HZ + 0.45), vec3(6.4, 1.8, 0.8));
    lit.occlude_box(vec3(-HX + 0.5, 0.55, 0.0), vec3(0.7, 1.1, 3.6));

    // A black epoxy floor, polished to a mirror; a ceiling painted black.
    let mut floor = MeshBuilder::default();
    let mut ceiling = MeshBuilder::default();
    floor_and_ceiling(&mut floor, &mut ceiling, cell, 0.35, 1.0, rgb(30, 30, 34));
    for v in &mut floor.vertices {
        v.color = [70, 70, 78, 255];
    }
    lit.floor(kit, floor, floor_tex, 0.45, (1.2, 120.0, 0.6));
    lit.add(kit, ceiling, None);
    if medium {
        // A runner of arcade carpet in front of the cabinets.
        let mut r = MeshBuilder::default();
        r.grid(
            vec3(-4.6, 0.004, -HZ + 1.75),
            vec3(7.4, 0.0, 0.0),
            vec3(0.0, 0.0, -1.0),
            30,
            4,
            WHITE,
            1.0 / 1.4,
        );
        lit.add(kit, r, carpet_tex);
    }

    // Walls of black-painted brick, a black ceiling grid.
    let mut bricks = MeshBuilder::default();
    for w in walls() {
        band(&mut bricks, &w, 0.0, w.length, 0.0, HEIGHT, cell, rgb(70, 66, 74), 1.0 / 2.0);
    }
    lit.add(kit, bricks, brick_tex);
    let mut black = MeshBuilder::default();
    for k in -3..=3 {
        black.cuboid(
            vec3(k as f32 * 1.6, HEIGHT - 0.1, 0.0),
            vec3(0.12, 0.2, 2.0 * HZ),
            0.0,
            rgb(24, 24, 28),
            1.0,
        );
    }

    // Neon: a line round the room near the ceiling, and signs.
    let mut tubes = MeshBuilder::default();
    let mut halos: Vec<(Vec3, f32, Color)> = Vec::new();
    for (i, w) in walls().iter().enumerate() {
        let color = [pink, cyan, cyan, violet][i];
        let a = w.start + w.inward * 0.06 + Vec3::Y * 2.75;
        let b = a + w.along * w.length;
        neon(&mut tubes, &[a, b], color);
        if detailed {
            let n = (w.length / 1.2) as i32;
            for k in 0..n {
                let p = a.lerp(b, (k as f32 + 0.5) / n as f32);
                halos.push((p, 0.4, Color::new(color.r, color.g, color.b, 0.16)));
            }
        }
    }
    // "PLAY" in pink on the right wall, an eight ball in cyan on the front.
    {
        let w = &walls()[3];
        let origin = w.start + w.along * (HZ - 1.3) + w.inward * 0.05 + Vec3::Y * 1.55;
        for (k, c) in "PLAY".chars().enumerate() {
            let at = origin + w.along * (k as f32 * 0.66);
            let path: Vec<Vec3> = letter(c)
                .iter()
                .map(|&(x, y)| at + w.along * (x * 0.5) + Vec3::Y * (y * 0.7))
                .collect();
            neon(&mut tubes, &path, pink);
        }
        halos.push((
            origin + w.along * 1.2 + Vec3::Y * 0.35,
            1.2,
            Color::new(1.0, 0.25, 0.7, 0.22),
        ));
        let w = &walls()[1];
        let c = w.start + w.along * (HX + 2.3) + w.inward * 0.05 + Vec3::Y * 1.8;
        let ring = |r: f32, at: Vec3| -> Vec<Vec3> {
            (0..=28)
                .map(|k| {
                    let a = k as f32 / 28.0 * std::f32::consts::TAU;
                    at + w.along * (a.cos() * r) + Vec3::Y * (a.sin() * r)
                })
                .collect()
        };
        neon(&mut tubes, &ring(0.4, c), cyan);
        neon(&mut tubes, &ring(0.08, c + Vec3::Y * 0.09), cyan);
        neon(&mut tubes, &ring(0.1, c - Vec3::Y * 0.08), cyan);
        halos.push((c, 0.9, Color::new(0.2, 0.85, 1.0, 0.25)));
        // A lightning bolt beside it.
        let bolt = [
            (0.0f32, 0.6f32),
            (0.25, 0.1),
            (0.1, 0.1),
            (0.3, -0.5),
            (0.05, 0.0),
            (0.2, 0.0),
            (0.0, 0.6),
        ];
        let at = c + w.along * 1.1;
        let path: Vec<Vec3> = bolt.iter().map(|&(x, y)| at + w.along * x + Vec3::Y * y).collect();
        neon(&mut tubes, &path, Color::new(1.0, 0.86, 0.2, 1.0));
        halos.push((at + w.along * 0.15, 0.7, Color::new(1.0, 0.8, 0.2, 0.18)));
    }

    // Arcade cabinets along the back wall.
    let mut cabinets = MeshBuilder::default();
    let mut glass = MeshBuilder::default();
    let mut screen_meshes: Vec<MeshBuilder> = (0..3).map(|_| MeshBuilder::default()).collect();
    let mut lightbox = MeshBuilder::default();
    let count = if medium { 6 } else { 3 };
    for k in 0..count {
        let x = -3.4 + k as f32 * 1.0;
        let z = -HZ + 0.42;
        let body = [rgb(30, 30, 90), rgb(110, 20, 40), rgb(20, 20, 24)][k % 3];
        // The cabinet: a tall box, the control panel sloping out, the
        // marquee lit on top.
        cabinets.cuboid(vec3(x, 0.85, z), vec3(0.72, 1.7, 0.7), 0.0, body, 1.0);
        cabinets.cuboid(vec3(x, 0.98, z + 0.42), vec3(0.72, 0.08, 0.3), 0.0, rgb(24, 24, 28), 1.0);
        // The marquee.
        let mc = [pink, cyan, Color::new(1.0, 0.8, 0.2, 1.0)][k % 3];
        lightbox.cuboid(vec3(x, 1.78, z + 0.24), vec3(0.66, 0.16, 0.06), 0.0, mc, 1.0);
        halos.push((vec3(x, 1.78, z + 0.32), 0.35, Color::new(mc.r, mc.g, mc.b, 0.22)));
        // The screen, set back and tilted.
        let sc = vec3(x, 1.38, z + 0.33);
        let up = vec3(0.0, 0.94, -0.34);
        picture(&mut screen_meshes[k % 3], sc, Vec3::X, up, 0.5, 0.42, WHITE);
        halos.push((sc + vec3(0.0, 0.0, 0.1), 0.42, Color::new(0.5, 0.6, 1.0, 0.12)));
        glass.quad(
            [
                sc + vec3(-0.27, -0.23, 0.02),
                sc + vec3(0.27, -0.23, 0.02),
                sc + vec3(0.27, 0.23, -0.04),
                sc + vec3(-0.27, 0.23, -0.04),
            ],
            [Vec2::ZERO; 4],
            rgb(20, 20, 26),
        );
        // A joystick and buttons.
        if detailed {
            let p = vec3(x - 0.15, 1.03, z + 0.45);
            tube(&mut cabinets, p, p + Vec3::Y * 0.07, 0.006, 6, rgb(20, 20, 20));
            cabinets.lathe(
                p + Vec3::Y * 0.07,
                &[(0.0, 0.0), (0.0, 0.02), (0.03, 0.02), (0.04, 0.0)],
                10,
                rgb(200, 30, 30),
            );
            for b in 0..3 {
                let q = vec3(x + 0.02 + b as f32 * 0.07, 1.025, z + 0.45);
                lightbox.lathe(
                    q,
                    &[(0.0, 0.018), (0.01, 0.018), (0.012, 0.0)],
                    10,
                    [pink, cyan, Color::new(1.0, 0.8, 0.2, 1.0)][b],
                );
            }
        }
    }
    for (k, m) in screen_meshes.into_iter().enumerate() {
        add_glow(kit, m, screens[k]);
    }

    // The bar on the left: a black counter with a light under its lip, a
    // lit shelf of bottles behind, chrome stools.
    let bx = -HX + 0.5;
    cabinets.cuboid(vec3(bx + 0.2, 0.52, 0.0), vec3(0.5, 1.04, 3.6), 0.0, rgb(26, 26, 30), 1.0);
    cabinets.cuboid(vec3(bx + 0.24, 1.06, 0.0), vec3(0.64, 0.05, 3.7), 0.0, rgb(40, 40, 46), 1.0);
    lightbox.cuboid(vec3(bx + 0.555, 1.02, 0.0), vec3(0.01, 0.02, 3.6), 0.0, violet, 1.0);
    lightbox.cuboid(
        vec3(-HX + 0.03, 1.75, 0.0),
        vec3(0.02, 0.7, 3.0),
        0.0,
        Color::new(0.35, 0.2, 0.6, 1.0),
        1.0,
    );
    halos.push((vec3(-HX + 0.3, 1.75, -1.0), 0.8, Color::new(0.6, 0.3, 1.0, 0.14)));
    halos.push((vec3(-HX + 0.3, 1.75, 1.0), 0.8, Color::new(0.6, 0.3, 1.0, 0.14)));
    for shelf in 0..2 {
        cabinets.cuboid(
            vec3(-HX + 0.12, 1.5 + shelf as f32 * 0.45, 0.0),
            vec3(0.2, 0.025, 3.0),
            0.0,
            rgb(30, 30, 34),
            1.0,
        );
        bottles(
            &mut glass,
            vec3(-HX + 0.12, 1.515 + shelf as f32 * 0.45, -1.35),
            Vec3::Z,
            13,
            if detailed { 12 } else { 8 },
        );
    }
    let mut chrome = MeshBuilder::default();
    for k in 0..4 {
        let c = vec3(bx + 0.95, 0.0, -1.3 + k as f32 * 0.86);
        chrome.lathe(
            c,
            &[(0.0, 0.2), (0.02, 0.2), (0.035, 0.04), (0.7, 0.03), (0.72, 0.17)],
            18,
            rgb(190, 192, 198),
        );
        hoop(&mut chrome, c + Vec3::Y * 0.28, 0.15, 0.01, 16, rgb(190, 192, 198));
        cabinets.lathe(
            c + Vec3::Y * 0.72,
            &[(0.0, 0.0), (0.0, 0.18), (0.04, 0.18), (0.06, 0.0)],
            18,
            rgb(20, 20, 24),
        );
    }

    // A pinball machine on the right wall: legs, a lit playfield under glass,
    // a backbox.
    if medium {
        for (k, z) in [-2.2f32, 1.7].into_iter().enumerate() {
            let c = vec3(HX - 0.8, 0.0, z);
            for (dx, dz) in [(-0.6f32, -0.28f32), (-0.6, 0.28), (0.5, -0.28), (0.5, 0.28)] {
                chrome.cuboid(
                    c + vec3(dx, 0.4, dz),
                    vec3(0.05, 0.8, 0.05),
                    0.0,
                    rgb(180, 182, 188),
                    1.0,
                );
            }
            cabinets.cuboid(
                c + vec3(0.0, 0.88, 0.0),
                vec3(1.3, 0.18, 0.65),
                0.0,
                [rgb(120, 20, 60), rgb(20, 60, 120)][k],
                1.0,
            );
            let top = c + vec3(0.0, 0.985, 0.0);
            let mut field = MeshBuilder::default();
            field.quad(
                [
                    top + vec3(-0.6, 0.02, 0.28),
                    top + vec3(0.6, -0.02, 0.28),
                    top + vec3(0.6, -0.02, -0.28),
                    top + vec3(-0.6, 0.02, -0.28),
                ],
                [vec2(0.0, 1.0), vec2(1.0, 1.0), vec2(1.0, 0.0), vec2(0.0, 0.0)],
                WHITE,
            );
            add_glow(kit, field, screens[(k + 1) % 3]);
            cabinets.cuboid(
                c + vec3(0.62, 1.3, 0.0),
                vec3(0.12, 0.7, 0.65),
                0.0,
                rgb(20, 20, 24),
                1.0,
            );
            lightbox.cuboid(
                c + vec3(0.555, 1.35, 0.0),
                vec3(0.01, 0.5, 0.56),
                0.0,
                [pink, cyan][k],
                1.0,
            );
            halos.push((c + vec3(0.45, 1.35, 0.0), 0.5, Color::new(0.8, 0.5, 1.0, 0.18)));
        }
    }
    lit.add(kit, black, None);
    lit.shiny(kit, cabinets, None, (0.8, 60.0, 0.4));
    lit.shiny(kit, chrome, None, BRASS);
    lit.shiny(kit, glass, None, GLASS);
    add_glow(kit, tubes, None);
    add_glow(kit, lightbox, None);
    if medium {
        // The neon's light washing down the walls.
        let mut pools = MeshBuilder::default();
        for (i, w) in walls().iter().enumerate() {
            let color = [pink, cyan, cyan, violet][i];
            let n = (w.length / 2.2) as i32;
            for k in 0..n {
                let c =
                    w.start + w.along * ((k as f32 + 0.5) * w.length / n as f32) + Vec3::Y * 2.45;
                crate::halls::pool(
                    &mut pools,
                    c,
                    w.inward,
                    2.6,
                    1.3,
                    Color::new(color.r, color.g, color.b, 0.16),
                );
            }
        }
        kit.add(pools, glow_tex, Surface::UNLIT, Layer::Blended);
    }
    for (at, size, color) in halos {
        kit.glow(at, size, color, false);
    }
}

/// A long black box over the table, lit from below, a neon line round it.
pub fn arcade_lamp(kit: &mut Kit, reach: f32) {
    let grow = reach - 0.45;
    let mut body = MeshBuilder::default();
    let mut glow = MeshBuilder::default();
    let (l, w) = (1.7 + 2.0 * grow, 0.42f32);
    let y = LAMP_Y + 0.05;
    body.cuboid(vec3(0.0, y + 0.05, 0.0), vec3(l, 0.1, w), 0.0, rgb(16, 16, 18), 1.0);
    glow.quad(
        [
            vec3(-l / 2.0 + 0.04, y - 0.001, w / 2.0 - 0.04),
            vec3(l / 2.0 - 0.04, y - 0.001, w / 2.0 - 0.04),
            vec3(l / 2.0 - 0.04, y - 0.001, -w / 2.0 + 0.04),
            vec3(-l / 2.0 + 0.04, y - 0.001, -w / 2.0 + 0.04),
        ],
        [Vec2::ZERO; 4],
        rgb(255, 248, 235),
    );
    // A neon line round the box.
    let c = |x: f32, z: f32| vec3(x, y + 0.05, z);
    let (hx, hz) = (l / 2.0 + 0.012, w / 2.0 + 0.012);
    let ring = [c(-hx, -hz), c(hx, -hz), c(hx, hz), c(-hx, hz), c(-hx, -hz)];
    for s in ring.windows(2) {
        tube(&mut glow, s[0], s[1], 0.008, 6, rgb(255, 60, 180));
    }
    for x in [-0.7 - grow, 0.7 + grow] {
        tube(&mut body, vec3(x, y + 0.1, 0.0), vec3(x, HEIGHT, 0.0), 0.004, 6, rgb(20, 20, 20));
    }
    kit.add(body, None, Surface::lit(1.0, 80.0, 0.1, 0.5), Layer::Lamp);
    kit.add(glow, None, Surface::UNLIT, Layer::Lamp);
    kit.glow(vec3(0.0, y + 0.05, hz), 0.6, Color::new(1.0, 0.3, 0.7, 0.2), true);
}

// --- The art deco lounge -------------------------------------------------------

/// A sunburst mirror: a round glass and gilt rays, on a wall facing `n`.
fn sunburst(gold: &mut MeshBuilder, mirror: &mut MeshBuilder, c: Vec3, n: Vec3, r: f32) {
    disc_facing(mirror, c + n * 0.03, n, r * 0.45, 32, rgb(150, 160, 170));
    let side = Vec3::Y.cross(n).normalize_or_zero();
    hoop_facing(gold, c + n * 0.03, n, r * 0.47, 0.012);
    for k in 0..24 {
        let a = k as f32 / 24.0 * std::f32::consts::TAU;
        let dir = side * a.cos() + Vec3::Y * a.sin();
        let len = if k % 2 == 0 { r } else { r * 0.78 };
        tube(
            gold,
            c + dir * r * 0.5 + n * 0.02,
            c + dir * len + n * 0.02,
            0.008,
            4,
            rgb(220, 176, 80),
        );
    }
}

/// A ring of tube in the plane facing `n`.
fn hoop_facing(mb: &mut MeshBuilder, c: Vec3, n: Vec3, radius: f32, r: f32) {
    let side = Vec3::Y.cross(n).normalize_or_zero();
    let up = n.cross(side);
    for k in 0..32 {
        let a0 = k as f32 / 32.0 * std::f32::consts::TAU;
        let a1 = (k + 1) as f32 / 32.0 * std::f32::consts::TAU;
        let p = |a: f32| c + (side * a.cos() + up * a.sin()) * radius;
        tube(mb, p(a0), p(a1), r, 6, rgb(220, 176, 80));
    }
}

/// An art deco lounge: a black and white marble floor polished to a
/// mirror, walnut and emerald silk, gilt pilasters, sunburst mirrors, fan
/// sconces, velvet banquettes and a mirrored cocktail bar.
pub fn deco(kit: &mut Kit, quality: Quality, table: Vec2) {
    let medium = quality >= Quality::Medium;
    let detailed = quality >= Quality::High;
    let (marble_px, silk_px) = both(
        || tx::marble_tiles(2, [0.92, 0.91, 0.88], [0.04, 0.04, 0.045], 173),
        || tx::fabric([0.06, 0.26, 0.20], 177),
    );
    let marble_tex = Some(kit.texture(marble_px, true));
    let silk_tex = Some(kit.texture(silk_px, true));
    let walnut_tex =
        Some(kit.texture(textures::fine_wood([0.22, 0.11, 0.05], [0.06, 0.03, 0.015], 179), true));
    let velvet_tex = Some(kit.texture(tx::fabric([0.05, 0.30, 0.22], 181), true));
    let glow_tex = Some(kit.texture(textures::glow(), false));

    let mut lights = table_lights(vec3(1.0, 0.86, 0.64), table);
    // Fan sconces between the pilasters light the walls upwards.
    let mut sconces = Vec::new();
    for (i, w) in walls().iter().enumerate() {
        let n = (w.length / 2.2) as i32;
        for k in 0..n {
            let along = (k as f32 + 0.5) * w.length / n as f32;
            // Not behind the bar, nor where the sunburst mirrors hang.
            if (i == 0 && (along - HX).abs() < 2.3)
                || (i > 0 && (along - w.length / 2.0).abs() < 0.7)
            {
                continue;
            }
            let at = w.start + w.along * along + w.inward * 0.12 + Vec3::Y * 1.9;
            sconces.push((at, w.inward));
            lights.push(light(at + Vec3::Y * 0.2, vec3(1.0, 0.76, 0.48), 0.42, false));
        }
    }
    for x in [-1.4f32, 0.0, 1.4] {
        lights.push(light(vec3(x, 2.4, -HZ + 0.9), vec3(1.0, 0.8, 0.55), 0.5, true));
    }
    let mut lit = Lighting::new(quality, lights, vec3(0.2, 0.17, 0.14), table, 2.9);
    let cell = cell(quality);
    lit.occlude_box(vec3(0.0, 0.55, -HZ + 0.75), vec3(4.4, 1.1, 0.6));

    // The chessboard floor, polished to a mirror; a cream ceiling.
    let mut floor = MeshBuilder::default();
    let mut ceiling = MeshBuilder::default();
    floor_and_ceiling(&mut floor, &mut ceiling, cell, 1.0 / 1.2, 1.0, rgb(235, 220, 196));
    lit.floor(kit, floor, marble_tex, 0.32, (1.4, 140.0, 0.7));
    lit.add(kit, ceiling, None);

    // Walls: walnut to the dado, silk above, gilt pilasters and a stepped
    // gilt cornice.
    const DADO: f32 = 1.0;
    let mut silk = MeshBuilder::default();
    let mut walnut = MeshBuilder::default();
    let mut gold = MeshBuilder::default();
    let gilt = rgb(214, 170, 76);
    for w in walls() {
        band(&mut walnut, &w, 0.0, w.length, 0.0, DADO, cell, WHITE, 0.9);
        band(&mut silk, &w, 0.0, w.length, DADO, HEIGHT, cell, WHITE, 1.0 / 0.8);
        if medium {
            // The dado rail and the stepped cornice.
            wall_box(&mut gold, &w, w.length / 2.0, DADO, 0.015, vec3(w.length, 0.04, 0.03), gilt);
            for (k, (h, d)) in [(0.06f32, 0.04f32), (0.05, 0.08), (0.04, 0.12)].iter().enumerate() {
                let y = HEIGHT - 0.03 - k as f32 * 0.055;
                wall_box(&mut gold, &w, w.length / 2.0, y, d / 2.0, vec3(w.length, *h, *d), gilt);
            }
            wall_box(
                &mut walnut,
                &w,
                w.length / 2.0,
                0.08,
                0.02,
                vec3(w.length, 0.16, 0.04),
                rgb(140, 110, 100),
            );
            // Fluted gilt pilasters.
            let n = (w.length / 2.2) as i32;
            for k in 0..=n {
                let along = k as f32 * w.length / n as f32;
                wall_box(
                    &mut walnut,
                    &w,
                    along,
                    (DADO + HEIGHT) / 2.0,
                    0.03,
                    vec3(0.3, HEIGHT - DADO, 0.06),
                    WHITE,
                );
                if detailed {
                    for f in -2..=2 {
                        wall_box(
                            &mut gold,
                            &w,
                            along + f as f32 * 0.05,
                            (DADO + HEIGHT) / 2.0 - 0.1,
                            0.065,
                            vec3(0.012, HEIGHT - DADO - 0.4, 0.01),
                            gilt,
                        );
                    }
                }
            }
        }
    }
    // Sunburst mirrors on the side walls and fan sconces.
    let mut mirror = MeshBuilder::default();
    let mut glow = MeshBuilder::default();
    let mut halos: Vec<(Vec3, f32, Color)> = Vec::new();
    if medium {
        for (i, along) in [(2usize, HZ), (3, HZ), (1, HX)] {
            let w = &walls()[i];
            let c = w.start + w.along * along + Vec3::Y * 2.05;
            sunburst(&mut gold, &mut mirror, c, w.inward, 0.55);
        }
    }
    for (at, n) in &sconces {
        // A gilt fan open upwards with a frosted glass inside it.
        let side = Vec3::Y.cross(*n).normalize_or_zero();
        let fan = |r: f32, k: f32| {
            (0..=8)
                .map(|j| {
                    let a = j as f32 / 8.0 * std::f32::consts::PI;
                    *at + side * a.cos() * r + Vec3::Y * a.sin() * r * k
                })
                .collect::<Vec<Vec3>>()
        };
        let outer = fan(0.16, 0.9);
        for s in outer.windows(2) {
            tube(&mut gold, s[0], s[1], 0.008, 4, gilt);
            glow.quad(
                [*at + *n * 0.005, s[0] + *n * 0.005, s[1] + *n * 0.005, *at + *n * 0.005],
                [Vec2::ZERO; 4],
                rgb(255, 232, 190),
            );
        }
        tube(&mut gold, *at - *n * 0.1, *at, 0.01, 6, gilt);
        halos.push((*at + Vec3::Y * 0.08 + *n * 0.05, 0.3, Color::new(1.0, 0.78, 0.5, 0.5)));
    }

    // A mirrored cocktail bar at the back: walnut and gilt, bottles on
    // glass shelves, three pendants.
    let bar_z = -HZ + 0.75;
    walnut.cuboid(vec3(0.0, 0.52, bar_z), vec3(4.2, 1.04, 0.56), 0.0, WHITE, 0.9);
    if medium {
        for k in 0..12 {
            let x = -2.0 + (k as f32 + 0.5) * 4.0 / 12.0;
            gold.cuboid(vec3(x, 0.52, bar_z + 0.285), vec3(0.015, 0.9, 0.01), 0.0, gilt, 1.0);
        }
    }
    let mut stone = MeshBuilder::default();
    stone.cuboid(vec3(0.0, 1.065, bar_z + 0.02), vec3(4.3, 0.05, 0.66), 0.0, rgb(20, 20, 22), 1.0);
    tube(&mut gold, vec3(-2.1, 0.18, bar_z + 0.42), vec3(2.1, 0.18, bar_z + 0.42), 0.02, 12, gilt);
    mirror.cuboid(vec3(0.0, 1.8, -HZ + 0.02), vec3(3.6, 1.2, 0.02), 0.0, rgb(52, 58, 64), 1.0);
    let mut glass = MeshBuilder::default();
    for shelf in 0..2 {
        glass.cuboid(
            vec3(0.0, 1.45 + shelf as f32 * 0.45, -HZ + 0.15),
            vec3(3.4, 0.015, 0.22),
            0.0,
            rgb(160, 190, 190),
            1.0,
        );
        bottles(
            &mut glass,
            vec3(-1.6, 1.46 + shelf as f32 * 0.45, -HZ + 0.15),
            Vec3::X,
            15,
            if detailed { 14 } else { 8 },
        );
    }
    for x in [-1.4f32, 0.0, 1.4] {
        let at = vec3(x, 2.4, bar_z + 0.15);
        tube(&mut gold, at + Vec3::Y * 0.2, vec3(x, HEIGHT, at.z), 0.005, 6, gilt);
        // A stepped glass globe.
        for (k, (r, h)) in [(0.12f32, 0.06f32), (0.1, 0.06), (0.07, 0.06)].iter().enumerate() {
            let y = k as f32 * 0.06;
            glow.frustum_at(at + Vec3::Y * y, 0.0, *r, *h, *r, 20, rgb(255, 236, 200));
            gold.frustum_at(at + Vec3::Y * (y + h), 0.0, r + 0.004, 0.008, r + 0.004, 20, gilt);
        }
        halos.push((at + Vec3::Y * 0.06, 0.3, Color::new(1.0, 0.82, 0.55, 0.45)));
    }
    // Stools in velvet and gilt.
    let mut velvet = MeshBuilder::default();
    for k in 0..5 {
        let c = vec3(-1.6 + k as f32 * 0.8, 0.0, bar_z + 0.75);
        gold.lathe(c, &[(0.0, 0.18), (0.02, 0.18), (0.03, 0.03), (0.66, 0.03)], 18, gilt);
        velvet.lathe(
            c + Vec3::Y * 0.66,
            &[(0.0, 0.0), (0.0, 0.19), (0.06, 0.2), (0.1, 0.17), (0.11, 0.0)],
            20,
            WHITE,
        );
    }

    // Velvet banquettes, curved in two corners, with round marble tables.
    if medium {
        for (cx, cz, a0) in
            [(-HX + 1.4f32, HZ - 1.4f32, 0.0f32), (HX - 1.4, HZ - 1.4, std::f32::consts::FRAC_PI_2)]
        {
            let steps = 10;
            for k in 0..steps {
                let a = a0 + (k as f32 + 0.5) / steps as f32 * std::f32::consts::FRAC_PI_2;
                let dir = vec3(-a.cos(), 0.0, a.sin());
                let dir = if cx > 0.0 { vec3(-dir.x, 0.0, dir.z) } else { dir };
                let yaw = dir.x.atan2(dir.z) + std::f32::consts::PI;
                let p = vec3(cx, 0.0, cz) + dir * 1.05;
                cushion(&mut velvet, p + Vec3::Y * 0.24, vec3(0.2, 0.48, 0.55), yaw, WHITE);
                cushion(
                    &mut velvet,
                    p + dir * 0.2 + Vec3::Y * 0.72,
                    vec3(0.2, 0.55, 0.16),
                    yaw,
                    WHITE,
                );
            }
            let t = vec3(cx, 0.0, cz);
            gold.lathe(t, &[(0.0, 0.22), (0.02, 0.22), (0.04, 0.04), (0.68, 0.03)], 20, gilt);
            stone.lathe(
                t + Vec3::Y * 0.68,
                &[(0.0, 0.0), (0.0, 0.42), (0.04, 0.42), (0.04, 0.0)],
                36,
                rgb(30, 30, 32),
            );
            // Two coupes of champagne.
            for (dx, dz) in [(0.12f32, 0.08f32), (-0.1, -0.12)] {
                glass.lathe(
                    t + vec3(dx, 0.72, dz),
                    &[(0.0, 0.035), (0.005, 0.005), (0.09, 0.004), (0.1, 0.05), (0.13, 0.055)],
                    14,
                    rgb(230, 210, 150),
                );
            }
        }
        // Palms in tall brass urns by the bar.
        let mut leaves = MeshBuilder::default();
        let mut pots = MeshBuilder::default();
        for x in [-2.8f32, 2.8] {
            potted_plant(
                &mut leaves,
                &mut pots,
                vec3(x, 0.0, -HZ + 0.6),
                1.8,
                true,
                rgb(200, 160, 70),
            );
        }
        lit.add(kit, leaves, None);
        lit.shiny(kit, pots, None, BRASS);
    }
    lit.add(kit, silk, silk_tex);
    lit.shiny(kit, walnut, walnut_tex, (1.0, 90.0, 0.55));
    lit.shiny(kit, gold, None, BRASS);
    lit.shiny(kit, mirror, None, (2.4, 200.0, 1.0));
    lit.shiny(kit, stone, None, (1.6, 140.0, 0.7));
    lit.shiny(kit, glass, None, GLASS);
    lit.shiny(kit, velvet, velvet_tex, (0.2, 12.0, 0.08));
    add_glow(kit, glow, None);
    if medium {
        // The sconces' light fanning up the silk.
        let mut pools = MeshBuilder::default();
        for (at, n) in &sconces {
            crate::halls::pool(
                &mut pools,
                *at + Vec3::Y * 0.45 - *n * 0.11,
                *n,
                1.0,
                1.2,
                Color::new(1.0, 0.78, 0.5, 0.2),
            );
        }
        kit.add(pools, glow_tex, Surface::UNLIT, Layer::Blended);
    }
    for (at, size, color) in halos {
        kit.glow(at, size, color, false);
    }
}

/// Three stepped shades of frosted glass with gilt bands.
pub fn deco_lamp(kit: &mut Kit, quality: Quality, reach: f32) {
    let profile = [
        (0.0, 0.16),
        (0.05, 0.16),
        (0.05, 0.13),
        (0.1, 0.13),
        (0.1, 0.09),
        (0.15, 0.09),
        (0.15, 0.05),
        (0.2, 0.04),
    ];
    shades_on_bar(
        kit,
        quality,
        reach,
        &profile,
        (rgb(236, 226, 206), Surface::lit(0.9, 80.0, 0.3, 0.5)),
        rgb(255, 240, 210),
        (rgb(214, 170, 76), Surface::lit(1.8, 100.0, 0.15, 0.8)),
    );
    let mut bands = MeshBuilder::default();
    let segs = if quality != Quality::Low { 32 } else { 18 };
    for x in lamp_xs(reach) {
        for (y, r) in [(0.0f32, 0.163f32), (0.05, 0.133), (0.1, 0.093)] {
            bands.frustum_at(
                vec3(x, LAMP_Y + y - 0.006, 0.0),
                0.0,
                r,
                0.012,
                r,
                segs,
                rgb(214, 170, 76),
            );
        }
    }
    kit.add(bands, None, Surface::lit(1.8, 100.0, 0.15, 0.8), Layer::Lamp);
}

// --- The penthouse -------------------------------------------------------------

/// A penthouse at night: glass walls on the city on two sides, a pale
/// marble floor, an oak slat wall, white plaster with a cove of light,
/// a long sofa, a bar island, planters.
pub fn penthouse(kit: &mut Kit, quality: Quality, table: Vec2) {
    let medium = quality >= Quality::Medium;
    let detailed = quality >= Quality::High;
    let ((marble_px, slats_px), (city_px, city2_px)) = both(
        || {
            both(
                || tx::marble_tiles(1, [0.86, 0.84, 0.80], [0.86, 0.84, 0.80], 183),
                || tx::slats([0.62, 0.45, 0.30], [0.12, 0.08, 0.05], 185),
            )
        },
        || both(|| tx::city(41), || tx::city(43)),
    );
    let marble_tex = Some(kit.texture(marble_px, true));
    let slats_tex = Some(kit.texture(slats_px, true));
    let city_tex = Some(kit.texture(city_px, true));
    let city2_tex = Some(kit.texture(city2_px, true));
    let linen_tex = Some(kit.texture(tx::fabric([0.62, 0.60, 0.57], 187), true));
    let art_tex = Some(kit.texture(abstract_art(), false));
    let glow_tex = Some(kit.texture(textures::glow(), false));

    let mut lights = table_lights(vec3(1.0, 0.9, 0.76), table);
    // Downlights in the ceiling and the glow of the city.
    let mut downlights = Vec::new();
    for x in [-4.2f32, -2.6, 2.6, 4.2] {
        for z in [-2.6f32, 0.0, 2.6] {
            downlights.push(vec3(x, HEIGHT - 0.01, z));
            lights.push(light(vec3(x, HEIGHT - 0.1, z), vec3(1.0, 0.86, 0.68), 0.3, true));
        }
    }
    for (p, c) in [
        (vec3(0.0, 1.4, -HZ - 1.5), vec3(0.45, 0.5, 0.85)),
        (vec3(HX + 1.5, 1.4, 0.0), vec3(0.45, 0.5, 0.85)),
    ] {
        lights.push(light(p, c, 0.9, false));
    }
    let mut lit = Lighting::new(quality, lights, vec3(0.2, 0.2, 0.22), table, 2.6);
    let cell = cell(quality);
    lit.occlude_box(vec3(-HX + 1.3, 0.35, 1.0), vec3(1.0, 0.7, 3.0));
    lit.occlude_box(vec3(2.6, 0.5, 2.9), vec3(2.4, 1.0, 0.7));

    let mut floor = MeshBuilder::default();
    let mut ceiling = MeshBuilder::default();
    floor_and_ceiling(&mut floor, &mut ceiling, cell, 1.0 / 1.2, 1.0, rgb(232, 230, 226));
    lit.floor(kit, floor, marble_tex, 0.3, (1.2, 120.0, 0.6));
    lit.add(kit, ceiling, None);

    // The back and right walls are glass from floor to ceiling.
    let mut plaster = MeshBuilder::default();
    let mut steel = MeshBuilder::default();
    let mut sky = MeshBuilder::default();
    let mut sky2 = MeshBuilder::default();
    let mut panes = MeshBuilder::default();
    let walls = walls();
    for (i, w) in walls.iter().enumerate() {
        match i {
            0 | 3 => {
                // Slim black mullions every 1.4 m, a transom low down.
                let n = (w.length / 1.4).round() as i32;
                for k in 0..=n {
                    let along = k as f32 * w.length / n as f32;
                    wall_box(
                        &mut steel,
                        w,
                        along,
                        HEIGHT / 2.0,
                        0.0,
                        vec3(0.05, HEIGHT, 0.08),
                        rgb(26, 26, 28),
                    );
                }
                wall_box(
                    &mut steel,
                    w,
                    w.length / 2.0,
                    0.05,
                    0.0,
                    vec3(w.length, 0.1, 0.1),
                    rgb(26, 26, 28),
                );
                wall_box(
                    &mut steel,
                    w,
                    w.length / 2.0,
                    HEIGHT - 0.05,
                    0.0,
                    vec3(w.length, 0.1, 0.1),
                    rgb(26, 26, 28),
                );
                // The city far below and the night sky.
                let target = if i == 0 { &mut sky } else { &mut sky2 };
                backdrop(target, w, 3.0, -1.6, 3.8, false, (w.length / 3.2).round().max(1.0));
                // The glass: a faint sheen over the view.
                let first = panes.len();
                band(
                    &mut panes,
                    w,
                    0.0,
                    w.length,
                    0.1,
                    HEIGHT - 0.1,
                    1.4,
                    Color::new(0.6, 0.7, 0.8, 0.07),
                    1.0,
                );
                for v in &mut panes.vertices[first..] {
                    v.position -= w.inward * 0.01;
                }
            }
            _ => band(&mut plaster, w, 0.0, w.length, 0.0, HEIGHT, cell, rgb(236, 232, 226), 1.0),
        }
    }
    // An oak slat wall on the left, and a cove of light round the ceiling.
    let mut slats = MeshBuilder::default();
    let mut glow = MeshBuilder::default();
    {
        let w = &walls[2];
        let first = slats.len();
        band(&mut slats, w, 0.6, w.length - 0.6, 0.0, HEIGHT - 0.25, cell, WHITE, 1.0 / 1.2);
        for v in &mut slats.vertices[first..] {
            v.position += w.inward * 0.04;
        }
        wall_box(
            &mut glow,
            w,
            w.length / 2.0,
            HEIGHT - 0.24,
            0.05,
            vec3(w.length - 1.2, 0.01, 0.05),
            rgb(255, 228, 190),
        );
    }
    for w in &walls {
        wall_box(
            &mut plaster,
            w,
            w.length / 2.0,
            HEIGHT - 0.12,
            0.15,
            vec3(w.length, 0.04, 0.3),
            rgb(240, 238, 234),
        );
    }
    let mut halos: Vec<(Vec3, f32, Color)> = Vec::new();
    for at in &downlights {
        glow.disc(*at - Vec3::Y * 0.002, 0.05, 16, rgb(255, 246, 228));
        halos.push((*at - Vec3::Y * 0.03, 0.18, Color::new(1.0, 0.92, 0.8, 0.5)));
    }

    // A long low sofa on the left, a coffee table, a rug; a credenza and
    // a big painting on the front wall; a bar island; planters.
    let mut linen = MeshBuilder::default();
    let mut oak = MeshBuilder::default();
    let mut dark = MeshBuilder::default();
    let mut glass = MeshBuilder::default();
    sofa(&mut linen, vec3(-HX + 1.2, 0.0, 1.0), std::f32::consts::FRAC_PI_2, 2.8, WHITE, false);
    if medium {
        sofa(&mut linen, vec3(-HX + 2.55, 0.0, 2.95), std::f32::consts::PI, 1.6, WHITE, false);
        let mut r = MeshBuilder::default();
        let first = r.len();
        rug(&mut r, 1.1, 1.5);
        for v in &mut r.vertices[first..] {
            v.position += vec3(-HX + 2.5, 0.0, 1.2);
            v.uv *= vec2(4.0, 5.0);
            v.color = [150, 146, 140, 255];
        }
        lit.add(kit, r, linen_tex);
        // A low coffee table of black marble on a steel frame.
        dark.cuboid(vec3(-HX + 2.5, 0.36, 1.2), vec3(0.8, 0.04, 1.2), 0.0, rgb(24, 24, 26), 1.0);
        for (dx, dz) in [(-0.34f32, -0.54f32), (-0.34, 0.54), (0.34, -0.54), (0.34, 0.54)] {
            let p = vec3(-HX + 2.5 + dx, 0.0, 1.2 + dz);
            steel.cuboid(p + Vec3::Y * 0.17, vec3(0.03, 0.34, 0.03), 0.0, rgb(30, 30, 32), 1.0);
        }
        // An arc floor lamp over the sofa.
        let base = vec3(-HX + 0.5, 0.0, -0.9);
        dark.lathe(base, &[(0.0, 0.18), (0.06, 0.18), (0.07, 0.0)], 24, rgb(240, 240, 240));
        let arc: Vec<Vec3> = (0..=12)
            .map(|k| {
                let t = k as f32 / 12.0 * std::f32::consts::FRAC_PI_2;
                base + vec3(t.sin() * 1.6, 0.07 + t.cos() * 1.3 + t.sin() * 0.7, 0.0)
            })
            .collect();
        for s in arc.windows(2) {
            tube(&mut steel, s[0], s[1], 0.012, 8, rgb(200, 200, 205));
        }
        let shade = *arc.last().unwrap_or(&base);
        steel.lathe(
            shade - Vec3::Y * 0.2,
            &[(0.0, 0.2), (0.12, 0.1), (0.2, 0.02)],
            24,
            rgb(200, 200, 205),
        );
        glow.disc(shade - Vec3::Y * 0.199, 0.19, 24, rgb(255, 236, 200));
        halos.push((shade - Vec3::Y * 0.22, 0.35, Color::new(1.0, 0.86, 0.6, 0.45)));
    }
    // The credenza and the painting.
    {
        let w = &walls[1];
        wall_box(&mut oak, w, HX - 2.4, 0.3, 0.25, vec3(2.4, 0.6, 0.45), WHITE);
        let c = w.start + w.along * (HX - 2.4) + Vec3::Y * 1.75 + w.inward * 0.02;
        let mut art = MeshBuilder::default();
        picture(&mut art, c, w.along, Vec3::Y, 1.8, 1.1, WHITE);
        lit.add(kit, art, art_tex);
        if detailed {
            for (k, x) in [-0.7f32, 0.5].iter().enumerate() {
                let at = w.start + w.along * (HX - 2.4 + x) + Vec3::Y * 0.6 + w.inward * 0.25;
                let h = [0.38f32, 0.26][k];
                glass.lathe(
                    at,
                    &[(0.0, 0.05), (h * 0.5, 0.09), (h, 0.03)],
                    16,
                    [rgb(40, 80, 90), rgb(170, 120, 60)][k],
                );
            }
        }
    }
    // The bar island with stools.
    oak.cuboid(vec3(2.6, 0.5, 2.9), vec3(2.4, 1.0, 0.6), 0.0, WHITE, 1.0);
    dark.cuboid(vec3(2.6, 1.02, 2.9), vec3(2.6, 0.04, 0.75), 0.0, rgb(232, 228, 222), 1.0);
    glow.cuboid(vec3(2.6, 0.06, 2.59), vec3(2.3, 0.01, 0.01), 0.0, rgb(255, 228, 190), 1.0);
    for k in 0..4 {
        let c = vec3(1.7 + k as f32 * 0.6, 0.0, 2.35);
        steel.lathe(
            c,
            &[(0.0, 0.18), (0.015, 0.18), (0.03, 0.02), (0.68, 0.02)],
            18,
            rgb(30, 30, 32),
        );
        linen.lathe(
            c + Vec3::Y * 0.68,
            &[(0.0, 0.0), (0.0, 0.18), (0.05, 0.18), (0.07, 0.0)],
            18,
            rgb(120, 110, 100),
        );
    }
    if medium {
        for x in [1.85f32, 2.6, 3.35].iter() {
            let at = vec3(*x, 2.3, 2.9);
            tube(&mut steel, at, vec3(*x, HEIGHT, 2.9), 0.003, 4, rgb(30, 30, 30));
            glass.lathe(
                at - Vec3::Y * 0.15,
                &[(0.0, 0.0), (0.0, 0.06), (0.12, 0.08), (0.15, 0.03)],
                16,
                rgb(170, 150, 120),
            );
            glow.disc(at - Vec3::Y * 0.149, 0.055, 12, rgb(255, 230, 190));
            halos.push((at - Vec3::Y * 0.17, 0.2, Color::new(1.0, 0.8, 0.55, 0.45)));
        }
        // Concrete planters with olive trees by the glass.
        let mut leaves = MeshBuilder::default();
        let mut pots = MeshBuilder::default();
        for (x, z) in [(HX - 0.7f32, -HZ + 0.7f32), (HX - 0.7, HZ - 0.8), (-HX + 0.8, -HZ + 0.7)] {
            potted_plant(&mut leaves, &mut pots, vec3(x, 0.0, z), 1.7, false, rgb(150, 150, 150));
        }
        lit.add(kit, leaves, None);
        lit.add(kit, pots, None);
    }
    lit.add(kit, plaster, None);
    lit.add(kit, slats, slats_tex);
    lit.shiny(kit, steel, None, (0.8, 60.0, 0.3));
    lit.shiny(kit, oak, slats_tex, SATIN);
    lit.shiny(kit, dark, None, (1.2, 120.0, 0.5));
    lit.shiny(kit, glass, None, GLASS);
    lit.add(kit, linen, linen_tex);
    // The city is a little dimmer than the room's own lights.
    for v in sky.vertices.iter_mut().chain(sky2.vertices.iter_mut()) {
        v.color = [196, 196, 210, 255];
    }
    add_glow(kit, sky, city_tex);
    add_glow(kit, sky2, city2_tex);
    add_glow(kit, glow, None);
    kit.add(panes, None, Surface::baked(1.0, 200.0, 0.6), Layer::Blended);
    if medium {
        let mut pools = MeshBuilder::default();
        let w = &walls[2];
        for k in 0..4 {
            let c = w.start + w.along * (1.0 + k as f32 * 2.0) + Vec3::Y * 2.5 + w.inward * 0.05;
            crate::halls::pool(
                &mut pools,
                c,
                w.inward,
                2.4,
                1.4,
                Color::new(1.0, 0.85, 0.65, 0.16),
            );
        }
        kit.add(pools, glow_tex, Surface::UNLIT, Layer::Blended);
    }
    for (at, size, color) in halos {
        kit.glow(at, size, color, false);
    }
}

/// A big abstract painting: soft fields of colour, a gold line.
fn abstract_art() -> textures::Pixels {
    tx::image_wh(256, 160, |u, v| {
        let n = textures::fbm(u * 3.0, v * 3.0, 3, 191, 4);
        let mut c = tx::mix(
            [0.86, 0.82, 0.76],
            [0.12, 0.2, 0.32],
            tx::smooth(0.35, 0.65, u + (n - 0.5) * 0.3),
        );
        c = tx::mix(
            c,
            [0.75, 0.38, 0.22],
            tx::smooth(0.1, 0.0, (v - 0.68 - (n - 0.5) * 0.2).abs()) * 0.9,
        );
        if ((u - 0.62) + (v - 0.5) * 0.3).abs() < 0.004 {
            c = [0.85, 0.66, 0.28];
        }
        let canvas = textures::noise(u * 120.0, v * 75.0, 120, 193) - 0.5;
        tx::to8(c.map(|x| x * (1.0 + canvas * 0.06)))
    })
}

/// A long, slim black bar with a line of light under it.
pub fn penthouse_lamp(kit: &mut Kit, reach: f32) {
    let grow = reach - 0.45;
    let mut body = MeshBuilder::default();
    let mut glow = MeshBuilder::default();
    let y = LAMP_Y + 0.05;
    body.cuboid(
        vec3(0.0, y + 0.03, 0.0),
        vec3(1.8 + 2.0 * grow, 0.06, 0.12),
        0.0,
        rgb(22, 22, 24),
        1.0,
    );
    glow.cuboid(
        vec3(0.0, y - 0.001, 0.0),
        vec3(1.74 + 2.0 * grow, 0.004, 0.08),
        0.0,
        rgb(255, 246, 228),
        1.0,
    );
    for x in [-0.8 - grow, 0.8 + grow] {
        tube(&mut body, vec3(x, y + 0.06, 0.0), vec3(x, HEIGHT, 0.0), 0.002, 4, rgb(60, 60, 60));
    }
    kit.add(body, None, Surface::lit(1.0, 80.0, 0.1, 0.5), Layer::Lamp);
    kit.add(glow, None, Surface::UNLIT, Layer::Lamp);
}
