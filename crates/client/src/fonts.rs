//! The fonts of every script the game speaks, and how a line of text is
//! laid out in them.
//!
//! Inter (the interface's own font, held by [`crate::ui::Ui`]; subsets
//! made by `tools/gen_ui_fonts.py`) draws Latin, Greek and Cyrillic,
//! character by character. The symbols it lacks come from DejaVu Sans, and
//! the other scripts from Noto (subsets made by `tools/gen_fonts.py`):
//!
//! - Chinese, Japanese and Korean, also character by character, the font
//!   of the player's language first (the same character is drawn a little
//!   differently in each);
//! - the scripts whose letters change with their neighbours (Arabic,
//!   Devanagari, Bengali, Thai): a run of them is shaped by rustybuzz (a
//!   port of HarfBuzz) and its glyphs drawn from an atlas of our own;
//! - Arabic reads right to left: a line with it is put in visual order by
//!   the Unicode bidirectional algorithm (numbers and Latin names inside it
//!   stay left to right).
//!
//! Lines are laid out once and kept (a few thousand of them): the
//! interface asks for the same texts every frame.

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::rc::Rc;

use macroquad::prelude::*;

use crate::i18n::Lang;

const DEJAVU: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");

/// The interface's text faces (Inter), for [`crate::ui::Ui`].
pub const INTER_REGULAR: &[u8] = include_bytes!("../assets/inter/Inter-Regular.ttf");
pub const INTER_BOLD: &[u8] = include_bytes!("../assets/inter/Inter-SemiBold.ttf");
/// Big bold titles: Inter Display (drawn for large sizes).
pub const INTER_DISPLAY: &[u8] = include_bytes!("../assets/inter/InterDisplay-Bold.ttf");

/// A font face beyond the interface's own (`Main`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    /// Inter: the [`crate::ui::Ui`]'s fonts.
    Main,
    /// DejaVu Sans: the symbols Inter lacks.
    Symbols,
    Chinese,
    Japanese,
    Korean,
    Arabic,
    Devanagari,
    Bengali,
    Thai,
}

impl Face {
    const ALL: [Face; 9] = [
        Face::Main,
        Face::Symbols,
        Face::Chinese,
        Face::Japanese,
        Face::Korean,
        Face::Arabic,
        Face::Devanagari,
        Face::Bengali,
        Face::Thai,
    ];

    /// Drawn from shaped glyphs (rather than character by character).
    pub fn shaped(self) -> bool {
        matches!(self, Face::Arabic | Face::Devanagari | Face::Bengali | Face::Thai)
    }

    fn bytes(self, bold: bool) -> &'static [u8] {
        macro_rules! noto {
            ($name:literal) => {
                if bold {
                    &include_bytes!(concat!("../assets/noto/", $name, "-Bold", ".otf"))[..]
                } else {
                    &include_bytes!(concat!("../assets/noto/", $name, "-Regular", ".otf"))[..]
                }
            };
            ($name:literal, ttf) => {
                if bold {
                    &include_bytes!(concat!("../assets/noto/", $name, "-Bold", ".ttf"))[..]
                } else {
                    &include_bytes!(concat!("../assets/noto/", $name, "-Regular", ".ttf"))[..]
                }
            };
        }
        match self {
            // (Only its coverage is asked for: the Ui draws it.)
            Face::Main => INTER_REGULAR,
            Face::Symbols => {
                if bold {
                    include_bytes!("../assets/DejaVuSans-Bold.ttf")
                } else {
                    DEJAVU
                }
            }
            Face::Chinese => noto!("NotoSansSC"),
            Face::Japanese => noto!("NotoSansJP"),
            Face::Korean => noto!("NotoSansKR"),
            Face::Arabic => noto!("NotoSansArabic", ttf),
            Face::Devanagari => noto!("NotoSansDevanagari", ttf),
            Face::Bengali => noto!("NotoSansBengali", ttf),
            Face::Thai => noto!("NotoSansThai", ttf),
        }
    }

    fn index(self, bold: bool) -> usize {
        Face::ALL.iter().position(|f| *f == self).unwrap_or(0) * 2 + bold as usize
    }
}

