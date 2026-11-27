//! Rate limits against floods and password guessing.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A token bucket: `burst` actions at once, refilled at `rate` per second.
#[derive(Debug, Clone)]
pub struct Bucket {
    tokens: f64,
    burst: f64,
    rate: f64,
    last: Instant,
}

impl Bucket {
    pub fn new(burst: f64, rate: f64) -> Bucket {
        Bucket { tokens: burst, burst, rate, last: Instant::now() }
    }

    /// Takes one token; false if the bucket is empty.
    pub fn take(&mut self) -> bool {
        let now = Instant::now();
        self.tokens =
            (self.tokens + now.duration_since(self.last).as_secs_f64() * self.rate).min(self.burst);
        self.last = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Whether [`Bucket::take`] would succeed now (nothing is taken).
    pub fn has_token(&self) -> bool {
        (self.tokens + self.last.elapsed().as_secs_f64() * self.rate).min(self.burst) >= 1.0
    }

    /// Whether the bucket has filled up again (it is then the same as a
    /// new one: forgetting it loses nothing).
    pub fn is_full(&self) -> bool {
        self.tokens + self.last.elapsed().as_secs_f64() * self.rate >= self.burst
    }
}

/// A [`Bucket`] per key (an address), each `burst` at once and `rate` a
/// second. Buckets that filled up again are forgotten when room is needed,
/// so the map stays within [`MAX_KEYS`] whoever asks.
#[derive(Debug)]
pub struct Buckets<K> {
    buckets: HashMap<K, Bucket>,
    burst: f64,
    rate: f64,
}

impl<K: std::hash::Hash + Eq> Buckets<K> {
    pub fn new(burst: f64, rate: f64) -> Self {
        Buckets { buckets: HashMap::new(), burst, rate }
    }

    /// Takes one token of `key`'s bucket; false if it is empty.
    pub fn take(&mut self, key: K) -> bool {
        if !self.buckets.contains_key(&key) && self.buckets.len() >= MAX_KEYS {
            self.buckets.retain(|_, b| !b.is_full());
            if self.buckets.len() >= MAX_KEYS {
                // Every one of them busy: start over rather than grow (or
                // turn away addresses that did nothing).
                self.buckets.clear();
            }
        }
        let (burst, rate) = (self.burst, self.rate);
        self.buckets.entry(key).or_insert_with(|| Bucket::new(burst, rate)).take()
    }

    /// Keys kept now.
    pub fn len(&self) -> usize {
        self.buckets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }
}

/// Counts events per key inside a sliding window.
///
/// At most [`MAX_KEYS`] keys are kept: a new key that finds it full takes the
/// place of the key whose last event is the oldest. Keys come from what
/// anyone can send (addresses, names), so the map must not grow with them;
/// and making room this way locks nobody out.
#[derive(Debug, Default)]
pub struct Window {
    events: HashMap<String, Vec<Instant>>,
}

/// Most keys a [`Window`] keeps.
pub const MAX_KEYS: usize = 50_000;

impl Window {
    fn prune(list: &mut Vec<Instant>, window: Duration) {
        let now = Instant::now();
        list.retain(|t| now.duration_since(*t) < window);
    }

    pub fn count(&mut self, key: &str, window: Duration) -> usize {
        match self.events.get_mut(key) {
            Some(list) => {
                Self::prune(list, window);
                list.len()
            }
            None => 0,
        }
    }

    pub fn add(&mut self, key: &str) {
        if !self.events.contains_key(key) && self.events.len() >= MAX_KEYS {
            let oldest = self
                .events
                .iter()
                .min_by_key(|(_, list)| list.last().copied())
                .map(|(k, _)| k.clone());
            if let Some(k) = oldest {
                self.events.remove(&k);
            }
        }
        self.events.entry(key.to_string()).or_default().push(Instant::now());
    }

    /// Takes back the latest event of `key` (one counted ahead of time that
    /// turned out not to be one).
    pub fn take_back(&mut self, key: &str) {
        if let Some(list) = self.events.get_mut(key) {
            list.pop();
            if list.is_empty() {
                self.events.remove(key);
            }
        }
    }

    pub fn clear(&mut self, key: &str) {
        self.events.remove(key);
    }

