//! The settings screen: graphics, game, controls and sound, in tabs. Every
//! tab is laid out the same way: section headings over grouped rows, each
//! row a label (with a short hint where it helps) on the left and its control
//! on the right. The panel is sized to the tab shown, and its top stays put
//! when the tab changes, so the tabs never move under the pointer (the long
//! Controls tab grows downwards from there and scrolls).

use macroquad::prelude::*;

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_circle_lines, draw_line, draw_rectangle, draw_triangle};

use crate::config::{AimGuide, Config, DEFAULT_SERVER, Quality};
use crate::i18n::{Lang, t, tw};
use crate::icons::Icon;
use crate::keys::{self, Action, Bindings, Conflict};
use crate::ui::{ALERT, BORDER, CREAM, Drag, GOLD, MUTED, Ui, rrect, rrect_lines};
use crate::update::{self, Status};
use crate::whatsnew::WhatsNew;

pub enum SettingsAction {
    None,
    Close,
    /// Start the new version that was downloaded.
    Restart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Graphics,
    Game,
    Controls,
    Sound,
}

const TABS: [Tab; 4] = [Tab::Graphics, Tab::Game, Tab::Controls, Tab::Sound];

/// The Controls tab's sections.
const KEY_SECTIONS: [(&str, &[Action]); 4] = [
    (
        "Aiming and shooting",
        &[Action::AimLeft, Action::AimRight, Action::FineAim, Action::Shoot, Action::BallInHand],
    ),
    (
        "Spin and elevation",
        &[
            Action::SpinUp,
            Action::SpinDown,
            Action::SpinLeft,
            Action::SpinRight,
            Action::ElevationUp,
            Action::ElevationDown,
        ],
    ),
    ("Camera", &[Action::ToggleView, Action::ResetCamera, Action::ZoomIn, Action::ZoomOut]),
    ("Game", &[Action::Replay, Action::Help, Action::Fullscreen]),
];

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
    /// The key slot waiting for a key (Controls tab).
    capture: Option<(Action, usize)>,
    /// A note under an action's keys (what a change did), and since when.
    key_note: Option<(Action, String, f64)>,
    /// An action whose keys just changed because of another one, lit up.
    flash: Option<(Action, f64)>,
    /// Where the key caps were drawn (a click elsewhere stops waiting).
    cap_rects: Vec<Rect>,
    /// The "What's new" dialog, over the settings.
    whats_new: Option<WhatsNew>,
    /// "Restart now" was pressed in the Updates section.
    restart: bool,
}

impl SettingsScreen {
    /// Development aid (`--settings` with SNOOK_LEGEND_SETTINGS_TAB=game): opens on a tab.
    pub fn dev_tab(&mut self) {
        match crate::dev::var("SNOOK_LEGEND_SETTINGS_TAB").as_deref() {
            Some("game") => self.tab = Tab::Game,
            Some("controls") => self.tab = Tab::Controls,
            Some("sound") => self.tab = Tab::Sound,
            _ => {}
        }
        if crate::dev::flag("SNOOK_LEGEND_SETTINGS_ADVANCED") {
            self.advanced = true;
        }
        // SNOOK_LEGEND_SETTINGS_CAPTURE=shoot: waiting for a key for that action.
        if let Some(a) = crate::dev::var("SNOOK_LEGEND_SETTINGS_CAPTURE")
            .and_then(|code| Action::ALL.into_iter().find(|a| a.code() == code))
        {
            self.capture = Some((a, 0));
        }
        // SNOOK_LEGEND_SETTINGS_NOTE=shoot: a conflict note on that action.
        if let Some(a) = crate::dev::var("SNOOK_LEGEND_SETTINGS_NOTE")
            .and_then(|code| Action::ALL.into_iter().find(|a| a.code() == code))
        {
            self.key_note = Some((
                a,
                tw(
                    "Swapped: {action} is now on {key}.",
                    &[("action", &t(Action::Replay.label())), ("key", &"F5")],
                ),
                f64::MAX / 2.0,
            ));
        }
        if crate::dev::flag("SNOOK_LEGEND_SETTINGS_WHATS_NEW") {
            self.whats_new = Some(WhatsNew::all());
        }
    }

