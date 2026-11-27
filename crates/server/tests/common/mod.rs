//! Signing in to the staff API as the portal does: the password, then a
//! code of the authenticator app (two-step verification is required). The
//! first sign-in of an account turns it on; its secret and recovery codes
//! are kept here for the next ones (a code is used once: a second sign-in
//! in the same 30 seconds takes the next step's code, then a recovery code).

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::{Value, json};
use snook_legend_server::{now, totp};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Per server and account: the secret, the codes' steps used, the
/// recovery codes left.
type Kept = HashMap<String, (String, Vec<i64>, Vec<String>)>;
static KEPT: Mutex<Option<Kept>> = Mutex::new(None);

/// A JSON request to the server at `host`; the status and the body.
pub async fn http(host: &str, method: &str, path: &str, token: &str, body: &Value) -> (u16, Value) {
    let body = if body.is_null() { String::new() } else { body.to_string() };
    let mut s = TcpStream::connect(host).await.unwrap();
    let auth =
        if token.is_empty() { String::new() } else { format!("Authorization: Bearer {token}\r\n") };
    let req = format!(
        "{method} {path} HTTP/1.0\r\nHost: {host}\r\nUser-Agent: Mozilla/5.0 (Linux; Android 14) Chrome/130.0 Mobile Safari/537.36\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.unwrap();
    let out = String::from_utf8_lossy(&out).to_string();
    let code = out[9..12].parse().unwrap();
    let body = out.split_once("\r\n\r\n").map(|x| x.1).unwrap_or("");
    (code, serde_json::from_str(body).unwrap_or(Value::Null))
}

/// The first step: `{step, ticket, secret?}`.
pub async fn password_step(host: &str, name: &str, password: &str, remember: bool) -> Value {
    let (code, v) = http(
        host,
        "POST",
        "/admin/api/login",
        "",
        &json!({ "username": name, "password": password, "remember": remember }),
    )
    .await;
    assert_eq!(code, 200, "{v}");
    v
}

/// Signs `name` in to the staff API; the whole answer (`token`, maybe
/// `device`, `recovery_codes`).
pub async fn staff_login(host: &str, name: &str, password: &str, remember: bool) -> Value {
    let first = password_step(host, name, password, remember).await;
    let ticket = first["ticket"].as_str().unwrap().to_string();
    let key = format!("{host}|{}", name.to_lowercase());
    if first["step"] == "setup" {
        let secret = first["secret"].as_str().unwrap().to_string();
        let t = now();
        let code = totp::code_at(&secret, t).unwrap();
        let (status, v) = http(
            host,
            "POST",
            "/admin/api/login/code",
            "",
            &json!({ "ticket": ticket, "code": code }),
        )
        .await;
        assert_eq!(status, 200, "{v}");
        let codes: Vec<String> = v["recovery_codes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap().to_string())
            .collect();
        assert_eq!(codes.len(), totp::RECOVERY_CODES);
        KEPT.lock()
            .unwrap()
            .get_or_insert_with(HashMap::new)
            .insert(key, (secret, vec![totp::step_of(t)], codes));
        return v;
    }
    assert_eq!(first["step"], "code", "{first}");
    let (secret, used) = {
        let kept = KEPT.lock().unwrap();
        let (s, used, _) = kept.as_ref().and_then(|k| k.get(&key)).expect("set up in this test");
        (s.clone(), used.clone())
    };
    // An unused step among those accepted now, else a recovery code.
    let t = now();
    let step =
        [t, t + totp::STEP].into_iter().map(totp::step_of).find(|s| used.iter().all(|u| u < s));
    let code = match step {
        Some(s) => {
            let mut kept = KEPT.lock().unwrap();
            kept.as_mut().unwrap().get_mut(&key).unwrap().1.push(s);
            totp::code_at(&secret, s * totp::STEP).unwrap()
        }
        None => {
            let mut kept = KEPT.lock().unwrap();
            kept.as_mut().unwrap().get_mut(&key).unwrap().2.pop().expect("a recovery code left")
        }
    };
    let (status, v) =
        http(host, "POST", "/admin/api/login/code", "", &json!({ "ticket": ticket, "code": code }))
            .await;
    assert_eq!(status, 200, "{v}");
    v
}

/// The staff API's session token of `name`.
pub async fn staff_token(host: &str, name: &str, password: &str) -> String {
    staff_login(host, name, password, false).await["token"].as_str().unwrap().to_string()
}

/// The secret of `name`'s app on the server at `host` (signed in once).
pub fn secret_of(host: &str, name: &str) -> String {
    let kept = KEPT.lock().unwrap();
    kept.as_ref().unwrap()[&format!("{host}|{}", name.to_lowercase())].0.clone()
}

/// Takes one of `name`'s recovery codes (it is no longer used here).
pub fn take_recovery_code(host: &str, name: &str) -> String {
    let mut kept = KEPT.lock().unwrap();
    kept.as_mut()
        .unwrap()
        .get_mut(&format!("{host}|{}", name.to_lowercase()))
        .unwrap()
        .2
        .pop()
        .unwrap()
}
