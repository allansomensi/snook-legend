//! Keyboard shortcuts the player can change (Settings → Controls).
//!
//! Every game action has up to two keys. The table in use is installed once a
//! frame from the settings ([`install`]) and looked up with [`down`],
//! [`pressed`] and [`released`] wherever the game used to test a fixed key.
//! Esc (menu, back, cancel) and Enter (confirm) are fixed and can't be bound.
//!
//! In `settings.ini` an action that differs from its defaults is one line,
//! `key.<action> = <key>, <key>` (`none` for no key), so the defaults of a
//! newer version reach everyone who never changed them. Anything unknown or
//! invalid falls back to the defaults.

use std::cell::Cell;

use macroquad::prelude::{KeyCode, is_key_down, is_key_pressed, is_key_released};

use crate::i18n::t;

/// Something the player does with a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    AimLeft,
    AimRight,
    /// Held: the keys and the mouse turn the cue very slowly.
    FineAim,
    /// Held to charge, released to shoot.
    Shoot,
    SpinUp,
    SpinDown,
    SpinLeft,
    SpinRight,
    ElevationUp,
    ElevationDown,
    /// Picks up the cue ball in hand (or puts it down).
    BallInHand,
    /// From the top view to 3D and back.
    ToggleView,
    ResetCamera,
    ZoomIn,
    ZoomOut,
    Replay,
    /// The list of controls.
    Help,
    Fullscreen,
}

pub const ACTION_COUNT: usize = 18;

impl Action {
    /// In the order the Controls tab lists them.
    pub const ALL: [Action; ACTION_COUNT] = [
        Action::AimLeft,
        Action::AimRight,
        Action::FineAim,
        Action::Shoot,
        Action::SpinUp,
        Action::SpinDown,
        Action::SpinLeft,
        Action::SpinRight,
        Action::ElevationUp,
        Action::ElevationDown,
        Action::BallInHand,
        Action::ToggleView,
        Action::ResetCamera,
        Action::ZoomIn,
        Action::ZoomOut,
        Action::Replay,
        Action::Help,
        Action::Fullscreen,
    ];

    fn index(self) -> usize {
        self as usize
    }

    /// The name in `settings.ini` (never changes).
    pub fn code(self) -> &'static str {
        match self {
            Action::AimLeft => "aim_left",
            Action::AimRight => "aim_right",
            Action::FineAim => "fine_aim",
            Action::Shoot => "shoot",
            Action::SpinUp => "spin_up",
            Action::SpinDown => "spin_down",
            Action::SpinLeft => "spin_left",
            Action::SpinRight => "spin_right",
            Action::ElevationUp => "elevation_up",
            Action::ElevationDown => "elevation_down",
            Action::BallInHand => "ball_in_hand",
            Action::ToggleView => "toggle_view",
            Action::ResetCamera => "reset_camera",
            Action::ZoomIn => "zoom_in",
            Action::ZoomOut => "zoom_out",
            Action::Replay => "replay",
            Action::Help => "help",
            Action::Fullscreen => "fullscreen",
        }
    }

    /// The name shown in the settings (English; translate with `t`).
    pub fn label(self) -> &'static str {
        match self {
            Action::AimLeft => "Aim left",
            Action::AimRight => "Aim right",
            Action::FineAim => "Fine aim (hold)",
            Action::Shoot => "Shoot (hold, release)",
            Action::SpinUp => "Top spin",
            Action::SpinDown => "Back spin",
            Action::SpinLeft => "Left spin",
            Action::SpinRight => "Right spin",
            Action::ElevationUp => "Raise the cue",
            Action::ElevationDown => "Lower the cue",
            Action::BallInHand => "Pick up the cue ball in hand",
            Action::ToggleView => "3D / top view",
            Action::ResetCamera => "Reset the camera",
            Action::ZoomIn => "Zoom in",
            Action::ZoomOut => "Zoom out",
            Action::Replay => "Replay the last shot",
            Action::Help => "List of controls",
            Action::Fullscreen => "Full screen",
        }
    }

    /// The keys a new player has.
    pub fn defaults(self) -> Slots {
        use KeyCode::*;
        match self {
            Action::AimLeft => [Some(A), Some(Left)],
            Action::AimRight => [Some(D), Some(Right)],
            Action::FineAim => [Some(LeftShift), Some(RightShift)],
            Action::Shoot => [Some(Space), None],
            Action::SpinUp => [Some(Up), None],
            Action::SpinDown => [Some(Down), None],
            Action::SpinLeft => [Some(Q), None],
            Action::SpinRight => [Some(E), None],
            Action::ElevationUp => [Some(W), None],
            Action::ElevationDown => [Some(S), None],
            Action::BallInHand => [Some(B), None],
            Action::ToggleView => [Some(Tab), Some(T)],
            Action::ResetCamera => [Some(C), None],
            Action::ZoomIn => [Some(Equal), Some(KpAdd)],
            Action::ZoomOut => [Some(Minus), Some(KpSubtract)],
            Action::Replay => [Some(R), None],
            Action::Help => [Some(H), Some(F1)],
            Action::Fullscreen => [Some(F11), None],
        }
    }

    fn parse(code: &str) -> Option<Action> {
        Action::ALL.into_iter().find(|a| a.code() == code)
    }
}

