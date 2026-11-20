//! The official rules ([`Rules::Official`]): 8-Ball as the World
//! Pool-Billiard Association's Rules of Play (effective 2025-09-15) have
//! it, section 4 and the general rules it refers to.
//!
//! - The break (4.3): the cue ball in hand behind the head string, nothing
//!   called. A ball made without a foul keeps the table, which stays open.
//!   With no ball made, at least four object balls must reach a cushion, or
//!   the break is illegal and the incoming player plays the table as it
//!   lies, or breaks again themselves, or has the breaker break again. The
//!   8 made on a legal break is spotted and the breaker plays on, or breaks
//!   again; made on a foul, the incoming player has it spotted and the cue
//!   ball in hand behind the head string, or breaks again. A ball off the
//!   table on the break is a foul and stays off (the 8 is spotted): the
//!   incoming player plays the table as it lies or takes the cue ball in
//!   hand behind the head string; after a scratch they have it in hand
//!   there. These decisions are [`BreakChoice`]s.
//! - Open table (4.4): the break never decides the groups. Before they are
//!   decided any ball may be hit first but the 8 (unless a group is off the
//!   table: then the shooter may claim it and play the 8), and the group of
//!   the called ball legally made is the shooter's.
//! - Calls (1.7, 4.6): every shot but the break is called. The game takes
//!   the pocket as the call: a ball of the shooter's group (on an open
//!   table, of either group; on the 8, the 8) made in the called pocket on
//!   a shot without a foul keeps the table; anything else made stays down.
//!   No call is a safety: play passes at the end of the shot.
//! - Fouls (4.9): a scratch or the cue ball off the table, no ball hit or
//!   the wrong one hit first, no ball made and none driven to a cushion
//!   after the contact, an object ball off the table (it stays off), a
//!   ball behind the head string hit first from the cue ball in hand
//!   behind it without crossing it (3.11), the shot clock (3.14). The
//!   opponent has the cue ball in hand anywhere on the table.
//! - Losing (4.8): making the 8 on a foul, before one's group is off the
//!   table, or in a pocket not called; driving the 8 off the table. Not on
//!   the break.
//! - Spotting (4.7, 1.5): only the 8, after the break, on the foot spot or
//!   the long string behind it. When the cue ball is in hand behind the
//!   head string and every ball the shooter may hit lies behind it too,
//!   the one nearest the head string is spotted (1.6).

use super::*;

