//! Controllers on macOS, through Apple's GameController framework (Xbox,
//! PlayStation and Switch Pro controllers, and those made for Apple's
//! devices), talked to through the Objective-C runtime directly.
//!
//! Every message is sent with `objc_msgSend` cast to the exact signature of
//! the method (on Apple silicon a variadic call would pass the arguments in
//! the wrong place). What's newer than macOS 10.14 (the menu, options and
//! home buttons, the stick buttons, the product category) is only asked for
//! when the object says it answers it. Each frame's messages run inside an
//! autorelease pool, so what the framework hands back is freed every frame.
//!
//! The framework only follows controllers while the game's window is in
//! front, and lays out buttons by position as this module does (`buttonA`
//! is the bottom one, a Switch controller's B).

use std::ffi::{CStr, c_char, c_void};

use super::tracker::{Choice, Deadzones, State};
use super::{Axis, Button, Family, family_of};

pub const DEADZONES: Deadzones = Deadzones::STANDARD;

/// More than this many controllers at once are not looked at.
const MAX_CONTROLLERS: usize = 8;

type Id = *mut c_void;
type Sel = *const c_void;

#[link(name = "GameController", kind = "framework")]
unsafe extern "C" {
    /// Only referred to so the framework is surely linked: its classes are
    /// looked up by name.
    static GCControllerDidConnectNotification: Id;
}

#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> Id;
    fn sel_registerName(name: *const c_char) -> Sel;
    fn objc_msgSend();
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

// `objc_msgSend` as each kind of method it sends to.
type SendId = unsafe extern "C" fn(Id, Sel) -> Id;
type SendIdAt = unsafe extern "C" fn(Id, Sel, usize) -> Id;
type SendUsize = unsafe extern "C" fn(Id, Sel) -> usize;
type SendF32 = unsafe extern "C" fn(Id, Sel) -> f32;
/// `BOOL` is a `signed char` on Intel and a `bool` on Apple silicon: one
/// byte either way, nonzero for YES.
type SendBool = unsafe extern "C" fn(Id, Sel) -> i8;
type SendSelBool = unsafe extern "C" fn(Id, Sel, Sel) -> i8;
type SendStr = unsafe extern "C" fn(Id, Sel) -> *const c_char;

/// The methods used, registered once.
#[derive(Clone, Copy)]
struct Selectors {
    controllers: Sel,
    count: Sel,
    object_at_index: Sel,
    responds_to_selector: Sel,
    utf8_string: Sel,
    extended_gamepad: Sel,
    vendor_name: Sel,
    product_category: Sel,
    button_a: Sel,
    button_b: Sel,
    button_x: Sel,
    button_y: Sel,
    left_shoulder: Sel,
    right_shoulder: Sel,
    left_trigger: Sel,
    right_trigger: Sel,
    button_menu: Sel,
    button_options: Sel,
    button_home: Sel,
    left_thumbstick_button: Sel,
    right_thumbstick_button: Sel,
    dpad: Sel,
    up: Sel,
    down: Sel,
    left: Sel,
    right: Sel,
    left_thumbstick: Sel,
    right_thumbstick: Sel,
    x_axis: Sel,
    y_axis: Sel,
    value: Sel,
    is_pressed: Sel,
}

/// The Objective-C runtime, as far as the controllers go.
///
/// Its methods send messages only to nil (which they skip) or to objects
/// the framework handed back for the methods' own receivers, with the
/// method's real signature: what makes each `objc_msgSend` below sound.
#[derive(Clone, Copy)]
struct Runtime {
    /// `GCController`.
    class: Id,
    sel: Selectors,
}

impl Runtime {
    fn load() -> Option<Runtime> {
        std::hint::black_box(&raw const GCControllerDidConnectNotification);
        // SAFETY: a NUL-terminated name; nil if the class isn't there.
        let class = unsafe { objc_getClass(c"GCController".as_ptr()) };
        if class.is_null() {
            return None;
        }
        // SAFETY: NUL-terminated names; registering is idempotent.
        let s = |name: &CStr| unsafe { sel_registerName(name.as_ptr()) };
        let sel = Selectors {
            controllers: s(c"controllers"),
            count: s(c"count"),
            object_at_index: s(c"objectAtIndex:"),
            responds_to_selector: s(c"respondsToSelector:"),
            utf8_string: s(c"UTF8String"),
            extended_gamepad: s(c"extendedGamepad"),
            vendor_name: s(c"vendorName"),
            product_category: s(c"productCategory"),
            button_a: s(c"buttonA"),
            button_b: s(c"buttonB"),
            button_x: s(c"buttonX"),
            button_y: s(c"buttonY"),
            left_shoulder: s(c"leftShoulder"),
            right_shoulder: s(c"rightShoulder"),
            left_trigger: s(c"leftTrigger"),
            right_trigger: s(c"rightTrigger"),
            button_menu: s(c"buttonMenu"),
            button_options: s(c"buttonOptions"),
            button_home: s(c"buttonHome"),
            left_thumbstick_button: s(c"leftThumbstickButton"),
            right_thumbstick_button: s(c"rightThumbstickButton"),
            dpad: s(c"dpad"),
            up: s(c"up"),
            down: s(c"down"),
            left: s(c"left"),
            right: s(c"right"),
            left_thumbstick: s(c"leftThumbstick"),
            right_thumbstick: s(c"rightThumbstick"),
            x_axis: s(c"xAxis"),
            y_axis: s(c"yAxis"),
            value: s(c"value"),
            is_pressed: s(c"isPressed"),
        };
        Some(Runtime { class, sel })
    }

