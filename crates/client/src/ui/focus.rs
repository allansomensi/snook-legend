//! The keyboard's focus: Tab and Shift+Tab go through what can be pressed
//! on the screen (buttons, tabs, switches, rows, text fields) in the order
//! it is drawn, Enter or Space presses the one focused, the arrows go
//! along a row of tabs or a segmented control (choosing as they go). A ring
//! shows where the focus is, only while the keyboard is used: the pointer
//! hides it again.
//!
//! Immediate mode keeps no widget, so what can take the focus is noted
//! while it is drawn (through [`Ui::released_in`] and [`Ui::clicked`], which
//! every widget uses), keyed by where it is (in a scrolled list: where it
//! is in the list), and the keys of the frame decide on the last frame's
//! list. A dialog starts a new list (nothing under it takes the focus);
//! what is drawn with the input suspended (a page under a dialog, a faded
//! HUD) takes none. At the table during a match it is off: Tab, Space and
//! the arrows are the game's there.

use std::cell::{Cell, RefCell};

use macroquad::prelude::*;

use super::motion;
use super::{GOLD, Ui, key_pressed};

/// What can take the focus, drawn this frame.
#[derive(Debug, Clone, Copy)]
pub(super) struct Item {
    pub key: u64,
    /// Where it is on screen (interface units), and where it is drawn.
    pub screen: Rect,
    pub local: Rect,
    /// The row of tabs (or the segmented control) it is in, and whether
    /// that row goes down; whether it is the one chosen there.
    pub group: Option<(u64, bool)>,
    pub selected: bool,
    /// A text field (the key is its id).
    pub field: bool,
}

/// The keyboard's focus (kept by [`Ui`]).
#[derive(Default)]
pub(super) struct Nav {
    pub items: RefCell<Vec<Item>>,
    pub prev: Vec<Item>,
    /// The focused one (a key of the list), and whether its ring shows.
    pub current: Option<u64>,
    pub visible: bool,
    /// Pressed with Enter or Space (or chosen with an arrow) this frame.
    pub hit: Option<u64>,
    /// The focus moved this frame (a list scrolls to it).
    pub moved: bool,
    /// Enter and Space, or the arrows, went to the focus this frame.
    pub keys_taken: bool,
    pub arrows_taken: bool,
    /// Off at the table during a match.
    pub off: bool,
    /// The row of tabs being drawn, and whether the widget is the one
    /// chosen in it.
    pub group: Cell<Option<(u64, bool)>>,
    pub selected: Cell<bool>,
    /// How far down the scrolled lists being drawn are (their rows keep
    /// their keys while they scroll).
    pub shift: Cell<f32>,
    /// The input is suspended (nothing drawn now takes the focus).
    pub inert: Cell<u32>,
    /// Where the pointer was last frame (moving it hides the ring).
    pub last_mouse: Vec2,
}

impl Ui {
    /// The keyboard's focus works from now on (not at the table during a
    /// match).
    pub fn keyboard_nav(&mut self, on: bool) {
        self.kb.off = !on;
    }

    /// A new list of what takes the focus: what was drawn before (a page
    /// under a dialog) takes none.
    pub(super) fn focus_layer(&self) {
        self.kb.items.borrow_mut().clear();
    }