/// The faces' tables, parsed on first use (regular and bold).
struct Parsed {
    shaper: rustybuzz::Face<'static>,
    raster: OnceCell<fontdue::Font>,
}

thread_local! {
    static PARSED: [OnceCell<Option<Parsed>>; 18] = Default::default();
    // Never dropped: a font freed after the window (at exit) would free its
    // texture in a dead graphics context.
    static CHARS: [OnceCell<Option<ManuallyDrop<Font>>>; 18] = Default::default();
    static LAYOUTS: RefCell<HashMap<(String, bool, Lang), Rc<Layout>>> =
        RefCell::new(HashMap::new());
    static ATLAS: RefCell<Option<Atlas>> = const { RefCell::new(None) };
}

fn with_parsed<R>(face: Face, bold: bool, f: impl FnOnce(&Parsed) -> R) -> Option<R> {
    PARSED.with(|p| {
        p[face.index(bold)]
            .get_or_init(|| {
                let shaper = rustybuzz::Face::from_slice(face.bytes(bold), 0)?;
                Some(Parsed { shaper, raster: OnceCell::new() })
            })
            .as_ref()
            .map(f)
    })
}

/// Whether `face` has a glyph for `c` (the regular weight decides: both
/// weights cover the same characters).
fn covers(face: Face, c: char) -> bool {
    with_parsed(face, false, |p| p.shaper.glyph_index(c).is_some()).unwrap_or(false)
}

/// The macroquad font that draws `face` character by character (Inter's
/// are the [`crate::ui::Ui`]'s; these are loaded the first time a line
/// needs them).
pub fn chars_font(face: Face, bold: bool) -> Option<Font> {
    CHARS.with(|c| {
        c[face.index(bold)]
            .get_or_init(|| load_ttf_font_from_bytes(face.bytes(bold)).ok().map(ManuallyDrop::new))
            .as_ref()
            .map(|f| Font::clone(f))
    })
}

/// A glyph of a shaped run, in font units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    pub id: u16,
    pub advance: i32,
    pub dx: i32,
    pub dy: i32,
}

/// A run of a laid-out line, in the order it is drawn (left to right).
#[derive(Debug, Clone, PartialEq)]
pub enum Run {
    /// Characters drawn one after the other in `face` (in visual order:
    /// reversed and mirrored in a right-to-left run).
    Chars { face: Face, text: String },
    /// Shaped glyphs, in visual order.
    Glyphs { face: Face, glyphs: Vec<Glyph>, units_per_em: i32 },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub runs: Vec<Run>,
}

/// Text the interface's font draws by itself (no layout needed): Latin,
/// Greek, Cyrillic and the common punctuation and symbols it has.
pub fn simple(text: &str) -> bool {
    text.chars().all(|c| {
        let n = c as u32;
        n < 0x80
            || ((n < 0x0590 || (0x1E00..0x200B).contains(&n) || (0x2010..0x2E00).contains(&n))
                && covers(Face::Main, c))
    })
}

/// The script a character belongs to, when it needs a face of its own.
fn script_face(c: char) -> Option<Face> {
    Some(match c as u32 {
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => {
            Face::Arabic
        }
        0x0900..=0x0963 | 0x0966..=0x097F | 0xA8E0..=0xA8FF => Face::Devanagari,
        0x0980..=0x09FF => Face::Bengali,
        0x0E00..=0x0E7F => Face::Thai,
        _ => return None,
    })
}

/// Marks and joiners: they belong to the character before them.
fn joins_previous(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x200C | 0x200D | 0xFE00..=0xFE0F)
}

/// Characters that read right to left.
fn is_rtl(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x200F)
}

/// Shows nothing and takes no room: the zero-width space, which marks
/// where a Thai line may break (see [`crate::ui::Ui::wrap`]).
pub const ZWSP: char = '\u{200B}';

/// The Chinese, Japanese and Korean faces, the one of the player's
/// language first.
fn cjk_order(lang: Lang) -> [Face; 3] {
    match lang {
        Lang::Japanese => [Face::Japanese, Face::Chinese, Face::Korean],
        Lang::Korean => [Face::Korean, Face::Chinese, Face::Japanese],
        _ => [Face::Chinese, Face::Japanese, Face::Korean],
    }
}

