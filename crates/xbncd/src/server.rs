//! Process wiring: shared context, network registry, listeners and
//! background maintenance tasks.

use crate::auth::LoginLimiter;
use crate::config::{Config, ListenConfig};
use crate::crypto::SecretBox;
use crate::db::{self, Db};
use crate::downstream;
use crate::metrics::Metrics;
use crate::network::{self, NetCmd, NetHandle};
use crate::settings::SettingsStore;
use crate::util::BoxStream;
use anyhow::{bail, Context, Result};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

/// State shared by every task in the process.
pub struct Ctx {
    pub config: Config,
    pub db: Db,
    pub secrets: SecretBox,
    pub tls_client: Arc<rustls::ClientConfig>,
    /// System root certificates, for per-network configs with a client cert.
    pub tls_roots: Arc<rustls::RootCertStore>,
    pub limiter: LoginLimiter,
    pub registry: Registry,
    pub next_client_id: AtomicU64,
    pub connections: AtomicUsize,
    /// History and limits, editable at runtime from the web panel.
    pub settings: SettingsStore,
    pub metrics: Metrics,
    /// Process start time (ms since epoch).
    pub started_at: i64,
    /// One-time code required to create the first account from the web
    /// panel. Present only while there are no users.
    pub setup_token: Mutex<Option<String>>,
    /// Plain-text log writer (the `log` module).
    pub logs: crate::logs::LogSink,
    /// Active alerts.
    pub alerts: crate::alerts::Alerts,
    /// Installed WebAssembly plugins.
    pub plugins: crate::plugins::PluginRegistry,
    /// DCC relay totals.
    pub dcc: Arc<crate::dcc::DccStats>,
    /// Latest published version, when update checks are on.
    pub updates: crate::updates::Updates,
    /// Events sessions subscribe to.
    pub events: crate::events::Events,
    /// Rate limits of notifications.
    pub notify: crate::notify::Notifier,
    /// Asks the process to stop so its service manager starts it again.
    pub restart: tokio::sync::Notify,
    /// Asks the process to stop for good (`*status shutdown`).
    pub stop: tokio::sync::Notify,
    /// Message of the day of this server, sent to clients when they connect
    /// (`*status setmotd`).
    pub motd: std::sync::RwLock<Vec<String>>,
    pub oidc: crate::sso::OidcCache,
}

/// Settings key of the server's message of the day (lines joined by \n).
pub const MOTD_SETTING: &str = "server.motd";
/// Most lines and characters per line of the server's message of the day.
pub const MOTD_MAX_LINES: usize = 20;
pub const MOTD_MAX_LEN: usize = 400;

impl Ctx {
    /// The server's message of the day.
    pub fn server_motd(&self) -> Vec<String> {
        self.motd.read().expect("motd lock").clone()
    }

    /// Replaces the server's message of the day.
    pub async fn set_server_motd(&self, lines: Vec<String>) -> Result<()> {
        let joined = lines.join("\n");
        self.db
            .call(move |c| db::set_setting(c, MOTD_SETTING, &joined))
            .await?;
        *self.motd.write().expect("motd lock") = lines;
        Ok(())
    }

    /// Re-applies settings that are cached outside the settings store.
    pub fn apply_settings(&self) {
        let l = self.settings.get().limits;
        self.limiter.configure(
            l.login_max_failures,
            Duration::from_secs(l.login_window_seconds),
            Duration::from_secs(l.login_block_seconds),
        );
    }
}

type ActorEntry = (NetHandle, Option<JoinHandle<()>>);

/// Running network actors, indexed by network id.
#[derive(Default)]
pub struct Registry {
    nets: Mutex<HashMap<i64, ActorEntry>>,
    clients_per_user: Mutex<HashMap<i64, usize>>,
}

impl Registry {
    /// Finds a user's network by name (case-insensitive).
    pub fn find(&self, user_id: i64, name: &str) -> Option<NetHandle> {
        self.nets
            .lock()
            .expect("registry lock")
            .values()
            .find(|(h, _)| h.user_id == user_id && h.name.eq_ignore_ascii_case(name))
            .map(|(h, _)| h.clone())
    }

    /// All running networks of a user, sorted by name.
    pub fn for_user(&self, user_id: i64) -> Vec<NetHandle> {
        let mut v: Vec<NetHandle> = self
            .nets
            .lock()
            .expect("registry lock")
            .values()
            .filter(|(h, _)| h.user_id == user_id)
            .map(|(h, _)| h.clone())
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    pub fn handles(&self) -> Vec<NetHandle> {
        self.nets
            .lock()
            .expect("registry lock")
            .values()
            .map(|(h, _)| h.clone())
            .collect()
    }

    /// Reserves a client slot for a user; false if the limit is reached.
    pub fn acquire_client(&self, user_id: i64, max: usize) -> bool {
        let mut m = self.clients_per_user.lock().expect("registry lock");
        let n = m.entry(user_id).or_insert(0);
        if *n >= max {
            return false;
        }
        *n += 1;
        true
    }

    /// Connected IRC clients per user.
    pub fn clients_of(&self, user_id: i64) -> usize {
        self.clients_per_user
            .lock()
            .expect("registry lock")
            .get(&user_id)
            .copied()
            .unwrap_or(0)
    }

    pub fn total_clients(&self) -> usize {
        self.clients_per_user
            .lock()
            .expect("registry lock")
            .values()
            .sum()
    }

    pub fn release_client(&self, user_id: i64) {
        let mut m = self.clients_per_user.lock().expect("registry lock");
        if let Some(n) = m.get_mut(&user_id) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                m.remove(&user_id);
            }
        }
    }
}