    /// The start of a frame: the keys move the focus, press what it is on.
    pub(super) fn focus_begin(&mut self) {
        let nav = &mut self.kb;
        nav.prev = std::mem::take(nav.items.get_mut());
        nav.hit = None;
        nav.moved = false;
        nav.keys_taken = false;
        nav.arrows_taken = false;
        nav.group.set(None);
        nav.selected.set(false);
        nav.shift.set(0.0);
        nav.inert.set(0);
        // The pointer used: the ring goes (the focus stays, for the next Tab).
        let moved = (self.mouse - nav.last_mouse).length() > 3.0 && self.mouse.x > -1.0e3;
        nav.last_mouse = self.mouse;
        if self.pressed || moved {
            nav.visible = false;
        }
        // Off; or a dialog holds the keyboard; or a field's own Tab moves on.
        let field = self.focus.filter(|&f| f < super::MODAL_FOCUS);
        let menus = !nav.off && !self.focus.is_some_and(|f| f >= super::MODAL_FOCUS);
        PAD_MENUS.with(|m| m.set(menus));
        if !menus {
            nav.visible = false;
            return;
        }
        if nav.prev.is_empty() {
            return;
        }
        // A game controller: the d-pad or the stick moves the focus to the
        // nearest thing that direction, A presses it.
        if self.pad_nav(field) {
            return;
        }
        let nav = &mut self.kb;
        // Tab and Shift+Tab (in a field: its own Tab, see `tab_move`).
        let shift = is_key_down(KeyCode::LeftShift)
            || is_key_down(KeyCode::RightShift)
            || motion::scripted_shift();
        let step = match self.tab_move.take() {
            Some(step) => Some(step),
            None if field.is_none() && key_pressed(KeyCode::Tab) => {
                Some(if shift { -1 } else { 1 })
            }
            None => None,
        };
        if let Some(step) = step {
            let from = field.or(nav.current.filter(|_| nav.visible || field.is_some()));
            let from = from.or(nav.current);
            if let Some(next) = next_stop(&nav.prev, from, step) {
                let item = nav.prev[next];
                nav.current = Some(item.key);
                nav.visible = true;
                nav.moved = true;
                if item.field {
                    self.focus = Some(item.key);
                    self.cursor_owner = None;
                } else if field.is_some() {
                    self.focus = None;
                    self.anchor = None;
                }
            }
            return;
        }
        // A field has the keyboard: its keys are its own.
        if field.is_some() || !nav.visible {
            return;
        }
        let Some(cur) = nav.current else { return };
        let Some(i) = nav.prev.iter().position(|it| it.key == cur) else { return };
        let item = nav.prev[i];
        if key_pressed(KeyCode::Enter)
            || key_pressed(KeyCode::KpEnter)
            || key_pressed(KeyCode::Space)
        {
            nav.hit = Some(cur);
            nav.keys_taken = true;
            return;
        }
        // The arrows along a row of tabs: the next one is chosen.
        if let Some((g, down)) = item.group {
            let (back, on) =
                if down { (KeyCode::Up, KeyCode::Down) } else { (KeyCode::Left, KeyCode::Right) };
            let dir = if key_pressed(back) {
                -1
            } else if key_pressed(on) {
                1
            } else {
                0
            };
            if dir != 0 {
                nav.arrows_taken = true;
                let mates: Vec<usize> = (0..nav.prev.len())
                    .filter(|&k| nav.prev[k].group.is_some_and(|(x, _)| x == g))
                    .collect();
                if let Some(at) = mates.iter().position(|&k| k == i) {
                    let to = at as i32 + dir;
                    if to >= 0 && (to as usize) < mates.len() {
                        let key = nav.prev[mates[to as usize]].key;
                        nav.current = Some(key);
                        nav.hit = Some(key);
                        nav.moved = true;
                    }
                }
            }
        }
    }

    /// The focus moved by a game controller (see [`crate::gamepad`]): the
    /// d-pad or the left stick go to the nearest thing that way (along a
    /// row of tabs, the next tab is chosen), A presses what has the focus.
    /// The first touch only shows where the focus is. True when the
    /// controller did something this frame.
    fn pad_nav(&mut self, field: Option<u64>) -> bool {
        use crate::gamepad::{self, Button};
        if !gamepad::connected() {
            return false;
        }
        let dir = [
            (Button::DpadLeft, (-1, 0)),
            (Button::DpadRight, (1, 0)),
            (Button::DpadUp, (0, -1)),
            (Button::DpadDown, (0, 1)),
        ]
        .into_iter()
        .find(|(b, _)| gamepad::repeat(*b))
        .map(|(_, d)| d)
        .or_else(|| gamepad::stick_flick(true));
        let press = gamepad::pressed(Button::South);
        if dir.is_none() && !press {
            return false;
        }
        let nav = &mut self.kb;
        let at = nav.current.or(field).and_then(|k| nav.prev.iter().position(|it| it.key == k));
        // The first touch: where the focus is (or the first thing there).
        if !nav.visible || at.is_none() {
            let i = at.unwrap_or_else(|| nav.prev.iter().position(|it| it.selected).unwrap_or(0));
            nav.current = Some(nav.prev[i].key);
            nav.visible = true;
            nav.moved = true;
            return true;
        }
        let i = at.unwrap_or(0);
        let item = nav.prev[i];
        if press {
            if item.field {
                self.focus = Some(item.key);
                self.cursor_owner = None;
            } else {
                nav.hit = Some(item.key);
                nav.keys_taken = true;
            }
            return true;
        }
        let Some((dx, dy)) = dir else { return false };
        // Along a row of tabs, its way: the next tab, chosen.
        if let Some((g, down)) = item.group {
            let step = if down { dy } else { dx };
            let across = if down { dx } else { dy };
            if step != 0 && across == 0 {
                let mates: Vec<usize> = (0..nav.prev.len())
                    .filter(|&k| nav.prev[k].group.is_some_and(|(x, _)| x == g))
                    .collect();
                if let Some(at) = mates.iter().position(|&k| k == i) {
                    let to = at as i32 + step;
                    if to >= 0 && (to as usize) < mates.len() {
                        let key = nav.prev[mates[to as usize]].key;
                        nav.current = Some(key);
                        nav.hit = Some(key);
                        nav.moved = true;
                        nav.arrows_taken = true;
                        return true;
                    }
                }
            }
        }
        if let Some(next) = nearest(&nav.prev, i, (dx, dy)) {
            let it = nav.prev[next];
            nav.current = Some(it.key);
            nav.moved = true;
            if it.field {
                self.focus = Some(it.key);
                self.cursor_owner = None;
            } else if field.is_some() {
                self.focus = None;
                self.anchor = None;
            }
        }
        true
    }

