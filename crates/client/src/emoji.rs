//! Emoji in text: the game's font has none, so the text functions of
//! [`crate::ui::Ui`] draw them as pictures from a bundled atlas (Twemoji,
//! CC-BY 4.0, made by tools/gen_emoji.py), inline with the text.
//!
//! Text is cut into units: plain characters for the font, emoji, and
//! blanks (characters that show nothing). An emoji is the longest sequence
//! of the atlas's table that matches (a flag, a keycap, a ZWJ sequence, a
//! single character), skipping U+FE0F and skin tones (an emoji with a skin
//! tone is drawn as its base, in the default yellow). The blanks are the
//! joiners, selectors and skin tones left over, and emoji the atlas doesn't
//! have: they take no room rather than show a box.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

use macroquad::miniquad::MipmapFilterMode;
use macroquad::prelude::*;

/// The atlas (see tools/gen_emoji.py): cells of `CELL` pixels, `COLUMNS`
/// to a row, in the order of the table, each with a picture `ART` pixels
/// across in its middle.
const CELL: f32 = 40.0;
const ART: f32 = 36.0;
const COLUMNS: usize = 51;
const PNG: &[u8] = include_bytes!("../assets/emoji.png");
const TABLE: &str = include_str!("../assets/emoji.txt");

/// The room an emoji takes in a line of text, in font sizes: its picture
/// (`SIDE`) and a little space on each side.
const ADVANCE: f32 = 1.14;
const SIDE: f32 = 1.0;
/// Where the picture's middle is, down from the top of the line, in font
/// sizes (the font's baseline is at 0.8, its capitals' tops at about 0.07).
const MIDDLE: f32 = 0.43;

const VS16: char = '\u{FE0F}';

fn is_tone(c: char) -> bool {
    ('\u{1F3FB}'..='\u{1F3FF}').contains(&c)
}

/// Characters that show nothing when they aren't part of an emoji the atlas
/// has: selectors, joiners, skin tones, regional indicator letters, tags,
/// the keycap mark, and pictographs this atlas doesn't have (newer emoji:
/// the font has none of those either).
fn is_blank(c: char) -> bool {
    matches!(c,
        '\u{FE0E}' | '\u{FE0F}' | '\u{200D}' | '\u{20E3}'
        | '\u{1F1E6}'..='\u{1F1FF}' | '\u{1F300}'..='\u{1FAFF}' | '\u{E0020}'..='\u{E007F}')
}

/// Could start an emoji: keycaps start with an ASCII character, © and ®
/// are emoji with U+FE0F, the others are all from U+203C up.
fn may_start(c: char) -> bool {
    matches!(c, '#' | '*' | '0'..='9' | '\u{A9}' | '\u{AE}') || c >= '\u{203C}'
}

/// True if `text` may hold an emoji or a blank: a character from U+2000 up
/// (they all start with a byte from 0xE2 up in UTF-8). Text without one is
/// the font's alone, as it always was.
#[inline]
pub fn maybe(text: &str) -> bool {
    text.bytes().any(|b| b >= 0xE2)
}

/// The atlas's table as a trie of code points.
struct Table {
    /// (node, character) → the next node; node 0 is the root.
    edges: HashMap<(u32, char), u32>,
    /// The emoji a node completes: its cell, and whether the font draws it
    /// as text unless U+FE0F (or a skin tone) follows.
    ends: Vec<Option<(u16, bool)>>,
}

impl Table {
    fn parse(src: &str) -> Table {
        let mut t = Table { edges: HashMap::new(), ends: vec![None] };
        let lines = src.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'));
        for (cell, line) in lines.enumerate() {
            let (seq, flag) = line.split_once(' ').unwrap_or((line, ""));
            let mut node = 0;
            for hex in seq.split('-') {
                let c = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
                let c = c.expect("a code point in assets/emoji.txt");
                node = match t.edges.get(&(node, c)) {
                    Some(&n) => n,
                    None => {
                        let n = t.ends.len() as u32;
                        t.ends.push(None);
                        t.edges.insert((node, c), n);
                        n
                    }
                };
            }
            t.ends[node as usize] = Some((cell as u16, flag.trim() == "text"));
        }
        t
    }

