//! The part of the controller support that makes no system calls: one
//! reading of a controller ([`State`]), and what the game asks of the
//! readings frame after frame ([`Tracker`]): what went down or up, what
//! auto-repeats, which way the stick was flicked. Kept apart so it can be
//! tested without a controller.

use super::{Axis, Button};

/// How many [`Button`]s there are (bits of [`State::buttons`]).
pub const BUTTON_COUNT: usize = Button::ALL.len();

/// A trigger counts as a button once pressed this far (of 0..1)...
pub const TRIGGER_PRESS: f32 = 0.35;
/// ...and is let go below this: a little less, so a trigger held right at
/// the threshold doesn't flicker between pressed and released.
pub const TRIGGER_LET_GO: f32 = 0.25;

/// A held button (or stick) repeats this long after it went down...
pub const REPEAT_DELAY: f64 = 0.4;
/// ...and then this often, like a key held on a keyboard.
pub const REPEAT_EVERY: f64 = 0.09;

/// A stick pushed this far towards one side is one step in a menu...
pub const FLICK_PUSH: f32 = 0.6;
/// ...and the step is over once it is back under this, towards that side.
/// Less than [`FLICK_PUSH`], so a stick held near the threshold doesn't
/// step again and again, and a stick swept from one side to the next
/// lets go of the first side before it takes the second.
pub const FLICK_LET_GO: f32 = 0.4;

/// One reading of a controller, in the module's conventions whatever the
/// system: buttons by position, the sticks −1..1 with Y up-positive, the
/// triggers 0..1 (released at 0). Deadzones aren't applied yet: the
/// [`Tracker`] does that, the same way for every system.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct State {
    /// One bit per [`Button`] that is down (`1 << button as u32`).
    /// Backends leave the trigger bits alone: the tracker sets them from
    /// the trigger axes.
    pub buttons: u32,
    /// Indexed by [`Axis`].
    pub axes: [f32; 6],
}

impl State {
    pub fn down(&self, b: Button) -> bool {
        self.buttons & b.bit() != 0
    }

    pub fn set(&mut self, b: Button, down: bool) {
        if down {
            self.buttons |= b.bit();
        } else {
            self.buttons &= !b.bit();
        }
    }

    pub fn axis(&self, a: Axis) -> f32 {
        self.axes[a as usize]
    }

    pub fn set_axis(&mut self, a: Axis, value: f32) {
        self.axes[a as usize] = value;
    }

    /// The same reading with every axis a number in its range: a device
    /// that misbehaves (or a float that came back as NaN) must not reach
    /// the game as a stick pushed to infinity.
    fn cleaned(mut self) -> State {
        for (i, v) in self.axes.iter_mut().enumerate() {
            let low = if i < Axis::LeftTrigger as usize { -1.0 } else { 0.0 };
            *v = if v.is_nan() { 0.0 } else { v.clamp(low, 1.0) };
        }
        self
    }
}

/// How far a stick or trigger must move before it counts. Sticks never
/// rest exactly at the centre, and some drift: without a deadzone the cue
/// would creep while nobody touches the controller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Deadzones {
    /// The left and right sticks, as a radius (of 1).
    pub sticks: [f32; 2],
    /// The triggers (of 0..1).
    pub trigger: f32,
}

impl Deadzones {
    /// What suits most controllers. (XInput has its own.)
    #[cfg_attr(windows, allow(dead_code))]
    pub const STANDARD: Deadzones = Deadzones { sticks: [0.2, 0.2], trigger: 0.05 };
}

/// A stick with a radial deadzone: nothing within `deadzone` of the centre,
/// and beyond it the distance rescaled so that just past the deadzone is
/// about 0 and the edge is 1, in the direction the stick points. (A
/// deadzone per axis would snap diagonals to the axes; without the
/// rescaling the smallest push would jump straight to `deadzone`.) Square
/// gates that reach past 1 in the corners are cut back to 1.
pub fn radial(x: f32, y: f32, deadzone: f32) -> (f32, f32) {
    let length = (x * x + y * y).sqrt();
    if length <= deadzone || length.is_nan() {
        return (0.0, 0.0);
    }
    let scaled = ((length - deadzone) / (1.0 - deadzone)).min(1.0);
    (x / length * scaled, y / length * scaled)
}