    /// Notes that `r` (where it is drawn now) can take the focus; true when
    /// the keyboard pressed it this frame. `field`: a text field's id.
    pub(super) fn focus_item(&self, r: Rect, field: Option<u64>) -> bool {
        let nav = &self.kb;
        if nav.off || nav.inert.get() > 0 {
            return false;
        }
        let key = field.unwrap_or_else(|| {
            let at = Rect::new(r.x, r.y + nav.shift.get(), r.w, r.h);
            motion::rect_key(motion::id("focus"), at)
        });
        let mut items = nav.items.borrow_mut();
        if !items.iter().rev().take(4).any(|it| it.key == key) {
            items.push(Item {
                key,
                screen: motion::xf().rect(r),
                local: r,
                group: nav.group.get(),
                selected: nav.selected.get(),
                field: field.is_some(),
            });
        }
        nav.hit == Some(key)
    }

    /// `r` has the keyboard's focus, and its ring shows.
    pub fn focused(&self, r: Rect) -> bool {
        let nav = &self.kb;
        nav.visible
            && nav.current.is_some_and(|k| {
                let at = Rect::new(r.x, r.y + nav.shift.get(), r.w, r.h);
                k == motion::rect_key(motion::id("focus"), at)
            })
    }

    /// Runs `f` (drawing a tab of a row, or a cell of a segmented control:
    /// `group` names the row, `down` when it goes down) with what it notes
    /// taking the focus as part of that row: the arrows go along it.
    pub fn in_group<R>(
        &mut self,
        group: u64,
        down: bool,
        selected: bool,
        f: impl FnOnce(&mut Ui) -> R,
    ) -> R {
        let before =
            (self.kb.group.replace(Some((group, down))), self.kb.selected.replace(selected));
        let r = f(self);
        self.kb.group.set(before.0);
        self.kb.selected.set(before.1);
        r
    }

    /// Enter (or the keypad's) was pressed this frame and is the screen's:
    /// not when it pressed the button the keyboard's focus is on.
    pub fn enter(&self) -> bool {
        (key_pressed(KeyCode::Enter) || key_pressed(KeyCode::KpEnter)) && !self.kb.keys_taken
    }

    /// Space (or Enter) pressed the button the keyboard's focus is on this
    /// frame: a screen's own use of them waits.
    pub fn keys_taken(&self) -> bool {
        self.kb.keys_taken
    }

    /// The arrows went along a row of tabs this frame (a list under them
    /// does not scroll).
    pub(super) fn arrows_taken(&self) -> bool {
        self.kb.arrows_taken
    }

    /// A scrolled list begins (`offset`: how far down it is): its rows keep
    /// their keys as they scroll. Returns where its rows start in the list
    /// of what takes the focus.
    pub(super) fn focus_scroll_begin(&self, offset: f32) -> usize {
        self.kb.shift.set(self.kb.shift.get() + offset);
        self.kb.items.borrow().len()
    }

