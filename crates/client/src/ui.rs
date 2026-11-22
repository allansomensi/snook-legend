//! A small immediate-mode UI toolkit drawn with macroquad shapes and a TTF font.

use macroquad::prelude::*;

use crate::emoji::{self, Piece};
use crate::fonts::{Face, Run};
#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_circle_lines, draw_line, draw_rectangle, draw_triangle};

pub const GOLD: Color = Color::new(0.91, 0.75, 0.38, 1.0);
pub const CREAM: Color = Color::new(0.94, 0.89, 0.80, 1.0);
pub const MUTED: Color = Color::new(0.70, 0.64, 0.56, 1.0);
pub const PANEL: Color = Color::new(0.09, 0.055, 0.035, 0.96);
pub const BORDER: Color = Color::new(0.45, 0.31, 0.16, 1.0);
pub const ALERT: Color = Color::new(1.0, 0.56, 0.44, 1.0);
pub const GREEN: Color = Color::new(0.12, 0.45, 0.25, 1.0);

/// A row of [`Ui::chat_log`]: the author's name (on a message's first
/// row), the text, where it starts, the line it belongs to, whether it is
/// a centred notice, and whether a little room goes above it (a new author
/// or a notice).
struct LogRow {
    name: Option<(String, Color)>,
    text: String,
    indent: f32,
    index: usize,
    centred: bool,
    gap: bool,
}

/// An author's colour in the chat: one of a few soft, readable hues on the
/// dark panel, always the same for the same name.
pub fn author_color(name: &str) -> Color {
    const HUES: [(f32, f32, f32); 8] = [
        (0.55, 0.78, 0.98),
        (0.62, 0.88, 0.62),
        (0.96, 0.66, 0.58),
        (0.80, 0.68, 0.98),
        (0.98, 0.80, 0.52),
        (0.52, 0.86, 0.84),
        (0.96, 0.64, 0.82),
        (0.78, 0.86, 0.52),
    ];
    let h = name
        .to_lowercase()
        .bytes()
        .fold(2_166_136_261u32, |h, b| (h ^ b as u32).wrapping_mul(16_777_619));
    let (r, g, b) = HUES[h as usize % HUES.len()];
    Color::new(r, g, b, 1.0)
}

/// One line of a chat panel.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FeedLine {
    /// Who wrote it, with their rating (0: unknown) for the name's colour.
    pub from: Option<(String, i32)>,
    pub text: String,
    pub kind: FeedKind,
    /// The lounge chat line's number (0: not one, can't be deleted).
    pub id: u64,
    /// Written by an administrator (the name is red).
    pub admin: bool,
    /// Deleted, and by whom (the text is gone).
    pub deleted: Option<snook_legend_proto::Deleted>,
    /// This player's own line, shown the moment it is sent and dimmed until
    /// the server passes it on (it never waits for the round trip).
    pub pending: bool,
}

/// What the reader did with a line of [`Ui::chat_log`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatAction {
    /// Delete line `id` (`silent`: without a trace, administrators).
    Delete { id: u64, silent: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FeedKind {
    /// A message from a player.
    #[default]
    Player,
    /// From the staff.
    Announcement,
    /// Something that happened in the game.
    Event,
    /// Something important that happened in the game (a foul…).
    Alert,
}

/// The colour of a rating tier (used for player names everywhere).
pub fn tier_color(tier: snook_legend_proto::Tier) -> Color {
    use snook_legend_proto::Tier;
    match tier {
        Tier::Rookie => Color::new(0.72, 0.72, 0.74, 1.0),
        Tier::Amateur => Color::new(0.93, 0.89, 0.80, 1.0),
        Tier::Skilled => Color::new(0.45, 0.86, 0.52, 1.0),
        Tier::Expert => Color::new(0.42, 0.72, 1.0, 1.0),
        Tier::Master => Color::new(0.78, 0.56, 1.0, 1.0),
        Tier::Grandmaster => Color::new(1.0, 0.63, 0.22, 1.0),
        Tier::Legend => Color::new(1.0, 0.83, 0.30, 1.0),
    }
}

/// The colour of administrators' names everywhere; no one else's name is
/// red (no tier and no name style from the shop is).
pub const ADMIN_RED: Color = Color::new(1.0, 0.32, 0.27, 1.0);

/// Keyboard marks from here up belong to dialogs, not to text fields: a
/// dialog over a screen holds the keyboard with one while it is open, so the
/// screen's Enter and Esc wait.
pub const MODAL_FOCUS: u64 = 9_000;

/// The colour of a name: red for administrators, else the rating tier's.
pub fn name_color_of(admin: bool, rating: i32) -> Color {
    if admin { ADMIN_RED } else { tier_color(snook_legend_proto::Tier::of(rating)) }
}

/// The colour of a player's name.
pub fn name_color(p: &snook_legend_proto::PlayerInfo) -> Color {
    if p.role == snook_legend_proto::Role::Admin { ADMIN_RED } else { tier_color(p.tier()) }
}

/// A VIP name style (two colours), if the player has one (administrators
/// keep their red).
pub fn name_style(p: &snook_legend_proto::PlayerInfo) -> Option<(Color, Color)> {
    if !p.vip || p.role == snook_legend_proto::Role::Admin {
        return None;
    }
    match snook_legend_proto::shop::item(&p.name_style)?.look {
        snook_legend_proto::shop::Look::Name(Some((a, b))) => {
            Some((Color::from_rgba(a[0], a[1], a[2], 255), Color::from_rgba(b[0], b[1], b[2], 255)))
        }
        _ => None,
    }
}

/// A player's name in their colour (their VIP style, else their tier's).
pub fn draw_name(ui: &Ui, p: &snook_legend_proto::PlayerInfo, x: f32, y: f32, size: f32) {
    match name_style(p) {
        Some((a, b)) => ui.styled_text(&p.username, x, y, size, a, b, true),
        None => ui.text(&p.username, x, y, size, name_color(p), true),
    }
}

/// State of a [`Ui::drag_area`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Drag {
    Idle,
    Hover,
    /// Being dragged; the pointer position.
    Held(Vec2),
    /// The drag ended this frame, here.
    Released(Vec2),
}

static UI_SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f80_0000);

/// Size factor of the interface: 1.0 at 1280×760, larger on big screens,
/// smaller on small windows (times the player's own preference).
pub fn ui_scale() -> f32 {
    f32::from_bits(UI_SCALE.load(std::sync::atomic::Ordering::Relaxed))
}

/// Width of the screen in interface units.
pub fn vw() -> f32 {
    screen_width() / ui_scale()
}

/// `v` kept between `lo` and `hi`, `lo` winning when there is less room
/// than it asks for. Unlike `f32::clamp` it never stops the game: a window
/// made tiny, or minimised (reported as no pixels at all), leaves `hi`
/// below `lo`.
pub fn clamp_soft(v: f32, lo: f32, hi: f32) -> f32 {
    v.min(hi).max(lo)
}

/// Height of the screen in interface units.
pub fn vh() -> f32 {
    screen_height() / ui_scale()
}

/// Works out the interface scale for this frame. `preference` is the
/// player's setting (1.0 = automatic size).
/// The fewest interface units the window is ever tall (see [`update_scale`]).
const MIN_UI_HEIGHT: f32 = 560.0;

