//! The settings screen: graphics, game and sound, in tabs. Every tab is laid
//! out the same way: section headings over grouped rows, each row a label
//! (with a short hint where it helps) on the left and its control on the
//! right. The panel is sized to the tab shown, and its top stays put when
//! the tab changes, so the tabs never move under the pointer.

use macroquad::prelude::*;

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_circle_lines, draw_line, draw_rectangle, draw_triangle};

use crate::config::{AimGuide, Config, DEFAULT_SERVER, Quality};
use crate::i18n::{Lang, t};
use crate::icons::Icon;
use crate::ui::{ALERT, BORDER, CREAM, Drag, GOLD, MUTED, Ui, rrect, rrect_lines};

pub enum SettingsAction {
    None,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Graphics,
    Game,
    Sound,
}

const TABS: [Tab; 3] = [Tab::Graphics, Tab::Game, Tab::Sound];

/// Panel width and its inner padding.
const WIDTH: f32 = 700.0;
const PAD: f32 = 28.0;
/// From the panel's top to the first section (title and tabs).
const HEADER_H: f32 = 112.0;
/// From the last section to the panel's bottom (the buttons).
const FOOTER_H: f32 = 84.0;
/// Space kept around the panel.
const MARGIN: f32 = 8.0;
/// Inside a section's card: padding left and right, above and below each row.
const CARD_PAD: f32 = 16.0;
const ROW_PAD: f32 = 8.0;
const ROW_MIN_H: f32 = 44.0;
/// Between a row's text and its control.
const GUTTER: f32 = 24.0;
const HEADING_H: f32 = 25.0;
const SECTION_GAP: f32 = 16.0;
const LABEL: f32 = 15.0;
const HINT: f32 = 12.0;

#[derive(Default)]
pub struct SettingsScreen {
    /// MSAA when the game started (changes apply after a restart).
    pub startup_msaa: Option<i32>,
    tab: Tab,
    /// The interface size being dragged (applied on release, so the slider
    /// doesn't move under the pointer).
    pending_size: Option<f32>,
    /// The "Advanced" section is open.
    advanced: bool,
    /// How far the rows are scrolled (only when the window is too short).
    scroll: f32,
    /// "Reset to defaults" was pressed once, at this time: a second press
    /// within a few seconds resets.
    reset_armed: Option<f64>,
}

impl SettingsScreen {
    /// Development aid (`--settings` with SNOOKERED_SETTINGS_TAB=game): opens on a tab.
    pub fn dev_tab(&mut self) {
        match crate::dev::var("SNOOKERED_SETTINGS_TAB").as_deref() {
            Some("game") => self.tab = Tab::Game,
            Some("sound") => self.tab = Tab::Sound,
            _ => {}
        }
        if crate::dev::flag("SNOOKERED_SETTINGS_ADVANCED") {
            self.advanced = true;
        }
    }