    /// `[object selector]`, for a method that returns an object.
    fn object(&self, object: Id, selector: Sel) -> Id {
        if object.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: see the type's notes.
        unsafe {
            let send = std::mem::transmute::<unsafe extern "C" fn(), SendId>(objc_msgSend);
            send(object, selector)
        }
    }

    /// `[object selector]` for a method that the object may not have (one
    /// newer than the system): nil without it.
    fn optional(&self, object: Id, selector: Sel) -> Id {
        if object.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: every object answers `respondsToSelector:` (NSObject).
        let answers = unsafe {
            let send = std::mem::transmute::<unsafe extern "C" fn(), SendSelBool>(objc_msgSend);
            send(object, self.sel.responds_to_selector, selector) != 0
        };
        if answers { self.object(object, selector) } else { std::ptr::null_mut() }
    }

    /// `[array count]`.
    fn count(&self, array: Id) -> usize {
        if array.is_null() {
            return 0;
        }
        // SAFETY: see the type's notes (an NSArray).
        unsafe {
            let send = std::mem::transmute::<unsafe extern "C" fn(), SendUsize>(objc_msgSend);
            send(array, self.sel.count)
        }
    }

    /// `[array objectAtIndex:i]`, `i` below the count.
    fn at(&self, array: Id, i: usize) -> Id {
        // SAFETY: see the type's notes (an NSArray, and `i` in range).
        unsafe {
            let send = std::mem::transmute::<unsafe extern "C" fn(), SendIdAt>(objc_msgSend);
            send(array, self.sel.object_at_index, i)
        }
    }

    /// `[button isPressed]` (false for nil).
    fn pressed(&self, button: Id) -> bool {
        if button.is_null() {
            return false;
        }
        // SAFETY: see the type's notes (a GCControllerButtonInput).
        unsafe {
            let send = std::mem::transmute::<unsafe extern "C" fn(), SendBool>(objc_msgSend);
            send(button, self.sel.is_pressed) != 0
        }
    }

    /// `[input value]` of a button or axis (0 for nil).
    fn value(&self, input: Id) -> f32 {
        if input.is_null() {
            return 0.0;
        }
        // SAFETY: see the type's notes (a GCControllerButtonInput or
        // GCControllerAxisInput; a float is returned in a register on both
        // processors, so plain objc_msgSend is the right entry point).
        unsafe {
            let send = std::mem::transmute::<unsafe extern "C" fn(), SendF32>(objc_msgSend);
            send(input, self.sel.value)
        }
    }

    /// An NSString as a Rust string (`None` for nil or empty).
    fn string(&self, string: Id) -> Option<String> {
        if string.is_null() {
            return None;
        }
        // SAFETY: see the type's notes (an NSString). The UTF-8 copy lives
        // in the autorelease pool, which outlives this.
        let text = unsafe {
            let send = std::mem::transmute::<unsafe extern "C" fn(), SendStr>(objc_msgSend);
            let utf8 = send(string, self.sel.utf8_string);
            if utf8.is_null() {
                return None;
            }
            CStr::from_ptr(utf8).to_string_lossy().trim().to_owned()
        };
        (!text.is_empty()).then_some(text)
    }

    /// A controller's name ("Xbox Wireless Controller", "DualSense Wireless
    /// Controller"…) and family, from its maker's name and Apple's category
    /// for it ("Xbox One", "DualShock 4", "Switch Pro Controller"…, on
    /// macOS 10.15 and later).
    fn describe(&self, controller: Id) -> (String, Family) {
        let vendor = self.string(self.object(controller, self.sel.vendor_name));
        let category = self.string(self.optional(controller, self.sel.product_category));
        let family = [&category, &vendor]
            .into_iter()
            .flatten()
            .map(|name| family_of(name))
            .find(|f| *f != Family::Generic)
            .unwrap_or(Family::Generic);
        let name = vendor.or(category).unwrap_or_else(|| "Controller".to_owned());
        (name, family)
    }

