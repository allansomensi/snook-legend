//! The moderation as the connection sees it (it needs the store): the
//! banned words loaded into the lounge, the mentions of players who are
//! offline, the line the staff pin from the game, what turns a text away,
//! and the history of names and countries.

use snook_legend_proto::moderation::{ChangeKind, ProfileChange, TextPlace, TextRefusal};
use snook_legend_proto::{ClientMsg, ServerMsg};

use crate::store::User;
use crate::{Server, now};

/// Most changes of name and country a profile shows.
const RENAMES_SHOWN: u32 = 20;

/// Loads the banned words from the store into the lounge (at the start,
/// and after the staff change the list).
pub async fn reload_words(server: &Server) {
    match server.store.banned_words().await {
        Ok(words) => {
            let filter = crate::wordfilter::Filter::new(
                words.iter().map(|w| (w.word.as_str(), w.lang.as_str())),
            );
            server.with(|l| l.set_words(filter));
        }
        Err(e) => tracing::error!("banned words: {e}"),
    }
}

/// Why `me` may not write `text` now (muted, or a banned word), as the
/// message their game understands. `user`: the account as it is now.
pub fn refusal(server: &Server, user: &User, text: &str, place: TextPlace) -> Option<ServerMsg> {
    let t = now();
    server.with(|l| {
        // (The lounge's copy of the account follows mutes by the staff.)
        let muted_until =
            l.session_user(&user.username).map_or(user.muted_until, |u| u.muted_until);
        let refusal = match muted_until.filter(|until| *until > t) {
            Some(until) if !text.is_empty() => Some(l.muted(user.id, until)),
            _ => l.banned_word(text).map(|h| h.refusal()),
        }?;
        Some(l.refusal_msg(&user.username, place, refusal))
    })
}

/// The refusal of a username with a banned word (for a game that may not
/// be signed in yet: worded in English for it).
pub fn name_refusal(server: &Server, name: &str) -> Option<String> {
    let hit = server.with(|l| l.banned_name(name))?;
    Some(crate::lounge::refusal_text(&TextRefusal::BannedWord { word: hit.word }))
}

/// A line in the lounge chat, with the players it mentions who are not
/// online looked up (a few at most).
pub async fn chat(server: &Server, me: &User, text: String) {
    use snook_legend_proto::moderation::{MAX_MENTIONS, mention_candidates};
    // A line past the chat's limit is refused by the lounge anyway: its
    // mentions cost no database requests (a flood of lines full of @names).
    let candidates: Vec<String> = if server.with(|l| l.may_chat_now(&me.username)) {
        mention_candidates(&text)
            .into_iter()
            .filter(|c| !server.with(|l| l.is_online(c)))
            .take(MAX_MENTIONS)
            .collect()
    } else {
        Vec::new()
    };
    let mut known = Vec::new();
    for c in candidates {
        if let Ok(Some(u)) = server.store.user_by_name(&c).await
            && u.deleted_at.is_none()
        {
            known.push(u.username);
        }
    }
    server.with(|l| l.chat_with_mentions(&me.username, &text, known));
}

/// The usernames among the `@names` of `text` (online or not).
pub async fn known_mentions(server: &Server, text: &str) -> Vec<String> {
    known_mentions_in(&server.store, text).await
}

/// [`known_mentions`], looked up in `store`.
pub async fn known_mentions_in(store: &crate::store::Store, text: &str) -> Vec<String> {
    use snook_legend_proto::moderation::{MAX_MENTIONS, mention_candidates};
    let mut known = Vec::new();
    for c in mention_candidates(text).into_iter().take(MAX_MENTIONS) {
        if let Ok(Some(u)) = store.user_by_name(&c).await
            && u.deleted_at.is_none()
        {
            known.push(u.username);
        }
    }
    known
}

pub fn is_msg(msg: &ClientMsg) -> bool {
    matches!(msg, ClientMsg::PinChat { .. } | ClientMsg::UnpinChat)
}

/// The staff pin a lounge chat line (or take it down) from the game.
pub async fn handle(server: &Server, me: &User, msg: ClientMsg, send: &impl Fn(ServerMsg)) {
    let error = |text: &str| send(ServerMsg::Error { message: text.to_string() });
    if !me.role.is_staff() {
        return error("You can't do that.");
    }
    match msg {
        ClientMsg::PinChat { id } => match server.with(|l| l.pin_chat(id)) {
            Some(line) => {
                let details = format!("{}: {}", line.from, line.text);
                audit(server, me, "chat-pin", "lounge", &details).await;
            }
            None => error("That line can't be pinned."),
        },
        ClientMsg::UnpinChat if server.with(|l| l.unpin_chat()) => {
            audit(server, me, "chat-unpin", "lounge", "").await;
        }
        _ => {}
    }
}

async fn audit(server: &Server, staff: &User, action: &str, target: &str, details: &str) {
    if let Err(e) = server.store.audit(staff.id, action, target, details, now()).await {
        tracing::error!("audit log: {e}");
    }
    tracing::info!(staff = %staff.username, action, target, "staff action in the game");
}

/// Keeps a change of name or country in the account's history.
pub async fn record_change(
    server: &Server,
    user_id: i64,
    kind: ChangeKind,
    old: &str,
    new: &str,
    by: Option<i64>,
) {
    let code = match kind {
        ChangeKind::Name => "name",
        ChangeKind::Country => "country",
        ChangeKind::Other => return,
    };
    if old == new {
        return;
    }
    if let Err(e) = server.store.add_profile_change(user_id, code, old, new, now(), by).await {
        tracing::error!("history of names and countries: {e}");
    }
}

/// The account's changes of name and country, newest first, as profiles
/// show them.
pub async fn renames(server: &Server, user_id: i64) -> Vec<ProfileChange> {
    server
        .store
        .profile_changes(user_id, RENAMES_SHOWN)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|c| ProfileChange {
            kind: match c.kind.as_str() {
                "name" => ChangeKind::Name,
                "country" => ChangeKind::Country,
                _ => ChangeKind::Other,
            },
            old: c.old,
            new: c.new,
            at: c.at,
        })
        .collect()
}
