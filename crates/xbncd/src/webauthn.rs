//! Passkeys (WebAuthn level 2, "none" attestation): registration and
//! sign-in with ES256 or RS256 keys, verified with `ring`.

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use ring::signature;
use sha2::{Digest, Sha256};

// ------------------------------------------------------------ CBOR

#[derive(Debug, Clone, PartialEq)]
pub enum Cbor {
    Uint(u64),
    Neg(i64),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Cbor>),
    Map(Vec<(Cbor, Cbor)>),
    Bool(bool),
    Null,
}

impl Cbor {
    pub fn get(&self, key: &Cbor) -> Option<&Cbor> {
        match self {
            Cbor::Map(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn get_int(&self, k: i64) -> Option<&Cbor> {
        self.get(&if k >= 0 {
            Cbor::Uint(k as u64)
        } else {
            Cbor::Neg(k)
        })
    }
    pub fn get_text(&self, k: &str) -> Option<&Cbor> {
        self.get(&Cbor::Text(k.to_string()))
    }
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Cbor::Bytes(b) => Some(b),
            _ => None,
        }
    }
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Cbor::Uint(u) => i64::try_from(*u).ok(),
            Cbor::Neg(n) => Some(*n),
            _ => None,
        }
    }
}

/// Decodes one CBOR item (definite lengths only); returns it and the bytes
/// it used.
pub fn cbor_decode(b: &[u8]) -> Result<(Cbor, usize)> {
    fn item(b: &[u8], pos: &mut usize, depth: usize) -> Result<Cbor> {
        if depth > 16 {
            bail!("CBOR nested too deeply");
        }
        let first = *b.get(*pos).context("truncated CBOR")?;
        *pos += 1;
        let major = first >> 5;
        let info = first & 0x1f;
        let mut arg = |info: u8| -> Result<u64> {
            let n = match info {
                0..=23 => return Ok(info as u64),
                24 => 1,
                25 => 2,
                26 => 4,
                27 => 8,
                _ => bail!("unsupported CBOR length"),
            };
            let s = b.get(*pos..*pos + n).context("truncated CBOR")?;
            *pos += n;
            Ok(s.iter().fold(0u64, |a, x| (a << 8) | *x as u64))
        };
        Ok(match major {
            0 => Cbor::Uint(arg(info)?),
            1 => {
                let v = arg(info)?;
                Cbor::Neg(-1 - i64::try_from(v).context("CBOR integer too large")?)
            }
            2 | 3 => {
                let n = arg(info)? as usize;
                let s = b
                    .get(*pos..pos.checked_add(n).context("CBOR length")?)
                    .context("truncated CBOR")?;
                *pos += n;
                if major == 2 {
                    Cbor::Bytes(s.to_vec())
                } else {
                    Cbor::Text(String::from_utf8(s.to_vec()).context("invalid CBOR text")?)
                }
            }
            4 => {
                let n = arg(info)? as usize;
                if n > 1024 {
                    bail!("CBOR array too long");
                }
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(item(b, pos, depth + 1)?);
                }
                Cbor::Array(v)
            }
            5 => {
                let n = arg(info)? as usize;
                if n > 1024 {
                    bail!("CBOR map too long");
                }
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    let k = item(b, pos, depth + 1)?;
                    let val = item(b, pos, depth + 1)?;
                    v.push((k, val));
                }
                Cbor::Map(v)
            }
            7 => match info {
                20 => Cbor::Bool(false),
                21 => Cbor::Bool(true),
                22 | 23 => Cbor::Null,
                _ => bail!("unsupported CBOR simple value"),
            },
            _ => bail!("unsupported CBOR type"),
        })
    }
    let mut pos = 0;
    let v = item(b, &mut pos, 0)?;
    Ok((v, pos))
}

// ------------------------------------------------------------ keys

/// A public key of a passkey, ready for `ring`.
#[derive(Debug, Clone, PartialEq)]
pub struct PublicKey {
    /// COSE algorithm: -7 (ES256) or -257 (RS256).
    pub alg: i64,
    /// ES256: uncompressed point; RS256: DER RSAPublicKey.
    pub key: Vec<u8>,
}

