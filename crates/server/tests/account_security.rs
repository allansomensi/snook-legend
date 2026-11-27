//! Accounts kept safe (0.12.3): the recovery e-mail confirmed by a link, a
//! forgotten password chosen again by e-mail (never telling whether an
//! account exists), every session ended by it, and the staff's two-step
//! verification: in the portal (set up on the first sign-in, recovery
//! codes, devices kept signed in for 30 days, an administrator turning a
//! lost phone's off) and in the game.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::http;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use snook_legend_proto::{
    AUTH_CODE_REQUIRED, AUTH_CODE_WRONG, ClientMsg, PROTOCOL_VERSION, Role, Rules, ServerMsg,
    parse_server,
};
use snook_legend_server::lounge::Settings;
use snook_legend_server::mail::MailSetup;
use snook_legend_server::store::{NewUser, Store};
use snook_legend_server::{Config, Server, now, serve, totp};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

const PASSWORD: &str = "correct-horse-1";
const SITE: &str = "https://snooklegend.app";

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Running {
    url: String,
    host: String,
    server: Arc<Server>,
}

/// A server that keeps its e-mails in an outbox, with an administrator
/// ("boss") and a moderator ("mod").
async fn start() -> Running {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let settings = Settings { max_connections_per_ip: 100, ..Settings::default() };
    let config = Config {
        bind: addr,
        settings,
        admins: vec!["boss".into()],
        mail: MailSetup::Outbox,
        admin_origin: Some(SITE.into()),
        ..Config::default()
    };
    let server = Server::with_store(config, Store::memory()).await;
    let hash = server.hash_password(PASSWORD).await.unwrap();
    for (name, role) in [("boss", Role::Player), ("mod", Role::Moderator)] {
        let user = NewUser {
            username: name,
            password_hash: &hash,
            country: "BR",
            rating: 1200,
            role,
            ip: "1.2.3.4",
            minor: Some(false),
        };
        server.store.create_user(&user).await.unwrap().expect("a new account");
    }
    tokio::spawn(serve(listener, server.clone()));
    Running { url: format!("ws://{addr}/ws"), host: addr.to_string(), server }
}

struct Client {
    ws: Ws,
}

impl Client {
    async fn connect(url: &str) -> Client {
        let (ws, _) = connect_async(url).await.unwrap();
        let mut c = Client { ws };
        c.send(ClientMsg::Hello { version: PROTOCOL_VERSION, rules: Rules::Brazilian }).await;
        c
    }

    async fn send(&mut self, msg: ClientMsg) {
        self.ws.send(Message::Text(msg.to_json().into())).await.unwrap();
    }

    async fn expect<F: Fn(&ServerMsg) -> bool>(&mut self, what: &str, pred: F) -> ServerMsg {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            let msg = tokio::time::timeout(left, self.ws.next())
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
                .expect("connection closed")
                .unwrap();
            if let Message::Text(t) = msg {
                let m = parse_server(t.as_str()).unwrap();
                if pred(&m) {
                    return m;
                }
            }
        }
    }

    async fn register(url: &str, name: &str) -> Client {
        let mut c = Client::connect(url).await;
        c.send(ClientMsg::Register {
            username: name.into(),
            password: PASSWORD.into(),
            country: "BR".into(),
            minor: Some(false),
        })
        .await;
        c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
        c
    }

    /// Signs in; the reason when refused.
    async fn try_login(url: &str, name: &str, password: &str) -> Result<Client, String> {
        let mut c = Client::connect(url).await;
        c.send(ClientMsg::Login { username: name.into(), password: password.into() }).await;
        match c
            .expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. } | ServerMsg::AuthError { .. }))
            .await
        {
            ServerMsg::AuthOk { .. } => Ok(c),
            ServerMsg::AuthError { reason } => Err(reason),
            _ => unreachable!(),
        }
    }

    async fn notice(&mut self, part: &str) -> String {
        match self
            .expect(part, |m| matches!(m, ServerMsg::Notice { text } if text.contains(part)))
            .await
        {
            ServerMsg::Notice { text } => text,
            _ => unreachable!(),
        }
    }

    async fn error(&mut self) -> String {
        match self.expect("error", |m| matches!(m, ServerMsg::Error { .. })).await {
            ServerMsg::Error { message } => message,
            _ => unreachable!(),
        }
    }

    /// The account page's state: (recovery e-mail, waiting for its link).
    async fn account(&mut self) -> (Option<String>, bool) {
        match self.expect("account", |m| matches!(m, ServerMsg::Account { .. })).await {
            ServerMsg::Account { recovery_email, recovery_email_unconfirmed, .. } => {
                (recovery_email, recovery_email_unconfirmed)
            }
            _ => unreachable!(),
        }
    }
}