/// A trigger with its deadzone, rescaled to 0..1 the same way.
pub fn trigger(value: f32, deadzone: f32) -> f32 {
    if value <= deadzone || value.is_nan() {
        return 0.0;
    }
    ((value - deadzone) / (1.0 - deadzone)).min(1.0)
}

/// Whether a controller was just picked up between two readings: a button
/// went down, or a stick or trigger left its deadzone. Merely being off
/// centre doesn't count, so a stick that drifts can't keep claiming the
/// game's attention.
pub fn woke(before: &State, after: &State, deadzones: &Deadzones) -> bool {
    let stick = |s: &State, i: usize| {
        let (x, y) = (s.axes[2 * i], s.axes[2 * i + 1]);
        radial(x, y, deadzones.sticks[i]) != (0.0, 0.0)
    };
    let pull = |s: &State, a: Axis| trigger(s.axis(a), deadzones.trigger) > 0.0;
    after.buttons & !before.buttons != 0
        || (0..2).any(|i| !stick(before, i) && stick(after, i))
        || [Axis::LeftTrigger, Axis::RightTrigger]
            .into_iter()
            .any(|a| !pull(before, a) && pull(after, a))
}

/// Which of the connected controllers the game follows: the one in use
/// stays in use while it is connected, unless another one is picked up
/// (see [`woke`]); with none in use, the first one.
#[derive(Debug, Default)]
pub struct Choice {
    current: Option<u64>,
}

impl Choice {
    /// Picks among `pads`, each a key that names one connection of one
    /// controller, with its previous and latest reading.
    pub fn pick<'a>(
        &mut self,
        pads: impl IntoIterator<Item = (u64, &'a State, &'a State)>,
        deadzones: &Deadzones,
    ) -> Option<u64> {
        let mut first = None;
        let mut still_there = false;
        let mut picked_up = None;
        for (key, before, after) in pads {
            first.get_or_insert(key);
            if Some(key) == self.current {
                still_there = true;
            } else if picked_up.is_none() && woke(before, after, deadzones) {
                picked_up = Some(key);
            }
        }
        self.current = picked_up.or(if still_there { self.current } else { first });
        self.current
    }

    pub fn current(&self) -> Option<u64> {
        self.current
    }
}

/// The stick used as arrow keys (see [`Tracker::flick`]).
#[derive(Debug, Clone, Copy, Default)]
struct Flick {
    /// The side the stick is held towards, in screen terms (y down).
    side: Option<(i32, i32)>,
    /// When it repeats next.
    next: f64,
    /// It stepped this frame.
    stepped: bool,
}

impl Flick {
    /// `x` and `y` are the stick with its deadzone, Y up-positive.
    fn update(&mut self, x: f32, y: f32, now: f64) {
        self.stepped = false;
        // Screen coordinates from here on: y grows downwards.
        let y = -y;
        if let Some((dx, dy)) = self.side
            && (x * dx as f32 + y * dy as f32) < FLICK_LET_GO
        {
            self.side = None;
        }
        match self.side {
            None => {
                let sides = [((1, 0), x), ((-1, 0), -x), ((0, 1), y), ((0, -1), -y)];
                let (side, push) =
                    sides.into_iter().fold(sides[0], |best, s| if s.1 > best.1 { s } else { best });
                if push >= FLICK_PUSH {
                    self.side = Some(side);
                    self.stepped = true;
                    self.next = now + REPEAT_DELAY;
                }
            }
            Some(_) if now >= self.next => {
                self.stepped = true;
                self.next = next_repeat(self.next, now);
            }
            Some(_) => {}
        }
    }

    fn step(&self) -> Option<(i32, i32)> {
        self.side.filter(|_| self.stepped)
    }
}

/// When a repeat that was due at `due` repeats next: one step on, or one
/// step from now if the game fell behind (a long frame mustn't be followed
/// by a burst of repeats to catch up).
fn next_repeat(due: f64, now: f64) -> f64 {
    let next = due + REPEAT_EVERY;
    if next <= now { now + REPEAT_EVERY } else { next }
}

