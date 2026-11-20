//! Moving from another bouncer: reads its configuration file (the
//! `<User>` / `<Network>` / `<Chan>` block format used by the classic
//! bouncers), optionally its data directory (module data, client
//! certificates) and its plain-text logs, and creates the same users,
//! networks, channels and modules in xBNC.
//!
//! The import is two-step: [`parse`] builds a [`Plan`] that can be shown to
//! the administrator (nothing is written), and [`apply`] creates what the
//! plan describes. Existing users are never modified; their entries are
//! skipped and reported.

use crate::crypto::SecretBox;
use crate::db::{self, NewMessage, NewNetwork};
use crate::sql::Conn as Connection;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Largest configuration file accepted (they are a few KB in practice).
pub const MAX_CONFIG_BYTES: usize = 4 * 1024 * 1024;

// ------------------------------------------------------------ parsing

/// One `<Block name>` … `</Block>` section.
#[derive(Debug, Default, Clone)]
struct Block {
    kind: String,
    name: String,
    values: Vec<(String, String)>,
    children: Vec<Block>,
}

impl Block {
    fn get(&self, key: &str) -> Option<&str> {
        self.values
            .iter()
            .rev()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }
    fn all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.values
            .iter()
            .filter(move |(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }
    fn flag(&self, key: &str) -> Option<bool> {
        self.get(key).map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "true" | "yes" | "1" | "on"
            )
        })
    }
    fn children<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a Block> + 'a {
        self.children
            .iter()
            .filter(move |b| b.kind.eq_ignore_ascii_case(kind))
    }
}

fn parse_blocks(text: &str) -> Result<Block> {
    let mut stack = vec![Block {
        kind: "root".into(),
        ..Default::default()
    }];
    let mut in_comment = false;
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if in_comment {
            if line.ends_with("*/") {
                in_comment = false;
            }
            continue;
        }
        if line.starts_with("/*") {
            in_comment = !line.ends_with("*/");
            continue;
        }
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("</") {
            let kind = rest.trim_end_matches('>').trim();
            let b = stack.pop().filter(|_| !stack.is_empty());
            match b {
                Some(b) if b.kind.eq_ignore_ascii_case(kind) => {
                    stack.last_mut().expect("parent").children.push(b)
                }
                _ => bail!("line {}: unexpected </{kind}>", n + 1),
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix('<') {
            let inner = rest
                .strip_suffix('>')
                .with_context(|| format!("line {}: unterminated <", n + 1))?;
            let (kind, name) = inner.split_once(char::is_whitespace).unwrap_or((inner, ""));
            if stack.len() > 8 {
                bail!("line {}: blocks nested too deeply", n + 1);
            }
            stack.push(Block {
                kind: kind.trim().to_string(),
                name: name.trim().to_string(),
                ..Default::default()
            });
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            stack
                .last_mut()
                .expect("stack")
                .values
                .push((k.trim().to_string(), v.trim().to_string()));
        }
        // Anything else is ignored, like the original parser does.
    }
    if stack.len() != 1 {
        bail!(
            "the file ends inside <{}>",
            stack.last().map(|b| b.kind.as_str()).unwrap_or("")
        );
    }
    Ok(stack.pop().expect("root"))
}

// ------------------------------------------------------------ the plan

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub users: Vec<PlanUser>,
    /// Things that will not be carried over, in English (shown in the
    /// panel and on the command line).
    pub warnings: Vec<Note>,
}

/// A remark about the import: a stable code and its arguments (so the
/// panel can translate it) plus the English text (for the CLI and logs).
#[derive(Debug, Clone, Serialize)]
pub struct Note {
    pub code: &'static str,
    pub args: Vec<String>,
    pub text: String,
}

fn note(code: &'static str, args: &[&str], text: String) -> Note {
    Note {
        code,
        args: args.iter().map(|a| a.to_string()).collect(),
        text,
    }
}

impl std::fmt::Display for Note {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanUser {
    pub name: String,
    /// admin or user (superadmin is never granted by an import).
    pub role: String,
    /// keep: the old password still works (re-hashed at first login);
    /// reset: a new random password is generated.
    pub password: &'static str,
    #[serde(skip)]
    pub password_hash: Option<String>,
    pub networks: Vec<PlanNetwork>,
    /// Set when the user cannot be imported (exists already, bad name...).
    pub skip: Option<Note>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanNetwork {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub tls: bool,
    pub nick: String,
    pub username: String,
    pub realname: String,
    pub enabled: bool,
    pub channels: Vec<String>,
    /// xBNC modules that will be enabled.
    pub modules: Vec<String>,
    pub on_connect_lines: usize,
    pub sasl: bool,
    pub client_cert: bool,
    #[serde(skip)]
    pub secret: NetSecrets,
    #[serde(skip)]
    pub module_cfg: Vec<(String, Map<String, Value>)>,
    pub skip: Option<Note>,
}

#[derive(Debug, Clone, Default)]
pub struct NetSecrets {
    server_password: Option<String>,
    channel_keys: BTreeMap<String, String>,
    sasl: Option<(String, String)>,
    on_connect: Vec<String>,
    cert: Option<(String, String)>,
}

/// Modules of the other bouncer that xBNC does natively, with no switch.
const BUILT_IN: &[&str] = &[
    "chansaver",
    "savebuff",
    "playback",
    "clientbuffer",
    "webadmin",
    "controlpanel",
    "fail2ban",
    "lastseen",
    "notes",
    "clearbufferonmsg",
    "simple_disconnect",
    "missingmotd",
    "certauth",
    "listsockets",
    "identfile",
    "partyline",
];

/// Module of the other bouncer → xBNC module.
fn module_equivalent(name: &str) -> Option<&'static str> {
    Some(match name {
        "keepnick" => "keepnick",
        "simple_away" | "away" | "autoaway" | "awaynick" => "away",
        "autoop" => "autoop",
        "kickrejoin" => "kickrejoin",
        "stickychan" => "stickychan",
        "buffextras" => "events",
        "log" => "log",
        "route_replies" => "route_replies",
        "cert" => "cert",
        "alias" => "alias",
        "watch" => "watch",
        "block_motd" => "block_motd",
        "ctcpreplies" => "ctcp",
        _ => return None,
    })
}

/// Reads a module data file: one `key value` per line, both URL-escaped.
fn read_registry(path: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return out;
    };
    for line in text.lines().take(10_000) {
        let (k, v) = line.split_once(' ').unwrap_or((line, ""));
        out.insert(unescape(k), unescape(v));
    }
    out
}

fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16)
                {
                    Ok(v) => {
                        out.push(v);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Turns a stored password line into something xBNC can verify, or `None`
/// when the user will need a new password.
fn convert_password(user: &Block) -> Option<String> {
    let (method, hash, salt) = if let Some(p) = user.children("Pass").next() {
        (
            p.get("Method").unwrap_or("").to_ascii_lowercase(),
            p.get("Hash").unwrap_or("").to_string(),
            p.get("Salt").unwrap_or("").to_string(),
        )
    } else {
        let v = user.get("Pass")?;
        let parts: Vec<&str> = v.split('#').collect();
        match parts.as_slice() {
            [m, h, s, ..] => (m.to_ascii_lowercase(), h.to_string(), s.to_string()),
            [m, h] => (m.to_ascii_lowercase(), h.to_string(), String::new()),
            [h] => ("plain".to_string(), h.to_string(), String::new()),
            [] => return None,
        }
    };
    match method.as_str() {
        "sha256" if hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit()) => {
            crate::crypto::legacy_sha256_hash(&hash, &salt)
        }
        "argon2id" | "argon2" if hash.starts_with("$argon2") => Some(hash),
        "plain" if !hash.is_empty() => crate::crypto::hash_password(&hash).ok(),
        _ => None,
    }
}

fn nick_ok(n: &str) -> bool {
    db::validate_network_fields("x", "h", 1, n, "u", "").is_ok()
}

/// Builds the import plan. `data_dir` is the other bouncer's data directory
/// (with `users/<name>/...`), when available, for module data and
/// certificates. `conn` is used to detect users that already exist.
pub fn parse(config: &str, data_dir: Option<&Path>, conn: &Connection) -> Result<Plan> {
    if config.len() > MAX_CONFIG_BYTES {
        bail!("the configuration file is too large");
    }
    let root = parse_blocks(config)?;
    let mut warnings = Vec::new();
    let mut users = Vec::new();
    if root.children("User").next().is_none() {
        bail!("no <User> sections found: this does not look like a bouncer configuration file");
    }
    for u in root.children("User") {
        let name = u.name.clone();
        let mut skip = None;
        if db::validate_name("username", &name).is_err() {
            skip = Some(note(
                "user_name_invalid",
                &[&name],
                format!(
                    "the name {name:?} is not valid in xBNC (1-32 characters of A-Z a-z 0-9 . _ -)"
                ),
            ));
        } else if db::get_user(conn, &name)?.is_some() {
            skip = Some(note(
                "user_exists",
                &[&name],
                format!("a user called {name} already exists in xBNC"),
            ));
        }
        let password_hash = convert_password(u);
        let user_dir = data_dir.map(|d| d.join("users").join(&name));
        let base_nick = u
            .get("Nick")
            .filter(|n| !n.is_empty())
            .unwrap_or(&name)
            .to_string();
        let base_ident = u
            .get("Ident")
            .filter(|n| !n.is_empty())
            .unwrap_or(&name)
            .to_string();
        let base_real = u.get("RealName").unwrap_or("").to_string();
        let user_mods: Vec<(String, String)> = u
            .all("LoadModule")
            .map(|m| {
                let (n, a) = m.split_once(' ').unwrap_or((m, ""));
                (n.to_ascii_lowercase(), a.trim().to_string())
            })
            .collect();
        let mut nets = Vec::new();
        for n in u.children("Network") {
            let net_dir = user_dir.as_ref().map(|d| d.join("networks").join(&n.name));
            nets.push(plan_network(
                n,
                &name,
                &base_nick,
                &base_ident,
                &base_real,
                &user_mods,
                user_dir.as_deref(),
                net_dir.as_deref(),
                &mut warnings,
            ));
        }
        for (m, _) in &user_mods {
            if module_equivalent(m).is_none() && !BUILT_IN.contains(&m.as_str()) {
                warnings.push(note(
                    "module_unsupported",
                    &[&name, m],
                    format!("{name}: module {m} has no equivalent in xBNC"),
                ));
            }
        }
        if password_hash.is_none() && skip.is_none() {
            warnings.push(note(
                "password_reset",
                &[&name],
                format!("{name}: the stored password uses a format xBNC cannot check; a new password will be generated"),
            ));
        }
        users.push(PlanUser {
            role: if u.flag("Admin") == Some(true) {
                "admin".into()
            } else {
                "user".into()
            },
            password: if password_hash.is_some() {
                "keep"
            } else {
                "reset"
            },
            password_hash,
            networks: nets,
            skip,
            name,
        });
    }
    if data_dir.is_none() {
        warnings.push(note(
            "config_only",
            &[],
            "only the configuration file was read: SASL, commands on connect, NickServ passwords and certificates are kept in the data directory; import from the server with `xbncd import` to bring them too".into(),
        ));
    }
    Ok(Plan { users, warnings })
}

#[allow(clippy::too_many_arguments)]
fn plan_network(
    n: &Block,
    user: &str,
    base_nick: &str,
    base_ident: &str,
    base_real: &str,
    user_mods: &[(String, String)],
    user_dir: Option<&Path>,
    net_dir: Option<&Path>,
    warnings: &mut Vec<Note>,
) -> PlanNetwork {
    let label = format!("{user}/{}", n.name);
    let mut skip = None;
    if db::validate_name("network name", &n.name).is_err() {
        skip = Some(note(
            "network_name_invalid",
            &[&n.name],
            format!("the network name {:?} is not valid in xBNC", n.name),
        ));
    }
    // Server lines: "host [+]port [password]"; the first usable one wins.
    let mut host = String::new();
    let mut port = 6697u16;
    let mut tls = true;
    let mut server_password = None;
    let servers: Vec<&str> = n.all("Server").collect();
    for s in &servers {
        let mut it = s.split_whitespace();
        let h = it.next().unwrap_or("");
        let p = it.next().unwrap_or("6667");
        let (t, p) = match p.strip_prefix('+') {
            Some(p) => (true, p),
            None => (false, p),
        };
        let Ok(p) = p.parse::<u16>() else { continue };
        // IPv6 literals come with or without brackets.
        let h = h.trim_start_matches('[').trim_end_matches(']');
        if h.is_empty() || (h.contains(':') && h.parse::<std::net::Ipv6Addr>().is_err()) {
            continue;
        }
        host = h.to_string();
        port = p;
        tls = t;
        server_password = it.next().map(str::to_string).filter(|s| !s.is_empty());
        break;
    }
    if host.is_empty() && skip.is_none() {
        skip = Some(note("no_server", &[], "no usable server".into()));
    }
    if servers.len() > 1 {
        warnings.push(note(
            "many_servers",
            &[&label, &servers.len().to_string(), &host],
            format!(
                "{label}: {} servers listed; only {host} is used (xBNC keeps one address per network)",
                servers.len()
            ),
        ));
    }
    if !tls && skip.is_none() {
        warnings.push(note(
            "no_tls",
            &[&label, &format!("{host}:{port}")],
            format!("{label}: connects without TLS ({host}:{port})"),
        ));
    }
    let mut nick = n
        .get("Nick")
        .filter(|v| !v.is_empty())
        .unwrap_or(base_nick)
        .to_string();
    if !nick_ok(&nick) {
        warnings.push(note(
            "nick_invalid",
            &[&label, &nick, user],
            format!("{label}: nick {nick:?} is not valid, using {user}"),
        ));
        nick = user.to_string();
    }
    let username = n
        .get("Ident")
        .filter(|v| !v.is_empty())
        .unwrap_or(base_ident)
        .to_string();
    let realname: String = n
        .get("RealName")
        .filter(|v| !v.is_empty())
        .unwrap_or(base_real)
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect();
    let enabled = n.flag("IRCConnectEnabled").unwrap_or(true);

    let mut secret = NetSecrets {
        server_password,
        ..Default::default()
    };
    let mut channels = Vec::new();
    for c in n.children("Chan") {
        let name = c.name.clone();
        if !name.starts_with(['#', '&', '!', '+']) || name.contains([' ', ',', '\x07']) {
            warnings.push(note(
                "channel_invalid",
                &[&label, &name],
                format!("{label}: channel {name:?} skipped (invalid name)"),
            ));
            continue;
        }
        if c.flag("Disabled") == Some(true) {
            warnings.push(note(
                "channel_disabled",
                &[&label, &name],
                format!("{label}: channel {name} was disabled and is not imported"),
            ));
            continue;
        }
        if let Some(k) = c.get("Key").filter(|k| !k.is_empty()) {
            secret.channel_keys.insert(name.clone(), k.to_string());
        }
        channels.push(name);
    }
    if channels.len() > 500 {
        warnings.push(note(
            "too_many_channels",
            &[&label],
            format!("{label}: only the first 500 channels are imported"),
        ));
        channels.truncate(500);
    }

    // Modules: network-level ones, plus user-level ones that work per network.
    let mut wanted: Vec<(String, String)> = n
        .all("LoadModule")
        .map(|m| {
            let (a, b) = m.split_once(' ').unwrap_or((m, ""));
            (a.to_ascii_lowercase(), b.trim().to_string())
        })
        .collect();
    wanted.extend(user_mods.iter().cloned());
    let reg = |m: &str| -> BTreeMap<String, String> {
        let mut r = BTreeMap::new();
        for d in [user_dir, net_dir].into_iter().flatten() {
            r.extend(read_registry(&d.join("moddata").join(m).join(".registry")));
        }
        r
    };
    let mut modules: Vec<String> = Vec::new();
    let mut module_cfg = Vec::new();
    for (m, args) in &wanted {
        match m.as_str() {
            "perform" => {
                let r = reg("perform");
                if let Some(p) = r.get("Perform") {
                    for l in p.lines().map(str::trim).filter(|l| !l.is_empty()) {
                        let l = l.strip_prefix('/').unwrap_or(l);
                        secret.on_connect.push(l.to_string());
                    }
                }
            }
            "sasl" => {
                let r = reg("sasl");
                if let (Some(u), Some(p)) = (r.get("username"), r.get("password")) {
                    if !u.is_empty() && !p.is_empty() {
                        secret.sasl = Some((u.clone(), p.clone()));
                    }
                }
            }
            "nickserv" => {
                let r = reg("nickserv");
                if let Some(p) = r.get("Password").filter(|p| !p.is_empty()) {
                    secret
                        .on_connect
                        .insert(0, format!("PRIVMSG NickServ :IDENTIFY {p}"));
                }
            }
            _ => {}
        }
        let Some(x) = module_equivalent(m) else {
            if !BUILT_IN.contains(&m.as_str())
                && !matches!(m.as_str(), "perform" | "sasl" | "nickserv")
                && !user_mods.iter().any(|(u, _)| u == m)
            {
                warnings.push(note(
                    "module_unsupported",
                    &[&label, m],
                    format!("{label}: module {m} has no equivalent in xBNC"),
                ));
            }
            continue;
        };
        if modules.iter().any(|e| e == x) {
            continue;
        }
        let mut cfg = Map::new();
        match x {
            "away" => {
                // simple_away [-notimer | -timer N] [reason]
                let mut it = args.split_whitespace().peekable();
                let mut reason = Vec::new();
                while let Some(w) = it.next() {
                    match w {
                        "-timer" => {
                            if let Some(n) = it.next().and_then(|n| n.parse::<i64>().ok()) {
                                cfg.insert("delay".into(), Value::from(n.clamp(0, 3600)));
                            }
                        }
                        "-notimer" => {
                            cfg.insert("delay".into(), Value::from(0));
                        }
                        w => reason.push(w),
                    }
                }
                let r: String = reason.join(" ").chars().take(200).collect();
                if !r.is_empty() && !r.contains('%') {
                    cfg.insert("message".into(), Value::String(r));
                }
            }
            "kickrejoin" => {
                if let Ok(n) = args.trim().parse::<i64>() {
                    cfg.insert("delay".into(), Value::from(n.clamp(0, 600)));
                }
            }
            "stickychan" => {
                let chans: Vec<String> = reg("stickychan")
                    .keys()
                    .filter(|c| c.starts_with(['#', '&']))
                    .take(100)
                    .cloned()
                    .collect();
                cfg.insert("channels".into(), Value::String(chans.join("\n")));
            }
            "ctcp" => {
                let r = reg("ctcpreplies");
                if let Some(v) = r.get("VERSION") {
                    let v: String = v.chars().filter(|c| !c.is_control()).take(200).collect();
                    cfg.insert("version".into(), Value::String(v));
                }
            }
            "cert" => {
                let pem = [net_dir, user_dir]
                    .into_iter()
                    .flatten()
                    .map(|d| d.join("moddata").join("cert").join("user.pem"))
                    .find_map(|p| std::fs::read_to_string(p).ok());
                if let Some(pem) = pem {
                    if let Some((c, k)) = split_pem(&pem) {
                        if crate::certs::parse(&c, &k).is_ok() {
                            secret.cert = Some((c, k));
                        }
                    }
                }
                if secret.cert.is_none() {
                    // Without a certificate the module would do nothing.
                    warnings.push(note(
                        "cert_missing",
                        &[&label],
                        format!("{label}: the client certificate was not found; generate or upload one in the panel and enable the module"),
                    ));
                    continue;
                }
            }
            "alias" | "watch" | "autoop" => {
                warnings.push(note(
                    "module_entries",
                    &[&label, x],
                    format!("{label}: module {x} is enabled but its entries use a different syntax; add them again in the Modules tab"),
                ));
            }
            _ => {}
        }
        match crate::modules::validate(x, &cfg) {
            Ok(v) => {
                modules.push(x.to_string());
                module_cfg.push((x.to_string(), v));
            }
            Err(e) => warnings.push(note(
                "module_invalid",
                &[&label, x, &e.to_string()],
                format!("{label}: module {x}: {e}"),
            )),
        }
    }
    if secret.on_connect.len() > 50 {
        secret.on_connect.truncate(50);
        warnings.push(note(
            "too_many_commands",
            &[&label],
            format!("{label}: only the first 50 commands on connect are kept"),
        ));
    }
    if skip.is_none() {
        if let Err(e) =
            db::validate_network_fields(&n.name, &host, port, &nick, &username, &realname)
        {
            skip = Some(note("network_invalid", &[&e.to_string()], e.to_string()));
        }
    }
    PlanNetwork {
        name: n.name.clone(),
        host,
        port,
        tls,
        nick,
        username,
        realname,
        enabled,
        channels,
        modules,
        on_connect_lines: secret.on_connect.len(),
        sasl: secret.sasl.is_some(),
        client_cert: secret.cert.is_some(),
        secret,
        module_cfg,
        skip,
    }
}

/// Splits a PEM file holding a private key and a certificate.
fn split_pem(pem: &str) -> Option<(String, String)> {
    let mut cert = String::new();
    let mut key = String::new();
    let mut cur: Option<&mut String> = None;
    for line in pem.lines() {
        if line.starts_with("-----BEGIN ") {
            cur = if line.contains("PRIVATE KEY") {
                Some(&mut key)
            } else if line.contains("CERTIFICATE") {
                Some(&mut cert)
            } else {
                None
            };
        }
        if let Some(c) = cur.as_deref_mut() {
            c.push_str(line);
            c.push('\n');
        }
        if line.starts_with("-----END ") {
            cur = None;
        }
    }
    (!cert.is_empty() && !key.is_empty()).then_some((cert, key))
}

// ------------------------------------------------------------ applying

#[derive(Debug, Clone, Serialize, Default)]
pub struct Report {
    pub users_created: Vec<String>,
    pub networks_created: usize,
    pub channels_created: usize,
    /// Users whose password could not be kept: (user, new password).
    /// Shown once to the administrator.
    pub new_passwords: Vec<(String, String)>,
    /// Users and user/network pairs that were not created (the preview
    /// says why).
    pub skipped: Vec<String>,
}

/// Creates everything in the plan inside one transaction.
pub fn apply(
    conn: &mut Connection,
    plan: &Plan,
    secrets: &SecretBox,
    max_networks: usize,
) -> Result<Report> {
    let tx = conn.transaction()?;
    let mut rep = Report::default();
    let cm = xbnc_proto::CaseMapping::Rfc1459;
    for u in &plan.users {
        if u.skip.is_some() {
            rep.skipped.push(u.name.clone());
            continue;
        }
        if db::get_user(&tx, &u.name)?.is_some() {
            rep.skipped.push(u.name.clone());
            continue;
        }
        let hash = match &u.password_hash {
            Some(h) => h.clone(),
            None => {
                let pw = generate_password();
                let h = crate::crypto::hash_password(&pw)?;
                rep.new_passwords.push((u.name.clone(), pw));
                h
            }
        };
        let role = if u.role == "admin" { "admin" } else { "user" };
        let uid = db::create_user_with_role(&tx, &u.name, &hash, role)?;
        rep.users_created.push(u.name.clone());
        for n in &u.networks {
            if n.skip.is_some() {
                rep.skipped.push(format!("{}/{}", u.name, n.name));
                continue;
            }
            let s = &n.secret;
            let nn = NewNetwork {
                name: n.name.clone(),
                host: n.host.clone(),
                port: n.port,
                tls: n.tls,
                nick: n.nick.clone(),
                username: n.username.clone(),
                realname: n.realname.clone(),
                server_password_enc: s.server_password.as_deref().map(|p| secrets.encrypt(p)),
                sasl_username: s.sasl.as_ref().map(|(u, _)| u.clone()),
                sasl_password_enc: s.sasl.as_ref().map(|(_, p)| secrets.encrypt(p)),
            };
            let nid = match db::create_network(&tx, uid, &nn, max_networks) {
                Ok(id) => id,
                Err(e) => {
                    tracing::warn!(user = %u.name, network = %n.name, error = %e, "import: network not created");
                    rep.skipped.push(format!("{}/{}", u.name, n.name));
                    continue;
                }
            };
            rep.networks_created += 1;
            if !n.enabled {
                db::set_network_enabled(&tx, uid, &n.name, false)?;
            }
            for c in &n.channels {
                let key = s.channel_keys.get(c).map(|k| secrets.encrypt(k));
                db::upsert_channel(&tx, nid, c, &cm.fold(c), key.as_deref(), false)?;
                rep.channels_created += 1;
            }
            for (m, cfg) in &n.module_cfg {
                db::set_module(
                    &tx,
                    nid,
                    m,
                    &crate::modules::ModState {
                        enabled: true,
                        config: cfg.clone(),
                    },
                )?;
            }
            if !s.on_connect.is_empty() {
                let enc = secrets.encrypt(&s.on_connect.join("\n"));
                db::set_on_connect(&tx, nid, Some(&enc))?;
            }
            if let Some((c, k)) = &s.cert {
                let k = secrets.encrypt(k);
                db::set_client_cert(&tx, nid, Some((c, &k)))?;
            }
        }
    }
    tx.commit()?;
    Ok(rep)
}

fn generate_password() -> String {
    use chacha20poly1305::aead::rand_core::RngCore;
    use chacha20poly1305::aead::OsRng;
    const ALPHA: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut b = [0u8; 16];
    OsRng.fill_bytes(&mut b);
    b.iter()
        .map(|x| ALPHA[*x as usize % ALPHA.len()] as char)
        .collect()
}

/// Finds the configuration file in a data directory (`configs/*.conf`), or
/// accepts a path to the file itself.
pub fn locate_config(path: &Path) -> Result<(PathBuf, Option<PathBuf>)> {
    if path.is_file() {
        // <data>/configs/<file>.conf → data dir is two levels up.
        let data = path
            .parent()
            .filter(|p| p.file_name().is_some_and(|n| n == "configs"))
            .and_then(Path::parent)
            .filter(|d| d.join("users").is_dir())
            .map(Path::to_path_buf);
        return Ok((path.to_path_buf(), data));
    }
    let dir = path.join("configs");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
        .with_context(|| format!("cannot read {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "conf"))
        .collect();
    found.sort();
    match found.len() {
        1 => Ok((found.remove(0), Some(path.to_path_buf()))),
        0 => bail!("no .conf file in {}", dir.display()),
        _ => bail!(
            "several .conf files in {}; pass the one to import",
            dir.display()
        ),
    }
}

// ------------------------------------------------------------ logs

#[derive(Debug, Clone, Serialize, Default)]
pub struct LogReport {
    pub files: usize,
    pub messages: usize,
    pub duplicates: usize,
    pub unrecognised: usize,
}

/// Directories that may hold the logs of one network, oldest layout first.
pub fn log_dirs(data_dir: &Path, user: &str, network: &str) -> Vec<PathBuf> {
    [
        data_dir.join("moddata/log").join(user).join(network),
        data_dir
            .join("users")
            .join(user)
            .join("moddata/log")
            .join(network),
        data_dir
            .join("users")
            .join(user)
            .join("networks")
            .join(network)
            .join("moddata/log"),
    ]
    .into_iter()
    .filter(|p| p.is_dir())
    .collect()
}

/// One parsed log line (time is seconds into the day).
#[derive(Debug, PartialEq)]
struct LogLine {
    secs: i64,
    sender: String,
    command: &'static str,
    text: String,
}

fn parse_log_line(line: &str) -> Option<LogLine> {
    let rest = line.strip_prefix('[')?;
    let (hms, rest) = rest.split_once("] ")?;
    let mut it = hms.split(':').map(|x| x.parse::<i64>().ok());
    let (h, m, s) = (it.next()??, it.next()??, it.next()??);
    if !(0..24).contains(&h) || !(0..60).contains(&m) || !(0..61).contains(&s) {
        return None;
    }
    let secs = h * 3600 + m * 60 + s;
    let ll = |sender: &str, command, text: String| {
        Some(LogLine {
            secs,
            sender: sender.to_string(),
            command,
            text,
        })
    };
    if let Some(r) = rest.strip_prefix('<') {
        let (nick, text) = r.split_once("> ")?;
        return ll(nick, "PRIVMSG", text.to_string());
    }
    if let Some(r) = rest.strip_prefix("*** ") {
        // Events, in the classic wording.
        let mask = |s: &str| -> Option<(String, String)> {
            // "nick (ident@host)" [rest]
            let (nick, r) = s.split_once(" (")?;
            let (uh, r) = r.split_once(')')?;
            Some((format!("{nick}!{uh}"), r.trim().to_string()))
        };
        let strip_parens = |s: String| -> String {
            s.strip_prefix('(')
                .and_then(|x| x.strip_suffix(')'))
                .map(str::to_string)
                .unwrap_or(s)
        };
        if let Some(x) = r.strip_prefix("Joins: ") {
            let (who, _) = mask(x)?;
            return ll(&who, "JOIN", String::new());
        }
        if let Some(x) = r.strip_prefix("Parts: ") {
            let (who, why) = mask(x)?;
            return ll(&who, "PART", strip_parens(why));
        }
        if let Some(x) = r.strip_prefix("Quits: ") {
            let (who, why) = mask(x)?;
            return ll(&who, "QUIT", strip_parens(why));
        }
        if let Some((victim, x)) = r.split_once(" was kicked by ") {
            let (op, why) = x.split_once(' ').unwrap_or((x, ""));
            return ll(
                op,
                "KICK",
                format!("{victim} {}", strip_parens(why.to_string()))
                    .trim_end()
                    .to_string(),
            );
        }
        if let Some((old, new)) = r.split_once(" is now known as ") {
            return ll(old, "NICK", new.to_string());
        }
        if let Some((who, t)) = r.split_once(" changes topic to ") {
            let t = t
                .strip_prefix('\'')
                .and_then(|x| x.strip_suffix('\''))
                .unwrap_or(t);
            return ll(who, "TOPIC", t.to_string());
        }
        if let Some((who, m)) = r.split_once(" sets mode: ") {
            return ll(who, "MODE", m.to_string());
        }
        return None;
    }
    if let Some(r) = rest.strip_prefix("* ") {
        let (nick, text) = r.split_once(' ').unwrap_or((r, ""));
        return ll(nick, "PRIVMSG", format!("\x01ACTION {text}\x01"));
    }
    if let Some(r) = rest.strip_prefix('-') {
        let (nick, text) = r.split_once("- ")?;
        return ll(nick, "NOTICE", text.to_string());
    }
    None
}

/// Time zone of imported logs.
#[derive(Debug, Clone)]
pub enum LogZone {
    /// Minutes east of UTC, the same all year.
    Offset(i32),
    /// A named time zone: each day uses the offset in force that day.
    Zone(crate::tz::Zone),
}

impl From<i32> for LogZone {
    fn from(m: i32) -> Self {
        LogZone::Offset(m)
    }
}

impl LogZone {
    /// `+02:00`, `-0500` or an IANA name like `Europe/Madrid`.
    pub fn parse(s: &str) -> Result<LogZone> {
        let t = s.trim();
        if t.contains('/') || t.eq_ignore_ascii_case("utc") {
            return crate::tz::Zone::parse(t)
                .map(LogZone::Zone)
                .map_err(|e| anyhow::anyhow!(e));
        }
        Ok(LogZone::Offset(crate::util::parse_utc_offset(t)?))
    }

    /// UTC milliseconds of a local wall-clock time given as seconds since
    /// 1970 as if it were UTC.
    fn to_utc_ms(&self, local_secs: i64) -> i64 {
        match self {
            LogZone::Offset(m) => (local_secs - *m as i64 * 60) * 1000,
            LogZone::Zone(z) => {
                let t = time::OffsetDateTime::from_unix_timestamp(local_secs)
                    .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
                z.to_utc_ms(
                    t.year(),
                    t.month() as u8,
                    t.day(),
                    t.hour(),
                    t.minute(),
                    t.second(),
                )
                .unwrap_or(local_secs * 1000)
            }
        }
    }
}

/// Imports the `YYYY-MM-DD.log` files found under `dirs` (one folder per
/// channel or query) into a network's history. Times are converted with
/// `zone` (a fixed offset or a time zone with daylight saving time).
/// Messages already present (same conversation,
/// time, sender, kind and text) are skipped, so the import can be repeated.
pub fn import_logs(
    conn: &mut Connection,
    network_id: i64,
    own_nick: &str,
    dirs: &[PathBuf],
    zone: impl Into<LogZone>,
) -> Result<LogReport> {
    let zone: LogZone = zone.into();
    let cm = xbnc_proto::CaseMapping::Rfc1459;
    let own = cm.fold(own_nick);
    let mut rep = LogReport::default();
    let fmt = time::macros::format_description!("[year]-[month]-[day]");
    for dir in dirs {
        let mut targets: Vec<PathBuf> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        targets.sort();
        for tdir in targets {
            let Some(target) = tdir
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_string)
            else {
                continue;
            };
            if target.is_empty() || target.len() > 64 || target.contains([' ', ',', '\0']) {
                continue;
            }
            let mut files: Vec<PathBuf> = std::fs::read_dir(&tdir)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e == "log"))
                .collect();
            files.sort();
            for f in files {
                let Some(day) = f
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| time::Date::parse(s, &fmt).ok())
                else {
                    continue;
                };
                let midnight = day.midnight().assume_utc().unix_timestamp();
                let text = String::from_utf8_lossy(&std::fs::read(&f)?).into_owned();
                rep.files += 1;
                let tx = conn.transaction()?;
                {
                    let mut exists = tx.prepare(
                        "SELECT 1 FROM messages WHERE network_id = ?1 AND target_cf = ?2 AND ts = ?3 \
                         AND sender = ?4 AND command = ?5 AND text = ?6 LIMIT 1",
                    )?;
                    let mut prev = -1i64;
                    let mut seq = 0i64;
                    for line in text.lines() {
                        let Some(l) = parse_log_line(line.trim_end_matches('\r')) else {
                            if !line.trim().is_empty() {
                                rep.unrecognised += 1;
                            }
                            continue;
                        };
                        // Keep the order of lines logged in the same second.
                        seq = if l.secs == prev { seq + 1 } else { 0 };
                        prev = l.secs;
                        let ts = zone.to_utc_ms(midnight + l.secs) + seq.min(999);
                        let target_cf = cm.fold(&target);
                        let nick = l.sender.split('!').next().unwrap_or("");
                        let outgoing =
                            matches!(l.command, "PRIVMSG" | "NOTICE") && cm.fold(nick) == own;
                        let dup = exists
                            .query_row(
                                crate::params![
                                    network_id, target_cf, ts, l.sender, l.command, l.text
                                ],
                                |_| Ok(()),
                            )
                            .is_ok();
                        if dup {
                            rep.duplicates += 1;
                            continue;
                        }
                        db::insert_message(
                            &tx,
                            &NewMessage {
                                network_id,
                                target: target.clone(),
                                target_cf,
                                ts,
                                sender: l.sender,
                                command: l.command.to_string(),
                                text: l.text,
                                outgoing,
                                mention: false,
                            },
                        )?;
                        rep.messages += 1;
                    }
                }
                tx.commit()?;
            }
        }
    }
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
// generated configuration
Version = 1.8.2
<Listener l>
	Port = 6697
	SSL = true
