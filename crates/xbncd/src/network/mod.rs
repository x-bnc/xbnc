//! The per-(user, network) actor.
//!
//! One Tokio task owns everything about a network connection: the upstream
//! socket, the protocol state (nick, channels, members, ISUPPORT), and the list
//! of attached client sessions. Other tasks talk to it only through
//! [`NetCmd`] messages, so no locks are needed on the message path.
//!
//! Three state machines are kept separate, as the architecture requires:
//! * upstream connection: [`Phase`] (Disconnected → Registering → Connected);
//! * client sessions: owned by `downstream`, represented here by [`Client`];
//! * the user/bouncer level: the registry in `server`.

use crate::auth::redact_line;
use crate::db::{self, Bound, Network, NewMessage, StoredMessage};
use crate::server::Ctx;
use crate::util::{format_server_time, format_short_time, parse_server_time, BoxStream, IrcCodec};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::time::Instant;
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::{debug, info, trace, warn};
use xbnc_proto::{ctcp_command, Isupport, Message};

/// `tr!(self, "key", a, b)`: the text of `key` in the owner's language with
/// `{0}`, `{1}`... replaced by the arguments (anything printable).
macro_rules! tr {
    ($s:expr, $key:expr $(, $a:expr)* $(,)?) => {
        $s.t($key, &[$(&$a.to_string()),*])
    };
}

mod dcc;
mod extra;
mod mods;
mod plugins;
mod readmark;
mod status;
mod status_more;

pub type ClientId = u64;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Product line shown wherever the version appears: "xBNC 1.0.0 Made by y2k".
pub const BRAND: &str = concat!("xBNC ", env!("CARGO_PKG_VERSION"), " Made by y2k");
/// The project's web site, shown with the version.
pub const SITE: &str = "https://xbnc.org";
/// `/msg *status version` and the default CTCP VERSION reply:
/// "xBNC 1.0.0 Made by y2k - https://xbnc.org".
pub const BRAND_SITE: &str = concat!("xBNC ", env!("CARGO_PKG_VERSION"), " Made by y2k - https://xbnc.org");
/// Default of the CTCP module's VERSION reply; `{version}` is replaced by
/// the running version, so the text never goes stale after an update.
pub const CTCP_VERSION_DEFAULT: &str = "xBNC {version} Made by y2k - https://xbnc.org";

/// The CTCP VERSION reply for a configured text (empty: the default).
pub fn ctcp_version_reply(configured: &str) -> String {
    let t = configured.trim();
    let t = if t.is_empty() { CTCP_VERSION_DEFAULT } else { t };
    t.replace("{version}", env!("CARGO_PKG_VERSION"))
}

const KEEPALIVE_IDLE: Duration = Duration::from_secs(90);
const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(180);
const REGISTRATION_TIMEOUT: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const BACKOFF_MIN: Duration = Duration::from_secs(5);
/// Wait before trying the next server of the list.
const BACKUP_SERVER_DELAY: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(300);
/// Minimum wait after a server says we reconnect too fast. Retrying sooner
/// only restarts the server's throttle timer.
const THROTTLE_BACKOFF: Duration = Duration::from_secs(120);
/// How often delivered-history cursors are saved (only changed ones are
/// written). After an abrupt stop, at most this much is played back twice.
const CURSOR_FLUSH: Duration = Duration::from_secs(5);
const UPSTREAM_QUEUE: usize = 2048;
/// Outgoing flood control towards the IRC server (conservative defaults
/// accepted by the major networks).
const FLOOD_BURST: f64 = 9.0;
const FLOOD_RATE: f64 = 2.0; // lines per second

/// Capabilities requested from IRC servers when offered (`sasl` is handled
/// separately). Messages they add are passed only to clients that negotiated
/// the same capability (see [`adapt_for`]).
const UPSTREAM_CAPS: &[&str] = &[
    "server-time",
    "message-tags",
    "multi-prefix",
    "extended-join",
    "away-notify",
    "account-notify",
    "account-tag",
    "chghost",
    "userhost-in-names",
    "invite-notify",
    "setname",
    "cap-notify",
];

/// Membership prefix symbols used when trimming replies for clients
/// without multi-prefix.
const PREFIX_SYMBOLS: &str = "~&@%+";

/// Capabilities negotiated by a client that change what we send it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClientCaps {
    pub server_time: bool,
    pub message_tags: bool,
    pub echo_message: bool,
    pub batch: bool,
    pub chathistory: bool,
    /// draft/event-playback: CHATHISTORY may include JOIN, PART, etc.
    pub event_playback: bool,
    /// draft/read-marker: read state shared between the user's devices.
    pub read_marker: bool,
    /// soju.im/bouncer-networks (and its -notify companion).
    pub bouncer_networks: bool,
    pub bouncer_networks_notify: bool,
    pub extended_join: bool,
    pub away_notify: bool,
    pub account_notify: bool,
    pub account_tag: bool,
    pub chghost: bool,
    pub multi_prefix: bool,
    pub userhost_in_names: bool,
    pub invite_notify: bool,
    pub setname: bool,
    pub cap_notify: bool,
}

/// Capabilities offered to clients, in the order they are listed. Only what
/// is actually implemented (`sasl` is handled by the session itself).
pub const CLIENT_CAPS: &[&str] = &[
    "server-time",
    "message-tags",
    "echo-message",
    "batch",
    "draft/chathistory",
    "draft/event-playback",
    "draft/read-marker",
    "soju.im/bouncer-networks",
    "soju.im/bouncer-networks-notify",
    "extended-join",
    "away-notify",
    "account-notify",
    "account-tag",
    "chghost",
    "multi-prefix",
    "userhost-in-names",
    "invite-notify",
    "setname",
    "cap-notify",
];

/// A client session as seen by the network actor.
#[derive(Debug)]
pub struct ClientHandle {
    pub id: ClientId,
    /// Remote address, for the sessions page.
    pub peer: String,
    /// Socket addresses of the client connection (DCC relays need them).
    pub peer_addr: Option<std::net::SocketAddr>,
    pub local_addr: Option<std::net::SocketAddr>,
    /// Device name used to keep a separate history cursor.
    pub name: String,
    pub tx: mpsc::Sender<String>,
    pub caps: ClientCaps,
}

