//! Persistent player settings, stored as a small `key = value` file in the
//! user's configuration directory.

use std::path::PathBuf;

/// Overall graphics detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Quality {
    Low,
    Medium,
    High,
    Ultra,
}

impl Quality {
    pub const ALL: [Quality; 4] = [Quality::Low, Quality::Medium, Quality::High, Quality::Ultra];

    pub fn name(self) -> &'static str {
        match self {
            Quality::Low => "Low",
            Quality::Medium => "Medium",
            Quality::High => "High",
            Quality::Ultra => "Ultra",
        }
    }

    fn parse(s: &str) -> Option<Quality> {
        Quality::ALL.into_iter().find(|q| q.name().eq_ignore_ascii_case(s))
    }

    /// Anti-aliasing samples suggested for this preset.
    pub fn default_msaa(self) -> i32 {
        match self {
            Quality::Low => 1,
            Quality::Medium => 2,
            Quality::High => 4,
            Quality::Ultra => 8,
        }
    }
}

/// How much aiming help is drawn on the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AimGuide {
    /// Line to the ghost ball, the object ball's path and the cue ball's tangent.
    Full,
    /// Line to the ghost ball only: judging the cut is up to the player.
    Standard,
    /// A short line from the cue ball (always used in rated games).
    Short,
    /// Nothing on the table: the shot is aimed by eye, along the cue.
    Off,
}

impl AimGuide {
    /// The guide used against a person over the network: the standard one
    /// (ghost ball) or the short line, never the full guide (it would play
    /// the cut for you).
    pub fn online(self) -> AimGuide {
        match self {
            AimGuide::Full => AimGuide::Standard,
            g => g,
        }
    }

    pub const ALL: [AimGuide; 4] =
        [AimGuide::Full, AimGuide::Standard, AimGuide::Short, AimGuide::Off];

    pub fn name(self) -> &'static str {
        match self {
            AimGuide::Full => "Full",
            AimGuide::Standard => "Standard",
            AimGuide::Short => "Short",
            AimGuide::Off => "No guide",
        }
    }

    fn parse(s: &str) -> Option<AimGuide> {
        AimGuide::ALL
            .into_iter()
            .find(|g| g.name().eq_ignore_ascii_case(s))
            .or_else(|| s.eq_ignore_ascii_case("off").then_some(AimGuide::Off))
    }
}

/// The default game server; change it in Settings or with `SNOOK_LEGEND_SERVER`
/// when building (an empty `SNOOK_LEGEND_SERVER`, as CI passes when the variable
/// isn't set, keeps the public server).
pub const DEFAULT_SERVER: &str = match option_env!("SNOOK_LEGEND_SERVER") {
    Some(s) if !s.is_empty() => s,
    _ => "wss://snook-server.onrender.com/ws",
};

impl Config {
    /// The saved session, if the server in use is the one that gave it. A
    /// session is a key to the account on its own server: pointing the game
    /// at another server (Settings → Game server) must not hand it over.
    pub fn session_token(&self) -> &str {
        let owner =
            if self.token_server.is_empty() { DEFAULT_SERVER } else { self.token_server.trim() };
        if owner == self.server.trim() { &self.token } else { "" }
    }
}

