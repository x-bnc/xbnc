//! SQLite storage: schema migrations, configuration entities and message history.
//!
//! All access goes through [`Db`], which serialises access to a single
//! connection. Async callers use [`Db::call`], which runs the closure on the
//! blocking thread pool so the Tokio workers are never blocked on disk I/O.

use crate::params;
use crate::sql::{
    params_from_iter, Conn as Connection, OptionalExtension, Row, SqlError, SqlResult,
};
use anyhow::{bail, Context, Result};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Ordered list of schema migrations. Never edit an existing entry: append.
const MIGRATIONS: &[&str] = &[
    // 1: initial schema
    r#"
    CREATE TABLE users (
        id            INTEGER PRIMARY KEY,
        username      TEXT NOT NULL UNIQUE COLLATE NOCASE,
        password_hash TEXT NOT NULL,
        is_admin      INTEGER NOT NULL DEFAULT 0,
        enabled       INTEGER NOT NULL DEFAULT 1,
        created_at    INTEGER NOT NULL
    );
    CREATE TABLE networks (
        id              INTEGER PRIMARY KEY,
        user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        name            TEXT NOT NULL COLLATE NOCASE,
        host            TEXT NOT NULL,
        port            INTEGER NOT NULL,
        tls             INTEGER NOT NULL,
        nick            TEXT NOT NULL,
        username        TEXT NOT NULL,
        realname        TEXT NOT NULL,
        server_password TEXT,
        sasl_username   TEXT,
        sasl_password   TEXT,
        enabled         INTEGER NOT NULL DEFAULT 1,
        created_at      INTEGER NOT NULL,
        UNIQUE(user_id, name)
    );
    CREATE TABLE channels (
        id         INTEGER PRIMARY KEY,
        network_id INTEGER NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
        name       TEXT NOT NULL,
        name_cf    TEXT NOT NULL,
        key        TEXT,
        UNIQUE(network_id, name_cf)
    );
    CREATE TABLE messages (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        network_id INTEGER NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
        target     TEXT NOT NULL,
        target_cf  TEXT NOT NULL,
        ts         INTEGER NOT NULL,
        sender     TEXT NOT NULL,
        command    TEXT NOT NULL,
        text       TEXT NOT NULL,
        outgoing   INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX messages_by_target ON messages(network_id, target_cf, ts);
    CREATE INDEX messages_by_ts ON messages(ts);
    CREATE TABLE client_cursors (
        network_id INTEGER NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
        client     TEXT NOT NULL,
        last_id    INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (network_id, client)
    );
    CREATE TABLE audit_log (
        id     INTEGER PRIMARY KEY,
        ts     INTEGER NOT NULL,
        actor  TEXT NOT NULL,
        action TEXT NOT NULL,
        detail TEXT NOT NULL
    );
    "#,
    // 2: roles, per-user limits, 2FA, web sessions, settings, login events
    r#"
    ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'user';
    UPDATE users SET role = 'superadmin' WHERE is_admin = 1;
    ALTER TABLE users ADD COLUMN max_networks INTEGER;
    ALTER TABLE users ADD COLUMN max_clients INTEGER;
    ALTER TABLE users ADD COLUMN totp_secret TEXT;
    ALTER TABLE users ADD COLUMN totp_enabled INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE users ADD COLUMN last_login_at INTEGER;
    CREATE TABLE web_sessions (
        id          INTEGER PRIMARY KEY,
        token_hash  TEXT NOT NULL UNIQUE,
        user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        csrf        TEXT NOT NULL,
        created_at  INTEGER NOT NULL,
        last_seen   INTEGER NOT NULL,
        expires_at  INTEGER NOT NULL,
        ip          TEXT NOT NULL,
        user_agent  TEXT NOT NULL
    );
    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    CREATE TABLE login_events (
        id       INTEGER PRIMARY KEY,
        ts       INTEGER NOT NULL,
        ip       TEXT NOT NULL,
        username TEXT NOT NULL,
        source   TEXT NOT NULL,
        success  INTEGER NOT NULL,
        reason   TEXT NOT NULL
    );
    CREATE INDEX login_events_ts ON login_events(ts);
    "#,
    // 3: commands sent after connecting (encrypted: they often hold passwords)
    r#"
    ALTER TABLE networks ADD COLUMN on_connect TEXT;
    "#,
    // 4: native modules, notes, language, last seen, client certificates
    r#"
    CREATE TABLE module_config (
        network_id INTEGER NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
        module     TEXT NOT NULL,
        enabled    INTEGER NOT NULL,
        config     TEXT NOT NULL DEFAULT '{}',
        PRIMARY KEY (network_id, module)
    );
    CREATE TABLE notes (
        user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        key        TEXT NOT NULL COLLATE NOCASE,
        text       TEXT NOT NULL,
        updated_at INTEGER NOT NULL,
        PRIMARY KEY (user_id, key)
    );
    ALTER TABLE users ADD COLUMN language TEXT NOT NULL DEFAULT '';
    ALTER TABLE users ADD COLUMN last_seen_at INTEGER;
    ALTER TABLE networks ADD COLUMN client_cert TEXT;
    ALTER TABLE networks ADD COLUMN client_key TEXT;
    "#,
    // 5: full-text index of the history (kept in sync by triggers)
    r#"
    CREATE VIRTUAL TABLE messages_fts USING fts5(
        text,
        content = 'messages',
        content_rowid = 'id',
        tokenize = 'unicode61 remove_diacritics 2'
    );
    INSERT INTO messages_fts(rowid, text) SELECT id, text FROM messages;
    CREATE TRIGGER messages_fts_insert AFTER INSERT ON messages BEGIN
        INSERT INTO messages_fts(rowid, text) VALUES (new.id, new.text);
    END;
    CREATE TRIGGER messages_fts_delete AFTER DELETE ON messages BEGIN
        INSERT INTO messages_fts(messages_fts, rowid, text) VALUES ('delete', old.id, old.text);
    END;
    CREATE TRIGGER messages_fts_update AFTER UPDATE OF text ON messages BEGIN
        INSERT INTO messages_fts(messages_fts, rowid, text) VALUES ('delete', old.id, old.text);
        INSERT INTO messages_fts(rowid, text) VALUES (new.id, new.text);
    END;
    "#,
    // 6: API tokens
    r#"
    CREATE TABLE api_tokens (
        id           INTEGER PRIMARY KEY,
        user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        name         TEXT NOT NULL,
        hash         TEXT NOT NULL UNIQUE,
        prefix       TEXT NOT NULL,
        scope        TEXT NOT NULL,
        created_at   INTEGER NOT NULL,
        expires_at   INTEGER,
        last_used_at INTEGER,
        last_ip      TEXT
    );
    CREATE INDEX api_tokens_user ON api_tokens(user_id);
    CREATE INDEX messages_by_network_ts ON messages(network_id, ts);
    "#,
    // 7: WebAssembly plugins
    r#"
    CREATE TABLE plugins (
        id           TEXT PRIMARY KEY,
        manifest     TEXT NOT NULL,
        sha256       TEXT NOT NULL,
        size         INTEGER NOT NULL,
        enabled      INTEGER NOT NULL DEFAULT 1,
        installed_at INTEGER NOT NULL,
        installed_by TEXT NOT NULL
    );
    CREATE TABLE plugin_config (
        network_id INTEGER NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
        plugin_id  TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
        enabled    INTEGER NOT NULL,
        config     TEXT NOT NULL DEFAULT '{}',
        state      TEXT,
        PRIMARY KEY (network_id, plugin_id)
    );
    "#,
    // 8: v0.8 (plugin storage, read markers, per-channel retention,
    // notifications, push, passkeys, quotas)
    r#"
    CREATE TABLE plugin_kv (
        network_id INTEGER NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
        plugin_id  TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
        key        TEXT NOT NULL,
        value      BLOB NOT NULL,
        updated    INTEGER NOT NULL,
        PRIMARY KEY (network_id, plugin_id, key)
    );
    ALTER TABLE channels ADD COLUMN retention_days INTEGER;
    ALTER TABLE users ADD COLUMN max_history_mb INTEGER;
    ALTER TABLE messages ADD COLUMN mention INTEGER NOT NULL DEFAULT 0;
    CREATE INDEX messages_mentions ON messages(network_id, ts) WHERE mention = 1;
    CREATE TABLE notify_settings (
        user_id INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
        config  TEXT NOT NULL
    );
    CREATE TABLE push_subscriptions (
        id         INTEGER PRIMARY KEY,
        user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        endpoint   TEXT NOT NULL UNIQUE,
        p256dh     TEXT NOT NULL,
        auth       TEXT NOT NULL,
        label      TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL
    );
    CREATE TABLE read_markers (
        network_id INTEGER NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
        target_cf  TEXT NOT NULL,
        target     TEXT NOT NULL,
        ts         INTEGER NOT NULL,
        PRIMARY KEY (network_id, target_cf)
    );
    ALTER TABLE plugins ADD COLUMN http_approved TEXT NOT NULL DEFAULT '[]';
    CREATE TABLE passkeys (
        id           INTEGER PRIMARY KEY,
        user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        cred_id      TEXT NOT NULL UNIQUE,
        public_key   TEXT NOT NULL,
        sign_count   INTEGER NOT NULL DEFAULT 0,
        label        TEXT NOT NULL DEFAULT '',
        created_at   INTEGER NOT NULL,
        last_used_at INTEGER
    );
    CREATE INDEX passkeys_user ON passkeys(user_id);
    CREATE TABLE user_identities (
        provider   TEXT NOT NULL,
        subject    TEXT NOT NULL,
        user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        created_at INTEGER NOT NULL,
        PRIMARY KEY (provider, subject)
    );
    ALTER TABLE plugins ADD COLUMN wasm BLOB;
    "#,
    // 9: per-user time zone
    r#"
    ALTER TABLE users ADD COLUMN timezone TEXT NOT NULL DEFAULT '';
    "#,
    // 10: alternate servers, trusted certificates and bind hosts; disabled
    // channels and channel order
    r#"
    ALTER TABLE networks ADD COLUMN options TEXT NOT NULL DEFAULT '{}';
    ALTER TABLE users ADD COLUMN bind_host TEXT NOT NULL DEFAULT '';
    ALTER TABLE channels ADD COLUMN disabled INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE channels ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
    "#,
    // 11: profile photos
    r#"
    CREATE TABLE avatars (
        user_id    INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
        mime       TEXT NOT NULL,
        data       BLOB NOT NULL,
        updated_at INTEGER NOT NULL
    );
    "#,
];

/// The same schema for PostgreSQL, as of schema version 8 (PostgreSQL is
/// supported from v0.8). New migrations append to both lists.
const PG_MIGRATIONS: &[(i64, &str)] = &[
    (
        8,
        r#"
    CREATE EXTENSION IF NOT EXISTS citext;
    CREATE TABLE users (
        id             BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
        username       CITEXT NOT NULL UNIQUE,
        password_hash  TEXT NOT NULL,
        is_admin       BIGINT NOT NULL DEFAULT 0,
        enabled        BIGINT NOT NULL DEFAULT 1,
        created_at     BIGINT NOT NULL,
        role           TEXT NOT NULL DEFAULT 'user',
        max_networks   BIGINT,
        max_clients    BIGINT,
        totp_secret    TEXT,
        totp_enabled   BIGINT NOT NULL DEFAULT 0,
        last_login_at  BIGINT,
        language       TEXT NOT NULL DEFAULT '',
        last_seen_at   BIGINT,
        max_history_mb BIGINT
    );
    CREATE TABLE networks (
        id              BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
        user_id         BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        name            CITEXT NOT NULL,
        host            TEXT NOT NULL,
        port            BIGINT NOT NULL,
        tls             BIGINT NOT NULL,
        nick            TEXT NOT NULL,
        username        TEXT NOT NULL,
        realname        TEXT NOT NULL,
        server_password TEXT,
        sasl_username   TEXT,
        sasl_password   TEXT,
        enabled         BIGINT NOT NULL DEFAULT 1,
        created_at      BIGINT NOT NULL,
        on_connect      TEXT,
        client_cert     TEXT,
        client_key      TEXT,
        UNIQUE(user_id, name)
    );
    CREATE TABLE channels (
        id             BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
        network_id     BIGINT NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
        name           TEXT NOT NULL,
        name_cf        TEXT NOT NULL,
        key            TEXT,
        retention_days BIGINT,
        UNIQUE(network_id, name_cf)
    );
    CREATE TABLE messages (
        id         BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
        network_id BIGINT NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
        target     TEXT NOT NULL,
        target_cf  TEXT NOT NULL,
        ts         BIGINT NOT NULL,
        sender     TEXT NOT NULL,
        command    TEXT NOT NULL,
        text       TEXT NOT NULL,
        outgoing   BIGINT NOT NULL DEFAULT 0,
        mention    BIGINT NOT NULL DEFAULT 0
    );
    CREATE INDEX messages_by_target ON messages(network_id, target_cf, ts);
    CREATE INDEX messages_by_ts ON messages(ts);
    CREATE INDEX messages_by_network_ts ON messages(network_id, ts);
    CREATE INDEX messages_mentions ON messages(network_id, ts) WHERE mention = 1;
    CREATE TABLE client_cursors (
        network_id BIGINT NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
        client     TEXT NOT NULL,
        last_id    BIGINT NOT NULL,
        updated_at BIGINT NOT NULL,
        PRIMARY KEY (network_id, client)
    );
    CREATE TABLE audit_log (
        id     BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
        ts     BIGINT NOT NULL,
        actor  TEXT NOT NULL,
        action TEXT NOT NULL,
        detail TEXT NOT NULL
    );
    CREATE TABLE web_sessions (
        id          BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
        token_hash  TEXT NOT NULL UNIQUE,
        user_id     BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        csrf        TEXT NOT NULL,
        created_at  BIGINT NOT NULL,
        last_seen   BIGINT NOT NULL,
        expires_at  BIGINT NOT NULL,
        ip          TEXT NOT NULL,
        user_agent  TEXT NOT NULL
    );
    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    CREATE TABLE login_events (
        id       BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
        ts       BIGINT NOT NULL,
        ip       TEXT NOT NULL,
        username TEXT NOT NULL,
        source   TEXT NOT NULL,
        success  BIGINT NOT NULL,
        reason   TEXT NOT NULL
    );
    CREATE INDEX login_events_ts ON login_events(ts);
    CREATE TABLE module_config (
        network_id BIGINT NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
        module     TEXT NOT NULL,
        enabled    BIGINT NOT NULL,
        config     TEXT NOT NULL DEFAULT '{}',
        PRIMARY KEY (network_id, module)
    );
    CREATE TABLE notes (
        user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        key        CITEXT NOT NULL,
        text       TEXT NOT NULL,
        updated_at BIGINT NOT NULL,
        PRIMARY KEY (user_id, key)
    );
    CREATE TABLE api_tokens (
        id           BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
        user_id      BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        name         TEXT NOT NULL,
        hash         TEXT NOT NULL UNIQUE,
        prefix       TEXT NOT NULL,
        scope        TEXT NOT NULL,
        created_at   BIGINT NOT NULL,
        expires_at   BIGINT,
        last_used_at BIGINT,
        last_ip      TEXT
    );
    CREATE INDEX api_tokens_user ON api_tokens(user_id);
    CREATE TABLE plugins (
        id            TEXT PRIMARY KEY,
        manifest      TEXT NOT NULL,
        sha256        TEXT NOT NULL,
        size          BIGINT NOT NULL,
        enabled       BIGINT NOT NULL DEFAULT 1,
        installed_at  BIGINT NOT NULL,
        installed_by  TEXT NOT NULL,
        http_approved TEXT NOT NULL DEFAULT '[]',
        wasm          BYTEA
    );
    CREATE TABLE plugin_config (
        network_id BIGINT NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
        plugin_id  TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
        enabled    BIGINT NOT NULL,
        config     TEXT NOT NULL DEFAULT '{}',
        state      TEXT,
        PRIMARY KEY (network_id, plugin_id)
    );
    CREATE TABLE plugin_kv (
        network_id BIGINT NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
        plugin_id  TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
        key        TEXT COLLATE "C" NOT NULL,
        value      BYTEA NOT NULL,
        updated    BIGINT NOT NULL,
        PRIMARY KEY (network_id, plugin_id, key)
    );
    CREATE TABLE notify_settings (
        user_id BIGINT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
        config  TEXT NOT NULL
    );
    CREATE TABLE push_subscriptions (
        id         BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
        user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        endpoint   TEXT NOT NULL UNIQUE,
        p256dh     TEXT NOT NULL,
        auth       TEXT NOT NULL,
        label      TEXT NOT NULL DEFAULT '',
        created_at BIGINT NOT NULL
    );
    CREATE TABLE read_markers (
        network_id BIGINT NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
        target_cf  TEXT NOT NULL,
        target     TEXT NOT NULL,
        ts         BIGINT NOT NULL,
        PRIMARY KEY (network_id, target_cf)
    );
    CREATE TABLE passkeys (
        id           BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
        user_id      BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        cred_id      TEXT NOT NULL UNIQUE,
        public_key   TEXT NOT NULL,
        sign_count   BIGINT NOT NULL DEFAULT 0,
        label        TEXT NOT NULL DEFAULT '',
        created_at   BIGINT NOT NULL,
        last_used_at BIGINT
    );
    CREATE INDEX passkeys_user ON passkeys(user_id);
    CREATE TABLE user_identities (
        provider   TEXT NOT NULL,
        subject    TEXT NOT NULL,
        user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        created_at BIGINT NOT NULL,
        PRIMARY KEY (provider, subject)
    );
    "#,
    ),
    (
        9,
        r#"
    ALTER TABLE users ADD COLUMN timezone TEXT NOT NULL DEFAULT '';
    "#,
    ),
    (
        10,
        r#"
    ALTER TABLE networks ADD COLUMN options TEXT NOT NULL DEFAULT '{}';
    ALTER TABLE users ADD COLUMN bind_host TEXT NOT NULL DEFAULT '';
    ALTER TABLE channels ADD COLUMN disabled BIGINT NOT NULL DEFAULT 0;
    ALTER TABLE channels ADD COLUMN position BIGINT NOT NULL DEFAULT 0;
    "#,
    ),
    (
        11,
        r#"
    CREATE TABLE avatars (
        user_id    BIGINT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
        mime       TEXT NOT NULL,
        data       BYTEA NOT NULL,
        updated_at BIGINT NOT NULL
    );
    "#,
    ),
];

/// Copies every table of a SQLite database into an empty PostgreSQL one
/// (`xbncd migrate-to-postgres`). `progress` gets (table, rows copied).
pub fn copy_sqlite_to_postgres(from: &Db, to: &Db, progress: impl Fn(&str, i64)) -> Result<()> {
    if from.with(|c| schema_version(c))? != SCHEMA_VERSION {
        bail!(
            "start this version of xBNC once with the SQLite database first, so it is up to date"
        );
    }
    let to_conn = to.conn.clone();
    let mut target = to_conn
        .lock()
        .map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
    let users: i64 = target.query_row("SELECT COUNT(*) FROM users", (), |r| r.get(0))?;
    if users > 0 {
        bail!("the PostgreSQL database is not empty; use a new one");
    }
    let tx = target.transaction()?;
    for table in TABLES {
        let cols: Vec<String> = from.with(|c| {
            let mut st = c.prepare(&format!("PRAGMA table_info({table})"))?;
            let rows = st.query_map((), |r| r.get::<String>(1))?;
            Ok(rows.collect::<SqlResult<Vec<_>>>()?)
        })?;
        let list = cols.join(", ");
        let marks: Vec<String> = (1..=cols.len()).map(|i| format!("?{i}")).collect();
        let insert = format!("INSERT INTO {table} ({list}) VALUES ({})", marks.join(", "));
        let mut last = i64::MIN;
        let mut copied = 0i64;
        // In pages by rowid, so a big history does not fill the memory.
        loop {
            let page: Vec<(i64, Vec<crate::sql::Value>)> = from.with(|c| {
                let mut st = c.prepare(&format!(
                    "SELECT rowid, {list} FROM {table} WHERE rowid > ?1 ORDER BY rowid LIMIT 5000"
                ))?;
                let n = cols.len();
                let rows = st.query_map([last], |r| {
                    let mut v = Vec::with_capacity(n);
                    for i in 0..n {
                        v.push(r.get::<crate::sql::Value>(i + 1)?);
                    }
                    Ok((r.get::<i64>(0)?, v))
                })?;
                Ok(rows.collect::<SqlResult<Vec<_>>>()?)
            })?;
            if page.is_empty() {
                break;
            }
            for (rowid, vals) in &page {
                tx.execute(&insert, params_from_iter(vals.iter()))
                    .with_context(|| format!("copying {table}"))?;
                last = *rowid;
                copied += 1;
            }
            progress(table, copied);
        }
        progress(table, copied);
    }
    // Identity columns continue after the copied ids.
    for table in TABLES {
        let has_id: bool = tx.query_row(
            "SELECT COUNT(*) > 0 FROM information_schema.columns WHERE table_name = ?1 AND column_name = 'id' AND is_identity = 'YES'",
            [*table],
            |r| r.get(0),
        )?;
        if has_id {
            tx.execute_batch(&format!(
                "SELECT setval(pg_get_serial_sequence('{table}', 'id'), COALESCE((SELECT MAX(id) FROM {table}), 0) + 1, false)"
            ))?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Tables in an order that respects foreign keys (for copying databases).
pub const TABLES: &[&str] = &[
    "users",
    "networks",
    "channels",
    "messages",
    "client_cursors",
    "audit_log",
    "web_sessions",
    "settings",
    "login_events",
    "module_config",
    "notes",
    "api_tokens",
    "plugins",
    "plugin_config",
    "plugin_kv",
    "notify_settings",
    "push_subscriptions",
    "read_markers",
    "passkeys",
    "user_identities",
    "avatars",
];

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    /// True for the `superadmin` and `admin` roles.
    pub is_admin: bool,
    pub enabled: bool,
    /// One of `crate::rbac::Role` names.
    pub role: String,
    /// Per-user overrides of the global limits.
    pub max_networks: Option<i64>,
    pub max_clients: Option<i64>,
    pub totp_secret_enc: Option<String>,
    pub totp_enabled: bool,
    pub last_login_at: Option<i64>,
    pub created_at: i64,
    /// Preferred language ("es", "en", ...); empty means the server default.
    pub language: String,
    /// When the user last had a client attached (ms).
    pub last_seen_at: Option<i64>,
    /// History quota in MB (None: unlimited).
    pub max_history_mb: Option<i64>,
    /// IANA time zone ("Europe/Madrid"); empty means UTC.
    pub timezone: String,
}

/// A network row. Encrypted fields hold `SecretBox` output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub tls: bool,
    pub nick: String,
    pub username: String,
    pub realname: String,
    pub server_password_enc: Option<String>,
    pub sasl_username: Option<String>,
    pub sasl_password_enc: Option<String>,
    pub enabled: bool,
}

/// Fields needed to create a network.
#[derive(Debug, Clone)]
pub struct NewNetwork {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub tls: bool,
    pub nick: String,
    pub username: String,
    pub realname: String,
    pub server_password_enc: Option<String>,
    pub sasl_username: Option<String>,
    pub sasl_password_enc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub name: String,
    pub key_enc: Option<String>,
    /// History kept for this channel: None = the server setting, 0 = forever,
    /// -1 = not stored, N = days.
    pub retention_days: Option<i64>,
    /// Saved but not joined (`*status disablechan`).
    pub disabled: bool,
    /// Join and list order (`*status movechan`).
    pub position: i64,
}

/// A backup server of a network, tried in turn after the main one.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AltServer {
    pub host: String,
    pub port: u16,
    pub tls: bool,
    /// Server password, encrypted with the secret key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password_enc: Option<String>,
}

/// Per-network options edited from `*status`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NetOptions {
    #[serde(default)]
    pub servers: Vec<AltServer>,
    /// Local address for the connection to the network ("" = automatic).
    #[serde(default)]
    pub bind_host: String,
    /// SHA-256 fingerprints (lowercase hex) of server certificates accepted
    /// even if they do not validate (self-signed, expired...).
    #[serde(default)]
    pub trusted_fps: Vec<String>,
}

