//! IRC protocol primitives for xBNC.
//!
//! This crate is deliberately free of I/O and async runtimes. It provides:
//!
//! * [`Message`]: parsing and serialization of IRC lines, including IRCv3
//!   message tags with correct escaping.
//! * [`CaseMapping`]: nickname/channel case folding (`ascii`, `rfc1459`,
//!   `rfc1459-strict`).
//! * [`Isupport`]: helpers to interpret `RPL_ISUPPORT` (005) tokens.
//! * Small helpers for prefixes (`nick!user@host`) and CTCP.

use std::fmt;

/// Maximum accepted length of a raw line in bytes (tags + message, as allowed
/// by IRCv3 message-tags: 8191 bytes of tags plus 512 bytes of message).
pub const MAX_LINE_LEN: usize = 8191 + 512;

/// Errors produced while parsing an IRC line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The line contained no command.
    Empty,
    /// Tags or prefix were present but no command followed.
    MissingCommand,
    /// The line exceeded [`MAX_LINE_LEN`].
    TooLong,
    /// The line contained CR, LF or NUL inside it (they can never be part
    /// of a message, and would let one line smuggle another).
    InvalidCharacter,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "empty line"),
            ParseError::MissingCommand => write!(f, "missing command"),
            ParseError::TooLong => write!(f, "line too long"),
            ParseError::InvalidCharacter => write!(f, "CR, LF or NUL inside the line"),
        }
    }
}

impl std::error::Error for ParseError {}

/// A single IRC message.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Message {
    /// IRCv3 tags in the order they appeared. A `None` value means the tag was
    /// present without a value.
    pub tags: Vec<(String, Option<String>)>,
    /// Source prefix without the leading colon.
    pub prefix: Option<String>,
    /// Command, upper-cased (numerics are kept as-is).
    pub command: String,
    /// Parameters; the trailing parameter is stored without its colon.
    pub params: Vec<String>,
}

impl Message {
    /// Builds a message with the given command and parameters.
    pub fn new<C: Into<String>>(command: C, params: Vec<String>) -> Self {
        Message {
            tags: Vec::new(),
            prefix: None,
            command: command.into(),
            params,
        }
    }

    /// Convenience builder from string slices.
    pub fn cmd(command: &str, params: &[&str]) -> Self {
        Message::new(command, params.iter().map(|s| s.to_string()).collect())
    }

    /// Sets the prefix and returns the message.
    pub fn with_prefix<P: Into<String>>(mut self, prefix: P) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Parses a raw IRC line. Trailing CR/LF are ignored.
    pub fn parse(line: &str) -> Result<Message, ParseError> {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.len() > MAX_LINE_LEN {
            return Err(ParseError::TooLong);
        }
        if line.contains(['\r', '\n', '\0']) {
            return Err(ParseError::InvalidCharacter);
        }
        let mut rest = line.trim_start_matches(' ');
        if rest.is_empty() {
            return Err(ParseError::Empty);
        }

        let mut tags = Vec::new();
        if let Some(stripped) = rest.strip_prefix('@') {
            let (raw_tags, after) = stripped.split_once(' ').ok_or(ParseError::MissingCommand)?;
            for item in raw_tags.split(';').filter(|s| !s.is_empty()) {
                match item.split_once('=') {
                    Some((k, v)) => tags.push((k.to_string(), Some(unescape_tag_value(v)))),
                    None => tags.push((item.to_string(), None)),
                }
            }
            rest = after.trim_start_matches(' ');
        }

        let mut prefix = None;
        if let Some(stripped) = rest.strip_prefix(':') {
            let (p, after) = stripped.split_once(' ').ok_or(ParseError::MissingCommand)?;
            prefix = Some(p.to_string());
            rest = after.trim_start_matches(' ');
        }

        let (command, mut rest) = match rest.split_once(' ') {
            Some((c, r)) => (c, r),
            None => (rest, ""),
        };
        // Commands are words or three-digit numerics; anything else (found
        // by fuzzing: "@ @") would not survive serialising the message.
        if command.is_empty() || !command.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(ParseError::MissingCommand);
        }

        let mut params = Vec::new();
        loop {
            rest = rest.trim_start_matches(' ');
            if rest.is_empty() {
                break;
            }
            if let Some(trailing) = rest.strip_prefix(':') {
                params.push(trailing.to_string());
                break;
            }
            match rest.split_once(' ') {
                Some((p, r)) => {
                    params.push(p.to_string());
                    rest = r;
                }
                None => {
                    params.push(rest.to_string());
                    break;
                }
            }
        }