/// An action's keys: the main one and an alternative.
pub type Slots = [Option<KeyCode>; 2];

/// A key that can be bound: its code, its name in `settings.ini`, its label
/// on a key cap, and whether text fields use it (then it does nothing else
/// while the player types).
struct KeyInfo {
    key: KeyCode,
    code: &'static str,
    label: &'static str,
}

macro_rules! keys {
    ($($key:ident $code:literal $label:literal),* $(,)?) => {
        const KEYS: &[KeyInfo] = &[$(KeyInfo { key: KeyCode::$key, code: $code, label: $label }),*];
    };
}

keys! {
    A "a" "A", B "b" "B", C "c" "C", D "d" "D", E "e" "E", F "f" "F", G "g" "G",
    H "h" "H", I "i" "I", J "j" "J", K "k" "K", L "l" "L", M "m" "M", N "n" "N",
    O "o" "O", P "p" "P", Q "q" "Q", R "r" "R", S "s" "S", T "t" "T", U "u" "U",
    V "v" "V", W "w" "W", X "x" "X", Y "y" "Y", Z "z" "Z",
    Key0 "0" "0", Key1 "1" "1", Key2 "2" "2", Key3 "3" "3", Key4 "4" "4",
    Key5 "5" "5", Key6 "6" "6", Key7 "7" "7", Key8 "8" "8", Key9 "9" "9",
    F1 "f1" "F1", F2 "f2" "F2", F3 "f3" "F3", F4 "f4" "F4", F5 "f5" "F5", F6 "f6" "F6",
    F7 "f7" "F7", F8 "f8" "F8", F9 "f9" "F9", F10 "f10" "F10", F11 "f11" "F11", F12 "f12" "F12",
    Space "space" "Space", Tab "tab" "Tab",
    Left "left" "←", Right "right" "→", Up "up" "↑", Down "down" "↓",
    Home "home" "Home", End "end" "End", PageUp "page_up" "PgUp", PageDown "page_down" "PgDn",
    Insert "insert" "Ins",
    Minus "minus" "-", Equal "equal" "=", LeftBracket "left_bracket" "[",
    RightBracket "right_bracket" "]", Backslash "backslash" "\\", Semicolon "semicolon" ";",
    Apostrophe "apostrophe" "'", Comma "comma" ",", Period "period" ".", Slash "slash" "/",
    GraveAccent "grave" "`",
    Kp0 "kp_0" "Num 0", Kp1 "kp_1" "Num 1", Kp2 "kp_2" "Num 2", Kp3 "kp_3" "Num 3",
    Kp4 "kp_4" "Num 4", Kp5 "kp_5" "Num 5", Kp6 "kp_6" "Num 6", Kp7 "kp_7" "Num 7",
    Kp8 "kp_8" "Num 8", Kp9 "kp_9" "Num 9", KpAdd "kp_add" "Num +",
    KpSubtract "kp_subtract" "Num -", KpMultiply "kp_multiply" "Num *",
    KpDivide "kp_divide" "Num /", KpDecimal "kp_decimal" "Num .",
    LeftShift "left_shift" "Shift", RightShift "right_shift" "Right Shift",
    LeftControl "left_ctrl" "Ctrl", RightControl "right_ctrl" "Right Ctrl",
    LeftAlt "left_alt" "Alt", RightAlt "right_alt" "Right Alt",
}

