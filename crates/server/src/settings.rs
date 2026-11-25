//! Settings the staff change from the portal while the server runs: the
//! shot clock, the lounge's message, the weekly cup, the connection limits,
//! the economy's numbers. No redeploy, no restart.
//!
//! The environment (`SNOOK_LEGEND_SHOT_CLOCK`, `SNOOK_LEGEND_MOTD`,
//! `SNOOK_LEGEND_CUP`…) gives each its first value, its default; a value set
//! in the portal is kept in the database (`live_settings`) and wins until it
//! is reset. Every change is applied at once and written to the audit log.

use std::collections::BTreeMap;
use std::sync::RwLock;
use std::time::Duration;

use serde_json::{Value, json};
use snook_legend_proto::{Rules, TournamentFormat, clean_text};

use crate::lounge::{Cup, Settings};
use crate::store::SettingRow;
use crate::{Config, Server};

/// What a setting holds, and what it may be.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Int {
        min: i64,
        max: i64,
    },
    Bool,
    /// Text up to this many characters.
    Text {
        max: usize,
    },
    /// One of these words.
    Choice(&'static [&'static str]),
    /// A time of day, `HH:MM`.
    Time,
    /// A shop item's code (empty: none).
    Item,
}

impl Kind {
    fn code(self) -> &'static str {
        match self {
            Kind::Int { .. } => "int",
            Kind::Bool => "bool",
            Kind::Text { .. } => "text",
            Kind::Choice(_) => "choice",
            Kind::Time => "time",
            Kind::Item => "item",
        }
    }

    /// The range the portal shows (and checks).
    fn range(self) -> Value {
        match self {
            Kind::Int { min, max } => json!({ "min": min, "max": max }),
            Kind::Text { max } => json!({ "max_length": max }),
            Kind::Choice(options) => json!({ "options": options }),
            Kind::Time => json!({ "format": "HH:MM" }),
            Kind::Bool | Kind::Item => Value::Null,
        }
    }

    /// `value` made valid, or why it can't be.
    pub fn check(self, value: &Value) -> Result<Value, String> {
        match self {
            Kind::Int { min, max } => {
                let n = value
                    .as_i64()
                    .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
                    .ok_or("A whole number is needed.")?;
                if !(min..=max).contains(&n) {
                    return Err(format!("From {min} to {max}."));
                }
                Ok(json!(n))
            }
            Kind::Bool => value.as_bool().map(Value::Bool).ok_or_else(|| "true or false.".into()),
            Kind::Text { max } => {
                let text = value.as_str().ok_or("Text is needed.")?;
                Ok(json!(clean_text(text, max)))
            }
            Kind::Choice(options) => {
                let text = value
                    .as_str()
                    .map(str::to_string)
                    .or_else(|| value.as_i64().map(|n| n.to_string()))
                    .ok_or("Choose one of the options.")?;
                if !options.contains(&text.as_str()) {
                    return Err(format!("One of: {}.", options.join(", ")));
                }
                Ok(json!(text))
            }
            Kind::Time => {
                let text = value.as_str().ok_or("A time is needed (HH:MM).")?.trim();
                parse_time(text).ok_or("A time is needed (HH:MM).")?;
                Ok(json!(text))
            }
            Kind::Item => {
                let code = value.as_str().ok_or("An item code is needed.")?.trim();
                if !code.is_empty() && snook_legend_proto::shop::item(code).is_none() {
                    return Err("No such item.".into());
                }
                Ok(json!(code))
            }
        }
    }
}

