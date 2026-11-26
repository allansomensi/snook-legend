//! The interface's icons: one family (Phosphor, MIT, with the game's own
//! drawn in its style), in a font made by `tools/gen_icons.py`. They are
//! drawn as text, rasterised at the size they are shown, so they stay
//! sharp at any interface size, and fade with the HUD.

use std::cell::OnceCell;
use std::mem::ManuallyDrop;

use macroquad::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Trophy,
    Person,
    People,
    Gear,
    /// Back to the default camera.
    CameraReset,
    Replay,
    Plus,
    Minus,
    Close,
    Back,
    Lock,
    Globe,
    /// Two arrows, one each way: switch between two things.
    Swap,
    Network,
    Robot,
    Medal,
    Star,
    Refresh,
    Table,
    Play,
    /// Watching a game.
    Eye,
    /// The shop (a shopping bag).
    Shop,
    /// A coin.
    Coin,
    /// The VIP crown.
    Crown,
    /// A wrapped present.
    Gift,
    /// A tick.
    Check,
    /// An hourglass: correspondence ("daily") games, a turn at a time.
    Hourglass,
    /// A person with a plus: ask to be friends.
    AddFriend,
    /// A pencil: edit.
    Pencil,
    /// A list (three lines with bullets): a log, the game's history.
    List,
    /// An open book: the rules.
    Book,
    /// Two bars: pause.
    Pause,
    /// Four corners: a clean view (the interface hidden).
    Clean,
    /// Chevrons: the previous and the next.
    ChevronLeft,
    ChevronRight,
    /// A person with a minus: stop being friends.
    RemoveFriend,
    /// A circle struck through: block a player.
    Block,
    /// A small flag on its pole: report a player.
    Flag,
    /// A door with an arrow going out of it: sign out of the account (an
    /// open door is quitting the game).
    SignOut,
    /// The settings' tabs: graphics, the game, controls and sound.
    Monitor,
    Sliders,
    Mouse,
    Speaker,
    /// No connection to the server.
    WifiSlash,
    /// An open door: quit the game (said at a glance, where a power sign
    /// read as "turn off").
    DoorOpen,
    /// A treasure chest: the collection (everything the player has).
    Chest,
}

thread_local! {
    // Never dropped: a font freed after the window (at exit) would free its
    // texture in a dead graphics context.
    static FONT: OnceCell<ManuallyDrop<Font>> = const { OnceCell::new() };
}

const FONT_BYTES: &[u8] = include_bytes!("../assets/icons.ttf");

/// How much of `s` the icon's square takes (Phosphor's shapes nearly fill it).
const SCALE: f32 = 0.92;

/// The felt under [`Icon::Table`]'s rails.
const TABLE_FELT: char = '\u{E200}';
const FELT: Color = Color::new(0.13, 0.5, 0.3, 1.0);

fn glyph(icon: Icon, filled: bool) -> char {
    let base = if filled { 0xE100 } else { 0xE000 };
    char::from_u32(base + icon as u32).unwrap_or(' ')
}

/// Draws `icon` centred on `c`, about `s` units across, in the outline style.
pub fn draw(icon: Icon, c: Vec2, s: f32, color: Color) {
    draw_style(icon, c, s, color, false);
}

/// Draws `icon` filled (a selected or solid look).
pub fn draw_filled(icon: Icon, c: Vec2, s: f32, color: Color) {
    draw_style(icon, c, s, color, true);
}

fn draw_style(icon: Icon, c: Vec2, s: f32, color: Color, filled: bool) {
    if icon == Icon::Table {
        draw_glyph(TABLE_FELT, c, s, Color::new(FELT.r, FELT.g, FELT.b, color.a));
    }
    draw_glyph(glyph(icon, filled), c, s, color);
}

fn draw_glyph(ch: char, c: Vec2, s: f32, color: Color) {
    let k = crate::ui::ui_scale();
    // Whole pixels: the glyph's square on the pixel grid.
    let px = (s * SCALE * k).round().max(1.0);
    let em = px / k;
    let x = ((c.x - em / 2.0) * k).round() / k;
    // The square spans 0.8 em above the baseline and 0.2 below.
    let baseline = ((c.y - em / 2.0) * k).round() / k + 0.8 * em;
    let mut buf = [0u8; 4];
    FONT.with(|f| {
        let font = f.get_or_init(|| {
            ManuallyDrop::new(load_ttf_font_from_bytes(FONT_BYTES).expect("the icon font"))
        });
        draw_text_ex(
            ch.encode_utf8(&mut buf),
            x,
            baseline,
            TextParams {
                font: Some(&**font),
                font_size: px as u16,
                font_scale: 1.0 / k,
                color: crate::paint::fade(color),
                ..Default::default()
            },
        );
    });
}

#[cfg(test)]
mod tests {
    /// The font has both styles of every icon (tools/gen_icons.py made it
    /// from this enum: run it again after adding one).
    #[test]
    fn every_icon_is_in_the_font() {
        let face = rustybuzz::Face::from_slice(super::FONT_BYTES, 0).unwrap();
        for k in 0..=super::Icon::Chest as u32 {
            for base in [0xE000, 0xE100] {
                let c = char::from_u32(base + k).unwrap();
                assert!(face.glyph_index(c).is_some(), "icon {k} (U+{:04X})", base + k);
            }
        }
        assert!(face.glyph_index(super::TABLE_FELT).is_some());
    }
}
