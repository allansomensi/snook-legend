//! Controllers on Linux, through the kernel's joystick devices
//! (`/dev/input/js0` to `js7`, the "joydev" interface): no libudev, and
//! readable by the logged-in user on every desktop distribution.
//!
//! Each device reports numbered buttons and axes. Two ioctls say which
//! kernel input code each number stands for (`BTN_SOUTH`, `ABS_RX`…), and
//! the drivers mostly agree on what those codes mean for a gamepad
//! (Documentation/input/gamepad.rst), with two exceptions handled here:
//!
//! - The Xbox driver (xpad), and anything that copies it, reports the
//!   Xbox X button (left) as `BTN_X` and Y (top) as `BTN_Y`. Those are the
//!   same codes as `BTN_NORTH` (top) and `BTN_WEST` (left) in the gamepad
//!   layout that the PlayStation and Nintendo drivers follow, so the two
//!   meanings can't be told apart from the codes. The device decides: one
//!   driven by xpad or Valve's driver (from sysfs), made by Microsoft or
//!   Valve, or whose name says Xbox, X-Box, XInput, Microsoft or Steam has
//!   the Xbox meaning; any other, the gamepad layout. (Steam Input's
//!   virtual controller is a "Microsoft X-Box 360 pad".)
//! - Cheap controllers on the generic HID driver have the right stick on
//!   `ABS_Z`/`ABS_RZ` (where the others have their analog triggers) and
//!   buttons numbered from `BTN_TRIGGER`. A device without `ABS_RX`/`ABS_RY`
//!   is taken to be one of those, and its buttons are mapped in order.
//!
//! The joystick interface scales every axis to −32767..32767 whatever its
//! range in the driver, Y growing downwards: an analog trigger is −32767
//! released and 32767 pulled all the way.

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use super::tracker::{Choice, Deadzones, State};
use super::{Axis, Button, Family, family_of};

pub const DEADZONES: Deadzones = Deadzones::STANDARD;

/// How many joystick devices are looked for (`js0` to `js7`).
const DEVICES: u8 = 8;
/// How often to look for controllers plugged in since (seconds).
const RESCAN_EVERY: f64 = 2.0;
/// How often a device that is no gamepad is looked at again when sysfs
/// can't say whether it is still the same device (seconds).
const RECHECK_EVERY: f64 = 30.0;

// struct js_event { __u32 time; __s16 value; __u8 type; __u8 number; }
const EVENT_SIZE: usize = 8;
const JS_EVENT_BUTTON: u8 = 0x01;
const JS_EVENT_AXIS: u8 = 0x02;
/// Marks the events that give the state at the time the device was opened.
const JS_EVENT_INIT: u8 = 0x80;

/// The request number of `_IOR(ty, nr, size)` (include/uapi/asm-generic/
/// ioctl.h): the direction (2, read) in the top two bits, then 14 bits of
/// argument size, the type and the number. x86, ARM and RISC-V use this
/// generic encoding (PowerPC, MIPS and SPARC differ; the game isn't built
/// for them).
const fn ior(ty: u8, nr: u8, size: usize) -> u32 {
    const IOC_READ: u32 = 2;
    (IOC_READ << 30) | ((size as u32) << 16) | ((ty as u32) << 8) | nr as u32
}

/// From include/uapi/linux/input-event-codes.h.
const ABS_CNT: usize = 0x40;
const BTN_MISC: usize = 0x100;
const KEY_MAX: usize = 0x2ff;
/// The longest name asked for.
const NAME_LEN: usize = 128;

/// The number of axes (`__u8`).
const JSIOCGAXES: u32 = ior(b'j', 0x11, 1);
/// The number of buttons (`__u8`).
const JSIOCGBUTTONS: u32 = ior(b'j', 0x12, 1);
/// The name, `JSIOCGNAME(len)`.
const JSIOCGNAME: u32 = ior(b'j', 0x13, NAME_LEN);
/// The input code of each axis (`__u8[ABS_CNT]`).
const JSIOCGAXMAP: u32 = ior(b'j', 0x32, ABS_CNT);
/// The input code of each button (`__u16[KEY_MAX - BTN_MISC + 1]`).
const JSIOCGBTNMAP: u32 = ior(b'j', 0x34, (KEY_MAX - BTN_MISC + 1) * 2);

