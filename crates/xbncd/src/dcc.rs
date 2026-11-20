//! DCC through the bouncer: file transfers (`DCC SEND`) and direct chats
//! (`DCC CHAT`) are relayed by xBNC, so the other person only ever sees the
//! server's address and transfers work even when the user's device is
//! behind NAT.
//!
//! Each offer gets a one-shot listener: the first connection accepted is
//! joined to the other side and the bytes are copied both ways, up to a
//! size limit. Nothing is stored on disk.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info};

/// `[dcc]` in xbnc.toml.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DccConfig {
    /// Address announced to people on IRC. Default: the address xBNC uses
    /// to talk to the network (right on a server with a public IP).
    pub public_ip: Option<IpAddr>,
    /// Ports for relay listeners, e.g. "50000-50100" (open them in the
    /// firewall). Default: any free port.
    pub ports: Option<String>,
}

impl DccConfig {
    pub fn port_range(&self) -> Result<Option<(u16, u16)>> {
        let Some(p) = &self.ports else {
            return Ok(None);
        };
        let (a, b) = p.split_once('-').unwrap_or((p, p));
        let (a, b): (u16, u16) = (
            a.trim().parse().context("dcc.ports")?,
            b.trim().parse().context("dcc.ports")?,
        );
        if a == 0 || a > b || b - a > 10_000 {
            bail!("dcc.ports must look like 50000-50100 (at most 10000 ports)");
        }
        Ok(Some((a, b)))
    }
}

/// A parsed DCC request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dcc {
    /// SEND, CHAT, RESUME or ACCEPT.
    pub kind: String,
    /// File name as written (with quotes if it had them), or "chat".
    pub arg: String,
    /// SEND/CHAT: peer address and port. RESUME/ACCEPT: only `port`.
    pub ip: Option<IpAddr>,
    pub port: u16,
    /// Everything after the port (size, position, token).
    pub rest: Vec<String>,
}

/// Parses the body of a CTCP message (`\x01DCC ...\x01`).
pub fn parse(text: &str) -> Option<Dcc> {
    let body = text.strip_prefix('\x01')?.trim_end_matches('\x01');
    if body.chars().any(|c| c.is_control()) {
        return None;
    }
    let rest = body.strip_prefix("DCC ")?.trim_start();
    let (kind, rest) = rest.split_once(' ')?;
    let kind = kind.to_ascii_uppercase();
    if !matches!(kind.as_str(), "SEND" | "CHAT" | "RESUME" | "ACCEPT") {
        return None;
    }
    let rest = rest.trim_start();
    let (arg, rest) = if let Some(q) = rest.strip_prefix('"') {
        let end = q.find('"')?;
        (format!("\"{}\"", &q[..end]), q[end + 1..].trim_start())
    } else {
        let (a, r) = rest.split_once(' ')?;
        (a.to_string(), r.trim_start())
    };
    if arg.len() > 260 || arg.contains(['\r', '\n', '\0', '\x01']) {
        return None;
    }
    let mut words = rest.split_whitespace();
    let (ip, port) = if matches!(kind.as_str(), "SEND" | "CHAT") {
        let ip = parse_ip(words.next()?)?;
        (Some(ip), words.next()?.parse().ok()?)
    } else {
        (None, words.next()?.parse().ok()?)
    };
    let rest: Vec<String> = words.map(str::to_string).collect();
    if rest.len() > 3 || rest.iter().any(|w| w.len() > 40) {
        return None;
    }
    Some(Dcc {
        kind,
        arg,
        ip,
        port,
        rest,
    })
}

/// DCC addresses: IPv4 as a decimal number, or a literal IPv6 address.
fn parse_ip(s: &str) -> Option<IpAddr> {
    if let Ok(n) = s.parse::<u32>() {
        return Some(IpAddr::V4(Ipv4Addr::from(n)));
    }
    s.parse().ok()
}

fn format_ip(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => u32::from(v4).to_string(),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => u32::from(v4).to_string(),
            None => v6.to_string(),
        },
    }
}

impl Dcc {
    /// The CTCP text for this request.
    pub fn render(&self) -> String {
        let mut s = format!("\x01DCC {} {}", self.kind, self.arg);
        if let Some(ip) = self.ip {
            s.push(' ');
            s.push_str(&format_ip(ip));
        }
        s.push(' ');
        s.push_str(&self.port.to_string());
        for w in &self.rest {
            s.push(' ');
            s.push_str(w);
        }
        s.push('\x01');
        s
    }

