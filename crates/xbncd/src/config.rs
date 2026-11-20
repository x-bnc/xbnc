//! Process-level configuration (TOML).
//!
//! Only what is needed before the daemon can start lives here: listeners,
//! database location, secret key, limits and history policy. Users, networks
//! and channels live in the database and are managed with the CLI.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Name the bouncer uses as its server name towards clients.
    #[serde(default = "default_hostname")]
    pub hostname: String,
    /// Default language for messages to IRC users and for the panel
    /// ("en", "es"). Users can choose their own.
    #[serde(default = "default_language")]
    pub language: String,
    /// Extra languages and corrections (`<code>.json` files, see
    /// TRANSLATING.md). The `XBNC_LOCALES_DIR` environment variable takes
    /// priority.
    #[serde(default = "default_locales_dir")]
    pub locales_dir: PathBuf,
    pub database: DatabaseConfig,
    /// Path of the 32-byte key used to encrypt stored credentials
    /// (SASL passwords, server passwords, channel keys).
    pub secret_key_file: PathBuf,
    #[serde(default)]
    pub listen: Vec<ListenConfig>,
    #[serde(default)]
    pub history: HistoryConfig,
    #[serde(default)]
    pub limits: LimitsConfig,
    #[serde(default)]
    pub log: LogConfig,
    /// Web administration panel and REST API. Absent = disabled.
    pub web: Option<WebConfig>,
    /// DCC relays (the `dcc` module).
    #[serde(default)]
    pub dcc: crate::dcc::DccConfig,
    /// Checks for new versions.
    #[serde(default)]
    pub updates: crate::updates::UpdatesConfig,
    /// Mail server for notifications by e-mail.
    #[serde(default)]
    pub smtp: Option<crate::notify::SmtpConfig>,
    /// Sign-in with an OpenID Connect provider.
    #[serde(default)]
    pub oidc: Option<crate::sso::OidcConfig>,
    /// Sign-in with an LDAP directory.
    #[serde(default)]
    pub ldap: Option<crate::sso::LdapConfig>,
    /// A remote plugin catalog (signed index) besides the official plugins.
    #[serde(default)]
    pub catalog: Option<crate::catalog::CatalogConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebConfig {
    /// Address for the panel and API, e.g. `127.0.0.1:8080`.
    pub address: String,
    pub tls_cert: Option<PathBuf>,
    pub tls_key: Option<PathBuf>,
    /// Allow plain HTTP on a non-loopback address (only behind a TLS proxy
    /// on a trusted network).
    #[serde(default)]
    pub insecure: bool,
    /// Mark the session cookie `Secure`. Defaults to true when TLS is
    /// configured here; set it to true when a TLS reverse proxy is in front.
    pub secure_cookies: Option<bool>,
    /// Public URL of the panel (e.g. `https://bnc.example.org`), used to
    /// validate the Origin of WebSocket and API requests.
    pub public_url: Option<String>,
    /// Session lifetime in hours.
    #[serde(default = "default_session_hours")]
    pub session_hours: u64,
    /// Trust `X-Forwarded-For` / `X-Real-IP` from a reverse proxy on a
    /// loopback address, so login blocking applies to the real client IP.
    #[serde(default)]
    pub trust_proxy: bool,
    /// Serve the panel on the IRC port of `address` (one of the `[[listen]]`
    /// entries, with TLS): browsers get the panel and IRC clients the
    /// bouncer, on the same port. The certificate is the listener's.
    #[serde(default)]
    pub shared: bool,
}

fn default_session_hours() -> u64 {
    12
}

impl WebConfig {
    /// The panel is served over TLS (its own certificate, or the shared
    /// IRC port's).
    pub fn uses_tls(&self) -> bool {
        self.tls_cert.is_some() || self.shared
    }