/// Plugin files live on disk and, as a copy, in the database: a node that
/// lacks a file (another node of a high-availability setup installed it, or
/// the database was restored) writes it from the copy, and files from
/// before v0.8 get their copy.
fn sync_plugin_files(db: &Db, dir: &std::path::Path, installed: &[(String, String)]) {
    use sha2::{Digest, Sha256};
    for (id, sha) in installed {
        let path = dir.join(format!("{id}.wasm"));
        let on_disk = std::fs::read(&path).ok();
        let disk_ok = on_disk.as_ref().is_some_and(|w| {
            Sha256::digest(w)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
                == *sha
        });
        let stored = db.with(|c| db::plugin_wasm(c, id)).ok().flatten();
        match (disk_ok, stored) {
            (true, None) => {
                if let Some(w) = on_disk {
                    let _ = db.with(|c| db::set_plugin_wasm(c, id, &w));
                }
            }
            (false, Some(w)) => {
                if let Err(e) = crate::plugins::store_file(dir, id, &w) {
                    warn!(plugin = %id, error = %e, "cannot write the plugin file from the database copy");
                }
            }
            _ => {}
        }
    }
}

/// Handle to a running bouncer.
pub struct Running {
    pub ctx: Arc<Ctx>,
    /// Actual bound addresses, in the order of the `[[listen]]` entries.
    pub local_addrs: Vec<SocketAddr>,
    /// Bound address of the web panel, if enabled.
    pub web_addr: Option<SocketAddr>,
    shutdown: watch::Sender<bool>,
    tasks: Vec<JoinHandle<()>>,
}

/// Builds the TLS client configuration used for upstream connections.
/// Only TLS 1.2 and 1.3 are enabled; certificates are verified against the
/// operating system's trust store.
pub fn tls_client_config() -> Result<Arc<rustls::ClientConfig>> {
    Ok(tls_client_parts()?.1)
}

/// System root certificates and the default TLS client configuration.
pub fn tls_client_parts() -> Result<(Arc<rustls::RootCertStore>, Arc<rustls::ClientConfig>)> {
    let mut roots = rustls::RootCertStore::empty();
    let native = rustls_native_certs::load_native_certs();
    for e in &native.errors {
        warn!(error = %e, "problem loading a system CA certificate");
    }
    let (added, _ignored) = roots.add_parsable_certificates(native.certs);
    if added == 0 {
        warn!(
            "no system CA certificates found; TLS connections to IRC networks will fail \
               (install the ca-certificates package)"
        );
    }
    let roots = Arc::new(roots);
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let cfg = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])?
        .with_root_certificates(roots.clone())
        .with_no_client_auth();
    Ok((roots, Arc::new(cfg)))
}

/// Which of `names` (a domain or an IP address) an ordinary IRC client
/// accepts for the certificate in `cert` without "accept invalid
/// certificates": the chain must lead to a system CA and cover the name.
/// Results are kept until the file changes (a renewed certificate).
pub fn cert_trusted_names(
    roots: &Arc<rustls::RootCertStore>,
    cert: &std::path::Path,
    names: &[String],
) -> Vec<String> {
    use rustls::client::danger::ServerCertVerifier;
    type Key = (std::path::PathBuf, String);
    /// (modification time of the file, valid for the name).
    type Answer = (Option<std::time::SystemTime>, bool);
    static CACHE: std::sync::LazyLock<std::sync::Mutex<HashMap<Key, Answer>>> =
        std::sync::LazyLock::new(Default::default);
    let mtime = std::fs::metadata(cert).and_then(|m| m.modified()).ok();
    let mut out = Vec::new();
    let mut todo = Vec::new();
    {
        let cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        for n in names {
            match cache.get(&(cert.to_path_buf(), n.clone())) {
                Some((t, ok)) if *t == mtime => {
                    if *ok {
                        out.push(n.clone());
                    }
                }
                _ => todo.push(n.clone()),
            }
        }
    }
    if todo.is_empty() {
        return out;
    }
    let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert)
        .map(|it| it.filter_map(|c| c.ok()).collect())
        .unwrap_or_default();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier =
        rustls::client::WebPkiServerVerifier::builder_with_provider(roots.clone(), provider)
            .build()
            .ok();
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    for n in todo {
        let ok = match (&verifier, chain.split_first()) {
            (Some(v), Some((leaf, rest))) => rustls_pki_types::ServerName::try_from(n.clone())
                .ok()
                .is_some_and(|sn| {
                    v.verify_server_cert(leaf, rest, &sn, &[], rustls_pki_types::UnixTime::now())
                        .is_ok()
                }),
            _ => false,
        };
        cache.insert((cert.to_path_buf(), n.clone()), (mtime, ok));
        if ok {
            out.push(n);
        }
    }
    out
}