/// The face for `c`, `current` being the face of the character before.
fn face_for(c: char, current: Option<Face>, lang: Lang) -> Face {
    if joins_previous(c) {
        return current.unwrap_or(Face::Main);
    }
    // The danda ends sentences in Bengali too.
    if matches!(c, '\u{0964}' | '\u{0965}') {
        return if current == Some(Face::Bengali) { Face::Bengali } else { Face::Devanagari };
    }
    if let Some(face) = script_face(c) {
        return face;
    }
    // Spaces and punctuation inside a shaped run stay in it (so an Arabic
    // sentence keeps its order, and the run isn't cut up), if its font has them.
    if let Some(face) = current
        && face.shaped()
        && (c.is_whitespace() || c.is_ascii_punctuation() || matches!(c, '«' | '»' | '…'))
        && covers(face, c)
    {
        return face;
    }
    if covers(Face::Main, c) {
        return Face::Main;
    }
    if covers(Face::Symbols, c) {
        return Face::Symbols;
    }
    cjk_order(lang).into_iter().find(|f| covers(*f, c)).unwrap_or(Face::Main)
}

/// The mirror image of a bracket, for a right-to-left run drawn character
/// by character.
fn mirrored(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        '‹' => '›',
        '›' => '‹',
        c => c,
    }
}

/// Shapes `text` in `face`: its glyphs in visual order.
fn shape(text: &str, face: Face, bold: bool, rtl: bool) -> Option<(Vec<Glyph>, i32)> {
    with_parsed(face, bold, |p| {
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        buffer.set_direction(if rtl {
            rustybuzz::Direction::RightToLeft
        } else {
            rustybuzz::Direction::LeftToRight
        });
        let out = rustybuzz::shape(&p.shaper, &[], buffer);
        let glyphs = out
            .glyph_infos()
            .iter()
            .zip(out.glyph_positions())
            .map(|(info, pos)| Glyph {
                id: info.glyph_id as u16,
                advance: pos.x_advance,
                dx: pos.x_offset,
                dy: pos.y_offset,
            })
            .collect();
        (glyphs, p.shaper.units_per_em())
    })
}

/// Lays out the characters of one direction's run (logical order).
fn runs_of(text: &str, rtl: bool, bold: bool, lang: Lang, out: &mut Vec<Run>) {
    let mut pieces: Vec<(Face, String)> = Vec::new();
    for c in text.chars() {
        if c == ZWSP {
            continue;
        }
        let current = pieces.last().map(|p| p.0);
        let face = face_for(c, current, lang);
        match pieces.last_mut() {
            Some((f, s)) if *f == face => s.push(c),
            _ => pieces.push((face, c.to_string())),
        }
    }
    if rtl {
        pieces.reverse();
    }
    for (face, s) in pieces {
        if face.shaped()
            && let Some((glyphs, units_per_em)) = shape(&s, face, bold, rtl)
        {
            out.push(Run::Glyphs { face, glyphs, units_per_em });
        } else {
            let text = if rtl { s.chars().rev().map(mirrored).collect() } else { s };
            match out.last_mut() {
                Some(Run::Chars { face: f, text: t }) if *f == face => t.push_str(&text),
                _ => out.push(Run::Chars { face, text }),
            }
        }
    }
}

/// Lays `text` out (one line), uncached.
fn lay_out(text: &str, bold: bool, lang: Lang) -> Layout {
    let mut runs = Vec::new();
    if !text.chars().any(is_rtl) {
        runs_of(text, false, bold, lang, &mut runs);
        return Layout { runs };
    }
    // A text in the interface's own right-to-left language reads right to
    // left from its start; any other (a name, a chat line) by its first letter.
    let level = lang.rtl().then(unicode_bidi::Level::rtl);
    let info = unicode_bidi::BidiInfo::new(text, level);
    for para in &info.paragraphs {
        let (levels, ranges) = info.visual_runs(para, para.range.clone());
        for range in ranges {
            let rtl = levels[range.start].is_rtl();
            runs_of(&text[range], rtl, bold, lang, &mut runs);
        }
    }
    Layout { runs }
}

