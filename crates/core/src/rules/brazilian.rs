//! The Brazilian rules of the bars and pool halls ([`Rules::Brazilian`]).
//!
//! - A foul costs the turn and gives the opponent balls: the lowest numbers
//!   of their group still on the table come off it, as if they had made
//!   them. One for missing every ball, one for a scratch or the cue ball off
//!   the table, two for hitting one of the opponent's balls first, or one
//!   when the cue ball went to a cushion before it. The counts add up (a
//!   miss that scratches gives two; the 8 hit first before one's group is
//!   cleared counts as a miss: one). While the table is open there is no
//!   opponent's group yet, and a foul costs the turn alone.
//! - When the opponent has fewer balls left than the foul gives them, the 8
//!   pays the rest and the foul loses the game: with only the 8 left for
//!   them, any foul that gives a ball (a miss, the cue ball down or off the
//!   table, the wrong ball first) loses; two owed with one left, that one
//!   and the 8 come off.
//! - The cue ball is in hand for the break only (behind the head string).
//!   One that goes down or leaves the table, on the break or later, goes
//!   back to the head spot (right behind a ball lying there) and is played
//!   from there, along with the foul's penalty.
//! - A break that hits no ball, or that loses the cue ball (down or off the
//!   table) without making a ball or together with balls of both groups
//!   (a solid and a stripe, in any order), hands the break to the
//!   opponent: no penalty ball, the groups still open, the balls stay where
//!   they are (the ones made stay down) and their shot is a break: from the
//!   head spot when the cue ball was lost (not in hand), in hand behind the
//!   head string when the rack was never hit.
//! - The break alternates between the players, one break each (the client
//!   and the server pick the breaker; see `Match::set_breaker`).
//! - The first ball made decides the groups, on the break too and whatever
//!   goes down after it (the 3 and then the 13: solids for the breaker),
//!   even on a break with a foul when the balls made are of one group (the
//!   cue ball down with them: the turn passes and the foul's penalty comes
//!   from the opponent's group). One of the opponent's balls made passes the
//!   turn, made for them, on any shot.
//! - A ball of the shooter's knocked off the table comes back to it; one of
//!   the opponent's stays off, made for them.
//! - The 8 alone on the break wins; the 8 on the break with any other ball
//!   loses. (The break of the rack: on a break handed over with the balls
//!   scattered, the 8 hit first is a foul and the 8 made loses, as on any
//!   shot.) At the end, the 8 loses if the cue ball or one of the
//!   opponent's balls goes down with it. The pocket called for the 8 is
//!   binding from the first shot taken with it.
//! - Nothing has to reach a cushion, on the break or after a contact.
//! - The shot clock running out is a foul that gives a ball, with the cue
//!   ball back on the head spot (see [`EightBallGame::time_foul`]).

use super::*;

