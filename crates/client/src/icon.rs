//! The Snook Legend emblem, drawn procedurally: an 8 ball seen at an angle,
//! its number circle turned up and to the right (so it reads as a sphere, in
//! perspective), resting on a tile of green cloth edged in brass.
//!
//! The same picture is the window icon, the menu's emblem and, exported to
//! files, the executable's, the installer's and the Store's icons
//! (`assets/icon/`, `msix/Assets/`; see the `export_icons` and
//! `export_msix_assets` tests to make them again after a change here).

use macroquad::miniquad::conf::Icon;

#[derive(Clone, Copy)]
struct Rgba([f32; 4]);

impl Rgba {
    const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Rgba([r, g, b, 1.0])
    }

    /// Paints `top` over `self` with coverage `a` (0..1).
    fn over(self, top: Rgba, a: f32) -> Rgba {
        let a = a.clamp(0.0, 1.0) * top.0[3];
        let mut o = self.0;
        for (k, v) in o.iter_mut().enumerate().take(3) {
            *v += (top.0[k] - *v) * a;
        }
        o[3] += (1.0 - o[3]) * a;
        Rgba(o)
    }

    fn scale(self, k: f32) -> Rgba {
        let c = self.0;
        Rgba([c[0] * k, c[1] * k, c[2] * k, c[3]])
    }

    fn mix(self, other: Rgba, t: f32) -> Rgba {
        let t = t.clamp(0.0, 1.0);
        let (a, b) = (self.0, other.0);
        Rgba([
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
            a[3] + (b[3] - a[3]) * t,
        ])
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn len(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

/// Signed distance to a rounded square of half size `b` and corner radius `r`.
fn rounded_square(x: f32, y: f32, b: f32, r: f32) -> f32 {
    let (qx, qy) = (x.abs() - (b - r), y.abs() - (b - r));
    len(qx.max(0.0), qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

const CLOTH_LIGHT: Rgba = Rgba::rgb(0.19, 0.60, 0.35);
const CLOTH_DARK: Rgba = Rgba::rgb(0.03, 0.20, 0.11);
const BRASS_LIGHT: Rgba = Rgba::rgb(1.0, 0.86, 0.52);
const BRASS_DARK: Rgba = Rgba::rgb(0.58, 0.38, 0.13);
const IVORY: Rgba = Rgba::rgb(0.98, 0.96, 0.90);
const BLACK: Rgba = Rgba::rgb(0.035, 0.04, 0.05);
const INK: Rgba = Rgba::rgb(0.05, 0.05, 0.06);

/// Half size of the tile (the picture spans -1..1) and its corner radius.
const TILE: f32 = 0.94;
const CORNER: f32 = 0.30;
/// The ball: centre and radius.
const BALL: (f32, f32, f32) = (-0.02, -0.04, 0.62);
/// Where the number circle faces: up and to the right, towards the viewer.
const FACE: [f32; 3] = [0.40, -0.36, 0.84];
/// The number circle's radius on the ball (as a share of the ball's).
const DISC: f32 = 0.47;

/// The colour of the emblem at `(x, y)` (-1..1, y down), with alpha.
fn shade(x: f32, y: f32, px: f32) -> Rgba {
    let mut c = Rgba([0.0; 4]);
    let d = rounded_square(x, y, TILE, CORNER);
    if d > px {
        return c;
    }
    // The cloth: a pool of light around the ball, darker to the edges.
    let lit = len(x + 0.15, y + 0.25) / 1.35;
    let mut cloth = CLOTH_LIGHT.mix(CLOTH_DARK, smoothstep(0.05, 1.0, lit));
    // The ball's soft shadow, down and to the right.
    let (bx, by, br) = BALL;
    let s = len((x - bx - 0.16) / 1.1, (y - by - 0.20) / 0.85) / br;
    cloth = cloth.scale(1.0 - 0.6 * (1.0 - smoothstep(0.55, 1.15, s)));
    c = c.over(cloth, smoothstep(px, -px, d));
    // A thin brass edge.
    let brass = BRASS_LIGHT.mix(BRASS_DARK, (y + x * 0.5 + 1.0) / 2.0);
    let edge = smoothstep(px, -px, d) * smoothstep(-0.05 - px, -0.05 + px, d);
    c = c.over(brass, edge);
    // The ball.
    let (u, v) = ((x - bx) / br, (y - by) / br);
    let r = (u * u + v * v).sqrt();
    let inside = smoothstep(1.0 + px / br, 1.0 - px / br, r);
    if inside <= 0.0 {
        return c;
    }
    let n = [u, v, (1.0 - (r * r).min(1.0)).sqrt()];
    let light = norm([-0.55, -0.80, 0.70]);
    let diffuse = dot(n, light).max(0.0);
    // The number circle, on the sphere: a disc around `FACE`, seen at an
    // angle (an ellipse), with an "8" that follows the curve.
    let face = norm(FACE);
    let t1 = norm(cross([0.0, 1.0, 0.0], face));
    let t2 = cross(face, t1);
    let on_face = dot(n, face);
    let (lu, lv) = (dot(n, t1) / DISC, dot(n, t2) / DISC);
    let dpx = px / br / DISC * 1.6;
    let disc = if on_face > 0.0 { smoothstep(1.0 + dpx, 1.0 - dpx, len(lu, lv)) } else { 0.0 };
    let mut base = BLACK.over(IVORY, disc);
    base = base.over(INK, eight(lu, -lv, dpx) * disc);
    // Light: a soft key from the upper left, the cloth's green bounce from
    // below, a clear highlight and a window-like glint.
    let mut col = base.scale(0.22 + 0.85 * diffuse);
    col = col.over(Rgba::rgb(0.18, 0.55, 0.32), 0.28 * smoothstep(0.55, 1.0, r) * v.max(0.0));
    let view = [0.0, 0.0, 1.0];
    let half = norm([light[0] + view[0], light[1] + view[1], light[2] + view[2]]);
    let nh = dot(n, half).max(0.0);
    col = col.over(Rgba::rgb(1.0, 1.0, 1.0), nh.powf(120.0) * 0.9 + nh.powf(10.0) * 0.12);
    // Fresnel: the rim catches a little of the room.
    let rim = (1.0 - n[2]).powf(3.0);
    col = col.over(Rgba::rgb(0.55, 0.62, 0.70), rim * 0.22);
    c.over(col, inside)
}

/// Coverage of an "8" in a unit disc (x right, y up), `px` the pixel size:
/// two rings, the lower one a little larger.
fn eight(x: f32, y: f32, px: f32) -> f32 {
    let w = 0.105;
    let ring = |cy: f32, rx: f32, ry: f32| -> f32 {
        let (dx, dy) = (x / rx, (y - cy) / ry);
        let d = (len(dx, dy) - 1.0) * rx.min(ry);
        smoothstep(px, -px, d.abs() - w)
    };
    ring(0.27, 0.25, 0.22).max(ring(-0.25, 0.30, 0.27))
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    [a[0] / l, a[1] / l, a[2] / l]
}

/// The emblem as `size`×`size` RGBA pixels (straight alpha).
pub fn render(size: usize) -> Vec<u8> {
    let ss = if size <= 64 { 5 } else { 3 };
    let mut out = vec![0u8; size * size * 4];
    let px = 2.0 / size as f32 * 0.5;
    for y in 0..size {
        for x in 0..size {
            let mut acc = [0f32; 4];
            for sy in 0..ss {
                for sx in 0..ss {
                    let fx = (x as f32 + (sx as f32 + 0.5) / ss as f32) / size as f32 * 2.0 - 1.0;
                    let fy = (y as f32 + (sy as f32 + 0.5) / ss as f32) / size as f32 * 2.0 - 1.0;
                    let c = shade(fx, fy, px).0;
                    // Premultiplied while averaging.
                    for k in 0..3 {
                        acc[k] += c[k] * c[3];
                    }
                    acc[3] += c[3];
                }
            }
            let n = (ss * ss) as f32;
            let i = (y * size + x) * 4;
            let a = acc[3] / n;
            for k in 0..3 {
                let v = if acc[3] > 0.0 { acc[k] / acc[3] } else { 0.0 };
                out[i + k] = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
            out[i + 3] = (a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
    out
}

pub fn window_icon() -> Icon {
    let mut icon = Icon { small: [0; 1024], medium: [0; 4096], big: [0; 16384] };
    icon.small.copy_from_slice(&render(16));
    icon.medium.copy_from_slice(&render(32));
    icon.big.copy_from_slice(&render(64));
    icon
}

/// Draws the emblem (menu, lounge, loading screen) at `(x, y)`, `size` wide.
pub fn draw(x: f32, y: f32, size: f32) {
    use macroquad::prelude::*;
    thread_local! {
        static EMBLEM: std::cell::OnceCell<Texture2D> = const { std::cell::OnceCell::new() };
    }
    let tex = EMBLEM.with(|e| {
        e.get_or_init(|| {
            let tex = Texture2D::from_rgba8(256, 256, &render(256));
            tex.set_filter(FilterMode::Linear);
            tex
        })
        .clone()
    });
    let alpha = crate::paint::alpha();
    draw_texture_ex(
        &tex,
        x,
        y,
        Color::new(1.0, 1.0, 1.0, alpha),
        DrawTextureParams { dest_size: Some(vec2(size, size)), ..Default::default() },
    );
}

#[cfg(test)]
pub(crate) mod tests {
    #[test]
    fn icon_has_transparent_corners_and_opaque_middle() {
        let px = super::render(32);
        assert_eq!(px[3], 0, "corner is transparent");
        let mid = (16 * 32 + 16) * 4;
        assert_eq!(px[mid + 3], 255);
    }

    /// Writes the emblem as PNG files and a Windows .ico into
    /// `crates/client/assets/icon/`:
    /// `cargo test --release -p snook-legend export_icons -- --ignored`.
    #[test]
    #[ignore]
    fn export_icons() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icon");
        std::fs::create_dir_all(&dir).unwrap();
        let png = |size: usize| -> Vec<u8> {
            let mut bytes = Vec::new();
            {
                let mut enc = png::Encoder::new(&mut bytes, size as u32, size as u32);
                enc.set_color(png::ColorType::Rgba);
                enc.set_depth(png::BitDepth::Eight);
                enc.set_compression(png::Compression::Best);
                let mut w = enc.write_header().unwrap();
                w.write_image_data(&super::render(size)).unwrap();
            }
            bytes
        };
        for size in [64, 128, 256, 512, 1024] {
            std::fs::write(dir.join(format!("snook-legend-{size}.png")), png(size)).unwrap();
        }
        // A .ico with PNG entries (Windows Vista and later).
        let sizes = [16usize, 20, 24, 32, 40, 48, 64, 128, 256];
        let images: Vec<Vec<u8>> = sizes.iter().map(|&s| png(s)).collect();
        let mut ico = Vec::new();
        ico.extend_from_slice(&[0, 0, 1, 0]);
        ico.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
        let mut offset = 6 + 16 * sizes.len();
        for (&s, img) in sizes.iter().zip(&images) {
            let b = if s >= 256 { 0 } else { s as u8 };
            ico.extend_from_slice(&[b, b, 0, 0]);
            ico.extend_from_slice(&1u16.to_le_bytes());
            ico.extend_from_slice(&32u16.to_le_bytes());
            ico.extend_from_slice(&(img.len() as u32).to_le_bytes());
            ico.extend_from_slice(&(offset as u32).to_le_bytes());
            offset += img.len();
        }
        for img in &images {
            ico.extend_from_slice(img);
        }
        std::fs::write(dir.join("snook-legend.ico"), ico).unwrap();

        // The Windows installer's pictures (WiX: 493x58 banner, 493x312 welcome page).
        let banner = picture(493, 58, |_, _| [1.0, 1.0, 1.0], &[(46, 493 - 52, 6)]);
        std::fs::write(dir.join("wix-banner.bmp"), bmp(493, 58, &banner)).unwrap();
        // The welcome page: the emblem on cloth on the left (the text goes on the right).
        let dialog = picture(
            493,
            312,
            |x, y| {
                if x >= 164 {
                    return [1.0, 1.0, 1.0];
                }
                let (u, v) = (x as f32 / 164.0 - 0.5, y as f32 / 312.0 - 0.35);
                let k = (1.0 - (u * u + v * v).sqrt() * 1.1).clamp(0.0, 1.0);
                let felt = [0.03 + 0.10 * k, 0.17 + 0.30 * k, 0.09 + 0.16 * k];
                // A brass line along the edge of the cloth.
                if x >= 160 { [0.80, 0.62, 0.30] } else { felt }
            },
            &[(120, 20, 64)],
        );
        std::fs::write(dir.join("wix-dialog.bmp"), bmp(493, 312, &dialog)).unwrap();
    }

    /// Writes the Microsoft Store package's pictures into
    /// `crates/client/msix/Assets/` (at twice their nominal size, which the
    /// Store recommends; Windows scales them down):
    /// `cargo test --release -p snook-legend export_msix_assets -- --ignored`.
    #[test]
    #[ignore]
    fn export_msix_assets() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("msix/Assets");
        std::fs::create_dir_all(&dir).unwrap();
        // RGBA, transparent around the emblem: (file, width, height, emblem size).
        let mut files: Vec<(String, usize, usize, usize)> = vec![
            ("Square44x44Logo.scale-200.png".into(), 88, 88, 88),
            ("Square150x150Logo.png".into(), 300, 300, 216),
            ("Wide310x150Logo.png".into(), 620, 300, 216),
            ("StoreLogo.png".into(), 100, 100, 100),
        ];
        // The app list, the taskbar and the title bar ask for these exact
        // sizes. Without the "unplated" ones Windows draws the icon on a
        // square of the accent colour (the blue plate).
        for size in MSIX_TARGET_SIZES {
            for form in ["", "_altform-unplated", "_altform-lightunplated"] {
                files.push((
                    format!("Square44x44Logo.targetsize-{size}{form}.png"),
                    size,
                    size,
                    size,
                ));
            }
        }
        for (name, w, h, e) in files {
            let emblem = super::render(e);
            let mut px = vec![0u8; w * h * 4];
            let (ox, oy) = ((w - e) / 2, (h - e) / 2);
            for y in 0..e {
                let src = &emblem[y * e * 4..(y + 1) * e * 4];
                let at = ((oy + y) * w + ox) * 4;
                px[at..at + e * 4].copy_from_slice(src);
            }
            let mut bytes = Vec::new();
            {
                let mut enc = png::Encoder::new(&mut bytes, w as u32, h as u32);
                enc.set_color(png::ColorType::Rgba);
                enc.set_depth(png::BitDepth::Eight);
                enc.set_compression(png::Compression::Best);
                enc.write_header().unwrap().write_image_data(&px).unwrap();
            }
            std::fs::write(dir.join(name), bytes).unwrap();
        }
    }

    /// The icon sizes the Windows shell asks a packaged app for.
    pub const MSIX_TARGET_SIZES: [usize; 9] = [16, 20, 24, 32, 40, 48, 64, 96, 256];

    /// An RGB picture: `background(x, y)` with emblems `(size, x, y)` on top.
    fn picture(
        w: usize,
        h: usize,
        background: impl Fn(usize, usize) -> [f32; 3],
        emblems: &[(usize, usize, usize)],
    ) -> Vec<[f32; 3]> {
        let mut out: Vec<[f32; 3]> = (0..w * h).map(|i| background(i % w, i / w)).collect();
        for &(size, ex, ey) in emblems {
            let px = super::render(size);
            for y in 0..size {
                for x in 0..size {
                    let s = &px[(y * size + x) * 4..][..4];
                    let a = s[3] as f32 / 255.0;
                    let d = &mut out[(ey + y) * w + ex + x];
                    for k in 0..3 {
                        d[k] += (s[k] as f32 / 255.0 - d[k]) * a;
                    }
                }
            }
        }
        out
    }

    /// A 24-bit Windows bitmap (bottom-up rows, padded to 4 bytes).
    fn bmp(w: usize, h: usize, px: &[[f32; 3]]) -> Vec<u8> {
        let row = (w * 3).div_ceil(4) * 4;
        let size = 54 + row * h;
        let mut out = Vec::with_capacity(size);
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&(size as u32).to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&54u32.to_le_bytes());
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&(w as i32).to_le_bytes());
        out.extend_from_slice(&(h as i32).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&24u16.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&((row * h) as u32).to_le_bytes());
        out.extend_from_slice(&2835u32.to_le_bytes());
        out.extend_from_slice(&2835u32.to_le_bytes());
        out.extend_from_slice(&[0; 8]);
        for y in (0..h).rev() {
            let start = out.len();
            for x in 0..w {
                let c = px[y * w + x];
                for k in [2, 1, 0] {
                    out.push((c[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
                }
            }
            out.resize(start + row, 0);
        }
        out
    }
}