/// `HH:MM` as (hour, minute).
fn parse_time(text: &str) -> Option<(u32, u32)> {
    let (h, m) = text.split_once(':')?;
    let (h, m): (u32, u32) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

/// A setting the staff may change.
#[derive(Debug, Clone, Copy)]
pub struct Def {
    pub key: &'static str,
    /// Where the portal shows it.
    pub group: &'static str,
    pub kind: Kind,
    pub description: &'static str,
}

const fn def(key: &'static str, group: &'static str, kind: Kind, description: &'static str) -> Def {
    Def { key, group, kind, description }
}

pub const SHOT_CLOCK: &str = "shot_clock_secs";
pub const MOTD: &str = "motd";
pub const RECONNECT_GRACE: &str = "reconnect_grace_secs";
pub const NO_SHOW: &str = "tournament_no_show_secs";
pub const MAX_OPEN_TOURNAMENTS: &str = "max_open_tournaments";
pub const SEEK_LIFETIME: &str = "seek_lifetime_secs";
pub const CHAT_HISTORY: &str = "chat_history";
pub const MAX_CONNECTIONS: &str = "max_connections_per_ip";
pub const MESSAGES_PER_SEC: &str = "messages_per_sec";
pub const MESSAGE_BURST: &str = "message_burst";
pub const CUP_ENABLED: &str = "cup_enabled";
pub const CUP_NAME: &str = "cup_name";
pub const CUP_WEEKDAY: &str = "cup_weekday";
pub const CUP_TIME: &str = "cup_time";
pub const CUP_UTC_OFFSET: &str = "cup_utc_offset_minutes";
pub const CUP_RULES: &str = "cup_rules";
pub const CUP_FORMAT: &str = "cup_format";
pub const CUP_MINUTES: &str = "cup_minutes";
pub const CUP_SIZE: &str = "cup_size";
pub const CUP_FEE: &str = "cup_entry_fee";
pub const CUP_PRIZE: &str = "cup_house_prize";
pub const NAME_CHANGE_PRICE: &str = "name_change_price";
pub const HOUSE_CUT: &str = "tournament_house_cut_percent";
pub const DAILY_BASE: &str = "daily_bonus_base";
pub const DAILY_STEP: &str = "daily_bonus_step";
pub const DAILY_DAYS: &str = "daily_streak_days";
pub const VIP_MONTH_ITEM: &str = "vip_month_item";

const RULES: &[&str] = &["brazilian", "official"];
const FORMATS: &[&str] = &["arena", "knockout"];
const SIZES: &[&str] = &["4", "8", "16", "32"];

/// Every setting, in the order the portal shows them.
pub const DEFS: &[Def] = &[
    def(SHOT_CLOCK, "games", Kind::Int { min: 10, max: 120 }, "Seconds for each shot."),
    def(
        RECONNECT_GRACE,
        "games",
        Kind::Int { min: 15, max: 600 },
        "Seconds a player who lost the connection has to come back before losing the game.",
    ),
    def(MOTD, "lounge", Kind::Text { max: 300 }, "The message shown at the top of the lounge."),
    def(
        CHAT_HISTORY,
        "lounge",
        Kind::Int { min: 10, max: 500 },
        "Lounge chat lines kept and shown to who arrives.",
    ),
    def(
        SEEK_LIFETIME,
        "lounge",
        Kind::Int { min: 60, max: 86_400 },
        "Seconds a note stays on the \"looking for a game\" board.",
    ),
    def(
        MAX_CONNECTIONS,
        "limits",
        Kind::Int { min: 1, max: 1000 },
        "Connections at once from one address (a home, a phone; IPv6 by its /64).",
    ),
    def(
        MESSAGES_PER_SEC,
        "limits",
        Kind::Int { min: 5, max: 500 },
        "Messages a second one connection may send, sustained (aiming sends about 20). New connections.",
    ),
    def(
        MESSAGE_BURST,
        "limits",
        Kind::Int { min: 10, max: 2000 },
        "Messages one connection may send at once, in a burst. New connections.",
    ),
    def(
        MAX_OPEN_TOURNAMENTS,
        "tournaments",
        Kind::Int { min: 1, max: 200 },
        "Tournaments open at once (taking names or running).",
    ),
    def(
        NO_SHOW,
        "tournaments",
        Kind::Int { min: 30, max: 1800 },
        "Seconds a tournament player may be missing when their game is ready before losing it.",
    ),
    def(
        HOUSE_CUT,
        "tournaments",
        Kind::Int { min: 0, max: 50 },
        "Percent of the entry fees the house keeps (coins that leave the economy); the rest is prizes. Tournaments opened from now on.",
    ),
    def(CUP_ENABLED, "weekly_cup", Kind::Bool, "The server organises the weekly cup."),
    def(CUP_NAME, "weekly_cup", Kind::Text { max: 32 }, "Its name (\"Saturday Cup\")."),
    def(CUP_WEEKDAY, "weekly_cup", Kind::Int { min: 0, max: 6 }, "Its day: 0 Sunday … 6 Saturday."),
    def(CUP_TIME, "weekly_cup", Kind::Time, "When it starts, local time (HH:MM)."),
    def(
        CUP_UTC_OFFSET,
        "weekly_cup",
        Kind::Int { min: -720, max: 840 },
        "The local time's offset from UTC, in minutes (Brasília: -180).",
    ),
    def(CUP_RULES, "weekly_cup", Kind::Choice(RULES), "The rules it is played under."),
    def(
        CUP_FORMAT,
        "weekly_cup",
        Kind::Choice(FORMATS),
        "Arena (points until time is up) or knockout.",
    ),
    def(
        CUP_MINUTES,
        "weekly_cup",
        Kind::Int { min: 10, max: 180 },
        "How long an arena lasts (minutes).",
    ),
    def(CUP_SIZE, "weekly_cup", Kind::Choice(SIZES), "A knockout's seats."),
    def(CUP_FEE, "weekly_cup", Kind::Int { min: 0, max: 1000 }, "Coins to enter (0: free)."),
    def(
        CUP_PRIZE,
        "weekly_cup",
        Kind::Int { min: 0, max: 100_000 },
        "Coins the house adds to the prizes.",
    ),
    def(
        NAME_CHANGE_PRICE,
        "economy",
        Kind::Int { min: 0, max: 100_000 },
        "Coins a username change costs once the free ones are used (0: no more changes).",
    ),
    def(
        DAILY_BASE,
        "economy",
        Kind::Int { min: 0, max: 1000 },
        "The daily bonus's coins on the first day.",
    ),
    def(
        DAILY_STEP,
        "economy",
        Kind::Int { min: 0, max: 1000 },
        "Coins more each day in a row (the streak ends on a day missed).",
    ),
    def(
        DAILY_DAYS,
        "economy",
        Kind::Int { min: 1, max: 60 },
        "The day of the streak whose bonus is the highest (it stays there after).",
    ),
    def(
        VIP_MONTH_ITEM,
        "economy",
        Kind::Item,
        "VIP's item of this month, instead of the catalogue's vip_month_YYYY_MM (empty: the catalogue's).",
    ),
];

pub fn def_of(key: &str) -> Option<&'static Def> {
    DEFS.iter().find(|d| d.key == key)
}