#[derive(Debug)]
pub enum NetCmd {
    Attach(ClientHandle),
    Detach(ClientId),
    UpdateCaps(ClientId, ClientCaps),
    FromClient(ClientId, Message),
    /// Re-read the network row and channel list from the database.
    Reload,
    Shutdown,
    /// Live state for the web panel.
    Status(tokio::sync::oneshot::Sender<NetStatus>),
    /// Connection control from the web panel.
    Control(NetAction),
    /// Join (and save) a channel on behalf of the user.
    Join {
        channel: String,
        key: Option<String>,
    },
    /// Leave (and forget) a channel.
    Part {
        channel: String,
    },
    /// Close one client session.
    DisconnectClient(ClientId),
    /// The answer to a plugin's web request.
    PluginHttp {
        plugin: String,
        event: serde_json::Value,
    },
    /// Mark a conversation read up to `ts` (from the web panel).
    MarkRead {
        target: String,
        ts: i64,
    },
    /// A message from an administrator to every connected user.
    Broadcast(String),
    /// An alert started or ended (sent to administrators' networks).
    Alert {
        kind: &'static str,
        subject: String,
        message: String,
        firing: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetAction {
    Connect,
    Disconnect,
    Jump,
}

/// Snapshot of a network actor, serialised to the web panel.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NetStatus {
    pub state: &'static str,
    pub nick: String,
    pub server: String,
    pub tls: bool,
    pub connected_since: Option<i64>,
    pub last_error: Option<String>,
    pub caps: Vec<String>,
    pub network_name: Option<String>,
    pub channels: Vec<ChannelStatus>,
    pub clients: Vec<ClientStatus>,
    /// For the test report: capabilities the server offered, its ISUPPORT
    /// tokens and what the v1.0 modules observed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub caps_offered: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub isupport: Vec<String>,
    pub logged_in: bool,
    pub host_hidden: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChannelStatus {
    pub name: String,
    pub topic: Option<String>,
    pub members: usize,
    pub ops: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ClientStatus {
    pub id: ClientId,
    pub name: String,
    pub peer: String,
    pub connected_at: i64,
    pub caps: Vec<&'static str>,
}

impl ClientCaps {
    fn field(&mut self, name: &str) -> Option<&mut bool> {
        Some(match name {
            "server-time" => &mut self.server_time,
            "message-tags" => &mut self.message_tags,
            "echo-message" => &mut self.echo_message,
            "batch" => &mut self.batch,
            "draft/chathistory" => &mut self.chathistory,
            "draft/event-playback" => &mut self.event_playback,
            "draft/read-marker" => &mut self.read_marker,
            "soju.im/bouncer-networks" => &mut self.bouncer_networks,
            "soju.im/bouncer-networks-notify" => &mut self.bouncer_networks_notify,
            "extended-join" => &mut self.extended_join,
            "away-notify" => &mut self.away_notify,
            "account-notify" => &mut self.account_notify,
            "account-tag" => &mut self.account_tag,
            "chghost" => &mut self.chghost,
            "multi-prefix" => &mut self.multi_prefix,
            "userhost-in-names" => &mut self.userhost_in_names,
            "invite-notify" => &mut self.invite_notify,
            "setname" => &mut self.setname,
            "cap-notify" => &mut self.cap_notify,
            _ => return None,
        })
    }

    /// Turns a capability on or off. False if it is not one we offer.
    pub fn set(&mut self, name: &str, on: bool) -> bool {
        match self.field(name) {
            Some(f) => {
                *f = on;
                true
            }
            None => false,
        }
    }

    pub fn names(&self) -> Vec<&'static str> {
        let mut copy = *self;
        CLIENT_CAPS
            .iter()
            .copied()
            .filter(|n| copy.field(n).is_some_and(|f| *f))
            .collect()
    }
}

/// Cheap handle used to talk to a network actor.
#[derive(Debug, Clone)]
pub struct NetHandle {
    pub network_id: i64,
    pub user_id: i64,
    pub name: String,
    pub tx: mpsc::Sender<NetCmd>,
}

struct Client {
    h: ClientHandle,
    /// Highest message id delivered to this client.
    last_id: i64,
    connected_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Disconnected,
    Registering,
    Connected,
}

#[derive(Debug, Default)]
struct Member {
    nick: String,
    /// Membership prefix symbols, highest first (e.g. "@+").
    symbols: String,
}

#[derive(Debug)]
struct Chan {
    name: String,
    topic: Option<String>,
    members: HashMap<String, Member>,
    names_done: bool,
}

impl Chan {
    fn new(name: &str) -> Chan {
        Chan {
            name: name.to_string(),
            topic: None,
            members: HashMap::new(),
            names_done: false,
        }
    }
}

enum Ev {
    Cmd(Option<NetCmd>),
    Line(Option<std::io::Result<String>>),
    Tick,
}

type Reader = FramedRead<ReadHalf<BoxStream>, IrcCodec>;

pub struct NetActor {
    ctx: Arc<Ctx>,
    net: Network,
    owner: String,
    rx: mpsc::Receiver<NetCmd>,
    /// To post results of background work back to this actor.
    self_tx: mpsc::WeakSender<NetCmd>,
    clients: Vec<Client>,
    reader: Option<Reader>,
    writer: Option<mpsc::Sender<String>>,
    phase: Phase,
    nick: String,
    own_prefix: Option<String>,
    isupport: Isupport,
    channels: HashMap<String, Chan>,
    caps_ls: Vec<String>,
    caps_acked: HashSet<String>,
    cap_end_sent: bool,
    sasl_pending: bool,
    burst_done: bool,
    last_rx: Instant,
    ping_sent: bool,
    connect_started: Instant,
    next_connect: Option<Instant>,
    backoff: Duration,
    last_cursor_flush: Instant,
    last_error: Option<String>,
    batch_seq: u64,
    connected_since: Option<i64>,
    /// Language of the network's owner, for messages from *status.
    lang: crate::i18n::Lang,
    mods: crate::modules::NetModules,
    rt: mods::ModRuntime,
    /// SASL mechanism chosen for this connection.
    sasl_mech: &'static str,
    /// Whether the owner may change this network's settings from IRC.
    can_edit: bool,
    /// WebAssembly plugins enabled on this network.
    plugins: Vec<plugins::ActivePlugin>,
    /// Our address on the connection to the IRC server (for DCC).
    upstream_local: Option<std::net::IpAddr>,
    /// Read markers by folded target: (target as shown, ms).
    read_markers: HashMap<String, (String, i64)>,
    markers_loaded: bool,
    /// Channels whose history is not stored (folded names).
    no_store: HashSet<String>,
    /// Cursors as last written to the database, by device name.
    flushed: HashMap<String, i64>,
    /// The owner's time zone (logs, scheduled tasks).
    zone: crate::tz::Zone,
    /// Options edited from *status (backup servers, bind host, trusted
    /// certificates), read on every connection attempt.
    opts: db::NetOptions,
    /// Which server of the list (main, then backups) the next attempt uses.
    server_idx: usize,
    /// The last attempt failed and the next one goes to another server.
    next_server_fast: bool,
}

/// Starts the actor for a network and returns its handle.
pub fn spawn(
    ctx: Arc<Ctx>,
    net: Network,
    owner: String,
) -> (NetHandle, tokio::task::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel(1024);
    let handle = NetHandle {
        network_id: net.id,
        user_id: net.user_id,
        name: net.name.clone(),
        tx,
    };
    let now = Instant::now();
    let actor = NetActor {
        ctx,
        nick: net.nick.clone(),
        net,
        owner,
        self_tx: handle.tx.downgrade(),
        rx,
        clients: Vec::new(),
        reader: None,
        writer: None,
        phase: Phase::Disconnected,
        own_prefix: None,
        isupport: Isupport::default(),
        channels: HashMap::new(),
        caps_ls: Vec::new(),
        caps_acked: HashSet::new(),
        cap_end_sent: false,
        sasl_pending: false,
        burst_done: false,
        last_rx: now,
        ping_sent: false,
        connect_started: now,
        next_connect: Some(now),
        backoff: BACKOFF_MIN,
        last_cursor_flush: now,
        connected_since: None,
        last_error: None,
        batch_seq: 0,
        lang: crate::i18n::Lang::default(),
        mods: crate::modules::NetModules::default(),
        rt: mods::ModRuntime::default(),
        sasl_mech: "PLAIN",
        can_edit: false,
        opts: db::NetOptions::default(),
        server_idx: 0,
        next_server_fast: false,
        plugins: Vec::new(),
        upstream_local: None,
        read_markers: HashMap::new(),
        markers_loaded: false,
        no_store: HashSet::new(),
        flushed: HashMap::new(),
        zone: crate::tz::Zone::default(),
    };
    let join = tokio::spawn(actor.run());
    (handle, join)
}

async fn next_line(r: &mut Option<Reader>) -> Option<std::io::Result<String>> {
    match r {
        Some(r) => r.next().await,
        None => std::future::pending().await,
    }
}

/// Writes lines to the server with token-bucket flood control.
async fn writer_task(ctx: Arc<Ctx>, w: WriteHalf<BoxStream>, mut rx: mpsc::Receiver<String>) {
    let mut sink = FramedWrite::new(w, IrcCodec::default());
    let mut tokens = FLOOD_BURST;
    let mut last = Instant::now();
    while let Some(line) = rx.recv().await {
        let now = Instant::now();
        tokens = (tokens + now.duration_since(last).as_secs_f64() * FLOOD_RATE).min(FLOOD_BURST);
        last = now;
        if tokens < 1.0 {
            let wait = (1.0 - tokens) / FLOOD_RATE;
            tokio::time::sleep(Duration::from_secs_f64(wait)).await;
            tokens = 1.0;
            last = Instant::now();
        }
        tokens -= 1.0;
        crate::metrics::Metrics::add(&ctx.metrics.bytes_to_servers, line.len() as u64 + 2);
        if sink.send(line).await.is_err() {
            break;
        }
    }
    let _ = sink.close().await;
}

async fn open_stream(
    ctx: &Ctx,
    net: &Network,
    tls_cfg: Option<Arc<rustls::ClientConfig>>,
    bind: Option<std::net::IpAddr>,
) -> anyhow::Result<(BoxStream, Option<std::net::IpAddr>)> {
    let tcp = match bind {
        None => TcpStream::connect((net.host.as_str(), net.port)).await?,
        Some(ip) => connect_from(ip, &net.host, net.port).await?,
    };
    tcp.set_nodelay(true)?;
    let local = tcp.local_addr().ok().map(|a| a.ip());
    if !net.tls {
        return Ok((Box::new(tcp), local));
    }
    let cfg = tls_cfg.unwrap_or_else(|| ctx.tls_client.clone());
    let connector = tokio_rustls::TlsConnector::from(cfg);
    let name = rustls_pki_types::ServerName::try_from(net.host.clone())?;
    let tls = connector.connect(name, tcp).await?;
    Ok((Box::new(tls), local))
}

/// Connects from a chosen local address (`*status setbindhost`), to the
/// server addresses of the same family.
async fn connect_from(ip: std::net::IpAddr, host: &str, port: u16) -> anyhow::Result<TcpStream> {
    let mut last = None;
    for addr in tokio::net::lookup_host((host, port)).await? {
        if addr.is_ipv4() != ip.is_ipv4() {
            continue;
        }
        let sock = if ip.is_ipv4() {
            tokio::net::TcpSocket::new_v4()?
        } else {
            tokio::net::TcpSocket::new_v6()?
        };
        if let Err(e) = sock.bind(std::net::SocketAddr::new(ip, 0)) {
            anyhow::bail!("cannot use the local address {ip}: {e}");
        }
        match sock.connect(addr).await {
            Ok(s) => return Ok(s),
            Err(e) => last = Some(e),
        }
    }
    match last {
        Some(e) => Err(e.into()),
        None => anyhow::bail!(
            "{host} has no {} address for the local address {ip}",
            if ip.is_ipv4() { "IPv4" } else { "IPv6" }
        ),
    }
}

/// Simple jitter in [0.8, 1.2) without pulling in an RNG crate.
fn jitter(d: Duration) -> Duration {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|t| t.subsec_nanos())
        .unwrap_or(0);
    let factor = 0.8 + (nanos % 400) as f64 / 1000.0;
    d.mul_f64(factor)
}

/// Removes tags the client did not negotiate and serialises the message.
pub fn render_for(msg: &Message, caps: &ClientCaps) -> String {
    let mut m = msg.clone();
    if caps.message_tags {
        if !caps.server_time {
            m.remove_tag("time");
        }
        if !caps.batch {
            m.remove_tag("batch");
        }
    } else {
        m.tags
            .retain(|(k, _)| (caps.server_time && k == "time") || (caps.batch && k == "batch"));
    }
    m.to_string()
}

/// Adapts a message from the server to what a client negotiated: drops
/// notifications it did not ask for and trims extended forms. None: do not
/// send it to this client.
pub fn adapt_for(msg: &Message, caps: &ClientCaps, me: &str) -> Option<Message> {
    let mut m = msg.clone();
    match m.command.as_str() {
        "AWAY" if !caps.away_notify => return None,
        "ACCOUNT" if !caps.account_notify => return None,
        "CHGHOST" if !caps.chghost => return None,
        "SETNAME" if !caps.setname => return None,
        "INVITE"
            if !caps.invite_notify && !m.param(0).is_some_and(|t| t.eq_ignore_ascii_case(me)) =>
        {
            return None
        }
        "JOIN" => {
            if caps.extended_join {
                // Servers without extended-join send the short form.
                if m.params.len() == 1 {
                    m.params.push("*".into());
                    m.params.push(String::new());
                }
            } else {
                m.params.truncate(1);
            }
        }
        "353" if !caps.multi_prefix || !caps.userhost_in_names => {
            if let Some(names) = m.params.last_mut() {
                *names = names
                    .split_whitespace()
                    .map(|item| trim_name(item, caps))
                    .collect::<Vec<_>>()
                    .join(" ");
            }
        }
        "352" if !caps.multi_prefix => {
            if let Some(flags) = m.params.get_mut(6) {
                *flags = trim_who_flags(flags);
            }
        }
        _ => {}
    }
    if !caps.account_tag {
        m.remove_tag("account");
    }
    Some(m)
}

/// One NAMES entry for a client: highest prefix only without multi-prefix,
/// bare nick without userhost-in-names.
fn trim_name(item: &str, caps: &ClientCaps) -> String {
    let syms: String = item
        .chars()
        .take_while(|c| PREFIX_SYMBOLS.contains(*c))
        .collect();
    let rest = &item[syms.len()..];
    let syms = if caps.multi_prefix {
        syms
    } else {
        syms.chars().next().map(String::from).unwrap_or_default()
    };
    let nick = if caps.userhost_in_names {
        rest
    } else {
        rest.split('!').next().unwrap_or(rest)
    };
    format!("{syms}{nick}")
}

/// WHO flags (`H*@+`) with only the highest prefix symbol.
fn trim_who_flags(flags: &str) -> String {
    let mut out = String::new();
    let mut seen = false;
    for c in flags.chars() {
        if PREFIX_SYMBOLS.contains(c) {
            if seen {
                continue;
            }
            seen = true;
        }
        out.push(c);
    }
    out
}

fn parse_msgid(s: &str) -> Option<i64> {
    s.strip_prefix('x').and_then(|n| n.parse().ok())
}

impl NetActor {
    async fn run(mut self) {
        info!(user = %self.owner, network = %self.net.name, "network actor started");
        self.load_modules().await;
        loop {
            if self.reader.is_none() {
                if let Some(t) = self.next_connect {
                    if Instant::now() >= t {
                        self.connect().await;
                    }
                }
            }
            let mut deadline = Instant::now() + Duration::from_secs(5);
            if let Some(t) = self.next_module_timer() {
                deadline = deadline.min(t.max(Instant::now() + Duration::from_millis(50)));
            }
            if self.reader.is_none() {
                if let Some(t) = self.next_connect {
                    deadline = deadline.min(t);
                }
            }
            let ev = tokio::select! {
                c = self.rx.recv() => Ev::Cmd(c),
                l = next_line(&mut self.reader) => Ev::Line(l),
                _ = tokio::time::sleep_until(deadline) => Ev::Tick,
            };
            match ev {
                Ev::Cmd(None) | Ev::Cmd(Some(NetCmd::Shutdown)) => {
                    self.shutdown().await;
                    break;
                }
                Ev::Cmd(Some(cmd)) => self.on_cmd(cmd).await,
                Ev::Line(Some(Ok(line))) => self.on_upstream_line(&line).await,
                Ev::Line(Some(Err(e))) => self.on_disconnect(&format!("read error: {e}")),
                Ev::Line(None) => self.on_disconnect("connection closed by server"),
                Ev::Tick => self.on_tick().await,
            }
        }
        info!(user = %self.owner, network = %self.net.name, "network actor stopped");
    }

    // ------------------------------------------------------------ helpers

    fn host(&self) -> &str {
        &self.ctx.config.hostname
    }

    fn current_nick(&self) -> String {
        if self.nick.is_empty() {
            self.net.nick.clone()
        } else {
            self.nick.clone()
        }
    }

    fn fold(&self, s: &str) -> String {
        self.isupport.casemapping.fold(s)
    }

    fn is_me(&self, nick: &str) -> bool {
        self.isupport.casemapping.eq(nick, &self.nick)
    }

    fn self_prefix(&self) -> String {
        self.own_prefix
            .clone()
            .unwrap_or_else(|| self.current_nick())
    }

