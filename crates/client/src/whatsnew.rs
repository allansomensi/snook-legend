//! "What's new": the highlights of each version, in the player's language.
//! Shown once when the game starts on a new version, and on demand from the
//! menu (the version label) and the settings.
//!
//! The notes live in `whatsnew/<language>.md`: `## <version>` starts a
//! version (newest first), `### <title>` a highlight, and the lines after it
//! are its text. Lines starting with a single `#` are comments.
//!
//! The same dialog shows the rules of the game, from
//! `rules/<rules>/<language>.md` (`## <part>`, then `### <rule>`): the
//! Brazilian rules and the official ones, a tab each.

use std::sync::OnceLock;

use macroquad::prelude::*;

#[allow(unused_imports)]
use crate::paint::{draw_line, draw_rectangle};

use snook_legend_core::Rules;

use crate::i18n::{Lang, t, tw};
use crate::ui::{BORDER, CREAM, GOLD, MUTED, Ui};
use crate::update::{VERSION, newer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notes {
    pub version: String,
    pub items: Vec<Item>,
}

fn source(lang: Lang) -> &'static str {
    match lang {
        Lang::English => include_str!("../whatsnew/en.md"),
        Lang::Portuguese => include_str!("../whatsnew/pt-BR.md"),
        Lang::Spanish => include_str!("../whatsnew/es.md"),
    }
}

pub fn parse(text: &str) -> Vec<Notes> {
    let mut out: Vec<Notes> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("## ") {
            out.push(Notes { version: v.trim().to_string(), items: Vec::new() });
        } else if let Some(title) = line.strip_prefix("### ") {
            if let Some(n) = out.last_mut() {
                n.items.push(Item { title: title.trim().to_string(), text: String::new() });
            }
        } else if line.is_empty() || line.starts_with('#') {
            continue;
        } else if let Some(item) = out.last_mut().and_then(|n| n.items.last_mut()) {
            if !item.text.is_empty() {
                item.text.push(' ');
            }
            item.text.push_str(line);
        }
    }
    out
}

fn rules_source(rules: Rules, lang: Lang) -> &'static str {
    match (rules, lang) {
        (Rules::Brazilian, Lang::English) => include_str!("../rules/brazilian/en.md"),
        (Rules::Brazilian, Lang::Portuguese) => include_str!("../rules/brazilian/pt-BR.md"),
        (Rules::Brazilian, Lang::Spanish) => include_str!("../rules/brazilian/es.md"),
        (Rules::Official, Lang::English) => include_str!("../rules/official/en.md"),
        (Rules::Official, Lang::Portuguese) => include_str!("../rules/official/pt-BR.md"),
        (Rules::Official, Lang::Spanish) => include_str!("../rules/official/es.md"),
    }
}

/// The `rules` in `lang`, part by part.
fn rules_book(rules: Rules, lang: Lang) -> &'static [Notes] {
    static CACHE: [OnceLock<Vec<Notes>>; 6] = [const { OnceLock::new() }; 6];
    CACHE[rules.index() * 3 + lang as usize].get_or_init(|| parse(rules_source(rules, lang)))
}

/// Every version's notes in `lang`, newest first.
fn notes(lang: Lang) -> &'static [Notes] {
    static CACHE: [OnceLock<Vec<Notes>>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    CACHE[lang as usize].get_or_init(|| parse(source(lang)))
}

/// One version's notes in the player's language (English if missing).
fn version_notes(version: &str) -> Option<&'static Notes> {
    let find = |lang| notes(lang).iter().find(|n| n.version == version);
    find(crate::i18n::lang()).or_else(|| find(Lang::English))
}

/// The versions to tell a player about who last saw the notes of `seen`
/// ("" for a settings file from before the notes: only this version's).
fn unseen(seen: &str) -> Vec<String> {
    notes(Lang::English)
        .iter()
        .map(|n| n.version.clone())
        .filter(|v| !newer(v, VERSION))
        .filter(|v| if seen.is_empty() { v == VERSION } else { newer(v, seen) })
        .collect()
}