pub fn get_net_options(conn: &Connection, network_id: i64) -> Result<NetOptions> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT options FROM networks WHERE id = ?1",
            [network_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(raw
        .and_then(|r| serde_json::from_str(&r).ok())
        .unwrap_or_default())
}

pub fn set_net_options(conn: &Connection, network_id: i64, o: &NetOptions) -> Result<()> {
    conn.execute(
        "UPDATE networks SET options = ?2 WHERE id = ?1",
        params![network_id, serde_json::to_string(o)?],
    )?;
    Ok(())
}

pub fn get_user_bind_host(conn: &Connection, user_id: i64) -> Result<String> {
    Ok(conn
        .query_row(
            "SELECT bind_host FROM users WHERE id = ?1",
            [user_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_default())
}

pub fn set_user_bind_host(conn: &Connection, user_id: i64, host: &str) -> Result<()> {
    conn.execute(
        "UPDATE users SET bind_host = ?2 WHERE id = ?1",
        params![user_id, host],
    )?;
    Ok(())
}

/// Marks a saved channel as disabled (not joined) or enabled. Returns false
/// if the channel is not saved.
pub fn set_channel_disabled(
    conn: &Connection,
    network_id: i64,
    name_cf: &str,
    disabled: bool,
) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE channels SET disabled = ?3 WHERE network_id = ?1 AND name_cf = ?2",
        params![network_id, name_cf, disabled],
    )? > 0)
}

/// Rewrites the order of the saved channels (folded names, first to last).
pub fn set_channel_order(conn: &Connection, network_id: i64, order_cf: &[String]) -> Result<()> {
    for (i, cf) in order_cf.iter().enumerate() {
        conn.execute(
            "UPDATE channels SET position = ?3 WHERE network_id = ?1 AND name_cf = ?2",
            params![network_id, cf, i as i64 + 1],
        )?;
    }
    Ok(())
}

/// Moves a network to another user (and optionally renames it).
pub fn move_network(
    conn: &Connection,
    network_id: i64,
    new_user: i64,
    new_name: &str,
    max_per_user: usize,
) -> Result<()> {
    validate_name("network name", new_name)?;
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM networks WHERE user_id = ?1 AND id <> ?2",
        params![new_user, network_id],
        |r| r.get(0),
    )?;
    if count as usize >= max_per_user {
        bail!("user already has the maximum of {max_per_user} networks");
    }
    let clash: i64 = conn.query_row(
        "SELECT COUNT(*) FROM networks WHERE user_id = ?1 AND name = ?2 AND id <> ?3",
        params![new_user, new_name, network_id],
        |r| r.get(0),
    )?;
    if clash > 0 {
        bail!("that user already has a network called {new_name}");
    }
    conn.execute(
        "UPDATE networks SET user_id = ?2, name = ?3 WHERE id = ?1",
        params![network_id, new_user, new_name],
    )?;
    Ok(())
}

/// Deletes the stored messages of a network's channels (`channels` true) or
/// of its private conversations. A target is a channel if it starts with one
/// of `chantypes`.
pub fn delete_history_kind(
    conn: &Connection,
    network_id: i64,
    channels: bool,
    chantypes: &str,
) -> Result<usize> {
    let targets: Vec<String> = {
        let mut st =
            conn.prepare("SELECT DISTINCT target_cf FROM messages WHERE network_id = ?1")?;
        let rows = st.query_map([network_id], |r| r.get::<String>(0))?;
        rows.collect::<SqlResult<_>>()?
    };
    let mut n = 0;
    for t in targets {
        let is_chan = t.starts_with(|c: char| chantypes.contains(c));
        if is_chan == channels {
            n += delete_history(conn, network_id, Some(&t))?;
        }
    }
    Ok(n)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
    pub id: i64,
    pub network_id: i64,
    /// Conversation: channel name, or the peer's nick for private messages.
    pub target: String,
    pub ts: i64,
    /// Full source prefix (nick!user@host or nick).
    pub sender: String,
    /// PRIVMSG or NOTICE.
    pub command: String,
    pub text: String,
    /// True if the message was sent by the bouncer user.
    pub outgoing: bool,
}

/// A message about to be stored.
#[derive(Debug, Clone)]
pub struct NewMessage {
    pub network_id: i64,
    pub target: String,
    pub target_cf: String,
    pub ts: i64,
    pub sender: String,
    pub command: String,
    pub text: String,
    pub outgoing: bool,
    /// Someone wrote the user's nick in a channel.
    pub mention: bool,
}

/// A CHATHISTORY bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    Timestamp(i64),
    MsgId(i64),
}

/// Validates bouncer usernames and network names: they appear in login
/// strings (`user/network@client`) so separators are forbidden.
pub fn validate_name(kind: &str, name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if !ok {
        bail!("{kind} must be 1-32 characters of [A-Za-z0-9._-]");
    }
    Ok(())
}

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
    writer: Arc<std::sync::OnceLock<HistoryWriter>>,
}

type InsertJob = (NewMessage, tokio::sync::oneshot::Sender<Result<i64>>);
type HistoryWriter = tokio::sync::mpsc::Sender<InsertJob>;

