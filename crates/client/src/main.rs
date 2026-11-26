// No console window behind the game on Windows (release builds).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Snook Legend desktop client.
//!
//! Rendering uses macroquad (OpenGL 2.1 / GLES 2), so the game is small and
//! runs on practically any PC, including old integrated graphics.

mod aa;
mod advanced;
mod analysis;
mod arena_hall;
mod assess;
mod audio;
mod avatars;
mod banter;
mod camera;
mod channel;
mod collection;
mod config;
mod coords;
mod correspondence;
mod countries;
mod country_names;
mod cues;
mod cup;
mod dev;
mod dm;
mod effects;
mod emoji;
mod emoji_tray;
mod flags;
mod fonts;
mod game;
mod gfx;
mod hall_textures;
mod halls;
mod hud;
mod i18n;
mod icon;
mod icons;
mod inbox;
mod instance;
mod intro;
mod keys;
mod ladder;
mod lan;
mod languages;
mod legal;
mod medal_art;
mod medal_detail;
mod medals;
mod menu;
mod more_halls;
mod msstore;
mod net;
mod notify;
mod observer;
mod online;
mod opponents;
mod paint;
mod perks;
mod play;
mod profile;
mod ranking;
mod reactions;
mod record;
mod report;
mod reveal;
mod room;
mod rulesui;
mod saved;
mod scene;
mod scoreboard;
mod settings;
mod setup;
mod shop;
mod showroom;
mod swatch;
mod sync;
mod table_social;
mod tables;
mod tabrow;
mod textures;
mod theme_art;
mod time_attack;
mod ui;
mod update;
mod util;
mod versus;
mod whatsnew;

use macroquad::prelude::*;
use snook_legend_core::{BallId, Difficulty, Rules, Table};

use crate::camera::{CameraMode, CameraRig};
use crate::config::Config;
use crate::game::Match;
use crate::hud::{Hud, HudAction};
use crate::lan::{LanAction, LanScreen};
use crate::menu::{Menu, MenuAction};
use crate::online::{Online, OnlineAction};
use crate::play::Controls;
use crate::profile::{ProfileAction, ProfileScreen};
use crate::ranking::{Ranking, RankingAction};
use crate::scene::{Overlay, Scene};
use crate::settings::{SettingsAction, SettingsScreen};
use crate::setup::{Setup, SetupAction};
use crate::ui::Ui;

/// Who plays a match.
#[derive(Debug, Clone, PartialEq)]
pub enum MatchMode {
    VsComputer(opponents::Opponent),
    HotSeat,
    /// Computer against computer (attract mode and testing).
    Demo,
}

/// Choices made before an offline match.
#[derive(Debug, Clone)]
pub struct MatchSettings {
    pub mode: MatchMode,
    pub names: [String; 2],
    pub guide: config::AimGuide,
    /// Seconds per shot, if timed.
    pub shot_clock: Option<u32>,
    /// A game of the ladder: its rung (0 is the first).
    pub ladder: Option<u8>,
    /// The rules the game is played under.
    pub rules: snook_legend_core::Rules,
}

impl MatchSettings {
    pub(crate) fn from_config(mode: MatchMode, c: &Config) -> Self {
        MatchSettings {
            mode,
            // An empty name field means the default name (the field itself is
            // left empty while the player types).
            names: [name_or(&c.player_name, "Player"), name_or(&c.second_player_name, "Player 2")],
            guide: c.guide,
            shot_clock: (c.shot_clock > 0).then_some(c.shot_clock),
            ladder: None,
            rules: c.rules(),
        }
    }
}

/// Command-line options (mostly for development and testing).
#[derive(Default)]
struct LaunchOptions {
    start: Option<MatchMode>,
    top_view: bool,
    screenshot: Option<String>,
    frames: u32,
    show_fps: bool,
    online: bool,
    settings: bool,
    on_eight: bool,
    screen: Option<String>,
}

fn parse_args() -> LaunchOptions {
    let mut o = LaunchOptions { frames: 120, ..Default::default() };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--demo" => o.start = Some(MatchMode::Demo),
            "--hotseat" => o.start = Some(MatchMode::HotSeat),
            "--vs" => {
                let who = args.next().unwrap_or_default();
                let opponent = match who.as_str() {
                    "easy" => opponents::Opponent::Level { level: Difficulty::Easy },
                    "hard" => opponents::Opponent::Level { level: Difficulty::Hard },
                    code if snook_legend_core::personalities::personality(code).is_some() => {
                        opponents::Opponent::personality(code)
                    }
                    _ => opponents::Opponent::Level { level: Difficulty::Medium },
                };
                o.start = Some(MatchMode::VsComputer(opponent));
            }
            "--top-view" => o.top_view = true,
            "--fps" => o.show_fps = true,
            "--online" => o.online = true,
            "--settings" => o.settings = true,
            "--on-eight" => o.on_eight = true,
            "--screen" => o.screen = args.next(),
            "--window" => {
                args.next();
            }
            "--screenshot" => o.screenshot = args.next(),
            "--frames" => o.frames = args.next().and_then(|s| s.parse().ok()).unwrap_or(120),
            "--help" | "-h" => {
                println!(
                    "Snook Legend\n\n  --vs <easy|medium|hard|name>  play the computer (a level or a personality)\n  --hotseat                two players\n  \
                     --demo                   computer vs computer\n  --top-view               overhead camera\n  \
                     --online                 open the online lounge\n  \
                     --settings               open the settings\n  \
                     --screen <name>          open a screen (lan, setup, ranking, avatars)\n  \
                     --screenshot <file.png>  save a screenshot after --frames <n> frames and quit"
                );
                std::process::exit(0);
            }
            other => eprintln!("ignoring unknown argument `{other}`"),
        }
    }
    o
}