fn tls_acceptor(l: &ListenConfig) -> Result<Option<tokio_rustls::TlsAcceptor>> {
    let (Some(cert), Some(key)) = (&l.tls_cert, &l.tls_key) else {
        return Ok(None);
    };
    tls_acceptor_from(cert, key).map(Some)
}

/// TLS 1.2/1.3 acceptor from PEM certificate chain and key files.
pub fn tls_acceptor_from(
    cert: &std::path::Path,
    key: &std::path::Path,
) -> Result<tokio_rustls::TlsAcceptor> {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert)
        .with_context(|| format!("cannot read certificate {}", cert.display()))?
        .collect::<Result<_, _>>()
        .with_context(|| format!("invalid certificate {}", cert.display()))?;
    if certs.is_empty() {
        bail!("no certificates in {}", cert.display());
    }
    let key = PrivateKeyDer::from_pem_file(key)
        .with_context(|| format!("cannot read private key {}", key.display()))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let cfg = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("certificate and key do not match")?;
    Ok(tokio_rustls::TlsAcceptor::from(Arc::new(cfg)))
}

/// Looks up a running network by id.
pub fn network_handle(ctx: &Ctx, id: i64) -> Option<NetHandle> {
    ctx.registry
        .nets
        .lock()
        .expect("registry lock")
        .get(&id)
        .map(|(h, _)| h.clone())
}

/// Live state of a running network (None if not running or not answering).
pub async fn network_status(ctx: &Ctx, id: i64) -> Option<network::NetStatus> {
    let h = network_handle(ctx, id)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    h.tx.send(NetCmd::Status(tx)).await.ok()?;
    tokio::time::timeout(Duration::from_secs(2), rx)
        .await
        .ok()?
        .ok()
}

/// Renames a running network without dropping its connection: the
/// registry's handle gets the new name (used to log in) and the actor
/// reloads its settings.
pub async fn rename_network(ctx: &Arc<Ctx>, id: i64, name: &str) -> Result<()> {
    let h = {
        let mut nets = ctx.registry.nets.lock().expect("registry lock");
        nets.get_mut(&id).map(|(h, _)| {
            h.name = name.to_string();
            h.clone()
        })
    };
    match h {
        Some(h) => {
            let _ = h.tx.send(NetCmd::Reload).await;
            ctx.events
                .send(crate::events::Event::NetworksChanged { user_id: h.user_id });
            Ok(())
        }
        None => sync_networks(ctx).await,
    }
}

/// Stops a network actor so the next `sync_networks` starts a fresh one.
pub async fn restart_network(ctx: &Arc<Ctx>, id: i64) -> Result<()> {
    let removed = ctx.registry.nets.lock().expect("registry lock").remove(&id);
    if let Some((h, j)) = removed {
        let _ = h.tx.send(NetCmd::Shutdown).await;
        if let Some(j) = j {
            let _ = tokio::time::timeout(Duration::from_secs(5), j).await;
        }
    }
    sync_networks(ctx).await
}

/// Starts actors for new networks, stops removed ones and reloads the rest.
/// Re-reads the installed plugins (after `xbncd plugin install/remove`).
/// Reads the locales directory again (new languages and corrections).
pub fn reload_languages(ctx: &Ctx) {
    for line in crate::i18n::load_dir(&ctx.config.locales_dir) {
        info!("{line}");
    }
}

pub async fn reload_plugins(ctx: &Arc<Ctx>) -> Result<()> {
    let rows = ctx.db.call(|c| db::list_plugins(c)).await?;
    let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    let stale: Vec<(String, String)> = rows
        .into_iter()
        .filter(|r| {
            ctx.plugins
                .get(&r.id)
                .map(|p| p.sha256 != r.sha256)
                .unwrap_or(true)
        })
        .map(|r| (r.id, r.sha256))
        .collect();
    ctx.plugins.retain(&ids);
    let c = ctx.clone();
    tokio::task::spawn_blocking(move || c.plugins.load_all(&stale)).await?;
    Ok(())
}

pub async fn sync_networks(ctx: &Arc<Ctx>) -> Result<()> {
    let active = ctx.db.call(|c| db::list_active_networks(c)).await?;
    let mut to_stop = Vec::new();
    let mut to_reload = Vec::new();
    {
        let mut nets = ctx.registry.nets.lock().expect("registry lock");
        let active_ids: Vec<i64> = active.iter().map(|(n, _)| n.id).collect();
        nets.retain(|id, (h, _)| {
            if active_ids.contains(id) {
                to_reload.push(h.clone());
                true
            } else {
                to_stop.push(h.clone());
                false
            }
        });
        for (net, owner) in active {
            if nets.contains_key(&net.id) {
                continue;
            }
            let (h, j) = network::spawn(ctx.clone(), net, owner);
            nets.insert(h.network_id, (h, Some(j)));
        }
    }
    let mut users = std::collections::BTreeSet::new();
    for h in to_stop {
        info!(network = %h.name, "stopping network (removed or disabled)");
        users.insert(h.user_id);
        let _ = h.tx.send(NetCmd::Shutdown).await;
    }
    for h in to_reload {
        users.insert(h.user_id);
        let _ = h.tx.send(NetCmd::Reload).await;
    }
    for h in ctx.registry.handles() {
        users.insert(h.user_id);
    }
    for user_id in users {
        ctx.events
            .send(crate::events::Event::NetworksChanged { user_id });
    }
    Ok(())
}

