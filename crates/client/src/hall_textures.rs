//! Textures of the themed halls (see [`crate::halls`]), made from code like
//! every other texture of the game.

use crate::textures::{Pixels, fbm, hash, image, lerp3, noise};

/// An image of `w` × `h` pixels (`f(u, v)` in 0..1).
pub(crate) fn image_wh(w: u16, h: u16, f: impl Fn(f32, f32) -> [u8; 3]) -> Pixels {
    let (w, h) = (crate::textures::detailed(w), crate::textures::detailed(h));
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        for x in 0..w {
            let c = f(x as f32 / w as f32, y as f32 / h as f32);
            rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    Pixels { width: w, height: h, rgba }
}

/// [`image_wh`] with every pixel the mean of four samples: patterns with
/// hard edges (a rug's motifs and lines) get smooth outlines instead of
/// steps, which show on anything as large as a rug seen from the table.
pub(crate) fn image_wh_smooth(w: u16, h: u16, f: impl Fn(f32, f32) -> [u8; 3]) -> Pixels {
    let (w, h) = (crate::textures::detailed(w), crate::textures::detailed(h));
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 3];
            for (dx, dy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                let c = f((x as f32 + dx) / w as f32, (y as f32 + dy) / h as f32);
                for k in 0..3 {
                    sum[k] += c[k] as u32;
                }
            }
            let c = sum.map(|s| ((s + 2) / 4) as u8);
            rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    Pixels { width: w, height: h, rgba }
}

pub(crate) fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

pub(crate) fn to8(c: [f32; 3]) -> [u8; 3] {
    lerp3(c, c, 0.0)
}

pub(crate) fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A deck of sun-bleached boards with dark gaps (5 boards a tile).
pub fn deck(size: u16) -> Pixels {
    image(size, |u, v| {
        let boards = 5.0;
        let b = (v * boards).floor();
        let within = v * boards - b;
        let offset = hash(b as i32, 2, 51);
        let along = (u + offset).fract() * 1.0;
        let id = b as i32 * 13 + (along * 1.0) as i32;
        let tone = (hash(id, 0, 9) - 0.5) * 0.3;
        let grain = fbm(u * 3.0 + b * 0.31, within * 2.0, 3, 17, 4) - 0.5;
        let streak = noise(u * 80.0, within * 2.5 + b, 80, 23) - 0.5;
        let weather = fbm(u * 6.0, v * 6.0, 6, 31, 3);
        let mut t = 0.35 + tone + grain * 0.5 + streak * 0.2 - weather * 0.15;
        // The gap between boards, and a screw pair at the ends.
        if !(0.035..=0.965).contains(&within) {
            t = 1.25;
        }
        let end = (along - 0.02).abs() < 0.006
            && ((within - 0.3).abs() < 0.05 || (within - 0.7).abs() < 0.05);
        if end {
            t = 0.95;
        }
        lerp3([0.74, 0.60, 0.44], [0.28, 0.19, 0.12], t)
    })
}

/// Horizontal planks (a wall or a bar front), 4 a tile, painted wood.
pub fn planks(base: [f32; 3], dark: [f32; 3], seed: u32) -> Pixels {
    image(256, move |u, v| {
        let n = 4.0;
        let b = (v * n).floor();
        let within = v * n - b;
        let tone = (hash(b as i32, 1, seed) - 0.5) * 0.25;
        let grain = fbm(u * 2.0, within * 3.0 + b, 2, seed + 3, 4) - 0.5;
        let streak = noise(u * 64.0, within * 2.0 + b * 3.0, 64, seed + 7) - 0.5;
        let mut t = 0.3 + tone + grain * 0.45 + streak * 0.15;
        if within < 0.04 {
            t = 1.0;
        } else if within < 0.08 {
            t += 0.25;
        }
        lerp3(base, dark, t)
    })
}

