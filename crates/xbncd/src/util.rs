//! Line codec and time helpers shared by upstream and downstream connections.

use bytes::{Buf, BufMut, BytesMut};
use std::io;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::OffsetDateTime;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Decoder, Encoder};

/// Any bidirectional byte stream (plain TCP or TLS).
pub trait AsyncStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> AsyncStream for T {}

pub type BoxStream = Box<dyn AsyncStream>;

/// A stream whose first bytes were already read (to tell a browser from an
/// IRC client on a shared port): they are given back first.
pub struct Prefixed {
    head: Vec<u8>,
    pos: usize,
    inner: BoxStream,
}

impl Prefixed {
    pub fn new(head: Vec<u8>, inner: BoxStream) -> Prefixed {
        Prefixed { head, pos: 0, inner }
    }
}

impl AsyncRead for Prefixed {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        if self.pos < self.head.len() {
            let n = (self.head.len() - self.pos).min(buf.remaining());
            let start = self.pos;
            buf.put_slice(&self.head[start..start + n]);
            self.pos += n;
            return std::task::Poll::Ready(Ok(()));
        }
        std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for Prefixed {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        std::pin::Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// HTTP request methods (and the HTTP/2 preface) a browser starts with.
/// No IRC command starts with one of these words followed by a space.
const HTTP_STARTS: &[&[u8]] = &[
    b"GET ", b"POST ", b"HEAD ", b"PUT ", b"DELETE ", b"OPTIONS ", b"PATCH ", b"CONNECT ",
    b"TRACE ", b"PRI * HTTP/2",
];

/// Whether the first bytes of a connection are an HTTP request; `None`
/// while there are too few bytes to tell.
pub fn sniff_http(head: &[u8]) -> Option<bool> {
    let mut undecided = false;
    for m in HTTP_STARTS {
        let n = head.len().min(m.len());
        if head[..n] == m[..n] {
            if head.len() >= m.len() {
                return Some(true);
            }
            undecided = true;
        }
    }
    if undecided {
        None
    } else {
        Some(false)
    }
}

/// Reads the first bytes of a connection until it is clear whether it is a
/// browser (true) or an IRC client (false). The bytes read are returned to
/// be given back with `Prefixed`.
pub async fn read_protocol<S: AsyncRead + Unpin>(s: &mut S) -> io::Result<(bool, Vec<u8>)> {
    use tokio::io::AsyncReadExt;
    let mut head = Vec::with_capacity(16);
    let mut b = [0u8; 16];
    loop {
        if let Some(http) = sniff_http(&head) {
            return Ok((http, head));
        }
        let n = s.read(&mut b[..16 - head.len().min(15)]).await?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "closed"));
        }
        head.extend_from_slice(&b[..n]);
    }
}

/// Splits a byte stream into IRC lines.
///
/// * Lines end with LF; a preceding CR is removed.
/// * Invalid UTF-8 is decoded as Windows-1252 (or the encoding chosen in the
///   charset module): many IRC channels still carry Latin-1.
/// * Lines longer than [`xbnc_proto::MAX_LINE_LEN`] terminate the connection,
///   which bounds per-connection memory.
/// * The encoder truncates at any embedded CR/LF so one logical line can never
///   inject a second command.
#[derive(Debug, Default)]
pub struct IrcCodec {
    scanned: usize,
    /// Encoding of lines that are not valid UTF-8.
    pub legacy: Legacy,
}

impl IrcCodec {
    pub fn with_legacy(legacy: Legacy) -> IrcCodec {
        IrcCodec { scanned: 0, legacy }
    }
}

/// 8-bit encodings still used on some IRC channels. Lines that are not
/// valid UTF-8 are decoded with one of these (Windows-1252 by default, a
/// superset of Latin-1 for text), so accents and `ñ` are not lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Legacy {
    #[default]
    Cp1252,
    Latin9,
    Latin1,
}

