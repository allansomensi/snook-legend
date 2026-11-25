//! Interface languages: English (the source text) and 26 translations,
//! the world's most spoken languages and more.
//!
//! Texts are written in English in the code and looked up with [`t`]; texts
//! with values use named placeholders, filled in by [`tw`]. The translations
//! live in one file per language (`lang/*.txt`: the English text, a tab, the
//! translation), so adding a language is adding a file and a line to
//! [`Lang`]. The fonts for every script are in [`crate::fonts`].

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use snook_legend_core::{
    BallId, BreakChoice, BreakOption, Difficulty, Foul, GameOverReason, Group, PocketId, Rules,
    ShotError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    English,
    Spanish,
    Portuguese,
    EuropeanPortuguese,
    French,
    German,
    Italian,
    Dutch,
    Swedish,
    Polish,
    Czech,
    Hungarian,
    Romanian,
    Greek,
    Russian,
    Ukrainian,
    Turkish,
    Indonesian,
    Filipino,
    Vietnamese,
    Thai,
    Hindi,
    Bengali,
    Arabic,
    Chinese,
    Japanese,
    Korean,
}

impl Lang {
    /// Every language, in the order the settings list them.
    pub const ALL: [Lang; 27] = [
        Lang::English,
        Lang::Spanish,
        Lang::Portuguese,
        Lang::EuropeanPortuguese,
        Lang::French,
        Lang::German,
        Lang::Italian,
        Lang::Dutch,
        Lang::Swedish,
        Lang::Polish,
        Lang::Czech,
        Lang::Hungarian,
        Lang::Romanian,
        Lang::Greek,
        Lang::Russian,
        Lang::Ukrainian,
        Lang::Turkish,
        Lang::Indonesian,
        Lang::Filipino,
        Lang::Vietnamese,
        Lang::Thai,
        Lang::Hindi,
        Lang::Bengali,
        Lang::Arabic,
        Lang::Chinese,
        Lang::Japanese,
        Lang::Korean,
    ];