    /// Queues a line for the server.
    fn send(&mut self, msg: Message) {
        // While the hidden host is pending, no JOIN may leave: it would
        // show the real address in the channel.
        if msg.command.eq_ignore_ascii_case("JOIN") && self.joins_held() {
            if self.rt.x.held_joins.len() < extra::HELD_JOINS_MAX {
                self.rt.x.held_joins.push(msg);
            }
            return;
        }
        let line = self.charset_mark(&msg, msg.to_string());
        if let Some(w) = &self.writer {
            trace!(network = %self.net.name, line = %redact_line(&line), "->");
            match w.try_send(line) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    warn!(network = %self.net.name, "upstream send queue full; dropping line")
                }
                Err(TrySendError::Closed(_)) => {}
            }
        }
    }

    fn send_to(&mut self, id: ClientId, line: String) {
        if let Some(c) = self.clients.iter().find(|c| c.h.id == id) {
            if c.h.tx.try_send(line).is_err() {
                self.drop_client(id, "send queue full");
            }
        }
    }

    fn status_line(&self, command: &str, text: &str) -> String {
        format!(
            ":*status!xbnc@{} {} {} :{}",
            self.host(),
            command,
            self.current_nick(),
            text
        )
    }

    /// Sends a notice from *status to every attached client.
    fn notice_all(&mut self, text: &str) {
        let line = self.status_line("NOTICE", text);
        let ids: Vec<ClientId> = self.clients.iter().map(|c| c.h.id).collect();
        for id in ids {
            self.send_to(id, line.clone());
        }
    }

    fn reply_status(&mut self, id: ClientId, text: &str) {
        let line = self.status_line("PRIVMSG", text);
        self.send_to(id, line);
    }

    fn drop_client(&mut self, id: ClientId, reason: &str) {
        if let Some(pos) = self.clients.iter().position(|c| c.h.id == id) {
            let c = self.clients.remove(pos);
            warn!(network = %self.net.name, client = %c.h.name, reason, "dropping client");
            self.persist_cursor(c.h.name.clone(), c.last_id);
            self.after_detach();
            // Dropping the sender closes the client's writer and its connection.
        }
    }

    fn persist_cursor(&self, client: String, last_id: i64) {
        if last_id <= 0 {
            return;
        }
        let db = self.ctx.db.clone();
        let nid = self.net.id;
        tokio::spawn(async move {
            if let Err(e) = db
                .call(move |c| db::set_cursor(c, nid, &client, last_id))
                .await
            {
                warn!(error = %e, "cannot store history cursor");
            }
        });
    }

    async fn flush_cursors(&mut self) {
        let mut best: HashMap<String, i64> = HashMap::new();
        for c in &self.clients {
            let e = best.entry(c.h.name.clone()).or_insert(0);
            *e = (*e).max(c.last_id);
        }
        let nid = self.net.id;
        let items: Vec<(String, i64)> = best
            .into_iter()
            .filter(|(k, v)| *v > 0 && self.flushed.get(k) != Some(v))
            .collect();
        self.last_cursor_flush = Instant::now();
        if items.is_empty() {
            return;
        }
        for (k, v) in &items {
            self.flushed.insert(k.clone(), *v);
        }
        if let Err(e) = self
            .ctx
            .db
            .call(move |c| {
                for (name, id) in &items {
                    db::set_cursor(c, nid, name, *id)?;
                }
                Ok(())
            })
            .await
        {
            warn!(error = %e, "cannot store history cursors");
        }
        self.last_cursor_flush = Instant::now();
    }

    /// Delivers a message to every attached client except `except`, and
    /// advances their cursor when the message has a history id.
    fn broadcast_except(&mut self, msg: &Message, id: Option<i64>, except: Option<ClientId>) {
        let mut dropped = Vec::new();
        let me = self.current_nick();
        for c in self.clients.iter_mut() {
            if Some(c.h.id) == except {
                continue;
            }
            let Some(m) = adapt_for(msg, &c.h.caps, &me) else {
                continue;
            };
            match c.h.tx.try_send(render_for(&m, &c.h.caps)) {
                Ok(()) => {
                    if let Some(id) = id {
                        c.last_id = c.last_id.max(id);
                    }
                }
                Err(_) => dropped.push(c.h.id),
            }
        }
        for id in dropped {
            self.drop_client(id, "client too slow or gone");
        }
    }

    fn broadcast(&mut self, msg: &Message, id: Option<i64>) {
        self.broadcast_except(msg, id, None);
    }

    // ------------------------------------------------------- connection

    async fn connect(&mut self) {
        let nid = self.net.id;
        match self
            .ctx
            .db
            .call(move |c| db::get_network_by_id(c, nid))
            .await
        {
            Ok(Some(n)) => self.net = n,
            Ok(None) => {
                info!(network = %self.net.name, "network was deleted; not connecting");
                self.next_connect = None;
                return;
            }
            Err(e) => {
                warn!(error = %e, "cannot load network");
                self.schedule_reconnect();
                return;
            }
        }
        if !self.net.enabled {
            self.next_connect = None;
            return;
        }
        self.load_modules().await;
        let (uid, nid) = (self.net.user_id, self.net.id);
        let (opts, user_bind) = self
            .ctx
            .db
            .call(move |c| {
                Ok((
                    db::get_net_options(c, nid)?,
                    db::get_user_bind_host(c, uid)?,
                ))
            })
            .await
            .unwrap_or_default();
        self.opts = opts;
        // Main server first, then the backups, in turn.
        let total = 1 + self.opts.servers.len();
        let idx = self.server_idx % total;
        if idx > 0 {
            let alt = self.opts.servers[idx - 1].clone();
            self.net.host = alt.host;
            self.net.port = alt.port;
            self.net.tls = alt.tls;
            if alt.password_enc.is_some() {
                self.net.server_password_enc = alt.password_enc;
            }
        }
        let bind = if self.opts.bind_host.is_empty() {
            user_bind
        } else {
            self.opts.bind_host.clone()
        };
        let bind: Option<std::net::IpAddr> = bind.parse().ok();
        let (tls_cfg, client_cert) = self.network_tls_config().await;
        info!(user = %self.owner, network = %self.net.name, host = %self.net.host,
              port = self.net.port, tls = self.net.tls, client_cert, server = idx + 1,
              servers = total, bind = ?bind, "connecting");
        let result = tokio::time::timeout(
            CONNECT_TIMEOUT,
            open_stream(&self.ctx, &self.net, tls_cfg.clone(), bind),
        )
        .await;
        let stream = match result {
            Ok(Ok((s, local))) => {
                self.upstream_local = local;
                s
            }
            Ok(Err(e)) => {
                self.connection_failed(&format!("connection failed: {e}"));
                return;
            }
            Err(_) => {
                self.connection_failed("connection timed out");
                return;
            }
        };
        let (r, w) = tokio::io::split(stream);
        self.reader = Some(FramedRead::new(
            r,
            IrcCodec::with_legacy(self.charset_legacy()),
        ));
        let (tx, rx) = mpsc::channel(UPSTREAM_QUEUE);
        tokio::spawn(writer_task(self.ctx.clone(), w, rx));
        self.writer = Some(tx);
        self.phase = Phase::Registering;
        self.nick = self.net.nick.clone();
        self.own_prefix = None;
        self.isupport = Isupport::default();
        self.channels.clear();
        self.caps_ls.clear();
        self.caps_acked.clear();
        self.cap_end_sent = false;
        self.sasl_pending = false;
        self.burst_done = false;
        self.ping_sent = false;
        self.last_rx = Instant::now();
        self.connect_started = Instant::now();
        self.next_connect = None;
        self.reset_module_state();
        self.sasl_mech = if client_cert && self.mods.cert.is_some_and(|c| c.sasl_external) {
            "EXTERNAL"
        } else {
            "PLAIN"
        };

        self.send(Message::cmd("CAP", &["LS", "302"]));
        if let Some(enc) = self.net.server_password_enc.clone() {
            match self.ctx.secrets.decrypt(&enc) {
                Ok(pw) => self.send(Message::cmd("PASS", &[pw.expose()])),
                Err(e) => {
                    warn!(network = %self.net.name, error = %e, "cannot decrypt server password")
                }
            }
        } else if let Some(loc) = self.services_pass() {
            self.send(Message::cmd("PASS", &[&loc]));
        }
        let nick = self.net.nick.clone();
        let user = self.net.username.clone();
        let real = self.net.realname.clone();
        self.send(Message::cmd("NICK", &[&nick]));
        self.send(Message::cmd("USER", &[&user, "0", "*", &real]));
    }

    fn connection_failed(&mut self, reason: &str) {
        warn!(user = %self.owner, network = %self.net.name, reason, "upstream connection failed");
        self.last_error = Some(reason.to_string());
        // The next attempt tries the next server of the list.
        self.server_idx = self.server_idx.wrapping_add(1);
        self.next_server_fast = true;
        self.schedule_reconnect();
    }

    fn schedule_reconnect(&mut self) {
        self.reader = None;
        self.writer = None;
        self.phase = Phase::Disconnected;
        self.ctx.events.send(crate::events::Event::NetworksChanged {
            user_id: self.net.user_id,
        });
        self.channels.clear();
        if !self.net.enabled {
            self.next_connect = None;
            return;
        }
        self.connected_since = None;
        crate::metrics::Metrics::add(&self.ctx.metrics.reconnects, 1);
        if self.last_error.as_deref().is_some_and(is_throttle_error) {
            self.backoff = self.backoff.max(THROTTLE_BACKOFF);
        }
        // Moving on to another server of the list does not wait; after a
        // full round the usual growing delay applies.
        let total = 1 + self.opts.servers.len();
        let fast = std::mem::take(&mut self.next_server_fast);
        let delay = if fast
            && total > 1
            && !self.server_idx.is_multiple_of(total)
            && !self.last_error.as_deref().is_some_and(is_throttle_error)
        {
            BACKUP_SERVER_DELAY
        } else {
            let d = jitter(self.backoff);
            self.backoff = (self.backoff * 2).min(BACKOFF_MAX);
            d
        };
        self.next_connect = Some(Instant::now() + delay);
        let reason = self
            .last_error
            .clone()
            .unwrap_or_else(|| self.t("phase_disconnected", &[]));
        let text = format!(
            "{} {}",
            self.t("disconnected_from", &[&self.net.name, &reason]),
            self.t("reconnecting_in", &[&delay.as_secs().to_string()])
        );
        self.notice_all(&text);
    }

    fn on_disconnect(&mut self, reason: &str) {
        info!(user = %self.owner, network = %self.net.name, reason, "upstream disconnected");
        if self.last_error.is_none() || self.phase == Phase::Connected {
            self.last_error = Some(reason.to_string());
        }
        self.schedule_reconnect();
    }

    /// Closes the upstream connection on purpose, optionally sending QUIT.
    fn close_upstream(&mut self, quit: Option<&str>) {
        if let Some(q) = quit {
            if self.phase != Phase::Disconnected {
                self.send(Message::cmd("QUIT", &[q]));
            }
        }
        self.reader = None;
        self.writer = None; // writer task flushes queued lines (incl. QUIT) and exits
        self.phase = Phase::Disconnected;
        self.ctx.events.send(crate::events::Event::NetworksChanged {
            user_id: self.net.user_id,
        });
        self.connected_since = None;
        self.channels.clear();
    }

    async fn on_tick(&mut self) {
        let now = Instant::now();
        match self.phase {
            Phase::Registering => {
                if now.duration_since(self.connect_started) > REGISTRATION_TIMEOUT {
                    self.close_upstream(None);
                    self.last_error = Some("registration timed out".into());
                    self.schedule_reconnect();
                }
            }
            Phase::Connected => {
                let idle = now.duration_since(self.last_rx);
                if idle > KEEPALIVE_TIMEOUT {
                    self.close_upstream(None);
                    self.last_error = Some("ping timeout".into());
                    self.schedule_reconnect();
                } else if idle > KEEPALIVE_IDLE && !self.ping_sent {
                    let host = self.host().to_string();
                    self.send(Message::cmd("PING", &[&host]));
                    self.ping_sent = true;
                }
                if !self.burst_done
                    && now.duration_since(self.connect_started) > Duration::from_secs(30)
                {
                    self.burst_done = true;
                    self.on_burst_end().await;
                }
            }
            Phase::Disconnected => {}
        }
        if now.duration_since(self.last_cursor_flush) > CURSOR_FLUSH {
            self.flush_cursors().await;
        }
        self.modules_tick().await;
        if self.plugins.iter().any(|p| !p.stopped) {
            self.plugin_event(None, serde_json::json!({"event": "timer"}), None)
                .await;
        }
    }

    /// TLS config with the network's client certificate, when the cert
    /// module is on and a certificate is stored.
    /// TLS settings of this network when they differ from the default: a
    /// client certificate (the `cert` module) and trusted server
    /// fingerprints. Returns the configuration (None: the default one) and
    /// whether a client certificate is in use.
    async fn network_tls_config(&mut self) -> (Option<Arc<rustls::ClientConfig>>, bool) {
        if !self.net.tls {
            return (None, false);
        }
        let mut client = None;
        if self.mods.cert.is_some() {
            let nid = self.net.id;
            if let Ok(Some((cert, key_enc))) =
                self.ctx.db.call(move |c| db::get_client_cert(c, nid)).await
            {
                match self
                    .ctx
                    .secrets
                    .decrypt(&key_enc)
                    .and_then(|key| crate::certs::parse(&cert, key.expose()))
                {
                    Ok(pair) => client = Some(pair),
                    Err(e) => {
                        warn!(network = %self.net.name, error = %e, "cannot use the client certificate")
                    }
                }
            }
        }
        if client.is_none() && self.opts.trusted_fps.is_empty() {
            return (None, false);
        }
        let has_cert = client.is_some();
        match crate::certs::network_client_config(
            self.ctx.tls_roots.clone(),
            client,
            &self.opts.trusted_fps,
        ) {
            Ok(c) => (Some(c), has_cert),
            Err(e) => {
                warn!(network = %self.net.name, error = %e, "cannot build the TLS settings");
                (None, false)
            }
        }
    }

    async fn shutdown(&mut self) {
        self.flush_cursors().await;
        let text = self.t("shutting_down", &[]);
        self.notice_all(&text);
        self.close_upstream(Some("xBNC shutting down"));
        self.clients.clear();
        // Give the writer task a moment to flush the QUIT.
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // ------------------------------------------------------ registration

    fn cap_end(&mut self) {
        if !self.cap_end_sent {
            self.cap_end_sent = true;
            self.send(Message::cmd("CAP", &["END"]));
        }
    }

    fn on_cap(&mut self, msg: &Message) {
        let sub = msg.param(1).unwrap_or("").to_ascii_uppercase();
        match sub.as_str() {
            "LS" => {
                let more = msg.params.len() >= 4 && msg.param(2) == Some("*");
                let list = if more {
                    msg.param(3)
                } else {
                    msg.params.last().map(String::as_str)
                };
                self.caps_ls
                    .extend(list.unwrap_or("").split_whitespace().map(str::to_string));
                if more || self.phase != Phase::Registering {
                    return;
                }
                let has = |name: &str| -> Option<String> {
                    self.caps_ls.iter().find_map(|c| {
                        let (k, v) = c.split_once('=').unwrap_or((c.as_str(), ""));
                        (k == name).then(|| v.to_string())
                    })
                };
                let mut want: Vec<&str> = Vec::new();
                for cap in UPSTREAM_CAPS {
                    if has(cap).is_some() {
                        want.push(cap);
                    }
                }
                let offers = |mech: &str| {
                    has("sasl").is_some_and(|v| v.is_empty() || v.split(',').any(|m| m == mech))
                };
                let (offers_external, offers_plain) = (offers("EXTERNAL"), offers("PLAIN"));
                if self.sasl_mech == "EXTERNAL" && !offers_external {
                    let text = self.t("cert_no_external", &[]);
                    self.notice_all(&text);
                    self.sasl_mech = "PLAIN";
                }
                if self.sasl_mech == "EXTERNAL" {
                    want.push("sasl");
                } else if self.net.sasl_username.is_some() {
                    if offers_plain {
                        want.push("sasl");
                    } else {
                        warn!(network = %self.net.name, "SASL configured but the server does not offer SASL PLAIN");
                        let text = self.t("sasl_not_offered", &["PLAIN"]);
                        self.notice_all(&text);
                    }
                }
                if want.is_empty() {
                    self.cap_end();
                } else {
                    self.send(Message::cmd("CAP", &["REQ", &want.join(" ")]));
                }
            }
            "ACK" => {
                for c in msg
                    .params
                    .last()
                    .map(String::as_str)
                    .unwrap_or("")
                    .split_whitespace()
                {
                    match c.strip_prefix('-') {
                        Some(removed) => {
                            self.caps_acked.remove(removed);
                        }
                        None => {
                            self.caps_acked.insert(c.to_string());
                        }
                    }
                }
                if self.phase == Phase::Registering && !self.cap_end_sent {
                    if self.caps_acked.contains("sasl") && !self.sasl_pending {
                        self.sasl_pending = true;
                        let mech = self.sasl_mech;
                        self.send(Message::cmd("AUTHENTICATE", &[mech]));
                    } else if !self.sasl_pending {
                        self.cap_end();
                    }
                }
            }
            "NAK" if self.phase == Phase::Registering => self.cap_end(),
            "NEW" => {
                // cap-notify: request what we use among the new capabilities.
                let list = msg.params.last().map(String::as_str).unwrap_or("");
                let mut want = Vec::new();
                for c in list.split_whitespace() {
                    self.caps_ls.push(c.to_string());
                    let name = c.split_once('=').map_or(c, |(k, _)| k);
                    if UPSTREAM_CAPS.contains(&name) && !self.caps_acked.contains(name) {
                        want.push(name.to_string());
                    }
                }
                if !want.is_empty() && self.phase == Phase::Connected {
                    self.send(Message::cmd("CAP", &["REQ", &want.join(" ")]));
                }
            }
            "DEL" => {
                let list = msg.params.last().map(String::as_str).unwrap_or("");
                for c in list.split_whitespace() {
                    self.caps_acked.remove(c);
                    self.caps_ls
                        .retain(|x| x.split_once('=').map_or(x.as_str(), |(k, _)| k) != c);
                }
            }
            _ => {}
        }
    }

    fn on_authenticate(&mut self, msg: &Message) {
        if !self.sasl_pending || msg.param(0) != Some("+") {
            return;
        }
        if self.sasl_mech == "EXTERNAL" {
            self.send(Message::cmd("AUTHENTICATE", &["+"]));
            return;
        }
        let user = self.net.sasl_username.clone().unwrap_or_default();
        let pass = match self
            .net
            .sasl_password_enc
            .as_deref()
            .map(|e| self.ctx.secrets.decrypt(e))
        {
            Some(Ok(p)) => p,
            _ => {
                warn!(network = %self.net.name, "cannot decrypt SASL password; aborting SASL");
                self.send(Message::cmd("AUTHENTICATE", &["*"]));
                self.sasl_pending = false;
                self.cap_end();
                return;
            }
        };
        let payload = B64.encode(format!("{user}\0{user}\0{}", pass.expose()));
        let bytes = payload.as_bytes();
        for chunk in bytes.chunks(400) {
            let s = String::from_utf8_lossy(chunk).into_owned();
            self.send(Message::new("AUTHENTICATE", vec![s]));
        }
        if bytes.len() % 400 == 0 {
            self.send(Message::cmd("AUTHENTICATE", &["+"]));
        }
    }

    async fn on_welcome(&mut self, msg: &Message) {
        if let Some(n) = msg.param(0) {
            self.nick = n.to_string();
        }
        self.phase = Phase::Connected;
        self.ctx.events.send(crate::events::Event::NetworksChanged {
            user_id: self.net.user_id,
        });
        self.connected_since = Some(db::now_ms());
        self.backoff = BACKOFF_MIN;
        self.last_error = None;
        info!(user = %self.owner, network = %self.net.name, nick = %self.nick, "registered on network");
        let text = self.t("connected_to", &[&self.net.name, &self.nick]);
        self.notice_all(&text);
        if self.clients.is_empty() {
            self.rt.detached_at = Some(Instant::now());
        }
        self.run_on_connect().await;
        self.services_on_welcome().await;
        self.plugin_event(None, serde_json::json!({"event": "connected"}), None)
            .await;
    }

    /// Sends the network's "commands on connect" (services login, user
    /// modes), before joining channels so a hidden host applies to them.
    async fn run_on_connect(&mut self) {
        let nid = self.net.id;
        let enc = match self.ctx.db.call(move |c| db::get_on_connect(c, nid)).await {
            Ok(Some(e)) => e,
            Ok(None) => return,
            Err(e) => {
                warn!(error = %e, "cannot load commands on connect");
                return;
            }
        };
        let text = match self.ctx.secrets.decrypt(&enc) {
            Ok(t) => t,
            Err(e) => {
                warn!(network = %self.net.name, error = %e, "cannot decrypt commands on connect");
                return;
            }
        };
        match crate::perform::to_irc_lines(text.expose(), &self.nick) {
            Ok(lines) => {
                let mut msgs = Vec::new();
                for line in &lines {
                    match Message::parse(line) {
                        Ok(m) => msgs.push(m),
                        Err(e) => {
                            warn!(network = %self.net.name, error = %e, "invalid command on connect")
                        }
                    }
                }
                // Networks added from the EvilNet preset before 1.0.0 was
                // corrected log in with AuthServ, but EvilNet answers to
                // NickServ: send the same account and password there.
                let evilnet = format!("{} {}", self.isupport.network.as_deref().unwrap_or(""), self.net.host)
                    .to_ascii_lowercase()
                    .contains("evilnet");
                if evilnet {
                    for m in msgs.iter_mut() {
                        evilnet_login_to_nickserv(m);
                    }
                }
                // +x asked for: hold every JOIN (also those written here)
                // until the server hides the host.
                if msgs.iter().any(|m| sets_own_hidden_host(m, &self.nick)) {
                    self.rt.x.perform_hide = true;
                    self.rt.x.join_hold = Some(Instant::now());
                }
                for m in msgs {
                    self.send(m);
                }
                if !lines.is_empty() {
                    info!(user = %self.owner, network = %self.net.name, count = lines.len(), "sent commands on connect");
                }
            }
            Err(e) => warn!(network = %self.net.name, error = %e, "invalid commands on connect"),
        }
    }

    async fn join_saved_channels(&mut self) {
        let nid = self.net.id;
        let chans = match self.ctx.db.call(move |c| db::list_channels(c, nid)).await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "cannot load channels");
                return;
            }
        };
        let mut plain: Vec<String> = Vec::new();
        for ch in chans {
            if ch.disabled || self.channels.contains_key(&self.fold(&ch.name)) {
                continue;
            }
            match ch.key_enc.as_deref().map(|k| self.ctx.secrets.decrypt(k)) {
                Some(Ok(key)) => {
                    let name = ch.name.clone();
                    self.send(Message::cmd("JOIN", &[&name, key.expose()]));
                }
                Some(Err(e)) => {
                    warn!(channel = %ch.name, error = %e, "cannot decrypt channel key; joining without it");
                    plain.push(ch.name);
                }
                None => plain.push(ch.name),
            }
        }
        let mut cur = String::new();
        for name in plain {
            if !cur.is_empty() && cur.len() + name.len() + 1 > 400 {
                let line = std::mem::take(&mut cur);
                self.send(Message::new("JOIN", vec![line]));
            }
            if !cur.is_empty() {
                cur.push(',');
            }
            cur.push_str(&name);
        }
        if !cur.is_empty() {
            self.send(Message::new("JOIN", vec![cur]));
        }
    }

    // ------------------------------------------------- upstream messages

    async fn on_upstream_line(&mut self, line: &str) {
        let msg = match Message::parse(line) {
            Ok(m) => m,
            Err(e) => {
                debug!(network = %self.net.name, error = %e, "ignoring unparseable line from server");
                return;
            }
        };
        let msg = self.crypt_topic(msg);
        trace!(network = %self.net.name, line = %redact_line(line), "<-");
        crate::metrics::Metrics::add(&self.ctx.metrics.bytes_from_servers, line.len() as u64 + 2);
        crate::metrics::Metrics::add(&self.ctx.metrics.lines_from_servers, 1);
        self.last_rx = Instant::now();
        self.ping_sent = false;

        let cmd = msg.command.clone();
        let numeric = cmd.len() == 3 && cmd.bytes().all(|b| b.is_ascii_digit());
        let events = match cmd.as_str() {
            "JOIN" | "PART" | "QUIT" | "KICK" | "NICK" | "TOPIC" | "MODE" => {
                self.capture_events(&msg)
            }
            _ => Vec::new(),
        };
        if matches!(cmd.as_str(), "375" | "372" | "376" | "422") {
            self.on_motd_numeric(&msg);
        }
        match cmd.as_str() {
            "PING" => {
                self.send(Message::new("PONG", msg.params.clone()));
                return;
            }
            "PONG" => return,
            "CAP" => {
                self.on_cap(&msg);
                return;
            }
            "AUTHENTICATE" => {
                self.on_authenticate(&msg);
                return;
            }
            "903" => {
                info!(network = %self.net.name, "SASL authentication succeeded");
                self.rt.x.logged_in = true;
                self.sasl_pending = false;
                self.cap_end();
                return;
            }
            "902" | "904" | "905" | "906" | "907" | "908" => {
                if self.sasl_pending {
                    self.sasl_pending = false;
                    warn!(network = %self.net.name, numeric = %cmd, "SASL authentication failed");
                    let reason = msg.params.last().cloned().unwrap_or_default();
                    let text = self.t("sasl_failed", &[&reason]);
                    self.notice_all(&text);
                    self.cap_end();
                }
                return;
            }
            "001" => {
                self.on_welcome(&msg).await;
                return;
            }
            "005" => {
                if msg.params.len() >= 3 {
                    let toks = msg.params[1..msg.params.len() - 1].to_vec();
                    self.isupport.apply(&toks);
                }
            }
            "432" | "433" | "437" if self.phase == Phase::Connected && self.rt.keepnick_attempt => {
                // Our keepnick attempt failed; the user does not need to see it.
                self.rt.keepnick_attempt = false;
                return;
            }
            "432" | "433" | "437" if self.phase == Phase::Registering => {
                let base = msg.param(1).unwrap_or(&self.nick).to_string();
                let mut alt: String = base.chars().take(29).collect();
                alt.push('_');
                self.nick = alt.clone();
                self.send(Message::cmd("NICK", &[&alt]));
                return;
            }
            "376" | "422" if !self.burst_done => {
                self.burst_done = true;
                self.on_burst_end().await;
                return;
            }
            "900" => {
                self.rt.x.logged_in = true;
                self.services_release().await;
            }
            "471" | "473" | "474" | "475" => {
                if let Some(chan) = msg.param(1) {
                    let chan = chan.to_string();
                    self.invite_on_join_error(&cmd, &chan);
                }
            }
            "324" => {
                if let (Some(c), Some(m)) = (msg.param(1), msg.param(2)) {
                    let (c, m) = (c.to_string(), m.to_string());
                    self.track_chan_modes(&c, &m);
                }
            }
            "INVITE" => {
                if let (Some(to), Some(chan)) = (msg.param(0), msg.param(1)) {
                    if self.is_me(to) && !msg.prefix.as_deref().is_some_and(|p| self.is_ignored(p))
                    {
                        let chan = chan.to_string();
                        self.invite_on_invite(&chan).await;
                    } else if self.is_me(to) {
                        return;
                    }
                }
            }
            "ERROR" => {
                let reason = msg.param(0).unwrap_or("").to_string();
                self.last_error = Some(format!("server error: {reason}"));
                return;
            }
            "JOIN" => {
                self.on_join(&msg).await;
                if let (Some(chan), Some(nick), Some(prefix)) =
                    (msg.param(0), msg.source_nick(), msg.prefix.as_deref())
                {
                    if !self.is_me(nick) {
                        self.autoop_on_join(chan, prefix);
                    } else {
                        self.autocycle_on_join(chan);
                    }
                    let ev = serde_json::json!({
                        "event": "join", "channel": chan, "from": nick,
                        "from_mask": prefix, "self": self.is_me(nick),
                    });
                    self.plugin_presence(ev).await;
                }
            }
            "PART" => {
                self.on_part(&msg);
                if let (Some(chans), Some(nick)) = (msg.param(0), msg.source_nick()) {
                    for chan in chans.split(',').filter(|c| !c.is_empty()) {
                        if !self.is_me(nick) {
                            self.autocycle_check(chan);
                        }
                        let ev = serde_json::json!({
                            "event": "part", "channel": chan, "from": nick,
                            "from_mask": msg.prefix.as_deref().unwrap_or(""),
                            "text": msg.param(1).unwrap_or(""), "self": self.is_me(nick),
                        });
                        self.plugin_presence(ev).await;
                    }
                }
            }
            "KICK" => {
                let me = msg.param(1).is_some_and(|v| self.is_me(v));
                self.on_kick(&msg);
                if let Some(chan) = msg.param(0) {
                    if me {
                        self.on_kicked(chan);
                    } else {
                        self.autocycle_check(chan);
                    }
                }
                if let (Some(chan), Some(target)) = (msg.param(0), msg.param(1)) {
                    let ev = serde_json::json!({
                        "event": "kick", "channel": chan, "target": target,
                        "from": msg.source_nick().unwrap_or(""),
                        "text": msg.param(2).unwrap_or(""), "self": me,
                    });
                    self.plugin_presence(ev).await;
                }
            }
            "QUIT" => {
                let chans = msg
                    .source_nick()
                    .map(|n| self.channels_of(n))
                    .unwrap_or_default();
                self.on_quit(&msg);
                for c in &chans {
                    self.autocycle_check(c);
                }
                if let Some(n) = msg.source_nick() {
                    self.keepnick_on_release(n);
                    let ev = serde_json::json!({
                        "event": "quit", "from": n,
                        "from_mask": msg.prefix.as_deref().unwrap_or(""),
                        "text": msg.param(0).unwrap_or(""), "channels": chans,
                    });
                    self.plugin_presence(ev).await;
                }
            }
            "NICK" => {
                let old = msg.source_nick().unwrap_or("").to_string();
                let was_me = self.is_me(&old);
                let chans = self.channels_of(&old);
                self.on_nick(&msg);
                if was_me {
                    self.rt.keepnick_attempt = false;
                } else {
                    self.keepnick_on_release(&old);
                }
                if let Some(new) = msg.param(0) {
                    let ev = serde_json::json!({
                        "event": "nick", "from": old, "new": new,
                        "channels": chans, "self": was_me,
                    });
                    self.plugin_presence(ev).await;
                }
            }
            "MODE" => {
                self.on_mode(&msg);
                if let (Some(c), Some(m)) = (msg.param(0), msg.param(1)) {
                    if self.isupport.is_channel(c) {
                        let (c, m) = (c.to_string(), m.to_string());
                        self.track_chan_modes(&c, &m);
                    }
                }
            }
            "CHGHOST" => {
                if let (Some(nick), Some(user), Some(host)) =
                    (msg.source_nick(), msg.param(0), msg.param(1))
                {
                    if self.is_me(nick) {
                        self.own_prefix = Some(format!("{nick}!{user}@{host}"));
                        if self.hide_pending() {
                            self.host_now_hidden().await;
                        }
                    }
                }
            }
            "396" => {
                // Undernet and others: "<host> :is now your hidden host".
                if let Some(host) = msg.param(1) {
                    let user = self
                        .own_prefix
                        .as_deref()
                        .and_then(|p| p.split_once('!'))
                        .and_then(|(_, r)| r.split_once('@'))
                        .map(|(u, _)| u.to_string());
                    if let Some(user) = user {
                        self.own_prefix = Some(format!("{}!{user}@{host}", self.current_nick()));
                    }
                }
                self.host_now_hidden().await;
            }
            "TOPIC" => {
                if let (Some(c), Some(t)) = (msg.param(0), msg.param(1)) {
                    let cf = self.fold(c);
                    if let Some(ch) = self.channels.get_mut(&cf) {
                        ch.topic = (!t.is_empty()).then(|| t.to_string());
                    }
                }
            }
            "332" => {
                if let (Some(c), Some(t)) = (msg.param(1), msg.param(2)) {
                    let cf = self.fold(c);
                    if let Some(ch) = self.channels.get_mut(&cf) {
                        ch.topic = Some(t.to_string());
                    }
                }
            }
            "331" => {
                if let Some(c) = msg.param(1) {
                    let cf = self.fold(c);
                    if let Some(ch) = self.channels.get_mut(&cf) {
                        ch.topic = None;
                    }
                }
            }
            "353" => self.on_names(&msg),
            "366" => {
                if let Some(c) = msg.param(1) {
                    let cf = self.fold(c);
                    let mut members = None;
                    if let Some(ch) = self.channels.get_mut(&cf) {
                        ch.names_done = true;
                        members = Some((
                            ch.name.clone(),
                            ch.members
                                .values()
                                .map(|m| m.nick.clone())
                                .collect::<Vec<_>>(),
                        ));
                    }
                    if let Some((name, members)) = members {
                        if self.wants_presence() {
                            let ev = serde_json::json!({
                                "event": "names", "channel": name, "members": members,
                            });
                            self.plugin_presence(ev).await;
                        }
                    }
                }
            }
            "PRIVMSG" | "NOTICE" => {
                self.on_upstream_message(msg).await;
                return;
            }
            _ => {}
        }
        // The registration burst (welcome numerics, LUSERS, MOTD) is not
        // forwarded: clients get our own welcome when they attach.
        if numeric && !self.burst_done {
            return;
        }
        if numeric {
            match self.route_numeric(&msg) {
                Some(None) => {
                    self.on_internal_reply(&msg);
                    return;
                }
                Some(Some(id)) => {
                    if let Some(caps) = self.clients.iter().find(|c| c.h.id == id).map(|c| c.h.caps)
                    {
                        if let Some(m) = adapt_for(&msg, &caps, &self.current_nick()) {
                            self.send_to(id, render_for(&m, &caps));
                        }
                    }
                    return;
                }
                None => {}
            }
        }
        let id = if events.is_empty() {
            None
        } else {
            self.store_events(&msg, events).await
        };
        let mut out = msg;
        if let Some(i) = id {
            out.set_tag("msgid", Some(format!("x{i}")));
        }
        if self
            .message_channel(&out)
            .is_some_and(|c| self.is_hidden(c))
        {
            return;
        }
        self.broadcast(&out, id);
    }

    async fn on_join(&mut self, msg: &Message) {
        let (Some(chan), Some(nick)) = (msg.param(0), msg.source_nick()) else {
            return;
        };
        let cf = self.fold(chan);
        if self.is_me(nick) {
            self.own_prefix = msg.prefix.clone();
            self.channels.insert(cf.clone(), Chan::new(chan));
            let nid = self.net.id;
            let name = chan.to_string();
            if let Err(e) = self
                .ctx
                .db
                .call(move |c| db::upsert_channel(c, nid, &name, &cf, None, false))
                .await
            {
                warn!(error = %e, "cannot save channel");
            }
        } else if let Some(ch) = self.channels.get_mut(&cf) {
            let ncf = self.isupport.casemapping.fold(nick);
            ch.members.insert(
                ncf,
                Member {
                    nick: nick.to_string(),
                    symbols: String::new(),
                },
            );
        }
    }

    /// Channels (display names) where `nick` is right now.
    fn channels_of(&self, nick: &str) -> Vec<String> {
        if self.is_me(nick) {
            return self.channels.values().map(|c| c.name.clone()).collect();
        }
        let ncf = self.fold(nick);
        self.channels
            .values()
            .filter(|c| c.members.contains_key(&ncf))
            .map(|c| c.name.clone())
            .collect()
    }

    fn on_part(&mut self, msg: &Message) {
        let (Some(chans), Some(nick)) = (msg.param(0), msg.source_nick()) else {
            return;
        };
        let me = self.is_me(nick);
        let ncf = self.fold(nick);
        for chan in chans.split(',') {
            let cf = self.fold(chan);
            if me {
                self.channels.remove(&cf);
            } else if let Some(ch) = self.channels.get_mut(&cf) {
                ch.members.remove(&ncf);
            }
        }
    }

    fn on_kick(&mut self, msg: &Message) {
        let (Some(chan), Some(victim)) = (msg.param(0), msg.param(1)) else {
            return;
        };
        let cf = self.fold(chan);
        if self.is_me(victim) {
            self.channels.remove(&cf);
        } else {
            let vcf = self.fold(victim);
            if let Some(ch) = self.channels.get_mut(&cf) {
                ch.members.remove(&vcf);
            }
        }
    }

    fn on_quit(&mut self, msg: &Message) {
        let Some(nick) = msg.source_nick() else {
            return;
        };
        let ncf = self.fold(nick);
        for ch in self.channels.values_mut() {
            ch.members.remove(&ncf);
        }
    }

    fn on_nick(&mut self, msg: &Message) {
        let (Some(old), Some(new)) = (msg.source_nick(), msg.param(0)) else {
            return;
        };
        let (old, new) = (old.to_string(), new.to_string());
        if self.is_me(&old) {
            self.nick = new.clone();
            if let Some(p) = &self.own_prefix {
                let rest = p.find('!').map(|i| p[i..].to_string()).unwrap_or_default();
                self.own_prefix = Some(format!("{new}{rest}"));
            }
        }
        let ocf = self.fold(&old);
        let ncf = self.fold(&new);
        for ch in self.channels.values_mut() {
            if let Some(mut m) = ch.members.remove(&ocf) {
                m.nick = new.clone();
                ch.members.insert(ncf.clone(), m);
            }
        }
    }

    fn on_mode(&mut self, msg: &Message) {
        let Some(target) = msg.param(0) else { return };
        if !self.isupport.is_channel(target) {
            return;
        }
        let cf = self.fold(target);
        let Some(modes) = msg.param(1) else { return };
        let mut args = msg.params.iter().skip(2);
        let mut adding = true;
        let isupport = self.isupport.clone();
        let Some(ch) = self.channels.get_mut(&cf) else {
            return;
        };
        for m in modes.chars() {
            match m {
                '+' => adding = true,
                '-' => adding = false,
                _ => {
                    let arg = if isupport.mode_takes_param(m, adding) {
                        args.next()
                    } else {
                        None
                    };
                    if let (Some(sym), Some(nick)) = (isupport.symbol_for_mode(m), arg) {
                        let ncf = isupport.casemapping.fold(nick);
                        if let Some(member) = ch.members.get_mut(&ncf) {
                            let mut s: String =
                                member.symbols.chars().filter(|c| *c != sym).collect();
                            if adding {
                                s.push(sym);
                            }
                            member.symbols = isupport.sort_symbols(&s);
                        }
                    }
                }
            }
        }
    }

    fn on_names(&mut self, msg: &Message) {
        let (Some(chan), Some(names)) = (msg.param(2), msg.param(3)) else {
            return;
        };
        let cf = self.fold(chan);
        let isupport = self.isupport.clone();
        let Some(ch) = self.channels.get_mut(&cf) else {
            return;
        };
        if ch.names_done {
            ch.members.clear();
            ch.names_done = false;
        }
        for item in names.split_whitespace() {
            let symbols: String = item
                .chars()
                .take_while(|c| isupport.is_prefix_symbol(*c))
                .collect();
            let rest = &item[symbols.len()..];
            let nick = rest.split('!').next().unwrap_or(rest);
            if nick.is_empty() {
                continue;
            }
            ch.members.insert(
                isupport.casemapping.fold(nick),
                Member {
                    nick: nick.to_string(),
                    symbols: isupport.sort_symbols(&symbols),
                },
            );
        }
    }

    async fn on_upstream_message(&mut self, msg: Message) {
        let Some(raw_target) = msg.param(0) else {
            return;
        };
        let text = msg.param(1).unwrap_or("").to_string();
        let target = self.isupport.strip_statusmsg(raw_target).to_string();
        let sender_nick = msg.source_nick().unwrap_or("").to_string();
        let is_chan = self.isupport.is_channel(&target);
        let conversation = if is_chan {
            target.clone()
        } else {
            sender_nick.clone()
        };
        if msg.source_is_user() && self.is_ignored(msg.prefix.as_deref().unwrap_or("")) {
            return;
        }
        if msg.source_is_user() && !is_chan && msg.command == "NOTICE" {
            if self.crypt_keyx_notice(&sender_nick, &text) {
                return;
            }
            self.services_notice(&sender_nick, &text).await;
        }
        // Decrypted (crypt) and cleaned (stripcontrols) text.
        let mut text = text;
        if let Some(plain) = self.crypt_incoming(&conversation, &text) {
            text = plain;
        }
        if is_chan {
            if let Some(list) = &self.mods.stripcontrols {
                if list.is_empty() || list.iter().any(|c| self.fold(c) == self.fold(&target)) {
                    text = extra::strip_codes(&text);
                }
            }
        }
        let ctcp = ctcp_command(&text);
        if msg.source_is_user()
            && !is_chan
            && msg.command == "PRIVMSG"
            && ctcp.is_some_and(|c| c.eq_ignore_ascii_case("DCC"))
            && self.dcc_from_network(&msg, &sender_nick, &text).await
        {
            return;
        }
        if msg.source_is_user() && !is_chan {
            if msg.command == "NOTICE"
                && text.starts_with(crate::modules::AUTOOP_TAG)
                && self.autoop_on_notice(&sender_nick, &text)
            {
                return;
            }
            if msg.command == "PRIVMSG" {
                if let Some(c) = ctcp {
                    if self.ctcp_request(&sender_nick, c, &text) {
                        return;
                    }
                }
            }
        }
        let storable = msg.source_is_user()
            && !conversation.is_empty()
            && match ctcp {
                None => true,
                Some(c) => c.eq_ignore_ascii_case("ACTION") && msg.command == "PRIVMSG",
            };
        let ts = msg
            .tag("time")
            .and_then(parse_server_time)
            .unwrap_or_else(db::now_ms);

        let mut out = msg.clone();
        if out.params.len() >= 2 {
            out.params[1] = text.clone();
        }
        out.remove_tag("msgid");
        out.set_tag("time", Some(format_server_time(ts)));
        let mut id = None;
        let sender = msg.prefix.clone().unwrap_or_default();
        let (mut mention, private) = (false, !is_chan);
        if storable {
            self.away_count(is_chan, &conversation, &text);
            let nm = NewMessage {
                network_id: self.net.id,
                target_cf: self.fold(&conversation),
                target: conversation.clone(),
                ts,
                sender: sender.clone(),
                command: msg.command.clone(),
                text: text.clone(),
                outgoing: false,
                mention: is_chan && crate::modules::mentions(&text, &self.current_nick()),
            };
            mention = nm.mention;
            if let Some(i) = self.record(nm).await {
                out.set_tag("msgid", Some(format!("x{i}")));
                id = Some(i);
            }
        }
        let hidden = is_chan && self.is_hidden(&target);
        if is_chan && storable {
            self.flood_count(&target, id);
        }
        if hidden {
            self.detach_check(&target, &sender_nick, &text).await;
        } else if !(is_chan && self.is_hidden(&target)) {
            self.broadcast(&out, id);
        }
        if storable && ctcp.is_none() {
            self.watch_check(&conversation, &sender, &text).await;
            if mention || private {
                self.notify_highlight(&conversation, &sender, &text, private, ts)
                    .await;
            }
        }
        if storable {
            self.plugin_message(&msg.command, &conversation, &sender, &text, false)
                .await;
        }
    }

    // ------------------------------------------------------ client side

    async fn on_cmd(&mut self, cmd: NetCmd) {
        match cmd {
            NetCmd::Attach(h) => self.on_attach(h).await,
            NetCmd::Detach(id) => {
                if let Some(pos) = self.clients.iter().position(|c| c.h.id == id) {
                    let c = self.clients.remove(pos);
                    debug!(network = %self.net.name, client = %c.h.name, "client detached");
                    self.persist_cursor(c.h.name, c.last_id);
                    self.after_detach();
                }
            }
            NetCmd::UpdateCaps(id, caps) => {
                if let Some(c) = self.clients.iter_mut().find(|c| c.h.id == id) {
                    c.h.caps = caps;
                }
            }
            NetCmd::FromClient(id, msg) => self.on_client_message(id, msg).await,
            NetCmd::Reload => self.on_reload().await,
            NetCmd::Shutdown => {}
            NetCmd::Status(reply) => {
                let _ = reply.send(self.status_snapshot());
            }
            NetCmd::Control(action) => self.control(action),
            NetCmd::Join { channel, key } => self.admin_join(channel, key).await,
            NetCmd::PluginHttp { plugin, event } => {
                self.plugin_http_done(&plugin);
                self.plugin_event(Some(&plugin), event, None).await;
            }
            NetCmd::MarkRead { target, ts } => {
                self.mark_read(&target, ts).await;
            }
            NetCmd::Part { channel } => self.admin_part(channel).await,
            NetCmd::Broadcast(text) => {
                if !self.clients.is_empty() {
                    let text = self.t("broadcast_notice", &[&text]);
                    self.notice_all(&text);
                }
            }
            NetCmd::Alert {
                kind,
                subject,
                firing,
                ..
            } => {
                if !self.clients.is_empty() {
                    let what = self.t(&format!("alert_{kind}"), &[&subject]);
                    let key = if firing {
                        "alert_firing"
                    } else {
                        "alert_resolved"
                    };
                    let text = self.t(key, &[&what]);
                    self.notice_all(&text);
                }
            }
            NetCmd::DisconnectClient(id) => {
                if let Some(pos) = self.clients.iter().position(|c| c.h.id == id) {
                    let c = self.clients.remove(pos);
                    let _ =
                        c.h.tx
                            .try_send("ERROR :Disconnected by an administrator".into());
                    self.persist_cursor(c.h.name.clone(), c.last_id);
                    self.after_detach();
                }
            }
        }
    }

    fn status_snapshot(&self) -> NetStatus {
        let mut channels: Vec<ChannelStatus> = self
            .channels
            .values()
            .map(|c| ChannelStatus {
                name: c.name.clone(),
                topic: c.topic.clone(),
                members: c.members.len(),
                ops: c
                    .members
                    .values()
                    .filter(|m| m.symbols.contains(['@', '~', '&']))
                    .count(),
            })
            .collect();
        channels.sort_by_key(|c| c.name.to_lowercase());
        let mut caps: Vec<String> = self.caps_acked.iter().cloned().collect();
        caps.sort();
        NetStatus {
            state: match self.phase {
                Phase::Connected => "connected",
                Phase::Registering => "connecting",
                Phase::Disconnected if self.next_connect.is_some() => "reconnecting",
                Phase::Disconnected => "disconnected",
            },
            nick: self.current_nick(),
            server: format!("{}:{}", self.net.host, self.net.port),
            tls: self.net.tls,
            connected_since: self.connected_since,
            last_error: self.last_error.clone(),
            caps,
            network_name: self.isupport.network.clone(),
            channels,
            clients: self
                .clients
                .iter()
                .map(|c| ClientStatus {
                    id: c.h.id,
                    name: c.h.name.clone(),
                    peer: c.h.peer.clone(),
                    connected_at: c.connected_at,
                    caps: c.h.caps.names(),
                })
                .collect(),
            caps_offered: self.caps_ls.clone(),
            isupport: self.isupport.tokens.clone(),
            logged_in: self.rt.x.logged_in,
            host_hidden: self.rt.x.host_hidden,
        }
    }

    fn control(&mut self, action: NetAction) {
        match action {
            NetAction::Jump => {
                self.close_upstream(Some("Reconnecting"));
                self.backoff = BACKOFF_MIN;
                self.next_connect = Some(Instant::now());
            }
            NetAction::Disconnect => {
                self.close_upstream(Some("Disconnected by user"));
                self.next_connect = None;
                let text = self.t("disconnected_panel", &[]);
                self.notice_all(&text);
            }
            NetAction::Connect => {
                if self.phase == Phase::Disconnected {
                    self.backoff = BACKOFF_MIN;
                    self.next_connect = Some(Instant::now());
                }
            }
        }
    }

    async fn admin_join(&mut self, channel: String, key: Option<String>) {
        let cf = self.fold(&channel);
        let key_enc = key
            .as_deref()
            .filter(|k| !k.is_empty())
            .map(|k| self.ctx.secrets.encrypt(k));
        let nid = self.net.id;
        let name = channel.clone();
        let clear = key.is_some();
        if let Err(e) = self
            .ctx
            .db
            .call(move |c| db::upsert_channel(c, nid, &name, &cf, key_enc.as_deref(), clear))
            .await
        {
            warn!(error = %e, "cannot save channel");
        }
        if self.phase == Phase::Connected {
            match key.filter(|k| !k.is_empty()) {
                Some(k) => self.send(Message::new("JOIN", vec![channel, k])),
                None => self.send(Message::new("JOIN", vec![channel])),
            }
        }
    }

    async fn admin_part(&mut self, channel: String) {
        let cf = self.fold(&channel);
        let nid = self.net.id;
        if let Err(e) = self
            .ctx
            .db
            .call(move |c| db::delete_channel(c, nid, &cf))
            .await
        {
            warn!(error = %e, "cannot remove channel");
        }
        if self.phase == Phase::Connected {
            self.send(Message::new("PART", vec![channel, "Leaving".into()]));
        }
    }

    async fn on_reload(&mut self) {
        self.load_modules().await;
        let nid = self.net.id;
        let net = match self
            .ctx
            .db
            .call(move |c| db::get_network_by_id(c, nid))
            .await
        {
            Ok(Some(n)) => n,
            _ => return,
        };
        let was_enabled = self.net.enabled;
        self.net = net;
        if !self.net.enabled && was_enabled {
            self.close_upstream(Some("Network disabled"));
            self.next_connect = None;
            let text = self.t("network_disabled", &[]);
            self.notice_all(&text);
            return;
        }
        // Only a network that was just re-enabled connects here: one the user
        // disconnected on purpose stays disconnected (a reload also happens
        // when the language or the modules change).
        if self.net.enabled
            && !was_enabled
            && self.phase == Phase::Disconnected
            && self.next_connect.is_none()
        {
            self.next_connect = Some(Instant::now());
        }
        if self.phase == Phase::Connected && !self.joins_held() {
            self.join_saved_channels().await;
        }
    }

    fn welcome_lines(&self, caps: &ClientCaps) -> Vec<String> {
        let host = self.host();
        let nick = self.current_nick();
        let mut lines = vec![
            format!(":{host} 001 {nick} :{}", self.t("welcome_001", &[&nick])),
            format!(
                ":{host} 002 {nick} :{}",
                self.t("welcome_002", &[host, BRAND])
            ),
            format!(
                ":{host} 003 {nick} :{}",
                self.t("welcome_003", &[&self.net.name])
            ),
            format!(":{host} 004 {nick} {host} xbnc-{VERSION} iow biklmnopstv"),
        ];
        let mut tokens: Vec<String> = if self.isupport.tokens.is_empty() {
            vec![
                "CASEMAPPING=rfc1459".into(),
                "CHANTYPES=#&".into(),
                "PREFIX=(ov)@+".into(),
            ]
        } else {
            self.isupport.tokens.clone()
        };
        if caps.chathistory {
            tokens.push(format!(
                "CHATHISTORY={}",
                self.ctx.settings.get().history.chathistory_max
            ));
            tokens.push("MSGREFTYPES=timestamp,msgid".into());
        }
        if caps.bouncer_networks {
            tokens.push(format!("BOUNCER_NETID={}", self.net.id));
        }
        for chunk in tokens.chunks(12) {
            lines.push(format!(
                ":{host} 005 {nick} {} :are supported by this server",
                chunk.join(" ")
            ));
        }
        lines.extend(self.motd_lines());
        lines
    }

    fn channel_lines(&self, caps: &ClientCaps) -> Vec<String> {
        self.channel_lines_for(caps, None)
    }

    /// JOIN, topic and NAMES for every visible channel, or only for `only`
    /// (folded name).
    fn channel_lines_for(&self, caps: &ClientCaps, only: Option<&str>) -> Vec<String> {
        let host = self.host();
        let nick = self.current_nick();
        let me = self.self_prefix();
        let mut lines = Vec::new();
        let mut chans: Vec<&Chan> = self
            .channels
            .iter()
            .filter(|(cf, c)| match only {
                Some(o) => cf.as_str() == o,
                None => !self.is_hidden(&c.name),
            })
            .map(|(_, c)| c)
            .collect();
        chans.sort_by(|a, b| a.name.cmp(&b.name));
        for ch in chans {
            if caps.extended_join {
                lines.push(format!(":{me} JOIN {} * :{}", ch.name, self.net.realname));
            } else {
                lines.push(format!(":{me} JOIN {}", ch.name));
            }
            if let Some(t) = &ch.topic {
                lines.push(format!(":{host} 332 {nick} {} :{t}", ch.name));
            }
            let names: Vec<String> = ch
                .members
                .values()
                .map(|m| {
                    let syms = if caps.multi_prefix {
                        m.symbols.clone()
                    } else {
                        m.symbols
                            .chars()
                            .next()
                            .map(String::from)
                            .unwrap_or_default()
                    };
                    format!("{syms}{}", m.nick)
                })
                .collect();
            for chunk in xbnc_proto::chunk_words(names.iter().map(String::as_str), 400) {
                lines.push(format!(":{host} 353 {nick} = {} :{chunk}", ch.name));
            }
            lines.push(format!(
                ":{host} 366 {nick} {} :End of /NAMES list.",
                ch.name
            ));
        }
        lines
    }

    /// Converts a stored message to a line for a client.
    fn stored_to_message(&self, m: &StoredMessage, caps: &ClientCaps, playback: bool) -> Message {
        if db::is_event(&m.command) {
            let mut msg = if !playback && caps.event_playback {
                self.event_message(m)
            } else {
                let mut text = self.event_text(self.lang, &m.command, &m.sender, &m.text);
                if playback && !caps.server_time {
                    text = format!("[{}] {text}", format_short_time(m.ts));
                }
                Message::new("PRIVMSG", vec![m.target.clone(), text])
                    .with_prefix(format!("*events!xbnc@{}", self.host()))
            };
            msg.set_tag("time", Some(format_server_time(m.ts)));
            msg.set_tag("msgid", Some(format!("x{}", m.id)));
            return msg;
        }
        let target = if m.outgoing || self.isupport.is_channel(&m.target) {
            m.target.clone()
        } else {
            self.current_nick()
        };
        let mut text = m.text.clone();
        if playback && !caps.server_time {
            let stamp = format!("[{}]", format_short_time(m.ts));
            text = match text.strip_prefix("\x01ACTION ") {
                Some(rest) => format!("\x01ACTION {stamp} {rest}"),
                None => format!("{stamp} {text}"),
            };
        }
        let mut msg =
            Message::new(m.command.clone(), vec![target, text]).with_prefix(m.sender.clone());
        msg.set_tag("time", Some(format_server_time(m.ts)));
        msg.set_tag("msgid", Some(format!("x{}", m.id)));
        msg
    }

    async fn on_attach(&mut self, h: ClientHandle) {
        info!(user = %self.owner, network = %self.net.name, client = %h.name, "client attached");
        let caps = h.caps;
        let mut lines = self.welcome_lines(&caps);
        lines.extend(self.channel_lines(&caps));
        self.ensure_markers().await;
        if caps.read_marker {
            lines.extend(self.marker_lines());
        }

        let nid = self.net.id;
        let name = h.name.clone();
        let hist = self.ctx.settings.get().history.clone();
        let chathistory = caps.chathistory;
        let res = self
            .ctx
            .db
            .call(move |c| {
                let max = db::max_message_id(c, nid)?;
                if chathistory {
                    return Ok((Vec::new(), max, false, false));
                }
                let (msgs, missed) = match db::get_cursor(c, nid, &name)? {
                    Some(cur) => (
                        db::messages_after_id(c, nid, cur, hist.playback_max + 1)?,
                        true,
                    ),
                    None => (
                        db::latest_messages(c, nid, hist.new_client_playback)?,
                        false,
                    ),
                };
                Ok((msgs, max, true, missed))
            })
            .await;
        let (mut msgs, max_id, did_playback, missed) = match res {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "cannot load playback");
                (Vec::new(), 0, false, false)
            }
        };
        // Missed messages already read on another device are not played
        // back again (a new device still gets the recent context).
        if missed {
            msgs.retain(|m| self.marker_of(&m.target).is_none_or(|r| m.ts > r));
        }
        msgs.retain(|m| !self.is_hidden(&m.target));
        let truncated = msgs.len() > self.ctx.settings.get().history.playback_max;
        if truncated {
            let skip = msgs.len() - self.ctx.settings.get().history.playback_max;
            msgs.drain(..skip);
        }
        for m in &msgs {
            lines.push(render_for(&self.stored_to_message(m, &caps, true), &caps));
        }
        if truncated {
            lines.push(self.status_line("NOTICE", &self.t("playback_truncated", &[])));
        } else if did_playback && !msgs.is_empty() {
            debug!(count = msgs.len(), "played back messages");
        }
        if self.phase != Phase::Connected {
            let why = self
                .last_error
                .clone()
                .unwrap_or_else(|| self.t("connecting", &[]));
            lines.push(self.status_line(
                "NOTICE",
                &self.t("not_connected_yet", &[&self.net.name, &why]),
            ));
        }
        for l in lines {
            if h.tx.try_send(l).is_err() {
                warn!(client = %h.name, "client queue overflow during attach");
                return;
            }
        }
        let id = h.id;
        self.clients.push(Client {
            h,
            last_id: max_id,
            connected_at: db::now_ms(),
        });
        self.away_on_attach(id);
        for line in self.ctx.server_motd() {
            let l = self.status_line("NOTICE", &line);
            self.send_to(id, l);
        }
        self.touch_last_seen();
    }

    fn touch_last_seen(&self) {
        let db = self.ctx.db.clone();
        let uid = self.net.user_id;
        tokio::spawn(async move {
            let _ = db.call(move |c| db::touch_last_seen(c, uid)).await;
        });
    }

    /// Bookkeeping after a client session goes away.
    fn after_detach(&mut self) {
        self.away_on_detach();
        self.touch_last_seen();
    }

    async fn on_client_message(&mut self, id: ClientId, msg: Message) {
        let Some(caps) = self.clients.iter().find(|c| c.h.id == id).map(|c| c.h.caps) else {
            return;
        };
        if self.try_alias(id, &msg) {
            return;
        }
        match msg.command.as_str() {
            "MARKREAD" => {
                self.client_markread(id, &msg).await;
            }
            "PRIVMSG" | "NOTICE" => {
                let (Some(target), Some(text)) = (msg.param(0), msg.param(1)) else {
                    return;
                };
                let (target, text) = (target.to_string(), text.to_string());
                if target.eq_ignore_ascii_case("*status") {
                    if msg.command == "PRIVMSG" {
                        self.status_command(id, &text).await;
                    }
                    return;
                }
                if target.eq_ignore_ascii_case("*crypt") || target.eq_ignore_ascii_case("*ignore") {
                    if msg.command == "PRIVMSG" {
                        if target.eq_ignore_ascii_case("*crypt") {
                            self.crypt_command(id, &text).await;
                        } else {
                            self.ignore_command(id, &text).await;
                        }
                    }
                    return;
                }
                if target.starts_with('*')
                    && msg.command == "PRIVMSG"
                    && self.plugin_command(id, &target, &text).await
                {
                    return;
                }
                if target.eq_ignore_ascii_case("*notes") || target.eq_ignore_ascii_case("*watch") {
                    if msg.command == "PRIVMSG" {
                        self.service_command(id, &target.to_ascii_lowercase(), &text)
                            .await;
                    }
                    return;
                }
                if self.phase != Phase::Connected {
                    let line = self.status_line("NOTICE", &self.t("not_connected_msg", &[]));
                    self.send_to(id, line);
                    return;
                }
                if text.is_empty() {
                    return;
                }
                let text = if msg.command == "PRIVMSG"
                    && ctcp_command(&text).is_some_and(|c| c.eq_ignore_ascii_case("DCC"))
                {
                    match self.dcc_from_client(id, &target, &text).await {
                        Some(t) => t,
                        None => return,
                    }
                } else {
                    text
                };
                // `+p text` sends one message in plain text on an encrypted
                // conversation (as FiSH does).
                let (text, plain) = match text.strip_prefix("+p ") {
                    Some(t) if self.mods.crypt.is_some() => (t.to_string(), true),
                    _ => (text, false),
                };
                match self.crypt_outgoing(&target, &text).filter(|_| !plain) {
                    Some(parts) => {
                        for p in parts {
                            self.send(Message::new(msg.command.clone(), vec![target.clone(), p]));
                        }
                    }
                    None => self.send(Message::new(
                        msg.command.clone(),
                        vec![target.clone(), text.clone()],
                    )),
                }
                let ctcp = ctcp_command(&text);
                let storable = match ctcp {
                    None => true,
                    Some(c) => c.eq_ignore_ascii_case("ACTION") && msg.command == "PRIVMSG",
                };
                let mut echo =
                    Message::new(msg.command.clone(), vec![target.clone(), text.clone()])
                        .with_prefix(self.self_prefix());
                let ts = db::now_ms();
                echo.set_tag("time", Some(format_server_time(ts)));
                let mut new_id = None;
                if storable {
                    let conv = self.isupport.strip_statusmsg(&target).to_string();
                    let nm = NewMessage {
                        network_id: self.net.id,
                        target_cf: self.fold(&conv),
                        target: conv,
                        ts,
                        sender: self.self_prefix(),
                        command: msg.command.clone(),
                        text: text.clone(),
                        outgoing: true,
                        mention: false,
                    };
                    if let Some(i) = self.record(nm).await {
                        echo.set_tag("msgid", Some(format!("x{i}")));
                        new_id = Some(i);
                    }
                }
                // Other clients of this user see what was sent from this one.
                self.broadcast_except(&echo, new_id, Some(id));
                if let Some(c) = self.clients.iter_mut().find(|c| c.h.id == id) {
                    if let Some(i) = new_id {
                        c.last_id = c.last_id.max(i);
                    }
                }
                if caps.echo_message {
                    self.send_to(id, render_for(&echo, &caps));
                }
                if storable {
                    let conv = self.isupport.strip_statusmsg(&target).to_string();
                    let me = self.self_prefix();
                    // Writing in a conversation means it has been read.
                    if msg.command == "PRIVMSG" {
                        self.mark_read(&conv, db::now_ms()).await;
                    }
                    self.plugin_message(&msg.command, &conv, &me, &text, true)
                        .await;
                }
            }
            "JOIN" => {
                let Some(chans) = msg.param(0).map(str::to_string) else {
                    return;
                };
                let keys: Vec<String> = msg
                    .param(1)
                    .map(|k| k.split(',').map(str::to_string).collect())
                    .unwrap_or_default();
                let mut shown = 0;
                let wanted: Vec<String> = chans.split(',').map(str::to_string).collect();
                for c in &wanted {
                    if self.detach_on_client_join(c).await {
                        shown += 1;
                    }
                }
                if self.phase == Phase::Connected && shown < wanted.len() {
                    if self.joins_held() && chans != "0" {
                        let text = self.t("join_held", &[&chans]);
                        let line = self.status_line("NOTICE", &text);
                        self.send_to(id, line);
                    }
                    self.send(Message::new("JOIN", msg.params.clone()));
                }
                if chans == "0" {
                    return;
                }
                let nid = self.net.id;
                let items: Vec<(String, String, Option<String>)> = chans
                    .split(',')
                    .enumerate()
                    .filter(|(_, c)| self.isupport.is_channel(c))
                    .map(|(i, c)| {
                        let key = keys
                            .get(i)
                            .filter(|k| !k.is_empty() && k.as_str() != "x")
                            .map(|k| self.ctx.secrets.encrypt(k));
                        (c.to_string(), self.fold(c), key)
                    })
                    .collect();
                if let Err(e) = self
                    .ctx
                    .db
                    .call(move |c| {
                        for (name, cf, key) in &items {
                            db::upsert_channel(c, nid, name, cf, key.as_deref(), false)?;
                        }
                        Ok(())
                    })
                    .await
                {
                    warn!(error = %e, "cannot save channels");
                }
            }
            "PART" => {
                let Some(requested) = msg.param(0).map(str::to_string) else {
                    return;
                };
                let (sticky, chans): (Vec<&str>, Vec<&str>) =
                    requested.split(',').partition(|c| self.is_sticky(c));
                for c in sticky {
                    let line = self.status_line("NOTICE", &self.t("sticky_blocked", &[c]));
                    self.send_to(id, line);
                }
                if chans.is_empty() {
                    return;
                }
                let chans = chans.join(",");
                if self.phase == Phase::Connected {
                    let mut params = vec![chans.clone()];
                    params.extend(msg.params.iter().skip(1).cloned());
                    self.send(Message::new("PART", params));
                }
                let nid = self.net.id;
                let cfs: Vec<String> = chans.split(',').map(|c| self.fold(c)).collect();
                if let Err(e) = self
                    .ctx
                    .db
                    .call(move |c| {
                        for cf in &cfs {
                            db::delete_channel(c, nid, cf)?;
                        }
                        Ok(())
                    })
                    .await
                {
                    warn!(error = %e, "cannot remove channels");
                }
            }
            "CHATHISTORY" => self.chathistory(id, &msg).await,
            "DETACH" | "ATTACH" => {
                let chan = msg.param(0).unwrap_or("").to_string();
                let attach = msg.command == "ATTACH";
                self.detach_command(id, attach, &chan).await;
            }
            "PING" | "PONG" | "CAP" | "AUTHENTICATE" | "PASS" | "USER" | "QUIT" => {}
            "MOTD" if msg.params.is_empty() => {
                // Answer from the stored MOTD (or the block_motd notice).
                for l in self.motd_lines() {
                    self.send_to(id, l);
                }
            }
            other => {
                match other {
                    "NICK" => {
                        if let Some(n) = msg.param(0) {
                            // Choosing another nick by hand pauses keepnick.
                            self.rt.keepnick_paused =
                                !self.isupport.casemapping.eq(n, &self.net.nick);
                        }
                    }
                    "AWAY" => {
                        self.rt.away_client = msg.param(0).is_some_and(|t| !t.is_empty());
                        self.rt.away_auto = false;
                    }
                    _ => {}
                }
                if self.phase == Phase::Connected {
                    self.route_track(id, &msg);
                    let mut m = msg.clone();
                    m.tags.clear();
                    m.prefix = None;
                    self.send(m);
                } else {
                    let line = self.status_line("NOTICE", &self.t("not_connected", &[]));
                    self.send_to(id, line);
                }
            }
        }
    }

    // ------------------------------------------------------- CHATHISTORY

    fn fail(&mut self, id: ClientId, code: &str, sub: &str, desc: &str) {
        let line = format!(":{} FAIL CHATHISTORY {code} {sub} :{desc}", self.host());
        self.send_to(id, line);
    }

    fn bound_fut(
        &self,
        s: &str,
    ) -> impl std::future::Future<Output = Result<Option<Bound>, &'static str>> + Send + 'static
    {
        parse_bound(self.ctx.db.clone(), self.net.id, s.to_string())
    }
}

