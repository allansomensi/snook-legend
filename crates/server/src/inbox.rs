//! The players' inbox (see `snook_legend_proto::inbox`): what the staff and
//! the game tell one player, kept until read and after.
//!
//! Everything that notifies a player goes through [`send`]: it stores the
//! notification and, if the player is online, pushes it at once
//! ([`ServerMsg::InboxNew`]). A game older than the inbox gets the
//! notification as a notice in English instead (what it was told before).
//!
//! The staff portal can see a notification before it is sent ([`preview`]):
//! the server words it in English, Portuguese and Spanish ([`wording`]); the
//! game words it in each of its languages.

use serde_json::{Value, json};
use snook_legend_proto::inbox::{Action, Kind, MAX_KEPT, Notification};
use snook_legend_proto::{ClientMsg, ServerMsg};

use crate::store::{NewNotification, NotificationRow, User};
use crate::{Server, now};

/// The inbox a game is sent (the newest; the store keeps [`MAX_KEPT`]).
const SENT: u32 = MAX_KEPT as u32;

/// A notification about to be sent.
#[derive(Debug, Clone, PartialEq)]
pub struct New {
    pub kind: Kind,
    /// What the staff wrote (empty for the game's own).
    pub text: String,
    /// The staff member who sends it (empty: the game).
    pub from: String,
    /// The staff want to know it was read.
    pub important: bool,
    /// Pinned at the top until then (0: not pinned; `i64::MAX`: until taken
    /// down).
    pub pinned_until: i64,
    pub action: Action,
    /// The player was told already by a message of its own (an achievement
    /// unlocked): a game older than the inbox isn't told again.
    pub quiet: bool,
}

impl New {
    /// A notification of the game's own, with no text, button or pin.
    pub fn game(kind: Kind) -> New {
        New {
            kind,
            text: String::new(),
            from: String::new(),
            important: false,
            pinned_until: 0,
            action: Action::None,
            quiet: false,
        }
    }

    pub fn text(mut self, text: impl Into<String>) -> New {
        self.text = text.into();
        self
    }

    pub fn from(mut self, staff: impl Into<String>) -> New {
        self.from = staff.into();
        self
    }

    pub fn important(mut self) -> New {
        self.important = true;
        self
    }

    pub fn pinned_until(mut self, until: i64) -> New {
        self.pinned_until = until;
        self
    }

    pub fn action(mut self, action: Action) -> New {
        self.action = action;
        self
    }

    pub fn quiet(mut self) -> New {
        self.quiet = true;
        self
    }

    /// The notification as the player would get it now (id 0: not stored).
    pub fn notification(&self) -> Notification {
        Notification {
            id: 0,
            kind: self.kind.clone(),
            text: snook_legend_proto::clean_text(&self.text, snook_legend_proto::inbox::MAX_TEXT),
            from: self.from.clone(),
            created_at: now(),
            read_at: 0,
            important: self.important,
            pinned_until: self.pinned_until,
            action: self.action.clone(),
        }
    }
}

/// A stored notification as the game reads it (one that can't be read, a
/// newer server's, comes as [`Kind::Other`] with its text).
pub fn notification(row: &NotificationRow) -> Notification {
    Notification {
        id: row.id,
        kind: serde_json::from_str(&row.kind).unwrap_or(Kind::Other),
        text: row.text.clone(),
        from: row.from.clone(),
        created_at: row.created_at,
        read_at: row.read_at.unwrap_or(0),
        important: row.important,
        pinned_until: row.pinned_until,
        action: serde_json::from_str(&row.action).unwrap_or_default(),
    }
}

/// Stores a notification for `user_id` and pushes it if they are online.
/// Returns its id (None when it could not be stored: the error is logged).
pub async fn send(server: &Server, user_id: i64, n: New) -> Option<i64> {
    let item = n.notification();
    let kind = serde_json::to_string(&item.kind).unwrap_or_else(|_| "{}".into());
    let action = serde_json::to_string(&item.action).unwrap_or_else(|_| "{\"to\":\"none\"}".into());
    // The staff member by account (their name may change).
    let from_id = if n.from.is_empty() {
        None
    } else {
        server.store.user_by_name(&n.from).await.ok().flatten().map(|u| u.id)
    };
    let new = NewNotification {
        user_id,
        kind: &kind,
        text: &item.text,
        from_id,
        created_at: item.created_at,
        important: item.important,
        pinned_until: item.pinned_until,
        action: &action,
    };
    let id = match server.store.add_notification(&new, MAX_KEPT).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!(user = user_id, "notification: {e}");
            return None;
        }
    };
    let item = Notification { id, ..item };
    let Some(name) = server.with(|l| l.online_name(user_id)) else { return Some(id) };
    if server.with(|l| l.has_inbox(&name)) {
        let unread = unread_of(server, user_id).await;
        server.with(|l| l.notify(&name, ServerMsg::InboxNew { item, unread }));
    } else if !n.quiet {
        let (title, body) = wording(&item, "en");
        let text = if body.is_empty() { title } else { format!("{title} {body}") };
        server.with(|l| l.notify(&name, ServerMsg::Notice { text }));
    }
    Some(id)
}

