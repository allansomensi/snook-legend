//! The medals of the tournaments against the computer (see
//! `snook_legend_proto::cup`): the game sends each one once the account is
//! online, the server keeps it once per tournament and answers with the
//! account's count. Only the player sees them (with the tournaments, not
//! on the profile), so a medal is taken as the game says; a day's worth is
//! limited all the same.

use snook_legend_proto::{ClientMsg, ServerMsg};

use crate::store::User;
use crate::{Server, now};

/// More medals than tournaments anyone plays in a day (a knockout is four
/// games at the table, an arena five).
const PER_DAY: u32 = 40;

pub fn is_msg(msg: &ClientMsg) -> bool {
    matches!(msg, ClientMsg::CupMedals | ClientMsg::CupMedal { .. })
}

pub async fn handle(server: &Server, me: &User, msg: ClientMsg, send: &impl Fn(ServerMsg)) {
    let mut kept = None;
    if let ClientMsg::CupMedal { medal } = msg {
        if !medal.is_sound() {
            return send(ServerMsg::Error { message: "That medal can't be.".into() });
        }
        let at = now();
        match server.store.cup_medals_since(me.id, at - 24 * 3600).await {
            Ok(n) if n < PER_DAY => match server.store.add_cup_medal(me.id, &medal, at).await {
                Ok(_) => kept = Some(medal.cup),
                Err(e) => {
                    tracing::error!("cup medal of {}: {e}", me.username);
                    return;
                }
            },
            // Past the day's limit: not kept, and the game keeps trying.
            Ok(_) => {}
            Err(e) => {
                tracing::error!("cup medals of {}: {e}", me.username);
                return;
            }
        }
    }
    match server.store.cup_medals(me.id).await {
        Ok(medals) => send(ServerMsg::CupMedals { medals, kept }),
        Err(e) => tracing::error!("cup medals of {}: {e}", me.username),
    }
}
