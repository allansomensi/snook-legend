//! Game controllers (gamepads): the one in use is read once a frame
//! ([`poll`]) and asked about like the keyboard ([`down`], [`pressed`],
//! [`released`], [`axis`]), with what menus need on top: auto-repeat for
//! the d-pad ([`repeat`]) and the stick as arrow keys ([`stick_flick`]).
//!
//! The crates that do this need system libraries at build time (gilrs
//! needs libudev on Linux), which the builds don't have, so each system is
//! read directly, with nothing to install:
//!
//! - Windows: XInput, loaded when the game first looks for a controller.
//!   Xbox controllers and everything that acts like one (Steam Input and
//!   DS4Windows make PlayStation controllers do so).
//! - Linux: the kernel's joystick devices, `/dev/input/js0` to `js7`. The
//!   Flatpak needs access to the devices (`--device=all`) to see them.
//! - macOS: Apple's GameController framework (Xbox, PlayStation and Switch
//!   Pro controllers, and those made for Apple's devices).
//! - Anywhere else there is never a controller.
//!
//! One controller is followed at a time: the first one connected, or
//! whichever was picked up last (a button pressed, a stick moved), so a
//! second controller simply takes over when someone uses it.
//!
//! Buttons are named by position, not by what is printed on them (the
//! letters differ from one maker to the next, and Nintendo swaps them); ask
//! [`family`] which labels to draw. Everything is kept on the main thread
//! (the game reads input there), in a thread-local.

// Where there are no controllers, what the systems share goes unused.
#![cfg_attr(not(any(windows, target_os = "linux", target_os = "macos")), allow(dead_code))]

use std::cell::RefCell;
use std::time::Instant;

use macroquad::prelude::{
    MouseButton, Vec2, get_last_key_pressed, is_mouse_button_pressed, mouse_position, mouse_wheel,
    vec2,
};

mod tracker;

#[cfg(target_os = "macos")]
mod gc;
#[cfg(target_os = "linux")]
mod joydev;
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod none;
#[cfg(windows)]
mod xinput;

#[cfg(target_os = "macos")]
use gc as backend;
#[cfg(target_os = "linux")]
use joydev as backend;
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
use none as backend;
#[cfg(windows)]
use xinput as backend;

use tracker::{State, Tracker};

/// A button, by where it is on the controller (see [`family`] for what is
/// printed on it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    /// The bottom face button: A on Xbox, Cross on PlayStation, B on
    /// Nintendo.
    South,
    /// The right face button: B on Xbox, Circle on PlayStation, A on
    /// Nintendo.
    East,
    /// The left face button: X on Xbox, Square on PlayStation, Y on
    /// Nintendo.
    West,
    /// The top face button: Y on Xbox, Triangle on PlayStation, X on
    /// Nintendo.
    North,
    /// LB, L1, L.
    LeftShoulder,
    /// RB, R1, R.
    RightShoulder,
    /// The left trigger (LT, L2, ZL) as a button: down once pulled past
    /// about a third of the way. [`axis`] says how far.
    LeftTrigger,
    /// The right trigger (RT, R2, ZR) as a button.
    RightTrigger,
    /// View or Back on Xbox, Share or Create on PlayStation, − on Nintendo.
    Back,
    /// Menu or Start on Xbox, Options on PlayStation, + on Nintendo.
    Start,
    /// The button with the maker's logo (Xbox, PS, Home). The systems tend
    /// to keep it for themselves: XInput never reports it, and macOS only
    /// on recent versions.
    Guide,
    /// The left stick pressed in.
    LeftStick,
    /// The right stick pressed in.
    RightStick,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
}

impl Button {
    /// Every button, in the order of their bits.
    pub const ALL: [Button; 17] = [
        Button::South,
        Button::East,
        Button::West,
        Button::North,
        Button::LeftShoulder,
        Button::RightShoulder,
        Button::LeftTrigger,
        Button::RightTrigger,
        Button::Back,
        Button::Start,
        Button::Guide,
        Button::LeftStick,
        Button::RightStick,
        Button::DpadUp,
        Button::DpadDown,
        Button::DpadLeft,
        Button::DpadRight,
    ];

