//! The terms of use and the privacy policy: the server publishes them (see
//! `crates/server/src/pages.rs`), in the player's language, and the game
//! links to them where they matter (signing up, buying, the settings).
//! Also the page where a forgotten password is replaced (signing in).

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

/// The page's address for a player of `server`, in the game's language: on
/// the game's website for the public server; another server's own address
/// for it (`wss://host/ws` → `https://host/termos`), which says where its
/// pages are.
pub fn url(server: &str, page: Page) -> Option<String> {
    url_in(server, page, lang())
}

fn url_in(server: &str, page: Page, lang: Lang) -> Option<String> {
    let server = server.trim();
    // The website has them in Portuguese, Spanish and English (the legal
    // texts are not machine-translated): English for the other languages.
    let (pt, es) =
        (matches!(lang, Lang::Portuguese | Lang::EuropeanPortuguese), lang == Lang::Spanish);
    if crate::config::same_server(server, crate::config::OFFICIAL_SERVER) {
        let path = match (page, pt, es) {
            (Page::Terms, true, _) => "/pt/termos/",
            (Page::Terms, _, true) => "/es/terminos/",
            (Page::Terms, ..) => "/terms/",
            (Page::Privacy, true, _) => "/pt/privacidade/",
            (Page::Privacy, _, true) => "/es/privacidad/",
            (Page::Privacy, ..) => "/privacy/",
        };
        return Some(format!("{}{path}", crate::config::SITE_URL));
    }
    let site = site_of(server)?;
    let path = match (page, pt, es) {
        (Page::Terms, true, _) => "termos",
        (Page::Terms, _, true) => "terminos",
        (Page::Terms, ..) => "terms",
        (Page::Privacy, true, _) => "privacidade",
        (Page::Privacy, _, true) => "privacidad",
        (Page::Privacy, ..) => "privacy",
    };
    Some(format!("{site}/{path}"))
}

/// A server's web address from its game address (`wss://host/ws` →
/// `https://host`, `ws://` → `http://`).
fn site_of(server: &str) -> Option<String> {
    let (scheme, rest) = if let Some(rest) = server.strip_prefix("wss://") {
        ("https", rest)
    } else {
        ("http", server.strip_prefix("ws://")?)
    };
    let host = rest.split('/').next().filter(|h| !h.is_empty())?;
    Some(format!("{scheme}://{host}"))
}

/// Opens the page in the browser (only from a server with `https`).
pub fn open(server: &str, page: Page) -> bool {
    url(server, page).is_some_and(|u| crate::util::open_url(&u))
}

/// Where a player of `server` who forgot their password asks for a link to
/// choose a new one (sent to the account's recovery e-mail), in the game's
/// language: the game's website for the public server; another server's
/// own page (`wss://host/ws` → `https://host/forgot-password?lang=de`),
/// which leads to its website's.
pub fn forgot_password_url(server: &str) -> Option<String> {
    forgot_password_url_in(server, lang())
}

fn forgot_password_url_in(server: &str, lang: Lang) -> Option<String> {
    let server = server.trim();
    if crate::config::same_server(server, crate::config::OFFICIAL_SERVER) {
        // The website's page is in Portuguese, Spanish and English.
        let path = match lang {
            Lang::Portuguese | Lang::EuropeanPortuguese => "/pt/redefinir-senha/",
            Lang::Spanish => "/es/restablecer-contrasena/",
            _ => "/reset-password/",
        };
        return Some(format!("{}{path}", crate::config::SITE_URL));
    }
    Some(format!("{}/forgot-password?lang={}", site_of(server)?, lang.code()))
}

/// Opens the page for a forgotten password in the browser (only from a
/// server with `https`).
pub fn open_forgot_password(server: &str) -> bool {
    forgot_password_url(server).is_some_and(|u| crate::util::open_url(&u))
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

    /// The public server's pages are on the game's website, in the player's
    /// language, from either of its addresses.
    #[test]
    fn the_public_servers_pages_are_on_the_website() {
        for s in [crate::config::OFFICIAL_SERVER, crate::config::LEGACY_SERVER] {
            let at = |page, lang| url_in(s, page, lang).unwrap();
            assert_eq!(
                at(Page::Privacy, Lang::Portuguese),
                "https://snooklegend.app/pt/privacidade/"
            );
            assert_eq!(
                at(Page::Terms, Lang::EuropeanPortuguese),
                "https://snooklegend.app/pt/termos/"
            );
            assert_eq!(at(Page::Terms, Lang::Spanish), "https://snooklegend.app/es/terminos/");
            assert_eq!(at(Page::Privacy, Lang::German), "https://snooklegend.app/privacy/");
        }
    }

    /// A forgotten password: the website's page for the public server (in
    /// Portuguese, Spanish or English), another server's own page with the
    /// game's language.
    #[test]
    fn a_forgotten_password_is_asked_for_on_the_right_page() {
        for s in [crate::config::OFFICIAL_SERVER, crate::config::LEGACY_SERVER] {
            let at = |lang| forgot_password_url_in(s, lang).unwrap();
            assert_eq!(at(Lang::English), "https://snooklegend.app/reset-password/");
            assert_eq!(at(Lang::Portuguese), "https://snooklegend.app/pt/redefinir-senha/");
            assert_eq!(at(Lang::EuropeanPortuguese), "https://snooklegend.app/pt/redefinir-senha/");
            assert_eq!(at(Lang::Spanish), "https://snooklegend.app/es/restablecer-contrasena/");
            assert_eq!(at(Lang::Japanese), "https://snooklegend.app/reset-password/");
        }
        assert_eq!(
            forgot_password_url_in("wss://snook.example.com/ws", Lang::Portuguese).as_deref(),
            Some("https://snook.example.com/forgot-password?lang=pt-BR")
        );
        assert_eq!(
            forgot_password_url_in(" ws://127.0.0.1:7878/ws ", Lang::German).as_deref(),
            Some("http://127.0.0.1:7878/forgot-password?lang=de")
        );
        assert_eq!(forgot_password_url_in("https://example.com", Lang::English), None);
        assert_eq!(forgot_password_url_in("wss:///ws", Lang::English), None);
    }
}