</Listener>
<User alice>
	Pass = sha256#5baf7bc4af0e74a5155fa16eea25573bad825c3fb5d34169871a226d9fc18cde#somesalt#
	Admin = true
	Nick = alice
	AltNick = alice_
	Ident = al
	RealName = Alice Example
	LoadModule = chansaver
	LoadModule = controlpanel
	LoadModule = something_odd
	<Network libera>
		LoadModule = simple_away -timer 30 Gone fishing
		LoadModule = keepnick
		LoadModule = buffextras
		LoadModule = sasl
		LoadModule = perform
		Server = irc.libera.chat +6697
		Server = irc.eu.libera.chat +6697
		<Chan #xbnc-dev>
		</Chan>
		<Chan #secret>
			Key = hunter2
		</Chan>
		<Chan #old>
			Disabled = true
		</Chan>
	</Network>
	<Network bad name>
		Server = x.example 6667
	</Network>
</User>
<User bob>
	<Pass password>
		Method = md5
		Hash = 00000000000000000000000000000000
		Salt = x
	</Pass>
	<Network efnet>
		Nick = bobby
		IRCConnectEnabled = false
		Server = irc.efnet.org 6667 srvpass
	</Network>
</User>
"#;

    /// Runs a test body against a fresh in-memory database.
    fn with_db(f: impl FnOnce(&mut Connection)) {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|c| {
            f(c);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn plan_from_config() {
        with_db(|conn| {
            let p = parse(SAMPLE, None, conn).unwrap();
            assert_eq!(p.users.len(), 2);
            let a = &p.users[0];
            assert_eq!(
                (a.name.as_str(), a.role.as_str(), a.password),
                ("alice", "admin", "keep")
            );
            let n = &a.networks[0];
            assert_eq!(
                (n.host.as_str(), n.port, n.tls),
                ("irc.libera.chat", 6697, true)
            );
            assert_eq!((n.nick.as_str(), n.username.as_str()), ("alice", "al"));
            assert_eq!(n.channels, vec!["#xbnc-dev", "#secret"]);
            assert_eq!(n.secret.channel_keys["#secret"], "hunter2");
            assert_eq!(n.modules, vec!["away", "keepnick", "events"]);
            let away = &n.module_cfg[0].1;
            assert_eq!(away["delay"], 30);
            assert_eq!(away["message"], "Gone fishing");
            assert!(a.networks[1].skip.is_some());
            let b = &p.users[1];
            assert_eq!(b.password, "reset");
            let e = &b.networks[0];
            assert!(!e.enabled && !e.tls);
            assert_eq!(e.nick, "bobby");
            assert_eq!(e.secret.server_password.as_deref(), Some("srvpass"));
            let w = p
                .warnings
                .iter()
                .map(|n| n.text.clone())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(w.contains("something_odd"), "{w}");
            assert!(w.contains("#old"), "{w}");
            assert!(w.contains("2 servers"), "{w}");
            assert!(
                !w.contains("chansaver") && !w.contains("controlpanel"),
                "{w}"
            );
        });
    }

    #[test]
    fn apply_creates_everything_once() {
        with_db(|conn| {
            let sb = SecretBox::from_key(&[7u8; 32]);
            let p = parse(SAMPLE, None, conn).unwrap();
            let r = apply(conn, &p, &sb, 10).unwrap();
            assert_eq!(r.users_created, vec!["alice", "bob"]);
            assert_eq!(r.networks_created, 2);
            assert_eq!(r.channels_created, 2);
            assert_eq!(r.new_passwords.len(), 1);
            assert_eq!(r.new_passwords[0].0, "bob");
            let alice = db::get_user(conn, "alice").unwrap().unwrap();
            assert_eq!(alice.role, "admin");
            // sha256(password + salt) with password "test" and salt "somesalt".
            assert!(crate::crypto::verify_password(&alice.password_hash, "test"));
            assert!(!crate::crypto::verify_password(
                &alice.password_hash,
                "wrong"
            ));
            let bob = db::get_user(conn, "bob").unwrap().unwrap();
            assert!(crate::crypto::verify_password(
                &bob.password_hash,
                &r.new_passwords[0].1
            ));
            let net = db::get_network(conn, alice.id, "libera").unwrap().unwrap();
            let chans = db::list_channels(conn, net.id).unwrap();
            let key = chans
                .iter()
                .find(|c| c.name == "#secret")
                .unwrap()
                .key_enc
                .clone()
                .unwrap();
            assert_eq!(sb.decrypt(&key).unwrap().expose(), "hunter2");
            let mods = db::module_rows(conn, net.id).unwrap();
            assert!(mods["away"].enabled);
            // A second run skips existing users and changes nothing.
            let p2 = parse(SAMPLE, None, conn).unwrap();
            assert!(p2.users.iter().all(|u| u.skip.is_some()));
            let r2 = apply(conn, &p2, &sb, 10).unwrap();
            assert!(r2.users_created.is_empty() && r2.skipped.len() == 2);
        });
    }

    #[test]
    fn data_directory_brings_module_data() {
        with_db(|conn| {
            let dir = tempfile::tempdir().unwrap();
            let net = dir.path().join("users/alice/networks/libera/moddata");
            std::fs::create_dir_all(net.join("sasl")).unwrap();
            std::fs::write(
                net.join("sasl/.registry"),
                "username alice\npassword p%40ss+word\n",
            )
            .unwrap();
            std::fs::create_dir_all(net.join("perform")).unwrap();
            std::fs::write(
                net.join("perform/.registry"),
                "Perform MODE+%25nick%25+%2Bx%0A/JOIN+%23x\n",
            )
            .unwrap();
            let p = parse(SAMPLE, Some(dir.path()), conn).unwrap();
            let n = &p.users[0].networks[0];
            assert_eq!(n.secret.sasl, Some(("alice".into(), "p@ss word".into())));
            assert_eq!(n.secret.on_connect, vec!["MODE %nick% +x", "JOIN #x"]);
        });
    }

    #[test]
    fn ipv6_literal_servers() {
        let conf = "Version = 1.8.2\n<User carol>\n\t<Pass password>\n\t\tMethod = sha256\n\t\tHash = 00\n\t\tSalt = x\n\t</Pass>\n\t<Network undernet>\n\t\tServer = [2001:db8::1] +6697\n\t</Network>\n\t<Network other>\n\t\tServer = 2001:db8::2 6667\n\t</Network>\n\t<Network bad>\n\t\tServer = 2001:zz::1 6667\n\t</Network>\n</User>\n";
        with_db(|conn| {
            let p = parse(conf, None, conn).unwrap();
            let n = &p.users[0].networks;
            assert_eq!(
                (n[0].host.as_str(), n[0].port, n[0].tls),
                ("2001:db8::1", 6697, true)
            );
            assert_eq!(
                (n[1].host.as_str(), n[1].port, n[1].tls),
                ("2001:db8::2", 6667, false)
            );
            assert!(
                crate::db::validate_network_fields("x", "2001:db8::1", 6697, "n", "u", "r").is_ok()
            );
            assert!(crate::db::validate_network_fields("x", "a:b", 6697, "n", "u", "r").is_err());
        });
    }

    #[test]
    fn rejects_garbage() {
        with_db(|conn| {
            assert!(parse("hello world", None, conn).is_err());
            assert!(parse("<User a>\n", None, conn).is_err());
            assert!(parse("</User>\n", None, conn).is_err());
            assert!(parse(&"<A>\n".repeat(20), None, conn).is_err());
        });
    }

    #[test]
    fn log_lines() {
        let p = |s| parse_log_line(s).unwrap();
        assert_eq!(p("[01:02:03] <bob> hi there").text, "hi there");
        assert_eq!(p("[01:02:03] <bob> hi").secs, 3723);
        let a = p("[00:00:00] * bob waves");
        assert_eq!(
            (a.command, a.text.as_str()),
            ("PRIVMSG", "\x01ACTION waves\x01")
        );
        assert_eq!(p("[00:00:00] -srv- note").command, "NOTICE");
        let j = p("[10:00:00] *** Joins: carol (c@host.example)");
        assert_eq!(
            (j.command, j.sender.as_str()),
            ("JOIN", "carol!c@host.example")
        );
        let q = p("[10:00:00] *** Quits: carol (c@h) (Ping timeout)");
        assert_eq!((q.command, q.text.as_str()), ("QUIT", "Ping timeout"));
        let k = p("[10:00:00] *** dave was kicked by op (spam)");
        assert_eq!(
            (k.command, k.sender.as_str(), k.text.as_str()),
            ("KICK", "op", "dave spam")
        );
        assert_eq!(p("[10:00:00] *** a is now known as b").text, "b");
        assert_eq!(
            p("[10:00:00] *** op changes topic to 'new topic'").text,
            "new topic"
        );
        assert_eq!(p("[10:00:00] *** op sets mode: +o bob").text, "+o bob");
        assert!(parse_log_line("garbage").is_none());
        assert!(parse_log_line("[99:00:00] <a> b").is_none());
    }

    #[test]
    fn logs_import_and_repeat() {
        with_db(|conn| {
            let uid = db::create_user_with_role(conn, "alice", "x", "user").unwrap();
            let nid = db::create_network(
                conn,
                uid,
                &NewNetwork {
                    name: "libera".into(),
                    host: "irc.libera.chat".into(),
                    port: 6697,
                    tls: true,
                    nick: "alice".into(),
                    username: "alice".into(),
                    realname: String::new(),
                    server_password_enc: None,
                    sasl_username: None,
                    sasl_password_enc: None,
                },
                10,
            )
            .unwrap();
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().join("users/alice/moddata/log/libera");
            std::fs::create_dir_all(root.join("#xbnc-dev")).unwrap();
            std::fs::write(
                root.join("#xbnc-dev/2026-01-02.log"),
                "[10:00:00] <bob> hello\n[10:00:00] <Alice> hi bob\n[10:00:05] *** Joins: c (c@h)\nnoise\n",
            )
            .unwrap();
            let dirs = log_dirs(dir.path(), "alice", "libera");
            assert_eq!(dirs.len(), 1);
            let r = import_logs(conn, nid, "alice", &dirs, 60).unwrap();
            assert_eq!((r.files, r.messages, r.unrecognised), (1, 3, 1));
            let msgs = db::messages_after_id(conn, nid, 0, 10).unwrap();
            // 10:00 at UTC+1 is 09:00 UTC.
            let expect = time::macros::datetime!(2026-01-02 09:00 UTC).unix_timestamp() * 1000;
            assert_eq!(msgs[0].ts, expect);
            assert_eq!(msgs[1].ts, expect + 1);
            assert!(!msgs[0].outgoing && msgs[1].outgoing);
            assert_eq!(msgs[0].target, "#xbnc-dev");
            // FTS sees imported messages.
            assert_eq!(
                db::search_messages(
                    conn,
                    nid,
                    &db::HistoryFilter {
                        target_cf: None,
                        text: Some("hello"),
                        sender: None,
                        before_id: None,
                        from_ts: None,
                        to_ts: None,
                        limit: 10,
                    },
                )
                .unwrap()
                .len(),
                1
            );
            let again = import_logs(conn, nid, "alice", &dirs, 60).unwrap();
            assert_eq!((again.messages, again.duplicates), (0, 3));
        });
    }
}
