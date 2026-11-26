//! Motion in the interface: values that ease toward where they are going
//! (a button lighting up under the pointer, a tab's underline moving to
//! the tab chosen, a switch's knob), what comes in (a screen, a dialog, a
//! pane, a page of a shop), and the player's choice to have none of it
//! (Settings → Game → Reduce motion).
//!
//! Immediate mode keeps no widget between frames, so their motion lives in
//! a small store in [`Ui`], keyed by a hash of what the widget is (its
//! place on screen, a name): a few floats per key, eased each frame with
//! the frame's time and forgotten a few frames after the widget is last
//! drawn. Everything here is framerate independent (exponential easing on
//! the real frame time).

use std::cell::Cell;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};

use macroquad::prelude::*;

use super::Ui;

/// How long a screen takes to come in.
pub const SCREEN_SECS: f32 = 0.2;
/// How long a dialog takes to come in.
pub const DIALOG_SECS: f32 = 0.18;
/// How long a pane or a page takes to slide in.
pub const SWITCH_SECS: f32 = 0.2;
/// How long a popover (a menu, a tray) takes to open.
pub const POPOVER_SECS: f32 = 0.13;
/// How quickly what follows a target catches up (per second): a hover's
/// light, a pressed button, the selection of a tab.
pub const HOVER_RATE: f32 = 18.0;
pub const PRESS_RATE: f32 = 32.0;
/// Frames a key is kept after its widget was last drawn.
const KEEP_FRAMES: u64 = 3;

static REDUCE: AtomicBool = AtomicBool::new(false);

/// The player asked for less motion: what slides, scales or glides gets
/// there at once (colours still ease: that is no motion).
pub fn set_reduce_motion(on: bool) {
    REDUCE.store(on, Ordering::Relaxed);
}

pub fn reduced() -> bool {
    REDUCE.load(Ordering::Relaxed)
}

/// The time the last frame took, for the interface's motion, in seconds
/// (at most a tenth: a stall never skips a whole animation, and a frame
/// that took no time moves nothing). Development builds: SNOOK_LEGEND_DT
/// fixes it (screenshots taken frame by frame show the easing as it is at
/// 60 frames a second however slow the renderer).
pub fn dt() -> f32 {
    static FIXED: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    let fixed = FIXED.get_or_init(|| crate::dev::var("SNOOK_LEGEND_DT")?.parse().ok());
    fixed.unwrap_or_else(get_frame_time).clamp(0.0, 0.1)
}

/// Eased out (cubic): fast at first, settling gently.
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// `v` moved toward `target` at `rate` (per second; exponential, so it
/// never overshoots and does not depend on the frame rate). Close enough,
/// it is there.
pub fn follow(v: f32, target: f32, rate: f32, dt: f32, snap: f32) -> f32 {
    let next = v + (target - v) * (1.0 - (-rate * dt).exp());
    if (target - next).abs() <= snap { target } else { next }
}

/// A key for the store: two values mixed (splitmix64).
pub const fn mix(a: u64, b: u64) -> u64 {
    let mut z = a ^ b.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A key from a name (at compile time where it can be).
pub const fn id(name: &str) -> u64 {
    let bytes = name.as_bytes();
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let mut i = 0;
    while i < bytes.len() {
        h = (h ^ bytes[i] as u64).wrapping_mul(0x100_0000_01b3);
        i += 1;
    }
    mix(h, 0x51ed)
}

/// A key from a kind of widget and where it is (to a quarter of a unit).
pub fn rect_key(kind: u64, r: Rect) -> u64 {
    let q = |v: f32| (v * 4.0).round() as i32 as u32 as u64;
    mix(mix(kind, q(r.x) | (q(r.y) << 32)), q(r.w) | (q(r.h) << 32))
}

/// The keys are hashes already: used as they are.
#[derive(Default)]
pub(super) struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 ^ *b as u64).wrapping_mul(0x100_0000_01b3);
        }
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = v;
    }
}

/// What the store keeps for a key: a few values, one integer (the last
/// tab or page seen, for one) and the frame it was last used.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Slot {
    pub v: [f32; 4],
    pub aux: i64,
    seen: u64,
}

pub(super) type Store = std::collections::HashMap<u64, Slot, BuildHasherDefault<KeyHasher>>;