    pub fn draw(&mut self, ui: &mut Ui, config: &mut Config, in_match: bool) -> SettingsAction {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let startup_msaa = *self.startup_msaa.get_or_insert(config.msaa);
        let advanced = self.advanced;
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.5));

        let w = WIDTH.min(sw - 2.0 * MARGIN);
        let inner = w - 2.0 * PAD;
        // Measure every tab (with "Advanced" open): the tallest one fixes
        // where the panel's top is, so neither changing tabs nor opening that
        // section moves it. The tab shown sets the panel's height.
        let mut tallest = 0.0f32;
        for tab in TABS {
            let mut rows = Rows::measure(inner);
            self.rows(tab, ui, config, in_match, startup_msaa, true, &mut rows);
            tallest = tallest.max(rows.finish());
        }
        let mut rows = Rows::measure(inner);
        self.rows(self.tab, ui, config, in_match, startup_msaa, advanced, &mut rows);
        let content = rows.finish();
        let cards = rows.cards;
        let room = (sh - 2.0 * MARGIN - HEADER_H - FOOTER_H).max(80.0);
        let body_h = content.min(room);
        let top = ((sh - HEADER_H - FOOTER_H - tallest.min(room)) / 2.0).max(MARGIN);
        let r = Rect::new((sw - w) / 2.0, top, w, HEADER_H + body_h + FOOTER_H);
        ui.panel(r);
        let x = r.x + PAD;

        // Title.
        crate::icons::draw(Icon::Gear, vec2(x + 13.0, r.y + 33.0), 26.0, GOLD);
        ui.text(t("Settings"), x + 38.0, r.y + 20.0, 24.0, GOLD, true);

        // Tabs.
        let tabs =
            [(Tab::Graphics, t("Graphics")), (Tab::Game, t("Game")), (Tab::Sound, t("Sound"))];
        let tab_w = (inner - 16.0) / 3.0;
        for (k, (tab, label)) in tabs.into_iter().enumerate() {
            let tr = Rect::new(x + k as f32 * (tab_w + 8.0), r.y + 60.0, tab_w, 34.0);
            if ui.tab(tr, label, self.tab == tab) {
                self.tab = tab;
                self.scroll = 0.0;
            }
        }

        // The rows, scrolled when the window is too short for them.
        let body = Rect::new(x, r.y + HEADER_H, inner, body_h);
        let max_scroll = (content - body_h).max(0.0);
        if max_scroll > 0.0 && ui.hovering(body) {
            let (_, wheel) = mouse_wheel();
            if wheel != 0.0 {
                self.scroll -= wheel.signum() * 48.0;
            }
        }
        self.scroll = self.scroll.clamp(0.0, max_scroll);
        let mut rows = Rows::draw(body, self.scroll, cards);
        let tab = self.tab;
        if max_scroll > 0.0 {
            // Rows cut by the edges are clipped there, and the parts out of
            // sight take no clicks (a drag already under way goes on).
            clip(Some(Rect::new(body.x - 4.0, body.y, body.w + 8.0, body.h)));
            let saved = ui.suspend_input();
            let inside = body.contains(saved.0);
            ui.resume_input((saved.0, saved.1 && inside, saved.2 && inside, saved.3));
            self.rows(tab, ui, config, in_match, startup_msaa, advanced, &mut rows);
            ui.resume_input(saved);
            clip(None);
        } else {
            self.rows(tab, ui, config, in_match, startup_msaa, advanced, &mut rows);
        }
        if max_scroll > 0.0 {
            let track = Rect::new(r.x + r.w - 15.0, body.y, 4.0, body.h);
            rrect(track, 2.0, Color::new(1.0, 1.0, 1.0, 0.06));
            let h = (track.h * body.h / content).max(20.0);
            let y = track.y + (track.h - h) * self.scroll / max_scroll;
            rrect(Rect::new(track.x, y, 4.0, h), 2.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.7));
        }

        // Buttons.
        let by = r.y + r.h - 22.0 - 40.0;
        let armed = self.reset_armed.is_some_and(|at| get_time() - at < 4.0);
        let reset_label = if armed { t("Reset everything?") } else { t("Reset to defaults") };
        // As wide as the longer of its two labels, so it doesn't jump.
        let reset_w = [t("Reset everything?"), t("Reset to defaults")]
            .iter()
            .map(|l| ui.text_size(l, 17.0, true).x + 40.0)
            .fold(190.0, f32::max)
            .min(280.0);
        let reset = Rect::new(x, by, reset_w, 40.0);
        let pressed = ui.button(reset, reset_label, true);
        if armed {
            rrect_lines(reset, 6.0, 1.6, ALERT);
        }
        if pressed {
            if armed {
                self.reset_armed = None;
                reset_defaults(config);
            } else {
                self.reset_armed = Some(get_time());
            }
        } else if !armed {
            self.reset_armed = None;
        }
        if armed && ui.hovering(reset) {
            ui.tooltip(t("Click again to reset every setting"), reset);
        }
        if ui.primary_button(Rect::new(x + inner - 190.0, by, 190.0, 40.0), t("Done"))
            || (is_key_pressed(KeyCode::Escape) && ui.focus.is_none())
        {
            if config.server.trim().is_empty() {
                config.server = DEFAULT_SERVER.into();
            }
            config.server = config.server.trim().to_string();
            return SettingsAction::Close;
        }
        SettingsAction::None
    }

    /// The rows of `tab`. Measuring and drawing share this code: while
    /// measuring, no row hands out a control, so nothing changes.
    #[allow(clippy::too_many_arguments)]
    fn rows(
        &mut self,
        tab: Tab,
        ui: &mut Ui,
        config: &mut Config,
        in_match: bool,
        startup_msaa: i32,
        advanced: bool,
        rows: &mut Rows,
    ) {
        match tab {
            Tab::Graphics => {
                rows.heading(ui, t("Quality"), None);
                let hint = match config.quality {
                    Quality::Low => "For old or very slow computers.",
                    Quality::Medium => "Balanced: smooth on most laptops.",
                    Quality::High => "Soft shadows from every lamp, contact shading.",
                    Quality::Ultra => "Everything, with the sharpest balls: for gaming PCs.",
                };
                if let Some((_, c)) =
                    rows.row(ui, t("Graphics quality"), t(hint), Control::Wide(32.0))
                {
                    let labels: Vec<&str> = Quality::ALL.iter().map(|q| t(q.name())).collect();
                    let sel = Quality::ALL.iter().position(|q| *q == config.quality).unwrap_or(0);
                    if let Some(i) = ui.segmented(c, &labels, sel)
                        && Quality::ALL[i] != config.quality
                    {
                        config.quality = Quality::ALL[i];
                        config.msaa = config.quality.default_msaa();
                    }
                }
                let restart = config.msaa != startup_msaa;
                let hint = if restart {
                    t("Restart the game to apply this change.")
                } else {
                    t("Smooths the jagged edges of lines and balls.")
                };
                let color = if restart { ALERT } else { MUTED };
                if let Some((_, c)) = rows.row_ex(
                    ui,
                    t("Anti-aliasing"),
                    hint,
                    color,
                    Control::Wide(32.0),
                    true,
                    false,
                ) {
                    let levels = [1, 2, 4, 8];
                    let sel = levels.iter().position(|n| *n == config.msaa).unwrap_or(0);
                    if let Some(i) = ui.segmented(c, &[t("Off"), "2×", "4×", "8×"], sel) {
                        config.msaa = levels[i];
                    }
                }

                rows.heading(ui, t("Display"), None);
                if switch_row(
                    rows,
                    ui,
                    t("Full screen"),
                    t("Also with F11."),
                    &mut config.fullscreen,
                ) {
                    set_fullscreen(config.fullscreen);
                }
                let size = self.pending_size.unwrap_or(config.ui_size);
                if let Some((_, c)) = rows.row(
                    ui,
                    t("Interface size"),
                    t("Text, buttons and panels."),
                    Control::Wide(24.0),
                ) {
                    let value = format!("{:.0}%", size * 100.0);
                    let (v, released) = slider(ui, 73, c, size, 0.7, 1.5, &value);
                    // Applied when the slider is let go, so it doesn't move
                    // under the pointer.
                    if released {
                        config.ui_size = (v * 20.0).round() / 20.0;
                        self.pending_size = None;
                    } else if v != size || self.pending_size.is_some() {
                        self.pending_size = Some((v * 20.0).round() / 20.0);
                    }
                }
                switch_row(
                    rows,
                    ui,
                    t("Vignette"),
                    t("Darkens the edges of the picture, like a film."),
                    &mut config.vignette,
                );
                switch_row(
                    rows,
                    ui,
                    t("Dark room"),
                    t("A spotlight on the table, the room in shadow."),
                    &mut config.focused_scene,
                );
                switch_row(
                    rows,
                    ui,
                    t("Show FPS"),
                    t("Frames per second, in a corner of the screen."),
                    &mut config.show_fps,
                );
                switch_row(
                    rows,
                    ui,
                    t("Show ping"),
                    t("Your connection's delay, in online games."),
                    &mut config.show_ping,
                );
            }
            Tab::Game => {
                rows.heading(ui, t("General"), None);
                if let Some((_, c)) = rows.row(ui, t("Language"), "", Control::Wide(32.0)) {
                    let names: Vec<&str> = Lang::ALL.iter().map(|l| short_name(*l)).collect();
                    let sel = Lang::ALL.iter().position(|l| *l == config.language).unwrap_or(0);
                    if let Some(i) = ui.segmented(c, &names, sel) {
                        config.language = Lang::ALL[i];
                    }
                }

                rows.heading(ui, t("Aiming"), None);
                let hint = match config.guide {
                    AimGuide::Full => {
                        t("Shows where the object ball and the cue ball go (not online).")
                    }
                    AimGuide::Standard => {
                        t("Shows the contact point; judging the cut is up to you.")
                    }
                    AimGuide::Short => t("Only a short line from the cue ball."),
                };
                if let Some((_, c)) = rows.row(ui, t("Aiming guide"), hint, Control::Wide(32.0)) {
                    let names: Vec<&str> = AimGuide::ALL.iter().map(|g| t(g.name())).collect();
                    let sel = AimGuide::ALL.iter().position(|g| *g == config.guide).unwrap_or(0);
                    if let Some(i) = ui.segmented(c, &names, sel) {
                        config.guide = AimGuide::ALL[i];
                    }
                }
                if let Some((_, c)) = rows.row(
                    ui,
                    t("Mouse aim sensitivity"),
                    t("How fast the cue turns with the mouse."),
                    Control::Wide(24.0),
                ) {
                    let value = format!("{:.1}×", config.aim_sensitivity);
                    let (v, _) = slider(ui, 72, c, config.aim_sensitivity, 0.3, 2.5, &value);
                    config.aim_sensitivity = v;
                }

                rows.heading(ui, t("Camera"), None);
                switch_row(
                    rows,
                    ui,
                    t("Camera follows the cue"),
                    t("Off: it stays where you put it."),
                    &mut config.camera_follow,
                );
                // Only the camera that follows the cue pulls back.
                let follows = config.camera_follow;
                let hint = if follows {
                    t("Camera pulls back to watch each shot")
                } else {
                    t("Only when the camera follows the cue.")
                };
                if let Some((row, c)) = rows.row_ex(
                    ui,
                    t("Pull back for shots"),
                    hint,
                    MUTED,
                    Control::Switch,
                    follows,
                    true,
                ) {
                    toggle_on_click(ui, row, c, &mut config.shot_camera, follows);
                }
                switch_row(
                    rows,
                    ui,
                    t("TV camera when watching"),
                    t("Broadcast angles for other players' games and replays."),
                    &mut config.tv_camera,
                );

                // The server, out of the way: few players ever change it.
                if !in_match {
                    // Closed, it still says when another server is in use.
                    let custom = config.server.trim() != DEFAULT_SERVER;
                    let note = if custom { t("Custom server") } else { "" };
                    if rows.heading(ui, t("Advanced"), Some((advanced, note))) {
                        self.advanced = !advanced;
                    }
                    if advanced
                        && let Some((_, c)) = rows.row(
                            ui,
                            t("Game server"),
                            t("Change it only to play on another server."),
                            Control::Wide(32.0),
                        )
                    {
                        let input = Rect::new(c.x, c.y, c.w - 40.0, c.h);
                        ui.text_input(71, input, &mut config.server, 200);
                        let default = config.server.trim() == DEFAULT_SERVER;
                        if ui.icon_button(
                            Rect::new(c.x + c.w - 32.0, c.y, 32.0, 32.0),
                            Icon::Refresh,
                            t("Use default"),
                            !default,
                        ) {
                            config.server = DEFAULT_SERVER.into();
                        }
                    }
                }
            }
            Tab::Sound => {
                rows.heading(ui, t("Sound"), None);
                if let Some((_, c)) = rows.row(
                    ui,
                    t("Volume"),
                    t("The cue, the balls, the cushions and the pockets."),
                    Control::Wide(24.0),
                ) {
                    let value = format!("{:.0}%", config.volume * 100.0);
                    let (v, _) = slider(ui, 70, c, config.volume, 0.0, 1.0, &value);
                    config.volume = v;
                }

                rows.heading(ui, t("Notifications"), None);
                switch_row(
                    rows,
                    ui,
                    t("System notifications"),
                    t("Invites, friends coming online and games starting, while you're away."),
                    &mut config.desktop_notifications,
                );
            }
        }
    }
}

