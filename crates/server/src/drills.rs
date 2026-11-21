//! Drills (see `snook_legend_proto::drills`): checking a session's strokes,
//! the best scores, the tables and the challenge rating.

use snook_legend_proto::drills::{self, DrillInfo, Entry, Info, RatingEntry, Refused};
use snook_legend_proto::{ClientMsg, ServerMsg};

use crate::store::User;
use crate::{Server, now};

/// How many lead each table (and are shown).
const BOARD: u32 = 10;

/// How long a session may take, from its start to its strokes.
const SESSION_LIFE: std::time::Duration = std::time::Duration::from_secs(45 * 60);

pub fn is_msg(msg: &ClientMsg) -> bool {
    matches!(msg, ClientMsg::Drills | ClientMsg::DrillSubmit { .. } | ClientMsg::DrillStart { .. })
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
        ClientMsg::DrillStart { code } => {
            if drills::drill(&code).is_none() {
                return send(ServerMsg::DrillResult { code, result: Err(Refused::UnknownDrill) });
            }
            // One session at a time per account: a new one replaces it.
            let session = loop {
                let s: u64 = rand::random();
                if s != 0 {
                    break s;
                }
            };
            let mut sessions = crate::computer::lock(&server.computer.drill_sessions);
            if sessions.len() > 10_000 {
                sessions.retain(|_, (_, _, at)| at.elapsed() < SESSION_LIFE);
            }
            sessions.insert(me.id, (code.clone(), session, std::time::Instant::now()));
            drop(sessions);
            send(ServerMsg::DrillStarted { code, session });
        }
        ClientMsg::DrillSubmit { code, session, strokes } => {
            // The session the server started for this drill, once, in time.
            let started = {
                let mut sessions = crate::computer::lock(&server.computer.drill_sessions);
                match sessions.get(&me.id) {
                    Some((c, s, at))
                        if *c == code && *s == session && at.elapsed() < SESSION_LIFE =>
                    {
                        sessions.remove(&me.id);
                        true
                    }
                    _ => false,
                }
            };
            let result = if started {
                submit(server, me, &code, session, strokes).await
            } else {
                Err(Refused::NoSession)
            };
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
    session: u64,
    strokes: Vec<snook_legend_core::Shot>,
) -> Result<u32, Refused> {
    let d = drills::drill(code).ok_or(Refused::UnknownDrill)?;
    if strokes.len() > d.tries as usize {
        return Err(Refused::BadStroke);
    }
    let checked = code.to_string();
    // A couple at a time: a session is up to fifteen shots to simulate.
    let _permit = server.computer.drills.acquire().await.map_err(|_| Refused::Other)?;
    let score =
        tokio::task::spawn_blocking(move || drills::score_session(&checked, session, &strokes))
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
            // (Nothing made yet is no place on the table.)
            if rating > 0 {
                let _ = server.store.set_challenge_rating(me.id, rating, stars_of(&bests), t).await;
            }
        }
        let gained =
            drills::stars(d, score).saturating_sub(before.map_or(0, |b| drills::stars(d, b)));
        crate::account::award_xp(server, me, gained * snook_legend_proto::xp::DRILL_STAR).await;
    }
    Ok(score)
}