/// Where a value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Default,
    /// An environment variable gave it.
    Env,
    /// Set in the portal.
    Portal,
}

impl Source {
    fn code(self) -> &'static str {
        match self {
            Source::Default => "default",
            Source::Env => "env",
            Source::Portal => "portal",
        }
    }
}

/// The settings as they are now.
pub struct Live {
    /// The values without the portal's (from the environment, or built in).
    defaults: BTreeMap<&'static str, (Value, Source)>,
    /// What the portal set (read from the database).
    set: RwLock<BTreeMap<String, SettingRow>>,
}

/// The daily bonus's numbers unless set: 5 coins, 5 more a day, a week.
pub const DAILY_BONUS_STEP: i64 = 5;
pub const DAILY_STREAK_DAYS: i64 = 7;
/// A username change once the free ones are used, unless set.
pub const NAME_CHANGE_COINS: i64 = 1500;
/// Connection limits unless set (see `lib.rs`).
pub const MESSAGES_PER_SECOND: i64 = 40;
pub const MESSAGES_AT_ONCE: i64 = 120;

impl Live {
    /// The defaults from the server's configuration (the environment).
    pub fn new(config: &Config) -> Live {
        let built_in = Settings::default();
        let s = &config.settings;
        let mut d = BTreeMap::new();
        let mut put = |k: &'static str, v: Value, env: bool| {
            d.insert(k, (v, if env { Source::Env } else { Source::Default }));
        };
        put(SHOT_CLOCK, json!(s.shot_clock.as_secs()), s.shot_clock != built_in.shot_clock);
        put(MOTD, json!(s.motd), s.motd != built_in.motd);
        put(RECONNECT_GRACE, json!(s.reconnect_grace.as_secs()), false);
        put(CHAT_HISTORY, json!(s.chat_history), false);
        put(SEEK_LIFETIME, json!(s.seek_lifetime.as_secs()), false);
        put(MAX_CONNECTIONS, json!(s.max_connections_per_ip), false);
        put(MESSAGES_PER_SEC, json!(MESSAGES_PER_SECOND), false);
        put(MESSAGE_BURST, json!(MESSAGES_AT_ONCE), false);
        put(MAX_OPEN_TOURNAMENTS, json!(s.max_open_tournaments), false);
        put(NO_SHOW, json!(s.tournament_no_show.as_secs()), false);
        put(HOUSE_CUT, json!(s.house_cut), false);
        let env = s.cup.is_some();
        let cup = s.cup.clone().unwrap_or_else(|| Cup::parse("sat 20:00").expect("a cup"));
        put(CUP_ENABLED, json!(env), env);
        put(CUP_NAME, json!(cup.name), env);
        put(CUP_WEEKDAY, json!(cup.weekday), env);
        put(CUP_TIME, json!(format!("{:02}:{:02}", cup.hour, cup.minute)), env);
        put(CUP_UTC_OFFSET, json!(cup.utc_offset), env);
        put(CUP_RULES, json!(cup.rules.code()), env);
        put(CUP_FORMAT, json!(format_code(cup.format)), env);
        put(CUP_MINUTES, json!(cup.minutes), env);
        put(CUP_SIZE, json!(cup.size.to_string()), env);
        put(CUP_FEE, json!(cup.entry_fee), env);
        put(CUP_PRIZE, json!(cup.house_prize), env);
        put(NAME_CHANGE_PRICE, json!(NAME_CHANGE_COINS), false);
        put(DAILY_BASE, json!(crate::account::DAILY_BONUS), false);
        put(DAILY_STEP, json!(DAILY_BONUS_STEP), false);
        put(DAILY_DAYS, json!(DAILY_STREAK_DAYS), false);
        put(VIP_MONTH_ITEM, json!(""), false);
        Live { defaults: d, set: RwLock::new(BTreeMap::new()) }
    }

    /// The value of `key` now (the portal's, else the default).
    pub fn get(&self, key: &str) -> Value {
        let set = self.set.read().unwrap_or_else(|e| e.into_inner());
        let from_portal = set.get(key).and_then(|r| serde_json::from_str::<Value>(&r.value).ok());
        // A value kept by an older server that no longer fits is left out.
        let valid = from_portal.and_then(|v| def_of(key).and_then(|d| d.kind.check(&v).ok()));
        valid.unwrap_or_else(|| self.defaults.get(key).map(|(v, _)| v.clone()).unwrap_or_default())
    }

    pub fn int(&self, key: &str) -> i64 {
        let v = self.get(key);
        v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0)
    }

    pub fn text(&self, key: &str) -> String {
        self.get(key).as_str().unwrap_or_default().to_string()
    }

    pub fn flag(&self, key: &str) -> bool {
        self.get(key).as_bool().unwrap_or(false)
    }

    fn replace(&self, rows: Vec<SettingRow>) {
        let map = rows.into_iter().map(|r| (r.key.clone(), r)).collect();
        *self.set.write().unwrap_or_else(|e| e.into_inner()) = map;
    }

    /// Every setting as the portal shows it.
    pub fn list(&self) -> Vec<Value> {
        let set = self.set.read().unwrap_or_else(|e| e.into_inner()).clone();
        DEFS.iter()
            .map(|d| {
                let (default, env) =
                    self.defaults.get(d.key).cloned().unwrap_or((Value::Null, Source::Default));
                let portal = set.get(d.key);
                let source = if portal.is_some() { Source::Portal } else { env };
                json!({
                    "key": d.key,
                    "group": d.group,
                    "type": d.kind.code(),
                    "value": self.get(d.key),
                    "default": default,
                    "source": source.code(),
                    "description": d.description,
                    "range": d.kind.range(),
                    "updated_by": portal.map(|r| r.updated_by.clone()),
                    "updated_at": portal.map(|r| r.updated_at),
                })
            })
            .collect()
    }

    /// The weekly cup as set now (None: not organised).
    pub fn cup(&self) -> Option<Cup> {
        if !self.flag(CUP_ENABLED) {
            return None;
        }
        let (hour, minute) = parse_time(&self.text(CUP_TIME)).unwrap_or((20, 0));
        Some(Cup {
            name: self.text(CUP_NAME),
            weekday: self.int(CUP_WEEKDAY).clamp(0, 6) as u32,
            hour,
            minute,
            utc_offset: self.int(CUP_UTC_OFFSET) as i32,
            minutes: self.int(CUP_MINUTES).clamp(10, 180) as u32,
            entry_fee: self.int(CUP_FEE).max(0) as u32,
            rules: Rules::parse(&self.text(CUP_RULES)).unwrap_or_default(),
            format: if self.text(CUP_FORMAT) == "knockout" {
                TournamentFormat::Knockout
            } else {
                TournamentFormat::Arena
            },
            size: self.int(CUP_SIZE).clamp(4, 32) as u32,
            house_prize: self.int(CUP_PRIZE).max(0) as u32,
        })
    }
}