    /// The language's own name, as shown in the settings.
    pub fn name(self) -> &'static str {
        match self {
            Lang::English => "English",
            Lang::Spanish => "Español",
            Lang::Portuguese => "Português (Brasil)",
            Lang::EuropeanPortuguese => "Português (Portugal)",
            Lang::French => "Français",
            Lang::German => "Deutsch",
            Lang::Italian => "Italiano",
            Lang::Dutch => "Nederlands",
            Lang::Swedish => "Svenska",
            Lang::Polish => "Polski",
            Lang::Czech => "Čeština",
            Lang::Hungarian => "Magyar",
            Lang::Romanian => "Română",
            Lang::Greek => "Ελληνικά",
            Lang::Russian => "Русский",
            Lang::Ukrainian => "Українська",
            Lang::Turkish => "Türkçe",
            Lang::Indonesian => "Bahasa Indonesia",
            Lang::Filipino => "Filipino",
            Lang::Vietnamese => "Tiếng Việt",
            Lang::Thai => "ไทย",
            Lang::Hindi => "हिन्दी",
            Lang::Bengali => "বাংলা",
            Lang::Arabic => "العربية",
            Lang::Chinese => "简体中文",
            Lang::Japanese => "日本語",
            Lang::Korean => "한국어",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::English => "en",
            Lang::Spanish => "es",
            Lang::Portuguese => "pt-BR",
            Lang::EuropeanPortuguese => "pt-PT",
            Lang::French => "fr",
            Lang::German => "de",
            Lang::Italian => "it",
            Lang::Dutch => "nl",
            Lang::Swedish => "sv",
            Lang::Polish => "pl",
            Lang::Czech => "cs",
            Lang::Hungarian => "hu",
            Lang::Romanian => "ro",
            Lang::Greek => "el",
            Lang::Russian => "ru",
            Lang::Ukrainian => "uk",
            Lang::Turkish => "tr",
            Lang::Indonesian => "id",
            Lang::Filipino => "fil",
            Lang::Vietnamese => "vi",
            Lang::Thai => "th",
            Lang::Hindi => "hi",
            Lang::Bengali => "bn",
            Lang::Arabic => "ar",
            Lang::Chinese => "zh-CN",
            Lang::Japanese => "ja",
            Lang::Korean => "ko",
        }
    }

    /// The language of a locale or a code ("pt_BR.UTF-8", "de-AT", "zh-Hans-CN").
    pub fn parse(s: &str) -> Option<Lang> {
        let s = s.trim().to_ascii_lowercase().replace('_', "-");
        let mut parts = s.split(['-', '.', '@']);
        let primary = parts.next()?;
        let region = parts.next().unwrap_or("");
        Some(match primary {
            "en" => Lang::English,
            "es" => Lang::Spanish,
            // Portuguese is Brazil's but in Portugal and the countries that
            // write as Portugal does (Angola, Mozambique, …).
            "pt" if region.is_empty() || region == "br" => Lang::Portuguese,
            "pt" => Lang::EuropeanPortuguese,
            "fr" => Lang::French,
            // German, also of Austria and Switzerland.
            "de" | "gsw" => Lang::German,
            "it" => Lang::Italian,
            "nl" => Lang::Dutch,
            "sv" => Lang::Swedish,
            "pl" => Lang::Polish,
            "cs" => Lang::Czech,
            "hu" => Lang::Hungarian,
            "ro" => Lang::Romanian,
            "el" => Lang::Greek,
            "ru" => Lang::Russian,
            "uk" => Lang::Ukrainian,
            "tr" => Lang::Turkish,
            // "in": Indonesian's old code, still used by some systems.
            "id" | "in" => Lang::Indonesian,
            "fil" | "tl" => Lang::Filipino,
            "vi" => Lang::Vietnamese,
            "th" => Lang::Thai,
            "hi" => Lang::Hindi,
            "bn" => Lang::Bengali,
            "ar" => Lang::Arabic,
            "zh" => Lang::Chinese,
            "ja" => Lang::Japanese,
            "ko" => Lang::Korean,
            _ => return None,
        })
    }

    /// The language of the operating system, if we have a translation for it.
    pub fn system() -> Lang {
        sys_locale::get_locale().and_then(|l| Lang::parse(&l)).unwrap_or(Lang::English)
    }

    /// The languages with a translation file (all but English).
    #[cfg(test)]
    pub fn translated() -> impl Iterator<Item = Lang> {
        Lang::ALL.into_iter().filter(|l| *l != Lang::English)
    }

    /// Written right to left.
    pub fn rtl(self) -> bool {
        self == Lang::Arabic
    }

    /// A date in the way the language writes it.
    pub fn date(self, y: i64, m: i64, d: i64) -> String {
        use Lang::*;
        match self {
            English | Swedish => format!("{y}-{m:02}-{d:02}"),
            Chinese | Japanese => format!("{y}/{m:02}/{d:02}"),
            Korean | Hungarian => format!("{y}. {m:02}. {d:02}."),
            German | Russian | Ukrainian | Polish | Czech | Romanian | Turkish => {
                format!("{d:02}.{m:02}.{y}")
            }
            Dutch => format!("{d:02}-{m:02}-{y}"),
            Filipino => format!("{m:02}/{d:02}/{y}"),
            _ => format!("{d:02}/{m:02}/{y}"),
        }
    }

    /// The translation file (none for English, the source).
    fn source(self) -> Option<&'static str> {
        match self {
            Lang::English => None,
            Lang::Spanish => Some(include_str!("../lang/es.txt")),
            Lang::Portuguese => Some(include_str!("../lang/pt-BR.txt")),
            Lang::EuropeanPortuguese => Some(include_str!("../lang/pt-PT.txt")),
            Lang::French => Some(include_str!("../lang/fr.txt")),
            Lang::German => Some(include_str!("../lang/de.txt")),
            Lang::Italian => Some(include_str!("../lang/it.txt")),
            Lang::Dutch => Some(include_str!("../lang/nl.txt")),
            Lang::Swedish => Some(include_str!("../lang/sv.txt")),
            Lang::Polish => Some(include_str!("../lang/pl.txt")),
            Lang::Czech => Some(include_str!("../lang/cs.txt")),
            Lang::Hungarian => Some(include_str!("../lang/hu.txt")),
            Lang::Romanian => Some(include_str!("../lang/ro.txt")),
            Lang::Greek => Some(include_str!("../lang/el.txt")),
            Lang::Russian => Some(include_str!("../lang/ru.txt")),
            Lang::Ukrainian => Some(include_str!("../lang/uk.txt")),
            Lang::Turkish => Some(include_str!("../lang/tr.txt")),
            Lang::Indonesian => Some(include_str!("../lang/id.txt")),
            Lang::Filipino => Some(include_str!("../lang/fil.txt")),
            Lang::Vietnamese => Some(include_str!("../lang/vi.txt")),
            Lang::Thai => Some(include_str!("../lang/th.txt")),
            Lang::Hindi => Some(include_str!("../lang/hi.txt")),
            Lang::Bengali => Some(include_str!("../lang/bn.txt")),
            Lang::Arabic => Some(include_str!("../lang/ar.txt")),
            Lang::Chinese => Some(include_str!("../lang/zh-CN.txt")),
            Lang::Japanese => Some(include_str!("../lang/ja.txt")),
            Lang::Korean => Some(include_str!("../lang/ko.txt")),
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

/// Tests that switch the language hold this while they do: it is the whole
/// game's, and tests run side by side.
#[cfg(test)]
pub fn lang_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn lang() -> Lang {
    Lang::ALL
        .into_iter()
        .find(|l| *l as u8 == LANG.load(Ordering::Relaxed))
        .unwrap_or(Lang::English)
}

type Table = HashMap<&'static str, &'static str>;

/// Reads a translation file: `English<TAB>translation` per line, `#` for
/// comments, `\n` and `\t` for a new line and a tab. A comment has no tab,
/// so texts may start with `#` ("#{n} in the world").
fn parse_table(text: &'static str) -> Table {
    fn unescape(s: &'static str) -> &'static str {
        if !s.contains('\\') {
            return s;
        }
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(other) => out.push(other),
                    None => {}
                }
            } else {
                out.push(c);
            }
        }
        // Once per text with an escape, for the life of the game.
        Box::leak(out.into_boxed_str())
    }
    text.lines()
        .filter(|l| !l.trim().is_empty() && !(l.starts_with('#') && !l.contains('\t')))
        .filter_map(|l| l.split_once('\t'))
        .map(|(en, tr)| (unescape(en), unescape(tr)))
        .collect()
}