    /// Stops waiting for a key (the settings are closing).
    pub fn stop_capture(&mut self) {
        self.capture = None;
        keys::set_capturing(false);
    }

    /// Waiting for a key: Esc cancels, Backspace or Delete clears the slot,
    /// a key that can be bound is bound (taking it from another action if
    /// needed), a click elsewhere cancels. Returns true when Esc was used.
    fn capture_keys(&mut self, ui: &mut Ui, keys: &mut Bindings) -> bool {
        let last_caps = std::mem::take(&mut self.cap_rects);
        let Some((action, slot)) = self.capture else { return false };
        let now = get_time();
        if is_key_pressed(KeyCode::Escape) {
            self.capture = None;
            return true;
        }
        let pressed = get_keys_pressed();
        let key = if is_key_pressed(KeyCode::Backspace) || is_key_pressed(KeyCode::Delete) {
            Some(None)
        } else {
            // Several keys in one frame: the one that can be bound.
            let mut list: Vec<KeyCode> =
                pressed.iter().copied().filter(|k| keys::bindable(*k)).collect();
            list.sort_by_key(|k| *k as u32);
            list.first().map(|k| Some(*k))
        };
        match key {
            Some(key) => {
                let conflict = keys.bind(action, slot, key);
                self.capture = None;
                self.key_note = match conflict {
                    Conflict::None => None,
                    Conflict::Swapped { other, given } => {
                        self.flash = Some((other, now));
                        let name = t(other.label());
                        let text = match given {
                            Some(g) => tw(
                                "Swapped: {action} is now on {key}.",
                                &[("action", &name), ("key", &keys::key_label(g))],
                            ),
                            None if keys.get(other).iter().all(Option::is_none) => tw(
                                "Taken from {action}, which now has no key.",
                                &[("action", &name)],
                            ),
                            None => tw("Taken from {action}.", &[("action", &name)]),
                        };
                        Some((action, text, now))
                    }
                };
            }
            None if !pressed.is_empty() => {
                self.key_note =
                    Some((action, t("That key can't be used: try another.").into(), now));
            }
            None => {
                if is_mouse_button_pressed(MouseButton::Left)
                    && !last_caps.iter().any(|r| r.contains(ui.mouse()))
                {
                    self.capture = None;
                }
            }
        }
        false
    }

