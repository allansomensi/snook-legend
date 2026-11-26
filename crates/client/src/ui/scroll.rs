//! Lists and pages that scroll smoothly: by the pixel, eased toward where
//! the wheel, the keys or the scroll bar send them, dragged with the
//! pointer (a flick carries on and slows down), the rows cut by the edges
//! clipped there and fading into the panel.
//!
//! ```ignore
//! let view = ui.list_begin(area, &mut self.scroll, items.len(), ROW_H);
//! for i in view.rows() {
//!     let y = view.y(i);
//!     // draw row i at y
//! }
//! ui.scroll_end(view, &mut self.scroll);
//! ```

use macroquad::prelude::*;

use super::motion::{self, follow};
use super::{GOLD, Ui, rrect};

/// How far one notch of a wheel scrolls (interface units).
const NOTCH: f32 = 96.0;
/// How quickly the view catches up with where the wheel or the keys sent
/// it (per second): there in about a fifth of a second.
const WHEEL_RATE: f32 = 16.0;
/// A flick carries on for about this long, slowing down.
const FLING_SECS: f32 = 0.32;
/// How far the pointer moves, pressed, before it drags the list (a click
/// that moves a little is still a click).
const DRAG_START: f32 = 7.0;

/// Where a list or a page is scrolled to, kept by the screen. Starts at
/// the top.
#[derive(Debug, Clone, Default)]
pub struct Scroll {
    /// How far down the view is now (interface units from the top).
    pos: f32,
    /// Where it is heading.
    target: f32,
    /// How quickly it gets there (per second; 0: the wheel's).
    rate: f32,
    /// How far it can go, and how tall the view was, last drawn.
    max: f32,
    view_h: f32,
    /// The pointer pressed on the content: where, and the scroll then;
    /// `dragging` once it has moved far enough to drag it.
    grab: Option<(f32, f32)>,
    dragging: bool,
    /// The speed of a drag (units per second), for a flick.
    vel: f32,
    /// Where the scroll bar was drawn (a press there is not a drag of the
    /// content).
    bar: Rect,
    /// The arrows, Page Up/Down, Home and End leave it alone (the list
    /// uses them itself: a picker that moves a selection).
    pub no_keys: bool,
}

impl Scroll {
    /// Back at the top at once (new content: another tab, another list).
    pub fn reset(&mut self) {
        let no_keys = self.no_keys;
        *self = Scroll { no_keys, ..Scroll::default() };
    }

    /// How far down it is scrolled now.
    pub fn offset(&self) -> f32 {
        self.pos
    }

    /// At `y` at once (kept within the content once it is drawn).
    pub fn jump_to(&mut self, y: f32) {
        self.pos = y;
        self.target = y;
        self.rate = 0.0;
    }

    /// Scrolls to `y` (eased).
    pub fn scroll_to(&mut self, y: f32) {
        self.target = y.clamp(0.0, self.max.max(0.0));
        self.rate = 0.0;
        if motion::reduced() {
            self.pos = self.target;
        }
    }

    /// Scrolls (eased) just enough for the part of the content from `top`
    /// to `bottom` to be in view.
    pub fn reveal(&mut self, top: f32, bottom: f32) {
        let view = self.view_h.max(1.0);
        let target = if top < self.target {
            top
        } else if bottom > self.target + view {
            bottom - view
        } else {
            return;
        };
        self.scroll_to(target);
    }

    /// [`Scroll::reveal`] for row `i` of rows `row_h` tall.
    pub fn reveal_row(&mut self, i: usize, row_h: f32) {
        self.reveal(i as f32 * row_h, (i + 1) as f32 * row_h);
    }

    /// Moves the content by `delta` (eased; a wheel's notches add up).
    fn nudge(&mut self, delta: f32) {
        let from = if self.rate > 0.0 { self.pos } else { self.target };
        self.target = (from + delta).clamp(0.0, self.max);
        self.rate = 0.0;
    }

    /// One frame of motion toward the target.
    fn step(&mut self, dt: f32) {
        if self.dragging {
            return;
        }
        // The content may have shrunk: never past its end.
        self.target = self.target.clamp(0.0, self.max);
        if motion::reduced() {
            self.pos = self.target;
            return;
        }
        let rate = if self.rate > 0.0 { self.rate } else { WHEEL_RATE };
        self.pos = follow(self.pos, self.target, rate, dt, 0.1);
        if self.pos == self.target {
            self.rate = 0.0;
        }
    }
}