/// How a listener treats clients, plus the hints sent to clients that use
/// the wrong port or protocol.
#[derive(Clone)]
struct ListenerPolicy {
    lan_only: bool,
    /// Port of a TLS listener, suggested to clients on a LAN-only or
    /// plain-text port.
    tls_port: Option<u16>,
    /// Port of a plain-text listener, suggested to clients that speak plain
    /// text to a TLS port.
    plain_port: Option<u16>,
    /// Shared port: browsers go to the web panel through here.
    web: Option<crate::web::SharedSender>,
    port: u16,
}

/// Writes one IRC `ERROR` line, closes our side and drains briefly so the
/// client actually reads the line instead of getting a connection reset.
async fn reject(mut tcp: tokio::net::TcpStream, text: &str) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let _ = tcp
        .write_all(format!("ERROR :Closing link: {text}\r\n").as_bytes())
        .await;
    let _ = tcp.shutdown().await;
    let mut sink = [0u8; 1024];
    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        while let Ok(n) = tcp.read(&mut sink).await {
            if n == 0 {
                break;
            }
        }
    })
    .await;
}

/// Connections from one address that have not logged in yet. Without this
/// limit, one host could hold every slot of `max_connections` with idle
/// sockets and lock everybody else out.
pub const MAX_PREAUTH_PER_IP: u32 = 16;

static PREAUTH: std::sync::LazyLock<std::sync::Mutex<HashMap<std::net::IpAddr, u32>>> =
    std::sync::LazyLock::new(Default::default);

/// One not-yet-authenticated connection; released when dropped.
pub struct PreAuth(std::net::IpAddr);

impl PreAuth {
    /// Loopback is exempt (local tools, tunnels such as stunnel).
    pub fn acquire(ip: std::net::IpAddr) -> Option<PreAuth> {
        if ip.is_loopback() {
            return Some(PreAuth(ip));
        }
        let mut m = PREAUTH.lock().unwrap_or_else(|e| e.into_inner());
        let n = m.entry(ip).or_insert(0);
        if *n >= MAX_PREAUTH_PER_IP {
            return None;
        }
        *n += 1;
        Some(PreAuth(ip))
    }
}

impl Drop for PreAuth {
    fn drop(&mut self) {
        if self.0.is_loopback() {
            return;
        }
        let mut m = PREAUTH.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = m.get_mut(&self.0) {
            *n -= 1;
            if *n == 0 {
                m.remove(&self.0);
            }
        }
    }
}

