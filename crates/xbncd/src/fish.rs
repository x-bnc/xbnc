//! FiSH message encryption (the `crypt` module): Blowfish in ECB mode with
//! FiSH's own base64, Blowfish-CBC (`+OK *...`), and the DH1080 key
//! exchange used for private conversations. Compatible with FiSH for mIRC,
//! HexChat, irssi and WeeChat.
//!
//! FiSH is old cryptography (Blowfish, ECB without integrity). It keeps
//! messages away from the network's servers and casual logging; it is not a
//! substitute for end-to-end encryption.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use blowfish::cipher::{generic_array::GenericArray, BlockDecrypt, BlockEncrypt, KeyInit};
use blowfish::Blowfish;
use chacha20poly1305::aead::rand_core::RngCore;
use num_bigint::BigUint;

const FISH_B64: &[u8; 64] = b"./0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Most plain-text bytes encrypted into one IRC message.
pub const MAX_PLAIN: usize = 280;

fn cipher(key: &str) -> Option<Blowfish> {
    Blowfish::new_from_slice(key.as_bytes()).ok()
}

fn pad(text: &[u8]) -> Vec<u8> {
    let mut v = text.to_vec();
    while v.is_empty() || !v.len().is_multiple_of(8) {
        v.push(0);
    }
    v
}

fn unpad(mut v: Vec<u8>) -> String {
    while v.last() == Some(&0) {
        v.pop();
    }
    String::from_utf8(v.clone()).unwrap_or_else(|_| crate::util::Legacy::Cp1252.decode(&v))
}

fn fish_b64_block(block: &[u8], out: &mut String) {
    let left = u32::from_be_bytes([block[0], block[1], block[2], block[3]]);
    let right = u32::from_be_bytes([block[4], block[5], block[6], block[7]]);
    for mut half in [right, left] {
        for _ in 0..6 {
            out.push(FISH_B64[(half & 0x3f) as usize] as char);
            half >>= 6;
        }
    }
}

fn fish_b64_unblock(chunk: &[u8]) -> Option<[u8; 8]> {
    let idx = |c: u8| FISH_B64.iter().position(|x| *x == c).map(|i| i as u32);
    let mut halves = [0u32; 2];
    for (h, part) in halves.iter_mut().zip(chunk.chunks(6)) {
        for (i, c) in part.iter().enumerate() {
            *h |= idx(*c)?.wrapping_shl(6 * i as u32);
        }
    }
    let (right, left) = (halves[0], halves[1]);
    let mut b = [0u8; 8];
    b[..4].copy_from_slice(&left.to_be_bytes());
    b[4..].copy_from_slice(&right.to_be_bytes());
    Some(b)
}

/// Encrypts `text` into the body of a FiSH message (`+OK ...`).
pub fn encrypt(key: &str, cbc: bool, text: &str) -> Option<String> {
    let bf = cipher(key)?;
    let mut data = pad(text.as_bytes());
    if cbc {
        let mut iv = [0u8; 8];
        chacha20poly1305::aead::OsRng.fill_bytes(&mut iv);
        let mut prev = iv;
        for block in data.chunks_mut(8) {
            for (b, p) in block.iter_mut().zip(prev.iter()) {
                *b ^= p;
            }
            bf.encrypt_block(GenericArray::from_mut_slice(block));
            prev.copy_from_slice(block);
        }
        let mut all = iv.to_vec();
        all.extend_from_slice(&data);
        return Some(format!("+OK *{}", B64.encode(all)));
    }
    let mut out = String::with_capacity(data.len() * 3 / 2 + 4);
    out.push_str("+OK ");
    for block in data.chunks_mut(8) {
        bf.encrypt_block(GenericArray::from_mut_slice(block));
        fish_b64_block(block, &mut out);
    }
    Some(out)
}

