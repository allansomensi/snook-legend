//! The server on its own addresses: the terms of use, the privacy policy and
//! the root lead to the game's website, and the staff portal answers on its
//! own address only.

use snook_legend_server::store::Store;
use snook_legend_server::{Config, Server, serve};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn start(config: Config) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = Server::with_store(Config { bind: addr, ..config }, Store::memory()).await;
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

fn permanent(code: u16) -> bool {
    code == 301 || code == 308
}

#[tokio::test]
async fn the_legal_pages_and_the_root_lead_to_the_website() {
    let addr = start(Config::default()).await;
    for (old, new) in [
        ("/privacidade", "/pt/privacidade/"),
        ("/privacy", "/privacy/"),
        ("/privacidad", "/es/privacidad/"),
        ("/termos", "/pt/termos/"),
        ("/terms", "/terms/"),
        ("/terminos", "/es/terminos/"),
        ("/", "/"),
    ] {
        let (code, to) = get(&addr, "api.snooklegend.app", old).await;
        assert!(permanent(code), "{old}: {code}");
        assert_eq!(to, format!("https://snooklegend.app{new}"), "{old}");
    }
    // What is the API's stays here.
    assert_eq!(get(&addr, "api.snooklegend.app", "/health").await.0, 200);
    assert_eq!(get(&addr, "api.snooklegend.app", "/api/status").await.0, 200);
}

#[tokio::test]
async fn another_website_can_be_set() {
    let config = Config { site_url: "https://pool.example".into(), ..Config::default() };
    let addr = start(config).await;
    let (code, to) = get(&addr, "x", "/termos").await;
    assert!(permanent(code));
    assert_eq!(to, "https://pool.example/pt/termos/");
}

#[tokio::test]
async fn the_portal_answers_anywhere_when_it_has_no_address() {
    let addr = start(Config::default()).await;
    assert_eq!(get(&addr, "snook-server.onrender.com", "/admin").await.0, 200);
}

#[tokio::test]
async fn the_portal_answers_on_its_own_address_only() {
    let config = Config { admin_host: Some("admin.snooklegend.app".into()), ..Config::default() };
    let addr = start(config).await;
    for path in ["/admin", "/admin/", "/admin/app.js", "/admin/api/me"] {
        assert_eq!(get(&addr, "api.snooklegend.app", path).await.0, 404, "{path}");
        assert_eq!(get(&addr, "snook-server.onrender.com", path).await.0, 404, "{path}");
    }
    assert_eq!(get(&addr, "admin.snooklegend.app", "/admin").await.0, 200);
    assert_eq!(get(&addr, "ADMIN.snooklegend.app:443", "/admin/app.js").await.0, 200);
    let (code, to) = get(&addr, "admin.snooklegend.app", "/").await;
    assert!(permanent(code));
    assert_eq!(to, "/admin/");
    // The game's own routes don't care.
    assert_eq!(get(&addr, "api.snooklegend.app", "/health").await.0, 200);
}
