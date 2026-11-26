//! Dialogs going out. A dialog is drawn only while it is open: closed, the
//! code that drew it (and often what it showed) is gone the same frame. So
//! the last look of each open dialog is kept, copied from the screen at
//! the end of every frame into a texture of its own (only the dialog's
//! panel, a few hundred pixels, on the graphics card), and once the dialog
//! is no longer drawn that picture fades out and shrinks a little where it
//! was (0.14 s) while the dimming behind it lifts. It takes no input: what
//! is under it works again at once.
//!
//! Every dialog drawn through [`Ui::modal_scope`] or [`Ui::dialog_scope`]
//! has it; where it is, is the panels ([`Ui::panel`]) it draws. Reduce
//! motion: closed at once, as before.

use std::cell::RefCell;
use std::collections::HashMap;

use macroquad::miniquad::{self, RawId, TextureFormat, TextureId, TextureParams};
use macroquad::prelude::*;

use super::motion::{self, ease_out};

/// How long a dialog takes to go out.
pub const CLOSE_SECS: f32 = 0.14;
/// How long the dimming behind it takes to lift.
const DIM_SECS: f32 = 0.15;
/// Dialogs kept at once (open, or going out): one over another at most.
const POOL: usize = 4;
/// (Not in the bundled bindings' constants.)
const GL_TEXTURE_BINDING_2D: u32 = 0x8069;

/// A dialog's last look: which texture, where it was (interface units, on
/// screen) and the pixels of it in the texture (x, y from the bottom, w,
/// h: the screen's own rows, as OpenGL copies them).
#[derive(Debug, Clone, Copy)]
struct Shot {
    slot: usize,
    rect: Rect,
    px: (i32, i32, i32, i32),
}

/// A dialog going out (`t`: seconds since it closed).
struct Ghost {
    shot: Shot,
    t: f32,
}

/// A dimming lifting (`alpha` when its dialog closed).
struct Dim {
    alpha: f32,
    t: f32,
}

