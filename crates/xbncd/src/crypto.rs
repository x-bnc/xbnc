//! Password hashing and encryption of stored credentials.
//!
//! * Bouncer user passwords are hashed with Argon2id (they never need to be
//!   recovered).
//! * Credentials xBNC must send on the user's behalf (SASL passwords, server
//!   passwords, channel keys) cannot be hashed, so they are encrypted with
//!   XChaCha20-Poly1305 using a key stored outside the database.

use anyhow::{anyhow, bail, Context, Result};
use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use std::fmt;
use std::io::Write;
use std::path::Path;

const KEY_LEN: usize = 32;
const ENC_PREFIX: &str = "v1:";

/// A string that must never appear in logs or debug output.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Secret(String);

impl Secret {
    pub fn new(s: impl Into<String>) -> Self {
        Secret(s.into())
    }
    /// Returns the secret value. Call sites must not log the result.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// Argon2id with the OWASP-recommended baseline (m=19 MiB, t=2, p=1).
fn argon2() -> Argon2<'static> {
    let params = Params::new(19 * 1024, 2, 1, None).expect("valid argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Argon2id runs at most this many at once (one per CPU, 2 to 16). Each run
/// takes 19 MiB and most of a CPU: without a limit, hundreds of clients
/// logging in together after a restart (or a flood of login attempts) ran
/// hundreds at once (found by the load test). Extra logins wait their turn.
fn with_hash_slot<T>(f: impl FnOnce() -> T) -> T {
    use std::sync::{Condvar, LazyLock, Mutex};
    static GATE: LazyLock<(Mutex<usize>, Condvar, usize)> = LazyLock::new(|| {
        let n = std::thread::available_parallelism()
            .map_or(2, |n| n.get())
            .clamp(2, 16);
        (Mutex::new(0), Condvar::new(), n)
    });
    let (lock, cv, max) = &*GATE;
    {
        let mut busy = lock.lock().unwrap_or_else(|e| e.into_inner());
        while *busy >= *max {
            busy = cv.wait(busy).unwrap_or_else(|e| e.into_inner());
        }
        *busy += 1;
    }
    struct Release<'a>(&'a Mutex<usize>, &'a Condvar);
    impl Drop for Release<'_> {
        fn drop(&mut self) {
            *self.0.lock().unwrap_or_else(|e| e.into_inner()) -= 1;
            self.1.notify_one();
        }
    }
    let _r = Release(lock, cv);
    f()
}

/// Hashes a password into a PHC string.
pub fn hash_password(password: &str) -> Result<String> {
    with_hash_slot(|| hash_password_now(password))
}

fn hash_password_now(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    argon2()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow!("password hashing failed: {e}"))
}

/// Prefix of password hashes imported from another bouncer: salted
/// SHA-256 (`sha256(password + salt)`). They are replaced by Argon2id the
/// first time the user logs in (see [`needs_rehash`]).
const LEGACY_SHA256: &str = "legacy-sha256$";

/// Stores an imported salted SHA-256 hash (hex) in xBNC's format.
pub fn legacy_sha256_hash(hex_hash: &str, salt: &str) -> Option<String> {
    let ok = hex_hash.len() == 64 && hex_hash.bytes().all(|c| c.is_ascii_hexdigit());
    let salt_ok = salt.len() <= 256 && !salt.contains(['$', '\n', '\r']);
    (ok && salt_ok).then(|| format!("{LEGACY_SHA256}{}${salt}", hex_hash.to_ascii_lowercase()))
}

/// True for hashes that should be upgraded to Argon2id after a successful
/// login.
pub fn needs_rehash(hash: &str) -> bool {
    hash.starts_with(LEGACY_SHA256)
}

fn verify_legacy_sha256(rest: &str, password: &str) -> bool {
    use sha2::{Digest, Sha256};
    let Some((hex, salt)) = rest.split_once('$') else {
        return false;
    };
    let mut h = Sha256::new();
    h.update(password.as_bytes());
    h.update(salt.as_bytes());
    let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    // Constant-time comparison.
    got.len() == hex.len()
        && got
            .bytes()
            .zip(hex.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

/// Verifies a password against a PHC string. Malformed hashes never match.
pub fn verify_password(hash: &str, password: &str) -> bool {
    if let Some(rest) = hash.strip_prefix(LEGACY_SHA256) {
        return verify_legacy_sha256(rest, password);
    }
    match PasswordHash::new(hash) {
        Ok(parsed) => with_hash_slot(|| {
            argon2()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        }),
        Err(_) => false,
    }
}

/// A precomputed hash used to keep login timing similar for unknown users.
pub fn dummy_hash() -> &'static str {
    use std::sync::OnceLock;
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| hash_password("xbnc-dummy-password").expect("dummy hash"))
}