/// The token in the newest e-mail to `to` whose link goes to `path`.
fn token_in_outbox(server: &Server, to: &str, path: &str) -> String {
    let sent = server.mail.outbox();
    let mail = sent
        .iter()
        .rev()
        .find(|m| m.to == to && m.text.contains(path))
        .unwrap_or_else(|| panic!("no e-mail to {to} for {path}: {sent:#?}"));
    let link = format!("{SITE}{path}#token=");
    let at = mail.text.find(&link).unwrap() + link.len();
    mail.text[at..at + 64].to_string()
}

/// A request from the website's pages.
async fn site(host: &str, path: &str, body: Value) -> (u16, Value) {
    http(host, "POST", path, "", &body).await
}

/// Waits until the outbox has `n` e-mails (they are sent in the background).
async fn outbox_reaches(server: &Server, n: usize) {
    for _ in 0..200 {
        if server.mail.outbox().len() >= n {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the outbox has {:#?}", server.mail.outbox());
}

#[tokio::test]
async fn a_forgotten_password_is_chosen_again_by_email() {
    let s = start().await;
    let mut ana = Client::register(&s.url, "ana").await;

    // A new recovery e-mail waits for its link.
    ana.send(ClientMsg::SetRecoveryEmail {
        email: "Ana@Example.com".into(),
        password: PASSWORD.into(),
        lang: "pt-BR".into(),
    })
    .await;
    assert_eq!(ana.account().await, (Some("ana@example.com".into()), true));
    ana.notice("Open the link we sent").await;
    let confirm = token_in_outbox(&s.server, "ana@example.com", "/pt/confirmar-email/");
    assert!(s.server.mail.outbox()[0].subject.contains("Confirme"), "in Portuguese");

    // Until then, a forgotten password sends nothing there.
    let (code, v) = site(&s.host, "/api/account/forgot", json!({ "login": "ana" })).await;
    assert_eq!((code, &v["ok"]), (200, &json!(true)));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(s.server.mail.outbox().len(), 1, "only the confirmation");

    // "Send again" sends a new link; the old one stops working.
    ana.send(ClientMsg::ConfirmEmailAgain { lang: "en".into() }).await;
    ana.notice("We sent the link again").await;
    let confirm2 = token_in_outbox(&s.server, "ana@example.com", "/confirm-email/");
    assert_ne!(confirm, confirm2);
    let (code, v) = site(&s.host, "/api/account/confirm-email", json!({ "token": confirm })).await;
    assert_eq!((code, v["code"].as_str()), (400, Some("invalid_token")));
    let (code, v) = site(&s.host, "/api/account/confirm-email", json!({ "token": confirm2 })).await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["username"], "ana");
    // The game open on the account sees it at once.
    assert_eq!(ana.account().await, (Some("ana@example.com".into()), false));
    ana.send(ClientMsg::ConfirmEmailAgain { lang: "en".into() }).await;
    ana.notice("is confirmed").await;

    // A forgotten password, by name (any case) or by e-mail.
    let before = s.server.mail.outbox().len();
    let (code, _) =
        site(&s.host, "/api/account/forgot", json!({ "login": "ANA", "lang": "es" })).await;
    assert_eq!(code, 200);
    outbox_reaches(&s.server, before + 1).await;
    let reset = token_in_outbox(&s.server, "ana@example.com", "/es/restablecer-contrasena/");
    assert!(s.server.mail.outbox().last().unwrap().subject.contains("contraseña"));

    // A weak password doesn't use the link up; a wrong token changes nothing.
    let (code, v) =
        site(&s.host, "/api/account/reset", json!({ "token": reset, "password": "1234567" })).await;
    assert_eq!((code, v["code"].as_str()), (400, Some("weak_password")));
    let (code, v) = site(
        &s.host,
        "/api/account/reset",
        json!({ "token": "0".repeat(64), "password": "a-new-one-1" }),
    )
    .await;
    assert_eq!((code, v["code"].as_str()), (400, Some("invalid_token")));
    let (code, v) =
        site(&s.host, "/api/account/reset", json!({ "token": reset, "password": "a-new-one-1" }))
            .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["username"], "ana");
    // Signed out of the game at once; the old password no longer works.
    ana.expect("signed out", |m| matches!(m, ServerMsg::Kicked { .. })).await;
    assert!(Client::try_login(&s.url, "ana", PASSWORD).await.is_err());
    Client::try_login(&s.url, "ana", "a-new-one-1").await.expect("the new password");
    // The link works once.
    let (code, _) =
        site(&s.host, "/api/account/reset", json!({ "token": reset, "password": "another-one-2" }))
            .await;
    assert_eq!(code, 400);
    // The address is told its password changed.
    outbox_reaches(&s.server, before + 2).await;
    let told = s.server.mail.outbox().last().unwrap().clone();
    assert_eq!(told.to, "ana@example.com");
    assert!(told.text.contains("ana"));
}

#[tokio::test]
async fn whether_an_account_exists_is_never_told() {
    let s = start().await;
    let (code, v) = site(&s.host, "/api/account/forgot", json!({ "login": "nobody" })).await;
    assert_eq!((code, &v["ok"]), (200, &json!(true)));
    let (code, v) =
        site(&s.host, "/api/account/forgot", json!({ "login": "nobody@example.com" })).await;
    assert_eq!((code, &v["ok"]), (200, &json!(true)));
    // An account without an e-mail answers the same.
    let _bob = Client::register(&s.url, "bob").await;
    let (code, _) = site(&s.host, "/api/account/forgot", json!({ "login": "bob" })).await;
    assert_eq!(code, 200);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(s.server.mail.outbox().is_empty());
    // Empty: a mistake, said so.
    let (code, v) = site(&s.host, "/api/account/forgot", json!({ "login": " " })).await;
    assert_eq!((code, v["code"].as_str()), (400, Some("bad_request")));
    // Asking again and again for one name is stopped.
    let mut last = 0;
    for _ in 0..6 {
        last = site(&s.host, "/api/account/forgot", json!({ "login": "carla" })).await.0;
    }
    assert_eq!(last, 429);
}

#[tokio::test]
async fn one_email_recovers_each_of_its_accounts() {
    let s = start().await;
    for name in ["cris", "cris_2"] {
        let mut c = Client::register(&s.url, name).await;
        c.send(ClientMsg::SetRecoveryEmail {
            email: "cris@example.com".into(),
            password: PASSWORD.into(),
            lang: String::new(),
        })
        .await;
        c.notice("Open the link").await;
        let t = token_in_outbox(&s.server, "cris@example.com", "/confirm-email/");
        let (code, _) = site(&s.host, "/api/account/confirm-email", json!({ "token": t })).await;
        assert_eq!(code, 200);
    }
    let before = s.server.mail.outbox().len();
    site(&s.host, "/api/account/forgot", json!({ "login": "Cris@example.com" })).await;
    outbox_reaches(&s.server, before + 2).await;
    let mut named: Vec<bool> =
        s.server.mail.outbox()[before..].iter().map(|m| m.text.contains("Hi cris_2,")).collect();
    named.sort();
    assert_eq!(named, [false, true], "one e-mail per account");
}

#[tokio::test]
async fn only_the_website_calls_the_account_api_from_a_browser() {
    let s = start().await;
    let preflight = |origin: &str| {
        format!(
            "OPTIONS /api/account/forgot HTTP/1.0\r\nHost: x\r\nOrigin: {origin}\r\nAccess-Control-Request-Method: POST\r\n\r\n"
        )
    };
    for (origin, allowed) in [(SITE, true), ("https://evil.example", false)] {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut sock = TcpStream::connect(&s.host).await.unwrap();
        sock.write_all(preflight(origin).as_bytes()).await.unwrap();
        let mut out = String::new();
        sock.read_to_string(&mut out).await.unwrap();
        let out = out.to_lowercase();
        assert_eq!(out.contains("access-control-allow-origin"), allowed, "{origin}: {out}");
        assert_eq!(out.starts_with("http/1.0 204") || out.starts_with("http/1.1 204"), allowed);
    }
    // Another server's game opens its website's page.
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut sock = TcpStream::connect(&s.host).await.unwrap();
    sock.write_all(b"GET /forgot-password?lang=pt-BR HTTP/1.0\r\nHost: x\r\n\r\n").await.unwrap();
    let mut out = String::new();
    sock.read_to_string(&mut out).await.unwrap();
    assert!(
        out.to_lowercase().contains("location: https://snooklegend.app/pt/redefinir-senha/"),
        "{out}"
    );
}

#[tokio::test]
async fn staff_sign_in_to_the_portal_in_two_steps() {
    let s = start().await;
    // The first time: the secret to add to the app, and a wrong code first.
    let first = common::password_step(&s.host, "mod", PASSWORD, false).await;
    assert_eq!(first["step"], "setup");
    let secret = first["secret"].as_str().unwrap().to_string();
    assert!(
        first["uri"].as_str().unwrap().starts_with("otpauth://totp/Snook%20Legend:mod?secret=")
    );
    let ticket = first["ticket"].as_str().unwrap();
    let (code, v) = http(
        &s.host,
        "POST",
        "/admin/api/login/code",
        "",
        &json!({ "ticket": ticket, "code": "000000" }),
    )
    .await;
    assert_eq!(code, 401, "{v}");
    assert_eq!(v["error"], AUTH_CODE_WRONG);
    // No session without the code.
    let (code, _) = http(&s.host, "GET", "/admin/api/me", "", &Value::Null).await;
    assert_eq!(code, 401);
    let right = totp::code_at(&secret, now()).unwrap();
    let (code, v) = http(
        &s.host,
        "POST",
        "/admin/api/login/code",
        "",
        &json!({ "ticket": ticket, "code": right }),
    )
    .await;
    assert_eq!(code, 200, "{v}");
    let codes: Vec<String> = v["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().into())
        .collect();
    assert_eq!(codes.len(), 10);
    let token = v["token"].as_str().unwrap();
    assert!(v.get("device").is_none(), "not asked to be kept");
    let (code, me) = http(&s.host, "GET", "/admin/api/me", token, &Value::Null).await;
    assert_eq!((code, me["username"].as_str()), (200, Some("mod")));

    // From now on, the code of the app is asked; the same code never twice.
    let next = common::password_step(&s.host, "mod", PASSWORD, true).await;
    assert_eq!(next["step"], "code");
    assert!(next.get("secret").is_none());
    let ticket = next["ticket"].as_str().unwrap();
    let (code, _) = http(
        &s.host,
        "POST",
        "/admin/api/login/code",
        "",
        &json!({ "ticket": ticket, "code": right }),
    )
    .await;
    assert_eq!(code, 401, "a code is used once");
    // A recovery code works (once), and keeps this device signed in.
    let (code, v) = http(
        &s.host,
        "POST",
        "/admin/api/login/code",
        "",
        &json!({ "ticket": ticket, "code": codes[0].to_uppercase() }),
    )
    .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["codes_left"], 9);
    let device = v["device"].as_str().unwrap().to_string();
    let device_id = v["device_id"].as_str().unwrap().to_string();
    let (code, _) = http(
        &s.host,
        "POST",
        "/admin/api/login/code",
        "",
        &json!({ "ticket": ticket, "code": codes[1] }),
    )
    .await;
    assert_eq!(code, 401, "a ticket is used once");

    // The device trades its token for a session, without password or code.
    let (code, v) =
        http(&s.host, "POST", "/admin/api/login/device", "", &json!({ "device": device })).await;
    assert_eq!(code, 200, "{v}");
    let session = v["token"].as_str().unwrap().to_string();
    assert_eq!(v["device_id"], device_id.as_str());
    let (code, sec) = http(&s.host, "GET", "/admin/api/security", &session, &Value::Null).await;
    assert_eq!(code, 200, "{sec}");
    assert_eq!(sec["recovery_codes_left"], 9);
    assert!(sec["two_step_at"].as_i64().is_some());
    let devices = sec["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0]["id"], device_id.as_str());
    assert_eq!(devices[0]["label"], "Chrome · Android");
    assert_eq!(sec["device_days"], 30);

    // New recovery codes need a code of the app (not a recovery code).
    let (code, _) = http(
        &s.host,
        "POST",
        "/admin/api/security/recovery-codes",
        &session,
        &json!({ "code": codes[2] }),
    )
    .await;
    assert_eq!(code, 400);
    let fresh = totp::code_at(&secret, now() + totp::STEP).unwrap();
    let (code, v) = http(
        &s.host,
        "POST",
        "/admin/api/security/recovery-codes",
        &session,
        &json!({ "code": fresh }),
    )
    .await;
    assert_eq!(code, 200, "{v}");
    assert_eq!(v["recovery_codes"].as_array().unwrap().len(), 10);

    // Signing the device out: it asks for the password again.
    let (code, _) = http(
        &s.host,
        "DELETE",
        &format!("/admin/api/security/devices/{device_id}"),
        &session,
        &Value::Null,
    )
    .await;
    assert_eq!(code, 200);
    let (code, _) =
        http(&s.host, "POST", "/admin/api/login/device", "", &json!({ "device": device })).await;
    assert_eq!(code, 401);

    // Everywhere: this session too.
    let (code, _) =
        http(&s.host, "POST", "/admin/api/security/sign-out-everywhere", &session, &Value::Null)
            .await;
    assert_eq!(code, 200);
    let (code, _) = http(&s.host, "GET", "/admin/api/me", &session, &Value::Null).await;
    assert_eq!(code, 401);
    let (code, _) = http(&s.host, "GET", "/admin/api/me", token, &Value::Null).await;
    assert_eq!(code, 401);
}

#[tokio::test]
async fn players_and_wrong_passwords_never_reach_the_code() {
    let s = start().await;
    let _eve = Client::register(&s.url, "eve").await;
    let (code, v) = http(
        &s.host,
        "POST",
        "/admin/api/login",
        "",
        &json!({ "username": "eve", "password": PASSWORD }),
    )
    .await;
    assert_eq!(code, 403, "{v}");
    assert!(v.get("ticket").is_none());
    let (code, v) = http(
        &s.host,
        "POST",
        "/admin/api/login",
        "",
        &json!({ "username": "mod", "password": "wrong-password" }),
    )
    .await;
    assert_eq!(code, 401, "{v}");
    assert!(v.get("ticket").is_none());
    let (code, _) = http(
        &s.host,
        "POST",
        "/admin/api/login/code",
        "",
        &json!({ "ticket": "x", "code": "123456" }),
    )
    .await;
    assert_eq!(code, 401);
}

#[tokio::test]
async fn a_lost_phone_is_turned_off_by_an_administrator() {
    let s = start().await;
    let boss = common::staff_token(&s.host, "boss", PASSWORD).await;
    let remembered = common::staff_login(&s.host, "mod", PASSWORD, true).await;
    let device = remembered["device"].as_str().unwrap().to_string();
    // The player's page says two-step verification is on.
    let (code, v) = http(&s.host, "GET", "/admin/api/users/mod", &boss, &Value::Null).await;
    assert_eq!(code, 200);
    assert!(v["two_step_at"].as_i64().is_some());
    // Only administrators turn it off.
    let modt = remembered["token"].as_str().unwrap();
    let (code, _) =
        http(&s.host, "POST", "/admin/api/users/boss/two-step/reset", modt, &Value::Null).await;
    assert_eq!(code, 403);
    let (code, v) =
        http(&s.host, "POST", "/admin/api/users/mod/two-step/reset", &boss, &Value::Null).await;
    assert_eq!(code, 200, "{v}");
    // Signed out of the portal everywhere, the device too; set up again next time.
    let (code, _) = http(&s.host, "GET", "/admin/api/me", modt, &Value::Null).await;
    assert_eq!(code, 401);
    let (code, _) =
        http(&s.host, "POST", "/admin/api/login/device", "", &json!({ "device": device })).await;
    assert_eq!(code, 401);
    let again = common::password_step(&s.host, "mod", PASSWORD, false).await;
    assert_eq!(again["step"], "setup");
    // Recorded in the audit log.
    let (_, audit) = http(&s.host, "GET", "/admin/api/audit", &boss, &Value::Null).await;
    assert!(audit.to_string().contains("two-step-reset"), "{audit}");
}

#[tokio::test]
async fn staff_sign_in_to_the_game_with_the_code_too() {
    let s = start().await;
    // Before two-step verification is on (in the portal), the password does.
    Client::try_login(&s.url, "mod", PASSWORD).await.expect("no code yet");
    common::staff_token(&s.host, "mod", PASSWORD).await;
    let secret = common::secret_of(&s.host, "mod");
    // Now the game asks for the code.
    let refused = Client::try_login(&s.url, "mod", PASSWORD).await.err().unwrap();
    assert_eq!(refused, AUTH_CODE_REQUIRED);
    let mut c = Client::connect(&s.url).await;
    c.send(ClientMsg::LoginWithCode {
        username: "mod".into(),
        password: PASSWORD.into(),
        code: "123 456".into(),
    })
    .await;
    match c.expect("auth", |m| matches!(m, ServerMsg::AuthError { .. })).await {
        ServerMsg::AuthError { reason } => assert_eq!(reason, AUTH_CODE_WRONG),
        _ => unreachable!(),
    }
    // A wrong password with a right code is still a wrong password.
    let code = totp::code_at(&secret, now() + totp::STEP).unwrap();
    c.send(ClientMsg::LoginWithCode {
        username: "mod".into(),
        password: "wrong-password".into(),
        code: code.clone(),
    })
    .await;
    match c.expect("auth", |m| matches!(m, ServerMsg::AuthError { .. })).await {
        ServerMsg::AuthError { reason } => assert_eq!(reason, "Wrong username or password."),
        _ => unreachable!(),
    }
    c.send(ClientMsg::LoginWithCode { username: "mod".into(), password: PASSWORD.into(), code })
        .await;
    c.expect("signed in", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
    // A recovery code works in the game too.
    let recovery = common::take_recovery_code(&s.host, "mod");
    let mut d = Client::connect(&s.url).await;
    d.send(ClientMsg::LoginWithCode {
        username: "mod".into(),
        password: PASSWORD.into(),
        code: recovery,
    })
    .await;
    d.expect("signed in", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
    // A player is never asked.
    Client::register(&s.url, "flor").await;
    Client::try_login(&s.url, "flor", PASSWORD).await.expect("players sign in as before");
}

#[tokio::test]
async fn a_new_password_signs_the_portal_devices_out() {
    let s = start().await;
    let remembered = common::staff_login(&s.host, "mod", PASSWORD, true).await;
    let device = remembered["device"].as_str().unwrap().to_string();
    // The new password is chosen in the game.
    let mut c = Client::connect(&s.url).await;
    let code = totp::code_at(&common::secret_of(&s.host, "mod"), now() + totp::STEP).unwrap();
    c.send(ClientMsg::LoginWithCode { username: "mod".into(), password: PASSWORD.into(), code })
        .await;
    c.expect("signed in", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
    c.send(ClientMsg::ChangePassword { old: PASSWORD.into(), new: "brand-new-pass-9".into() })
        .await;
    c.notice("Password changed").await;
    let (code, _) =
        http(&s.host, "POST", "/admin/api/login/device", "", &json!({ "device": device })).await;
    assert_eq!(code, 401);
    // Without e-mail confirmed, nobody is told by e-mail.
    assert!(s.server.mail.outbox().is_empty());
    // A setting that asks for nothing new: removing the address.
    c.send(ClientMsg::SetRecoveryEmail {
        email: String::new(),
        password: "brand-new-pass-9".into(),
        lang: String::new(),
    })
    .await;
    c.notice("Recovery e-mail removed").await;
    c.send(ClientMsg::ConfirmEmailAgain { lang: String::new() }).await;
    assert_eq!(c.error().await, "Set a recovery e-mail first.");
}