    pub fn secure_cookies(&self) -> bool {
        self.secure_cookies.unwrap_or(self.tls_cert.is_some() || self.shared)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    /// SQLite file (also the folder for backups and the data directory).
    pub path: PathBuf,
    /// PostgreSQL instead of SQLite: `postgres://user:pass@host/db`.
    /// Add `?sslmode=require` to encrypt the connection.
    #[serde(default)]
    pub url: Option<String>,
    /// High availability (needs `url`): several xBNC nodes share the
    /// database; one runs and the others wait to take over if it stops.
    #[serde(default)]
    pub ha: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListenConfig {
    /// Address and port, e.g. `0.0.0.0:6697` or `[::]:6697`.
    pub address: String,
    /// TLS certificate chain (PEM). Required unless `insecure` is true or the
    /// address is a loopback address.
    pub tls_cert: Option<PathBuf>,
    /// TLS private key (PEM).
    pub tls_key: Option<PathBuf>,
    /// Explicitly allow plain-text connections on a non-loopback address.
    #[serde(default)]
    pub insecure: bool,
    /// Only accept clients from the local network (loopback, private,
    /// link-local, CGNAT/Tailscale and IPv6 ULA addresses). Other clients get
    /// an explanatory ERROR. A LAN-only listener may run without TLS.
    #[serde(default)]
    pub lan_only: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HistoryConfig {
    /// Delete messages older than this many days. 0 keeps them forever.
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
    /// Maximum number of messages replayed automatically on reconnection.
    #[serde(default = "default_playback_max")]
    pub playback_max: usize,
    /// Messages replayed to a client name never seen before.
    #[serde(default = "default_new_client_playback")]
    pub new_client_playback: usize,
    /// Maximum `limit` accepted in a CHATHISTORY request.
    #[serde(default = "default_chathistory_max")]
    pub chathistory_max: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LimitsConfig {
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    #[serde(default = "default_max_clients_per_user")]
    pub max_clients_per_user: usize,
    #[serde(default = "default_max_networks_per_user")]
    pub max_networks_per_user: usize,
    #[serde(default = "default_login_max_failures")]
    pub login_max_failures: u32,
    #[serde(default = "default_login_window_seconds")]
    pub login_window_seconds: u64,
    #[serde(default = "default_login_block_seconds")]
    pub login_block_seconds: u64,
    #[serde(default = "default_registration_timeout")]
    pub registration_timeout_seconds: u64,
    /// Lines buffered per client before it is considered too slow and dropped.
    #[serde(default = "default_client_queue")]
    pub client_queue: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogConfig {
    /// tracing filter, e.g. `info` or `xbncd=debug`. Overridden by RUST_LOG.
    #[serde(default = "default_log_level")]
    pub level: String,
}

fn default_hostname() -> String {
    "xbnc".into()
}
fn default_language() -> String {
    "en".into()
}
fn default_locales_dir() -> PathBuf {
    PathBuf::from("/etc/xbnc/locales")
}
fn default_retention_days() -> u32 {
    30
}
fn default_playback_max() -> usize {
    1000
}
fn default_new_client_playback() -> usize {
    100
}
fn default_chathistory_max() -> usize {
    500
}
fn default_max_connections() -> usize {
    1000
}
fn default_max_clients_per_user() -> usize {
    10
}
fn default_max_networks_per_user() -> usize {
    10
}
fn default_login_max_failures() -> u32 {
    5
}
fn default_login_window_seconds() -> u64 {
    600
}
fn default_login_block_seconds() -> u64 {
    900
}
fn default_registration_timeout() -> u64 {
    30
}
fn default_client_queue() -> usize {
    4096
}
fn default_log_level() -> String {
    "info".into()
}

impl Default for HistoryConfig {
    fn default() -> Self {
        HistoryConfig {
            retention_days: default_retention_days(),
            playback_max: default_playback_max(),
            new_client_playback: default_new_client_playback(),
            chathistory_max: default_chathistory_max(),
        }
    }
}

impl Default for LimitsConfig {
    fn default() -> Self {
        LimitsConfig {
            max_connections: default_max_connections(),
            max_clients_per_user: default_max_clients_per_user(),
            max_networks_per_user: default_max_networks_per_user(),
            login_max_failures: default_login_max_failures(),
            login_window_seconds: default_login_window_seconds(),
            login_block_seconds: default_login_block_seconds(),
            registration_timeout_seconds: default_registration_timeout(),
            client_queue: default_client_queue(),
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        LogConfig {
            level: default_log_level(),
        }
    }
}

impl Config {
    /// Loads and validates a configuration file.
    pub fn load(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read config file {}", path.display()))?;
        let mut cfg: Config = toml::from_str(&text)
            .with_context(|| format!("invalid config file {}", path.display()))?;
        if let Some(d) = std::env::var_os("XBNC_LOCALES_DIR").filter(|d| !d.is_empty()) {
            cfg.locales_dir = PathBuf::from(d);
        }
        // Languages first: `language` may name one of them.
        for line in crate::i18n::load_dir(&cfg.locales_dir) {
            tracing::info!("{line}");
        }
        cfg.validate()?;
        Ok(cfg)
    }

    /// Checks semantic constraints that serde cannot express.
    pub fn validate(&self) -> Result<()> {
        self.dcc.port_range()?;
        self.updates.validate()?;
        if let Some(s) = &self.smtp {
            s.validate()?;
        }
        if let Some(o) = &self.oidc {
            o.validate()?;
            if self
                .web
                .as_ref()
                .and_then(|w| w.public_url.as_ref())
                .is_none()
            {
                bail!("[oidc] needs web.public_url (the provider sends users back to it)");
            }
        }
        if let Some(l) = &self.ldap {
            l.validate()?;
        }
        if let Some(c) = &self.catalog {
            c.validate()?;
        }
        if let Some(u) = &self.database.url {
            if !(u.starts_with("postgres://") || u.starts_with("postgresql://")) {
                bail!("database.url must start with postgres://");
            }
        } else if self.database.ha {
            bail!("database.ha needs database.url (PostgreSQL)");
        }
        if self.hostname.is_empty() || self.hostname.contains(' ') {
            bail!("hostname must be a non-empty name without spaces");
        }
        if crate::i18n::Lang::parse(&self.language).is_none() {
            bail!(
                "language must be one of: {}",
                crate::i18n::codes().join(", ")
            );
        }
        for l in &self.listen {
            let addr: SocketAddr = l
                .address
                .parse()
                .with_context(|| format!("invalid listen address {:?}", l.address))?;
            match (&l.tls_cert, &l.tls_key) {
                (Some(_), Some(_)) => {}
                (None, None) => {
                    if !addr.ip().is_loopback() && !l.insecure && !l.lan_only {
                        bail!(
                            "listener {} has no TLS certificate; set tls_cert/tls_key \
                             or `lan_only = true` / `insecure = true` to accept plain-text connections",
                            l.address
                        );
                    }
                }
                _ => bail!(
                    "listener {}: tls_cert and tls_key must be set together",
                    l.address
                ),
            }
        }
        if let Some(w) = &self.web {
            let addr: SocketAddr = w
                .address
                .parse()
                .with_context(|| format!("invalid web address {:?}", w.address))?;
            if w.shared {
                let l = self
                    .listen
                    .iter()
                    .find(|l| l.address.parse::<SocketAddr>().ok() == Some(addr))
                    .with_context(|| {
                        format!("web.shared: {} must be one of the [[listen]] addresses", w.address)
                    })?;
                if l.tls_cert.is_none() || l.lan_only {
                    bail!("web.shared: the listener {} must use TLS and not be lan_only", l.address);
                }
                if w.tls_cert.is_some() || w.tls_key.is_some() {
                    bail!("web.shared: leave web.tls_cert/tls_key out (the listener's certificate is used)");
                }
            } else {
                match (&w.tls_cert, &w.tls_key) {
                    (Some(_), Some(_)) => {}
                    (None, None) => {
                        if !addr.ip().is_loopback() && !w.insecure {
                            bail!(
                                "web panel on {} has no TLS certificate; set tls_cert/tls_key, \
                                 listen on 127.0.0.1 behind a TLS reverse proxy, or set `insecure = true`",
                                w.address
                            );
                        }
                    }
                    _ => bail!("web: tls_cert and tls_key must be set together"),
                }
            }
            if w.session_hours == 0 || w.session_hours > 24 * 30 {
                bail!("web.session_hours must be between 1 and 720");
            }
            if let Some(u) = &w.public_url {
                if !(u.starts_with("https://") || u.starts_with("http://")) {
                    bail!("web.public_url must start with http:// or https://");
                }
            }
        }
        self.history.validate()?;
        self.limits.validate()?;
        Ok(())
    }
}

impl HistoryConfig {
    pub fn validate(&self) -> Result<()> {
        if self.retention_days > 36_500 {
            bail!("history.retention_days must be at most 36500");
        }
        if self.chathistory_max == 0 || self.chathistory_max > 10_000 {
            bail!("history.chathistory_max must be between 1 and 10000");
        }
        if self.playback_max > 100_000 || self.new_client_playback > 100_000 {
            bail!("history playback limits must be at most 100000");
        }
        Ok(())
    }
}

impl LimitsConfig {
    pub fn validate(&self) -> Result<()> {
        if self.client_queue < 64 || self.client_queue > 1_000_000 {
            bail!("limits.client_queue must be between 64 and 1000000");
        }
        if self.max_connections == 0
            || self.max_clients_per_user == 0
            || self.max_networks_per_user == 0
        {
            bail!("connection, client and network limits must be greater than 0");
        }
        if self.login_max_failures == 0 {
            bail!("limits.login_max_failures must be greater than 0");
        }
        if self.registration_timeout_seconds < 5 || self.registration_timeout_seconds > 600 {
            bail!("limits.registration_timeout_seconds must be between 5 and 600");
        }
        if self.login_window_seconds == 0 || self.login_block_seconds == 0 {
            bail!("login window and block durations must be greater than 0");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<Config> {
        let c: Config = toml::from_str(s)?;
        c.validate()?;
        Ok(c)
    }

    #[test]
    fn minimal_config_uses_defaults() {
        let c = parse(
            r#"
            secret_key_file = "/tmp/k"
            [database]
            path = "/tmp/db"
            [[listen]]
            address = "127.0.0.1:6667"
            "#,
        )
        .unwrap();
        assert_eq!(c.hostname, "xbnc");
        assert_eq!(c.history.retention_days, 30);
        assert_eq!(c.limits.login_max_failures, 5);
    }

    #[test]
    fn public_plaintext_listener_requires_opt_in() {
        let base = r#"
            secret_key_file = "/tmp/k"
            [database]
            path = "/tmp/db"
            [[listen]]
            address = "0.0.0.0:6667"
        "#;
        assert!(parse(base).is_err());
        assert!(parse(&format!("{base}\ninsecure = true")).is_ok());
        assert!(parse(&format!("{base}\nlan_only = true")).is_ok());
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(parse(
            r#"
            secret_key_file = "/tmp/k"
            typo = 1
            [database]
            path = "/tmp/db"
            "#
        )
        .is_err());
    }

    #[test]
    fn half_tls_is_rejected() {
        assert!(parse(
            r#"
            secret_key_file = "/tmp/k"
            [database]
            path = "/tmp/db"
            [[listen]]
            address = "0.0.0.0:6697"
            tls_cert = "/tmp/c"
            "#
        )
        .is_err());
    }
}
