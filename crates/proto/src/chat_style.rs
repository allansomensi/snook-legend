//! Styles in a chat line, written as plain text (there is no button for
//! them): `**bold**`, `*italic*`, `__underlined__`, `~~struck through~~`,
//! and colours, `[blue]like this[/blue]` (or closed with `[/]`). Only the
//! lines of VIPs, moderators and administrators are styled; anyone else's
//! read exactly as they were written. Red is the staff's alone.
//!
//! Mentions (`@name`) are never styled apart: a name with underscores in
//! it stays whole. A marker with no partner, or a colour the writer may
//! not use, is left as it was typed.

use serde::{Deserialize, Serialize};

/// What the writer of a line may style in it (set by the server, from who
/// they are when they write it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Styles {
    /// Nothing: the text as it was written.
    #[default]
    None,
    /// A VIP: every style and colour but the staff's red.
    Vip,
    /// A moderator or an administrator: red too.
    Staff,
}

impl Styles {
    pub fn is_none(&self) -> bool {
        *self == Styles::None
    }
}

/// The colours a line may be written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    Gold,
    Yellow,
    Orange,
    Pink,
    Purple,
    Blue,
    Cyan,
    Green,
    White,
    Grey,
    /// The staff's.
    Red,
    /// One of its own, by a name of its own (not listed anywhere).
    Secret,
}

impl Ink {
    /// The colour a tag names (in English or Portuguese), if any.
    fn named(name: &str) -> Option<Ink> {
        let name = name.trim().to_lowercase();
        Some(match name.as_str() {
            "gold" | "dourado" => Ink::Gold,
            "yellow" | "amarelo" => Ink::Yellow,
            "orange" | "laranja" => Ink::Orange,
            "pink" | "rosa" => Ink::Pink,
            "purple" | "roxo" => Ink::Purple,
            "blue" | "azul" => Ink::Blue,
            "cyan" | "ciano" => Ink::Cyan,
            "green" | "verde" => Ink::Green,
            "white" | "branco" => Ink::White,
            "grey" | "gray" | "cinza" => Ink::Grey,
            "red" | "vermelho" => Ink::Red,
            _ if SECRET.contains(&fnv(&name)) => Ink::Secret,
            _ => return None,
        })
    }

    /// Whether a writer who may style `styles` may use it.
    fn allowed(self, styles: Styles) -> bool {
        match styles {
            Styles::None => false,
            Styles::Vip => self != Ink::Red,
            Styles::Staff => true,
        }
    }
}

/// The secret colour's names, as their FNV-1a hashes.
const SECRET: [u64; 4] =
    [0xbb1a3eb9e4c6e529, 0x8f3d6074698ca31b, 0x2eeb5b49ff3d884c, 0x86c77fedf2f3d6e3];

fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// How a run of text is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub ink: Option<Ink>,
}

/// A run of the plain text (byte offsets) drawn one way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    pub start: usize,
    pub end: usize,
    pub style: Style,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    Bold,
    Italic,
    Underline,
    Strike,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    /// A marker, and whether it may open (a letter after it) and close (a
    /// letter before it).
    Mark(Mark, bool, bool),
    Open(Ink),
    Close,
    /// Text as it is.
    Text,
}