/// Where the interface is drawn this moment: a point `p` (interface units)
/// lands on screen at `p * s + t`. Something coming in is drawn moved and
/// scaled through it (see [`Ui::push_fx`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Xf {
    pub s: f32,
    pub t: Vec2,
}

impl Xf {
    pub const ID: Xf = Xf { s: 1.0, t: Vec2::ZERO };

    pub fn point(&self, p: Vec2) -> Vec2 {
        p * self.s + self.t
    }

    pub fn rect(&self, r: Rect) -> Rect {
        let p = self.point(vec2(r.x, r.y));
        Rect::new(p.x, p.y, r.w * self.s, r.h * self.s)
    }
}

thread_local! {
    static XF: Cell<Xf> = const { Cell::new(Xf::ID) };
    /// Model matrices pushed by [`push_model`] and not popped yet.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
    /// The clipping rectangles in force, on screen (each inside the one
    /// before).
    static CLIPS: std::cell::RefCell<Vec<Rect>> = const { std::cell::RefCell::new(Vec::new()) };
    /// The pointer's shape asked for this frame, and the one shown.
    static CURSOR_WANT: Cell<Option<macroquad::miniquad::CursorIcon>> = const { Cell::new(None) };
    static CURSOR_SHOWN: Cell<Option<macroquad::miniquad::CursorIcon>> = const { Cell::new(None) };
}

pub(super) fn xf() -> Xf {
    XF.with(Cell::get)
}

/// Nothing moved any more (the start of a frame's interface, or the
/// hints at its end, should something have been left moved); clips gone.
pub(super) fn reset_xf() {
    XF.with(|x| x.set(Xf::ID));
    let depth = DEPTH.with(|d| d.replace(0));
    if depth > 0 {
        let gl = unsafe { get_internal_gl() }.quad_gl;
        for _ in 0..depth {
            gl.pop_model_matrix();
        }
    }
    if CLIPS.with(|c| !c.borrow().is_empty()) {
        CLIPS.with(|c| c.borrow_mut().clear());
        scissor(None);
    }
}

/// The camera for the interface (set once a frame: a camera change flushes
/// what was drawn so far, and the bundled miniquad's texture cache does not
/// take that well mid-frame, see `push_model`).
pub(super) fn apply_camera() {
    let (w, h) = (super::vw(), super::vh());
    let cam = Camera2D {
        target: vec2(w / 2.0, h / 2.0),
        zoom: vec2(2.0 / w, 2.0 / h),
        ..Default::default()
    };
    set_camera(&cam);
}

/// Draws what follows through `p -> p * s + t` (inside whatever moves
/// already), with the batcher's model matrix: no flush, unlike a camera
/// (an extra flush in a frame let a font's atlas be drawn from a texture
/// the miniquad cache wrongly thought bound: black text).
fn push_model(s: f32, t: Vec2) {
    let m = Mat4::from_translation(vec3(t.x, t.y, 0.0)) * Mat4::from_scale(vec3(s, s, 1.0));
    unsafe { get_internal_gl() }.quad_gl.push_model_matrix(m);
    DEPTH.with(|d| d.set(d.get() + 1));
}

fn pop_model() {
    if DEPTH.with(|d| d.get()) > 0 {
        unsafe { get_internal_gl() }.quad_gl.pop_model_matrix();
        DEPTH.with(|d| d.set(d.get() - 1));
    }
}

/// From here on, drawing is cut to `r` (interface units, where things are
/// drawn now) and to any clip already in force; [`pop_clip`] ends it.
pub fn push_clip(r: Rect) {
    let screen = xf().rect(r);
    CLIPS.with(|c| {
        let mut c = c.borrow_mut();
        let r = match c.last() {
            Some(outer) => intersect(*outer, screen),
            None => screen,
        };
        c.push(r);
        scissor(Some(r));
    });
}

/// Ends the clip of the last [`push_clip`] (the one before it holds again).
pub fn pop_clip() {
    CLIPS.with(|c| {
        let mut c = c.borrow_mut();
        c.pop();
        scissor(c.last().copied());
    });
}

fn intersect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.w).min(b.x + b.w);
    let y1 = (a.y + a.h).min(b.y + b.h);
    Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