/// Most messages written in one transaction.
const WRITE_BATCH: usize = 512;

impl Db {
    /// Opens (creating if needed) the database and applies pending migrations.
    pub fn open(path: &Path) -> Result<Db> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("cannot create {}", parent.display()))?;
            }
        }
        let conn = rusqlite::Connection::open(path)
            .with_context(|| format!("cannot open database {}", path.display()))?;
        Self::init_lite(conn)
    }

    /// Opens a PostgreSQL database and applies pending migrations.
    pub fn open_postgres(url: &str, tls: Arc<rustls::ClientConfig>) -> Result<Db> {
        let mut conn = Connection::Pg(crate::sql::PgWorker::connect(url, tls)?);
        migrate(&mut conn)?;
        Ok(Db {
            conn: Arc::new(Mutex::new(conn)),
            writer: Arc::new(std::sync::OnceLock::new()),
        })
    }

    /// Opens the database the configuration names (SQLite or PostgreSQL).
    pub fn open_config(cfg: &crate::config::Config) -> Result<Db> {
        match &cfg.database.url {
            Some(url) => Self::open_postgres(url, crate::server::tls_client_config()?),
            None => Self::open(&cfg.database.path),
        }
    }

    pub fn is_postgres(&self) -> bool {
        self.with(|c| Ok(c.is_postgres())).unwrap_or(false)
    }

    /// In-memory database, for tests.
    pub fn open_in_memory() -> Result<Db> {
        Self::init_lite(rusqlite::Connection::open_in_memory()?)
    }

    fn init_lite(conn: rusqlite::Connection) -> Result<Db> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let mut conn = Connection::Lite(conn);
        migrate(&mut conn)?;
        Ok(Db {
            conn: Arc::new(Mutex::new(conn)),
            writer: Arc::new(std::sync::OnceLock::new()),
        })
    }

    /// Runs `f` synchronously. Intended for the CLI and for tests.
    pub fn with<R>(&self, f: impl FnOnce(&mut Connection) -> Result<R>) -> Result<R> {
        let mut guard = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        f(&mut guard)
    }

    /// Stores a message in the history and returns its id. Messages from all
    /// networks go through one writer thread that commits whatever is
    /// waiting in a single transaction: with hundreds of networks, one
    /// commit per message made the database the bottleneck (load test).
    pub async fn insert_message(&self, m: NewMessage) -> Result<i64> {
        let tx = self.writer.get_or_init(|| self.spawn_writer());
        let (done, wait) = tokio::sync::oneshot::channel();
        tx.send((m, done))
            .await
            .map_err(|_| anyhow::anyhow!("history writer stopped"))?;
        wait.await
            .map_err(|_| anyhow::anyhow!("history writer stopped"))?
    }

    fn spawn_writer(&self) -> HistoryWriter {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<InsertJob>(8 * WRITE_BATCH);
        let db = self.clone_without_writer();
        std::thread::Builder::new()
            .name("xbnc-history".into())
            .spawn(move || {
                let mut jobs = Vec::with_capacity(WRITE_BATCH);
                while let Some(first) = rx.blocking_recv() {
                    jobs.push(first);
                    while jobs.len() < WRITE_BATCH {
                        match rx.try_recv() {
                            Ok(j) => jobs.push(j),
                            Err(_) => break,
                        }
                    }
                    let res = db.with(|c| {
                        let t = c.transaction()?;
                        let ids = jobs
                            .iter()
                            .map(|(m, _)| insert_message(&t, m))
                            .collect::<Result<Vec<_>>>()?;
                        t.commit()?;
                        Ok(ids)
                    });
                    match res {
                        Ok(ids) => {
                            for ((_, done), id) in jobs.drain(..).zip(ids) {
                                let _ = done.send(Ok(id));
                            }
                        }
                        Err(_) => {
                            // Something in the batch failed: store them one
                            // by one so only the bad one is lost.
                            for (m, done) in jobs.drain(..) {
                                let _ = done.send(db.with(|c| insert_message(c, &m)));
                            }
                        }
                    }
                }
            })
            .expect("cannot start the history writer thread");
        tx
    }

    /// The same database for the writer thread (which must not keep its own
    /// sender alive, or it would never stop).
    fn clone_without_writer(&self) -> Db {
        Db {
            conn: self.conn.clone(),
            writer: Arc::new(std::sync::OnceLock::new()),
        }
    }

    /// Runs `f` on the blocking pool.
    pub async fn call<R, F>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut Connection) -> Result<R> + Send + 'static,
        R: Send + 'static,
    {
        let db = self.clone();
        tokio::task::spawn_blocking(move || db.with(f))
            .await
            .context("database task panicked")?
    }
}

fn migrate(conn: &mut Connection) -> Result<()> {
    if conn.is_postgres() {
        return migrate_pg(conn);
    }
    let current = schema_version(conn)?;
    let target = MIGRATIONS.len() as i64;
    if current > target {
        bail!(
            "database schema version {current} is newer than this binary supports ({target}); \
             refusing to start to avoid data loss"
        );
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)
            .with_context(|| format!("migration {} failed", i + 1))?;
        tx.execute_batch(&format!("PRAGMA user_version = {}", i + 1))?;
        tx.commit()?;
        tracing::info!(version = i + 1, "applied database migration");
    }
    Ok(())
}

/// Current schema version.
/// Schema version this binary creates and understands.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

pub fn schema_version(conn: &Connection) -> Result<i64> {
    if conn.is_postgres() {
        return Ok(conn
            .query_row("SELECT MAX(version) FROM xbnc_schema", (), |r| {
                r.get::<Option<i64>>(0)
            })
            .optional()?
            .flatten()
            .unwrap_or(0));
    }
    Ok(conn.query_row("PRAGMA user_version", (), |r| r.get(0))?)
}

fn migrate_pg(conn: &mut Connection) -> Result<()> {
    let exists: bool =
        conn.query_row("SELECT to_regclass('xbnc_schema') IS NOT NULL", (), |r| {
            r.get(0)
        })?;
    if !exists {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS xbnc_schema (version BIGINT PRIMARY KEY, applied_at BIGINT NOT NULL)")?;
    }
    // Only one node migrates at a time.
    let tx = conn.transaction()?;
    tx.execute_batch("LOCK TABLE xbnc_schema IN EXCLUSIVE MODE")?;
    let current = schema_version(&tx)?;
    if current > SCHEMA_VERSION {
        bail!(
            "database schema version {current} is newer than this binary supports ({SCHEMA_VERSION}); \
             refusing to start to avoid data loss"
        );
    }
    for (v, sql) in PG_MIGRATIONS.iter().filter(|(v, _)| *v > current) {
        tx.execute_batch(sql)
            .with_context(|| format!("PostgreSQL migration {v} failed"))?;
        tx.execute(
            "INSERT INTO xbnc_schema (version, applied_at) VALUES (?1, ?2)",
            params![*v, now_ms()],
        )?;
        tracing::info!(version = v, "applied database migration (PostgreSQL)");
    }
    tx.commit()?;
    Ok(())
}

// ---------------------------------------------------------------- users

const USER_COLS: &str = "id, username, password_hash, role, enabled, max_networks, max_clients, \
     totp_secret, totp_enabled, last_login_at, created_at, language, last_seen_at, max_history_mb, \
     timezone";

fn row_user(r: &Row<'_>) -> SqlResult<User> {
    let role: String = r.get(3)?;
    Ok(User {
        id: r.get(0)?,
        username: r.get(1)?,
        password_hash: r.get(2)?,
        is_admin: role == "superadmin" || role == "admin",
        role,
        enabled: r.get(4)?,
        max_networks: r.get(5)?,
        max_clients: r.get(6)?,
        totp_secret_enc: r.get(7)?,
        totp_enabled: r.get(8)?,
        last_login_at: r.get(9)?,
        created_at: r.get(10)?,
        language: r.get(11)?,
        last_seen_at: r.get(12)?,
        max_history_mb: r.get(13)?,
        timezone: r.get(14)?,
    })
}

/// Creates a user; `admin` gives the `superadmin` role, otherwise `user`.
pub fn create_user(
    conn: &Connection,
    username: &str,
    password_hash: &str,
    admin: bool,
) -> Result<i64> {
    create_user_with_role(
        conn,
        username,
        password_hash,
        if admin { "superadmin" } else { "user" },
    )
}

pub fn create_user_with_role(
    conn: &Connection,
    username: &str,
    password_hash: &str,
    role: &str,
) -> Result<i64> {
    validate_name("username", username)?;
    let admin = role == "superadmin" || role == "admin";
    conn.execute(
        "INSERT INTO users (username, password_hash, is_admin, role, enabled, created_at) VALUES (?1, ?2, ?3, ?4, 1, ?5)",
        params![username, password_hash, admin, role, now_ms()],
    )
    .map_err(|e| match e {
        SqlError::Constraint(_) => {
            anyhow::anyhow!("user {username} already exists")
        }
        e => e.into(),
    })?;
    Ok(conn.last_insert_rowid())
}

pub fn get_user(conn: &Connection, username: &str) -> Result<Option<User>> {
    Ok(conn
        .query_row(
            &format!("SELECT {USER_COLS} FROM users WHERE username = ?1"),
            [username],
            row_user,
        )
        .optional()?)
}

pub fn get_user_by_id(conn: &Connection, id: i64) -> Result<Option<User>> {
    Ok(conn
        .query_row(
            &format!("SELECT {USER_COLS} FROM users WHERE id = ?1"),
            [id],
            row_user,
        )
        .optional()?)
}

pub fn list_users(conn: &Connection) -> Result<Vec<User>> {
    let mut st = conn.prepare(&format!("SELECT {USER_COLS} FROM users ORDER BY username"))?;
    let rows = st.query_map((), row_user)?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

pub fn delete_user(conn: &Connection, username: &str) -> Result<bool> {
    Ok(conn.execute("DELETE FROM users WHERE username = ?1", [username])? > 0)
}

pub fn set_user_password(conn: &Connection, username: &str, hash: &str) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE users SET password_hash = ?2 WHERE username = ?1",
        params![username, hash],
    )? > 0)
}

pub fn set_user_enabled(conn: &Connection, username: &str, enabled: bool) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE users SET enabled = ?2 WHERE username = ?1",
        params![username, enabled],
    )? > 0)
}

// ------------------------------------------------------------- networks

const NETWORK_COLS: &str = "id, user_id, name, host, port, tls, nick, username, realname, \
     server_password, sasl_username, sasl_password, enabled";

fn row_network(r: &Row<'_>) -> SqlResult<Network> {
    Ok(Network {
        id: r.get(0)?,
        user_id: r.get(1)?,
        name: r.get(2)?,
        host: r.get(3)?,
        port: r.get::<i64>(4)? as u16,
        tls: r.get(5)?,
        nick: r.get(6)?,
        username: r.get(7)?,
        realname: r.get(8)?,
        server_password_enc: r.get(9)?,
        sasl_username: r.get(10)?,
        sasl_password_enc: r.get(11)?,
        enabled: r.get(12)?,
    })
}

pub fn create_network(
    conn: &Connection,
    user_id: i64,
    n: &NewNetwork,
    max_per_user: usize,
) -> Result<i64> {
    validate_network_fields(&n.name, &n.host, n.port, &n.nick, &n.username, &n.realname)?;
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM networks WHERE user_id = ?1",
        [user_id],
        |r| r.get(0),
    )?;
    if count as usize >= max_per_user {
        bail!("user already has the maximum of {max_per_user} networks");
    }
    conn.execute(
        "INSERT INTO networks (user_id, name, host, port, tls, nick, username, realname, \
         server_password, sasl_username, sasl_password, enabled, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, ?12)",
        params![
            user_id,
            n.name,
            n.host,
            n.port as i64,
            n.tls,
            n.nick,
            n.username,
            n.realname,
            n.server_password_enc,
            n.sasl_username,
            n.sasl_password_enc,
            now_ms()
        ],
    )
    .map_err(|e| match e {
        SqlError::Constraint(_) => {
            anyhow::anyhow!("network {} already exists for this user", n.name)
        }
        e => e.into(),
    })?;
    Ok(conn.last_insert_rowid())
}

/// Validates the user-editable fields of a network.
pub fn validate_network_fields(
    name: &str,
    host: &str,
    port: u16,
    nick: &str,
    username: &str,
    realname: &str,
) -> Result<()> {
    validate_name("network name", name)?;
    // A name, an IPv4 address or an IPv6 address (without brackets).
    let ipv6 = host.parse::<std::net::Ipv6Addr>().is_ok();
    if host.is_empty()
        || host.len() > 253
        || host.contains([' ', '/', '@', '[', ']'])
        || (host.contains(':') && !ipv6)
    {
        bail!("invalid host");
    }
    if port == 0 {
        bail!("invalid port");
    }
    let bad_nick = nick.is_empty()
        || nick.len() > 64
        || nick.starts_with(['#', '&', ':', '$', '+'])
        || nick
            .chars()
            .any(|c| c.is_whitespace() || ",*?!@".contains(c));
    if bad_nick {
        bail!("invalid nick");
    }
    if username.is_empty()
        || username.len() > 64
        || username.chars().any(|c| c.is_whitespace() || c == '@')
    {
        bail!("invalid username");
    }
    if realname.len() > 200 || realname.contains(['\r', '\n', '\0']) {
        bail!("invalid realname");
    }
    Ok(())
}

// ------------------------------------------------------------ modules

/// Stored module rows of a network (absent modules use their defaults).
pub fn module_rows(
    conn: &Connection,
    network_id: i64,
) -> Result<std::collections::HashMap<String, crate::modules::ModState>> {
    let mut st =
        conn.prepare("SELECT module, enabled, config FROM module_config WHERE network_id = ?1")?;
    let rows = st.query_map([network_id], |r| {
        Ok((r.get::<String>(0)?, r.get::<bool>(1)?, r.get::<String>(2)?))
    })?;
    let mut out = std::collections::HashMap::new();
    for row in rows {
        let (id, enabled, config) = row?;
        let config = serde_json::from_str::<serde_json::Value>(&config)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        out.insert(id, crate::modules::ModState { enabled, config });
    }
    Ok(out)
}

pub fn set_module(
    conn: &Connection,
    network_id: i64,
    module: &str,
    st: &crate::modules::ModState,
) -> Result<()> {
    conn.execute(
        "INSERT INTO module_config (network_id, module, enabled, config) VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(network_id, module) DO UPDATE SET enabled = excluded.enabled, config = excluded.config",
        params![
            network_id,
            module,
            st.enabled,
            serde_json::Value::Object(st.config.clone()).to_string()
        ],
    )?;
    Ok(())
}

// ------------------------------------------------------------ notes

pub const MAX_NOTES: i64 = 200;

