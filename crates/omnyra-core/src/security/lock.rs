//! App lock with a PIN (6c). The PIN is stored as a salted PBKDF2-HMAC-SHA256 hash in the
//! OS keyring (`omnyra/lock/pin`); a wrong PIN slows down further tries. It protects the
//! interface only (decision D7): the database itself is not encrypted.

use crate::{AppError, Result};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
use sha2::{Digest, Sha256};

pub const SECRET_KEY: &str = "lock/pin";
pub const ITERATIONS: u32 = 210_000;
/// Failed attempts allowed before each further try waits.
pub const FREE_ATTEMPTS: u32 = 5;

fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    const B: usize = 64;
    let mut k = [0u8; B];
    if key.len() > B {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; B];
    let mut opad = [0x5cu8; B];
    for i in 0..B {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let inner = Sha256::new()
        .chain_update(ipad)
        .chain_update(msg)
        .finalize();
    Sha256::new()
        .chain_update(opad)
        .chain_update(inner)
        .finalize()
        .into()
}

/// PBKDF2-HMAC-SHA256 with a single 32-byte block (RFC 8018).
pub fn pbkdf2(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut block = salt.to_vec();
    block.extend_from_slice(&1u32.to_be_bytes());
    let mut u = hmac_sha256(password, &block);
    let mut out = u;
    for _ in 1..iterations {
        u = hmac_sha256(password, &u);
        for (o, x) in out.iter_mut().zip(u.iter()) {
            *o ^= x;
        }
    }
    out
}

/// A PIN must be 4 to 32 characters (digits or a passphrase).
pub fn check_pin(pin: &str) -> Result<()> {
    let n = pin.chars().count();
    if !(4..=32).contains(&n) || pin.trim() != pin {
        return Err(AppError::Validation {
            field: "pin".into(),
        });
    }
    Ok(())
}

/// `pbkdf2-sha256$<iterations>$<salt>$<hash>`.
pub fn hash_pin(pin: &str, iterations: u32) -> Result<String> {
    check_pin(pin)?;
    let mut salt = [0u8; 16];
    rand::fill(&mut salt);
    let h = pbkdf2(pin.as_bytes(), &salt, iterations);
    Ok(format!(
        "pbkdf2-sha256${iterations}${}${}",
        STANDARD_NO_PAD.encode(salt),
        STANDARD_NO_PAD.encode(h)
    ))
}

pub fn verify_pin(pin: &str, stored: &str) -> bool {
    let parts: Vec<&str> = stored.split('$').collect();
    let [scheme, iter, salt, hash] = parts[..] else {
        return false;
    };
    let (Ok(iter), Ok(salt), Ok(hash)) = (
        iter.parse::<u32>(),
        STANDARD_NO_PAD.decode(salt),
        STANDARD_NO_PAD.decode(hash),
    ) else {
        return false;
    };
    if scheme != "pbkdf2-sha256" || iter == 0 || iter > 10_000_000 {
        return false;
    }
    let h = pbkdf2(pin.as_bytes(), &salt, iter);
    crate::auth::pkce::ct_eq(&STANDARD_NO_PAD.encode(h), &STANDARD_NO_PAD.encode(&hash))
}

/// Seconds to wait before the next try after `failures` wrong PINs.
pub fn backoff_s(failures: u32) -> u64 {
    if failures < FREE_ATTEMPTS {
        0
    } else {
        (30u64 << (failures - FREE_ATTEMPTS).min(5)).min(900)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pbkdf2_matches_rfc7914_vector() {
        // RFC 7914 §11: PBKDF2-HMAC-SHA256("passwd", "salt", 1), first 32 bytes.
        let out = pbkdf2(b"passwd", b"salt", 1);
        let hex: String = out.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc"
        );
    }

    #[test]
    fn hash_and_verify() {
        let h = hash_pin("1234", 1000).unwrap();
        assert!(h.starts_with("pbkdf2-sha256$1000$"));
        assert!(verify_pin("1234", &h));
        assert!(!verify_pin("1235", &h));
        assert!(!verify_pin("1234", "garbage"));
        assert!(!verify_pin("1234", "md5$1$a$b"));
        assert_ne!(hash_pin("1234", 1000).unwrap(), h, "random salt");
        assert!(hash_pin("12", 1000).is_err());
        assert!(hash_pin(" 1234", 1000).is_err());
    }

    #[test]
    fn backoff_grows_after_free_attempts() {
        assert_eq!(backoff_s(0), 0);
        assert_eq!(backoff_s(4), 0);
        assert_eq!(backoff_s(5), 30);
        assert_eq!(backoff_s(6), 60);
        assert_eq!(backoff_s(50), 900);
    }
}
