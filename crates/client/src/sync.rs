//! The settings that follow the account: kept on the server, so the game
//! signed in on another computer plays the same (the aiming guide, the
//! camera, the keys, the language, the muted players…). What belongs to the
//! computer stays on it: the graphics (quality, anti-aliasing, shadows, the
//! scene, full screen), the size of the interface, the volume, the server
//! and the session.
//!
//! The game sends its settings a few seconds after they change; after
//! signing in it gets the account's back and keeps the newest: the server's
//! when they were saved after the last change made here, these otherwise
//! (a change made here while signed out goes up then).

use crate::config::Config;
use snook_legend_proto::{ClientMsg, MAX_SETTINGS_BYTES};

/// The settings that follow the account (keys of the settings file; the
/// keyboard shortcuts, `key.*`, too).
pub const PORTABLE: &[&str] = &[
    "player_name",
    "second_player_name",
    "guide",
    "head_string",
    "show_help",
    "shot_camera",
    "camera_follow",
    "language",
    "show_ping",
    "opponent_talk",
    "top_view_only",
    "aim_sensitivity",
    "difficulty",
    "opponent",
    "shot_clock",
    "tv_view",
    "desktop_notifications",
    "interface_sounds",
    "focus_mode",
    "rules",
    "mute_chat",
    "hide_reactions",
    "muted_chat",
    "muted_reactions",
];

/// A change is sent this long after the last one (a slider dragged sends
/// once).
const SETTLE_SECS: f64 = 3.0;
/// How often the settings are looked at for changes.
const CHECK_EVERY: f32 = 1.0;

fn portable_key(k: &str) -> bool {
    PORTABLE.contains(&k) || k.starts_with("key.")
}

/// The settings that follow the account, as the JSON the server keeps.
pub fn portable(config: &Config) -> String {
    let mut map = serde_json::Map::new();
    for line in config.to_text().lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        if portable_key(k) {
            map.insert(k.to_string(), serde_json::Value::String(v.to_string()));
        }
    }
    serde_json::Value::Object(map).to_string()
}

/// `config` with the account's settings from `data` (only those that follow
/// the account; the rest stays). The keys not in `data` are the defaults.
pub fn apply(config: &Config, data: &str) -> Option<Config> {
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(data).ok()?;
    let mut base = config.clone();
    base.keys = crate::keys::Bindings::default();
    let mut text = base.to_text();
    text.push('\n');
    for (k, v) in &map {
        // Only settings that follow the account, one line each (a line
        // break in a value would slip another setting in, the session's).
        let Some(v) = v.as_str() else { continue };
        if !portable_key(k) || v.contains(['\n', '\r']) || k.contains(['\n', '\r', '=']) {
            continue;
        }
        text.push_str(&format!("{k} = {v}\n"));
    }
    let mut out = Config::parse(&text);
    // (What parsing a file assumes of a new one doesn't apply here.)
    out.last_version = config.last_version.clone();
    out.coached = config.coached;
    Some(out)
}

/// Keeps the settings in step with the account.
#[derive(Default)]
pub struct SettingsSync {
    /// The settings as last sent or received (changes are against it).
    known: Option<String>,
    /// A change seen and not sent yet: when it was last seen to change.
    changed: Option<f64>,
    timer: f32,
    /// The session whose settings came (a new one: they come again).
    session: Option<u32>,
}

impl SettingsSync {
    /// Called every frame: what to send, if anything. `account`: the
    /// account signed in on the official server (None otherwise), and its
    /// session's number. Returns true if `config` changed (to save it).
    pub fn update(
        &mut self,
        dt: f32,
        now: f64,
        config: &mut Config,
        account: Option<(&str, u32)>,
        out: &mut Vec<ClientMsg>,
    ) -> bool {
        let mut save = false;
        self.timer -= dt;
        if self.timer > 0.0 {
            return false;
        }
        self.timer = CHECK_EVERY;
        let data = portable(config);
        match &self.known {
            None => self.known = Some(data.clone()),
            Some(known) if *known != data => {
                if self.changed.is_none() {
                    // A change made here, also while signed out: it is newer
                    // than what the server kept before now.
                    config.settings_changed_at = crate::game::unix_now();
                    save = true;
                }
                self.changed = Some(now);
                self.known = Some(data.clone());
            }
            Some(_) => {}
        }
        // Sent once it settles, when this session's settings have come
        // (until then they could be older than the account's).
        let ready = account.is_some_and(|(_, s)| self.session == Some(s));
        if ready
            && self.changed.is_some_and(|at| now - at >= SETTLE_SECS)
            && data.len() <= MAX_SETTINGS_BYTES
        {
            self.changed = None;
            out.push(ClientMsg::SaveSettings { data });
        }
        save
    }

