//! E-mail to players: a link to choose a new password, a link to confirm
//! the recovery e-mail, and a notice when the password changed. Sent
//! through [Resend](https://resend.com)'s API (`SNOOK_LEGEND_RESEND_API_KEY`,
//! `SNOOK_LEGEND_MAIL_FROM`); a server without it sends none (a server in
//! memory, for development, writes them to its log instead).
//!
//! Every message is in Portuguese, Spanish or English (the website's
//! languages, where its links lead), as plain text and as simple HTML.

use std::sync::Mutex;

/// How the server sends e-mail.
#[derive(Clone, Default)]
pub enum MailSetup {
    /// It doesn't: password resets and confirmations are not offered.
    #[default]
    Off,
    /// Kept in memory and written to the log (development and tests).
    Outbox,
    /// Through Resend.
    Resend(ResendConfig),
}

impl std::fmt::Debug for MailSetup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MailSetup::Off => f.write_str("Off"),
            MailSetup::Outbox => f.write_str("Outbox"),
            MailSetup::Resend(c) => write!(f, "Resend(from {})", c.from),
        }
    }
}

/// Resend's settings. The key is never logged.
#[derive(Clone)]
pub struct ResendConfig {
    pub api_key: String,
    /// `Snook Legend <support@snooklegend.app>`: an address of a domain
    /// verified in Resend (replies go there too).
    pub from: String,
}

/// An e-mail as sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    pub to: String,
    pub subject: String,
    pub text: String,
    pub html: String,
}

pub struct Mailer {
    setup: MailSetup,
    outbox: Mutex<Vec<Sent>>,
    #[cfg(feature = "mail")]
    http: Option<reqwest::Client>,
}

/// Resend's endpoint for sending.
#[cfg(feature = "mail")]
const RESEND_URL: &str = "https://api.resend.com/emails";

impl Mailer {
    pub fn new(setup: MailSetup) -> Mailer {
        #[cfg(feature = "mail")]
        let http = matches!(setup, MailSetup::Resend(_)).then(|| {
            // TLS with ring, like the database and the payments.
            let _ = rustls::crypto::ring::default_provider().install_default();
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("http client")
        });
        Mailer {
            setup,
            outbox: Mutex::new(Vec::new()),
            #[cfg(feature = "mail")]
            http,
        }
    }

    /// Whether this server sends e-mail at all.
    pub fn enabled(&self) -> bool {
        match self.setup {
            MailSetup::Off => false,
            MailSetup::Outbox => true,
            #[cfg(feature = "mail")]
            MailSetup::Resend(_) => true,
            #[cfg(not(feature = "mail"))]
            MailSetup::Resend(_) => false,
        }
    }