/// DER of an RSAPublicKey { n, e }.
pub fn rsa_der(n: &[u8], e: &[u8]) -> Vec<u8> {
    fn len(n: usize) -> Vec<u8> {
        if n < 128 {
            vec![n as u8]
        } else if n < 256 {
            vec![0x81, n as u8]
        } else {
            vec![0x82, (n >> 8) as u8, n as u8]
        }
    }
    fn int(v: &[u8]) -> Vec<u8> {
        let v: Vec<u8> = v.iter().copied().skip_while(|b| *b == 0).collect();
        let mut body = Vec::new();
        if v.first().is_none_or(|b| b & 0x80 != 0) {
            body.push(0);
        }
        body.extend_from_slice(&v);
        let mut out = vec![0x02];
        out.extend(len(body.len()));
        out.extend(body);
        out
    }
    let mut seq = int(n);
    seq.extend(int(e));
    let mut out = vec![0x30];
    out.extend(len(seq.len()));
    out.extend(seq);
    out
}

pub fn parse_cose_key(k: &Cbor) -> Result<PublicKey> {
    let kty = k
        .get_int(1)
        .and_then(Cbor::as_int)
        .context("COSE key without kty")?;
    let alg = k
        .get_int(3)
        .and_then(Cbor::as_int)
        .context("COSE key without alg")?;
    match (kty, alg) {
        (2, -7) => {
            let crv = k.get_int(-1).and_then(Cbor::as_int);
            let x = k.get_int(-2).and_then(Cbor::as_bytes).context("no x")?;
            let y = k.get_int(-3).and_then(Cbor::as_bytes).context("no y")?;
            if crv != Some(1) || x.len() != 32 || y.len() != 32 {
                bail!("only P-256 keys are supported");
            }
            let mut key = vec![4];
            key.extend_from_slice(x);
            key.extend_from_slice(y);
            Ok(PublicKey { alg, key })
        }
        (3, -257) => {
            let n = k.get_int(-1).and_then(Cbor::as_bytes).context("no n")?;
            let e = k.get_int(-2).and_then(Cbor::as_bytes).context("no e")?;
            if n.len() < 256 {
                bail!("RSA keys must be at least 2048 bits");
            }
            Ok(PublicKey {
                alg,
                key: rsa_der(n, e),
            })
        }
        _ => bail!("unsupported key type (kty {kty}, alg {alg})"),
    }
}

impl PublicKey {
    /// Verifies a WebAuthn signature (ASN.1 DER for ES256).
    pub fn verify(&self, msg: &[u8], sig: &[u8]) -> Result<()> {
        let algo: &dyn signature::VerificationAlgorithm = match self.alg {
            -7 => &signature::ECDSA_P256_SHA256_ASN1,
            -257 => &signature::RSA_PKCS1_2048_8192_SHA256,
            _ => bail!("unsupported algorithm"),
        };
        signature::UnparsedPublicKey::new(algo, &self.key)
            .verify(msg, sig)
            .map_err(|_| anyhow::anyhow!("invalid signature"))
    }

    /// Stored form: "<alg>:<base64url key>".
    pub fn encode(&self) -> String {
        format!("{}:{}", self.alg, B64URL.encode(&self.key))
    }
    pub fn decode(s: &str) -> Result<PublicKey> {
        let (alg, key) = s.split_once(':').context("invalid stored key")?;
        Ok(PublicKey {
            alg: alg.parse()?,
            key: B64URL.decode(key)?,
        })
    }
}

// ------------------------------------------------------------ ceremonies

pub struct AuthData<'a> {
    pub rp_id_hash: &'a [u8],
    pub flags: u8,
    pub sign_count: u32,
    /// Present on registration.
    pub credential: Option<(Vec<u8>, PublicKey)>,
}

pub const FLAG_UP: u8 = 0x01;
pub const FLAG_UV: u8 = 0x04;
const FLAG_AT: u8 = 0x40;

