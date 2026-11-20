//! Signed releases: an Ed25519 signature over the package name and its
//! SHA-256, so the panel's one-click update only ever installs a package
//! made by whoever holds the release key.
//!
//! Signature file (`<package>.sig`):
//!
//! ```text
//! xbnc-signature-v1
//! <base64 Ed25519 signature of "xbnc-release-v1\n<name>\n<sha256 hex>\n">
//! ```

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
use sha2::{Digest, Sha256};

const HEADER: &str = "xbnc-signature-v1";

fn message(name: &str, data: &[u8]) -> Vec<u8> {
    let hex: String = Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("xbnc-release-v1\n{name}\n{hex}\n").into_bytes()
}

/// A new release key: (private key, PKCS#8 base64; public key, base64).
pub fn keygen() -> Result<(String, String)> {
    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|_| anyhow::anyhow!("key generation failed"))?;
    let kp =
        Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|_| anyhow::anyhow!("invalid key"))?;
    Ok((
        B64.encode(pkcs8.as_ref()),
        B64.encode(kp.public_key().as_ref()),
    ))
}

/// Signs a package; returns the contents of its `.sig` file.
pub fn sign(private_b64: &str, name: &str, data: &[u8]) -> Result<String> {
    let pkcs8 = B64
        .decode(private_b64.trim())
        .context("the private key is not base64")?;
    let kp = Ed25519KeyPair::from_pkcs8(&pkcs8)
        .map_err(|_| anyhow::anyhow!("not an Ed25519 private key"))?;
    let sig = kp.sign(&message(name, data));
    Ok(format!("{HEADER}\n{}\n", B64.encode(sig.as_ref())))
}

/// Checks a package against its signature and the trusted public key.
pub fn verify(public_b64: &str, name: &str, data: &[u8], sig_file: &str) -> Result<()> {
    let public = B64
        .decode(public_b64.trim())
        .context("the public key is not base64")?;
    if public.len() != 32 {
        bail!("the public key is not an Ed25519 key");
    }
    let mut lines = sig_file.lines();
    if lines.next().map(str::trim) != Some(HEADER) {
        bail!("not an xBNC signature file");
    }
    let sig = B64
        .decode(lines.next().unwrap_or("").trim())
        .context("the signature is not base64")?;
    UnparsedPublicKey::new(&ED25519, &public)
        .verify(&message(name, data), &sig)
        .map_err(|_| anyhow::anyhow!("the signature does not match: the package was not made with the release key, or it was changed"))
}

/// Version in a package file name: xbnc_0.7.0_amd64.deb, xbnc-0.7.0-1.x86_64.rpm.
pub fn package_version(name: &str) -> Option<String> {
    let rest = name
        .strip_prefix("xbnc_")
        .or_else(|| name.strip_prefix("xbnc-"))?;
    let v: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let v = v.trim_end_matches('.').to_string();
    (v.split('.').count() == 3 && v.split('.').all(|p| !p.is_empty())).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify() {
        let (private, public) = keygen().unwrap();
        let sig = sign(&private, "xbnc_0.7.0_amd64.deb", b"package").unwrap();
        verify(&public, "xbnc_0.7.0_amd64.deb", b"package", &sig).unwrap();
        // Changed content, renamed file (e.g. an old version) or another key fail.
        assert!(verify(&public, "xbnc_0.7.0_amd64.deb", b"packagE", &sig).is_err());
        assert!(verify(&public, "xbnc_0.8.0_amd64.deb", b"package", &sig).is_err());
        let (_, other) = keygen().unwrap();
        assert!(verify(&other, "xbnc_0.7.0_amd64.deb", b"package", &sig).is_err());
        assert!(verify(&public, "xbnc_0.7.0_amd64.deb", b"package", "garbage").is_err());
    }

    #[test]
    fn versions_from_names() {
        assert_eq!(
            package_version("xbnc_0.7.0_amd64.deb").as_deref(),
            Some("0.7.0")
        );
        assert_eq!(
            package_version("xbnc-1.2.3-1.x86_64.rpm").as_deref(),
            Some("1.2.3")
        );
        assert_eq!(package_version("other_1.0.0.deb"), None);
        assert_eq!(package_version("xbnc_1.0_amd64.deb"), None);
    }
}