    /// Drops keys with no recent events (call now and then).
    pub fn sweep(&mut self, window: Duration) {
        self.events.retain(|_, list| {
            Self::prune(list, window);
            !list.is_empty()
        });
    }

    /// Keys kept now.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// Failed sign-ins, new accounts and sign-ins, per address and per account.
///
/// Every attempt is counted when it is allowed, before the slow part (the
/// password hash, the database) runs: attempts made at the same moment
/// can't all slip under a limit that was checked before any of them was
/// recorded. What turns out not to count is taken back.
#[derive(Debug, Default)]
pub struct AuthLimiter {
    failures_ip: Window,
    /// Wrong passwords for an account from one address.
    failures_pair: Window,
    /// Wrong passwords for an account from anywhere (a far higher limit:
    /// a stranger can't lock an account out with a few wrong passwords).
    failures_user: Window,
    registrations: Window,
    sign_ins_ip: Window,
    sign_ins_user: Window,
    /// Asks for an e-mailed link (a forgotten password), per address and
    /// per name or e-mail typed.
    mail_requests_ip: Window,
    mail_requests_login: Window,
    /// E-mails sent to an account (links and confirmations): an inbox is
    /// never flooded, whoever asks.
    mails_user: Window,
    /// Links tried (a new password, a confirmation), per address.
    tokens_ip: Window,
    /// Wrong codes of the authenticator app, per account.
    codes_user: Window,
}

pub const AUTH_WINDOW: Duration = Duration::from_secs(15 * 60);
const REGISTER_WINDOW: Duration = Duration::from_secs(60 * 60);
const MAX_FAILURES_PER_IP: usize = 20;
const MAX_FAILURES_PER_USER: usize = 8;
/// Wrong passwords for an account from every address together: guessing
/// from many addresses stays slow, and locking someone out takes far more
/// than a few tries from one place.
const MAX_FAILURES_PER_ACCOUNT: usize = 60;
const MAX_REGISTRATIONS_PER_IP: usize = 5;
/// Sign-ins (by password, by a saved session or with a new account) cost
/// a dozen database requests each; a game reconnects at most every second
/// or so. These are far above what people do and stop a script that signs
/// in again and again (a new connection each time) to wear the database
/// down.
const SIGN_IN_WINDOW: Duration = Duration::from_secs(60);
// Generous per address: after a restart everyone signs in again at once,
// and many players can share one address (carrier-grade NAT, a LAN party).
const MAX_SIGN_INS_PER_IP: usize = 120;
const MAX_SIGN_INS_PER_USER: usize = 12;
const MAIL_WINDOW: Duration = Duration::from_secs(60 * 60);
const MAX_MAIL_REQUESTS_PER_IP: usize = 10;
const MAX_MAIL_REQUESTS_PER_LOGIN: usize = 5;
const MAX_MAILS_PER_ACCOUNT: usize = 5;
const MAX_TOKENS_PER_IP: usize = 30;
/// Wrong codes for an account in [`AUTH_WINDOW`]: six digits are a million
/// guesses, and a code changes every 30 seconds.
const MAX_CODE_FAILURES: usize = 10;

impl AuthLimiter {
    /// Whether another password may be tried for `username` from `ip`. If
    /// so, it is counted as a failure straight away; [`AuthLimiter::succeeded`]
    /// takes it back when the password was right. `username` must be a
    /// valid one (names are matched without regard to case, as the
    /// database does).
    pub fn try_password(&mut self, ip: &str, username: &str) -> bool {
        let user = username.to_ascii_lowercase();
        let pair = format!("{ip}|{user}");
        if self.failures_ip.count(ip, AUTH_WINDOW) >= MAX_FAILURES_PER_IP
            || self.failures_pair.count(&pair, AUTH_WINDOW) >= MAX_FAILURES_PER_USER
            || self.failures_user.count(&user, AUTH_WINDOW) >= MAX_FAILURES_PER_ACCOUNT
        {
            return false;
        }
        self.failures_ip.add(ip);
        self.failures_pair.add(&pair);
        self.failures_user.add(&user);
        true
    }