    /// What was "sent" with [`MailSetup::Outbox`], oldest first.
    pub fn outbox(&self) -> Vec<Sent> {
        self.outbox.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Sends `mail` to `to`. Errors are logged here (the player was told
    /// "if an account matches, an e-mail is on its way" either way).
    pub async fn send(&self, to: &str, mail: Mail) -> bool {
        let sent =
            Sent { to: to.to_string(), subject: mail.subject, text: mail.text, html: mail.html };
        match &self.setup {
            MailSetup::Off => false,
            MailSetup::Outbox => {
                tracing::info!(to = %sent.to, subject = %sent.subject, "e-mail (not sent: outbox)\n{}", sent.text);
                let mut out = self.outbox.lock().unwrap_or_else(|e| e.into_inner());
                out.push(sent);
                // Development only, but never without bound.
                if out.len() > 1000 {
                    out.remove(0);
                }
                true
            }
            #[cfg(feature = "mail")]
            MailSetup::Resend(cfg) => self.resend(cfg, &sent).await,
            #[cfg(not(feature = "mail"))]
            MailSetup::Resend(_) => false,
        }
    }

    #[cfg(feature = "mail")]
    async fn resend(&self, cfg: &ResendConfig, mail: &Sent) -> bool {
        let Some(http) = &self.http else { return false };
        let body = serde_json::json!({
            "from": cfg.from,
            "to": [mail.to],
            "subject": mail.subject,
            "text": mail.text,
            "html": mail.html,
        });
        // One more try when Resend or the network fails for a moment.
        for attempt in 0..2 {
            let res = http.post(RESEND_URL).bearer_auth(&cfg.api_key).json(&body).send().await;
            match res {
                Ok(r) if r.status().is_success() => {
                    tracing::info!(subject = %mail.subject, "e-mail sent");
                    return true;
                }
                Ok(r) => {
                    let status = r.status();
                    let text = r.text().await.unwrap_or_default();
                    tracing::error!(%status, "Resend refused an e-mail: {text}");
                    if !status.is_server_error() && status.as_u16() != 429 {
                        return false;
                    }
                }
                Err(e) => tracing::error!("Resend: {e}"),
            }
            if attempt == 0 {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
        false
    }
}

/// An e-mail ready to send.
#[derive(Debug, Clone)]
pub struct Mail {
    pub subject: String,
    pub text: String,
    pub html: String,
}

/// The languages e-mails are written in (those of the website).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Pt,
    Es,
}

impl Lang {
    /// From a language code as the game or the website send it (`pt-BR`,
    /// `es`, `fr`…): anything else reads English.
    pub fn parse(code: &str) -> Lang {
        let code = code.trim().to_ascii_lowercase();
        if code == "pt" || code.starts_with("pt-") || code.starts_with("pt_") {
            Lang::Pt
        } else if code == "es" || code.starts_with("es-") || code.starts_with("es_") {
            Lang::Es
        } else {
            Lang::En
        }
    }

    /// A guess from the account's country, when nothing better is known
    /// (the game's own language is, where it asks for an e-mail).
    pub fn of_country(country: &str) -> Lang {
        const PT: &[&str] = &["BR", "PT", "AO", "MZ", "CV", "GW", "ST", "TL"];
        const ES: &[&str] = &[
            "ES", "MX", "AR", "CO", "CL", "PE", "VE", "EC", "GT", "CU", "BO", "DO", "HN", "PY",
            "SV", "NI", "CR", "PA", "UY", "PR", "GQ",
        ];
        let c = country.trim().to_ascii_uppercase();
        if PT.contains(&c.as_str()) {
            Lang::Pt
        } else if ES.contains(&c.as_str()) {
            Lang::Es
        } else {
            Lang::En
        }
    }

    /// The website's page for choosing a new password.
    pub fn reset_path(self) -> &'static str {
        match self {
            Lang::En => "/reset-password/",
            Lang::Pt => "/pt/redefinir-senha/",
            Lang::Es => "/es/restablecer-contrasena/",
        }
    }

    /// The website's page that confirms a recovery e-mail.
    pub fn confirm_path(self) -> &'static str {
        match self {
            Lang::En => "/confirm-email/",
            Lang::Pt => "/pt/confirmar-email/",
            Lang::Es => "/es/confirmar-correo/",
        }
    }
}

/// `&`, `<`, `>`, `"` and `'` written so HTML shows them as they are.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// The words of one e-mail: its subject, the paragraphs before the
/// button, the button, the paragraphs after it.
struct Words {
    subject: String,
    before: Vec<String>,
    button: Option<(String, String)>,
    after: Vec<String>,
    footer: String,
}

/// Lays an e-mail out as plain text and HTML.
fn layout(w: Words) -> Mail {
    let mut text = String::new();
    for p in &w.before {
        text.push_str(p);
        text.push_str("\n\n");
    }
    if let Some((label, url)) = &w.button {
        text.push_str(&format!("{label}:\n{url}\n\n"));
    }
    for p in &w.after {
        text.push_str(p);
        text.push_str("\n\n");
    }
    text.push_str("-- \n");
    text.push_str(&w.footer);
    text.push('\n');

    let para = |p: &String| {
        format!(
            "<p style=\"margin:0 0 16px;font-size:16px;line-height:1.5;color:#1d2a24\">{}</p>",
            esc(p)
        )
    };
    let mut body = String::new();
    for p in &w.before {
        body.push_str(&para(p));
    }
    if let Some((label, url)) = &w.button {
        body.push_str(&format!(
            "<p style=\"margin:24px 0\"><a href=\"{url}\" style=\"display:inline-block;background:#1f7a4d;color:#ffffff;text-decoration:none;font-weight:600;font-size:16px;padding:12px 22px;border-radius:8px\">{label}</a></p>\
             <p style=\"margin:0 0 16px;font-size:13px;line-height:1.5;color:#5b6b63;word-break:break-all\">{url}</p>",
            url = esc(url),
            label = esc(label),
        ));
    }
    for p in &w.after {
        body.push_str(&para(p));
    }
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>{subject}</title></head>\
         <body style=\"margin:0;padding:0;background:#f1f4f2\">\
         <div style=\"max-width:520px;margin:0 auto;padding:32px 20px;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Helvetica,Arial,sans-serif\">\
         <p style=\"margin:0 0 24px;font-size:18px;font-weight:700;color:#1f7a4d\">Snook Legend</p>\
         <div style=\"background:#ffffff;border-radius:12px;padding:28px 24px\">{body}</div>\
         <p style=\"margin:20px 4px 0;font-size:12px;line-height:1.5;color:#6b7a72\">{footer}</p>\
         </div></body></html>",
        subject = esc(&w.subject),
        footer = esc(&w.footer),
    );
    Mail { subject: w.subject, text, html }
}

