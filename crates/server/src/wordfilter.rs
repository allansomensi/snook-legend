//! Words the chat doesn't allow (the staff keep the list per language, in
//! the staff portal): a line, a status, a note on the board, a present's
//! message or a new username with one of them is turned away, and the
//! player is told which word.
//!
//! How a text is matched, and why:
//! - Each text is cut into words (anything that isn't a letter or a digit
//!   separates them), and only whole words match: "class" never matches
//!   "ass", nor "Scunthorpe" anything. The price: words glued together
//!   ("fuckyou") get through, and the staff can add those forms.
//! - Before comparing, both sides are brought to a plain form: lower case,
//!   without accents ("otário" is "otario"), leetspeak read as letters
//!   (0→o, 1→i, 3→e, 4→a, 5→s, 7→t, @→a, $→s), and a letter repeated
//!   counts once ("puuuta", "fuuuck"). A plural ("s", "es") still matches.
//! - Letters spaced out ("p.u.t.a", "f u c k", "pu-ta"): runs of words of
//!   one or two letters are also read together. Two short words that
//!   happen to spell a listed one are refused too (rare, and the player can
//!   write them otherwise).
//! - A listed phrase ("filho da puta") matches its words in a row.
//! - Every list applies to every text: the server doesn't know which
//!   language a line is in. The languages keep the lists apart for the
//!   staff; a word innocent in another language belongs on no list.

use std::collections::BTreeSet;

use snook_legend_proto::moderation::TextRefusal;

/// The starting list (the same as migration 0027's): insults and slurs
/// in Portuguese, English and Spanish. The staff add and remove words.
pub const SEED: &[(&str, &str)] = &[
    ("puta", "pt"),
    ("puto", "pt"),
    ("caralho", "pt"),
    ("porra", "pt"),
    ("foda", "pt"),
    ("fodase", "pt"),
    ("foder", "pt"),
    ("fdp", "pt"),
    ("arrombado", "pt"),
    ("arrombada", "pt"),
    ("viado", "pt"),
    ("sapatao", "pt"),
    ("traveco", "pt"),
    ("buceta", "pt"),
    ("cuzao", "pt"),
    ("otario", "pt"),
    ("vagabunda", "pt"),
    ("vadia", "pt"),
    ("crioulo", "pt"),
    ("retardado", "pt"),
    ("mongoloide", "pt"),
    ("corno", "pt"),
    ("babaca", "pt"),
    ("pau no cu", "pt"),
    ("vai se foder", "pt"),
    ("filho da puta", "pt"),
    ("fuck", "en"),
    ("fucker", "en"),
    ("fucking", "en"),
    ("motherfucker", "en"),
    ("shit", "en"),
    ("bitch", "en"),
    ("cunt", "en"),
    ("asshole", "en"),
    ("bastard", "en"),
    ("whore", "en"),
    ("slut", "en"),
    ("nigger", "en"),
    ("nigga", "en"),
    ("faggot", "en"),
    ("retard", "en"),
    ("tranny", "en"),
    ("kys", "en"),
    ("kill yourself", "en"),
    ("puta", "es"),
    ("puto", "es"),
    ("cabron", "es"),
    ("cabrona", "es"),
    ("pendejo", "es"),
    ("pendeja", "es"),
    ("gilipollas", "es"),
    ("maricon", "es"),
    ("marica", "es"),
    ("joder", "es"),
    ("mierda", "es"),
    ("zorra", "es"),
    ("chingada", "es"),
    ("chinga tu madre", "es"),
    ("hijo de puta", "es"),
    ("culero", "es"),
    ("verga", "es"),
    ("subnormal", "es"),
];

/// The longest word or phrase the staff may list.
pub const MAX_WORD: usize = 40;

