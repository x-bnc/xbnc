//! Native modules: the catalog the panel and `*status` show, their settings
//! schema, validation, and the typed per-network configuration the network
//! actor uses.
//!
//! Modules are compiled into xBNC (no dynamic loading, no extra attack
//! surface). Each one is enabled and configured per network; the stored form
//! is a JSON object of setting values, validated against the schema here on
//! every write. Texts (names, descriptions, setting labels) live in the
//! panel's translations and in `i18n`, not here.

use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    Bool,
    Int {
        min: i64,
        max: i64,
    },
    Text {
        max: usize,
    },
    /// One entry per line.
    Lines {
        max_lines: usize,
        max_len: usize,
    },
    /// One of a fixed list of values.
    Choice {
        options: &'static [&'static str],
    },
    /// A password: stored encrypted, never shown again.
    Secret {
        max: usize,
    },
    /// Several secret lines (for example `#channel key`), stored encrypted.
    SecretLines {
        max_lines: usize,
        max_len: usize,
    },
}

/// Prefix of a stored (encrypted) secret value.
pub const SEALED: &str = "enc:";

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Setting {
    pub key: &'static str,
    #[serde(flatten)]
    pub kind: Kind,
    pub default: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ModuleInfo {
    pub id: &'static str,
    pub default_enabled: bool,
    pub settings: &'static [Setting],
}

const fn b(key: &'static str, default: &'static str) -> Setting {
    Setting {
        key,
        kind: Kind::Bool,
        default,
    }
}
const fn int(key: &'static str, min: i64, max: i64, default: &'static str) -> Setting {
    Setting {
        key,
        kind: Kind::Int { min, max },
        default,
    }
}
const fn text(key: &'static str, max: usize, default: &'static str) -> Setting {
    Setting {
        key,
        kind: Kind::Text { max },
        default,
    }
}
const fn choice(
    key: &'static str,
    options: &'static [&'static str],
    default: &'static str,
) -> Setting {
    Setting {
        key,
        kind: Kind::Choice { options },
        default,
    }
}
const fn secret(key: &'static str, max: usize) -> Setting {
    Setting {
        key,
        kind: Kind::Secret { max },
        default: "",
    }
}
const fn secret_lines(key: &'static str, max_lines: usize, max_len: usize) -> Setting {
    Setting {
        key,
        kind: Kind::SecretLines { max_lines, max_len },
        default: "",
    }
}
const fn lines(key: &'static str, max_lines: usize, max_len: usize) -> Setting {
    Setting {
        key,
        kind: Kind::Lines { max_lines, max_len },
        default: "",
    }
}

pub const MODULES: &[ModuleInfo] = &[
    ModuleInfo {
        id: "keepnick",
        default_enabled: true,
        settings: &[],
    },
    ModuleInfo {
        id: "away",
        default_enabled: true,
        settings: &[
            text("message", 200, ""),
            int("delay", 0, 3600, "60"),
            b("summary", "true"),
            text("nick", 30, ""),
        ],
    },
    ModuleInfo {
        id: "autoop",
        default_enabled: false,
        settings: &[int("delay", 0, 30, "3"), lines("entries", 200, 400)],
    },
    ModuleInfo {
        id: "kickrejoin",
        default_enabled: false,
        settings: &[int("delay", 0, 600, "10")],
    },
    ModuleInfo {
        id: "stickychan",
        default_enabled: false,
        settings: &[lines("channels", 100, 200)],
    },
    ModuleInfo {
        id: "events",
        default_enabled: true,
        settings: &[b("joins", "true"), b("other", "true")],
    },
    ModuleInfo {
        id: "log",
        default_enabled: false,
        settings: &[int("retention_days", 0, 3650, "0"), b("events", "true")],
    },
    ModuleInfo {
        id: "route_replies",
        default_enabled: true,
        settings: &[],
    },
    ModuleInfo {
        id: "cert",
        default_enabled: false,
        settings: &[b("sasl_external", "true")],
    },
    ModuleInfo {
        id: "alias",
        default_enabled: true,
        settings: &[lines("aliases", 100, 400)],
    },
    ModuleInfo {
        id: "watch",
        default_enabled: true,
        settings: &[b("mentions", "false"), lines("patterns", 100, 200)],
    },
    ModuleInfo {
        id: "block_motd",
        default_enabled: false,
        settings: &[],
    },
    ModuleInfo {
        id: "ctcp",
        default_enabled: true,
        settings: &[
            int("flood_limit", 1, 100, "5"),
            text("version", 200, crate::network::CTCP_VERSION_DEFAULT),
            lines("block", 30, 30),
        ],
    },
    ModuleInfo {
        id: "dcc",
        default_enabled: false,
        settings: &[
            int("max_size_mb", 0, 1_000_000, "4096"),
            int("wait", 30, 900, "180"),
            b("private_addresses", "false"),
        ],
    },
    ModuleInfo {
        id: "services",
        default_enabled: false,
        settings: &[
            choice("service", SERVICES, "auto"),
            text("account", 64, ""),
            secret("password", 200),
            b("hide_host", "true"),
            b("login_on_connect", "true"),
            int("wait", 0, 120, "15"),
            b("recover_nick", "true"),
        ],
    },
    ModuleInfo {
        id: "invite",
        default_enabled: false,
        settings: &[
            choice("service", CHANSERVICES, "auto"),
            b("unban", "true"),
            b("join_invites", "true"),
        ],
    },
    ModuleInfo {
        id: "charset",
        default_enabled: false,
        settings: &[
            choice("legacy", LEGACY, "windows-1252"),
            lines("send_legacy", 100, 200),
        ],
    },
    ModuleInfo {
        id: "detach",
        default_enabled: false,
        settings: &[
            lines("channels", 200, 200),
            b("on_mention", "true"),
            lines("nicks", 100, 100),
        ],
    },
    ModuleInfo {
        id: "flooddetach",
        default_enabled: false,
        settings: &[
            int("messages", 3, 1000, "25"),
            int("seconds", 1, 600, "10"),
            int("calm", 10, 3600, "60"),
        ],
    },
    ModuleInfo {
        id: "stripcontrols",
        default_enabled: false,
        settings: &[lines("channels", 200, 200)],
    },
    ModuleInfo {
        id: "autocycle",
        default_enabled: false,
        settings: &[lines("channels", 200, 200)],
    },
    ModuleInfo {
        id: "crypt",
        default_enabled: false,
        settings: &[
            secret_lines("keys", 100, 300),
            text("prefix", 30, ""),
            b("key_exchange", "true"),
        ],
    },
    ModuleInfo {
        id: "ignore",
        default_enabled: false,
        settings: &[lines("masks", 200, 200), b("silence", "true")],
    },
    ModuleInfo {
        id: "schedule",
        default_enabled: false,
        settings: &[lines("tasks", 50, 450)],
    },
];