    fn bit(self) -> u32 {
        1 << self as u32
    }
}

/// An analog control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    /// The left stick, −1 (left) to 1 (right).
    LeftX,
    /// The left stick, −1 (down) to 1 (up).
    LeftY,
    RightX,
    RightY,
    /// The left trigger, 0 (released) to 1 (pulled all the way).
    LeftTrigger,
    RightTrigger,
}

/// Whose controller it is, for the labels and pictures of its buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    /// A, B, X, Y (also Steam's controllers, and any XInput controller).
    Xbox,
    /// Cross, Circle, Square, Triangle.
    PlayStation,
    /// A, B, X, Y in Nintendo's places (A on the right, B at the bottom).
    Nintendo,
    /// Unknown: neutral labels.
    Generic,
}

/// The family a controller's name points to (its maker, or the name of
/// the model). Shared by the systems that give names.
fn family_of(name: &str) -> Family {
    let name = name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| name.contains(w));
    if has(&["playstation", "dualshock", "dualsense", "sony", "ps3", "ps4", "ps5"])
        // A DualShock 4 over Bluetooth on Linux is just this.
        || name == "wireless controller"
    {
        Family::PlayStation
    } else if has(&["nintendo", "switch", "joy-con", "pro controller"]) {
        Family::Nintendo
    } else if has(&["xbox", "x-box", "xinput", "microsoft", "steam"]) {
        Family::Xbox
    } else {
        Family::Generic
    }
}

/// The mouse and keyboard, as far as [`active`] cares.
#[derive(Debug, Clone, Copy, Default)]
struct Others {
    /// Where the pointer is, in pixels.
    pointer: (f32, f32),
    /// A key, a mouse button or the wheel was used this frame.
    used: bool,
}

impl Others {
    fn read() -> Others {
        let (wheel_x, wheel_y) = mouse_wheel();
        let used = get_last_key_pressed().is_some()
            || [MouseButton::Left, MouseButton::Right, MouseButton::Middle]
                .into_iter()
                .any(is_mouse_button_pressed)
            || wheel_x != 0.0
            || wheel_y != 0.0;
        Others { pointer: mouse_position(), used }
    }
}

/// How far (in pixels, across and down added up) the pointer may wander
/// from where it was when the controller was picked up before the mouse
/// counts as being in use: a mouse knocked on the desk, or a pointer that
/// shakes by a pixel, shouldn't take the game away from the controller.
const POINTER_SLACK: f32 = 6.0;

struct Gamepad {
    backend: backend::Backend,
    tracker: Tracker,
    /// The connection the tracker follows (the backend's key for it).
    pad: Option<u64>,
    /// The controller is the input in use (see [`active`]).
    active: bool,
    /// Where the pointer was when the controller was last picked up.
    pointer: (f32, f32),
    /// When the rumble stops.
    rumble_until: Option<f64>,
    started: Instant,
}

impl Gamepad {
    fn new() -> Gamepad {
        Gamepad {
            backend: backend::Backend::new(),
            tracker: Tracker::new(backend::DEADZONES),
            pad: None,
            active: false,
            pointer: (0.0, 0.0),
            rumble_until: None,
            started: Instant::now(),
        }
    }

    fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// One frame: reads the controller (`now` in seconds since
    /// [`Gamepad::started`]) and weighs it against the mouse and keyboard.
    fn step(&mut self, now: f64, others: Others) {
        let reading = self.backend.poll(now);
        self.take(reading, now, others);
    }