// Axes.
const ABS_X: u8 = 0x00;
const ABS_Y: u8 = 0x01;
const ABS_Z: u8 = 0x02;
const ABS_RX: u8 = 0x03;
const ABS_RY: u8 = 0x04;
const ABS_RZ: u8 = 0x05;
const ABS_GAS: u8 = 0x09;
const ABS_BRAKE: u8 = 0x0a;
const ABS_HAT0X: u8 = 0x10;
const ABS_HAT0Y: u8 = 0x11;

// Buttons.
/// The first of the buttons of joysticks and generic controllers
/// (`BTN_TRIGGER`, `BTN_THUMB`… `BTN_BASE6`).
const BTN_JOYSTICK: u16 = 0x120;
const BTN_SOUTH: u16 = 0x130;
const BTN_EAST: u16 = 0x131;
/// `BTN_X` on xpad: the Xbox X button (left).
const BTN_NORTH: u16 = 0x133;
/// `BTN_Y` on xpad: the Xbox Y button (top).
const BTN_WEST: u16 = 0x134;
const BTN_TL: u16 = 0x136;
const BTN_TR: u16 = 0x137;
const BTN_TL2: u16 = 0x138;
const BTN_TR2: u16 = 0x139;
const BTN_SELECT: u16 = 0x13a;
const BTN_START: u16 = 0x13b;
const BTN_MODE: u16 = 0x13c;
const BTN_THUMBL: u16 = 0x13d;
const BTN_THUMBR: u16 = 0x13e;
const BTN_DPAD_UP: u16 = 0x220;
const BTN_DPAD_DOWN: u16 = 0x221;
const BTN_DPAD_LEFT: u16 = 0x222;
const BTN_DPAD_RIGHT: u16 = 0x223;
/// xpad's d-pad on dance mats and the like: left, right, up, down.
const BTN_TRIGGER_HAPPY1: u16 = 0x2c0;

/// USB vendors.
const MICROSOFT: u16 = 0x045e;
const SONY: u16 = 0x054c;
const NINTENDO: u16 = 0x057e;
const VALVE: u16 = 0x28de;

/// What a hat axis must pass to count as a d-pad direction.
const HAT_PRESS: i16 = 16384;

/// What a device's axis is to the game.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Role {
    Unused,
    /// A stick axis, upside down (the joystick interface's Y grows
    /// downwards).
    Stick(Axis, bool),
    Trigger(Axis),
    /// The d-pad as an axis: −1 left or up, 1 right or down.
    HatX,
    HatY,
}

/// What a device's button is to the game.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Press {
    Unused,
    Button(Button),
    /// A trigger with no analog axis (the Switch Pro Controller's ZL and
    /// ZR): pulled all the way while down.
    Trigger(Axis),
}

/// What a device's numbered buttons and axes are to the game.
#[derive(Debug, Clone, PartialEq)]
struct Layout {
    buttons: Vec<Press>,
    axes: Vec<Role>,
}

