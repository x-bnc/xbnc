//! Web Push (RFC 8030/8291/8292): notifications to the panel installed as an
//! app on a phone or computer, through the browser's push service.

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use ring::rand::{SecureRandom, SystemRandom};
use ring::{aead, agreement, hkdf, signature};

/// The server's VAPID key pair (P-256), kept encrypted in the settings.
pub struct Vapid {
    pkcs8: Vec<u8>,
    pub public_b64: String,
}

impl Vapid {
    pub fn generate() -> Result<Vapid> {
        let rng = SystemRandom::new();
        let pkcs8 = signature::EcdsaKeyPair::generate_pkcs8(
            &signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            &rng,
        )
        .map_err(|_| anyhow::anyhow!("cannot create the push key"))?;
        Vapid::from_pkcs8(pkcs8.as_ref())
    }

    pub fn from_pkcs8(pkcs8: &[u8]) -> Result<Vapid> {
        let kp = signature::EcdsaKeyPair::from_pkcs8(
            &signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            pkcs8,
            &SystemRandom::new(),
        )
        .map_err(|_| anyhow::anyhow!("invalid push key"))?;
        use signature::KeyPair;
        Ok(Vapid {
            pkcs8: pkcs8.to_vec(),
            public_b64: B64URL.encode(kp.public_key().as_ref()),
        })
    }

    pub fn pkcs8(&self) -> &[u8] {
        &self.pkcs8
    }

    /// `Authorization` header value for a push service.
    pub fn authorization(&self, endpoint: &str, subject: &str, now_s: i64) -> Result<String> {
        let u = crate::http::Url::parse(endpoint).context("invalid push endpoint")?;
        let aud = if u.port == 443 {
            format!("https://{}", u.host)
        } else {
            format!("https://{}:{}", u.host, u.port)
        };
        let header = B64URL.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = B64URL.encode(
            serde_json::json!({ "aud": aud, "exp": now_s + 12 * 3600, "sub": subject }).to_string(),
        );
        let input = format!("{header}.{claims}");
        let kp = signature::EcdsaKeyPair::from_pkcs8(
            &signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            &self.pkcs8,
            &SystemRandom::new(),
        )
        .map_err(|_| anyhow::anyhow!("invalid push key"))?;
        let sig = kp
            .sign(&SystemRandom::new(), input.as_bytes())
            .map_err(|_| anyhow::anyhow!("cannot sign"))?;
        Ok(format!(
            "vapid t={input}.{}, k={}",
            B64URL.encode(sig.as_ref()),
            self.public_b64
        ))
    }
}

struct Len(usize);
impl hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

fn hkdf(salt: &[u8], ikm: &[u8], info: &[u8], len: usize) -> Result<Vec<u8>> {
    let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, salt).extract(ikm);
    let mut out = vec![0u8; len];
    prk.expand(&[info], Len(len))
        .and_then(|o| o.fill(&mut out))
        .map_err(|_| anyhow::anyhow!("hkdf failed"))?;
    Ok(out)
}

/// Encrypts a payload for one subscription (`aes128gcm`, RFC 8291).
pub fn encrypt(p256dh_b64: &str, auth_b64: &str, payload: &[u8]) -> Result<Vec<u8>> {
    let ua_public = B64URL
        .decode(p256dh_b64.trim_end_matches('='))
        .context("invalid p256dh")?;
    let auth = B64URL
        .decode(auth_b64.trim_end_matches('='))
        .context("invalid auth")?;
    if ua_public.len() != 65 || auth.len() < 16 {
        bail!("invalid subscription keys");
    }
    let rng = SystemRandom::new();
    let private = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng)
        .map_err(|_| anyhow::anyhow!("cannot create a key"))?;
    let as_public = private
        .compute_public_key()
        .map_err(|_| anyhow::anyhow!("cannot create a key"))?
        .as_ref()
        .to_vec();
    let ecdh = agreement::agree_ephemeral(
        private,
        &agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, &ua_public),
        |s| s.to_vec(),
    )
    .map_err(|_| anyhow::anyhow!("invalid subscription key"))?;
    let mut key_info = b"WebPush: info\0".to_vec();
    key_info.extend_from_slice(&ua_public);
    key_info.extend_from_slice(&as_public);
    let ikm = hkdf(&auth, &ecdh, &key_info, 32)?;
    let mut salt = [0u8; 16];
    rng.fill(&mut salt)
        .map_err(|_| anyhow::anyhow!("no randomness"))?;
    let cek = hkdf(&salt, &ikm, b"Content-Encoding: aes128gcm\0", 16)?;
    let nonce = hkdf(&salt, &ikm, b"Content-Encoding: nonce\0", 12)?;
    let mut data = payload.to_vec();
    data.push(2); // last (and only) record
    let key = aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_128_GCM, &cek).map_err(|_| anyhow::anyhow!("aead"))?,
    );
    key.seal_in_place_append_tag(
        aead::Nonce::try_assume_unique_for_key(&nonce).map_err(|_| anyhow::anyhow!("nonce"))?,
        aead::Aad::empty(),
        &mut data,
    )
    .map_err(|_| anyhow::anyhow!("cannot encrypt"))?;
    let mut body = salt.to_vec();
    body.extend_from_slice(&4096u32.to_be_bytes());
    body.push(65);
    body.extend_from_slice(&as_public);
    body.extend_from_slice(&data);
    Ok(body)
}

