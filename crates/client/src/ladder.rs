//! The ladder against the computer: how far the player is (kept in a file
//! next to the settings) and the claim of the reward, which the server
//! gives after checking the final game.

use serde::{Deserialize, Serialize};
use snookered_proto::ClientMsg;
use snookered_proto::ladder::{self, ClaimFailure, ClaimResult};
use snookered_proto::replay::Replay;

use crate::online::Online;

/// Bumped when the file changes incompatibly (an older file is then read
/// as far as it goes: unknown fields are ignored, missing ones default).
/// Version 2 keeps the opponents beaten by name: the ladder grew from ten
/// to fifteen, the new ones between the old.
const VERSION: u32 = 2;

/// A ladder game being played: the rung and every stroke so far (the
/// final rung's game is what the server checks).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LadderRun {
    /// 0 for the first opponent.
    pub rung: u8,
    pub record: Replay,
    /// The result has been counted.
    #[serde(default)]
    pub reported: bool,
}

/// How far the player is.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Progress {
    #[serde(default)]
    version: u32,
    /// How many opponents are beaten (kept from `won`).
    #[serde(default)]
    pub beaten: u8,
    /// The opponents beaten (personality codes, in no order).
    #[serde(default)]
    pub won: Vec<String>,
    /// The game won on the last rung, waiting to be sent to the server.
    #[serde(default)]
    pub pending: Option<Replay>,
    /// The server gave the reward to the account signed in then.
    #[serde(default)]
    pub claimed: bool,
}

/// Where the claim stands, for the ladder screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Claim {
    Idle,
    /// Sent at this time (seconds since the game started).
    Claiming(f64),
    Granted,
    Failed(Failure),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Failure {
    Server(ClaimFailure),
    /// No answer in time.
    NoAnswer,
}

/// How long to wait for the server's verdict (checking a game takes a
/// while, and it may be queued behind others).
const CLAIM_TIMEOUT: f64 = 120.0;

impl Failure {
    /// The player can simply try again later (the game itself is fine).
    pub fn retryable(self) -> bool {
        matches!(
            self,
            Failure::NoAnswer
                | Failure::Server(
                    ClaimFailure::Busy
                        | ClaimFailure::TooSoon
                        | ClaimFailure::ServerError
                        | ClaimFailure::Unknown
                )
        )
    }

    /// What went wrong, for the player.
    pub fn text(self) -> &'static str {
        use crate::i18n::t;
        match self {
            Failure::NoAnswer => t("The server did not answer. Try again in a moment."),
            Failure::Server(f) => match f {
                ClaimFailure::Busy => {
                    t("The server is busy checking other games. Try again in a moment.")
                }
                ClaimFailure::TooSoon => t("Wait a minute before trying again."),
                ClaimFailure::ServerError | ClaimFailure::Unknown => {
                    t("Something went wrong on the server. Try again later.")
                }
                ClaimFailure::TooLong => t("That game is too long to be checked."),
                ClaimFailure::Illegal => t("That game has a move the rules don't allow."),
                ClaimFailure::NotFinished => t("That game was not played to the end."),
                ClaimFailure::NotWon => t("That game was not a win."),
                ClaimFailure::ComputerMismatch => t(
                    "The computer's shots in that game don't match how it plays (another version of the game?).",
                ),
            },
        }
    }
}

fn path() -> Option<std::path::PathBuf> {
    crate::config::config_dir().map(|d| d.join("ladder.json"))
}

impl Progress {
    /// The progress on disk. A damaged file falls back to the copy kept
    /// before the last save, then to a fresh ladder.
    pub fn load() -> Progress {
        let Some(p) = path() else { return Progress::default() };
        Self::read(&p).or_else(|| Self::read(&p.with_extension("json.bak"))).unwrap_or_default()
    }

    fn read(p: &std::path::Path) -> Option<Progress> {
        let text = std::fs::read_to_string(p).ok()?;
        Progress::parse(&text)
    }