fn footer(lang: Lang, username: &str) -> String {
    match lang {
        Lang::Pt => format!(
            "Você recebeu este e-mail porque este endereço é o e-mail de recuperação da conta {username} no Snook Legend (snooklegend.app)."
        ),
        Lang::Es => format!(
            "Recibes este correo porque esta dirección es el correo de recuperación de la cuenta {username} en Snook Legend (snooklegend.app)."
        ),
        Lang::En => format!(
            "You got this e-mail because this address is the recovery e-mail of the account {username} on Snook Legend (snooklegend.app)."
        ),
    }
}

/// A link to choose a new password, valid `minutes`.
pub fn password_reset(lang: Lang, username: &str, url: &str, minutes: i64) -> Mail {
    let w = match lang {
        Lang::Pt => Words {
            subject: "Escolha uma nova senha para o Snook Legend".into(),
            before: vec![
                format!("Olá, {username}!"),
                "Recebemos um pedido para redefinir a senha da sua conta no Snook Legend. Para escolher uma nova senha, abra o link abaixo.".into(),
            ],
            button: Some(("Escolher uma nova senha".into(), url.into())),
            after: vec![
                format!("O link vale por {minutes} minutos e funciona uma só vez."),
                "Não foi você? Ignore este e-mail: sua senha continua a mesma.".into(),
            ],
            footer: footer(lang, username),
        },
        Lang::Es => Words {
            subject: "Elige una nueva contraseña para Snook Legend".into(),
            before: vec![
                format!("¡Hola, {username}!"),
                "Recibimos una solicitud para restablecer la contraseña de tu cuenta en Snook Legend. Para elegir una nueva contraseña, abre el enlace de abajo.".into(),
            ],
            button: Some(("Elegir una nueva contraseña".into(), url.into())),
            after: vec![
                format!("El enlace vale {minutes} minutos y funciona una sola vez."),
                "¿No fuiste tú? Ignora este correo: tu contraseña sigue siendo la misma.".into(),
            ],
            footer: footer(lang, username),
        },
        Lang::En => Words {
            subject: "Choose a new password for Snook Legend".into(),
            before: vec![
                format!("Hi {username},"),
                "Someone asked to reset the password of your Snook Legend account. To choose a new password, open the link below.".into(),
            ],
            button: Some(("Choose a new password".into(), url.into())),
            after: vec![
                format!("The link works for {minutes} minutes, and only once."),
                "Wasn't you? Ignore this e-mail: your password stays the same.".into(),
            ],
            footer: footer(lang, username),
        },
    };
    layout(w)
}

/// A link to confirm `email` as the account's recovery e-mail.
pub fn confirm_email(lang: Lang, username: &str, email: &str, url: &str, days: i64) -> Mail {
    let w = match lang {
        Lang::Pt => Words {
            subject: "Confirme seu e-mail de recuperação do Snook Legend".into(),
            before: vec![
                format!("Olá, {username}!"),
                format!(
                    "O endereço {email} foi indicado para recuperar a conta {username} no Snook Legend, caso a senha seja esquecida. Confirme que ele é seu:"
                ),
            ],
            button: Some(("Confirmar o e-mail".into(), url.into())),
            after: vec![
                format!(
                    "O link vale por {days} dias. Enquanto o e-mail não for confirmado, não enviamos links de nova senha para ele."
                ),
                "Não foi você? Ignore este e-mail: nada muda.".into(),
            ],
            footer: footer(lang, username),
        },
        Lang::Es => Words {
            subject: "Confirma tu correo de recuperación de Snook Legend".into(),
            before: vec![
                format!("¡Hola, {username}!"),
                format!(
                    "La dirección {email} se indicó para recuperar la cuenta {username} en Snook Legend si se olvida la contraseña. Confirma que es tuya:"
                ),
            ],
            button: Some(("Confirmar el correo".into(), url.into())),
            after: vec![
                format!(
                    "El enlace vale {days} días. Mientras el correo no esté confirmado, no enviamos enlaces de nueva contraseña a él."
                ),
                "¿No fuiste tú? Ignora este correo: no cambia nada.".into(),
            ],
            footer: footer(lang, username),
        },
        Lang::En => Words {
            subject: "Confirm your Snook Legend recovery e-mail".into(),
            before: vec![
                format!("Hi {username},"),
                format!(
                    "The address {email} was given to recover the account {username} on Snook Legend if its password is forgotten. Confirm it is yours:"
                ),
            ],
            button: Some(("Confirm the e-mail".into(), url.into())),
            after: vec![
                format!(
                    "The link works for {days} days. Until the e-mail is confirmed, no new-password links are sent to it."
                ),
                "Wasn't you? Ignore this e-mail: nothing changes.".into(),
            ],
            footer: footer(lang, username),
        },
    };
    layout(w)
}