/// What the game asks of a controller frame after frame, from its
/// successive readings ([`Tracker::update`], once a frame).
#[derive(Debug, Clone)]
pub struct Tracker {
    deadzones: Deadzones,
    /// The latest reading as the backend gave it (for [`woke`]).
    raw: State,
    /// The previous and latest readings, with the triggers as buttons.
    before: State,
    now: State,
    /// The sticks with their deadzones (left, right).
    sticks: [(f32, f32); 2],
    /// The triggers with their deadzone (left, right).
    triggers: [f32; 2],
    /// When each held button repeats next.
    next_repeat: [f64; BUTTON_COUNT],
    /// The buttons that went down or repeated this frame.
    repeats: u32,
    flicks: [Flick; 2],
    /// The controller was picked up this frame (see [`woke`]).
    woke: bool,
}

impl Tracker {
    pub fn new(deadzones: Deadzones) -> Tracker {
        Tracker {
            deadzones,
            raw: State::default(),
            before: State::default(),
            now: State::default(),
            sticks: [(0.0, 0.0); 2],
            triggers: [0.0; 2],
            next_repeat: [0.0; BUTTON_COUNT],
            repeats: 0,
            flicks: [Flick::default(); 2],
            woke: false,
        }
    }

    /// Forgets everything: the controller went away, or another one took
    /// its place. What was held on it is dropped without being reported as
    /// released (a pad whose battery dies mid-shot mustn't take the shot),
    /// and what is held on the next one is reported as pressed.
    pub fn forget(&mut self) {
        *self = Tracker::new(self.deadzones);
    }

    /// Takes the frame's reading (`now` in seconds, from any fixed start).
    pub fn update(&mut self, raw: State, now: f64) {
        let mut state = raw.cleaned();
        for (axis, button) in
            [(Axis::LeftTrigger, Button::LeftTrigger), (Axis::RightTrigger, Button::RightTrigger)]
        {
            let threshold = if self.now.down(button) { TRIGGER_LET_GO } else { TRIGGER_PRESS };
            state.set(button, state.axis(axis) >= threshold);
        }
        self.woke = woke(&self.raw, &state, &self.deadzones);
        self.raw = state;
        self.before = self.now;
        self.now = state;

        for (i, stick) in self.sticks.iter_mut().enumerate() {
            *stick = radial(state.axes[2 * i], state.axes[2 * i + 1], self.deadzones.sticks[i]);
        }
        self.triggers = [Axis::LeftTrigger, Axis::RightTrigger]
            .map(|a| trigger(state.axis(a), self.deadzones.trigger));

        let pressed = self.now.buttons & !self.before.buttons;
        self.repeats = 0;
        for (i, next) in self.next_repeat.iter_mut().enumerate() {
            let bit = 1 << i;
            if pressed & bit != 0 {
                self.repeats |= bit;
                *next = now + REPEAT_DELAY;
            } else if self.now.buttons & bit != 0 && now >= *next {
                self.repeats |= bit;
                *next = next_repeat(*next, now);
            }
        }

        for (flick, &(x, y)) in self.flicks.iter_mut().zip(&self.sticks) {
            flick.update(x, y, now);
        }
    }

    pub fn down(&self, b: Button) -> bool {
        self.now.down(b)
    }

    pub fn pressed(&self, b: Button) -> bool {
        self.now.down(b) && !self.before.down(b)
    }

    pub fn released(&self, b: Button) -> bool {
        !self.now.down(b) && self.before.down(b)
    }

    pub fn repeat(&self, b: Button) -> bool {
        self.repeats & b.bit() != 0
    }

    pub fn any_pressed(&self) -> bool {
        self.now.buttons & !self.before.buttons != 0
    }

    /// An axis with its deadzone.
    pub fn axis(&self, a: Axis) -> f32 {
        match a {
            Axis::LeftX => self.sticks[0].0,
            Axis::LeftY => self.sticks[0].1,
            Axis::RightX => self.sticks[1].0,
            Axis::RightY => self.sticks[1].1,
            Axis::LeftTrigger => self.triggers[0],
            Axis::RightTrigger => self.triggers[1],
        }
    }

    /// A stick (0 left, 1 right) with its deadzone, Y up-positive.
    pub fn stick(&self, i: usize) -> (f32, f32) {
        self.sticks[i]
    }