        Ok(Message {
            tags,
            prefix,
            command: command.to_ascii_uppercase(),
            params,
        })
    }

    /// Returns the parameter at `index`, if any.
    pub fn param(&self, index: usize) -> Option<&str> {
        self.params.get(index).map(String::as_str)
    }

    /// Returns the value of a tag. A tag present without value yields `Some("")`.
    pub fn tag(&self, key: &str) -> Option<&str> {
        self.tags
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_deref().unwrap_or(""))
    }

    /// Sets (or replaces) a tag.
    pub fn set_tag(&mut self, key: &str, value: Option<String>) {
        if let Some(slot) = self.tags.iter_mut().find(|(k, _)| k == key) {
            slot.1 = value;
        } else {
            self.tags.push((key.to_string(), value));
        }
    }

    /// Removes a tag if present.
    pub fn remove_tag(&mut self, key: &str) {
        self.tags.retain(|(k, _)| k != key);
    }

    /// The nickname part of the prefix (`nick` in `nick!user@host`).
    pub fn source_nick(&self) -> Option<&str> {
        self.prefix.as_deref().map(|p| Prefix::parse(p).nick)
    }

    /// True if the prefix looks like a user (`nick!user@host`) rather than a server.
    pub fn source_is_user(&self) -> bool {
        self.prefix
            .as_deref()
            .map(|p| p.contains('!') || p.contains('@'))
            .unwrap_or(false)
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.tags.is_empty() {
            f.write_str("@")?;
            for (i, (k, v)) in self.tags.iter().enumerate() {
                if i > 0 {
                    f.write_str(";")?;
                }
                f.write_str(k)?;
                if let Some(v) = v {
                    if !v.is_empty() {
                        f.write_str("=")?;
                        f.write_str(&escape_tag_value(v))?;
                    }
                }
            }
            f.write_str(" ")?;
        }
        if let Some(p) = &self.prefix {
            write!(f, ":{} ", p)?;
        }
        f.write_str(&self.command)?;
        let n = self.params.len();
        for (i, p) in self.params.iter().enumerate() {
            let last = i + 1 == n;
            if last && (p.is_empty() || p.contains(' ') || p.starts_with(':')) {
                write!(f, " :{}", p)?;
            } else {
                write!(f, " {}", p)?;
            }
        }
        Ok(())
    }
}

/// Escapes a tag value according to IRCv3 message-tags.
pub fn escape_tag_value(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for c in v.chars() {
        match c {
            ';' => out.push_str("\\:"),
            ' ' => out.push_str("\\s"),
            '\\' => out.push_str("\\\\"),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out
}

/// Unescapes a tag value according to IRCv3 message-tags.
pub fn unescape_tag_value(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some(':') => out.push(';'),
            Some('s') => out.push(' '),
            Some('\\') => out.push('\\'),
            Some('r') => out.push('\r'),
            Some('n') => out.push('\n'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// A parsed `nick!user@host` prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefix<'a> {
    pub nick: &'a str,
    pub user: Option<&'a str>,
    pub host: Option<&'a str>,
}

impl<'a> Prefix<'a> {
    /// Parses a prefix. Server names are returned as `nick`.
    pub fn parse(s: &'a str) -> Prefix<'a> {
        let (nick_user, host) = match s.split_once('@') {
            Some((a, b)) => (a, Some(b)),
            None => (s, None),
        };
        let (nick, user) = match nick_user.split_once('!') {
            Some((a, b)) => (a, Some(b)),
            None => (nick_user, None),
        };
        Prefix { nick, user, host }
    }
}

/// Case mapping used to compare nicknames and channel names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaseMapping {
    Ascii,
    #[default]
    Rfc1459,
    Rfc1459Strict,
}

impl CaseMapping {
    /// Interprets an ISUPPORT `CASEMAPPING` value. Unknown values fall back to
    /// `rfc1459`, the historical default.
    pub fn from_isupport(value: &str) -> CaseMapping {
        match value.to_ascii_lowercase().as_str() {
            "ascii" => CaseMapping::Ascii,
            "rfc1459-strict" | "strict-rfc1459" => CaseMapping::Rfc1459Strict,
            _ => CaseMapping::Rfc1459,
        }
    }

    /// Folds a name to its canonical lower-case form.
    pub fn fold(&self, s: &str) -> String {
        s.chars()
            .map(|c| match (self, c) {
                (_, 'A'..='Z') => c.to_ascii_lowercase(),
                (CaseMapping::Rfc1459 | CaseMapping::Rfc1459Strict, '[') => '{',
                (CaseMapping::Rfc1459 | CaseMapping::Rfc1459Strict, ']') => '}',
                (CaseMapping::Rfc1459 | CaseMapping::Rfc1459Strict, '\\') => '|',
                (CaseMapping::Rfc1459, '~') => '^',
                _ => c,
            })
            .collect()
    }

    /// Case-insensitive equality under this mapping.
    pub fn eq(&self, a: &str, b: &str) -> bool {
        a.len() == b.len() && self.fold(a) == self.fold(b)
    }
}

/// Interpreted subset of RPL_ISUPPORT relevant to a bouncer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Isupport {
    /// Raw tokens in the order received (latest value wins on re-send).
    pub tokens: Vec<String>,
    pub casemapping: CaseMapping,
    /// Channel prefix modes as (mode, symbol), highest first, e.g. ('o','@').
    pub prefixes: Vec<(char, char)>,
    pub chantypes: String,
    /// CHANMODES groups A,B,C,D.
    pub chanmodes: [String; 4],
    pub network: Option<String>,
    pub statusmsg: String,
}

impl Isupport {
    /// Value of a token (`NICKLEN=12` -> "12"); "" for a token without one.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.tokens.iter().rev().find_map(|t| {
            let (k, v) = t.split_once('=').unwrap_or((t.as_str(), ""));
            k.eq_ignore_ascii_case(name).then_some(v)
        })
    }
}