    /// The emoji that starts at byte `i` of `text`: where it ends and its
    /// cell. The longest sequence of the table wins; U+FE0F and skin tones
    /// after any of its characters are part of it.
    fn longest(&self, text: &str, i: usize) -> Option<(usize, u16)> {
        let mut node = 0;
        let mut count = 0;
        let mut best = None;
        let mut chars = text[i..].char_indices().peekable();
        while let Some((o, c)) = chars.next() {
            let Some(&next) = self.edges.get(&(node, c)) else { break };
            node = next;
            count += 1;
            let mut end = i + o + c.len_utf8();
            let mut selected = false;
            while let Some(&(o, d)) = chars.peek() {
                if d != VS16 && !is_tone(d) {
                    break;
                }
                selected = true;
                end = i + o + d.len_utf8();
                chars.next();
            }
            if let Some((cell, text_style)) = self.ends[node as usize]
                && (!text_style || count > 1 || selected)
            {
                best = Some((end, cell));
            }
        }
        best
    }
}

fn table() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| Table::parse(TABLE))
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Unit {
    Plain,
    Emoji(u16),
    Blank,
}

/// Calls `f(start, end, unit)` for each unit of `text`, in order (byte
/// offsets).
fn scan(text: &str, mut f: impl FnMut(usize, usize, Unit)) {
    let t = table();
    let mut i = 0;
    while let Some(c) = text[i..].chars().next() {
        if may_start(c)
            && let Some((end, cell)) = t.longest(text, i)
        {
            f(i, end, Unit::Emoji(cell));
            i = end;
            continue;
        }
        let end = i + c.len_utf8();
        f(i, end, if is_blank(c) { Unit::Blank } else { Unit::Plain });
        i = end;
    }
}

/// A piece of a line of text: plain text for the font, or an emoji (its
/// cell in the atlas).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Piece<'a> {
    Text(&'a str),
    Emoji(u16),
}

/// The pieces of `text` (blanks left out), or None when it is all plain
/// text, for the font as it is.
pub fn pieces(text: &str) -> Option<Vec<Piece<'_>>> {
    if !maybe(text) {
        return None;
    }
    let mut out = Vec::new();
    let mut plain = true;
    let mut run: Option<usize> = None;
    scan(text, |a, _, unit| match unit {
        Unit::Plain => {
            run.get_or_insert(a);
        }
        _ => {
            plain = false;
            if let Some(s) = run.take() {
                out.push(Piece::Text(&text[s..a]));
            }
            if let Unit::Emoji(cell) = unit {
                out.push(Piece::Emoji(cell));
            }
        }
    });
    if plain {
        return None;
    }
    if let Some(s) = run {
        out.push(Piece::Text(&text[s..]));
    }
    Some(out)
}

/// The units of `text` (a character, or a whole emoji) as byte ranges.
pub fn units(text: &str) -> Vec<(usize, usize)> {
    if !maybe(text) {
        return text.char_indices().map(|(i, c)| (i, i + c.len_utf8())).collect();
    }
    let mut out = Vec::new();
    scan(text, |a, b, _| out.push((a, b)));
    out
}

/// Removes the last unit of `s` (a character, or a whole emoji).
pub fn pop(s: &mut String) {
    if !maybe(s) {
        s.pop();
        return;
    }
    let mut last = 0;
    scan(s, |a, _, _| last = a);
    s.truncate(last);
}

/// Where a caret may stand in `chars`, as character indices from 0 to the
/// end, in order: never inside an emoji.
pub fn stops(chars: &[char]) -> Vec<usize> {
    if chars.iter().all(|&c| c < '\u{2000}') {
        return (0..=chars.len()).collect();
    }
    let text: String = chars.iter().collect();
    let mut out = Vec::with_capacity(chars.len() + 1);
    let mut at = 0;
    scan(&text, |a, b, _| {
        out.push(at);
        at += text[a..b].chars().count();
    });
    out.push(at);
    out
}

/// The room an emoji takes in a line of text of `size`.
pub fn advance(size: f32) -> f32 {
    size * ADVANCE
}

/// The atlas texture: decoded on another thread the first time an emoji
/// is drawn (emoji drawn meanwhile, for a few frames, take their room but
/// show nothing), then mipmapped, as they are drawn much smaller than
/// their cells.
enum Atlas {
    Idle,
    Decoding(std::thread::JoinHandle<Option<Image>>),
    Ready(Texture2D),
    Failed,
}

thread_local! {
    static ATLAS: RefCell<Atlas> = const { RefCell::new(Atlas::Idle) };
}