fn table(lang: Lang) -> Option<&'static Table> {
    static TABLES: [OnceLock<Table>; Lang::ALL.len()] =
        [const { OnceLock::new() }; Lang::ALL.len()];
    let source = lang.source()?;
    Some(TABLES[lang as usize].get_or_init(|| parse_table(source)))
}

fn translate(lang: Lang, en: &str) -> Option<&'static str> {
    table(lang)?.get(en).copied()
}

/// Whether `en` has a translation in `lang` (for tests of texts kept
/// outside the code's `t` calls).
#[cfg(test)]
pub fn has_translation(lang: Lang, en: &str) -> bool {
    translate(lang, en).is_some()
}

/// Translates an interface text (returns it unchanged when there is no translation).
pub fn t(en: &str) -> &str {
    translate(lang(), en).unwrap_or(en)
}

/// Translates a text with `{name}` placeholders and fills them in.
pub fn tw(en: &str, values: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut out = t(en).to_string();
    for (key, value) in values {
        out = out.replace(&format!("{{{key}}}"), &value.to_string());
    }
    out
}

/// Translates a message that came from the server or the network (exact
/// texts, plus a few with a variable part).
pub fn server_text(text: &str) -> String {
    if lang() == Lang::English {
        return text.to_string();
    }
    if let Some(t) = translate(lang(), text) {
        return t.to_string();
    }
    if let Some(rest) = text.strip_prefix("Warning from the staff: ") {
        return tw("Warning from the staff: {text}", &[("text", &rest)]);
    }
    if let Some(rest) = text.strip_prefix("The staff changed your username to ") {
        let name = rest.split(". ").next().unwrap_or(rest).trim_end_matches('.');
        return tw("The staff changed your username to {name}. Sign in again.", &[("name", &name)]);
    }
    if let Some(rest) = text.strip_prefix("The chat was cleared by ") {
        let name = rest.trim_end_matches('.');
        return tw("The chat was cleared by {name}.", &[("name", &name)]);
    }
    if let Some(rest) = text.strip_prefix("LAN game hosted by ") {
        let name = rest.trim_end_matches('.');
        return tw("LAN game hosted by {name}.", &[("name", &name)]);
    }
    if let Some(rest) =
        text.strip_prefix("You left a game in progress: you can play online again in ")
    {
        let n = rest.split_whitespace().next().unwrap_or("?");
        return tw(
            "You left a game in progress: you can play online again in {n} min.",
            &[("n", &n)],
        );
    }
    if let Some(rest) = text.strip_prefix("You are muted for ") {
        let n = rest.split_whitespace().next().unwrap_or("?");
        return tw("You are muted for {n} more minutes.", &[("n", &n)]);
    }
    // A name (or a text an older way refused) with a banned word.
    if let Some(rest) = text.strip_prefix("That word isn't allowed here: ") {
        let word = rest.trim_end_matches('.');
        return tw("That word isn't allowed here: {word}", &[("word", &word)]);
    }
    if let Some(rest) = text.strip_prefix("This account is banned for ") {
        let n = rest.split_whitespace().next().unwrap_or("?");
        let reason = rest.split("Reason: ").nth(1).unwrap_or("").trim_end_matches('.');
        return tw(
            "This account is suspended for {n} more day(s). Reason: {reason}.",
            &[("n", &n), ("reason", &reason)],
        );
    }
    if let Some(rest) = text.strip_prefix("This account is banned. Reason: ") {
        return tw(
            "This account is banned. Reason: {reason}.",
            &[("reason", &rest.trim_end_matches('.'))],
        );
    }
    // "… for 60 minutes: <reason>": the number and the reason apart.
    for (en, unit, key) in [
        ("You were muted for ", " minutes: ", "You were muted for {n} minutes: {r}"),
        (
            "Your account was suspended for ",
            " hours: ",
            "Your account was suspended for {n} hours: {r}",
        ),
    ] {
        if let Some((n, r)) = text.strip_prefix(en).and_then(|rest| rest.split_once(unit))
            && n.parse::<u64>().is_ok()
        {
            return tw(key, &[("n", &n), ("r", &r)]);
        }
    }
    if let Some(n) = text
        .strip_prefix("The server restarts in ")
        .and_then(|rest| rest.split_whitespace().next())
        .filter(|n| n.parse::<u64>().is_ok())
    {
        return tw(
            "The server restarts in {minutes} min for an update. Games in progress go on; new games start right after.",
            &[("minutes", &n)],
        );
    }
    if let Some(cup) = text
        .strip_prefix("The ")
        .and_then(|rest| rest.strip_suffix(" is open for sign-up: it starts soon in Tournaments."))
    {
        return tw(
            "The {cup} is open for sign-up: it starts soon in Tournaments.",
            &[("cup", &tournament_name(cup))],
        );
    }
    if let Some(cup) = text
        .strip_prefix("The ")
        .and_then(|rest| rest.strip_suffix(" was called off: not enough players signed up."))
    {
        return tw(
            "The {cup} was called off: not enough players signed up.",
            &[("cup", &tournament_name(cup))],
        );
    }
    if let Some((who, name)) = text
        .strip_suffix("\u{201d}!")
        .and_then(|rest| rest.split_once(" won the tournament \u{201c}"))
    {
        return tw(
            "{display} won the tournament \u{201c}{name}\u{201d}!",
            &[("display", &who), ("name", &tournament_name(name))],
        );
    }
    for (en, key) in [
        ("You were muted for ", "You were muted: {r}"),
        ("Your account was suspended for ", "Your account was suspended: {r}"),
        ("Your account was banned: ", "Your account was banned: {r}"),
        ("You were disconnected by a moderator: ", "You were disconnected by a moderator: {r}"),
    ] {
        if let Some(rest) = text.strip_prefix(en) {
            return tw(key, &[("r", &rest)]);
        }
    }
    if let Some(rest) = text.strip_prefix("Can't shoot: ") {
        let reason = rest.trim_end_matches('.');
        return tw("Can't shoot: {e}.", &[("e", &t(reason))]);
    }
    text.to_string()
}