    /// The password tried was right: the account's failures are forgotten
    /// and the attempt is not one of the address's.
    pub fn succeeded(&mut self, ip: &str, username: &str) {
        self.failures_ip.take_back(ip);
        let user = username.to_ascii_lowercase();
        self.failures_pair.clear(&format!("{ip}|{user}"));
        self.failures_user.clear(&user);
    }

    /// The password could not be checked (the server was busy): the attempt
    /// counted for it is given back.
    pub fn not_tried(&mut self, ip: &str, username: &str) {
        self.failures_ip.take_back(ip);
        let user = username.to_ascii_lowercase();
        self.failures_pair.take_back(&format!("{ip}|{user}"));
        self.failures_user.take_back(&user);
    }

    /// Whether another account may be made from `key` (an address; reports
    /// use their own keys). If so it is counted straight away;
    /// [`AuthLimiter::not_registered`] takes it back when none was made.
    pub fn try_register(&mut self, key: &str) -> bool {
        if self.registrations.count(key, REGISTER_WINDOW) >= MAX_REGISTRATIONS_PER_IP {
            return false;
        }
        self.registrations.add(key);
        true
    }

    pub fn not_registered(&mut self, key: &str) {
        self.registrations.take_back(key);
    }

    /// Whether another sign-in may start from `ip` (counted straight away).
    pub fn try_sign_in(&mut self, ip: &str) -> bool {
        if self.sign_ins_ip.count(ip, SIGN_IN_WINDOW) >= MAX_SIGN_INS_PER_IP {
            return false;
        }
        self.sign_ins_ip.add(ip);
        true
    }

    /// Whether account `id` may sign in once more (counted straight away).
    pub fn try_sign_in_as(&mut self, id: i64) -> bool {
        let key = id.to_string();
        if self.sign_ins_user.count(&key, SIGN_IN_WINDOW) >= MAX_SIGN_INS_PER_USER {
            return false;
        }
        self.sign_ins_user.add(&key);
        true
    }

    /// Whether `ip` may ask for another e-mailed link for `login` (a name
    /// or an address, lower case). Counted straight away.
    pub fn try_mail_request(&mut self, ip: &str, login: &str) -> bool {
        if self.mail_requests_ip.count(ip, MAIL_WINDOW) >= MAX_MAIL_REQUESTS_PER_IP
            || self.mail_requests_login.count(login, MAIL_WINDOW) >= MAX_MAIL_REQUESTS_PER_LOGIN
        {
            return false;
        }
        self.mail_requests_ip.add(ip);
        self.mail_requests_login.add(login);
        true
    }

    /// Whether another e-mail may go to account `id` (counted straight away).
    pub fn try_mail_to(&mut self, id: i64) -> bool {
        let key = id.to_string();
        if self.mails_user.count(&key, MAIL_WINDOW) >= MAX_MAILS_PER_ACCOUNT {
            return false;
        }
        self.mails_user.add(&key);
        true
    }

    /// Whether `ip` may try another e-mailed link (counted straight away).
    pub fn try_token(&mut self, ip: &str) -> bool {
        if self.tokens_ip.count(ip, AUTH_WINDOW) >= MAX_TOKENS_PER_IP {
            return false;
        }
        self.tokens_ip.add(ip);
        true
    }

    /// Whether another code may be tried for account `id`. Counted as a
    /// failure straight away; [`AuthLimiter::code_ok`] forgets them.
    pub fn try_code(&mut self, id: i64) -> bool {
        let key = id.to_string();
        if self.codes_user.count(&key, AUTH_WINDOW) >= MAX_CODE_FAILURES {
            return false;
        }
        self.codes_user.add(&key);
        true
    }

    /// The code was right: the account's wrong ones are forgotten.
    pub fn code_ok(&mut self, id: i64) {
        self.codes_user.clear(&id.to_string());
    }

    /// The account's owner chose a new password by e-mail: the wrong
    /// passwords tried for it are forgotten (from every address).
    pub fn forget_failures(&mut self, username: &str) {
        self.failures_user.clear(&username.to_ascii_lowercase());
    }

