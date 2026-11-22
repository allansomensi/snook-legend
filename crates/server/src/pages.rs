//! Public web pages: the terms of use and the privacy policy, in Portuguese
//! (the reference text), English and Spanish.
//!
//! | Page | Português | English | Español |
//! |---|---|---|---|
//! | Terms of use | `/termos` | `/terms` | `/terminos` |
//! | Privacy policy | `/privacidade` | `/privacy` | `/privacidad` |
//!
//! Stores (Microsoft Store), the payment provider (Stripe) and the game link
//! to them; see `docs/MSSTORE.md`. Who provides the game is filled in from
//! the server's configuration ([`LegalInfo`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::{Html, IntoResponse, Redirect, Response};

use crate::Server;

const PRIVACY_PT: &str = include_str!("../pages/privacy.pt-BR.html");
const PRIVACY_EN: &str = include_str!("../pages/privacy.en.html");
const PRIVACY_ES: &str = include_str!("../pages/privacy.es.html");
const TERMS_PT: &str = include_str!("../pages/terms.pt-BR.html");
const TERMS_EN: &str = include_str!("../pages/terms.en.html");
const TERMS_ES: &str = include_str!("../pages/terms.es.html");

/// The date shown at the top of the privacy policy: change it with the text.
pub const PRIVACY_UPDATED: &str = "2026-11-19";
/// The date shown at the top of the terms of use: change it with the text.
pub const TERMS_UPDATED: &str = "2026-11-19";