pub fn update_scale(preference: f32) {
    let (sw, sh) = (screen_width().max(1.0), screen_height().max(1.0));
    // The layout is designed for about 1180×720 interface units.
    // Small windows scale everything down rather than let it overflow.
    let auto = (sw / 1180.0).min(sh / 720.0).clamp(0.45, 2.6);
    // A larger interface stops where the screens no longer fit in the
    // window's height (the menu's last row went below it).
    let scale = (auto * preference.clamp(0.7, 1.5)).min(sh / MIN_UI_HEIGHT).max(0.42);
    UI_SCALE.store(scale.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// Draws the interface from here on in interface units (see [`vw`]).
pub fn begin_ui_camera() {
    let (w, h) = (vw(), vh());
    let cam = Camera2D {
        target: vec2(w / 2.0, h / 2.0),
        zoom: vec2(2.0 / w, 2.0 / h),
        ..Default::default()
    };
    set_camera(&cam);
}

/// Something drawn on top of the interface at the end of the frame.
struct Overlay {
    tooltip: bool,
    draw: Box<dyn FnOnce(&Ui)>,
}

/// Bold text this size and larger is drawn in the display face (Inter
/// Display, its letters a little closer, made for titles).
pub const DISPLAY_SIZE: f32 = 21.0;

pub struct Ui {
    regular: Font,
    bold: Font,
    /// Big bold titles (see [`DISPLAY_SIZE`]).
    display: Font,
    mouse: Vec2,
    pressed: bool,
    released: bool,
    down: bool,
    /// The widget being dragged (sliders, the spin ball).
    active: Option<u64>,
    /// Where the latest press of the button started.
    press_at: Vec2,
    /// The widget whose drag ended this frame.
    released_active: Option<u64>,
    /// The text field with keyboard focus.
    pub focus: Option<u64>,
    /// True if the pointer is over a UI element this frame (blocks game input).
    pub hovered: bool,
    hovered_next: bool,
    /// UI rectangles drawn last frame (so a click is recognised as UI even
    /// when the pointer just moved there).
    blocks: Vec<Rect>,
    prev_blocks: Vec<Rect>,
    pub wants_keyboard: bool,
    /// Caret position in the focused field, in characters.
    cursor: usize,
    /// The other end of the selection in the focused field (the selection
    /// runs from here to the caret).
    anchor: Option<usize>,
    /// The text fields drawn this frame and last frame, in order (Tab moves
    /// between them).
    fields: Vec<u64>,
    prev_fields: Vec<u64>,
    /// A text field took this frame's Esc (to let go of the keyboard): the
    /// screen around it must not take it too (see [`Ui::escape`]).
    escape_taken: bool,
    /// When the caret last moved or text was typed: it stays lit while
    /// typing, and blinks from there.
    caret_at: f64,
    /// Tab (+1) or Shift+Tab (-1) was pressed in a field: the focus moves on
    /// at the start of the next frame, once every field has been drawn.
    tab_move: Option<i32>,
    /// The last click in a field: (field, time, caret), for double clicks.
    last_click: Option<(u64, f64, usize)>,
    /// The editing key being held and when it repeats next.
    repeat: Option<(KeyCode, f64)>,
    /// The field `cursor` belongs to (focus can also be set from outside).
    cursor_owner: Option<u64>,
    /// Hover hints asked for this frame, with the opacity at the time. They
    /// are drawn by [`Ui::draw_overlays`] after everything else, so panels,
    /// chat and toasts drawn later in the frame never cover them.
    overlays: std::cell::RefCell<Vec<(Overlay, f32)>>,
    /// The panels drawn so far this frame: text that starts inside one never
    /// runs past its edge (see [`Ui::text`]).
    panels: std::cell::RefCell<Vec<Rect>>,
}

impl Ui {
    pub fn new() -> Self {
        let font = |bytes| load_ttf_font_from_bytes(bytes).expect("a bundled font");
        Ui {
            regular: font(crate::fonts::INTER_REGULAR),
            bold: font(crate::fonts::INTER_BOLD),
            display: font(crate::fonts::INTER_DISPLAY),
            mouse: Vec2::ZERO,
            pressed: false,
            released: false,
            down: false,
            active: None,
            press_at: Vec2::new(-1.0e6, -1.0e6),
            released_active: None,
            focus: None,
            hovered: false,
            hovered_next: false,
            blocks: Vec::new(),
            prev_blocks: Vec::new(),
            wants_keyboard: false,
            cursor: 0,
            anchor: None,
            fields: Vec::new(),
            prev_fields: Vec::new(),
            escape_taken: false,
            caret_at: 0.0,
            tab_move: None,
            last_click: None,
            repeat: None,
            cursor_owner: None,
            overlays: std::cell::RefCell::new(Vec::new()),
            panels: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// Call once at the start of each frame.
    pub fn begin(&mut self) {
        self.overlays.get_mut().clear();
        self.panels.get_mut().clear();
        self.mouse = Vec2::from(mouse_position()) / ui_scale();
        self.pressed = is_mouse_button_pressed(MouseButton::Left);
        self.released = is_mouse_button_released(MouseButton::Left);
        if self.pressed {
            self.press_at = self.mouse;
        }
        self.down = is_mouse_button_down(MouseButton::Left);
        self.prev_blocks = std::mem::take(&mut self.blocks);
        let over_ui = self.prev_blocks.iter().any(|r| r.contains(self.mouse));
        self.hovered = over_ui || self.hovered_next || self.active.is_some();
        self.hovered_next = false;
        self.released_active = None;
        if !self.down {
            self.released_active = self.active.take();
        }
        self.prev_fields = std::mem::take(&mut self.fields);
        self.escape_taken = false;
        // The keyboard stays with no field that is gone (a match that ended
        // while its chat had it): Esc and the shortcuts work again. The
        // dialogs' own marks (no field of their own) are left alone.
        if let Some(f) = self.focus
            && f < MODAL_FOCUS
            && !self.prev_fields.contains(&f)
        {
            self.focus = None;
            self.anchor = None;
        }
        // Tab: the next (or previous) field of last frame, in drawing order.
        if let Some(step) = self.tab_move.take()
            && let Some(f) = self.focus
            && let Some(i) = self.prev_fields.iter().position(|&x| x == f)
        {
            let n = self.prev_fields.len() as i32;
            let next = self.prev_fields[(i as i32 + step).rem_euclid(n) as usize];
            self.focus = Some(next);
            self.cursor_owner = None;
        }
        self.wants_keyboard = self.focus.is_some();
        if self.focus.is_none() {
            self.cursor_owner = None;
        }
    }

    /// Marks a rectangle as UI so clicks there don't reach the game.
    pub fn block(&mut self, r: Rect) {
        self.blocks.push(r);
        if r.contains(self.mouse) {
            self.hovered_next = true;
        }
    }

    /// The largest size up to `size` at which bold `text` fits in `width`.
    pub fn fit(&self, size: f32, text: &str, width: f32) -> f32 {
        self.fit_with(size, text, width, true, 10.0)
    }

    fn fit_with(&self, size: f32, text: &str, width: f32, bold: bool, min: f32) -> f32 {
        let mut s = size;
        while s > min && self.text_size(text, s, bold).x > width {
            s = (s - 1.0).max(min);
        }
        s
    }

    /// A label that fits `width`: smaller type first, then cut with "…".
    pub fn fitted(&self, text: &str, size: f32, width: f32, bold: bool) -> (String, f32) {
        self.fitted_with(text, size, width, bold, 10.0)
    }

    fn fitted_with(
        &self,
        text: &str,
        size: f32,
        width: f32,
        bold: bool,
        min: f32,
    ) -> (String, f32) {
        let size = self.fit_with(size, text, width, bold, min.min(size));
        if self.text_size(text, size, bold).x <= width {
            return (text.to_string(), size);
        }
        let mut s: String = text.to_string();
        while !s.is_empty() && self.text_size(&format!("{s}…"), size, bold).x > width {
            emoji::pop(&mut s);
        }
        (format!("{}…", s.trim_end()), size)
    }

    /// How much room `text` takes (emoji included: see [`crate::emoji`]).
    pub fn text_size(&self, text: &str, size: f32, bold: bool) -> Vec2 {
        let width = match emoji::pieces(text) {
            None => self.font_width(text, size, bold),
            Some(pieces) => pieces
                .iter()
                .map(|p| match p {
                    Piece::Text(t) => self.font_width(t, size, bold),
                    Piece::Emoji(_) => emoji::advance(size),
                })
                .sum(),
        };
        vec2(width, size)
    }

    /// The width of text without emoji, in the fonts (see [`crate::fonts`]).
    fn font_width(&self, text: &str, size: f32, bold: bool) -> f32 {
        if crate::fonts::simple(text) {
            return self.chars_width(text, None, size, bold);
        }
        crate::fonts::layout(text, bold)
            .runs
            .iter()
            .map(|run| match run {
                Run::Chars { face, text } => self.chars_width(text, Some(*face), size, bold),
                Run::Glyphs { glyphs, units_per_em, .. } => {
                    crate::fonts::glyphs_width(glyphs, *units_per_em, size)
                }
            })
            .sum()
    }

    /// The interface's own font for text of `size`: big bold titles in the
    /// display face, made for large sizes.
    fn main_font(&self, size: f32, bold: bool) -> &Font {
        if !bold {
            &self.regular
        } else if size >= DISPLAY_SIZE {
            &self.display
        } else {
            &self.bold
        }
    }

    /// The font that draws `face` character by character (None and `Main`:
    /// the interface's own).
    fn chars_font(&self, face: Option<Face>, bold: bool) -> Option<Font> {
        match face {
            None | Some(Face::Main) => None,
            Some(face) => crate::fonts::chars_font(face, bold),
        }
    }

    /// The width of characters drawn one by one in `face`.
    fn chars_width(&self, text: &str, face: Option<Face>, size: f32, bold: bool) -> f32 {
        let other = self.chars_font(face, bold);
        let font = other.as_ref().unwrap_or(self.main_font(size, bold));
        // Fonts are rasterised at the real pixel size, so text stays sharp at
        // any interface scale.
        let k = ui_scale();
        measure_text(text, Some(font), (size * k).round().max(1.0) as u16, 1.0 / k).width
    }

    /// The smallest panel drawn this frame that contains `p`.
    fn panel_at(&self, p: Vec2) -> Option<Rect> {
        self.panels
            .borrow()
            .iter()
            .filter(|r| r.contains(p))
            .min_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h)))
            .copied()
    }

    /// Text that would run `width` past the room it has is made smaller
    /// (down to 70%), then cut with "…". Returns what to draw, its size and
    /// how far down to move it so it stays centred on the same line.
    fn shrink_to(&self, text: &str, size: f32, room: f32, bold: bool) -> Option<(String, f32)> {
        if room <= 0.0 || self.text_size(text, size, bold).x <= room {
            return None;
        }
        Some(self.fitted_with(text, size, room, bold, (size * 0.7).max(9.0)))
    }

    /// Draws text with its top-left corner at (`x`, `y`). Text that starts
    /// inside a panel never runs past the panel's right edge.
    pub fn text(&self, text: &str, x: f32, y: f32, size: f32, color: Color, bold: bool) {
        if let Some(r) = self.panel_at(vec2(x + 0.5, y + size * 0.5)) {
            let room = r.x + r.w - 6.0 - x;
            if let Some((t, s)) = self.shrink_to(text, size, room, bold) {
                self.draw_text_raw(&t, x, y + (size - s) * 0.5, s, color, bold);
                return;
            }
        }
        self.draw_text_raw(text, x, y, size, color, bold);
    }

    /// Text squeezed (or stretched) sideways by `aspect` (a number seen at
    /// an angle on a ball).
    #[allow(clippy::too_many_arguments)]
    pub fn text_aspect(
        &self,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        color: Color,
        bold: bool,
        aspect: f32,
    ) {
        let font = if bold { &self.bold } else { &self.regular };
        draw_text_ex(
            text,
            x,
            y + size * 0.8,
            TextParams {
                font: Some(font),
                font_size: (size * ui_scale()).round().max(1.0) as u16,
                font_scale: 1.0 / ui_scale(),
                font_scale_aspect: aspect,
                color: crate::paint::fade(color),
                ..Default::default()
            },
        );
    }

    /// Draws text, emoji included, with its top-left corner at (`x`, `y`).
    fn draw_text_raw(&self, text: &str, x: f32, y: f32, size: f32, color: Color, bold: bool) {
        let Some(pieces) = emoji::pieces(text) else {
            self.draw_font_text(text, x, y, size, color, bold);
            return;
        };
        let mut x = x;
        for p in pieces {
            match p {
                Piece::Text(t) => {
                    self.draw_font_text(t, x, y, size, color, bold);
                    x += self.font_width(t, size, bold);
                }
                Piece::Emoji(cell) => {
                    emoji::draw(cell, x, y, size, color.a);
                    x += emoji::advance(size);
                }
            }
        }
    }

    /// Draws text without emoji, in the fonts (see [`crate::fonts`]).
    fn draw_font_text(&self, text: &str, x: f32, y: f32, size: f32, color: Color, bold: bool) {
        if crate::fonts::simple(text) {
            self.draw_chars(text, None, x, y, size, color, bold);
            return;
        }
        let x = (x * ui_scale()).round() / ui_scale();
        let baseline = ((y + size * 0.8) * ui_scale()).round() / ui_scale();
        let mut x = x;
        for run in &crate::fonts::layout(text, bold).runs {
            match run {
                Run::Chars { face, text } => {
                    self.draw_chars(text, Some(*face), x, y, size, color, bold);
                    x += self.chars_width(text, Some(*face), size, bold);
                }
                Run::Glyphs { face, glyphs, units_per_em } => {
                    let c = crate::paint::fade(color);
                    crate::fonts::draw_glyphs(
                        *face,
                        bold,
                        glyphs,
                        *units_per_em,
                        x,
                        baseline,
                        size,
                        c,
                    );
                    x += crate::fonts::glyphs_width(glyphs, *units_per_em, size);
                }
            }
        }
    }

    /// Draws characters one by one in `face`.
    #[allow(clippy::too_many_arguments)]
    fn draw_chars(
        &self,
        text: &str,
        face: Option<Face>,
        x: f32,
        y: f32,
        size: f32,
        color: Color,
        bold: bool,
    ) {
        let other = self.chars_font(face, bold);
        let font = other.as_ref().unwrap_or(self.main_font(size, bold));
        draw_text_ex(
            text,
            (x * ui_scale()).round() / ui_scale(),
            ((y + size * 0.8) * ui_scale()).round() / ui_scale(),
            TextParams {
                font: Some(font),
                font_size: (size * ui_scale()).round().max(1.0) as u16,
                font_scale: 1.0 / ui_scale(),
                color: crate::paint::fade(color),
                ..Default::default()
            },
        );
    }

    /// Text in two colours (a VIP name style): the second one as a soft
    /// glow around the first.
    #[allow(clippy::too_many_arguments)]
    pub fn styled_text(
        &self,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        a: Color,
        b: Color,
        bold: bool,
    ) {
        let glow = Color::new(b.r, b.g, b.b, 0.5);
        for (dx, dy) in [(-0.8, 0.0), (0.8, 0.0), (0.0, 0.8), (0.0, -0.8)] {
            self.text(text, x + dx, y + dy, size, glow, bold);
        }
        self.text(text, x, y, size, a, bold);
    }

    /// Text centred on `cx`; inside a panel it stays within the panel.
    pub fn text_centered(&self, text: &str, cx: f32, y: f32, size: f32, color: Color, bold: bool) {
        if let Some(r) = self.panel_at(vec2(cx, y + size * 0.5)) {
            let half = (cx - r.x).min(r.x + r.w - cx) - 8.0;
            if let Some((t, s)) = self.shrink_to(text, size, half * 2.0, bold) {
                let w = self.text_size(&t, s, bold).x;
                self.draw_text_raw(&t, cx - w / 2.0, y + (size - s) * 0.5, s, color, bold);
                return;
            }
        }
        let w = self.text_size(text, size, bold).x;
        self.draw_text_raw(text, cx - w / 2.0, y, size, color, bold);
    }

    pub fn panel(&mut self, r: Rect) {
        self.panels.get_mut().push(r);
        self.block(r);
        // Soft drop shadow, body, a sheen that fades out down the top and a
        // thin gold-brown edge.
        soft_shadow(r, 8.0, 1.6);
        // Opaque: a dialog over another panel must not let its text through.
        let body = Color::new(PANEL.r, PANEL.g, PANEL.b, 1.0);
        rrect_gradient(r, 8.0, shade(body, 0.03), shade(body, -0.25));
        let sheen = Rect::new(r.x + 1.0, r.y + 1.0, r.w - 2.0, (r.h * 0.5).min(64.0));
        rrect_gradient(
            sheen,
            7.0,
            Color::new(1.0, 0.85, 0.6, 0.05),
            Color::new(1.0, 0.85, 0.6, 0.0),
        );
        rrect_lines(r, 8.0, 1.2, BORDER);
    }

    /// A panel that lets what is under it show through by `clear` (0: as
    /// [`Ui::panel`]; 1: mostly glass), for one over the table that should
    /// not hide the game while nobody uses it.
    pub fn glass_panel(&mut self, r: Rect, clear: f32) {
        self.panels.get_mut().push(r);
        self.block(r);
        let clear = clear.clamp(0.0, 1.0);
        soft_shadow(r, 8.0, 1.6 * (1.0 - 0.6 * clear));
        let a = 1.0 - 0.3 * clear;
        let body = Color::new(PANEL.r, PANEL.g, PANEL.b, a);
        rrect_gradient(r, 8.0, shade(body, 0.03), shade(body, -0.25));
        rrect_lines(r, 8.0, 1.2, Color::new(BORDER.r, BORDER.g, BORDER.b, 1.0 - 0.4 * clear));
    }

    pub fn button(&mut self, r: Rect, label: &str, enabled: bool) -> bool {
        self.block(r);
        let hover = enabled && r.contains(self.mouse);
        if !enabled {
            // Flat and quiet: clearly not something to press.
            rrect(r, 6.0, Color::new(0.11, 0.08, 0.06, 0.9));
            rrect_lines(r, 6.0, 1.0, Color::new(BORDER.r, BORDER.g, BORDER.b, 0.45));
        } else {
            let bg = if hover && self.down {
                Color::new(0.46, 0.30, 0.14, 1.0)
            } else if hover {
                Color::new(0.40, 0.27, 0.13, 1.0)
            } else {
                Color::new(0.28, 0.18, 0.10, 1.0)
            };
            raised(r, 6.0, bg, if hover { GOLD } else { BORDER }, 1.2);
        }
        let (label, size) = self.fitted(label, (r.h * 0.45).clamp(13.0, 22.0), r.w - 12.0, true);
        let color = if enabled { CREAM } else { Color::new(MUTED.r, MUTED.g, MUTED.b, 0.6) };
        let y = r.y + (r.h - size) / 2.0 + if hover && self.down { 1.0 } else { 0.0 };
        self.text_centered(&label, r.x + r.w / 2.0, y, size, color, true);
        enabled && hover && self.released_in(r)
    }

    /// A button styled as the main call to action.
    pub fn primary_button(&mut self, r: Rect, label: &str) -> bool {
        self.block(r);
        let hover = r.contains(self.mouse);
        let bg = if hover && self.down {
            Color::new(0.10, 0.40, 0.22, 1.0)
        } else if hover {
            Color::new(0.18, 0.58, 0.33, 1.0)
        } else {
            GREEN
        };
        rrect(Rect::new(r.x, r.y + 2.0, r.w, r.h), 7.0, Color::new(0.0, 0.0, 0.0, 0.3));
        raised(r, 7.0, bg, if hover { GOLD } else { Color::new(0.3, 0.7, 0.45, 1.0) }, 1.2);
        let (label, size) = self.fitted(label, (r.h * 0.45).clamp(13.0, 24.0), r.w - 12.0, true);
        let y = r.y + (r.h - size) / 2.0 + if hover && self.down { 1.0 } else { 0.0 };
        self.text_centered(&label, r.x + r.w / 2.0, y, size, WHITE, true);
        hover && self.released_in(r)
    }

    pub fn checkbox(&mut self, x: f32, y: f32, label: &str, value: &mut bool) -> bool {
        let r = Rect::new(x, y, 20.0 + self.text_size(label, 15.0, false).x + 10.0, 20.0);
        self.block(r);
        let hover = r.contains(self.mouse);
        let b = Rect::new(x, y, 18.0, 18.0);
        rrect(b, 4.0, Color::new(0.06, 0.04, 0.03, 1.0));
        rrect_lines(b, 4.0, 1.2, if hover { GOLD } else { BORDER });
        if *value {
            // A check mark.
            draw_line(x + 4.0, y + 9.5, x + 7.5, y + 13.0, 2.2, GOLD);
            draw_line(x + 7.5, y + 13.0, x + 14.0, y + 5.0, 2.2, GOLD);
        }
        self.text(label, x + 26.0, y + 1.0, 15.0, CREAM, false);
        if hover && self.released_in(r) {
            *value = !*value;
            return true;
        }
        false
    }

    pub fn radio(&mut self, x: f32, y: f32, label: &str, selected: bool) -> bool {
        let r = Rect::new(x, y, 24.0 + self.text_size(label, 15.0, false).x, 20.0);
        self.block(r);
        let hover = r.contains(self.mouse);
        draw_circle(x + 9.0, y + 9.0, 8.0, Color::new(0.06, 0.04, 0.03, 1.0));
        draw_circle_lines(x + 9.0, y + 9.0, 8.0, 1.5, if hover { GOLD } else { BORDER });
        if selected {
            draw_circle(x + 9.0, y + 9.0, 4.5, GOLD);
        }
        self.text(label, x + 24.0, y + 1.0, 15.0, CREAM, false);
        hover && self.released_in(r)
    }

    /// Horizontal slider for `value` in `[min, max]`.
    pub fn slider(&mut self, id: u64, r: Rect, value: &mut f32, min: f32, max: f32) -> bool {
        self.block(r);
        if r.contains(self.mouse) && self.pressed {
            self.active = Some(id);
        }
        let mut changed = false;
        if self.active == Some(id) {
            let t = ((self.mouse.x - r.x) / r.w).clamp(0.0, 1.0);
            let v = min + t * (max - min);
            changed = v != *value;
            *value = v;
        }
        let t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
        let cy = r.y + r.h / 2.0;
        rrect(Rect::new(r.x, cy - 3.0, r.w, 6.0), 3.0, Color::new(0.06, 0.04, 0.03, 1.0));
        if t > 0.0 {
            rrect(Rect::new(r.x, cy - 3.0, (r.w * t).max(6.0), 6.0), 3.0, GOLD);
        }
        draw_circle(r.x + r.w * t, cy, 8.0, CREAM);
        changed
    }

    /// A press-and-drag area: returns what the pointer is doing with it.
    pub fn drag_area(&mut self, id: u64, r: Rect) -> Drag {
        self.block(r);
        if self.pressed && r.contains(self.mouse) {
            self.active = Some(id);
        }
        if self.active == Some(id) {
            Drag::Held(self.mouse)
        } else if self.released_active == Some(id) {
            Drag::Released(self.mouse)
        } else if r.contains(self.mouse) {
            Drag::Hover
        } else {
            Drag::Idle
        }
    }

    /// While the HUD is faded out it must not react to the pointer (a drag
    /// that ends over a hidden button is not a click). Returns what to give
    /// back to [`Ui::resume_input`].
    pub fn suspend_input(&mut self) -> (Vec2, bool, bool, bool) {
        let saved = (self.mouse, self.pressed, self.released, self.down);
        self.mouse = vec2(-1.0e6, -1.0e6);
        self.pressed = false;
        self.released = false;
        self.down = false;
        saved
    }

    pub fn resume_input(&mut self, saved: (Vec2, bool, bool, bool)) {
        (self.mouse, self.pressed, self.released, self.down) = saved;
    }

    pub fn mouse(&self) -> Vec2 {
        self.mouse
    }

    /// Single-line text input. Returns true when Enter is pressed.
    pub fn text_input(&mut self, id: u64, r: Rect, value: &mut String, max_len: usize) -> bool {
        self.input(id, r, value, max_len, false)
    }

    /// Like [`Ui::text_input`], but shows dots instead of the characters.
    pub fn password_input(&mut self, id: u64, r: Rect, value: &mut String, max_len: usize) -> bool {
        self.input(id, r, value, max_len, true)
    }

    /// How tall a [`Ui::text_area`] `width` wide is with `value` in it: a
    /// line more for each line the text wraps to, up to `max_lines`.
    pub fn text_area_height(&self, value: &str, width: f32, max_lines: usize) -> f32 {
        let chars: Vec<char> = value.chars().collect();
        let lines = self.area_lines(&chars, width - 2.0 * AREA_PAD_X).len();
        AREA_LINE * lines.clamp(1, max_lines.max(1)) as f32 + 2.0 * AREA_PAD_Y
    }

    /// A text field that wraps its text over as many lines as it takes
    /// (the caller sizes it with [`Ui::text_area_height`]; past the lines it
    /// has, it scrolls to the caret). Enter sends, as in a single line.
    /// Returns true when Enter is pressed.
    pub fn text_area(&mut self, id: u64, r: Rect, value: &mut String, max_len: usize) -> bool {
        self.block(r);
        self.fields.push(id);
        if self.focus == Some(id) && self.cursor_owner != Some(id) {
            self.cursor_owner = Some(id);
            self.cursor = value.chars().count();
            self.anchor = (self.cursor > 0).then_some(0);
        }
        let room = r.w - 2.0 * AREA_PAD_X;
        let mut submitted = false;
        if self.focus == Some(id) {
            // Up and Down move a line (a single line's field sends them to
            // the start and the end): where the caret stood before.
            let before: Vec<char> = value.chars().collect();
            let lines = self.area_lines(&before, room);
            let (from, shift) =
                (self.cursor, is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift));
            let anchor = self.anchor;
            submitted = self.edit(value, max_len, false);
            let step: i32 = if self.key_fires(KeyCode::Up) {
                -1
            } else if self.key_fires(KeyCode::Down) {
                1
            } else {
                0
            };
            if step != 0 && *value == before.iter().collect::<String>() {
                let line = area_line_of(&lines, from);
                let target = line as i32 + step;
                self.anchor = if shift { anchor.or(Some(from)) } else { None };
                self.cursor = if target < 0 {
                    0
                } else if target as usize >= lines.len() {
                    before.len()
                } else {
                    let x = self.width_between(&before, lines[line].0, from);
                    self.nearest_in(&before, lines[target as usize], x)
                };
            }
        }
        let chars: Vec<char> = value.chars().collect();
        let lines = self.area_lines(&chars, room);
        let shown_lines = (((r.h - 2.0 * AREA_PAD_Y) / AREA_LINE).round() as usize).max(1);
        let caret = if self.focus == Some(id) { self.cursor.min(chars.len()) } else { chars.len() };
        let caret_line = area_line_of(&lines, caret);
        let first_line = (caret_line + 1)
            .saturating_sub(shown_lines)
            .min(lines.len().saturating_sub(shown_lines));
        // A box taller than its text (a single line in a tall field) holds it
        // in the middle.
        let used = lines.len().min(shown_lines) as f32 * AREA_LINE;
        let top = r.y + AREA_PAD_Y + ((r.h - 2.0 * AREA_PAD_Y) - used).max(0.0) / 2.0;
        let at_mouse = |ui: &Ui| {
            let k = ((ui.mouse.y - top) / AREA_LINE).floor().max(0.0) as usize;
            let line = (first_line + k).min(lines.len() - 1);
            ui.nearest_in(&chars, lines[line], ui.mouse.x - r.x - AREA_PAD_X)
        };
        if self.pressed {
            if r.contains(self.mouse) {
                if self.focus != Some(id) {
                    while get_char_pressed().is_some() {}
                    self.focus = Some(id);
                    self.cursor_owner = Some(id);
                }
                let at = at_mouse(self);
                let now = get_time();
                let double = self
                    .last_click
                    .is_some_and(|(f, t, c)| f == id && now - t < 0.4 && c.abs_diff(at) <= 1);
                if double {
                    let mut a = at.min(chars.len());
                    while a > 0 && !chars[a - 1].is_whitespace() {
                        a -= 1;
                    }
                    let mut b = at.min(chars.len());
                    while b < chars.len() && !chars[b].is_whitespace() {
                        b += 1;
                    }
                    self.anchor = (b > a).then_some(a);
                    self.cursor = b;
                    self.last_click = None;
                } else {
                    if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) {
                        self.anchor.get_or_insert(self.cursor);
                    } else {
                        self.anchor = None;
                    }
                    self.cursor = at;
                    self.last_click = Some((id, now, at));
                    self.active = Some(id);
                }
            } else if self.focus == Some(id) {
                self.focus = None;
                self.anchor = None;
            }
        } else if self.down && self.active == Some(id) && self.focus == Some(id) {
            let at = at_mouse(self);
            if at != self.cursor {
                self.anchor.get_or_insert(self.cursor);
                self.cursor = at;
            }
        }
        let focused = self.focus == Some(id);
        rrect(r, 5.0, Color::new(0.04, 0.03, 0.02, 1.0));
        rrect_lines(r, 5.0, if focused { 1.6 } else { 1.2 }, if focused { GOLD } else { BORDER });
        let selection = if focused { self.selection() } else { None };
        for (k, &(a, b)) in lines.iter().enumerate().skip(first_line).take(shown_lines) {
            let y = top + (k - first_line) as f32 * AREA_LINE;
            if let Some((sa, sb)) = selection {
                let (sa, sb) = (sa.clamp(a, b), sb.clamp(a, b));
                if sb > sa {
                    let x0 = self.width_between(&chars, a, sa);
                    let x1 = self.width_between(&chars, a, sb);
                    draw_rectangle(
                        r.x + AREA_PAD_X + x0,
                        y,
                        x1 - x0 + 1.0,
                        AREA_LINE,
                        Color::new(0.75, 0.55, 0.2, 0.45),
                    );
                }
            }
            let text: String = chars[a..b].iter().collect();
            self.draw_text_raw(
                text.trim_end(),
                r.x + AREA_PAD_X,
                y + (AREA_LINE - AREA_SIZE) / 2.0,
                AREA_SIZE,
                CREAM,
                false,
            );
        }
        if focused && ((get_time() - self.caret_at) * 2.0) as i64 % 2 == 0 {
            let (a, _) = lines[caret_line];
            let x = r.x + AREA_PAD_X + 1.0 + self.width_between(&chars, a, caret);
            let y = top + (caret_line - first_line) as f32 * AREA_LINE;
            draw_line(x, y + 2.0, x, y + AREA_LINE - 2.0, 1.5, CREAM);
        }
        submitted
    }

    /// The lines of a text area's characters `room` wide: (start, end) of
    /// each, breaking after a space when it can and between characters
    /// (never inside an emoji) when a word is longer than the line. At
    /// least one line.
    fn area_lines(&self, chars: &[char], room: f32) -> Vec<(usize, usize)> {
        let stops = caret_stops(chars, false);
        let mut lines = Vec::new();
        let mut start = 0;
        while start < chars.len() {
            let mut end = start;
            let mut last_space = None;
            loop {
                let next = stop_after(&stops, end);
                if next == end || self.width_between(chars, start, next) > room && end > start {
                    break;
                }
                if chars[end].is_whitespace() || crate::fonts::ZWSP == chars[end] {
                    last_space = Some(next);
                }
                end = next;
                if end >= chars.len() {
                    break;
                }
            }
            if end < chars.len() && !chars[end].is_whitespace() {
                // Break after the last space, if the line has one.
                if let Some(sp) = last_space.filter(|&sp| sp > start && sp < end) {
                    end = sp;
                }
            }
            // A space the line ends on stays with it (it takes no room there).
            while end < chars.len() && chars[end].is_whitespace() {
                end += 1;
            }
            lines.push((start, end));
            start = end;
        }
        if lines.is_empty() || chars.last().is_some_and(|c| *c == '\n') {
            lines.push((chars.len(), chars.len()));
        }
        lines
    }

    /// The width of `chars[a..b]` in a text area's type.
    fn width_between(&self, chars: &[char], a: usize, b: usize) -> f32 {
        self.text_size(&chars[a..b].iter().collect::<String>(), AREA_SIZE, false).x
    }

    /// The caret stop in `line` of `chars` nearest `x` from its start.
    fn nearest_in(&self, chars: &[char], line: (usize, usize), x: f32) -> usize {
        let stops = caret_stops(chars, false);
        let (a, b) = line;
        // The end of a wrapped line is the start of the next: stop before it.
        let b = if b > a && b < chars.len() { stop_before(&stops, b).max(a) } else { b };
        stops
            .iter()
            .copied()
            .filter(|&i| (a..=b).contains(&i))
            .min_by(|&i, &j| {
                let di = (self.width_between(chars, a, i) - x).abs();
                let dj = (self.width_between(chars, a, j) - x).abs();
                di.total_cmp(&dj)
            })
            .unwrap_or(a)
    }

    /// Where the caret stands in field `id` while it has the keyboard.
    pub fn caret_in(&self, id: u64) -> Option<usize> {
        (self.focus == Some(id) && self.cursor_owner == Some(id)).then_some(self.cursor)
    }

    /// Puts `text` into field `id`'s `value` at `at` (its end when None), as
    /// typing it would (within `max_len`, never inside an emoji), and gives
    /// the field the keyboard with the caret after it: a picker next to a
    /// field (emoji) types into it.
    pub fn insert_into(
        &mut self,
        id: u64,
        value: &mut String,
        text: &str,
        max_len: usize,
        at: Option<usize>,
    ) -> usize {
        let mut chars: Vec<char> = value.chars().collect();
        let stops = caret_stops(&chars, false);
        let at = stop_at(&stops, at.unwrap_or(chars.len()).min(chars.len()));
        let typed: Vec<char> = text.chars().collect();
        let cursor = insert_typed(&mut chars, at, &typed, max_len, false);
        *value = chars.into_iter().collect();
        self.focus = Some(id);
        self.cursor_owner = Some(id);
        self.cursor = cursor;
        self.anchor = None;
        self.caret_at = get_time();
        cursor
    }

    /// True when `key` was pressed this frame or is held long enough to
    /// repeat (macroquad reports only the first press).
    fn key_fires(&mut self, key: KeyCode) -> bool {
        let now = get_time();
        if is_key_pressed(key) {
            self.repeat = Some((key, now + 0.45));
            return true;
        }
        match self.repeat {
            Some((k, next)) if k == key && is_key_down(key) && now >= next => {
                self.repeat = Some((key, now + 0.035));
                true
            }
            Some((k, _)) if k == key && !is_key_down(key) => {
                self.repeat = None;
                false
            }
            _ => false,
        }
    }

    fn input(
        &mut self,
        id: u64,
        r: Rect,
        value: &mut String,
        max_len: usize,
        masked: bool,
    ) -> bool {
        self.block(r);
        self.fields.push(id);
        let mut submitted = false;
        let size = (r.h * 0.5).clamp(12.0, 18.0);
        if self.focus == Some(id) && self.cursor_owner != Some(id) {
            // Focus arrived from elsewhere (Tab, or set by the page): the
            // whole text is selected, ready to be replaced, as in a form.
            self.cursor_owner = Some(id);
            self.cursor = value.chars().count();
            self.anchor = (self.cursor > 0).then_some(0);
        }
        if self.focus == Some(id) {
            submitted = self.edit(value, max_len, masked);
        }
        let shown: Vec<char> =
            if masked { vec!['•'; value.chars().count()] } else { value.chars().collect() };
        let stops = caret_stops(&shown, masked);
        // Scroll so the caret stays visible: as much text as fits, ending
        // at the caret when the caret would be off the right edge.
        let room = r.w - 20.0;
        let caret = if self.focus == Some(id) { self.cursor.min(shown.len()) } else { shown.len() };
        let width_of = |ui: &Ui, a: usize, b: usize| {
            ui.text_size(&shown[a..b].iter().collect::<String>(), size, false).x
        };
        let mut first = 0;
        while first < caret && width_of(self, first, caret) > room {
            first = stop_after(&stops, first).min(caret);
        }
        let mut last = caret;
        while last < shown.len() && width_of(self, first, stop_after(&stops, last)) <= room {
            last = stop_after(&stops, last);
        }
        // The caret stop nearest the pointer.
        let at_mouse = |ui: &Ui| {
            let mx = ui.mouse.x - (r.x + 8.0);
            let mut best = first;
            let mut best_d = f32::MAX;
            for &i in stops.iter().filter(|&&i| (first..=last).contains(&i)) {
                let d = (width_of(ui, first, i) - mx).abs();
                if d < best_d {
                    best = i;
                    best_d = d;
                }
            }
            // Dragged past an edge: scroll that way.
            if mx < 0.0 {
                best = stop_before(&stops, first);
            } else if mx > room && last < shown.len() {
                best = stop_after(&stops, last);
            }
            best
        };
        // Focus changes after typing is handled, so nothing typed is lost.
        if self.pressed {
            if r.contains(self.mouse) {
                if self.focus != Some(id) {
                    while get_char_pressed().is_some() {}
                    self.focus = Some(id);
                    self.cursor_owner = Some(id);
                }
                let at = at_mouse(self);
                let now = get_time();
                let double = self
                    .last_click
                    .is_some_and(|(f, t, c)| f == id && now - t < 0.4 && c.abs_diff(at) <= 1);
                if double && !masked {
                    // A double click selects the word.
                    let chars: Vec<char> = value.chars().collect();
                    let at = at.min(chars.len());
                    let mut a = at;
                    while a > 0 && !chars[a - 1].is_whitespace() {
                        a -= 1;
                    }
                    let mut b = at;
                    while b < chars.len() && !chars[b].is_whitespace() {
                        b += 1;
                    }
                    self.anchor = (b > a).then_some(a);
                    self.cursor = b;
                    self.last_click = None;
                } else {
                    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
                    if shift {
                        self.anchor.get_or_insert(self.cursor);
                    } else {
                        self.anchor = None;
                    }
                    self.cursor = at;
                    self.last_click = Some((id, now, at));
                    self.active = Some(id);
                }
            } else if self.focus == Some(id) {
                self.focus = None;
                self.anchor = None;
            }
        } else if self.down && self.active == Some(id) && self.focus == Some(id) {
            // Dragging selects.
            let at = at_mouse(self);
            if at != self.cursor {
                self.anchor.get_or_insert(self.cursor);
                self.cursor = at;
            }
        }
        let focused = self.focus == Some(id);
        rrect(r, 5.0, Color::new(0.04, 0.03, 0.02, 1.0));
        rrect_lines(r, 5.0, if focused { 1.6 } else { 1.2 }, if focused { GOLD } else { BORDER });
        let visible: String = shown[first..last].iter().collect();
        if focused && let Some((a, b)) = self.selection() {
            let (a, b) = (a.clamp(first, last), b.clamp(first, last));
            if b > a {
                let x0 = width_of(self, first, a);
                let x1 = width_of(self, first, b);
                draw_rectangle(
                    r.x + 8.0 + x0,
                    r.y + 5.0,
                    x1 - x0 + 1.0,
                    r.h - 10.0,
                    Color::new(0.75, 0.55, 0.2, 0.45),
                );
            }
        }
        self.text(&visible, r.x + 8.0, r.y + (r.h - size) / 2.0, size, CREAM, false);
        if focused && ((get_time() - self.caret_at) * 2.0) as i64 % 2 == 0 {
            let caret = self.cursor.min(shown.len()).clamp(first, last);
            let w = width_of(self, first, caret);
            draw_line(r.x + 9.0 + w, r.y + 6.0, r.x + 9.0 + w, r.y + r.h - 6.0, 1.5, CREAM);
        }
        submitted
    }

    /// The selected characters of the focused field: (start, end).
    fn selection(&self) -> Option<(usize, usize)> {
        let a = self.anchor?;
        (a != self.cursor).then(|| (a.min(self.cursor), a.max(self.cursor)))
    }

    /// The keyboard in the focused field. Returns true when Enter is pressed.
    fn edit(&mut self, value: &mut String, max_len: usize, masked: bool) -> bool {
        // AltGr reaches Windows programs as Ctrl+Alt: with it held the keys
        // type (@, #, €, [ ] on many keyboards), they are no shortcuts.
        let altgr = is_key_down(KeyCode::RightAlt);
        let ctrl = (is_key_down(KeyCode::LeftControl)
            || is_key_down(KeyCode::RightControl)
            || is_key_down(KeyCode::LeftSuper)
            || is_key_down(KeyCode::RightSuper))
            && !altgr;
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        let mut chars: Vec<char> = value.chars().collect();
        // The caret moves and erases a whole emoji at a time.
        let mut stops = caret_stops(&chars, masked);
        self.cursor = stop_at(&stops, self.cursor);
        self.anchor = self.anchor.map(|a| stop_at(&stops, a));
        let mut changed = false;
        let mut typed: Vec<char> = Vec::new();
        while let Some(c) = get_char_pressed() {
            if !c.is_control() && !ctrl {
                typed.push(c);
            }
        }
        if ctrl
            && is_key_pressed(KeyCode::V)
            && let Some(text) = macroquad::miniquad::window::clipboard_get()
        {
            typed.extend(text.chars().filter(|c| !c.is_control()));
        }
        if ctrl && is_key_pressed(KeyCode::A) {
            self.anchor = (!chars.is_empty()).then_some(0);
            self.cursor = chars.len();
        }
        let selected = self.selection();
        if ctrl && (is_key_pressed(KeyCode::C) || is_key_pressed(KeyCode::X)) && !masked {
            let text: String = match selected {
                Some((a, b)) => chars[a..b].iter().collect(),
                None => value.clone(),
            };
            macroquad::miniquad::window::clipboard_set(&text);
        }
        let cut = ctrl && is_key_pressed(KeyCode::X) && !masked && selected.is_some();
        let erase_back = self.key_fires(KeyCode::Backspace);
        let erase_fwd = self.key_fires(KeyCode::Delete);
        let mut cursor = self.cursor;
        if let Some((a, b)) = selected
            && (erase_back || erase_fwd || cut || !typed.is_empty())
        {
            // Whatever is selected goes first.
            chars.drain(a..b);
            cursor = a;
            self.anchor = None;
            changed = true;
        } else if erase_back && cursor > 0 {
            let from = if ctrl { word_start(&chars, cursor) } else { stop_before(&stops, cursor) };
            chars.drain(from..cursor);
            cursor = from;
            changed = true;
        } else if erase_fwd && cursor < chars.len() {
            let to = if ctrl { word_end(&chars, cursor) } else { stop_after(&stops, cursor) };
            chars.drain(cursor..to);
            changed = true;
        }
        if !typed.is_empty() {
            let at = insert_typed(&mut chars, cursor, &typed, max_len, masked);
            changed |= at != cursor;
            cursor = at;
        }
        if changed {
            // (Two letters brought together by an erase can make a flag.)
            stops = caret_stops(&chars, masked);
            cursor = stop_at(&stops, cursor);
        }
        // Moving the caret; with Shift, the selection follows it.
        let mut moved_to = None;
        if self.key_fires(KeyCode::Left) {
            moved_to = Some(match (selected, shift, ctrl) {
                (Some((a, _)), false, false) => a,
                (_, _, true) => word_start(&chars, cursor),
                _ => stop_before(&stops, cursor),
            });
        }
        if self.key_fires(KeyCode::Right) {
            moved_to = Some(match (selected, shift, ctrl) {
                (Some((_, b)), false, false) => b,
                (_, _, true) => word_end(&chars, cursor),
                _ => stop_after(&stops, cursor),
            });
        }
        if is_key_pressed(KeyCode::Home) || is_key_pressed(KeyCode::Up) {
            moved_to = Some(0);
        }
        if is_key_pressed(KeyCode::End) || is_key_pressed(KeyCode::Down) {
            moved_to = Some(chars.len());
        }
        if let Some(to) = moved_to {
            if shift {
                self.anchor.get_or_insert(cursor);
            } else {
                self.anchor = None;
            }
            cursor = to;
        }
        if changed {
            *value = chars.into_iter().collect();
        }
        if cursor != self.cursor || changed {
            self.caret_at = get_time();
        }
        self.cursor = cursor;
        let mut submitted = false;
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
            submitted = true;
            self.anchor = None;
        }
        if is_key_pressed(KeyCode::Tab) {
            self.tab_move = Some(if shift { -1 } else { 1 });
        }
        if is_key_pressed(KeyCode::Escape) {
            self.focus = None;
            self.anchor = None;
            self.escape_taken = true;
        }
        submitted
    }

    /// Esc, for a screen or a dialog to go back: not while a field or a
    /// dialog over it has the keyboard, and not the Esc a field just used
    /// to let go of it.
    pub fn escape(&self) -> bool {
        is_key_pressed(KeyCode::Escape) && self.focus.is_none() && !self.escape_taken
    }

    /// A square toggle button with no label (the caller draws its content);
    /// returns true when clicked.
    pub fn toggle(&mut self, r: Rect, on: bool) -> bool {
        self.block(r);
        let hover = r.contains(self.mouse);
        let bg = if on {
            Color::new(0.38, 0.25, 0.12, 1.0)
        } else if hover {
            Color::new(0.25, 0.16, 0.09, 1.0)
        } else {
            Color::new(0.12, 0.08, 0.05, 1.0)
        };
        raised(r, 8.0, bg, if hover || on { GOLD } else { BORDER }, if on { 1.8 } else { 1.2 });
        hover && self.released_in(r)
    }

    /// A tab header; returns true when clicked.
    pub fn tab(&mut self, r: Rect, label: &str, selected: bool) -> bool {
        self.tab_with_count(r, label, "", selected)
    }

    /// A tab with a small count at its right end (e.g. "3/14"); the label
    /// keeps clear of it, shrinking if it must.
    pub fn tab_with_count(&mut self, r: Rect, label: &str, count: &str, selected: bool) -> bool {
        self.block(r);
        let hover = r.contains(self.mouse);
        let bg = if selected {
            Color::new(0.38, 0.25, 0.12, 1.0)
        } else if hover {
            Color::new(0.25, 0.16, 0.09, 1.0)
        } else {
            Color::new(0.12, 0.08, 0.05, 1.0)
        };
        if selected || hover {
            rrect_gradient(r, 6.0, shade(bg, 0.06), shade(bg, -0.15));
        } else {
            rrect(r, 6.0, bg);
        }
        if selected {
            rrect(Rect::new(r.x + 8.0, r.y + r.h - 3.0, r.w - 16.0, 3.0), 1.5, GOLD);
        }
        // Room for the count on the right; the label is centred when that
        // clears it, else moved left (and shrunk only if it must).
        let reserve = if count.is_empty() {
            0.0
        } else {
            let cw = self.text_size(count, 11.0, false).x;
            self.text(count, r.x + r.w - cw - 10.0, r.y + (r.h - 11.0) / 2.0, 11.0, MUTED, false);
            cw + 14.0
        };
        let (label, size) = self.fitted(label, 15.0, r.w - 16.0 - reserve, true);
        let width = self.text_size(&label, size, true).x;
        let centre = (r.x + r.w / 2.0).min(r.x + r.w - reserve - 6.0 - width / 2.0);
        self.text_centered(
            &label,
            centre,
            r.y + (r.h - size) / 2.0,
            size,
            if selected { GOLD } else { CREAM },
            true,
        );
        hover && self.released_in(r) && !selected
    }

    /// Splits `text` into lines no wider than `width`: at spaces, and also
    /// between Chinese and Japanese characters and at the zero-width spaces
    /// of Thai (see [`break_points`]).
    pub fn wrap(&self, text: &str, width: f32, size: f32) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        for (word, spaced) in break_points(text) {
            let candidate = if line.is_empty() {
                word.to_string()
            } else if spaced {
                format!("{line} {word}")
            } else {
                format!("{line}{word}")
            };
            if self.text_size(&candidate, size, false).x <= width || line.is_empty() {
                line = candidate;
            } else {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            }
            // A single word longer than the line: cut it (between
            // characters, never inside an emoji).
            while self.text_size(&line, size, false).x > width && emoji::units(&line).len() > 1 {
                let mut head = 0;
                for (_, end) in emoji::units(&line) {
                    if self.text_size(&line[..end], size, false).x > width {
                        break;
                    }
                    head = end;
                }
                if head == 0 {
                    break;
                }
                let rest = line[head..].to_string();
                line.truncate(head);
                lines.push(std::mem::replace(&mut line, rest));
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }

    /// Shortens `text` with an ellipsis to fit `width`.
    pub fn ellipsize(&self, text: &str, width: f32, size: f32) -> String {
        if self.text_size(text, size, false).x <= width {
            return text.to_string();
        }
        let mut s: String = text.to_string();
        while !s.is_empty() && self.text_size(&format!("{s}…"), size, false).x > width {
            emoji::pop(&mut s);
        }
        format!("{s}…")
    }

    /// A scrollable chat log (newest at the bottom, against the field)
    /// mixing player messages and what happened in the game. A player's
    /// lines in a row sit under one name, each author in a colour of their
    /// own (the reader's gold, administrators red); the game's notices are
    /// small, quiet and centred. `scroll` counts rows up from the bottom.
    /// `admin`: the reader may delete anyone's lines. Returns the new scroll
    /// and what the reader asked for.
    pub fn chat_log(
        &mut self,
        r: Rect,
        lines: &[FeedLine],
        me: Option<&str>,
        admin: bool,
        scroll: usize,
    ) -> (usize, Option<ChatAction>) {
        self.block(r);
        const SIZE: f32 = 14.0;
        const NOTE: f32 = 12.0;
        const LINE: f32 = 19.0;
        const GAP: f32 = 6.0;
        let inner = Rect::new(r.x + 8.0, r.y + 4.0, r.w - 16.0, r.h - 8.0);
        // Wrap only the newest messages that can be shown (measuring text is
        // the expensive part with a long history).
        let budget = (inner.h / LINE).ceil() as usize + scroll + 24;
        let mut rows: Vec<LogRow> = Vec::new();
        for (index, l) in lines.iter().enumerate().rev() {
            let older = index.checked_sub(1).map(|i| &lines[i]);
            let player = l.kind == FeedKind::Player;
            let author = l.from.as_ref().map(|(n, _)| n.as_str());
            // A line under the one before it: same author, nothing between.
            let follows = player
                && older.is_some_and(|o| {
                    o.kind == FeedKind::Player
                        && o.deleted.is_none()
                        && o.from.as_ref().map(|(n, _)| n.as_str()) == author
                });
            let text = match l.deleted {
                Some(snook_legend_proto::Deleted::Author) => {
                    crate::i18n::t("(message deleted by its author)").to_string()
                }
                Some(snook_legend_proto::Deleted::Admin) => {
                    crate::i18n::t("(message deleted by an administrator)").to_string()
                }
                None => l.text.clone(),
            };
            let mut block: Vec<LogRow> = Vec::new();
            match (l.kind, &l.from) {
                (FeedKind::Player, Some((name, _))) => {
                    let mine = me.is_some_and(|m| m.eq_ignore_ascii_case(name));
                    let color = if l.admin {
                        ADMIN_RED
                    } else if mine {
                        GOLD
                    } else {
                        author_color(name)
                    };
                    let label = format!("{name}: ");
                    let indent = self.text_size(&label, SIZE, true).x;
                    let room = (inner.w - indent).max(60.0);
                    for (k, w) in self.wrap(&text, room, SIZE).into_iter().enumerate() {
                        block.push(LogRow {
                            name: (k == 0 && !follows).then(|| (label.clone(), color)),
                            text: w,
                            indent,
                            index,
                            centred: false,
                            gap: k == 0 && !follows,
                        });
                    }
                }
                _ => {
                    let prefix = if l.kind == FeedKind::Announcement { "★ " } else { "" };
                    let text = format!("{prefix}{text}");
                    let room = (inner.w - 24.0).max(60.0);
                    for (k, w) in self.wrap(&text, room, NOTE).into_iter().enumerate() {
                        block.push(LogRow {
                            name: None,
                            text: w,
                            indent: 0.0,
                            index,
                            centred: true,
                            gap: k == 0,
                        });
                    }
                }
            }
            block.append(&mut rows);
            rows = block;
            if rows.len() >= budget {
                break;
            }
        }
        // Heights, from the oldest row kept: the first one needs no gap.
        let height =
            |rows: &[LogRow], i: usize| LINE + if rows[i].gap && i > 0 { GAP } else { 0.0 };
        // How far up the log scrolls: until its first row is at the top.
        let mut fit = 0.0;
        let mut top_rows = 0;
        for i in 0..rows.len() {
            fit += height(&rows, i);
            if fit > inner.h {
                break;
            }
            top_rows += 1;
        }
        let max_scroll = rows.len().saturating_sub(top_rows.max(1));
        let mut scroll = scroll.min(max_scroll);
        if r.contains(self.mouse) {
            let (_, wheel) = mouse_wheel();
            if wheel > 0.0 {
                scroll = (scroll + 3).min(max_scroll);
            } else if wheel < 0.0 {
                scroll = scroll.saturating_sub(3);
            }
        }
        // Laid out from the bottom up: the newest against the field.
        let end = rows.len() - scroll;
        let mut placed: Vec<(usize, f32)> = Vec::new();
        let mut bottom = inner.y + inner.h;
        for i in (0..end).rev() {
            let y = bottom - LINE;
            if y < inner.y - 0.5 {
                break;
            }
            placed.push((i, y));
            bottom = y - if rows[i].gap { GAP } else { 0.0 };
        }
        placed.reverse();
        // The message under the pointer, if it may be deleted by this reader.
        let hovered = placed
            .iter()
            .find(|(_, y)| r.contains(self.mouse) && self.mouse.y >= *y && self.mouse.y < y + LINE)
            .map(|(i, _)| rows[*i].index)
            .filter(|&i| {
                let l = &lines[i];
                let mine = me.is_some_and(|m| {
                    l.from.as_ref().is_some_and(|(n, _)| n.eq_ignore_ascii_case(m))
                });
                l.id > 0 && l.deleted.is_none() && l.kind == FeedKind::Player && (mine || admin)
            });
        let mut action = None;
        let mut buttons_drawn = false;
        for &(i, y) in &placed {
            let row = &rows[i];
            let line = &lines[row.index];
            let deleted = line.deleted.is_some();
            let pending = line.pending;
            let soft = |c: Color| if pending { Color::new(c.r, c.g, c.b, c.a * 0.5) } else { c };
            if hovered == Some(row.index) {
                draw_rectangle(
                    r.x + 2.0,
                    y - 1.0,
                    r.w - 4.0,
                    LINE,
                    Color::new(1.0, 1.0, 1.0, 0.05),
                );
            }
            if row.centred {
                let color = match line.kind {
                    FeedKind::Announcement => Color::new(1.0, 0.88, 0.6, 0.95),
                    FeedKind::Alert => Color::new(0.86, 0.66, 0.56, 0.85),
                    _ => Color::new(0.68, 0.63, 0.56, 0.8),
                };
                self.text_centered(
                    &row.text,
                    inner.x + inner.w / 2.0,
                    y + (SIZE - NOTE) * 0.5 + 1.0,
                    NOTE,
                    color,
                    false,
                );
                continue;
            }
            if let Some((name, color)) = &row.name {
                self.text(name, inner.x, y, SIZE, soft(*color), true);
            }
            let color = if deleted { Color::new(0.62, 0.56, 0.5, 0.8) } else { CREAM };
            let size = if deleted { 13.0 } else { SIZE };
            self.text(
                &row.text,
                inner.x + row.indent,
                y + (SIZE - size) * 0.5,
                size,
                soft(color),
                false,
            );
            // Delete buttons on the hovered message's first row on screen.
            if hovered == Some(row.index) && !buttons_drawn {
                buttons_drawn = true;
                let id = line.id;
                let mut bx = r.x + r.w - 26.0;
                let b = Rect::new(bx, y - 1.0, 20.0, 18.0);
                let tip = crate::i18n::t("Delete (it shows as deleted)");
                if self.icon_button_ex(b, crate::icons::Icon::Close, tip, true, true) {
                    action = Some(ChatAction::Delete { id, silent: false });
                }
                if admin {
                    bx -= 24.0;
                    let b = Rect::new(bx, y - 1.0, 20.0, 18.0);
                    let tip = crate::i18n::t("Remove without a trace");
                    if self.icon_button_ex(b, crate::icons::Icon::Minus, tip, true, true) {
                        action = Some(ChatAction::Delete { id, silent: true });
                    }
                }
            }
        }
        // Newer lines below: a pill to bring them back.
        if scroll > 0 {
            let label = crate::i18n::t("▼ newer messages");
            let w = self.text_size(label, 12.0, true).x + 20.0;
            let pill = Rect::new(r.x + (r.w - w) / 2.0, r.y + r.h - 26.0, w, 20.0);
            rrect(pill, 10.0, Color::new(0.12, 0.09, 0.06, 0.95));
            rrect_lines(pill, 10.0, 1.0, GOLD);
            self.text_centered(label, pill.x + pill.w / 2.0, pill.y + 4.0, 12.0, GOLD, true);
            if self.hovering(pill) && self.released_in(pill) {
                scroll = 0;
            }
        }
        if rows.is_empty() {
            let msg = crate::i18n::t("No messages yet.");
            self.text_centered(msg, r.x + r.w / 2.0, r.y + r.h / 2.0 - 7.0, 13.0, MUTED, false);
        }
        (scroll, action)
    }

    /// A square button with an icon; `tip` is shown while hovering.
    pub fn icon_button(
        &mut self,
        r: Rect,
        icon: crate::icons::Icon,
        tip: &str,
        enabled: bool,
    ) -> bool {
        self.icon_button_ex(r, icon, tip, enabled, false)
    }

    /// Like [`Ui::icon_button`]; `subtle` draws it without a background until hovered.
    pub fn icon_button_ex(
        &mut self,
        r: Rect,
        icon: crate::icons::Icon,
        tip: &str,
        enabled: bool,
        subtle: bool,
    ) -> bool {
        self.block(r);
        let hover = enabled && r.contains(self.mouse);
        let bg = if hover && self.down {
            Color::new(0.50, 0.33, 0.15, 1.0)
        } else if hover {
            Color::new(0.38, 0.25, 0.12, 0.95)
        } else if subtle {
            Color::new(0.05, 0.035, 0.025, 0.45)
        } else if !enabled {
            Color::new(0.11, 0.08, 0.06, 0.9)
        } else {
            Color::new(0.24, 0.16, 0.09, 0.95)
        };
        let radius = (r.h * 0.22).min(8.0);
        if !subtle || hover {
            let edge = if hover {
                GOLD
            } else if enabled {
                BORDER
            } else {
                Color::new(BORDER.r, BORDER.g, BORDER.b, 0.45)
            };
            raised(r, radius, bg, edge, 1.2);
        } else {
            rrect(r, radius, bg);
        }
        let color = if !enabled {
            Color::new(0.5, 0.45, 0.4, 0.6)
        } else if hover {
            GOLD
        } else {
            CREAM
        };
        crate::icons::draw(icon, vec2(r.x + r.w / 2.0, r.y + r.h / 2.0), r.h * 0.52, color);
        // Also when it can't be used now: what it is, all the same.
        if r.contains(self.mouse) && !tip.is_empty() {
            self.tooltip(tip, r);
        }
        enabled && hover && self.released_in(r)
    }

    /// A small dark label under (or above, near the bottom) `anchor`. It is
    /// drawn at the end of the frame, on top of the whole interface; only the
    /// last one asked for in a frame is shown.
    pub fn tooltip(&self, text: &str, anchor: Rect) {
        let text = text.to_string();
        let mut list = self.overlays.borrow_mut();
        list.retain(|(o, _)| !o.tooltip);
        list.push((
            Overlay {
                tooltip: true,
                draw: Box::new(move |ui: &Ui| {
                    // One line or several (a line break, or a text wider
                    // than the window), never wider than the window.
                    let room = (vw() - 26.0).max(60.0);
                    let lines: Vec<String> = text
                        .split('\n')
                        .flat_map(|l| {
                            if ui.text_size(l, 13.0, true).x <= room {
                                vec![l.to_string()]
                            } else {
                                // (Measured in regular type: bold is a little wider.)
                                ui.wrap(l, room * 0.9, 13.0)
                            }
                        })
                        .collect();
                    let widest =
                        lines.iter().map(|l| ui.text_size(l, 13.0, true).x).fold(0.0, f32::max);
                    let w = widest + 18.0;
                    let h = 24.0 + 17.0 * (lines.len().max(1) - 1) as f32;
                    let below = anchor.y + anchor.h + h + 6.0 < vh();
                    let y = if below {
                        anchor.y + anchor.h + 6.0
                    } else {
                        (anchor.y - h - 6.0).max(2.0)
                    };
                    let x = clamp_soft(anchor.x + anchor.w / 2.0 - w / 2.0, 4.0, vw() - w - 4.0);
                    let r = Rect::new(x, y, w, h);
                    let radius = if lines.len() > 1 { 9.0 } else { 12.0 };
                    rrect(r, radius, Color::new(0.0, 0.0, 0.0, 0.9));
                    rrect_lines(r, radius, 1.0, Color::new(BORDER.r, BORDER.g, BORDER.b, 0.7));
                    for (k, line) in lines.iter().enumerate() {
                        ui.text(line, r.x + 9.0, r.y + 5.0 + 17.0 * k as f32, 13.0, CREAM, true);
                    }
                }),
            },
            crate::paint::opacity(),
        ));
    }

    /// Draws `f` at the end of the frame, over the whole interface (hover
    /// cards and hints that must never end up under a panel).
    pub fn overlay(&self, f: impl FnOnce(&Ui) + 'static) {
        self.overlays
            .borrow_mut()
            .push((Overlay { tooltip: false, draw: Box::new(f) }, crate::paint::opacity()));
    }

    /// Draws this frame's hover hints over everything drawn so far. Call
    /// once, after the rest of the interface (in interface units).
    pub fn draw_overlays(&self) {
        // Hints float above everything: they are not held to any panel.
        self.panels.borrow_mut().clear();
        let list = std::mem::take(&mut *self.overlays.borrow_mut());
        let before = crate::paint::opacity();
        for (o, alpha) in list {
            crate::paint::set_opacity(alpha);
            (o.draw)(self);
        }
        crate::paint::set_opacity(before);
    }

    /// A flag (if any) and a player name in their tier colour. Returns the width used.
    pub fn player_name(
        &self,
        p: &snook_legend_proto::PlayerInfo,
        x: f32,
        y: f32,
        size: f32,
        max_w: f32,
    ) -> f32 {
        let crown = if p.vip { size + 4.0 } else { 0.0 };
        let w = match name_style(p) {
            Some((a, b)) => {
                let fh = (size * 0.78).round();
                let mut used = 0.0;
                if crate::flags::draw(&p.country, x, y + (size - fh) / 2.0 + 1.0, fh) {
                    used = crate::flags::width(fh) + 6.0;
                }
                let name = self.ellipsize(&p.username, max_w - crown - used, size);
                self.styled_text(&name, x + used, y, size, a, b, true);
                used + self.text_size(&name, size, true).x
            }
            None => self.name_with_flag(
                &p.username,
                &p.country,
                name_color(p),
                x,
                y,
                size,
                max_w - crown,
            ),
        };
        // The shop frame: a thin line of its colours under the name.
        if let Some(snook_legend_proto::shop::Look::Frame { a, b, .. }) =
            snook_legend_proto::shop::item(&p.frame).map(|i| i.look)
            && p.frame != "frame_none"
        {
            let ca = Color::from_rgba(a[0], a[1], a[2], 220);
            let cb = Color::from_rgba(b[0], b[1], b[2], 220);
            crate::paint::draw_rectangle(x, y + size + 1.0, w * 0.5, 2.0, ca);
            crate::paint::draw_rectangle(x + w * 0.5, y + size + 1.0, w * 0.5, 2.0, cb);
        }
        if p.vip {
            // VIP members: a small gold crown after the name.
            crate::icons::draw(
                crate::icons::Icon::Crown,
                vec2(x + w + 4.0 + size * 0.45, y + size * 0.55),
                size * 0.8,
                GOLD,
            );
        }
        w + crown
    }

    /// A flag and a name in `color`. Returns the width used.
    #[allow(clippy::too_many_arguments)]
    pub fn name_with_flag(
        &self,
        name: &str,
        country: &str,
        color: Color,
        x: f32,
        y: f32,
        size: f32,
        max_w: f32,
    ) -> f32 {
        let fh = (size * 0.78).round();
        let mut used = 0.0;
        if crate::flags::draw(country, x, y + (size - fh) / 2.0 + 1.0, fh) {
            used = crate::flags::width(fh) + 6.0;
        }
        let name = self.ellipsize(name, max_w - used, size);
        self.text(&name, x + used, y, size, color, true);
        used + self.text_size(&name, size, true).x
    }

    /// A rounded pill with short text (a tag such as "rated").
    pub fn pill(&self, text: &str, x: f32, y: f32, color: Color) -> f32 {
        let w = self.text_size(text, 11.0, true).x + 12.0;
        let r = Rect::new(x, y, w, 17.0);
        rrect(r, 8.5, Color::new(color.r, color.g, color.b, 0.16));
        rrect_lines(r, 8.5, 1.0, Color::new(color.r, color.g, color.b, 0.6));
        self.text(text, x + 6.0, y + 2.5, 11.0, color, true);
        w
    }

    /// Scrolls a list area with the mouse wheel; returns the new first row.
    pub fn scroll_list(&mut self, area: Rect, first: usize, rows: usize, visible: usize) -> usize {
        self.block(area);
        let max = rows.saturating_sub(visible);
        let mut first = first.min(max);
        if area.contains(self.mouse) {
            let (_, wheel) = mouse_wheel();
            if wheel > 0.0 {
                first = first.saturating_sub(3);
            } else if wheel < 0.0 {
                first = (first + 3).min(max);
            }
        }
        if rows > visible && visible > 0 {
            // A scroll bar on the right: the track shows there is more.
            let track = Rect::new(area.x + area.w - 5.0, area.y, 5.0, area.h);
            rrect(track, 2.5, Color::new(1.0, 1.0, 1.0, 0.06));
            let h = (track.h * visible as f32 / rows as f32).max(16.0);
            let y = track.y + (track.h - h) * first as f32 / max.max(1) as f32;
            rrect(Rect::new(track.x, y, 5.0, h), 2.5, Color::new(0.91, 0.75, 0.38, 0.7));
        }
        first
    }

    /// True if the left button was released over `r` this frame (a click).
    /// The button was let go this frame after being pressed inside `r`:
    /// a press that started elsewhere (dragging the power, the spin, the
    /// aim) and ends over a button is not a click on it.
    pub fn released_in(&self, r: Rect) -> bool {
        self.released && r.contains(self.press_at)
    }

    pub fn clicked(&self, r: Rect) -> bool {
        self.released_in(r) && r.contains(self.mouse)
    }

    /// A floating panel or popover (the match's chat, the spin panel)
    /// closes when the button is pressed anywhere outside it: true when it
    /// was pressed this frame outside every one of `keep` (the panel, and
    /// the buttons that open it or belong to it). The press still reaches
    /// what is under it, so a click on the table closes the chat and aims.
    /// Never while the input is suspended (a faded HUD, a dialog above).
    pub fn pressed_outside(&self, keep: &[Rect]) -> bool {
        self.pressed && !keep.iter().any(|r| r.contains(self.mouse))
    }

    /// A dialog over a dimmed screen closes when the dimmed part is
    /// pressed, as with its Cancel or Close: true when the button was
    /// pressed this frame outside `panel`. That press is spent, so nothing
    /// under the dimming acts when it is let go.
    pub fn dismissed(&mut self, panel: Rect) -> bool {
        if self.pressed && !panel.contains(self.mouse) {
            self.pressed = false;
            self.press_at = vec2(-1.0e6, -1.0e6);
            true
        } else {
            false
        }
    }

    pub fn hovering(&self, r: Rect) -> bool {
        r.contains(self.mouse)
    }

    /// A row of joined toggle buttons; returns the index clicked.
    pub fn segmented(&mut self, r: Rect, options: &[&str], selected: usize) -> Option<usize> {
        self.block(r);
        let n = options.len().max(1) as f32;
        let w = r.w / n;
        rrect(r, 7.0, Color::new(0.06, 0.04, 0.03, 1.0));
        let mut clicked = None;
        for (i, label) in options.iter().enumerate() {
            let cell = Rect::new(r.x + i as f32 * w, r.y, w, r.h);
            let hover = cell.contains(self.mouse);
            if i == selected {
                let bg = Color::new(0.38, 0.25, 0.12, 1.0);
                rrect_gradient(
                    Rect::new(cell.x + 2.0, cell.y + 2.0, cell.w - 4.0, cell.h - 4.0),
                    6.0,
                    shade(bg, 0.08),
                    shade(bg, -0.15),
                );
            } else if hover {
                rrect(
                    Rect::new(cell.x + 2.0, cell.y + 2.0, cell.w - 4.0, cell.h - 4.0),
                    6.0,
                    Color::new(0.22, 0.15, 0.09, 1.0),
                );
            }
            let (label, size) = self.fitted(label, (r.h * 0.42).clamp(12.0, 16.0), w - 10.0, true);
            let color = if i == selected { GOLD } else { CREAM };
            self.text_centered(
                &label,
                cell.x + w / 2.0,
                cell.y + (r.h - size) / 2.0,
                size,
                color,
                true,
            );
            if hover && self.released_in(cell) && i != selected {
                clicked = Some(i);
            }
        }
        rrect_lines(r, 7.0, 1.2, BORDER);
        clicked
    }

    /// The circular "where does the tip hit the cue ball" control.
    pub fn spin_ball(&mut self, id: u64, center: Vec2, radius: f32, tip: &mut (f32, f32)) {
        let r = Rect::new(center.x - radius, center.y - radius, radius * 2.0, radius * 2.0);
        self.block(r);
        if self.pressed && center.distance(self.mouse) <= radius {
            self.active = Some(id);
        }
        if self.active == Some(id) {
            let mut d = (self.mouse - center) / radius;
            if d.length() > 1.0 {
                d = d.normalize();
            }
            *tip = (d.x, -d.y);
        }
        draw_circle(center.x, center.y, radius, Color::new(0.96, 0.94, 0.88, 1.0));
        draw_circle_lines(center.x, center.y, radius, 2.0, Color::new(0.5, 0.42, 0.33, 1.0));
        draw_circle_lines(center.x, center.y, radius * 0.5, 1.0, Color::new(0.8, 0.76, 0.68, 1.0));
        let faint = Color::new(0.8, 0.76, 0.68, 1.0);
        draw_line(center.x - radius, center.y, center.x + radius, center.y, 1.0, faint);
        draw_line(center.x, center.y - radius, center.x, center.y + radius, 1.0, faint);
        let dot = center + vec2(tip.0, -tip.1) * radius;
        draw_circle(dot.x, dot.y, 7.0, Color::new(0.78, 0.16, 0.16, 1.0));
        draw_circle_lines(dot.x, dot.y, 7.0, 1.5, Color::new(0.35, 0.04, 0.04, 1.0));
    }
}