/// Peeks at a plain-text connection: an HTTP request (a browser), or not
/// (an IRC client with SSL turned off).
async fn looks_like_http(tcp: &tokio::net::TcpStream) -> bool {
    let mut buf = [0u8; 16];
    for _ in 0..20 {
        let n = match tokio::time::timeout(Duration::from_secs(5), tcp.peek(&mut buf)).await {
            Ok(Ok(n)) if n > 0 => n,
            _ => return false,
        };
        match crate::util::sniff_http(&buf[..n]) {
            Some(http) => return http,
            None => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }
    false
}

/// The configured address (`0.0.0.0:7000`) matches the bound one (port 0
/// in tests is resolved by the OS).
fn same_listen_addr(configured: SocketAddr, bound: SocketAddr) -> bool {
    configured.ip() == bound.ip() && (configured.port() == bound.port() || configured.port() == 0)
}

/// First byte of a TLS connection is a handshake record (0x16).
const TLS_HANDSHAKE: u8 = 0x16;

async fn handle_connection(
    ctx: Arc<Ctx>,
    tcp: tokio::net::TcpStream,
    peer: SocketAddr,
    tls: Option<tokio_rustls::TlsAcceptor>,
    policy: ListenerPolicy,
) {
    if policy.lan_only && !crate::netinfo::is_local_network(peer.ip()) {
        info!(%peer, "client rejected: this port only accepts the local network");
        let hint = match policy.tls_port {
            Some(p) => format!(
                "this port only accepts connections from the local network. \
                 Connect to port {p} with SSL/TLS enabled instead."
            ),
            None => "this port only accepts connections from the local network.".into(),
        };
        reject(tcp, &hint).await;
        return;
    }
    let Some(pre_auth) = PreAuth::acquire(peer.ip()) else {
        warn!(%peer, "too many connections from this address that have not logged in");
        reject(
            tcp,
            "too many connections from your address that have not logged in yet.",
        )
        .await;
        return;
    };
    let local = tcp.local_addr().ok();
    let mut first = [0u8; 1];
    let peeked = tokio::time::timeout(Duration::from_secs(15), tcp.peek(&mut first)).await;
    let Ok(Ok(1)) = peeked else {
        return;
    };
    let stream: BoxStream = match tls {
        Some(acc) => {
            if first[0] != TLS_HANDSHAKE && policy.web.is_some() && looks_like_http(&tcp).await {
                // Shared port: a browser that typed http:// goes to https://.
                drop(pre_auth);
                crate::web::redirect_to_https(tcp, policy.port).await;
                return;
            }
            if first[0] != TLS_HANDSHAKE {
                info!(%peer, "client sent plain text to a TLS port (SSL disabled in the client?)");
                let hint = match policy.plain_port {
                    Some(p) => format!(
                        "this port uses SSL/TLS. Enable SSL in your IRC client, \
                         or connect to port {p} without SSL."
                    ),
                    None => "this port uses SSL/TLS. Enable SSL in your IRC client.".into(),
                };
                reject(tcp, &hint).await;
                return;
            }
            match tokio::time::timeout(Duration::from_secs(15), acc.accept(tcp)).await {
                Ok(Ok(s)) => Box::new(s),
                Ok(Err(e)) => {
                    info!(
                        %peer,
                        error = %e,
                        "TLS handshake failed (does the client reject the certificate?)"
                    );
                    return;
                }
                Err(_) => {
                    info!(%peer, "TLS handshake timed out");
                    return;
                }
            }
        }
        None => {
            if first[0] == TLS_HANDSHAKE {
                info!(%peer, "client sent TLS to a plain-text port (disable SSL or use the TLS port)");
                return;
            }
            Box::new(tcp)
        }
    };
    let stream = match &policy.web {
        None => stream,
        Some(web) => {
            // Shared port: the first bytes tell a browser from an IRC client.
            let mut stream = stream;
            let sniff = tokio::time::timeout(
                Duration::from_secs(15),
                crate::util::read_protocol(&mut stream),
            )
            .await;
            let Ok(Ok((http, head))) = sniff else {
                return;
            };
            let stream: BoxStream = Box::new(crate::util::Prefixed::new(head, stream));
            if http {
                drop(pre_auth);
                let _ = web.send((stream, peer)).await;
                return;
            }
            stream
        }
    };
    downstream::serve(ctx, stream, peer, local, pre_auth).await;
}

async fn accept_loop(
    ctx: Arc<Ctx>,
    listener: TcpListener,
    tls: Option<tokio_rustls::TlsAcceptor>,
    policy: ListenerPolicy,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        let accepted = tokio::select! {
            r = listener.accept() => r,
            _ = shutdown.changed() => break,
        };
        let (tcp, peer) = match accepted {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "accept failed");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let max = ctx.settings.get().limits.max_connections;
        if ctx.connections.fetch_add(1, Ordering::SeqCst) >= max {
            ctx.connections.fetch_sub(1, Ordering::SeqCst);
            warn!(%peer, "connection limit reached; rejecting client");
            drop(tcp);
            continue;
        }
        let _ = tcp.set_nodelay(true);
        let ctx2 = ctx.clone();
        let tls = tls.clone();
        let policy = policy.clone();
        tokio::spawn(async move {
            handle_connection(ctx2.clone(), tcp, peer, tls, policy).await;
            ctx2.connections.fetch_sub(1, Ordering::SeqCst);
        });
    }
}

/// Hourly maintenance: history retention (read live from the settings) and
/// expired web sessions.
async fn maintenance_loop(ctx: Arc<Ctx>, mut shutdown: watch::Receiver<bool>) {
    loop {
        let days = ctx.settings.get().history.retention_days;
        if days > 0 {
            let cutoff = db::now_ms() - days as i64 * 86_400_000;
            match ctx
                .db
                .call(move |c| db::purge_messages_before(c, cutoff))
                .await
            {
                Ok(0) => {}
                Ok(n) => info!(deleted = n, days, "history retention: old messages deleted"),
                Err(e) => error!(error = %e, "history retention failed"),
            }
        }
        match ctx
            .db
            .call(|c| db::purge_channel_retention(c, db::now_ms()))
            .await
        {
            Ok(0) => {}
            Ok(n) => info!(deleted = n, "channel retention: old messages deleted"),
            Err(e) => error!(error = %e, "channel retention failed"),
        }
        match ctx.db.call(|c| db::enforce_history_quotas(c)).await {
            Ok(list) => {
                for (user, n) in list {
                    info!(user = %user, deleted = n, "history quota: oldest messages deleted");
                }
            }
            Err(e) => error!(error = %e, "history quotas failed"),
        }
        let b = ctx.settings.get().backups;
        if b.enabled {
            let newest = crate::backup::list(&ctx.config)
                .first()
                .map(|x| x.created_at)
                .unwrap_or(0);
            if db::now_ms() - newest >= b.interval_hours as i64 * 3_600_000 {
                let c2 = ctx.clone();
                match tokio::task::spawn_blocking(move || {
                    let info = crate::backup::create(&c2.config, &c2.db, "automatic")?;
                    crate::backup::rotate(&c2.config, b.keep as usize);
                    Ok::<_, anyhow::Error>(info)
                })
                .await
                {
                    Ok(Ok(info)) => {
                        info!(name = %info.name, size = info.size, "automatic backup created")
                    }
                    Ok(Err(e)) => error!(error = %e, "automatic backup failed"),
                    Err(e) => error!(error = %e, "automatic backup panicked"),
                }
            }
        }
        if let Err(e) = ctx.db.call(|c| db::purge_expired_sessions(c)).await {
            error!(error = %e, "cannot purge expired web sessions");
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(3600)) => {}
            _ = shutdown.changed() => break,
        }
    }
}