fn window_conf() -> macroquad::conf::Conf {
    // Before any window: the updater runs `--version` on a new executable
    // to make sure it works (see update.rs). Keep this answer as it is.
    if std::env::args().skip(1).any(|a| a == "--version" || a == "-V") {
        println!("snook-legend {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }
    // One game at a time: a second start shows the first and ends.
    if !instance::claim() {
        std::process::exit(0);
    }
    update::init();
    let config = Config::load();
    // `--window WxH` (development): start with this window size.
    let args: Vec<String> = std::env::args().collect();
    let size = args
        .iter()
        .position(|a| a == "--window")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.split_once('x'))
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .unwrap_or((1280, 760));
    let window = Conf {
        window_title: snook_legend_proto::GAME_NAME.into(),
        window_width: size.0,
        window_height: size.1,
        // Safe graphics mode (troubleshooting) also turns multisampling off.
        sample_count: if std::env::var_os("SNOOK_LEGEND_SAFE_GRAPHICS").is_some() {
            1
        } else {
            config.msaa
        },
        fullscreen: config.fullscreen,
        window_resizable: true,
        high_dpi: false,
        icon: Some(icon::window_icon()),
        platform: macroquad::miniquad::conf::Platform {
            // Vertical sync: no tearing, and no burning the GPU/battery for
            // frames the screen can't show.
            swap_interval: Some(1),
            ..Default::default()
        },
    };
    macroquad::conf::Conf {
        miniquad_conf: window,
        update_on: None,
        default_filter_mode: FilterMode::Linear,
        // macroquad's defaults: large per-draw-call buffers cost a lot of
        // video memory and upset some drivers (big meshes are split instead).
        draw_call_vertex_capacity: 10_000,
        draw_call_index_capacity: 5_000,
    }
}

/// Which full-screen page is open over the 3D table.
enum Screen {
    Menu,
    Setup(Setup),
    /// Playing the computer: a free game or the ladder.
    Versus(versus::Versus),
    /// The online lounge (or the sign-in form).
    Online,
    Lan,
    /// The lounge of a LAN game.
    LanLounge,
    Ranking,
    Profile,
    Shop,
    /// What the staff and the game told the player.
    Inbox,
}

/// A LAN game: the server this computer hosts (if any) and the connection.
struct LanSession {
    host: Option<snook_legend_server::lan::LanHost>,
    net: Online,
}

#[macroquad::main(window_conf)]
async fn main() {
    let options = parse_args();
    let mut ui = Ui::new();
    // Show something while the textures are generated.
    clear_background(Color::new(0.05, 0.035, 0.025, 1.0));
    {
        let size = (screen_height() * 0.22).clamp(96.0, 220.0);
        icon::draw((screen_width() - size) / 2.0, screen_height() / 2.0 - size * 0.75, size);
        ui.text_centered(
            &snook_legend_proto::GAME_NAME.to_uppercase(),
            screen_width() / 2.0,
            screen_height() / 2.0 + size * 0.35,
            32.0,
            ui::GOLD,
            true,
        );
    }
    next_frame().await;

    let mut config = Config::load();
    i18n::set_lang(config.language);
    avatars::set_mine(&config.avatar);
    // Development aid: SNOOK_LEGEND_AVATARS opens the settings with the
    // picture picker.
    let pick_avatar = crate::dev::flag("SNOOK_LEGEND_AVATARS");
    if pick_avatar {
        avatars::open();
    }
    // The first start on a new version: what changed (on the main menu).
    let mut whats_new = whatsnew::WhatsNew::after_update(&config.last_version);
    // Development aid: SNOOK_LEGEND_WHATS_NEW=1 shows the notes, =0.2.0 that
    // version's, =rules the rules (=official their official tab).
    if let Some(v) = crate::dev::var("SNOOK_LEGEND_WHATS_NEW") {
        whats_new = if v == "rules" || v == "official" {
            let rules = if v == "official" { Rules::Official } else { Rules::Brazilian };
            Some(whatsnew::WhatsNew::rules(rules))
        } else {
            whatsnew::WhatsNew::of(&v).or_else(|| Some(whatsnew::WhatsNew::all()))
        };
    } else if options.screenshot.is_some() {
        whats_new = None;
    }
    if options.screenshot.is_some() && !crate::dev::flag("SNOOK_LEGEND_POINTER") {
        ui::park_pointer();
    }
    if config.last_version != update::VERSION {
        config.last_version = update::VERSION.to_string();
        config.save();
    }
    let mut scene = Scene::new(
        Table::default(),
        config.quality,
        config.focused_scene,
        &scene::Looks::of(&config.cloth, &config.balls).themed(&config.table, &config.hall),
    );
    let mut retired: Vec<Scene> = Vec::new();
    let sounds = audio::Sounds::load().await;
    // Sounds of the interface, not of a game (an invitation's chime).
    let mut ui_sounds: Vec<game::ImpactSound> = Vec::new();
    let mut menu = Menu::default();
    let mut hud = Hud::new(config.show_help);
    let mut rig = CameraRig::default();
    rig.follow_cue = config.camera_follow;
    rig.reset();
    let mut controls = Controls::default();
    let mut current: Option<Match> = None;
    // The saved game against the computer (object balls left), and what the
    // last save was taken at.
    let mut saved_balls = saved::load().map(|s| s.balls_left());
    let mut save_key = None;
    // A new game against the computer asked for while one is saved: it
    // waits for the player to say which one (the new one takes the save's
    // place).
    let mut replace_save: Option<MatchSettings> = None;
    // The game saved that a new one would replace is the tournament's (lost
    // if it is replaced).
    let mut replace_loses_cup = false;
    // The new game asked for is a tournament's (it begins the round).
    let mut replace_cup = false;
    // The online match came from the LAN game (not the global server).
    let mut match_from_lan = false;
    let mut screen = Screen::Menu;
    // A screen that just opened fades in (0 to 1).
    let mut screen_fade = 1.0f32;
    // The global server: signs in in the background when a session was saved.
    let mut online = Online::new(&config);
    let mut lan_screen: Option<LanScreen> = None;
    let mut lan: Option<LanSession> = None;
    let mut ranking = Ranking::default();
    let mut profile = ProfileScreen::default();
    let mut shop = shop::ShopScreen::default();
    let mut inbox = inbox::InboxScreen::default();
    // The shop item being tried on the 3D table, and since when.
    // An item of the shop looked at closely (see `showroom`).
    let mut showroom: Option<showroom::Showroom> = None;
    // Winning effects playing over the table.
    let mut win_effects: Vec<effects::WinEffect> = Vec::new();
    // System notifications for a player in another window.
    let mut notifier = notify::Notifier::default();
    // Where the profile and ranking go back to.
    let mut back_to_lounge = false;
    let mut settings_open = options.settings || pick_avatar;
    let mut settings_screen = SettingsScreen::default();
    settings_screen.dev_tab();
    if options.top_view {
        rig.mode = CameraMode::Top;
    }
    // Development aid (screenshots): SNOOK_LEGEND_VIEW=3d starts in the 3D view.
    if crate::dev::var("SNOOK_LEGEND_VIEW").is_some_and(|v| v == "3d") {
        rig.enter_3d();
    }
    // Development aid: SNOOK_LEGEND_SAMPLE_REPLAY=1 watches a recorded game.
    if crate::dev::flag("SNOOK_LEGEND_SAMPLE_REPLAY") {
        current = Some(Match::replay_of(&game::Video::sample(11, 30), config.guide));
    }
    if let Some(mode) = options.start {
        let mut m = Match::new(&MatchSettings::from_config(mode, &config), game::clock_seed());
        // SNOOK_LEGEND_SOFT_BREAK=1: a soft break first (the choice it leaves).
        if crate::dev::flag("SNOOK_LEGEND_SOFT_BREAK") {
            m.debug_soft_break();
        }
        if options.on_eight {
            m.debug_on_eight();
        }
        // Development aid: SNOOK_LEGEND_RESIGN=1 resigns at once (the game-over panel).
        if crate::dev::flag("SNOOK_LEGEND_RESIGN") {
            m.debug_shots();
            m.resign();
        }
        current = Some(m);
    }
    if options.online {
        online.connect();
        screen = Screen::Online;
    }
    match options.screen.as_deref() {
        Some("lan") => {
            lan_screen = Some(LanScreen::new());
            screen = Screen::Lan;
        }
        Some("setup") => screen = Screen::Versus(versus::Versus::new(versus::Tab::Free, &config)),
        Some("ladder") => {
            screen = Screen::Versus(versus::Versus::new(versus::Tab::Ladder, &config))
        }
        Some("cup") => screen = Screen::Versus(versus::Versus::new(versus::Tab::Cup, &config)),
        Some("drills") => {
            screen = Screen::Versus(versus::Versus::new(versus::Tab::Drills, &config))
        }
        Some("challenge") => {
            screen = Screen::Versus(versus::Versus::new(versus::Tab::Daily, &config))
        }
        Some("hotseat") => screen = Screen::Setup(Setup::new()),
        // Development aid (screenshots): the settings with the picture picker.
        Some("avatars") => {
            settings_open = true;
            avatars::open();
        }
        Some(name) if name.starts_with("ghost:") => {
            online.connect();
            screen = Screen::Versus(versus::Versus::ghost_of(
                &name["ghost:".len()..],
                &config,
                &mut online,
            ));
        }
        // Development aid (screenshots): `--screen showroom:<item>` opens an
        // item of the shop in the showroom (see `Showroom::dev`).
        Some(name) if name.starts_with("showroom:") => {
            online.connect();
            shop.open(&mut online);
            showroom = showroom::Showroom::open(&name["showroom:".len()..], &scene.table);
            if let Some(s) = showroom.as_mut() {
                s.dev();
            }
            screen = Screen::Shop;
        }
        Some("collection") => {
            online.connect();
            shop.open_collection(&mut online);
            screen = Screen::Shop;
        }
        Some(tab) if tab.starts_with("shop") => {
            online.connect();
            shop.open(&mut online);
            shop.show_tab(tab);
            screen = Screen::Shop;
        }
        Some(board @ ("ranking" | "ranking-friends" | "ranking-doubles")) => {
            ranking.dev(board);
            ranking.open(&mut online);
            screen = Screen::Ranking;
        }
        Some(
            tab @ ("players" | "tables" | "tournaments" | "friends" | "daily" | "doubles"
            | "auctions" | "credits"),
        ) => {
            online.connect();
            online.tab = match tab {
                "players" | "auctions" | "credits" => online::Tab::Players,
                "tables" => online::Tab::Tables,
                "tournaments" => online::Tab::Tournaments,
                "daily" => online::Tab::Daily,
                "doubles" => online::Tab::Doubles,
                _ => online::Tab::Friends,
            };
            screen = Screen::Online;
        }
        Some(name) if name.starts_with("dm:") => {
            online.connect();
            online.tab = online::Tab::Friends;
            screen = Screen::Online;
        }
        // An achievement's card and toasts over the menu.
        Some("toasts") => online.dev_toasts(),
        Some("inbox") => {
            online.connect();
            inbox.open(&mut online);
            screen = Screen::Inbox;
        }
        Some("clock") => {
            screen = Screen::Versus(versus::Versus::new(versus::Tab::TimeAttack, &config))
        }
        Some(name) if name.starts_with("profile:") => {
            profile.open(&name["profile:".len()..], &mut online);
            screen = Screen::Profile;
        }
        _ => {}
    }
    // The ladder against the computer, and the claim of its reward.
    let mut ladder_progress = ladder::Progress::load();
    let mut claimer = ladder::Claimer::default();
    // A game waiting for the server before it starts (and since when): the
    // tour's final, or a drill session for the tables.
    let mut waiting: Option<(Waiting, f64)> = None;
    // Daily attempts and ranked drill sessions left while signed out:
    // (account, message), sent once that account is signed in again.
    let mut unsent_runs: Vec<(String, snook_legend_proto::ClientMsg)> = Vec::new();
    // The tournament against the computer's opponents, if one is going on.
    let mut cup = cup::Cup::load();
    // The medals of the tournaments against the computer.
    let mut medal_book = medals::MedalBook::load();
    let menu_rack = snook_legend_core::rules::rack(&scene.table, 2024);
    let vignette = vignette_texture();

    // First run: measure the frame rate on the menu and pick graphics that
    // stay smooth on this computer.
    let mut probe = (0.0f32, 0u32);
    let mut toast: Option<(String, f32)> = None;
    // Fades the screen in from black after a scene change.
    let mut fade = 1.0f32;
    let mut was_in_match = current.is_some();
    let mut hud_opacity = 1.0f32;
    let mut frame = 0u32;
    // Development aid: SNOOK_LEGEND_PROFILE prints the CPU time of a frame (without
    // waiting for the GPU) every 300 frames.
    let profiling = crate::dev::flag("SNOOK_LEGEND_PROFILE");
    let mut cpu = (0.0f64, 0.0f64, 0u32);
    // Development aid (screenshots): SNOOK_LEGEND_INTRO shows a match's
    // introduction (made-up players) over whatever is on screen.
    // ("computer": the introduction of the game against the computer
    // instead, held once in; SNOOK_LEGEND_NO_INTRO: none against it.)
    let dev_intro_var = crate::dev::var("SNOOK_LEGEND_INTRO");
    let mut dev_intro =
        dev_intro_var.as_deref().filter(|v| *v != "computer").map(|_| intro::Intro::sample());
    let computer_intro = crate::dev::var("SNOOK_LEGEND_NO_INTRO").is_none()
        && dev_intro_var.as_deref().is_none_or(|v| v == "computer");
    let hold_computer_intro = dev_intro_var.as_deref() == Some("computer");
    // Development aid: SNOOK_LEGEND_RECORD=<file.mp4> records a video, one
    // frame per 1/60 s of the game's clock (see record.rs).
    let mut recorder = record::Recorder::from_env();
    if recorder.is_some() {
        ui::park_pointer();
    }
    loop {
        let frame_start = std::time::Instant::now();
        let dt = get_frame_time().min(0.1);
        ui::update_scale(config.ui_size);
        // Every frame starts with nothing fading (a screen that opened sets
        // its fade while it is drawn).
        paint::set_screen_fade(1.0);
        paint::set_opacity(1.0);
        ui.begin();
        // The chat's dialog (drawn near the end, over any screen or game)
        // holds the keyboard too: no Esc, Space or arrow reaches the game,
        // the HUD or the screen under it. It gets it back to draw itself.
        let chat_dialog = online.chat_dialog_open && !settings_open;
        if chat_dialog {
            ui.focus = Some(CHAT_DIALOG_FOCUS);
            ui.wants_keyboard = true;
        }
        // Under the invitations' cards (drawn last, over everything) nothing
        // drawn before them takes the pointer: one click answers the card
        // only.
        let under_invites = online.invite_cards_cover(ui.mouse())
            || lan.as_ref().is_some_and(|l| l.net.invite_cards_cover(ui.mouse()));
        let invite_cover = under_invites.then(|| ui.suspend_input());
        // How far down the cards start: under the top bars of the screens,
        // or under what a game shows on the right (set while drawing it).
        let mut invite_top = 98.0;
        keys::install(&config.keys);
        // The settings may close without their Done button (an online game
        // starting): no key stays waited for.
        if !settings_open {
            settings_screen.stop_capture();
        }

        if keys::pressed_anywhere(keys::Action::Fullscreen, ui.wants_keyboard) {
            config.fullscreen = !config.fullscreen;
            set_fullscreen(config.fullscreen);
            config.save();
        }

        // Network traffic. Only the server that owns the current online match
        // gets it (and sends its messages).
        let online_match = current.as_ref().is_some_and(|m| m.is_online());
        let was_online_match = online_match;
        {
            let mut none = None;
            let global_slot = if online_match && match_from_lan { &mut none } else { &mut current };
            if online.update(dt, global_slot, &mut config) {
                config.save();
            }
            if !was_online_match && current.as_ref().is_some_and(|m| m.is_online()) {
                match_from_lan = false;
            }
            // A game of the official server that started while one on the
            // local network has the table (a search left running): resigned
            // at once, rather than lost later on the clock unseen.
            if let Some(m) = none.take()
                && m.is_online()
                && !m.is_spectating()
                && !correspondence::is_corr(&m)
            {
                online.send(snook_legend_proto::ClientMsg::Resign);
            }
        }
        if let Some(l) = lan.as_mut() {
            let mut none = None;
            let owned = current.as_ref().is_some_and(|m| m.is_online());
            let slot = if owned && !match_from_lan { &mut none } else { &mut current };
            l.net.update(dt, slot, &mut config);
            // The same the other way round: a local game that started while
            // one of the official server's has the table.
            if let Some(m) = none.take()
                && m.is_online()
                && !m.is_spectating()
            {
                l.net.send(snook_legend_proto::ClientMsg::Resign);
            }
            // Only a match that wasn't there before came from the local
            // network: one of the global server's stays its own (the slot
            // above was empty then, which once made it look new).
            if !owned && current.as_ref().is_some_and(|m| m.is_online()) {
                match_from_lan = true;
            }
        }
        // An online game took the table from a daily attempt or a drill
        // session: what it made still goes to the server.
        let displaced =
            [online.displaced.take(), lan.as_mut().and_then(|l| l.net.displaced.take())];
        for mut old in displaced.into_iter().flatten() {
            send_run(&mut old, &mut online, &mut unsent_runs);
        }
        update::tick(config.auto_update);
        // The new version's installer is running: it replaces this copy.
        if online.quit_for_update {
            config.save();
            break;
        }
        notifier.update();
        // Away in the lounge after ten minutes untouched (a minimised
        // window too: nothing moves in it).
        let away = notifier.idle() >= std::time::Duration::from_secs(10 * 60);
        online.away = away;
        if let Some(l) = lan.as_mut() {
            l.net.away = away;
        }
        for net in [Some(&mut online), lan.as_mut().map(|l| &mut l.net)].into_iter().flatten() {
            if let Some(text) = net.attention.take()
                && config.desktop_notifications
            {
                notifier.tell(&text);
            }
        }
        // The daily challenge: an attempt the server started begins here
        // (unless an online game has the table); its strokes go back once
        // the visit is over, and the server's count comes to the table.
        // (Kept, not dropped, while an online game has the table.)
        if !current.as_ref().is_some_and(|m| m.is_online())
            && let Some((day, seed)) = online.daily_started.take()
        {
            let name =
                online.me.as_ref().map_or(config.player_name.clone(), |me| me.username.clone());
            online.daily_result = None;
            if let Some(m) = current.as_mut() {
                send_run(m, &mut online, &mut unsent_runs);
            }
            current = Some(Match::daily(day, seed, &name, config.guide));
            hud = Hud::new(config.show_help);
            rig.reset();
            screen = Screen::Versus(versus::Versus::new(versus::Tab::Daily, &config));
        }
        // Against the clock: an attempt the server started begins here, and
        // a cleared one goes back to it.
        if time_attack::tick(&mut current, &mut online, &config) {
            hud = Hud::new(config.show_help);
            rig.reset();
            screen = Screen::Versus(versus::Versus::new(versus::Tab::TimeAttack, &config));
        }
        // A daily attempt or a ranked drill session that is over goes to
        // the server once its account is signed in (it waits meanwhile,
        // and the table says so); the server's count comes back to it.
        if let Some(m) = current.as_mut() {
            let over = m.daily.as_ref().is_some_and(|r| r.over)
                || m.drill.as_ref().is_some_and(|r| r.over);
            if over
                && signed_in_as(&online, run_account(m))
                && let Some(msg) = run_to_send(m)
            {
                online.send(msg);
            }
        }
        if let Some(run) = current.as_mut().and_then(|m| m.daily.as_mut())
            && let Some(result) = online.daily_result.take()
        {
            run.result = Some(result);
        }
        if let Some(run) = current.as_mut().and_then(|m| m.drill.as_mut())
            && let Some((code, result)) = online.drill_result.take()
            && code == run.code
        {
            run.result = Some(result);
        }
        // Runs left while signed out: sent once their account is back.
        if let Some(me) = online.account().filter(|_| online.live()) {
            unsent_runs.retain(|(account, msg)| {
                let back = account.eq_ignore_ascii_case(me);
                if back {
                    online.send(msg.clone());
                }
                !back
            });
        }
        // A game waiting for the server: dealt (or set) as it says, or
        // started anyway (not counted) when it doesn't answer. Every way
        // the wait ends, a late ticket is no longer awaited.
        if current.as_ref().is_some_and(|m| m.is_online()) && waiting.is_some() {
            waiting = None;
            online.awaiting_ticket = false;
        }
        if let Some((w, since)) = &waiting {
            let late = get_time() - since > SERVER_WAIT || !online.signed_in();
            let name =
                online.me.as_ref().map_or(config.player_name.clone(), |me| me.username.clone());
            let started = match w {
                Waiting::Final(settings) => match online.ladder_ticket.take() {
                    Some((seed, breaker)) => {
                        let mut m = Match::new(settings, seed);
                        m.deal(seed, breaker);
                        if let Some(run) = m.ladder.as_mut() {
                            run.dealt = true;
                            run.account = online.account().unwrap_or_default().to_string();
                        }
                        Some(Some(m))
                    }
                    None if late || std::mem::take(&mut online.ticket_refused) => {
                        let mut m = Match::new(settings, game::clock_seed());
                        m.notify(
                            i18n::t("The server didn't deal the final: this game won't earn the reward.")
                                .to_string(),
                            true,
                        );
                        Some(Some(m))
                    }
                    None => None,
                },
                Waiting::Drill(code) => match online.drill_session.take() {
                    Some((c, session)) if c == *code => {
                        Some(Match::drill(code, &name, config.guide, session))
                    }
                    _ if late => Some(Match::drill(code, &name, config.guide, 0).map(|mut m| {
                        m.notify(
                            i18n::t("The server didn't answer: this session won't be ranked.")
                                .to_string(),
                            true,
                        );
                        m
                    })),
                    _ => None,
                },
            };
            if let Some(m) = started {
                let final_game = matches!(w, Waiting::Final(_));
                // No answer in time: the drill sessions asked for are given
                // up (a lost answer must not hold the next one back).
                if late && !final_game {
                    online.drill_starts = 0;
                }
                waiting = None;
                online.awaiting_ticket = false;
                if let Some(m) = m {
                    online.drill_result = None;
                    if let Some(old) = current.as_mut() {
                        send_run(old, &mut online, &mut unsent_runs);
                    }
                    current = Some(m);
                    hud = Hud::new(config.show_help);
                    rig.reset();
                    if final_game {
                        save_key = None;
                        screen = Screen::Menu;
                    }
                }
            }
        }
        // A stored game to watch again (from the profile).
        // Only over nothing (or another stored game): one that comes after
        // the player went on to a game of their own (a daily attempt, two
        // players) is dropped, not played over it.
        if !current.as_ref().is_some_and(|m| m.is_online())
            && let Some(replay) = online.replay.take()
            && current.as_ref().is_none_or(|m| m.is_replay())
        {
            current = Some(Match::replay_of(&replay, config.guide));
            hud = Hud::new(config.show_help);
            rig.reset();
        }
        let in_online_match = current.as_ref().is_some_and(|m| m.is_online());
        if in_online_match != was_online_match {
            hud = Hud::new(config.show_help);
            rig.reset();
            if in_online_match {
                settings_open = false;
            }
        }

        // A ladder game just ended: count it (a win on the last rung is
        // sent to the server for the reward).
        if let Some(m) = current.as_mut()
            && let Some(run) = m.ladder.as_mut()
            && !run.reported
            && let Some(over) = m.game.game_over()
        {
            run.reported = true;
            let won = over.winner == snook_legend_proto::ladder::PLAYER_SEAT;
            let rung = run.rung as usize;
            let before = ladder_progress.beaten;
            let recorded = ladder_progress.record(run, won);
            if recorded.changed {
                ladder_progress.save();
            }
            // A new game to claim with: an old refusal no longer stands.
            if recorded.to_claim {
                claimer.new_game();
            }
            // A new rung: say what comes next.
            if ladder_progress.beaten > before {
                let text = match snook_legend_proto::ladder::rung(rung + 1) {
                    Some(next) => i18n::tw(
                        "Stage {n} won! Next on the tour: {name}.",
                        &[("n", &(rung + 1)), ("name", &next.name)],
                    ),
                    None => i18n::t("You won the whole tour!").to_string(),
                };
                m.notify(text, true);
            }
        }
        // A rematch at this computer: the break alternates on from it (the
        // next game started is broken by the one who doesn't break this one).
        if let Some(m) = current.as_mut()
            && std::mem::take(&mut m.rematched)
            && m.game.rules() == snook_legend_core::Rules::Brazilian
        {
            config.next_breaker = 1 - (m.breaker % 2) as u8;
            config.save();
        }
        // The player's game of the tournament just ended: the round is
        // played out and the tournament moves on.
        if let Some(m) = current.as_mut()
            && let Some(c) = cup.as_mut()
            && m.is_against_computer()
            && c.playing == Some(m.seed())
            && let Some(over) = m.game.game_over()
        {
            let won = over.winner == snook_legend_proto::ladder::PLAYER_SEAT;
            c.report(won);
            c.save();
            let text = if c.finished() {
                if c.champion().is_some_and(|w| c.is_player(w)) {
                    i18n::t("You won the tournament!").to_string()
                } else {
                    match c.player_place() {
                        Some(n) => i18n::tw("You finished in place {n}.", &[("n", &n)]),
                        None => i18n::t("The tournament is over.").to_string(),
                    }
                }
            } else if c.player_in() {
                i18n::t("The round is over: see who you play next.").to_string()
            } else {
                i18n::t("You are out of the tournament.").to_string()
            };
            m.notify(text, true);
        }
        // The medal of a tournament played to its end: kept for the
        // account at once (the one signed in, or whose session is saved),
        // and sent once it is online; the server's count comes back.
        let medal_account = medals::account_of(&online, &config);
        if let Some(medal) = cup.as_ref().and_then(|c| c.medal())
            && medal_book.give(medal, medal_account.as_deref())
            && let Some(m) = current.as_mut()
        {
            let text = match medal.place {
                1 => i18n::t("A gold medal!"),
                2 => i18n::t("A silver medal!"),
                _ => i18n::t("A bronze medal!"),
            };
            m.notify(text, true);
        }
        if online.live()
            && let Some(account) = online.account().map(str::to_string)
        {
            let session = online.session_count;
            medal_book.sync(&account, session, &mut |msg| online.send(msg));
            for (medals, kept) in std::mem::take(&mut online.cup_medal_news) {
                medal_book.heard(&account, medals, kept);
            }
        }
        if claimer.update(&mut ladder_progress, &mut online, get_time()) {
            ladder_progress.save();
        }
        // The tour's progress lives on this computer; its reward (the
        // achievement that gives the champion's cue) on the account: an
        // account that won it has the tour complete here too.
        if ladder_progress
            .restore_from_reward(online.has_achievement(snook_legend_proto::ladder::ACHIEVEMENT))
        {
            ladder_progress.save();
        }
        // A ghost asked for while the game was waiting to sign in.
        if online.signed_in()
            && let Screen::Versus(v) = &mut screen
        {
            v.send_waiting(&mut online);
        }

        // VIP and bought reactions follow the wallet.
        if let Some(o) = current.as_mut().and_then(|m| m.online.as_mut()) {
            // The chat kept to the quick phrases (a parent's or guardian's choice).
            o.phrases_only = !match_from_lan && online.phrases_only();
            o.vip = !match_from_lan && online.wallet.as_ref().is_some_and(|w| w.is_vip());
            if let Some(w) = online.wallet.as_ref().filter(|_| !match_from_lan) {
                let bought = snook_legend_proto::reactions::bought(&w.owned);
                if o.bought != bought {
                    o.bought = bought;
                }
            }
        }
        // A game against the computer starts as an online one does: the two
        // players face each other (no rating at stake: none is shown).
        if let Some(m) = current.as_mut()
            && m.wants_intro()
        {
            m.intro_seen = true;
            if let Some(o) = m.opponent.clone() {
                let me = match online.me.as_ref().filter(|_| online.signed_in()) {
                    Some(me) => me.clone(),
                    None => snook_legend_proto::PlayerInfo {
                        username: m.names[0].clone(),
                        avatar: config.avatar.clone(),
                        guest: true,
                        ..Default::default()
                    },
                };
                let context = match (&m.ladder, m.cup) {
                    (Some(run), _) => format!(
                        "{} · {}",
                        i18n::t("The computer tour"),
                        i18n::tw(
                            "Stage {n} of {total}",
                            &[
                                ("n", &(run.rung + 1)),
                                ("total", &snook_legend_proto::ladder::RUNGS.len()),
                            ],
                        )
                    ),
                    (None, true) => i18n::t("Tournament").to_string(),
                    (None, false) => i18n::t("Against the computer").to_string(),
                };
                let intro = intro::Intro::vs_computer(me, &o, m.game.rules(), context);
                m.intro =
                    computer_intro.then(|| if hold_computer_intro { intro.held() } else { intro });
            }
        }
        if let Some(m) = current.as_mut() {
            // (A game against the computer waits for its introduction.)
            if (!hud.paused || m.is_online()) && !(m.online.is_none() && m.intro.is_some()) {
                m.update(dt);
                correspondence::drive(m, dt);
            }
            sounds.play(&mut m.sounds, config.volume);
        }

        rig.follow_shots = config.shot_camera;
        // Watching someone else's game (live or a replay): the broadcast camera.
        rig.tv = config.tv_camera
            && current.as_ref().is_some_and(|m| {
                m.is_spectating() || m.is_replay() || crate::dev::flag("SNOOK_LEGEND_TV")
            });
        // Watching: the camera the spectator chose (the pro observer mode).
        let watching = current.as_ref().is_some_and(|m| m.is_spectating());
        if !watching {
            hud.observer.camera = None;
        }
        hud.tv_default = config.tv_camera;
        if let Some(tv) = hud.observer.broadcast().filter(|_| watching) {
            rig.tv = tv;
        }
        if rig.top_only != config.top_view_only {
            rig.top_only = config.top_view_only;
            rig.reset();
        }
        let follow = hud.observer.follow_cue().filter(|_| watching).unwrap_or(config.camera_follow);
        if hud.observer.changed {
            hud.observer.apply(&mut rig);
        } else if rig.follow_cue != follow {
            rig.follow_cue = follow;
            rig.reset();
        }
        hud.top_view = rig.mode == CameraMode::Top;
        hud.top_only = rig.top_only;
        hud.can_report = !match_from_lan && online.signed_in();
        hud.rule_players = online.counts().map(|c| c.map(|_, r| r.players));
        hud.camera_default = rig.is_default();
        // Online (not on a local network): "Add friend" after the game.
        hud.opponent_relation = current
            .as_ref()
            .filter(|m| m.is_online() && !match_from_lan && !m.is_spectating())
            .and_then(|m| {
                let seat = m.online.as_ref()?.seat;
                online.relation_to(&m.names[1 - seat])
            });
        // The opponent muted in the settings or by name (remembered).
        hud.opponent_muted = current
            .as_ref()
            .filter(|m| m.is_online() && !m.is_spectating())
            .and_then(|m| m.online.as_ref().map(|o| m.names[1 - o.seat.min(1)].clone()))
            .map_or([false, false], |name| [config.muted(&name, true), config.muted(&name, false)]);
        hud.aim_sensitivity = config.aim_sensitivity;
        hud.zoom = rig.zoom_level();
        // The chat's height as the player left it (not while it is dragged).
        if !hud.resizing_chat() {
            hud.chat_height = config.chat_height;
        }
        controls.sensitivity = config.aim_sensitivity;
        // New cloth or balls from the shop: dress the table again (in the
        // background). Online, the table is the host's (or the higher
        // rated player's).
        if std::mem::take(&mut online.looks_changed) {
            config.save();
        }
        let table_look = current
            .as_ref()
            .filter(|m| m.is_online() && !match_from_lan)
            .map(|_| &online.table_look)
            .filter(|l| !l[0].is_empty());
        // The table and the hall are always this player's own.
        let mut looks = match table_look {
            Some(l) => scene::Looks::of(&l[0], &l[1]),
            None => scene::Looks::of(&config.cloth, &config.balls),
        }
        .themed(&config.table, &config.hall);
        // An official tournament's game is played in the pro arena.
        if current.as_ref().and_then(|m| m.online.as_ref()).is_some_and(|o| o.tournament.is_some())
            && !match_from_lan
        {
            looks.hall = snook_legend_proto::shop::HallStyle::Arena;
        }
        perks::update(&config.aim_color, config.show_reactions);
        // An item of the shop in the showroom: on its stage, or tried on
        // the table (in the hall of the setting chosen).
        let in_showroom = matches!(screen, Screen::Shop) && current.is_none();
        let showing = showroom.as_ref().filter(|_| in_showroom);
        if let Some(s) = showing {
            looks = s.looks(&config);
        }
        shop.dark_stage = config.focused_scene;
        // A hall tried on shows even on the dark stage.
        scene.preview_hall(showing.is_some_and(|s| s.shows_hall()));
        scene.set_looks(&looks);
        // Offline, the people at the table play with this player's cue
        // (and win with their effect).
        if let Some(m) = current.as_mut().filter(|m| !m.is_online()) {
            for seat in 0..2 {
                if m.controllers[seat] == game::Controller::Human {
                    let look = (config.cue.clone(), config.chalk.clone());
                    if m.cue_looks[seat] != look {
                        m.cue_looks[seat] = look;
                    }
                    m.cue_levels[seat] = config.cue_level;
                    if m.effects[seat] != config.effect {
                        m.effects[seat] = config.effect.clone();
                    }
                }
            }
        }
        // A game won on the 8: the winner's effect over the pocket.
        if let Some(m) = current.as_mut()
            && let Some((seat, pocket)) = m.win_effect.take()
        {
            let at = coords::surface(scene.pocket_center(pocket), 0.04);
            win_effects.extend(effects::WinEffect::of(&m.effects[seat], at));
        }
        let mut cam = rig.update(dt, current.as_ref(), get_time() as f32);
        // The showroom's own camera.
        if let Some(s) = showroom.as_mut().filter(|_| in_showroom) {
            cam = s.camera(dt);
        }
        // Development aid: SNOOK_LEGEND_CAM="eye x,y,z,target x,y,z" fixes the camera.
        if let Some(v) = crate::dev::var("SNOOK_LEGEND_CAM")
            .map(|v| v.split(',').filter_map(|x| x.trim().parse::<f32>().ok()).collect::<Vec<_>>())
            && v.len() == 6
        {
            cam.eye = vec3(v[0], v[1], v[2]);
            cam.target = vec3(v[3], v[4], v[5]);
            cam.up = Vec3::Y;
        }
        if let Some(m) = current.as_mut()
            && !hud.modal_open()
            && !settings_open
        {
            controls.camera(&mut rig, &cam, ui.hovered, ui.wants_keyboard);
            controls.player(m, &scene, &cam, &mut rig, ui.hovered, ui.wants_keyboard);
        } else {
            controls.idle();
        }

        let studio = in_showroom && showroom.as_ref().is_some_and(|s| s.in_studio());
        clear_background(if studio {
            // The showroom's stage melts into this.
            Color::new(0.012, 0.011, 0.011, 1.0)
        } else if config.focused_scene {
            Color::new(0.055, 0.05, 0.048, 1.0)
        } else {
            Color::new(0.03, 0.02, 0.015, 1.0)
        });
        // The lamp over the table hides from a camera above it (the
        // overhead view, a high view of the whole table), as a broadcast's
        // would: its light stays.
        let above_lamp =
            cam.eye.y > scene::LAMP_Y - 0.05 && cam.eye.x.abs() < 3.5 && cam.eye.z.abs() < 3.0;
        scene.lamp_visible = !(current.is_some() && (rig.mode == CameraMode::Top || above_lamp))
            || crate::dev::var("SNOOK_LEGEND_CAM").is_some();
        // Looking down on the table in a game: no cord, fan or beam of the
        // hall in the way.
        scene.clear_over_table = !scene.lamp_visible;
        scene.ball_shadows = config.shadows;
        // A profile picture chosen (in the profile or the settings): kept,
        // and the account's too.
        let picture = avatars::mine();
        if picture != config.avatar {
            config.avatar = picture;
            config.save();
            if online.signed_in() && online.me.as_ref().is_some_and(|me| !me.guest) {
                online.send(snook_legend_proto::ClientMsg::SetAvatar {
                    avatar: config.avatar.clone(),
                });
            }
        }
        if let Some(m) = current.as_mut() {
            m.talk = config.opponent_talk;
            m.head_string = config.head_string;
            // A first game against the computer or two players here: the
            // pointers, until the player has taken two shots.
            m.coach = !config.coached && !m.is_online() && !m.is_replay();
            if m.coach
                && m.timeline
                    .iter()
                    .filter(|s| m.controllers[s.shooter] == game::Controller::Human)
                    .count()
                    >= 2
            {
                config.coached = true;
                config.save();
            }
            // Offline, the first player is this one.
            if m.online.is_none() && m.avatars[0] != config.avatar {
                m.avatars[0] = config.avatar.clone();
            }
        }
        match current.as_ref() {
            Some(m) => {
                let overlay = play::overlay(m, &scene, get_time() as f32);
                scene.draw(&cam, m.balls(), dt, &overlay);
            }
            // The lounge fills the window and hides the table: don't spend
            // the GPU (and the battery) drawing it behind.
            None if matches!(screen, Screen::LanLounge)
                || (matches!(screen, Screen::Online) && online.signed_in()) => {}
            // The showroom's stage: the item alone.
            None if showroom.as_ref().is_some_and(|s| s.in_studio()) && in_showroom => {
                let s = showroom.as_ref().expect("checked");
                // The marks the cue shown has earned (its owner's wins).
                let wins = online.wallet.as_ref().map_or(0, |w| w.cue_wins(s.code));
                let level = snook_legend_proto::shop::cue_level(wins);
                let show = s.showcase(&config, level, config.cue_level);
                scene.draw_showcase(&cam, &show);
            }
            None => {
                let mut overlay = Overlay::default();
                if let Some((cue, chalk)) =
                    showroom.as_ref().filter(|_| in_showroom).and_then(|s| s.cue_on_table(&config))
                {
                    // The cue lined up behind the cue ball.
                    if let Some(ball) = menu_rack.iter().find(|b| b.id == BallId::CUE) {
                        let params = snook_legend_core::PhysicsParams::default();
                        let pose = scene.cue_pose(
                            &params,
                            ball.position,
                            0.0,
                            0.08,
                            ::glam::DVec2::ZERO,
                            0.03,
                        );
                        // The marks this cue has earned.
                        overlay.cue_level = snook_legend_proto::shop::cue_level(
                            online.wallet.as_ref().map_or(0, |w| w.cue_wins(&cue)),
                        );
                        overlay.cue = Some(pose);
                        overlay.cue_look = (cue, chalk);
                    }
                }
                scene.draw(&cam, &menu_rack, dt, &overlay)
            }
        }

        // Development aid: with SNOOK_LEGEND_SCENE_ONLY, --screenshot saves the 3D
        // scene without the interface (pictures for the store pages).
        if crate::dev::flag("SNOOK_LEGEND_SCENE_ONLY")
            && let Some(path) = &options.screenshot
            && frame + 1 == options.frames
        {
            let mut image = get_screen_data();
            for px in image.bytes.as_chunks_mut::<4>().0 {
                px[3] = 255;
            }
            image.export_png(path);
            break;
        }

        // From here on: the 2D interface, in interface units.
        ui::begin_ui_camera();
        if !win_effects.is_empty() {
            let k = ui::ui_scale();
            let scale = ui::vh() * 0.22;
            win_effects.retain_mut(|e| e.draw(dt, cam.project(e.at).map(|p| p / k), scale));
        }
        if config.vignette {
            draw_texture_ex(
                &vignette,
                0.0,
                0.0,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(ui::vw(), ui::vh())),
                    ..Default::default()
                },
            );
        }

        // 2D interface. The e-mail offer is drawn over the page at the end:
        // until then the page doesn't react to the pointer.
        // A warning from the staff waits for OK over any page (never over a
        // game, nor while a game waits for the server); the other dialogs
        // wait for it.
        let staff_up =
            current.is_none() && !settings_open && waiting.is_none() && online.staff_warning_open();
        // "What's new" waits for the main menu, and is drawn over it.
        let show_news = whats_new.is_some()
            && current.is_none()
            && !settings_open
            && !staff_up
            && matches!(screen, Screen::Menu);
        let offer_up = current.is_none()
            && !settings_open
            && !show_news
            && !staff_up
            && waiting.is_none()
            && online.email_offer.is_some();
        // The rules of the player's games, asked once (after what's new),
        // on the main menu.
        let ask_rules = config.rules.is_none()
            && current.is_none()
            && !settings_open
            && !show_news
            && !staff_up
            && !offer_up
            && waiting.is_none()
            && matches!(screen, Screen::Menu)
            // Development aid (screenshots): SNOOK_LEGEND_ASK_RULES shows it.
            && (options.screenshot.is_none() || crate::dev::flag("SNOOK_LEGEND_ASK_RULES"));
        // An invitation being set up, over the lounge.
        // (A daily game's challenge also from a profile.)
        let invite_here = matches!(screen, Screen::Online)
            || (matches!(screen, Screen::Profile)
                && online.invite_dialog.as_ref().is_some_and(|d| d.daily));
        let inviting = online.invite_dialog.is_some()
            && current.is_none()
            && !settings_open
            && !staff_up
            && invite_here;
        if !invite_here || current.is_some() {
            online.invite_dialog = None;
        }
        // The chat with other players' dialog is over every screen and game
        // (drawn near the end): nothing under it takes the pointer. Saved
        // first, given back last.
        // The auctions' and the credits' pages, the same way (they open
        // over the main menu, the lounge and the shop).
        let ops_saved = (!settings_open && current.is_none() && online.ops_page_open())
            .then(|| ui.suspend_input());
        let chat_saved = (!settings_open && online.chat_dialog_open).then(|| ui.suspend_input());
        // A conversation with a friend, over the lounge: nothing under it
        // takes the pointer.
        let dm_up = online.dms.open.is_some()
            && current.is_none()
            && !settings_open
            && matches!(screen, Screen::Online)
            && online.stage == online::Stage::Lounge;
        if !dm_up {
            online.dms.open = None;
        }
        let dm_saved = dm_up.then(|| ui.suspend_input());
        let ask_saved = ask_rules.then(|| ui.suspend_input());
        // The offer before the invitation: drawn over it, it is given the
        // pointer back last (the invitation under it gets none).
        let offer_saved = offer_up.then(|| ui.suspend_input());
        let invite_saved = inviting.then(|| ui.suspend_input());
        let news_saved = show_news.then(|| ui.suspend_input());
        // A game waiting for the server: its overlay holds the keyboard too,
        // so the screen or the game under it gets neither Esc nor Enter.
        let waiting_saved = waiting.is_some().then(|| {
            ui.focus = Some(WAITING_FOCUS);
            ui.wants_keyboard = true;
            ui.suspend_input()
        });
        if settings_open {
            let before = config.clone();
            let action = settings_screen.draw(&mut ui, &mut config, current.is_some());
            if config.quality != before.quality || config.focused_scene != before.focused_scene {
                // The old scene's materials may still be referenced by draw calls
                // queued this frame: drop it after the frame is shown.
                let table = scene.table.clone();
                retired.push(std::mem::replace(
                    &mut scene,
                    Scene::new(
                        table,
                        config.quality,
                        config.focused_scene,
                        &scene::Looks::of(&config.cloth, &config.balls)
                            .themed(&config.table, &config.hall),
                    ),
                ));
            }
            if config.language != before.language {
                i18n::set_lang(config.language);
            }
            if config.server != before.server && !online.signed_in() {
                online = Online::new(&config);
            }
            if let Some(m) = current.as_mut() {
                m.guide = match m.online.as_ref() {
                    Some(_) => config.guide.online(),
                    None => config.guide,
                };
            }
            match action {
                SettingsAction::Close => {
                    settings_open = false;
                    config.save();
                }
                SettingsAction::Restart => {
                    config.save();
                    if update::restart() {
                        break;
                    }
                }
                SettingsAction::None => {}
            }
        } else {
            match current.as_mut() {
                Some(m) => {
                    let action =
                        hud_draw(&mut hud, &mut ui, m, controls.aiming(), &mut hud_opacity, dt);
                    // The chat was made taller or shorter: kept for the next games.
                    if let Some(h) = hud.chat_resized.take() {
                        config.chat_height = h;
                        config.save();
                    }
                    // The opponent muted (or not) from the game: their next
                    // games start the same.
                    if let Some([chat, reactions]) = hud.mute_request.take()
                        && let Some(o) = m.online.as_ref()
                    {
                        let name = m.names[1 - o.seat.min(1)].clone();
                        // (Only them: the settings' "every opponent" stays.)
                        config.set_muted(&name, true, chat);
                        config.set_muted(&name, false, reactions);
                        config.save();
                    }
                    // Passing the time while "Play now" looks for an opponent.
                    if m.practice {
                        online.draw_queue_banner(&mut ui);
                    }
                    match action {
                        HudAction::DrillAgain(code) if has_account(&online) => {
                            waiting = Some((ask_drill(&mut online, code), get_time()));
                        }
                        HudAction::DrillAgain(code) => {
                            let name = online
                                .me
                                .as_ref()
                                .map_or(config.player_name.clone(), |me| me.username.clone());
                            if let Some(next) = Match::drill(code, &name, config.guide, 0) {
                                online.drill_result = None;
                                // A ranked session over while signed out: kept to send.
                                send_run(m, &mut online, &mut unsent_runs);
                                current = Some(next);
                                hud = Hud::new(config.show_help);
                                rig.reset();
                            }
                        }
                        HudAction::TimeAttackAgain => {
                            let counted = m.time_attack.as_ref().is_some_and(|r| r.attempt != 0);
                            if counted && online.signed_in() {
                                online.send(snook_legend_proto::ClientMsg::TimeAttackStart);
                            } else {
                                let name = m.names[0].clone();
                                current = Some(Match::time_attack(
                                    0,
                                    game::clock_seed(),
                                    &name,
                                    config.guide,
                                ));
                                hud = Hud::new(config.show_help);
                                rig.reset();
                            }
                        }
                        HudAction::NextStage(k) => {
                            // The next opponent of the tour, if it is open.
                            if ladder_progress.unlocked(k)
                                && let Some(p) = snook_legend_proto::ladder::rung(k as usize)
                            {
                                let o = crate::opponents::Opponent::personality(p.code);
                                let settings = versus::settings(&config, o, Some(k));
                                if wants_deal(&settings, &online) {
                                    waiting = Some((ask_deal(&mut online, settings), get_time()));
                                } else {
                                    let mut m = Match::new(&settings, game::clock_seed());
                                    alternate_break(&mut m, &mut config);
                                    warn_unrewarded(&mut m, &settings, &online, &ladder_progress);
                                    current = Some(m);
                                    save_key = None;
                                    hud = Hud::new(config.show_help);
                                    rig.reset();
                                }
                            }
                        }
                        HudAction::MainMenu => {
                            // A tournament's game left while its last shot
                            // was rolling: the shot counts (a game left over
                            // is a game lost, and this one is won or lost).
                            if let Some(c) = cup.as_mut()
                                && m.is_against_computer()
                                && c.playing == Some(m.seed())
                            {
                                m.settle_shot();
                                if let Some(over) = m.game.game_over() {
                                    c.report(
                                        over.winner == snook_legend_proto::ladder::PLAYER_SEAT,
                                    );
                                    c.save();
                                }
                            }
                            // Back from a replay: to the profile it was opened
                            // from; from a game while waiting: to the lounge.
                            let replay = m.is_replay();
                            let practice = m.practice;
                            // Left half way through a daily attempt: what was
                            // made so far still counts.
                            let daily = m.daily.is_some();
                            // Left during a drill: the tries made still count.
                            let drill = m.drill.is_some();
                            let time_attack = m.time_attack.is_some();
                            // Signed out: sent once back.
                            send_run(m, &mut online, &mut unsent_runs);
                            // From a game against the computer: back to its
                            // screen (the ladder, after a ladder game).
                            let versus = m.is_against_computer().then(|| {
                                let tab = if m.ladder.is_some() {
                                    versus::Tab::Ladder
                                } else if cup.as_ref().is_some_and(|c| {
                                    let seed = Some(m.seed());
                                    c.playing == seed || c.counted == seed
                                }) {
                                    versus::Tab::Cup
                                } else {
                                    versus::Tab::Free
                                };
                                versus::Versus::new(tab, &config)
                            });
                            current = None;
                            hud = Hud::new(config.show_help);
                            rig.reset();
                            screen = if time_attack {
                                online.send(snook_legend_proto::ClientMsg::TimeAttack);
                                Screen::Versus(versus::Versus::new(
                                    versus::Tab::TimeAttack,
                                    &config,
                                ))
                            } else if drill {
                                online.send(snook_legend_proto::ClientMsg::Drills);
                                Screen::Versus(versus::Versus::new(versus::Tab::Drills, &config))
                            } else if daily {
                                online.send(snook_legend_proto::ClientMsg::Daily);
                                Screen::Versus(versus::Versus::new(versus::Tab::Daily, &config))
                            } else if replay {
                                Screen::Profile
                            } else if practice {
                                Screen::Online
                            } else if let Some(v) = versus {
                                Screen::Versus(v)
                            } else {
                                Screen::Menu
                            };
                        }
                        HudAction::Lounge => {
                            if match_from_lan {
                                if let Some(l) = lan.as_mut() {
                                    l.net.leave_match(m);
                                }
                                screen = Screen::LanLounge;
                            } else {
                                online.leave_match(m);
                                screen = Screen::Online;
                            }
                            current = None;
                            hud = Hud::new(config.show_help);
                            rig.reset();
                        }
                        HudAction::Settings => settings_open = true,
                        HudAction::AddFriend(name) => online.befriend(&name),
                        HudAction::ResetCamera => rig.reset(),
                        HudAction::Zoom(z) => {
                            rig.tv_paused = true;
                            rig.set_zoom_level(z);
                        }
                        HudAction::None => {
                            // Focus mode: no invitations while a game is
                            // played (the server turns them away; one that
                            // slipped through waits for the game's end).
                            let focus = config.focus_mode && m.is_focus_game();
                            if !m.is_online() {
                                // Any game here (the computer, a tournament
                                // or the tour against it, two players, a
                                // drill, a replay): still in the lounge.
                                // How many are online is in the rules'
                                // badge at the top; under the top bar on the
                                // right, for a moment, who just came (someone
                                // to invite); the invitations' cards come
                                // under it (drawn at the end of the frame).
                                let queue = online.queue_secs().is_some();
                                let counted =
                                    online.draw_arrival(&ui, ui::vw() - 14.0, 70.0, focus);
                                invite_top = match (queue, counted) {
                                    (true, _) => 134.0,
                                    (false, true) => 94.0,
                                    (false, false) => 62.0,
                                };
                            }
                        }
                    }
                }
                None => {
                    let mut next: Option<Screen> = None;
                    screen_fade = (screen_fade + get_frame_time() / 0.2).min(1.0);
                    // Eased out: quick at first, settling gently.
                    paint::set_screen_fade(1.0 - (1.0 - screen_fade).powi(3));
                    // A gift is on screen (drawn over it below): the screen
                    // gets neither the pointer nor Esc (screens leave Esc
                    // alone while a field has the keyboard).
                    // The e-mail offer and a staff warning too: their Esc
                    // is their own (and the offer's field keeps the keyboard).
                    let gift_modal = (online.gift_dialog_open()
                        || offer_saved.is_some()
                        || replace_save.is_some()
                        || staff_up)
                        && !settings_open;
                    let under_gift = gift_modal.then(|| {
                        let field = ui.focus.filter(|f| *f == online::EMAIL_OFFER_FIELD);
                        ui.focus = Some(GIFT_DIALOG_FOCUS);
                        (ui.suspend_input(), field)
                    });
                    match &mut screen {
                        Screen::Menu => {
                            match menu.draw(&mut ui, &online, saved_balls, config.rules()) {
                                action @ (MenuAction::VsComputer | MenuAction::TwoPlayers) => {
                                    // Signed in: the account's name, which can still be changed.
                                    if let Some(me) = online.me.as_ref().filter(|m| !m.guest) {
                                        config.player_name = me.username.clone();
                                    }
                                    next = Some(if matches!(action, MenuAction::VsComputer) {
                                        Screen::Versus(versus::Versus::new(
                                            versus::Tab::Free,
                                            &config,
                                        ))
                                    } else {
                                        Screen::Setup(Setup::new())
                                    });
                                }
                                MenuAction::ContinueGame => match saved::load() {
                                    Some(s) => {
                                        let mut m = Match::resume(&s, config.guide);
                                        // The tournament's game, picked up here.
                                        if cup.as_ref().is_some_and(|c| c.playing == Some(s.seed)) {
                                            m.make_cup_game();
                                        }
                                        current = Some(m);
                                        save_key = None;
                                        hud = Hud::new(config.show_help);
                                        rig.reset();
                                    }
                                    None => saved_balls = None,
                                },
                                MenuAction::Online => {
                                    online.connect();
                                    next = Some(Screen::Online);
                                }
                                MenuAction::Daily => {
                                    online.connect();
                                    online.tab = online::Tab::Daily;
                                    online.send(snook_legend_proto::ClientMsg::CorrList);
                                    next = Some(Screen::Online);
                                }
                                MenuAction::Lan => {
                                    lan_screen.get_or_insert_with(LanScreen::new);
                                    next = Some(if lan.is_some() {
                                        Screen::LanLounge
                                    } else {
                                        Screen::Lan
                                    });
                                }
                                MenuAction::Ranking => {
                                    ranking.open(&mut online);
                                    back_to_lounge = false;
                                    next = Some(Screen::Ranking);
                                }
                                MenuAction::Profile => {
                                    back_to_lounge = false;
                                    match online.me.clone() {
                                        Some(me) => {
                                            profile.open(&me.username, &mut online);
                                            next = Some(Screen::Profile);
                                        }
                                        None => {
                                            online.connect();
                                            next = Some(Screen::Online);
                                        }
                                    }
                                }
                                MenuAction::Shop => {
                                    shop.open(&mut online);
                                    shop.close_collection();
                                    back_to_lounge = false;
                                    next = Some(Screen::Shop);
                                }
                                MenuAction::Collection => {
                                    shop.open_collection(&mut online);
                                    back_to_lounge = false;
                                    next = Some(Screen::Shop);
                                }
                                MenuAction::Update => {
                                    online.quit_for_update = update::start();
                                }
                                MenuAction::Note(a) => match online.note_action(a) {
                                    Some(OnlineAction::ShopVip) => {
                                        shop.open(&mut online);
                                        shop.show_tab("shop-vip");
                                        back_to_lounge = false;
                                        next = Some(Screen::Shop);
                                    }
                                    Some(_) => {
                                        shop.open(&mut online);
                                        back_to_lounge = false;
                                        next = Some(Screen::Shop);
                                    }
                                    // A page of the lounge.
                                    None => {
                                        online.connect();
                                        next = Some(Screen::Online);
                                    }
                                },
                                MenuAction::Settings => settings_open = true,
                                MenuAction::Inbox => {
                                    back_to_lounge = false;
                                    if online.signed_in() {
                                        inbox.open(&mut online);
                                        next = Some(Screen::Inbox);
                                    } else {
                                        online.connect();
                                        next = Some(Screen::Online);
                                    }
                                }
                                MenuAction::WhatsNew => {
                                    whats_new = Some(whatsnew::WhatsNew::all());
                                }
                                MenuAction::Rules(rules) => {
                                    whats_new = Some(whatsnew::WhatsNew::rules(rules));
                                }
                                MenuAction::SetRules(rules) => config.set_rules(rules),
                                MenuAction::Restart => {
                                    config.save();
                                    if update::restart() {
                                        break;
                                    }
                                }
                                MenuAction::Quit => break,
                                MenuAction::None => {}
                            }
                        }
                        Screen::Versus(v) => match v.draw(
                            &mut ui,
                            &mut config,
                            &mut online,
                            &ladder_progress,
                            &mut claimer,
                            &mut cup,
                            &medal_book,
                        ) {
                            versus::VersusAction::Back => {
                                config.save();
                                next = Some(Screen::Menu);
                            }
                            versus::VersusAction::Start(settings) if saved_balls.is_some() => {
                                config.save();
                                replace_save = Some(settings);
                                replace_cup = false;
                                replace_loses_cup = saved::load().is_some_and(|s| {
                                    cup.as_ref().is_some_and(|c| c.playing == Some(s.seed))
                                });
                            }
                            versus::VersusAction::Start(settings)
                                if wants_deal(&settings, &online) =>
                            {
                                config.save();
                                waiting = Some((ask_deal(&mut online, settings), get_time()));
                            }
                            versus::VersusAction::Start(settings) => {
                                config.save();
                                let mut m = Match::new(&settings, game::clock_seed());
                                alternate_break(&mut m, &mut config);
                                warn_unrewarded(&mut m, &settings, &online, &ladder_progress);
                                current = Some(m);
                                save_key = None;
                                hud = Hud::new(config.show_help);
                                rig.reset();
                                next = Some(Screen::Menu);
                            }
                            // A game saved that isn't this tournament's: which
                            // one, first (as for any new game).
                            versus::VersusAction::StartCup(settings)
                                if saved_balls.is_some()
                                    && !saved::load().is_some_and(|s| {
                                        cup.as_ref().is_some_and(|c| c.playing == Some(s.seed))
                                    }) =>
                            {
                                config.save();
                                replace_save = Some(settings);
                                replace_cup = true;
                                replace_loses_cup = false;
                            }
                            versus::VersusAction::StartCup(settings) => {
                                config.save();
                                let seed = game::clock_seed();
                                if let Some(c) = cup.as_mut() {
                                    c.begin(seed);
                                    c.save();
                                }
                                let mut m = Match::new(&settings, seed);
                                // One break each, round after round.
                                m.set_breaker(cup.as_ref().map_or(0, |c| c.rounds.len() % 2));
                                m.make_cup_game();
                                current = Some(m);
                                save_key = None;
                                hud = Hud::new(config.show_help);
                                rig.reset();
                                next = Some(Screen::Menu);
                            }
                            versus::VersusAction::ResumeCup => {
                                if let Some(s) = saved::load() {
                                    let mut m = Match::resume(&s, config.guide);
                                    m.make_cup_game();
                                    current = Some(m);
                                    save_key = None;
                                    hud = Hud::new(config.show_help);
                                    rig.reset();
                                    next = Some(Screen::Menu);
                                }
                            }
                            versus::VersusAction::Drill(code) if has_account(&online) => {
                                waiting = Some((ask_drill(&mut online, &code), get_time()));
                            }
                            versus::VersusAction::Drill(code) => {
                                let name = online
                                    .me
                                    .as_ref()
                                    .map_or(config.player_name.clone(), |me| me.username.clone());
                                if let Some(m) = Match::drill(&code, &name, config.guide, 0) {
                                    online.drill_result = None;
                                    current = Some(m);
                                    hud = Hud::new(config.show_help);
                                    rig.reset();
                                }
                            }
                            versus::VersusAction::TimeAttackPractice => {
                                let name = online
                                    .me
                                    .as_ref()
                                    .map_or(config.player_name.clone(), |me| me.username.clone());
                                current = Some(Match::time_attack(
                                    0,
                                    game::clock_seed(),
                                    &name,
                                    config.guide,
                                ));
                                hud = Hud::new(config.show_help);
                                rig.reset();
                            }
                            versus::VersusAction::SignIn => {
                                online.connect();
                                next = Some(Screen::Online);
                            }
                            versus::VersusAction::None => {}
                        },
                        Screen::Setup(setup) => match setup.draw(&mut ui, &mut config) {
                            SetupAction::Back => {
                                // The names and the clock chosen are kept.
                                config.save();
                                next = Some(Screen::Menu);
                            }
                            SetupAction::Start(settings) => {
                                config.save();
                                let mut m = Match::new(&settings, game::clock_seed());
                                alternate_break(&mut m, &mut config);
                                current = Some(m);
                                hud = Hud::new(config.show_help);
                                rig.reset();
                                next = Some(Screen::Menu);
                            }
                            SetupAction::None => {}
                        },
                        Screen::Online => match online.draw(&mut ui, &mut config) {
                            OnlineAction::Leave => next = Some(Screen::Menu),
                            OnlineAction::Profile(name) => {
                                profile.open(&name, &mut online);
                                back_to_lounge = true;
                                next = Some(Screen::Profile);
                            }
                            OnlineAction::Ranking => {
                                ranking.open(&mut online);
                                back_to_lounge = true;
                                next = Some(Screen::Ranking);
                            }
                            OnlineAction::Shop => {
                                shop.open(&mut online);
                                back_to_lounge = true;
                                next = Some(Screen::Shop);
                            }
                            OnlineAction::PracticeWhileWaiting => {
                                // The opponent last picked against the computer.
                                let code = opponents::chosen(&config.opponent).code;
                                let mut settings = MatchSettings::from_config(
                                    MatchMode::VsComputer(opponents::Opponent::personality(code)),
                                    &config,
                                );
                                if let Some(me) = online.me.as_ref() {
                                    settings.names[0] = me.username.clone();
                                }
                                let mut m = Match::new(&settings, game::clock_seed());
                                m.practice = true;
                                current = Some(m);
                                hud = Hud::new(config.show_help);
                                rig.reset();
                            }
                            OnlineAction::FreeTableWhileWaiting => {
                                // A table to oneself: a fresh rack, shoot as
                                // you like, racked again when it's over.
                                let mut settings =
                                    MatchSettings::from_config(MatchMode::HotSeat, &config);
                                // No clock on a table to oneself (it would
                                // give balls away "to" the same player).
                                settings.shot_clock = None;
                                let name = online
                                    .me
                                    .as_ref()
                                    .map_or(settings.names[0].clone(), |me| me.username.clone());
                                settings.names = [name.clone(), name];
                                let mut m = Match::new(&settings, game::clock_seed());
                                m.practice = true;
                                m.free_play = true;
                                current = Some(m);
                                hud = Hud::new(config.show_help);
                                rig.reset();
                            }
                            OnlineAction::ShopVip => {
                                shop.open(&mut online);
                                shop.show_tab("shop-vip");
                                back_to_lounge = true;
                                next = Some(Screen::Shop);
                            }
                            OnlineAction::Inbox => {
                                inbox.open(&mut online);
                                back_to_lounge = true;
                                next = Some(Screen::Inbox);
                            }
                            OnlineAction::None => {}
                        },
                        Screen::Lan => {
                            let ls = lan_screen.get_or_insert_with(LanScreen::new);
                            let hosting =
                                lan.as_ref().and_then(|l| l.host.as_ref()).map(|h| h.port);
                            match ls.draw(&mut ui, &mut config, hosting) {
                                LanAction::Back => next = Some(Screen::Menu),
                                LanAction::Host(name) => {
                                    if let Some((host, url)) = ls.host(&name) {
                                        let mut net = Online::lan(&url, &name);
                                        net.host_address =
                                            lan::local_ip().map(|ip| format!("{ip}:{}", host.port));
                                        lan = Some(LanSession { host: Some(host), net });
                                        config.save();
                                        next = Some(Screen::LanLounge);
                                    }
                                }
                                LanAction::Join(url, name) => {
                                    lan = Some(LanSession {
                                        host: None,
                                        net: Online::lan(&url, &name),
                                    });
                                    config.save();
                                    next = Some(Screen::LanLounge);
                                }
                                LanAction::None => {}
                            }
                        }
                        Screen::LanLounge => match lan.as_mut() {
                            Some(l) => match l.net.draw(&mut ui, &mut config) {
                                OnlineAction::Leave => {
                                    // Leaving a LAN game closes it (and stops hosting).
                                    lan = None;
                                    next = Some(Screen::Menu);
                                }
                                OnlineAction::Profile(name) => {
                                    l.net.request_profile(&name);
                                }
                                _ => {}
                            },
                            None => next = Some(Screen::Lan),
                        },
                        Screen::Ranking => match ranking.draw(&mut ui, &mut online) {
                            RankingAction::Back => {
                                next =
                                    Some(if back_to_lounge { Screen::Online } else { Screen::Menu })
                            }
                            RankingAction::Profile(name) => {
                                profile.open(&name, &mut online);
                                next = Some(Screen::Profile);
                            }
                            RankingAction::None => {}
                        },
                        Screen::Shop if showroom.is_some() => {
                            let dressing = scene.is_dressing();
                            let s = showroom.as_mut().expect("checked");
                            let code = s.code;
                            match s.ui(&mut ui, &mut shop, &mut online, dressing) {
                                showroom::ShowroomAction::None => {}
                                showroom::ShowroomAction::Close => {
                                    shop.show_item(code);
                                    showroom = None;
                                }
                                showroom::ShowroomAction::Buy => {
                                    shop.show_item(code);
                                    shop.ask_to_buy(code);
                                    showroom = None;
                                }
                            }
                        }
                        Screen::Shop => match shop.draw(&mut ui, &mut online) {
                            shop::ShopAction::Back => {
                                next = Some(if back_to_lounge && online.signed_in() {
                                    Screen::Online
                                } else {
                                    Screen::Menu
                                })
                            }
                            shop::ShopAction::Preview(code) => {
                                showroom = showroom::Showroom::open(code, &scene.table);
                            }
                            shop::ShopAction::SignIn => {
                                online.connect();
                                next = Some(Screen::Online);
                            }
                            shop::ShopAction::ShowHall => {
                                // As in the settings: the room around the table.
                                config.focused_scene = false;
                                config.save();
                                let table = scene.table.clone();
                                retired.push(std::mem::replace(
                                    &mut scene,
                                    Scene::new(
                                        table,
                                        config.quality,
                                        config.focused_scene,
                                        &scene::Looks::of(&config.cloth, &config.balls)
                                            .themed(&config.table, &config.hall),
                                    ),
                                ));
                            }
                            shop::ShopAction::None => {}
                        },
                        Screen::Inbox => match inbox.draw(&mut ui, &mut online) {
                            inbox::InboxAction::None => {}
                            inbox::InboxAction::Back => {
                                next = Some(if back_to_lounge && online.signed_in() {
                                    Screen::Online
                                } else {
                                    Screen::Menu
                                })
                            }
                            inbox::InboxAction::Profile(name) => {
                                profile.open(&name, &mut online);
                                next = Some(Screen::Profile);
                            }
                            inbox::InboxAction::Shop(code) => {
                                shop.open(&mut online);
                                if !code.is_empty() {
                                    shop.show_item(&code);
                                }
                                next = Some(Screen::Shop);
                            }
                            inbox::InboxAction::Vip => {
                                shop.open(&mut online);
                                shop.show_tab("shop-vip");
                                next = Some(Screen::Shop);
                            }
                            inbox::InboxAction::Tournaments => {
                                online.connect();
                                next = Some(Screen::Online);
                            }
                            // The collection (on the item) and the auctions' page.
                            inbox::InboxAction::Collection(code) => {
                                shop.open_collection(&mut online);
                                shop.show_item(&code);
                                next = Some(Screen::Shop);
                            }
                            inbox::InboxAction::Auctions => online.open_auctions(),
                        },
                        Screen::Profile => match profile.draw(&mut ui, &mut online) {
                            ProfileAction::Back => {
                                next = Some(if back_to_lounge && online.signed_in() {
                                    Screen::Online
                                } else {
                                    Screen::Menu
                                })
                            }
                            ProfileAction::Ghost(name) => {
                                next = Some(Screen::Versus(versus::Versus::ghost_of(
                                    &name,
                                    &config,
                                    &mut online,
                                )));
                            }
                            // Signed out: the sign-in form (for another account).
                            ProfileAction::SignOut => {
                                online.logout(&mut config);
                                next = Some(Screen::Online);
                            }
                            // The advanced statistics' preview: becoming VIP.
                            ProfileAction::ShopVip => {
                                shop.open(&mut online);
                                shop.show_tab("shop-vip");
                                next = Some(Screen::Shop);
                            }
                            ProfileAction::None => {}
                        },
                    }
                    if let Some((saved, field)) = under_gift {
                        ui.resume_input(saved);
                        if ui.focus == Some(GIFT_DIALOG_FOCUS) {
                            ui.focus = field;
                        }
                    }
                    paint::set_screen_fade(1.0);
                    if let Some(n) = next {
                        screen = n;
                        screen_fade = 0.0;
                    }
                }
            }
        }

        if !config.graphics_checked
            && current.is_none()
            && !settings_open
            && options.screenshot.is_none()
        {
            probe.0 += get_frame_time();
            probe.1 += 1;
            if probe.0 > 5.0 {
                // Average over the first seconds on the menu.
                let fps = probe.1 as f32 / probe.0;
                let pick = if fps < 22.0 {
                    Some(config::Quality::Low)
                } else if fps < 40.0 && config.quality > config::Quality::Medium {
                    Some(config::Quality::Medium)
                } else {
                    None
                };
                if let Some(q) = pick {
                    config.quality = q;
                    config.msaa = config.msaa.min(q.default_msaa());
                    let table = scene.table.clone();
                    retired.push(std::mem::replace(
                        &mut scene,
                        Scene::new(
                            table,
                            q,
                            config.focused_scene,
                            &scene::Looks::of(&config.cloth, &config.balls)
                                .themed(&config.table, &config.hall),
                        ),
                    ));
                    toast = Some((
                        i18n::tw(
                            "Graphics set to {q} for smooth play. You can change this in Settings.",
                            &[("q", &i18n::t(q.name()))],
                        ),
                        7.0,
                    ));
                }
                config.graphics_checked = true;
                config.save();
            }
        }
        if let Some(saved) = news_saved {
            ui.resume_input(saved);
            if whats_new.as_mut().is_some_and(|w| w.draw(&mut ui)) {
                whats_new = None;
            }
        }
        if let Some(saved) = ask_saved {
            ui.resume_input(saved);
            let counts = online.counts().map(|c| c.map(|_, n| n.players));
            match rulesui::ask(&mut ui, counts) {
                rulesui::Asked::Chose(rules) => config.set_rules(rules),
                rulesui::Asked::Read => {
                    whats_new = Some(whatsnew::WhatsNew::rules(Rules::Brazilian));
                }
                rulesui::Asked::None => {}
            }
        }
        if let Some(saved) = invite_saved {
            ui.resume_input(saved);
            online.draw_invite_dialog(&mut ui);
        }
        if let Some(saved) = offer_saved {
            ui.resume_input(saved);
        }
        if let Some(saved) = waiting_saved {
            ui.resume_input(saved);
            if let Some((w, _)) = &waiting {
                let text = match w {
                    Waiting::Final(_) => i18n::t("The server is dealing the final…"),
                    Waiting::Drill(_) => i18n::t("The server is setting up the drill…"),
                };
                draw_waiting(&mut ui, text, i18n::t("Esc to cancel"));
                // Its own Esc (nothing under it reads the keyboard).
                if is_key_pressed(KeyCode::Escape) {
                    waiting = None;
                    online.awaiting_ticket = false;
                }
            }
        }
        // The overlay keeps the keyboard while a game waits for the server.
        if waiting.is_some() {
            ui.focus = Some(WAITING_FOCUS);
        } else if ui.focus == Some(WAITING_FOCUS) {
            ui.focus = None;
        }
        if let Some((text, t)) = &mut toast {
            *t -= dt;
            let alpha = t.clamp(0.0, 1.0);
            let w = ui.text_size(text, 15.0, false).x + 32.0;
            let r = Rect::new((ui::vw() - w) / 2.0, ui::vh() - 60.0, w, 34.0);
            ui::rrect(r, 17.0, Color::new(0.0, 0.0, 0.0, 0.7 * alpha));
            ui.text_centered(
                text,
                ui::vw() / 2.0,
                r.y + 9.0,
                15.0,
                Color::new(1.0, 0.95, 0.85, alpha),
                false,
            );
            if *t <= 0.0 {
                toast = None;
            }
        }
        // The round trip to the server, discreet, during online matches.
        if config.show_ping && current.as_ref().is_some_and(|m| m.is_online()) && !settings_open {
            let rtt =
                if match_from_lan { lan.as_ref().and_then(|l| l.net.rtt) } else { online.rtt };
            online::draw_ping(&ui, rtt, ui::vw() - 12.0, 64.0);
        }
        // Achievements, friend requests, declined invitations…
        let toast_bottom = if current.is_some() { ui::vh() - 92.0 } else { ui::vh() - 20.0 };
        if current.is_none()
            && !settings_open
            && !show_news
            && !staff_up
            && waiting.is_none()
            && online.draw_email_offer(&mut ui, &mut config)
        {
            config.save();
        }
        // Presents from friends, one after another (never over a game).
        if current.is_none()
            && !settings_open
            && !staff_up
            && waiting.is_none()
            && online.email_offer.is_none()
        {
            online.draw_gift_dialog(&mut ui);
        }
        // A new game against the computer while one is saved: which one.
        if let Some(settings) = replace_save.clone()
            && current.is_none()
            && !settings_open
            && !staff_up
        {
            match draw_replace_save(&mut ui, saved_balls.unwrap_or(0), replace_loses_cup) {
                Some(Some(true)) if replace_cup => {
                    replace_save = None;
                    let seed = game::clock_seed();
                    if let Some(c) = cup.as_mut() {
                        c.begin(seed);
                        c.save();
                    }
                    let mut m = Match::new(&settings, seed);
                    m.set_breaker(cup.as_ref().map_or(0, |c| c.rounds.len() % 2));
                    m.make_cup_game();
                    current = Some(m);
                    save_key = None;
                    hud = Hud::new(config.show_help);
                    rig.reset();
                    screen = Screen::Menu;
                }
                Some(Some(true)) if wants_deal(&settings, &online) => {
                    replace_save = None;
                    waiting = Some((ask_deal(&mut online, settings), get_time()));
                }
                Some(Some(true)) => {
                    replace_save = None;
                    let mut m = Match::new(&settings, game::clock_seed());
                    alternate_break(&mut m, &mut config);
                    warn_unrewarded(&mut m, &settings, &online, &ladder_progress);
                    current = Some(m);
                    save_key = None;
                    hud = Hud::new(config.show_help);
                    rig.reset();
                    screen = Screen::Menu;
                }
                Some(Some(false)) => {
                    replace_save = None;
                    if let Some(s) = saved::load() {
                        let mut m = Match::resume(&s, config.guide);
                        if cup.as_ref().is_some_and(|c| c.playing == Some(s.seed)) {
                            m.make_cup_game();
                        }
                        current = Some(m);
                        save_key = None;
                        hud = Hud::new(config.show_help);
                        rig.reset();
                        screen = Screen::Menu;
                    } else {
                        saved_balls = None;
                    }
                }
                // Neither (Esc, or a press on the dimmed screen around it).
                Some(None) => replace_save = None,
                None => {}
            }
        }
        if let Some(saved) = dm_saved {
            ui.resume_input(saved);
            online.draw_dms(&mut ui);
        }
        // The chat with other players turned on (opened by writing with it
        // off), over any screen, a game too.
        if let Some(saved) = chat_saved {
            ui.resume_input(saved);
        }
        if ui.focus == Some(CHAT_DIALOG_FOCUS) {
            ui.focus = None;
            ui.wants_keyboard = false;
        }
        if !settings_open {
            online.draw_chat_dialog(&mut ui, &config);
        }
        // A maintenance planned (on the main menu and in the lounge), the
        // auctions' and the credits' pages (opened from the shop).
        if let Some(saved) = ops_saved {
            ui.resume_input(saved);
        }
        if current.is_none() && !settings_open && waiting.is_none() {
            let in_lounge =
                matches!(screen, Screen::Online) && online.stage == online::Stage::Lounge;
            let on_menu = matches!(screen, Screen::Menu);
            online.draw_ops(&mut ui, on_menu || in_lounge);
        }
        // A warning from the staff, over everything else, until OK.
        if current.is_none() && !settings_open && waiting.is_none() {
            online.draw_staff_warning(&mut ui);
        }
        // Invitations: cards in the top right corner, like the system's
        // notifications, on any screen and over any game (an online one
        // only declines them). Focus mode keeps them away during a game.
        if let Some(saved) = invite_cover {
            ui.resume_input(saved);
        }
        let focus = config.focus_mode && current.as_ref().is_some_and(Match::is_focus_game);
        // In the lounge: under the side's tabs, which stay in reach.
        if current.is_none()
            && matches!(screen, Screen::Online)
            && online.stage == online::Stage::Lounge
        {
            invite_top = 142.0;
        }
        let spot = match current.as_ref() {
            // Only watching, or at a daily game's table: free to play (the
            // server stops the watching when the game starts).
            Some(m) if m.is_spectating() || correspondence::is_corr(m) => {
                online::InviteSpot::Lounge
            }
            Some(m) if m.is_online() => online::InviteSpot::OnlineMatch,
            Some(m) => online::InviteSpot::VsComputer { saved: m.is_against_computer() },
            None => online::InviteSpot::Lounge,
        };
        let invites_on = !focus && !settings_open;
        let mut top = invite_top;
        for net in [Some(&mut online), lan.as_mut().map(|l| &mut l.net)].into_iter().flatten() {
            if invites_on {
                // A game (or a game watched) of the other network has the
                // table: this one's invitations can only be declined (one
                // accepted would start a game with nowhere to go).
                let other =
                    current.as_ref().is_some_and(|m| m.is_online()) && match_from_lan != net.lan;
                let spot = if other { online::InviteSpot::OnlineMatch } else { spot };
                // The local network's under the server's, if both have some.
                top = net.draw_invite_cards(&mut ui, top, spot);
            } else {
                net.hide_invite_cards();
            }
        }
        // A new invitation rings once (not in focus mode).
        let chime = online.invite_sound || lan.as_ref().is_some_and(|l| l.net.invite_sound);
        online.invite_sound = false;
        if let Some(l) = lan.as_mut() {
            l.net.invite_sound = false;
        }
        if chime && !focus {
            ui_sounds.push(game::ImpactSound { kind: game::ImpactKind::Chime, speed: 1.0 });
        }
        if ui.take_click() && config.interface_sounds {
            ui_sounds.push(game::ImpactSound { kind: game::ImpactKind::Click, speed: 1.0 });
        }
        sounds.play(&mut ui_sounds, config.volume);
        if let Some(intro) = dev_intro.as_mut() {
            intro.draw(&mut ui, 0.03);
        }
        if current.is_none() && matches!(screen, Screen::Menu) {
            online.toast_left.set(Some(menu.column_right + 16.0));
            online.card_left.set(Some(menu.title_right + 16.0));
        }
        online.draw_toasts(&ui, toast_bottom);
        if let Some(l) = &mut lan {
            l.net.draw_toasts(&ui, toast_bottom);
        }

        // Hover hints go on top of everything (panels, chat, toasts).
        ui.draw_overlays();

        let in_match = current.is_some();
        if in_match != was_in_match {
            was_in_match = in_match;
            fade = 0.85;
        }
        if fade > 0.0 {
            draw_rectangle(0.0, 0.0, ui::vw(), ui::vh(), Color::new(0.0, 0.0, 0.0, fade));
            fade -= dt * 2.2;
        }

        if config.show_fps || options.show_fps {
            let text = format!("{} FPS", get_fps());
            let w = ui.text_size(&text, 13.0, true).x;
            ui.text(&text, ui::vw() - w - 10.0, ui::vh() - 20.0, 13.0, ui::MUTED, true);
        }
        frame += 1;
        if options.show_fps && frame.is_multiple_of(120) {
            println!("fps {}", get_fps());
        }
        // Development aid: SNOOK_LEGEND_SHOTS="300,900" also saves `<name>-300.png`… on the way.
        if let (Some(path), Some(list)) =
            (&options.screenshot, crate::dev::var("SNOOK_LEGEND_SHOTS"))
            && list.split(',').filter_map(|n| n.trim().parse::<u32>().ok()).any(|n| n == frame)
        {
            let mut image = get_screen_data();
            for px in image.bytes.as_chunks_mut::<4>().0 {
                px[3] = 255;
            }
            image.export_png(&path.replace(".png", &format!("-{frame}.png")));
        }
        if let Some(path) = &options.screenshot
            && frame == options.frames
        {
            let mut image = get_screen_data();
            // The framebuffer's alpha is meaningless; save an opaque picture.
            for px in image.bytes.as_chunks_mut::<4>().0 {
                px[3] = 255;
            }
            image.export_png(path);
            break;
        }
        // The game against the computer is kept between shots, so closing
        // the window never loses it; a finished game is forgotten.
        if let Some(m) = current.as_ref().filter(|m| m.is_against_computer()) {
            let over = m.game.game_over().is_some();
            // A clock that ran out (or a choice) changes the turn and the
            // balls without a shot: they count too.
            let in_play = (0..16u8).filter(|&b| m.game.ball(BallId(b)).in_play()).count();
            let key = (
                m.games_played,
                m.game.shots_taken(),
                m.game.current_player(),
                in_play,
                m.at_rest(),
                over,
                // (A turn passed by a choice that changed nothing else.)
                m.turns_begun,
            );
            if save_key != Some(key) {
                save_key = Some(key);
                match saved::SavedGame::kept(m) {
                    saved::Kept::Game(s) => {
                        saved::save(&s);
                        saved_balls = Some(s.balls_left());
                    }
                    // The shot being played ends it: nothing to come back to.
                    saved::Kept::Over => {
                        saved::delete();
                        saved_balls = None;
                    }
                    saved::Kept::Not if over => {
                        saved::delete();
                        saved_balls = None;
                    }
                    saved::Kept::Not => {}
                }
            }
        }
        if let Some(r) = recorder.as_mut()
            && !r.frame()
        {
            break;
        }
        if profiling {
            let ms = frame_start.elapsed().as_secs_f64() * 1000.0;
            cpu = (cpu.0 + ms, cpu.1.max(ms), cpu.2 + 1);
            if cpu.2 == 300 {
                eprintln!("cpu per frame: avg {:.2} ms, worst {:.2} ms", cpu.0 / 300.0, cpu.1);
                cpu = (0.0, 0.0, 0);
            }
        }
        next_frame().await;
        retired.clear();
    }
    config.save();
}