/// Decrypts the body of a FiSH message (after `+OK ` or `mcps `). CBC is
/// recognised by its leading `*`.
pub fn decrypt(key: &str, body: &str) -> Option<String> {
    let bf = cipher(key)?;
    let body = body.trim();
    if let Some(b64) = body.strip_prefix('*') {
        let raw = B64
            .decode(b64.trim_end_matches('='))
            .ok()
            .or_else(|| B64.decode(b64).ok())?;
        if raw.len() < 16 || !raw.len().is_multiple_of(8) {
            return None;
        }
        let mut prev: [u8; 8] = raw[..8].try_into().ok()?;
        let mut out = Vec::with_capacity(raw.len() - 8);
        for block in raw[8..].chunks(8) {
            let mut b: [u8; 8] = block.try_into().ok()?;
            let saved = b;
            bf.decrypt_block(GenericArray::from_mut_slice(&mut b));
            for (x, p) in b.iter_mut().zip(prev.iter()) {
                *x ^= p;
            }
            out.extend_from_slice(&b);
            prev = saved;
        }
        return Some(unpad(out));
    }
    let bytes = body.as_bytes();
    if bytes.len() < 12 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 12 * 8);
    for chunk in bytes.chunks_exact(12) {
        let mut b = fish_b64_unblock(chunk)?;
        bf.decrypt_block(GenericArray::from_mut_slice(&mut b));
        out.extend_from_slice(&b);
    }
    Some(unpad(out))
}

/// Splits long text so each encrypted message fits in an IRC line.
pub fn split(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if cur.len() + c.len_utf8() > MAX_PLAIN {
            parts.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() || parts.is_empty() {
        parts.push(cur);
    }
    parts
}

// ------------------------------------------------------------------ DH1080

const DH1080_PRIME: &str = "FBE1022E23D213E8ACFA9AE8B9DFADA3EA6B7AC7A7B7E95AB5EB2DF858921FEADE95E6AC7BE7DE6ADBAB8A783E7AF7A7FA6A2B7BEB1E72EAE2B72F9FA2BFB2A2EFBEFAC868BADB3E828FA8BADFADA3E4CC1BE7E8AFE85E9698A783EB68FA07A77AB6AD7BEB618ACF9CA2897EB28A6189EFA07AB99A8A7FA9AE299EFA7BA66DEAFEFBEFBF0B7D8B";

fn prime() -> BigUint {
    BigUint::parse_bytes(DH1080_PRIME.as_bytes(), 16).expect("valid DH1080 prime")
}

/// DH1080's base64: no padding, plus an `A` when no padding was needed.
fn dh_b64_encode(bytes: &[u8]) -> String {
    let mut s = base64::engine::general_purpose::STANDARD_NO_PAD.encode(bytes);
    if bytes.len().is_multiple_of(3) {
        s.push('A');
    }
    s
}

fn dh_b64_decode(s: &str) -> Option<Vec<u8>> {
    let mut s = s.trim().trim_end_matches('=').to_string();
    if s.len() % 4 == 1 && s.ends_with('A') {
        s.pop();
    }
    base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(s)
        .ok()
}

/// One side of a DH1080 exchange.
pub struct Dh1080 {
    private: BigUint,
    /// Our public key, encoded for the INIT/FINISH notice.
    pub public: String,
}

impl Default for Dh1080 {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Dh1080 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Dh1080 { .. }")
    }
}

impl Dh1080 {
    pub fn new() -> Dh1080 {
        let p = prime();
        let mut raw = [0u8; 135];
        chacha20poly1305::aead::OsRng.fill_bytes(&mut raw);
        let private = BigUint::from_bytes_be(&raw) % (&p - 2u32) + 1u32;
        let public = BigUint::from(2u32).modpow(&private, &p);
        Dh1080 {
            private,
            public: dh_b64_encode(&public.to_bytes_be()),
        }
    }