const CORNER_STEPS: usize = 6;
const OUTLINE: usize = 4 * (CORNER_STEPS + 1);

/// Outline points of a rounded rectangle, clockwise from the top-left corner
/// (on the stack: these are drawn hundreds of times a frame).
fn rounded_points(r: Rect, radius: f32) -> [Vec2; OUTLINE] {
    let rad = radius.min(r.w / 2.0).min(r.h / 2.0).max(0.0);
    let corners = [
        (vec2(r.x + rad, r.y + rad), std::f32::consts::PI),
        (vec2(r.x + r.w - rad, r.y + rad), 1.5 * std::f32::consts::PI),
        (vec2(r.x + r.w - rad, r.y + r.h - rad), 0.0),
        (vec2(r.x + rad, r.y + r.h - rad), 0.5 * std::f32::consts::PI),
    ];
    let mut pts = [Vec2::ZERO; OUTLINE];
    let mut i = 0;
    for (c, start) in corners {
        for k in 0..=CORNER_STEPS {
            let a = start + k as f32 / CORNER_STEPS as f32 * std::f32::consts::FRAC_PI_2;
            pts[i] = c + vec2(a.cos(), a.sin()) * rad;
            i += 1;
        }
    }
    pts
}

/// A filled rounded rectangle (one non-overlapping fan, so it blends correctly
/// when translucent).
pub fn rrect(r: Rect, radius: f32, color: Color) {
    if r.w <= 0.0 || r.h <= 0.0 {
        return;
    }
    let pts = rounded_points(r, radius);
    let c = vec2(r.x + r.w / 2.0, r.y + r.h / 2.0);
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        draw_triangle(c, a, b, color);
    }
}