    /// The frame's reading of the controller in use (with the key of its
    /// connection), or `None` without one.
    fn take(&mut self, reading: Option<(u64, State)>, now: f64, others: Others) {
        let pad = reading.map(|(key, _)| key);
        if pad != self.pad {
            self.tracker.forget();
            self.pad = pad;
            if self.rumble_until.take().is_some() {
                self.backend.rumble(0.0);
            }
        }
        self.tracker.update(reading.map_or_else(State::default, |(_, state)| state), now);

        if self.rumble_until.is_some_and(|until| now >= until) {
            self.rumble_until = None;
            self.backend.rumble(0.0);
        }

        if self.active {
            let (x, y) = others.pointer;
            let moved = (x - self.pointer.0).abs() + (y - self.pointer.1).abs() > POINTER_SLACK;
            if others.used || moved {
                self.active = false;
            }
        }
        // The controller wins a frame where both were used: it was surely
        // picked up on purpose.
        if self.tracker.woke() {
            self.active = true;
            self.pointer = others.pointer;
        }
        if self.pad.is_none() {
            self.active = false;
        }
    }
}

thread_local! {
    static PAD: RefCell<Gamepad> = RefCell::new(Gamepad::new());
}

fn with<T>(f: impl FnOnce(&Gamepad) -> T) -> T {
    PAD.with_borrow(f)
}

/// Reads the controller: once a frame, at the start of the frame, before
/// anything asks about it. With no controller connected it costs next to
/// nothing (the systems are only searched for new controllers every couple
/// of seconds).
///
/// It also watches macroquad's mouse and keyboard for [`active`]: the
/// pointer moving, a key or mouse button going down, or the wheel turning
/// this frame.
pub fn poll() {
    let others = Others::read();
    PAD.with_borrow_mut(|pad| {
        let now = pad.now();
        pad.step(now, others);
    });
}

/// A controller is connected.
pub fn connected() -> bool {
    with(|pad| pad.pad.is_some())
}

/// The name of the controller in use, as its system gives it (for the
/// settings): "Xbox Wireless Controller", "DualSense Wireless Controller"…
/// Windows gives no names: there it is "XInput controller" and its number.
pub fn name() -> Option<String> {
    with(|pad| pad.pad.and_then(|_| pad.backend.name()))
}

/// Whose controller is in use, for the labels of its buttons ([`Generic`]
/// with none connected).
///
/// [`Generic`]: Family::Generic
pub fn family() -> Family {
    with(|pad| if pad.pad.is_some() { pad.backend.family() } else { Family::Generic })
}

/// The button is held.
pub fn down(b: Button) -> bool {
    with(|pad| pad.tracker.down(b))
}

/// The button went down this frame.
pub fn pressed(b: Button) -> bool {
    with(|pad| pad.tracker.pressed(b))
}

/// The button went up this frame. A controller that goes away (or a
/// battery that dies) while a button is held reports no release, so a
/// release can be trusted to be the player's.
pub fn released(b: Button) -> bool {
    with(|pad| pad.tracker.released(b))
}

/// The button went down this frame, or has been held long enough to repeat
/// (first after 0.4 s, then every 0.09 s), like a key held on a keyboard:
/// for moving through menus with the d-pad.
pub fn repeat(b: Button) -> bool {
    with(|pad| pad.tracker.repeat(b))
}

/// An axis with its deadzone: the sticks −1..1 (Y up-positive), nothing
/// until a stick is a fifth of the way out (round the centre, so diagonals
/// stay diagonal) and then rising smoothly from 0; the triggers 0..1,
/// nothing for the first 5% of their travel.
pub fn axis(a: Axis) -> f32 {
    with(|pad| pad.tracker.axis(a))
}

/// A stick as a vector, with its deadzone (as [`axis`]), Y up-positive.
/// Its length is at most 1.
pub fn stick(left: bool) -> Vec2 {
    let (x, y) = with(|pad| pad.tracker.stick(usize::from(!left)));
    vec2(x, y)
}

/// A step through a menu from the stick, in screen terms (x right, y
/// down): the stick pushed most of the way towards one side gives one
/// step that way, and it repeats while held there, like [`repeat`]. `None`
/// on every other frame.
pub fn stick_flick(left: bool) -> Option<(i32, i32)> {
    with(|pad| pad.tracker.flick(usize::from(!left)))
}