pub fn list_notes(conn: &Connection, user_id: i64) -> Result<Vec<(String, String, i64)>> {
    let mut st =
        conn.prepare("SELECT key, text, updated_at FROM notes WHERE user_id = ?1 ORDER BY key")?;
    let rows = st.query_map([user_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

pub fn set_note(conn: &Connection, user_id: i64, key: &str, text: &str) -> Result<()> {
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM notes WHERE user_id = ?1 AND key = ?2",
            params![user_id, key],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if !exists {
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM notes WHERE user_id = ?1",
            [user_id],
            |r| r.get(0),
        )?;
        if n >= MAX_NOTES {
            anyhow::bail!("at most {MAX_NOTES} notes");
        }
    }
    conn.execute(
        "INSERT INTO notes (user_id, key, text, updated_at) VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(user_id, key) DO UPDATE SET text = excluded.text, updated_at = excluded.updated_at",
        params![user_id, key, text, now_ms()],
    )?;
    Ok(())
}

pub fn delete_note(conn: &Connection, user_id: i64, key: &str) -> Result<bool> {
    Ok(conn.execute(
        "DELETE FROM notes WHERE user_id = ?1 AND key = ?2",
        params![user_id, key],
    )? > 0)
}

// ------------------------------------------------------------ user extras

pub fn set_language(conn: &Connection, user_id: i64, lang: &str) -> Result<()> {
    conn.execute(
        "UPDATE users SET language = ?2 WHERE id = ?1",
        params![user_id, lang],
    )?;
    Ok(())
}

pub fn set_timezone(conn: &Connection, user_id: i64, tz: &str) -> Result<()> {
    conn.execute(
        "UPDATE users SET timezone = ?2 WHERE id = ?1",
        params![user_id, tz],
    )?;
    Ok(())
}

pub fn touch_last_seen(conn: &Connection, user_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE users SET last_seen_at = ?2 WHERE id = ?1",
        params![user_id, now_ms()],
    )?;
    Ok(())
}

// ------------------------------------------------------------ client cert

/// Client certificate (PEM) and encrypted private key of a network.
pub fn get_client_cert(conn: &Connection, network_id: i64) -> Result<Option<(String, String)>> {
    Ok(conn
        .query_row(
            "SELECT client_cert, client_key FROM networks WHERE id = ?1",
            [network_id],
            |r| Ok((r.get::<Option<String>>(0)?, r.get::<Option<String>>(1)?)),
        )
        .optional()?
        .and_then(|(c, k)| Some((c?, k?))))
}

pub fn set_client_cert(
    conn: &Connection,
    network_id: i64,
    cert_and_key_enc: Option<(&str, &str)>,
) -> Result<()> {
    let (c, k) = match cert_and_key_enc {
        Some((c, k)) => (Some(c), Some(k)),
        None => (None, None),
    };
    conn.execute(
        "UPDATE networks SET client_cert = ?2, client_key = ?3 WHERE id = ?1",
        params![network_id, c, k],
    )?;
    Ok(())
}

// ------------------------------------------------------------ plugins

#[derive(Debug, Clone, serde::Serialize)]
pub struct PluginRow {
    pub id: String,
    pub manifest: crate::plugins::Manifest,
    pub sha256: String,
    pub size: i64,
    pub enabled: bool,
    pub installed_at: i64,
    pub installed_by: String,
}

pub fn list_plugins(conn: &Connection) -> Result<Vec<PluginRow>> {
    let mut st = conn.prepare(
        "SELECT id, manifest, sha256, size, enabled, installed_at, installed_by FROM plugins ORDER BY id",
    )?;
    let rows = st.query_map((), |r| {
        Ok((
            r.get::<String>(0)?,
            r.get::<String>(1)?,
            r.get::<String>(2)?,
            r.get::<i64>(3)?,
            r.get::<bool>(4)?,
            r.get::<i64>(5)?,
            r.get::<String>(6)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, manifest, sha256, size, enabled, installed_at, installed_by) = row?;
        // A manifest that no longer parses is skipped (and reported by the loader).
        if let Ok(manifest) = serde_json::from_str(&manifest) {
            out.push(PluginRow {
                id,
                manifest,
                sha256,
                size,
                enabled,
                installed_at,
                installed_by,
            });
        }
    }
    Ok(out)
}

/// Installs or upgrades a plugin (per-network settings are kept).
pub fn upsert_plugin(
    conn: &Connection,
    m: &crate::plugins::Manifest,
    sha256: &str,
    size: i64,
    by: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO plugins (id, manifest, sha256, size, enabled, installed_at, installed_by) \
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6) ON CONFLICT(id) DO UPDATE SET \
         manifest = excluded.manifest, sha256 = excluded.sha256, size = excluded.size, \
         installed_at = excluded.installed_at, installed_by = excluded.installed_by",
        params![m.id, serde_json::to_string(m)?, sha256, size, now_ms(), by],
    )?;
    // An upgrade keeps only approvals for domains it still asks for.
    let kept: Vec<String> = plugin_http_approved(conn, &m.id)?
        .into_iter()
        .filter(|d| m.has("http") && m.http_domains.contains(d))
        .collect();
    set_plugin_http_approved(conn, &m.id, &kept)?;
    Ok(())
}

/// Keeps a copy of a plugin file in the database, so every node of a
/// high-availability setup (and a restored database) has it.
pub fn set_plugin_wasm(conn: &Connection, id: &str, wasm: &[u8]) -> Result<()> {
    conn.execute(
        "UPDATE plugins SET wasm = ?2 WHERE id = ?1",
        params![id, wasm],
    )?;
    Ok(())
}

pub fn plugin_wasm(conn: &Connection, id: &str) -> Result<Option<Vec<u8>>> {
    Ok(conn
        .query_row("SELECT wasm FROM plugins WHERE id = ?1", [id], |r| {
            r.get::<Option<Vec<u8>>>(0)
        })
        .optional()?
        .flatten())
}

/// Domains an administrator allowed a plugin to reach.
pub fn plugin_http_approved(conn: &Connection, id: &str) -> Result<Vec<String>> {
    let v: Option<String> = conn
        .query_row(
            "SELECT http_approved FROM plugins WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(v.and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default())
}

pub fn set_plugin_http_approved(conn: &Connection, id: &str, domains: &[String]) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE plugins SET http_approved = ?2 WHERE id = ?1",
        params![id, serde_json::to_string(domains)?],
    )? > 0)
}

pub fn set_plugin_enabled(conn: &Connection, id: &str, enabled: bool) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE plugins SET enabled = ?2 WHERE id = ?1",
        params![id, enabled],
    )? > 0)
}

pub fn delete_plugin(conn: &Connection, id: &str) -> Result<bool> {
    Ok(conn.execute("DELETE FROM plugins WHERE id = ?1", [id])? > 0)
}

/// A network's settings for one plugin: (plugin id, enabled, config, state).
pub type PluginConfigRow = (
    String,
    bool,
    serde_json::Map<String, serde_json::Value>,
    Option<serde_json::Value>,
);

pub fn plugin_config_rows(conn: &Connection, network_id: i64) -> Result<Vec<PluginConfigRow>> {
    let mut st = conn.prepare(
        "SELECT plugin_id, enabled, config, state FROM plugin_config WHERE network_id = ?1",
    )?;
    let rows = st.query_map([network_id], |r| {
        Ok((
            r.get::<String>(0)?,
            r.get::<bool>(1)?,
            r.get::<String>(2)?,
            r.get::<Option<String>>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, en, cfg, state) = row?;
        let cfg = serde_json::from_str::<serde_json::Value>(&cfg)
            .ok()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        let state = state.and_then(|s| serde_json::from_str(&s).ok());
        out.push((id, en, cfg, state));
    }
    Ok(out)
}

pub fn set_plugin_config(
    conn: &Connection,
    network_id: i64,
    plugin_id: &str,
    enabled: bool,
    config: &serde_json::Map<String, serde_json::Value>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO plugin_config (network_id, plugin_id, enabled, config) VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(network_id, plugin_id) DO UPDATE SET enabled = excluded.enabled, config = excluded.config",
        params![
            network_id,
            plugin_id,
            enabled,
            serde_json::Value::Object(config.clone()).to_string()
        ],
    )?;
    Ok(())
}

/// Read markers of a network: (target, target_cf, ts).
pub fn read_markers(conn: &Connection, network_id: i64) -> Result<Vec<(String, String, i64)>> {
    let mut st =
        conn.prepare("SELECT target, target_cf, ts FROM read_markers WHERE network_id = ?1")?;
    let rows = st.query_map(params![network_id], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
    })?;
    Ok(rows.collect::<SqlResult<Vec<_>>>()?)
}

/// Moves a read marker forward (never back).
pub fn set_read_marker(
    conn: &Connection,
    network_id: i64,
    target: &str,
    target_cf: &str,
    ts: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO read_markers (network_id, target_cf, target, ts) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (network_id, target_cf) DO UPDATE SET ts = MAX(ts, excluded.ts), target = excluded.target",
        params![network_id, target_cf, target, ts],
    )?;
    Ok(())
}

/// Bytes and keys a plugin stores on a network.
pub fn kv_usage(conn: &Connection, network_id: i64, plugin_id: &str) -> Result<(i64, i64)> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(LENGTH(key) + LENGTH(value)), 0), COUNT(*) FROM plugin_kv
         WHERE network_id = ?1 AND plugin_id = ?2",
        params![network_id, plugin_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?)
}

pub fn kv_get(
    conn: &Connection,
    network_id: i64,
    plugin_id: &str,
    key: &str,
) -> Result<Option<Vec<u8>>> {
    Ok(conn
        .query_row(
            "SELECT value FROM plugin_kv WHERE network_id = ?1 AND plugin_id = ?2 AND key = ?3",
            params![network_id, plugin_id, key],
            |r| r.get(0),
        )
        .optional()?)
}

/// Applies a batch of writes (`None` deletes) in one transaction.
pub fn kv_apply(
    conn: &mut Connection,
    network_id: i64,
    plugin_id: &str,
    writes: &std::collections::BTreeMap<String, Option<Vec<u8>>>,
) -> Result<()> {
    let tx = conn.transaction()?;
    let now = now_ms();
    for (k, v) in writes {
        match v {
            Some(v) => tx.execute(
                "INSERT INTO plugin_kv (network_id, plugin_id, key, value, updated) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (network_id, plugin_id, key) DO UPDATE SET value = excluded.value, updated = excluded.updated",
                params![network_id, plugin_id, k, v, now],
            )?,
            None => tx.execute(
                "DELETE FROM plugin_kv WHERE network_id = ?1 AND plugin_id = ?2 AND key = ?3",
                params![network_id, plugin_id, k],
            )?,
        };
    }
    tx.commit()?;
    Ok(())
}

/// Keys starting with `prefix`, after `after`, in order.
pub fn kv_list(
    conn: &Connection,
    network_id: i64,
    plugin_id: &str,
    prefix: &str,
    after: &str,
    limit: usize,
) -> Result<Vec<String>> {
    let mut st = conn.prepare(
        "SELECT key FROM plugin_kv WHERE network_id = ?1 AND plugin_id = ?2 AND key > ?3
         AND key >= ?4 ORDER BY key LIMIT ?5",
    )?;
    let rows = st.query_map(
        params![network_id, plugin_id, after, prefix, limit as i64],
        |r| r.get::<String>(0),
    )?;
    // Keys with the prefix are contiguous from `prefix` on.
    let mut out = Vec::new();
    for k in rows {
        let k = k?;
        if !k.starts_with(prefix) {
            break;
        }
        out.push(k);
    }
    Ok(out)
}

pub fn set_plugin_state(
    conn: &Connection,
    network_id: i64,
    plugin_id: &str,
    state: &serde_json::Value,
) -> Result<()> {
    conn.execute(
        "UPDATE plugin_config SET state = ?3 WHERE network_id = ?1 AND plugin_id = ?2",
        params![network_id, plugin_id, state.to_string()],
    )?;
    Ok(())
}

// ------------------------------------------------------------ API tokens

pub const MAX_TOKENS_PER_USER: i64 = 50;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiToken {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    /// First characters of the token, to recognise it in lists.
    pub prefix: String,
    pub scope: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub last_used_at: Option<i64>,
    pub last_ip: Option<String>,
}

const TOKEN_COLS: &str =
    "id, user_id, name, prefix, scope, created_at, expires_at, last_used_at, last_ip";

fn row_token(r: &Row<'_>) -> SqlResult<ApiToken> {
    Ok(ApiToken {
        id: r.get(0)?,
        user_id: r.get(1)?,
        name: r.get(2)?,
        prefix: r.get(3)?,
        scope: r.get(4)?,
        created_at: r.get(5)?,
        expires_at: r.get(6)?,
        last_used_at: r.get(7)?,
        last_ip: r.get(8)?,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn create_token(
    conn: &Connection,
    user_id: i64,
    name: &str,
    hash: &str,
    prefix: &str,
    scope: &str,
    expires_at: Option<i64>,
) -> Result<i64> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM api_tokens WHERE user_id = ?1",
        [user_id],
        |r| r.get(0),
    )?;
    if n >= MAX_TOKENS_PER_USER {
        anyhow::bail!("at most {MAX_TOKENS_PER_USER} tokens per user");
    }
    conn.execute(
        "INSERT INTO api_tokens (user_id, name, hash, prefix, scope, created_at, expires_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![user_id, name, hash, prefix, scope, now_ms(), expires_at],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_tokens(conn: &Connection, user_id: i64) -> Result<Vec<ApiToken>> {
    let mut st = conn.prepare(&format!(
        "SELECT {TOKEN_COLS} FROM api_tokens WHERE user_id = ?1 ORDER BY id"
    ))?;
    let rows = st.query_map([user_id], row_token)?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

pub fn delete_token(conn: &Connection, user_id: i64, id: i64) -> Result<bool> {
    Ok(conn.execute(
        "DELETE FROM api_tokens WHERE id = ?1 AND user_id = ?2",
        params![id, user_id],
    )? > 0)
}

/// Finds a valid (not expired) token by hash and records its use.
pub fn use_token(conn: &Connection, hash: &str, ip: &str) -> Result<Option<ApiToken>> {
    let t = conn
        .query_row(
            &format!("SELECT {TOKEN_COLS} FROM api_tokens WHERE hash = ?1"),
            [hash],
            row_token,
        )
        .optional()?;
    let Some(t) = t else { return Ok(None) };
    let now = now_ms();
    if t.expires_at.is_some_and(|e| e <= now) {
        return Ok(None);
    }
    conn.execute(
        "UPDATE api_tokens SET last_used_at = ?2, last_ip = ?3 WHERE id = ?1",
        params![t.id, now, ip],
    )?;
    Ok(Some(t))
}

/// Encrypted "commands on connect" of a network, if any.
pub fn get_on_connect(conn: &Connection, network_id: i64) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT on_connect FROM networks WHERE id = ?1",
            [network_id],
            |r| r.get::<Option<String>>(0),
        )
        .optional()?
        .flatten())
}