/// A language code the list takes: two or three letters (`pt`, `fil`), a
/// region (`pt-BR`, `zh-CN`), or `*` for every language.
pub fn valid_lang(lang: &str) -> bool {
    if lang == "*" {
        return true;
    }
    let mut parts = lang.split('-');
    let base = parts.next().unwrap_or("");
    let region = parts.next();
    parts.next().is_none()
        && (2..=3).contains(&base.len())
        && base.chars().all(|c| c.is_ascii_lowercase())
        && region.is_none_or(|r| r.len() == 2 && r.chars().all(|c| c.is_ascii_uppercase()))
}

/// The listed words, ready to match.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    entries: Vec<Entry>,
}

#[derive(Debug, Clone)]
struct Entry {
    /// As listed (what the player and the staff are shown).
    word: String,
    lang: String,
    /// Its words in the plain form.
    tokens: Vec<String>,
}

/// A listed word found in a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub word: String,
    pub lang: String,
}

impl Hit {
    pub fn refusal(&self) -> TextRefusal {
        TextRefusal::BannedWord { word: self.word.clone() }
    }
}

impl Filter {
    /// A filter of `words` (word, language).
    pub fn new<'a>(words: impl IntoIterator<Item = (&'a str, &'a str)>) -> Filter {
        let mut seen = BTreeSet::new();
        let entries = words
            .into_iter()
            .filter_map(|(word, lang)| {
                let tokens = tokens(word);
                (!tokens.is_empty() && seen.insert((tokens.clone(), lang.to_string()))).then(|| {
                    Entry { word: word.trim().to_string(), lang: lang.to_string(), tokens }
                })
            })
            .collect();
        Filter { entries }
    }

    /// The starting list.
    pub fn seed() -> Filter {
        Filter::new(SEED.iter().copied())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The first listed word `text` has, if any.
    pub fn check(&self, text: &str) -> Option<Hit> {
        if self.entries.is_empty() {
            return None;
        }
        let words = tokens(text);
        let joined = joined_short(&words);
        for list in [&words, &joined] {
            for start in 0..list.len() {
                for e in &self.entries {
                    if matches_at(list, start, &e.tokens) {
                        return Some(Hit { word: e.word.clone(), lang: e.lang.clone() });
                    }
                }
            }
        }
        None
    }

    /// The first listed word a username has: its parts between underscores
    /// and digits, and the whole name ("xX_fuck_Xx", "Fuck99").
    pub fn check_name(&self, name: &str) -> Option<Hit> {
        let spaced: String =
            name.chars().map(|c| if c == '_' || c.is_ascii_digit() { ' ' } else { c }).collect();
        self.check(&spaced).or_else(|| self.check(name))
    }
}

/// Whether the listed `word` (its tokens) starts at `list[start]`: a single
/// word also as a plural.
fn matches_at(list: &[String], start: usize, word: &[String]) -> bool {
    if start + word.len() > list.len() {
        return false;
    }
    word.iter().enumerate().all(|(k, w)| {
        let t = &list[start + k];
        t == w
            || (k == word.len() - 1
                && (t.strip_suffix('s').is_some_and(|b| b == w)
                    || t.strip_suffix("es").is_some_and(|b| b == w)))
    })
}

/// Runs of words of one or two letters read together ("p.u.t.a" is
/// "puta"); a run of one is left out (it is in the words already).
fn joined_short(words: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut run = String::new();
    let mut n = 0;
    for w in words.iter().map(String::as_str).chain(std::iter::once("too long to join")) {
        if w.chars().count() <= 2 {
            run.push_str(w);
            n += 1;
        } else {
            if n > 1 {
                out.push(collapse(&run));
            }
            run.clear();
            n = 0;
        }
    }
    out
}

/// The words of `text` in the plain form (see the module's notes).
pub fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    for c in text.chars() {
        match plain(c) {
            Some(p) => word.push(p),
            None => {
                if !word.is_empty() {
                    out.push(collapse(&word));
                    word.clear();
                }
            }
        }
    }
    if !word.is_empty() {
        out.push(collapse(&word));
    }
    out
}