/// A rounded rectangle shaded from `top` to `bottom` (a smooth vertical
/// gradient: per-vertex colours, so no banding and no hard edges).
pub fn rrect_gradient(r: Rect, radius: f32, top: Color, bottom: Color) {
    if r.w <= 0.0 || r.h <= 0.0 {
        return;
    }
    let pts = rounded_points(r, radius);
    let at = |y: f32| {
        let k = ((y - r.y) / r.h).clamp(0.0, 1.0);
        crate::paint::fade(Color::new(
            top.r + (bottom.r - top.r) * k,
            top.g + (bottom.g - top.g) * k,
            top.b + (bottom.b - top.b) * k,
            top.a + (bottom.a - top.a) * k,
        ))
    };
    let c = vec2(r.x + r.w / 2.0, r.y + r.h / 2.0);
    // On the stack: panels and buttons draw hundreds of these a frame.
    let vertex = |p: Vec2| macroquad::models::Vertex::new(p.x, p.y, 0.0, 0.0, 0.0, at(p.y));
    let mut vertices = [vertex(c); OUTLINE + 1];
    for (v, p) in vertices[1..].iter_mut().zip(pts) {
        *v = vertex(p);
    }
    let indices: [u16; OUTLINE * 3] = std::array::from_fn(|k| {
        let (i, corner) = (k / 3, k % 3);
        match corner {
            0 => 0,
            1 => 1 + i as u16,
            _ => 1 + ((i + 1) % OUTLINE) as u16,
        }
    });
    let gl = unsafe { get_internal_gl() }.quad_gl;
    gl.texture(None);
    gl.draw_mode(DrawMode::Triangles);
    gl.geometry(&vertices, &indices);
}