pub fn parse_auth_data(d: &[u8]) -> Result<AuthData<'_>> {
    if d.len() < 37 {
        bail!("authenticator data too short");
    }
    let flags = d[32];
    let sign_count = u32::from_be_bytes([d[33], d[34], d[35], d[36]]);
    let mut credential = None;
    if flags & FLAG_AT != 0 {
        let rest = &d[37..];
        if rest.len() < 18 {
            bail!("truncated attested credential data");
        }
        let id_len = u16::from_be_bytes([rest[16], rest[17]]) as usize;
        let id = rest
            .get(18..18 + id_len)
            .context("truncated credential id")?
            .to_vec();
        if id.is_empty() || id.len() > 1023 {
            bail!("invalid credential id");
        }
        let (key, _) = cbor_decode(&rest[18 + id_len..])?;
        credential = Some((id, parse_cose_key(&key)?));
    }
    Ok(AuthData {
        rp_id_hash: &d[..32],
        flags,
        sign_count,
        credential,
    })
}

fn check_client_data(raw: &[u8], kind: &str, challenge: &[u8], origin: &str) -> Result<()> {
    let v: serde_json::Value = serde_json::from_slice(raw).context("invalid clientDataJSON")?;
    if v["type"] != kind {
        bail!("wrong ceremony type");
    }
    let got = v["challenge"]
        .as_str()
        .and_then(|c| B64URL.decode(c.trim_end_matches('=')).ok());
    if got.as_deref() != Some(challenge) {
        bail!("the challenge does not match");
    }
    if v["origin"].as_str() != Some(origin) {
        bail!("the origin does not match ({})", v["origin"]);
    }
    Ok(())
}

fn b64(s: &str) -> Result<Vec<u8>> {
    B64URL
        .decode(s.trim_end_matches('='))
        .context("invalid base64url")
}

/// Registration: returns (credential id, key, sign count).
pub fn verify_registration(
    client_data_b64: &str,
    attestation_b64: &str,
    challenge: &[u8],
    origin: &str,
    rp_id: &str,
) -> Result<(Vec<u8>, PublicKey, u32)> {
    let cd = b64(client_data_b64)?;
    check_client_data(&cd, "webauthn.create", challenge, origin)?;
    let (att, _) = cbor_decode(&b64(attestation_b64)?)?;
    let auth = att
        .get_text("authData")
        .and_then(Cbor::as_bytes)
        .context("no authData")?;
    let ad = parse_auth_data(auth)?;
    if ad.rp_id_hash != Sha256::digest(rp_id.as_bytes()).as_slice() {
        bail!("the passkey is for another site");
    }
    if ad.flags & FLAG_UP == 0 {
        bail!("user presence was not confirmed");
    }
    let (id, key) = ad.credential.context("no credential in the registration")?;
    Ok((id, key, ad.sign_count))
}