impl Layout {
    /// From the input codes of the device's axes and buttons, in the order
    /// of their numbers. `xbox` is the meaning of `BTN_X`/`BTN_Y` (see the
    /// module's notes).
    fn new(axis_codes: &[u8], button_codes: &[u16], xbox: bool) -> Layout {
        let has = |code: u8| axis_codes.contains(&code);
        // Sticks on X/Y and RX/RY, triggers on Z/RZ (xpad, PlayStation,
        // Nintendo), or the right stick on Z/RZ (generic controllers, Xbox
        // controllers over Bluetooth, with their triggers on brake and gas).
        let right_on_z = !(has(ABS_RX) && has(ABS_RY)) && has(ABS_Z) && has(ABS_RZ);
        let axes: Vec<Role> = axis_codes
            .iter()
            .map(|&code| match code {
                ABS_X => Role::Stick(Axis::LeftX, false),
                ABS_Y => Role::Stick(Axis::LeftY, true),
                ABS_RX if !right_on_z => Role::Stick(Axis::RightX, false),
                ABS_RY if !right_on_z => Role::Stick(Axis::RightY, true),
                ABS_Z if right_on_z => Role::Stick(Axis::RightX, false),
                ABS_RZ if right_on_z => Role::Stick(Axis::RightY, true),
                ABS_Z | ABS_BRAKE => Role::Trigger(Axis::LeftTrigger),
                ABS_RZ | ABS_GAS => Role::Trigger(Axis::RightTrigger),
                ABS_HAT0X => Role::HatX,
                ABS_HAT0Y => Role::HatY,
                _ => Role::Unused,
            })
            .collect();
        let analog = |a: Axis| axes.contains(&Role::Trigger(a));
        let (left_analog, right_analog) = (analog(Axis::LeftTrigger), analog(Axis::RightTrigger));
        let buttons = button_codes
            .iter()
            .map(|&code| {
                let b = match code {
                    BTN_SOUTH => Button::South,
                    BTN_EAST => Button::East,
                    BTN_NORTH if xbox => Button::West,
                    BTN_NORTH => Button::North,
                    BTN_WEST if xbox => Button::North,
                    BTN_WEST => Button::West,
                    BTN_TL => Button::LeftShoulder,
                    BTN_TR => Button::RightShoulder,
                    // PlayStation controllers also report their analog
                    // triggers as buttons, from the lightest touch.
                    BTN_TL2 if left_analog => return Press::Unused,
                    BTN_TR2 if right_analog => return Press::Unused,
                    BTN_TL2 => return Press::Trigger(Axis::LeftTrigger),
                    BTN_TR2 => return Press::Trigger(Axis::RightTrigger),
                    BTN_SELECT => Button::Back,
                    BTN_START => Button::Start,
                    BTN_MODE => Button::Guide,
                    BTN_THUMBL => Button::LeftStick,
                    BTN_THUMBR => Button::RightStick,
                    BTN_DPAD_UP => Button::DpadUp,
                    BTN_DPAD_DOWN => Button::DpadDown,
                    BTN_DPAD_LEFT => Button::DpadLeft,
                    BTN_DPAD_RIGHT => Button::DpadRight,
                    _ if xbox && (BTN_TRIGGER_HAPPY1..BTN_TRIGGER_HAPPY1 + 4).contains(&code) => {
                        [Button::DpadLeft, Button::DpadRight, Button::DpadUp, Button::DpadDown]
                            [usize::from(code - BTN_TRIGGER_HAPPY1)]
                    }
                    // Generic controllers: their buttons in order (1 to 12
                    // printed on them, mostly), a guess at the usual places.
                    _ if (BTN_JOYSTICK..BTN_JOYSTICK + 12).contains(&code) => {
                        let order = [
                            Button::South,
                            Button::East,
                            Button::West,
                            Button::North,
                            Button::LeftShoulder,
                            Button::RightShoulder,
                        ];
                        match usize::from(code - BTN_JOYSTICK) {
                            i @ 0..6 => order[i],
                            6 if !left_analog => return Press::Trigger(Axis::LeftTrigger),
                            7 if !right_analog => return Press::Trigger(Axis::RightTrigger),
                            8 => Button::Back,
                            9 => Button::Start,
                            10 => Button::LeftStick,
                            11 => Button::RightStick,
                            _ => return Press::Unused,
                        }
                    }
                    _ => return Press::Unused,
                };
                Press::Button(b)
            })
            .collect();
        Layout { buttons, axes }
    }

    /// Whether the device looks like a controller: some button of a
    /// gamepad or joystick, and a stick. (Accelerometers and the motion
    /// sensors of PlayStation controllers can be joystick devices too.)
    fn is_controller(&self) -> bool {
        self.buttons.iter().any(|p| *p != Press::Unused)
            && self.axes.contains(&Role::Stick(Axis::LeftX, false))
    }
}

/// Whether `BTN_X`/`BTN_Y` have xpad's meaning on this device (see the
/// module's notes).
fn xbox_layout(name: &str, driver: Option<&str>, vendor: Option<u16>) -> bool {
    let name = name.to_lowercase();
    matches!(driver, Some("xpad" | "steam" | "hid-steam"))
        || matches!(vendor, Some(MICROSOFT | VALVE))
        || ["xbox", "x-box", "xinput", "microsoft", "steam"].iter().any(|w| name.contains(w))
}

/// The family of a device, from its maker or driver when sysfs says (every
/// controller xpad drives is labelled like an Xbox one), or its name.
fn family(name: &str, driver: Option<&str>, vendor: Option<u16>) -> Family {
    match (vendor, driver) {
        (Some(MICROSOFT | VALVE), _) | (_, Some("xpad")) => Family::Xbox,
        (Some(SONY), _) => Family::PlayStation,
        (Some(NINTENDO), _) => Family::Nintendo,
        _ => family_of(name),
    }
}

/// An axis of the joystick interface, −32767..32767, as −1..1.
fn unit(value: i16) -> f32 {
    (f32::from(value) / 32767.0).clamp(-1.0, 1.0)
}

