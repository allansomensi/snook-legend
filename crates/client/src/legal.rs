//! The terms of use and the privacy policy: the server publishes them (see
//! `crates/server/src/pages.rs`), in the player's language, and the game
//! links to them where they matter (signing up, buying, the settings).

use macroquad::prelude::*;

use crate::i18n::{Lang, lang, t};
use crate::paint::draw_line;
use crate::ui::{GOLD, Ui};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Terms,
    Privacy,
}

impl Page {
    pub fn label(self) -> &'static str {
        match self {
            Page::Terms => t("Terms of use"),
            Page::Privacy => t("Privacy policy"),
        }
    }
}

/// The page's address on the server at `server` (`wss://host/ws` →
/// `https://host/termos`), in the game's language.
pub fn url(server: &str, page: Page) -> Option<String> {
    url_in(server, page, lang())
}

fn url_in(server: &str, page: Page, lang: Lang) -> Option<String> {
    let server = server.trim();
    // The public server's pages are on the game's website.
    let base = if crate::config::same_server(server, crate::config::OFFICIAL_SERVER) {
        crate::config::SITE_URL.to_string()
    } else {
        let (scheme, rest) = if let Some(rest) = server.strip_prefix("wss://") {
            ("https", rest)
        } else {
            ("http", server.strip_prefix("ws://")?)
        };
        let host = rest.split('/').next().filter(|h| !h.is_empty())?;
        format!("{scheme}://{host}")
    };
    // The server has them in Portuguese, Spanish and English (the legal
    // texts are not machine-translated): English for the other languages.
    let path = match (page, lang) {
        (Page::Terms, Lang::Portuguese | Lang::EuropeanPortuguese) => "termos",
        (Page::Terms, Lang::Spanish) => "terminos",
        (Page::Terms, _) => "terms",
        (Page::Privacy, Lang::Portuguese | Lang::EuropeanPortuguese) => "privacidade",
        (Page::Privacy, Lang::Spanish) => "privacidad",
        (Page::Privacy, _) => "privacy",
    };
    Some(format!("{base}/{path}"))
}

/// Opens the page in the browser (only from a server with `https`).
pub fn open(server: &str, page: Page) -> bool {
    url(server, page).is_some_and(|u| crate::util::open_url(&u))
}

/// A link: the text in gold, underlined under the pointer. Whether it was
/// clicked; its width.
pub fn link(ui: &mut Ui, label: &str, x: f32, y: f32, size: f32) -> (bool, f32) {
    let w = ui.text_size(label, size, true).x;
    let r = Rect::new(x - 2.0, y - 2.0, w + 4.0, size + 6.0);
    let hover = ui.hovering(r);
    ui.block(r);
    let color = if hover { GOLD } else { Color::new(GOLD.r, GOLD.g, GOLD.b, 0.8) };
    ui.text(label, x, y, size, color, true);
    if hover {
        draw_line(x, y + size + 2.0, x + w, y + size + 2.0, 1.0, color);
    }
    (ui.clicked(r), w)
}

/// "Terms of use · Privacy policy", centred on `cx`; opens the one clicked.
pub fn links(ui: &mut Ui, server: &str, cx: f32, y: f32, size: f32) {
    let w = links_width(ui, size);
    links_at(ui, server, cx - w / 2.0, y, size);
}

fn links_width(ui: &Ui, size: f32) -> f32 {
    let a = ui.text_size(Page::Terms.label(), size, true).x;
    let b = ui.text_size(Page::Privacy.label(), size, true).x;
    a + ui.text_size(" · ", size, false).x + b
}

/// The same links from `x` on.
pub fn links_at(ui: &mut Ui, server: &str, x: f32, y: f32, size: f32) {
    let dot = " · ";
    let d = ui.text_size(dot, size, false).x;
    let mut x = x;
    for (k, page) in [Page::Terms, Page::Privacy].into_iter().enumerate() {
        if k == 1 {
            ui.text(dot, x, y, size, crate::ui::MUTED, false);
            x += d;
        }
        let (clicked, w) = link(ui, page.label(), x, y, size);
        if clicked {
            open(server, page);
        }
        x += w;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_are_next_to_the_game_address_in_each_language() {
        let s = "wss://snook.example.com/ws";
        assert_eq!(
            url_in(s, Page::Terms, Lang::Portuguese).as_deref(),
            Some("https://snook.example.com/termos")
        );
        assert_eq!(
            url_in(s, Page::Privacy, Lang::Spanish).as_deref(),
            Some("https://snook.example.com/privacidad")
        );
        assert_eq!(
            url_in("ws://127.0.0.1:7878/ws", Page::Terms, Lang::English).as_deref(),
            Some("http://127.0.0.1:7878/terms")
        );
        assert_eq!(
            url_in(s, Page::Terms, Lang::Japanese).as_deref(),
            Some("https://snook.example.com/terms")
        );
        assert_eq!(url_in("https://example.com", Page::Terms, Lang::English), None);
    }

    /// The public server's pages are on the game's website, from either of
    /// its addresses.
    #[test]
    fn the_public_servers_pages_are_on_the_website() {
        for s in [crate::config::OFFICIAL_SERVER, crate::config::LEGACY_SERVER] {
            assert_eq!(
                url_in(s, Page::Privacy, Lang::Portuguese).as_deref(),
                Some("https://snooklegend.app/privacidade")
            );
            assert_eq!(
                url_in(s, Page::Terms, Lang::German).as_deref(),
                Some("https://snooklegend.app/terms")
            );
        }
    }
}