    /// The shared Blowfish key from the other side's public key, or None if
    /// that key is not valid.
    pub fn shared_key(&self, their_public: &str) -> Option<String> {
        let p = prime();
        let theirs = BigUint::from_bytes_be(&dh_b64_decode(their_public)?);
        if theirs <= BigUint::from(1u32) || theirs >= &p - 1u32 {
            return None;
        }
        let secret = theirs.modpow(&self.private, &p);
        use sha2::Digest;
        let digest = sha2::Sha256::digest(secret.to_bytes_be());
        Some(dh_b64_encode(&digest))
    }
}

/// A DH1080 notice: `DH1080_INIT <key> [CBC]` or `DH1080_FINISH <key> [CBC]`.
/// Returns (is_init, public key, cbc).
pub fn parse_dh(text: &str) -> Option<(bool, String, bool)> {
    let mut w = text.split_whitespace();
    let init = match w.next()? {
        "DH1080_INIT" | "DH1080_INIT_cbc" => true,
        "DH1080_FINISH" | "DH1080_FINISH_cbc" => false,
        _ => return None,
    };
    let key = w.next()?.to_string();
    if key.len() < 100 || key.len() > 400 {
        return None;
    }
    let cbc = text.contains("_cbc") || w.any(|x| x.eq_ignore_ascii_case("CBC"));
    Some((init, key, cbc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prime_is_1080_bits() {
        assert_eq!(prime().bits(), 1080);
    }

    #[test]
    fn ecb_round_trip_and_format() {
        let enc = encrypt("secretkey", false, "hola, qué tal").unwrap();
        assert!(enc.starts_with("+OK "));
        // 15 bytes → 2 blocks → 24 characters.
        assert_eq!(enc.len(), 4 + 24);
        assert_eq!(decrypt("secretkey", &enc[4..]).unwrap(), "hola, qué tal");
        assert_ne!(
            decrypt("otherkey", &enc[4..]).as_deref(),
            Some("hola, qué tal")
        );
    }

    #[test]
    fn ecb_known_vector() {
        // Computed with PyCryptodome's Blowfish and the FiSH base64 of the
        // FiSH reference implementation.
        assert_eq!(encrypt("xbnc", false, "test").unwrap(), "+OK PpHfh03YXCf.");
        assert_eq!(
            encrypt("secretkey", false, "hola, qué tal").unwrap(),
            "+OK idb1o15zeB51glK4m.YYdgZ."
        );
        assert_eq!(decrypt("xbnc", "PpHfh03YXCf.").unwrap(), "test");
    }

    #[test]
    fn cbc_round_trip() {
        let enc = encrypt("secretkey", true, "mensaje largo con CBC").unwrap();
        assert!(enc.starts_with("+OK *"));
        assert_eq!(
            decrypt("secretkey", &enc[4..]).unwrap(),
            "mensaje largo con CBC"
        );
        // A random IV: the same text encrypts differently.
        assert_ne!(
            enc,
            encrypt("secretkey", true, "mensaje largo con CBC").unwrap()
        );
    }

    #[test]
    fn garbage_does_not_panic() {
        for body in ["", "*", "*AAAA", "abc", "!!!!!!!!!!!!", "*@@@@"] {
            let _ = decrypt("secretkey", body);
        }
        assert!(encrypt("abc", false, "x").is_none(), "key too short");
    }

    #[test]
    fn dh1080_agrees() {
        let a = Dh1080::new();
        let b = Dh1080::new();
        let ka = a.shared_key(&b.public).unwrap();
        let kb = b.shared_key(&a.public).unwrap();
        assert_eq!(ka, kb);
        assert_eq!(ka.len(), 43);
        assert!(a.shared_key("AAAA").is_none());
        let (init, key, cbc) = parse_dh(&format!("DH1080_INIT {} CBC", a.public)).unwrap();
        assert!(init && cbc && key == a.public);
        assert!(parse_dh("DH1080_FINISH short").is_none());
    }

    #[test]
    fn splitting() {
        let long = "ñ".repeat(500);
        let parts = split(&long);
        assert!(parts.iter().all(|p| p.len() <= MAX_PLAIN));
        assert_eq!(parts.concat(), long);
    }
}