/// An open joystick device.
struct Device {
    /// N in `/dev/input/jsN`.
    index: u8,
    /// Names this connection (see [`Choice`]).
    key: u64,
    file: File,
    name: String,
    family: Family,
    layout: Layout,
    /// The device's buttons and axes as it last reported them.
    buttons: Vec<bool>,
    axes: Vec<i16>,
    /// The previous and latest readings.
    before: State,
    state: State,
}

impl Device {
    fn new(
        index: u8,
        key: u64,
        file: File,
        name: String,
        family: Family,
        layout: Layout,
    ) -> Device {
        // Until the device says otherwise, the triggers are released (0 on
        // this scale is pulled halfway).
        let axes = layout
            .axes
            .iter()
            .map(|role| if matches!(role, Role::Trigger(_)) { -32767 } else { 0 })
            .collect();
        Device {
            index,
            key,
            file,
            name,
            family,
            buttons: vec![false; layout.buttons.len()],
            axes,
            layout,
            before: State::default(),
            state: State::default(),
        }
    }

    /// Takes one `js_event`.
    fn apply(&mut self, event: &[u8]) {
        let value = i16::from_ne_bytes([event[4], event[5]]);
        let number = usize::from(event[7]);
        match event[6] & !JS_EVENT_INIT {
            JS_EVENT_BUTTON => {
                if let Some(b) = self.buttons.get_mut(number) {
                    *b = value != 0;
                }
            }
            JS_EVENT_AXIS => {
                if let Some(a) = self.axes.get_mut(number) {
                    *a = value;
                }
            }
            _ => {}
        }
    }

    /// Reads what the device reported since the last frame. False once it
    /// is gone (unplugged: `ENODEV`) or fails in any other way that won't
    /// mend.
    fn read(&mut self) -> bool {
        let mut buffer = [0u8; EVENT_SIZE * 64];
        // A bound, so a device that never stops talking can't hold up a
        // frame (what is left is read next frame).
        for _ in 0..16 {
            match self.file.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    for event in buffer[..n].chunks_exact(EVENT_SIZE) {
                        self.apply(event);
                    }
                    if n < buffer.len() {
                        break;
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(_) => return false,
            }
        }
        true
    }

    /// The reading, from the buttons and axes as last reported.
    fn reading(&self) -> State {
        let mut s = State::default();
        for (press, _) in self.layout.buttons.iter().zip(&self.buttons).filter(|(_, down)| **down) {
            match *press {
                Press::Button(b) => s.set(b, true),
                Press::Trigger(a) => s.set_axis(a, 1.0),
                Press::Unused => {}
            }
        }
        for (role, &value) in self.layout.axes.iter().zip(&self.axes) {
            match *role {
                Role::Stick(a, flip) => {
                    s.set_axis(a, if flip { -unit(value) } else { unit(value) })
                }
                Role::Trigger(a) => {
                    let pulled = (f32::from(value) + 32767.0) / 65534.0;
                    s.set_axis(a, s.axis(a).max(pulled));
                }
                Role::HatX => {
                    s.set(Button::DpadLeft, s.down(Button::DpadLeft) || value <= -HAT_PRESS);
                    s.set(Button::DpadRight, s.down(Button::DpadRight) || value >= HAT_PRESS);
                }
                Role::HatY => {
                    s.set(Button::DpadUp, s.down(Button::DpadUp) || value <= -HAT_PRESS);
                    s.set(Button::DpadDown, s.down(Button::DpadDown) || value >= HAT_PRESS);
                }
                Role::Unused => {}
            }
        }
        s
    }
}

