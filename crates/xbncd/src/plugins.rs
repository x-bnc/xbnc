//! Third-party plugins: WebAssembly modules run in a sandbox.
//!
//! A plugin is a `.wasm` file with an embedded manifest (custom section
//! `xbnc.manifest`, JSON). It runs in the Wasmi interpreter with:
//!
//! - no access to files, network, clock or randomness beyond the two host
//!   functions below (no WASI);
//! - a fuel budget per call (an infinite loop just runs out of fuel);
//! - a memory cap and limits on tables and instances;
//! - declarative output: the plugin returns JSON *actions* and xBNC decides
//!   whether its permissions allow them.
//!
//! ABI version 1 (see docs/PLUGINS.md):
//!
//! exports: `memory`, `xbnc_alloc(len: i32) -> i32`,
//!          `xbnc_on_event(ptr: i32, len: i32) -> i64` (`ptr << 32 | len` of
//!          the JSON answer, or 0 for none).
//! imports (module `xbnc`, all optional): `log(ptr: i32, len: i32)`,
//!          `now_ms() -> i64`.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use wasmi::{
    Engine, Instance, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc,
};

pub const ABI_VERSION: u32 = 1;
/// Largest plugin file accepted.
pub const MAX_WASM_BYTES: usize = 4 * 1024 * 1024;
/// Linear memory a plugin may use.
pub const MAX_MEMORY_BYTES: usize = 16 * 1024 * 1024;
/// Instructions (roughly) per event.
pub const FUEL_PER_CALL: u64 = 20_000_000;
/// Fuel for instantiation (start function, data init).
pub const FUEL_FOR_START: u64 = 50_000_000;
/// Largest JSON answer, actions per answer and persisted state.
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024;
pub const MAX_ACTIONS: usize = 20;
pub const MAX_STATE_BYTES: usize = 64 * 1024;
/// Failed calls in a row before a plugin is stopped on a network.
pub const MAX_FAILURES: u32 = 5;
/// Key-value storage per plugin and network (`xbnc.kv_*` imports).
pub const KV_MAX_KEY: usize = 200;
pub const KV_MAX_VALUE: usize = 64 * 1024;
pub const KV_MAX_BYTES: i64 = 16 * 1024 * 1024;
pub const KV_MAX_KEYS: i64 = 200_000;
/// Writes one event may make (they are committed together).
pub const KV_MAX_WRITES: usize = 2_000;
/// Imports a plugin may use.
pub const IMPORTS: &[&str] = &["log", "now_ms", "kv_get", "kv_set", "kv_del", "kv_list"];

/// What a plugin may do beyond talking to its own user.
pub const PERMISSIONS: &[&str] = &["send", "raw", "timer", "presence", "http"];
/// Web requests a plugin may have running at once (per network).
pub const HTTP_MAX_INFLIGHT: usize = 3;
/// Web requests per plugin and network per minute.
pub const HTTP_PER_MINUTE: u32 = 30;
/// Largest answer handed to a plugin.
pub const HTTP_MAX_BODY: usize = 256 * 1024;
/// Events only delivered to plugins with the `presence` permission.
pub const PRESENCE_EVENTS: &[&str] = &["join", "part", "kick", "quit", "nick", "names"];

/// Names a plugin cannot take (services and built-in modules).
const RESERVED: &[&str] = &[
    "status", "notes", "watch", "events", "xbnc", "bnc", "plugin", "plugins",
];

// ------------------------------------------------------------ manifest

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// How to use it from IRC (shown in the panel and when the plugin does
    /// not answer `/msg *<id>` itself).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub usage: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub settings: Vec<SettingDef>,
    #[serde(default = "default_abi")]
    pub abi: u32,
    /// Texts in other languages, by language code ("es", "en"...).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub translations: std::collections::BTreeMap<String, Translation>,
    /// Sites the plugin wants to reach with the `http` permission
    /// ("api.example.org" or "*.example.org"). An administrator approves them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub http_domains: Vec<String>,
    /// Seconds between `timer` events (5 to 60; default 60). Games and
    /// countdowns use short timers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timer_secs: Option<u32>,
}

/// Usage text: plain characters plus line breaks.
fn usage_ok(s: &str) -> bool {
    s.chars().count() <= 1000 && s.lines().all(|l| plain(l, 1000))
}

/// A domain pattern of `http_domains`: a host name, optionally `*.` first.
pub fn domain_pattern_ok(d: &str) -> bool {
    let host = d.strip_prefix("*.").unwrap_or(d);
    host.len() <= 200
        && host.contains('.')
        && host.split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
        && host.parse::<std::net::IpAddr>().is_err()
}

