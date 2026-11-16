//! Drills (see `snookered_proto::drills`): checking a session's strokes,
//! the best scores, the tables and the challenge rating.

use snookered_proto::drills::{self, DrillInfo, Entry, Info, RatingEntry, Refused};
use snookered_proto::{ClientMsg, ServerMsg};

use crate::store::User;
use crate::{Server, now};

/// How many lead each table (and are shown).
const BOARD: u32 = 10;

pub fn is_msg(msg: &ClientMsg) -> bool {
    matches!(msg, ClientMsg::Drills | ClientMsg::DrillSubmit { .. })
}

/// The drills as `me` sees them.
pub async fn info(server: &Server, me: &User) -> Option<Info> {
    let bests = server.store.drill_bests(me.id).await.ok()?;
    let mut list = Vec::new();
    for d in drills::DRILLS {
        let board = server.store.drill_board(d.code, BOARD).await.ok()?;
        list.push(DrillInfo {
            code: d.code.to_string(),
            best: bests.iter().find(|(c, _)| c == d.code).map(|(_, b)| *b),
            board: board
                .into_iter()
                .map(|r| Entry { username: r.username, country: r.country, score: r.score })
                .collect(),
        });
    }
    let (leaders, rank) = server.store.challenge_board(me.id, BOARD).await.ok()?;
    Some(Info {
        drills: list,
        rating: drills::challenge_rating(&bests),
        stars: stars_of(&bests),
        rank,
        leaders: leaders
            .into_iter()
            .map(|r| RatingEntry {
                username: r.username,
                country: r.country,
                rating: r.score,
                stars: r.stars,
            })
            .collect(),
    })
}

fn stars_of(bests: &[(String, u32)]) -> u32 {
    bests.iter().filter_map(|(c, b)| drills::drill(c).map(|d| drills::stars(d, *b))).sum()
}

pub async fn handle(server: &Server, me: &User, msg: ClientMsg, send: &impl Fn(ServerMsg)) {
    let info_msg = |info: Option<Info>| match info {
        Some(info) => ServerMsg::Drills { info },
        None => ServerMsg::Error { message: "Something went wrong. Try again.".into() },
    };
    match msg {
        ClientMsg::Drills => send(info_msg(info(server, me).await)),
        ClientMsg::DrillSubmit { code, strokes } => {
            let result = submit(server, me, &code, strokes).await;
            send(ServerMsg::DrillResult { code, result });
            send(info_msg(info(server, me).await));
        }
        _ => {}
    }
}

/// Checks and keeps a session: its score.
async fn submit(
    server: &Server,
    me: &User,
    code: &str,
    strokes: Vec<snookered_core::Shot>,
) -> Result<u32, Refused> {
    let d = drills::drill(code).ok_or(Refused::UnknownDrill)?;
    if strokes.len() > d.tries as usize {
        return Err(Refused::BadStroke);
    }
    let checked = code.to_string();
    let score = tokio::task::spawn_blocking(move || drills::score(&checked, &strokes))
        .await
        .unwrap_or(Err(Refused::Other))?;
    let t = now();
    let before = server.store.drill_record(me.id, code, score, t).await.map_err(|e| {
        tracing::error!("drill of {}: {e}", me.username);
        Refused::Other
    })?;
    if before.is_none_or(|b| score > b) {
        // A new best: the challenge rating moves, and new stars give
        // experience.
        if let Ok(bests) = server.store.drill_bests(me.id).await {
            let rating = drills::challenge_rating(&bests);
            let _ = server.store.set_challenge_rating(me.id, rating, stars_of(&bests), t).await;
        }
        let gained =
            drills::stars(d, score).saturating_sub(before.map_or(0, |b| drills::stars(d, b)));
        crate::account::award_xp(server, me, gained * snookered_proto::xp::DRILL_STAR).await;
    }
    Ok(score)
}