async fn unread_of(server: &Server, user_id: i64) -> u32 {
    let list = server.store.notifications(user_id, SENT).await.unwrap_or_default();
    list.iter().filter(|n| n.read_at.is_none()).count() as u32
}

/// The player's inbox as their game is sent it.
async fn inbox_msg(server: &Server, user_id: i64) -> Option<ServerMsg> {
    match server.store.notifications(user_id, SENT).await {
        Ok(rows) => {
            let items: Vec<Notification> = rows.iter().map(notification).collect();
            let unread = items.iter().filter(|n| n.unread()).count() as u32;
            Some(ServerMsg::Inbox { items, unread })
        }
        Err(e) => {
            tracing::error!(user = user_id, "inbox: {e}");
            None
        }
    }
}

/// After signing in: the inbox (a game that has one), and why the player
/// is muted if they are (the lounge says so when they try to write).
pub async fn signed_in(server: &Server, user: &User) {
    let t = now();
    if user.is_muted(t)
        && let Ok(rows) = server.store.notifications_sent(user.id, 50).await
        && let Some(reason) = rows.iter().map(notification).find_map(|n| match n.kind {
            Kind::Muted { until } if until > t => Some(n.text),
            _ => None,
        })
    {
        server.with(|l| l.set_mute_reason(user.id, &reason));
    }
    if !server.with(|l| l.has_inbox(&user.username)) {
        return;
    }
    if let Some(msg) = inbox_msg(server, user.id).await {
        server.with(|l| l.notify(&user.username, msg));
    }
}

pub fn is_msg(msg: &ClientMsg) -> bool {
    matches!(msg, ClientMsg::Inbox | ClientMsg::InboxRead { .. } | ClientMsg::InboxDelete { .. })
}

/// Most notifications one message reads or deletes.
const MAX_IDS: usize = MAX_KEPT;

pub async fn handle(server: &Server, me: &User, msg: ClientMsg, send: &impl Fn(ServerMsg)) {
    let t = now();
    match msg {
        ClientMsg::Inbox => {}
        ClientMsg::InboxRead { mut ids } => {
            ids.truncate(MAX_IDS);
            if let Err(e) = server.store.read_notifications(me.id, &ids, t).await {
                tracing::error!(user = me.id, "inbox read: {e}");
            }
        }
        ClientMsg::InboxDelete { mut ids } => {
            ids.truncate(MAX_IDS);
            if let Err(e) = server.store.delete_notifications(me.id, &ids, t).await {
                tracing::error!(user = me.id, "inbox delete: {e}");
            }
        }
        _ => return,
    }
    if let Some(msg) = inbox_msg(server, me.id).await {
        send(msg);
    }
}

/// Achievements just unlocked, in the inbox (the player was told of each
/// already: an achievement that comes with an item offers to put it on).
pub async fn achievements(server: &Server, user_id: i64, codes: &[String]) {
    for code in codes {
        let item = snook_legend_proto::shop::ITEMS.iter().find(
            |i| matches!(i.price, snook_legend_proto::shop::Price::Achievement(a) if a == code),
        );
        let mut n = New::game(Kind::Achievement { code: code.clone() }).quiet();
        if let Some(item) = item {
            n = n.action(Action::Equip { code: item.code.to_string() });
        }
        send(server, user_id, n).await;
    }
}

/// What the player would get, without sending anything: the notification
/// and its title and body in English, Portuguese and Spanish (the staff
/// portal's "preview").
pub fn preview(n: &New) -> Value {
    let item = n.notification();
    let words = |lang: &str| {
        let (title, body) = wording(&item, lang);
        json!({ "title": title, "body": body })
    };
    json!({
        "preview": true,
        "notification": item,
        "category": item.category(),
        "rendered": { "en": words("en"), "pt": words("pt"), "es": words("es") },
    })
}