/// The sky at sunset over the sea, for a backdrop seen through windows
/// (2:1; the horizon at `horizon` from the top). `sun`: where the sun sets
/// (u), if it shows; `palms`: dark palm trees against the sky.
pub fn sunset(sun: Option<f32>, palms: bool, seed: u32) -> Pixels {
    let horizon = 0.62;
    image_wh(512, 256, move |u, v| {
        let mut c = if v < horizon {
            // The sky: deep violet high up, rose, then orange at the horizon.
            let h = v / horizon;
            let mut c = mix([0.16, 0.13, 0.32], [0.62, 0.30, 0.42], smooth(0.0, 0.6, h));
            c = mix(c, [1.0, 0.56, 0.24], smooth(0.5, 0.95, h));
            c = mix(c, [1.0, 0.80, 0.45], smooth(0.9, 1.0, h));
            // Long, thin clouds lit from below.
            let cloud = fbm(u * 3.0, v * 14.0, 3, seed, 4);
            let band =
                smooth(0.55, 0.75, cloud) * smooth(0.15, 0.4, h) * (1.0 - smooth(0.85, 0.98, h));
            c = mix(c, [1.0, 0.62, 0.48], band * 0.65);
            if let Some(su) = sun {
                let d = (((u - su) * 2.0).powi(2) + (v - horizon + 0.05).powi(2)).sqrt();
                c = mix(c, [1.0, 0.86, 0.55], smooth(0.25, 0.0, d) * 0.6);
                if d < 0.05 {
                    c = [1.0, 0.93, 0.70];
                }
            }
            c
        } else {
            // The sea: dark, with the sky's glow on it and a path of light.
            let d = (v - horizon) / (1.0 - horizon);
            let mut c = mix([0.55, 0.32, 0.30], [0.10, 0.10, 0.20], smooth(0.0, 0.5, d));
            // Long, flat ripples (stretched along the horizon).
            let ripple = noise(u * 24.0, v * 70.0, 24, seed + 5) * 0.7
                + noise(u * 48.0, v * 140.0, 48, seed + 6) * 0.3;
            if let Some(su) = sun {
                let path = smooth(0.08 + d * 0.25, 0.0, (u - su).abs());
                c = mix(c, [1.0, 0.75, 0.45], path * smooth(0.45, 0.8, ripple) * 0.9);
            }
            c = mix(c, [0.8, 0.5, 0.4], smooth(0.7, 0.9, ripple) * (1.0 - d) * 0.3);
            c
        };
        if palms {
            // Palm trees: a leaning trunk and a crown of drooping fronds.
            for k in 0..3 {
                let x0 = 0.12 + k as f32 * 0.37 + hash(k, 3, seed) * 0.08;
                let lean = (hash(k, 4, seed) - 0.5) * 0.25;
                let top = 0.12 + hash(k, 5, seed) * 0.12;
                if v > top {
                    let t = (v - top) / (1.0 - top);
                    let x = x0 + lean * (1.0 - t) * (1.0 - t);
                    if (u - x).abs() < 0.006 + t * 0.006 {
                        c = [0.06, 0.04, 0.06];
                    }
                }
                let cx = x0 + lean;
                for f in 0..7 {
                    let a = f as f32 / 7.0 * std::f32::consts::TAU + hash(k, f, seed) * 0.5;
                    let (dx, dy) = (u - cx, (v - top) * 0.5);
                    let along = dx * a.cos() + dy * a.sin();
                    if (0.0..0.12).contains(&along) {
                        let droop = along * along * 6.0;
                        let across = -dx * a.sin() + dy * a.cos() - droop * a.cos().abs() * 0.5;
                        let width = 0.012 * (1.0 - along / 0.12);
                        let leaf = (along * 140.0).sin() * 0.004;
                        if across.abs() < width + leaf.abs() {
                            c = [0.05, 0.035, 0.05];
                        }
                    }
                }
            }
        }
        to8(c)
    })
}

/// Thatch seen from below: layers of dry palm leaves (tileable).
pub fn thatch() -> Pixels {
    image(256, |u, v| {
        let rows = 6.0;
        let r = (v * rows).floor();
        let within = v * rows - r;
        let shift = hash(r as i32, 0, 71) * 0.5;
        let blade = noise((u + shift) * 90.0, r * 7.0, 90, 73);
        let tip = smooth(0.6, 1.0, within);
        let t = 0.25 + (1.0 - blade) * 0.35 + tip * 0.35 + fbm(u * 4.0, v * 4.0, 4, 75, 2) * 0.15;
        lerp3([0.78, 0.64, 0.38], [0.28, 0.19, 0.09], t)
    })
}

