//! A minimal HTTP/1.1 client for the few outgoing requests xBNC makes
//! (alert webhooks, update checks): one request per connection, TLS verified
//! with the system roots, no redirects, bounded response size and time.

use anyhow::{bail, Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub tls: bool,
    pub host: String,
    pub port: u16,
    pub path: String,
}

impl Url {
    /// http:// or https:// URLs without credentials, spaces or control
    /// characters.
    pub fn parse(u: &str) -> Option<Url> {
        let (tls, rest) = if let Some(r) = u.strip_prefix("https://") {
            (true, r)
        } else {
            (false, u.strip_prefix("http://")?)
        };
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        if authority.is_empty()
            || authority.contains('@')
            || u.chars().any(|c| c.is_control() || c == ' ')
        {
            return None;
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) if !h.ends_with(']') || authority.starts_with('[') => match p.parse() {
                Ok(p) => (h, p),
                Err(_) if authority.starts_with('[') && authority.ends_with(']') => {
                    (authority, if tls { 443 } else { 80 })
                }
                Err(_) => return None,
            },
            _ => (authority, if tls { 443 } else { 80 }),
        };
        let host = host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string();
        if host.is_empty() {
            return None;
        }
        Some(Url {
            tls,
            host,
            port,
            path: path.to_string(),
        })
    }
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    /// `Location` header, for redirects.
    pub location: Option<String>,
}

/// Sends one request. `body` is (content type, bytes). The whole exchange
/// must finish within 15 seconds and the body may not exceed `max_body`.
pub async fn request(
    method: &str,
    url: &str,
    body: Option<(&str, &[u8])>,
    tls: Arc<rustls::ClientConfig>,
    max_body: usize,
) -> Result<Response> {
    request_timeout(method, url, body, tls, max_body, Duration::from_secs(15)).await
}

/// The address a redirect points to: an absolute `https://` address, or a
/// path (`/x`) on the same host. Anything else (http://, `//host`, relative
/// paths) gives `None`.
pub fn redirect_target(from: &str, location: &str) -> Option<String> {
    let location = location.trim();
    if location.starts_with("https://") {
        return Url::parse(location).map(|_| location.to_string());
    }
    if location.starts_with('/') && !location.starts_with("//") {
        let u = Url::parse(from)?;
        if !u.tls {
            return None;
        }
        let host = if u.host.contains(':') { format!("[{}]", u.host) } else { u.host };
        let port = if u.port == 443 { String::new() } else { format!(":{}", u.port) };
        let next = format!("https://{host}{port}{location}");
        return Url::parse(&next).map(|_| next);
    }
    None
}

/// GET that follows up to 5 redirects (https:// only) and allows a longer
/// time, for downloading release packages. Errors unless the final answer
/// is 200.
pub async fn download(
    url: &str,
    tls: Arc<rustls::ClientConfig>,
    max_body: usize,
    timeout: Duration,
) -> Result<Vec<u8>> {
    let mut url = url.to_string();
    for _ in 0..6 {
        let r = request_timeout("GET", &url, None, tls.clone(), max_body, timeout).await?;
        match r.status {
            200 => return Ok(r.body),
            301 | 302 | 303 | 307 | 308 => {
                let next = r.location.context("redirect without Location")?;
                if !next.starts_with("https://") {
                    bail!("refusing a redirect to a non-https address");
                }
                url = next;
            }
            s => bail!("the server answered {s}"),
        }
    }
    bail!("too many redirects")
}

async fn request_timeout(
    method: &str,
    url: &str,
    body: Option<(&str, &[u8])>,
    tls: Arc<rustls::ClientConfig>,
    max_body: usize,
    timeout: Duration,
) -> Result<Response> {
    request_opts(method, url, body, tls, max_body, timeout, false, &[]).await
}

/// A request to an address chosen by a user: only `https://` to public
/// addresses (the name is resolved once and the checked address is the one
/// used, so DNS cannot point it elsewhere afterwards).
pub async fn request_public(
    method: &str,
    url: &str,
    body: Option<(&str, &[u8])>,
    tls: Arc<rustls::ClientConfig>,
    max_body: usize,
    timeout: Duration,
    headers: &[(&str, &str)],
) -> Result<Response> {
    if !url.starts_with("https://") {
        bail!("only https:// addresses are allowed");
    }
    request_opts(method, url, body, tls, max_body, timeout, true, headers).await
}