/// Everything back to how it was on the first run, except who is playing
/// and the language.
fn reset_defaults(config: &mut Config) {
    let keep = config.clone();
    *config = Config {
        player_name: keep.player_name,
        second_player_name: keep.second_player_name,
        username: keep.username,
        token: keep.token,
        language: keep.language,
        graphics_checked: true,
        ..Config::default()
    };
    set_fullscreen(config.fullscreen);
}

/// The language's name as the selector shows it (short enough for its cell).
fn short_name(lang: Lang) -> &'static str {
    match lang {
        Lang::Portuguese => "Português",
        other => other.name(),
    }
}

/// What sits at the right of a row.
#[derive(Clone, Copy)]
enum Control {
    /// Fills the controls' column, this tall (choices, sliders, fields).
    Wide(f32),
    /// An on/off switch (the text gets the rest of the row).
    Switch,
}

impl Control {
    /// Its size in rows `w` wide.
    fn size(self, w: f32) -> Vec2 {
        match self {
            Control::Wide(h) => vec2((w * 0.5).min(330.0), h),
            Control::Switch => vec2(44.0, 24.0),
        }
    }
}

/// Lays out a tab: section headings, each over a card of rows. The same
/// calls measure the tab (nothing drawn, no control handed out) and then
/// draw it, so the panel can be sized before anything inside it is drawn.
struct Rows {
    x: f32,
    w: f32,
    y: f32,
    /// Where the rows start (to work out the height).
    y0: f32,
    drawing: bool,
    /// The band in sight (the rows scroll): rows out of it are skipped.
    clip: (f32, f32),
    /// The height of each section's card: recorded while measuring, used
    /// to draw the card before its rows.
    cards: Vec<f32>,
    /// Where the current section's card starts, and its index.
    card_top: Option<f32>,
    section: usize,
    /// Rows so far in the current section (a rule goes between rows).
    in_section: usize,
}

