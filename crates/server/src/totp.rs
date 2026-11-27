//! Two-step verification for the staff: time-based one-time passwords
//! (TOTP, RFC 6238: HMAC-SHA1, six digits, 30-second steps), as every
//! authenticator app makes them, and single-use recovery codes for a lost
//! phone.
//!
//! The secret is 20 random bytes, shown once as base32 (and as an
//! `otpauth://` address the portal draws as a QR code). A code is accepted
//! for its step and the one on either side (a phone's clock a little off),
//! and each step only once: the account keeps the last one used.

use hmac::{Hmac, Mac};
use rand::{Rng, RngCore};
use sha1::Sha1;

/// Seconds per code.
pub const STEP: i64 = 30;
/// Digits per code.
pub const DIGITS: u32 = 6;
/// Steps accepted on either side of the current one.
pub const SKEW: i64 = 1;
/// Recovery codes made at a time.
pub const RECOVERY_CODES: usize = 10;
/// Who the codes are from, in the authenticator app.
pub const ISSUER: &str = "Snook Legend";

/// A new secret, in base32 (32 letters and digits, no padding).
pub fn new_secret() -> String {
    let mut bytes = [0u8; 20];
    rand::thread_rng().fill_bytes(&mut bytes);
    data_encoding::BASE32_NOPAD.encode(&bytes)
}

/// The secret's bytes (spaces and case don't matter, as typed in an app).
fn secret_bytes(secret: &str) -> Option<Vec<u8>> {
    let clean: String =
        secret.chars().filter(|c| !c.is_whitespace() && *c != '=').collect::<String>();
    data_encoding::BASE32_NOPAD.decode(clean.to_ascii_uppercase().as_bytes()).ok()
}

/// The step a Unix time falls in.
pub fn step_of(unix: i64) -> i64 {
    unix.div_euclid(STEP)
}

/// The code of `step` (HOTP of the step's counter), as a number.
fn hotp(key: &[u8], counter: u64) -> u32 {
    let mut mac = <Hmac<Sha1> as Mac>::new_from_slice(key).expect("any key length");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = (digest[19] & 0x0f) as usize;
    let bin = u32::from_be_bytes([
        digest[offset] & 0x7f,
        digest[offset + 1],
        digest[offset + 2],
        digest[offset + 3],
    ]);
    bin % 10u32.pow(DIGITS)
}

/// The code an app shows at `unix` for `secret` (tests, and the tools that
/// sign in to the staff API).
pub fn code_at(secret: &str, unix: i64) -> Option<String> {
    let key = secret_bytes(secret)?;
    Some(format!("{:06}", hotp(&key, step_of(unix) as u64)))
}

/// The step `code` belongs to, if it is the code of `secret` at `unix` or
/// one step either side; `None` for a wrong code. The caller refuses a
/// step that was already used.
pub fn verify(secret: &str, code: &str, unix: i64) -> Option<i64> {
    let digits: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    if digits.len() != DIGITS as usize || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let wanted: u32 = digits.parse().ok()?;
    let key = secret_bytes(secret)?;
    let now = step_of(unix);
    // Every candidate is computed, so the time taken says nothing.
    let mut found = None;
    for step in (now - SKEW)..=(now + SKEW) {
        if step >= 0 && hotp(&key, step as u64) == wanted && found.is_none() {
            found = Some(step);
        }
    }
    found
}

/// The address an authenticator app reads (from the QR code) for
/// `account` on this server.
pub fn uri(secret: &str, account: &str) -> String {
    let issuer = encode(ISSUER);
    format!(
        "otpauth://totp/{issuer}:{}?secret={secret}&issuer={issuer}&algorithm=SHA1&digits={DIGITS}&period={STEP}",
        encode(account)
    )
}

/// Percent-encoding for the parts of the address.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Letters and digits that can't be mistaken for one another.
const RECOVERY_ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";