/// A scrolled view being drawn: where its content goes. From
/// [`Ui::list_begin`] or [`Ui::scroll_begin`], back to [`Ui::scroll_end`].
#[must_use = "give it back to Ui::scroll_end"]
pub struct ScrollView {
    /// The part of the screen it shows.
    pub area: Rect,
    /// How far down it is scrolled (whole pixels).
    pub offset: f32,
    /// How tall the content is.
    pub content_h: f32,
    /// Where its scroll bar goes (the caller may move it before
    /// [`Ui::scroll_end`]): at the right edge of the area by default.
    pub track: Rect,
    /// Its edges fade into the panel under it.
    pub fades: bool,
    /// It may scroll (false: it is only cut at its edges for now, a panel
    /// growing to its content).
    pub bar: bool,
    row_h: f32,
    rows: usize,
    max: f32,
    id: u64,
    /// The pointer as it was, when it was hidden from the rows outside the
    /// view.
    saved: Option<(Vec2, bool, bool)>,
    clipped: bool,
}

impl ScrollView {
    /// The rows at least partly in view.
    pub fn rows(&self) -> std::ops::Range<usize> {
        if self.row_h <= 0.0 || self.rows == 0 {
            return 0..0;
        }
        let first = (self.offset / self.row_h).floor().max(0.0) as usize;
        let end = ((self.offset + self.area.h) / self.row_h).ceil().max(0.0) as usize;
        first.min(self.rows)..end.min(self.rows)
    }

    /// Where row `i` starts.
    pub fn y(&self, i: usize) -> f32 {
        self.area.y + i as f32 * self.row_h - self.offset
    }

    /// Where the content starts (above the view once scrolled).
    pub fn top(&self) -> f32 {
        self.area.y - self.offset
    }

    /// The content is taller than the view (it has a scroll bar).
    pub fn scrolls(&self) -> bool {
        self.max > 0.5 && self.bar
    }

    /// The part `y..y + h` of the content (where drawn now) is in view.
    pub fn visible(&self, y: f32, h: f32) -> bool {
        y + h >= self.area.y && y <= self.area.y + self.area.h
    }
}

/// Wheel turns since the last frame, as interface units to scroll (down:
/// positive). The platforms report them differently: Windows in 120ths of
/// a notch, X11 in notches, macOS in points (a touchpad) or tenths of
/// lines; a touchpad's small steps scroll a little at a time.
pub(super) fn wheel_units(raw: f32) -> f32 {
    if raw == 0.0 {
        return 0.0;
    }
    let notches = if cfg!(windows) {
        raw / 120.0
    } else if cfg!(target_os = "macos") {
        if raw.abs() >= 10.0 && (raw / 10.0).fract() == 0.0 {
            raw / 10.0
        } else {
            return -raw * 1.4;
        }
    } else {
        raw.clamp(-4.0, 4.0)
    };
    -notches * NOTCH
}