/// The atlas as pixels, with the colour of each picture's edge spread into
/// the clear pixels around it: filtering and mipmaps mix in the colour of
/// clear pixels too, and black ones would darken the edges.
fn decode() -> Option<Image> {
    let mut img = Image::from_file_with_format(PNG, None).ok()?;
    let (w, h) = (img.width as usize, img.height as usize);
    let px = &mut img.bytes;
    let mut known: Vec<bool> = px.chunks(4).map(|p| p[3] > 0).collect();
    // Two pixels out: all the clear room round the pictures in their cells.
    for _ in 0..2 {
        let mut fill = Vec::new();
        for y in 0..h {
            for x in 0..w {
                if known[y * w + x] {
                    continue;
                }
                let (mut sum, mut n) = ([0u32; 3], 0);
                for (nx, ny) in
                    [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)]
                {
                    if nx < w && ny < h && known[ny * w + nx] {
                        let j = (ny * w + nx) * 4;
                        for (s, v) in sum.iter_mut().zip(&px[j..j + 3]) {
                            *s += *v as u32;
                        }
                        n += 1;
                    }
                }
                if n > 0 {
                    fill.push((y * w + x, sum.map(|s| (s / n) as u8)));
                }
            }
        }
        for (i, rgb) in fill {
            px[i * 4..i * 4 + 3].copy_from_slice(&rgb);
            known[i] = true;
        }
    }
    Some(img)
}

fn atlas() -> Option<Texture2D> {
    ATLAS.with(|a| {
        let mut a = a.borrow_mut();
        match &*a {
            Atlas::Ready(tex) => return Some(tex.clone()),
            Atlas::Failed => return None,
            Atlas::Idle => {
                *a = Atlas::Decoding(std::thread::spawn(decode));
                return None;
            }
            Atlas::Decoding(job) if !job.is_finished() => return None,
            Atlas::Decoding(_) => {}
        }
        let Atlas::Decoding(job) = std::mem::replace(&mut *a, Atlas::Failed) else {
            return None;
        };
        let img = job.join().ok().flatten()?;
        let tex = Texture2D::from_image(&img);
        let id = tex.raw_miniquad_id();
        let ctx = unsafe { get_internal_gl() }.quad_context;
        // (Old GLES 2 drivers can't mipmap a texture whose sides aren't
        // powers of two; this atlas's are.)
        ctx.texture_generate_mipmaps(id);
        ctx.texture_set_min_filter(id, FilterMode::Linear, MipmapFilterMode::Linear);
        ctx.texture_set_mag_filter(id, FilterMode::Linear);
        *a = Atlas::Ready(tex.clone());
        Some(tex)
    })
}

