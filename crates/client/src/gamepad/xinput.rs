//! Controllers on Windows, through XInput: Xbox controllers, and the many
//! that act like one (PlayStation controllers too, through Steam Input or
//! DS4Windows).
//!
//! XInput is loaded when the game first looks for a controller, from the
//! system folder only (a DLL of that name next to the game is never
//! loaded): `xinput1_4.dll` (Windows 8 and later), else `xinput1_3.dll`
//! (the DirectX runtime), else `xinput9_1_0.dll`. Without any, there is
//! never a controller.
//!
//! XInput has four slots. Asking about an empty one is slow (it looks for
//! the device each time, a millisecond or more), so a connected slot is
//! read every frame and an empty one only every two seconds, one slot at a
//! time.

use std::ffi::{c_char, c_void};

use super::tracker::{Choice, Deadzones, State};
use super::{Axis, Button, Family, family_of};

/// XInput's own deadzones (`XINPUT_GAMEPAD_LEFT_THUMB_DEADZONE`,
/// `XINPUT_GAMEPAD_RIGHT_THUMB_DEADZONE`, `XINPUT_GAMEPAD_TRIGGER_THRESHOLD`
/// of their ranges), the ones Microsoft recommends for its controllers.
pub const DEADZONES: Deadzones =
    Deadzones { sticks: [7849.0 / 32767.0, 8689.0 / 32767.0], trigger: 30.0 / 255.0 };

const SLOTS: usize = 4;
/// How long an empty slot goes unasked (seconds): the slots take turns,
/// one every half second.
const EMPTY_EVERY: f64 = 2.0;

const ERROR_SUCCESS: u32 = 0;
const LOAD_LIBRARY_SEARCH_SYSTEM32: u32 = 0x0000_0800;

/// `XINPUT_GAMEPAD`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct Gamepad {
    buttons: u16,
    left_trigger: u8,
    right_trigger: u8,
    thumb_lx: i16,
    thumb_ly: i16,
    thumb_rx: i16,
    thumb_ry: i16,
}

/// `XINPUT_STATE`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct XInputState {
    packet_number: u32,
    gamepad: Gamepad,
}

/// `XINPUT_VIBRATION`: the low (left) and high (right) frequency motors.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct Vibration {
    left_motor: u16,
    right_motor: u16,
}

type GetState = unsafe extern "system" fn(u32, *mut XInputState) -> u32;
type SetState = unsafe extern "system" fn(u32, *mut Vibration) -> u32;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryExW(name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
}

/// `XINPUT_GAMEPAD_*`: the bits of [`Gamepad::buttons`]. (0x0400, the
/// guide button, is only reported by an undocumented call.)
const BUTTONS: [(u16, Button); 14] = [
    (0x0001, Button::DpadUp),
    (0x0002, Button::DpadDown),
    (0x0004, Button::DpadLeft),
    (0x0008, Button::DpadRight),
    (0x0010, Button::Start),
    (0x0020, Button::Back),
    (0x0040, Button::LeftStick),
    (0x0080, Button::RightStick),
    (0x0100, Button::LeftShoulder),
    (0x0200, Button::RightShoulder),
    (0x1000, Button::South),
    (0x2000, Button::East),
    (0x4000, Button::West),
    (0x8000, Button::North),
];

/// The functions of the XInput that was loaded.
#[derive(Clone, Copy)]
struct Api {
    get_state: GetState,
    set_state: Option<SetState>,
}

impl Api {
    fn load() -> Option<Api> {
        for dll in ["xinput1_4.dll", "xinput1_3.dll", "xinput9_1_0.dll"] {
            let name: Vec<u16> = dll.encode_utf16().chain(Some(0)).collect();
            // SAFETY: a NUL-terminated name that outlives the call. The
            // library stays loaded for as long as the game runs.
            let module = unsafe {
                LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32)
            };
            if module.is_null() {
                continue;
            }
            // SAFETY: a module that is loaded, and NUL-terminated names.
            let (get, set) = unsafe {
                (
                    GetProcAddress(module, c"XInputGetState".as_ptr()),
                    GetProcAddress(module, c"XInputSetState".as_ptr()),
                )
            };
            if get.is_null() {
                continue;
            }
            // SAFETY: the functions of those names have these signatures
            // (Xinput.h), and the module is never unloaded.
            return Some(unsafe {
                Api {
                    get_state: std::mem::transmute::<*mut c_void, GetState>(get),
                    set_state: (!set.is_null())
                        .then(|| std::mem::transmute::<*mut c_void, SetState>(set)),
                }
            });
        }
        None
    }
}

/// A stick axis, −32768..32767, as −1..1.
fn unit(value: i16) -> f32 {
    (f32::from(value) / 32767.0).max(-1.0)
}