    /// Declared size of a SEND (0 when unknown).
    pub fn size(&self) -> u64 {
        if self.kind == "SEND" {
            self.rest.first().and_then(|s| s.parse().ok()).unwrap_or(0)
        } else {
            0
        }
    }
}

/// True for addresses xBNC will not connect to on behalf of someone on IRC
/// (loopback, private networks, link-local, multicast...).
pub fn is_internal(ip: IpAddr) -> bool {
    crate::netinfo::is_local_network(ip)
        || ip.is_unspecified()
        || ip.is_multicast()
        || matches!(ip, IpAddr::V4(v4) if v4.is_broadcast() || v4.octets()[0] == 0)
}

/// Totals for the metrics page.
#[derive(Default)]
pub struct DccStats {
    pub relays: AtomicU64,
    pub bytes: AtomicU64,
}

#[derive(Debug, Clone, Copy)]
pub struct RelayLimits {
    /// Most bytes copied in each direction (0: no limit).
    pub max_bytes: u64,
    /// How long the listener waits for its one connection.
    pub wait: Duration,
    /// A transfer with no data for this long is closed.
    pub idle: Duration,
}

/// Binds a listener on `ip` (in the configured port range, if any).
pub async fn bind(ip: IpAddr, range: Option<(u16, u16)>) -> Result<TcpListener> {
    match range {
        None => Ok(TcpListener::bind(SocketAddr::new(ip, 0)).await?),
        Some((a, b)) => {
            // Start at a random point so concurrent offers do not collide.
            let span = (b - a) as u32 + 1;
            let start = (crate::db::now_ms() as u32) % span;
            for i in 0..span {
                let port = a + ((start + i) % span) as u16;
                if let Ok(l) = TcpListener::bind(SocketAddr::new(ip, port)).await {
                    return Ok(l);
                }
            }
            bail!("no free port in dcc.ports")
        }
    }
}

/// Waits for one connection on `listener` (only from `allowed`, when set),
/// then connects to `target` and copies bytes both ways.
pub fn spawn_relay(
    listener: TcpListener,
    allowed: Option<IpAddr>,
    target: SocketAddr,
    limits: RelayLimits,
    stats: Arc<DccStats>,
    label: String,
) {
    tokio::spawn(async move {
        let accepted = tokio::time::timeout(limits.wait, async {
            loop {
                let (s, peer) = listener.accept().await?;
                match allowed {
                    Some(a) if !same_ip(a, peer.ip()) => {
                        debug!(%peer, "DCC relay: connection from an unexpected address refused");
                        continue;
                    }
                    _ => return anyhow::Ok((s, peer)),
                }
            }
        })
        .await;
        drop(listener);
        let (inbound, peer) = match accepted {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                debug!(error = %e, "DCC relay: accept failed");
                return;
            }
            Err(_) => {
                debug!(%label, "DCC relay: nobody connected in time");
                return;
            }
        };
        let outbound =
            match tokio::time::timeout(Duration::from_secs(20), TcpStream::connect(target)).await {
                Ok(Ok(s)) => s,
                _ => {
                    info!(%label, %target, "DCC relay: cannot reach the other side");
                    return;
                }
            };
        stats.relays.fetch_add(1, Ordering::Relaxed);
        info!(%label, from = %peer, to = %target, "DCC relay started");
        let (a, b) = pipe(inbound, outbound, limits).await;
        stats.bytes.fetch_add(a + b, Ordering::Relaxed);
        info!(%label, sent = a, received = b, "DCC relay finished");
    });
}

fn same_ip(a: IpAddr, b: IpAddr) -> bool {
    let norm = |ip: IpAddr| match ip {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(v6)),
        v4 => v4,
    };
    norm(a) == norm(b)
}