/// A tournament's name: the server's weekly cups ("Saturday Cup") in the
/// player's language, any other name as its organiser wrote it.
/// What the shot clock running out does, under `rules`.
pub fn clock_hint(rules: snook_legend_core::Rules) -> &'static str {
    t(match rules {
        snook_legend_core::Rules::Brazilian => {
            "When time runs out, it's a foul: your opponent gets a ball and the cue ball goes back to the head spot."
        }
        snook_legend_core::Rules::Official => {
            "When time runs out, it's a foul: your opponent gets the cue ball in hand."
        }
    })
}

pub fn tournament_name(name: &str) -> String {
    const CUPS: [&str; 7] = [
        "Sunday Cup",
        "Monday Cup",
        "Tuesday Cup",
        "Wednesday Cup",
        "Thursday Cup",
        "Friday Cup",
        "Saturday Cup",
    ];
    match CUPS.iter().find(|c| **c == name) {
        Some(cup) => t(cup).to_string(),
        None => name.to_string(),
    }
}

pub fn difficulty(d: Difficulty) -> &'static str {
    match d {
        Difficulty::Easy => t("Easy"),
        Difficulty::Medium => {
            if lang() == Lang::English {
                "Medium"
            } else {
                t("Normal")
            }
        }
        Difficulty::Hard => t("Hard"),
    }
}