/// Whether `host` is covered by one of the approved patterns.
pub fn domain_allowed(approved: &[String], host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    approved.iter().any(|p| {
        let p = p.to_ascii_lowercase();
        match p.strip_prefix("*.") {
            Some(base) => host.len() > base.len() + 1 && host.ends_with(&format!(".{base}")),
            None => host == p,
        }
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Translation {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub usage: String,
    /// Setting key → label and help.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub settings: std::collections::BTreeMap<String, SettingText>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SettingText {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub help: String,
}

fn default_abi() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SettingDef {
    pub key: String,
    /// bool, int, text or lines
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub help: String,
    #[serde(default)]
    pub default: Value,
    #[serde(default)]
    pub min: Option<i64>,
    #[serde(default)]
    pub max: Option<i64>,
}

fn plain(s: &str, max: usize) -> bool {
    s.chars().count() <= max && !s.chars().any(char::is_control)
}

fn key_ok(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        && s.starts_with(|c: char| c.is_ascii_lowercase())
}

impl Manifest {
    pub fn validate(&self) -> Result<()> {
        if !key_ok(&self.id, 32) {
            bail!("plugin id must be 1-32 characters of a-z 0-9 _ - starting with a letter");
        }
        if RESERVED.contains(&self.id.as_str()) || crate::modules::info(&self.id).is_some() {
            bail!("plugin id {:?} is reserved", self.id);
        }
        if self.name.trim().is_empty() || !plain(&self.name, 60) {
            bail!("plugin name must be 1-60 characters");
        }
        if self.version.is_empty() || !plain(&self.version, 20) {
            bail!("plugin version must be 1-20 characters");
        }
        if !plain(&self.description, 400) || !plain(&self.author, 100) {
            bail!("plugin description or author too long");
        }
        if !usage_ok(&self.usage) || self.translations.values().any(|t| !usage_ok(&t.usage)) {
            bail!("plugin usage must be at most 1000 characters");
        }
        if !self.homepage.is_empty()
            && !(self.homepage.starts_with("https://") && plain(&self.homepage, 200))
        {
            bail!("plugin homepage must be an https:// URL");
        }
        if self.abi != ABI_VERSION {
            bail!(
                "plugin uses ABI version {}; this xBNC supports {ABI_VERSION}",
                self.abi
            );
        }
        for p in &self.permissions {
            if !PERMISSIONS.contains(&p.as_str()) {
                bail!("unknown permission {p:?}");
            }
        }
        if self.has("http") {
            if self.http_domains.is_empty() || self.http_domains.len() > 10 {
                bail!("the http permission needs 1 to 10 http_domains");
            }
            for d in &self.http_domains {
                if !domain_pattern_ok(d) {
                    bail!("http_domains: {d:?} is not a valid domain");
                }
            }
        } else if !self.http_domains.is_empty() {
            bail!("http_domains needs the http permission");
        }
        if let Some(t) = self.timer_secs {
            if !self.has("timer") || !(5..=60).contains(&t) {
                bail!("timer_secs goes from 5 to 60 and needs the timer permission");
            }
        }
        if self.settings.len() > 20 {
            bail!("at most 20 settings");
        }
        let mut seen = Vec::new();
        for s in &self.settings {
            if !key_ok(&s.key, 32) || seen.contains(&&s.key) {
                bail!("setting key {:?} is not valid or repeated", s.key);
            }
            seen.push(&s.key);
            if !matches!(s.kind.as_str(), "bool" | "int" | "text" | "lines") {
                bail!("setting {} has unknown type {:?}", s.key, s.kind);
            }
            if !plain(&s.label, 80) || !plain(&s.help, 300) {
                bail!("setting {}: label or help too long", s.key);
            }
            if s.kind == "int" {
                let (min, max) = (s.min.unwrap_or(i64::MIN), s.max.unwrap_or(i64::MAX));
                if min > max {
                    bail!("setting {}: min is greater than max", s.key);
                }
            }
        }
        if self.translations.len() > 20 {
            bail!("at most 20 translations");
        }
        for (lang, t) in &self.translations {
            if lang.is_empty()
                || lang.len() > 8
                || !lang.chars().all(|c| c.is_ascii_lowercase() || c == '-')
            {
                bail!("translation language {lang:?} is not a language code");
            }
            if !plain(&t.name, 60) || !plain(&t.description, 400) {
                bail!("translation {lang}: name or description too long");
            }
            for (k, st) in &t.settings {
                if !self.settings.iter().any(|d| &d.key == k)
                    || !plain(&st.label, 80)
                    || !plain(&st.help, 300)
                {
                    bail!("translation {lang}: setting {k:?} is unknown or its texts are too long");
                }
            }
        }
        // The defaults themselves must be valid.
        validate_config(&self.settings, &Map::new()).context("default settings")?;
        Ok(())
    }

    pub fn has(&self, perm: &str) -> bool {
        self.permissions.iter().any(|p| p == perm)
    }
}

/// Validates and completes a plugin's per-network configuration.
pub fn validate_config(
    defs: &[SettingDef],
    values: &Map<String, Value>,
) -> Result<Map<String, Value>> {
    for k in values.keys() {
        if !defs.iter().any(|d| &d.key == k) {
            bail!("unknown setting {k:?}");
        }
    }
    let mut out = Map::new();
    for d in defs {
        let v = values.get(&d.key).unwrap_or(&d.default);
        let norm = match d.kind.as_str() {
            "bool" => Value::Bool(match v {
                Value::Null => false,
                Value::Bool(b) => *b,
                Value::String(t) if matches!(t.as_str(), "true" | "on" | "1") => true,
                Value::String(t) if matches!(t.as_str(), "false" | "off" | "0") => false,
                _ => bail!("{}: expected true or false", d.key),
            }),
            "int" => {
                let n = match v {
                    Value::Null => d.min.unwrap_or(0).max(0).min(d.max.unwrap_or(i64::MAX)),
                    Value::Number(n) => n
                        .as_i64()
                        .ok_or_else(|| anyhow::anyhow!("{}: expected a whole number", d.key))?,
                    Value::String(t) => t
                        .trim()
                        .parse()
                        .map_err(|_| anyhow::anyhow!("{}: expected a whole number", d.key))?,
                    _ => bail!("{}: expected a whole number", d.key),
                };
                let (min, max) = (d.min.unwrap_or(i64::MIN), d.max.unwrap_or(i64::MAX));
                if n < min || n > max {
                    bail!("{}: must be between {min} and {max}", d.key);
                }
                Value::from(n)
            }
            "text" => {
                let t = match v {
                    Value::Null => String::new(),
                    Value::String(t) => t.trim().to_string(),
                    _ => bail!("{}: expected text", d.key),
                };
                if !plain(&t, 400) {
                    bail!("{}: at most 400 characters, no control characters", d.key);
                }
                Value::String(t)
            }
            _ => {
                let t = match v {
                    Value::Null => String::new(),
                    Value::String(t) => t.clone(),
                    Value::Array(a) => a
                        .iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => bail!("{}: expected text", d.key),
                };
                let ls: Vec<&str> = t.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
                if ls.len() > 200 || ls.iter().any(|l| !plain(l, 400)) {
                    bail!("{}: at most 200 lines of 400 characters", d.key);
                }
                Value::String(ls.join("\n"))
            }
        };
        out.insert(d.key.clone(), norm);
    }
    Ok(out)
}

/// Reads the manifest from a wasm binary's `xbnc.manifest` custom section.
pub fn read_manifest(wasm: &[u8]) -> Result<Manifest> {
    if wasm.len() > MAX_WASM_BYTES {
        bail!(
            "the plugin is larger than {} MB",
            MAX_WASM_BYTES / 1024 / 1024
        );
    }
    if wasm.len() < 8 || &wasm[..4] != b"\0asm" {
        bail!("this is not a WebAssembly file");
    }
    if wasm[4..8] != [1, 0, 0, 0] {
        bail!("unsupported WebAssembly version");
    }
    let mut pos = 8;
    let mut found = None;
    while pos < wasm.len() {
        let id = wasm[pos];
        pos += 1;
        let (size, n) = leb128(&wasm[pos..]).context("malformed section header")?;
        pos += n;
        let end = pos
            .checked_add(size as usize)
            .filter(|e| *e <= wasm.len())
            .context("truncated section")?;
        if id == 0 {
            let (nlen, n) = leb128(&wasm[pos..end]).context("malformed custom section")?;
            let nstart = pos + n;
            let nend = nstart
                .checked_add(nlen as usize)
                .filter(|e| *e <= end)
                .context("malformed custom section")?;
            if &wasm[nstart..nend] == b"xbnc.manifest" {
                if found.is_some() {
                    bail!("the plugin has more than one manifest");
                }
                found = Some(&wasm[nend..end]);
            }
        }
        pos = end;
    }
    let raw = found.context("no xbnc.manifest section: build the plugin with the xBNC SDK")?;
    // C strings may carry a trailing NUL.
    let raw = raw.strip_suffix(&[0]).unwrap_or(raw);
    if raw.len() > 32 * 1024 {
        bail!("the manifest is too large");
    }
    let m: Manifest = serde_json::from_slice(raw).context("the manifest is not valid JSON")?;
    m.validate()?;
    Ok(m)
}

/// Returns a copy of `wasm` with `manifest` (JSON) as its `xbnc.manifest`
/// custom section, replacing any previous one. The manifest is validated.
pub fn embed_manifest(wasm: &[u8], manifest: &[u8]) -> Result<Vec<u8>> {
    let m: Manifest = serde_json::from_slice(manifest).context("the manifest is not valid JSON")?;
    m.validate()?;
    let compact = serde_json::to_vec(&m)?;
    if wasm.len() < 8 || &wasm[..4] != b"\0asm" || wasm[4..8] != [1, 0, 0, 0] {
        bail!("this is not a WebAssembly file");
    }
    let mut out = wasm[..8].to_vec();
    let mut pos = 8;
    while pos < wasm.len() {
        let start = pos;
        let id = wasm[pos];
        pos += 1;
        let (size, n) = leb128(&wasm[pos..]).context("malformed section header")?;
        pos += n;
        let end = pos
            .checked_add(size as usize)
            .filter(|e| *e <= wasm.len())
            .context("truncated section")?;
        let is_manifest = id == 0
            && leb128(&wasm[pos..end])
                .and_then(|(l, k)| {
                    let a = pos.checked_add(k)?;
                    let b = a.checked_add(usize::try_from(l).ok()?)?;
                    Some(wasm.get(a..b) == Some(&b"xbnc.manifest"[..]))
                })
                .unwrap_or(false);
        if !is_manifest {
            out.extend_from_slice(&wasm[start..end]);
        }
        pos = end;
    }
    let name = b"xbnc.manifest";
    let mut payload = Vec::new();
    push_leb128(&mut payload, name.len() as u64);
    payload.extend_from_slice(name);
    payload.extend_from_slice(&compact);
    out.push(0);
    push_leb128(&mut out, payload.len() as u64);
    out.extend_from_slice(&payload);
    Ok(out)
}

fn push_leb128(out: &mut Vec<u8>, mut n: u64) {
    loop {
        let mut b = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 {
            b |= 0x80;
        }
        out.push(b);
        if n == 0 {
            break;
        }
    }
}

fn leb128(b: &[u8]) -> Option<(u64, usize)> {
    let mut v = 0u64;
    for (i, byte) in b.iter().enumerate().take(10) {
        v |= ((byte & 0x7f) as u64) << (7 * i);
        if byte & 0x80 == 0 {
            return Some((v, i + 1));
        }
    }
    None
}

// ------------------------------------------------------------ engine

/// A validated, compiled plugin.
pub struct Plugin {
    pub manifest: Manifest,
    pub sha256: String,
    module: Module,
}

pub fn engine() -> Engine {
    let mut c = wasmi::Config::default();
    c.consume_fuel(true);
    // Features a plugin does not need are off: a smaller surface.
    // (SIMD and memory64 are not compiled in.)
    c.wasm_multi_memory(false);
    c.wasm_reference_types(false);
    c.wasm_tail_call(false);
    c.set_max_stack_height(64 * 1024);
    Engine::new(&c)
}

impl Plugin {
    /// Validates the binary, reads its manifest and checks its exports.
    pub fn compile(engine: &Engine, wasm: &[u8]) -> Result<Plugin> {
        let manifest = read_manifest(wasm)?;
        let module =
            Module::new(engine, wasm).map_err(|e| anyhow::anyhow!("invalid WebAssembly: {e}"))?;
        for imp in module.imports() {
            let ok = imp.module() == "xbnc" && IMPORTS.contains(&imp.name());
            if !ok {
                bail!(
                    "the plugin imports {}.{}, which xBNC does not provide (only xbnc.{})",
                    imp.module(),
                    imp.name(),
                    IMPORTS.join(", xbnc.")
                );
            }
        }
        let mut need = vec!["memory", "xbnc_alloc", "xbnc_on_event"];
        for e in module.exports() {
            need.retain(|n| *n != e.name());
        }
        if !need.is_empty() {
            bail!("the plugin does not export {}", need.join(", "));
        }
        use sha2::Digest;
        let sha256 = sha2::Sha256::digest(wasm)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(Plugin {
            manifest,
            sha256,
            module,
        })
    }
}

/// Host state of one running plugin instance.
pub struct HostState {
    limits: StoreLimits,
    logs: Vec<String>,
    kv: Option<KvStore>,
}

/// The storage of one plugin on one network. Writes made during an event
/// are kept here and committed together when the event succeeds.
pub struct KvStore {
    db: crate::db::Db,
    network_id: i64,
    plugin_id: String,
    pending: std::collections::BTreeMap<String, Option<Vec<u8>>>,
    bytes: i64,
    keys: i64,
}

impl KvStore {
    fn read(&self, key: &str) -> Option<Vec<u8>> {
        if let Some(v) = self.pending.get(key) {
            return v.clone();
        }
        let (nid, pid, k) = (self.network_id, self.plugin_id.clone(), key.to_string());
        self.db
            .with(|c| crate::db::kv_get(c, nid, &pid, &k))
            .ok()
            .flatten()
    }

    fn write(&mut self, key: &str, value: Option<Vec<u8>>) -> bool {
        if self.pending.len() >= KV_MAX_WRITES && !self.pending.contains_key(key) {
            return false;
        }
        let old = self.read(key);
        let old_size = old.as_ref().map_or(0, |v| (v.len() + key.len()) as i64);
        let new_size = value.as_ref().map_or(0, |v| (v.len() + key.len()) as i64);
        let keys = self.keys + i64::from(value.is_some()) - i64::from(old.is_some());
        let bytes = self.bytes - old_size + new_size;
        if value.is_some() && (bytes > KV_MAX_BYTES || keys > KV_MAX_KEYS) {
            return false;
        }
        self.bytes = bytes;
        self.keys = keys;
        self.pending.insert(key.to_string(), value);
        true
    }

    fn commit(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let writes = std::mem::take(&mut self.pending);
        let (nid, pid) = (self.network_id, self.plugin_id.clone());
        if let Err(e) = self.db.with(|c| crate::db::kv_apply(c, nid, &pid, &writes)) {
            tracing::warn!(plugin = %pid, error = %e, "cannot save plugin storage");
            self.reload_usage();
        }
    }

    fn discard(&mut self) {
        if !self.pending.is_empty() {
            self.pending.clear();
            self.reload_usage();
        }
    }

    fn reload_usage(&mut self) {
        let (nid, pid) = (self.network_id, self.plugin_id.clone());
        if let Ok((b, k)) = self.db.with(|c| crate::db::kv_usage(c, nid, &pid)) {
            self.bytes = b;
            self.keys = k;
        }
    }
}

fn mem_slice(mem: &[u8], ptr: i32, len: i32, max: usize) -> Option<&[u8]> {
    let (p, l) = (ptr as u32 as usize, len as u32 as usize);
    if l > max {
        return None;
    }
    p.checked_add(l).and_then(|e| mem.get(p..e))
}

fn caller_memory(caller: &wasmi::Caller<'_, HostState>) -> Option<Memory> {
    match caller.get_export("memory") {
        Some(wasmi::Extern::Memory(m)) => Some(m),
        _ => None,
    }
}

fn kv_key(
    caller: &wasmi::Caller<'_, HostState>,
    mem: &Memory,
    ptr: i32,
    len: i32,
) -> Option<String> {
    let bytes = mem_slice(mem.data(caller), ptr, len, KV_MAX_KEY)?;
    let k = std::str::from_utf8(bytes).ok()?;
    (!k.is_empty()).then(|| k.to_string())
}

/// A plugin instance for one network.
pub struct PluginInstance {
    pub plugin: Arc<Plugin>,
    store: Store<HostState>,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    on_event: TypedFunc<(i32, i32), i64>,
    pub failures: u32,
}

/// What a plugin asked for in answer to an event (already checked against
/// its permissions).
#[derive(Debug, Default, PartialEq)]
pub struct Output {
    pub actions: Vec<Action>,
    /// New persisted state, when the plugin returned one.
    pub state: Option<Value>,
    pub logs: Vec<String>,
    /// Actions refused for lack of permission or invalid.
    pub refused: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Message to the conversation the event came from (needs `send`).
    Reply(String),
    /// PRIVMSG to a channel or nick (needs `send`).
    Privmsg { target: String, text: String },
    /// NOTICE to a channel or nick (needs `send`).
    Notice { target: String, text: String },
    /// Any IRC line (needs `raw`).
    Raw(String),
    /// Message from `*<plugin>` to the user's own clients (always allowed).
    Tell(String),
    /// A web request (needs `http` and an approved domain); the answer comes
    /// back as an `http` event with the same `id`.
    Http {
        id: String,
        method: String,
        url: String,
        body: Option<String>,
        content_type: Option<String>,
    },
}

impl PluginInstance {
    /// Starts an instance. It always runs on the engine the plugin was
    /// compiled with (`_engine` is kept for call-site clarity).
    pub fn new(_engine: &Engine, plugin: Arc<Plugin>) -> Result<PluginInstance> {
        let engine = &plugin.module.engine().clone();
        let limits = StoreLimitsBuilder::new()
            .memory_size(MAX_MEMORY_BYTES)
            .table_elements(10_000)
            .instances(1)
            .tables(2)
            .memories(1)
            .trap_on_grow_failure(false)
            .build();
        let mut store = Store::new(
            engine,
            HostState {
                limits,
                logs: Vec::new(),
                kv: None,
            },
        );
        store.limiter(|s| &mut s.limits);
        store
            .set_fuel(FUEL_FOR_START)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut linker = <Linker<HostState>>::new(engine);
        linker
            .func_wrap(
                "xbnc",
                "log",
                |mut caller: wasmi::Caller<'_, HostState>, ptr: i32, len: i32| {
                    let Some(wasmi::Extern::Memory(mem)) = caller.get_export("memory") else {
                        return;
                    };
                    if caller.data().logs.len() >= 10 {
                        return;
                    }
                    let data = mem.data(&caller);
                    let (p, l) = (ptr as u32 as usize, (len as u32 as usize).min(500));
                    if let Some(bytes) = p.checked_add(l).and_then(|e| data.get(p..e)) {
                        let text: String = String::from_utf8_lossy(bytes)
                            .chars()
                            .filter(|c| !c.is_control())
                            .collect();
                        caller.data_mut().logs.push(text);
                    }
                },
            )
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        linker
            .func_wrap("xbnc", "now_ms", || -> i64 { crate::db::now_ms() })
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        // kv_get(key, keylen, out, outmax) -> full length, -1 absent, -2 error
        linker
            .func_wrap(
                "xbnc",
                "kv_get",
                |mut caller: wasmi::Caller<'_, HostState>,
                 kp: i32,
                 kl: i32,
                 op: i32,
                 omax: i32|
                 -> i32 {
                    let Some(mem) = caller_memory(&caller) else {
                        return -2;
                    };
                    let Some(key) = kv_key(&caller, &mem, kp, kl) else {
                        return -2;
                    };
                    let Some(kv) = caller.data().kv.as_ref() else {
                        return -2;
                    };
                    let Some(v) = kv.read(&key) else { return -1 };
                    let n = v.len().min(omax.max(0) as usize);
                    let (p, data) = (op as u32 as usize, mem.data_mut(&mut caller));
                    match p.checked_add(n).and_then(|e| data.get_mut(p..e)) {
                        Some(dst) => {
                            dst.copy_from_slice(&v[..n]);
                            v.len() as i32
                        }
                        None => -2,
                    }
                },
            )
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        // kv_set(key, keylen, value, valuelen) -> 0, or -1 (too big, quota)
        linker
            .func_wrap(
                "xbnc",
                "kv_set",
                |mut caller: wasmi::Caller<'_, HostState>,
                 kp: i32,
                 kl: i32,
                 vp: i32,
                 vl: i32|
                 -> i32 {
                    let Some(mem) = caller_memory(&caller) else {
                        return -2;
                    };
                    let Some(key) = kv_key(&caller, &mem, kp, kl) else {
                        return -1;
                    };
                    let Some(value) =
                        mem_slice(mem.data(&caller), vp, vl, KV_MAX_VALUE).map(<[u8]>::to_vec)
                    else {
                        return -1;
                    };
                    match caller
                        .data_mut()
                        .kv
                        .as_mut()
                        .map(|kv| kv.write(&key, Some(value)))
                    {
                        Some(true) => 0,
                        Some(false) => -1,
                        None => -2,
                    }
                },
            )
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        linker
            .func_wrap(
                "xbnc",
                "kv_del",
                |mut caller: wasmi::Caller<'_, HostState>, kp: i32, kl: i32| -> i32 {
                    let Some(mem) = caller_memory(&caller) else {
                        return -2;
                    };
                    let Some(key) = kv_key(&caller, &mem, kp, kl) else {
                        return -1;
                    };
                    match caller.data_mut().kv.as_mut().map(|kv| kv.write(&key, None)) {
                        Some(true) => 0,
                        Some(false) => -1,
                        None => -2,
                    }
                },
            )
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        // kv_list(prefix, plen, after, alen, out, outmax) -> bytes written:
        // keys separated by \n, in order (pending writes are not included).
        linker
            .func_wrap(
                "xbnc",
                "kv_list",
                |mut caller: wasmi::Caller<'_, HostState>,
                 pp: i32,
                 pl: i32,
                 ap: i32,
                 al: i32,
                 op: i32,
                 omax: i32|
                 -> i32 {
                    let Some(mem) = caller_memory(&caller) else {
                        return -2;
                    };
                    let data = mem.data(&caller);
                    let (Some(prefix), Some(after)) = (
                        mem_slice(data, pp, pl, KV_MAX_KEY)
                            .and_then(|b| std::str::from_utf8(b).ok())
                            .map(str::to_string),
                        mem_slice(data, ap, al, KV_MAX_KEY)
                            .and_then(|b| std::str::from_utf8(b).ok())
                            .map(str::to_string),
                    ) else {
                        return -2;
                    };
                    let Some(kv) = caller.data().kv.as_ref() else {
                        return -2;
                    };
                    let (nid, pid) = (kv.network_id, kv.plugin_id.clone());
                    let keys = kv
                        .db
                        .with(|c| crate::db::kv_list(c, nid, &pid, &prefix, &after, 1000))
                        .unwrap_or_default();
                    let max = omax.max(0) as usize;
                    let mut out = Vec::new();
                    for k in keys {
                        if out.len() + k.len() + 1 > max {
                            break;
                        }
                        out.extend_from_slice(k.as_bytes());
                        out.push(b'\n');
                    }
                    let (p, data) = (op as u32 as usize, mem.data_mut(&mut caller));
                    match p.checked_add(out.len()).and_then(|e| data.get_mut(p..e)) {
                        Some(dst) => {
                            dst.copy_from_slice(&out);
                            out.len() as i32
                        }
                        None => -2,
                    }
                },
            )
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let instance: Instance = linker
            .instantiate_and_start(&mut store, &plugin.module)
            .map_err(|e| anyhow::anyhow!("the plugin failed to start: {e}"))?;
        let memory = instance
            .get_memory(&store, "memory")
            .context("no memory export")?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&store, "xbnc_alloc")
            .map_err(|e| anyhow::anyhow!("xbnc_alloc has the wrong signature: {e}"))?;
        let on_event = instance
            .get_typed_func::<(i32, i32), i64>(&store, "xbnc_on_event")
            .map_err(|e| anyhow::anyhow!("xbnc_on_event has the wrong signature: {e}"))?;
        Ok(PluginInstance {
            plugin,
            store,
            memory,
            alloc,
            on_event,
            failures: 0,
        })
    }

    pub fn id(&self) -> &str {
        &self.plugin.manifest.id
    }

    /// Gives the instance its key-value storage on a network.
    pub fn attach_storage(&mut self, db: crate::db::Db, network_id: i64) {
        let mut kv = KvStore {
            db,
            network_id,
            plugin_id: self.plugin.manifest.id.clone(),
            pending: Default::default(),
            bytes: 0,
            keys: 0,
        };
        kv.reload_usage();
        self.store.data_mut().kv = Some(kv);
    }

    /// (bytes, keys) stored, for the panel.
    pub fn storage_usage(&self) -> Option<(i64, i64)> {
        self.store.data().kv.as_ref().map(|k| (k.bytes, k.keys))
    }

    /// Delivers one event. Blocking (run it off the async threads).
    pub fn call(&mut self, event: &Value) -> Result<Output> {
        let input = serde_json::to_vec(event)?;
        self.store.data_mut().logs.clear();
        // A channel member list can hold thousands of nicks.
        let fuel = if event["event"] == "names" {
            FUEL_PER_CALL * 5
        } else {
            FUEL_PER_CALL
        };
        self.store
            .set_fuel(fuel)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let res = self.call_inner(&input);
        let logs = std::mem::take(&mut self.store.data_mut().logs);
        // Storage changes are kept only if the whole event succeeded.
        let ok =
            matches!(&res, Ok(raw) if raw.as_ref().is_none_or(|b| self.parse_output(b).is_ok()));
        if let Some(kv) = self.store.data_mut().kv.as_mut() {
            if ok {
                kv.commit();
            } else {
                kv.discard();
            }
        }
        match res {
            Ok(raw) => {
                let mut out = match raw {
                    None => Output::default(),
                    Some(bytes) => self.parse_output(&bytes)?,
                };
                out.logs = logs;
                self.failures = 0;
                Ok(out)
            }
            Err(e) => {
                self.failures += 1;
                Err(e)
            }
        }
    }

    fn call_inner(&mut self, input: &[u8]) -> Result<Option<Vec<u8>>> {
        let len = i32::try_from(input.len()).context("event too large")?;
        let ptr = self
            .alloc
            .call(&mut self.store, len)
            .map_err(|e| anyhow::anyhow!("xbnc_alloc failed: {e}"))?;
        let p = ptr as u32 as usize;
        let mem = self.memory.data_mut(&mut self.store);
        let dst = p
            .checked_add(input.len())
            .and_then(|e| mem.get_mut(p..e))
            .context("xbnc_alloc returned memory out of bounds")?;
        dst.copy_from_slice(input);
        let r = self
            .on_event
            .call(&mut self.store, (ptr, len))
            .map_err(|e| {
                if matches!(e.as_trap_code(), Some(wasmi::TrapCode::OutOfFuel))
                    || e.to_string().contains("fuel")
                {
                    anyhow::anyhow!("the plugin used too much CPU time for one event")
                } else {
                    anyhow::anyhow!("the plugin failed: {e}")
                }
            })?;
        if r == 0 {
            return Ok(None);
        }
        let (op, ol) = ((r as u64 >> 32) as usize, (r as u64 & 0xffff_ffff) as usize);
        if ol > MAX_OUTPUT_BYTES {
            bail!(
                "the plugin answered more than {} KB",
                MAX_OUTPUT_BYTES / 1024
            );
        }
        let mem = self.memory.data(&self.store);
        let bytes = op
            .checked_add(ol)
            .and_then(|e| mem.get(op..e))
            .context("the plugin answered with memory out of bounds")?;
        Ok(Some(bytes.to_vec()))
    }

    fn parse_output(&self, bytes: &[u8]) -> Result<Output> {
        let v: Value = serde_json::from_slice(bytes).context("the plugin answered invalid JSON")?;
        let mut out = Output::default();
        if let Some(st) = v.get("state") {
            if !st.is_null() {
                let size = serde_json::to_vec(st)?.len();
                if size > MAX_STATE_BYTES {
                    out.refused
                        .push(format!("state larger than {} KB", MAX_STATE_BYTES / 1024));
                } else {
                    out.state = Some(st.clone());
                }
            }
        }
        let empty = Vec::new();
        let actions = v.get("actions").and_then(Value::as_array).unwrap_or(&empty);
        let m = &self.plugin.manifest;
        for a in actions.iter() {
            if out.actions.len() >= MAX_ACTIONS {
                out.refused.push(format!("more than {MAX_ACTIONS} actions"));
                break;
            }
            match parse_action(a) {
                Some(act) => {
                    let needed = match &act {
                        Action::Reply(_) | Action::Privmsg { .. } | Action::Notice { .. } => {
                            Some("send")
                        }
                        Action::Raw(_) => Some("raw"),
                        Action::Http { .. } => Some("http"),
                        Action::Tell(_) => None,
                    };
                    match needed {
                        Some(p) if !m.has(p) => out
                            .refused
                            .push(format!("{} needs the {p:?} permission", action_name(&act))),
                        _ => out.actions.push(act),
                    }
                }
                None => out
                    .refused
                    .push(format!("invalid action {}", truncate(&a.to_string(), 80))),
            }
        }
        Ok(out)
    }
}