/// Herringbone parquet of dark oak (tileable).
pub fn parquet() -> Pixels {
    image(512, |u, v| {
        // Blocks 1:4 at right angles, in a zigzag.
        let (x, y) = (u * 8.0, v * 8.0);
        let col = x.floor();
        let flip = (col as i32).rem_euclid(2) == 0;
        let yy = if flip { y + x.fract() } else { y - x.fract() + 1.0 };
        let row = (yy * 2.0).floor();
        let within_y = (yy * 2.0).fract();
        let within_x = x.fract();
        let id = col as i32 * 31 + row as i32;
        let tone = (hash(id, 0, 81) - 0.5) * 0.28;
        let grain_dir = if flip { within_x + within_y * 0.1 } else { within_y + within_x * 0.1 };
        let grain = noise(grain_dir * 20.0, id as f32 * 0.7, 20, 83) - 0.5;
        let mut t = 0.45 + tone + grain * 0.3;
        if within_y < 0.03 || within_x < 0.025 {
            t = 1.1;
        }
        lerp3([0.36, 0.22, 0.12], [0.08, 0.045, 0.025], t)
    })
}

/// Rows of leather-bound books on a shelf (a strip: 2 shelves a tile).
pub fn books() -> Pixels {
    image(256, |u, v| {
        let shelves = 2.0;
        let s = (v * shelves).floor();
        let within = v * shelves - s;
        // The shelf board at the bottom of each row.
        if within > 0.88 {
            let t = if within > 0.97 { 0.9 } else { 0.35 };
            return lerp3([0.30, 0.17, 0.09], [0.08, 0.04, 0.02], t);
        }
        // Books of varied widths and heights.
        let mut x = 0.0;
        let mut k = 0;
        let row_seed = s as i32 * 97;
        while x <= u {
            x += 0.018 + hash(row_seed + k, 0, 91) * 0.03;
            k += 1;
        }
        let id = row_seed + k;
        let height = 0.55 + hash(id, 1, 93) * 0.33;
        let top = 0.88 - height;
        if within < top {
            // The dark back of the shelf.
            return lerp3([0.05, 0.03, 0.02], [0.05, 0.03, 0.02], 0.0);
        }
        let palette = [
            [0.38, 0.06, 0.06],
            [0.08, 0.18, 0.12],
            [0.10, 0.12, 0.28],
            [0.42, 0.26, 0.12],
            [0.22, 0.10, 0.06],
            [0.55, 0.42, 0.22],
            [0.14, 0.05, 0.10],
        ];
        let base = palette[(hash(id, 2, 95) * palette.len() as f32) as usize % palette.len()];
        let start = x - (0.018 + hash(row_seed + k - 1, 0, 91) * 0.03);
        let w = (x - start).max(1e-3);
        let across = ((u - start) / w).clamp(0.0, 1.0);
        let round = 1.0 - ((across - 0.5) * 2.0).powi(2) * 0.6;
        let mut c = base.map(|c| c * (0.55 + 0.6 * round));
        // Gilt bands near the top and bottom of the spine.
        let rel = (within - top) / height;
        if (rel - 0.12).abs() < 0.02
            || (rel - 0.82).abs() < 0.02
            || (rel - 0.3).abs() < 0.05 && hash(id, 3, 97) > 0.5
        {
            c = mix(c, [0.75, 0.58, 0.25], 0.8);
        }
        if !(0.06..=0.94).contains(&across) {
            c = c.map(|c| c * 0.5);
        }
        to8(c)
    })
}

/// An oil painting in the club: a landscape or a portrait (dark, varnished).
pub fn painting(k: u32) -> Pixels {
    image(256, move |u, v| {
        let age = fbm(u * 5.0, v * 5.0, 5, 101 + k, 3);
        let c = if k.is_multiple_of(2) {
            // A landscape: sky, hills, a lake.
            let hill =
                0.55 + (u * 7.0 + k as f32).sin() * 0.05 + fbm(u * 3.0, 1.0, 3, 103, 2) * 0.1;
            if v < hill - 0.1 {
                mix([0.55, 0.50, 0.36], [0.30, 0.36, 0.38], v * 1.5)
            } else if v < hill {
                mix([0.20, 0.24, 0.12], [0.12, 0.16, 0.08], age)
            } else if v < 0.75 {
                mix([0.30, 0.34, 0.30], [0.20, 0.22, 0.20], age)
            } else {
                mix([0.16, 0.14, 0.08], [0.08, 0.07, 0.04], age)
            }
        } else {
            // A portrait: a dark ground, a face and a dark coat.
            let (x, y) = (u - 0.5, v - 0.36);
            let face = (x * x * 9.0 + y * y * 5.5).sqrt() < 0.36;
            let body = v > 0.52 && (x.abs() < 0.12 + (v - 0.52) * 0.9);
            let collar = body && v < 0.6 && x.abs() < 0.05;
            if collar {
                [0.75, 0.72, 0.62]
            } else if face {
                mix([0.72, 0.52, 0.38], [0.45, 0.30, 0.20], (x + 0.15) * 2.0)
            } else if body {
                [0.06, 0.05, 0.05]
            } else {
                mix([0.16, 0.12, 0.08], [0.06, 0.05, 0.04], (u - 0.5).abs() * 2.0 + age * 0.3)
            }
        };
        to8(c.map(|x| x * (0.8 + age * 0.3)))
    })
}