/// The line's text without its markers, and how each run of it is drawn
/// (none when there is nothing to style). Anyone not allowed: the text as
/// it is.
pub fn parse(text: &str, styles: Styles) -> (String, Vec<Run>) {
    if styles.is_none() {
        return (text.to_string(), Vec::new());
    }
    // The text cut into tokens: markers, tags, and the rest.
    let mut tokens: Vec<(Token, usize, usize)> = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    let space_at = |at: usize| text[at..].chars().next().is_none_or(char::is_whitespace);
    let space_before = |at: usize| text[..at].chars().next_back().is_none_or(char::is_whitespace);
    while i < bytes.len() {
        let rest = &text[i..];
        // A mention, whole.
        if let Some(after) = rest.strip_prefix('@') {
            let name = after.bytes().take_while(|b| b.is_ascii_alphanumeric() || *b == b'_');
            let len = 1 + name.count();
            tokens.push((Token::Text, i, i + len));
            i += len;
            continue;
        }
        let double = [("**", Mark::Bold), ("__", Mark::Underline), ("~~", Mark::Strike)]
            .into_iter()
            .find(|(m, _)| rest.starts_with(m));
        if let Some((m, mark)) = double {
            let end = i + m.len();
            tokens.push((Token::Mark(mark, !space_at(end), !space_before(i)), i, end));
            i = end;
            continue;
        }
        if rest.starts_with('*') {
            tokens.push((Token::Mark(Mark::Italic, !space_at(i + 1), !space_before(i)), i, i + 1));
            i += 1;
            continue;
        }
        if rest.starts_with('[')
            && let Some(close) = rest.find(']')
            && close <= 14
        {
            let inside = &rest[1..close];
            let token = if inside == "/" {
                Some(Token::Close)
            } else if let Some(name) = inside.strip_prefix('/') {
                Ink::named(name).filter(|ink| ink.allowed(styles)).map(|_| Token::Close)
            } else {
                Ink::named(inside).filter(|ink| ink.allowed(styles)).map(Token::Open)
            };
            if let Some(token) = token {
                tokens.push((token, i, i + close + 1));
                i += close + 1;
                continue;
            }
        }
        let len = rest.chars().next().map_or(1, char::len_utf8);
        match tokens.last_mut() {
            Some((Token::Text, _, end)) if *end == i => *end = i + len,
            _ => tokens.push((Token::Text, i, i + len)),
        }
        i += len;
    }
    // Markers in pairs (an opener, then a closer of the same kind); the
    // rest stay as text.
    let mut used = vec![false; tokens.len()];
    for mark in [Mark::Bold, Mark::Italic, Mark::Underline, Mark::Strike] {
        let mut open: Option<usize> = None;
        for (k, (t, _, _)) in tokens.iter().enumerate() {
            let Token::Mark(m, can_open, can_close) = *t else { continue };
            if m != mark {
                continue;
            }
            match open {
                Some(o) if can_close => {
                    used[o] = true;
                    used[k] = true;
                    open = None;
                }
                None if can_open => open = Some(k),
                // (An opener after an opener: it takes its place.)
                Some(_) if can_open => open = Some(k),
                _ => {}
            }
        }
    }
    // The plain text, and the style at each of its runs.
    let mut plain = String::with_capacity(text.len());
    let mut runs: Vec<Run> = Vec::new();
    let mut style = Style::default();
    let mut inks: Vec<Ink> = Vec::new();
    let mut styled = false;
    for (k, &(t, a, b)) in tokens.iter().enumerate() {
        let literal = match t {
            Token::Mark(m, _, _) if used[k] => {
                let flag = match m {
                    Mark::Bold => &mut style.bold,
                    Mark::Italic => &mut style.italic,
                    Mark::Underline => &mut style.underline,
                    Mark::Strike => &mut style.strike,
                };
                *flag = !*flag;
                styled = true;
                false
            }
            Token::Open(ink) => {
                inks.push(ink);
                style.ink = Some(ink);
                styled = true;
                false
            }
            Token::Close if !inks.is_empty() => {
                inks.pop();
                style.ink = inks.last().copied();
                false
            }
            _ => true,
        };
        if !literal {
            continue;
        }
        let start = plain.len();
        plain.push_str(&text[a..b]);
        match runs.last_mut() {
            Some(r) if r.style == style && r.end == start => r.end = plain.len(),
            _ => runs.push(Run { start, end: plain.len(), style }),
        }
    }
    if !styled {
        return (text.to_string(), Vec::new());
    }
    (plain, runs)
}