/// Key names translated for the player (the rest read the same everywhere).
const WORDS: [&str; 9] =
    ["Space", "Right Shift", "Right Ctrl", "Right Alt", "Home", "End", "PgUp", "PgDn", "Ins"];

fn info(key: KeyCode) -> Option<&'static KeyInfo> {
    KEYS.iter().find(|k| k.key == key)
}

/// Whether `key` can be bound (Esc and Enter are fixed; Backspace and
/// Delete clear a key in the settings; the rest isn't on every keyboard).
pub fn bindable(key: KeyCode) -> bool {
    info(key).is_some()
}

/// Keys a text field uses: while the player types, they do nothing else.
fn types_text(key: KeyCode) -> bool {
    use KeyCode::*;
    let n = key as u32;
    (0x20..=0x60).contains(&n)
        || matches!(
            key,
            Tab | Left
                | Right
                | Up
                | Down
                | Home
                | End
                | Insert
                | Kp0
                | Kp1
                | Kp2
                | Kp3
                | Kp4
                | Kp5
                | Kp6
                | Kp7
                | Kp8
                | Kp9
                | KpAdd
                | KpSubtract
                | KpMultiply
                | KpDivide
                | KpDecimal
        )
}

/// The label on a key's cap, in the player's language.
pub fn key_label(key: KeyCode) -> &'static str {
    match info(key) {
        // Only the names that are words: a letter must never be looked up
        // ("W" is a translated abbreviation elsewhere).
        Some(k) if WORDS.contains(&k.label) => t(k.label),
        Some(k) => k.label,
        None => "?",
    }
}

fn key_code(key: KeyCode) -> &'static str {
    info(key).map_or("none", |k| k.code)
}

fn parse_key(s: &str) -> Option<KeyCode> {
    let s = s.trim().to_ascii_lowercase();
    KEYS.iter().find(|k| k.code == s).map(|k| k.key)
}

/// Every action's keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bindings {
    slots: [Slots; ACTION_COUNT],
}

impl Default for Bindings {
    fn default() -> Self {
        Bindings { slots: Action::ALL.map(Action::defaults) }
    }
}

/// What binding a key changed besides the slot it went to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conflict {
    /// No other action had the key.
    None,
    /// `other` had the key: it now has the key the slot had before
    /// (`given`, if there was one).
    Swapped { other: Action, given: Option<KeyCode> },
}

impl Bindings {
    pub fn get(&self, action: Action) -> Slots {
        self.slots[action.index()]
    }

    pub fn is_default(&self, action: Action) -> bool {
        self.get(action) == action.defaults()
    }

    pub fn reset(&mut self, action: Action) {
        // The defaults may hold keys another action was given since: those
        // go back to their own defaults as well, so no key is bound twice.
        for key in action.defaults().into_iter().flatten() {
            if let Some((other, slot)) = self.find(key)
                && other != action
            {
                self.slots[other.index()][slot] = None;
            }
        }
        self.slots[action.index()] = action.defaults();
    }

    /// The action and slot a key is bound to.
    pub fn find(&self, key: KeyCode) -> Option<(Action, usize)> {
        Action::ALL.into_iter().find_map(|a| {
            self.slots[a.index()].iter().position(|k| *k == Some(key)).map(|s| (a, s))
        })
    }