/// Old red brick in a running bond with pale mortar (tileable; 8 courses).
pub fn brick() -> Pixels {
    image(512, |u, v| {
        let courses = 8.0;
        let row = (v * courses).floor();
        let within_y = v * courses - row;
        let shift = if (row as i32) % 2 == 0 { 0.0 } else { 0.5 };
        let x = u * 4.0 + shift;
        let col = x.floor();
        let within_x = x - col;
        let id = row as i32 * 17 + col as i32;
        let mortar = within_y < 0.09 || within_x < 0.025;
        if mortar {
            let t = 0.4 + noise(u * 200.0, v * 200.0, 200, 111) * 0.3;
            return lerp3([0.62, 0.58, 0.52], [0.40, 0.37, 0.33], t);
        }
        let tone = hash(id, 0, 113);
        let base = if tone > 0.8 {
            [0.36, 0.16, 0.10]
        } else if tone > 0.15 {
            [0.52, 0.24, 0.15]
        } else {
            [0.62, 0.36, 0.22]
        };
        let spots = fbm(u * 40.0, v * 40.0, 40, 115, 3);
        let soot = fbm(u * 3.0, v * 3.0, 3, 117, 3);
        let edge = smooth(0.0, 0.08, within_y - 0.09).min(smooth(0.0, 0.03, within_x - 0.025));
        let c = base.map(|c| c * (0.75 + spots * 0.45) * (0.85 + soot * 0.2) * (0.8 + edge * 0.2));
        to8(c)
    })
}

/// Polished concrete: soft mottling, a few saw cuts.
pub fn concrete() -> Pixels {
    image(512, |u, v| {
        let m = fbm(u * 4.0, v * 4.0, 4, 121, 5);
        let fine = noise(u * 160.0, v * 160.0, 160, 123);
        let mut t = 0.35 + (m - 0.5) * 0.5 + (fine - 0.5) * 0.08;
        if (u.fract() - 0.002).abs() < 0.002 || (v.fract() - 0.002).abs() < 0.002 {
            t += 0.35;
        }
        lerp3([0.58, 0.58, 0.57], [0.26, 0.26, 0.27], t)
    })
}

/// A city at dusk seen from a high window: a deep blue sky with the last
/// light low down, towers with lit windows (2:1).
pub fn city(seed: u32) -> Pixels {
    image_wh(512, 256, move |u, v| {
        let mut c = mix([0.04, 0.06, 0.16], [0.20, 0.22, 0.42], smooth(0.0, 0.75, v));
        c = mix(c, [0.70, 0.42, 0.40], smooth(0.62, 0.82, v) * 0.6);
        // Towers: several rows, the far ones paler.
        for layer in 0..3 {
            let width = [0.05, 0.035, 0.025][layer];
            let col = (u / width).floor();
            let id = col as i32 * 7 + layer as i32 * 1000;
            let h =
                [0.35, 0.5, 0.62][layer] + (hash(id, 0, seed) - 0.5) * [0.35, 0.25, 0.12][layer];
            if v > h {
                let tone = [[0.03, 0.035, 0.06], [0.06, 0.07, 0.12], [0.12, 0.13, 0.22]][layer];
                c = tone;
                // Lit windows.
                let (wx, wy) = ((u / width).fract(), (v - h) * 90.0);
                let cell = (wx * 5.0).floor() as i32 + (wy.floor() as i32) * 13 + id * 3;
                let lit = hash(cell, 1, seed + layer as u32) > 0.62;
                let inside = (wx * 5.0).fract() > 0.3 && wy.fract() > 0.35 && wx > 0.1 && wx < 0.9;
                if lit && inside {
                    let warm = hash(cell, 2, seed);
                    c = mix([1.0, 0.82, 0.50], [0.75, 0.85, 1.0], warm * 0.6);
                    c = c.map(|x| x * (0.55 + 0.3 * (2 - layer) as f32 / 2.0));
                }
            }
        }
        to8(c)
    })
}

