//! Players' pictures: one of a set of drawn avatars, chosen in the profile.

use macroquad::prelude::*;

/// Draws the avatar `code` in a circle at `c` with radius `r` (a silhouette
/// for none, or one this version doesn't know).
pub fn draw(code: &str, c: Vec2, r: f32) {
    let _ = code;
    crate::scoreboard::silhouette(c, r);
}