    /// Puts `key` in `action`'s `slot` (`None` clears it). A key bound
    /// elsewhere moves here, and the slot it leaves gets the key this slot
    /// had: a swap, so nothing is bound twice and no key is lost.
    pub fn bind(&mut self, action: Action, slot: usize, key: Option<KeyCode>) -> Conflict {
        let slot = slot.min(1);
        let old = self.slots[action.index()][slot];
        let mut conflict = Conflict::None;
        if let Some(k) = key
            && let Some((other, other_slot)) = self.find(k)
            && (other, other_slot) != (action, slot)
        {
            self.slots[other.index()][other_slot] = old;
            if other != action {
                conflict = Conflict::Swapped { other, given: old };
            }
        }
        self.slots[action.index()][slot] = key;
        conflict
    }

    /// The action's keys for people: "A / ←" ("" when it has none).
    pub fn label(&self, action: Action) -> String {
        let keys: Vec<&str> = self.get(action).into_iter().flatten().map(key_label).collect();
        keys.join(" / ")
    }

    /// The action's main key ("" when it has none).
    pub fn short(&self, action: Action) -> &'static str {
        self.get(action).into_iter().flatten().next().map_or("", key_label)
    }

    /// `settings.ini` lines for the actions that differ from the defaults.
    pub fn to_text(self) -> String {
        let mut out = String::new();
        for a in Action::ALL.into_iter().filter(|a| !self.is_default(*a)) {
            let keys: Vec<&str> =
                self.get(a).into_iter().map(|k| k.map_or("none", key_code)).collect();
            out.push_str(&format!("key.{} = {}\n", a.code(), keys.join(", ")));
        }
        out
    }

    /// Reads one `key.<action> = …` line (other keys are not ours: false).
    /// A line that can't be read leaves the action as it was.
    pub fn parse_line(&mut self, key: &str, value: &str) -> bool {
        let Some(code) = key.strip_prefix("key.") else { return false };
        let Some(action) = Action::parse(code.trim()) else { return true };
        let mut slots: Slots = [None, None];
        let parts: Vec<&str> = value.split(',').map(str::trim).collect();
        if parts.is_empty() || parts.len() > 2 {
            return true;
        }
        for (k, part) in parts.iter().enumerate() {
            if part.eq_ignore_ascii_case("none") || part.is_empty() {
                continue;
            }
            match parse_key(part) {
                Some(key) => slots[k] = Some(key),
                // An unknown key: the whole action keeps its defaults.
                None => return true,
            }
        }
        if slots[0] == slots[1] {
            slots[1] = None;
        }
        self.slots[action.index()] = slots;
        true
    }

    /// After reading a file: a key bound twice (edited by hand) stays with
    /// the first action that has it.
    pub fn dedup(&mut self) {
        let mut seen: Vec<KeyCode> = Vec::new();
        for slots in &mut self.slots {
            for slot in slots.iter_mut() {
                if let Some(k) = *slot {
                    if seen.contains(&k) {
                        *slot = None;
                    } else {
                        seen.push(k);
                    }
                }
            }
        }
    }
}