/// The account's password changed (by a link or in the game).
pub fn password_changed(lang: Lang, username: &str) -> Mail {
    let w = match lang {
        Lang::Pt => Words {
            subject: "A senha do Snook Legend foi alterada".into(),
            before: vec![
                format!("Olá, {username}!"),
                format!(
                    "A senha da conta {username} no Snook Legend acaba de ser alterada, e as sessões abertas em outros lugares foram encerradas."
                ),
            ],
            button: None,
            after: vec![
                "Foi você? Então está tudo certo, não precisa fazer nada.".into(),
                "Não foi você? Responda a este e-mail para falar com o suporte.".into(),
            ],
            footer: footer(lang, username),
        },
        Lang::Es => Words {
            subject: "La contraseña de Snook Legend cambió".into(),
            before: vec![
                format!("¡Hola, {username}!"),
                format!(
                    "La contraseña de la cuenta {username} en Snook Legend acaba de cambiar, y las sesiones abiertas en otros lugares se cerraron."
                ),
            ],
            button: None,
            after: vec![
                "¿Fuiste tú? Entonces todo está bien, no tienes que hacer nada.".into(),
                "¿No fuiste tú? Responde a este correo para hablar con el soporte.".into(),
            ],
            footer: footer(lang, username),
        },
        Lang::En => Words {
            subject: "Your Snook Legend password was changed".into(),
            before: vec![
                format!("Hi {username},"),
                format!(
                    "The password of the account {username} on Snook Legend was just changed, and the sessions open elsewhere were signed out."
                ),
            ],
            button: None,
            after: vec![
                "Was it you? Then all is well, there is nothing to do.".into(),
                "Wasn't you? Reply to this e-mail to reach support.".into(),
            ],
            footer: footer(lang, username),
        },
    };
    layout(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_from_codes() {
        assert_eq!(Lang::parse("pt-BR"), Lang::Pt);
        assert_eq!(Lang::parse("pt-PT"), Lang::Pt);
        assert_eq!(Lang::parse("PT"), Lang::Pt);
        assert_eq!(Lang::parse("es"), Lang::Es);
        assert_eq!(Lang::parse("es-419"), Lang::Es);
        assert_eq!(Lang::parse("fr"), Lang::En);
        assert_eq!(Lang::parse(""), Lang::En);
    }

    #[test]
    fn html_shows_text_as_it_is() {
        let m = password_reset(Lang::En, "a<b>&\"c'", "https://x.example/r/#token=abc&x=1", 60);
        assert!(m.html.contains("a&lt;b&gt;&amp;&quot;c&#39;"));
        assert!(!m.html.contains("a<b>"));
        assert!(m.html.contains("href=\"https://x.example/r/#token=abc&amp;x=1\""));
        // The plain text keeps the link whole.
        assert!(m.text.contains("https://x.example/r/#token=abc&x=1"));
    }

    #[test]
    fn every_message_in_every_language() {
        for lang in [Lang::En, Lang::Pt, Lang::Es] {
            let r = password_reset(lang, "ana", "https://s/r#token=t", 60);
            let c = confirm_email(lang, "ana", "ana@example.com", "https://s/c#token=t", 7);
            let p = password_changed(lang, "ana");
            for m in [&r, &c, &p] {
                assert!(!m.subject.is_empty());
                assert!(m.text.contains("ana"));
                assert!(m.html.starts_with("<!doctype html>"));
            }
            assert!(r.text.contains("60") && r.text.contains("https://s/r#token=t"));
            assert!(c.text.contains("ana@example.com") && c.text.contains("7"));
        }
    }
}