/// Darkened screen corners, like a camera lens: draws the eye to the table.
/// Asks before a new game against the computer takes the place of the one
/// in progress: Some(Some(true)) starts the new one, Some(Some(false)) goes
/// back to the saved one, Some(None) is neither (Esc, or a press on the
/// dimmed screen around it), None while the player decides.
fn draw_replace_save(ui: &mut Ui, balls_left: usize, cup_game: bool) -> Option<Option<bool>> {
    use crate::i18n::{t, tw};
    let (sw, sh) = (ui::vw(), ui::vh());
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.45));
    let w = 460.0f32.min(sw - 24.0);
    let text = if cup_game {
        t("You have your tournament game in progress. A new game takes its place, and the tournament game counts as lost.")
            .to_string()
    } else if balls_left == 1 {
        t("You have a game in progress against the computer, with 1 ball left. A new game takes its place.")
            .to_string()
    } else {
        tw(
            "You have a game in progress against the computer, with {n} balls left. A new game takes its place.",
            &[("n", &balls_left)],
        )
    };
    let lines = ui.wrap(&text, w - 48.0, 14.0);
    let h = 24.0 + 26.0 + 12.0 + lines.len() as f32 * 19.0 + 24.0 + 40.0 + 24.0;
    let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
    ui.block(Rect::new(0.0, 0.0, sw, sh));
    ui.panel(r);
    let mut y = r.y + 24.0;
    ui.text_centered(t("Start a new game?"), sw / 2.0, y, 19.0, ui::GOLD, true);
    y += 26.0 + 12.0;
    for line in &lines {
        ui.text_centered(line, sw / 2.0, y, 14.0, ui::CREAM, false);
        y += 19.0;
    }
    let by = r.y + r.h - 24.0 - 40.0;
    let bw = (w - 48.0 - 12.0) / 2.0;
    if ui.button(Rect::new(r.x + 24.0, by, bw, 40.0), t("Continue that game"), true) {
        return Some(Some(false));
    }
    if ui.primary_button(Rect::new(r.x + 24.0 + bw + 12.0, by, bw, 40.0), t("Start a new one")) {
        return Some(Some(true));
    }
    if ui.escape() || ui.dismissed(r) {
        return Some(None);
    }
    None
}