    pub fn sweep(&mut self) {
        self.mail_requests_ip.sweep(MAIL_WINDOW);
        self.mail_requests_login.sweep(MAIL_WINDOW);
        self.mails_user.sweep(MAIL_WINDOW);
        self.tokens_ip.sweep(AUTH_WINDOW);
        self.codes_user.sweep(AUTH_WINDOW);
        self.failures_ip.sweep(AUTH_WINDOW);
        self.failures_pair.sweep(AUTH_WINDOW);
        self.failures_user.sweep(AUTH_WINDOW);
        self.registrations.sweep(REGISTER_WINDOW);
        self.sign_ins_ip.sweep(SIGN_IN_WINDOW);
        self.sign_ins_user.sweep(SIGN_IN_WINDOW);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_per_key_are_bounded() {
        let mut b = Buckets::new(2.0, 0.0);
        assert!(b.take(1) && b.take(1) && !b.take(1), "per key");
        assert!(b.take(2), "another key has its own");
        // Buckets filled up again are forgotten to make room.
        let mut b = Buckets::new(1.0, 1000.0);
        for k in 0..MAX_KEYS {
            b.buckets.insert(k, Bucket::new(1.0, 1000.0));
        }
        assert!(b.take(MAX_KEYS));
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn bucket_limits_bursts() {
        let mut b = Bucket::new(3.0, 0.0);
        assert!(b.has_token() && b.has_token(), "looking takes nothing");
        assert!(b.take() && b.take() && b.take());
        assert!(!b.has_token());
        assert!(!b.take());
    }

    #[test]
    fn password_guessing_is_limited() {
        let mut l = AuthLimiter::default();
        for _ in 0..MAX_FAILURES_PER_USER {
            assert!(l.try_password("1.2.3.4", "Bob"));
        }
        assert!(!l.try_password("1.2.3.4", "bob"), "per account from one address");
        // Someone else's wrong passwords don't lock the owner out…
        assert!(l.try_password("5.6.7.8", "bob"), "the owner elsewhere still may");
        assert!(l.try_password("1.2.3.4", "alice"));
        // A right password takes its own attempt back and clears the account.
        l.succeeded("1.2.3.4", "alice");
        l.succeeded("1.2.3.4", "BOB");
        assert!(l.try_password("1.2.3.4", "bob"));
        // …but guessing from many addresses stops at the account's limit.
        let mut l = AuthLimiter::default();
        let mut tried = 0;
        for k in 0..200 {
            if l.try_password(&format!("10.0.{}.{}", k / 250, k % 250), "carol") {
                tried += 1;
            }
        }
        assert_eq!(tried, MAX_FAILURES_PER_ACCOUNT);
    }

    #[test]
    fn attempts_are_counted_before_they_are_made() {
        let mut l = AuthLimiter::default();
        // Many at once from one address: only the allowed number get through.
        let allowed =
            (0..3 * MAX_REGISTRATIONS_PER_IP).filter(|_| l.try_register("1.2.3.4")).count();
        assert_eq!(allowed, MAX_REGISTRATIONS_PER_IP);
        // One that made no account is given back.
        l.not_registered("1.2.3.4");
        assert!(l.try_register("1.2.3.4"));
        assert!(!l.try_register("1.2.3.4"));
        let signed = (0..2 * MAX_SIGN_INS_PER_IP).filter(|_| l.try_sign_in("1.2.3.4")).count();
        assert_eq!(signed, MAX_SIGN_INS_PER_IP);
        assert!(l.try_sign_in("1.2.3.5"));
        let as_one = (0..2 * MAX_SIGN_INS_PER_USER).filter(|_| l.try_sign_in_as(7)).count();
        assert_eq!(as_one, MAX_SIGN_INS_PER_USER);
    }

    /// Keys come from what anyone sends: the map stays bounded, and the
    /// newest keys stay counted.
    #[test]
    fn windows_keep_a_bounded_number_of_keys() {
        let mut w = Window::default();
        for i in 0..MAX_KEYS + 100 {
            w.add(&format!("k{i}"));
        }
        assert_eq!(w.len(), MAX_KEYS);
        let last = format!("k{}", MAX_KEYS + 99);
        assert_eq!(w.count(&last, AUTH_WINDOW), 1);
        w.take_back(&last);
        assert_eq!(w.count(&last, AUTH_WINDOW), 0);
    }
}