fn scissor(r: Option<Rect>) {
    let k = super::ui_scale();
    let gl = unsafe { get_internal_gl() }.quad_gl;
    gl.scissor(r.map(|r| {
        let (x0, y0) = ((r.x * k).floor() as i32, (r.y * k).floor() as i32);
        let (x1, y1) = (((r.x + r.w) * k).ceil() as i32, ((r.y + r.h) * k).ceil() as i32);
        (x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
    }));
}

/// Asks for the pointer's shape this frame (the last ask wins; none: the
/// arrow). Shown by [`apply_cursor`] at the end of the frame.
pub fn want_cursor(icon: macroquad::miniquad::CursorIcon) {
    CURSOR_WANT.with(|c| c.set(Some(icon)));
}

/// Shows the pointer's shape asked for this frame (only when it changes:
/// it is a call to the system).
pub(super) fn apply_cursor() {
    use macroquad::miniquad::{CursorIcon, window::set_mouse_cursor};
    let want = CURSOR_WANT.with(|c| c.replace(None)).unwrap_or(CursorIcon::Default);
    if CURSOR_SHOWN.with(Cell::get) != Some(want) {
        CURSOR_SHOWN.with(|c| c.set(Some(want)));
        set_mouse_cursor(want);
    }
}

/// What [`Ui::push_fx`] changed, to put back with [`Ui::pop_fx`].
#[must_use = "give it back to Ui::pop_fx"]
pub struct Fx {
    /// Nothing was changed (no motion this frame).
    idle: bool,
    xf: Xf,
    opacity: f32,
    mouse: Vec2,
    press_at: Vec2,
}

/// What [`Ui::modal_scope`] changed, put back when it is dropped.
#[must_use = "keep it while the dialog is drawn"]
pub struct Scope {
    idle: bool,
    xf: Xf,
    opacity: f32,
}

impl Drop for Scope {
    fn drop(&mut self) {
        if !self.idle {
            XF.with(|x| x.set(self.xf));
            pop_model();
            crate::paint::set_opacity(self.opacity);
        }
    }
}

impl Ui {
    /// Forgets what was not drawn for a few frames (once a frame).
    pub(super) fn sweep_anims(&mut self) {
        let frame = self.frame;
        self.anims.get_mut().retain(|_, s| s.seen + KEEP_FRAMES >= frame);
    }

    /// The store's slot for `key`, made with `init` if it is new; marked as
    /// used this frame. `f` reads or changes it.
    pub(super) fn with_slot<R>(
        &self,
        key: u64,
        init: [f32; 4],
        f: impl FnOnce(&mut Slot) -> R,
    ) -> R {
        let mut map = self.anims.borrow_mut();
        let frame = self.frame;
        let slot = map.entry(key).or_insert(Slot { v: init, aux: i64::MIN, seen: frame });
        slot.seen = frame;
        f(slot)
    }

    /// A value for `key` eased toward `target` at `rate` (a new key starts
    /// there). For colours and lights: it eases with reduced motion too.
    pub fn anim(&self, key: u64, target: f32, rate: f32) -> f32 {
        let dt = dt();
        self.with_slot(key, [target; 4], |s| {
            s.v[0] = follow(s.v[0], target, rate, dt, 0.001);
            s.v[0]
        })
    }

    /// Like [`Ui::anim`], for where something is (a position, a size): with
    /// reduced motion it is there at once.
    pub fn glide(&self, key: u64, target: f32, rate: f32) -> f32 {
        if reduced() {
            return self.with_slot(key, [target; 4], |s| {
                s.v[0] = target;
                target
            });
        }
        let dt = dt();
        self.with_slot(key, [target; 4], |s| {
            s.v[0] = follow(s.v[0], target, rate, dt, 0.05);
            s.v[0]
        })
    }

    /// A rectangle for `key` gliding toward `target` (a selection moving
    /// from one tab to another). A new key starts there.
    pub fn glide_rect(&self, key: u64, target: Rect, rate: f32) -> Rect {
        let t = [target.x, target.y, target.w, target.h];
        if reduced() {
            return self.with_slot(key, t, |s| {
                s.v = t;
                target
            });
        }
        let dt = dt();
        self.with_slot(key, t, |s| {
            for (v, t) in s.v.iter_mut().zip(t) {
                *v = follow(*v, t, rate, dt, 0.05);
            }
            Rect::new(s.v[0], s.v[1], s.v[2], s.v[3])
        })
    }

    /// How far something drawn under `key` has come in, 0 to 1 over `secs`
    /// from the first frame it is drawn (1 at once with reduced motion).
    pub fn appear(&self, key: u64, secs: f32) -> f32 {
        if reduced() {
            return 1.0;
        }
        let dt = dt();
        self.with_slot(key, [0.0; 4], |s| {
            let t = s.v[0];
            s.v[0] = (t + dt / secs.max(0.001)).min(1.0);
            t
        })
    }

    /// Draws what follows moved by `offset`, scaled by `scale` about
    /// `pivot` and `alpha` as opaque (interface units of the space drawn
    /// in now); the pointer is moved alike, so what is drawn still answers
    /// to it. Give the result to [`Ui::pop_fx`] when done.
    pub fn push_fx(&mut self, offset: Vec2, scale: f32, pivot: Vec2, alpha: f32) -> Fx {
        let outer = xf();
        let opacity = crate::paint::opacity();
        let (mouse, press_at) = (self.mouse, self.press_at);
        if offset.length_squared() < 1.0e-6 && (scale - 1.0).abs() < 1.0e-4 && alpha >= 0.999 {
            return Fx { idle: true, xf: outer, opacity, mouse, press_at };
        }
        let scale = scale.max(0.01);
        // p → pivot + (p - pivot) * scale + offset, inside what was before.
        let t = pivot * (1.0 - scale) + offset;
        let mut inner = Xf { s: outer.s * scale, t: outer.t + t * outer.s };
        // Whole pixels where nothing is scaled (text stays crisp).
        if (inner.s - 1.0).abs() < 1.0e-4 {
            let k = super::ui_scale();
            inner.t = (inner.t * k).round() / k;
        }
        XF.with(|x| x.set(inner));
        // In the outer space: p -> p * scale + (inner.t - outer.t) / outer.s.
        push_model(scale, (inner.t - outer.t) / outer.s);
        // The pointer in the new space.
        let local = |p: Vec2| (outer.point(p) - inner.t) / inner.s;
        self.mouse = local(mouse);
        self.press_at = local(press_at);
        crate::paint::set_opacity(opacity * alpha.clamp(0.0, 1.0));
        Fx { idle: false, xf: outer, opacity, mouse, press_at }
    }

    /// Puts back what [`Ui::push_fx`] changed.
    pub fn pop_fx(&mut self, fx: Fx) {
        if fx.idle {
            return;
        }
        XF.with(|x| x.set(fx.xf));
        pop_model();
        crate::paint::set_opacity(fx.opacity);
        self.mouse = fx.mouse;
        self.press_at = fx.press_at;
    }

    /// `f` drawn where the screen is, whatever is moving now (the dimming
    /// behind a dialog covers the window even while the screen under it
    /// comes in).
    pub fn unmoved<R>(&mut self, f: impl FnOnce(&mut Ui) -> R) -> R {
        let x = xf();
        if x == Xf::ID {
            return f(self);
        }
        let (mouse, press_at) = (self.mouse, self.press_at);
        self.mouse = x.point(mouse);
        self.press_at = x.point(press_at);
        XF.with(|c| c.set(Xf::ID));
        // Undone: p -> (p - t) / s.
        push_model(1.0 / x.s, -x.t / x.s);
        let r = f(self);
        pop_model();
        XF.with(|c| c.set(x));
        self.mouse = mouse;
        self.press_at = press_at;
        r
    }

    /// The darkening behind the full-screen pages (shop, profile,
    /// ranking…): it fades in from the main menu, and stays put from one
    /// page to the next (only the page comes in).
    pub fn screen_dim(&mut self, alpha: f32) {
        let a = alpha * ease_out(self.appear(id("screen-dim"), 0.16));
        self.unmoved(|_| {
            // Not faded with the page coming in: it is already there.
            macroquad::shapes::draw_rectangle(
                0.0,
                0.0,
                super::vw(),
                super::vh(),
                Color::new(0.0, 0.0, 0.0, a * crate::paint::opacity()),
            );
        });
    }

    /// The darkening behind a dialog (`key`: the dialog's), faded in; it
    /// holds the pointer, so nothing under it reacts.
    pub fn backdrop(&mut self, key: u64, alpha: f32) {
        let a = alpha * ease_out(self.appear(mix(key, 0xbac0), 0.15));
        self.unmoved(|ui| {
            let (w, h) = (super::vw(), super::vh());
            crate::paint::draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, a));
            ui.block(Rect::new(0.0, 0.0, w, h));
        });
    }

    /// A dialog coming in (`key`: which one): it fades in and grows the
    /// last bit to its size, rising a little. Draw it, then
    /// [`Ui::pop_fx`].
    pub fn dialog_in(&mut self, key: u64) -> Fx {
        let t = self.appear(key, DIALOG_SECS);
        let e = ease_out(t);
        let pivot = vec2(super::vw() / 2.0, super::vh() / 2.0);
        let alpha = ease_out((t * 2.2).min(1.0));
        self.push_fx(vec2(0.0, 10.0 * (1.0 - e)), 0.95 + 0.05 * e, pivot, alpha)
    }

    /// [`Ui::backdrop`] and [`Ui::dialog_in`] for a dialog drawn by the
    /// rest of the function (however it returns): what it changed is put
    /// back when the result goes out of scope. The pointer is left as it
    /// is (off by a few units for the first frames only).
    pub fn modal_scope(&mut self, key: u64, dim: f32) -> Scope {
        self.backdrop(key, dim);
        let fx = self.dialog_in(key);
        self.scope_of(fx)
    }

    /// [`Ui::switch_in`] for content drawn by the rest of the function
    /// (however it returns), put back when the result is dropped; the
    /// pointer is left as it is.
    pub fn switch_scope(&mut self, key: u64, index: i64, dist: f32) -> Scope {
        let fx = self.switch_in(key, index, dist);
        self.scope_of(fx)
    }

    /// [`Ui::popover_in`] for a popover drawn by the rest of the function
    /// (however it returns), put back when the result is dropped; the
    /// pointer is left as it is.
    pub fn popover_scope(&mut self, key: u64, anchor: Vec2) -> Scope {
        let fx = self.popover_in(key, anchor);
        self.scope_of(fx)
    }

    fn scope_of(&mut self, fx: Fx) -> Scope {
        self.mouse = fx.mouse;
        self.press_at = fx.press_at;
        Scope { idle: fx.idle, xf: fx.xf, opacity: fx.opacity }
    }

    /// A popover opening from `anchor` (a menu under its button, a tray):
    /// it fades in and grows from there. Draw it, then [`Ui::pop_fx`].
    pub fn popover_in(&mut self, key: u64, anchor: Vec2) -> Fx {
        let t = self.appear(key, POPOVER_SECS);
        let e = ease_out(t);
        self.push_fx(Vec2::ZERO, 0.94 + 0.06 * e, anchor, ease_out((t * 1.4).min(1.0)))
    }

    /// Where the selection of a run of tabs is this frame: tabs drawn one
    /// after another side by side (or one under another: `down`), each
    /// passing where it is and whether it is the one chosen. The selection
    /// glides from the tab chosen before to the one chosen now, and every
    /// tab of the run gets the same place in a frame (moved when its first
    /// tab is drawn, toward where the chosen one was last seen), so each
    /// can draw the part over itself. None until a tab is chosen.
    pub fn selection_glide(&mut self, r: Rect, selected: bool, down: bool) -> Option<Rect> {
        let same_run = |prev: Rect| {
            if down {
                (prev.x - r.x).abs() < 0.5
                    && (prev.w - r.w).abs() < 0.5
                    && (r.y - (prev.y + prev.h)).abs() < 20.0
            } else {
                (prev.y - r.y).abs() < 0.5
                    && (prev.h - r.h).abs() < 0.5
                    && (r.x - (prev.x + prev.w)).abs() < 20.0
            }
        };
        let (key, first) = match self.tab_run {
            Some((prev, d, key)) if d == down && same_run(prev) => (key, false),
            _ => {
                let q = |v: f32| (v * 4.0).round() as i64 as u64;
                (mix(mix(id("tab-run"), q(r.x) | (q(r.y) << 32)), down as u64), true)
            }
        };
        self.tab_run = Some((r, down, key));
        let target_key = mix(key, 1);
        let frame = self.frame;
        let reduce = reduced();
        let dt = dt();
        let mut map = self.anims.borrow_mut();
        // (Where the chosen tab was last seen: this frame or the last one.
        // No tab chosen any more, no selection.)
        let target = map.get(&target_key).filter(|s| s.aux + 1 >= frame as i64).map(|s| s.v);
        if selected {
            let v = [r.x, r.y, r.w, r.h];
            map.insert(target_key, Slot { v, aux: frame as i64, seen: frame });
            map.entry(key).or_insert(Slot { v, aux: 0, seen: frame });
        } else if target.is_none() {
            return None;
        } else if let Some(t) = map.get_mut(&target_key) {
            t.seen = frame;
        }
        let slot = map.get_mut(&key)?;
        slot.seen = frame;
        if first && let Some(t) = target {
            for (v, t) in slot.v.iter_mut().zip(t) {
                *v = if reduce { t } else { follow(*v, t, 22.0, dt, 0.05) };
            }
        }
        Some(Rect::new(slot.v[0], slot.v[1], slot.v[2], slot.v[3]))
    }

    /// What follows takes the place of what was there when `index`
    /// changes for `key` (a tab chosen): it slides in by `dist` from the
    /// side it was asked from (the next one from the right, the one before
    /// from the left) and fades in. The first time, it is simply there.
    /// Draw it, then [`Ui::pop_fx`].
    pub fn switch_in(&mut self, key: u64, index: i64, dist: f32) -> Fx {
        self.switch_fx(key, index, vec2(dist, 0.0))
    }

    /// Like [`Ui::switch_in`], for tabs one above the other (the shop's
    /// categories): the next one rises from below.
    pub fn switch_down(&mut self, key: u64, index: i64, dist: f32) -> Fx {
        self.switch_fx(key, index, vec2(0.0, dist))
    }

    /// What follows fades in, moving the last of `offset` into place, the
    /// first frames it is drawn after a while without (a part of a screen
    /// that comes back). Draw it, then [`Ui::pop_fx`].
    pub fn come_in(&mut self, key: u64, offset: Vec2) -> Fx {
        let t = self.appear(key, SWITCH_SECS);
        if t >= 1.0 {
            return self.push_fx(Vec2::ZERO, 1.0, Vec2::ZERO, 1.0);
        }
        let e = ease_out(t);
        self.push_fx(offset * (1.0 - e), 1.0, Vec2::ZERO, ease_out((t * 1.3).min(1.0)))
    }

    fn switch_fx(&mut self, key: u64, index: i64, toward: Vec2) -> Fx {
        let (t, dir) = self.switch_progress(key, index);
        if t >= 1.0 {
            return self.push_fx(Vec2::ZERO, 1.0, Vec2::ZERO, 1.0);
        }
        let e = ease_out(t);
        self.push_fx(toward * dir * (1.0 - e), 1.0, Vec2::ZERO, ease_out((t * 1.3).min(1.0)))
    }

    /// How far the switch of `key` to `index` has come (0..1) and from
    /// which side (+1: from the right).
    fn switch_progress(&self, key: u64, index: i64) -> (f32, f32) {
        let reduce = reduced();
        let dt = dt();
        self.with_slot(key, [1.0, 1.0, 0.0, 0.0], |s| {
            if s.aux == i64::MIN {
                s.aux = index;
            } else if s.aux != index {
                s.v[1] = if index > s.aux { 1.0 } else { -1.0 };
                s.aux = index;
                s.v[0] = if reduce { 1.0 } else { 0.0 };
            }
            let t = s.v[0];
            s.v[0] = (t + dt / SWITCH_SECS).min(1.0);
            (t, s.v[1])
        })
    }

    /// The widget at `r` under the pointer (`hover`) and pressed (`down`):
    /// how lit (0..1) and how far pressed in (0..1) it is now, eased.
    pub fn hover_press(&self, r: Rect, hover: bool, down: bool) -> (f32, f32) {
        let key = rect_key(id("hover"), r);
        let dt = dt();
        let (h, p) = (hover as u8 as f32, (hover && down) as u8 as f32);
        self.with_slot(key, [h, p, 0.0, 0.0], |s| {
            s.v[0] = follow(s.v[0], h, HOVER_RATE, dt, 0.001);
            // Pressed in quickly, let go a little slower.
            let rate = if p > s.v[1] { PRESS_RATE } else { PRESS_RATE * 0.6 };
            s.v[1] = follow(s.v[1], p, rate, dt, 0.001);
            (s.v[0], s.v[1])
        })
    }
}

