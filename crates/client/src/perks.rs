//! Small things the settings change all over the game: the VIP's colour of
//! the aim line and the ghost ball, and whether reactions are shown. Set
//! once a frame (see [`update`]) and read where things are drawn.

use std::cell::Cell;

use macroquad::prelude::Color;

/// The colours a VIP may give the aim line and the ghost ball (codes kept in
/// the settings; the first is the default white, for everyone).
pub const AIM_COLORS: [(&str, [u8; 3]); 9] = [
    ("white", [255, 255, 255]),
    ("gold", [255, 206, 84]),
    ("rose", [255, 120, 170]),
    ("ruby", [255, 70, 80]),
    ("amber", [255, 150, 40]),
    ("lime", [150, 255, 90]),
    ("mint", [80, 240, 190]),
    ("sky", [90, 200, 255]),
    ("violet", [190, 130, 255]),
];

thread_local! {
    static AIM: Cell<[u8; 3]> = const { Cell::new([255, 255, 255]) };
    static REACTIONS: Cell<bool> = const { Cell::new(true) };
    static VIP: Cell<bool> = const { Cell::new(false) };
}

/// The colour of a code of [`AIM_COLORS`] (white when unknown).
pub fn aim_rgb(code: &str) -> [u8; 3] {
    AIM_COLORS.iter().find(|(c, _)| *c == code).map_or([255, 255, 255], |(_, rgb)| *rgb)
}

/// What the settings say, and whether the player is VIP now (the aim's
/// colour is VIP's: it goes back to white when VIP ends).
pub fn update(aim_color: &str, show_reactions: bool) {
    let vip = VIP.with(Cell::get);
    AIM.with(|a| a.set(if vip { aim_rgb(aim_color) } else { [255, 255, 255] }));
    REACTIONS.with(|r| r.set(show_reactions));
}

/// The player is VIP (as their wallet says).
pub fn set_vip(vip: bool) {
    VIP.with(|v| v.set(vip));
}

pub fn vip() -> bool {
    VIP.with(Cell::get)
}

/// The aim line's and the ghost ball's colour, with `alpha`.
pub fn aim(alpha: f32) -> Color {
    let c = AIM.with(Cell::get);
    Color::new(c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, alpha)
}

/// Reactions are shown (the setting "Show reactions").
pub fn reactions_shown() -> bool {
    REACTIONS.with(Cell::get)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_aim_is_white_unless_vip() {
        set_vip(false);
        update("gold", true);
        assert_eq!(aim(1.0), Color::new(1.0, 1.0, 1.0, 1.0));
        set_vip(true);
        update("gold", false);
        assert!(aim(1.0).b < 0.5);
        assert!(!reactions_shown());
        update("nonsense", true);
        assert_eq!(aim(0.5), Color::new(1.0, 1.0, 1.0, 0.5));
        set_vip(false);
    }
}
