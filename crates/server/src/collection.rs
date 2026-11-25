//! The players' collections (see `snook_legend_proto::collection`): every
//! item with how and when it was got.
//!
//! Whatever gives a player an item records its origin with [`record`].

use snook_legend_proto::collection::Origin;
use snook_legend_proto::{ClientMsg, ServerMsg};

use crate::Server;
use crate::store::User;

/// Records how `user_id` got `code` (kept with the item; the first origin
/// stays when an item is got twice).
pub async fn record(server: &Server, user_id: i64, code: &str, origin: Origin) {
    // TODO(0.12 collection): stored by the collection's implementation.
    let _ = (server, user_id, code, origin);
}

pub fn is_msg(msg: &ClientMsg) -> bool {
    matches!(msg, ClientMsg::Collection)
}

pub async fn handle(server: &Server, me: &User, msg: ClientMsg, send: &impl Fn(ServerMsg)) {
    // TODO(0.12 collection): the owned items with their origins.
    let _ = (server, me, msg);
    send(ServerMsg::Collection { items: Vec::new() });
}