#[allow(clippy::too_many_arguments)]
pub async fn request_opts(
    method: &str,
    url: &str,
    body: Option<(&str, &[u8])>,
    tls: Arc<rustls::ClientConfig>,
    max_body: usize,
    timeout: Duration,
    public_only: bool,
    headers: &[(&str, &str)],
) -> Result<Response> {
    let u = Url::parse(url).context("invalid URL")?;
    let host_header = if (u.tls && u.port == 443) || (!u.tls && u.port == 80) {
        u.host.clone()
    } else if u.host.contains(':') {
        format!("[{}]:{}", u.host, u.port)
    } else {
        format!("{}:{}", u.host, u.port)
    };
    let mut req = format!(
        "{method} {} HTTP/1.1\r\nHost: {host_header}\r\nUser-Agent: xBNC/{} (+https://xbnc.org)\r\nAccept: application/json, */*;q=0.5\r\nConnection: close\r\n",
        u.path,
        crate::network::VERSION
    )
    .into_bytes();
    for (k, v) in headers {
        if k.contains([':', '\r', '\n']) || v.contains(['\r', '\n']) {
            bail!("invalid header");
        }
        req.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
    }
    if let Some((ct, b)) = body {
        req.extend_from_slice(
            format!("Content-Type: {ct}\r\nContent-Length: {}\r\n", b.len()).as_bytes(),
        );
        req.extend_from_slice(b"\r\n");
        req.extend_from_slice(b);
    } else {
        req.extend_from_slice(b"\r\n");
    }
    let fut = async {
        let tcp = if public_only {
            let addrs: Vec<std::net::SocketAddr> =
                tokio::net::lookup_host((u.host.as_str(), u.port))
                    .await?
                    .collect();
            let Some(addr) = addrs.first().copied() else {
                bail!("cannot resolve {}", u.host);
            };
            if addrs.iter().any(|a| crate::dcc::is_internal(a.ip())) {
                bail!("{} is a private or local address", u.host);
            }
            tokio::net::TcpStream::connect(addr).await?
        } else {
            tokio::net::TcpStream::connect((u.host.as_str(), u.port)).await?
        };
        if u.tls {
            let name = rustls_pki_types::ServerName::try_from(u.host.clone())?;
            let s = tokio_rustls::TlsConnector::from(tls)
                .connect(name, tcp)
                .await?;
            exchange(s, &req, max_body).await
        } else {
            exchange(tcp, &req, max_body).await
        }
    };
    tokio::time::timeout(timeout, fut)
        .await
        .context("the request timed out")?
}

async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(
    mut s: S,
    req: &[u8],
    max_body: usize,
) -> Result<Response> {
    s.write_all(req).await?;
    let mut raw = Vec::new();
    // Headers are small; allow 16 KB for them on top of the body.
    let _ = (&mut s)
        .take((max_body + 16 * 1024) as u64 + 1)
        .read_to_end(&mut raw)
        .await;
    parse_response(&raw, max_body)
}

/// Splits a raw HTTP/1.1 response into status and (de-chunked) body.
pub fn parse_response(raw: &[u8], max_body: usize) -> Result<Response> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .context("incomplete HTTP response")?;
    let head = std::str::from_utf8(&raw[..split]).context("invalid HTTP headers")?;
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .context("invalid HTTP status line")?;
    let mut chunked = false;
    let mut length: Option<usize> = None;
    let mut location = None;
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
            if k == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked") {
                chunked = true;
            } else if k == "content-length" {
                length = v.parse().ok();
            } else if k == "location" {
                location = Some(v.to_string());
            }
        }
    }
    let rest = &raw[split + 4..];
    let body = if chunked {
        dechunk(rest, max_body)?
    } else {
        let n = length.unwrap_or(rest.len()).min(rest.len());
        rest[..n].to_vec()
    };
    if body.len() > max_body {
        bail!("the response is larger than {max_body} bytes");
    }
    Ok(Response {
        status,
        body,
        location,
    })
}

fn dechunk(mut data: &[u8], max: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let eol = data
            .windows(2)
            .position(|w| w == b"\r\n")
            .context("truncated chunked body")?;
        let size_str = std::str::from_utf8(&data[..eol])?
            .split(';')
            .next()
            .unwrap_or("")
            .trim();
        let size = usize::from_str_radix(size_str, 16).context("invalid chunk size")?;
        data = &data[eol + 2..];
        if size == 0 {
            return Ok(out);
        }
        if size > data.len() || out.len() + size > max {
            bail!("truncated or oversized chunked body");
        }
        out.extend_from_slice(&data[..size]);
        data = data.get(size + 2..).context("truncated chunk")?;
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn redirect_targets() {
        let from = "https://en.wikipedia.org/api/rest_v1/page/summary/USA";
        assert_eq!(
            redirect_target(from, "/api/rest_v1/page/summary/United_States").as_deref(),
            Some("https://en.wikipedia.org/api/rest_v1/page/summary/United_States")
        );
        assert_eq!(
            redirect_target(from, "https://example.org/x").as_deref(),
            Some("https://example.org/x")
        );
        for bad in ["http://example.org/x", "//evil.example/x", "United_States", "", "https://a b"] {
            assert_eq!(redirect_target(from, bad), None, "{bad}");
        }
        assert_eq!(redirect_target("http://h/x", "/y"), None);
    }

    use super::*;

    #[test]
    fn urls() {
        let u = Url::parse("https://hooks.example.org/a/b?x=1").unwrap();
        assert!(u.tls && u.host == "hooks.example.org" && u.port == 443 && u.path == "/a/b?x=1");
        let u = Url::parse("http://10.0.0.5:8080").unwrap();
        assert!(!u.tls && u.port == 8080 && u.path == "/");
        assert_eq!(
            Url::parse("http://[fd00::1]:9000/x").unwrap().host,
            "fd00::1"
        );
        for bad in [
            "ftp://x",
            "http://user:pw@x/",
            "http://x/\r\nHost: y",
            "http://:80/",
            "javascript:alert(1)",
        ] {
            assert!(Url::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn responses() {
        let r =
            parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhiEXTRA", 100).unwrap();
        assert_eq!((r.status, r.body.as_slice()), (200, &b"hi"[..]));
        let r = parse_response(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWiki\r\n5;x=y\r\npedia\r\n0\r\n\r\n",
            100,
        )
        .unwrap();
        assert_eq!(r.body, b"Wikipedia");
        assert!(parse_response(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffff\r\nab",
            100
        )
        .is_err());
        assert!(parse_response(b"garbage", 100).is_err());
        assert!(parse_response(b"HTTP/1.1 200 OK\r\n\r\n0123456789", 5).is_err());
    }
}