/// Copies both ways until either side closes, the idle timeout passes or
/// the size limit is reached. Returns the bytes copied each way.
async fn pipe(a: TcpStream, b: TcpStream, limits: RelayLimits) -> (u64, u64) {
    let (ar, aw) = a.into_split();
    let (br, bw) = b.into_split();
    let one = |mut r: tokio::net::tcp::OwnedReadHalf, mut w: tokio::net::tcp::OwnedWriteHalf| async move {
        let mut buf = vec![0u8; 32 * 1024];
        let mut total = 0u64;
        loop {
            let n = match tokio::time::timeout(limits.idle, r.read(&mut buf)).await {
                Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
                Ok(Ok(n)) => n,
            };
            total += n as u64;
            if limits.max_bytes > 0 && total > limits.max_bytes {
                break;
            }
            if w.write_all(&buf[..n]).await.is_err() {
                break;
            }
        }
        let _ = w.shutdown().await;
        total
    };
    let (x, y) = tokio::join!(one(ar, bw), one(br, aw));
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_render() {
        let d = parse("\x01DCC SEND file.txt 3232235777 5000 1024\x01").unwrap();
        assert_eq!(d.kind, "SEND");
        assert_eq!(d.ip, Some("192.168.1.1".parse().unwrap()));
        assert_eq!((d.port, d.size()), (5000, 1024));
        assert_eq!(d.render(), "\x01DCC SEND file.txt 3232235777 5000 1024\x01");
        let d = parse("\x01DCC SEND \"my file.zip\" ::1 6000 10 7\x01").unwrap();
        assert_eq!(d.arg, "\"my file.zip\"");
        assert_eq!(d.rest, vec!["10", "7"]);
        assert_eq!(d.render(), "\x01DCC SEND \"my file.zip\" ::1 6000 10 7\x01");
        let d = parse("\x01DCC CHAT chat 16843009 7000\x01").unwrap();
        assert_eq!(d.ip, Some("1.1.1.1".parse().unwrap()));
        let d = parse("\x01DCC RESUME file.txt 5000 512\x01").unwrap();
        assert_eq!(
            (d.ip, d.port, d.rest.clone()),
            (None, 5000, vec!["512".to_string()])
        );
        for bad in [
            "\x01DCC SEND\x01",
            "\x01DCC GET x 1 2\x01",
            "\x01DCC SEND f notanip 1\x01",
            "\x01DCC SEND f 1 99999\x01",
            "\x01DCC SEND \"unterminated 1 2\x01",
            // Found by fuzzing: a NUL in a trailing word.
            "\x01DCC RESUME f 0000 10\x11\0\0\0\0\0\0\0\x01",
            "hello",
        ] {
            assert!(parse(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn internal_addresses() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "192.168.0.5",
            "169.254.1.1",
            "0.0.0.0",
            "::1",
            "fe80::1",
            "224.0.0.1",
            "0.1.2.3",
        ] {
            assert!(is_internal(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["8.8.8.8", "2001:4860::8888"] {
            assert!(!is_internal(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn ports_config() {
        let c = DccConfig {
            public_ip: None,
            ports: Some("50000-50010".into()),
        };
        assert_eq!(c.port_range().unwrap(), Some((50000, 50010)));
        for bad in ["0-10", "10-5", "a-b", "1-60000"] {
            let c = DccConfig {
                public_ip: None,
                ports: Some(bad.into()),
            };
            assert!(c.port_range().is_err(), "{bad}");
        }
    }

    #[tokio::test]
    async fn relay_copies_both_ways_and_enforces_limits() {
        // The "other side": echoes what it gets, prefixed.
        let far = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let far_addr = far.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = far.accept().await.unwrap();
            let mut buf = [0u8; 5];
            s.read_exact(&mut buf).await.unwrap();
            s.write_all(b"got:").await.unwrap();
            s.write_all(&buf).await.unwrap();
        });
        let stats = Arc::new(DccStats::default());
        let l = bind("127.0.0.1".parse().unwrap(), None).await.unwrap();
        let port = l.local_addr().unwrap().port();
        let limits = RelayLimits {
            max_bytes: 0,
            wait: Duration::from_secs(5),
            idle: Duration::from_secs(5),
        };
        spawn_relay(
            l,
            Some("127.0.0.1".parse().unwrap()),
            far_addr,
            limits,
            stats.clone(),
            "test".into(),
        );
        let mut c = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        c.write_all(b"hello").await.unwrap();
        let mut out = Vec::new();
        c.read_to_end(&mut out).await.unwrap();
        assert_eq!(out, b"got:hello");
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(stats.relays.load(Ordering::Relaxed), 1);

        // Size limit: a stream bigger than allowed is cut.
        let far = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let far_addr = far.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = far.accept().await.unwrap();
            let _ = s.write_all(&vec![7u8; 200_000]).await;
        });
        let l = bind("127.0.0.1".parse().unwrap(), None).await.unwrap();
        let port = l.local_addr().unwrap().port();
        let limits = RelayLimits {
            max_bytes: 50_000,
            ..limits
        };
        spawn_relay(l, None, far_addr, limits, stats, "test".into());
        let mut c = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let mut out = Vec::new();
        let _ = c.read_to_end(&mut out).await;
        assert!(out.len() <= 50_000 + 32 * 1024, "{}", out.len());
        assert!(out.len() < 200_000);
    }
}