/// The dialog.
pub struct WhatsNew {
    versions: Vec<String>,
    scroll: f32,
    /// The rules of the game (the tab shown) instead of the notes.
    rules: Option<Rules>,
    /// The text laid out (blocks and height), for the width, the language
    /// and the interface scale it was laid out at: wrapping every version's
    /// notes again each frame was the dialog's whole cost.
    layout: Option<(LayoutKey, Vec<Block>, f32)>,
}

/// What a layout was made for: the width, the interface scale (as bits),
/// the language and the rules shown.
type LayoutKey = (u32, u32, crate::i18n::Lang, Option<Rules>);

/// A run of lines: (top, lines, size, colour, bold).
type Block = (f32, Vec<String>, f32, Color, bool);

impl WhatsNew {
    /// After an update: the notes the player hasn't seen, if any.
    pub fn after_update(seen: &str) -> Option<WhatsNew> {
        let versions = unseen(seen);
        (!versions.is_empty()).then_some(WhatsNew {
            versions,
            scroll: 0.0,
            rules: None,
            layout: None,
        })
    }

    /// From the menu or the settings: this version's notes and the ones
    /// before.
    pub fn all() -> WhatsNew {
        let versions = notes(Lang::English)
            .iter()
            .map(|n| n.version.clone())
            .filter(|v| !newer(v, VERSION))
            .collect();
        WhatsNew { versions, scroll: 0.0, rules: None, layout: None }
    }

    /// One version's notes (development aid: SNOOK_LEGEND_WHATS_NEW=0.2.0
    /// shows the notes written for the next version).
    pub fn of(version: &str) -> Option<WhatsNew> {
        version_notes(version).map(|n| WhatsNew {
            versions: vec![n.version.clone()],
            scroll: 0.0,
            rules: None,
            layout: None,
        })
    }

    /// The rules of the game, in the player's language, open on `rules`
    /// (the other's tab beside it).
    pub fn rules(rules: Rules) -> WhatsNew {
        WhatsNew { versions: Vec::new(), scroll: 0.0, rules: Some(rules), layout: None }
    }

    /// Lays the notes (or the rules) out `inner` wide: the blocks of text and
    /// the height they take.
    fn lay_out(&self, ui: &Ui, inner: f32, several: bool) -> (Vec<Block>, f32) {
        let shown: Vec<&Notes> = match self.rules {
            Some(r) => rules_book(r, crate::i18n::lang()).iter().collect(),
            None => self.versions.iter().filter_map(|v| version_notes(v)).collect(),
        };
        let mut blocks: Vec<Block> = Vec::new();
        let mut y = 0.0;
        for (k, n) in shown.iter().enumerate() {
            if several {
                if k > 0 {
                    y += 10.0;
                }
                let title = if self.rules.is_some() {
                    n.version.clone()
                } else {
                    tw("Version {v}", &[("v", &n.version)])
                };
                blocks.push((y, vec![title], 17.0, GOLD, true));
                y += 30.0;
            }
            for item in &n.items {
                let title = ui.wrap(&item.title, inner - 18.0, 16.0);
                blocks.push((y, title.clone(), 16.0, CREAM, true));
                y += title.len() as f32 * 21.0 + 2.0;
                let text = ui.wrap(&item.text, inner - 18.0, 14.0);
                blocks.push((y, text.clone(), 14.0, MUTED, false));
                y += text.len() as f32 * 19.0 + 14.0;
            }
        }
        (blocks, y)
    }