    /// Reads a saved progress; nonsense values are brought back into range.
    pub fn parse(text: &str) -> Option<Progress> {
        let mut p: Progress = serde_json::from_str(text).ok()?;
        // A file from the ladder of ten counted the rungs beaten, in order:
        // those opponents are still beaten.
        if p.version < 2 && p.won.is_empty() {
            let n = (p.beaten as usize).min(ladder::RUNGS_OF_TEN.len());
            p.won = ladder::RUNGS_OF_TEN[..n].iter().map(|c| c.to_string()).collect();
        }
        p.won.retain(|c| ladder::RUNGS.contains(&c.as_str()));
        p.won.sort();
        p.won.dedup();
        p.beaten = p.won.len() as u8;
        if p.pending.as_ref().is_some_and(|r| r.strokes.len() > ladder::MAX_STROKES) {
            p.pending = None;
        }
        Some(p)
    }

    pub fn save(&self) {
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut me = self.clone();
        me.version = VERSION;
        let Ok(text) = serde_json::to_string(&me) else { return };
        // The last good file is kept as a backup; the new one is written
        // beside and moved over, so a crash never leaves half a file.
        if Self::read(&p).is_some() {
            let _ = std::fs::copy(&p, p.with_extension("json.bak"));
        }
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }

    /// The opponent on rung `k` is beaten.
    pub fn is_beaten(&self, k: u8) -> bool {
        ladder::RUNGS.get(k as usize).is_some_and(|code| self.won.iter().any(|w| w == code))
    }

    /// Every rung beaten.
    pub fn complete(&self) -> bool {
        (0..ladder::RUNGS.len() as u8).all(|k| self.is_beaten(k))
    }

    /// The rung played next: the first not beaten yet (the last one again
    /// once the ladder is done).
    pub fn next(&self) -> u8 {
        (0..ladder::RUNGS.len() as u8)
            .find(|k| !self.is_beaten(*k))
            .unwrap_or(ladder::RUNGS.len() as u8 - 1)
    }

    /// Rung `k` can be played: every rung before it is beaten (so it is
    /// beaten already, or the next one).
    pub fn unlocked(&self, k: u8) -> bool {
        (k as usize) < ladder::RUNGS.len() && (0..k).all(|j| self.is_beaten(j))
    }

    /// Counts a finished ladder game. True if it changed the progress.
    pub fn record(&mut self, run: &LadderRun, won: bool) -> bool {
        if !won || !self.unlocked(run.rung) {
            return false;
        }
        if let Some(code) = ladder::RUNGS.get(run.rung as usize)
            && !self.is_beaten(run.rung)
        {
            self.won.push(code.to_string());
            self.won.sort();
        }
        self.beaten = self.won.len() as u8;
        // The last rung: the game that earns the reward (unless it is in
        // hand already).
        if run.rung as usize == ladder::RUNGS.len() - 1 && !self.claimed {
            self.pending = Some(run.record.clone());
        }
        true
    }
}

/// The claim of the reward: sent when signed in with a game to send, and
/// the server's verdict applied.
pub struct Claimer {
    pub state: Claim,
}

impl Default for Claimer {
    fn default() -> Self {
        Claimer { state: Claim::Idle }
    }
}

impl Claimer {
    /// Moves the claim along. Returns true if the progress changed (save it).
    pub fn update(&mut self, progress: &mut Progress, online: &mut Online, now: f64) -> bool {
        let mut changed = false;
        if let Some(result) = online.ladder_claim.take() {
            match result {
                ClaimResult::Granted { .. } => {
                    self.state = Claim::Granted;
                    progress.pending = None;
                    progress.claimed = true;
                    changed = true;
                }
                ClaimResult::Failed { reason } => {
                    let failure = Failure::Server(reason);
                    self.state = Claim::Failed(failure);
                    if !failure.retryable() {
                        // This game will never pass: beat the last rung again.
                        progress.pending = None;
                        changed = true;
                    }
                }
            }
        }
        // The account has it already (another computer, or before).
        if online.has_achievement(ladder::ACHIEVEMENT) && self.state != Claim::Granted {
            self.state = Claim::Granted;
            if progress.pending.is_some() {
                progress.pending = None;
                changed = true;
            }
        }
        match self.state {
            Claim::Idle if progress.pending.is_some() && online.signed_in() && !online.lan => {
                self.send(progress, online, now);
            }
            Claim::Claiming(since) if now - since > CLAIM_TIMEOUT => {
                self.state = Claim::Failed(Failure::NoAnswer);
            }
            Claim::Claiming(_) if !online.signed_in() => self.state = Claim::Idle,
            _ => {}
        }
        changed
    }

    fn send(&mut self, progress: &Progress, online: &mut Online, now: f64) {
        if let Some(replay) = progress.pending.clone() {
            online.send(ClientMsg::LadderClaim { replay });
            self.state = Claim::Claiming(now);
        }
    }