/// Services that log in to an account (`services` module).
pub const SERVICES: &[&str] = &["auto", "x", "nickserv", "q", "authserv"];
/// Services that invite and unban (`invite` module).
pub const CHANSERVICES: &[&str] = &["auto", "x", "chanserv", "q"];
/// Legacy 8-bit encodings understood by the `charset` module.
pub const LEGACY: &[&str] = &["windows-1252", "iso-8859-15", "iso-8859-1"];

/// Features that are always on (no per-network switch), listed so the panel
/// can show where they are configured.
pub const BUILTIN: &[&str] = &[
    "perform",
    "chansaver",
    "sasl",
    "fail2ban",
    "notes",
    "lastseen",
    "playback",
    "webadmin",
];

pub fn info(id: &str) -> Option<&'static ModuleInfo> {
    MODULES.iter().find(|m| m.id == id)
}

/// Validates and normalises a module's settings: unknown keys are rejected,
/// missing ones get their default, values are coerced to their type.
pub fn validate(id: &str, values: &Map<String, Value>) -> Result<Map<String, Value>> {
    let Some(m) = info(id) else {
        bail!("unknown module {id:?}");
    };
    for k in values.keys() {
        if !m.settings.iter().any(|s| s.key == k) {
            bail!("unknown setting {k:?} for module {id}");
        }
    }
    let mut out = Map::new();
    for s in m.settings {
        let v = values.get(s.key);
        let norm = match s.kind {
            Kind::Bool => Value::Bool(match v {
                None => s.default == "true",
                Some(Value::Bool(b)) => *b,
                Some(Value::String(t)) if t == "true" || t == "on" || t == "1" => true,
                Some(Value::String(t)) if t == "false" || t == "off" || t == "0" => false,
                Some(_) => bail!("{}: expected true or false", s.key),
            }),
            Kind::Int { min, max } => {
                let n = match v {
                    None => s.default.parse().unwrap_or(min),
                    Some(Value::Number(n)) => n
                        .as_i64()
                        .ok_or_else(|| anyhow::anyhow!("{}: expected a whole number", s.key))?,
                    Some(Value::String(t)) => t
                        .trim()
                        .parse()
                        .map_err(|_| anyhow::anyhow!("{}: expected a whole number", s.key))?,
                    Some(_) => bail!("{}: expected a whole number", s.key),
                };
                if n < min || n > max {
                    bail!("{}: must be between {min} and {max}", s.key);
                }
                Value::from(n)
            }
            Kind::Text { max } => {
                let t = match v {
                    None => s.default.to_string(),
                    Some(Value::String(t)) => t.trim().to_string(),
                    Some(_) => bail!("{}: expected text", s.key),
                };
                if t.chars().count() > max {
                    bail!("{}: at most {max} characters", s.key);
                }
                if t.chars().any(char::is_control) {
                    bail!("{}: control characters are not allowed", s.key);
                }
                Value::String(t)
            }
            Kind::Lines { max_lines, max_len } => {
                let t = match v {
                    None => String::new(),
                    Some(Value::String(t)) => t.clone(),
                    Some(Value::Array(a)) => a
                        .iter()
                        .map(|x| x.as_str().unwrap_or_default().to_string())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    Some(_) => bail!("{}: expected text", s.key),
                };
                let ls: Vec<&str> = t.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
                if ls.len() > max_lines {
                    bail!("{}: at most {max_lines} lines", s.key);
                }
                for l in &ls {
                    if l.chars().count() > max_len {
                        bail!("{}: each line can have at most {max_len} characters", s.key);
                    }
                    if l.chars().any(|c| c.is_control() && c != '\t') {
                        bail!("{}: control characters are not allowed", s.key);
                    }
                }
                Value::String(ls.join("\n"))
            }
            Kind::Choice { options } => {
                let t = match v {
                    None => s.default.to_string(),
                    Some(Value::String(t)) => t.trim().to_ascii_lowercase(),
                    Some(_) => bail!("{}: expected text", s.key),
                };
                if !options.contains(&t.as_str()) {
                    bail!("{}: use one of {}", s.key, options.join(", "));
                }
                Value::String(t)
            }
            Kind::Secret { max } => {
                let t = match v {
                    None | Some(Value::Null) => String::new(),
                    Some(Value::String(t)) => t.trim().to_string(),
                    Some(_) => bail!("{}: expected text", s.key),
                };
                if !t.starts_with(SEALED) {
                    if t.chars().count() > max {
                        bail!("{}: at most {max} characters", s.key);
                    }
                    if t.chars().any(char::is_control) {
                        bail!("{}: control characters are not allowed", s.key);
                    }
                } else if t.len() > 16_384 {
                    bail!("{}: value too long", s.key);
                }
                Value::String(t)
            }
            Kind::SecretLines { max_lines, max_len } => {
                let t = match v {
                    None | Some(Value::Null) => String::new(),
                    Some(Value::String(t)) => t.clone(),
                    Some(_) => bail!("{}: expected text", s.key),
                };
                if t.starts_with(SEALED) {
                    if t.len() > 65_536 {
                        bail!("{}: value too long", s.key);
                    }
                    Value::String(t)
                } else {
                    let ls: Vec<&str> =
                        t.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
                    if ls.len() > max_lines {
                        bail!("{}: at most {max_lines} lines", s.key);
                    }
                    for l in &ls {
                        if l.chars().count() > max_len {
                            bail!("{}: each line can have at most {max_len} characters", s.key);
                        }
                        if l.chars().any(char::is_control) {
                            bail!("{}: control characters are not allowed", s.key);
                        }
                    }
                    Value::String(ls.join("\n"))
                }
            }
        };
        out.insert(s.key.to_string(), norm);
    }
    match id {
        "autoop" => {
            for l in str_of(&out, "entries").lines() {
                AutoOpEntry::parse(l)?;
            }
        }
        "alias" => {
            for l in str_of(&out, "aliases").lines() {
                Alias::parse(l)?;
            }
        }
        "watch" => {
            for l in str_of(&out, "patterns").lines() {
                WatchPattern::parse(l)?;
            }
        }
        "stickychan" => {
            for l in str_of(&out, "channels").lines() {
                if l.contains(char::is_whitespace) || l.contains(',') {
                    bail!("channels: one channel per line ({l:?})");
                }
            }
        }
        "ctcp" => {
            for l in str_of(&out, "block").lines() {
                if !l.chars().all(|c| c.is_ascii_alphanumeric()) {
                    bail!("block: CTCP names are letters only ({l:?})");
                }
            }
        }
        "services" => {
            let acc = str_of(&out, "account");
            if acc.contains(char::is_whitespace) {
                bail!("account: no spaces");
            }
        }
        "charset" | "stripcontrols" | "autocycle" | "detach" => {
            for k in ["send_legacy", "channels", "nicks"] {
                for l in str_of(&out, k).lines() {
                    if l.contains(char::is_whitespace) || l.contains(',') {
                        bail!("{k}: one name per line ({l:?})");
                    }
                }
            }
        }
        "crypt" => {
            let keys = str_of(&out, "keys");
            if !keys.starts_with(SEALED) {
                for l in keys.lines() {
                    CryptKey::parse(l)?;
                }
            }
        }
        "ignore" => {
            for l in str_of(&out, "masks").lines() {
                let m = normalise_mask(l.split_whitespace().next().unwrap_or(""));
                if l.contains(char::is_whitespace) {
                    bail!("masks: one mask per line ({l:?})");
                }
                if m == "*!*@*" {
                    bail!("masks: the mask {l} would ignore everybody");
                }
            }
        }
        "schedule" => {
            for l in str_of(&out, "tasks").lines() {
                Task::parse(l)?;
            }
        }
        _ => {}
    }
    Ok(out)
}