    /// Draws the dialog over everything; returns true when it is closed
    /// (the button, Esc or Enter).
    pub fn draw(&mut self, ui: &mut Ui) -> bool {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.55));
        const PAD: f32 = 28.0;
        // The rules have their tabs under the title.
        let header = if self.rules.is_some() { 76.0 + 58.0 } else { 76.0 };
        const FOOTER: f32 = 92.0;
        let w = 640.0f32.min(sw - 16.0);
        let inner = w - 2.0 * PAD;
        let several = self.versions.len() > 1 || self.rules.is_some();
        let shown: Vec<&Notes> = match self.rules {
            Some(r) => rules_book(r, crate::i18n::lang()).iter().collect(),
            None => self.versions.iter().filter_map(|v| version_notes(v)).collect(),
        };
        let key =
            (inner.to_bits(), crate::ui::ui_scale().to_bits(), crate::i18n::lang(), self.rules);
        let (blocks, y) = match self.layout.take() {
            Some((k, blocks, y)) if k == key => (blocks, y),
            _ => self.lay_out(ui, inner, several),
        };
        self.layout = Some((key, blocks.clone(), y));
        let content = (y - 14.0).max(0.0);
        let room = (sh - 16.0 - header - FOOTER).max(80.0);
        // The rules keep their height from one tab to the other.
        let body_h = if self.rules.is_some() { room.min(560.0) } else { content.min(room) };
        let h = header + body_h + FOOTER;
        let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
        ui.panel(r);

        crate::icons::draw(
            if self.rules.is_some() { crate::icons::Icon::Book } else { crate::icons::Icon::Star },
            vec2(r.x + PAD + 12.0, r.y + 36.0),
            24.0,
            GOLD,
        );
        let title = if self.rules.is_some() {
            t("Rules of the game").to_string()
        } else if several || shown.is_empty() {
            t("What's new").to_string()
        } else {
            tw("What's new in version {v}", &[("v", &shown[0].version)])
        };
        let (title, size) = ui.fitted(&title, 24.0, inner - 36.0, true);
        ui.text(&title, r.x + PAD + 34.0, r.y + 24.0, size, GOLD, true);
        draw_line(r.x + PAD, r.y + 64.0, r.x + r.w - PAD, r.y + 64.0, 1.0, BORDER);
        // A tab for each of the rules.
        if let Some(shown_rules) = self.rules
            && let Some(other) = crate::rulesui::switch(
                ui,
                Rect::new(r.x + PAD, r.y + 76.0, inner, 44.0),
                shown_rules,
                None,
            )
        {
            self.rules = Some(other);
            self.scroll = 0.0;
            self.layout = None;
        }

        let body = Rect::new(r.x + PAD, r.y + header, inner, body_h);
        let max_scroll = (content - body_h).max(0.0);
        if max_scroll > 0.0 && ui.hovering(body) {
            let (_, wheel) = mouse_wheel();
            if wheel != 0.0 {
                self.scroll -= wheel.signum() * 48.0;
            }
        }
        if max_scroll > 0.0 {
            if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::PageDown) {
                self.scroll += if is_key_pressed(KeyCode::PageDown) { body_h * 0.8 } else { 40.0 };
            }
            if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::PageUp) {
                self.scroll -= if is_key_pressed(KeyCode::PageUp) { body_h * 0.8 } else { 40.0 };
            }
        }
        self.scroll = self.scroll.clamp(0.0, max_scroll);
        if max_scroll > 0.0 {
            crate::settings::clip(Some(Rect::new(body.x - 4.0, body.y, body.w + 8.0, body.h)));
        }
        for (by, lines, size, color, bold) in &blocks {
            let x = if *size == 16.0 || *size == 14.0 { body.x + 18.0 } else { body.x };
            for (i, line) in lines.iter().enumerate() {
                let ly = body.y + by - self.scroll + i as f32 * (size + 5.0);
                if ly + size < body.y || ly > body.y + body.h {
                    continue;
                }
                // A gold dot before each highlight.
                if *size == 16.0 && i == 0 {
                    crate::paint::draw_circle(body.x + 5.0, ly + 9.0, 3.5, GOLD);
                }
                ui.text(line, x, ly, *size, *color, *bold);
            }
        }
        if max_scroll > 0.0 {
            crate::settings::clip(None);
            let track = Rect::new(r.x + r.w - 15.0, body.y, 4.0, body.h);
            crate::ui::rrect(track, 2.0, Color::new(1.0, 1.0, 1.0, 0.06));
            let bar = (track.h * body.h / content).max(20.0);
            let at = track.y + (track.h - bar) * self.scroll / max_scroll;
            crate::ui::rrect(
                Rect::new(track.x, at, 4.0, bar),
                2.0,
                Color::new(GOLD.r, GOLD.g, GOLD.b, 0.7),
            );
        }
        if shown.is_empty() {
            ui.text_centered(
                t("No notes for this version."),
                sw / 2.0,
                body.y + 8.0,
                14.0,
                MUTED,
                false,
            );
        }

        let bw = 180.0f32.min(inner);
        let button = Rect::new(r.x + r.w - PAD - bw, r.y + r.h - PAD - 40.0, bw, 40.0);
        ui.primary_button(button, if self.rules.is_some() { t("Close") } else { t("Got it") })
            || is_key_pressed(KeyCode::Escape)
            || is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::KpEnter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_notes_for_this_version() {
        let en = notes(Lang::English);
        assert!(
            en.iter().any(|n| n.version == VERSION),
            "whatsnew/en.md has no section for {VERSION}"
        );
        for lang in Lang::ALL {
            let n = notes(lang);
            let versions: Vec<&str> = n.iter().map(|n| n.version.as_str()).collect();
            let english: Vec<&str> = en.iter().map(|n| n.version.as_str()).collect();
            assert_eq!(versions, english, "{lang:?}: the same versions, in the same order");
            for (a, b) in n.iter().zip(en) {
                assert_eq!(
                    a.items.len(),
                    b.items.len(),
                    "{lang:?} {}: as many highlights",
                    a.version
                );
                assert!(!a.items.is_empty(), "{lang:?} {}: no highlights", a.version);
                for item in &a.items {
                    assert!(!item.title.is_empty() && !item.text.is_empty(), "{lang:?}: {item:?}");
                    assert!(item.text.len() < 400, "{lang:?}: keep it short: {}", item.title);
                }
            }
            // Newest first.
            for pair in versions.windows(2) {
                assert!(newer(pair[0], pair[1]), "{lang:?}: {} before {}", pair[0], pair[1]);
            }
        }
    }

    #[test]
    fn every_language_has_the_same_rules() {
        for book in Rules::ALL {
            the_same_rules(book);
        }
    }

    fn the_same_rules(book: Rules) {
        let en = rules_book(book, Lang::English);
        assert!(en.len() >= 3);
        for lang in Lang::ALL {
            let r = rules_book(book, lang);
            assert_eq!(r.len(), en.len(), "{lang:?}: as many parts");
            for (a, b) in r.iter().zip(en) {
                assert_eq!(a.items.len(), b.items.len(), "{lang:?} {}: as many rules", a.version);
                for item in &a.items {
                    assert!(!item.title.is_empty() && !item.text.is_empty(), "{lang:?}: {item:?}");
                }
            }
        }
    }

    #[test]
    fn parses_and_picks_the_unseen_versions() {
        let n = parse(
            "# comment\n## 0.3.0\n### A\nOne\ntwo.\n\n### B\nThree.\n## 0.2.0\n### C\nFour.\n",
        );
        assert_eq!(n.len(), 2);
        assert_eq!(n[0].items[0], Item { title: "A".into(), text: "One two.".into() });
        assert_eq!(n[0].items[1].text, "Three.");
        assert_eq!(n[1].version, "0.2.0");
        // This version is in the notes (see the test above).
        assert_eq!(unseen(VERSION), Vec::<String>::new());
        assert_eq!(unseen(""), vec![VERSION.to_string()]);
        // Every version since, newest first, none after this one.
        let since = unseen("0.0.1");
        assert_eq!(since.first().map(String::as_str), Some(VERSION));
        assert!(since.iter().all(|v| !newer(v, VERSION)));
        assert!(WhatsNew::after_update(VERSION).is_none());
        assert!(!WhatsNew::all().versions.is_empty());
    }
}
