//! Profile pictures: one of the predefined pictures
//! ([`snook_legend_proto::AVATARS`]), chosen in the profile (or in the
//! settings, offline) and shown in the score panel, the profile and the
//! lounge.

use std::cell::{OnceCell, RefCell};

use macroquad::miniquad::MipmapFilterMode;
use macroquad::prelude::*;

use crate::paint::draw_rectangle;

use crate::i18n::t;
use crate::icons::Icon;
use crate::ui::{BORDER, GOLD, Ui, rrect, rrect_lines};

/// The pictures' atlas (made by tools/gen_avatars.py): a cell of `CELL`
/// pixels for each code of AVATARS, in that order, `COLUMNS` to a row.
/// Each picture is a disc with clear corners, `DISC` of the cell across.
const CELL: f32 = 128.0;
const COLUMNS: usize = 8;
const DISC: f32 = 126.0 / 128.0;

thread_local! {
    static ATLAS: OnceCell<Texture2D> = const { OnceCell::new() };
}

fn atlas() -> Texture2D {
    ATLAS.with(|a| {
        a.get_or_init(|| {
            let tex =
                Texture2D::from_file_with_format(include_bytes!("../assets/avatars.png"), None);
            // Mipmapped: a picture is drawn from about 20 pixels across to
            // more than its cell, and shrunk a lot without them it looks
            // grainy. (Old GLES 2 drivers can't mipmap a texture whose
            // sides aren't powers of two: such an atlas is drawn plain.)
            let id = tex.raw_miniquad_id();
            let ctx = unsafe { get_internal_gl() }.quad_context;
            let pot = |v: f32| (v as u32).is_power_of_two();
            if pot(tex.width()) && pot(tex.height()) {
                ctx.texture_generate_mipmaps(id);
                ctx.texture_set_min_filter(id, FilterMode::Linear, MipmapFilterMode::Linear);
            } else {
                ctx.texture_set_min_filter(id, FilterMode::Linear, MipmapFilterMode::None);
            }
            ctx.texture_set_mag_filter(id, FilterMode::Linear);
            tex
        })
        .clone()
    })
}

/// Draws picture `code` in a disc of radius `r` at `c` (a plain silhouette
/// for none, or for a code this version doesn't know).
pub fn draw(code: &str, c: Vec2, r: f32) {
    // A retired face is drawn as the thing of the game in its place.
    let code = snook_legend_proto::shown_avatar(code);
    let Some(i) = snook_legend_proto::AVATARS.iter().position(|a| *a == code) else {
        crate::scoreboard::silhouette(c, r);
        return;
    };
    let (col, row) = (i % COLUMNS, i / COLUMNS);
    let half = r / DISC;
    draw_texture_ex(
        &atlas(),
        c.x - half,
        c.y - half,
        Color::new(1.0, 1.0, 1.0, crate::paint::alpha()),
        DrawTextureParams {
            dest_size: Some(vec2(2.0 * half, 2.0 * half)),
            source: Some(Rect::new(col as f32 * CELL, row as f32 * CELL, CELL, CELL)),
            ..Default::default()
        },
    );
}

#[derive(Default)]
struct State {
    /// This player's picture (from the settings, or from their account).
    mine: String,
    open: bool,
    /// The click that opened the picker doesn't choose in it.
    armed: bool,
    /// How far the lines are scrolled, when they don't all fit.
    scroll: crate::ui::Scroll,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// This player's picture.
pub fn mine() -> String {
    STATE.with(|s| s.borrow().mine.clone())
}

pub fn set_mine(code: &str) {
    if snook_legend_proto::valid_avatar(code) {
        let code = snook_legend_proto::shown_avatar(code);
        STATE.with(|s| s.borrow_mut().mine = code.to_string());
    }
}

/// Opens the picker.
pub fn open() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.open = true;
        s.armed = false;
        s.scroll.reset();
    });
}

pub fn is_open() -> bool {
    STATE.with(|s| s.borrow().open)
}

