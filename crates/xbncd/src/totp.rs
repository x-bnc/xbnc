//! Time-based one-time passwords (RFC 6238, HMAC-SHA1, 6 digits, 30 s),
//! compatible with Google Authenticator, Aegis, 1Password, FreeOTP, etc.

use hmac::{Hmac, Mac};
use sha1::Sha1;

const STEP: u64 = 30;
const DIGITS: u32 = 6;

/// Generates a random 160-bit secret encoded in base32.
pub fn generate_secret() -> String {
    use chacha20poly1305::aead::rand_core::RngCore;
    let mut bytes = [0u8; 20];
    chacha20poly1305::aead::OsRng.fill_bytes(&mut bytes);
    base32_encode(&bytes)
}

fn base32_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for &b in data {
        buffer = (buffer << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            out.push(ALPHABET[((buffer >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for c in s.chars().filter(|c| !c.is_whitespace() && *c != '=') {
        let v = match c.to_ascii_uppercase() {
            c @ 'A'..='Z' => c as u32 - 'A' as u32,
            c @ '2'..='7' => c as u32 - '2' as u32 + 26,
            _ => return None,
        };
        buffer = (buffer << 5) | v;
        bits += 5;
        if bits >= 8 {
            out.push((buffer >> (bits - 8)) as u8);
            bits -= 8;
        }
    }
    Some(out)
}

fn hotp(key: &[u8], counter: u64) -> u32 {
    let mut mac = Hmac::<Sha1>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(&counter.to_be_bytes());
    let h = mac.finalize().into_bytes();
    let off = (h[19] & 0x0f) as usize;
    let bin = ((h[off] as u32 & 0x7f) << 24)
        | ((h[off + 1] as u32) << 16)
        | ((h[off + 2] as u32) << 8)
        | h[off + 3] as u32;
    bin % 10u32.pow(DIGITS)
}

/// Code for a given Unix time (seconds).
pub fn code_at(secret: &str, unix: u64) -> Option<String> {
    let key = base32_decode(secret)?;
    Some(format!("{:06}", hotp(&key, unix / STEP)))
}

/// Verifies a code allowing one step of clock drift either way.
pub fn verify(secret: &str, code: &str, unix: u64) -> bool {
    let code = code.trim().replace(' ', "");
    if code.len() != DIGITS as usize || !code.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let Some(key) = base32_decode(secret) else {
        return false;
    };
    let step = unix / STEP;
    [step.saturating_sub(1), step, step + 1]
        .iter()
        .any(|s| format!("{:06}", hotp(&key, *s)) == code)
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `otpauth://` URI for authenticator apps.
pub fn provisioning_uri(secret: &str, account: &str, issuer: &str) -> String {
    let enc = |s: &str| {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect::<String>()
    };
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm=SHA1&digits=6&period=30",
        enc(issuer),
        enc(account),
        secret,
        enc(issuer)
    )
}

/// The provisioning URI as an SVG QR code.
pub fn qr_svg(uri: &str) -> String {
    use qrcode::render::svg;
    match qrcode::QrCode::new(uri.as_bytes()) {
        Ok(code) => code
            .render::<svg::Color>()
            .min_dimensions(200, 200)
            .quiet_zone(true)
            .build(),
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 6238 Appendix B test vectors (SHA1, secret "12345678901234567890").
    #[test]
    fn rfc6238_vectors() {
        let secret = base32_encode(b"12345678901234567890");
        assert_eq!(secret, "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        for (t, expected) in [
            (59, "287082"),
            (1111111109, "081804"),
            (1234567890, "005924"),
            (2000000000, "279037"),
        ] {
            assert_eq!(code_at(&secret, t).unwrap(), expected, "t={t}");
        }
    }

    #[test]
    fn verify_with_drift() {
        let secret = generate_secret();
        assert_eq!(secret.len(), 32);
        let now = 1_700_000_000;
        let code = code_at(&secret, now).unwrap();
        assert!(verify(&secret, &code, now));
        assert!(verify(&secret, &code, now + 30));
        assert!(!verify(&secret, &code, now + 90));
        assert!(!verify(&secret, "12345", now));
        assert!(!verify(&secret, "abcdef", now));
    }

    #[test]
    fn uri_and_qr() {
        let uri = provisioning_uri("ABC", "alice", "xBNC bnc.example");
        assert!(uri.starts_with("otpauth://totp/xBNC%20bnc.example:alice?secret=ABC"));
        assert!(qr_svg(&uri).contains("<svg"));
    }
}