/// A reading of XInput's, in the module's conventions (its Y is already
/// up-positive).
fn reading(pad: &Gamepad) -> State {
    let mut s = State::default();
    for (mask, button) in BUTTONS {
        s.set(button, pad.buttons & mask != 0);
    }
    s.set_axis(Axis::LeftX, unit(pad.thumb_lx));
    s.set_axis(Axis::LeftY, unit(pad.thumb_ly));
    s.set_axis(Axis::RightX, unit(pad.thumb_rx));
    s.set_axis(Axis::RightY, unit(pad.thumb_ry));
    s.set_axis(Axis::LeftTrigger, f32::from(pad.left_trigger) / 255.0);
    s.set_axis(Axis::RightTrigger, f32::from(pad.right_trigger) / 255.0);
    s
}

/// One of XInput's four slots.
#[derive(Debug, Clone, Copy, Default)]
struct Slot {
    /// Names the connection while a controller is there (see [`Choice`]).
    key: Option<u64>,
    before: State,
    state: State,
}

pub struct Backend {
    /// `None` until XInput is first needed, then whether it was found.
    api: Option<Option<Api>>,
    slots: [Slot; SLOTS],
    /// When an empty slot is asked about next (`None`: never yet), and
    /// which one's turn it is.
    next_empty: Option<f64>,
    turn: usize,
    next_key: u64,
    choice: Choice,
    /// The slot whose motors are running.
    rumbling: Option<usize>,
}

impl Backend {
    pub fn new() -> Backend {
        Backend {
            api: None,
            slots: [Slot::default(); SLOTS],
            next_empty: None,
            turn: 0,
            next_key: 0,
            choice: Choice::default(),
            rumbling: None,
        }
    }

    /// Asks XInput about slot `i`.
    fn read(&mut self, api: Api, i: usize) {
        let mut raw = XInputState::default();
        // SAFETY: a valid slot number and a buffer of the right type.
        let result = unsafe { (api.get_state)(i as u32, &mut raw) };
        let slot = &mut self.slots[i];
        if result == ERROR_SUCCESS {
            if slot.key.is_none() {
                *slot = Slot { key: Some(self.next_key), ..Slot::default() };
                self.next_key += 1;
            }
            slot.before = slot.state;
            slot.state = reading(&raw.gamepad);
        } else {
            // ERROR_DEVICE_NOT_CONNECTED, or anything else: no controller.
            *slot = Slot::default();
            if self.rumbling == Some(i) {
                self.rumbling = None;
            }
        }
    }

    /// Reads the controllers; the one in use, if any, with its key.
    pub fn poll(&mut self, now: f64) -> Option<(u64, State)> {
        let api = (*self.api.get_or_insert_with(Api::load))?;
        for i in 0..SLOTS {
            if self.slots[i].key.is_some() {
                self.read(api, i);
            }
        }
        if self.next_empty.is_none_or(|next| now >= next) {
            let first = self.next_empty.is_none();
            self.next_empty = Some(now + EMPTY_EVERY / SLOTS as f64);
            // At the start, every slot (so a controller that is already
            // connected is there from the first frame); then one in turn.
            let asked = if first { SLOTS } else { 1 };
            for _ in 0..asked {
                let empty = (0..SLOTS)
                    .map(|n| (self.turn + n) % SLOTS)
                    .find(|&i| self.slots[i].key.is_none());
                let Some(i) = empty else { break };
                self.turn = (i + 1) % SLOTS;
                self.read(api, i);
            }
        }
        let pads = self.slots.iter().filter_map(|s| Some((s.key?, &s.before, &s.state)));
        let key = self.choice.pick(pads, &DEADZONES)?;
        self.slots.iter().find(|s| s.key == Some(key)).map(|s| (key, s.state))
    }

    fn current(&self) -> Option<usize> {
        let key = self.choice.current()?;
        self.slots.iter().position(|s| s.key == Some(key))
    }

    pub fn name(&self) -> Option<String> {
        // XInput knows no names, only the slot (the light on an Xbox 360
        // controller shows its number).
        self.current().map(|i| format!("XInput controller {}", i + 1))
    }

    pub fn family(&self) -> Family {
        self.name().map_or(Family::Generic, |name| family_of(&name))
    }

    /// Runs both motors of the controller in use at `strength` (0..1), or
    /// stops them at 0.
    pub fn rumble(&mut self, strength: f32) {
        let Some(Some(Api { set_state: Some(set_state), .. })) = self.api else { return };
        let set = |slot: usize, strength: f32| {
            let speed = (strength.clamp(0.0, 1.0) * 65535.0) as u16;
            let mut vibration = Vibration { left_motor: speed, right_motor: speed };
            // SAFETY: a valid slot number and a buffer of the right type.
            unsafe { set_state(slot as u32, &mut vibration) };
        };
        let current = self.current();
        if let Some(old) = self.rumbling.take()
            && (Some(old) != current || strength <= 0.0)
        {
            set(old, 0.0);
        }
        if let Some(slot) = current
            && strength > 0.0
        {
            set(slot, strength);
            self.rumbling = Some(slot);
        }
    }
}