/// First character of a line that must be sent in a legacy encoding (see
/// [`Legacy::mark`]). Private-use code points never start a real IRC line.
const LEGACY_MARKS: [char; 3] = ['\u{E000}', '\u{E001}', '\u{E002}'];

const CP1252_HIGH: [u32; 32] = [
    0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0x8D, 0x017D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
];
const LATIN9_DIFF: [(u8, u32); 8] = [
    (0xA4, 0x20AC),
    (0xA6, 0x0160),
    (0xA8, 0x0161),
    (0xB4, 0x017D),
    (0xB8, 0x017E),
    (0xBC, 0x0152),
    (0xBD, 0x0153),
    (0xBE, 0x0178),
];

impl Legacy {
    pub fn parse(s: &str) -> Legacy {
        match s {
            "iso-8859-15" => Legacy::Latin9,
            "iso-8859-1" => Legacy::Latin1,
            _ => Legacy::Cp1252,
        }
    }

    fn char_of(self, b: u8) -> char {
        let cp = match self {
            Legacy::Cp1252 if (0x80..0xA0).contains(&b) => CP1252_HIGH[(b - 0x80) as usize],
            Legacy::Latin9 => LATIN9_DIFF
                .iter()
                .find(|(x, _)| *x == b)
                .map_or(b as u32, |(_, c)| *c),
            _ => b as u32,
        };
        char::from_u32(cp).unwrap_or('?')
    }

    pub fn decode(self, bytes: &[u8]) -> String {
        bytes.iter().map(|b| self.char_of(*b)).collect()
    }

    pub fn encode(self, text: &str) -> Vec<u8> {
        text.chars()
            .map(|c| {
                if (c as u32) < 0x80 {
                    return c as u8;
                }
                (0x80u16..=0xFF)
                    .map(|b| b as u8)
                    .find(|b| self.char_of(*b) == c)
                    .unwrap_or(b'?')
            })
            .collect()
    }

    /// Marks an outgoing line so the writer sends it in this encoding.
    pub fn mark(self, line: &str) -> String {
        let m = LEGACY_MARKS[self as usize];
        format!("{m}{line}")
    }

    fn from_mark(c: char) -> Option<Legacy> {
        match LEGACY_MARKS.iter().position(|m| *m == c)? {
            0 => Some(Legacy::Cp1252),
            1 => Some(Legacy::Latin9),
            _ => Some(Legacy::Latin1),
        }
    }
}

impl Decoder for IrcCodec {
    type Item = String;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<String>, io::Error> {
        loop {
            let pos = buf[self.scanned..].iter().position(|b| *b == b'\n');
            match pos {
                Some(p) => {
                    let end = self.scanned + p;
                    let mut line = buf.split_to(end + 1);
                    self.scanned = 0;
                    line.truncate(end);
                    if line.last() == Some(&b'\r') {
                        line.truncate(line.len() - 1);
                    }
                    if line.iter().all(|b| *b == b' ' || *b == b'\r') {
                        continue;
                    }
                    if line.len() > xbnc_proto::MAX_LINE_LEN {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long"));
                    }
                    return Ok(Some(match std::str::from_utf8(&line) {
                        Ok(t) => t.to_string(),
                        Err(_) => self.legacy.decode(&line),
                    }));
                }
                None => {
                    if buf.len() > xbnc_proto::MAX_LINE_LEN + 2 {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long"));
                    }
                    self.scanned = buf.len();
                    return Ok(None);
                }
            }
        }
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<String>, io::Error> {
        match self.decode(buf)? {
            Some(l) => Ok(Some(l)),
            None => {
                buf.advance(buf.len());
                self.scanned = 0;
                Ok(None)
            }
        }
    }
}

impl Encoder<String> for IrcCodec {
    type Error = io::Error;