impl Default for Isupport {
    fn default() -> Self {
        Isupport {
            tokens: Vec::new(),
            casemapping: CaseMapping::Rfc1459,
            prefixes: vec![('o', '@'), ('v', '+')],
            chantypes: "#&".to_string(),
            chanmodes: [
                "b".to_string(),
                "k".to_string(),
                "l".to_string(),
                "imnpst".to_string(),
            ],
            network: None,
            statusmsg: String::new(),
        }
    }
}

impl Isupport {
    /// Applies the tokens of one RPL_ISUPPORT line (without the target nick
    /// and the trailing human-readable text).
    pub fn apply<S: AsRef<str>>(&mut self, tokens: &[S]) {
        for raw in tokens {
            let raw = raw.as_ref();
            if raw.is_empty() {
                continue;
            }
            let (negated, tok) = match raw.strip_prefix('-') {
                Some(t) => (true, t),
                None => (false, raw),
            };
            let (key, value) = match tok.split_once('=') {
                Some((k, v)) => (k, v),
                None => (tok, ""),
            };
            let key_upper = key.to_ascii_uppercase();
            self.tokens.retain(|t| {
                let k = t.split('=').next().unwrap_or("");
                !k.eq_ignore_ascii_case(&key_upper)
            });
            if negated {
                continue;
            }
            self.tokens.push(tok.to_string());
            match key_upper.as_str() {
                "CASEMAPPING" => self.casemapping = CaseMapping::from_isupport(value),
                "PREFIX" => {
                    if let Some(p) = parse_prefix_token(value) {
                        self.prefixes = p;
                    }
                }
                "CHANTYPES" => self.chantypes = value.to_string(),
                "CHANMODES" => {
                    let mut groups = value.split(',');
                    for slot in self.chanmodes.iter_mut() {
                        *slot = groups.next().unwrap_or("").to_string();
                    }
                }
                "NETWORK" => self.network = Some(value.to_string()),
                "STATUSMSG" => self.statusmsg = value.to_string(),
                _ => {}
            }
        }
    }

    /// True if `name` is a channel name for this network.
    pub fn is_channel(&self, name: &str) -> bool {
        name.chars()
            .next()
            .map(|c| self.chantypes.contains(c))
            .unwrap_or(false)
    }

    /// Strips STATUSMSG prefixes (e.g. `@#chan`) and returns the bare target.
    pub fn strip_statusmsg<'a>(&self, target: &'a str) -> &'a str {
        let trimmed = target.trim_start_matches(|c| self.statusmsg.contains(c));
        if self.is_channel(trimmed) {
            trimmed
        } else {
            target
        }
    }

    /// Returns the prefix symbol for a membership mode letter.
    pub fn symbol_for_mode(&self, mode: char) -> Option<char> {
        self.prefixes
            .iter()
            .find(|(m, _)| *m == mode)
            .map(|(_, s)| *s)
    }

    /// True if `c` is a membership prefix symbol.
    pub fn is_prefix_symbol(&self, c: char) -> bool {
        self.prefixes.iter().any(|(_, s)| *s == c)
    }

    /// Whether channel mode `mode` takes a parameter when set (`adding`) or unset.
    pub fn mode_takes_param(&self, mode: char, adding: bool) -> bool {
        if self.prefixes.iter().any(|(m, _)| *m == mode) {
            return true;
        }
        if self.chanmodes[0].contains(mode) || self.chanmodes[1].contains(mode) {
            return true;
        }
        if self.chanmodes[2].contains(mode) {
            return adding;
        }
        false
    }

    /// Sorts a set of prefix symbols by rank (highest first).
    pub fn sort_symbols(&self, symbols: &str) -> String {
        self.prefixes
            .iter()
            .filter(|(_, s)| symbols.contains(*s))
            .map(|(_, s)| *s)
            .collect()
    }
}