pub fn group(g: Group) -> &'static str {
    t(g.name())
}

pub fn pocket(p: PocketId) -> &'static str {
    t(p.name())
}

pub fn ball(id: BallId) -> String {
    if id == BallId::CUE { t("cue ball").to_string() } else { tw("{n}-ball", &[("n", &id.0)]) }
}

/// How a game ended, naming who did it ("Ana sank the 8 too early").
pub fn result_line(r: GameOverReason, winner: &str, loser: &str) -> String {
    use GameOverReason::*;
    let (key, name) = match r {
        PocketedEight => ("{name} sank the 8-ball", winner),
        EightPocketedEarly => ("{name} sank the 8 too early", loser),
        EightInWrongPocket => ("{name} sank the 8 in the wrong pocket", loser),
        EightPocketedOnFoul => ("{name} fouled on the 8", loser),
        EightOffTable => ("{name} knocked the 8 off the table", loser),
        EightWithAnotherBall => ("{name} sank the 8 together with another ball", loser),
        Resignation => ("{name} resigned", loser),
        Abandoned => ("{name} left the game", loser),
    };
    tw(key, &[("name", &name)])
}

/// How a game in the history ended, from the player's side.
pub fn history_reason(r: GameOverReason, won: bool) -> &'static str {
    use GameOverReason::*;
    t(match (r, won) {
        (PocketedEight, true) => "sank the 8-ball",
        (PocketedEight, false) => "the opponent sank the 8-ball",
        (EightPocketedEarly, true) => "the opponent sank the 8 too early",
        (EightPocketedEarly, false) => "sank the 8 too early",
        (EightInWrongPocket, true) => "the opponent sank the 8 in the wrong pocket",
        (EightInWrongPocket, false) => "sank the 8 in the wrong pocket",
        (EightPocketedOnFoul, true) => "the opponent fouled on the 8",
        (EightPocketedOnFoul, false) => "fouled on the 8",
        (EightOffTable, true) => "the opponent knocked the 8 off the table",
        (EightOffTable, false) => "knocked the 8 off the table",
        (EightWithAnotherBall, true) => "the opponent sank the 8 with another ball",
        (EightWithAnotherBall, false) => "sank the 8 with another ball",
        (Resignation, true) => "the opponent resigned",
        (Resignation, false) => "resigned",
        (Abandoned, true) => "the opponent left",
        (Abandoned, false) => "left the game",
    })
}