impl EightBallGame {
    pub(super) fn finish_brazilian(&mut self, pending: PendingShot) -> ShotReport {
        let sim = &pending.simulation;
        let shooter = self.current;
        let opponent = 1 - shooter;
        let was_break = self.is_break;
        // The 8's rules on the break are for the break of the rack: on one
        // handed over with the balls scattered it is hit and made as on any
        // other shot (first is a foul; down early loses).
        let rack_break = self.break_on_rack();
        let on_eight = self.is_on_eight(shooter);
        let open_table = self.groups[shooter].is_none();
        if on_eight && !rack_break && pending.called_pocket.is_some() {
            self.called[shooter] = pending.called_pocket;
        }
        let events = sim.events();
        let facts = Facts::of(events);
        let first_contact = facts.first_contact();
        let object_pocketed = facts.object_pocketed();
        let cue_pocketed = facts.cue_pocketed();
        let cue_off = facts.cue_off();
        // The cue ball went to a cushion before it reached a ball.
        let cushion_first = facts.first_contact.is_some_and(|(t0, _)| {
            events.iter().any(
                |e| matches!(e, Event::Cushion { time, ball, frozen: false, .. } if *ball == BallId::CUE && *time < t0),
            )
        });

        let mut fouls = Vec::new();
        // How many of the opponent's balls the fouls give them.
        let mut owed = 0usize;
        if cue_pocketed {
            fouls.push(Foul::CueBallPocketed);
            owed += 1;
        }
        if cue_off {
            fouls.push(Foul::CueBallOffTable);
            owed += 1;
        }
        for &b in facts.off_table.iter().filter(|b| b.is_object()) {
            fouls.push(Foul::ObjectBallOffTable(b));
        }
        match first_contact {
            None => {
                fouls.push(Foul::NoContact);
                owed += 1;
            }
            Some(hit) => {
                let legal = if rack_break {
                    true
                } else if on_eight {
                    hit == BallId::EIGHT
                } else {
                    match self.groups[shooter] {
                        Some(g) => g.contains(hit),
                        None => hit != BallId::EIGHT,
                    }
                };
                if !legal {
                    fouls.push(Foul::WrongBallFirst(hit));
                    // One of the opponent's balls hit directly costs two.
                    // Off a cushion it costs one, and so does the 8 hit
                    // too soon (it is nobody's ball: the same as hitting
                    // none of one's own).
                    let theirs = self.groups[opponent].is_some_and(|g| g.contains(hit));
                    owed += if theirs && !cushion_first { 2 } else { 1 };
                }
            }
        }

        // A break that loses the cue ball with balls of both groups is
        // handed over like one that makes nothing (see below).
        let both_groups = Group::ALL.iter().all(|g| object_pocketed.iter().any(|b| g.contains(*b)));
        let break_lost_with_both = was_break && (cue_pocketed || cue_off) && both_groups;

        let eight_pocket = facts.pocket_of(BallId::EIGHT);
        let eight_off = facts.off_table.contains(&BallId::EIGHT);
        let lost = |reason| Some(GameOver { winner: opponent, reason });
        let mut game_over = None;
        let mut group_assigned = None;
        let mut spotted = Vec::new();
        if rack_break {
            if eight_pocket.is_some() {
                game_over = if facts.pocketed.len() > 1 {
                    lost(GameOverReason::EightWithAnotherBall)
                } else if !fouls.is_empty() {
                    lost(GameOverReason::EightPocketedOnFoul)
                } else {
                    Some(GameOver { winner: shooter, reason: GameOverReason::PocketedEight })
                };
            } else if eight_off {
                spotted.push(BallId::EIGHT);
            }
        } else if eight_off {
            game_over = lost(GameOverReason::EightOffTable);
        } else if let Some(pocket) = eight_pocket {
            let with_opponents_ball = object_pocketed.iter().any(|b| *b != BallId::EIGHT);
            game_over = if !on_eight {
                lost(GameOverReason::EightPocketedEarly)
            } else if !fouls.is_empty() {
                lost(GameOverReason::EightPocketedOnFoul)
            } else if with_opponents_ball {
                lost(GameOverReason::EightWithAnotherBall)
            } else if pending.called_pocket != Some(pocket) {
                lost(GameOverReason::EightInWrongPocket)
            } else {
                Some(GameOver { winner: shooter, reason: GameOverReason::PocketedEight })
            };
        }
        // The first ball made decides the groups: on a shot without a foul,
        // and on the break with one too (the 11 made and the cue ball down:
        // stripes for the breaker, and the foul's penalty is paid from the
        // opponent's group), unless the break is handed over.
        if game_over.is_none()
            && self.groups[shooter].is_none()
            && (fouls.is_empty() || was_break)
            && !break_lost_with_both
            && let Some(g) = object_pocketed.iter().find_map(|b| Group::of(*b))
        {
            self.groups[shooter] = Some(g);
            self.groups[opponent] = Some(g.opposite());
            group_assigned = Some(g);
        }
        let theirs = self.groups[opponent];
        // Balls knocked off the table: the opponent's stay off, the others
        // come back.
        let mut penalty = Vec::new();
        for &b in facts.off_table.iter().filter(|b| b.is_object() && **b != BallId::EIGHT) {
            if theirs.is_some_and(|g| g.contains(b)) {
                penalty.push(b);
            } else {
                spotted.push(b);
            }
        }

        // One of the opponent's balls made passes the turn, on the break
        // too (the first ball made gave the shooter the other kind).
        let opponent_ball = self.groups[shooter]
            .is_some_and(|g| object_pocketed.iter().any(|b| g.opposite().contains(*b)));
        let continues = game_over.is_none()
            && fouls.is_empty()
            && !opponent_ball
            && if was_break {
                !object_pocketed.is_empty()
            } else {
                match self.groups[shooter] {
                    Some(g) => object_pocketed.iter().any(|b| g.contains(*b)),
                    None => false,
                }
            };

        // Commit the new ball positions.
        self.balls = sim.balls.clone();
        self.balls.sort_by_key(|b| b.id);
        for &b in &spotted {
            self.spot_ball(b);
        }
        // (The opponent's balls knocked off the table stay off it as they
        // are: gone, like the ones made.)
        if game_over.is_none()
            && let Some(g) = theirs
        {
            // Fewer left than owed: the 8 pays the rest, and the foul loses.
            let given = self.pay_penalty(g, owed);
            if given.contains(&BallId::EIGHT) {
                game_over = lost(GameOverReason::EightPocketedOnFoul);
            }
            penalty.extend(given);
        } else if game_over.is_none()
            && theirs.is_none()
            && owed > 0
            && !was_break
            && self.only_the_eight_left()
        {
            // An open table with only the 8 left: both players are on it,
            // and a foul that gives a ball loses (the 8 pays).
            self.take_off(BallId::EIGHT);
            penalty.push(BallId::EIGHT);
            game_over = lost(GameOverReason::EightPocketedOnFoul);
        }
        self.shots_taken += 1;
        self.is_break = false;
        self.game_over = game_over;
        self.ball_in_hand = None;
        // No ball hit on the break, or the cue ball lost on it with no ball
        // made or with balls of both groups: the opponent breaks from where
        // the balls lie (and, the groups still open, no ball is owed). The
        // cue ball lost: it goes on the head spot, where they break from
        // (not in hand); the rack never hit: in hand behind the head string,
        // as on any break.
        let break_again = was_break
            && game_over.is_none()
            && (cue_pocketed || cue_off || first_contact.is_none())
            && (object_pocketed.is_empty() || break_lost_with_both);
        if break_again {
            self.is_break = true;
            self.current = opponent;
            if cue_pocketed || cue_off {
                self.return_cue_ball();
            } else {
                self.respot_cue_ball();
                self.ball_in_hand = Some(BallInHand::Kitchen);
            }
        } else if game_over.is_none() {
            if cue_pocketed || cue_off {
                // Back where it began, without ball in hand.
                self.return_cue_ball();
            }
            if !continues {
                self.current = opponent;
            }
        }

        ShotReport {
            shooter,
            was_break,
            first_contact,
            pocketed: facts.pocketed,
            fouls,
            spotted,
            group_assigned,
            continues,
            game_over,
            called: pending.called_pocket.filter(|_| on_eight && !rack_break),
            opponent_ball,
            penalty,
            break_again,
            choice: None,
            open_table,
        }
    }
}