/// Minimum password policy for bouncer accounts.
pub fn check_password_policy(password: &str) -> Result<()> {
    if password.chars().count() < 10 {
        bail!("password must have at least 10 characters");
    }
    if password.len() > 1024 {
        bail!("password is too long");
    }
    if password.contains(['\r', '\n', '\0']) {
        bail!("password contains forbidden control characters");
    }
    Ok(())
}

/// Symmetric encryption for stored credentials.
#[derive(Clone)]
pub struct SecretBox {
    cipher: XChaCha20Poly1305,
}

impl fmt::Debug for SecretBox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretBox(***)")
    }
}

impl SecretBox {
    pub fn from_key(key: &[u8; KEY_LEN]) -> SecretBox {
        SecretBox {
            cipher: XChaCha20Poly1305::new(key.into()),
        }
    }

    /// Loads the key file. With `create`, a new random key is written (mode
    /// 0600) if the file does not exist.
    pub fn load(path: &Path, create: bool) -> Result<SecretBox> {
        if !path.exists() {
            if !create {
                bail!(
                    "secret key file {} not found; run `xbncd init` first",
                    path.display()
                );
            }
            let key = XChaCha20Poly1305::generate_key(&mut chacha20poly1305::aead::OsRng);
            write_private_file(path, B64.encode(key).as_bytes())?;
        }
        check_key_permissions(path)?;
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read secret key {}", path.display()))?;
        let raw = B64
            .decode(text.trim())
            .context("secret key file is not valid base64")?;
        let key: [u8; KEY_LEN] = raw
            .try_into()
            .map_err(|_| anyhow!("secret key must be {KEY_LEN} bytes"))?;
        Ok(SecretBox::from_key(&key))
    }

    /// Encrypts a value. The output is `v1:` + base64(nonce || ciphertext).
    pub fn encrypt(&self, plain: &str) -> String {
        let nonce = XChaCha20Poly1305::generate_nonce(&mut chacha20poly1305::aead::OsRng);
        let ct = self
            .cipher
            .encrypt(&nonce, plain.as_bytes())
            .expect("XChaCha20-Poly1305 encryption cannot fail for in-memory data");
        let mut out = nonce.to_vec();
        out.extend_from_slice(&ct);
        format!("{ENC_PREFIX}{}", B64.encode(out))
    }

    /// Decrypts a value produced by [`SecretBox::encrypt`].
    pub fn decrypt(&self, value: &str) -> Result<Secret> {
        let body = value
            .strip_prefix(ENC_PREFIX)
            .ok_or_else(|| anyhow!("unknown encrypted value format"))?;
        let raw = B64.decode(body).context("encrypted value is not base64")?;
        if raw.len() < 24 {
            bail!("encrypted value too short");
        }
        let (nonce, ct) = raw.split_at(24);
        let plain = self
            .cipher
            .decrypt(XNonce::from_slice(nonce), ct)
            .map_err(|_| anyhow!("cannot decrypt stored credential (wrong secret key?)"))?;
        Ok(Secret(
            String::from_utf8(plain).context("decrypted value is not UTF-8")?,
        ))
    }
}

/// Writes a file readable only by its owner.
pub fn write_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts
        .open(path)
        .with_context(|| format!("cannot create {}", path.display()))?;
    f.write_all(contents)?;
    f.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn check_key_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path)?.permissions().mode();
    if mode & 0o077 != 0 {
        bail!(
            "secret key {} is accessible by other users (mode {:o}); run chmod 600",
            path.display(),
            mode & 0o777
        );
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_key_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip() {
        let h = hash_password("correct horse battery").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_password(&h, "correct horse battery"));
        assert!(!verify_password(&h, "wrong"));
        assert!(!verify_password("garbage", "x"));
    }

    #[test]
    fn policy() {
        assert!(check_password_policy("short").is_err());
        assert!(check_password_policy("long enough pw").is_ok());
        assert!(check_password_policy("abcdefghij\n").is_err());
    }

    #[test]
    fn secretbox_roundtrip_and_key_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("k");
        assert!(SecretBox::load(&path, false).is_err());
        let sb = SecretBox::load(&path, true).unwrap();
        let enc = sb.encrypt("hunter2");
        assert!(enc.starts_with("v1:"));
        assert_ne!(sb.encrypt("hunter2"), enc, "nonces must differ");
        assert_eq!(sb.decrypt(&enc).unwrap().expose(), "hunter2");
        // Reloading the same file yields the same key.
        let sb2 = SecretBox::load(&path, false).unwrap();
        assert_eq!(sb2.decrypt(&enc).unwrap().expose(), "hunter2");
        // A different key cannot decrypt.
        let other = SecretBox::from_key(&[7u8; 32]);
        assert!(other.decrypt(&enc).is_err());
        assert!(sb.decrypt("plain").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_world_readable_key() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("k");
        SecretBox::load(&path, true).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(SecretBox::load(&path, false).is_err());
    }

    #[test]
    fn secret_debug_is_redacted() {
        assert_eq!(format!("{:?}", Secret::new("pw")), "Secret(***)");
    }
}