/// What earlier builds saved as the default; read as "the default" so they
/// move to the public server (a local test server is `ws://127.0.0.1:7878/ws`).
const OLD_DEFAULT_SERVER: &str = "ws://localhost:7878/ws";

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub player_name: String,
    pub second_player_name: String,
    pub guide: AimGuide,
    /// The head string drawn on the table at all times (off: only while
    /// the cue ball is being placed behind it).
    pub head_string: bool,
    pub show_help: bool,
    /// Pull the camera back to watch each shot.
    pub shot_camera: bool,
    /// The camera turns with the cue (off: it stays where the player puts it).
    pub camera_follow: bool,
    /// The graphics were already matched to this computer on the first run.
    pub graphics_checked: bool,
    pub language: crate::i18n::Lang,
    /// Darkened screen edges.
    pub vignette: bool,
    /// Dark stage around the table (off: the whole bar room).
    pub focused_scene: bool,
    pub show_fps: bool,
    /// Show the round trip to the server (lounge and online matches).
    pub show_ping: bool,
    /// The shadows of the balls and of the cue on the table (some players
    /// read the table better without them).
    pub shadows: bool,
    /// Computer opponents make remarks during a game.
    pub opponent_talk: bool,
    /// The table only ever seen from above (no 3D view).
    pub top_view_only: bool,
    /// The profile picture (a code from `snook_legend_proto::AVATARS`).
    pub avatar: String,
    /// Interface size (1.0 = automatic for the window size).
    pub ui_size: f32,
    /// Multiplier for fine aiming with the mouse.
    pub aim_sensitivity: f32,
    pub quality: Quality,
    /// Multisample anti-aliasing samples (1 = off). Applies on restart.
    pub msaa: i32,
    pub fullscreen: bool,
    pub volume: f32,
    pub server: String,
    pub username: String,
    /// Login token for automatic sign-in (empty when logged out).
    pub token: String,
    /// The server that gave the token (empty in files from before this was
    /// kept: the default server). The token is never sent to another one.
    pub token_server: String,
    /// Last difficulty chosen against the computer (0 easy, 1 medium, 2 hard).
    pub difficulty: u8,
    /// The computer opponent last picked (a personality's code).
    pub opponent: String,
    /// Offline shot clock in seconds (0: no clock).
    pub shot_clock: u32,
    /// Name used on local network games.
    pub lan_name: String,
    /// Shop items in use (as the server last confirmed them), so offline
    /// games look the same: cloth, cue, chalk and ball set.
    pub cloth: String,
    pub cue: String,
    pub chalk: String,
    pub balls: String,
    /// The winning effect on the 8.
    pub effect: String,
    /// The table and the hall around it (only this player sees them).
    pub table: String,
    pub hall: String,
    /// The level of the cue in use (its marks), as the server last said.
    pub cue_level: u8,
    /// The player declined to use their payment e-mail for recovery.
    pub email_offer_declined: bool,
    /// The newest note on the lounge's board the player has seen.
    pub board_seen: i64,
    /// Watching a game: the camera cuts between angles like on TV.
    pub tv_camera: bool,
    /// Windows notifications when a friend comes online or a tournament
    /// is about to start (while the game is in the background).
    pub desktop_notifications: bool,
    /// Keyboard shortcuts (Settings → Controls).
    pub keys: crate::keys::Bindings,
    /// Download and install new versions in the background.
    pub auto_update: bool,
    /// The newest version whose "What's new" the player has seen (empty:
    /// a settings file from before the notes, i.e. before 0.2.0).
    pub last_version: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            // Empty: the field shows the default name as a hint, in the
            // current language, and that name is used if it stays empty.
            player_name: String::new(),
            second_player_name: String::new(),
            guide: AimGuide::Standard,
            head_string: false,
            show_help: false,
            shot_camera: true,
            camera_follow: false,
            graphics_checked: false,
            language: crate::i18n::Lang::system(),
            vignette: true,
            focused_scene: true,
            show_fps: false,
            show_ping: false,
            shadows: true,
            opponent_talk: true,
            top_view_only: false,
            avatar: String::new(),
            aim_sensitivity: 1.0,
            ui_size: 1.0,
            quality: Quality::High,
            msaa: 4,
            fullscreen: false,
            volume: 0.8,
            server: DEFAULT_SERVER.into(),
            username: String::new(),
            token: String::new(),
            token_server: String::new(),
            difficulty: 1,
            opponent: crate::opponents::DEFAULT_PERSONALITY.to_string(),
            shot_clock: 0,
            lan_name: String::new(),
            cloth: "cloth_green".into(),
            cue: "cue_classic".into(),
            chalk: "chalk_blue".into(),
            balls: "balls_classic".into(),
            effect: "effect_none".into(),
            table: "table_classic".into(),
            hall: "hall_pool".into(),
            cue_level: 0,
            email_offer_declined: false,
            board_seen: 0,
            tv_camera: false,
            desktop_notifications: true,
            keys: crate::keys::Bindings::default(),
            auto_update: true,
            // A first run has nothing new to tell.
            last_version: env!("CARGO_PKG_VERSION").into(),
        }
    }
}

/// Where the settings, the session, the tour and the saved games live.
/// The folder of the game's earlier name ([`folders`]) is moved over the
/// first time (see [`move_old_folder`]), so nobody loses anything.
pub fn config_dir() -> Option<PathBuf> {
    let (new, old) = folders()?;
    static MOVED: std::sync::Once = std::sync::Once::new();
    MOVED.call_once(|| move_old_folder(&old, &new));
    Some(new)
}