    /// A scrolled list ends: if the focus just moved to one of its rows,
    /// where that row is in the list (top, bottom), to scroll there.
    pub(super) fn focus_scroll_end(
        &self,
        offset: f32,
        from: usize,
        top: f32,
    ) -> Option<(f32, f32)> {
        let nav = &self.kb;
        nav.shift.set(nav.shift.get() - offset);
        if !nav.moved || !nav.visible {
            return None;
        }
        let cur = nav.current?;
        let items = nav.items.borrow();
        let item = items.get(from..)?.iter().find(|it| it.key == cur)?;
        Some((item.local.y - top, item.local.y + item.local.h - top))
    }

    /// The ring round what has the focus (at the end of the frame, over
    /// everything; it glides from one to the next).
    pub(super) fn focus_ring(&self) {
        let nav = &self.kb;
        let found = nav
            .current
            .filter(|_| nav.visible)
            .and_then(|k| nav.items.borrow().iter().find(|it| it.key == k && !it.field).copied());
        let a = self.anim(motion::id("focus-ring-alpha"), found.is_some() as u8 as f32, 24.0);
        let Some(item) = found else {
            // (Read and let go before the glide below writes it: a borrow in
            // the `if let` lasts the whole block, and the game came down on
            // Esc while the ring was showing.)
            let last = if a > 0.01 {
                self.anims.borrow().get(&motion::id("focus-ring")).map(|s| s.v)
            } else {
                None
            };
            if let Some(last) = last {
                let r = Rect::new(last[0], last[1], last[2], last[3]);
                ring(r, a);
                // (Kept while it fades.)
                self.glide_rect(motion::id("focus-ring"), r, 30.0);
            }
            return;
        };
        let s = item.screen;
        let target = Rect::new(s.x - 3.0, s.y - 3.0, s.w + 6.0, s.h + 6.0);
        let r = self.glide_rect(motion::id("focus-ring"), target, 30.0);
        ring(r, a);
    }
}

fn ring(r: Rect, a: f32) {
    let radius = (r.h * 0.3).clamp(4.0, 12.0);
    super::rrect_lines(r, radius, 2.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.95 * a));
    let outer = Rect::new(r.x - 2.0, r.y - 2.0, r.w + 4.0, r.h + 4.0);
    super::rrect_lines(outer, radius + 2.0, 1.5, Color::new(0.0, 0.0, 0.0, 0.35 * a));
}

thread_local! {
    /// The keyboard's focus goes through the screen this frame (menus, not
    /// the table): a controller's B is Esc there.
    static PAD_MENUS: Cell<bool> = const { Cell::new(false) };
}

/// The screen's menus have the controller's buttons (not the table's game).
pub(super) fn pad_menus() -> bool {
    PAD_MENUS.with(Cell::get)
}

/// The nearest item to `from` in direction `dir` (y down), on screen: what
/// lies that way, the closest first, the ones in line with it before those
/// off to the side.
fn nearest(items: &[Item], from: usize, dir: (i32, i32)) -> Option<usize> {
    let centre = |r: Rect| vec2(r.x + r.w / 2.0, r.y + r.h / 2.0);
    let a = items[from].screen;
    let c0 = centre(a);
    let d = vec2(dir.0 as f32, dir.1 as f32);
    let own_group = items[from].group.map(|g| g.0);
    let mut best: Option<(f32, usize)> = None;
    for (k, it) in items.iter().enumerate() {
        if k == from || it.key == items[from].key {
            continue;
        }
        // A row of tabs is reached at its chosen tab.
        if let Some((g, _)) = it.group
            && Some(g) != own_group
            && !it.selected
            && items.iter().any(|x| x.selected && x.group.is_some_and(|(y, _)| y == g))
        {
            continue;
        }
        let r = it.screen;
        // How far ahead, from edge to edge (overlapping: by the centres).
        let ahead = match dir {
            (1, 0) => r.x - (a.x + a.w),
            (-1, 0) => a.x - (r.x + r.w),
            (0, 1) => r.y - (a.y + a.h),
            _ => a.y - (r.y + r.h),
        };
        let delta = centre(r) - c0;
        let forward = delta.dot(d);
        if forward <= 2.0 {
            continue;
        }
        let ahead = ahead.max(0.0).max(forward * 0.25);
        let side = (delta - d * forward).length();
        // Overlapping across the way it goes: in line.
        let in_line = if dir.0 != 0 {
            r.y < a.y + a.h && a.y < r.y + r.h
        } else {
            r.x < a.x + a.w && a.x < r.x + r.w
        };
        let score = ahead + side * if in_line { 0.3 } else { 2.0 };
        if best.is_none_or(|(b, _)| score < b) {
            best = Some((score, k));
        }
    }
    best.map(|(_, k)| k)
}