/// A notification's title and body in `lang` (`en`, `pt` or `es`; others
/// read English): what the game shows, for the staff and for games older
/// than the inbox. The staff's own text is added to the body as written.
pub fn wording(n: &Notification, lang: &str) -> (String, String) {
    let l = |en: &str, pt: &str, es: &str| -> String {
        match lang {
            "pt" => pt,
            "es" => es,
            _ => en,
        }
        .to_string()
    };
    let date = |t: i64| crate::admin::utc(t);
    let item = |code: &str| {
        snook_legend_proto::shop::item(code)
            .map_or_else(|| code.to_string(), |i| i.name.to_string())
    };
    let rules = |r: snook_legend_proto::Rules| match (r, lang) {
        (snook_legend_proto::Rules::Brazilian, "pt") => "regras brasileiras".to_string(),
        (snook_legend_proto::Rules::Brazilian, "es") => "reglas brasileñas".to_string(),
        (snook_legend_proto::Rules::Brazilian, _) => "Brazilian rules".to_string(),
        (_, "pt") => "regras oficiais".to_string(),
        (_, "es") => "reglas oficiales".to_string(),
        _ => "official rules".to_string(),
    };
    let (title, body) = match &n.kind {
        Kind::StaffMessage => {
            (l("Message from the staff", "Mensagem da equipe", "Mensaje del equipo"), String::new())
        }
        Kind::Warning => (
            l("Warning from the staff", "Advertência da equipe", "Advertencia del equipo"),
            String::new(),
        ),
        Kind::Muted { until } => (
            l("You were muted", "Você foi silenciado", "Has sido silenciado"),
            l(
                &format!("You can't write in the chat until {}.", date(*until)),
                &format!("Você não pode escrever no chat até {}.", date(*until)),
                &format!("No puedes escribir en el chat hasta {}.", date(*until)),
            ),
        ),
        Kind::Unmuted => (
            l("You can chat again", "Você pode conversar de novo", "Puedes volver a chatear"),
            l(
                "The staff lifted your mute.",
                "A equipe retirou o seu silenciamento.",
                "El equipo levantó tu silencio.",
            ),
        ),
        Kind::RatingAdjusted { rules: r, delta, rating, .. } => (
            l(
                "Your rating was adjusted",
                "Sua pontuação foi ajustada",
                "Tu puntuación fue ajustada",
            ),
            l(
                &format!("{delta:+} under the {}: now {rating}.", rules(*r)),
                &format!("{delta:+} nas {}: agora {rating}.", rules(*r)),
                &format!("{delta:+} en las {}: ahora {rating}.", rules(*r)),
            ),
        ),
        Kind::MatchVoided { opponent, delta, .. } => (
            l("A game was annulled", "Uma partida foi anulada", "Se anuló una partida"),
            l(
                &format!("Your game against {opponent} no longer counts ({:+} rating).", -delta),
                &format!(
                    "Sua partida contra {opponent} não conta mais ({:+} de pontuação).",
                    -delta
                ),
                &format!("Tu partida contra {opponent} ya no cuenta ({:+} de puntuación).", -delta),
            ),
        ),
        Kind::MatchEnded { opponent, won, .. } => (
            l(
                "The staff ended a game",
                "A equipe encerrou uma partida",
                "El equipo terminó una partida",
            ),
            match won {
                Some(true) => l(
                    &format!("Your game against {opponent} was given to you."),
                    &format!("A partida contra {opponent} foi dada a você."),
                    &format!("La partida contra {opponent} se te ha dado a ti."),
                ),
                Some(false) => l(
                    &format!("Your game against {opponent} was given to them."),
                    &format!("A partida contra {opponent} foi dada a {opponent}."),
                    &format!("La partida contra {opponent} se le dio a {opponent}."),
                ),
                None => l(
                    &format!("Your game against {opponent} ended without a result."),
                    &format!("A partida contra {opponent} terminou sem resultado."),
                    &format!("La partida contra {opponent} terminó sin resultado."),
                ),
            },
        ),
        Kind::ReportActed { username } => (
            l("Thank you for your report", "Obrigado pela sua denúncia", "Gracias por tu denuncia"),
            l(
                &format!("The staff took action about {username}."),
                &format!("A equipe tomou providências sobre {username}."),
                &format!("El equipo tomó medidas sobre {username}."),
            ),
        ),
        Kind::StaffGift { coins, item: code, vip_days } => {
            let mut parts = Vec::new();
            if *coins > 0 {
                parts.push(l(
                    &format!("{coins} coins"),
                    &format!("{coins} moedas"),
                    &format!("{coins} monedas"),
                ));
            }
            if !code.is_empty() {
                parts.push(item(code));
            }
            if *vip_days > 0 {
                parts.push(l(
                    &format!("{vip_days} days of VIP"),
                    &format!("{vip_days} dias de VIP"),
                    &format!("{vip_days} días de VIP"),
                ));
            }
            (
                l("A gift from the staff", "Um presente da equipe", "Un regalo del equipo"),
                parts.join(", "),
            )
        }
        Kind::ItemAwarded { code, .. } => {
            (l("You won an item", "Você ganhou um item", "Ganaste un objeto"), item(code))
        }
        Kind::VipItem { code, month } => (
            l("VIP item of the month", "Item VIP do mês", "Objeto VIP del mes"),
            format!("{} ({month})", item(code)),
        ),
        Kind::Achievement { code } => {
            let title = snook_legend_proto::ACHIEVEMENTS
                .iter()
                .find(|a| a.code == code)
                .map_or(code.as_str(), |a| a.title);
            (
                l("Achievement unlocked", "Conquista desbloqueada", "Logro desbloqueado"),
                title.to_string(),
            )
        }
        Kind::TournamentStarting { name, starts_at, .. } => (
            l("A tournament is about to start", "Um torneio vai começar", "Un torneo va a empezar"),
            format!("{name} · {}", date(*starts_at)),
        ),
        Kind::TournamentResult { name, place, prize, .. } => (
            l("Tournament result", "Resultado do torneio", "Resultado del torneo"),
            l(
                &format!("{name}: place {place}, {prize} coins."),
                &format!("{name}: {place}º lugar, {prize} moedas."),
                &format!("{name}: puesto {place}, {prize} monedas."),
            ),
        ),
        Kind::AuctionOutbid { code, bid, .. } => (
            l("You were outbid", "Seu lance foi superado", "Superaron tu oferta"),
            format!("{} · {bid}", item(code)),
        ),
        Kind::AuctionWon { code, price, .. } => (
            l("You won an auction", "Você venceu um leilão", "Ganaste una subasta"),
            format!("{} · {price}", item(code)),
        ),
        Kind::Maintenance { at, minutes } => (
            l("Maintenance", "Manutenção", "Mantenimiento"),
            l(
                &format!("The server stops at {} for about {minutes} min.", date(*at)),
                &format!("O servidor para às {} por cerca de {minutes} min.", date(*at)),
                &format!("El servidor se detiene a las {} durante unos {minutes} min.", date(*at)),
            ),
        ),
        Kind::Other => (l("Notification", "Notificação", "Notificación"), String::new()),
    };
    let body = match (body.is_empty(), n.text.is_empty()) {
        (_, true) => body,
        (true, false) => n.text.clone(),
        (false, false) => format!("{body} {}", n.text),
    };
    (title, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worded_in_three_languages_with_the_staff_text() {
        let n = New::game(Kind::Warning).text("No insults, please.").from("mod").notification();
        assert_eq!(
            wording(&n, "en"),
            ("Warning from the staff".into(), "No insults, please.".into())
        );
        assert_eq!(wording(&n, "pt").0, "Advertência da equipe");
        assert_eq!(wording(&n, "es").0, "Advertencia del equipo");
        let gift = New::game(Kind::StaffGift { coins: 500, item: String::new(), vip_days: 7 })
            .notification();
        assert_eq!(wording(&gift, "pt").1, "500 moedas, 7 dias de VIP");
    }

    #[test]
    fn rows_that_cannot_be_read_are_kept_as_other() {
        let row = NotificationRow {
            id: 3,
            user_id: 1,
            kind: r#"{"kind":"from_the_future","x":1}"#.into(),
            text: "hello".into(),
            from: String::new(),
            from_id: None,
            created_at: 5,
            read_at: Some(6),
            important: false,
            pinned_until: 0,
            action: "not json".into(),
            deleted_at: None,
        };
        let n = notification(&row);
        assert_eq!(n.kind, Kind::Other);
        assert_eq!(n.action, Action::None);
        assert_eq!(n.read_at, 6);
    }
}
