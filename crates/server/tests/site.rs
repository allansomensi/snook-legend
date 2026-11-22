//! The server on its own addresses: the terms of use, the privacy policy and
//! the root lead to the game's website, `/admin` to the staff portal's own
//! site, and only the portal may call the staff API from a browser.

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
async fn the_old_portal_address_leads_to_the_portal() {
    let addr = start(Config::default()).await;
    assert_eq!(get(&addr, "api.snooklegend.app", "/admin").await.0, 404);
    let config =
        Config { admin_origin: Some("https://admin.snooklegend.app".into()), ..Config::default() };
    let addr = start(config).await;
    for path in ["/admin", "/admin/"] {
        let (code, to) = get(&addr, "api.snooklegend.app", path).await;
        assert!(permanent(code), "{path}");
        assert_eq!(to, "https://admin.snooklegend.app");
    }
    // The old portal's files are gone.
    assert_eq!(get(&addr, "api.snooklegend.app", "/admin/app.js").await.0, 404);
    // The game's own routes don't care.
    assert_eq!(get(&addr, "api.snooklegend.app", "/health").await.0, 200);
}

/// A request with these headers: the status and the response's headers
/// (lower-case names).
async fn request(addr: &str, head: &str) -> (u16, String) {
    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(format!("{head}\r\nHost: api.snooklegend.app\r\n\r\n").as_bytes()).await.unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.unwrap();
    let out = String::from_utf8_lossy(&out).to_string();
    let code = out[9..12].parse().unwrap();
    let head = out.split("\r\n\r\n").next().unwrap_or("").to_ascii_lowercase();
    (code, head)
}

#[tokio::test]
async fn only_the_portal_may_call_the_staff_api_from_a_browser() {
    let config =
        Config { admin_origin: Some("https://admin.snooklegend.app".into()), ..Config::default() };
    let addr = start(config).await;
    // The browser asks first, for the portal's origin: yes.
    let (code, head) = request(
        &addr,
        "OPTIONS /admin/api/overview HTTP/1.0\r\nOrigin: https://admin.snooklegend.app\r\n\
         Access-Control-Request-Method: GET\r\nAccess-Control-Request-Headers: authorization",
    )
    .await;
    assert_eq!(code, 204, "{head}");
    assert!(head.contains("access-control-allow-origin: https://admin.snooklegend.app"), "{head}");
    assert!(head.contains("access-control-allow-headers: authorization, content-type"), "{head}");
    assert!(!head.contains("access-control-allow-credentials"), "{head}");
    // For any other site: no.
    let (code, head) = request(
        &addr,
        "OPTIONS /admin/api/login HTTP/1.0\r\nOrigin: https://evil.example\r\nAccess-Control-Request-Method: POST",
    )
    .await;
    assert_eq!(code, 403);
    assert!(!head.contains("access-control-allow-origin"), "{head}");
    // The answers themselves name the portal only.
    let (code, head) =
        request(&addr, "GET /admin/api/overview HTTP/1.0\r\nOrigin: https://admin.snooklegend.app")
            .await;
    assert_eq!(code, 401);
    assert!(head.contains("access-control-allow-origin: https://admin.snooklegend.app"), "{head}");
    let (_, head) =
        request(&addr, "GET /admin/api/overview HTTP/1.0\r\nOrigin: https://evil.example").await;
    assert!(!head.contains("access-control-allow-origin"), "{head}");
}

#[tokio::test]
async fn without_a_portal_no_browser_may_call_the_staff_api() {
    let addr = start(Config::default()).await;
    let (code, head) = request(
        &addr,
        "OPTIONS /admin/api/login HTTP/1.0\r\nOrigin: https://admin.snooklegend.app\r\nAccess-Control-Request-Method: POST",
    )
    .await;
    assert_eq!(code, 403);
    assert!(!head.contains("access-control-allow-origin"), "{head}");
}