pub fn set_on_connect(conn: &Connection, network_id: i64, enc: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE networks SET on_connect = ?2 WHERE id = ?1",
        params![network_id, enc],
    )?;
    Ok(())
}

pub fn get_network_by_id(conn: &Connection, id: i64) -> Result<Option<Network>> {
    Ok(conn
        .query_row(
            &format!("SELECT {NETWORK_COLS} FROM networks WHERE id = ?1"),
            [id],
            row_network,
        )
        .optional()?)
}

pub fn get_network(conn: &Connection, user_id: i64, name: &str) -> Result<Option<Network>> {
    Ok(conn
        .query_row(
            &format!("SELECT {NETWORK_COLS} FROM networks WHERE user_id = ?1 AND name = ?2"),
            params![user_id, name],
            row_network,
        )
        .optional()?)
}

pub fn list_networks(conn: &Connection, user_id: i64) -> Result<Vec<Network>> {
    let mut st = conn.prepare(&format!(
        "SELECT {NETWORK_COLS} FROM networks WHERE user_id = ?1 ORDER BY name"
    ))?;
    let rows = st.query_map([user_id], row_network)?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

/// Networks that should be connected: enabled network of an enabled user.
pub fn list_active_networks(conn: &Connection) -> Result<Vec<(Network, String)>> {
    let cols = NETWORK_COLS
        .split(", ")
        .map(|c| format!("n.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut st = conn.prepare(&format!(
        "SELECT {cols}, u.username FROM networks n JOIN users u ON u.id = n.user_id \
         WHERE n.enabled = 1 AND u.enabled = 1 ORDER BY n.id"
    ))?;
    let rows = st.query_map((), |r| Ok((row_network(r)?, r.get::<String>(13)?)))?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

pub fn delete_network(conn: &Connection, user_id: i64, name: &str) -> Result<bool> {
    Ok(conn.execute(
        "DELETE FROM networks WHERE user_id = ?1 AND name = ?2",
        params![user_id, name],
    )? > 0)
}

pub fn set_network_enabled(
    conn: &Connection,
    user_id: i64,
    name: &str,
    enabled: bool,
) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE networks SET enabled = ?3 WHERE user_id = ?1 AND name = ?2",
        params![user_id, name, enabled],
    )? > 0)
}

// ------------------------------------------------------------- channels

/// Adds or updates a channel. `key_enc = None` keeps an existing key unless
/// `clear_key` is set.
pub fn upsert_channel(
    conn: &Connection,
    network_id: i64,
    name: &str,
    name_cf: &str,
    key_enc: Option<&str>,
    clear_key: bool,
) -> Result<()> {
    conn.execute(
        "INSERT INTO channels (network_id, name, name_cf, key, position) VALUES (?1, ?2, ?3, ?4, \
         (SELECT COALESCE(MAX(position), 0) + 1 FROM channels WHERE network_id = ?1)) \
         ON CONFLICT(network_id, name_cf) DO UPDATE SET \
           name = excluded.name, \
           key = CASE WHEN ?5 THEN excluded.key ELSE COALESCE(excluded.key, channels.key) END",
        params![network_id, name, name_cf, key_enc, clear_key],
    )?;
    Ok(())
}

pub fn delete_channel(conn: &Connection, network_id: i64, name_cf: &str) -> Result<bool> {
    Ok(conn.execute(
        "DELETE FROM channels WHERE network_id = ?1 AND name_cf = ?2",
        params![network_id, name_cf],
    )? > 0)
}

pub fn list_channels(conn: &Connection, network_id: i64) -> Result<Vec<Channel>> {
    let mut st = conn.prepare(
        "SELECT name, key, retention_days, disabled, position FROM channels \
         WHERE network_id = ?1 ORDER BY position, name_cf",
    )?;
    let rows = st.query_map([network_id], |r| {
        Ok(Channel {
            name: r.get(0)?,
            key_enc: r.get(1)?,
            retention_days: r.get(2)?,
            disabled: r.get(3)?,
            position: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

// ------------------------------------------------------------- messages

const MSG_COLS: &str = "id, network_id, target, ts, sender, command, text, outgoing";

fn row_msg(r: &Row<'_>) -> SqlResult<StoredMessage> {
    Ok(StoredMessage {
        id: r.get(0)?,
        network_id: r.get(1)?,
        target: r.get(2)?,
        ts: r.get(3)?,
        sender: r.get(4)?,
        command: r.get(5)?,
        text: r.get(6)?,
        outgoing: r.get(7)?,
    })
}

pub fn insert_message(conn: &Connection, m: &NewMessage) -> Result<i64> {
    conn.execute(
        "INSERT INTO messages (network_id, target, target_cf, ts, sender, command, text, outgoing, mention) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            m.network_id,
            m.target,
            m.target_cf,
            m.ts,
            m.sender,
            m.command,
            m.text,
            m.outgoing,
            m.mention
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Messages imported later (from another bouncer's logs) get new ids but old
/// times; replay skips anything more than this much older than the cursor.
const REPLAY_SKEW_MS: i64 = 3_600_000;

/// Messages of a network with `id > after_id`, oldest first (skipping old
/// imported history, see [`REPLAY_SKEW_MS`]).
pub fn messages_after_id(
    conn: &Connection,
    network_id: i64,
    after_id: i64,
    limit: usize,
) -> Result<Vec<StoredMessage>> {
    let mut st = conn.prepare(&format!(
        "SELECT {MSG_COLS} FROM messages WHERE network_id = ?1 AND id > ?2 \
         AND ts >= COALESCE((SELECT ts FROM messages WHERE id = ?2), 0) - {REPLAY_SKEW_MS} \
         ORDER BY id LIMIT ?3"
    ))?;
    let rows = st.query_map(params![network_id, after_id, limit as i64], row_msg)?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

/// The newest `limit` messages of a network, oldest first.
pub fn latest_messages(
    conn: &Connection,
    network_id: i64,
    limit: usize,
) -> Result<Vec<StoredMessage>> {
    let mut st = conn.prepare(&format!(
        "SELECT {MSG_COLS} FROM messages WHERE network_id = ?1 ORDER BY ts DESC, id DESC LIMIT ?2"
    ))?;
    let rows = st.query_map(params![network_id, limit as i64], row_msg)?;
    let mut v: Vec<StoredMessage> = rows.collect::<SqlResult<_>>()?;
    v.reverse();
    Ok(v)
}

/// Highest message id of a network (0 if none).
pub fn max_message_id(conn: &Connection, network_id: i64) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COALESCE(MAX(id), 0) FROM messages WHERE network_id = ?1",
        [network_id],
        |r| r.get(0),
    )?)
}

fn bound_clause(b: Bound, op: &str) -> (String, i64) {
    match b {
        Bound::Timestamp(t) => (format!("ts {op} ?"), t),
        Bound::MsgId(id) => (format!("id {op} ?"), id),
    }
}

/// Commands stored as channel events (the `events` module).
pub const EVENT_COMMANDS: &[&str] = &["JOIN", "PART", "QUIT", "KICK", "NICK", "TOPIC", "MODE"];

pub fn is_event(command: &str) -> bool {
    EVENT_COMMANDS.contains(&command)
}

/// Parameters of a CHATHISTORY query. `after`/`before` are exclusive bounds.
#[derive(Debug, Clone)]
pub struct HistQuery {
    pub target_cf: String,
    pub after: Option<Bound>,
    pub before: Option<Bound>,
    pub limit: usize,
    /// Return the `limit` messages closest to `before` (BEFORE/LATEST)
    /// instead of those closest to `after` (AFTER).
    pub newest_first_window: bool,
    /// Include channel events (JOIN, PART, ...).
    pub events: bool,
}

pub fn history_query(
    conn: &Connection,
    network_id: i64,
    q: &HistQuery,
) -> Result<Vec<StoredMessage>> {
    let HistQuery {
        target_cf,
        after,
        before,
        limit,
        newest_first_window,
        events,
    } = q.clone();
    let mut sql = format!("SELECT {MSG_COLS} FROM messages WHERE network_id = ? AND target_cf = ?");
    if !events {
        sql.push_str(" AND command IN ('PRIVMSG', 'NOTICE')");
    }
    let mut args: Vec<crate::sql::Value> = vec![network_id.into(), target_cf.into()];
    if let Some(b) = after {
        let (c, v) = bound_clause(b, ">");
        sql.push_str(&format!(" AND {c}"));
        args.push(v.into());
    }
    if let Some(b) = before {
        let (c, v) = bound_clause(b, "<");
        sql.push_str(&format!(" AND {c}"));
        args.push(v.into());
    }
    sql.push_str(if newest_first_window {
        " ORDER BY ts DESC, id DESC LIMIT ?"
    } else {
        " ORDER BY ts ASC, id ASC LIMIT ?"
    });
    args.push((limit as i64).into());
    let mut st = conn.prepare(&sql)?;
    let rows = st.query_map(params_from_iter(args), row_msg)?;
    let mut v: Vec<StoredMessage> = rows.collect::<SqlResult<_>>()?;
    if newest_first_window {
        v.reverse();
    }
    Ok(v)
}

/// Looks up a message id and returns it only if it belongs to the network.
pub fn message_exists(conn: &Connection, network_id: i64, id: i64) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM messages WHERE id = ?1 AND network_id = ?2",
            params![id, network_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// Conversations with activity between two timestamps (CHATHISTORY TARGETS).
pub fn history_targets(
    conn: &Connection,
    network_id: i64,
    from_ts: i64,
    to_ts: i64,
    limit: usize,
) -> Result<Vec<(String, i64)>> {
    let (lo, hi) = if from_ts <= to_ts {
        (from_ts, to_ts)
    } else {
        (to_ts, from_ts)
    };
    let mut st = conn.prepare(
        "SELECT MAX(target), MAX(ts) AS last FROM messages WHERE network_id = ?1 AND ts > ?2 AND ts < ?3 \
         AND command IN ('PRIVMSG', 'NOTICE') GROUP BY target_cf ORDER BY last ASC LIMIT ?4",
    )?;
    let rows = st.query_map(params![network_id, lo, hi, limit as i64], |r| {
        Ok((r.get(0)?, r.get(1)?))
    })?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

/// Messages that mentioned the user, newest first, across networks.
pub fn mentions(
    conn: &Connection,
    network_ids: &[i64],
    before_ts: Option<i64>,
    limit: usize,
) -> Result<Vec<StoredMessage>> {
    if network_ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids = network_ids
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let mut st = conn.prepare(&format!(
        "SELECT {MSG_COLS} FROM messages WHERE mention = 1 AND network_id IN ({ids}) AND ts < ?1
         ORDER BY ts DESC, id DESC LIMIT ?2"
    ))?;
    let rows = st.query_map(
        params![before_ts.unwrap_or(i64::MAX), limit as i64],
        row_msg,
    )?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

/// Deletes messages older than `ts`, except in channels with their own
/// retention. Returns the number removed.
pub fn purge_messages_before(conn: &Connection, ts: i64) -> Result<usize> {
    Ok(conn.execute(
        "DELETE FROM messages WHERE ts < ?1 AND NOT EXISTS (
            SELECT 1 FROM channels c WHERE c.network_id = messages.network_id
            AND c.name_cf = messages.target_cf AND c.retention_days IS NOT NULL)",
        [ts],
    )?)
}

/// Applies the retention of channels that have their own (N days, or -1:
/// nothing kept). Returns the number of messages removed.
pub fn purge_channel_retention(conn: &Connection, now: i64) -> Result<usize> {
    Ok(conn.execute(
        "DELETE FROM messages WHERE EXISTS (
            SELECT 1 FROM channels c WHERE c.network_id = messages.network_id
            AND c.name_cf = messages.target_cf AND c.retention_days IS NOT NULL
            AND c.retention_days != 0
            AND (c.retention_days < 0 OR messages.ts < ?1 - c.retention_days * 86400000))",
        [now],
    )?)
}

/// Sets a channel's own retention (see [`Channel::retention_days`]).
pub fn set_channel_retention(
    conn: &Connection,
    network_id: i64,
    name_cf: &str,
    days: Option<i64>,
) -> Result<bool> {
    if let Some(d) = days {
        if !(-1..=36_500).contains(&d) {
            bail!("retention must be between -1 and 36500 days");
        }
    }
    Ok(conn.execute(
        "UPDATE channels SET retention_days = ?3 WHERE network_id = ?1 AND name_cf = ?2",
        params![network_id, name_cf, days],
    )? > 0)
}

// --------------------------------------------------------------- cursors

pub fn get_cursor(conn: &Connection, network_id: i64, client: &str) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT last_id FROM client_cursors WHERE network_id = ?1 AND client = ?2",
            params![network_id, client],
            |r| r.get(0),
        )
        .optional()?)
}

/// Stores a cursor; it never moves backwards.
pub fn set_cursor(conn: &Connection, network_id: i64, client: &str, last_id: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO client_cursors (network_id, client, last_id, updated_at) VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(network_id, client) DO UPDATE SET \
           last_id = MAX(client_cursors.last_id, excluded.last_id), updated_at = excluded.updated_at",
        params![network_id, client, last_id, now_ms()],
    )?;
    Ok(())
}

// ----------------------------------------------------------------- audit

pub fn audit(conn: &Connection, actor: &str, action: &str, detail: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO audit_log (ts, actor, action, detail) VALUES (?1, ?2, ?3, ?4)",
        params![now_ms(), actor, action, detail],
    )?;
    Ok(())
}

pub fn recent_audit(conn: &Connection, limit: usize) -> Result<Vec<(i64, String, String, String)>> {
    let mut st =
        conn.prepare("SELECT ts, actor, action, detail FROM audit_log ORDER BY id DESC LIMIT ?1")?;
    let rows = st.query_map([limit as i64], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
    })?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

/// Online backup to `dest` (safe while the daemon is running).
pub fn backup_to(conn: &Connection, dest: &Path) -> Result<()> {
    if dest.exists() {
        bail!("{} already exists; refusing to overwrite", dest.display());
    }
    let conn = conn
        .lite()
        .context("online backups are for SQLite; with PostgreSQL use pg_dump")?;
    let mut out = rusqlite::Connection::open(dest)?;
    {
        let backup = rusqlite::backup::Backup::new(conn, &mut out)?;
        backup.run_to_completion(256, std::time::Duration::from_millis(10), None)?;
    }
    let check: String = out.query_row("PRAGMA integrity_check", (), |r| r.get(0))?;
    if check != "ok" {
        bail!("backup integrity check failed: {check}");
    }
    Ok(())
}

// ------------------------------------------------- users (v0.2 additions)

/// Fields an administrator can change on a user. `None` leaves a field as is.
#[derive(Debug, Clone, Default)]
pub struct UserUpdate {
    pub role: Option<String>,
    pub enabled: Option<bool>,
    /// `Some(None)` clears the override.
    pub max_networks: Option<Option<i64>>,
    pub max_clients: Option<Option<i64>>,
    pub max_history_mb: Option<Option<i64>>,
}

pub fn update_user(conn: &Connection, id: i64, u: &UserUpdate) -> Result<bool> {
    let Some(cur) = get_user_by_id(conn, id)? else {
        return Ok(false);
    };
    let role = u.role.clone().unwrap_or(cur.role);
    let enabled = u.enabled.unwrap_or(cur.enabled);
    let max_networks = u.max_networks.unwrap_or(cur.max_networks);
    let max_clients = u.max_clients.unwrap_or(cur.max_clients);
    let max_history_mb = u.max_history_mb.unwrap_or(cur.max_history_mb);
    for v in [max_networks, max_clients].into_iter().flatten() {
        if !(0..=10_000).contains(&v) {
            bail!("limits must be between 0 and 10000");
        }
    }
    if max_history_mb.is_some_and(|v| !(1..=1_000_000).contains(&v)) {
        bail!("the history quota must be between 1 and 1000000 MB");
    }
    let admin = role == "superadmin" || role == "admin";
    conn.execute(
        "UPDATE users SET role = ?2, is_admin = ?3, enabled = ?4, max_networks = ?5, max_clients = ?6, max_history_mb = ?7 WHERE id = ?1",
        params![id, role, admin, enabled, max_networks, max_clients, max_history_mb],
    )?;
    Ok(true)
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Usage {
    pub networks: i64,
    pub messages: i64,
    /// Approximate size of the stored history.
    pub bytes: i64,
}

/// Approximate bytes per stored message on top of its text (row, indexes).
const MSG_OVERHEAD: i64 = 120;

pub fn user_usage(conn: &Connection, user_id: i64) -> Result<Usage> {
    Ok(conn.query_row(
        "SELECT (SELECT COUNT(*) FROM networks WHERE user_id = ?1),
                COUNT(m.id),
                COALESCE(SUM(LENGTH(m.text) + LENGTH(m.sender) + LENGTH(m.target)), 0) + COUNT(m.id) * ?2
         FROM messages m JOIN networks n ON n.id = m.network_id WHERE n.user_id = ?1",
        params![user_id, MSG_OVERHEAD],
        |r| Ok(Usage { networks: r.get(0)?, messages: r.get(1)?, bytes: r.get(2)? }),
    )?)
}

/// Deletes the oldest messages of users over their history quota, down to
/// 90% of it. Returns (user, messages deleted).
pub fn enforce_history_quotas(conn: &Connection) -> Result<Vec<(String, usize)>> {
    let mut st = conn.prepare(
        "SELECT id, username, max_history_mb FROM users WHERE max_history_mb IS NOT NULL",
    )?;
    let users: Vec<(i64, String, i64)> = st
        .query_map((), |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<SqlResult<_>>()?;
    let mut out = Vec::new();
    for (id, name, mb) in users {
        let limit = mb * 1024 * 1024;
        let usage = user_usage(conn, id)?;
        if usage.bytes <= limit {
            continue;
        }
        let target = limit * 9 / 10;
        let avg = (usage.bytes / usage.messages.max(1)).max(1);
        let excess = ((usage.bytes - target) / avg).max(1);
        let n = conn.execute(
            "DELETE FROM messages WHERE id IN (
                SELECT m.id FROM messages m JOIN networks n ON n.id = m.network_id
                WHERE n.user_id = ?1 ORDER BY m.ts, m.id LIMIT ?2)",
            params![id, excess],
        )?;
        out.push((name, n));
    }
    Ok(out)
}

// ---------------------------------------------------------------- avatars

/// Stores a user's profile photo (already checked by `crate::avatar`).
pub fn set_avatar(conn: &Connection, user_id: i64, mime: &str, data: &[u8]) -> Result<i64> {
    let now = now_ms();
    conn.execute("DELETE FROM avatars WHERE user_id = ?1", [user_id])?;
    conn.execute(
        "INSERT INTO avatars (user_id, mime, data, updated_at) VALUES (?1, ?2, ?3, ?4)",
        params![user_id, mime, data, now],
    )?;
    Ok(now)
}

/// (MIME type, bytes, last change) of a user's photo.
pub fn get_avatar(conn: &Connection, user_id: i64) -> Result<Option<(String, Vec<u8>, i64)>> {
    Ok(conn
        .query_row(
            "SELECT mime, data, updated_at FROM avatars WHERE user_id = ?1",
            [user_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?)
}

pub fn delete_avatar(conn: &Connection, user_id: i64) -> Result<bool> {
    Ok(conn.execute("DELETE FROM avatars WHERE user_id = ?1", [user_id])? > 0)
}

/// Last change of every photo, by user id (to build their addresses).
pub fn avatar_versions(conn: &Connection) -> Result<std::collections::HashMap<i64, i64>> {
    let mut st = conn.prepare("SELECT user_id, updated_at FROM avatars")?;
    let rows = st.query_map((), |r| Ok((r.get::<i64>(0)?, r.get::<i64>(1)?)))?;
    let mut out = std::collections::HashMap::new();
    for r in rows {
        let (k, v) = r?;
        out.insert(k, v);
    }
    Ok(out)
}

pub fn delete_user_by_id(conn: &Connection, id: i64) -> Result<bool> {
    Ok(conn.execute("DELETE FROM users WHERE id = ?1", [id])? > 0)
}

pub fn set_password_by_id(conn: &Connection, id: i64, hash: &str) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE users SET password_hash = ?2 WHERE id = ?1",
        params![id, hash],
    )? > 0)
}

pub fn set_user_role(conn: &Connection, username: &str, role: &str) -> Result<bool> {
    let admin = role == "superadmin" || role == "admin";
    Ok(conn.execute(
        "UPDATE users SET role = ?2, is_admin = ?3 WHERE username = ?1",
        params![username, role, admin],
    )? > 0)
}

/// Stores a TOTP secret (encrypted) and whether it is active.
pub fn set_totp(conn: &Connection, id: i64, secret_enc: Option<&str>, enabled: bool) -> Result<()> {
    conn.execute(
        "UPDATE users SET totp_secret = ?2, totp_enabled = ?3 WHERE id = ?1",
        params![id, secret_enc, enabled],
    )?;
    Ok(())
}

pub fn set_last_login(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE users SET last_login_at = ?2 WHERE id = ?1",
        params![id, now_ms()],
    )?;
    Ok(())
}

pub fn count_users(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM users", (), |r| r.get(0))?)
}

pub fn count_networks(conn: &Connection, user_id: i64) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM networks WHERE user_id = ?1",
        [user_id],
        |r| r.get(0),
    )?)
}