thread_local! {
    static CURRENT: Cell<Bindings> = Cell::new(Bindings::default());
    /// The settings are waiting for a key to bind: no shortcut works.
    static CAPTURING: Cell<bool> = const { Cell::new(false) };
    /// The window lost the focus this frame (see [`focus_lost`]).
    static FOCUS_LOST: Cell<bool> = const { Cell::new(false) };
    /// The game's own place in the queue of window events.
    static SUBSCRIBER: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Makes `bindings` the keys in use (once a frame, from the settings), and
/// notes whether the window lost the focus since the last frame.
pub fn install(bindings: &Bindings) {
    CURRENT.with(|c| c.set(*bindings));
    struct Watch(bool);
    impl macroquad::miniquad::EventHandler for Watch {
        fn update(&mut self) {}
        fn draw(&mut self) {}
        fn window_minimized_event(&mut self) {
            self.0 = true;
        }
    }
    let id = SUBSCRIBER
        .with(|s| *s.get().get_or_insert_with(macroquad::input::utils::register_input_subscriber));
    SUBSCRIBER.with(|s| s.set(Some(id)));
    let mut watch = Watch(false);
    macroquad::input::utils::repeat_all_miniquad_input(&mut watch, id);
    FOCUS_LOST.with(|f| f.set(watch.0));
}

/// The window lost the focus (another window came to the front, or it was
/// minimised): every key held is then reported as let go, which is no
/// release of the player's.
pub fn focus_lost() -> bool {
    FOCUS_LOST.with(Cell::get)
}

/// The keys in use.
pub fn current() -> Bindings {
    CURRENT.with(Cell::get)
}

/// While the settings wait for a key, shortcuts do nothing.
pub fn set_capturing(on: bool) {
    CAPTURING.with(|c| c.set(on));
}

pub fn capturing() -> bool {
    CAPTURING.with(Cell::get)
}

fn any(action: Action, test: fn(KeyCode) -> bool) -> bool {
    !capturing() && current().get(action).into_iter().flatten().any(test)
}

/// One of the action's keys is held.
pub fn down(action: Action) -> bool {
    any(action, is_key_down)
}

/// One of the action's keys went down this frame.
pub fn pressed(action: Action) -> bool {
    any(action, is_key_pressed)
}

/// One of the action's keys was let go this frame.
pub fn released(action: Action) -> bool {
    any(action, is_key_released)
}

/// [`pressed`], for a shortcut that works everywhere: while a text field
/// has the keyboard, only keys that don't type count (F11 does, a letter
/// doesn't).
pub fn pressed_anywhere(action: Action, typing: bool) -> bool {
    !capturing()
        && current()
            .get(action)
            .into_iter()
            .flatten()
            .any(|k| !(typing && types_text(k)) && is_key_pressed(k))
}

/// The action's main key in use ("" when it has none).
pub fn short(action: Action) -> &'static str {
    current().short(action)
}