#[derive(Default)]
struct State {
    /// The textures (the screen's size, in pixels) and which are free.
    size: (u32, u32),
    slots: Vec<TextureId>,
    free: Vec<usize>,
    /// The dialogs drawn this frame and the last (key, where on screen).
    open: Vec<(u64, Option<Rect>)>,
    prev_open: Vec<u64>,
    /// The dialogs being drawn now, innermost last (places in `open`).
    stack: Vec<usize>,
    /// The dimmings drawn this frame and the last (key, how dark).
    dims: Vec<(u64, f32)>,
    prev_dims: Vec<(u64, f32)>,
    /// The last look of each dialog open.
    shots: HashMap<u64, Shot>,
    ghosts: Vec<Ghost>,
    lifting: Vec<Dim>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// A new frame: what was drawn becomes what was drawn last.
pub(super) fn begin_frame() {
    STATE.with(|s| {
        let s = &mut *s.borrow_mut();
        s.prev_open = s.open.drain(..).map(|(k, _)| k).collect();
        s.prev_dims = std::mem::take(&mut s.dims);
        s.stack.clear();
    });
}

/// A dialog (`key`) begins to be drawn.
pub(super) fn dialog_begin(key: u64) {
    STATE.with(|s| {
        let s = &mut *s.borrow_mut();
        s.open.push((key, None));
        let i = s.open.len() - 1;
        s.stack.push(i);
    });
}

/// The dialog drawn last has been drawn.
pub(super) fn dialog_end() {
    STATE.with(|s| {
        s.borrow_mut().stack.pop();
    });
}

/// A panel drawn at `r` (on screen): part of the dialog being drawn.
pub(super) fn note_panel(r: Rect) {
    STATE.with(|s| {
        let s = &mut *s.borrow_mut();
        if let Some(&i) = s.stack.last() {
            let rect = &mut s.open[i].1;
            *rect = Some(match *rect {
                Some(a) => union(a, r),
                None => r,
            });
        }
    });
}

/// The dimming behind a dialog (`key`), `alpha` dark this frame.
pub(super) fn note_dim(key: u64, alpha: f32) {
    STATE.with(|s| s.borrow_mut().dims.push((key, alpha)));
}

fn union(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.w).max(b.x + b.w);
    let y1 = (a.y + a.h).max(b.y + b.h);
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// The end of the frame's interface (before the hover hints): the dialogs
/// closed this frame start going out, the open ones' look is kept, and
/// what is going out is drawn (interface units, nothing moved).
pub(super) fn end_frame() {
    STATE.with(|s| {
        let s = &mut *s.borrow_mut();
        if motion::reduced() {
            // Closed at once (and nothing kept).
            s.ghosts.clear();
            s.lifting.clear();
            let shots: Vec<Shot> = s.shots.drain().map(|(_, v)| v).collect();
            s.free.extend(shots.iter().map(|v| v.slot));
            return;
        }
        let (w, h) = miniquad::window::screen_size();
        let size = (w.max(1.0) as u32, h.max(1.0) as u32);
        if size != s.size {
            // Another window size: the textures go (and what they kept).
            let ctx = unsafe { get_internal_gl() }.quad_context;
            for id in s.slots.drain(..) {
                ctx.delete_texture(id);
            }
            s.free.clear();
            s.shots.clear();
            s.ghosts.clear();
            s.size = size;
        }
        // Closed this frame: drawn last frame, not this one.
        let open: Vec<u64> = s.open.iter().map(|(k, _)| *k).collect();
        for key in std::mem::take(&mut s.prev_open) {
            if !open.contains(&key)
                && let Some(shot) = s.shots.remove(&key)
            {
                s.ghosts.push(Ghost { shot, t: 0.0 });
            }
        }
        for (key, alpha) in std::mem::take(&mut s.prev_dims) {
            if alpha > 0.01 && !s.dims.iter().any(|(k, _)| *k == key) {
                s.lifting.push(Dim { alpha, t: 0.0 });
            }
        }
        // Whatever is no longer open keeps no look.
        let gone: Vec<u64> = s.shots.keys().filter(|k| !open.contains(k)).copied().collect();
        for key in gone {
            if let Some(shot) = s.shots.remove(&key) {
                s.free.push(shot.slot);
            }
        }
        capture(s);
        draw(s);
    });
}

/// Keeps the look of every dialog open this frame (what is drawn so far is
/// put on screen first).
fn capture(s: &mut State) {
    let wanted: Vec<(u64, Rect)> =
        s.open.iter().filter_map(|(k, r)| r.map(|r| (*k, r))).filter(|(_, r)| r.w > 1.0).collect();
    if wanted.is_empty() {
        return;
    }
    let k = s.size.0 as f32 / super::vw();
    let (sw, sh) = (s.size.0 as i32, s.size.1 as i32);
    let mut gl = unsafe { get_internal_gl() };
    let mut flushed = false;
    for (key, rect) in wanted {
        let slot = match s.shots.get(&key) {
            Some(shot) => shot.slot,
            None => match s.free.pop() {
                Some(slot) => slot,
                None if s.slots.len() < POOL => {
                    let id = gl.quad_context.new_render_texture(TextureParams {
                        width: s.size.0,
                        height: s.size.1,
                        // (The screen's alpha means nothing: none is kept.)
                        format: TextureFormat::RGB8,
                        min_filter: miniquad::FilterMode::Linear,
                        mag_filter: miniquad::FilterMode::Linear,
                        ..Default::default()
                    });
                    s.slots.push(id);
                    s.slots.len() - 1
                }
                None => continue,
            },
        };
        let raw = match unsafe { gl.quad_context.texture_raw_id(s.slots[slot]) } {
            RawId::OpenGl(raw) => raw,
            // (Metal, where it is used: the dialog closes at once.)
            #[allow(unreachable_patterns)]
            _ => {
                s.free.push(slot);
                continue;
            }
        };
        // Whole pixels, within the screen; rows from the bottom.
        let x0 = ((rect.x * k).floor() as i32).clamp(0, sw);
        let x1 = (((rect.x + rect.w) * k).ceil() as i32).clamp(0, sw);
        let top = ((rect.y * k).floor() as i32).clamp(0, sh);
        let bottom = (((rect.y + rect.h) * k).ceil() as i32).clamp(0, sh);
        let (pw, ph) = (x1 - x0, bottom - top);
        if pw <= 0 || ph <= 0 {
            s.free.push(slot);
            continue;
        }
        if !flushed {
            gl.flush();
            flushed = true;
        }
        let y0 = sh - bottom;
        unsafe {
            use miniquad::gl::*;
            // Bound for the copy only: the texture cache's idea of what is
            // bound stays true.
            let mut before: GLint = 0;
            glGetIntegerv(GL_TEXTURE_BINDING_2D, &mut before);
            glBindTexture(GL_TEXTURE_2D, raw);
            glCopyTexSubImage2D(GL_TEXTURE_2D, 0, x0, y0, x0, y0, pw, ph);
            glBindTexture(GL_TEXTURE_2D, before as GLuint);
        }
        let shown = Rect::new(x0 as f32 / k, top as f32 / k, pw as f32 / k, ph as f32 / k);
        s.shots.insert(key, Shot { slot, rect: shown, px: (x0, y0, pw, ph) });
    }
}

/// Draws what is going out, and forgets what has gone.
fn draw(s: &mut State) {
    let dt = motion::dt();
    let (w, h) = (super::vw(), super::vh());
    s.lifting.retain_mut(|d| {
        let p = (d.t / DIM_SECS).min(1.0);
        d.t += dt;
        let a = d.alpha * (1.0 - p);
        if a > 0.002 {
            draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, a));
        }
        p < 1.0
    });
    let mut done = Vec::new();
    s.ghosts.retain_mut(|g| {
        let p = (g.t / CLOSE_SECS).min(1.0);
        g.t += dt;
        if p >= 1.0 {
            done.push(g.shot.slot);
            return false;
        }
        let e = ease_out(p);
        let scale = 1.0 - 0.05 * e;
        // (Going out speeds up: it is seen to start.)
        let alpha = 1.0 - p * p;
        let r = g.shot.rect;
        let (cw, ch) = (r.w * scale, r.h * scale);
        let (x, y) = (r.x + (r.w - cw) / 2.0, r.y + (r.h - ch) / 2.0 + 4.0 * e);
        let (px, py, pw, ph) = g.shot.px;
        draw_texture_ex(
            &Texture2D::from_miniquad_texture(s.slots[g.shot.slot]),
            x,
            y,
            Color::new(1.0, 1.0, 1.0, alpha),
            DrawTextureParams {
                dest_size: Some(vec2(cw, ch)),
                source: Some(Rect::new(px as f32, py as f32, pw as f32, ph as f32)),
                flip_y: true,
                ..Default::default()
            },
        );
        true
    });
    s.free.extend(done);
}