/// `text` laid out in the fonts (kept for the next frames).
pub fn layout(text: &str, bold: bool) -> Rc<Layout> {
    let lang = crate::i18n::lang();
    LAYOUTS.with(|l| {
        let mut l = l.borrow_mut();
        if let Some(found) = l.get(&(text.to_string(), bold, lang)) {
            return found.clone();
        }
        if l.len() > 4000 {
            l.clear();
        }
        let out = Rc::new(lay_out(text, bold, lang));
        l.insert((text.to_string(), bold, lang), out.clone());
        out
    })
}

/// The pixel size macroquad rasterises text of `size` (interface units)
/// at, and how many interface units one of those pixels is.
fn pixels(size: f32) -> (u16, f32) {
    let k = crate::ui::ui_scale();
    let dpi = macroquad::miniquad::window::dpi_scale();
    let font_size = (size * k).round().max(1.0);
    let px = (font_size * dpi).ceil().max(1.0);
    (px as u16, 1.0 / (k * dpi))
}

/// How wide a run of shaped glyphs is at `size` (interface units).
pub fn glyphs_width(glyphs: &[Glyph], units_per_em: i32, size: f32) -> f32 {
    let (px, unit) = pixels(size);
    let total: i32 = glyphs.iter().map(|g| g.advance).sum();
    total as f32 * px as f32 / units_per_em.max(1) as f32 * unit
}

/// Where a glyph sits in the atlas.
#[derive(Clone, Copy)]
struct Sprite {
    rect: Rect,
    xmin: i32,
    ymin: i32,
}

/// The shaped glyphs drawn so far, packed in rows into one texture
/// (emptied and refilled if it ever runs out of room).
struct Atlas {
    texture: Texture2D,
    x: u16,
    y: u16,
    row: u16,
    sprites: HashMap<(usize, u16, u16), Option<Sprite>>,
}

const ATLAS_SIDE: u16 = 1024;

impl Atlas {
    fn new() -> Atlas {
        let side = ATLAS_SIDE as usize;
        let texture = Texture2D::from_rgba8(ATLAS_SIDE, ATLAS_SIDE, &vec![0; side * side * 4]);
        texture.set_filter(FilterMode::Linear);
        Atlas { texture, x: 1, y: 1, row: 0, sprites: HashMap::new() }
    }

    fn sprite(&mut self, face: Face, bold: bool, glyph: u16, px: u16) -> Option<Sprite> {
        let key = (face.index(bold), glyph, px);
        if let Some(s) = self.sprites.get(&key) {
            return *s;
        }
        let raster = with_parsed(face, bold, |p| {
            let font = p.raster.get_or_init(|| {
                fontdue::Font::from_bytes(face.bytes(bold), fontdue::FontSettings::default())
                    .expect("a bundled font")
            });
            font.rasterize_indexed(glyph, px as f32)
        });
        let sprite = raster.and_then(|(m, bitmap)| {
            let (w, h) = (m.width as u16, m.height as u16);
            if w == 0 || h == 0 || w + 2 > ATLAS_SIDE || h + 2 > ATLAS_SIDE {
                return None;
            }
            if self.x + w + 1 > ATLAS_SIDE {
                self.x = 1;
                self.y += self.row + 1;
                self.row = 0;
            }
            if self.y + h + 1 > ATLAS_SIDE {
                // Full: start over (the glyphs come back as they're drawn).
                self.sprites.clear();
                self.x = 1;
                self.y = 1;
                self.row = 0;
            }
            let image = Image {
                bytes: bitmap.iter().flat_map(|a| [255, 255, 255, *a]).collect(),
                width: w,
                height: h,
            };
            self.texture.update_part(&image, self.x as i32, self.y as i32, w as i32, h as i32);
            let rect = Rect::new(self.x as f32, self.y as f32, w as f32, h as f32);
            self.x += w + 1;
            self.row = self.row.max(h);
            Some(Sprite { rect, xmin: m.xmin, ymin: m.ymin })
        });
        self.sprites.insert(key, sprite);
        sprite
    }
}