// --------------------------------------------- networks (v0.2 additions)

/// Editable network fields. Secrets: `None` keeps the stored value,
/// `Some(None)` clears it, `Some(Some(enc))` replaces it.
#[derive(Debug, Clone, Default)]
pub struct NetworkUpdate {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub tls: Option<bool>,
    pub nick: Option<String>,
    pub username: Option<String>,
    pub realname: Option<String>,
    pub enabled: Option<bool>,
    pub server_password_enc: Option<Option<String>>,
    pub sasl_username: Option<Option<String>>,
    pub sasl_password_enc: Option<Option<String>>,
}

pub fn update_network(conn: &Connection, id: i64, u: &NetworkUpdate) -> Result<Option<Network>> {
    let Some(mut n) = get_network_by_id(conn, id)? else {
        return Ok(None);
    };
    if let Some(v) = &u.name {
        n.name = v.clone();
    }
    if let Some(v) = &u.host {
        n.host = v.clone();
    }
    if let Some(v) = u.port {
        n.port = v;
    }
    if let Some(v) = u.tls {
        n.tls = v;
    }
    if let Some(v) = &u.nick {
        n.nick = v.clone();
    }
    if let Some(v) = &u.username {
        n.username = v.clone();
    }
    if let Some(v) = &u.realname {
        n.realname = v.clone();
    }
    if let Some(v) = u.enabled {
        n.enabled = v;
    }
    if let Some(v) = &u.server_password_enc {
        n.server_password_enc = v.clone();
    }
    if let Some(v) = &u.sasl_username {
        n.sasl_username = v.clone();
    }
    if let Some(v) = &u.sasl_password_enc {
        n.sasl_password_enc = v.clone();
    }
    if n.sasl_username.is_some() != n.sasl_password_enc.is_some() {
        bail!("SASL needs both an account name and a password");
    }
    validate_network_fields(&n.name, &n.host, n.port, &n.nick, &n.username, &n.realname)?;
    conn.execute(
        "UPDATE networks SET name = ?2, host = ?3, port = ?4, tls = ?5, nick = ?6, username = ?7, \
         realname = ?8, enabled = ?9, server_password = ?10, sasl_username = ?11, sasl_password = ?12 \
         WHERE id = ?1",
        params![
            id, n.name, n.host, n.port as i64, n.tls, n.nick, n.username, n.realname, n.enabled,
            n.server_password_enc, n.sasl_username, n.sasl_password_enc
        ],
    )
    .map_err(|e| match e {
        SqlError::Constraint(_) => {
            anyhow::anyhow!("a network named {} already exists for this user", n.name)
        }
        e => e.into(),
    })?;
    Ok(Some(n))
}

pub fn delete_network_by_id(conn: &Connection, id: i64) -> Result<bool> {
    Ok(conn.execute("DELETE FROM networks WHERE id = ?1", [id])? > 0)
}

// ------------------------------------------------------- web sessions

#[derive(Debug, Clone)]
pub struct WebSession {
    pub id: i64,
    pub user_id: i64,
    pub csrf: String,
    pub created_at: i64,
    pub last_seen: i64,
    pub expires_at: i64,
    pub ip: String,
    pub user_agent: String,
}

fn row_session(r: &Row<'_>) -> SqlResult<WebSession> {
    Ok(WebSession {
        id: r.get(0)?,
        user_id: r.get(1)?,
        csrf: r.get(2)?,
        created_at: r.get(3)?,
        last_seen: r.get(4)?,
        expires_at: r.get(5)?,
        ip: r.get(6)?,
        user_agent: r.get(7)?,
    })
}

const SESSION_COLS: &str = "id, user_id, csrf, created_at, last_seen, expires_at, ip, user_agent";

pub fn create_session(
    conn: &Connection,
    token_hash: &str,
    user_id: i64,
    csrf: &str,
    ip: &str,
    user_agent: &str,
    ttl_ms: i64,
) -> Result<i64> {
    let now = now_ms();
    conn.execute(
        "INSERT INTO web_sessions (token_hash, user_id, csrf, created_at, last_seen, expires_at, ip, user_agent) \
         VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7)",
        params![token_hash, user_id, csrf, now, now + ttl_ms, ip, user_agent],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Returns a valid (not expired) session and extends it (sliding expiry).
pub fn touch_session(
    conn: &Connection,
    token_hash: &str,
    ttl_ms: i64,
) -> Result<Option<WebSession>> {
    let now = now_ms();
    let s = conn
        .query_row(
            &format!(
                "SELECT {SESSION_COLS} FROM web_sessions WHERE token_hash = ?1 AND expires_at > ?2"
            ),
            params![token_hash, now],
            row_session,
        )
        .optional()?;
    if let Some(sess) = &s {
        if now - sess.last_seen > 60_000 {
            conn.execute(
                "UPDATE web_sessions SET last_seen = ?2, expires_at = ?3 WHERE id = ?1",
                params![sess.id, now, now + ttl_ms],
            )?;
        }
    }
    Ok(s)
}

pub fn delete_session_by_token(conn: &Connection, token_hash: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM web_sessions WHERE token_hash = ?1",
        [token_hash],
    )?;
    Ok(())
}

pub fn delete_session(conn: &Connection, id: i64) -> Result<bool> {
    Ok(conn.execute("DELETE FROM web_sessions WHERE id = ?1", [id])? > 0)
}

pub fn get_session(conn: &Connection, id: i64) -> Result<Option<WebSession>> {
    Ok(conn
        .query_row(
            &format!("SELECT {SESSION_COLS} FROM web_sessions WHERE id = ?1"),
            [id],
            row_session,
        )
        .optional()?)
}

/// Deletes every session of a user, optionally keeping one.
pub fn delete_user_sessions(conn: &Connection, user_id: i64, keep: Option<i64>) -> Result<usize> {
    Ok(conn.execute(
        "DELETE FROM web_sessions WHERE user_id = ?1 AND id != ?2",
        params![user_id, keep.unwrap_or(-1)],
    )?)
}

pub fn list_sessions(conn: &Connection, user_id: Option<i64>) -> Result<Vec<(WebSession, String)>> {
    let cols = SESSION_COLS
        .split(", ")
        .map(|c| format!("s.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut st = conn.prepare(&format!(
        "SELECT {cols}, u.username FROM web_sessions s JOIN users u ON u.id = s.user_id \
         WHERE s.expires_at > ?1 AND (CAST(?2 AS BIGINT) IS NULL OR s.user_id = ?2) ORDER BY s.last_seen DESC"
    ))?;
    let rows = st.query_map(params![now_ms(), user_id], |r| {
        Ok((row_session(r)?, r.get(8)?))
    })?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

pub fn purge_expired_sessions(conn: &Connection) -> Result<usize> {
    Ok(conn.execute(
        "DELETE FROM web_sessions WHERE expires_at <= ?1",
        [now_ms()],
    )?)
}

// ------------------------------------------------------------ settings

pub fn load_settings_map(conn: &Connection) -> Result<std::collections::HashMap<String, String>> {
    let mut st = conn.prepare("SELECT key, value FROM settings")?;
    let rows = st.query_map((), |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}

// ------------------------------------------------------- notifications

pub fn notify_settings(conn: &Connection, user_id: i64) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT config FROM notify_settings WHERE user_id = ?1",
            [user_id],
            |r| r.get(0),
        )
        .optional()?)
}

pub fn set_notify_settings(conn: &Connection, user_id: i64, enc: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO notify_settings (user_id, config) VALUES (?1, ?2)
         ON CONFLICT(user_id) DO UPDATE SET config = excluded.config",
        params![user_id, enc],
    )?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PushSubscription {
    pub endpoint: String,
    #[serde(skip)]
    pub p256dh: String,
    #[serde(skip)]
    pub auth: String,
    pub label: String,
    pub created_at: i64,
}