thread_local! {
    /// The window's own queue of wheel events (all of them: macroquad keeps
    /// only a frame's last).
    static WHEEL_SUB: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// The wheel's turns this frame (all the events, added up), raw.
pub(super) fn wheel_this_frame() -> f32 {
    struct Sum(f32);
    impl macroquad::miniquad::EventHandler for Sum {
        fn update(&mut self) {}
        fn draw(&mut self) {}
        fn mouse_wheel_event(&mut self, _x: f32, y: f32) {
            self.0 += y;
        }
    }
    let id = WHEEL_SUB
        .with(|s| *s.get().get_or_insert_with(macroquad::input::utils::register_input_subscriber));
    WHEEL_SUB.with(|s| s.set(Some(id)));
    let mut sum = Sum(0.0);
    macroquad::input::utils::repeat_all_miniquad_input(&mut sum, id);
    sum.0
}

impl Ui {
    /// The wheel's turns this frame as interface units to scroll (down:
    /// positive), when the pointer is over `r` (None: anywhere) and no
    /// scrolled view took them; they are taken.
    pub fn take_wheel(&mut self, r: Option<Rect>) -> f32 {
        if self.wheel_taken || r.is_some_and(|r| !r.contains(self.mouse)) {
            return 0.0;
        }
        let units = wheel_units(self.wheel);
        if units != 0.0 {
            self.wheel_taken = true;
        }
        units
    }

    /// Begins a list of `rows` rows `row_h` tall in `area`, scrolled by
    /// `s`: draw the rows of [`ScrollView::rows`] at [`ScrollView::y`],
    /// then [`Ui::scroll_end`].
    pub fn list_begin(
        &mut self,
        area: Rect,
        s: &mut Scroll,
        rows: usize,
        row_h: f32,
    ) -> ScrollView {
        let mut v = self.scroll_begin(area, s, rows as f32 * row_h);
        v.row_h = row_h;
        v.rows = rows;
        v
    }

    /// Begins a scrolled view of content `content_h` tall in `area`: draw
    /// it from [`ScrollView::top`] down, then [`Ui::scroll_end`].
    pub fn scroll_begin(&mut self, area: Rect, s: &mut Scroll, content_h: f32) -> ScrollView {
        self.block(area);
        let id = motion::rect_key(motion::id("scroll"), area);
        let max = (content_h - area.h).max(0.0);
        s.max = max;
        s.view_h = area.h;
        // Less content than before (or a jump to the end): within it at
        // once. (Past an end after a drag, it springs back instead.)
        if s.pos > max && s.grab.is_none() && s.rate == 0.0 {
            s.pos = max;
        }
        let dt = motion::dt();
        // A press on the content may become a drag of it.
        let over = area.contains(self.mouse);
        if self.pressed && over && !s.bar.contains(self.mouse) && max > 0.0 {
            s.grab = Some((self.mouse.y, s.pos));
            s.dragging = false;
            s.vel = 0.0;
        }
        if let Some((y0, from)) = s.grab {
            if !self.down {
                // Let go: a flick carries on, slowing down; past an end it
                // springs back.
                s.grab = None;
                if s.dragging {
                    s.dragging = false;
                    let fling = if s.vel.abs() > 60.0 && !motion::reduced() {
                        s.vel * FLING_SECS
                    } else {
                        0.0
                    };
                    s.target = (s.pos + fling).clamp(0.0, max);
                    s.rate = 1.0 / FLING_SECS;
                }
            } else if !s.dragging && (self.active.is_some_and(|a| a != id) || self.mouse.x < -1.0e5)
            {
                // Something in it took the press (a slider), or input went away.
                s.grab = None;
            } else {
                let dy = self.mouse.y - y0;
                if !s.dragging && dy.abs() > DRAG_START {
                    s.dragging = true;
                    self.active = Some(id);
                    // The press is a drag now, not a click on a row.
                    self.press_at = vec2(-1.0e6, -1.0e6);
                }
                if s.dragging {
                    let raw = from - dy;
                    // Past an end, it follows the pointer less and less.
                    let wanted = if raw < 0.0 {
                        -rubber(-raw, area.h)
                    } else if raw > max {
                        max + rubber(raw - max, area.h)
                    } else {
                        raw
                    };
                    if dt > 0.0 {
                        let v = (wanted - s.pos) / dt;
                        s.vel = s.vel + (v - s.vel) * (1.0 - (-dt * 18.0).exp());
                    }
                    s.pos = wanted;
                    s.target = wanted.clamp(0.0, max);
                }
            }
        }
        s.step(dt);
        // Whole pixels: text stays crisp.
        let k = super::ui_scale();
        let offset = (s.pos * k).round() / k;
        let scrolls = max > 0.0;
        let clipped = scrolls || s.pos.abs() > 0.01;
        if clipped {
            motion::push_clip(area);
        }
        // The rows out of sight take no pointer (a drag under way goes on).
        let saved = (!over && self.active.is_none()).then(|| {
            let saved = (self.mouse, self.pressed, self.released);
            self.mouse = vec2(-1.0e6, -1.0e6);
            self.pressed = false;
            self.released = false;
            saved
        });
        let track = Rect::new(area.x + area.w - 5.0, area.y + 2.0, 5.0, area.h - 4.0);
        ScrollView {
            area,
            offset,
            content_h,
            track,
            fades: true,
            bar: true,
            row_h: 0.0,
            rows: 0,
            max,
            id,
            saved,
            clipped,
        }
    }

    /// Ends a scrolled view: its clip, the fades at its edges, its scroll
    /// bar (dragged, or pressed on its track to go there), and the wheel
    /// and keys over it.
    pub fn scroll_end(&mut self, view: ScrollView, s: &mut Scroll) {
        if let Some((mouse, pressed, released)) = view.saved {
            self.mouse = mouse;
            self.pressed = pressed;
            self.released = released;
        }
        if view.clipped {
            motion::pop_clip();
        }
        let area = view.area;
        if view.fades && view.scrolls() {
            let centre = vec2(area.x + area.w / 2.0, area.y + area.h / 2.0);
            if let Some(panel) = self.panel_at(centre) {
                let fade = Rect::new(area.x - 2.0, area.y, area.w + 4.0, area.h);
                Ui::scroll_fades(panel, fade, s.pos > 0.5, s.pos < view.max - 0.5);
            }
        }
        let over = area.contains(self.mouse);
        // The wheel: the innermost view under the pointer takes it.
        if over && view.scrolls() && !self.wheel_taken {
            let units = wheel_units(self.wheel);
            if units != 0.0 {
                self.wheel_taken = true;
                s.nudge(units);
            }
        }
        if view.scrolls() {
            // (Those under a dialog, their pointer taken away, don't count.)
            if self.mouse.x > -1.0e5 {
                self.scroll_views += 1;
            }
            self.scroll_keys(&view, s, over);
            self.scroll_bar(&view, s);
        } else {
            s.bar = Rect::default();
        }
    }

    /// The arrows (a row, or a few lines), Page Up and Down (most of a
    /// view), Home and End: for the view under the pointer, or the only
    /// one on screen.
    fn scroll_keys(&mut self, view: &ScrollView, s: &mut Scroll, over: bool) {
        if s.no_keys || self.focus.is_some() || self.mouse.x < -1.0e5 {
            return;
        }
        if !over && self.last_scroll_views != 1 {
            return;
        }
        let step = if view.row_h > 0.0 { view.row_h.max(32.0) } else { 48.0 };
        let page = (view.area.h * 0.85).max(step);
        if self.key_fires(KeyCode::Down) {
            s.nudge(step);
        } else if self.key_fires(KeyCode::Up) {
            s.nudge(-step);
        } else if self.key_fires(KeyCode::PageDown) {
            s.nudge(page);
        } else if self.key_fires(KeyCode::PageUp) {
            s.nudge(-page);
        } else if is_key_pressed(KeyCode::Home) {
            s.scroll_to(0.0);
        } else if is_key_pressed(KeyCode::End) {
            s.scroll_to(view.max);
        }
    }

    /// The scroll bar: a slim thumb as long as the part shown, wider and
    /// brighter under the pointer; dragged, or a press on the track sends
    /// the view there.
    fn scroll_bar(&mut self, view: &ScrollView, s: &mut Scroll) {
        let track = view.track;
        // Easier to take than to see.
        let hold = Rect::new(track.x - 6.0, track.y, track.w + 9.0, track.h);
        s.bar = hold;
        let content = view.content_h.max(1.0);
        let thumb_h = (track.h * view.area.h / content).clamp(22.0f32.min(track.h), track.h);
        let travel = (track.h - thumb_h).max(1.0);
        let shown = s.pos.clamp(0.0, view.max);
        let thumb_y = track.y + travel * shown / view.max.max(1.0);
        let bar_id = motion::mix(view.id, 0xba2);
        if self.pressed && hold.contains(self.mouse) {
            // Taken where it was pressed; from the track, by its middle.
            self.scroll_grab = if self.mouse.y >= thumb_y && self.mouse.y <= thumb_y + thumb_h {
                self.mouse.y - thumb_y
            } else {
                thumb_h / 2.0
            };
        }
        let drag = self.drag_area(bar_id, hold);
        if let Drag::Held(p) = drag {
            let f = ((p.y - self.scroll_grab - track.y) / travel).clamp(0.0, 1.0);
            s.target = f * view.max;
            s.rate = 30.0;
            s.grab = None;
            s.dragging = false;
        }
        let hot = matches!(drag, Drag::Held(_) | Drag::Hover);
        let lit = self.anim(motion::mix(bar_id, 1), hot as u8 as f32, motion::HOVER_RATE);
        if hot {
            motion::want_cursor(macroquad::miniquad::CursorIcon::Default);
        }
        // Grows to the left from its slim self.
        let w = track.w + 3.0 * lit;
        let x = track.x + track.w - w;
        rrect(
            Rect::new(x, track.y, w, track.h),
            w / 2.0,
            Color::new(1.0, 1.0, 1.0, 0.05 + 0.04 * lit),
        );
        rrect(
            Rect::new(x, thumb_y, w, thumb_h),
            w / 2.0,
            Color::new(GOLD.r, GOLD.g, GOLD.b, 0.55 + 0.4 * lit),
        );
    }
}

/// How far content dragged `over` past its end goes (it resists more and
/// more, never more than a third of the view).
fn rubber(over: f32, view_h: f32) -> f32 {
    let limit = (view_h / 3.0).max(1.0);
    limit * (1.0 - 1.0 / (over / limit * 0.55 + 1.0))
}

use super::Drag;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_in_view_include_the_ones_cut_by_the_edges() {
        let v = ScrollView {
            area: Rect::new(0.0, 100.0, 200.0, 100.0),
            offset: 30.0,
            content_h: 400.0,
            track: Rect::default(),
            fades: true,
            bar: true,
            row_h: 40.0,
            rows: 10,
            max: 300.0,
            id: 0,
            saved: None,
            clipped: true,
        };
        // Rows 0 (cut at the top), 1, 2 (cut at the bottom).
        assert_eq!(v.rows(), 0..4);
        assert_eq!(v.y(1), 110.0);
        assert!(v.visible(v.y(0), 40.0), "cut by the top edge, still drawn");
    }

    #[test]
    fn notches_add_up_and_stop_at_the_ends() {
        let mut s = Scroll { max: 500.0, view_h: 200.0, ..Scroll::default() };
        s.nudge(96.0);
        s.nudge(96.0);
        assert_eq!(s.target, 192.0);
        s.nudge(-1000.0);
        assert_eq!(s.target, 0.0);
        s.nudge(10_000.0);
        assert_eq!(s.target, 500.0);
    }

    #[test]
    fn easing_reaches_the_target_whatever_the_frame_rate() {
        for fps in [30.0, 60.0, 144.0] {
            let mut s = Scroll { max: 500.0, view_h: 200.0, ..Scroll::default() };
            s.nudge(300.0);
            let mut t = 0.0;
            while t < 0.6 {
                s.step(1.0 / fps);
                t += 1.0 / fps;
            }
            assert_eq!(s.pos, 300.0, "{fps} fps");
        }
    }

    #[test]
    fn reveal_scrolls_only_as_far_as_needed() {
        let mut s = Scroll { max: 1000.0, view_h: 200.0, ..Scroll::default() };
        s.reveal_row(2, 40.0);
        assert_eq!(s.target, 0.0, "already in view");
        s.reveal_row(9, 40.0);
        assert_eq!(s.target, 200.0, "its bottom at the view's bottom");
        s.reveal_row(1, 40.0);
        assert_eq!(s.target, 40.0, "its top at the view's top");
    }

    #[test]
    fn a_wheel_notch_scrolls_the_same_everywhere() {
        // One notch down (negative, as the platforms report it) scrolls the
        // content down by a notch; up, back.
        let notch = if cfg!(windows) {
            120.0
        } else if cfg!(target_os = "macos") {
            10.0
        } else {
            1.0
        };
        assert_eq!(wheel_units(-notch), NOTCH);
        assert_eq!(wheel_units(notch), -NOTCH);
        assert_eq!(wheel_units(0.0), 0.0);
        // A touchpad's small step scrolls a little, never a whole notch.
        let small = if cfg!(target_os = "macos") { 3.0 } else { notch / 4.0 };
        let units = wheel_units(-small);
        assert!(units > 0.0 && units < NOTCH, "{units}");
    }

    #[test]
    fn dragging_past_an_end_resists() {
        assert!(rubber(10.0, 300.0) < 10.0);
        assert!(rubber(1000.0, 300.0) < 100.0);
        assert!(rubber(50.0, 300.0) > rubber(20.0, 300.0));
    }
}