impl EightBallGame {
    pub(super) fn finish_official(&mut self, pending: PendingShot) -> ShotReport {
        if self.is_break {
            return self.finish_official_break(pending);
        }
        let sim = &pending.simulation;
        let shooter = self.current;
        let opponent = 1 - shooter;
        let call = pending.called_pocket;
        let on_eight = self.is_on_eight(shooter);
        let from_kitchen = self.ball_in_hand == Some(BallInHand::Kitchen);
        let events = sim.events();
        let facts = Facts::of(events);
        let first_contact = facts.first_contact();

        let mut fouls = Vec::new();
        if facts.cue_pocketed() {
            fouls.push(Foul::CueBallPocketed);
        }
        if facts.cue_off() {
            fouls.push(Foul::CueBallOffTable);
        }
        for &b in facts.off_table.iter().filter(|b| b.is_object()) {
            fouls.push(Foul::ObjectBallOffTable(b));
        }
        match facts.first_contact {
            None => fouls.push(Foul::NoContact),
            Some((t0, hit)) => {
                let legal = match self.groups[shooter] {
                    Some(_) if on_eight => hit == BallId::EIGHT,
                    Some(g) => g.contains(hit),
                    // On an open table any ball but the 8, which only a
                    // shooter with a group off the table may play.
                    None => hit != BallId::EIGHT || on_eight,
                };
                if !legal {
                    fouls.push(Foul::WrongBallFirst(hit));
                }
                // From the cue ball in hand behind the head string, a ball
                // lying behind it is hit only after crossing it (the ball
                // hit first lay still until then: it is where it was). One
                // on the head string is playable.
                let behind = self.ball(hit).xy().x < self.table.head_string_x();
                let crossed = sim.crossed_at().is_some_and(|t| t <= t0);
                if from_kitchen && behind && !crossed {
                    fouls.push(Foul::BallBehindHeadString(hit));
                }
                // Something made, or some ball driven to a cushion after
                // the contact.
                let rail_after_contact =
                    events.iter().any(|e| matches!(e, Event::Cushion { time, .. } if *time >= t0));
                if facts.pocketed.is_empty() && !rail_after_contact {
                    fouls.push(Foul::NoRailAfterContact);
                }
            }
        }

        // The 8. A shooter on an open table with a group off it plays the 8
        // only by hitting it first.
        let shooting_eight =
            on_eight && (self.groups[shooter].is_some() || first_contact == Some(BallId::EIGHT));
        let lost = |reason| Some(GameOver { winner: opponent, reason });
        let mut game_over = None;
        if facts.off_table.contains(&BallId::EIGHT) {
            game_over = lost(GameOverReason::EightOffTable);
        } else if let Some(pocket) = facts.pocket_of(BallId::EIGHT) {
            game_over = if !fouls.is_empty() {
                lost(GameOverReason::EightPocketedOnFoul)
            } else if !shooting_eight {
                lost(GameOverReason::EightPocketedEarly)
            } else if call != Some(pocket) {
                lost(GameOverReason::EightInWrongPocket)
            } else {
                Some(GameOver { winner: shooter, reason: GameOverReason::PocketedEight })
            };
        }

        // The called ball: one the shooter may call, made in the called
        // pocket on a shot without a foul. On an open table the first one
        // made there decides the groups.
        let callable = |b: BallId| match self.groups[shooter] {
            Some(g) => g.contains(b),
            None => Group::of(b).is_some(),
        };
        // A shooter on an open table who played the 8 (their group off the
        // table) shot at the 8: a ball of the other group made with it is
        // not theirs to take.
        let at_eight_open = shooting_eight && self.groups[shooter].is_none();
        let made_call = match call {
            Some(c) if game_over.is_none() && fouls.is_empty() && !at_eight_open => {
                facts.pocketed.iter().find(|(b, p)| *p == c && callable(*b)).map(|(b, _)| *b)
            }
            _ => None,
        };
        let mut group_assigned = None;
        if self.groups[shooter].is_none()
            && let Some(g) = made_call.and_then(Group::of)
        {
            self.groups[shooter] = Some(g);
            self.groups[opponent] = Some(g.opposite());
            group_assigned = Some(g);
        }
        let continues = game_over.is_none() && made_call.is_some();

        // Commit the new ball positions: balls made and balls off the table
        // stay where they are, out of play.
        self.balls = sim.balls.clone();
        self.balls.sort_by_key(|b| b.id);
        self.shots_taken += 1;
        self.game_over = game_over;
        self.ball_in_hand = None;
        if game_over.is_none() {
            if !fouls.is_empty() {
                self.grant_hand(BallInHand::Anywhere);
            }
            if !continues {
                self.current = opponent;
            }
        }

        ShotReport {
            shooter,
            was_break: false,
            first_contact,
            pocketed: facts.pocketed,
            fouls,
            spotted: Vec::new(),
            group_assigned,
            continues,
            game_over,
            called: call,
            opponent_ball: false,
            penalty: Vec::new(),
            break_again: false,
            choice: None,
        }
    }