pub fn push_subscriptions(conn: &Connection, user_id: i64) -> Result<Vec<PushSubscription>> {
    let mut st = conn.prepare(
        "SELECT endpoint, p256dh, auth, label, created_at FROM push_subscriptions WHERE user_id = ?1 ORDER BY id",
    )?;
    let rows = st.query_map([user_id], |r| {
        Ok(PushSubscription {
            endpoint: r.get(0)?,
            p256dh: r.get(1)?,
            auth: r.get(2)?,
            label: r.get(3)?,
            created_at: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

/// Adds (or moves to this user) a push subscription; at most 20 per user.
pub fn add_push_subscription(conn: &Connection, user_id: i64, s: &PushSubscription) -> Result<()> {
    conn.execute(
        "DELETE FROM push_subscriptions WHERE endpoint = ?1",
        [&s.endpoint],
    )?;
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM push_subscriptions WHERE user_id = ?1",
        [user_id],
        |r| r.get(0),
    )?;
    if n >= 20 {
        bail!("too many devices with notifications (20)");
    }
    conn.execute(
        "INSERT INTO push_subscriptions (user_id, endpoint, p256dh, auth, label, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![user_id, s.endpoint, s.p256dh, s.auth, s.label, now_ms()],
    )?;
    Ok(())
}

pub fn delete_push_subscription(conn: &Connection, user_id: i64, endpoint: &str) -> Result<bool> {
    Ok(conn.execute(
        "DELETE FROM push_subscriptions WHERE user_id = ?1 AND endpoint = ?2",
        params![user_id, endpoint],
    )? > 0)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Passkey {
    pub id: i64,
    pub user_id: i64,
    pub cred_id: String,
    #[serde(skip)]
    pub public_key: String,
    #[serde(skip)]
    pub sign_count: i64,
    pub label: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

const PASSKEY_COLS: &str =
    "id, user_id, cred_id, public_key, sign_count, label, created_at, last_used_at";

fn passkey_row(r: &Row) -> SqlResult<Passkey> {
    Ok(Passkey {
        id: r.get(0)?,
        user_id: r.get(1)?,
        cred_id: r.get(2)?,
        public_key: r.get(3)?,
        sign_count: r.get(4)?,
        label: r.get(5)?,
        created_at: r.get(6)?,
        last_used_at: r.get(7)?,
    })
}

pub fn passkeys_of(conn: &Connection, user_id: i64) -> Result<Vec<Passkey>> {
    let mut st = conn.prepare(&format!(
        "SELECT {PASSKEY_COLS} FROM passkeys WHERE user_id = ?1 ORDER BY id"
    ))?;
    let rows = st.query_map([user_id], passkey_row)?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

pub fn passkey_by_cred(conn: &Connection, cred_id: &str) -> Result<Option<Passkey>> {
    Ok(conn
        .query_row(
            &format!("SELECT {PASSKEY_COLS} FROM passkeys WHERE cred_id = ?1"),
            [cred_id],
            passkey_row,
        )
        .optional()?)
}

/// Stores a new passkey; at most 20 per user.
pub fn add_passkey(
    conn: &Connection,
    user_id: i64,
    cred_id: &str,
    public_key: &str,
    sign_count: i64,
    label: &str,
) -> Result<i64> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM passkeys WHERE user_id = ?1",
        [user_id],
        |r| r.get(0),
    )?;
    if n >= 20 {
        bail!("too many passkeys (20)");
    }
    conn.execute(
        "INSERT INTO passkeys (user_id, cred_id, public_key, sign_count, label, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![user_id, cred_id, public_key, sign_count, label, now_ms()],
    )
    .map_err(|e| match e {
        SqlError::Constraint(_) => {
            anyhow::anyhow!("this passkey is already registered")
        }
        e => e.into(),
    })?;
    Ok(conn.last_insert_rowid())
}

pub fn touch_passkey(conn: &Connection, id: i64, sign_count: i64) -> Result<()> {
    conn.execute(
        "UPDATE passkeys SET sign_count = ?2, last_used_at = ?3 WHERE id = ?1",
        params![id, sign_count, now_ms()],
    )?;
    Ok(())
}

pub fn delete_passkey(conn: &Connection, user_id: i64, id: i64) -> Result<bool> {
    Ok(conn.execute(
        "DELETE FROM passkeys WHERE id = ?1 AND user_id = ?2",
        params![id, user_id],
    )? > 0)
}

/// The local user linked to an outside identity (OIDC subject, LDAP dn).
pub fn identity_user(conn: &Connection, provider: &str, subject: &str) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT user_id FROM user_identities WHERE provider = ?1 AND subject = ?2",
            params![provider, subject],
            |r| r.get(0),
        )
        .optional()?)
}

pub fn link_identity(conn: &Connection, provider: &str, subject: &str, user_id: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO user_identities (provider, subject, user_id, created_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(provider, subject) DO UPDATE SET user_id = excluded.user_id",
        params![provider, subject, user_id, now_ms()],
    )?;
    Ok(())
}

pub fn identities_of(conn: &Connection, user_id: i64) -> Result<Vec<(String, String, i64)>> {
    let mut st = conn.prepare(
        "SELECT provider, subject, created_at FROM user_identities WHERE user_id = ?1 ORDER BY created_at",
    )?;
    let rows = st.query_map([user_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

pub fn unlink_identity(
    conn: &Connection,
    user_id: i64,
    provider: &str,
    subject: &str,
) -> Result<bool> {
    Ok(conn.execute(
        "DELETE FROM user_identities WHERE user_id = ?1 AND provider = ?2 AND subject = ?3",
        params![user_id, provider, subject],
    )? > 0)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

// -------------------------------------------------------- login events

#[derive(Debug, Clone)]
pub struct LoginEvent {
    pub ts: i64,
    pub ip: String,
    pub username: String,
    pub source: String,
    pub success: bool,
    pub reason: String,
}

pub fn record_login(
    conn: &Connection,
    ip: &str,
    username: &str,
    source: &str,
    success: bool,
    reason: &str,
) -> Result<()> {
    let username: String = username.chars().take(64).collect();
    conn.execute(
        "INSERT INTO login_events (ts, ip, username, source, success, reason) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![now_ms(), ip, username, source, success, reason],
    )?;
    // Keep the table bounded.
    conn.execute(
        "DELETE FROM login_events WHERE id <= (SELECT MAX(id) - 10000 FROM login_events)",
        (),
    )?;
    Ok(())
}

pub fn recent_logins(
    conn: &Connection,
    limit: usize,
    only_failures: bool,
) -> Result<Vec<LoginEvent>> {
    let mut st = conn.prepare(
        "SELECT ts, ip, username, source, success, reason FROM login_events \
         WHERE (?2 = 0 OR success = 0) ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = st.query_map(params![limit as i64, only_failures], |r| {
        Ok(LoginEvent {
            ts: r.get(0)?,
            ip: r.get(1)?,
            username: r.get(2)?,
            source: r.get(3)?,
            success: r.get(4)?,
            reason: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

pub fn count_failed_logins_since(conn: &Connection, since_ms: i64) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM login_events WHERE success = 0 AND ts >= ?1",
        [since_ms],
        |r| r.get(0),
    )?)
}

// -------------------------------------------- history browsing & search

/// Newest-first page of a network's history, optionally filtered by
/// conversation, text and sender. `before_id` pages backwards.
/// Filters for [`search_messages`].
#[derive(Debug, Clone, Copy, Default)]
pub struct HistoryFilter<'a> {
    pub target_cf: Option<&'a str>,
    /// Substring of the message text.
    pub text: Option<&'a str>,
    /// Prefix of the sender (`nick!user@host`).
    pub sender: Option<&'a str>,
    pub before_id: Option<i64>,
    pub from_ts: Option<i64>,
    pub to_ts: Option<i64>,
    pub limit: usize,
}

/// Searches the history. With SQLite, words are matched as word prefixes
/// first (fast, accent-insensitive); when that finds nothing, every word is
/// looked for anywhere inside the text, so fragments like `ploy` find
/// «deploy».
pub fn search_messages(
    conn: &Connection,
    network_id: i64,
    f: &HistoryFilter<'_>,
) -> Result<Vec<StoredMessage>> {
    let found = search_impl(conn, network_id, f, false)?;
    let words = f.text.is_some_and(|q| fts_query(q).is_some());
    if found.is_empty() && words && !conn.is_postgres() {
        return search_impl(conn, network_id, f, true);
    }
    Ok(found)
}

fn search_impl(
    conn: &Connection,
    network_id: i64,
    f: &HistoryFilter<'_>,
    substring: bool,
) -> Result<Vec<StoredMessage>> {
    let HistoryFilter {
        target_cf,
        text,
        sender,
        before_id,
        from_ts,
        to_ts,
        limit,
    } = *f;
    let mut sql = format!("SELECT {MSG_COLS} FROM messages WHERE network_id = ?");
    let mut args: Vec<crate::sql::Value> = vec![network_id.into()];
    if let Some(t) = target_cf {
        sql.push_str(" AND target_cf = ?");
        args.push(t.to_string().into());
    }
    if let Some(q) = text.filter(|q| !q.is_empty()) {
        match fts_query(q) {
            Some(_) if conn.is_postgres() || substring => {
                // Every word (or phrase in quotes) must appear somewhere in
                // the text (accents count).
                for w in substring_terms(q).iter().take(10) {
                    sql.push_str(" AND text LIKE ? ESCAPE '\\'");
                    args.push(format!("%{}%", escape_like(w)).into());
                }
            }
            Some(fq) => {
                sql.push_str(
                    " AND id IN (SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?)",
                );
                args.push(fq.into());
            }
            None => {
                // Only punctuation or symbols: plain substring search.
                sql.push_str(" AND text LIKE ? ESCAPE '\\'");
                args.push(format!("%{}%", escape_like(q)).into());
            }
        }
    }
    if let Some(q) = sender.filter(|q| !q.is_empty()) {
        sql.push_str(" AND sender LIKE ? ESCAPE '\\'");
        args.push(format!("{}%", escape_like(q)).into());
    }
    if let Some(b) = before_id {
        // Pages follow time order (imported history has new ids but old times).
        sql.push_str(" AND (ts, id) < (SELECT ts, id FROM messages WHERE id = ?)");
        args.push(b.into());
    }
    if let Some(t) = from_ts {
        sql.push_str(" AND ts >= ?");
        args.push(t.into());
    }
    if let Some(t) = to_ts {
        sql.push_str(" AND ts <= ?");
        args.push(t.into());
    }
    sql.push_str(" ORDER BY ts DESC, id DESC LIMIT ?");
    args.push((limit as i64).into());
    let mut st = conn.prepare(&sql)?;
    let rows = st.query_map(params_from_iter(args), row_msg)?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

/// Words and quoted phrases of a search, for substring matching.
fn substring_terms(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, part) in input.split('"').enumerate() {
        if i % 2 == 1 {
            let p = part.trim();
            if !p.is_empty() {
                out.push(p.to_string());
            }
        } else {
            out.extend(part.split_whitespace().map(str::to_string));
        }
    }
    out
}

/// Turns what a person types into a safe FTS5 query: every word must appear
/// (as a word prefix, accents ignored); a phrase in double quotes must
/// appear as written. FTS5 syntax characters are never passed through.
pub fn fts_query(input: &str) -> Option<String> {
    let mut parts = Vec::new();
    let mut rest = input;
    while let Some(start) = rest.find('"') {
        let (before, after) = rest.split_at(start);
        words_into(before, &mut parts);
        match after[1..].find('"') {
            Some(end) => {
                let phrase: Vec<&str> = after[1..1 + end]
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| !w.is_empty())
                    .collect();
                if !phrase.is_empty() {
                    parts.push(format!("\"{}\"", phrase.join(" ")));
                }
                rest = &after[2 + end..];
            }
            None => {
                rest = &after[1..];
            }
        }
    }
    words_into(rest, &mut parts);
    if parts.is_empty() {
        return None;
    }
    parts.truncate(16);
    Some(parts.join(" "))
}

fn words_into(text: &str, parts: &mut Vec<String>) {
    for w in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        parts.push(format!("\"{w}\"*"));
    }
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Conversations of a network with message counts and last activity.
pub fn conversation_list(conn: &Connection, network_id: i64) -> Result<Vec<(String, i64, i64)>> {
    let mut st = conn.prepare(
        "SELECT MAX(target), COUNT(*), MAX(ts) FROM messages WHERE network_id = ?1 \
         GROUP BY target_cf ORDER BY MAX(ts) DESC LIMIT 500",
    )?;
    let rows = st.query_map([network_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    Ok(rows.collect::<SqlResult<_>>()?)
}

/// Deletes the history of one conversation (or the whole network).
pub fn delete_history(
    conn: &Connection,
    network_id: i64,
    target_cf: Option<&str>,
) -> Result<usize> {
    Ok(match target_cf {
        Some(t) => conn.execute(
            "DELETE FROM messages WHERE network_id = ?1 AND target_cf = ?2",
            params![network_id, t],
        )?,
        None => conn.execute("DELETE FROM messages WHERE network_id = ?1", [network_id])?,
    })
}

/// Database-wide figures for the dashboard.
pub fn storage_stats(conn: &Connection) -> Result<(i64, i64, i64)> {
    let messages: i64 = conn.query_row("SELECT COUNT(*) FROM messages", (), |r| r.get(0))?;
    let bytes: i64 = if conn.is_postgres() {
        conn.query_row("SELECT pg_database_size(current_database())", (), |r| {
            r.get(0)
        })?
    } else {
        let pages: i64 = conn.query_row("PRAGMA page_count", (), |r| r.get(0))?;
        let size: i64 = conn.query_row("PRAGMA page_size", (), |r| r.get(0))?;
        pages * size
    };
    let networks: i64 = conn.query_row("SELECT COUNT(*) FROM networks", (), |r| r.get(0))?;
    Ok((messages, bytes, networks))
}

pub fn messages_per_user(conn: &Connection, user_id: i64) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM messages m JOIN networks n ON n.id = m.network_id WHERE n.user_id = ?1",
        [user_id],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hq(
        t: &str,
        after: Option<Bound>,
        before: Option<Bound>,
        limit: usize,
        newest: bool,
    ) -> HistQuery {
        HistQuery {
            target_cf: t.into(),
            after,
            before,
            limit,
            newest_first_window: newest,
            events: true,
        }
    }

    #[test]
    fn fts_query_is_safe() {
        assert_eq!(fts_query("hola mundo").unwrap(), "\"hola\"* \"mundo\"*");
        assert_eq!(
            fts_query("say \"good night\" now").unwrap(),
            "\"say\"* \"good night\" \"now\"*"
        );
        assert_eq!(fts_query("a\"b").unwrap(), "\"a\"* \"b\"*");
        assert_eq!(fts_query("*** ---"), None);
        assert_eq!(
            fts_query("x OR y NOT z").unwrap(),
            "\"x\"* \"OR\"* \"y\"* \"NOT\"* \"z\"*"
        );
    }

    fn setup() -> (Db, i64, i64) {
        let db = Db::open_in_memory().unwrap();
        let (u, n) = db
            .with(|c| {
                let u = create_user(c, "alice", "hash", false)?;
                let n = create_network(
                    c,
                    u,
                    &NewNetwork {
                        name: "libera".into(),
                        host: "irc.libera.chat".into(),
                        port: 6697,
                        tls: true,
                        nick: "alice".into(),
                        username: "alice".into(),
                        realname: "Alice".into(),
                        server_password_enc: None,
                        sasl_username: None,
                        sasl_password_enc: None,
                    },
                    10,
                )?;
                Ok((u, n))
            })
            .unwrap();
        (db, u, n)
    }

    #[test]
    fn history_quota_and_mentions() {
        let (db, u, n) = setup();
        db.with(|c| {
            let long = "x".repeat(200);
            let tx = c.transaction()?;
            for i in 0..10_000 {
                let mut m = msg(n, "#xbnc-dev", i, &long);
                m.mention = i % 1000 == 0;
                insert_message(&tx, &m)?;
            }
            tx.commit()?;
            let before = user_usage(c, u)?;
            assert_eq!((before.networks, before.messages), (1, 10_000));
            assert!(before.bytes > 3_000_000, "{}", before.bytes);
            update_user(
                c,
                u,
                &UserUpdate {
                    max_history_mb: Some(Some(1)),
                    ..Default::default()
                },
            )?;
            let done = enforce_history_quotas(c)?;
            assert_eq!(done.len(), 1);
            let after = user_usage(c, u)?;
            assert!(
                after.bytes <= 1024 * 1024 && after.bytes > 800 * 1024,
                "{}",
                after.bytes
            );
            // The newest messages are the ones kept.
            let m = mentions(c, &[n], None, 100)?;
            assert!(m.first().is_some_and(|x| x.ts == 9000), "{m:?}");
            assert!(update_user(
                c,
                u,
                &UserUpdate {
                    max_history_mb: Some(Some(0)),
                    ..Default::default()
                }
            )
            .is_err());
            Ok(())
        })
        .unwrap();
    }

    fn msg(n: i64, target: &str, ts: i64, text: &str) -> NewMessage {
        NewMessage {
            network_id: n,
            target: target.into(),
            target_cf: target.to_lowercase(),
            ts,
            sender: "bob!b@h".into(),
            command: "PRIVMSG".into(),
            text: text.into(),
            outgoing: false,
            mention: false,
        }
    }

    #[test]
    fn migrations_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.db");
        Db::open(&p).unwrap();
        let db = Db::open(&p).unwrap();
        assert_eq!(
            db.with(|c| schema_version(c)).unwrap(),
            MIGRATIONS.len() as i64
        );
    }

    /// PostgreSQL reaches the same schema version as SQLite (a migration
    /// added to one list only would break PostgreSQL installs).
    #[test]
    fn postgres_migrations_follow_sqlite() {
        assert_eq!(PG_MIGRATIONS.last().unwrap().0, SCHEMA_VERSION);
        let sqlite_cols = ["options", "bind_host", "disabled", "position"];
        let pg = PG_MIGRATIONS.iter().map(|(_, s)| *s).collect::<String>();
        for c in sqlite_cols {
            assert!(
                pg.contains(&format!("ADD COLUMN {c} ")),
                "{c} missing in PostgreSQL"
            );
        }
    }

    /// A 1.0 beta database (schema 9) gains the new columns with neutral
    /// values: no backup servers, channels enabled and in their old order.
    #[test]
    fn upgrade_from_schema_9_keeps_channels() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.db");
        {
            let mut conn = rusqlite::Connection::open(&p).unwrap();
            let tx = conn.transaction().unwrap();
            for sql in &MIGRATIONS[..9] {
                tx.execute_batch(sql).unwrap();
            }
            tx.pragma_update(None, "user_version", 9).unwrap();
            tx.execute(
                "INSERT INTO users (username, password_hash, is_admin, created_at) VALUES ('old', 'h', 0, 0)",
                [],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO networks (user_id, name, host, port, tls, nick, username, realname, created_at) \
                 VALUES (1, 'net', 'irc.example.org', 6697, 1, 'old', 'old', 'Old', 0)",
                [],
            )
            .unwrap();
            for ch in ["#zeta", "#alpha"] {
                tx.execute(
                    "INSERT INTO channels (network_id, name, name_cf) VALUES (1, ?1, ?1)",
                    [ch],
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }
        let db = Db::open(&p).unwrap();
        db.with(|c| {
            assert_eq!(schema_version(c)?, SCHEMA_VERSION);
            let ch = list_channels(c, 1)?;
            assert_eq!(ch.len(), 2);
            assert!(ch.iter().all(|c| !c.disabled));
            assert!(get_net_options(c, 1)?.servers.is_empty());
            assert_eq!(get_user_bind_host(c, 1)?, "");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn users_and_names() {
        let (db, _, _) = setup();
        db.with(|c| {
            assert!(
                create_user(c, "ALICE", "h", false).is_err(),
                "case-insensitive unique"
            );
            assert!(create_user(c, "bad/name", "h", false).is_err());
            assert!(create_user(c, "", "h", false).is_err());
            assert!(get_user(c, "Alice")?.is_some());
            assert!(set_user_enabled(c, "alice", false)?);
            assert!(!get_user(c, "alice")?.unwrap().enabled);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn network_limits_and_cascade() {
        let (db, u, n) = setup();
        db.with(|c| {
            let mut nn = NewNetwork {
                name: "oftc".into(),
                host: "irc.oftc.net".into(),
                port: 6697,
                tls: true,
                nick: "alice".into(),
                username: "alice".into(),
                realname: "A".into(),
                server_password_enc: None,
                sasl_username: None,
                sasl_password_enc: None,
            };
            assert!(create_network(c, u, &nn, 1).is_err(), "limit enforced");
            nn.name = "libera".into();
            assert!(create_network(c, u, &nn, 10).is_err(), "duplicate");
            upsert_channel(c, n, "#Xbnc-Dev", "#xbnc-dev", Some("k1"), false)?;
            upsert_channel(c, n, "#xbnc-dev", "#xbnc-dev", None, false)?;
            assert_eq!(list_channels(c, n)?[0].key_enc.as_deref(), Some("k1"));
            insert_message(c, &msg(n, "#xbnc-dev", 1, "x"))?;
            assert_eq!(list_active_networks(c)?.len(), 1);
            delete_user(c, "alice")?;
            assert_eq!(list_channels(c, n)?.len(), 0);
            assert_eq!(max_message_id(c, n)?, 0);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn history_queries() {
        let (db, _, n) = setup();
        db.with(|c| {
            let mut ids = vec![];
            for i in 1..=10 {
                ids.push(insert_message(
                    c,
                    &msg(n, "#a", i * 1000, &format!("m{i}")),
                )?);
            }
            insert_message(c, &msg(n, "#b", 5500, "other"))?;
            let latest = history_query(c, n, &hq("#a", None, None, 3, true))?;
            assert_eq!(
                latest.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
                ["m8", "m9", "m10"]
            );
            let before =
                history_query(c, n, &hq("#a", None, Some(Bound::Timestamp(5000)), 2, true))?;
            assert_eq!(
                before.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
                ["m3", "m4"]
            );
            let after = history_query(c, n, &hq("#a", Some(Bound::MsgId(ids[7])), None, 5, false))?;
            assert_eq!(
                after.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
                ["m9", "m10"]
            );
            let between = history_query(
                c,
                n,
                &hq(
                    "#a",
                    Some(Bound::Timestamp(2000)),
                    Some(Bound::Timestamp(5000)),
                    10,
                    false,
                ),
            )?;
            assert_eq!(between.len(), 2);
            assert_eq!(messages_after_id(c, n, ids[8], 100)?.len(), 2);
            // Ordered by time, not by insertion.
            assert_eq!(latest_messages(c, n, 2)?.last().unwrap().text, "m10");
            // History imported later (old times, new ids) is not replayed.
            insert_message(c, &msg(n, "#a", 10_000 - 7_200_000, "imported"))?;
            let replay = messages_after_id(c, n, ids[9], 100)?;
            assert_eq!(
                replay.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
                ["other"]
            );
            let targets = history_targets(c, n, 0, 100_000, 10)?;
            assert_eq!(targets.len(), 2);
            assert_eq!(purge_messages_before(c, 3000)?, 3);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn cursor_never_goes_back() {
        let (db, _, n) = setup();
        db.with(|c| {
            assert_eq!(get_cursor(c, n, "laptop")?, None);
            set_cursor(c, n, "laptop", 10)?;
            set_cursor(c, n, "laptop", 5)?;
            assert_eq!(get_cursor(c, n, "laptop")?, Some(10));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn backup_works() {
        let dir = tempfile::tempdir().unwrap();
        let (db, _, _) = setup();
        let dest = dir.path().join("b.db");
        db.with(|c| backup_to(c, &dest)).unwrap();
        assert!(db.with(|c| backup_to(c, &dest)).is_err(), "no overwrite");
        let restored = Db::open(&dest).unwrap();
        assert!(restored.with(|c| get_user(c, "alice")).unwrap().is_some());
    }

    #[test]
    fn v2_features() {
        let (db, u, n) = setup();
        db.with(|c| {
            let user = get_user_by_id(c, u)?.unwrap();
            assert_eq!(user.role, "user");
            update_user(
                c,
                u,
                &UserUpdate {
                    role: Some("moderator".into()),
                    max_clients: Some(Some(3)),
                    ..Default::default()
                },
            )?;
            let user = get_user_by_id(c, u)?.unwrap();
            assert_eq!(
                (user.role.as_str(), user.max_clients),
                ("moderator", Some(3))
            );
            assert!(update_user(
                c,
                u,
                &UserUpdate {
                    max_networks: Some(Some(-1)),
                    ..Default::default()
                }
            )
            .is_err());

            let upd = NetworkUpdate {
                host: Some("irc.oftc.net".into()),
                port: Some(6697),
                ..Default::default()
            };
            assert_eq!(update_network(c, n, &upd)?.unwrap().host, "irc.oftc.net");
            let bad = NetworkUpdate {
                sasl_username: Some(Some("x".into())),
                ..Default::default()
            };
            assert!(
                update_network(c, n, &bad).is_err(),
                "SASL needs a password too"
            );

            let sid = create_session(c, "h1", u, "csrf", "127.0.0.1", "ua", 1000)?;
            assert!(touch_session(c, "h1", 1000)?.is_some());
            assert!(touch_session(c, "nope", 1000)?.is_none());
            assert_eq!(list_sessions(c, Some(u))?.len(), 1);
            assert!(delete_session(c, sid)?);

            set_setting(c, "history.retention_days", "7")?;
            set_setting(c, "history.retention_days", "8")?;
            assert_eq!(
                load_settings_map(c)?
                    .get("history.retention_days")
                    .map(String::as_str),
                Some("8")
            );

            record_login(c, "192.0.2.1", "alice", "web", false, "bad password")?;
            record_login(c, "192.0.2.1", "alice", "irc", true, "ok")?;
            assert_eq!(recent_logins(c, 10, true)?.len(), 1);
            assert_eq!(recent_logins(c, 10, false)?.len(), 2);

            for i in 0..5 {
                insert_message(c, &msg(n, "#a", 1000 + i, &format!("hola 100% mundo {i}")))?;
            }
            insert_message(c, &msg(n, "#b", 2000, "otro"))?;
            let all = search_messages(
                c,
                n,
                &HistoryFilter {
                    target_cf: None,
                    text: None,
                    sender: None,
                    before_id: None,
                    from_ts: None,
                    to_ts: None,
                    limit: 100,
                },
            )?;
            assert_eq!(all.len(), 6);
            let found = search_messages(
                c,
                n,
                &HistoryFilter {
                    target_cf: Some("#a"),
                    text: Some("100%"),
                    sender: None,
                    before_id: None,
                    from_ts: None,
                    to_ts: None,
                    limit: 2,
                },
            )?;
            assert_eq!(found.len(), 2);
            assert_eq!(found[0].text, "hola 100% mundo 4", "newest first");
            let page2 = search_messages(
                c,
                n,
                &HistoryFilter {
                    target_cf: Some("#a"),
                    text: None,
                    sender: None,
                    before_id: Some(found[1].id),
                    from_ts: None,
                    to_ts: None,
                    limit: 10,
                },
            )?;
            assert_eq!(page2.len(), 3);
            assert_eq!(
                search_messages(
                    c,
                    n,
                    &HistoryFilter {
                        target_cf: None,
                        text: Some("_"),
                        sender: None,
                        before_id: None,
                        from_ts: None,
                        to_ts: None,
                        limit: 10
                    }
                )?
                .len(),
                0,
                "LIKE wildcards are escaped"
            );
            assert_eq!(conversation_list(c, n)?.len(), 2);
            assert_eq!(delete_history(c, n, Some("#b"))?, 1);

            // Full-text search: words in any order, prefixes, accents ignored,
            // phrases in quotes, and the index follows deletions.
            insert_message(c, &msg(n, "#c", 3000, "El despliegue falló otra vez"))?;
            insert_message(c, &msg(n, "#c", 3001, "otra vez el café"))?;
            let q = |text: &'static str| HistoryFilter {
                target_cf: None,
                text: Some(text),
                sender: None,
                before_id: None,
                from_ts: None,
                to_ts: None,
                limit: 10,
            };
            assert_eq!(search_messages(c, n, &q("falló despliegue"))?.len(), 1);
            assert_eq!(search_messages(c, n, &q("desplieg"))?.len(), 1, "prefix");
            assert_eq!(search_messages(c, n, &q("fallo"))?.len(), 1, "accents");
            assert_eq!(search_messages(c, n, &q("CAFE"))?.len(), 1, "case");
            assert_eq!(
                search_messages(c, n, &q("spliegue"))?.len(),
                1,
                "fragments inside a word"
            );
            assert_eq!(search_messages(c, n, &q("\"otra vez\""))?.len(), 2);
            assert_eq!(
                search_messages(c, n, &q("\"vez otra\""))?.len(),
                0,
                "phrase order"
            );
            assert_eq!(
                search_messages(c, n, &q("NEAR(a b) OR *"))?.len(),
                0,
                "syntax is not passed through"
            );
            delete_history(c, n, Some("#c"))?;
            assert_eq!(search_messages(c, n, &q("despliegue"))?.len(), 0);
            Ok(())
        })
        .unwrap();
    }
}