/// The text as read, without its markers (a writer who may not style:
/// unchanged).
pub fn plain(text: &str, styles: Styles) -> String {
    parse(text, styles).0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styled(text: &str, styles: Styles) -> Vec<(String, Style)> {
        let (plain, runs) = parse(text, styles);
        runs.iter().map(|r| (plain[r.start..r.end].to_string(), r.style)).collect()
    }

    const BOLD: Style =
        Style { bold: true, italic: false, underline: false, strike: false, ink: None };

    #[test]
    fn only_vips_and_the_staff_are_styled() {
        let text = "**hello** [blue]there[/blue]";
        assert_eq!(parse(text, Styles::None), (text.to_string(), vec![]));
        assert_eq!(plain(text, Styles::Vip), "hello there");
        assert_eq!(plain(text, Styles::Staff), "hello there");
    }

    #[test]
    fn each_marker_styles_what_it_wraps() {
        let runs = styled("say **this** now", Styles::Vip);
        assert_eq!(runs[1], ("this".to_string(), BOLD));
        let it = styled("*so*", Styles::Vip);
        assert!(it[0].1.italic && !it[0].1.bold);
        let u = styled("__under__", Styles::Vip);
        assert!(u[0].1.underline);
        let s = styled("~~gone~~", Styles::Vip);
        assert!(s[0].1.strike);
        let both = styled("***both***", Styles::Vip);
        assert_eq!(both.len(), 1);
        assert!(both[0].1.bold && both[0].1.italic);
    }

    #[test]
    fn colours_and_who_may_use_them() {
        let runs = styled("[blue]sea[/blue] and [verde]grass[/]", Styles::Vip);
        assert_eq!(
            runs[0],
            ("sea".to_string(), Style { ink: Some(Ink::Blue), ..Style::default() })
        );
        assert_eq!(runs[2].1.ink, Some(Ink::Green));
        // Red is the staff's: a VIP's red tag stays as typed.
        assert_eq!(plain("[red]stop[/red]", Styles::Vip), "[red]stop[/red]");
        assert_eq!(styled("[red]stop[/red]", Styles::Staff)[0].1.ink, Some(Ink::Red));
        // The secret one (its name spelt backwards here).
        let name: String = "wobniar".chars().rev().collect();
        let tagged = format!("[{name}]wow[/]");
        assert_eq!(styled(&tagged, Styles::Vip)[0].1.ink, Some(Ink::Secret));
        // Nested: the inner colour, then the outer again.
        let nested = styled("[blue]a[gold]b[/]c[/]", Styles::Vip);
        let inks: Vec<_> = nested.iter().map(|r| r.1.ink).collect();
        assert_eq!(inks, vec![Some(Ink::Blue), Some(Ink::Gold), Some(Ink::Blue)]);
        // Unknown names are text.
        assert_eq!(plain("[link]x[/link]", Styles::Vip), "[link]x[/link]");
    }

    #[test]
    fn what_is_not_a_style_stays_as_typed() {
        // No partner, or spaces where a marker would need a letter.
        assert_eq!(plain("2 * 3 * 4", Styles::Vip), "2 * 3 * 4");
        assert_eq!(plain("**open", Styles::Vip), "**open");
        assert_eq!(plain("a ** b", Styles::Vip), "a ** b");
        // Mentions keep their underscores.
        assert_eq!(plain("hi @ana__b and @c__d", Styles::Vip), "hi @ana__b and @c__d");
        // Nothing styled: the text exactly.
        assert_eq!(parse("plain [x]", Styles::Staff), ("plain [x]".to_string(), vec![]));
    }

    #[test]
    fn runs_cover_the_plain_text() {
        let (plain, runs) = parse("é **ção** [pink]ok[/] 🎱 ~~x~~", Styles::Staff);
        assert_eq!(plain, "é ção ok 🎱 x");
        assert_eq!(runs.first().map(|r| r.start), Some(0));
        assert_eq!(runs.last().map(|r| r.end), Some(plain.len()));
        assert!(runs.windows(2).all(|w| w[0].end == w[1].start));
        assert!(
            runs.iter().all(|r| plain.is_char_boundary(r.start) && plain.is_char_boundary(r.end))
        );
    }
}