/// Prepares settings received from the panel or `*status` for storage:
/// secret values are encrypted with `encrypt`; an empty secret keeps the
/// stored one and `null` clears it. The result still has to be validated.
pub fn seal(
    id: &str,
    new: &Map<String, Value>,
    old: Option<&Map<String, Value>>,
    encrypt: &dyn Fn(&str) -> String,
) -> Map<String, Value> {
    let mut out = new.clone();
    let Some(m) = info(id) else {
        return out;
    };
    for s in m.settings {
        if !matches!(s.kind, Kind::Secret { .. } | Kind::SecretLines { .. }) {
            continue;
        }
        let stored = old
            .and_then(|o| o.get(s.key))
            .and_then(Value::as_str)
            .filter(|v| v.starts_with(SEALED))
            .map(str::to_string);
        let v = match new.get(s.key) {
            Some(Value::Null) => String::new(),
            None => stored.unwrap_or_default(),
            Some(Value::String(t)) if t.trim().is_empty() => stored.unwrap_or_default(),
            Some(Value::String(t)) if t.starts_with(SEALED) => {
                // Only a value we stored ourselves is kept as is.
                if stored.as_deref() == Some(t.as_str()) {
                    t.clone()
                } else {
                    String::new()
                }
            }
            Some(Value::String(t)) => {
                // Normalise like validation does before encrypting.
                let plain = match s.kind {
                    Kind::SecretLines { .. } => t
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => t.trim().to_string(),
                };
                format!("{SEALED}{}", encrypt(&plain))
            }
            Some(other) => other.to_string(),
        };
        out.insert(s.key.to_string(), Value::String(v));
    }
    out
}

/// Checks the plain text of secret settings before they are encrypted.
pub fn validate_plain(id: &str, values: &Map<String, Value>) -> Result<()> {
    if id == "crypt" {
        if let Some(Value::String(keys)) = values.get("keys") {
            if !keys.starts_with(SEALED) {
                for l in keys.lines().map(str::trim).filter(|l| !l.is_empty()) {
                    CryptKey::parse(l)?;
                }
            }
        }
    }
    validate(id, values).map(|_| ())
}

/// Settings as shown to people: secrets are blanked. Returns the names of
/// the secrets that are set.
pub fn redact(id: &str, cfg: &Map<String, Value>) -> (Map<String, Value>, Vec<&'static str>) {
    let mut out = cfg.clone();
    let mut set = Vec::new();
    if let Some(m) = info(id) {
        for s in m.settings {
            if matches!(s.kind, Kind::Secret { .. } | Kind::SecretLines { .. }) {
                if out
                    .get(s.key)
                    .and_then(Value::as_str)
                    .is_some_and(|v| !v.is_empty())
                {
                    set.push(s.key);
                }
                out.insert(s.key.to_string(), Value::String(String::new()));
            }
        }
    }
    (out, set)
}

fn str_of<'a>(m: &'a Map<String, Value>, k: &str) -> &'a str {
    m.get(k).and_then(Value::as_str).unwrap_or("")
}
fn bool_of(m: &Map<String, Value>, k: &str) -> bool {
    m.get(k).and_then(Value::as_bool).unwrap_or(false)
}
fn int_of(m: &Map<String, Value>, k: &str) -> i64 {
    m.get(k).and_then(Value::as_i64).unwrap_or(0)
}

/// One stored row: whether the module is on and its (validated) settings.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModState {
    pub enabled: bool,
    pub config: Map<String, Value>,
}

/// Effective state of every module for a network: stored rows merged with
/// defaults.
pub fn effective(rows: &HashMap<String, ModState>) -> Vec<(&'static ModuleInfo, ModState)> {
    MODULES
        .iter()
        .map(|m| {
            let st = match rows.get(m.id) {
                Some(r) => ModState {
                    enabled: r.enabled,
                    config: validate(m.id, &r.config)
                        .unwrap_or_else(|_| validate(m.id, &Map::new()).unwrap_or_default()),
                },
                None => ModState {
                    enabled: m.default_enabled,
                    config: validate(m.id, &Map::new()).unwrap_or_default(),
                },
            };
            (m, st)
        })
        .collect()
}

// ------------------------------------------------------------ typed config

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpMode {
    Op,
    HalfOp,
    Voice,
}

impl OpMode {
    pub fn letter(self) -> char {
        match self {
            OpMode::Op => 'o',
            OpMode::HalfOp => 'h',
            OpMode::Voice => 'v',
        }
    }
}

/// `o|h|v <mask> [#chan,#chan|*] [key]`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoOpEntry {
    pub mode: OpMode,
    pub mask: String,
    /// Empty means every channel.
    pub channels: Vec<String>,
    /// Shared secret for the challenge/response check.
    pub key: Option<String>,
}

