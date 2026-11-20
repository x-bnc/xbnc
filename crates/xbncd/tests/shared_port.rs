//! `[web] shared = true`: the panel and IRC on one TLS port. Browsers get
//! the panel, IRC clients the bouncer, and plain http:// is sent to https://.

use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Accepts the self-signed certificate of the test server.
#[derive(Debug)]
struct AnyCert;

impl rustls::client::danger::ServerCertVerifier for AnyCert {
    fn verify_server_cert(
        &self,
        _: &rustls_pki_types::CertificateDer<'_>,
        _: &[rustls_pki_types::CertificateDer<'_>],
        _: &rustls_pki_types::ServerName<'_>,
        _: &[u8],
        _: rustls_pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &rustls_pki_types::CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &rustls_pki_types::CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

async fn tls(addr: std::net::SocketAddr) -> tokio_rustls::client::TlsStream<TcpStream> {
    let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .dangerous()
    .with_custom_certificate_verifier(Arc::new(AnyCert))
    .with_no_client_auth();
    let tcp = TcpStream::connect(addr).await.unwrap();
    tokio_rustls::TlsConnector::from(Arc::new(cfg))
        .connect("localhost".try_into().unwrap(), tcp)
        .await
        .unwrap()
}

async fn read_some<S: tokio::io::AsyncRead + Unpin>(s: &mut S, until: &str) -> String {
    let mut out = Vec::new();
    let mut b = [0u8; 4096];
    let _ = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match s.read(&mut b).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    out.extend_from_slice(&b[..n]);
                    if String::from_utf8_lossy(&out).contains(until) {
                        break;
                    }
                }
            }
        }
    })
    .await;
    String::from_utf8_lossy(&out).into_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn panel_and_irc_share_one_tls_port() {
    let dir = tempfile::tempdir().unwrap();
    let (cert, key) = xbncd::certs::generate("localhost").unwrap();
    std::fs::write(dir.path().join("cert.pem"), cert).unwrap();
    std::fs::write(dir.path().join("key.pem"), key).unwrap();
    let cfg_path = dir.path().join("xbnc.toml");
    std::fs::write(
        &cfg_path,
        format!(
            r#"
            secret_key_file = "{d}/secret.key"
            [database]
            path = "{d}/xbnc.db"
            [[listen]]
            address = "127.0.0.1:0"
            tls_cert = "{d}/cert.pem"
            tls_key = "{d}/key.pem"
            [web]
            address = "127.0.0.1:0"
            shared = true
            "#,
            d = dir.path().display()
        ),
    )
    .unwrap();
    let cfg = xbncd::Config::load(&cfg_path).unwrap();
    let run = xbncd::start(cfg, true).await.unwrap();
    let addr = run.local_addrs[0];
    assert_eq!(run.web_addr, Some(addr), "the panel is on the IRC port");

    // A browser gets the panel.
    let mut b = tls(addr).await;
    b.write_all(b"GET /api/v1/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let resp = read_some(&mut b, "\r\n\r\n").await;
    assert!(resp.starts_with("HTTP/1.1 200"), "{resp:?}");
    assert!(resp.to_ascii_lowercase().contains("strict-transport-security"), "{resp:?}");

    // An IRC client gets the bouncer on the same port.
    let mut c = tls(addr).await;
    c.write_all(b"NICK alice\r\nUSER alice 0 * :Alice\r\n").await.unwrap();
    let irc = read_some(&mut c, "\r\n").await;
    assert!(irc.starts_with(':') || irc.starts_with("ERROR") || irc.starts_with("NOTICE"), "{irc:?}");
    assert!(!irc.contains("HTTP/1.1"), "{irc:?}");

    // Plain http:// on the TLS port is sent to https://.
    let mut p = TcpStream::connect(addr).await.unwrap();
    p.write_all(b"GET /login HTTP/1.1\r\nHost: localhost:7000\r\n\r\n").await.unwrap();
    let r = read_some(&mut p, "\r\n\r\n").await;
    assert!(r.starts_with("HTTP/1.1 301"), "{r:?}");
    assert!(r.contains("Location: https://localhost:7000/login"), "{r:?}");

    // An IRC client with SSL off is told to turn it on.
    let mut q = TcpStream::connect(addr).await.unwrap();
    q.write_all(b"NICK alice\r\nUSER alice 0 * :Alice\r\n").await.unwrap();
    let r = read_some(&mut q, "\r\n").await;
    assert!(r.starts_with("ERROR") && r.contains("SSL"), "{r:?}");
}

#[test]
fn shared_needs_a_tls_listener_on_that_address() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("xbnc.toml");
    let base = format!(
        "secret_key_file = \"{d}/s.key\"\n[database]\npath = \"{d}/x.db\"\n",
        d = dir.path().display()
    );
    std::fs::write(
        &p,
        format!("{base}[[listen]]\naddress = \"127.0.0.1:6697\"\n[web]\naddress = \"127.0.0.1:7000\"\nshared = true\n"),
    )
    .unwrap();
    let e = xbncd::Config::load(&p).unwrap_err().to_string();
    assert!(e.contains("must be one of the [[listen]] addresses"), "{e}");
    std::fs::write(
        &p,
        format!("{base}[[listen]]\naddress = \"127.0.0.1:7000\"\n[web]\naddress = \"127.0.0.1:7000\"\nshared = true\n"),
    )
    .unwrap();
    let e = xbncd::Config::load(&p).unwrap_err().to_string();
    assert!(e.contains("must use TLS"), "{e}");
}