fn action_name(a: &Action) -> &'static str {
    match a {
        Action::Reply(_) => "reply",
        Action::Privmsg { .. } => "privmsg",
        Action::Notice { .. } => "notice",
        Action::Raw(_) => "raw",
        Action::Tell(_) => "tell",
        Action::Http { .. } => "http",
    }
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Text that can go in one IRC line: no CR, LF or NUL, at most 400 bytes.
fn line_text(v: &Value) -> Option<String> {
    let s = v.as_str()?;
    if s.is_empty() || s.len() > 400 || s.contains(['\r', '\n', '\0']) {
        return None;
    }
    Some(s.to_string())
}

fn target_ok(t: &str) -> bool {
    !t.is_empty()
        && t.len() <= 64
        && !t.contains([' ', ',', '\r', '\n', '\0', ':'])
        && !t.starts_with('*')
}

fn parse_action(a: &Value) -> Option<Action> {
    let o = a.as_object()?;
    if o.len() != 1 {
        return None;
    }
    let (k, v) = o.iter().next()?;
    Some(match k.as_str() {
        "reply" => Action::Reply(line_text(v)?),
        "tell" => Action::Tell(line_text(v)?),
        "privmsg" | "notice" => {
            let target = v.get("target")?.as_str()?.to_string();
            if !target_ok(&target) {
                return None;
            }
            let text = line_text(v.get("text")?)?;
            if k == "privmsg" {
                Action::Privmsg { target, text }
            } else {
                Action::Notice { target, text }
            }
        }
        "raw" => {
            let line = line_text(v)?;
            let cmd = line.split(' ').next().unwrap_or("").to_ascii_uppercase();
            // Commands that would break the session or leak credentials.
            if matches!(
                cmd.as_str(),
                "QUIT"
                    | "PASS"
                    | "USER"
                    | "NICK"
                    | "CAP"
                    | "AUTHENTICATE"
                    | "OPER"
                    | "SERVER"
                    | "SQUIT"
            ) || cmd.is_empty()
                || !cmd.chars().all(|c| c.is_ascii_alphanumeric())
            {
                return None;
            }
            Action::Raw(line)
        }
        "http" => {
            let id = v
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if id.len() > 64 || id.contains(['\r', '\n', '\0']) {
                return None;
            }
            let url = v.get("url")?.as_str()?.to_string();
            if !url.starts_with("https://")
                || url.len() > 2048
                || url.contains([' ', '\r', '\n', '\0'])
            {
                return None;
            }
            let method = v
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("GET")
                .to_ascii_uppercase();
            if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "DELETE") {
                return None;
            }
            let body = match v.get("body") {
                None | Some(Value::Null) => None,
                Some(Value::String(b)) if b.len() <= 16 * 1024 => Some(b.clone()),
                Some(b @ (Value::Object(_) | Value::Array(_))) => {
                    let s = b.to_string();
                    if s.len() > 16 * 1024 {
                        return None;
                    }
                    Some(s)
                }
                _ => return None,
            };
            let content_type = match v.get("content_type").and_then(Value::as_str) {
                Some(c) if c.len() <= 100 && !c.contains(['\r', '\n']) => Some(c.to_string()),
                Some(_) => return None,
                None if matches!(v.get("body"), Some(Value::Object(_) | Value::Array(_))) => {
                    Some("application/json".into())
                }
                None => body
                    .as_ref()
                    .map(|_| "text/plain; charset=utf-8".to_string()),
            };
            Action::Http {
                id,
                method,
                url,
                body,
                content_type,
            }
        }
        _ => return None,
    })
}