/// The pointer as a development script has it this frame (see
/// [`scripted_input`]).
pub(super) struct Scripted {
    pub mouse: Vec2,
    pub pressed: bool,
    pub released: bool,
    pub down: bool,
    pub wheel: f32,
}

/// A development script of the pointer: its steps (frame, what), where
/// the pointer is and whether it is held.
type Script = (Vec<(u64, String)>, Vec2, bool);

thread_local! {
    static SCRIPT: std::cell::RefCell<Option<Script>> = const { std::cell::RefCell::new(None) };
    /// The keys the script presses this frame (and whether with Shift), and
    /// the mouse's back button.
    static SCRIPT_KEYS: std::cell::RefCell<(Vec<KeyCode>, bool, bool)> =
        const { std::cell::RefCell::new((Vec::new(), false, false)) };
}

/// The script pressed `key` this frame.
pub(super) fn scripted_key(key: KeyCode) -> bool {
    SCRIPT_KEYS.with(|k| k.borrow().0.contains(&key))
}

/// The script holds Shift with its keys this frame.
pub(super) fn scripted_shift() -> bool {
    SCRIPT_KEYS.with(|k| k.borrow().1)
}

/// The script pressed the mouse's back button this frame.
pub(super) fn scripted_back() -> bool {
    SCRIPT_KEYS.with(|k| k.borrow().2)
}