    /// The step the stick (0 left, 1 right) takes in a menu this frame,
    /// in screen terms (y down): when it is pushed towards a side, then
    /// repeating while it is held there, like [`Tracker::repeat`].
    pub fn flick(&self, i: usize) -> Option<(i32, i32)> {
        self.flicks[i].step()
    }

    /// The controller was picked up this frame: a button went down, or a
    /// stick or trigger left its deadzone.
    pub fn woke(&self) -> bool {
        self.woke
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The time of a frame, 128 frames a second.
    fn at(frame: u32) -> f64 {
        f64::from(frame) / 128.0
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn with(buttons: &[Button]) -> State {
        let mut s = State::default();
        for &b in buttons {
            s.set(b, true);
        }
        s
    }

    fn stick(x: f32, y: f32) -> State {
        let mut s = State::default();
        s.set_axis(Axis::LeftX, x);
        s.set_axis(Axis::LeftY, y);
        s
    }

    #[test]
    fn gamepad_edges() {
        let mut t = Tracker::new(Deadzones::STANDARD);
        t.update(State::default(), 0.0);
        assert!(!t.down(Button::South) && !t.pressed(Button::South) && !t.any_pressed());

        t.update(with(&[Button::South]), 0.016);
        assert!(t.down(Button::South) && t.pressed(Button::South) && t.any_pressed());
        assert!(!t.released(Button::South) && !t.pressed(Button::East));

        t.update(with(&[Button::South]), 0.032);
        assert!(t.down(Button::South) && !t.pressed(Button::South) && !t.any_pressed());

        t.update(State::default(), 0.048);
        assert!(!t.down(Button::South) && t.released(Button::South));

        t.update(State::default(), 0.064);
        assert!(!t.released(Button::South));
    }

    #[test]
    fn gamepad_repeat_timing() {
        let mut t = Tracker::new(Deadzones::STANDARD);
        let held = with(&[Button::DpadDown]);
        let mut fired = Vec::new();
        // 128 frames a second (exact in binary, so no frame is off by a
        // rounding) for a second.
        for frame in 0..=128 {
            t.update(held, at(frame));
            if t.repeat(Button::DpadDown) {
                fired.push(frame);
            }
            assert!(!t.repeat(Button::DpadUp));
        }
        // Down at 0, then 0.4 s later (frame 51.2), then every 0.09 s
        // (11.52 frames).
        assert_eq!(fired, [0, 52, 63, 75, 86, 98, 109, 121]);

        // Let go and pressed again: at once, and the delay starts over.
        t.update(State::default(), at(129));
        assert!(!t.repeat(Button::DpadDown));
        t.update(held, at(130));
        assert!(t.repeat(Button::DpadDown));
        t.update(held, at(170));
        assert!(!t.repeat(Button::DpadDown));
    }

    #[test]
    fn gamepad_repeat_after_a_long_frame_does_not_burst() {
        let mut t = Tracker::new(Deadzones::STANDARD);
        let held = with(&[Button::DpadLeft]);
        t.update(held, 0.0);
        // A two second hitch: one repeat, not twenty.
        t.update(held, 2.0);
        assert!(t.repeat(Button::DpadLeft));
        t.update(held, 2.01);
        assert!(!t.repeat(Button::DpadLeft));
        t.update(held, 2.1);
        assert!(t.repeat(Button::DpadLeft));
    }

    #[test]
    fn gamepad_flick_directions() {
        // Y is up-positive on the stick and down-positive in the step.
        for ((x, y), step) in [
            ((1.0, 0.0), (1, 0)),
            ((-1.0, 0.0), (-1, 0)),
            ((0.0, 1.0), (0, -1)),
            ((0.0, -1.0), (0, 1)),
        ] {
            let mut t = Tracker::new(Deadzones::STANDARD);
            t.update(State::default(), 0.0);
            assert_eq!(t.flick(0), None);
            t.update(stick(x, y), 0.01);
            assert_eq!(t.flick(0), Some(step), "stick at {x}, {y}");
            assert_eq!(t.flick(1), None);
            t.update(stick(x, y), 0.02);
            assert_eq!(t.flick(0), None);
        }

        // Diagonals take the stronger side.
        let mut t = Tracker::new(Deadzones::STANDARD);
        t.update(stick(0.5, -0.9), 0.0);
        assert_eq!(t.flick(0), Some((0, 1)));

        // A small push is no step.
        let mut t = Tracker::new(Deadzones::STANDARD);
        t.update(stick(0.5, 0.0), 0.0);
        assert_eq!(t.flick(0), None);
    }

    #[test]
    fn gamepad_flick_repeat_and_let_go() {
        let mut t = Tracker::new(Deadzones::STANDARD);
        let mut steps = Vec::new();
        for frame in 0..=80 {
            t.update(stick(1.0, 0.0), at(frame));
            if t.flick(0).is_some() {
                steps.push(frame);
            }
        }
        // Like a held button.
        assert_eq!(steps, [0, 52, 63, 75]);

        // Eased back but still towards the right: held, no new step.
        t.update(stick(0.7, 0.0), at(81));
        assert_eq!(t.flick(0), None);
        // Back near the centre: let go; pushed again, a step at once.
        t.update(stick(0.3, 0.0), at(82));
        assert_eq!(t.flick(0), None);
        t.update(stick(1.0, 0.0), at(83));
        assert_eq!(t.flick(0), Some((1, 0)));

        // Swept round from the right to down without passing the centre:
        // a step down as soon as it leaves the right.
        t.update(stick(0.7, -0.7), at(84));
        assert_eq!(t.flick(0), None);
        t.update(stick(0.3, -0.95), at(85));
        assert_eq!(t.flick(0), Some((0, 1)));
    }

    #[test]
    fn gamepad_radial_deadzone() {
        assert_eq!(radial(0.0, 0.0, 0.2), (0.0, 0.0));
        assert_eq!(radial(0.2, 0.0, 0.2), (0.0, 0.0));
        assert_eq!(radial(0.1, -0.15, 0.2), (0.0, 0.0));
        // Just past the deadzone is about nothing, not a jump to 0.2.
        let (x, y) = radial(0.21, 0.0, 0.2);
        assert!(x > 0.0 && x < 0.02 && y == 0.0);
        // Halfway between the deadzone and the edge is half.
        let (x, _) = radial(0.6, 0.0, 0.2);
        assert!(close(x, 0.5));
        assert_eq!(radial(1.0, 0.0, 0.2), (1.0, 0.0));
        assert_eq!(radial(0.0, -1.0, 0.2), (0.0, -1.0));
        // A square gate's corner is cut back to 1, the direction kept.
        let (x, y) = radial(1.0, 1.0, 0.2);
        assert!(close(x, std::f32::consts::FRAC_1_SQRT_2) && close(x, y));
        // Diagonals stay diagonal (no snapping to the axes).
        let (x, y) = radial(0.3, 0.3, 0.2);
        assert!(x > 0.0 && close(x, y));
        assert_eq!(radial(f32::NAN, 0.0, 0.2), (0.0, 0.0));
    }

    #[test]
    fn gamepad_trigger_deadzone() {
        assert_eq!(trigger(0.0, 0.05), 0.0);
        assert_eq!(trigger(0.05, 0.05), 0.0);
        assert!(close(trigger(0.525, 0.05), 0.5));
        assert_eq!(trigger(1.0, 0.05), 1.0);
        assert_eq!(trigger(f32::NAN, 0.05), 0.0);
    }

    #[test]
    fn gamepad_tracker_applies_deadzones() {
        let mut t = Tracker::new(Deadzones::STANDARD);
        let mut s = stick(0.1, 0.1);
        s.set_axis(Axis::RightX, -1.0);
        s.set_axis(Axis::LeftTrigger, 0.04);
        s.set_axis(Axis::RightTrigger, 1.0);
        t.update(s, 0.0);
        assert_eq!(t.stick(0), (0.0, 0.0));
        assert_eq!(t.axis(Axis::LeftX), 0.0);
        assert_eq!(t.axis(Axis::RightX), -1.0);
        assert_eq!(t.stick(1), (-1.0, 0.0));
        assert_eq!(t.axis(Axis::LeftTrigger), 0.0);
        assert_eq!(t.axis(Axis::RightTrigger), 1.0);
    }

    #[test]
    fn gamepad_trigger_as_button() {
        let mut t = Tracker::new(Deadzones::STANDARD);
        let pull = |v: f32| {
            let mut s = State::default();
            s.set_axis(Axis::RightTrigger, v);
            s
        };
        t.update(pull(0.34), 0.0);
        assert!(!t.down(Button::RightTrigger));
        t.update(pull(0.36), 0.01);
        assert!(t.down(Button::RightTrigger) && t.pressed(Button::RightTrigger));
        assert!(t.repeat(Button::RightTrigger) && t.any_pressed());
        assert!(!t.down(Button::LeftTrigger));
        // Eased back a little: still held.
        t.update(pull(0.3), 0.02);
        assert!(t.down(Button::RightTrigger) && !t.released(Button::RightTrigger));
        t.update(pull(0.2), 0.03);
        assert!(!t.down(Button::RightTrigger) && t.released(Button::RightTrigger));

        // A trigger bit from a backend means nothing without the axis.
        t.update(with(&[Button::LeftTrigger]), 0.04);
        assert!(!t.down(Button::LeftTrigger));
    }

    #[test]
    fn gamepad_forget_reports_no_release() {
        let mut t = Tracker::new(Deadzones::STANDARD);
        t.update(with(&[Button::South]), 0.0);
        t.forget();
        t.update(State::default(), 0.01);
        assert!(!t.released(Button::South) && !t.down(Button::South));

        // What is held on the next controller counts as pressed.
        t.update(with(&[Button::East]), 0.02);
        t.forget();
        t.update(with(&[Button::East]), 0.03);
        assert!(t.pressed(Button::East));
    }

    #[test]
    fn gamepad_cleans_bad_readings() {
        let mut t = Tracker::new(Deadzones::STANDARD);
        let s = State {
            buttons: 0,
            axes: [f32::NAN, f32::INFINITY, -7.0, 0.0, f32::NEG_INFINITY, 3.0],
        };
        t.update(s, 0.0);
        assert_eq!(t.stick(0), (0.0, 1.0));
        assert_eq!(t.stick(1), (-1.0, 0.0));
        assert_eq!(t.axis(Axis::LeftTrigger), 0.0);
        assert_eq!(t.axis(Axis::RightTrigger), 1.0);
    }

    #[test]
    fn gamepad_wakes_on_moves_not_on_drift() {
        let dz = Deadzones::STANDARD;
        let rest = State::default();
        assert!(!woke(&rest, &rest, &dz));
        assert!(woke(&rest, &with(&[Button::Start]), &dz));
        assert!(!woke(&with(&[Button::Start]), &with(&[Button::Start]), &dz));
        assert!(!woke(&rest, &stick(0.15, 0.0), &dz));
        assert!(woke(&rest, &stick(0.5, 0.0), &dz));
        // A stick that stays off centre (drift) doesn't wake it again.
        assert!(!woke(&stick(0.5, 0.0), &stick(0.6, 0.0), &dz));
        let mut pulled = State::default();
        pulled.set_axis(Axis::LeftTrigger, 0.2);
        assert!(woke(&rest, &pulled, &dz));

        let mut t = Tracker::new(dz);
        t.update(stick(0.1, 0.0), 0.0);
        assert!(!t.woke());
        t.update(stick(0.9, 0.0), 0.01);
        assert!(t.woke());
        t.update(stick(0.9, 0.0), 0.02);
        assert!(!t.woke());
    }

    #[test]
    fn gamepad_choice_follows_the_pad_in_use() {
        let dz = Deadzones::STANDARD;
        let rest = State::default();
        let start = with(&[Button::Start]);
        let mut choice = Choice::default();
        assert_eq!(choice.pick([], &dz), None);
        // The first one connected.
        assert_eq!(choice.pick([(7, &rest, &rest), (9, &rest, &rest)], &dz), Some(7));
        // Another one picked up takes over...
        assert_eq!(choice.pick([(7, &rest, &rest), (9, &rest, &start)], &dz), Some(9));
        // ...and keeps it while the first one sits there.
        assert_eq!(choice.pick([(7, &rest, &rest), (9, &start, &start)], &dz), Some(9));
        assert_eq!(choice.current(), Some(9));
        // Gone: back to the first one there is.
        assert_eq!(choice.pick([(7, &rest, &rest)], &dz), Some(7));
        assert_eq!(choice.pick([], &dz), None);
    }
}