    /// The player asks to try again.
    pub fn retry(&mut self, progress: &Progress, online: &mut Online, now: f64) {
        if online.signed_in() {
            self.send(progress, online, now);
        } else {
            self.state = Claim::Idle;
            online.connect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(rung: u8) -> LadderRun {
        LadderRun { rung, record: Replay::new(rung as u64, 0), reported: false }
    }

    #[test]
    fn rungs_unlock_one_by_one() {
        let mut p = Progress::default();
        assert!(p.unlocked(0) && !p.unlocked(1));
        assert!(!p.record(&run(1), true), "a locked rung counts for nothing");
        assert!(!p.record(&run(0), false), "a loss counts for nothing");
        assert!(p.record(&run(0), true));
        assert_eq!((p.beaten, p.next()), (1, 1));
        // Beating an earlier rung again changes nothing.
        p.record(&run(0), true);
        assert_eq!(p.beaten, 1);
        let last = ladder::RUNGS.len() as u8 - 1;
        for k in 1..last {
            assert!(p.record(&run(k), true));
        }
        assert!(p.pending.is_none());
        assert!(p.record(&run(last), true));
        assert!(p.complete());
        assert_eq!(p.next(), last);
        assert_eq!(p.pending.as_ref().map(|r| r.seed), Some(last as u64));
        // Once claimed, a new win on the last rung is not sent again.
        p.pending = None;
        p.claimed = true;
        p.record(&run(last), true);
        assert!(p.pending.is_none());
    }

    /// A progress saved when the ladder had ten opponents: those beaten stay
    /// beaten, the new ones wait their turn, and nothing claimed is lost.
    #[test]
    fn a_ladder_of_ten_carries_over() {
        let p = Progress::parse(r#"{"version":1,"beaten":10,"claimed":true}"#).unwrap();
        assert_eq!(p.beaten, 10);
        assert!(p.claimed);
        assert!(!p.complete(), "five new opponents to beat");
        assert_eq!(ladder::RUNGS[p.next() as usize], "tito");
        for code in ladder::RUNGS_OF_TEN {
            let k = ladder::RUNGS.iter().position(|c| *c == code).unwrap() as u8;
            assert!(p.is_beaten(k), "{code}");
        }
        // Four of the old rungs beaten: Leo, Nina, Lola, Bruno.
        let mut p = Progress::parse(r#"{"version":1,"beaten":4}"#).unwrap();
        assert_eq!(ladder::RUNGS[p.next() as usize], "tito");
        assert!(p.record(&run(2), true));
        // Lola and Bruno were beaten already: next is Inês.
        assert_eq!(ladder::RUNGS[p.next() as usize], "ines");
        assert!(p.unlocked(5) && !p.unlocked(6));
        // A saved progress is read back as it was.
        let again = Progress::parse(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!((again.won.clone(), again.beaten), (p.won.clone(), p.beaten));
    }

    #[test]
    fn damaged_files_are_survived() {
        assert!(Progress::parse("{not json").is_none());
        assert_eq!(Progress::parse("{}"), Some(Progress::default()));
        let p = Progress::parse(r#"{"version":1,"beaten":250,"claimed":false,"extra":3}"#).unwrap();
        assert_eq!(p.beaten, 10, "the ten of the old ladder");
        let p = Progress::parse(r#"{"version":2,"won":["leo","nobody","leo"]}"#).unwrap();
        assert_eq!((p.won, p.beaten), (vec!["leo".to_string()], 1));
        let mut long = Replay::new(1, 0);
        long.strokes = vec![snookered_proto::replay::Stroke::TimeFoul; ladder::MAX_STROKES + 1];
        let text = serde_json::to_string(&Progress {
            version: 1,
            beaten: 10,
            won: Vec::new(),
            pending: Some(long),
            claimed: false,
        })
        .unwrap();
        assert!(Progress::parse(&text).unwrap().pending.is_none());
    }

    #[test]
    fn failures_say_whether_to_retry() {
        assert!(Failure::NoAnswer.retryable());
        assert!(Failure::Server(ClaimFailure::Busy).retryable());
        assert!(!Failure::Server(ClaimFailure::ComputerMismatch).retryable());
        assert!(!Failure::Server(ClaimFailure::NotWon).retryable());
    }
}