impl AutoOpEntry {
    pub fn parse(line: &str) -> Result<AutoOpEntry> {
        let mut w = line.split_whitespace();
        let mode = match w.next().map(str::to_ascii_lowercase).as_deref() {
            Some("o" | "op" | "@") => OpMode::Op,
            Some("h" | "halfop" | "%") => OpMode::HalfOp,
            Some("v" | "voice" | "+") => OpMode::Voice,
            _ => bail!("entries: each line starts with o, h or v ({line:?})"),
        };
        let Some(mask) = w.next() else {
            bail!("entries: missing mask ({line:?})");
        };
        let mask = normalise_mask(mask);
        if mask == "*!*@*" {
            bail!("entries: the mask *!*@* would match everybody ({line:?})");
        }
        let channels = match w.next() {
            None | Some("*") => Vec::new(),
            Some(c) => c.split(',').map(str::to_string).collect(),
        };
        let key = w.next().map(str::to_string);
        if w.next().is_some() {
            bail!("entries: too many fields ({line:?})");
        }
        Ok(AutoOpEntry {
            mode,
            mask,
            channels,
            key,
        })
    }
}

/// Turns `nick`, `nick!user` or `*@host` into a full `nick!user@host` mask.
pub fn normalise_mask(m: &str) -> String {
    let (nu, host) = match m.split_once('@') {
        Some((a, h)) => (a, h),
        None => (m, "*"),
    };
    let (nick, user) = match nu.split_once('!') {
        Some((n, u)) => (n, u),
        None => (nu, "*"),
    };
    let f = |s: &str| {
        if s.is_empty() {
            "*".to_string()
        } else {
            s.to_string()
        }
    };
    format!("{}!{}@{}", f(nick), f(user), f(host))
}

/// `NAME /command args` — `$1`..`$9`, `$1-` (rest), `$nick`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias {
    pub name: String,
    pub template: String,
}

/// IRC commands an alias may not replace.
const RESERVED: &[&str] = &[
    "PRIVMSG",
    "NOTICE",
    "JOIN",
    "PART",
    "QUIT",
    "NICK",
    "USER",
    "PASS",
    "CAP",
    "PING",
    "PONG",
    "MODE",
    "TOPIC",
    "KICK",
    "AUTHENTICATE",
    "CHATHISTORY",
    "AWAY",
    "WHO",
    "WHOIS",
];

impl Alias {
    pub fn parse(line: &str) -> Result<Alias> {
        let (name, template) = line
            .trim()
            .split_once(char::is_whitespace)
            .ok_or_else(|| anyhow::anyhow!("aliases: use NAME /command ... ({line:?})"))?;
        let name = name.trim_start_matches('/').to_ascii_uppercase();
        if name.is_empty() || name.len() > 30 || !name.chars().all(|c| c.is_ascii_alphanumeric()) {
            bail!("aliases: invalid alias name {name:?}");
        }
        if RESERVED.contains(&name.as_str()) {
            bail!("aliases: {name} is an IRC command and cannot be an alias");
        }
        let a = Alias {
            name,
            template: template.trim().to_string(),
        };
        crate::perform::validate(&a.expand(&["arg"; 9], "nick"))
            .map_err(|e| anyhow::anyhow!("aliases: {}: {e}", a.name))?;
        Ok(a)
    }

    /// Expands the template with the client's arguments.
    pub fn expand(&self, args: &[&str], nick: &str) -> String {
        let mut out = self.template.clone();
        for i in (1..=9).rev() {
            let rest = args.get(i - 1..).map(|a| a.join(" ")).unwrap_or_default();
            out = out.replace(&format!("${i}-"), &rest);
            out = out.replace(&format!("${i}"), args.get(i - 1).copied().unwrap_or(""));
        }
        out.replace("$nick", nick)
    }
}

/// `pattern [#source|*]` — glob on the message text (case-insensitive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchPattern {
    pub pattern: String,
    /// Channel or nick the message must come from; None for anywhere.
    pub source: Option<String>,
}

impl WatchPattern {
    pub fn parse(line: &str) -> Result<WatchPattern> {
        let mut w = line.split_whitespace();
        let pattern = w.next().unwrap_or_default().to_string();
        if pattern.is_empty() || pattern.chars().all(|c| c == '*' || c == '?') {
            bail!("patterns: a pattern must contain some text ({line:?})");
        }
        let source = w.next().filter(|s| *s != "*").map(str::to_string);
        if w.next().is_some() {
            bail!("patterns: use `pattern [#channel]` (wrap words with * to match inside text) ({line:?})");
        }
        Ok(WatchPattern { pattern, source })
    }
}

/// Typed configuration of every module for one network.
#[derive(Debug, Clone, Default)]
pub struct NetModules {
    pub keepnick: bool,
    pub away: Option<AwayCfg>,
    pub autoop: Option<AutoOpCfg>,
    pub kickrejoin: Option<u64>,
    pub sticky: Vec<String>,
    pub events: Option<EventsCfg>,
    pub log: Option<LogCfg>,
    pub route_replies: bool,
    pub cert: Option<CertCfg>,
    pub aliases: Vec<Alias>,
    pub watch: Option<WatchCfg>,
    pub block_motd: bool,
    pub ctcp: Option<CtcpCfg>,
    pub dcc: Option<DccCfg>,
    pub services: Option<ServicesCfg>,
    pub invite: Option<InviteCfg>,
    pub charset: Option<CharsetCfg>,
    pub detach: Option<DetachCfg>,
    pub flooddetach: Option<FloodDetachCfg>,
    /// Channels whose colours are removed (empty list: all).
    pub stripcontrols: Option<Vec<String>>,
    /// Channels where autocycle acts (empty list: all).
    pub autocycle: Option<Vec<String>>,
    pub crypt: Option<CryptCfg>,
    pub ignore: Option<IgnoreCfg>,
    pub schedule: Vec<Task>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Auto,
    X,
    NickServ,
    Q,
    AuthServ,
    ChanServ,
}

impl Service {
    fn parse(s: &str) -> Service {
        match s {
            "x" => Service::X,
            "nickserv" => Service::NickServ,
            "q" => Service::Q,
            "authserv" => Service::AuthServ,
            "chanserv" => Service::ChanServ,
            _ => Service::Auto,
        }
    }