    fn encode(&mut self, line: String, dst: &mut BytesMut) -> Result<(), io::Error> {
        let clean = match line.find(['\r', '\n', '\0']) {
            Some(i) => &line[..i],
            None => &line[..],
        };
        let mut chars = clean.chars();
        match chars.next().and_then(Legacy::from_mark) {
            Some(enc) => {
                let bytes = enc.encode(chars.as_str());
                dst.reserve(bytes.len() + 2);
                dst.put_slice(&bytes);
            }
            None => {
                dst.reserve(clean.len() + 2);
                dst.put_slice(clean.as_bytes());
            }
        }
        dst.put_slice(b"\r\n");
        Ok(())
    }
}

/// Formats milliseconds since the epoch as an IRCv3 server-time value
/// (`YYYY-MM-DDThh:mm:ss.sssZ`).
pub fn format_server_time(ms: i64) -> String {
    let dt = OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH);
    dt.format(format_description!(
        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
    ))
    .unwrap_or_default()
}

/// Short human timestamp used to prefix replayed lines for clients without
/// server-time support.
pub fn format_short_time(ms: i64) -> String {
    let dt = OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH);
    dt.format(format_description!(
        "[year]-[month]-[day] [hour]:[minute]:[second]"
    ))
    .unwrap_or_default()
}

/// Parses an RFC 3339 timestamp into milliseconds since the epoch.
pub fn parse_server_time(s: &str) -> Option<i64> {
    OffsetDateTime::parse(s, &Rfc3339)
        .ok()
        .map(|dt| (dt.unix_timestamp_nanos() / 1_000_000) as i64)
}

/// Makes glibc return large blocks (Argon2's 19 MiB, big history pages) to
/// the system as soon as they are freed. By default glibc raises its mmap
/// threshold after the first free and then keeps such blocks in per-thread
/// arenas: in the load test, 200 users logging in together left the process
/// at 730 MB instead of 30 MB. Call once at startup.
pub fn tune_allocator() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: mallopt only changes allocator tuning; called before the
    // runtime starts allocating from several threads.
    unsafe {
        libc::mallopt(libc::M_MMAP_THRESHOLD, 1024 * 1024);
    }
}