pub fn foul(f: &Foul) -> String {
    match f {
        Foul::CueBallPocketed => t("Scratch: the cue ball was pocketed").into(),
        Foul::CueBallOffTable => t("The cue ball left the table").into(),
        Foul::NoContact => t("The cue ball didn't hit any ball").into(),
        Foul::WrongBallFirst(id) => tw("Wrong ball hit first ({ball})", &[("ball", &ball(*id))]),
        Foul::NoRailAfterContact => t("No ball reached a cushion after contact").into(),
        Foul::ObjectBallOffTable(id) => tw("The {ball} left the table", &[("ball", &ball(*id))]),
        Foul::IllegalBreak => t("Illegal break").into(),
        Foul::BallBehindHeadString(id) => tw(
            "The {ball} lay behind the head string and the cue ball hit it without crossing the line first",
            &[("ball", &ball(*id))],
        ),
    }
}

/// The rules' name, as the player picks them ("Brazilian rules").
pub fn rules_name(r: Rules) -> &'static str {
    t(match r {
        Rules::Brazilian => "Brazilian rules",
        Rules::Official => "Official rules",
    })
}

/// The rules' name inside a sentence ("the Brazilian rules", "a regra
/// brasileira", "las reglas oficiales", "die brasilianischen Regeln"): as
/// each language writes it there (the translations' "(inside a sentence)"
/// texts); English keeps the capital of a people's name.
pub fn rules_name_inline(r: Rules) -> String {
    let key = match r {
        Rules::Brazilian => "Brazilian rules (inside a sentence)",
        Rules::Official => "Official rules (inside a sentence)",
    };
    inline(r, rules_name(r), key)
}

/// The rules' short name inside a line ("Brazilian", "brasileira",
/// "official"), as [`rules_name_inline`].
pub fn rules_short_inline(r: Rules) -> String {
    let key = match r {
        Rules::Brazilian => "Brazilian (inside a sentence)",
        Rules::Official => "Official (inside a sentence)",
    };
    inline(r, rules_short(r), key)
}

fn inline(r: Rules, name: &str, key: &str) -> String {
    match lang() {
        Lang::English if r == Rules::Brazilian => name.to_string(),
        Lang::English => name.to_lowercase(),
        other => translate(other, key).map_or_else(|| name.to_lowercase(), str::to_string),
    }
}

/// The rules' short name, where the context says they are rules
/// ("Brazilian", "Official").
pub fn rules_short(r: Rules) -> &'static str {
    t(match r {
        Rules::Brazilian => "Brazilian",
        Rules::Official => "Official",
    })
}

/// A player (or a game) under the rules, after their status ("on the
/// official rules").
pub fn rules_in(r: Rules) -> &'static str {
    t(match r {
        Rules::Brazilian => "on the Brazilian rules",
        Rules::Official => "on the official rules",
    })
}

/// What the rules are in a few words (under their name).
pub fn rules_tagline(r: Rules) -> &'static str {
    t(match r {
        Rules::Brazilian => "As played in Brazil's bars",
        Rules::Official => "As played in tournaments (WPA)",
    })
}

/// What a break that leaves a choice did (the notice's text), and who now
/// decides what (its detail). `chooser` decides; `breaker` broke.
pub fn break_choice(choice: BreakChoice, chooser: &str, breaker: &str) -> (String, String) {
    let n = [("name", &chooser as &dyn std::fmt::Display), ("breaker", &breaker)];
    match choice {
        BreakChoice::IllegalBreak { scratch: false, ball_off: true } => (
            t("Illegal break: no ball made and fewer than four balls reached a cushion.").into(),
            tw(
                "{name} chooses: play the table as it lies, take the cue ball in hand behind the head string, rack the balls and break, or have {breaker} break again.",
                &n,
            ),
        ),
        BreakChoice::IllegalBreak { .. } => (
            t("Illegal break: no ball made and fewer than four balls reached a cushion.").into(),
            tw(
                "{name} chooses: play the table as it lies, rack the balls and break, or have {breaker} break again.",
                &n,
            ),
        ),
        BreakChoice::EightOnBreak => (
            t("The 8 went down on the break: it is spotted.").into(),
            tw("{name} chooses: play on, or rack the balls and break again.", &n),
        ),
        BreakChoice::EightOnFoulBreak => (
            t("The 8 went down on a break with a foul: it is spotted.").into(),
            tw(
                "{name} chooses: the cue ball in hand behind the head string, or rack the balls and break.",
                &n,
            ),
        ),
        BreakChoice::BallOffOnBreak => (
            t("A ball left the table on the break.").into(),
            tw(
                "{name} chooses: play the table as it lies, or take the cue ball in hand behind the head string.",
                &n,
            ),
        ),
    }
}