/// Dots going round: something is on its way. One look everywhere (the
/// dots scale with `radius`).
pub fn spinner(c: Vec2, radius: f32, color: Color) {
    let time = get_time() as f32;
    let n = 10;
    let dot = (radius * 0.2).max(1.4);
    for k in 0..n {
        let a = k as f32 / n as f32 * std::f32::consts::TAU + time * 4.5;
        let alpha = 0.15 + 0.85 * k as f32 / n as f32;
        let p = c + Vec2::from_angle(a) * radius;
        draw_circle(p.x, p.y, dot, Color::new(color.r, color.g, color.b, color.a * alpha));
    }
}

/// The same colour, lighter (`k` > 0) or darker (`k` < 0).
pub fn shade(c: Color, k: f32) -> Color {
    let f = |v: f32| if k >= 0.0 { v + (1.0 - v) * k } else { v * (1.0 + k) };
    Color::new(f(c.r), f(c.g), f(c.b), c.a)
}

/// A soft drop shadow under a rounded rectangle (a few widening layers).
pub fn soft_shadow(r: Rect, radius: f32, strength: f32) {
    for (grow, drop, alpha) in [(6.0, 6.0, 0.08), (3.0, 4.0, 0.12), (1.0, 2.0, 0.16)] {
        rrect(
            Rect::new(r.x - grow, r.y - grow + drop, r.w + 2.0 * grow, r.h + 2.0 * grow),
            radius + grow,
            Color::new(0.0, 0.0, 0.0, alpha * strength),
        );
    }
}