/// The next place for Tab (`step` 1) or Shift+Tab (-1) from `from`: a row
/// of tabs is one stop (its chosen tab), however many tabs it has.
fn next_stop(items: &[Item], from: Option<u64>, step: i32) -> Option<usize> {
    let n = items.len() as i32;
    if n == 0 {
        return None;
    }
    let at = from.and_then(|k| items.iter().position(|it| it.key == k));
    let own_group = at.and_then(|i| items[i].group.map(|g| g.0));
    let start = match at {
        Some(i) => i as i32,
        None if step > 0 => -1,
        None => n,
    };
    for k in 1..=n {
        let i = (start + step * k).rem_euclid(n) as usize;
        let it = &items[i];
        match it.group {
            // The rest of the row it is in.
            Some((g, _)) if Some(g) == own_group => continue,
            Some((g, _)) => {
                // Into a row: its chosen tab (or the first met).
                let chosen =
                    items.iter().position(|x| x.selected && x.group.is_some_and(|(y, _)| y == g));
                return Some(chosen.unwrap_or(i));
            }
            None => return Some(i),
        }
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(key: u64, group: Option<u64>, selected: bool) -> Item {
        Item {
            key,
            screen: Rect::default(),
            local: Rect::default(),
            group: group.map(|g| (g, false)),
            selected,
            field: false,
        }
    }

    fn at(key: u64, x: f32, y: f32) -> Item {
        let r = Rect::new(x, y, 100.0, 30.0);
        Item { key, screen: r, local: r, group: None, selected: false, field: false }
    }

    #[test]
    fn a_controller_goes_to_the_nearest_thing_that_way() {
        // Two columns of buttons, and one far off to the right and down.
        let items = [
            at(1, 0.0, 0.0),
            at(2, 0.0, 50.0),
            at(3, 200.0, 0.0),
            at(4, 200.0, 50.0),
            at(5, 600.0, 400.0),
        ];
        assert_eq!(nearest(&items, 0, (0, 1)), Some(1), "down: the one under it");
        assert_eq!(nearest(&items, 0, (1, 0)), Some(2), "right: the one in line");
        assert_eq!(nearest(&items, 1, (1, 0)), Some(3));
        assert_eq!(nearest(&items, 3, (0, 1)), Some(4), "down, off to the side if nothing else");
        assert_eq!(nearest(&items, 0, (-1, 0)), None, "nothing that way");
        assert_eq!(nearest(&items, 0, (0, -1)), None);
    }

    #[test]
    fn tab_goes_round_and_back() {
        let items = [item(1, None, false), item(2, None, false), item(3, None, false)];
        assert_eq!(next_stop(&items, None, 1), Some(0));
        assert_eq!(next_stop(&items, None, -1), Some(2));
        assert_eq!(next_stop(&items, Some(1), 1), Some(1));
        assert_eq!(next_stop(&items, Some(3), 1), Some(0), "round to the first");
        assert_eq!(next_stop(&items, Some(1), -1), Some(2), "back to the last");
    }

    #[test]
    fn a_row_of_tabs_is_one_stop_at_its_chosen_tab() {
        let items = [
            item(1, None, false),
            item(10, Some(7), false),
            item(11, Some(7), true),
            item(12, Some(7), false),
            item(2, None, false),
        ];
        assert_eq!(next_stop(&items, Some(1), 1), Some(2), "into the row: the chosen tab");
        assert_eq!(next_stop(&items, Some(11), 1), Some(4), "out of the row at once");
        assert_eq!(next_stop(&items, Some(2), -1), Some(2), "back into it: the chosen tab");
        assert_eq!(next_stop(&items, Some(10), -1), Some(0));
    }
}