/// The question a choice asks, for the dialog's title.
pub fn choice_title(choice: BreakChoice) -> &'static str {
    t(match choice {
        BreakChoice::IllegalBreak { .. } => "Illegal break",
        BreakChoice::EightOnBreak => "The 8 on the break",
        BreakChoice::EightOnFoulBreak => "The 8 on a break with a foul",
        BreakChoice::BallOffOnBreak => "A ball off the table on the break",
    })
}

/// One answer to a choice, from the chooser's side: its label and what it
/// does (`other`: the other player).
pub fn break_option(choice: BreakChoice, option: BreakOption, other: &str) -> (String, String) {
    let n = [("name", &other as &dyn std::fmt::Display)];
    match option {
        BreakOption::PlayOn => {
            let what = match choice {
                BreakChoice::IllegalBreak { scratch: true, .. } => {
                    "With the cue ball in hand behind the head string."
                }
                BreakChoice::EightOnBreak => "The 8 is spotted and you keep shooting.",
                BreakChoice::EightOnFoulBreak => {
                    "The 8 is spotted; the cue ball in hand behind the head string."
                }
                _ => "The table as it lies.",
            };
            (t("Play on").into(), t(what).into())
        }
        BreakOption::CueBallInHand => {
            (t("Cue ball in hand").into(), t("Place it anywhere behind the head string.").into())
        }
        BreakOption::BreakAgain => {
            (t("Break yourself").into(), t("The balls are racked again and you break.").into())
        }
        BreakOption::OpponentBreaks => {
            (tw("{name} breaks again", &n), tw("The balls are racked again and {name} breaks.", &n))
        }
    }
}

/// What a player chose, for everyone (`other`: the other player).
pub fn chosen(choice: BreakChoice, option: BreakOption, name: &str, other: &str) -> String {
    let n = [("name", &name as &dyn std::fmt::Display), ("other", &other)];
    match option {
        BreakOption::PlayOn if choice == BreakChoice::EightOnBreak => {
            tw("{name} plays on (the 8 is spotted).", &n)
        }
        BreakOption::PlayOn => tw("{name} plays the table as it lies.", &n),
        BreakOption::CueBallInHand => tw("{name} takes the cue ball in hand.", &n),
        BreakOption::BreakAgain => tw("The balls are racked again: {name} breaks.", &n),
        BreakOption::OpponentBreaks => tw("The balls are racked again: {other} breaks.", &n),
    }
}