/// A raised surface: gradient body, a faint highlight along the top edge
/// and an outline. Buttons, tabs and toggles share it.
fn raised(r: Rect, radius: f32, bg: Color, edge: Color, thickness: f32) {
    rrect_gradient(r, radius, shade(bg, 0.07), shade(bg, -0.18));
    let inset = radius.min(r.w / 2.0);
    draw_line(
        r.x + inset,
        r.y + 1.5,
        r.x + r.w - inset,
        r.y + 1.5,
        1.0,
        Color::new(1.0, 0.92, 0.78, 0.10 * bg.a),
    );
    rrect_lines(r, radius, thickness, edge);
}

/// The outline of a rounded rectangle.
pub fn rrect_lines(r: Rect, radius: f32, thickness: f32, color: Color) {
    let pts = rounded_points(r, radius);
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        draw_line(a.x, a.y, b.x, b.y, thickness, color);
    }
}

/// How much of a chat line is used ("180/200"), at the top right of its
/// field, once it is close to the limit.
pub fn chat_counter(ui: &Ui, value: &str, field: Rect) {
    let n = value.chars().count();
    let max = snook_legend_proto::MAX_CHAT_LEN;
    if n + 40 < max {
        return;
    }
    let label = format!("{n}/{max}");
    let w = ui.text_size(&label, 11.0, false).x;
    let color = if n >= max { Color::new(0.95, 0.45, 0.35, 1.0) } else { MUTED };
    rrect(
        Rect::new(field.x + field.w - w - 12.0, field.y - 9.0, w + 8.0, 15.0),
        4.0,
        Color::new(0.04, 0.03, 0.02, 1.0),
    );
    ui.text(&label, field.x + field.w - w - 8.0, field.y - 8.0, 11.0, color, false);
}