    /// The break (4.3).
    fn finish_official_break(&mut self, pending: PendingShot) -> ShotReport {
        let sim = &pending.simulation;
        let shooter = self.current;
        let opponent = 1 - shooter;
        let facts = Facts::of(sim.events());
        let object_pocketed = facts.object_pocketed();
        let scratch = facts.cue_pocketed() || facts.cue_off();
        let objects_off: Vec<BallId> =
            facts.off_table.iter().copied().filter(|b| b.is_object()).collect();
        // The object balls driven to a cushion: those that touched one, and
        // those made or off the table (2.7).
        let mut to_rail: Vec<BallId> = sim
            .events()
            .iter()
            .filter_map(|e| match *e {
                Event::Cushion { ball, .. } if ball.is_object() => Some(ball),
                _ => None,
            })
            .chain(object_pocketed.iter().copied())
            .chain(objects_off.iter().copied())
            .collect();
        to_rail.sort();
        to_rail.dedup();
        let legal = !object_pocketed.is_empty() || to_rail.len() >= 4;
        let foul = scratch || !objects_off.is_empty();
        let eight_down = facts.pocket_of(BallId::EIGHT).is_some();

        let mut fouls = Vec::new();
        if facts.cue_pocketed() {
            fouls.push(Foul::CueBallPocketed);
        }
        if facts.cue_off() {
            fouls.push(Foul::CueBallOffTable);
        }
        for &b in &objects_off {
            fouls.push(Foul::ObjectBallOffTable(b));
        }
        if !legal {
            fouls.push(Foul::IllegalBreak);
        }

        // Commit the new ball positions. The 8 off the table is spotted at
        // once; the 8 made waits for the choice (spotted, or racked again).
        self.balls = sim.balls.clone();
        self.balls.sort_by_key(|b| b.id);
        let mut spotted = Vec::new();
        if objects_off.contains(&BallId::EIGHT) {
            self.spot_ball(BallId::EIGHT);
            spotted.push(BallId::EIGHT);
        }
        self.shots_taken += 1;
        self.is_break = false;
        self.ball_in_hand = None;

        let choice = if eight_down {
            Some(if foul { BreakChoice::EightOnFoulBreak } else { BreakChoice::EightOnBreak })
        } else if !legal {
            Some(BreakChoice::IllegalBreak { scratch })
        } else if foul && !scratch {
            Some(BreakChoice::BallOffOnBreak)
        } else {
            None
        };
        let continues = choice.is_none() && !foul && !object_pocketed.is_empty();
        if let Some(c) = choice {
            // The breaker decides after the 8 made on a legal break; the
            // incoming player after anything else. Meanwhile the cue ball
            // waits on the table (behind the head string, if it went down).
            self.current = if c == BreakChoice::EightOnBreak { shooter } else { opponent };
            if scratch {
                self.respot_cue_ball();
            }
            self.choice = Some(c);
        } else if scratch {
            // A scratch on a legal break: the cue ball in hand behind the
            // head string for the incoming player.
            self.current = opponent;
            self.grant_hand(BallInHand::Kitchen);
        } else if !continues {
            self.current = opponent;
        }

        ShotReport {
            shooter,
            was_break: true,
            first_contact: facts.first_contact(),
            pocketed: facts.pocketed,
            fouls,
            spotted,
            group_assigned: None,
            continues,
            game_over: None,
            called: None,
            opponent_ball: false,
            penalty: Vec::new(),
            break_again: false,
            choice,
        }
    }

    /// Applies the current player's answer to a break's choice.
    pub(super) fn apply_choice(&mut self, choice: BreakChoice, option: BreakOption) {
        let chooser = self.current;
        self.choice = None;
        match (choice, option) {
            (_, BreakOption::BreakAgain) => self.rerack(chooser),
            (_, BreakOption::OpponentBreaks) => self.rerack(1 - chooser),
            (BreakChoice::IllegalBreak { scratch }, _) => {
                if scratch {
                    self.grant_hand(BallInHand::Kitchen);
                }
            }
            (BreakChoice::EightOnBreak, _) => self.spot_ball(BallId::EIGHT),
            (BreakChoice::EightOnFoulBreak, _) => {
                self.spot_ball(BallId::EIGHT);
                self.grant_hand(BallInHand::Kitchen);
            }
            (BreakChoice::BallOffOnBreak, BreakOption::CueBallInHand) => {
                self.grant_hand(BallInHand::Kitchen);
            }
            (BreakChoice::BallOffOnBreak, _) => {}
        }
    }

    /// Gives the current player the cue ball in hand, `where` it may go:
    /// it is put on the head spot (or the nearest free place behind the head
    /// string) when it is off the table or where it lies is not allowed.
    /// In hand behind the head string with every ball they may hit lying
    /// behind it too, the one nearest the head string is spotted (1.6).
    pub(super) fn grant_hand(&mut self, place: BallInHand) {
        self.ball_in_hand = Some(place);
        let cue = self.ball(BallId::CUE);
        if !cue.in_play() || !self.can_place_cue_ball(cue.xy()) {
            self.respot_cue_ball();
        }
        if place == BallInHand::Kitchen {
            let line = self.table.head_string_x();
            let targets = self.legal_targets();
            if targets.iter().all(|b| self.ball(*b).xy().x < line)
                && let Some(nearest) = targets
                    .into_iter()
                    .max_by(|a, b| self.ball(*a).xy().x.total_cmp(&self.ball(*b).xy().x))
            {
                self.spot_ball(nearest);
            }
        }
    }
}
