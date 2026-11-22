//! The server on its own addresses: the legal pages and the root sent to the
//! game's website, the staff portal on its own address only, and the pages
//! written out for the website.

use snook_legend_server::store::Store;
use snook_legend_server::{Config, Server, serve};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn start(site: Option<&str>, admin: Option<&str>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let config = Config {
        bind: addr,
        site_url: site.map(str::to_string),
        admin_host: admin.map(str::to_string),
        ..Config::default()
    };
    let server = Server::with_store(config, Store::memory()).await;
    tokio::spawn(serve(listener, server));
    addr.to_string()
}

/// GET `path` on `addr` as host `host`: the status and the `Location`.
async fn get(addr: &str, host: &str, path: &str) -> (u16, String) {
    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(format!("GET {path} HTTP/1.0\r\nHost: {host}\r\n\r\n").as_bytes()).await.unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.unwrap();
    let out = String::from_utf8_lossy(&out).to_string();
    let code = out[9..12].parse().unwrap();
    let location = out
        .lines()
        .find_map(|l| l.strip_prefix("location: ").or_else(|| l.strip_prefix("Location: ")))
        .unwrap_or("")
        .trim()
        .to_string();
    (code, location)
}

#[tokio::test]
async fn the_legal_pages_and_the_root_go_to_the_website() {
    let addr = start(Some("https://snooklegend.app"), None).await;
    for path in ["privacidade", "privacy", "privacidad", "termos", "terms", "terminos"] {
        let (code, to) = get(&addr, "api.snooklegend.app", &format!("/{path}")).await;
        assert!(code == 301 || code == 308, "{path}: {code}");
        assert_eq!(to, format!("https://snooklegend.app/{path}"));
    }
    let (code, to) = get(&addr, "api.snooklegend.app", "/").await;
    assert!(code == 301 || code == 308);
    assert_eq!(to, "https://snooklegend.app");
    // What is the API's stays here.
    assert_eq!(get(&addr, "api.snooklegend.app", "/health").await.0, 200);
    assert_eq!(get(&addr, "api.snooklegend.app", "/api/status").await.0, 200);
}

#[tokio::test]
async fn without_a_website_the_server_shows_the_pages() {
    let addr = start(None, None).await;
    assert_eq!(get(&addr, "x", "/privacidade").await.0, 200);
    assert_eq!(get(&addr, "x", "/terms").await.0, 200);
    assert_eq!(get(&addr, "x", "/").await.0, 200);
    assert_eq!(get(&addr, "x", "/admin").await.0, 200);
}

#[tokio::test]
async fn the_portal_answers_on_its_own_address_only() {
    let addr = start(None, Some("admin.snooklegend.app")).await;
    for path in ["/admin", "/admin/", "/admin/app.js", "/admin/api/me"] {
        assert_eq!(get(&addr, "api.snooklegend.app", path).await.0, 404, "{path}");
        assert_eq!(get(&addr, "snook-server.onrender.com", path).await.0, 404, "{path}");
    }
    assert_eq!(get(&addr, "admin.snooklegend.app", "/admin").await.0, 200);
    assert_eq!(get(&addr, "ADMIN.snooklegend.app:443", "/admin/app.js").await.0, 200);
    let (code, to) = get(&addr, "admin.snooklegend.app", "/").await;
    assert!(code == 301 || code == 308);
    assert_eq!(to, "/admin/");
    // The game's own routes don't care.
    assert_eq!(get(&addr, "api.snooklegend.app", "/health").await.0, 200);
}

#[test]
fn the_pages_are_written_for_the_website() {
    let dir = std::env::temp_dir().join(format!("snook-pages-{}", std::process::id()));
    let info = snook_legend_server::pages::LegalInfo {
        controller: "Fulano de Tal".into(),
        contact: "suporte@example.com".into(),
        ..Default::default()
    };
    let files = snook_legend_server::pages::export(&dir, &info).unwrap();
    assert_eq!(files.len(), 6);
    for f in &files {
        let html = std::fs::read_to_string(f).unwrap();
        assert!(html.contains("Fulano de Tal"), "{}", f.display());
        assert!(html.contains("mailto:suporte@example.com"), "{}", f.display());
        assert!(!html.contains("{{"), "{}: a placeholder left", f.display());
    }
    assert!(dir.join("privacidade.html").exists() && dir.join("terminos.html").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
