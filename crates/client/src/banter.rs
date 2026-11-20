//! What the computer opponents say during a game: a line now and then in a
//! speech bubble, about the player's shots and their own, in the voice of
//! each personality. English here, translated by the game.

/// A moment of the game worth a remark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Moment {
    Start,
    /// The player made a ball (and keeps the table).
    YouPot,
    YouMiss,
    YouFoul,
    /// The computer made a ball.
    IPot,
    IMiss,
    IFoul,
    /// The game is over.
    IWin,
    YouWin,
}

impl Moment {
    /// How often the opponent speaks up at this moment (0 to 1).
    pub fn chance(self) -> f32 {
        match self {
            Moment::Start | Moment::IWin | Moment::YouWin => 1.0,
            Moment::YouFoul | Moment::IFoul => 0.7,
            Moment::YouMiss | Moment::IMiss => 0.35,
            Moment::YouPot => 0.3,
            Moment::IPot => 0.2,
        }
    }

    /// Spoken even right after another line (the start and the end).
    pub fn always(self) -> bool {
        matches!(self, Moment::Start | Moment::IWin | Moment::YouWin)
    }
}

/// The lines of personality `code` for a moment (the classic levels speak
/// like a polite machine).
pub fn lines(code: &str, moment: Moment) -> &'static [&'static str] {
    use Moment::*;
    match (code, moment) {
        // Leo, the rookie: eager, a little lost, always cheerful.
        ("leo", Start) => &["Hi! I watched a lot of videos, I'm ready!", "Be gentle, okay?"],
        ("leo", YouPot) => &["Whoa, how did you do that?", "Teach me that one later!"],
        ("leo", YouMiss) => &["Phew! My turn!", "Even you miss? Good to know."],
        ("leo", YouFoul) => &["A free ball! What do I do with it?"],
        ("leo", IPot) => &["Did you see that?! It went in!", "Beginner's luck counts too!"],
        ("leo", IMiss) => &["Oops. That was a practice shot.", "Aim, Leo, aim..."],
        ("leo", IFoul) => &["Sorry! I touched the wrong one again."],
        ("leo", IWin) => &["I WON? I'm calling my mum!"],
        ("leo", YouWin) => &["Good game! I'm getting better, right?"],

        // Nina, the weekend player: relaxed, friendly, in it for the fun.
        ("nina", Start) => &["Saturday, a table and good company. Let's play!"],
        ("nina", YouPot) => &["Nice one!", "Oh, you're good. I'll pay attention now."],
        ("nina", YouMiss) => &["Happens to the best of us.", "Don't worry, it's just a game!"],
        ("nina", YouFoul) => &["Thanks for the gift!"],
        ("nina", IPot) => &["Yes! Weekend skills!", "Lucky me!"],
        ("nina", IMiss) => &["Hm, where was I aiming?", "Too much chatting, not enough aiming."],
        ("nina", IFoul) => &["Oops, my bad!"],
        ("nina", IWin) => &["That was fun! Same time next weekend?"],
        ("nina", YouWin) => &["Well played! Rematch over a lemonade?"],

        // Lola, the bank artist: sees angles everywhere.
        ("lola", Start) => &["Four cushions, endless possibilities."],
        ("lola", YouPot) => {
            &["Straight in? So... direct.", "Nice, but a bank would have been prettier."]
        }
        ("lola", YouMiss) => {
            &["You should have used the cushion.", "The rail was right there, asking for it."]
        }
        ("lola", YouFoul) => &["The table didn't like that."],
        ("lola", IPot) => &["Off the rail and in. Art!", "Geometry never lies."],
        ("lola", IMiss) => {
            &["The angle was right. The cushion disagreed.", "One diamond off. Just one!"]
        }
        ("lola", IFoul) => &["Too much imagination, not enough contact."],
        ("lola", IWin) => &["A masterpiece, if I may say so."],
        ("lola", YouWin) => &["Well played. Now try it off two rails."],

        // Bruno, the cannon: everything hard, everything loud.
        ("bruno", Start) => &["I hope these balls are made of steel."],
        ("bruno", YouPot) => &["Soft touch, huh? Not my style.", "Hit it harder next time!"],
        ("bruno", YouMiss) => &["Too gentle! Power solves everything.", "HA!"],
        ("bruno", YouFoul) => &["My table now. Cover your ears."],
        ("bruno", IPot) => &["BOOM!", "Did you hear that pocket?"],
        ("bruno", IMiss) => &["Too much power? No such thing.", "The pocket moved."],
        ("bruno", IFoul) => &["The ball flew. It happens. A lot."],
        ("bruno", IWin) => &["Power wins again! BOOM!"],
        ("bruno", YouWin) => &["Fine, finesse works too. Sometimes."],

        // Marta, the wall: patient, dry, loves a good safety.
        ("marta", Start) => &["Take your time. I have all day."],
        ("marta", YouPot) => &["Acceptable.", "Enjoy it while it lasts."],
        ("marta", YouMiss) => &["Patience is a virtue. Not yours, apparently.", "Thank you."],
        ("marta", YouFoul) => &["Sloppy."],
        ("marta", IPot) => &["Steady wins.", "One at a time."],
        ("marta", IMiss) => &["Hm. I'll leave you nothing anyway.", "Intentional. Probably."],
        ("marta", IFoul) => &["Unacceptable. Of me."],
        ("marta", IWin) => &["As I said: patience."],
        ("marta", YouWin) => &["You got through the wall. I'm impressed."],

        // Otto, the old pro: stories from the old days, quiet wisdom.
        ("otto", Start) => &["I've been playing since before these balls were plastic."],
        ("otto", YouPot) => {
            &["Good stroke. Reminds me of a fellow in '78.", "Smooth. Very smooth."]
        }
        ("otto", YouMiss) => &["Keep your head down, kid.", "The cue ball doesn't lie."],
        ("otto", YouFoul) => &["Ah, a little present. Thank you kindly."],
        ("otto", IPot) => &["Like riding a bicycle.", "Still got it."],
        ("otto", IMiss) => {
            &["The eyes aren't what they used to be.", "In my day the pockets were bigger."]
        }
        ("otto", IFoul) => &["Even old pros slip."],
        ("otto", IWin) => &["Experience, my friend. Experience."],
        ("otto", YouWin) => &["You've got a future in this game."],

        // Rosa, the hustler: sly, teasing, always playing a little game.
        ("rosa", Start) => &["Me? Oh, I hardly ever play..."],
        ("rosa", YouPot) => &["Lucky! Do it again, I dare you.", "Ooh, someone's been practising."],
        ("rosa", YouMiss) => &["Aw, so close! Bless.", "Want me to show you how?"],
        ("rosa", YouFoul) => &["Thank you, sweetie."],
        ("rosa", IPot) => &["Oops, did I do that?", "Must be beginner's luck. Again."],
        ("rosa", IMiss) => &["Missed on purpose. Keeps it interesting.", "Just warming up."],
        ("rosa", IFoul) => &["Oh dear, how clumsy of me. Wink."],
        ("rosa", IWin) => &["Hardly ever play, I said. Hardly."],
        ("rosa", YouWin) => &["Fine, you got me. This time."],

        // Kenji, the professor: physics, angles and statistics.
        ("kenji", Start) => &["Friction, momentum, spin. Let us begin."],
        ("kenji", YouPot) => &["A fine cut. About thirty degrees, I'd say.", "Textbook."],
        ("kenji", YouMiss) => {
            &["You underestimated the throw.", "Interesting. Statistically expected."]
        }
        ("kenji", YouFoul) => &["The cue ball followed the laws of physics, not your plan."],
        ("kenji", IPot) => &["As calculated.", "Elastic collision, elegant result."],
        ("kenji", IMiss) => &["An error in my measurement.", "I'll revise the model."],
        ("kenji", IFoul) => &["Hm. Unexpected spin."],
        ("kenji", IWin) => &["The numbers were on my side."],
        ("kenji", YouWin) => &["A result outside my predictions. Well done."],

        // Sofia, the sharpshooter: precise, focused, few words.
        ("sofia", Start) => &["Focus."],
        ("sofia", YouPot) => &["Clean.", "Not bad."],
        ("sofia", YouMiss) => &["Half a millimetre.", "Breathe before you shoot."],
        ("sofia", YouFoul) => &["Careless."],
        ("sofia", IPot) => &["Target down.", "Centre pocket."],
        ("sofia", IMiss) => &["Hm. Recalibrating.", "That never happens."],
        ("sofia", IFoul) => &["Unacceptable."],
        ("sofia", IWin) => &["Precision wins."],
        ("sofia", YouWin) => &["Good shooting."],

        // Viktor, the champion: confident, gracious, a showman.
        ("viktor", Start) => &["Welcome to my table. Enjoy the show."],
        ("viktor", YouPot) => {
            &["Very good. You've done your homework.", "Applause for the challenger!"]
        }
        ("viktor", YouMiss) => &["Pressure. It gets to everyone.", "Thank you for the table."],
        ("viktor", YouFoul) => &["A champion never refuses a gift."],
        ("viktor", IPot) => &["Naturally.", "That's how it's done."],
        ("viktor", IMiss) => {
            &["Even champions give the crowd a chance.", "Leaving you something, out of courtesy."]
        }
        ("viktor", IFoul) => &["A rare sight. Remember it."],
        ("viktor", IWin) => &["The title stays with me. Good game."],
        ("viktor", YouWin) => &["Remarkable. You've earned my respect."],

        // Tito, the gambler: everything on red, risk is the fun.
        ("tito", Start) => &["Place your bets! Mine's on me."],
        ("tito", YouPot) => &["Lucky roll. Let it ride!", "Okay, okay. Double or nothing?"],
        ("tito", YouMiss) => &["The house always wins!"],
        ("tito", YouFoul) => &["Free chips! Thank you!"],
        ("tito", IPot) => &["All in, and it paid!", "That's how you play the odds!"],
        ("tito", IMiss) => &["Bad beat. Next hand.", "Too much spin? There's no such thing."],
        ("tito", IFoul) => &["Busted. Worth the risk!"],
        ("tito", IWin) => &["Jackpot! Same stakes again?"],
        ("tito", YouWin) => &["You beat the odds. Respect."],

        // Inês, the soft touch: quiet, kind, precise.
        ("ines", Start) => &["Softly, now. The table likes a gentle hand."],
        ("ines", YouPot) => &["Nicely done.", "A little hard, but it went in."],
        ("ines", YouMiss) => &["Less power, more feel."],
        ("ines", YouFoul) => &["Oh dear. A free ball for me."],
        ("ines", IPot) => &["Just enough to drop.", "See? No need to hit it hard."],
        ("ines", IMiss) => &["A touch too soft."],
        ("ines", IFoul) => &["Clumsy of me. Sorry."],
        ("ines", IWin) => &["Slow and steady. Thank you for the game."],
        ("ines", YouWin) => &["Beautifully played. Well deserved."],

        // Diego, the showman: plays to the crowd.
        ("diego", Start) => &["Ladies and gentlemen... the show begins!"],
        ("diego", YouPot) => &["Not bad! Bow to the crowd!", "You stole my applause!"],
        ("diego", YouMiss) => &["The crowd wanted more!"],
        ("diego", YouFoul) => &["Ooh, a plot twist!"],
        ("diego", IPot) => &["Ta-da!", "And for my next trick..."],
        ("diego", IMiss) => &["Even magicians have off nights."],
        ("diego", IFoul) => &["That was... part of the act."],
        ("diego", IWin) => &["Thank you, thank you! Encore?"],
        ("diego", YouWin) => &["Take a bow, champion. You earned it."],

        // Amara, the tactician: thinks in moves, like chess.
        ("amara", Start) => &["Every rack is a puzzle. Let's solve this one."],
        ("amara", YouPot) => &["Good. Now where is your cue ball?", "I saw that coming."],
        ("amara", YouMiss) => &["That left me something. Thank you."],
        ("amara", YouFoul) => &["As planned."],
        ("amara", IPot) => &["Exactly as I calculated.", "Three moves ahead."],
        ("amara", IMiss) => &["A variable I missed."],
        ("amara", IFoul) => &["Hm. Recalculating."],
        ("amara", IWin) => &["Checkmate."],
        ("amara", YouWin) => &["You out-thought me. That's rare."],

        // Yuri, the machine: no emotion, only results.
        ("yuri", Start) => &["Game initialised."],
        ("yuri", YouPot) => &["Acceptable.", "Noted."],
        ("yuri", YouMiss) => &["Error margin exceeded."],
        ("yuri", YouFoul) => &["Penalty applied."],
        ("yuri", IPot) => &["As expected.", "Next."],
        ("yuri", IMiss) => &["Deviation logged."],
        ("yuri", IFoul) => &["Malfunction. It will not repeat."],
        ("yuri", IWin) => &["Result: win. Thank you for the data."],
        ("yuri", YouWin) => &["Unexpected outcome. Impressive."],

        // The classic levels: a polite machine.
        (_, Start) => &["Calibration complete. Good luck, human."],
        (_, YouPot) => &["Shot registered: successful."],
        (_, YouMiss) => &["Probability of my victory: increasing."],
        (_, YouFoul) => &["Foul detected. Processing penalty."],
        (_, IPot) => &["Executed as planned."],
        (_, IMiss) => &["Error. Recalculating."],
        (_, IFoul) => &["Malfunction. Please disregard."],
        (_, IWin) => &["Game complete. Thank you for playing."],
        (_, YouWin) => &["Defeat acknowledged. Well played."],
    }
}

/// Every line of every voice (for the translation test).
#[cfg(test)]
pub fn all_lines() -> Vec<&'static str> {
    use Moment::*;
    let moments = [Start, YouPot, YouMiss, YouFoul, IPot, IMiss, IFoul, IWin, YouWin];
    // Every named opponent, and the classic levels ("").
    let codes = snook_legend_core::personalities::PERSONALITIES.iter().map(|p| p.code).chain([""]);
    codes.flat_map(|c| moments.iter().flat_map(move |m| lines(c, *m).iter().copied())).collect()
}

/// A number from 0 to 1 that changes with each input (deterministic, so a
/// game's banter can't be told from its seed alone but stays reproducible).
pub fn roll(a: u64, b: u64) -> f32 {
    let mut h = a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    (h % 10_000) as f32 / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_line_is_translated() {
        use crate::i18n::Lang;
        for line in all_lines() {
            for lang in [Lang::Portuguese, Lang::Spanish] {
                assert!(crate::i18n::has_translation(lang, line), "{lang:?}: {line}");
            }
        }
    }
}