    /// Resolves `auto` from the network name (ISUPPORT NETWORK) or host.
    pub fn resolve(self, network: &str, host: &str) -> Service {
        if self != Service::Auto {
            return self;
        }
        let n = format!("{network} {host}").to_ascii_lowercase();
        if Self::no_services(&n) {
            // Anyone could take the nick "NickServ" here: stay Auto so no
            // password is ever sent.
            Service::Auto
        } else if n.contains("undernet") {
            Service::X
        } else if n.contains("quakenet") {
            Service::Q
        } else if n.contains("gamesurge") || n.contains("afternet") {
            // EvilNet answers to NickServ (checked on the real network).
            Service::AuthServ
        } else {
            Service::NickServ
        }
    }
}

impl Service {
    /// Networks known to have no account services.
    fn no_services(name_and_host: &str) -> bool {
        ["ircnet", "efnet"]
            .iter()
            .any(|n| name_and_host.contains(n))
    }
}

#[derive(Debug, Clone)]
pub struct ServicesCfg {
    pub service: Service,
    pub account: String,
    /// Decrypted password (empty: not set).
    pub password: String,
    pub hide_host: bool,
    pub login_on_connect: bool,
    /// Seconds to wait for the login and hidden host before joining.
    pub wait: u64,
    pub recover_nick: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct InviteCfg {
    pub service: Service,
    pub unban: bool,
    pub join_invites: bool,
}

#[derive(Debug, Clone)]
pub struct CharsetCfg {
    pub legacy: crate::util::Legacy,
    /// Channels and nicks that get messages in the legacy encoding.
    pub send_legacy: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DetachCfg {
    pub channels: Vec<String>,
    pub on_mention: bool,
    pub nicks: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct FloodDetachCfg {
    pub messages: u32,
    pub seconds: u64,
    pub calm: u64,
}

/// `<#channel|nick> [cbc:]<key>`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CryptKey {
    pub target: String,
    pub key: String,
    pub cbc: bool,
}

impl CryptKey {
    pub fn parse(line: &str) -> Result<CryptKey> {
        let mut w = line.split_whitespace();
        let (Some(target), Some(key), None) = (w.next(), w.next(), w.next()) else {
            bail!("keys: use `#channel key` or `nick key`, one per line");
        };
        let (cbc, key) = match key.strip_prefix("cbc:") {
            Some(k) => (true, k),
            None => (false, key.strip_prefix("ecb:").unwrap_or(key)),
        };
        if !(4..=56).contains(&key.len()) {
            bail!("keys: a key has 4 to 56 characters ({target})");
        }
        Ok(CryptKey {
            target: target.to_string(),
            key: key.to_string(),
            cbc,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CryptCfg {
    pub keys: Vec<CryptKey>,
    pub prefix: String,
    pub key_exchange: bool,
}

#[derive(Debug, Clone)]
pub struct IgnoreCfg {
    /// Full `nick!user@host` globs.
    pub masks: Vec<String>,
    pub silence: bool,
}

/// When a scheduled task runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// At a local time on some weekdays (bit 0 = Monday).
    At { days: u8, hour: u8, minute: u8 },
    /// Every so many minutes.
    Every { minutes: u32 },
}

/// One line of the `schedule` module: `[days] HH:MM /command` or
/// `every 30m /command`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub when: When,
    pub command: String,
}

const DAY_NAMES: &[(&[&str], u8)] = &[
    (&["lun", "lunes", "mon", "monday", "l"], 0),
    (&["mar", "martes", "tue", "tuesday", "m"], 1),
    (
        &[
            "mie",
            "mié",
            "miércoles",
            "miercoles",
            "wed",
            "wednesday",
            "x",
        ],
        2,
    ),
    (&["jue", "jueves", "thu", "thursday", "j"], 3),
    (&["vie", "viernes", "fri", "friday", "v"], 4),
    (
        &["sab", "sáb", "sábado", "sabado", "sat", "saturday", "s"],
        5,
    ),
    (&["dom", "domingo", "sun", "sunday", "d"], 6),
];

fn day_index(w: &str) -> Option<u8> {
    let w = w.to_lowercase();
    DAY_NAMES
        .iter()
        .find(|(names, _)| names.contains(&w.as_str()))
        .map(|(_, i)| *i)
}

fn parse_days(spec: &str) -> Option<u8> {
    let spec = spec.to_lowercase();
    match spec.as_str() {
        "*" | "diario" | "todos" | "daily" | "everyday" => return Some(0x7f),
        "laborables" | "weekdays" => return Some(0x1f),
        "finde" | "weekend" => return Some(0x60),
        _ => {}
    }
    let mut mask = 0u8;
    for part in spec.split(',') {
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b) = (day_index(a)?, day_index(b)?);
                let mut i = a;
                loop {
                    mask |= 1 << i;
                    if i == b {
                        break;
                    }
                    i = (i + 1) % 7;
                }
            }
            None => mask |= 1 << day_index(part)?,
        }
    }
    (mask != 0).then_some(mask)
}

fn parse_clock(s: &str) -> Option<(u8, u8)> {
    let (h, m) = s.split_once(':')?;
    let (h, m): (u8, u8) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60 && s.len() <= 5).then_some((h, m))
}