impl Rows {
    fn measure(w: f32) -> Rows {
        Rows {
            x: 0.0,
            w,
            y: 0.0,
            y0: 0.0,
            drawing: false,
            clip: (f32::MIN, f32::MAX),
            cards: Vec::new(),
            card_top: None,
            section: 0,
            in_section: 0,
        }
    }

    fn draw(body: Rect, scroll: f32, cards: Vec<f32>) -> Rows {
        Rows {
            x: body.x,
            w: body.w,
            y: body.y - scroll,
            y0: body.y - scroll,
            drawing: true,
            clip: (body.y, body.y + body.h),
            cards,
            card_top: None,
            section: 0,
            in_section: 0,
        }
    }

    /// Whether any of this band shows (what is out of sight isn't drawn).
    fn visible(&self, y: f32, h: f32) -> bool {
        y + h > self.clip.0 && y < self.clip.1
    }

    /// Ends the current section's card.
    fn close_card(&mut self) {
        if let Some(top) = self.card_top.take() {
            if !self.drawing {
                self.cards.push(self.y - top);
            }
            self.section += 1;
        }
    }

    /// The height of everything laid out so far.
    fn finish(&mut self) -> f32 {
        self.close_card();
        self.y - self.y0
    }

    /// A section heading. `fold`: the section can be opened and closed (it
    /// is open when `Some((true, _))`), with a note shown beside the heading
    /// while it is closed; returns true when the heading is clicked.
    fn heading(&mut self, ui: &mut Ui, text: &str, fold: Option<(bool, &str)>) -> bool {
        self.close_card();
        if self.section > 0 {
            self.y += SECTION_GAP;
        }
        let mut clicked = false;
        if self.drawing && self.visible(self.y, 16.0) {
            match fold {
                None => ui.text(text, self.x + 2.0, self.y, 15.0, GOLD, true),
                Some((open, note)) => {
                    let tw = ui.text_size(text, 15.0, true).x;
                    let hit = Rect::new(self.x - 6.0, self.y - 5.0, tw + 36.0, 26.0);
                    let hover = ui.hovering(hit);
                    if hover {
                        rrect(hit, 6.0, Color::new(1.0, 0.9, 0.7, 0.06));
                    }
                    ui.block(hit);
                    let color = if hover { GOLD } else { Color::new(GOLD.r, GOLD.g, GOLD.b, 0.85) };
                    ui.text(text, self.x + 2.0, self.y, 15.0, color, true);
                    // A small arrow: pointing right when closed, down when open.
                    let c = vec2(self.x + tw + 16.0, self.y + 8.5);
                    let (a, b, d) = if open {
                        (vec2(-5.0, -3.0), vec2(5.0, -3.0), vec2(0.0, 3.5))
                    } else {
                        (vec2(-3.0, -5.0), vec2(-3.0, 5.0), vec2(3.5, 0.0))
                    };
                    draw_triangle(c + a, c + b, c + d, color);
                    if !open && !note.is_empty() {
                        ui.pill(note, hit.x + hit.w + 6.0, self.y - 0.5, MUTED);
                    }
                    clicked = hover && ui.clicked(hit);
                }
            }
        }
        // A closed section has no card under its heading.
        let closed = matches!(fold, Some((false, _)));
        self.y += if closed { 16.0 } else { HEADING_H };
        // The card behind the section's rows.
        self.card_top = Some(self.y);
        if self.drawing
            && let Some(&h) = self.cards.get(self.section)
            && h > 0.0
            && self.visible(self.y, h)
        {
            let card = Rect::new(self.x, self.y, self.w, h);
            rrect(card, 8.0, Color::new(0.0, 0.0, 0.0, 0.22));
            rrect_lines(card, 8.0, 1.0, Color::new(BORDER.r, BORDER.g, BORDER.b, 0.55));
        }
        self.in_section = 0;
        clicked
    }