/// A line of the picker: pictures, or the line between the people and the
/// things.
enum Line {
    Cells(Vec<&'static str>),
    Divider,
}

/// The picker's lines with `cols` pictures to a line: no picture and the
/// people first, then the things of the game.
fn lines(cols: usize) -> Vec<Line> {
    let all = snook_legend_proto::AVATARS;
    let people: Vec<&'static str> =
        std::iter::once("").chain(all.into_iter().filter(|c| !is_thing(c))).collect();
    let things: Vec<&'static str> = all.into_iter().filter(|c| is_thing(c)).collect();
    let mut out: Vec<Line> = people.chunks(cols).map(|c| Line::Cells(c.to_vec())).collect();
    out.push(Line::Divider);
    out.extend(things.chunks(cols).map(|c| Line::Cells(c.to_vec())));
    out
}

/// A picture of a thing of the game, not a person.
fn is_thing(code: &str) -> bool {
    code.starts_with("pic_")
}

/// Draws the picker when open; returns the picture chosen ("" for none),
/// which is then this player's.
pub fn draw_picker(ui: &mut Ui) -> Option<String> {
    if !is_open() {
        return None;
    }
    let sw = crate::ui::vw();
    let sh = crate::ui::vh();
    let _modal = ui.modal_scope(crate::ui::motion::id("avatars"), 0.55);
    const PAD: f32 = 24.0;
    const GAP: f32 = 10.0;
    // The room between the people and the things.
    const SEP: f32 = 18.0;
    const MIN_CELL: f32 = 40.0;
    // As many columns as fit (11 at most: the people make three full
    // lines), cells as large as fit.
    let cols = (((sw - 40.0 - 2.0 * PAD + GAP) / (60.0 + GAP)).floor() as usize).clamp(4, 11);
    let lines = lines(cols);
    let rows = lines.iter().filter(|l| matches!(l, Line::Cells(_))).count();
    let head = 64.0;
    let room_h = (sh - 40.0 - head - PAD).max(160.0);
    let cell = ((room_h - SEP + GAP) / rows as f32 - GAP).clamp(MIN_CELL, 72.0);
    let line_h = |l: &Line| match l {
        Line::Cells(_) => cell + GAP,
        Line::Divider => SEP,
    };
    // In a window too small for them all, the lines scroll.
    let total: f32 = lines.iter().map(line_h).sum::<f32>() - GAP;
    let shown = total.min(room_h);
    let w = 2.0 * PAD + cols as f32 * (cell + GAP) - GAP;
    let h = head + shown + PAD;
    let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
    ui.panel(r);
    ui.block(Rect::new(0.0, 0.0, sw, sh));
    let head_mid = r.y + PAD + 14.0;
    crate::icons::draw(Icon::Person, vec2(r.x + PAD + 11.0, head_mid), 22.0, GOLD);
    ui.text(t("Profile picture"), r.x + PAD + 32.0, head_mid - 12.0, 22.0, GOLD, true);
    let close = ui.icon_button_ex(
        Rect::new(r.x + r.w - PAD - 32.0, head_mid - 16.0, 32.0, 32.0),
        Icon::Close,
        t("Close"),
        true,
        true,
    );
    let (mine, armed) = STATE.with(|s| {
        let mut s = s.borrow_mut();
        if is_mouse_button_pressed(MouseButton::Left) {
            s.armed = true;
        }
        (s.mine.clone(), s.armed)
    });
    let mut chosen = None;
    let mut scroll = STATE.with(|s| s.borrow().scroll.clone());
    let area = Rect::new(r.x + PAD, r.y + head, r.w - PAD, shown);
    let view = ui.scroll_begin(area, &mut scroll, total);
    let mut y = view.top();
    for line in &lines {
        if !view.visible(y, line_h(line)) {
            y += line_h(line);
            continue;
        }
        let codes = match line {
            Line::Divider => {
                let mid = y + SEP / 2.0 - GAP / 2.0;
                draw_rectangle(r.x + PAD, mid - 0.5, r.w - 2.0 * PAD, 1.0, with_alpha(BORDER, 0.7));
                y += SEP;
                continue;
            }
            Line::Cells(codes) => codes,
        };
        for (col, &code) in codes.iter().enumerate() {
            let cr = Rect::new(r.x + PAD + col as f32 * (cell + GAP), y, cell, cell);
            let hover = ui.hovering(cr);
            let lit = ui.hover_anim(cr, hover);
            let selected = code == mine;
            let fill = if selected { 0.04 } else { 0.0 }
                + (0.08 - if selected { 0.04 } else { 0.0 }) * lit;
            if fill > 0.001 {
                rrect(cr, 10.0, Color::new(1.0, 1.0, 1.0, fill));
            }
            draw(code, vec2(cr.x + cell / 2.0, cr.y + cell / 2.0), cell / 2.0 - 4.0);
            if selected {
                rrect_lines(cr, 10.0, 2.0, GOLD);
            } else if lit > 0.01 {
                rrect_lines(cr, 10.0, 1.0, Color { a: BORDER.a * lit, ..BORDER });
            }
            if code.is_empty() && hover {
                ui.tooltip(t("No picture"), cr);
            }
            if hover && armed && ui.clicked(cr) {
                chosen = Some(code.to_string());
            }
        }
        y += cell + GAP;
    }
    ui.scroll_end(view, &mut scroll);
    STATE.with(|s| s.borrow_mut().scroll = scroll);
    // Closed also by a press on the dimmed screen around it.
    if chosen.is_some() || close || is_key_pressed(KeyCode::Escape) || (armed && ui.dismissed(r)) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.open = false;
            if let Some(c) = &chosen {
                s.mine = c.clone();
            }
        });
    }
    chosen
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_atlas_has_a_cell_for_every_picture() {
        let png = include_bytes!("../assets/avatars.png");
        // The PNG header: width and height, big-endian, from byte 16.
        let w = u32::from_be_bytes(png[16..20].try_into().unwrap()) as f32;
        let h = u32::from_be_bytes(png[20..24].try_into().unwrap()) as f32;
        assert_eq!(w, CELL * COLUMNS as f32);
        let cells = (h / CELL) as usize * COLUMNS;
        assert!(cells >= snook_legend_proto::AVATARS.len(), "regenerate: tools/gen_avatars.py");
        assert!(cells < snook_legend_proto::AVATARS.len() + COLUMNS);
    }

    #[test]
    fn the_picker_shows_every_picture_once() {
        for cols in 4..=11 {
            let shown: Vec<&str> = lines(cols)
                .into_iter()
                .flat_map(|l| match l {
                    Line::Cells(c) => c,
                    Line::Divider => vec![],
                })
                .collect();
            assert_eq!(shown.len(), snook_legend_proto::AVATARS.len() + 1);
            assert_eq!(shown[0], "");
            for code in snook_legend_proto::AVATARS {
                assert_eq!(shown.iter().filter(|c| **c == code).count(), 1, "{code}");
            }
        }
    }

    #[test]
    fn only_known_pictures_are_kept() {
        set_mine("pic_cues");
        assert_eq!(mine(), "pic_cues");
        set_mine("no-such-picture");
        assert_eq!(mine(), "pic_cues");
        set_mine("");
        assert_eq!(mine(), "");
    }

    /// A face chosen before the faces were retired becomes the thing of the
    /// game shown in its place (and every retired face has one).
    #[test]
    fn a_retired_face_becomes_a_thing_of_the_game() {
        set_mine("zuri");
        assert_eq!(mine(), snook_legend_proto::shown_avatar("zuri"));
        assert!(mine().starts_with("pic_"));
        for (face, shown) in snook_legend_proto::RETIRED_AVATARS {
            assert!(!snook_legend_proto::AVATARS.contains(&face), "{face}");
            assert!(snook_legend_proto::AVATARS.contains(&shown), "{face} -> {shown}");
        }
    }
}