/// Woven rattan (a lamp shade, chair backs): tileable.
pub fn rattan() -> Pixels {
    image(128, |u, v| {
        let (a, b) = ((u * 16.0).fract(), (v * 16.0).fract());
        let (i, j) = ((u * 16.0) as i32, (v * 16.0) as i32);
        let over = (i + j) % 2 == 0;
        let t = if over {
            0.3 + ((a - 0.5).abs() * 2.0).powi(2) * 0.5
        } else {
            0.3 + ((b - 0.5).abs() * 2.0).powi(2) * 0.5
        };
        lerp3([0.86, 0.68, 0.42], [0.36, 0.24, 0.12], t)
    })
}

/// A Persian rug (2:1): a deep red field with a navy border and a medallion.
pub fn persian_rug() -> Pixels {
    image_wh_smooth(1024, 512, |u, v| {
        let edge = (u.min(1.0 - u) * 2.0).min(v.min(1.0 - v));
        let red = [0.42, 0.07, 0.07];
        let navy = [0.07, 0.09, 0.22];
        let cream = [0.78, 0.68, 0.48];
        let wool = fbm(u * 64.0, v * 32.0, 32, 131, 2) - 0.5;
        let mut c = if edge < 0.015 {
            cream
        } else if edge < 0.1 {
            let t = ((u * 2.0 + v) * 40.0).sin() * ((u * 2.0 - v) * 40.0).sin();
            if t > 0.4 { cream } else { navy }
        } else if (edge - 0.11).abs() < 0.008 {
            cream
        } else {
            let (x, y) = ((u - 0.5) * 2.0, v - 0.5);
            let r = (x * x + y * y).sqrt();
            let a = y.atan2(x);
            let lobe = 0.2 + 0.04 * (a * 8.0).cos();
            if r < lobe * 0.45 {
                cream
            } else if r < lobe {
                navy
            } else if (r - lobe - 0.02).abs() < 0.008 {
                cream
            } else {
                let lattice = ((x * 18.0).sin() * (y * 18.0).sin()).abs();
                if lattice > 0.9 { [0.60, 0.30, 0.12] } else { red }
            }
        };
        c = c.map(|x| (x * (1.0 + wool * 0.3)).clamp(0.0, 1.0));
        to8(c)
    })
}

// --- The newer halls -----------------------------------------------------------

/// Veins of marble at (u, v): thin, wandering, branching lines (0..1).
fn veins(u: f32, v: f32, seed: u32) -> f32 {
    let warp = fbm(u * 3.0, v * 3.0, 3, seed, 4);
    let a = ((u * 2.0 + v * 1.3 + warp * 2.6) * std::f32::consts::PI * 2.0).sin().abs();
    let b = ((u * 1.1 - v * 2.4 + warp * 3.1 + 0.3) * std::f32::consts::PI * 3.0).sin().abs();
    let thin = (1.0 - a).powf(18.0) + (1.0 - b).powf(30.0) * 0.6;
    thin.min(1.0) * (0.6 + 0.4 * fbm(u * 8.0, v * 8.0, 8, seed + 1, 3))
}

/// Polished marble tiles, `n` × `n` a tile: `light` and `dark` alternate
/// like a chessboard (the same colour twice: plain tiles), with veins and
/// fine grout lines.
pub fn marble_tiles(n: u32, light: [f32; 3], dark: [f32; 3], seed: u32) -> Pixels {
    image(256, move |u, v| {
        let (x, y) = (u * n as f32, v * n as f32);
        let (i, j) = (x.floor() as i32, y.floor() as i32);
        let (fx, fy) = (x.fract(), y.fract());
        let black = (i + j) % 2 != 0;
        let base = if black { dark } else { light };
        let cloud = fbm(u * 5.0, v * 5.0, 5, seed + 7, 4) - 0.5;
        let vein = veins(u + i as f32 * 0.37, v + j as f32 * 0.21, seed + i as u32 * 3);
        let vein_color = if black { [0.62, 0.60, 0.56] } else { [0.36, 0.35, 0.36] };
        let mut c = base.map(|c| c * (1.0 + cloud * 0.18));
        c = mix(c, vein_color, vein * 0.55);
        let grout = fx.min(1.0 - fx).min(fy.min(1.0 - fy));
        if grout < 0.008 {
            c = [0.30, 0.29, 0.27];
        } else if grout < 0.016 {
            c = c.map(|x| x * 0.9);
        }
        to8(c)
    })
}