/// Opens `/dev/input/js{index}` and learns its buttons and axes. `None`
/// when there is no such device, it can't be opened, or it isn't a
/// joystick device at all; [`Opened::Ignored`] when it is no controller.
fn open(index: u8, key: u64) -> Option<Opened> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(format!("/dev/input/js{index}"))
        .ok()?;
    let fd = file.as_raw_fd();
    let ask = |request: u32, out: *mut libc::c_void| {
        // SAFETY: each request is given a buffer of the size encoded in its
        // number (see the call sites), which the kernel writes at most; the
        // descriptor is open for as long as `file` lives.
        unsafe { libc::ioctl(fd, request as libc::Ioctl, out) >= 0 }
    };
    let mut axis_count = 0u8;
    let mut button_count = 0u8;
    if !ask(JSIOCGAXES, (&raw mut axis_count).cast())
        || !ask(JSIOCGBUTTONS, (&raw mut button_count).cast())
    {
        return None;
    }
    let mut name = [0u8; NAME_LEN];
    let name = if ask(JSIOCGNAME, name.as_mut_ptr().cast()) {
        let end = name.iter().position(|&c| c == 0).unwrap_or(NAME_LEN);
        String::from_utf8_lossy(&name[..end]).trim().to_owned()
    } else {
        String::new()
    };
    // Without the maps (a very old kernel, or one whose KEY_MAX differs,
    // which changes the request number), the codes are a guess: axes X, Y,
    // Z…, and generic buttons in order.
    let mut axis_map = [0u8; ABS_CNT];
    if !ask(JSIOCGAXMAP, axis_map.as_mut_ptr().cast()) {
        axis_map = std::array::from_fn(|i| i as u8);
    }
    let mut button_map = [0u16; KEY_MAX - BTN_MISC + 1];
    if !ask(JSIOCGBTNMAP, button_map.as_mut_ptr().cast()) {
        button_map = std::array::from_fn(|i| BTN_JOYSTICK + i as u16);
    }
    let axis_codes = &axis_map[..usize::from(axis_count).min(ABS_CNT)];
    let button_codes = &button_map[..usize::from(button_count)];

    let (vendor, driver) = sysfs_details(index);
    let layout =
        Layout::new(axis_codes, button_codes, xbox_layout(&name, driver.as_deref(), vendor));
    if !layout.is_controller() {
        return Some(Opened::Ignored);
    }
    let family = family(&name, driver.as_deref(), vendor);
    Some(Opened::Controller(Device::new(index, key, file, name, family, layout)))
}

enum Opened {
    Controller(Device),
    /// A joystick device that is no controller.
    Ignored,
}

/// The USB (or Bluetooth) vendor of `jsN` and the name of its driver, from
/// sysfs when it can be read (a sandbox may hide it).
fn sysfs_details(index: u8) -> (Option<u16>, Option<String>) {
    let input = format!("/sys/class/input/js{index}/device");
    let vendor = std::fs::read_to_string(format!("{input}/id/vendor"))
        .ok()
        .and_then(|v| u16::from_str_radix(v.trim(), 16).ok());
    let driver = std::fs::read_link(format!("{input}/device/driver"))
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
    (vendor, driver)
}

/// Where sysfs says `jsN` is: it changes when another device takes the
/// number.
fn sysfs_place(index: u8) -> Option<PathBuf> {
    std::fs::read_link(format!("/sys/class/input/js{index}")).ok()
}

/// A joystick device found to be no controller, so it isn't opened again
/// every few seconds (opening some devices wakes them up).
struct Ignored {
    place: Option<PathBuf>,
    since: f64,
}

pub struct Backend {
    devices: Vec<Device>,
    ignored: [Option<Ignored>; DEVICES as usize],
    next_scan: f64,
    next_key: u64,
    choice: Choice,
}

impl Backend {
    pub fn new() -> Backend {
        Backend {
            devices: Vec::new(),
            ignored: Default::default(),
            next_scan: 0.0,
            next_key: 0,
            choice: Choice::default(),
        }
    }

    /// Opens the devices that appeared since the last look.
    fn scan(&mut self, now: f64) {
        for index in 0..DEVICES {
            if self.devices.iter().any(|d| d.index == index) {
                continue;
            }
            if let Some(ignored) = &self.ignored[usize::from(index)] {
                let changed = match (sysfs_place(index), &ignored.place) {
                    (Some(place), Some(was)) => place != *was,
                    (None, None) => now - ignored.since >= RECHECK_EVERY,
                    // It appeared in sysfs, or went away.
                    _ => true,
                };
                if !changed {
                    continue;
                }
                self.ignored[usize::from(index)] = None;
            }
            match open(index, self.next_key) {
                Some(Opened::Controller(device)) => {
                    self.next_key += 1;
                    self.devices.push(device);
                }
                Some(Opened::Ignored) => {
                    self.ignored[usize::from(index)] =
                        Some(Ignored { place: sysfs_place(index), since: now });
                }
                None => {}
            }
        }
        self.devices.sort_by_key(|d| d.index);
    }