fn format_code(f: TournamentFormat) -> &'static str {
    match f {
        TournamentFormat::Arena => "arena",
        TournamentFormat::Knockout => "knockout",
    }
}

/// Reads the portal's settings again and applies them (at start-up, and
/// after every change).
pub async fn reload(server: &Server) {
    match server.store.live_settings().await {
        Ok(rows) => server.ops.settings.replace(rows),
        Err(e) => {
            tracing::warn!("settings: {e}");
            return;
        }
    }
    apply(server);
}

/// Puts the settings in force: the lounge's at once (a shot clock running
/// keeps its time; the next one has the new length).
pub fn apply(server: &Server) {
    let live = &server.ops.settings;
    let secs = |k: &str| Duration::from_secs(live.int(k).max(0) as u64);
    let motd = live.text(MOTD);
    let cup = live.cup();
    server.with(|l| {
        let s = &mut l.settings;
        s.shot_clock = secs(SHOT_CLOCK);
        s.reconnect_grace = secs(RECONNECT_GRACE);
        s.tournament_no_show = secs(NO_SHOW);
        s.seek_lifetime = secs(SEEK_LIFETIME);
        s.chat_history = live.int(CHAT_HISTORY).max(1) as usize;
        s.max_open_tournaments = live.int(MAX_OPEN_TOURNAMENTS).max(1) as usize;
        s.max_connections_per_ip = live.int(MAX_CONNECTIONS).max(1) as usize;
        s.house_cut = live.int(HOUSE_CUT).clamp(0, 100) as u32;
        // The weekly cup is the server's own: a LAN game has none.
        if !s.lan {
            s.cup = cup;
        }
        l.set_motd(&motd);
    });
}