/// Opens storage, starts every network actor and binds the listeners.
/// `create_secret` allows generating the secret key file (used by tests and
/// `xbncd init`).
pub async fn start(config: Config, create_secret: bool) -> Result<Running> {
    // Config::load already read the locales directory (before logging was
    // set up); report what is in use.
    for line in crate::i18n::load_dir(&config.locales_dir) {
        info!("{line}");
    }
    let langs: Vec<String> = crate::i18n::languages()
        .into_iter()
        .map(|l| format!("{} {}%", l.code, l.complete))
        .collect();
    info!(languages = %langs.join(", "), "languages");
    match crate::backup::apply_pending_restore(&config) {
        Ok(Some(m)) => info!(created_at = m.created_at, by = %m.created_by, "backup restored"),
        Ok(None) => {}
        Err(e) => {
            error!(error = %e, "cannot apply the staged restore; starting with the current data")
        }
    }
    let db = Db::open_config(&config)?;
    let secrets = SecretBox::load(&config.secret_key_file, create_secret)?;
    let settings = SettingsStore::load(
        &db,
        config.history.clone(),
        config.limits.clone(),
        &config.language,
    )?;
    let motd: Vec<String> = db
        .with(|c| db::get_setting(c, MOTD_SETTING))
        .ok()
        .flatten()
        .map(|m| m.lines().map(str::to_string).collect())
        .unwrap_or_default();
    let l = settings.get().limits;
    let limiter = LoginLimiter::new(
        l.login_max_failures,
        Duration::from_secs(l.login_window_seconds),
        Duration::from_secs(l.login_block_seconds),
    );
    let users = db.with(|c| db::count_users(c))?;
    let setup_token = (users == 0 && config.web.is_some()).then(crate::web::generate_setup_token);
    let log_root = config
        .database
        .path
        .parent()
        .map(|p| p.join("logs"))
        .unwrap_or_else(|| "logs".into());
    let plugin_dir = config
        .database
        .path
        .parent()
        .map(|p| p.join("plugins"))
        .unwrap_or_else(|| "plugins".into());
    let plugins = crate::plugins::PluginRegistry::new(plugin_dir);
    crate::catalog::sync_official(&db, &plugins);
    let installed: Vec<(String, String)> = db
        .with(|c| db::list_plugins(c))?
        .into_iter()
        .map(|p| (p.id, p.sha256))
        .collect();
    sync_plugin_files(&db, &plugins.dir, &installed);
    plugins.load_all(&installed);
    let (tls_roots, tls_client) = tls_client_parts()?;
    let ctx = Arc::new(Ctx {
        tls_client,
        tls_roots,
        config,
        db,
        secrets,
        limiter,
        registry: Registry::default(),
        next_client_id: AtomicU64::new(1),
        connections: AtomicUsize::new(0),
        settings,
        metrics: Metrics::default(),
        started_at: db::now_ms(),
        setup_token: Mutex::new(setup_token),
        logs: crate::logs::LogSink::new(log_root),
        alerts: Default::default(),
        plugins,
        dcc: Default::default(),
        updates: Default::default(),
        events: Default::default(),
        notify: Default::default(),
        oidc: Default::default(),
        restart: Default::default(),
        stop: Default::default(),
        motd: std::sync::RwLock::new(motd),
    });

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut tasks = Vec::new();
    let mut local_addrs = Vec::new();
    let mut bound = Vec::new();
    for l in &ctx.config.listen {
        let acceptor = tls_acceptor(l)?;
        let listener = TcpListener::bind(&l.address)
            .await
            .with_context(|| format!("cannot listen on {}", l.address))?;
        let addr = listener.local_addr()?;
        info!(
            address = %addr,
            tls = acceptor.is_some(),
            lan_only = l.lan_only,
            "listening for IRC clients"
        );
        local_addrs.push(addr);
        bound.push((listener, acceptor, l.lan_only));
    }
    let tls_port = bound
        .iter()
        .find(|(_, a, lan)| a.is_some() && !lan)
        .or_else(|| bound.iter().find(|(_, a, _)| a.is_some()))
        .and_then(|(l, _, _)| l.local_addr().ok())
        .map(|a| a.port());
    let plain_port = bound
        .iter()
        .find(|(_, a, _)| a.is_none())
        .and_then(|(l, _, _)| l.local_addr().ok())
        .map(|a| a.port());
    // `[web] shared = true`: the panel is served on that IRC port.
    let mut shared_rx = None;
    let shared_addr: Option<SocketAddr> = ctx
        .config
        .web
        .as_ref()
        .filter(|w| w.shared)
        .and_then(|w| w.address.parse().ok());
    for (listener, acceptor, lan_only) in bound {
        let laddr = listener.local_addr()?;
        let web = match shared_addr {
            Some(a) if acceptor.is_some() && same_listen_addr(a, laddr) && shared_rx.is_none() => {
                let (tx, rx) = tokio::sync::mpsc::channel(64);
                shared_rx = Some((rx, laddr));
                Some(tx)
            }
            _ => None,
        };
        let policy = ListenerPolicy {
            lan_only,
            tls_port,
            plain_port,
            web,
            port: laddr.port(),
        };
        tasks.push(tokio::spawn(accept_loop(
            ctx.clone(),
            listener,
            acceptor,
            policy,
            shutdown_rx.clone(),
        )));
    }
    if local_addrs.is_empty() {
        warn!("no [[listen]] entries configured; clients cannot connect");
    }
    sync_networks(&ctx).await?;
    tasks.push(tokio::spawn(crate::updates::update_loop(
        ctx.clone(),
        shutdown_rx.clone(),
    )));
    tasks.push(tokio::spawn(crate::catalog::auto_update_loop(
        ctx.clone(),
        shutdown_rx.clone(),
    )));
    tasks.push(tokio::spawn(crate::alerts::alert_loop(
        ctx.clone(),
        shutdown_rx.clone(),
    )));
    tasks.push(tokio::spawn(maintenance_loop(
        ctx.clone(),
        shutdown_rx.clone(),
    )));
    tasks.push(tokio::spawn(crate::plugin_dev::dev_loop(
        ctx.clone(),
        shutdown_rx.clone(),
    )));
    let mut web_addr = None;
    if let Some(w) = ctx.config.web.clone() {
        if w.shared && shared_rx.is_none() {
            anyhow::bail!("web.shared: no TLS listener on {}", w.address);
        }
        let (addr, task) =
            crate::web::serve(ctx.clone(), &w, shutdown_rx.clone(), shared_rx.take()).await?;
        web_addr = Some(addr);
        tasks.push(task);
        let url = panel_url(&w, addr, &ctx.config.hostname);
        let path = setup_code_path(&ctx.config);
        if let Some(token) = ctx.setup_token.lock().expect("setup lock").clone() {
            if let Err(e) = write_private(&path, &format!("{token}\n{url}\n")) {
                warn!(error = %e, path = %path.display(), "cannot write the setup code file");
            }
            warn!(
                "No users exist yet. Open {url}/ and use this one-time setup code to create \
                 the first administrator: {token}  (also shown by `xbncd setup-code`)"
            );
        } else {
            let _ = std::fs::remove_file(&path);
        }
        info!(url = %url, "web panel ready");
    }
    Ok(Running {
        ctx,
        local_addrs,
        web_addr,
        shutdown: shutdown_tx,
        tasks,
    })
}