pub fn shot_error(e: ShotError) -> String {
    t(&e.to_string()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_and_fills_placeholders() {
        let _lang = lang_lock();
        set_lang(Lang::Portuguese);
        assert_eq!(t("Play"), "Jogar");
        assert_eq!(t("not in the table"), "not in the table");
        assert_eq!(tw("{name} wins!", &[("name", &"Ana")]), "Ana vence!");
        // A text that starts with `#` is not a comment.
        assert_eq!(tw("#{n} in the world", &[("n", &3)]), "#3 no mundo");
        assert_eq!(ball(BallId(8)), "bola 8");
        assert_eq!(
            server_text("Can't shoot: the game is over."),
            "Não dá para tacar: a partida acabou."
        );
        set_lang(Lang::Spanish);
        assert_eq!(t("Play"), "Jugar");
        set_lang(Lang::English);
        assert_eq!(tw("{name} wins!", &[("name", &"Ana")]), "Ana wins!");
    }

    /// A rules' name inside a sentence is in lower case, but for the
    /// capital of a people's name in English.
    #[test]
    fn rules_names_inside_a_sentence() {
        let _lang = lang_lock();
        let names = |l| {
            set_lang(l);
            Rules::ALL.map(|r| (rules_name_inline(r), rules_short_inline(r)))
        };
        assert_eq!(
            names(Lang::English),
            [
                ("Brazilian rules".into(), "Brazilian".into()),
                ("official rules".into(), "official".into())
            ]
        );
        assert_eq!(
            names(Lang::Portuguese),
            [
                ("regra brasileira".into(), "brasileira".into()),
                ("regra oficial".into(), "oficial".into())
            ]
        );
        assert_eq!(
            names(Lang::Spanish),
            [
                ("regla brasileña".into(), "brasileña".into()),
                ("regla oficial".into(), "oficial".into())
            ]
        );
        set_lang(Lang::English);
    }

    /// The server's announcements and moderation notices, with their
    /// numbers and names, read in the player's language.
    #[test]
    fn server_announcements_are_translated() {
        let _lang = lang_lock();
        set_lang(Lang::Portuguese);
        assert_eq!(
            server_text("You were muted for 60 minutes: spam"),
            "Você foi silenciado por 60 minutos: spam"
        );
        assert_eq!(
            server_text("Your account was suspended for 24 hours: cheating"),
            "Sua conta foi suspensa por 24 horas: cheating"
        );
        assert!(server_text("The server restarts in 5 min for an update. Games in progress go on; new games start right after.").starts_with("O servidor reinicia em 5 min"));
        assert_eq!(
            server_text("The Saturday Cup is open for sign-up: it starts soon in Tournaments."),
            "As inscrições para a Copa de Sábado estão abertas: ela começa em breve, em Torneios."
        );
        assert_eq!(
            server_text("The Friday Cup was called off: not enough players signed up."),
            "A Copa de Sexta foi cancelada: não houve inscritos suficientes."
        );
        assert_eq!(
            server_text("Ana won the tournament \u{201c}Friday Cup\u{201d}!"),
            "Ana venceu o torneio \u{201c}Copa de Sexta\u{201d}!"
        );
        assert_eq!(tournament_name("Copa da Ana"), "Copa da Ana");
        // What the staff write comes through as it is.
        assert_eq!(server_text("Maintenance at 3 am."), "Maintenance at 3 am.");
        set_lang(Lang::English);
    }

    #[test]
    fn quick_phrases_are_translated_everywhere() {
        for lang in Lang::translated() {
            for p in snook_legend_proto::QUICK_CHAT {
                assert!(translate(lang, p).is_some(), "{lang:?}: {p}");
            }
        }
    }

    #[test]
    fn drills_are_translated_everywhere() {
        for lang in Lang::translated() {
            for d in snook_legend_proto::drills::DRILLS {
                assert!(translate(lang, d.name).is_some(), "{lang:?}: {}", d.name);
                assert!(translate(lang, d.description).is_some(), "{lang:?}: {}", d.description);
            }
        }
    }

    #[test]
    fn translation_files_are_consistent() {
        let placeholders = |s: &str| {
            let mut v: Vec<String> = s
                .split('{')
                .skip(1)
                .filter_map(|p| p.split_once('}').map(|(k, _)| k.to_string()))
                .collect();
            v.sort();
            v
        };
        let pt = table(Lang::Portuguese).unwrap();
        for lang in Lang::translated() {
            let table = table(lang).unwrap();
            for (en, tr) in table {
                assert_eq!(placeholders(en), placeholders(tr), "{lang:?}: {en} → {tr}");
            }
            // Every language translates the same texts.
            let mut missing: Vec<&&str> = pt.keys().filter(|k| !table.contains_key(*k)).collect();
            missing.sort();
            assert!(missing.is_empty(), "not in {lang:?}: {missing:?}");
            let mut extra: Vec<&&str> = table.keys().filter(|k| !pt.contains_key(*k)).collect();
            extra.sort();
            assert!(extra.is_empty(), "{lang:?} only: {extra:?}");
        }
    }
}