/// "+02:00", "-0530", "+2" → minutes east of UTC.
pub fn parse_utc_offset(s: &str) -> anyhow::Result<i32> {
    let s = s.trim();
    let (sign, rest) = match s.as_bytes().first() {
        Some(b'-') => (-1, &s[1..]),
        Some(b'+') => (1, &s[1..]),
        _ => (1, s),
    };
    let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
    let (h, m) = match (rest.contains(':'), digits.len()) {
        (true, _) => {
            let (h, m) = rest.split_once(':').unwrap_or((rest, "0"));
            (h.parse::<i32>().ok(), m.parse::<i32>().ok())
        }
        (false, 1 | 2) => (digits.parse().ok(), Some(0)),
        (false, 4) => (digits[..2].parse().ok(), digits[2..].parse().ok()),
        _ => (None, None),
    };
    match (h, m) {
        (Some(h), Some(m)) if h <= 14 && m < 60 => Ok(sign * (h * 60 + m)),
        _ => anyhow::bail!("invalid UTC offset {s:?}; use a form like +02:00 or -05:00"),
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn browsers_and_irc_clients_are_told_apart() {
        for http in [&b"GET / HTTP/1.1"[..], b"POST /api", b"HEAD /", b"OPTIONS *", b"PRI * HTTP/2.0"] {
            assert_eq!(sniff_http(http), Some(true), "{:?}", String::from_utf8_lossy(http));
        }
        for irc in [&b"CAP LS 302"[..], b"NICK y2k", b"PASS x", b"USER a", b"PING :x", b"GETX", b"POSTMAN"] {
            assert_eq!(sniff_http(irc), Some(false), "{:?}", String::from_utf8_lossy(irc));
        }
        assert_eq!(sniff_http(b""), None);
        assert_eq!(sniff_http(b"GE"), None);
        assert_eq!(sniff_http(b"PO"), None);
    }

    #[tokio::test]
    async fn read_protocol_gives_the_bytes_back() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut a, b) = tokio::io::duplex(64);
        a.write_all(b"CAP LS 302\r\nNICK y2k\r\n").await.unwrap();
        let mut b: BoxStream = Box::new(b);
        let (http, head) = read_protocol(&mut b).await.unwrap();
        assert!(!http);
        let mut p = Prefixed::new(head, b);
        let mut got = vec![0u8; 22];
        p.read_exact(&mut got).await.unwrap();
        assert_eq!(&got, b"CAP LS 302\r\nNICK y2k\r\n");
    }


    #[test]
    fn legacy_encodings() {
        let latin1 = b"caf\xe9 a\xf1o \x80";
        assert_eq!(Legacy::Cp1252.decode(latin1), "café año €");
        assert_eq!(Legacy::Latin1.decode(b"\xe9"), "é");
        assert_eq!(Legacy::Latin9.decode(b"\xa4"), "€");
        assert_eq!(Legacy::Cp1252.encode("ñandú € ✓"), b"\xf1and\xfa \x80 ?");
        let mut c = IrcCodec::default();
        let mut buf = BytesMut::from(&b"PRIVMSG #c :ni\xf1o\r\n"[..]);
        assert_eq!(c.decode(&mut buf).unwrap().unwrap(), "PRIVMSG #c :niño");
        let mut out = BytesMut::new();
        c.encode(Legacy::Latin1.mark("PRIVMSG #c :niño"), &mut out)
            .unwrap();
        assert_eq!(&out[..], b"PRIVMSG #c :ni\xf1o\r\n");
        let mut out = BytesMut::new();
        c.encode("PRIVMSG #c :niño".into(), &mut out).unwrap();
        assert_eq!(&out[..], "PRIVMSG #c :niño\r\n".as_bytes());
    }

    use super::*;

    #[test]
    fn decodes_lines() {
        let mut c = IrcCodec::default();
        let mut b = BytesMut::from(&b"PING :a\r\n\r\nPRIVMSG #x :y\npartial"[..]);
        assert_eq!(c.decode(&mut b).unwrap().as_deref(), Some("PING :a"));
        assert_eq!(c.decode(&mut b).unwrap().as_deref(), Some("PRIVMSG #x :y"));
        assert_eq!(c.decode(&mut b).unwrap(), None);
        b.extend_from_slice(b" line\r\n");
        assert_eq!(c.decode(&mut b).unwrap().as_deref(), Some("partial line"));
    }

    #[test]
    fn lossy_utf8() {
        let mut c = IrcCodec::default();
        let mut b = BytesMut::from(&b"PRIVMSG #x :caf\xe9\r\n"[..]);
        let l = c.decode(&mut b).unwrap().unwrap();
        assert!(l.starts_with("PRIVMSG #x :caf"));
    }

    #[test]
    fn rejects_overlong() {
        let mut c = IrcCodec::default();
        let mut b = BytesMut::from(vec![b'A'; xbnc_proto::MAX_LINE_LEN + 10].as_slice());
        assert!(c.decode(&mut b).is_err());
    }

    #[test]
    fn encoder_blocks_injection() {
        let mut c = IrcCodec::default();
        let mut out = BytesMut::new();
        c.encode("PRIVMSG #x :hi\r\nQUIT :bye".into(), &mut out)
            .unwrap();
        assert_eq!(&out[..], b"PRIVMSG #x :hi\r\n");
    }

    #[test]
    fn server_time_roundtrip() {
        let s = format_server_time(1_700_000_000_123);
        assert_eq!(s, "2023-11-14T22:13:20.123Z");
        assert_eq!(parse_server_time(&s), Some(1_700_000_000_123));
        assert_eq!(
            parse_server_time("2023-11-14T22:13:20Z"),
            Some(1_700_000_000_000)
        );
        assert_eq!(parse_server_time("nonsense"), None);
    }
}