    /// A row: `label` and `hint` ("" for none) on the left, `control` on
    /// the right. Returns the row and the control's place when it is drawn.
    fn row(
        &mut self,
        ui: &mut Ui,
        label: &str,
        hint: &str,
        control: Control,
    ) -> Option<(Rect, Rect)> {
        self.row_ex(ui, label, hint, MUTED, control, true, false)
    }

    /// [`Rows::row`] with the hint in `hint_color`, greyed out when not
    /// `enabled`, and lit up on hover when the whole row is `clickable`.
    #[allow(clippy::too_many_arguments)]
    fn row_ex(
        &mut self,
        ui: &mut Ui,
        label: &str,
        hint: &str,
        hint_color: Color,
        control: Control,
        enabled: bool,
        clickable: bool,
    ) -> Option<(Rect, Rect)> {
        let size = control.size(self.w);
        let text_w = self.w - 2.0 * CARD_PAD - size.x - GUTTER;
        let label_lines = ui.wrap(label, text_w, LABEL);
        let hint_lines = if hint.is_empty() { Vec::new() } else { ui.wrap(hint, text_w, HINT) };
        let label_h = label_lines.len() as f32 * (LABEL + 4.0) - 4.0;
        let hint_h =
            if hint_lines.is_empty() { 0.0 } else { 3.0 + hint_lines.len() as f32 * (HINT + 3.0) };
        let text_h = label_h + hint_h;
        let h = (text_h.max(size.y) + 2.0 * ROW_PAD).max(ROW_MIN_H);
        let row = Rect::new(self.x, self.y, self.w, h);
        let first = self.in_section == 0;
        self.y += h;
        self.in_section += 1;
        if !self.drawing || !self.visible(row.y, row.h) {
            return None;
        }
        if !first {
            draw_line(
                row.x + CARD_PAD,
                row.y,
                row.x + row.w - CARD_PAD,
                row.y,
                1.0,
                Color::new(BORDER.r, BORDER.g, BORDER.b, 0.35),
            );
        }
        if clickable && enabled && ui.hovering(row) {
            rrect(
                Rect::new(row.x + 3.0, row.y + 3.0, row.w - 6.0, row.h - 6.0),
                6.0,
                Color::new(1.0, 0.9, 0.7, 0.05),
            );
        }
        let fade = |c: Color| if enabled { c } else { Color::new(c.r, c.g, c.b, c.a * 0.5) };
        let mut ty = row.y + (h - text_h) / 2.0;
        for line in &label_lines {
            ui.text(line, row.x + CARD_PAD, ty, LABEL, fade(CREAM), false);
            ty += LABEL + 4.0;
        }
        ty -= 1.0;
        for line in &hint_lines {
            ui.text(line, row.x + CARD_PAD, ty, HINT, fade(hint_color), false);
            ty += HINT + 3.0;
        }
        let place = Rect::new(
            row.x + row.w - CARD_PAD - size.x,
            row.y + (h - size.y) / 2.0,
            size.x,
            size.y,
        );
        Some((row, place))
    }
}

