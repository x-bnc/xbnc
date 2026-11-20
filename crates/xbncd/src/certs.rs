//! Client certificates for IRC networks (the `cert` module): CertFP
//! identification and SASL EXTERNAL.
//!
//! A certificate is either generated here (self-signed, ECDSA P-256, valid
//! 10 years, which is what IRC services expect) or uploaded as PEM. The
//! private key is stored encrypted with the server's secret key.

use anyhow::{bail, Context, Result};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use std::sync::Arc;

/// Generates a self-signed client certificate; returns (cert PEM, key PEM).
pub fn generate(common_name: &str) -> Result<(String, String)> {
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new())?;
    let cn: String = common_name
        .chars()
        .filter(|c| c.is_ascii_graphic())
        .take(64)
        .collect();
    params.distinguished_name.push(
        rcgen::DnType::CommonName,
        if cn.is_empty() { "xbnc".into() } else { cn },
    );
    params.not_before = time::OffsetDateTime::now_utc() - time::Duration::days(1);
    params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(3650);
    let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    let cert = params.self_signed(&key)?;
    Ok((cert.pem(), key.serialize_pem()))
}

/// Parses a PEM certificate chain and private key, checking they are usable
/// for TLS client authentication.
pub fn parse(
    cert_pem: &str,
    key_pem: &str,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    if cert_pem.len() > 64 * 1024 || key_pem.len() > 64 * 1024 {
        bail!("certificate or key too large");
    }
    let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(cert_pem.as_bytes())
        .collect::<std::result::Result<_, _>>()
        .context("the certificate is not valid PEM")?;
    if chain.is_empty() {
        bail!("no certificate found in the PEM text");
    }
    let key = PrivateKeyDer::from_pem_slice(key_pem.as_bytes())
        .context("the private key is not valid PEM (PKCS#8, PKCS#1 or SEC1)")?;
    // Building a config validates the key and that it suits the certificate.
    client_config(
        Arc::new(rustls::RootCertStore::empty()),
        chain.clone(),
        key.clone_key(),
    )?;
    Ok((chain, key))
}