/// A text area's type, line height and padding.
const AREA_SIZE: f32 = 14.0;
const AREA_LINE: f32 = 19.0;
const AREA_PAD_X: f32 = 8.0;
const AREA_PAD_Y: f32 = 6.0;

/// The line of a text area's `lines` the caret `at` is on (the end of a
/// wrapped line belongs to the next one).
fn area_line_of(lines: &[(usize, usize)], at: usize) -> usize {
    lines
        .iter()
        .position(|&(a, b)| {
            at >= a && (at < b || (at == b && b == lines.last().map_or(b, |l| l.1)))
        })
        .unwrap_or(lines.len().saturating_sub(1))
}

/// Where the caret may stand in a text field's characters: anywhere but
/// inside an emoji (in a password, anywhere: it shows a dot for each
/// character). From 0 to the end, in order.
fn caret_stops(chars: &[char], masked: bool) -> Vec<usize> {
    if masked { (0..=chars.len()).collect() } else { emoji::stops(chars) }
}

/// The caret stop before `i` (0 at the start).
fn stop_before(stops: &[usize], i: usize) -> usize {
    stops.iter().rev().copied().find(|&s| s < i).unwrap_or(0)
}

/// The caret stop after `i` (the end at the end).
fn stop_after(stops: &[usize], i: usize) -> usize {
    stops.iter().copied().find(|&s| s > i).unwrap_or(stops[stops.len() - 1])
}