// ------------------------------------------------------------ registry

/// Installed plugins, compiled once and shared by every network.
pub struct PluginRegistry {
    pub engine: Engine,
    pub dir: PathBuf,
    loaded: RwLock<HashMap<String, Arc<Plugin>>>,
    /// Recent plugin output, for the panel's live log.
    pub logs: PluginLogs,
    /// Developer mode: plugins in `<dir>/dev` are installed when they change.
    pub dev: std::sync::atomic::AtomicBool,
}

/// One line of plugin output.
#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub ts: i64,
    pub plugin: String,
    pub network: String,
    /// "info", "warn" or "error".
    pub level: &'static str,
    pub text: String,
}

/// The last lines of plugin output and a feed of new ones.
pub struct PluginLogs {
    pub tx: tokio::sync::broadcast::Sender<LogLine>,
    buf: std::sync::Mutex<std::collections::VecDeque<LogLine>>,
}

const LOG_KEEP: usize = 500;

impl Default for PluginLogs {
    fn default() -> Self {
        PluginLogs {
            tx: tokio::sync::broadcast::channel(256).0,
            buf: std::sync::Mutex::new(std::collections::VecDeque::with_capacity(LOG_KEEP)),
        }
    }
}

impl PluginLogs {
    pub fn push(&self, plugin: &str, network: &str, level: &'static str, text: &str) {
        let l = LogLine {
            ts: crate::db::now_ms(),
            plugin: plugin.to_string(),
            network: network.to_string(),
            level,
            text: text.chars().take(2000).collect(),
        };
        let mut b = self.buf.lock().unwrap_or_else(|e| e.into_inner());
        if b.len() >= LOG_KEEP {
            b.pop_front();
        }
        b.push_back(l.clone());
        drop(b);
        let _ = self.tx.send(l);
    }
    pub fn recent(&self) -> Vec<LogLine> {
        self.buf
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect()
    }
}