/// New recovery codes, written as `xxxx-xxxx` (about 39 bits each; each
/// one works once, and wrong ones count against the account's limits).
pub fn new_recovery_codes() -> Vec<String> {
    let mut rng = rand::thread_rng();
    (0..RECOVERY_CODES)
        .map(|_| {
            let mut s = String::with_capacity(9);
            for i in 0..8 {
                if i == 4 {
                    s.push('-');
                }
                s.push(RECOVERY_ALPHABET[rng.gen_range(0..RECOVERY_ALPHABET.len())] as char);
            }
            s
        })
        .collect()
}

/// A recovery code as it is kept: lower case, without the dash or
/// spaces; `None` when it can't be one.
pub fn normalize_recovery(code: &str) -> Option<String> {
    let s: String = code
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_lowercase())
        .collect();
    (s.len() == 8 && s.bytes().all(|b| RECOVERY_ALPHABET.contains(&b))).then_some(s)
}

/// Whether `code` looks like a code from the app (six digits) rather than
/// a recovery code.
pub fn looks_like_totp(code: &str) -> bool {
    let digits: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    digits.len() == DIGITS as usize && digits.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238's test vectors for SHA-1 (its key is "12345678901234567890";
    /// the RFC shows eight digits, the last six are what six-digit apps show).
    #[test]
    fn rfc_6238_vectors() {
        let secret = data_encoding::BASE32_NOPAD.encode(b"12345678901234567890");
        for (t, eight) in [
            (59, "94287082"),
            (1_111_111_109, "07081804"),
            (1_111_111_111, "14050471"),
            (1_234_567_890, "89005924"),
            (2_000_000_000, "69279037"),
            (20_000_000_000, "65353130"),
        ] {
            assert_eq!(code_at(&secret, t).unwrap(), eight[2..], "t = {t}");
        }
    }

    #[test]
    fn a_code_works_one_step_either_side() {
        let secret = new_secret();
        assert_eq!(secret.len(), 32);
        let t = 1_800_000_000;
        let code = code_at(&secret, t).unwrap();
        assert_eq!(verify(&secret, &code, t), Some(step_of(t)));
        assert_eq!(verify(&secret, &code, t + STEP), Some(step_of(t)));
        assert_eq!(verify(&secret, &code, t - STEP), Some(step_of(t)));
        assert_eq!(verify(&secret, &code, t + 3 * STEP), None);
        // Spaces as some apps show them ("123 456"), and the secret typed
        // in lower case with spaces.
        let spaced = format!("{} {}", &code[..3], &code[3..]);
        assert_eq!(verify(&secret, &spaced, t), Some(step_of(t)));
        let typed = secret
            .to_lowercase()
            .chars()
            .collect::<Vec<_>>()
            .chunks(4)
            .map(|c| c.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(verify(&typed, &code, t), Some(step_of(t)));
        assert_eq!(verify(&secret, "12345", t), None);
        assert_eq!(verify(&secret, "abcdef", t), None);
        assert_eq!(verify("not base32!", &code, t), None);
    }

    #[test]
    fn recovery_codes() {
        let codes = new_recovery_codes();
        assert_eq!(codes.len(), RECOVERY_CODES);
        for c in &codes {
            assert_eq!(c.len(), 9);
            assert_eq!(&c[4..5], "-");
            let n = normalize_recovery(c).unwrap();
            assert_eq!(normalize_recovery(&c.to_uppercase()), Some(n.clone()));
            assert_eq!(normalize_recovery(&n), Some(n));
            assert!(!looks_like_totp(c));
        }
        assert_eq!(normalize_recovery("abcd-efg"), None);
        assert_eq!(normalize_recovery("abcd-efg0"), None, "0 is not in the alphabet");
        assert!(looks_like_totp("123 456"));
    }

    #[test]
    fn the_address_for_the_app() {
        let u = uri("JBSWY3DPEHPK3PXP", "Ana Maria");
        assert_eq!(
            u,
            "otpauth://totp/Snook%20Legend:Ana%20Maria?secret=JBSWY3DPEHPK3PXP&issuer=Snook%20Legend&algorithm=SHA1&digits=6&period=30"
        );
    }
}