/// A row with an on/off switch; clicking anywhere on the row flips it.
fn switch_row(rows: &mut Rows, ui: &mut Ui, label: &str, hint: &str, value: &mut bool) -> bool {
    match rows.row_ex(ui, label, hint, MUTED, Control::Switch, true, true) {
        Some((row, place)) => toggle_on_click(ui, row, place, value, true),
        None => false,
    }
}

/// Draws the switch in `place` and flips `value` when the row is clicked.
fn toggle_on_click(ui: &mut Ui, row: Rect, place: Rect, value: &mut bool, enabled: bool) -> bool {
    ui.block(row);
    let hover = enabled && ui.hovering(row);
    switch(place, *value, enabled, hover);
    if hover && ui.clicked(row) {
        *value = !*value;
        return true;
    }
    false
}

/// An on/off switch: a gold track with the knob on the right when on, a
/// dark one with the knob on the left when off.
fn switch(track: Rect, on: bool, enabled: bool, hover: bool) {
    let a = if enabled { 1.0 } else { 0.4 };
    let radius = track.h / 2.0;
    let (bg, edge) = if on {
        (
            Color::new(0.62, 0.46, 0.19, a),
            if hover { GOLD } else { Color::new(0.80, 0.63, 0.30, a) },
        )
    } else {
        (
            Color::new(0.07, 0.05, 0.035, a),
            if hover { GOLD } else { Color::new(BORDER.r, BORDER.g, BORDER.b, a) },
        )
    };
    rrect(track, radius, bg);
    rrect_lines(track, radius, 1.2, edge);
    let kx = if on { track.x + track.w - radius } else { track.x + radius };
    let cy = track.y + radius;
    draw_circle(kx, cy + 1.0, radius - 3.5, Color::new(0.0, 0.0, 0.0, 0.3 * a));
    let knob =
        if on { Color::new(1.0, 0.97, 0.9, a) } else { Color::new(MUTED.r, MUTED.g, MUTED.b, a) };
    draw_circle(kx, cy, radius - 3.5, knob);
}