/// Sign-in: returns the new sign count and whether the user was verified.
#[allow(clippy::too_many_arguments)]
pub fn verify_assertion(
    key: &PublicKey,
    stored_count: u32,
    client_data_b64: &str,
    auth_data_b64: &str,
    signature_b64: &str,
    challenge: &[u8],
    origin: &str,
    rp_id: &str,
) -> Result<(u32, bool)> {
    let cd = b64(client_data_b64)?;
    check_client_data(&cd, "webauthn.get", challenge, origin)?;
    let auth = b64(auth_data_b64)?;
    let ad = parse_auth_data(&auth)?;
    if ad.rp_id_hash != Sha256::digest(rp_id.as_bytes()).as_slice() {
        bail!("the passkey is for another site");
    }
    if ad.flags & FLAG_UP == 0 {
        bail!("user presence was not confirmed");
    }
    let mut msg = auth.clone();
    msg.extend_from_slice(&Sha256::digest(&cd));
    key.verify(&msg, &b64(signature_b64)?)?;
    if (ad.sign_count != 0 || stored_count != 0) && ad.sign_count <= stored_count {
        bail!("the passkey's counter went back (a cloned authenticator?)");
    }
    Ok((ad.sign_count, ad.flags & FLAG_UV != 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING};

    fn cbor_bytes(v: &[u8]) -> Vec<u8> {
        let mut out = vec![0x58, v.len() as u8];
        out.extend_from_slice(v);
        out
    }

    /// Builds what a browser would send, with a software authenticator.
    #[test]
    fn register_and_sign_in_with_a_software_passkey() {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
        let kp = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng)
            .unwrap();
        let pk = kp.public_key().as_ref();
        let (x, y) = (&pk[1..33], &pk[33..65]);
        // COSE key {1:2, 3:-7, -1:1, -2:x, -3:y}
        let mut cose = vec![0xa5, 0x01, 0x02, 0x03, 0x26, 0x20, 0x01, 0x21];
        cose.extend(cbor_bytes(x));
        cose.push(0x22);
        cose.extend(cbor_bytes(y));
        let rp = "bnc.example.org";
        let origin = "https://bnc.example.org";
        let cred_id = [9u8; 16];
        let mut auth = Sha256::digest(rp.as_bytes()).to_vec();
        auth.push(FLAG_UP | FLAG_UV | 0x40);
        auth.extend_from_slice(&0u32.to_be_bytes());
        auth.extend_from_slice(&[0u8; 16]);
        auth.extend_from_slice(&(cred_id.len() as u16).to_be_bytes());
        auth.extend_from_slice(&cred_id);
        auth.extend_from_slice(&cose);
        // {"fmt":"none","attStmt":{},"authData":...}
        let mut att = vec![0xa3, 0x63];
        att.extend_from_slice(b"fmt");
        att.extend_from_slice(&[0x64]);
        att.extend_from_slice(b"none");
        att.push(0x67);
        att.extend_from_slice(b"attStmt");
        att.push(0xa0);
        att.push(0x68);
        att.extend_from_slice(b"authData");
        att.extend_from_slice(&[0x59, (auth.len() >> 8) as u8, auth.len() as u8]);
        att.extend_from_slice(&auth);
        let challenge = [5u8; 32];
        let cd = serde_json::json!({"type":"webauthn.create","challenge":B64URL.encode(challenge),"origin":origin}).to_string();
        let (id, key, count) = verify_registration(
            &B64URL.encode(&cd),
            &B64URL.encode(&att),
            &challenge,
            origin,
            rp,
        )
        .unwrap();
        assert_eq!((id.as_slice(), count), (&cred_id[..], 0));
        assert!(verify_registration(
            &B64URL.encode(&cd),
            &B64URL.encode(&att),
            &challenge,
            "https://evil.example",
            rp
        )
        .is_err());
        let key = PublicKey::decode(&key.encode()).unwrap();

        // Sign in.
        let challenge = [6u8; 32];
        let cd = serde_json::json!({"type":"webauthn.get","challenge":B64URL.encode(challenge),"origin":origin}).to_string();
        let mut ad = Sha256::digest(rp.as_bytes()).to_vec();
        ad.push(FLAG_UP | FLAG_UV);
        ad.extend_from_slice(&5u32.to_be_bytes());
        let mut msg = ad.clone();
        msg.extend_from_slice(&Sha256::digest(cd.as_bytes()));
        let sig = kp.sign(&rng, &msg).unwrap();
        let ok = verify_assertion(
            &key,
            0,
            &B64URL.encode(&cd),
            &B64URL.encode(&ad),
            &B64URL.encode(sig.as_ref()),
            &challenge,
            origin,
            rp,
        )
        .unwrap();
        assert_eq!(ok, (5, true));
        // Replayed counter, wrong challenge, tampered data.
        assert!(verify_assertion(
            &key,
            5,
            &B64URL.encode(&cd),
            &B64URL.encode(&ad),
            &B64URL.encode(sig.as_ref()),
            &challenge,
            origin,
            rp
        )
        .is_err());
        assert!(verify_assertion(
            &key,
            0,
            &B64URL.encode(&cd),
            &B64URL.encode(&ad),
            &B64URL.encode(sig.as_ref()),
            &[7u8; 32],
            origin,
            rp
        )
        .is_err());
        let mut bad = ad.clone();
        bad[33] = 9;
        assert!(verify_assertion(
            &key,
            0,
            &B64URL.encode(&cd),
            &B64URL.encode(&bad),
            &B64URL.encode(sig.as_ref()),
            &challenge,
            origin,
            rp
        )
        .is_err());
    }

    #[test]
    fn rsa_der_encoding() {
        let n = vec![0xc0; 256];
        let der = rsa_der(&n, &[1, 0, 1]);
        assert_eq!(&der[..4], &[0x30, 0x82, 0x01, 0x0a]);
        assert!(
            cbor_decode(&[0x9f]).is_err(),
            "indefinite lengths are refused"
        );
    }
}
