//! A whole game as frames for the staff panel's replay viewer: the panel is
//! plain JavaScript without the physics, so the server replays the strokes
//! and sends where every ball was, 30 times a second of play.
//!
//! Positions are in millimetres. Each shot has the table at its start and
//! frames that list only what changed: `[ball, x, y, ball, x, y, …]`, with
//! `null` coordinates once a ball has left the table.

use serde_json::{Value, json};
use snook_legend_core::{EightBallGame, Foul, PocketId, Simulation};
use snook_legend_proto::replay::{Replay, Stroke, place_cue_ball};

pub const FPS: f64 = 30.0;

type Spot = Option<(i32, i32)>;

fn spots_of(balls: &[snook_legend_core::Ball]) -> [Spot; 16] {
    let mut out = [None; 16];
    for b in balls {
        if b.in_play() && b.id.index() < 16 {
            let p = b.xy() * 1000.0;
            out[b.id.index()] = Some((p.x.round() as i32, p.y.round() as i32));
        }
    }
    out
}

fn spots_json(s: &[Spot; 16]) -> Value {
    Value::Array(s.iter().map(|p| p.map_or(Value::Null, |(x, y)| json!([x, y]))).collect())
}

/// Plays a shot to rest (as the rules do), recording the frames.
fn film(sim: &mut Simulation) -> Vec<Vec<Value>> {
    let mut last = spots_of(&sim.balls);
    let mut frames = Vec::new();
    let mut next = 1.0 / FPS;
    let push = |sim: &Simulation, last: &mut [Spot; 16], frames: &mut Vec<Vec<Value>>| {
        let now = spots_of(&sim.balls);
        let mut f = Vec::new();
        for (k, (a, b)) in now.iter().zip(last.iter()).enumerate() {
            if a != b {
                f.push(json!(k));
                match a {
                    Some((x, y)) => {
                        f.push(json!(x));
                        f.push(json!(y));
                    }
                    None => {
                        f.push(Value::Null);
                        f.push(Value::Null);
                    }
                }
            }
        }
        *last = now;
        frames.push(f);
    };
    while !sim.is_settled() && sim.time() <= sim.params.max_shot_duration {
        sim.step();
        if sim.time() >= next {
            next += 1.0 / FPS;
            push(sim, &mut last, &mut frames);
        }
    }
    sim.run_to_rest();
    push(sim, &mut last, &mut frames);
    // Trailing frames where nothing moved any more.
    while frames.len() > 1 && frames.last().is_some_and(|f| f.is_empty()) {
        frames.pop();
    }
    frames
}

pub fn pocket_name(p: PocketId) -> &'static str {
    p.name()
}

fn foul_text(f: &Foul) -> String {
    f.to_string()
}

/// The viewer's data for a game: the table, then every stroke.
pub fn view(replay: &Replay) -> Value {
    let mut game: EightBallGame = replay.start();
    let table = game.table().clone();
    let mut strokes = Vec::new();
    let mut error = Value::Null;
    for stroke in &replay.strokes {
        let seat = game.current_player();
        let start = spots_of(game.balls());
        match *stroke {
            Stroke::Shot { shot, called, cue_ball, timeout } => {
                let in_hand = game.ball_in_hand().is_some();
                if let Err(e) = place_cue_ball(&mut game, cue_ball) {
                    error = json!(e);
                    break;
                }
                let start = spots_of(game.balls());
                let top_speed = game.max_cue_speed();
                let mut pending = match game.prepare_shot(&shot, called) {
                    Ok(p) => p,
                    Err(e) => {
                        error = json!(e.to_string());
                        break;
                    }
                };
                let called = pending.called_pocket;
                let frames = film(&mut pending.simulation);
                let report = game.finish_shot(pending);
                let pocketed: Vec<Value> = report
                    .pocketed
                    .iter()
                    .map(|(b, p)| json!({ "ball": b.0, "pocket": pocket_name(*p) }))
                    .collect();
                strokes.push(json!({
                    "kind": "shot",
                    "seat": seat,
                    "break": report.was_break,
                    "in_hand": in_hand,
                    "timeout": timeout,
                    "called": called.map(pocket_name),
                    "first": report.first_contact.map(|b| b.0),
                    "pocketed": pocketed,
                    "fouls": report.fouls.iter().map(foul_text).collect::<Vec<_>>(),
                    "continues": report.continues,
                    "power": (shot.speed / top_speed * 100.0).round(),
                    "start": spots_json(&start),
                    "frames": frames,
                }));
            }
            other => {
                if let Err(e) = snook_legend_proto::replay::apply(&mut game, &other) {
                    error = json!(e);
                    break;
                }
                let (kind, seat) = match other {
                    Stroke::TimeFoul => ("time_foul", seat),
                    Stroke::Resign { seat } => ("resign", seat % 2),
                    Stroke::Abandon { seat } => ("abandon", seat % 2),
                    Stroke::Shot { .. } => unreachable!(),
                };
                strokes.push(json!({
                    "kind": kind, "seat": seat, "start": spots_json(&start), "frames": [],
                }));
            }
        }
    }
    let over = game.game_over();
    json!({
        "fps": FPS,
        "table": {
            "length": (table.spec.length * 1000.0).round(),
            "width": (table.spec.width * 1000.0).round(),
            "ball_radius": table.spec.ball_radius * 1000.0,
            "pockets": table.pockets.iter().map(|p| json!({
                "name": pocket_name(p.id),
                "x": (p.hole_center.x * 1000.0).round(),
                "y": (p.hole_center.y * 1000.0).round(),
                "r": (p.hole_radius * 1000.0).round(),
            })).collect::<Vec<_>>(),
            "cushions": table.cushions.iter().map(|c| json!([
                (c.a.x * 1000.0).round(), (c.a.y * 1000.0).round(),
                (c.b.x * 1000.0).round(), (c.b.y * 1000.0).round(),
            ])).collect::<Vec<_>>(),
        },
        "breaker": replay.breaker,
        "strokes": strokes,
        "end": spots_json(&spots_of(game.balls())),
        "over": over.map(|o| json!({ "winner": o.winner, "reason": crate::reason_code(o.reason) })),
        "error": error,
    })
}