/// Parses a PREFIX token value such as `(ohv)@%+`.
pub fn parse_prefix_token(value: &str) -> Option<Vec<(char, char)>> {
    if value.is_empty() {
        return Some(Vec::new());
    }
    let inner = value.strip_prefix('(')?;
    let (modes, symbols) = inner.split_once(')')?;
    if modes.chars().count() != symbols.chars().count() {
        return None;
    }
    Some(modes.chars().zip(symbols.chars()).collect())
}

/// Returns the CTCP command of a PRIVMSG/NOTICE body (`ACTION`, `VERSION`...),
/// or `None` if the text is not CTCP.
pub fn ctcp_command(text: &str) -> Option<&str> {
    let inner = text.strip_prefix('\x01')?;
    let inner = inner.strip_suffix('\x01').unwrap_or(inner);
    Some(inner.split(' ').next().unwrap_or(""))
}

/// Splits a list of items into lines whose joined length stays under `max`.
pub fn chunk_words<'a, I: IntoIterator<Item = &'a str>>(items: I, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for item in items {
        if !cur.is_empty() && cur.len() + 1 + item.len() > max {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(item);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_command() {
        let m = Message::parse("PING :irc.example.org\r\n").unwrap();
        assert_eq!(m.command, "PING");
        assert_eq!(m.params, vec!["irc.example.org"]);
        assert!(m.prefix.is_none());
    }

    #[test]
    fn rejects_embedded_line_breaks() {
        // Found by fuzzing: CR inside a trailing parameter round-tripped
        // into a second line.
        let e = Message::parse(":nick!u PRIVMSG #c :Po hh\rRIVMSG #la\r\u{5}\0\n");
        assert_eq!(e.unwrap_err(), ParseError::InvalidCharacter);
        assert!(Message::parse("PRIVMSG #a :x\ny").is_err());
        assert!(Message::parse("PRIVMSG #a :x\0").is_err());
        assert!(Message::parse("PRIVMSG #a :ok\r\n").is_ok());
        assert!(Message::parse("@ @").is_err());
        assert!(Message::parse(":p :x").is_err());
    }

    #[test]
    fn parses_prefix_and_trailing() {
        let m = Message::parse(":nick!user@host PRIVMSG #chan :hello world").unwrap();
        assert_eq!(m.prefix.as_deref(), Some("nick!user@host"));
        assert_eq!(m.command, "PRIVMSG");
        assert_eq!(m.params, vec!["#chan", "hello world"]);
        assert_eq!(m.source_nick(), Some("nick"));
        assert!(m.source_is_user());
    }

    #[test]
    fn parses_tags_with_escapes() {
        let m = Message::parse(
            "@time=2024-01-01T00:00:00.000Z;+example=a\\sb\\:c\\\\d;flag :srv NOTICE * :hi",
        )
        .unwrap();
        assert_eq!(m.tag("time"), Some("2024-01-01T00:00:00.000Z"));
        assert_eq!(m.tag("+example"), Some("a b;c\\d"));
        assert_eq!(m.tag("flag"), Some(""));
        assert_eq!(m.tag("missing"), None);
    }

    #[test]
    fn roundtrips_tags() {
        let mut m = Message::cmd("PRIVMSG", &["#c", "x y"]);
        m.set_tag("+k", Some("a;b c\\".into()));
        let s = m.to_string();
        assert_eq!(s, "@+k=a\\:b\\sc\\\\ PRIVMSG #c :x y");
        assert_eq!(Message::parse(&s).unwrap(), m);
    }

    #[test]
    fn trailing_rules() {
        assert_eq!(Message::cmd("JOIN", &["#a"]).to_string(), "JOIN #a");
        assert_eq!(Message::cmd("TOPIC", &["#a", ""]).to_string(), "TOPIC #a :");
        assert_eq!(
            Message::cmd("PRIVMSG", &["#a", ":)"]).to_string(),
            "PRIVMSG #a ::)"
        );
    }

    #[test]
    fn multiple_spaces_are_tolerated() {
        let m = Message::parse(":srv  001   nick  :Welcome").unwrap();
        assert_eq!(m.command, "001");
        assert_eq!(m.params, vec!["nick", "Welcome"]);
    }

    #[test]
    fn lowercase_command_is_normalised() {
        assert_eq!(Message::parse("privmsg #a b").unwrap().command, "PRIVMSG");
    }

    #[test]
    fn rejects_bad_lines() {
        assert_eq!(Message::parse(""), Err(ParseError::Empty));
        assert_eq!(Message::parse("   \r\n"), Err(ParseError::Empty));
        assert_eq!(Message::parse("@a=b"), Err(ParseError::MissingCommand));
        assert_eq!(Message::parse(":prefix"), Err(ParseError::MissingCommand));
        let long = "A".repeat(MAX_LINE_LEN + 1);
        assert_eq!(Message::parse(&long), Err(ParseError::TooLong));
    }

    #[test]
    fn unescape_edge_cases() {
        assert_eq!(unescape_tag_value("abc\\"), "abc");
        assert_eq!(unescape_tag_value("a\\bc"), "abc");
        assert_eq!(unescape_tag_value("\\r\\n"), "\r\n");
    }

    #[test]
    fn prefix_parsing() {
        let p = Prefix::parse("nick!user@host");
        assert_eq!(
            (p.nick, p.user, p.host),
            ("nick", Some("user"), Some("host"))
        );
        let s = Prefix::parse("irc.example.org");
        assert_eq!((s.nick, s.user, s.host), ("irc.example.org", None, None));
    }

    #[test]
    fn casemapping() {
        assert_eq!(CaseMapping::Rfc1459.fold("Nick[]\\~"), "nick{}|^");
        assert_eq!(CaseMapping::Rfc1459Strict.fold("Nick[]\\~"), "nick{}|~");
        assert_eq!(CaseMapping::Ascii.fold("Nick[]\\~"), "nick[]\\~");
        assert!(CaseMapping::Rfc1459.eq("#Foo[x]", "#foo{X}"));
        assert_eq!(CaseMapping::from_isupport("ascii"), CaseMapping::Ascii);
    }

    #[test]
    fn isupport_apply() {
        let mut is = Isupport::default();
        is.apply(&[
            "CASEMAPPING=ascii",
            "PREFIX=(qaohv)~&@%+",
            "CHANTYPES=#",
            "CHANMODES=beI,k,l,imnpst",
            "NETWORK=Libera.Chat",
            "STATUSMSG=@+",
        ]);
        assert_eq!(is.casemapping, CaseMapping::Ascii);
        assert_eq!(is.prefixes.len(), 5);
        assert_eq!(is.symbol_for_mode('h'), Some('%'));
        assert!(is.is_channel("#xbnc-dev"));
        assert!(!is.is_channel("&local"));
        assert_eq!(is.network.as_deref(), Some("Libera.Chat"));
        assert_eq!(is.strip_statusmsg("@#xbnc-dev"), "#xbnc-dev");
        assert_eq!(is.strip_statusmsg("+nick"), "+nick");
        assert!(is.mode_takes_param('b', false));
        assert!(is.mode_takes_param('l', true));
        assert!(!is.mode_takes_param('l', false));
        assert!(!is.mode_takes_param('m', true));
        assert_eq!(is.sort_symbols("+@"), "@+");
        is.apply(&["-NETWORK"]);
        assert!(is.tokens.iter().all(|t| !t.starts_with("NETWORK")));
    }

    #[test]
    fn prefix_token() {
        assert_eq!(
            parse_prefix_token("(ov)@+"),
            Some(vec![('o', '@'), ('v', '+')])
        );
        assert_eq!(parse_prefix_token("(ov)@"), None);
        assert_eq!(parse_prefix_token(""), Some(vec![]));
    }

    #[test]
    fn ctcp() {
        assert_eq!(ctcp_command("\x01ACTION waves\x01"), Some("ACTION"));
        assert_eq!(ctcp_command("\x01VERSION\x01"), Some("VERSION"));
        assert_eq!(ctcp_command("hello"), None);
    }

    #[test]
    fn chunking() {
        let lines = chunk_words(["aaa", "bbb", "ccc"], 7);
        assert_eq!(lines, vec!["aaa bbb", "ccc"]);
    }
}