/// The folder (named after the game) and the one of its earlier name,
/// before 0.4.0. The earlier name stays here only to find that folder.
fn folders() -> Option<(PathBuf, PathBuf)> {
    if cfg!(windows) {
        let base = PathBuf::from(std::env::var_os("APPDATA")?);
        Some((base.join("Snook Legend"), base.join("Snookered")))
    } else if cfg!(target_os = "macos") {
        let base = PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support");
        Some((base.join("Snook Legend"), base.join("Snookered")))
    } else {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some((base.join("snook-legend"), base.join("snookered")))
    }
}

/// Moves the folder of the old name to the new one, once: only when the new
/// one doesn't exist yet. A folder that can't be moved (another copy of the
/// game has it open on Windows) is copied instead and left where it was.
fn move_old_folder(old: &std::path::Path, new: &std::path::Path) {
    if new.exists() || !old.is_dir() {
        return;
    }
    if std::fs::rename(old, new).is_ok() {
        return;
    }
    fn copy(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            let target = to.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                copy(&entry.path(), &target)?;
            } else {
                std::fs::copy(entry.path(), &target)?;
            }
        }
        Ok(())
    }
    if copy(old, new).is_err() {
        // Half a copy is worse than none: the old folder is read again
        // next time.
        let _ = std::fs::remove_dir_all(new);
    }
}

fn config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("settings.ini"))
}