    /// The account's settings came (after signing in): the newest are kept.
    /// Returns true if `config` changed (to save it).
    pub fn received(
        &mut self,
        config: &mut Config,
        account: &str,
        session: u32,
        data: &str,
        updated_at: i64,
        out: &mut Vec<ClientMsg>,
    ) -> bool {
        self.session = Some(session);
        let same = config.settings_account.eq_ignore_ascii_case(account);
        let local_newer = same && config.settings_changed_at > updated_at;
        let newer = !same || updated_at > config.settings_synced_at;
        if data.is_empty() || local_newer || (!newer && portable(config) != data) {
            // The account has none, or this computer's are the newest.
            let mine = portable(config);
            self.known = Some(mine.clone());
            self.changed = None;
            config.settings_account = account.to_string();
            out.push(ClientMsg::SaveSettings { data: mine });
            return true;
        }
        if newer && let Some(fresh) = apply(config, data) {
            let language = fresh.language != config.language;
            *config = fresh;
            if language {
                crate::i18n::set_lang(config.language);
            }
        }
        config.settings_account = account.to_string();
        config.settings_synced_at = updated_at;
        config.settings_changed_at = 0;
        self.known = Some(portable(config));
        self.changed = None;
        true
    }

    /// The server kept what was sent.
    pub fn saved(&mut self, config: &mut Config, updated_at: i64) -> bool {
        config.settings_synced_at = updated_at;
        if self.changed.is_none() {
            config.settings_changed_at = 0;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AimGuide, Quality};

    #[test]
    fn only_what_follows_the_account_travels() {
        let mine = Config {
            guide: AimGuide::Short,
            quality: Quality::Low,
            volume: 0.1,
            token: "secret".into(),
            mute_chat: true,
            muted_reactions: vec!["ana".into()],
            ..Config::default()
        };
        let data = portable(&mine);
        assert!(data.contains("\"guide\":\"Short\""));
        assert!(!data.contains("secret") && !data.contains("quality") && !data.contains("volume"));
        // Another computer: its graphics, volume and session stay.
        let other = Config {
            quality: Quality::Ultra,
            volume: 0.9,
            token: "t2".into(),
            ..Config::default()
        };
        let got = apply(&other, &data).unwrap();
        assert_eq!(got.guide, AimGuide::Short);
        assert!(got.mute_chat);
        assert_eq!(got.muted_reactions, ["ana"]);
        assert_eq!((got.quality, got.volume, got.token.as_str()), (Quality::Ultra, 0.9, "t2"));
    }

    #[test]
    fn nothing_else_slips_in() {
        let other = Config { token: "t2".into(), ..Config::default() };
        let data = r#"{"token":"stolen","server":"ws://evil","guide":"Short\ntoken = stolen2","quality":"Low"}"#;
        let got = apply(&other, data).unwrap();
        assert_eq!(got.token, "t2");
        assert_eq!(got.server, other.server);
        assert_eq!(got.guide, other.guide);
        assert_eq!(got.quality, other.quality);
        assert!(apply(&other, "not json").is_none());
    }

    #[test]
    fn the_newest_settings_win() {
        let mut sync = SettingsSync::default();
        let mut out = Vec::new();
        let theirs = portable(&Config { guide: AimGuide::Off, ..Config::default() });
        // Never in step with this account: the account's come.
        let mut c = Config::default();
        assert!(sync.received(&mut c, "ana", 1, &theirs, 100, &mut out));
        assert_eq!((c.guide, c.settings_synced_at, out.len()), (AimGuide::Off, 100, 0));
        // Changed here after that: sent once it settles.
        c.guide = AimGuide::Short;
        sync.update(1.0, 10.0, &mut c, Some(("ana", 1)), &mut out);
        assert!(c.settings_changed_at > 0);
        assert!(out.is_empty());
        sync.update(1.0, 14.0, &mut c, Some(("ana", 1)), &mut out);
        assert!(
            matches!(out.pop(), Some(ClientMsg::SaveSettings { data }) if data.contains("Short"))
        );
        sync.saved(&mut c, 200);
        assert_eq!((c.settings_synced_at, c.settings_changed_at), (200, 0));
        // Signing in again, nothing newer there: nothing changes.
        let same = portable(&c);
        assert!(sync.received(&mut c, "ana", 2, &same, 200, &mut out));
        assert_eq!(c.guide, AimGuide::Short);
        // An account without settings gets these.
        let mut fresh = Config::default();
        sync.received(&mut fresh, "bob", 3, "", 0, &mut out);
        assert!(matches!(out.pop(), Some(ClientMsg::SaveSettings { .. })));
        // A change made here later than the server's: it goes up.
        let mut c2 = Config {
            settings_account: "ana".into(),
            settings_synced_at: 200,
            settings_changed_at: 500,
            guide: AimGuide::Full,
            ..Config::default()
        };
        sync.received(&mut c2, "ana", 4, &theirs, 300, &mut out);
        assert_eq!(c2.guide, AimGuide::Full);
        assert!(matches!(out.pop(), Some(ClientMsg::SaveSettings { .. })));
    }
}