    /// The reading of an extended gamepad (`GCExtendedGamepad`).
    fn reading(&self, pad: Id) -> State {
        let s = &self.sel;
        let mut state = State::default();
        for (selector, button) in [
            (s.button_a, Button::South),
            (s.button_b, Button::East),
            (s.button_x, Button::West),
            (s.button_y, Button::North),
            (s.left_shoulder, Button::LeftShoulder),
            (s.right_shoulder, Button::RightShoulder),
        ] {
            state.set(button, self.pressed(self.object(pad, selector)));
        }
        for (selector, button) in [
            // ≡ on Xbox, Options on PlayStation, + on Switch (10.15).
            (s.button_menu, Button::Start),
            // View, Share or Create, − (10.15).
            (s.button_options, Button::Back),
            (s.button_home, Button::Guide),
            (s.left_thumbstick_button, Button::LeftStick),
            (s.right_thumbstick_button, Button::RightStick),
        ] {
            state.set(button, self.pressed(self.optional(pad, selector)));
        }
        let dpad = self.object(pad, s.dpad);
        for (selector, button) in [
            (s.up, Button::DpadUp),
            (s.down, Button::DpadDown),
            (s.left, Button::DpadLeft),
            (s.right, Button::DpadRight),
        ] {
            state.set(button, self.pressed(self.object(dpad, selector)));
        }
        for (selector, x, y) in [
            (s.left_thumbstick, Axis::LeftX, Axis::LeftY),
            (s.right_thumbstick, Axis::RightX, Axis::RightY),
        ] {
            let stick = self.object(pad, selector);
            state.set_axis(x, self.value(self.object(stick, s.x_axis)));
            state.set_axis(y, self.value(self.object(stick, s.y_axis)));
        }
        state.set_axis(Axis::LeftTrigger, self.value(self.object(pad, s.left_trigger)));
        state.set_axis(Axis::RightTrigger, self.value(self.object(pad, s.right_trigger)));
        state
    }
}

/// A connected controller.
struct Pad {
    /// The `GCController`, as an address (it names the controller while it
    /// is connected; the framework holds on to it).
    controller: usize,
    /// Names this connection (see [`Choice`]).
    key: u64,
    name: String,
    family: Family,
    before: State,
    state: State,
    /// It was in the framework's list this frame.
    seen: bool,
}

pub struct Backend {
    /// `None` until the framework is first needed, then whether it was
    /// found.
    runtime: Option<Option<Runtime>>,
    pads: Vec<Pad>,
    next_key: u64,
    choice: Choice,
}

impl Backend {
    pub fn new() -> Backend {
        Backend { runtime: None, pads: Vec::new(), next_key: 0, choice: Choice::default() }
    }

    /// Reads the controllers; the one in use, if any, with its key.
    pub fn poll(&mut self, _now: f64) -> Option<(u64, State)> {
        let rt = (*self.runtime.get_or_insert_with(Runtime::load))?;
        for pad in &mut self.pads {
            pad.seen = false;
        }
        // SAFETY: popped below, with nothing from the pool kept past it
        // (names are copied into Rust strings).
        let pool = unsafe { objc_autoreleasePoolPush() };
        let controllers = rt.object(rt.class, rt.sel.controllers);
        for i in 0..rt.count(controllers).min(MAX_CONTROLLERS) {
            let controller = rt.at(controllers, i);
            // Controllers without the full set of sticks and buttons (the
            // Siri Remote) have none.
            let gamepad = rt.object(controller, rt.sel.extended_gamepad);
            if gamepad.is_null() {
                continue;
            }
            let state = rt.reading(gamepad);
            if let Some(pad) = self.pads.iter_mut().find(|p| p.controller == controller as usize) {
                pad.before = pad.state;
                pad.state = state;
                pad.seen = true;
            } else {
                let (name, family) = rt.describe(controller);
                self.pads.push(Pad {
                    controller: controller as usize,
                    key: self.next_key,
                    name,
                    family,
                    before: State::default(),
                    state,
                    seen: true,
                });
                self.next_key += 1;
            }
        }
        // SAFETY: the pool pushed above.
        unsafe { objc_autoreleasePoolPop(pool) };
        self.pads.retain(|p| p.seen);

        let pads = self.pads.iter().map(|p| (p.key, &p.before, &p.state));
        let key = self.choice.pick(pads, &DEADZONES)?;
        self.pads.iter().find(|p| p.key == key).map(|p| (key, p.state))
    }

    fn current(&self) -> Option<&Pad> {
        let key = self.choice.current()?;
        self.pads.iter().find(|p| p.key == key)
    }

    pub fn name(&self) -> Option<String> {
        self.current().map(|p| p.name.clone())
    }

    pub fn family(&self) -> Family {
        self.current().map_or(Family::Generic, |p| p.family)
    }

    /// Not on macOS (the framework's haptics are another matter).
    pub fn rumble(&mut self, _strength: f32) {}
}