impl Config {
    /// Loads the settings, falling back to defaults for anything missing.
    pub fn load() -> Config {
        config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| Config::parse(&t))
            .unwrap_or_default()
    }

    pub fn parse(text: &str) -> Config {
        let mut c = Config { last_version: String::new(), ..Config::default() };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            if c.keys.parse_line(k, v) {
                continue;
            }
            match k {
                "player_name" if !v.is_empty() => c.player_name = v.chars().take(20).collect(),
                "second_player_name" if !v.is_empty() => {
                    c.second_player_name = v.chars().take(20).collect()
                }
                "guide" => c.guide = AimGuide::parse(v).unwrap_or(c.guide),
                "head_string" => c.head_string = v == "true",
                "show_help" => c.show_help = v == "true",
                "shot_camera" => c.shot_camera = v == "true",
                "camera_follow" => c.camera_follow = v == "true",
                "graphics_checked" => c.graphics_checked = v == "true",
                "language" => c.language = crate::i18n::Lang::parse(v).unwrap_or(c.language),
                "vignette" => c.vignette = v == "true",
                "focused_scene" => c.focused_scene = v == "true",
                "show_fps" => c.show_fps = v == "true",
                "show_ping" => c.show_ping = v == "true",
                "shadows" => c.shadows = v != "false",
                "opponent_talk" => c.opponent_talk = v != "false",
                "top_view_only" => c.top_view_only = v == "true",
                "avatar" if snook_legend_proto::valid_avatar(v) => c.avatar = v.to_string(),
                "ui_size" => {
                    c.ui_size = v.parse::<f32>().map(|x| x.clamp(0.7, 1.5)).unwrap_or(c.ui_size)
                }
                "aim_sensitivity" => {
                    c.aim_sensitivity =
                        v.parse::<f32>().map(|x| x.clamp(0.3, 2.5)).unwrap_or(c.aim_sensitivity)
                }
                "quality" => c.quality = Quality::parse(v).unwrap_or(c.quality),
                "msaa" => {
                    c.msaa =
                        v.parse::<i32>().ok().filter(|n| [1, 2, 4, 8].contains(n)).unwrap_or(c.msaa)
                }
                "fullscreen" => c.fullscreen = v == "true",
                "volume" => {
                    c.volume = v.parse::<f32>().map(|x| x.clamp(0.0, 1.0)).unwrap_or(c.volume)
                }
                "server" if !v.is_empty() && v != OLD_DEFAULT_SERVER => c.server = v.to_string(),
                "username" => c.username = v.to_string(),
                "token" => c.token = v.to_string(),
                "token_server" => c.token_server = v.to_string(),
                "difficulty" => c.difficulty = v.parse::<u8>().map(|d| d.min(2)).unwrap_or(1),
                "opponent" => c.opponent = crate::opponents::chosen(v).code.to_string(),
                "shot_clock" => {
                    c.shot_clock =
                        v.parse::<u32>().ok().filter(|s| [0, 30, 60].contains(s)).unwrap_or(0)
                }
                "lan_name" => c.lan_name = v.chars().take(16).collect(),
                // Only real items of the right kind (a typo means the default).
                "cloth" | "cue" | "chalk" | "balls" | "effect" | "table" | "hall" => {
                    let fits =
                        snook_legend_proto::shop::item(v).is_some_and(|i| i.category.code() == k);
                    if fits {
                        match k {
                            "cloth" => c.cloth = v.to_string(),
                            "cue" => c.cue = v.to_string(),
                            "chalk" => c.chalk = v.to_string(),
                            "effect" => c.effect = v.to_string(),
                            "table" => c.table = v.to_string(),
                            "hall" => c.hall = v.to_string(),
                            _ => c.balls = v.to_string(),
                        }
                    }
                }
                "cue_level" => {
                    c.cue_level =
                        v.parse::<u8>().unwrap_or(0).min(snook_legend_proto::shop::MAX_CUE_LEVEL)
                }
                "email_offer_declined" => c.email_offer_declined = v == "true",
                "board_seen" => c.board_seen = v.parse().unwrap_or(0),
                // Renamed when the default became off: the old key was
                // saved for everyone, so it says nothing about a choice.
                "tv_view" => c.tv_camera = v == "true",
                "desktop_notifications" => c.desktop_notifications = v != "false",
                "auto_update" => c.auto_update = v != "false",
                "last_version" => c.last_version = v.chars().take(32).collect(),
                _ => {}
            }
        }
        c.keys.dedup();
        c
    }

    pub fn to_text(&self) -> String {
        format!(
            "# Snook Legend settings\nplayer_name = {}\nsecond_player_name = {}\nguide = {}\nhead_string = {}\nshow_help = {}\nshot_camera = {}\ncamera_follow = {}\ngraphics_checked = {}\nlanguage = {}\nvignette = {}\nfocused_scene = {}\nshow_fps = {}\nshow_ping = {}\nshadows = {}\nopponent_talk = {}\ntop_view_only = {}\navatar = {}\naim_sensitivity = {:.2}\nui_size = {:.2}\n\
             quality = {}\nmsaa = {}\nfullscreen = {}\nvolume = {:.2}\nserver = {}\nusername = {}\ntoken = {}\ntoken_server = {}\n\
             difficulty = {}\nopponent = {}\nshot_clock = {}\nlan_name = {}\ncloth = {}\ncue = {}\nchalk = {}\nballs = {}\neffect = {}\ntable = {}\nhall = {}\ncue_level = {}\nemail_offer_declined = {}\n\
             board_seen = {}\ntv_view = {}\ndesktop_notifications = {}\nauto_update = {}\nlast_version = {}\n{}",
            self.player_name,
            self.second_player_name,
            self.guide.name(),
            self.head_string,
            self.show_help,
            self.shot_camera,
            self.camera_follow,
            self.graphics_checked,
            self.language.code(),
            self.vignette,
            self.focused_scene,
            self.show_fps,
            self.show_ping,
            self.shadows,
            self.opponent_talk,
            self.top_view_only,
            self.avatar,
            self.aim_sensitivity,
            self.ui_size,
            self.quality.name(),
            self.msaa,
            self.fullscreen,
            self.volume,
            self.server,
            self.username,
            self.token,
            self.token_server,
            self.difficulty,
            self.opponent,
            self.shot_clock,
            self.lan_name,
            self.cloth,
            self.cue,
            self.chalk,
            self.balls,
            self.effect,
            self.table,
            self.hall,
            self.cue_level,
            self.email_offer_declined,
            self.board_seen,
            self.tv_camera,
            self.desktop_notifications,
            self.auto_update,
            self.last_version,
            self.keys.to_text()
        )
    }

    /// Writes the settings; failures are logged and otherwise ignored.
    pub fn save(&self) {
        let Some(path) = config_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Written to a temporary file first, so a crash never leaves a
        // half-written settings file; private to the user (it holds the
        // session token).
        let tmp = path.with_extension("ini.tmp");
        let result = write_private(&tmp, self.to_text().as_bytes())
            .and_then(|()| std::fs::rename(&tmp, &path));
        if let Err(e) = result {
            eprintln!("could not save settings to {}: {e}", path.display());
        }
    }
}