/// A tooltip with the action's key after it: "Watch the last shot again (R)"
/// (or just the text when the action has no key).
pub fn tip(text: &str, action: Action) -> String {
    match short(action) {
        "" => text.to_string(),
        key => format!("{text} ({key})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_bind_each_key_once() {
        let b = Bindings::default();
        let mut keys: Vec<KeyCode> = Action::ALL.iter().flat_map(|a| b.get(*a)).flatten().collect();
        let n = keys.len();
        keys.sort_by_key(|k| *k as u32);
        keys.dedup();
        assert_eq!(keys.len(), n, "a default key is bound twice");
        for k in keys {
            assert!(bindable(k), "{k:?} has no name");
        }
        assert_eq!(Action::ALL.len(), ACTION_COUNT);
        for (i, a) in Action::ALL.into_iter().enumerate() {
            assert_eq!(a.index(), i, "ALL is in declaration order");
        }
        // The old fixed keys.
        assert_eq!(b.find(KeyCode::Space), Some((Action::Shoot, 0)));
        assert_eq!(b.find(KeyCode::C), Some((Action::ResetCamera, 0)));
        assert_eq!(b.find(KeyCode::F1), Some((Action::Help, 1)));
        assert_eq!(b.find(KeyCode::Escape), None);
        assert!(!bindable(KeyCode::Escape) && !bindable(KeyCode::Enter));
    }

    #[test]
    fn binding_a_used_key_swaps() {
        let mut b = Bindings::default();
        // W (raise the cue) on aim left's main slot: raising gets A.
        let c = b.bind(Action::AimLeft, 0, Some(KeyCode::W));
        assert_eq!(c, Conflict::Swapped { other: Action::ElevationUp, given: Some(KeyCode::A) });
        assert_eq!(b.get(Action::AimLeft), [Some(KeyCode::W), Some(KeyCode::Left)]);
        assert_eq!(b.get(Action::ElevationUp), [Some(KeyCode::A), None]);
        // Into an empty slot: the other action loses the key.
        let c = b.bind(Action::Shoot, 1, Some(KeyCode::R));
        assert_eq!(c, Conflict::Swapped { other: Action::Replay, given: None });
        assert_eq!(b.get(Action::Replay), [None, None]);
        // Within one action: the two slots trade places, no conflict.
        let c = b.bind(Action::AimLeft, 1, Some(KeyCode::W));
        assert_eq!(c, Conflict::None);
        assert_eq!(b.get(Action::AimLeft), [Some(KeyCode::Left), Some(KeyCode::W)]);
        // The same key again changes nothing.
        let before = b;
        assert_eq!(b.bind(Action::AimLeft, 1, Some(KeyCode::W)), Conflict::None);
        assert_eq!(b, before);
        // A free key, and clearing.
        assert_eq!(b.bind(Action::Replay, 0, Some(KeyCode::F5)), Conflict::None);
        assert_eq!(b.bind(Action::Replay, 0, None), Conflict::None);
        assert_eq!(b.get(Action::Replay), [None, None]);
        assert_eq!(b.label(Action::Replay), "");
        assert_eq!(b.label(Action::AimLeft), "← / W");
    }

    #[test]
    fn reset_takes_the_defaults_back() {
        let mut b = Bindings::default();
        b.bind(Action::Replay, 0, Some(KeyCode::C));
        assert_eq!(b.get(Action::ResetCamera), [Some(KeyCode::R), None]);
        b.reset(Action::ResetCamera);
        assert_eq!(b.get(Action::ResetCamera), [Some(KeyCode::C), None]);
        // Replay lost C to the camera; its own default R is free again.
        assert_eq!(b.get(Action::Replay), [None, None]);
        b.reset(Action::Replay);
        assert_eq!(b, Bindings::default());
    }

    #[test]
    fn text_round_trip_and_bad_lines() {
        let mut b = Bindings::default();
        assert_eq!(b.to_text(), "", "defaults are not written");
        b.bind(Action::Shoot, 0, Some(KeyCode::Kp0));
        b.bind(Action::Shoot, 1, Some(KeyCode::Space));
        b.bind(Action::Help, 1, None);
        b.bind(Action::AimLeft, 0, Some(KeyCode::J));
        let text = b.to_text();
        assert!(text.contains("key.shoot = kp_0, space\n"), "{text}");
        assert!(text.contains("key.help = h, none\n"), "{text}");
        let mut c = Bindings::default();
        for line in text.lines() {
            let (k, v) = line.split_once('=').unwrap();
            assert!(c.parse_line(k.trim(), v.trim()));
        }
        c.dedup();
        assert_eq!(c, b);

        // Unknown actions and keys, too many keys: the defaults stay.
        let mut d = Bindings::default();
        assert!(!d.parse_line("volume", "1"));
        assert!(d.parse_line("key.teleport", "x"));
        assert!(d.parse_line("key.shoot", "space, hyperdrive"));
        assert!(d.parse_line("key.replay", "a, b, c"));
        assert!(d.parse_line("key.help", " F2 ,  F2 "));
        assert_eq!(d.get(Action::Shoot), Action::Shoot.defaults());
        assert_eq!(d.get(Action::Replay), Action::Replay.defaults());
        assert_eq!(d.get(Action::Help), [Some(KeyCode::F2), None]);
        // A key twice in the file stays with the first action.
        assert!(d.parse_line("key.replay", "a"));
        d.dedup();
        assert_eq!(d.get(Action::AimLeft), [Some(KeyCode::A), Some(KeyCode::Left)]);
        assert_eq!(d.get(Action::Replay), [None, None]);
    }

    #[test]
    fn typing_keys() {
        assert!(types_text(KeyCode::A) && types_text(KeyCode::Space) && types_text(KeyCode::Left));
        assert!(!types_text(KeyCode::F11) && !types_text(KeyCode::LeftShift));
        assert_eq!(tip("Replay", Action::Replay), "Replay (R)");
    }

    #[test]
    fn letters_are_never_translated() {
        // Other tests switch the language too: compare with the table.
        for k in KEYS.iter().filter(|k| !WORDS.contains(&k.label)) {
            assert_eq!(key_label(k.key), k.label);
        }
        crate::i18n::set_lang(crate::i18n::Lang::Spanish);
        assert_eq!(key_label(KeyCode::W), "W");
        assert_eq!(key_label(KeyCode::Space), "Espacio");
        crate::i18n::set_lang(crate::i18n::Lang::English);
    }
}