/// TLS client configuration that presents a client certificate.
pub fn client_config(
    roots: Arc<rustls::RootCertStore>,
    chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<Arc<rustls::ClientConfig>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let cfg = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])?
        .with_root_certificates(roots)
        .with_client_auth_cert(chain, key)
        .context("the private key does not match the certificate or is not supported")?;
    Ok(Arc::new(cfg))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Fingerprints {
    pub sha1: String,
    pub sha256: String,
    pub sha512: String,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Fingerprints of the first certificate, as IRC services show them
/// (OFTC uses SHA-1; Libera.Chat and most Atheme/Anope setups SHA-512 or SHA-256).
pub fn fingerprints(cert_pem: &str) -> Result<Fingerprints> {
    let der = CertificateDer::pem_slice_iter(cert_pem.as_bytes())
        .next()
        .context("no certificate")??;
    Ok(Fingerprints {
        sha1: hex(&Sha1::digest(der.as_ref())),
        sha256: hex(&Sha256::digest(der.as_ref())),
        sha512: hex(&Sha512::digest(der.as_ref())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_parse_fingerprint() {
        let (cert, key) = generate("unixfool").unwrap();
        assert!(cert.contains("BEGIN CERTIFICATE"));
        assert!(key.contains("PRIVATE KEY"));
        let (chain, _k) = parse(&cert, &key).unwrap();
        assert_eq!(chain.len(), 1);
        let f = fingerprints(&cert).unwrap();
        assert_eq!(f.sha1.len(), 40);
        assert_eq!(f.sha256.len(), 64);
        assert_eq!(f.sha512.len(), 128);
        // A key from another certificate is rejected.
        let (_, other_key) = generate("x").unwrap();
        assert!(parse(&cert, &other_key).is_err(), "mismatched key accepted");
        assert!(parse("not pem", &key).is_err());
        assert!(parse(&cert, "not pem").is_err());
    }

    #[test]
    fn fingerprint_formats() {
        let hex = "3f9a00112233445566778899aabbccddeeff0123456789abcdef1032547698c2";
        let colons = colon_hex(hex);
        assert!(colons.starts_with("3F:9A:00:") && colons.ends_with(":98:C2"));
        assert_eq!(normalize_sha256(&colons).as_deref(), Some(hex));
        assert_eq!(normalize_sha256(&hex.to_uppercase()).as_deref(), Some(hex));
        assert_eq!(normalize_sha256("3f 9a").as_deref(), None);
        assert_eq!(normalize_sha256(&format!("{}zz", &hex[..62])), None);
    }

    /// A self-signed server certificate is refused unless its SHA-256
    /// fingerprint is trusted; a different fingerprint does not help.
    #[tokio::test]
    async fn pinned_server_certificate() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let cert = rcgen::CertificateParams::new(vec!["irc.self-signed.test".to_string()])
            .unwrap()
            .self_signed(&key)
            .unwrap();
        let der = cert.der().clone();
        let fp = hex(&Sha256::digest(der.as_ref()));
        let server_cfg = Arc::new(
            rustls::ServerConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![der], PrivateKeyDer::Pkcs8(key.serialize_der().into()))
            .unwrap(),
        );
        // Some unrelated trust anchor, as the system roots would be.
        let mut roots = rustls::RootCertStore::empty();
        let (other, _) = generate("other-ca").unwrap();
        roots
            .add(CertificateDer::from_pem_slice(other.as_bytes()).unwrap())
            .unwrap();
        let roots = Arc::new(roots);

        async fn handshake(
            server: Arc<rustls::ServerConfig>,
            client: Arc<rustls::ClientConfig>,
        ) -> bool {
            let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = l.local_addr().unwrap();
            let srv = tokio::spawn(async move {
                let (s, _) = l.accept().await.unwrap();
                if let Ok(mut t) = tokio_rustls::TlsAcceptor::from(server).accept(s).await {
                    let _ = t.write_all(b"ok").await;
                    let _ = t.shutdown().await;
                }
            });
            let s = tokio::net::TcpStream::connect(addr).await.unwrap();
            let name = rustls_pki_types::ServerName::try_from("irc.self-signed.test").unwrap();
            let ok = match tokio_rustls::TlsConnector::from(client)
                .connect(name, s)
                .await
            {
                Ok(mut t) => {
                    let mut b = [0u8; 2];
                    t.read_exact(&mut b).await.is_ok() && &b == b"ok"
                }
                Err(_) => false,
            };
            let _ = srv.await;
            ok
        }

        let none = network_client_config(roots.clone(), None, &[]).unwrap();
        assert!(
            !handshake(server_cfg.clone(), none).await,
            "unknown certificate accepted"
        );
        let wrong = network_client_config(roots.clone(), None, &["00".repeat(32)]).unwrap();
        assert!(
            !handshake(server_cfg.clone(), wrong).await,
            "wrong pin accepted"
        );
        let pinned = network_client_config(roots, None, &[fp]).unwrap();
        assert!(
            handshake(server_cfg, pinned).await,
            "pinned certificate refused"
        );
    }
}

/// Accepts a server whose certificate does not validate (self-signed,
/// expired, wrong name) when its SHA-256 fingerprint was trusted with
/// `*status addtrustedserverfingerprint`. Everything else, including the
/// handshake signatures, is checked as usual.
#[derive(Debug)]
struct PinnedVerifier {
    inner: Arc<rustls::client::WebPkiServerVerifier>,
    pins: Vec<[u8; 32]>,
}

impl rustls::client::danger::ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &rustls_pki_types::ServerName<'_>,
        ocsp: &[u8],
        now: rustls_pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        match self
            .inner
            .verify_server_cert(end_entity, intermediates, server_name, ocsp, now)
        {
            Ok(v) => Ok(v),
            Err(e) => {
                let fp = Sha256::digest(end_entity.as_ref());
                if self.pins.iter().any(|p| p.as_slice() == fp.as_slice()) {
                    Ok(rustls::client::danger::ServerCertVerified::assertion())
                } else {
                    Err(e)
                }
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// Parses a SHA-256 fingerprint written as 64 hex digits, with or without
/// colons or spaces. Returns it as lowercase hex.
pub fn normalize_sha256(fp: &str) -> Option<String> {
    let hex: String = fp
        .chars()
        .filter(|c| !matches!(c, ':' | ' ' | '-'))
        .collect::<String>()
        .to_ascii_lowercase();
    (hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit())).then_some(hex)
}

/// Writes a lowercase hex fingerprint as AA:BB:CC...
pub fn colon_hex(hex: &str) -> String {
    hex.as_bytes()
        .chunks(2)
        .map(|c| String::from_utf8_lossy(c).to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join(":")
}

/// TLS client configuration for a network: the system roots, optionally a
/// client certificate, and optionally trusted server fingerprints.
pub fn network_client_config(
    roots: Arc<rustls::RootCertStore>,
    client_cert: Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)>,
    trusted_fps: &[String],
) -> Result<Arc<rustls::ClientConfig>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])?;
    let pins: Vec<[u8; 32]> = trusted_fps
        .iter()
        .filter_map(|h| {
            let bytes: Vec<u8> = (0..h.len())
                .step_by(2)
                .filter_map(|i| u8::from_str_radix(h.get(i..i + 2)?, 16).ok())
                .collect();
            bytes.try_into().ok()
        })
        .collect();
    let builder = if pins.is_empty() {
        builder.with_root_certificates(roots)
    } else {
        let inner = rustls::client::WebPkiServerVerifier::builder_with_provider(roots, provider)
            .build()
            .context("cannot build the certificate verifier")?;
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PinnedVerifier { inner, pins }))
    };
    let cfg = match client_cert {
        Some((chain, key)) => builder
            .with_client_auth_cert(chain, key)
            .context("the private key does not match the certificate or is not supported")?,
        None => builder.with_no_client_auth(),
    };
    Ok(Arc::new(cfg))
}