/// Sets `key` from the portal (`None`: back to its default). Returns the
/// value now in force, or why it can't be set.
pub async fn set(
    server: &Server,
    key: &str,
    value: Option<&Value>,
    by: i64,
) -> Result<Value, String> {
    let d = def_of(key).ok_or("No such setting.")?;
    let stored = match value {
        Some(v) => Some(d.kind.check(v)?.to_string()),
        None => None,
    };
    server.store.set_live_setting(key, stored.as_deref(), by, crate::now()).await.map_err(|e| {
        tracing::error!("setting {key}: {e}");
        "Database error.".to_string()
    })?;
    reload(server).await;
    Ok(server.ops.settings.get(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_checked() {
        let int = Kind::Int { min: 10, max: 120 };
        assert_eq!(int.check(&json!(30)), Ok(json!(30)));
        assert_eq!(int.check(&json!("45")), Ok(json!(45)));
        assert!(int.check(&json!(5)).is_err());
        assert!(int.check(&json!("x")).is_err());
        assert_eq!(Kind::Time.check(&json!("09:30")), Ok(json!("09:30")));
        assert!(Kind::Time.check(&json!("24:00")).is_err());
        assert_eq!(Kind::Choice(SIZES).check(&json!(16)), Ok(json!("16")));
        assert!(Kind::Choice(RULES).check(&json!("snooker")).is_err());
        assert_eq!(Kind::Text { max: 5 }.check(&json!("  hello world")), Ok(json!("hello")));
        assert!(Kind::Item.check(&json!("cue_nothing")).is_err());
        assert_eq!(Kind::Item.check(&json!("")), Ok(json!("")));
        assert!(Kind::Bool.check(&json!(1)).is_err());
    }

    #[test]
    fn every_setting_has_a_valid_default() {
        let mut config = Config::default();
        config.settings.cup = Cup::parse("sun 18:30 official fee=50 utc-3");
        let live = Live::new(&config);
        for d in DEFS {
            let v = live.get(d.key);
            assert!(d.kind.check(&v).is_ok(), "{}: {v}", d.key);
        }
        let cup = live.cup().expect("from the environment");
        assert_eq!((cup.weekday, cup.hour, cup.minute, cup.entry_fee), (0, 18, 30, 50));
        assert_eq!(cup.rules, Rules::Official);
        assert_eq!(Some(cup), config.settings.cup, "the same cup as configured");
        assert!(Live::new(&Config::default()).cup().is_none(), "no cup unless asked");
    }
}