async fn parse_bound(
    db: crate::db::Db,
    nid: i64,
    s: String,
) -> Result<Option<Bound>, &'static str> {
    {
        let s = s.as_str();
        if s == "*" {
            return Ok(None);
        }
        if let Some(t) = s.strip_prefix("timestamp=") {
            return parse_server_time(t)
                .map(|ms| Some(Bound::Timestamp(ms)))
                .ok_or("INVALID_PARAMS");
        }
        if let Some(m) = s.strip_prefix("msgid=") {
            let Some(mid) = parse_msgid(m) else {
                return Err("INVALID_MSGREFID");
            };
            return match db.call(move |c| db::message_exists(c, nid, mid)).await {
                Ok(true) => Ok(Some(Bound::MsgId(mid))),
                _ => Err("INVALID_MSGREFID"),
            };
        }
        Err("INVALID_PARAMS")
    }
}

impl NetActor {
    async fn chathistory(&mut self, id: ClientId, msg: &Message) {
        let Some(caps) = self.clients.iter().find(|c| c.h.id == id).map(|c| c.h.caps) else {
            return;
        };
        let sub = msg.param(0).unwrap_or("").to_ascii_uppercase();
        if !caps.chathistory {
            self.fail(
                id,
                "INVALID_PARAMS",
                &sub,
                "draft/chathistory was not negotiated",
            );
            return;
        }
        let max = self.ctx.settings.get().history.chathistory_max;
        let parse_limit = |s: Option<&str>| -> Option<usize> {
            s.and_then(|v| v.parse::<usize>().ok())
                .filter(|n| *n > 0)
                .map(|n| n.min(max))
        };
        let nid = self.net.id;
        self.batch_seq += 1;
        let bid = format!("ch{}", self.batch_seq);
        let host = self.host().to_string();

        if sub == "TARGETS" {
            let (Some(a), Some(b), Some(limit)) =
                (msg.param(1), msg.param(2), parse_limit(msg.param(3)))
            else {
                self.fail(
                    id,
                    "INVALID_PARAMS",
                    &sub,
                    "Usage: TARGETS <timestamp> <timestamp> <limit>",
                );
                return;
            };
            let (Ok(Some(Bound::Timestamp(ta))), Ok(Some(Bound::Timestamp(tb)))) =
                (self.bound_fut(a).await, self.bound_fut(b).await)
            else {
                self.fail(id, "INVALID_PARAMS", &sub, "TARGETS requires timestamps");
                return;
            };
            let rows = self
                .ctx
                .db
                .call(move |c| db::history_targets(c, nid, ta, tb, limit))
                .await
                .unwrap_or_default();
            let mut lines = vec![format!(":{host} BATCH +{bid} draft/chathistory-targets")];
            for (t, ts) in rows {
                let mut m = Message::new(
                    "CHATHISTORY",
                    vec![
                        "TARGETS".into(),
                        t,
                        format!("timestamp={}", format_server_time(ts)),
                    ],
                )
                .with_prefix(host.clone());
                m.set_tag("batch", Some(bid.clone()));
                lines.push(render_for(&m, &caps));
            }
            lines.push(format!(":{host} BATCH -{bid}"));
            for l in lines {
                self.send_to(id, l);
            }
            return;
        }

        let Some(target) = msg.param(1).map(str::to_string) else {
            self.fail(id, "INVALID_PARAMS", &sub, "Missing target");
            return;
        };
        let target_cf = self.fold(&target);
        let (after, before, limit, newest) = match sub.as_str() {
            "LATEST" | "BEFORE" | "AFTER" | "AROUND" => {
                let Some(limit) = parse_limit(msg.param(3)) else {
                    self.fail(id, "INVALID_PARAMS", &sub, "Invalid limit");
                    return;
                };
                let bound = match self.bound_fut(msg.param(2).unwrap_or("")).await {
                    Ok(b) => b,
                    Err(code) => {
                        self.fail(id, code, &sub, "Invalid message reference");
                        return;
                    }
                };
                match sub.as_str() {
                    "LATEST" => (bound, None, limit, true),
                    "BEFORE" | "AFTER" | "AROUND" if bound.is_none() => {
                        self.fail(
                            id,
                            "INVALID_PARAMS",
                            &sub,
                            "A message reference is required",
                        );
                        return;
                    }
                    "BEFORE" => (None, bound, limit, true),
                    "AFTER" => (bound, None, limit, false),
                    _ => {
                        // AROUND: half before, half after the reference.
                        let half = (limit / 2).max(1);
                        let b = bound;
                        let ev = caps.event_playback;
                        let tc = target_cf.clone();
                        let res = self
                            .ctx
                            .db
                            .call(move |c| {
                                let q = |after, before, limit, newest| db::HistQuery {
                                    target_cf: tc.clone(),
                                    after,
                                    before,
                                    limit,
                                    newest_first_window: newest,
                                    events: ev,
                                };
                                let mut v = db::history_query(
                                    c,
                                    nid,
                                    &q(None, b, limit.saturating_sub(half), true),
                                )?;
                                v.extend(db::history_query(c, nid, &q(b, None, half, false))?);
                                Ok(v)
                            })
                            .await
                            .unwrap_or_default();
                        self.send_history_batch(id, &caps, &bid, &target, res);
                        return;
                    }
                }
            }
            "BETWEEN" => {
                let Some(limit) = parse_limit(msg.param(4)) else {
                    self.fail(id, "INVALID_PARAMS", &sub, "Invalid limit");
                    return;
                };
                let a = self.bound_fut(msg.param(2).unwrap_or("")).await;
                let b = self.bound_fut(msg.param(3).unwrap_or("")).await;
                let (Ok(Some(a)), Ok(Some(b))) = (a, b) else {
                    self.fail(
                        id,
                        "INVALID_PARAMS",
                        &sub,
                        "Two message references are required",
                    );
                    return;
                };
                let forward = match (a, b) {
                    (Bound::Timestamp(x), Bound::Timestamp(y))
                    | (Bound::MsgId(x), Bound::MsgId(y)) => x <= y,
                    _ => true,
                };
                if forward {
                    (Some(a), Some(b), limit, false)
                } else {
                    (Some(b), Some(a), limit, true)
                }
            }
            _ => {
                self.fail(id, "INVALID_PARAMS", &sub, "Unknown subcommand");
                return;
            }
        };
        let tc = target_cf.clone();
        let ev = caps.event_playback;
        let res = self
            .ctx
            .db
            .call(move |c| {
                db::history_query(
                    c,
                    nid,
                    &db::HistQuery {
                        target_cf: tc,
                        after,
                        before,
                        limit,
                        newest_first_window: newest,
                        events: ev,
                    },
                )
            })
            .await;
        match res {
            Ok(rows) => self.send_history_batch(id, &caps, &bid, &target, rows),
            Err(e) => {
                warn!(error = %e, "history query failed");
                self.fail(
                    id,
                    "MESSAGE_ERROR",
                    &sub,
                    "History is temporarily unavailable",
                );
            }
        }
    }

