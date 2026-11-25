//! The settings an account keeps (`snook_legend_proto::ClientMsg::SaveSettings`):
//! the game sends its settings that follow the player (not those of the
//! computer it runs on, such as the graphics) whenever they change, and
//! gets them back after signing in, so the game signed in on another
//! computer plays the same. The latest saved wins: an account is used in
//! one place at a time, and the game compares the time the server gives.

use snook_legend_proto::{ClientMsg, MAX_SETTINGS_BYTES, ServerMsg};

use crate::store::User;
use crate::{Server, now};

pub fn is_msg(msg: &ClientMsg) -> bool {
    matches!(msg, ClientMsg::SaveSettings { .. })
}

/// Whether `data` is settings the server keeps: a JSON object, small.
pub fn valid(data: &str) -> bool {
    data.len() <= MAX_SETTINGS_BYTES
        && serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(data).is_ok()
}

pub async fn handle(server: &Server, me: &User, msg: ClientMsg, send: &impl Fn(ServerMsg)) {
    let ClientMsg::SaveSettings { data } = msg else { return };
    if !valid(&data) {
        return send(ServerMsg::Error { message: "These settings can't be kept.".into() });
    }
    let at = now();
    match server.store.save_account_settings(me.id, &data, at).await {
        Ok(()) => send(ServerMsg::SettingsSaved { updated_at: at }),
        Err(e) => {
            tracing::warn!("settings of {}: {e}", me.username);
            send(ServerMsg::Error { message: "Something went wrong. Please try again.".into() })
        }
    }
}

/// After signing in: the settings the account keeps (empty, at time 0, when
/// it keeps none: the game then sends its own).
pub async fn signed_in(server: &Server, user: &User) {
    let (data, updated_at) = match server.store.account_settings(user.id).await {
        Ok(kept) => kept.unwrap_or_default(),
        Err(e) => {
            // Said nothing: the game keeps its own and sends nothing over
            // the account's (it waits for them).
            return tracing::warn!("settings of {}: {e}", user.username);
        }
    };
    server.with(|l| l.notify(&user.username, ServerMsg::Settings { data, updated_at }));
}