/// Rough fieldstone set in mortar (a chimney breast, a lodge's walls).
pub fn fieldstone() -> Pixels {
    image(256, |u, v| {
        // Stones: the nearest of a jittered grid of points, in rows.
        let rows = 5.0;
        let (x, y) = (u * 4.0, v * rows);
        let (cx, cy) = (x.floor() as i32, y.floor() as i32);
        let mut best = (9.0f32, 9.0f32, 0i32);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (gx, gy) = (cx + dx, cy + dy);
                let id = gx.rem_euclid(4) + gy.rem_euclid(5) * 7;
                let px = gx as f32 + 0.2 + hash(id, 1, 141) * 0.6;
                let py = gy as f32 + 0.25 + hash(id, 2, 141) * 0.5;
                // Stones are wider than tall.
                let d = ((x - px) * 0.8).hypot(y - py);
                if d < best.0 {
                    best = (d, best.0, id);
                } else if d < best.1 {
                    best.1 = d;
                }
            }
        }
        let edge = best.1 - best.0;
        let tone = hash(best.2, 3, 143);
        let base = mix([0.50, 0.46, 0.40], [0.34, 0.31, 0.28], tone);
        let base = mix(base, [0.55, 0.42, 0.30], hash(best.2, 4, 143) * 0.35);
        let rough = fbm(u * 24.0, v * 24.0, 24, 145, 4);
        // Rounded: darker towards the joint.
        let round = smooth(0.0, 0.25, edge);
        let mut c = base.map(|c| c * (0.65 + rough * 0.45) * (0.55 + 0.45 * round));
        if edge < 0.05 {
            let t = noise(u * 120.0, v * 120.0, 120, 147);
            c = mix([0.42, 0.40, 0.37], [0.26, 0.25, 0.23], t);
        }
        to8(c)
    })
}

/// Round logs laid one on another (a log cabin's wall): 4 logs a tile, with
/// bark-dark joints, pale wood and long cracks along the grain.
pub fn logs() -> Pixels {
    image(256, |u, v| {
        let n = 4.0;
        let k = (v * n).floor();
        let y = v * n - k;
        // Round: lit from above, dark in the joint under each log.
        let round = (y * std::f32::consts::PI).sin();
        let tone = (hash(k as i32, 0, 151) - 0.5) * 0.15;
        let grain = fbm(u * 2.0, y * 6.0 + k, 2, 153, 4) - 0.5;
        let streak = noise(u * 90.0, y * 4.0 + k * 3.0, 90, 155) - 0.5;
        let crack = (noise(u * 6.0, k, 6, 157) > 0.72 && (y - 0.55).abs() < 0.012) as u8 as f32;
        let mut t = 0.6 - round * 0.45 + tone + grain * 0.35 + streak * 0.12 + crack * 0.4;
        t += (1.0 - y) * 0.1;
        if !(0.04..=0.96).contains(&y) {
            t = 1.15;
        }
        lerp3([0.80, 0.58, 0.36], [0.24, 0.14, 0.07], t)
    })
}