    fn send_history_batch(
        &mut self,
        id: ClientId,
        caps: &ClientCaps,
        bid: &str,
        target: &str,
        rows: Vec<StoredMessage>,
    ) {
        let host = self.host().to_string();
        let mut lines = Vec::with_capacity(rows.len() + 2);
        if caps.batch {
            lines.push(format!(":{host} BATCH +{bid} chathistory {target}"));
        }
        for m in &rows {
            let mut msg = self.stored_to_message(m, caps, false);
            if caps.batch {
                msg.set_tag("batch", Some(bid.to_string()));
            }
            lines.push(render_for(&msg, caps));
        }
        if caps.batch {
            lines.push(format!(":{host} BATCH -{bid}"));
        }
        for l in lines {
            self.send_to(id, l);
        }
    }
}

/// Recognises "reconnecting too fast" messages from common IRC daemons
/// (ircu/Undernet, InspIRCd, UnrealIRCd, Solanum/charybdis, Hybrid).
fn is_throttle_error(reason: &str) -> bool {
    let r = reason.to_ascii_lowercase();
    r.contains("throttl")
        || r.contains("too fast")
        || r.contains("reconnecting too")
        || r.contains("connect again later")
}

/// Whether a line sets the user mode +x on ourselves (`MODE <nick> +x`,
/// also `+ix`, `-w+x`…): the hidden host of Undernet and others.
/// `PRIVMSG AuthServ :AUTH account password` becomes
/// `PRIVMSG NickServ :IDENTIFY account password` (EvilNet only).
fn evilnet_login_to_nickserv(m: &mut Message) {
    if !m.command.eq_ignore_ascii_case("PRIVMSG") || m.params.len() != 2 {
        return;
    }
    if !m.params[0].eq_ignore_ascii_case("AuthServ") {
        return;
    }
    let text = m.params[1].clone();
    let mut words = text.splitn(2, ' ');
    if !words.next().is_some_and(|w| w.eq_ignore_ascii_case("AUTH")) {
        return;
    }
    let Some(rest) = words.next().filter(|r| !r.trim().is_empty()) else {
        return;
    };
    m.params[0] = "NickServ".to_string();
    m.params[1] = format!("IDENTIFY {rest}");
}