/// A letter repeated counts once.
fn collapse(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    let mut last = None;
    for c in word.chars() {
        if last != Some(c) {
            out.push(c);
        }
        last = Some(c);
    }
    out
}

/// A character in the plain form: lower case, no accent, leetspeak as the
/// letter it stands for; None for a separator.
fn plain(c: char) -> Option<char> {
    let c = match c {
        '0' => 'o',
        '1' => 'i',
        '3' => 'e',
        '4' | '@' => 'a',
        '5' | '$' => 's',
        '7' => 't',
        _ => c,
    };
    if !c.is_alphanumeric() {
        return None;
    }
    let lower = c.to_lowercase().next().unwrap_or(c);
    Some(match lower {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' => 'i',
        'ñ' | 'ń' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' => 'u',
        'ý' | 'ÿ' => 'y',
        'ś' | 'š' => 's',
        'ź' | 'ż' | 'ž' => 'z',
        other => other,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(f: &Filter, text: &str) -> Option<String> {
        f.check(text).map(|h| h.word)
    }

    #[test]
    fn whole_words_only() {
        let f = Filter::new([("ass", "en"), ("puta", "pt")]);
        assert_eq!(hit(&f, "what an ass"), Some("ass".into()));
        assert_eq!(hit(&f, "first class pass"), None);
        assert_eq!(hit(&f, "Computador"), None);
        assert_eq!(hit(&f, "reputação"), None);
        assert_eq!(hit(&f, "disputa boa"), None);
    }

    #[test]
    fn simple_evasions_are_caught() {
        let f = Filter::seed();
        for text in [
            "PUTA",
            "seu puuuuta",
            "p.u.t.a",
            "p u t a",
            "pu-ta",
            "pût@",
            "otário",
            "f*ck you",
            "fuuuck",
            "5h1t",
            "sh1t happens",
            "c a b r ó n",
            "FILHO   DA   PUTA",
            "filho-da-puta",
            "putas",
            "bitches",
        ] {
            // "f*ck" is not a word of the list (an asterisk hides a letter).
            if text == "f*ck you" {
                assert_eq!(hit(&f, text), None, "{text}");
                continue;
            }
            assert!(hit(&f, text).is_some(), "{text}");
        }
    }

    #[test]
    fn innocent_words_pass() {
        let f = Filter::seed();
        for text in [
            "Boa partida, parabéns!",
            "computer science",
            "Scunthorpe United",
            "que jogada! disputa acirrada",
            "a shitake mushroom",
            "assassin's creed",
            "pass the cue",
            "cocktail",
            "@ana good game",
            "1 2 3 4 5",
            "ok ok",
            "matchday",
            "vou fodar",
        ] {
            assert_eq!(hit(&f, text), None, "{text}");
        }
    }

    #[test]
    fn phrases_match_in_a_row() {
        let f = Filter::new([("kill yourself", "en")]);
        assert!(hit(&f, "just kill yourself").is_some());
        assert!(hit(&f, "kill.your.self").is_none(), "three words, not two");
        assert!(hit(&f, "yourself kill").is_none());
    }

    #[test]
    fn usernames_by_their_parts() {
        let f = Filter::seed();
        assert!(f.check_name("xX_fuck_Xx").is_some());
        assert!(f.check_name("Fuck99").is_some());
        assert!(f.check_name("shitake_lover").is_none());
        assert!(f.check_name("Ana_1990").is_none());
    }

    #[test]
    fn languages() {
        assert!(valid_lang("pt") && valid_lang("fil") && valid_lang("*"));
        assert!(valid_lang("pt-BR") && valid_lang("zh-CN"));
        assert!(!valid_lang("PT") && !valid_lang("p") && !valid_lang("pt-br"));
        assert!(!valid_lang("") && !valid_lang("pt-BR-x"));
    }
}