impl PluginRegistry {
    pub fn new(dir: PathBuf) -> PluginRegistry {
        PluginRegistry {
            engine: engine(),
            dir,
            loaded: RwLock::new(HashMap::new()),
            logs: PluginLogs::default(),
            dev: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Where developers drop plugins in developer mode.
    pub fn dev_dir(&self) -> PathBuf {
        self.dir.join("dev")
    }

    pub fn path_of(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.wasm"))
    }

    pub fn get(&self, id: &str) -> Option<Arc<Plugin>> {
        self.loaded.read().expect("plugins lock").get(id).cloned()
    }

    pub fn insert(&self, p: Arc<Plugin>) {
        self.loaded
            .write()
            .expect("plugins lock")
            .insert(p.manifest.id.clone(), p);
    }

    /// Forgets plugins that are no longer installed.
    pub fn retain(&self, ids: &[String]) {
        self.loaded
            .write()
            .expect("plugins lock")
            .retain(|k, _| ids.contains(k));
    }

    pub fn remove(&self, id: &str) {
        self.loaded.write().expect("plugins lock").remove(id);
    }

    /// Loads the installed plugins from disk, checking each against the
    /// hash recorded at install time.
    pub fn load_all(&self, installed: &[(String, String)]) {
        for (id, sha) in installed {
            let path = self.path_of(id);
            match std::fs::read(&path)
                .map_err(anyhow::Error::from)
                .and_then(|w| Plugin::compile(&self.engine, &w))
            {
                Ok(p) if &p.sha256 == sha && &p.manifest.id == id => self.insert(Arc::new(p)),
                Ok(_) => {
                    tracing::warn!(plugin = %id, "plugin file changed since it was installed; not loaded")
                }
                Err(e) => tracing::warn!(plugin = %id, error = %e, "cannot load plugin"),
            }
        }
    }
}

/// Writes a plugin file atomically with private permissions.
pub fn store_file(dir: &Path, id: &str, wasm: &[u8]) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    let tmp = dir.join(format!(".{id}.wasm.tmp"));
    crate::crypto::write_private_file(&tmp, wasm)?;
    std::fs::rename(&tmp, dir.join(format!("{id}.wasm")))?;
    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Builds a wasm plugin from WAT with a manifest section appended.
    pub fn plugin_wasm(wat_src: &str, manifest: &str) -> Vec<u8> {
        let w = wat::parse_str(wat_src).unwrap();
        // Bypass validation so tests can check how bad manifests are refused.
        let mut out = w;
        let mut payload = Vec::new();
        push_leb128(&mut payload, 13);
        payload.extend_from_slice(b"xbnc.manifest");
        payload.extend_from_slice(manifest.as_bytes());
        out.push(0);
        push_leb128(&mut out, payload.len() as u64);
        out.extend_from_slice(&payload);
        out
    }

    /// A plugin that answers with the JSON stored at offset 1024.
    pub fn answering(answer: &str) -> String {
        let esc: String = answer.bytes().map(|b| format!("\\{b:02x}")).collect();
        format!(
            r#"(module
              (import "xbnc" "log" (func $log (param i32 i32)))
              (memory (export "memory") 2)
              (data (i32.const 1024) "{esc}")
              (func (export "xbnc_alloc") (param i32) (result i32) (i32.const 8192))
              (func (export "xbnc_on_event") (param i32 i32) (result i64)
                (call $log (i32.const 1024) (i32.const 5))
                (i64.or (i64.shl (i64.const 1024) (i64.const 32)) (i64.const {len})))
            )"#,
            len = answer.len()
        )
    }