fn sets_own_hidden_host(m: &Message, nick: &str) -> bool {
    if !m.command.eq_ignore_ascii_case("MODE") {
        return false;
    }
    let (Some(target), Some(modes)) = (m.param(0), m.param(1)) else {
        return false;
    };
    if !target.eq_ignore_ascii_case(nick) {
        return false;
    }
    let mut adding = true;
    for c in modes.chars() {
        match c {
            '+' => adding = true,
            '-' => adding = false,
            'x' if adding => return true,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_texts_carry_the_site() {
        let v = env!("CARGO_PKG_VERSION");
        assert_eq!(BRAND_SITE, format!("xBNC {v} Made by y2k - https://xbnc.org"));
        assert_eq!(ctcp_version_reply(""), BRAND_SITE);
        assert_eq!(ctcp_version_reply("  "), BRAND_SITE);
        assert_eq!(ctcp_version_reply(CTCP_VERSION_DEFAULT), BRAND_SITE);
        assert_eq!(ctcp_version_reply("mIRC {version}"), format!("mIRC {v}"));
        assert_eq!(ctcp_version_reply("My client"), "My client");
    }

    /// Regression: EvilNet networks created from the old preset logged in
    /// with AuthServ; EvilNet answers to NickServ.
    #[test]
    fn evilnet_authserv_login_goes_to_nickserv() {
        let mut m = Message::parse("PRIVMSG AuthServ :AUTH y2k s3cret pass").unwrap();
        evilnet_login_to_nickserv(&mut m);
        assert_eq!(m.params, vec!["NickServ", "IDENTIFY y2k s3cret pass"]);
        for line in ["PRIVMSG AuthServ :HELP", "PRIVMSG #xbnc :AUTH a b", "PRIVMSG AuthServ :AUTH", "JOIN #xbnc"] {
            let mut m = Message::parse(line).unwrap();
            let before = m.clone();
            evilnet_login_to_nickserv(&mut m);
            assert_eq!(m, before, "{line}");
        }
    }

    #[test]
    fn render_strips_unnegotiated_tags() {
        let mut m = Message::cmd("PRIVMSG", &["#a", "hi"]).with_prefix("n!u@h");
        m.set_tag("time", Some("2024-01-01T00:00:00.000Z".into()));
        m.set_tag("msgid", Some("x1".into()));
        m.set_tag("batch", Some("b1".into()));
        let none = ClientCaps::default();
        assert_eq!(render_for(&m, &none), ":n!u@h PRIVMSG #a hi");
        let st = ClientCaps {
            server_time: true,
            ..Default::default()
        };
        assert_eq!(
            render_for(&m, &st),
            "@time=2024-01-01T00:00:00.000Z :n!u@h PRIVMSG #a hi"
        );
        let all = ClientCaps {
            server_time: true,
            message_tags: true,
            batch: true,
            ..Default::default()
        };
        assert!(render_for(&m, &all).contains("msgid=x1"));
        assert!(render_for(&m, &all).contains("batch=b1"));
    }

    #[test]
    fn msgid_parsing() {
        assert_eq!(parse_msgid("x42"), Some(42));
        assert_eq!(parse_msgid("42"), None);
        assert_eq!(parse_msgid("xabc"), None);
    }

    #[test]
    fn jitter_in_range() {
        assert!(is_throttle_error(
            "server error: Your host is trying to (re)connect too fast -- throttled"
        ));
        assert!(is_throttle_error(
            "server error: Trying to reconnect too fast."
        ));
        assert!(!is_throttle_error(
            "server error: Closing Link: (Ping timeout)"
        ));
        let d = jitter(Duration::from_secs(10));
        assert!(d >= Duration::from_secs(8) && d < Duration::from_secs(12));
    }
}