fn vignette_texture() -> Texture2D {
    const N: u16 = 128;
    let mut rgba = Vec::with_capacity(N as usize * N as usize * 4);
    for y in 0..N {
        for x in 0..N {
            let u = (x as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            let v = (y as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            let d = (u * u * 0.8 + v * v).sqrt();
            let a = ((d - 0.55) / 0.85).clamp(0.0, 1.0);
            let a = a * a * (3.0 - 2.0 * a) * 0.55;
            rgba.extend_from_slice(&[0, 0, 0, (a * 255.0) as u8]);
        }
    }
    let tex = Texture2D::from_rgba8(N, N, &rgba);
    tex.set_filter(FilterMode::Linear);
    tex
}

/// The keyboard "focus" the gift dialog holds while it is up, so the
/// screen under it leaves Esc alone (no field has this id).
const GIFT_DIALOG_FOCUS: u64 = ui::MODAL_FOCUS;

/// The same for the overlay of a game waiting for the server: the screen
/// or the game under it leaves Esc and Enter alone.
const WAITING_FOCUS: u64 = ui::MODAL_FOCUS + 1;
/// The keyboard is the chat dialog's (see the start of the frame).
const CHAT_DIALOG_FOCUS: u64 = ui::MODAL_FOCUS + 2;

fn name_or(name: &str, default: &str) -> String {
    match name.trim() {
        "" => i18n::t(default).to_string(),
        n => n.to_string(),
    }
}

/// Draws the in-game HUD, fading it away while the player aims.
fn hud_draw(
    hud: &mut Hud,
    ui: &mut Ui,
    m: &mut Match,
    aiming: bool,
    opacity: &mut f32,
    dt: f32,
) -> HudAction {
    let target = if aiming { 0.0 } else { 1.0 };
    let speed = if target < *opacity { 7.0 } else { 5.0 };
    *opacity += (target - *opacity) * (1.0 - (-dt * speed).exp());
    paint::set_opacity(*opacity);
    let action = if *opacity < 0.6 {
        let saved = ui.suspend_input();
        let a = hud.draw(ui, m);
        ui.resume_input(saved);
        a
    } else {
        hud.draw(ui, m)
    };
    paint::set_opacity(1.0);
    action
}

/// A game waiting for the server before it starts.
enum Waiting {
    /// The tour's final, played for the reward: the server deals its rack
    /// and who breaks.
    Final(MatchSettings),
    /// A drill session for the tables: the server sets where its tries'
    /// cue balls lie.
    Drill(String),
}

/// How long a game waits for the server before it starts anyway (not
/// counted), in seconds.
const SERVER_WAIT: f64 = 8.0;

/// Signed in to an account of the online server (not as a guest, not on a
/// LAN): what drills and the tour's reward are counted for.
fn has_account(online: &Online) -> bool {
    online.account().is_some()
}

/// Signed in to the account `account`, the connection working (a daily
/// attempt or a drill session goes to the account it was started for, not
/// to another one).
fn signed_in_as(online: &Online, account: &str) -> bool {
    online.live() && online.account().is_some_and(|me| me.eq_ignore_ascii_case(account))
}

/// The account a daily attempt or a ranked drill session was started for:
/// its player, named after the account.
fn run_account(m: &Match) -> &str {
    &m.names[0]
}

/// What a daily attempt or a ranked drill session has made, to send once
/// (it is marked sent); None when there is nothing to send.
fn run_to_send(m: &mut Match) -> Option<snook_legend_proto::ClientMsg> {
    if let Some(run) = m.drill.as_mut().filter(|r| !r.sent && r.session != 0) {
        if run.strokes.is_empty() {
            return None;
        }
        run.sent = true;
        return Some(snook_legend_proto::ClientMsg::DrillSubmit {
            code: run.code.to_string(),
            session: run.session,
            strokes: run.strokes.clone(),
        });
    }
    let run = m.daily.as_mut().filter(|r| !r.sent)?;
    run.sent = true;
    Some(snook_legend_proto::ClientMsg::DailySubmit { day: run.day, strokes: run.strokes.clone() })
}

/// A daily attempt or a ranked drill session leaves the table (the player
/// left it, or another game took its place): what it made goes to the
/// server now, or once its account is signed in again (the player is told).
fn send_run(
    m: &mut Match,
    online: &mut Online,
    unsent: &mut Vec<(String, snook_legend_proto::ClientMsg)>,
) {
    let account = run_account(m).to_string();
    let Some(msg) = run_to_send(m) else { return };
    if signed_in_as(online, &account) {
        online.send(msg);
        return;
    }
    // A few at most (the server refuses old ones anyway).
    if unsent.len() >= 8 {
        unsent.remove(0);
    }
    unsent.push((account, msg));
    online.toast(i18n::t("Your strokes will be sent when you're back online."), false);
}

/// The tour's final, played for the reward: dealt by the server.
/// The Brazilian rules: the break alternates from one game to the next
/// started at this computer (the player one game, the other the next).
fn alternate_break(m: &mut Match, config: &mut Config) {
    if m.game.rules() != snook_legend_core::Rules::Brazilian {
        return;
    }
    let breaker = (config.next_breaker % 2) as usize;
    m.set_breaker(breaker);
    config.next_breaker = 1 - breaker as u8;
    config.save();
}

fn wants_deal(settings: &MatchSettings, online: &Online) -> bool {
    let last = snook_legend_proto::ladder::RUNGS.len() as u8 - 1;
    settings.ladder == Some(last)
        && has_account(online)
        && !online.has_achievement(snook_legend_proto::ladder::ACHIEVEMENT)
}

fn ask_deal(online: &mut Online, settings: MatchSettings) -> Waiting {
    online.ladder_ticket = None;
    online.ticket_refused = false;
    online.awaiting_ticket = true;
    online.send(snook_legend_proto::ClientMsg::LadderTicket);
    Waiting::Final(settings)
}

fn ask_drill(online: &mut Online, code: &str) -> Waiting {
    online.drill_session = None;
    online.drill_starts += 1;
    online.send(snook_legend_proto::ClientMsg::DrillStart { code: code.to_string() });
    Waiting::Drill(code.to_string())
}

/// The tour's final started without the server (signed out): said at
/// once, while the reward is still to be won.
fn warn_unrewarded(
    m: &mut Match,
    settings: &MatchSettings,
    online: &Online,
    progress: &ladder::Progress,
) {
    let last = snook_legend_proto::ladder::RUNGS.len() as u8 - 1;
    if settings.ladder == Some(last)
        && !progress.claimed
        && !online.has_achievement(snook_legend_proto::ladder::ACHIEVEMENT)
    {
        m.notify(
            i18n::t("Not signed in: a win in this final won't earn the reward.").to_string(),
            true,
        );
    }
}

/// A word in the middle of the screen while the server is asked, and how
/// to stop waiting (`hint`).
fn draw_waiting(ui: &mut Ui, text: &str, hint: &str) {
    let (sw, sh) = (ui::vw(), ui::vh());
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.45));
    let widest = ui.text_size(text, 15.0, false).x.max(ui.text_size(hint, 12.0, false).x);
    let w = (widest + 64.0).min(sw - 32.0);
    let r = Rect::new((sw - w) / 2.0, sh / 2.0 - 36.0, w, 72.0);
    ui.panel(r);
    ui.block(Rect::new(0.0, 0.0, sw, sh));
    crate::ui::spinner(vec2(r.x + 24.0, r.y + 26.0), 7.0, ui::GOLD);
    let text = ui.ellipsize(text, w - 56.0, 15.0);
    ui.text(&text, r.x + 42.0, r.y + 17.0, 15.0, ui::CREAM, false);
    let hint = ui.ellipsize(hint, w - 56.0, 12.0);
    ui.text(&hint, r.x + 42.0, r.y + 43.0, 12.0, ui::MUTED, false);
}