    pub fn draw(&mut self, ui: &mut Ui, config: &mut Config, in_match: bool) -> SettingsAction {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let startup_msaa = *self.startup_msaa.get_or_insert(config.msaa);
        let advanced = self.advanced;
        // "What's new" is over the settings: nothing under it reacts.
        let dialog =
            (self.whats_new.is_some() || crate::avatars::is_open()).then(|| ui.suspend_input());
        let esc_used = dialog.is_none() && self.capture_keys(ui, &mut config.keys);
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.5));

        let w = WIDTH.min(sw - 2.0 * MARGIN);
        let inner = w - 2.0 * PAD;
        // Measure every tab (with "Advanced" open): the tallest one fixes
        // where the panel's top is, so neither changing tabs nor opening that
        // section moves it. The tab shown sets the panel's height. The
        // Controls tab, much longer, takes the room below that top and
        // scrolls.
        let mut tallest = 0.0f32;
        for tab in TABS.into_iter().filter(|t| *t != Tab::Controls) {
            let mut rows = Rows::measure(inner);
            self.rows(tab, ui, config, in_match, startup_msaa, true, &mut rows);
            tallest = tallest.max(rows.finish());
        }
        let mut rows = Rows::measure(inner);
        self.rows(self.tab, ui, config, in_match, startup_msaa, advanced, &mut rows);
        let content = rows.finish();
        let cards = rows.cards;
        let room = (sh - 2.0 * MARGIN - HEADER_H - FOOTER_H).max(80.0);
        let top = ((sh - HEADER_H - FOOTER_H - tallest.min(room)) / 2.0).max(MARGIN);
        let room = if self.tab == Tab::Controls {
            (sh - MARGIN - top - HEADER_H - FOOTER_H).max(80.0)
        } else {
            room
        };
        let body_h = content.min(room);
        let r = Rect::new((sw - w) / 2.0, top, w, HEADER_H + body_h + FOOTER_H);
        ui.panel(r);
        let x = r.x + PAD;

        // Title.
        crate::icons::draw(Icon::Gear, vec2(x + 13.0, r.y + 33.0), 26.0, GOLD);
        ui.text(t("Settings"), x + 38.0, r.y + 20.0, 24.0, GOLD, true);

        // Tabs.
        let tabs = [
            (Tab::Graphics, t("Graphics")),
            (Tab::Game, t("Game")),
            (Tab::Controls, t("Controls")),
            (Tab::Sound, t("Sound")),
        ];
        let tab_w = (inner - 8.0 * (tabs.len() - 1) as f32) / tabs.len() as f32;
        for (k, (tab, label)) in tabs.into_iter().enumerate() {
            let tr = Rect::new(x + k as f32 * (tab_w + 8.0), r.y + 60.0, tab_w, 34.0);
            if ui.tab(tr, label, self.tab == tab) && self.tab != tab {
                self.tab = tab;
                self.scroll = 0.0;
                self.capture = None;
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

        // Buttons. On the Controls tab, the reset is for the keys only.
        let by = r.y + r.h - 22.0 - 40.0;
        let armed = self.reset_armed.is_some_and(|at| get_time() - at < 4.0);
        let (ask, idle) = if self.tab == Tab::Controls {
            (t("Reset every key?"), t("Reset all keys"))
        } else {
            (t("Reset everything?"), t("Reset to defaults"))
        };
        let reset_label = if armed { ask } else { idle };
        // As wide as the longer of its two labels, so it doesn't jump.
        let reset_w = [ask, idle]
            .iter()
            .map(|l| ui.text_size(l, 17.0, true).x + 40.0)
            .fold(190.0, f32::max)
            .min(280.0);
        let reset = Rect::new(x, by, reset_w, 40.0);
        let nothing_to_reset = self.tab == Tab::Controls && config.keys == Bindings::default();
        let pressed = ui.button(reset, reset_label, !nothing_to_reset);
        if armed {
            rrect_lines(reset, 6.0, 1.6, ALERT);
        }
        if pressed {
            if armed {
                self.reset_armed = None;
                if self.tab == Tab::Controls {
                    config.keys = Bindings::default();
                    self.key_note = None;
                    self.capture = None;
                } else {
                    reset_defaults(config);
                }
            } else {
                self.reset_armed = Some(get_time());
            }
        } else if !armed {
            self.reset_armed = None;
        }
        if armed && ui.hovering(reset) {
            let tip = if self.tab == Tab::Controls {
                t("Click again to put every key back")
            } else {
                t("Click again to reset every setting")
            };
            ui.tooltip(tip, reset);
        }
        let mut action = SettingsAction::None;
        if ui.primary_button(Rect::new(x + inner - 190.0, by, 190.0, 40.0), t("Done"))
            || (is_key_pressed(KeyCode::Escape)
                && ui.focus.is_none()
                && !esc_used
                && dialog.is_none())
        {
            if config.server.trim().is_empty() {
                config.server = DEFAULT_SERVER.into();
            }
            config.server = config.server.trim().to_string();
            self.stop_capture();
            action = SettingsAction::Close;
        }
        if std::mem::take(&mut self.restart) {
            action = SettingsAction::Restart;
        }
        if let Some(saved) = dialog {
            ui.resume_input(saved);
            if self.whats_new.as_mut().is_some_and(|w| w.draw(ui)) {
                self.whats_new = None;
            }
            // The chosen picture is saved (and sent to the account) by the game.
            crate::avatars::draw_picker(ui);
        }
        keys::set_capturing(self.capture.is_some());
        action
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
                    Quality::High => "Soft shadows from every lamp, glossy halls, halos.",
                    Quality::Ultra => {
                        "Everything, with reflections in polished floors: for gaming PCs."
                    }
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
                let hint = match config.keys.short(Action::Fullscreen) {
                    "" => String::new(),
                    key => tw("Also with {key}.", &[("key", &key)]),
                };
                if switch_row(rows, ui, t("Full screen"), &hint, &mut config.fullscreen) {
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
                if let Some((_, c)) = rows.row(
                    ui,
                    t("Profile picture"),
                    t("Shown by your name at the table and in your profile."),
                    Control::Wide(44.0),
                ) {
                    let mine = crate::avatars::mine();
                    crate::avatars::draw(&mine, vec2(c.x + 22.0, c.y + 22.0), 21.0);
                    let b = Rect::new(c.x + 54.0, c.y + 6.0, (c.w - 54.0).min(150.0), 32.0);
                    if ui.button(b, t("Choose…"), true) {
                        crate::avatars::open();
                    }
                }
                if let Some((_, c)) = rows.row(ui, t("Language"), "", Control::Wide(32.0)) {
                    let names: Vec<&str> = Lang::ALL.iter().map(|l| short_name(*l)).collect();
                    let sel = Lang::ALL.iter().position(|l| *l == config.language).unwrap_or(0);
                    if let Some(i) = ui.segmented(c, &names, sel) {
                        config.language = Lang::ALL[i];
                    }
                }

                switch_row(
                    rows,
                    ui,
                    t("Opponents' remarks"),
                    t("The computer opponents comment on the game in speech bubbles."),
                    &mut config.opponent_talk,
                );

                rows.heading(ui, t("Aiming"), None);
                let hint = match config.guide {
                    AimGuide::Full => t(
                        "Shows where the object ball and the cue ball go (only in free games and practice).",
                    ),
                    AimGuide::Standard => {
                        t("Shows the contact point; judging the cut is up to you.")
                    }
                    AimGuide::Short => t("Only a short line from the cue ball."),
                    AimGuide::Off => t("No lines on the table: aim by eye, along the cue."),
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
                    t("Top view only"),
                    t("The table is always seen from above: no 3D view, no turning."),
                    &mut config.top_view_only,
                );
                // The 3D camera's own settings, idle while it is locked above.
                let free = !config.top_view_only;
                let hint = if free {
                    t("Off: it stays where you put it.")
                } else {
                    t("Not with the top view only.")
                };
                if let Some((row, c)) = rows.row_ex(
                    ui,
                    t("Camera follows the cue"),
                    hint,
                    MUTED,
                    Control::Switch,
                    free,
                    true,
                ) {
                    toggle_on_click(ui, row, c, &mut config.camera_follow, free);
                }
                // Only the camera that follows the cue pulls back.
                let follows = config.camera_follow && free;
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
                let hint = if free {
                    t("Broadcast angles for other players' games and replays.")
                } else {
                    t("Not with the top view only.")
                };
                if let Some((row, c)) = rows.row_ex(
                    ui,
                    t("TV camera when watching"),
                    hint,
                    MUTED,
                    Control::Switch,
                    free,
                    true,
                ) {
                    toggle_on_click(ui, row, c, &mut config.tv_camera, free);
                }

                self.updates_rows(ui, config, in_match, rows);

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
            Tab::Controls => {
                for (heading, actions) in KEY_SECTIONS {
                    rows.heading(ui, t(heading), None);
                    for action in actions {
                        self.key_row(rows, ui, &mut config.keys, *action);
                    }
                }
                rows.heading(ui, t("Fixed keys"), None);
                for (label, key) in [
                    (t("Menu, back and cancel"), KeyCode::Escape),
                    (t("Confirm (forms, the cue ball in hand)"), KeyCode::Enter),
                ] {
                    if let Some((_, c)) = rows.row(ui, label, "", Control::Keys) {
                        let cap = Rect::new(c.x, c.y, (c.w - 46.0) / 2.0, c.h);
                        let name = if key == KeyCode::Escape { "Esc" } else { "Enter" };
                        key_cap(ui, cap, name, false, false);
                        if ui.hovering(cap) {
                            ui.tooltip(t("This key can't be changed."), cap);
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

impl SettingsScreen {
    /// An action's row on the Controls tab: its two keys (click one to
    /// change it) and a button back to the defaults.
    fn key_row(&mut self, rows: &mut Rows, ui: &mut Ui, keys: &mut Bindings, action: Action) {
        let now = get_time();
        let waiting = self.capture.filter(|(a, _)| *a == action).map(|(_, s)| s);
        let note = self
            .key_note
            .as_ref()
            .filter(|(a, _, at)| *a == action && now - at < 6.0)
            .map(|(_, n, _)| n.clone());
        let (hint, color) = match (&note, waiting) {
            (Some(n), _) => (n.as_str(), GOLD),
            (None, Some(_)) => (t("Press a key · Esc cancels · Backspace clears"), GOLD),
            (None, None) => ("", MUTED),
        };
        let Some((row, c)) =
            rows.row_ex(ui, t(action.label()), hint, color, Control::Keys, true, false)
        else {
            return;
        };
        if let Some((a, at)) = self.flash
            && a == action
            && now - at < 2.0
        {
            let k = (1.0 - (now - at) / 2.0) as f32;
            rrect(
                Rect::new(row.x + 3.0, row.y + 3.0, row.w - 6.0, row.h - 6.0),
                6.0,
                Color::new(GOLD.r, GOLD.g, GOLD.b, 0.14 * k),
            );
        }
        let reset = Rect::new(c.x + c.w - 30.0, c.y, 30.0, 30.0);
        let cap_w = (c.w - 46.0) / 2.0;
        let slots = keys.get(action);
        for (slot, key) in slots.into_iter().enumerate() {
            let r = Rect::new(c.x + slot as f32 * (cap_w + 8.0), c.y, cap_w, c.h);
            self.cap_rects.push(r);
            let on = waiting == Some(slot);
            let label = match key {
                _ if on => t("Press a key…"),
                Some(k) => keys::key_label(k),
                None => "—",
            };
            if key_cap(ui, r, label, on, true) {
                self.capture = if on { None } else { Some((action, slot)) };
                self.key_note = None;
                ui.focus = None;
            }
            if !on && ui.hovering(r) {
                let tip =
                    if key.is_some() { t("Click to change") } else { t("Click to add a key") };
                ui.tooltip(tip, r);
            }
        }
        let defaults = Bindings::default().label(action);
        let tip = if defaults.is_empty() {
            t("Back to no key").to_string()
        } else {
            tw("Back to {keys}", &[("keys", &defaults)])
        };
        if ui.icon_button(reset, Icon::Refresh, &tip, !keys.is_default(action)) {
            keys.reset(action);
            self.key_note = None;
            if self.capture.is_some_and(|(a, _)| a == action) {
                self.capture = None;
            }
        }
    }

    /// The Updates section (Game tab): automatic updates, this version,
    /// what the updater is doing, and "What's new".
    fn updates_rows(&mut self, ui: &mut Ui, config: &mut Config, in_match: bool, rows: &mut Rows) {
        rows.heading(ui, t("Updates"), None);
        if !crate::channel::STORE {
            switch_row(
                rows,
                ui,
                t("Update automatically"),
                t("New versions download in the background and install when you restart."),
                &mut config.auto_update,
            );
        }
        let status = update::status();
        let (line, bad) = update::describe(&status);
        let label = tw("Version {v}", &[("v", &update::VERSION)]);
        let color = if bad { ALERT } else { MUTED };
        let Some((_, c)) = rows.row_ex(ui, &label, &line, color, Control::Wide(32.0), true, false)
        else {
            return;
        };
        let half = (c.w - 8.0) / 2.0;
        if ui.button(Rect::new(c.x, c.y, half, c.h), t("What's new"), true) {
            self.whats_new = Some(WhatsNew::all());
        }
        let second = Rect::new(c.x + half + 8.0, c.y, half, c.h);
        match status {
            Status::Ready { .. } if !in_match => {
                if ui.primary_button(second, t("Restart now")) {
                    self.restart = true;
                }
            }
            Status::Ready { .. } => {
                ui.button(second, t("Restart now"), false);
                if ui.hovering(second) {
                    ui.tooltip(t("After the game, from the main menu."), second);
                }
            }
            Status::Available { .. } => {
                if ui.button(second, t("Download"), true) {
                    crate::util::open_url(update::RELEASES);
                }
            }
            _ => {
                let can = update::allowed() && !update::busy();
                if ui.button(second, t("Check now"), can) {
                    update::check_now();
                }
            }
        }
    }
}

/// A key on its cap: `label`, waiting for a key (`waiting`, pulsing), or
/// greyed out (`enabled` false: a fixed key). Returns true when clicked.
fn key_cap(ui: &mut Ui, r: Rect, label: &str, waiting: bool, enabled: bool) -> bool {
    ui.block(r);
    let hover = enabled && ui.hovering(r);
    let a = if enabled { 1.0 } else { 0.5 };
    // A key: a darker edge underneath, a slightly lighter face.
    rrect(Rect::new(r.x, r.y + 2.0, r.w, r.h), 6.0, Color::new(0.0, 0.0, 0.0, 0.4 * a));
    let face = if hover || waiting {
        Color::new(0.20, 0.14, 0.09, a)
    } else {
        Color::new(0.13, 0.09, 0.06, a)
    };
    crate::ui::rrect_gradient(r, 6.0, crate::ui::shade(face, 0.06), crate::ui::shade(face, -0.12));
    let edge = if waiting {
        let pulse = ((get_time() * 4.0).sin() * 0.5 + 0.5) as f32;
        Color::new(GOLD.r, GOLD.g, GOLD.b, 0.45 + 0.55 * pulse)
    } else if hover {
        GOLD
    } else {
        Color::new(BORDER.r, BORDER.g, BORDER.b, a)
    };
    rrect_lines(r, 6.0, if waiting { 1.8 } else { 1.2 }, edge);
    let (size, bold, color) = if waiting {
        (13.0, false, GOLD)
    } else if label == "—" {
        (15.0, false, MUTED)
    } else {
        (15.0, true, Color::new(CREAM.r, CREAM.g, CREAM.b, a))
    };
    let (text, size) = ui.fitted(label, size, r.w - 12.0, bold);
    ui.text_centered(&text, r.x + r.w / 2.0, r.y + (r.h - size) / 2.0 - 1.0, size, color, bold);
    hover && ui.clicked(r)
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
        last_version: keep.last_version,
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
    /// Two key caps and a reset button (Controls tab).
    Keys,
}

impl Control {
    /// Its size in rows `w` wide.
    fn size(self, w: f32) -> Vec2 {
        match self {
            Control::Wide(h) => vec2((w * 0.5).min(330.0), h),
            Control::Switch => vec2(44.0, 24.0),
            Control::Keys => vec2((w * 0.5).min(330.0), 30.0),
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
pub(crate) fn clip(r: Option<Rect>) {
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