/// Development builds: SNOOK_LEGEND_INPUT plays the pointer from a script,
/// for screenshots taken frame by frame (SNOOK_LEGEND_SHOTS) of what moves:
/// `40:move 600 300;45:wheel -3;50:down;52:up` (interface units; the wheel
/// in the platform's raw steps, -1 a notch down on Linux); keys too
/// (`60:key tab`, `61:key shift+tab`, `62:key enter`: tab, enter, space,
/// escape and the arrows) and the mouse's back button (`70:back`). None: no
/// script.
pub(super) fn scripted_input(frame: u64) -> Option<Scripted> {
    SCRIPT.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            let text = crate::dev::var("SNOOK_LEGEND_INPUT")?;
            let steps = text
                .split(';')
                .filter_map(|step| {
                    let (at, what) = step.split_once(':')?;
                    Some((at.trim().parse().ok()?, what.trim().to_string()))
                })
                .collect();
            *cell = Some((steps, Vec2::splat(-1.0e4), false));
        }
        let (steps, at, held) = cell.as_mut()?;
        let mut keys = (Vec::new(), false, false);
        let mut out =
            Scripted { mouse: *at, pressed: false, released: false, down: *held, wheel: 0.0 };
        for (_, what) in steps.iter().filter(|(f, _)| *f == frame) {
            let mut words = what.split_whitespace();
            match words.next() {
                Some("move") => {
                    let x = words.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    let y = words.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    *at = vec2(x, y);
                    out.mouse = *at;
                }
                Some("down") => {
                    *held = true;
                    out.pressed = true;
                    out.down = true;
                }
                Some("up") => {
                    *held = false;
                    out.released = true;
                    out.down = false;
                }
                Some("wheel") => {
                    out.wheel += words.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
                }
                Some("key") => {
                    let name = words.next().unwrap_or("");
                    let (shift, name) = match name.strip_prefix("shift+") {
                        Some(rest) => (true, rest),
                        None => (false, name),
                    };
                    let key = match name {
                        "tab" => Some(KeyCode::Tab),
                        "enter" => Some(KeyCode::Enter),
                        "space" => Some(KeyCode::Space),
                        "escape" => Some(KeyCode::Escape),
                        "left" => Some(KeyCode::Left),
                        "right" => Some(KeyCode::Right),
                        "up" => Some(KeyCode::Up),
                        "down" => Some(KeyCode::Down),
                        _ => None,
                    };
                    keys.0.extend(key);
                    keys.1 |= shift;
                }
                Some("back") => keys.2 = true,
                _ => {}
            }
        }
        SCRIPT_KEYS.with(|k| *k.borrow_mut() = keys);
        Some(out)
    })
}