/// URL users should open for the panel: `public_url`, or the listening
/// address with 0.0.0.0 replaced by this machine's address.
/// Address of the panel as users type it: `public_url`, else the configured
/// hostname when the panel listens on every interface and the hostname is a
/// domain or an IP (the address given at installation), else the machine's
/// own address.
pub fn panel_url(w: &crate::config::WebConfig, addr: SocketAddr, hostname: &str) -> String {
    if let Some(u) = &w.public_url {
        return u.trim_end_matches('/').to_string();
    }
    let tls = w.uses_tls();
    let h = hostname.trim();
    let host = if addr.ip().is_unspecified() && (h.contains('.') || h.contains(':')) {
        if h.contains(':') && !h.starts_with('[') {
            format!("[{h}]")
        } else {
            h.to_string()
        }
    } else {
        crate::netinfo::display_host(addr)
    };
    let default_port = if tls { 443 } else { 80 };
    let scheme = if tls { "https" } else { "http" };
    if addr.port() == default_port {
        format!("{scheme}://{host}")
    } else {
        format!("{scheme}://{host}:{}", addr.port())
    }
}

/// File holding the one-time setup code while no user exists, next to the
/// database so only the service user (and root) can read it.
pub fn setup_code_path(cfg: &Config) -> std::path::PathBuf {
    cfg.database
        .path
        .parent()
        .map(|p| p.join("setup-code"))
        .unwrap_or_else(|| "setup-code".into())
}

fn write_private(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    f.write_all(content.as_bytes())
}

impl Running {
    /// Applies database changes made with the CLI (new/removed networks,
    /// channels). Triggered by SIGHUP.
    pub async fn reload(&self) -> Result<()> {
        info!("reloading networks and plugins from the database");
        reload_languages(&self.ctx);
        reload_plugins(&self.ctx).await?;
        for h in self.ctx.registry.handles() {
            let _ = h.tx.send(NetCmd::Reload).await;
        }
        sync_networks(&self.ctx).await
    }