/// The controller is the input in use, so the game can show its button
/// labels and move a focus through menus instead of following the
/// pointer: true from the moment a button is pressed (or a stick or
/// trigger leaves its deadzone), false again once the mouse moves (a few
/// pixels), a mouse button, the wheel or a key is used, or the controller
/// goes away. [`poll`] watches macroquad's mouse and keyboard itself.
pub fn active() -> bool {
    with(|pad| pad.active)
}

/// Shakes the controller in use, `strength` 0..1 for `seconds` (0 stops
/// it). Only Windows (XInput) has rumble; elsewhere this does nothing.
pub fn rumble(strength: f32, seconds: f32) {
    PAD.with_borrow_mut(|pad| {
        let strength = if strength.is_nan() { 0.0 } else { strength.clamp(0.0, 1.0) };
        if pad.pad.is_none() || strength == 0.0 || seconds.is_nan() || seconds <= 0.0 {
            if pad.rumble_until.take().is_some() {
                pad.backend.rumble(0.0);
            }
            return;
        }
        pad.backend.rumble(strength);
        pad.rumble_until = Some(pad.now() + f64::from(seconds));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gamepad_families_from_names() {
        for (name, family) in [
            ("Microsoft X-Box 360 pad", Family::Xbox),
            ("Xbox Wireless Controller", Family::Xbox),
            ("Generic X-Box pad", Family::Xbox),
            ("XInput controller 1", Family::Xbox),
            ("Steam Deck", Family::Xbox),
            ("Sony Interactive Entertainment Wireless Controller", Family::PlayStation),
            ("Wireless Controller", Family::PlayStation),
            ("DualSense Wireless Controller", Family::PlayStation),
            ("DUALSHOCK 4 Wireless Controller", Family::PlayStation),
            ("Sony PLAYSTATION(R)3 Controller", Family::PlayStation),
            ("Nintendo Switch Pro Controller", Family::Nintendo),
            ("Pro Controller", Family::Nintendo),
            ("Joy-Con (L/R)", Family::Nintendo),
            ("USB Gamepad", Family::Generic),
            ("", Family::Generic),
        ] {
            assert_eq!(family_of(name), family, "{name}");
        }
    }

    #[test]
    fn gamepad_bits_follow_the_order() {
        for (i, b) in Button::ALL.into_iter().enumerate() {
            assert_eq!(b.bit(), 1 << i);
        }
    }

    /// The whole frame without macroquad: this system's real backend (on
    /// Linux it searches `/dev/input`, which has no controller here), the
    /// tracker and the mouse rule. Not on macOS: the tests run off the main
    /// thread, which the GameController framework isn't meant for.
    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn gamepad_frames_without_a_controller() {
        let mut pad = Gamepad::new();
        for frame in 0..300 {
            // Five simulated seconds, so the search for controllers runs
            // more than once.
            let now = f64::from(frame) / 60.0;
            pad.step(now, Others { pointer: (100.0, 100.0), used: frame % 7 == 0 });
            if pad.pad.is_some() {
                // A real controller on the machine running the tests.
                return;
            }
            assert!(!pad.active && !pad.tracker.any_pressed());
            assert_eq!(pad.backend.poll(now), None);
        }
        assert_eq!(pad.tracker.stick(0), (0.0, 0.0));
        pad.backend.rumble(0.5);
        pad.backend.rumble(0.0);
    }

    #[test]
    fn gamepad_active_until_the_mouse_or_keyboard_is_used() {
        let mut pad = Gamepad::new();
        let rest = State::default();
        let mut south = State::default();
        south.set(Button::South, true);
        let mut pushed = State::default();
        pushed.set_axis(Axis::LeftX, 0.9);
        let at = |x: f32, y: f32| Others { pointer: (x, y), used: false };
        let used = Others { pointer: (10.0, 10.0), used: true };

        // Connected is not in use.
        pad.take(Some((1, rest)), 0.0, at(10.0, 10.0));
        assert!(!pad.active);
        pad.take(Some((1, south)), 0.1, at(10.0, 10.0));
        assert!(pad.active);
        // A shaking pointer leaves it in use...
        pad.take(Some((1, south)), 0.2, at(12.0, 11.0));
        pad.take(Some((1, rest)), 0.3, at(9.0, 12.0));
        assert!(pad.active);
        // ...the mouse moved doesn't.
        pad.take(Some((1, rest)), 0.4, at(20.0, 10.0));
        assert!(!pad.active);
        // A stick pushed is the controller again; a key, the keyboard.
        pad.take(Some((1, pushed)), 0.5, at(20.0, 10.0));
        assert!(pad.active);
        pad.take(Some((1, pushed)), 0.6, used);
        assert!(!pad.active);
        // Still pushed: no claim on the game (it may be drift).
        pad.take(Some((1, pushed)), 0.7, at(20.0, 10.0));
        assert!(!pad.active);
        // Both in the same frame: the controller.
        pad.take(Some((1, south)), 0.8, used);
        assert!(pad.active);

        // The controller goes away mid-press: no release, not in use.
        pad.take(None, 0.9, used);
        assert!(!pad.active && !pad.tracker.down(Button::South));
        assert!(!pad.tracker.released(Button::South));
        // Back (a new connection): what is held is a press.
        pad.take(Some((2, south)), 1.0, at(10.0, 10.0));
        assert!(pad.active && pad.tracker.pressed(Button::South));
    }
}

/// What is printed on button `b` of the controller in use (its maker's
/// names: "A", "Cross", "LB", "L1"…), for the list of controls.
pub fn label(b: Button) -> &'static str {
    label_for(family(), b)
}

fn label_for(f: Family, b: Button) -> &'static str {
    use Button::*;
    match (f, b) {
        (Family::PlayStation, South) => "✕",
        (Family::PlayStation, East) => "○",
        (Family::PlayStation, West) => "□",
        (Family::PlayStation, North) => "△",
        (Family::PlayStation, LeftShoulder) => "L1",
        (Family::PlayStation, RightShoulder) => "R1",
        (Family::PlayStation, LeftTrigger) => "L2",
        (Family::PlayStation, RightTrigger) => "R2",
        (Family::PlayStation, Back) => "Create",
        (Family::PlayStation, Start) => "Options",
        (Family::PlayStation, LeftStick) => "L3",
        (Family::PlayStation, RightStick) => "R3",
        (Family::Nintendo, South) => "B",
        (Family::Nintendo, East) => "A",
        (Family::Nintendo, West) => "Y",
        (Family::Nintendo, North) => "X",
        (Family::Nintendo, LeftShoulder) => "L",
        (Family::Nintendo, RightShoulder) => "R",
        (Family::Nintendo, LeftTrigger) => "ZL",
        (Family::Nintendo, RightTrigger) => "ZR",
        (Family::Nintendo, Back) => "−",
        (Family::Nintendo, Start) => "+",
        (_, South) => "A",
        (_, East) => "B",
        (_, West) => "X",
        (_, North) => "Y",
        (_, LeftShoulder) => "LB",
        (_, RightShoulder) => "RB",
        (_, LeftTrigger) => "LT",
        (_, RightTrigger) => "RT",
        (_, Back) => "View",
        (_, Start) => "Menu",
        (_, Guide) => "Home",
        (_, LeftStick) => "LS",
        (_, RightStick) => "RS",
        (_, DpadUp) => "↑",
        (_, DpadDown) => "↓",
        (_, DpadLeft) => "←",
        (_, DpadRight) => "→",
    }
}

#[cfg(test)]
mod label_tests {
    use super::*;

    #[test]
    fn buttons_are_named_as_their_maker_prints_them() {
        assert_eq!(label_for(Family::Xbox, Button::South), "A");
        assert_eq!(label_for(Family::Nintendo, Button::South), "B", "Nintendo's A is on the right");
        assert_eq!(label_for(Family::PlayStation, Button::RightShoulder), "R1");
        assert_eq!(label_for(Family::Generic, Button::LeftTrigger), "LT");
    }
}