/// The pages' look (the game's colours), shared by every page.
const STYLE: &str = "
  body { background: #140d08; color: #efe3cc; font: 16px/1.6 system-ui, sans-serif; margin: 0; padding: 32px 16px; }
  main { max-width: 760px; margin: 0 auto; }
  h1, h2, h3 { color: #e8bf61; line-height: 1.25; }
  h2 { margin-top: 2em; font-size: 1.2em; }
  h3 { margin-top: 1.5em; font-size: 1.05em; }
  a { color: #e8bf61; }
  .muted { color: #b9a98c; }
  li { margin: .3em 0; }
  .box { background: rgba(232, 191, 97, .08); border: 1px solid rgba(232, 191, 97, .35); border-radius: 8px; padding: 4px 18px; margin: 1.5em 0; }
  .toc ol { columns: 2; column-gap: 32px; padding-left: 1.4em; }
  .toc li { margin: 0; padding: .15em 0; break-inside: avoid; }
  @media (max-width: 600px) { .toc ol { columns: 1; } }
";

/// Who provides the game, as the pages name them (from `SNOOK_LEGEND_GAME_NAME`,
/// `SNOOK_LEGEND_PRIVACY_CONTROLLER`, `SNOOK_LEGEND_LEGAL_DOCUMENT`,
/// `SNOOK_LEGEND_LEGAL_ADDRESS` and `SNOOK_LEGEND_PRIVACY_CONTACT`).
///
/// Brazilian law on online sales (Decreto 7.962/2013, art. 2) asks a seller
/// to show their name, CPF or CNPJ, address and an electronic contact: set
/// them all when the server sells anything. An empty document or address
/// leaves its line out of the pages.
#[derive(Debug, Clone)]
pub struct LegalInfo {
    /// The game's name in the store.
    pub game: String,
    /// The person or company who provides the game and controls the data.
    pub controller: String,
    /// Their CPF or CNPJ (empty: not shown).
    pub document: String,
    /// Their address (empty: not shown).
    pub address: String,
    /// An e-mail address or a web address for customer service, refunds,
    /// appeals and privacy requests.
    pub contact: String,
}

impl Default for LegalInfo {
    fn default() -> Self {
        LegalInfo {
            game: snook_legend_proto::GAME_NAME.into(),
            controller: "Allan Somensi".into(),
            document: String::new(),
            address: String::new(),
            contact: format!("{}/issues", env!("CARGO_PKG_REPOSITORY")),
        }
    }
}

impl LegalInfo {
    /// From `SNOOK_LEGEND_GAME_NAME`, `SNOOK_LEGEND_PRIVACY_CONTROLLER`,
    /// `SNOOK_LEGEND_LEGAL_DOCUMENT`, `SNOOK_LEGEND_LEGAL_ADDRESS` and
    /// `SNOOK_LEGEND_PRIVACY_CONTACT` (the defaults for those not set).
    pub fn from_env() -> LegalInfo {
        let env = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        let mut p = LegalInfo::default();
        if let Some(v) = env("SNOOK_LEGEND_GAME_NAME") {
            p.game = v;
        }
        if let Some(v) = env("SNOOK_LEGEND_PRIVACY_CONTROLLER") {
            p.controller = v;
        }
        if let Some(v) = env("SNOOK_LEGEND_LEGAL_DOCUMENT") {
            p.document = v;
        }
        if let Some(v) = env("SNOOK_LEGEND_LEGAL_ADDRESS") {
            p.address = v;
        }
        if let Some(v) = env("SNOOK_LEGEND_PRIVACY_CONTACT") {
            p.contact = v;
        }
        p
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// A contact or address as a link: `mailto:` for an e-mail address, the
/// address itself for a web address, plain text for anything else.
fn link(contact: &str) -> String {
    let c = contact.trim();
    let href = if c.contains('@') && !c.contains("://") && !c.contains(char::is_whitespace) {
        format!("mailto:{c}")
    } else if c.starts_with("https://") || c.starts_with("http://") {
        c.to_string()
    } else {
        return escape(c);
    };
    format!("<a href=\"{}\">{}</a>", escape(&href), escape(c))
}

/// Takes out the blocks `<!--?KEY-->…<!--/KEY-->` whose value is empty, and
/// only the markers of the others.
fn optional_blocks(html: &str, values: &[(&str, &str)]) -> String {
    let mut out = html.to_string();
    for (key, value) in values {
        let open = format!("<!--?{key}-->");
        let close = format!("<!--/{key}-->");
        while let Some(start) = out.find(&open) {
            let Some(len) = out[start..].find(&close) else { break };
            let end = start + len + close.len();
            if value.trim().is_empty() {
                out.replace_range(start..end, "");
            } else {
                let inner = out[start + open.len()..start + len].to_string();
                out.replace_range(start..end, &inner);
            }
        }
    }
    out
}

/// Fills a page: every `{{NAME}}` in one pass (a value is never read again
/// for placeholders), the values escaped.
pub fn render(template: &str, info: &LegalInfo, updated: &str) -> String {
    let html = optional_blocks(
        template,
        &[("DOCUMENT", info.document.as_str()), ("ADDRESS", info.address.as_str())],
    );
    let mut out = String::with_capacity(html.len() + STYLE.len() + 256);
    let mut rest = html.as_str();
    while let Some(at) = rest.find("{{") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[at..]);
            rest = "";
            break;
        };
        let value = match &after[..end] {
            "GAME" => escape(&info.game),
            "CONTROLLER" => escape(&info.controller),
            "DOCUMENT" => escape(info.document.trim()),
            "ADDRESS" => escape(info.address.trim()),
            "CONTACT" => link(&info.contact),
            "REPO" => link(env!("CARGO_PKG_REPOSITORY")),
            "UPDATED" => updated.to_string(),
            "STYLE" => STYLE.to_string(),
            // Not a placeholder: left as it is.
            other => format!("{{{{{other}}}}}"),
        };
        out.push_str(&value);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

fn page(html: String) -> Response {
    let mut res = Html(html).into_response();
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=3600"));
    res
}

/// The pages, by their address (`/privacidade` …): the template and the
/// date it was last changed.
const PAGES: [(&str, &str, &str); 6] = [
    ("privacidade", PRIVACY_PT, PRIVACY_UPDATED),
    ("privacy", PRIVACY_EN, PRIVACY_UPDATED),
    ("privacidad", PRIVACY_ES, PRIVACY_UPDATED),
    ("termos", TERMS_PT, TERMS_UPDATED),
    ("terms", TERMS_EN, TERMS_UPDATED),
    ("terminos", TERMS_ES, TERMS_UPDATED),
];

/// The page at `/<path>`: on the game's website when it has one (a
/// permanent redirect, so old links keep working), else shown here.
fn serve(server: &Server, path: &str) -> Response {
    if let Some(site) = &server.config.site_url {
        let to = format!("{}/{path}", site.trim_end_matches('/'));
        return Redirect::permanent(&to).into_response();
    }
    let Some((_, template, updated)) = PAGES.iter().find(|(p, ..)| *p == path) else {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    };
    page(render(template, &server.config.privacy, updated))
}

/// Writes the pages as `<path>.html` files in `dir`, filled in with `info`,
/// for the game's website (served there as `/privacidade` …: with Vercel,
/// `"cleanUrls": true`). Returns the files written.
pub fn export(dir: &Path, info: &LegalInfo) -> std::io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let mut written = Vec::new();
    for (path, template, updated) in PAGES {
        let file = dir.join(format!("{path}.html"));
        std::fs::write(&file, render(template, info, updated))?;
        written.push(file);
    }
    Ok(written)
}

pub async fn privacy_pt(State(server): State<Arc<Server>>) -> Response {
    serve(&server, "privacidade")
}

pub async fn privacy_en(State(server): State<Arc<Server>>) -> Response {
    serve(&server, "privacy")
}

pub async fn privacy_es(State(server): State<Arc<Server>>) -> Response {
    serve(&server, "privacidad")
}

pub async fn terms_pt(State(server): State<Arc<Server>>) -> Response {
    serve(&server, "termos")
}

pub async fn terms_en(State(server): State<Arc<Server>>) -> Response {
    serve(&server, "terms")
}

pub async fn terms_es(State(server): State<Arc<Server>>) -> Response {
    serve(&server, "terminos")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [&str; 6] = [PRIVACY_PT, PRIVACY_EN, PRIVACY_ES, TERMS_PT, TERMS_EN, TERMS_ES];

    fn full() -> LegalInfo {
        LegalInfo {
            game: "Pool <Club>".into(),
            controller: "Ana & Bia".into(),
            document: "12.345.678/0001-90".into(),
            address: "Rua A, 1 <b>".into(),
            contact: "privacy@example.com".into(),
        }
    }

    #[test]
    fn pages_fill_every_placeholder_and_escape_values() {
        for template in ALL {
            let html = render(template, &full(), "2026-01-02");
            assert!(!html.contains("{{"), "a placeholder was left");
            assert!(!html.contains("<!--?") && !html.contains("<!--/"), "a marker was left");
            assert!(html.contains("Pool &lt;Club&gt;") && html.contains("Ana &amp; Bia"));
            assert!(html.contains("12.345.678/0001-90") && html.contains("Rua A, 1 &lt;b&gt;"));
            assert!(html.contains("href=\"mailto:privacy@example.com\""));
            assert!(html.contains("2026-01-02"));
            assert!(html.contains("background: #140d08"), "the style");
        }
        assert_eq!(link("https://x.org/a"), "<a href=\"https://x.org/a\">https://x.org/a</a>");
        assert_eq!(link("javascript:alert(1)"), "javascript:alert(1)");
    }

    /// Lines with nothing to show (no document, no address) leave the page.
    #[test]
    fn unset_lines_disappear() {
        let info = LegalInfo { document: String::new(), address: " ".into(), ..full() };
        for template in ALL {
            let html = render(template, &info, "2026-01-02");
            assert!(!html.contains("{{") && !html.contains("<!--"), "left over");
            assert!(!html.contains("CPF/CNPJ:"), "the document's line stayed");
            assert!(
                !html.contains("Endereço:")
                    && !html.contains("Address:")
                    && !html.contains("Dirección:"),
                "the address's line stayed"
            );
            assert!(html.contains("Ana &amp; Bia"), "the provider is always named");
        }
    }

    /// A value is put in once: placeholders written in it stay text.
    #[test]
    fn values_are_not_read_again() {
        let info = LegalInfo { game: "{{CONTACT}}".into(), ..full() };
        let html = render(TERMS_EN, &info, "x");
        assert!(html.contains("{{CONTACT}}"));
        assert!(!html.contains("{{GAME}}"));
    }

    /// Every page links its translations and the other document.
    #[test]
    fn pages_link_each_other() {
        for (template, links) in [
            (PRIVACY_PT, ["/privacy", "/privacidad", "/termos"]),
            (PRIVACY_EN, ["/privacidade", "/privacidad", "/terms"]),
            (PRIVACY_ES, ["/privacidade", "/privacy", "/terminos"]),
            (TERMS_PT, ["/terms", "/terminos", "/privacidade"]),
            (TERMS_EN, ["/termos", "/terminos", "/privacy"]),
            (TERMS_ES, ["/termos", "/terms", "/privacidad"]),
        ] {
            for l in links {
                assert!(template.contains(&format!("href=\"{l}\"")), "{l}");
            }
        }
    }
}