/// Draws emoji `cell` in a line of text of `size` whose box starts at
/// (`x`, `y`), as [`crate::ui::Ui::text`] places text, in the room
/// [`advance`] gives it; `alpha` is the text colour's.
pub fn draw(cell: u16, x: f32, y: f32, size: f32, alpha: f32) {
    let Some(tex) = atlas() else { return };
    // Whole pixels, so it stays as sharp as it can be.
    let k = crate::ui::ui_scale();
    let px = |v: f32| (v * k).round() / k;
    let side = (size * SIDE * k).round().max(1.0) / k;
    let left = px(x + (advance(size) - side) / 2.0);
    let top = px(y + size * MIDDLE - side / 2.0);
    let (col, row) = (cell as usize % COLUMNS, cell as usize / COLUMNS);
    let pad = (CELL - ART) / 2.0;
    let src = Rect::new(col as f32 * CELL + pad, row as f32 * CELL + pad, ART, ART);
    draw_texture_ex(
        &tex,
        left,
        top,
        Color::new(1.0, 1.0, 1.0, alpha * crate::paint::opacity()),
        DrawTextureParams {
            dest_size: Some(vec2(side, side)),
            source: Some(src),
            ..Default::default()
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The emoji of `text` (their cells), in order.
    fn emoji(text: &str) -> Vec<u16> {
        pieces(text)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| if let Piece::Emoji(c) = p { Some(c) } else { None })
            .collect()
    }

    /// The cell of the sequence `seq` (the table's spelling) in the table.
    fn cell(seq: &str) -> u16 {
        TABLE
            .lines()
            .filter(|l| !l.starts_with('#'))
            .position(|l| l.split(' ').next() == Some(seq))
            .unwrap_or_else(|| panic!("{seq} not in the table")) as u16
    }

    fn texts(text: &str) -> Vec<String> {
        pieces(text)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| if let Piece::Text(t) = p { Some(t.to_string()) } else { None })
            .collect()
    }

    #[test]
    fn plain_text_is_left_alone() {
        for t in ["", "hello", "Olá, você! ação 123 #1 *", "a → b … “c” — d ▼ ★ • ◀ ▶", "© ® ™"]
        {
            assert_eq!(pieces(t), None, "{t}");
            assert_eq!(units(t).len(), t.chars().count());
        }
        assert!(!maybe("Olá, você! ação"));
        assert!(maybe("a…"));
        // Characters the font draws as text stay text without U+FE0F.
        assert_eq!(pieces("I ❤ it ☺ ♠ ✔"), None);
    }

    #[test]
    fn single_emoji() {
        assert_eq!(emoji("😂"), [cell("1f602")]);
        assert_eq!(
            pieces("ha 😂!"),
            Some(vec![Piece::Text("ha "), Piece::Emoji(cell("1f602")), Piece::Text("!")])
        );
        assert_eq!(
            emoji("👍🔥💯🎱⭐✅❌"),
            [
                cell("1f44d"),
                cell("1f525"),
                cell("1f4af"),
                cell("1f3b1"),
                cell("2b50"),
                cell("2705"),
                cell("274c")
            ]
        );
        // With U+FE0F, the ones the font has become pictures, and the
        // selector goes with them.
        assert_eq!(pieces("❤\u{FE0F}"), Some(vec![Piece::Emoji(cell("2764"))]));
        assert_eq!(emoji("☺\u{FE0F} ©\u{FE0F}"), [cell("263a"), cell("a9")]);
        // Pictographs the font doesn't have are pictures without it.
        assert_eq!(emoji("🖐 ⏸"), [cell("1f590"), cell("23f8")]);
    }

    #[test]
    fn flags() {
        assert_eq!(emoji("🇧🇷"), [cell("1f1e7-1f1f7")]);
        assert_eq!(
            emoji("🇧🇷🇵🇹🇺🇸"),
            [cell("1f1e7-1f1f7"), cell("1f1f5-1f1f9"), cell("1f1fa-1f1f8")]
        );
        // A letter on its own shows nothing.
        assert_eq!(pieces("a🇧"), Some(vec![Piece::Text("a")]));
        assert_eq!(emoji("🇧🇷🇧"), [cell("1f1e7-1f1f7")]);
        // Every country of the game has its flag.
        for (code, _, _) in snook_legend_proto::COUNTRIES {
            let flag: String = code
                .chars()
                .map(|c| char::from_u32(0x1F1E6 + (c as u32 - 'A' as u32)).unwrap())
                .collect();
            assert_eq!(emoji(&flag).len(), 1, "{code}");
        }
        // England's, with tags.
        let england = "🏴\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}";
        assert_eq!(emoji(england), [cell("1f3f4-e0067-e0062-e0065-e006e-e0067-e007f")]);
    }

    #[test]
    fn keycaps() {
        assert_eq!(emoji("1\u{FE0F}\u{20E3}"), [cell("31-20e3")]);
        assert_eq!(emoji("#\u{20E3}"), [cell("23-20e3")]);
        assert_eq!(texts("1\u{FE0F}2"), ["1", "2"]);
    }

    #[test]
    fn zwj_sequences() {
        assert_eq!(emoji("❤\u{FE0F}\u{200D}🔥"), [cell("2764-200d-1f525")]);
        assert_eq!(emoji("🏳\u{FE0F}\u{200D}🌈"), [cell("1f3f3-200d-1f308")]);
        assert_eq!(emoji("🤷\u{200D}♂\u{FE0F}"), [cell("1f937-200d-2642")]);
        assert_eq!(
            emoji("👨\u{200D}👩\u{200D}👧\u{200D}👦"),
            [cell("1f468-200d-1f469-200d-1f467-200d-1f466")]
        );
        // A sequence the atlas doesn't have: its emoji one after the other.
        assert_eq!(emoji("🐦\u{200D}🔥"), [cell("1f426"), cell("1f525")]);
        // A joiner left over shows nothing.
        assert_eq!(pieces("a\u{200D}b"), Some(vec![Piece::Text("a"), Piece::Text("b")]));
        assert_eq!(emoji("😂\u{200D}"), [cell("1f602")]);
    }

    #[test]
    fn skin_tones() {
        assert_eq!(emoji("👍🏽"), [cell("1f44d")]);
        assert_eq!(pieces("👍🏿!"), Some(vec![Piece::Emoji(cell("1f44d")), Piece::Text("!")]));
        assert_eq!(emoji("🤷🏽\u{200D}♀\u{FE0F}"), [cell("1f937-200d-2640")]);
        assert_eq!(emoji("✌🏻"), [cell("270c")]);
        assert_eq!(emoji("🧑🏻\u{200D}🤝\u{200D}🧑🏼"), [cell("1f9d1-200d-1f91d-200d-1f9d1")]);
        // On their own they show nothing.
        assert_eq!(pieces("🏽"), Some(vec![]));
    }

    #[test]
    fn unknown_emoji_show_nothing() {
        // Newer than the atlas (Unicode 16): no box, no room.
        assert_eq!(pieces("a\u{1FAE9}b"), Some(vec![Piece::Text("a"), Piece::Text("b")]));
        assert_eq!(pieces("\u{FE0F}\u{FE0E}\u{20E3}"), Some(vec![]));
    }

    #[test]
    fn units_and_stops() {
        let t = "a🇧🇷b👍🏽❤\u{FE0F}";
        let u: Vec<&str> = units(t).into_iter().map(|(a, b)| &t[a..b]).collect();
        assert_eq!(u, ["a", "🇧🇷", "b", "👍🏽", "❤\u{FE0F}"]);
        let chars: Vec<char> = t.chars().collect();
        assert_eq!(stops(&chars), [0, 1, 3, 4, 6, 8]);
        assert_eq!(stops(&['a', 'b']), [0, 1, 2]);
        assert_eq!(stops(&[]), [0]);
        let mut s = t.to_string();
        pop(&mut s);
        assert_eq!(s, "a🇧🇷b👍🏽");
        pop(&mut s);
        assert_eq!(s, "a🇧🇷b");
        pop(&mut s);
        pop(&mut s);
        assert_eq!(s, "a");
        // A prefix cut between units splits into the same units.
        for (_, b) in units(t) {
            let p = &t[..b];
            let mine: Vec<_> = units(p);
            let theirs: Vec<_> = units(t).into_iter().filter(|u| u.1 <= b).collect();
            assert_eq!(mine, theirs);
        }
    }

    #[test]
    fn the_atlas_matches_the_table() {
        let n = TABLE.lines().filter(|l| !l.starts_with('#')).count();
        let img = Image::from_file_with_format(PNG, None).unwrap();
        let (w, h) = (img.width as usize, img.height as usize);
        assert!(w.is_power_of_two() && h.is_power_of_two());
        assert!(COLUMNS * CELL as usize <= w);
        let used = |cell: usize| {
            let (x0, y0) = ((cell % COLUMNS) * CELL as usize, (cell / COLUMNS) * CELL as usize);
            (y0..y0 + CELL as usize)
                .any(|y| (x0..x0 + CELL as usize).any(|x| img.bytes[(y * w + x) * 4 + 3] > 0))
        };
        assert!((n.div_ceil(COLUMNS)) * CELL as usize <= h);
        // Every cell of the table has a picture, and the next one is empty.
        assert!((0..n).all(used));
        assert!(!used(n));
        // The pictures keep clear of their cells' edges.
        let pad = ((CELL - ART) / 2.0) as usize;
        for cell in 0..n {
            let (x0, y0) = ((cell % COLUMNS) * CELL as usize, (cell / COLUMNS) * CELL as usize);
            for y in y0..y0 + CELL as usize {
                for x in [x0, x0 + pad - 1, x0 + CELL as usize - pad, x0 + CELL as usize - 1] {
                    assert_eq!(img.bytes[(y * w + x) * 4 + 3], 0, "cell {cell}");
                }
            }
        }
        // Decoded for drawing, the edges' colours spread into the clear
        // pixels round them (nothing else changes).
        let spread = decode().unwrap();
        assert_eq!(spread.bytes.len(), img.bytes.len());
        assert!(spread.bytes.chunks(4).zip(img.bytes.chunks(4)).all(|(a, b)| a[3] == b[3]));
        let first = (pad - 1) * w + pad + 10;
        assert_eq!(img.bytes[first * 4 + 3], 0);
        assert_ne!(spread.bytes[first * 4..first * 4 + 3], [0, 0, 0]);
    }
}
