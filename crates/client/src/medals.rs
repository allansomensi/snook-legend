//! The player's medals of the tournaments against the computer (see
//! `snook_legend_proto::cup`). A medal is kept here at once, for the account
//! of the game (the one signed in, or the one whose session is saved; none:
//! the next account to sign in takes it), and sent to the server once that
//! account is online; the server's count comes back and is kept here too,
//! so the medals show offline. They are shown with the tournaments only.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use snook_legend_proto::cup::{Medal, Medals};

/// The tournaments whose medal was given, remembered (so one is never given
/// twice); the oldest are forgotten past this.
const REMEMBERED: usize = 300;

/// A medal the server hasn't kept yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Pending {
    /// The account it belongs to (lower case; empty: the next to sign in).
    account: String,
    medal: Medal,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MedalBook {
    /// What the server last said each account has.
    #[serde(default)]
    accounts: BTreeMap<String, Medals>,
    #[serde(default)]
    pending: Vec<Pending>,
    /// The tournaments whose medal was given (newest last).
    #[serde(default)]
    given: Vec<u64>,
    /// The connection the pending medals were last sent on, and the account
    /// asked for its count on it (neither kept on disk).
    #[serde(skip)]
    sent_on: Option<(u32, String)>,
}

/// The account a medal belongs to: the one signed in, else the one whose
/// session is saved (it signs in by itself next time); none: the next
/// account to sign in takes it.
pub fn account_of(
    online: &crate::online::Online,
    config: &crate::config::Config,
) -> Option<String> {
    online.account().map(str::to_string).or_else(|| {
        (!config.session_token().is_empty() && !config.username.trim().is_empty())
            .then(|| config.username.clone())
    })
}

fn path() -> Option<std::path::PathBuf> {
    crate::config::config_dir().map(|d| d.join("cup_medals.json"))
}

fn key(account: &str) -> String {
    account.trim().to_lowercase()
}

impl MedalBook {
    pub fn load() -> MedalBook {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(self) {
            let _ = crate::util::write_atomically(&p, text.as_bytes());
        }
    }

    /// Gives the medal of a tournament (once): kept for `account` (None:
    /// the next account to sign in). True if it is new.
    pub fn give(&mut self, medal: Medal, account: Option<&str>) -> bool {
        if !medal.is_sound() || self.given.contains(&medal.cup) {
            return false;
        }
        self.given.push(medal.cup);
        if self.given.len() > REMEMBERED {
            self.given.remove(0);
        }
        self.pending.push(Pending { account: account.map(key).unwrap_or_default(), medal });
        // Sent again on this connection.
        self.sent_on = None;
        self.save();
        true
    }

    /// The medals to show for `account` (None: signed out, no session): what
    /// the server said it has, and those not kept yet that are, or will be,
    /// its own.
    pub fn shown(&self, account: Option<&str>) -> Medals {
        let account = account.map(key);
        let mut medals =
            account.as_ref().and_then(|a| self.accounts.get(a)).copied().unwrap_or_default();
        for p in &self.pending {
            if p.account.is_empty() || Some(&p.account) == account.as_ref() {
                medals.add(&p.medal);
            }
        }
        medals
    }

    /// Medals not kept by the server yet for `account` (or for the next to
    /// sign in).
    pub fn unsynced(&self, account: Option<&str>) -> usize {
        let account = account.map(key).unwrap_or_default();
        self.pending.iter().filter(|p| p.account.is_empty() || p.account == account).count()
    }

    /// With `account` signed in on connection `session`: the medals won
    /// signed out become its own, and every one not kept yet is sent, with
    /// a question for its count (once per connection).
    pub fn sync(
        &mut self,
        account: &str,
        session: u32,
        send: &mut impl FnMut(snook_legend_proto::ClientMsg),
    ) {
        let account = key(account);
        if self.sent_on.as_ref() == Some(&(session, account.clone())) {
            return;
        }
        let mut changed = false;
        for p in &mut self.pending {
            if p.account.is_empty() {
                p.account = account.clone();
                changed = true;
            }
        }
        if changed {
            self.save();
        }
        for p in self.pending.iter().filter(|p| p.account == account) {
            send(snook_legend_proto::ClientMsg::CupMedal { medal: p.medal });
        }
        send(snook_legend_proto::ClientMsg::CupMedals);
        self.sent_on = Some((session, account));
    }

    /// The server's count for `account`, and the tournament it just kept.
    pub fn heard(&mut self, account: &str, medals: Medals, kept: Option<u64>) {
        let account = key(account);
        self.accounts.insert(account.clone(), medals);
        if let Some(cup) = kept {
            self.pending.retain(|p| !(p.account == account && p.medal.cup == cup));
        }
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snook_legend_proto::cup::Format;

    fn medal(cup: u64, place: u8) -> Medal {
        Medal { cup, format: Format::Arena, place }
    }

    /// A medal won signed out goes to the next account to sign in; it shows
    /// at once, is sent once per connection, and is forgotten once kept,
    /// the server's count standing in for it.
    #[test]
    fn medals_wait_for_the_account_then_its_count_stands() {
        let mut book = MedalBook::default();
        assert!(book.give(medal(1, 1), None));
        assert!(!book.give(medal(1, 1), None), "a tournament gives one medal");
        assert!(book.give(medal(2, 3), Some("Ana")));
        assert_eq!(book.shown(None).arena, [1, 0, 0]);
        assert_eq!(book.shown(Some("ana")).arena, [1, 0, 1]);
        let mut sent = Vec::new();
        book.sync("ANA", 1, &mut |m| sent.push(m));
        assert_eq!(sent.len(), 3, "two medals and the question: {sent:?}");
        book.sync("ana", 1, &mut |m| sent.push(m));
        assert_eq!(sent.len(), 3, "once per connection");
        // The server kept the first: its count stands in for it.
        book.heard("ana", Medals { knockout: [0; 3], arena: [1, 0, 0] }, Some(1));
        assert_eq!(book.shown(Some("ana")).arena, [1, 0, 1]);
        assert_eq!(book.unsynced(Some("ana")), 1);
        book.heard("ana", Medals { knockout: [0; 3], arena: [1, 0, 1] }, Some(2));
        assert_eq!(book.shown(Some("ana")).arena, [1, 0, 1]);
        assert_eq!(book.unsynced(Some("ana")), 0);
        // Another account sees none of them.
        assert_eq!(book.shown(Some("bia")), Medals::default());
    }
}