/// `i`, or the caret stop after it when it is inside an emoji.
fn stop_at(stops: &[usize], i: usize) -> usize {
    stops.iter().copied().find(|&s| s >= i).unwrap_or(stops[stops.len() - 1])
}

/// Puts what was typed (or pasted) into a text field's characters at
/// `cursor`, a whole emoji at a time, while it fits in `max_len`
/// characters. Returns where the caret goes: after it, never inside an
/// emoji (a flag's second letter makes one with the first).
fn insert_typed(
    chars: &mut Vec<char>,
    cursor: usize,
    typed: &[char],
    max_len: usize,
    masked: bool,
) -> usize {
    let mut cursor = cursor;
    let stops = caret_stops(typed, masked);
    for unit in stops.windows(2).map(|w| &typed[w[0]..w[1]]) {
        if chars.len() + unit.len() > max_len {
            break;
        }
        chars.splice(cursor..cursor, unit.iter().copied());
        cursor += unit.len();
    }
    stop_at(&caret_stops(chars, masked), cursor)
}

/// Where the word before `i` starts (Ctrl+Left, Ctrl+Backspace).
fn word_start(chars: &[char], mut i: usize) -> usize {
    while i > 0 && chars[i - 1].is_whitespace() {
        i -= 1;
    }
    while i > 0 && !chars[i - 1].is_whitespace() {
        i -= 1;
    }
    i
}

/// Where the word after `i` ends (Ctrl+Right, Ctrl+Delete).
fn word_end(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    while i < chars.len() && !chars[i].is_whitespace() {
        i += 1;
    }
    i
}

/// Chinese and Japanese characters (and their punctuation): a line may
/// break before or after any of them.
fn breaks_anywhere(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x2FDF | 0x3000..=0x303F | 0x3040..=0x30FF | 0x31F0..=0x31FF
        | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF)
}

/// Punctuation a line never starts with (it stays with what it closes).
fn closes(c: char) -> bool {
    matches!(
        c,
        '、' | '。'
            | '，'
            | '．'
            | '：'
            | '；'
            | '！'
            | '？'
            | '）'
            | '」'
            | '』'
            | '】'
            | '〕'
            | '〉'
            | '》'
            | '〟'
            | '’'
            | '”'
            | 'ー'
            | '…'
            | '・'
            | '々'
            | 'ぁ'
            | 'ぃ'
            | 'ぅ'
            | 'ぇ'
            | 'ぉ'
            | 'っ'
            | 'ゃ'
            | 'ゅ'
            | 'ょ'
            | 'ァ'
            | 'ィ'
            | 'ゥ'
            | 'ェ'
            | 'ォ'
            | 'ッ'
            | 'ャ'
            | 'ュ'
            | 'ョ'
            | ')'
            | ']'
            | '}'
            | ','
            | '.'
            | ':'
            | ';'
            | '!'
            | '?'
            | '%'
    )
}

/// Punctuation a line never ends with (it stays with what it opens).
fn opens(c: char) -> bool {
    matches!(c, '（' | '「' | '『' | '【' | '〔' | '〈' | '《' | '〝' | '‘' | '“' | '(' | '[' | '{')
}

/// The pieces a line may break between, with whether a space stood
/// before each (spaces themselves left out): words, and inside a word,
/// each Chinese or Japanese character and each part between Thai's
/// zero-width spaces.
pub fn break_points(text: &str) -> Vec<(&str, bool)> {
    let mut out = Vec::new();
    for word in text.split_whitespace() {
        let mut spaced = true;
        let mut start = 0;
        let mut prev: Option<char> = None;
        for (i, c) in word.char_indices() {
            let cut = if c == crate::fonts::ZWSP {
                true
            } else if let Some(p) = prev {
                (breaks_anywhere(p) || breaks_anywhere(c))
                    && !closes(c)
                    && !opens(p)
                    && p != crate::fonts::ZWSP
            } else {
                false
            };
            if cut && i > start {
                out.push((&word[start..i], spaced));
                spaced = false;
                start = i;
            }
            if c == crate::fonts::ZWSP {
                start = i + c.len_utf8();
            }
            prev = Some(c);
        }
        if start < word.len() {
            out.push((&word[start..], spaced));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn the_caret_steps_over_whole_emoji() {
        // a, the flag (2), b, thumbs up with a skin tone (2), the heart with
        // its selector (2), the family (7).
        let c = chars("a🇧🇷b👍🏽❤\u{FE0F}👨\u{200D}👩\u{200D}👧\u{200D}👦");
        let stops = caret_stops(&c, false);
        assert_eq!(stops, [0, 1, 3, 4, 6, 8, 15]);
        assert_eq!(stop_after(&stops, 1), 3);
        assert_eq!(stop_before(&stops, 3), 1);
        assert_eq!(stop_before(&stops, 15), 8);
        assert_eq!(stop_after(&stops, 15), 15);
        assert_eq!(stop_before(&stops, 0), 0);
        // Inside an emoji: the end of it.
        assert_eq!(stop_at(&stops, 2), 3);
        assert_eq!(stop_at(&stops, 99), 15);
        // A password shows a dot for each character: every one is a stop.
        assert_eq!(caret_stops(&c, true), (0..=15).collect::<Vec<_>>());
        // Without emoji, every character.
        assert_eq!(caret_stops(&chars("olá"), false), [0, 1, 2, 3]);
    }

    #[test]
    fn typing_puts_in_whole_emoji() {
        let mut c = chars("ab");
        // A flag typed a letter at a time becomes one.
        let at = insert_typed(&mut c, 1, &['🇧'], 50, false);
        assert_eq!(at, 2);
        let at = insert_typed(&mut c, at, &['🇷'], 50, false);
        assert_eq!((c.iter().collect::<String>(), at), ("a🇧🇷b".to_string(), 3));
        // U+FE0F after the heart: one unit with it, the caret after both.
        let mut c = chars("x❤");
        let at = insert_typed(&mut c, 2, &['\u{FE0F}'], 50, false);
        assert_eq!(at, 3);
        assert_eq!(caret_stops(&c, false), [0, 1, 3]);
        // Near the limit an emoji goes in whole or not at all.
        let mut c = chars("abcd");
        let at = insert_typed(&mut c, 4, &chars("e🇧🇷f"), 6, false);
        assert_eq!((c.iter().collect::<String>(), at), ("abcde".to_string(), 5));
        let mut c = chars("abcd");
        let at = insert_typed(&mut c, 0, &chars("🇧🇷f"), 6, false);
        assert_eq!((c.iter().collect::<String>(), at), ("🇧🇷abcd".to_string(), 2));
        // A pasted family goes in whole.
        let mut c = Vec::new();
        let family = chars("👨\u{200D}👩\u{200D}👧\u{200D}👦!");
        assert_eq!(insert_typed(&mut c, 0, &family, 200, false), 8);
    }
}