/// Sends one notification. Ok(false) means the subscription no longer
/// exists (it should be deleted).
pub async fn send(
    tls: std::sync::Arc<rustls::ClientConfig>,
    vapid: &Vapid,
    subject: &str,
    endpoint: &str,
    p256dh: &str,
    auth: &str,
    payload: &[u8],
) -> Result<bool> {
    let body = encrypt(p256dh, auth, payload)?;
    let authz = vapid.authorization(endpoint, subject, crate::db::now_ms() / 1000)?;
    let r = crate::http::request_public(
        "POST",
        endpoint,
        Some(("application/octet-stream", &body)),
        tls,
        16 * 1024,
        std::time::Duration::from_secs(15),
        &[
            ("Authorization", &authz),
            ("Content-Encoding", "aes128gcm"),
            ("TTL", "86400"),
            ("Urgency", "high"),
        ],
    )
    .await?;
    match r.status {
        200..=299 => Ok(true),
        404 | 410 => Ok(false),
        s => bail!("the push service answered {s}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decrypts like a browser would (to check the encryption).
    fn decrypt(
        ua_private: agreement::EphemeralPrivateKey,
        ua_public: &[u8],
        auth: &[u8],
        body: &[u8],
    ) -> Vec<u8> {
        let salt = &body[..16];
        let idlen = body[20] as usize;
        let as_public = &body[21..21 + idlen];
        let ct = &body[21 + idlen..];
        let ecdh = agreement::agree_ephemeral(
            ua_private,
            &agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, as_public),
            |s| s.to_vec(),
        )
        .unwrap();
        let mut info = b"WebPush: info\0".to_vec();
        info.extend_from_slice(ua_public);
        info.extend_from_slice(as_public);
        let ikm = hkdf(auth, &ecdh, &info, 32).unwrap();
        let cek = hkdf(salt, &ikm, b"Content-Encoding: aes128gcm\0", 16).unwrap();
        let nonce = hkdf(salt, &ikm, b"Content-Encoding: nonce\0", 12).unwrap();
        let key = aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_128_GCM, &cek).unwrap());
        let mut data = ct.to_vec();
        let plain = key
            .open_in_place(
                aead::Nonce::try_assume_unique_for_key(&nonce).unwrap(),
                aead::Aad::empty(),
                &mut data,
            )
            .unwrap()
            .to_vec();
        assert_eq!(*plain.last().unwrap(), 2);
        plain[..plain.len() - 1].to_vec()
    }

    #[test]
    fn encrypts_for_a_browser_and_signs_vapid() {
        let rng = SystemRandom::new();
        let ua = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).unwrap();
        let ua_public = ua.compute_public_key().unwrap().as_ref().to_vec();
        let auth = [7u8; 16];
        let body = encrypt(
            &B64URL.encode(&ua_public),
            &B64URL.encode(auth),
            b"hola, alice",
        )
        .unwrap();
        assert_eq!(decrypt(ua, &ua_public, &auth, &body), b"hola, alice");

        let v = Vapid::generate().unwrap();
        let v2 = Vapid::from_pkcs8(v.pkcs8()).unwrap();
        assert_eq!(v.public_b64, v2.public_b64);
        let h = v
            .authorization("https://push.example.org/send/abc", "mailto:a@b", 1000)
            .unwrap();
        assert!(h.starts_with("vapid t=") && h.contains(&format!("k={}", v.public_b64)));
        // The JWT verifies with the public key.
        let jwt = h.trim_start_matches("vapid t=").split(',').next().unwrap();
        let (input, sig) = jwt.rsplit_once('.').unwrap();
        let pk = B64URL.decode(&v.public_b64).unwrap();
        signature::UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_FIXED, pk)
            .verify(input.as_bytes(), &B64URL.decode(sig).unwrap())
            .unwrap();
        let claims: serde_json::Value =
            serde_json::from_slice(&B64URL.decode(input.split('.').nth(1).unwrap()).unwrap())
                .unwrap();
        assert_eq!(claims["aud"], "https://push.example.org");
    }
}