/// Writes `data` to a file only the user can read, from the start (the
/// token is never readable by others, not even for a moment).
fn write_private(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(0o600);
        // A leftover file keeps its old mode: tighten it before writing.
        if let Ok(meta) = std::fs::metadata(path)
            && meta.permissions().mode() & 0o077 != 0
        {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
    }
    let mut file = options.open(path)?;
    file.write_all(data)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_old_folder_moves_once() {
        let base = std::env::temp_dir().join(format!("snook-move-{}", std::process::id()));
        let (old, new) = (base.join("Snookered"), base.join("Snook Legend"));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(old.join("replays")).unwrap();
        std::fs::write(old.join("settings.ini"), "volume = 0.5\n").unwrap();
        std::fs::write(old.join("replays/1.json"), "{}").unwrap();
        move_old_folder(&old, &new);
        assert_eq!(std::fs::read_to_string(new.join("settings.ini")).unwrap(), "volume = 0.5\n");
        assert!(new.join("replays/1.json").exists());
        // Never over a folder of the new name.
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("settings.ini"), "volume = 0.9\n").unwrap();
        move_old_folder(&old, &new);
        assert_eq!(std::fs::read_to_string(new.join("settings.ini")).unwrap(), "volume = 0.5\n");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn round_trip_and_validation() {
        let c = Config {
            player_name: "Allan".into(),
            head_string: true,
            quality: Quality::Low,
            msaa: 8,
            volume: 0.25,
            token: "abc".into(),
            ..Config::default()
        };
        assert_eq!(Config::parse(&c.to_text()), c);
        let bad = Config::parse("msaa = 3\nvolume = 9\nquality = potato\nnonsense");
        assert_eq!(bad.msaa, Config::default().msaa);
        assert_eq!(bad.volume, 1.0);
        assert_eq!(bad.quality, Quality::High);
        // The head string is shown only when needed unless asked for.
        assert!(!bad.head_string);
        assert!(!Config::default().head_string);
    }

    /// A saved session goes only to the server that gave it.
    #[test]
    fn the_session_stays_with_its_server() {
        let mut c = Config { token: "abc".into(), ..Config::default() };
        // A file from before the server was kept: the default server's.
        assert_eq!(c.session_token(), "abc");
        c.server = "wss://elsewhere.example/ws".into();
        assert_eq!(c.session_token(), "", "never sent to another server");
        c.token_server = "wss://elsewhere.example/ws".into();
        assert_eq!(c.session_token(), "abc");
        c.server = DEFAULT_SERVER.into();
        assert_eq!(c.session_token(), "");
        assert_eq!(Config::parse(&c.to_text()), c);
    }

    #[test]
    fn tables_halls_and_cue_levels() {
        let c = Config {
            table: "table_club".into(),
            hall: "hall_beach".into(),
            cue_level: 4,
            ..Config::default()
        };
        assert_eq!(Config::parse(&c.to_text()), c);
        // Only items of the right kind; levels within range.
        let bad = Config::parse("table = hall_beach\nhall = cue_maple\ncue_level = 99");
        assert_eq!(bad.table, "table_classic");
        assert_eq!(bad.hall, "hall_pool");
        assert_eq!(bad.cue_level, snook_legend_proto::shop::MAX_CUE_LEVEL);
    }

    #[test]
    fn keys_updates_and_old_files() {
        use crate::keys::Action;
        use macroquad::prelude::KeyCode;
        let mut c =
            Config { auto_update: false, last_version: "0.2.0".into(), ..Config::default() };
        c.keys.bind(Action::Replay, 0, Some(KeyCode::F5));
        c.keys.bind(Action::Shoot, 0, Some(KeyCode::W));
        let back = Config::parse(&c.to_text());
        assert_eq!(back, c);
        assert_eq!(back.keys.get(Action::ElevationUp), [Some(KeyCode::Space), None]);

        // A file from an older version: default keys, updates on, and the
        // notes of this version still to show.
        let old = Config::parse("# Snook Legend settings\nvolume = 0.50\nfullscreen = true\n");
        assert_eq!(old.keys, crate::keys::Bindings::default());
        assert!(old.auto_update);
        assert_eq!(old.last_version, "");
        assert_eq!(Config::default().last_version, env!("CARGO_PKG_VERSION"));
        // Nonsense keys fall back to the defaults.
        let bad = Config::parse("key.shoot = warp\nkey.nothing = a\nkey.replay = ,\n");
        assert_eq!(bad.keys.get(Action::Shoot), Action::Shoot.defaults());
        assert_eq!(bad.keys.get(Action::Replay), [None, None]);
    }
}