/// Snowy mountains at dusk under the first stars, pines below (2:1), for a
/// lodge's big windows.
pub fn mountains(seed: u32) -> Pixels {
    image_wh(512, 256, move |u, v| {
        // The sky: deep blue, pink near the peaks, stars.
        let mut c = mix([0.06, 0.08, 0.20], [0.42, 0.36, 0.56], smooth(0.0, 0.62, v));
        c = mix(c, [0.86, 0.56, 0.52], smooth(0.4, 0.62, v) * 0.55);
        let star = hash((u * 512.0) as i32, (v * 256.0) as i32, seed);
        if star > 0.996 && v < 0.4 {
            c = mix(c, [1.0, 1.0, 0.95], (star - 0.996) * 200.0);
        }
        // Two ranges of peaks, the far one pale and pink.
        let ridge = |scale: f32, base: f32, amp: f32, s: u32| {
            base - amp * (fbm(u * scale, 0.5, scale as i32, s, 5) - 0.3)
                - amp * 0.6 * (1.0 - ((u * scale * 0.5).fract() - 0.5).abs() * 2.0)
        };
        let far = ridge(3.0, 0.62, 0.32, seed + 1);
        if v > far {
            let snow = smooth(far + 0.02, far + 0.12, v);
            c = mix([0.88, 0.80, 0.86], [0.38, 0.38, 0.52], snow);
            let shade = fbm(u * 20.0, v * 10.0, 20, seed + 2, 3);
            c = c.map(|x| x * (0.8 + shade * 0.3));
        }
        let near = ridge(5.0, 0.74, 0.22, seed + 3);
        if v > near {
            let snow = smooth(near, near + 0.08, v);
            c = mix([0.82, 0.84, 0.92], [0.28, 0.30, 0.42], snow);
            let shade = fbm(u * 30.0, v * 14.0, 30, seed + 4, 3);
            c = c.map(|x| x * (0.75 + shade * 0.35));
        }
        // Pines: dark triangles along the bottom.
        let col = (u * 60.0).floor();
        let fx = (u * 60.0).fract() - 0.5;
        let h = 0.18 + hash(col as i32, 5, seed) * 0.12;
        let top = 1.0 - h;
        if v > top {
            let width = (v - top) / h * 0.5;
            let tiers = ((v - top) / h * 5.0).fract() * 0.15;
            if fx.abs() < width * (0.85 + tiers) {
                c = [0.03, 0.05, 0.06];
            }
        }
        if v > 0.93 {
            c = mix(c, [0.75, 0.78, 0.88], 0.7);
        }
        to8(c)
    })
}

/// The carpet of an arcade: near black, with neon shapes (stars, rings,
/// squiggles) scattered over it.
pub fn arcade_carpet() -> Pixels {
    image(256, |u, v| {
        let wool = fbm(u * 96.0, v * 96.0, 96, 161, 2);
        let mut c = [0.035, 0.03, 0.06].map(|x| x * (0.8 + wool * 0.4));
        let colors = [[0.95, 0.20, 0.65], [0.20, 0.85, 1.0], [1.0, 0.85, 0.2], [0.55, 0.3, 1.0]];
        let cells = 6.0;
        let (i, j) = ((u * cells).floor() as i32, (v * cells).floor() as i32);
        for dj in -1..=1 {
            for di in -1..=1 {
                let (ci, cj) = ((i + di).rem_euclid(6), (j + dj).rem_euclid(6));
                let id = ci * 11 + cj;
                let cx = ((i + di) as f32 + 0.2 + hash(id, 1, 163) * 0.6) / cells;
                let cy = ((j + dj) as f32 + 0.2 + hash(id, 2, 163) * 0.6) / cells;
                let (dx, dy) = (u - cx, v - cy);
                let r = dx.hypot(dy) * cells;
                let a = dy.atan2(dx);
                let color = colors[(id as usize) % colors.len()];
                let line = match id % 3 {
                    // A ring.
                    0 => ((r - 0.22).abs() < 0.035) as u8 as f32,
                    // A star.
                    1 => (r < 0.12 + 0.12 * (a * 5.0).cos().max(0.0)) as u8 as f32,
                    // A squiggle.
                    _ => {
                        let w = (dx * cells * 9.0).sin() * 0.06;
                        ((dy * cells - w).abs() < 0.03 && (dx * cells).abs() < 0.3) as u8 as f32
                    }
                };
                if line > 0.0 {
                    c = color.map(|x| x * (0.75 + wool * 0.3));
                }
            }
        }
        to8(c)
    })
}

/// Narrow vertical wooden slats with dark gaps (a feature wall), 8 a tile.
pub fn slats(light: [f32; 3], dark: [f32; 3], seed: u32) -> Pixels {
    image(256, move |u, v| {
        let n = 8.0;
        let k = (u * n).floor();
        let x = u * n - k;
        let tone = (hash(k as i32, 0, seed) - 0.5) * 0.2;
        let grain = fbm(x * 2.0 + k, v * 3.0, 3, seed + 1, 4) - 0.5;
        let streak = noise(x * 3.0 + k * 7.0, v * 80.0, 80, seed + 2) - 0.5;
        let mut t = 0.35 + tone + grain * 0.35 + streak * 0.2;
        // Rounded edges catch the light; the gap behind is dark.
        if !(0.12..=0.88).contains(&x) {
            t = 1.3;
        } else {
            t += (1.0 - ((x - 0.5).abs() * 2.6).min(1.0)) * -0.08;
        }
        lerp3(light, dark, t)
    })
}