    pub const MANIFEST: &str = r#"{"id":"demo","name":"Demo","version":"1.0","permissions":["send"],
        "settings":[{"key":"greeting","type":"text","default":"hi"},{"key":"times","type":"int","min":1,"max":5,"default":2}]}"#;

    #[test]
    fn manifest_rules() {
        let w = plugin_wasm(&answering("{}"), MANIFEST);
        let m = read_manifest(&w).unwrap();
        assert_eq!(m.id, "demo");
        let bad = |s: &str| read_manifest(&plugin_wasm(&answering("{}"), s)).is_err();
        assert!(bad(r#"{"id":"status","name":"x","version":"1"}"#));
        assert!(bad(r#"{"id":"keepnick","name":"x","version":"1"}"#));
        assert!(bad(r#"{"id":"Bad Id","name":"x","version":"1"}"#));
        assert!(bad(
            r#"{"id":"ok","name":"x","version":"1","permissions":["root"]}"#
        ));
        assert!(bad(r#"{"id":"ok","name":"x","version":"1","abi":2}"#));
        assert!(bad(
            r#"{"id":"ok","name":"x","version":"1","settings":[{"key":"n","type":"int","min":5,"max":1}]}"#
        ));
        assert!(bad("not json"));
        assert!(read_manifest(b"hello").is_err());
        assert!(read_manifest(&wat::parse_str("(module)").unwrap()).is_err());
        let cfg = validate_config(&m.settings, &Map::new()).unwrap();
        assert_eq!(cfg["greeting"], "hi");
        assert_eq!(cfg["times"], 2);
        let mut over = Map::new();
        over.insert("times".into(), Value::from(9));
        assert!(validate_config(&m.settings, &over).is_err());
    }

    #[test]
    fn http_permission_rules() {
        let ok = |s: &str| read_manifest(&plugin_wasm(&answering("{}"), s)).is_ok();
        assert!(ok(
            r#"{"id":"web","name":"x","version":"1","permissions":["http"],"http_domains":["api.example.org","*.example.net"]}"#
        ));
        assert!(
            !ok(r#"{"id":"web","name":"x","version":"1","permissions":["http"]}"#),
            "no domains"
        );
        assert!(
            !ok(r#"{"id":"web","name":"x","version":"1","http_domains":["api.example.org"]}"#),
            "no permission"
        );
        assert!(!ok(
            r#"{"id":"web","name":"x","version":"1","permissions":["http"],"http_domains":["10.0.0.1"]}"#
        ));
        assert!(!ok(
            r#"{"id":"web","name":"x","version":"1","permissions":["http"],"http_domains":["*"]}"#
        ));
        let ap = vec!["api.example.org".to_string(), "*.example.net".to_string()];
        assert!(domain_allowed(&ap, "API.example.org"));
        assert!(domain_allowed(&ap, "a.b.example.net"));
        assert!(!domain_allowed(&ap, "example.net"));
        assert!(!domain_allowed(&ap, "evilexample.net"));
        assert!(!domain_allowed(&ap, "api.example.org.evil.com"));
        let a = |j: &str| parse_action(&serde_json::from_str(j).unwrap());
        assert!(
            matches!(a(r#"{"http":{"id":"1","url":"https://api.example.org/x"}}"#), Some(Action::Http { method, .. }) if method == "GET")
        );
        assert!(
            matches!(a(r#"{"http":{"id":"1","method":"post","url":"https://a.example.org","body":{"k":1}}}"#),
            Some(Action::Http { method, body: Some(b), content_type: Some(c), .. }) if method == "POST" && b == r#"{"k":1}"# && c == "application/json")
        );
        assert!(
            a(r#"{"http":{"id":"1","url":"http://api.example.org/"}}"#).is_none(),
            "https only"
        );
        assert!(
            a(r#"{"http":{"id":"1","method":"CONNECT","url":"https://a.example.org"}}"#).is_none()
        );
        assert!(a(
            r#"{"http":{"id":"1","url":"https://a.example.org","content_type":"x\r\nHost: y"}}"#
        )
        .is_none());
    }

    #[test]
    fn actions_are_checked_against_permissions() {
        let e = engine();
        let answer = r##"{"actions":[{"reply":"hello"},{"raw":"MODE #x +o bob"},{"tell":"note"},{"privmsg":{"target":"#c","text":"a\r\nQUIT"}},{"raw":"QUIT :bye"}],"state":{"n":1}}"##;
        let p = Arc::new(Plugin::compile(&e, &plugin_wasm(&answering(answer), MANIFEST)).unwrap());
        let mut i = PluginInstance::new(&e, p).unwrap();
        let out = i.call(&serde_json::json!({"event":"message"})).unwrap();
        assert_eq!(
            out.actions,
            vec![Action::Reply("hello".into()), Action::Tell("note".into())]
        );
        assert_eq!(out.refused.len(), 3, "{:?}", out.refused);
        assert_eq!(out.state, Some(serde_json::json!({"n":1})));
        assert_eq!(out.logs, vec!["{\"act".to_string()]);
    }

    #[test]
    fn sandbox_limits() {
        let e = engine();
        // An infinite loop runs out of fuel instead of hanging xBNC.
        let looping = r#"(module (memory (export "memory") 1)
            (func (export "xbnc_alloc") (param i32) (result i32) (i32.const 0))
            (func (export "xbnc_on_event") (param i32 i32) (result i64) (loop $l (br $l)) (i64.const 0)))"#;
        let p = Arc::new(Plugin::compile(&e, &plugin_wasm(looping, MANIFEST)).unwrap());
        let mut i = PluginInstance::new(&e, p).unwrap();
        let t = std::time::Instant::now();
        let err = i.call(&serde_json::json!({})).unwrap_err().to_string();
        assert!(err.contains("CPU"), "{err}");
        assert!(t.elapsed() < std::time::Duration::from_secs(5));
        assert_eq!(i.failures, 1);
        // Memory cannot grow past the cap.
        let greedy = r#"(module (memory (export "memory") 1)
            (func (export "xbnc_alloc") (param i32) (result i32) (i32.const 0))
            (func (export "xbnc_on_event") (param i32 i32) (result i64)
              (if (i32.ne (memory.grow (i32.const 1000)) (i32.const -1)) (then unreachable))
              (i64.const 0)))"#;
        let p = Arc::new(Plugin::compile(&e, &plugin_wasm(greedy, MANIFEST)).unwrap());
        let mut i = PluginInstance::new(&e, p).unwrap();
        assert!(i.call(&serde_json::json!({})).is_ok());
        // Out-of-bounds answers are rejected, not read.
        let oob = r#"(module (memory (export "memory") 1)
            (func (export "xbnc_alloc") (param i32) (result i32) (i32.const 0))
            (func (export "xbnc_on_event") (param i32 i32) (result i64) (i64.const 0x0000fff000001000)))"#;
        let p = Arc::new(Plugin::compile(&e, &plugin_wasm(oob, MANIFEST)).unwrap());
        let mut i = PluginInstance::new(&e, p).unwrap();
        assert!(i.call(&serde_json::json!({})).is_err());
        // Only xbnc.log and xbnc.now_ms can be imported (no WASI, no files).
        let wasi = r#"(module (import "wasi_snapshot_preview1" "fd_write" (func (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "xbnc_alloc") (param i32) (result i32) (i32.const 0))
            (func (export "xbnc_on_event") (param i32 i32) (result i64) (i64.const 0)))"#;
        assert!(Plugin::compile(&e, &plugin_wasm(wasi, MANIFEST)).is_err());
        // Missing exports are reported.
        assert!(Plugin::compile(
            &e,
            &plugin_wasm(r#"(module (memory (export "memory") 1))"#, MANIFEST)
        )
        .is_err());
    }

    /// The C examples from sdk/c, as built by its Makefile.
    pub const DICE: &[u8] = include_bytes!("../../../sdk/c/examples/dice.wasm");
    pub const AUTORESPONDER: &[u8] = include_bytes!("../../../sdk/c/examples/autoresponder.wasm");

    #[test]
    fn fuzz_regressions() {
        // A custom section whose name length overflows the offset.
        let crash = include_bytes!("plugins_fuzz_regression.bin");
        let _ = embed_manifest(crash, br#"{"id":"fuzz","name":"Fuzz","version":"1"}"#);
        let mut w = b"\0asm\x01\0\0\0".to_vec();
        w.extend_from_slice(&[
            0, 11, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01, b'x',
        ]);
        assert!(read_manifest(&w).is_err());
        let _ = embed_manifest(&w, br#"{"id":"fuzz","name":"Fuzz","version":"1"}"#);
    }

    #[test]
    fn long_running_code_does_not_grow_the_native_stack() {
        // Found by fuzzing: a loop that crashed the process with a stack
        // overflow when the interpreter used tail-call dispatch in a build
        // where the calls were not optimized. It must run out of fuel on a
        // small thread stack instead.
        let data = include_bytes!("plugins_fuzz_stack.bin").to_vec();
        std::thread::Builder::new()
            .stack_size(2048 * 1024)
            .spawn(move || {
                let m =
                    br#"{"id":"fuzz","name":"Fuzz","version":"1","permissions":["send","raw"]}"#;
                let Ok(wasm) = embed_manifest(&data, m) else {
                    return;
                };
                let e = engine();
                let Ok(p) = Plugin::compile(&e, &wasm) else {
                    return;
                };
                if let Ok(mut i) = PluginInstance::new(&e, Arc::new(p)) {
                    let _ = i.call(&serde_json::json!({"event": "message", "text": "!x"}));
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn pack_replaces_the_manifest() {
        let w = plugin_wasm(&answering("{}"), MANIFEST);
        let packed = embed_manifest(&w, br#"{"id":"other","name":"Other","version":"2"}"#).unwrap();
        assert_eq!(read_manifest(&packed).unwrap().id, "other");
        // Only one manifest remains, and the module is still valid.
        Plugin::compile(&engine(), &packed).unwrap();
        assert!(embed_manifest(&w, br#"{"id":"status","name":"x","version":"1"}"#).is_err());
        assert!(embed_manifest(b"nope", br#"{"id":"a","name":"x","version":"1"}"#).is_err());
    }

    #[test]
    fn c_sdk_examples() {
        let e = engine();
        let p = Arc::new(Plugin::compile(&e, DICE).unwrap());
        assert_eq!(p.manifest.id, "dice");
        let cfg = validate_config(&p.manifest.settings, &Map::new()).unwrap();
        let mut i = PluginInstance::new(&e, p).unwrap();
        let ev = |text: &str| {
            serde_json::json!({"event":"message","channel":true,"outgoing":false,"from":"bob","lang":"en",
                "text":text,"config":cfg.clone(),"state":null})
        };
        let out = i.call(&ev("!roll 3d6")).unwrap();
        let Action::Reply(r) = &out.actions[0] else {
            panic!("{out:?}")
        };
        assert!(r.starts_with("bob rolls 3 6-sided dice: "), "{r}");
        let total: i64 = r
            .rsplit("(total ")
            .next()
            .unwrap()
            .trim_end_matches(").")
            .parse()
            .unwrap();
        assert!((3..=18).contains(&total), "{r}");
        assert!(i.call(&ev("hello")).unwrap().actions.is_empty());
        let out = i.call(&ev("!dado abc")).unwrap();
        assert_eq!(
            out.actions,
            vec![Action::Reply(
                "bob: I do not understand the roll. Examples: !roll, !roll 2d6, !roll d20.".into()
            )]
        );
        // Unicode in and out survives the JSON round trip.
        let out = i.call(&ev("!roll d20 ñ")).unwrap();
        assert!(matches!(&out.actions[0], Action::Reply(t) if t.contains("Examples")));

        let p = Arc::new(Plugin::compile(&e, AUTORESPONDER).unwrap());
        let mut cfg = Map::new();
        cfg.insert(
            "rules".into(),
            Value::String("hola = ¡Hola, {nick}!\nprecio = Mira https://example.org".into()),
        );
        cfg.insert("cooldown".into(), Value::from(0));
        let cfg = validate_config(&p.manifest.settings, &cfg).unwrap();
        let mut i = PluginInstance::new(&e, p).unwrap();
        let out = i
            .call(
                &serde_json::json!({"event":"message","channel":true,"outgoing":false,"from":"ana",
                "text":"HOLA a todos","config":cfg.clone()}),
            )
            .unwrap();
        assert_eq!(out.actions, vec![Action::Reply("¡Hola, ana!".into())]);
        let out = i
            .call(&serde_json::json!({"event":"command","text":"list","config":cfg,"lang":"es"}))
            .unwrap();
        assert_eq!(out.actions.len(), 1);
        assert!(
            matches!(&out.actions[0], Action::Tell(t) if t.starts_with("Reglas: hola | precio."))
        );
    }

    #[test]
    fn seen_plugin_handles_big_channels() {
        let seen: &[u8] = include_bytes!("../../../sdk/c/examples/seen.wasm");
        let e = engine();
        let p = Arc::new(Plugin::compile(&e, seen).unwrap());
        let cfg = validate_config(&p.manifest.settings, &Map::new()).unwrap();
        let mut i = PluginInstance::new(&e, p).unwrap();
        // A big channel: 5,000 members at once, within the CPU limit.
        let members: Vec<String> = (0..5000).map(|n| format!("user{n}")).collect();
        let ev = serde_json::json!({"event":"names","channel":"#big","members":members,
            "config":cfg.clone(),"nick":"alice","connected":true});
        i.call(&ev).unwrap();
        let ev = serde_json::json!({"event":"command","text":"user4999","config":cfg.clone(),"nick":"alice","connected":true});
        let out = i.call(&ev).unwrap();
        assert!(
            matches!(&out.actions[0], Action::Tell(t) if t == "user4999 está ahora en #big."),
            "{:?}",
            out.actions
        );
        let ev = serde_json::json!({"event":"command","text":"stats","config":cfg,"nick":"alice","connected":true});
        let out = i.call(&ev).unwrap();
        assert!(
            matches!(&out.actions[0], Action::Tell(t) if t.starts_with("5000 ")),
            "{:?}",
            out.actions
        );
    }
}