/// Draws shaped glyphs from `x`, on the baseline `baseline` (interface
/// units), at `size`.
#[allow(clippy::too_many_arguments)]
pub fn draw_glyphs(
    face: Face,
    bold: bool,
    glyphs: &[Glyph],
    units_per_em: i32,
    x: f32,
    baseline: f32,
    size: f32,
    color: Color,
) {
    let (px, unit) = pixels(size);
    let scale = px as f32 / units_per_em.max(1) as f32;
    ATLAS.with(|a| {
        let mut a = a.borrow_mut();
        let atlas = a.get_or_insert_with(Atlas::new);
        let mut pen = 0.0;
        for g in glyphs {
            if let Some(s) = atlas.sprite(face, bold, g.id, px) {
                let gx = x + (pen + g.dx as f32 * scale + s.xmin as f32) * unit;
                let gy = baseline - (g.dy as f32 * scale + s.ymin as f32 + s.rect.h) * unit;
                draw_texture_ex(
                    &atlas.texture,
                    gx,
                    gy,
                    color,
                    DrawTextureParams {
                        dest_size: Some(vec2(s.rect.w * unit, s.rect.h * unit)),
                        source: Some(s.rect),
                        ..Default::default()
                    },
                );
            }
            pen += g.advance as f32 * scale;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn faces(l: &Layout) -> Vec<Face> {
        l.runs
            .iter()
            .map(|r| match r {
                Run::Chars { face, .. } | Run::Glyphs { face, .. } => *face,
            })
            .collect()
    }

    #[test]
    fn latin_greek_and_cyrillic_are_dejavus_alone() {
        for text in ["Play", "Jogar · Configurações", "Ελληνικά", "Українська", "Tiếng Việt"]
        {
            assert!(simple(text), "{text}");
            assert_eq!(faces(&lay_out(text, false, Lang::English)), [Face::Main], "{text}");
        }
    }

    #[test]
    fn chinese_japanese_and_korean_take_the_languages_own_font_first() {
        let l = lay_out("VIP 会员", false, Lang::Chinese);
        assert_eq!(faces(&l), [Face::Main, Face::Chinese]);
        let l = lay_out("設定", false, Lang::Japanese);
        assert_eq!(faces(&l), [Face::Japanese]);
        // Hangul is in the Korean font only, whatever the language.
        let l = lay_out("설정", false, Lang::English);
        assert_eq!(faces(&l), [Face::Korean]);
    }

    #[test]
    fn indic_and_thai_text_is_shaped() {
        // "क्षमा": a conjunct, fewer glyphs than characters.
        let l = lay_out("क्षमा करें", false, Lang::Hindi);
        let Run::Glyphs { face, glyphs, .. } = &l.runs[0] else { panic!("{l:?}") };
        assert_eq!(*face, Face::Devanagari);
        assert!(glyphs.len() < "क्षमा करें".chars().count(), "{glyphs:?}");
        assert_eq!(l.runs.len(), 1, "the space stays in the run: {l:?}");
        // The danda after Bengali is Bengali's.
        let l = lay_out("খেলুন।", false, Lang::Bengali);
        assert_eq!(faces(&l), [Face::Bengali]);
        // Thai, with the zero-width spaces that mark its line breaks left out.
        let l = lay_out("เล่น\u{200B}เกม", false, Lang::Thai);
        assert_eq!(faces(&l), [Face::Thai]);
    }

    #[test]
    fn arabic_reads_right_to_left_with_numbers_left_to_right() {
        // "500 coins": in an Arabic interface the number is at the right
        // (read first), and keeps its digits in order.
        let l = lay_out("500 عملة", false, Lang::Arabic);
        assert_eq!(faces(&l), [Face::Arabic, Face::Main]);
        let Run::Chars { text, .. } = &l.runs[1] else { panic!("{l:?}") };
        assert_eq!(text.trim(), "500");
        // A Latin name first in a chat line read in another language: the
        // line runs left to right.
        let l = lay_out("Ana: مرحبا", false, Lang::English);
        let Run::Chars { text, .. } = &l.runs[0] else { panic!("{l:?}") };
        assert!(text.starts_with("Ana"), "{l:?}");
    }
}