    /// Reads the controllers; the one in use, if any, with its key.
    pub fn poll(&mut self, now: f64) -> Option<(u64, State)> {
        if now >= self.next_scan {
            self.next_scan = now + RESCAN_EVERY;
            self.scan(now);
        }
        self.devices.retain_mut(Device::read);
        for device in &mut self.devices {
            device.before = device.state;
            device.state = device.reading();
        }
        let pads = self.devices.iter().map(|d| (d.key, &d.before, &d.state));
        let key = self.choice.pick(pads, &DEADZONES)?;
        self.devices.iter().find(|d| d.key == key).map(|d| (key, d.state))
    }

    fn current(&self) -> Option<&Device> {
        let key = self.choice.current()?;
        self.devices.iter().find(|d| d.key == key)
    }

    pub fn name(&self) -> Option<String> {
        self.current().map(|d| d.name.clone())
    }

    pub fn family(&self) -> Family {
        self.current().map_or(Family::Generic, |d| d.family)
    }

    /// The joystick interface has no force feedback.
    pub fn rumble(&mut self, _strength: f32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gamepad_ioctl_numbers() {
        assert_eq!(JSIOCGAXES, 0x80016a11);
        assert_eq!(JSIOCGBUTTONS, 0x80016a12);
        assert_eq!(JSIOCGNAME, 0x80806a13);
        assert_eq!(JSIOCGAXMAP, 0x80406a32);
        assert_eq!(JSIOCGBTNMAP, 0x84006a34);
    }

    /// A device on a pipe instead of `/dev/input` (the same reads, without
    /// the ioctls).
    fn device(
        axis_codes: &[u8],
        button_codes: &[u16],
        name: &str,
    ) -> (Device, std::io::PipeWriter) {
        let (reader, writer) = std::io::pipe().unwrap();
        let file = File::from(std::os::fd::OwnedFd::from(reader));
        // SAFETY: plain fcntl calls on a descriptor that is open.
        unsafe {
            let flags = libc::fcntl(file.as_raw_fd(), libc::F_GETFL);
            libc::fcntl(file.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
        let layout = Layout::new(axis_codes, button_codes, xbox_layout(name, None, None));
        (Device::new(0, 0, file, name.to_owned(), family(name, None, None), layout), writer)
    }

    fn event(kind: u8, number: u8, value: i16) -> [u8; EVENT_SIZE] {
        let mut e = [0u8; EVENT_SIZE];
        e[..4].copy_from_slice(&1234u32.to_ne_bytes());
        e[4..6].copy_from_slice(&value.to_ne_bytes());
        e[6] = kind;
        e[7] = number;
        e
    }

    fn send(writer: &mut std::io::PipeWriter, events: &[[u8; EVENT_SIZE]]) {
        use std::io::Write;
        writer.write_all(&events.concat()).unwrap();
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    /// xpad's axes and buttons, in the order it numbers them.
    const XPAD_AXES: [u8; 8] = [ABS_X, ABS_Y, ABS_Z, ABS_RX, ABS_RY, ABS_RZ, ABS_HAT0X, ABS_HAT0Y];
    const XPAD_BUTTONS: [u16; 11] = [
        BTN_SOUTH, BTN_EAST, BTN_NORTH, BTN_WEST, BTN_TL, BTN_TR, BTN_SELECT, BTN_START, BTN_MODE,
        BTN_THUMBL, BTN_THUMBR,
    ];

    #[test]
    fn gamepad_xbox_through_xpad() {
        let (mut d, mut w) = device(&XPAD_AXES, &XPAD_BUTTONS, "Microsoft X-Box 360 pad");
        assert_eq!(d.family, Family::Xbox);
        assert!(d.layout.is_controller());
        // Nothing sent yet: the triggers are released, not half pulled.
        assert!(d.read());
        assert_eq!(d.reading(), State::default());

        // The state at opening, then a few changes, in one go.
        send(
            &mut w,
            &[
                event(JS_EVENT_BUTTON | JS_EVENT_INIT, 0, 0),
                event(JS_EVENT_AXIS | JS_EVENT_INIT, 2, -32767),
                event(JS_EVENT_AXIS | JS_EVENT_INIT, 5, -32767),
                // X (BTN_X, the left face button), and LB.
                event(JS_EVENT_BUTTON, 2, 1),
                event(JS_EVENT_BUTTON, 4, 1),
                // The left stick up and to the right, the right one down.
                event(JS_EVENT_AXIS, 0, 32767),
                event(JS_EVENT_AXIS, 1, -32767),
                event(JS_EVENT_AXIS, 4, 16384),
                // The right trigger all the way, the left one halfway.
                event(JS_EVENT_AXIS, 5, 32767),
                event(JS_EVENT_AXIS, 2, 0),
                // The d-pad up and left.
                event(JS_EVENT_AXIS, 6, -32767),
                event(JS_EVENT_AXIS, 7, -32767),
                // Numbers it doesn't have are ignored.
                event(JS_EVENT_BUTTON, 200, 1),
                event(JS_EVENT_AXIS, 63, 1),
            ],
        );
        assert!(d.read());
        let s = d.reading();
        assert!(s.down(Button::West) && !s.down(Button::North));
        assert!(s.down(Button::LeftShoulder) && s.down(Button::DpadUp) && s.down(Button::DpadLeft));
        assert!(!s.down(Button::DpadDown) && !s.down(Button::DpadRight));
        assert_eq!(s.axis(Axis::LeftX), 1.0);
        assert_eq!(s.axis(Axis::LeftY), 1.0);
        assert!(close(s.axis(Axis::RightY), -0.5));
        assert_eq!(s.axis(Axis::RightTrigger), 1.0);
        assert!(close(s.axis(Axis::LeftTrigger), 0.5));

        // Y (BTN_Y, the top one), the guide button.
        send(&mut w, &[event(JS_EVENT_BUTTON, 2, 0), event(JS_EVENT_BUTTON, 3, 1)]);
        send(&mut w, &[event(JS_EVENT_BUTTON, 8, 1)]);
        assert!(d.read());
        let s = d.reading();
        assert!(s.down(Button::North) && !s.down(Button::West) && s.down(Button::Guide));

        // A writer gone is no error (a device that is unplugged is ENODEV,
        // which a pipe can't give).
        drop(w);
        assert!(d.read());
    }

    #[test]
    fn gamepad_many_events_at_once() {
        let (mut d, mut w) = device(&XPAD_AXES, &XPAD_BUTTONS, "Xbox Wireless Controller");
        // More than one read's worth: the last one wins.
        let events: Vec<_> = (0..200).map(|i| event(JS_EVENT_AXIS, 0, i * 100)).collect();
        send(&mut w, &events);
        assert!(d.read());
        assert!(close(d.reading().axis(Axis::LeftX), 19900.0 / 32767.0));
    }

    #[test]
    fn gamepad_dualsense_through_hid_playstation() {
        let buttons = [
            BTN_SOUTH, BTN_EAST, BTN_NORTH, BTN_WEST, BTN_TL, BTN_TR, BTN_TL2, BTN_TR2, BTN_SELECT,
            BTN_START, BTN_MODE, BTN_THUMBL, BTN_THUMBR,
        ];
        let (mut d, mut w) = device(&XPAD_AXES, &buttons, "DualSense Wireless Controller");
        assert_eq!(d.family, Family::PlayStation);
        // Triangle is BTN_NORTH here, and L2 is its analog axis only.
        send(
            &mut w,
            &[
                event(JS_EVENT_BUTTON, 2, 1),
                event(JS_EVENT_BUTTON, 6, 1),
                event(JS_EVENT_AXIS, 2, -30000),
            ],
        );
        assert!(d.read());
        let s = d.reading();
        assert!(s.down(Button::North) && !s.down(Button::West));
        assert!(s.axis(Axis::LeftTrigger) < 0.1);
    }

    #[test]
    fn gamepad_switch_pro_digital_triggers() {
        let axes = [ABS_X, ABS_Y, ABS_RX, ABS_RY, ABS_HAT0X, ABS_HAT0Y];
        let buttons = [
            BTN_SOUTH,
            BTN_EAST,
            BTN_NORTH,
            BTN_WEST,
            BTN_TL,
            BTN_TR,
            BTN_TL2,
            BTN_TR2,
            BTN_SELECT,
            BTN_START,
            BTN_MODE,
            BTN_THUMBL,
            BTN_THUMBR,
            BTN_DPAD_UP,
            BTN_DPAD_DOWN,
        ];
        let (mut d, mut w) = device(&axes, &buttons, "Nintendo Switch Pro Controller");
        assert_eq!(d.family, Family::Nintendo);
        // ZR, and the d-pad as buttons.
        send(&mut w, &[event(JS_EVENT_BUTTON, 7, 1), event(JS_EVENT_BUTTON, 14, 1)]);
        assert!(d.read());
        let s = d.reading();
        assert_eq!(s.axis(Axis::RightTrigger), 1.0);
        assert_eq!(s.axis(Axis::LeftTrigger), 0.0);
        assert!(s.down(Button::DpadDown));
    }

    #[test]
    fn gamepad_generic_hid_controller() {
        // A cheap USB controller: the right stick on Z/RZ, joystick buttons.
        let axes = [ABS_X, ABS_Y, ABS_Z, ABS_RZ, ABS_HAT0X, ABS_HAT0Y];
        let buttons: Vec<u16> = (0..12).map(|i| BTN_JOYSTICK + i).collect();
        let layout = Layout::new(&axes, &buttons, false);
        assert!(layout.is_controller());
        assert_eq!(layout.axes[2], Role::Stick(Axis::RightX, false));
        assert_eq!(layout.axes[3], Role::Stick(Axis::RightY, true));
        assert_eq!(layout.buttons[0], Press::Button(Button::South));
        assert_eq!(layout.buttons[3], Press::Button(Button::North));
        assert_eq!(layout.buttons[6], Press::Trigger(Axis::LeftTrigger));
        assert_eq!(layout.buttons[9], Press::Button(Button::Start));

        // An Xbox controller over Bluetooth (generic HID driver): the right
        // stick on Z/RZ too, the triggers on brake and gas.
        let axes = [ABS_X, ABS_Y, ABS_Z, ABS_RZ, ABS_GAS, ABS_BRAKE, ABS_HAT0X, ABS_HAT0Y];
        let layout = Layout::new(&axes, &[BTN_SOUTH, BTN_NORTH], true);
        assert_eq!(layout.axes[3], Role::Stick(Axis::RightY, true));
        assert_eq!(layout.axes[4], Role::Trigger(Axis::RightTrigger));
        assert_eq!(layout.axes[5], Role::Trigger(Axis::LeftTrigger));
        assert_eq!(layout.buttons[1], Press::Button(Button::West));
    }

    #[test]
    fn gamepad_devices_that_are_no_controllers() {
        // Motion sensors: axes only.
        assert!(
            !Layout::new(&[ABS_X, ABS_Y, ABS_Z, ABS_RX, ABS_RY, ABS_RZ], &[], false)
                .is_controller()
        );
        // Buttons but no stick.
        assert!(!Layout::new(&[ABS_GAS], &[BTN_SOUTH], false).is_controller());
        assert!(!Layout::new(&[], &[], false).is_controller());
    }

    #[test]
    fn gamepad_linux_heuristics() {
        assert!(xbox_layout("Microsoft X-Box 360 pad", None, None));
        assert!(xbox_layout("Logitech Gamepad F310", Some("xpad"), Some(0x046d)));
        assert!(xbox_layout("Xbox Wireless Controller", Some("microsoft"), Some(MICROSOFT)));
        assert!(xbox_layout("Steam Deck", Some("steam"), Some(VALVE)));
        assert!(!xbox_layout("Sony Interactive Entertainment Wireless Controller", None, None));
        assert!(!xbox_layout("Wireless Controller", Some("playstation"), Some(SONY)));
        assert!(!xbox_layout("Nintendo Switch Pro Controller", Some("nintendo"), Some(NINTENDO)));
        assert!(!xbox_layout("USB Gamepad", Some("hid-generic"), Some(0x0079)));

        assert_eq!(
            family("Wireless Controller", Some("playstation"), Some(SONY)),
            Family::PlayStation
        );
        assert_eq!(family("Pro Controller", None, Some(NINTENDO)), Family::Nintendo);
        assert_eq!(family("Logitech Gamepad F310", Some("xpad"), Some(0x046d)), Family::Xbox);
        assert_eq!(family("USB Gamepad", Some("hid-generic"), Some(0x0079)), Family::Generic);
        assert_eq!(family("Steam Deck", Some("steam"), Some(VALVE)), Family::Xbox);
        assert_eq!(family("Generic X-Box pad", None, None), Family::Xbox);
    }

    /// The real thing with whatever the machine has (no `/dev/input` at all
    /// where the tests run): nothing found, nothing breaks.
    #[test]
    fn gamepad_linux_backend_without_devices() {
        let mut backend = Backend::new();
        for frame in 0..400 {
            let now = f64::from(frame) / 60.0;
            if backend.poll(now).is_some() {
                // A real controller on the machine running the tests.
                return;
            }
            assert_eq!(backend.name(), None);
            assert_eq!(backend.family(), Family::Generic);
        }
        assert!(backend.devices.is_empty());
        assert!(backend.next_scan > 6.0);
    }
}