/// An arcade game's screen (pixel art: a maze, invaders or a racer), lit.
pub fn arcade_screen(k: u32) -> Pixels {
    image(128, move |u, v| {
        let (px, py) = ((u * 32.0).floor(), (v * 32.0).floor());
        let (x, y) = (px as i32, py as i32);
        let scan = 0.85 + 0.15 * ((v * 128.0 * std::f32::consts::PI).sin()).abs();
        let mut c = [0.02, 0.02, 0.06];
        match k % 3 {
            0 => {
                // A maze with dots.
                let wall = (x % 6 == 0 || y % 6 == 0) && hash(x / 6, y / 6, 171) > 0.3;
                if wall {
                    c = [0.15, 0.25, 1.0];
                } else if x % 3 == 1 && y % 3 == 1 {
                    c = [1.0, 0.85, 0.7];
                }
                if (u - 0.6).hypot(v - 0.45) < 0.05 {
                    c = [1.0, 0.9, 0.1];
                }
            }
            1 => {
                // Rows of invaders and a cannon.
                let (cx, cy) = (x % 5, y % 5);
                if y < 16 && x > 2 && x < 30 && cy > 0 && cx > 0 && (cx + cy) % 2 == 0 {
                    c = [[0.3, 1.0, 0.4], [1.0, 0.3, 0.9], [0.4, 0.9, 1.0]][(y / 5) as usize % 3];
                }
                if y > 27 && (x - 16).abs() < 2 {
                    c = [0.4, 1.0, 0.4];
                }
            }
            _ => {
                // A road to the horizon.
                let road = (u - 0.5).abs() < (v - 0.4).max(0.0) * 0.9;
                if v < 0.4 {
                    c = mix([0.9, 0.3, 0.6], [0.2, 0.1, 0.4], v / 0.4);
                } else if road {
                    c = if ((v * 30.0).fract() < 0.5) && (u - 0.5).abs() < 0.01 + v * 0.01 {
                        [1.0, 1.0, 1.0]
                    } else {
                        [0.25, 0.25, 0.3]
                    };
                } else {
                    c = [0.1, 0.5, 0.25];
                }
            }
        }
        to8(c.map(|x| x * scan))
    })
}

/// Fabric (velvet, linen): a fine soft weave in one colour.
pub fn fabric(base: [f32; 3], seed: u32) -> Pixels {
    image(128, move |u, v| {
        let weave = ((u * 128.0 * std::f32::consts::PI).sin()
            * (v * 128.0 * std::f32::consts::PI).sin())
            * 0.04;
        let n = fbm(u * 8.0, v * 8.0, 8, seed, 3) - 0.5;
        let k = 1.0 + weave + n * 0.18;
        to8(base.map(|x| x * k))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hall_textures_have_their_sizes() {
        for p in [
            deck(256),
            thatch(),
            parquet(),
            books(),
            brick(),
            concrete(),
            rattan(),
            marble_tiles(2, [0.9, 0.9, 0.88], [0.05, 0.05, 0.06], 1),
            fieldstone(),
            logs(),
            arcade_carpet(),
            slats([0.6, 0.45, 0.3], [0.2, 0.12, 0.07], 3),
            arcade_screen(1),
            fabric([0.1, 0.3, 0.2], 5),
        ] {
            assert_eq!(p.rgba.len(), p.width as usize * p.height as usize * 4);
            assert_eq!(p.width, p.height, "square, tileable");
        }
        for p in [sunset(Some(0.5), true, 1), city(3), mountains(2)] {
            assert_eq!((p.width, p.height), (512, 256));
        }
        // A rug is seen from close by: twice the pixels each way.
        let rug = persian_rug();
        assert_eq!((rug.width, rug.height), (1024, 512));
        // The sunset is brighter at the horizon than high up.
        let s = sunset(None, false, 1);
        let px = |x: usize, y: usize| s.rgba[(y * 512 + x) * 4] as i32;
        assert!(px(100, 150) > px(100, 5));
    }
}
