//! Systems without controller support: never a controller.

use super::Family;
use super::tracker::{Deadzones, State};

pub const DEADZONES: Deadzones = Deadzones::STANDARD;

pub struct Backend;

impl Backend {
    pub fn new() -> Backend {
        Backend
    }

    pub fn poll(&mut self, _now: f64) -> Option<(u64, State)> {
        None
    }

    pub fn name(&self) -> Option<String> {
        None
    }

    pub fn family(&self) -> Family {
        Family::Generic
    }

    pub fn rumble(&mut self, _strength: f32) {}
}