/// Clips what is drawn from now on to `r` (in interface units), or stops
/// clipping.
fn clip(r: Option<Rect>) {
    let k = crate::ui::ui_scale();
    let gl = unsafe { get_internal_gl() }.quad_gl;
    gl.scissor(r.map(|r| {
        let (x0, y0) = ((r.x * k).floor() as i32, (r.y * k).floor() as i32);
        let (x1, y1) = (((r.x + r.w) * k).ceil() as i32, ((r.y + r.h) * k).ceil() as i32);
        (x0, y0, x1 - x0, y1 - y0)
    }));
}

/// A slider with a visible track and its value written at the right.
/// Returns the value (under the pointer while it is dragged) and whether
/// the drag ended this frame.
#[allow(clippy::too_many_arguments)]
fn slider(
    ui: &mut Ui,
    id: u64,
    r: Rect,
    value: f32,
    min: f32,
    max: f32,
    label: &str,
) -> (f32, bool) {
    let value_w = 52.0;
    let track_r = Rect::new(r.x + 9.0, r.y, r.w - value_w - 18.0, r.h);
    // The pointer can grab the knob a little past either end.
    let hit = Rect::new(r.x, r.y - 4.0, r.w - value_w, r.h + 8.0);
    let drag = ui.drag_area(id, hit);
    let at = |p: Vec2| min + ((p.x - track_r.x) / track_r.w).clamp(0.0, 1.0) * (max - min);
    let (v, released) = match drag {
        Drag::Held(p) => (at(p), false),
        Drag::Released(p) => (at(p), true),
        _ => (value, false),
    };
    let k = ((v - min) / (max - min)).clamp(0.0, 1.0);
    let cy = r.y + r.h / 2.0;
    let track = Rect::new(track_r.x, cy - 3.0, track_r.w, 6.0);
    rrect(track, 3.0, Color::new(0.24, 0.17, 0.11, 1.0));
    rrect_lines(track, 3.0, 1.0, Color::new(BORDER.r, BORDER.g, BORDER.b, 0.8));
    if k > 0.0 {
        rrect(Rect::new(track.x, track.y, (track.w * k).max(6.0), track.h), 3.0, GOLD);
    }
    let hot = matches!(drag, Drag::Held(_) | Drag::Hover);
    let kx = track.x + track.w * k;
    draw_circle(kx, cy + 1.0, 9.0, Color::new(0.0, 0.0, 0.0, 0.35));
    draw_circle(kx, cy, 9.0, CREAM);
    draw_circle_lines(kx, cy, 9.0, 1.5, if hot { GOLD } else { Color::new(0.55, 0.42, 0.25, 1.0) });
    let lw = ui.text_size(label, 14.0, true).x;
    ui.text(label, r.x + r.w - lw, cy - 8.0, 14.0, CREAM, true);
    (v, released)
}