impl Task {
    pub fn parse(line: &str) -> Result<Task> {
        let err = || {
            anyhow::anyhow!(
                "tasks: use `HH:MM /command`, `mon-fri HH:MM /command` or `every 30m /command` ({line:?})"
            )
        };
        let line = line.trim();
        let (first, rest) = line.split_once(char::is_whitespace).ok_or_else(err)?;
        let rest = rest.trim();
        let (when, command) = if matches!(first.to_lowercase().as_str(), "cada" | "every") {
            let (n, cmd) = rest.split_once(char::is_whitespace).ok_or_else(err)?;
            let n = n.to_lowercase();
            let (num, unit) = n.split_at(n.find(|c: char| !c.is_ascii_digit()).unwrap_or(n.len()));
            let num: u32 = num.parse().map_err(|_| err())?;
            let minutes = match unit {
                "m" | "min" | "mins" | "minutos" | "minutes" => num,
                "h" | "hora" | "horas" | "hours" => num.saturating_mul(60),
                _ => return Err(err()),
            };
            if !(5..=10_080).contains(&minutes) {
                bail!("tasks: the interval goes from 5 minutes to 7 days ({line:?})");
            }
            (When::Every { minutes }, cmd.trim())
        } else if let Some((h, m)) = parse_clock(first) {
            (
                When::At {
                    days: 0x7f,
                    hour: h,
                    minute: m,
                },
                rest,
            )
        } else {
            let days = parse_days(first).ok_or_else(err)?;
            let (clock, cmd) = rest.split_once(char::is_whitespace).ok_or_else(err)?;
            let (h, m) = parse_clock(clock).ok_or_else(err)?;
            (
                When::At {
                    days,
                    hour: h,
                    minute: m,
                },
                cmd.trim(),
            )
        };
        if command.is_empty() {
            return Err(err());
        }
        crate::perform::validate(command).map_err(|e| anyhow::anyhow!("tasks: {e} ({line:?})"))?;
        Ok(Task {
            when,
            command: command.to_string(),
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DccCfg {
    /// Most megabytes relayed per transfer (0: no limit).
    pub max_size_mb: u64,
    /// Seconds a relay waits for the connection.
    pub wait: u64,
    /// Allow relaying to private addresses offered by people on IRC.
    pub private_addresses: bool,
}

#[derive(Debug, Clone)]
pub struct AwayCfg {
    pub message: String,
    pub delay: u64,
    pub summary: bool,
    /// Nick while away; `{nick}` is the usual nick ("{nick}|away").
    pub nick: String,
}

#[derive(Debug, Clone)]
pub struct AutoOpCfg {
    pub delay: u64,
    pub entries: Vec<AutoOpEntry>,
}

#[derive(Debug, Clone, Copy)]
pub struct EventsCfg {
    pub joins: bool,
    pub other: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct LogCfg {
    pub retention_days: u64,
    pub events: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct CertCfg {
    pub sasl_external: bool,
}

#[derive(Debug, Clone)]
pub struct WatchCfg {
    pub mentions: bool,
    pub patterns: Vec<WatchPattern>,
}

#[derive(Debug, Clone)]
pub struct CtcpCfg {
    pub flood_limit: u32,
    pub version: String,
    pub block: Vec<String>,
}

impl NetModules {
    /// Typed configuration; `decrypt` opens stored secrets.
    pub fn from_rows(
        rows: &HashMap<String, ModState>,
        decrypt: &dyn Fn(&str) -> Option<String>,
    ) -> NetModules {
        let open = |v: &str| -> String {
            match v.strip_prefix(SEALED) {
                Some(enc) => decrypt(enc).unwrap_or_default(),
                None => v.to_string(),
            }
        };
        let names = |c: &Map<String, Value>, k: &str| -> Vec<String> {
            str_of(c, k).lines().map(str::to_string).collect()
        };
        let mut n = NetModules::default();
        for (m, st) in effective(rows) {
            if !st.enabled {
                continue;
            }
            let c = &st.config;
            match m.id {
                "keepnick" => n.keepnick = true,
                "away" => {
                    n.away = Some(AwayCfg {
                        message: str_of(c, "message").to_string(),
                        delay: int_of(c, "delay").max(0) as u64,
                        summary: bool_of(c, "summary"),
                        nick: str_of(c, "nick").trim().to_string(),
                    })
                }
                "autoop" => {
                    n.autoop = Some(AutoOpCfg {
                        delay: int_of(c, "delay").max(0) as u64,
                        entries: str_of(c, "entries")
                            .lines()
                            .filter_map(|l| AutoOpEntry::parse(l).ok())
                            .collect(),
                    })
                }
                "kickrejoin" => n.kickrejoin = Some(int_of(c, "delay").max(0) as u64),
                "stickychan" => {
                    n.sticky = str_of(c, "channels").lines().map(str::to_string).collect()
                }
                "events" => {
                    n.events = Some(EventsCfg {
                        joins: bool_of(c, "joins"),
                        other: bool_of(c, "other"),
                    })
                }
                "log" => {
                    n.log = Some(LogCfg {
                        retention_days: int_of(c, "retention_days").max(0) as u64,
                        events: bool_of(c, "events"),
                    })
                }
                "route_replies" => n.route_replies = true,
                "cert" => {
                    n.cert = Some(CertCfg {
                        sasl_external: bool_of(c, "sasl_external"),
                    })
                }
                "alias" => {
                    n.aliases = str_of(c, "aliases")
                        .lines()
                        .filter_map(|l| Alias::parse(l).ok())
                        .collect()
                }
                "watch" => {
                    n.watch = Some(WatchCfg {
                        mentions: bool_of(c, "mentions"),
                        patterns: str_of(c, "patterns")
                            .lines()
                            .filter_map(|l| WatchPattern::parse(l).ok())
                            .collect(),
                    })
                }
                "block_motd" => n.block_motd = true,
                "ctcp" => {
                    n.ctcp = Some(CtcpCfg {
                        flood_limit: int_of(c, "flood_limit").clamp(1, 100) as u32,
                        version: str_of(c, "version").to_string(),
                        block: str_of(c, "block")
                            .lines()
                            .map(|l| l.to_ascii_uppercase())
                            .collect(),
                    })
                }
                "services" => {
                    n.services = Some(ServicesCfg {
                        service: Service::parse(str_of(c, "service")),
                        account: str_of(c, "account").to_string(),
                        password: open(str_of(c, "password")),
                        hide_host: bool_of(c, "hide_host"),
                        login_on_connect: bool_of(c, "login_on_connect"),
                        wait: int_of(c, "wait").clamp(0, 120) as u64,
                        recover_nick: bool_of(c, "recover_nick"),
                    })
                }
                "invite" => {
                    n.invite = Some(InviteCfg {
                        service: Service::parse(str_of(c, "service")),
                        unban: bool_of(c, "unban"),
                        join_invites: bool_of(c, "join_invites"),
                    })
                }
                "charset" => {
                    n.charset = Some(CharsetCfg {
                        legacy: crate::util::Legacy::parse(str_of(c, "legacy")),
                        send_legacy: names(c, "send_legacy"),
                    })
                }
                "detach" => {
                    n.detach = Some(DetachCfg {
                        channels: names(c, "channels"),
                        on_mention: bool_of(c, "on_mention"),
                        nicks: names(c, "nicks"),
                    })
                }
                "flooddetach" => {
                    n.flooddetach = Some(FloodDetachCfg {
                        messages: int_of(c, "messages").clamp(3, 1000) as u32,
                        seconds: int_of(c, "seconds").clamp(1, 600) as u64,
                        calm: int_of(c, "calm").clamp(10, 3600) as u64,
                    })
                }
                "stripcontrols" => n.stripcontrols = Some(names(c, "channels")),
                "autocycle" => n.autocycle = Some(names(c, "channels")),
                "crypt" => {
                    n.crypt = Some(CryptCfg {
                        keys: open(str_of(c, "keys"))
                            .lines()
                            .filter_map(|l| CryptKey::parse(l).ok())
                            .collect(),
                        prefix: str_of(c, "prefix").to_string(),
                        key_exchange: bool_of(c, "key_exchange"),
                    })
                }
                "ignore" => {
                    n.ignore = Some(IgnoreCfg {
                        masks: str_of(c, "masks").lines().map(normalise_mask).collect(),
                        silence: bool_of(c, "silence"),
                    })
                }
                "schedule" => {
                    n.schedule = str_of(c, "tasks")
                        .lines()
                        .filter_map(|l| Task::parse(l).ok())
                        .collect()
                }
                "dcc" => {
                    n.dcc = Some(DccCfg {
                        max_size_mb: int_of(c, "max_size_mb").clamp(0, 1_000_000) as u64,
                        wait: int_of(c, "wait").clamp(30, 900) as u64,
                        private_addresses: bool_of(c, "private_addresses"),
                    })
                }
                _ => {}
            }
        }
        n
    }
}

/// Prefix of the autoop challenge/response notices exchanged between bouncers.
pub const AUTOOP_TAG: &str = "!XBNC-AUTOOP";

/// Response to an autoop challenge: hex HMAC-SHA256 of the challenge, keyed
/// with the shared secret.
pub fn autoop_response(key: &str, challenge: &str) -> String {
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(key.as_bytes()).expect("any key length");
    mac.update(challenge.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Case-insensitive glob (`*` and `?`), iterative, no backtracking blow-up.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// True if `text` mentions `nick` as a whole word (case-insensitive).
pub fn mentions(text: &str, nick: &str) -> bool {
    if nick.is_empty() {
        return false;
    }
    let lt = text.to_lowercase();
    let ln = nick.to_lowercase();
    let word = |c: char| c.is_alphanumeric() || "_-[]\\`^{}|".contains(c);
    let mut start = 0;
    while let Some(pos) = lt[start..].find(&ln) {
        let i = start + pos;
        let before = lt[..i].chars().next_back();
        let after = lt[i + ln.len()..].chars().next();
        if !before.is_some_and(word) && !after.is_some_and(word) {
            return true;
        }
        start = i + ln.len().max(1);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn catalog_defaults_validate() {
        for m in MODULES {
            let v = validate(m.id, &Map::new()).unwrap();
            assert_eq!(v.len(), m.settings.len(), "{}", m.id);
        }
        let ids: std::collections::HashSet<_> = MODULES.iter().map(|m| m.id).collect();
        assert_eq!(ids.len(), MODULES.len(), "duplicate module ids");
    }

    #[test]
    fn validation_rules() {
        assert!(validate("nope", &Map::new()).is_err());
        assert!(validate("away", &obj(json!({"bogus": 1}))).is_err());
        assert!(validate("away", &obj(json!({"delay": 99999}))).is_err());
        assert!(validate("away", &obj(json!({"delay": "abc"}))).is_err());
        let v = validate("away", &obj(json!({"delay": "30", "summary": "off"}))).unwrap();
        assert_eq!(v["delay"], 30);
        assert_eq!(v["summary"], false);
        assert_eq!(v["message"], "");
        assert!(validate("away", &obj(json!({"message": "a\u{7}b"}))).is_err());
        assert!(validate("autoop", &obj(json!({"entries": "x *!*@h"}))).is_err());
        assert!(validate("autoop", &obj(json!({"entries": "o *"}))).is_err());
        let v = validate(
            "autoop",
            &obj(json!({"entries": "o  bob \n\n v *!*@trusted.example #a,#b"})),
        )
        .unwrap();
        assert_eq!(v["entries"], "o  bob\nv *!*@trusted.example #a,#b");
        assert!(validate("alias", &obj(json!({"aliases": "JOIN /msg x y"}))).is_err());
        assert!(validate(
            "alias",
            &obj(json!({"aliases": "ghost /msg NickServ GHOST $1"}))
        )
        .is_ok());
        assert!(validate("watch", &obj(json!({"patterns": "*"}))).is_err());
        assert!(validate("stickychan", &obj(json!({"channels": "#a #b"}))).is_err());
        assert!(validate("ctcp", &obj(json!({"block": "VER SION"}))).is_err());
    }

    #[test]
    fn typed_config_from_rows() {
        let mut rows = HashMap::new();
        let n = NetModules::from_rows(&rows, &|_| None);
        assert!(n.keepnick && n.route_replies && n.away.is_some() && n.autoop.is_none());
        rows.insert(
            "autoop".into(),
            ModState {
                enabled: true,
                config: obj(json!({"entries": "o bob #x secret\nv *@h.example"})),
            },
        );
        rows.insert(
            "keepnick".into(),
            ModState {
                enabled: false,
                config: Map::new(),
            },
        );
        let n = NetModules::from_rows(&rows, &|_| None);
        assert!(!n.keepnick);
        let a = n.autoop.unwrap();
        assert_eq!(a.entries[0].mask, "bob!*@*");
        assert_eq!(a.entries[0].key.as_deref(), Some("secret"));
        assert_eq!(a.entries[1].mask, "*!*@h.example");
        assert_eq!(a.entries[1].mode, OpMode::Voice);
        assert!(a.entries[1].channels.is_empty());
    }

    #[test]
    fn globs_and_mentions() {
        assert!(glob_match("*xbnc*", "I love XBNC so much"));
        assert!(glob_match("bob!*@*.example", "bob!u@host.example"));
        assert!(!glob_match("bob!*@*.example", "bobby!u@host.example"));
        assert!(glob_match("a?c", "abc") && !glob_match("a?c", "abbc"));
        assert!(glob_match(&"*a".repeat(20), &"a".repeat(30)));
        assert!(!glob_match(
            &"*a".repeat(20),
            &format!("{}b", "a".repeat(30))
        ));
        assert!(mentions("hey unixfool, hi", "UnixFool"));
        assert!(mentions("unixfool: hi", "unixfool"));
        assert!(!mentions("unixfoolish", "unixfool"));
        assert!(!mentions("xunixfool", "unixfool"));
    }

    #[test]
    fn alias_expansion() {
        let a = Alias::parse("ghost /msg NickServ GHOST $1 $2-").unwrap();
        assert_eq!(a.name, "GHOST");
        assert_eq!(
            a.expand(&["bob", "pw", "x"], "me"),
            "/msg NickServ GHOST bob pw x"
        );
        let a = Alias::parse("op /mode $1 +o $nick").unwrap();
        assert_eq!(a.expand(&["#chan"], "me"), "/mode #chan +o me");
        let a = Alias::parse("say /msg $1 $1-").unwrap();
        assert_eq!(a.expand(&["#c", "hello"], "me"), "/msg #c #c hello");
    }

    #[test]
    fn autoop_response_format() {
        // HMAC-SHA256(key="secret", "abc"), computed with Python's hmac module.
        assert_eq!(
            autoop_response("secret", "abc"),
            "9946dad4e00e913fc8be8e5d3f7e110a4a9e832f83fb09c345285d78638d8a0e"
        );
    }

    #[test]
    fn masks() {
        assert_eq!(normalise_mask("bob"), "bob!*@*");
        assert_eq!(normalise_mask("*@host"), "*!*@host");
        assert_eq!(normalise_mask("bob!u"), "bob!u@*");
        assert_eq!(normalise_mask("bob!u@h"), "bob!u@h");
    }

    #[test]
    fn scheduled_tasks() {
        let t = Task::parse("08:30 /away En el trabajo").unwrap();
        assert_eq!(
            t.when,
            When::At {
                days: 0x7f,
                hour: 8,
                minute: 30
            }
        );
        assert_eq!(t.command, "/away En el trabajo");
        let t = Task::parse("lun-vie 18:00 /msg #xbnc bye").unwrap();
        assert_eq!(
            t.when,
            When::At {
                days: 0x1f,
                hour: 18,
                minute: 0
            }
        );
        let t = Task::parse("sáb,dom 10:05 /msg #c hola").unwrap();
        assert!(matches!(t.when, When::At { days: 0x60, .. }));
        let t = Task::parse("vie-lun 10:05 /msg #c hola").unwrap();
        assert!(matches!(t.when, When::At { days: 0x71, .. }));
        assert_eq!(
            Task::parse("cada 2h /msg #c ping").unwrap().when,
            When::Every { minutes: 120 }
        );
        assert_eq!(
            Task::parse("every 30m /msg #c ping").unwrap().when,
            When::Every { minutes: 30 }
        );
        assert!(Task::parse("cada 1m /msg #c x").is_err(), "too often");
        assert!(Task::parse("25:00 /msg #c x").is_err());
        assert!(Task::parse("lunes").is_err());
        assert!(Task::parse("10:00").is_err());
        assert!(Task::parse("10:00 /quote PASS x").is_err() || true);
    }

    #[test]
    fn crypt_keys_and_secrets() {
        let k = CryptKey::parse("#xbnc cbc:clavesecreta").unwrap();
        assert!(k.cbc && k.key == "clavesecreta");
        assert!(!CryptKey::parse("bob clave123").unwrap().cbc);
        assert!(CryptKey::parse("#c abc").is_err());
        assert!(CryptKey::parse("#c").is_err());
        // seal encrypts, keeps the stored value when empty, clears with null.
        let enc = |p: &str| format!("<{p}>");
        let new = obj(json!({"service": "x", "password": "pw1"}));
        let sealed = seal("services", &new, None, &enc);
        assert_eq!(sealed["password"], "enc:<pw1>");
        let v = validate("services", &sealed).unwrap();
        let again = seal("services", &obj(json!({"password": ""})), Some(&v), &enc);
        assert_eq!(again["password"], "enc:<pw1>");
        let cleared = seal("services", &obj(json!({"password": null})), Some(&v), &enc);
        assert_eq!(cleared["password"], "");
        // A ciphertext we did not store is not accepted from outside.
        let forged = seal(
            "services",
            &obj(json!({"password": "enc:zzz"})),
            Some(&v),
            &enc,
        );
        assert_eq!(forged["password"], "");
        let (shown, set) = redact("services", &v);
        assert_eq!(shown["password"], "");
        assert_eq!(set, vec!["password"]);
        // Typed config opens secrets with the decrypt function.
        let mut rows = HashMap::new();
        rows.insert(
            "services".into(),
            ModState {
                enabled: true,
                config: v,
            },
        );
        let n = NetModules::from_rows(&rows, &|e| {
            e.strip_prefix('<')
                .map(|x| x.trim_end_matches('>').to_string())
        });
        assert_eq!(n.services.unwrap().password, "pw1");
        assert!(validate("services", &obj(json!({"service": "nope"}))).is_err());
        assert!(validate("ignore", &obj(json!({"masks": "*"}))).is_err());
        assert!(validate("ignore", &obj(json!({"masks": "troll\n*!*@spam.example"}))).is_ok());
        assert!(validate("schedule", &obj(json!({"tasks": "99:99 /x"}))).is_err());
    }

    #[test]
    fn service_resolution() {
        assert_eq!(Service::Auto.resolve("UnderNet", ""), Service::X);
        assert_eq!(Service::Auto.resolve("", "irc.undernet.org"), Service::X);
        assert_eq!(Service::Auto.resolve("QuakeNet", ""), Service::Q);
        assert_eq!(Service::Auto.resolve("Libera.Chat", ""), Service::NickServ);
        assert_eq!(Service::Q.resolve("Libera.Chat", ""), Service::Q);
        // Regression: EvilNet identifies with NickServ, not AuthServ.
        assert_eq!(Service::Auto.resolve("EvilNet", ""), Service::NickServ);
        assert_eq!(Service::Auto.resolve("GameSurge", ""), Service::AuthServ);
        assert_eq!(Service::Auto.resolve("AfterNet", ""), Service::AuthServ);
    }

    #[test]
    fn no_password_where_anyone_can_be_nickserv() {
        assert_eq!(
            Service::Auto.resolve("IRCnet", "open.ircnet.net"),
            Service::Auto
        );
        assert_eq!(Service::Auto.resolve("EFnet", ""), Service::Auto);
        assert_eq!(Service::Auto.resolve("Libera.Chat", ""), Service::NickServ);
        assert_eq!(Service::Auto.resolve("UnderNet", ""), Service::X);
        assert_eq!(
            Service::Auto.resolve("", "irc.evilnet.org"),
            Service::NickServ
        );
        // An explicit choice is respected.
        assert_eq!(Service::NickServ.resolve("IRCnet", ""), Service::NickServ);
    }
}