    /// Stops listeners and actors, sending QUIT to every network.
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(true);
        let actors: Vec<ActorEntry> = {
            let mut nets = self.ctx.registry.nets.lock().expect("registry lock");
            nets.drain().map(|(_, v)| v).collect()
        };
        for (h, _) in &actors {
            let _ = h.tx.send(NetCmd::Shutdown).await;
        }
        for (_, j) in actors {
            if let Some(j) = j {
                let _ = tokio::time::timeout(Duration::from_secs(5), j).await;
            }
        }
        for t in self.tasks {
            t.abort();
        }
        info!("xBNC stopped");
    }

    /// Network handles currently running (for diagnostics and tests).
    pub fn networks(&self) -> Vec<NetHandle> {
        self.ctx.registry.handles()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The panel address shown by `xbncd info`, the setup code file and the
    /// log is the one given at installation, not whatever interface the
    /// machine happens to report (installing by public IP showed the LAN IP).
    #[test]
    fn panel_url_uses_the_installed_address() {
        let w: crate::config::WebConfig =
            toml::from_str("address = \"0.0.0.0:8443\"\ntls_cert = \"c.pem\"\ntls_key = \"k.pem\"")
                .unwrap();
        let any: SocketAddr = "0.0.0.0:8443".parse().unwrap();
        assert_eq!(panel_url(&w, any, "10.0.0.5"), "https://10.0.0.5:8443");
        assert_eq!(
            panel_url(&w, any, "bnc.example.org"),
            "https://bnc.example.org:8443"
        );
        assert_eq!(
            panel_url(&w, any, "2001:db8::5"),
            "https://[2001:db8::5]:8443"
        );
        // A bare machine name is no address for others: the machine's own IP.
        assert!(!panel_url(&w, any, "xbnc").contains("//xbnc"));
        // Listening on one address: that address.
        let one: SocketAddr = "127.0.0.1:8443".parse().unwrap();
        assert_eq!(panel_url(&w, one, "10.0.0.5"), "https://127.0.0.1:8443");
        // public_url always wins.
        let p: crate::config::WebConfig = toml::from_str(
            "address = \"127.0.0.1:8080\"\npublic_url = \"https://bnc.example.org/\"",
        )
        .unwrap();
        assert_eq!(panel_url(&p, one, "10.0.0.5"), "https://bnc.example.org");
    }

    /// The connect hints need to know whether clients must "accept invalid
    /// certificates": only a certificate from a known CA that covers the
    /// name is accepted by ordinary clients.
    #[test]
    fn trusted_names_of_a_certificate() {
        let dir = tempfile::tempdir().unwrap();
        // A CA (in the roots) and a certificate for bnc.example.org from it.
        let ca_key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let mut ca_params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let ca = ca_params.self_signed(&ca_key).unwrap();
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let leaf = rcgen::CertificateParams::new(vec!["bnc.example.org".to_string()])
            .unwrap()
            .signed_by(&key, &ca, &ca_key)
            .unwrap();
        let signed = dir.path().join("signed.pem");
        std::fs::write(&signed, format!("{}{}", leaf.pem(), ca.pem())).unwrap();
        // A self-signed one for an IP address, as install.sh makes by IP.
        let own_key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let own = rcgen::CertificateParams::new(vec!["192.168.1.50".to_string()])
            .unwrap()
            .self_signed(&own_key)
            .unwrap();
        let selfsigned = dir.path().join("own.pem");
        std::fs::write(&selfsigned, own.pem()).unwrap();

        let mut roots = rustls::RootCertStore::empty();
        roots.add(ca.der().clone()).unwrap();
        let roots = Arc::new(roots);
        let names = vec!["bnc.example.org".to_string(), "192.168.1.50".to_string()];
        assert_eq!(
            cert_trusted_names(&roots, &signed, &names),
            vec!["bnc.example.org".to_string()],
            "valid for its domain, not for the local IP"
        );
        assert!(cert_trusted_names(&roots, &selfsigned, &names).is_empty());
        // Cached answers are the same.
        assert_eq!(cert_trusted_names(&roots, &signed, &names).len(), 1);
        assert!(cert_trusted_names(&roots, &dir.path().join("missing.pem"), &names).is_empty());
    }

    #[test]
    fn preauth_limit_per_address() {
        let ip: std::net::IpAddr = "203.0.113.7".parse().unwrap();
        let held: Vec<_> = (0..MAX_PREAUTH_PER_IP)
            .map(|_| PreAuth::acquire(ip).unwrap())
            .collect();
        assert!(PreAuth::acquire(ip).is_none());
        assert!(PreAuth::acquire("203.0.113.8".parse().unwrap()).is_some());
        drop(held);
        assert!(PreAuth::acquire(ip).is_some());
        let local: Vec<_> = (0..100)
            .map(|_| PreAuth::acquire("127.0.0.1".parse().unwrap()))
            .collect();
        assert!(local.iter().all(Option::is_some));
    }
}
