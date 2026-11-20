//! Client (downstream) connections: registration, capability negotiation,
//! authentication and relaying to the network actor.

use crate::auth::{self, normalise_client_name, parse_login, split_pass, AuthResult, LoginSpec};
use crate::crypto::Secret;
use crate::db::User;
use crate::network::{ClientCaps, ClientHandle, NetCmd, NetHandle};
use crate::server::Ctx;
use crate::util::{BoxStream, IrcCodec};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{ReadHalf, WriteHalf};
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, FramedWrite};
use tracing::{debug, info, warn};
use xbnc_proto::Message;

type Reader = FramedRead<ReadHalf<BoxStream>, IrcCodec>;

/// Records a login attempt in the security log and the metrics.
fn log_login(ctx: &Arc<Ctx>, ip: IpAddr, username: &str, success: bool, reason: &'static str) {
    if success {
        crate::metrics::Metrics::add(&ctx.metrics.client_logins, 1);
    } else {
        crate::metrics::Metrics::add(&ctx.metrics.failed_logins, 1);
    }
    let db = ctx.db.clone();
    let name = username.to_string();
    tokio::spawn(async move {
        let _ = db
            .call(move |c| {
                crate::db::record_login(c, &ip.to_string(), &name, "irc", success, reason)
            })
            .await;
    });
}

async fn client_writer(ctx: Arc<Ctx>, w: WriteHalf<BoxStream>, mut rx: mpsc::Receiver<String>) {
    let mut sink = FramedWrite::new(w, IrcCodec::default());
    while let Some(line) = rx.recv().await {
        crate::metrics::Metrics::add(&ctx.metrics.bytes_to_clients, line.len() as u64 + 2);
        if sink.feed(line).await.is_err() {
            return;
        }
        // Coalesce whatever is already queued into one write.
        while let Ok(more) = rx.try_recv() {
            crate::metrics::Metrics::add(&ctx.metrics.bytes_to_clients, more.len() as u64 + 2);
            if sink.feed(more).await.is_err() {
                return;
            }
        }
        if sink.flush().await.is_err() {
            return;
        }
    }
    let _ = sink.close().await;
}

/// Result of capability negotiation with one client.
#[derive(Debug, Default, Clone, Copy)]
struct CapState {
    caps: ClientCaps,
    /// `sasl` requested (only meaningful before registration).
    sasl: bool,
    /// Between CAP LS/REQ and CAP END: registration is suspended.
    negotiating: bool,
    version: u32,
}

#[derive(Default)]
struct Reg {
    nick: Option<String>,
    user: Option<String>,
    pass: Option<Secret>,
    cap: CapState,
    sasl_started: bool,
    sasl_buf: String,
    sasl_ok: Option<(User, LoginSpec)>,
    /// `BOUNCER BIND <netid>` sent before registration.
    bind: Option<i64>,
}

struct Session {
    user: User,
    /// None: a soju.im/bouncer-networks control connection.
    net: Option<NetHandle>,
    client_name: String,
    caps: ClientCaps,
}

fn cap_list(version: u32) -> String {
    let mut v: Vec<String> = crate::network::CLIENT_CAPS
        .iter()
        .map(|c| c.to_string())
        .collect();
    v.push(if version >= 302 { "sasl=PLAIN" } else { "sasl" }.to_string());
    v.join(" ")
}

fn enabled_caps(caps: &ClientCaps, sasl: bool) -> Vec<&'static str> {
    let mut v = caps.names();
    if sasl {
        v.push("sasl");
    }
    v
}

/// Handles a CAP command. Returns true if the client's caps changed.
fn handle_cap(
    host: &str,
    nick: &str,
    msg: &Message,
    st: &mut CapState,
    registered: bool,
    tx: &mpsc::Sender<String>,
) -> bool {
    let CapState {
        caps,
        sasl: sasl_cap,
        negotiating,
        version: cap_version,
    } = st;
    let sub = msg.param(0).unwrap_or("").to_ascii_uppercase();
    let send = |l: String| {
        let _ = tx.try_send(l);
    };
    match sub.as_str() {
        "LS" => {
            if !registered {
                *negotiating = true;
            }
            *cap_version = msg
                .param(1)
                .and_then(|v| v.parse().ok())
                .unwrap_or(0)
                .max(*cap_version);
            send(format!(":{host} CAP {nick} LS :{}", cap_list(*cap_version)));
            false
        }
        "LIST" => {
            send(format!(
                ":{host} CAP {nick} LIST :{}",
                enabled_caps(caps, *sasl_cap).join(" ")
            ));
            false
        }
        "REQ" => {
            if !registered {
                *negotiating = true;
            }
            let req = msg.param(1).unwrap_or("").to_string();
            let mut new_caps = *caps;
            let mut new_sasl = *sasl_cap;
            let mut ok = true;
            for item in req.split_whitespace() {
                let (remove, name) = match item.strip_prefix('-') {
                    Some(n) => (true, n),
                    None => (false, item),
                };
                let on = !remove;
                match name {
                    "sasl" if !registered => new_sasl = on,
                    // cap-notify cannot be turned off once negotiated (302).
                    "cap-notify" if remove => ok = false,
                    n => ok &= new_caps.set(n, on),
                }
            }
            if ok {
                let changed = new_caps != *caps;
                *caps = new_caps;
                *sasl_cap = new_sasl;
                send(format!(":{host} CAP {nick} ACK :{req}"));
                changed
            } else {
                send(format!(":{host} CAP {nick} NAK :{req}"));
                false
            }
        }
        "END" => {
            *negotiating = false;
            false
        }
        other => {
            send(format!(":{host} 410 {nick} {other} :Invalid CAP command"));
            false
        }
    }
}

/// Serves one client connection until it disconnects.
pub async fn serve(
    ctx: Arc<Ctx>,
    stream: BoxStream,
    peer: SocketAddr,
    local: Option<SocketAddr>,
    pre_auth: crate::server::PreAuth,
) {
    let ip = peer.ip();
    let (r, w) = tokio::io::split(stream);
    let mut reader: Reader = FramedRead::new(r, IrcCodec::default());
    let (tx, rx) = mpsc::channel::<String>(ctx.settings.get().limits.client_queue);
    let mut writer = tokio::spawn(client_writer(ctx.clone(), w, rx));
    let host = ctx.config.hostname.clone();

    if let Some(left) = ctx.limiter.blocked_for(ip) {
        let lang = ctx.settings.get().lang_for("");
        let mins = left.as_secs().div_ceil(60).max(1).to_string();
        let text = crate::i18n::tr(lang, "login.blocked", &[&ip.to_string(), &mins]);
        let _ = tx.try_send(format!(":{host} NOTICE * :{text}"));
        let _ = tx.try_send(format!("ERROR :{text}"));
        drop(tx);
        let _ = tokio::time::timeout(Duration::from_secs(5), writer).await;
        return;
    }

    let timeout = Duration::from_secs(ctx.settings.get().limits.registration_timeout_seconds);
    let session = match tokio::time::timeout(timeout, register(&ctx, &mut reader, &tx, ip)).await {
        Ok(Some(s)) => {
            drop(pre_auth);
            s
        }
        Ok(None) => {
            drop(tx);
            let _ = tokio::time::timeout(Duration::from_secs(5), writer).await;
            return;
        }
        Err(_) => {
            let _ = tx.try_send("ERROR :Registration timed out".into());
            drop(tx);
            let _ = tokio::time::timeout(Duration::from_secs(5), writer).await;
            return;
        }
    };

    let Session {
        user,
        net,
        client_name,
        caps,
    } = session;
    if !ctx.registry.acquire_client(
        user.id,
        user.max_clients
            .map(|m| m.max(0) as usize)
            .unwrap_or(ctx.settings.get().limits.max_clients_per_user),
    ) {
        let _ = tx.try_send("ERROR :Too many simultaneous clients for this account".into());
        drop(tx);
        let _ = tokio::time::timeout(Duration::from_secs(5), writer).await;
        return;
    }
    let Some(net) = net else {
        info!(user = %user.username, client = %client_name, %peer, "control client logged in");
        control_session(&ctx, &user, &mut reader, tx, &mut writer, caps).await;
        ctx.registry.release_client(user.id);
        return;
    };
    let id = ctx.next_client_id.fetch_add(1, Ordering::Relaxed);
    info!(user = %user.username, network = %net.name, client = %client_name, %peer, "client logged in");
    let own = tx.downgrade();
    let handle = ClientHandle {
        peer: peer.to_string(),
        peer_addr: Some(peer),
        local_addr: local,
        id,
        name: client_name.clone(),
        tx,
        caps,
    };
    if net.tx.send(NetCmd::Attach(handle)).await.is_err() {
        warn!(network = %net.name, "network actor unavailable");
        ctx.registry.release_client(user.id);
        return;
    }

    let mut writer_done = false;
    let nick_hint = "*".to_string();
    let mut cap = CapState {
        caps,
        ..Default::default()
    };
    let mut notify = NetworkNotify::default();
    loop {
        notify.update(&ctx, user.id, cap.caps).await;
        let line = tokio::select! {
            l = reader.next() => l,
            _ = &mut writer => { writer_done = true; break; }
            ev = notify.next() => {
                if let (Some(crate::events::Event::NetworksChanged { user_id }), Some(t)) = (ev, own.upgrade()) {
                    if user_id == user.id {
                        notify.send_changes(&ctx, user.id, &t).await;
                    }
                }
                continue;
            }
        };
        let line = match line {
            Some(Ok(l)) => {
                crate::metrics::Metrics::add(&ctx.metrics.bytes_from_clients, l.len() as u64 + 2);
                l
            }
            Some(Err(e)) => {
                debug!(%peer, error = %e, "client read error");
                break;
            }
            None => break,
        };
        let Ok(msg) = Message::parse(&line) else {
            continue;
        };
        match msg.command.as_str() {
            "PING" => {
                if let Some(t) = own.upgrade() {
                    let token = msg.param(0).unwrap_or("").to_string();
                    let _ = t.try_send(
                        Message::new("PONG", vec![host.clone(), token])
                            .with_prefix(host.clone())
                            .to_string(),
                    );
                }
            }
            "QUIT" => break,
            "CAP" => {
                if let Some(t) = own.upgrade() {
                    let changed = handle_cap(&host, &nick_hint, &msg, &mut cap, true, &t);
                    if changed && net.tx.send(NetCmd::UpdateCaps(id, cap.caps)).await.is_err() {
                        break;
                    }
                }
            }
            "BOUNCER" if cap.caps.bouncer_networks => {
                if let Some(t) = own.upgrade() {
                    crate::bouncer::command(&ctx, &user, &msg, &t).await;
                }
            }
            _ => {
                if net.tx.send(NetCmd::FromClient(id, msg)).await.is_err() {
                    break;
                }
            }
        }
    }
    let _ = net.tx.send(NetCmd::Detach(id)).await;
    ctx.registry.release_client(user.id);
    info!(user = %user.username, network = %net.name, client = %client_name, "client disconnected");
    if !writer_done {
        let _ = tokio::time::timeout(Duration::from_secs(5), writer).await;
    }
}

/// Sends `BOUNCER NETWORK` lines to a client with
/// soju.im/bouncer-networks-notify when the user's networks change.
#[derive(Default)]
struct NetworkNotify {
    rx: Option<tokio::sync::broadcast::Receiver<crate::events::Event>>,
    known: std::collections::BTreeMap<i64, String>,
}

impl NetworkNotify {
    async fn update(&mut self, ctx: &Arc<Ctx>, user_id: i64, caps: ClientCaps) {
        if caps.bouncer_networks_notify && self.rx.is_none() {
            self.rx = Some(ctx.events.subscribe());
            self.known = crate::bouncer::network_list(ctx, user_id).await;
        } else if !caps.bouncer_networks_notify {
            self.rx = None;
        }
    }

    async fn next(&mut self) -> Option<crate::events::Event> {
        match self.rx.as_mut() {
            Some(rx) => loop {
                match rx.recv().await {
                    Ok(e) => return Some(e),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => return None,
                }
            },
            None => std::future::pending().await,
        }
    }

    async fn send_changes(&mut self, ctx: &Arc<Ctx>, user_id: i64, tx: &mpsc::Sender<String>) {
        // Let a burst of changes settle into one update.
        tokio::time::sleep(Duration::from_millis(200)).await;
        while let Some(rx) = self.rx.as_mut() {
            if rx.try_recv().is_err() {
                break;
            }
        }
        let now = crate::bouncer::network_list(ctx, user_id).await;
        for l in crate::bouncer::diff(&mut self.known, now, &ctx.config.hostname) {
            let _ = tx.try_send(l);
        }
    }
}

/// A soju.im/bouncer-networks connection without a network: lists and
/// manages networks; the client opens one connection per network.
async fn control_session(
    ctx: &Arc<Ctx>,
    user: &User,
    reader: &mut Reader,
    tx: mpsc::Sender<String>,
    writer: &mut tokio::task::JoinHandle<()>,
    caps: ClientCaps,
) {
    let host = ctx.config.hostname.clone();
    let nick = user.username.clone();
    let send = |l: String| {
        let _ = tx.try_send(l);
    };
    send(format!(":{host} 001 {nick} :Welcome to xBNC, {nick}"));
    send(format!(
        ":{host} 002 {nick} :Your host is {host}, running {}",
        crate::network::BRAND
    ));
    send(format!(
        ":{host} 003 {nick} :Choose a network with BOUNCER BIND"
    ));
    send(format!(
        ":{host} 004 {nick} {host} xbnc-{} iow biklmnopstv",
        crate::network::VERSION
    ));
    send(format!(
        ":{host} 005 {nick} CASEMAPPING=ascii :are supported by this server"
    ));
    send(format!(":{host} 375 {nick} :- {host} -"));
    send(format!(":{host} 372 {nick} :- {}", crate::network::BRAND));
    send(format!(":{host} 376 {nick} :End of /MOTD command."));
    let mut cap = CapState {
        caps,
        ..Default::default()
    };
    let mut notify = NetworkNotify::default();
    loop {
        notify.update(ctx, user.id, cap.caps).await;
        let line = tokio::select! {
            l = reader.next() => l,
            _ = &mut *writer => return,
            ev = notify.next() => {
                if let Some(crate::events::Event::NetworksChanged { user_id }) = ev {
                    if user_id == user.id {
                        notify.send_changes(ctx, user.id, &tx).await;
                    }
                }
                continue;
            }
        };
        let Some(Ok(line)) = line else { break };
        let Ok(msg) = Message::parse(&line) else {
            continue;
        };
        match msg.command.as_str() {
            "PING" => send(format!(
                ":{host} PONG {host} :{}",
                msg.param(0).unwrap_or("")
            )),
            "QUIT" => break,
            "CAP" => {
                handle_cap(&host, &nick, &msg, &mut cap, true, &tx);
            }
            "BOUNCER" => crate::bouncer::command(ctx, user, &msg, &tx).await,
            "PRIVMSG" | "NOTICE" | "JOIN" | "PART" | "WHO" | "MODE" => {
                send(format!(":{host} NOTICE {nick} :This connection is not bound to a network: use BOUNCER BIND"));
            }
            _ => {}
        }
    }
    drop(tx);
    let _ = tokio::time::timeout(Duration::from_secs(5), &mut *writer).await;
}

async fn register(
    ctx: &Arc<Ctx>,
    reader: &mut Reader,
    tx: &mpsc::Sender<String>,
    ip: IpAddr,
) -> Option<Session> {
    let host = ctx.config.hostname.clone();
    let mut reg = Reg::default();
    let send = |l: String| {
        let _ = tx.try_send(l);
    };
    while let Some(line) = reader.next().await {
        let Ok(line) = line else { return None };
        let Ok(msg) = Message::parse(&line) else {
            continue;
        };
        let nick = reg.nick.clone().unwrap_or_else(|| "*".into());
        match msg.command.as_str() {
            "CAP" => {
                handle_cap(&host, &nick, &msg, &mut reg.cap, false, tx);
            }
            "PASS" => reg.pass = msg.param(0).map(Secret::new),
            "NICK" => match msg.param(0) {
                Some(n) if !n.is_empty() && !n.contains([' ', ',', '*', '?', '!', '@', ':']) => {
                    reg.nick = Some(n.to_string())
                }
                _ => send(format!(":{host} 432 {nick} * :Erroneous nickname")),
            },
            "USER" => reg.user = msg.param(0).map(str::to_string),
            "BOUNCER" => {
                if msg.param(0).is_some_and(|s| s.eq_ignore_ascii_case("BIND")) {
                    match msg.param(1).and_then(|v| v.parse::<i64>().ok()) {
                        Some(id) => reg.bind = Some(id),
                        None => send(format!(
                            ":{host} FAIL BOUNCER INVALID_NETID BIND * :Invalid network id"
                        )),
                    }
                } else {
                    send(format!(":{host} 451 {nick} :You have not registered"));
                }
            }
            "PING" => send(format!(
                ":{host} PONG {host} :{}",
                msg.param(0).unwrap_or("")
            )),
            "QUIT" => return None,
            "AUTHENTICATE" => {
                if let Some(done) = sasl_step(ctx, &mut reg, &msg, tx, ip, &nick).await {
                    if !done {
                        return None;
                    }
                }
            }
            _ => send(format!(":{host} 451 {nick} :You have not registered")),
        }
        if reg.nick.is_some() && reg.user.is_some() && !reg.cap.negotiating {
            return finish(ctx, reg, tx, ip).await;
        }
    }
    None
}

/// Handles one AUTHENTICATE line. Returns `Some(false)` when the connection
/// must be closed (address blocked).
async fn sasl_step(
    ctx: &Arc<Ctx>,
    reg: &mut Reg,
    msg: &Message,
    tx: &mpsc::Sender<String>,
    ip: IpAddr,
    nick: &str,
) -> Option<bool> {
    let host = &ctx.config.hostname;
    let send = |l: String| {
        let _ = tx.try_send(l);
    };
    let arg = msg.param(0).unwrap_or("");
    if !reg.cap.sasl || reg.sasl_ok.is_some() {
        send(format!(":{host} 904 {nick} :SASL authentication failed"));
        return None;
    }
    if arg == "*" {
        reg.sasl_started = false;
        reg.sasl_buf.clear();
        send(format!(":{host} 906 {nick} :SASL authentication aborted"));
        return None;
    }
    if !reg.sasl_started {
        if arg.eq_ignore_ascii_case("PLAIN") {
            reg.sasl_started = true;
            send("AUTHENTICATE +".to_string());
        } else {
            send(format!(
                ":{host} 908 {nick} PLAIN :are available SASL mechanisms"
            ));
            send(format!(":{host} 904 {nick} :SASL authentication failed"));
        }
        return None;
    }
    if arg != "+" {
        reg.sasl_buf.push_str(arg);
    }
    if reg.sasl_buf.len() > 2048 {
        reg.sasl_started = false;
        reg.sasl_buf.clear();
        send(format!(":{host} 905 {nick} :SASL message too long"));
        return None;
    }
    if arg.len() == 400 {
        return None; // more chunks follow
    }
    reg.sasl_started = false;
    let data = std::mem::take(&mut reg.sasl_buf);
    let decoded = B64
        .decode(data.as_bytes())
        .ok()
        .and_then(|b| String::from_utf8(b).ok());
    let parts: Vec<String> = decoded
        .map(|s| s.split('\0').map(str::to_string).collect())
        .unwrap_or_default();
    if parts.len() != 3 {
        send(format!(":{host} 904 {nick} :SASL authentication failed"));
        return None;
    }
    let login = if parts[1].is_empty() {
        parts[0].clone()
    } else {
        parts[1].clone()
    };
    let spec = parse_login(&login);
    match auth::authenticate_ctx(ctx, spec.username.clone(), Secret::new(parts[2].clone())).await {
        Ok(AuthResult::Ok(user)) => {
            let user = *user;
            send(format!(
                ":{host} 900 {nick} {nick}!{}@{host} {} :You are now logged in as {}",
                user.username, user.username, user.username
            ));
            send(format!(
                ":{host} 903 {nick} :SASL authentication successful"
            ));
            reg.sasl_ok = Some((user, spec));
            None
        }
        Ok(AuthResult::Disabled) => {
            send(format!(":{host} 904 {nick} :Account disabled"));
            None
        }
        Ok(AuthResult::BadCredentials) => {
            warn!(%ip, user = %spec.username, "failed SASL login");
            log_login(ctx, ip, &spec.username, false, "bad password (SASL)");
            send(format!(":{host} 904 {nick} :SASL authentication failed"));
            if ctx.limiter.record_failure(ip) {
                let lang = ctx.settings.get().lang_for("");
                let mins = ctx.limiter.block_minutes().to_string();
                let text = crate::i18n::tr(lang, "login.blocked", &[&ip.to_string(), &mins]);
                send(format!(":{host} NOTICE {nick} :{text}"));
                send(format!("ERROR :{text}"));
                return Some(false);
            }
            None
        }
        Err(e) => {
            warn!(error = %e, "authentication error");
            send(format!(":{host} 904 {nick} :SASL authentication failed"));
            None
        }
    }
}

async fn finish(
    ctx: &Arc<Ctx>,
    reg: Reg,
    tx: &mpsc::Sender<String>,
    ip: IpAddr,
) -> Option<Session> {
    let host = &ctx.config.hostname;
    let nick = reg.nick.clone().unwrap_or_else(|| "*".into());
    let send = |l: String| {
        let _ = tx.try_send(l);
    };
    // Before the login the user's language is not known: server default.
    let lang = ctx.settings.get().lang_for("");
    let tr = |key: &str, args: &[&str]| crate::i18n::tr(lang, key, args);
    let user_spec = reg.user.as_deref().map(parse_login).unwrap_or_default();

    let (user, mut spec) = match reg.sasl_ok {
        Some(v) => v,
        None => {
            let Some(pass) = reg.pass.as_ref() else {
                let text = tr("login.password_required", &[]);
                send(format!(":{host} 464 {nick} :{text}"));
                send(format!("ERROR :{text}"));
                return None;
            };
            let (login, pw) = split_pass(pass.expose());
            let spec = match login {
                Some(l) => parse_login(l),
                None => user_spec.clone(),
            };
            // The example copied as is: say so, without counting it as an
            // attack (that would block the address).
            if auth::is_password_placeholder(pw) {
                info!(%ip, user = %spec.username, "login with the example password");
                let text = tr("login.placeholder", &[pw]);
                send(format!(":{host} NOTICE {nick} :{text}"));
                send(format!(":{host} 464 {nick} :{text}"));
                send(format!("ERROR :{text}"));
                return None;
            }
            match auth::authenticate_ctx(ctx, spec.username.clone(), Secret::new(pw)).await {
                Ok(AuthResult::Ok(u)) => (*u, spec),
                Ok(AuthResult::Disabled) => {
                    let text = tr("login.disabled", &[]);
                    send(format!(":{host} 465 {nick} :{text}"));
                    send(format!("ERROR :{text}"));
                    return None;
                }
                Ok(AuthResult::BadCredentials) => {
                    warn!(%ip, user = %spec.username, "failed login");
                    log_login(ctx, ip, &spec.username, false, "bad password");
                    let blocked = ctx.limiter.record_failure(ip);
                    let text = tr("login.bad_credentials", &[&spec.username]);
                    send(format!(":{host} NOTICE {nick} :{text}"));
                    if blocked {
                        let mins = ctx.limiter.block_minutes().to_string();
                        let b = tr("login.blocked", &[&ip.to_string(), &mins]);
                        send(format!(":{host} NOTICE {nick} :{b}"));
                    }
                    send(format!(":{host} 464 {nick} :{text}"));
                    send(format!("ERROR :{text}"));
                    return None;
                }
                Err(e) => {
                    warn!(error = %e, "authentication error");
                    send("ERROR :Internal error".into());
                    return None;
                }
            }
        }
    };
    ctx.limiter.record_success(ip);
    log_login(ctx, ip, &user.username, true, "ok");
    if spec.network.is_none() {
        spec.network = user_spec.network.clone();
    }
    if spec.client.is_none() {
        spec.client = user_spec.client.clone();
    }

    let nets = ctx.registry.for_user(user.id);
    if let Some(id) = reg.bind {
        let Some(net) = nets.iter().find(|n| n.network_id == id).cloned() else {
            send(format!(
                ":{host} FAIL BOUNCER INVALID_NETID BIND {id} :Unknown or stopped network"
            ));
            send("ERROR :Unknown network".into());
            return None;
        };
        return Some(Session {
            client_name: normalise_client_name(spec.client.as_deref()),
            caps: reg.cap.caps,
            user,
            net: Some(net),
        });
    }
    // A modern client without a network: a control connection that lists
    // and manages networks (soju.im/bouncer-networks).
    if spec.network.is_none() && reg.cap.caps.bouncer_networks {
        return Some(Session {
            client_name: normalise_client_name(spec.client.as_deref()),
            caps: reg.cap.caps,
            user,
            net: None,
        });
    }
    let net = match &spec.network {
        Some(name) => ctx.registry.find(user.id, name),
        None if nets.len() == 1 => nets.first().cloned(),
        None => None,
    };
    let Some(net) = net else {
        let names: Vec<String> = nets.iter().map(|n| n.name.clone()).collect();
        let msg = if names.is_empty() {
            tr("login.no_networks", &[])
        } else {
            let example = format!("{}/{}@{}", user.username, names[0], "hexchat");
            match &spec.network {
                Some(asked) => tr(
                    "login.unknown_network",
                    &[asked, &names.join(", "), &example],
                ),
                None => tr("login.choose_network", &[&names.join(", "), &example]),
            }
        };
        send(format!(":{host} NOTICE {nick} :{msg}"));
        send(format!("ERROR :{msg}"));
        return None;
    };
    Some(Session {
        client_name: normalise_client_name(spec.client.as_deref()),
        caps: reg.cap.caps,
        user,
        net: Some(net),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_negotiation() {
        let (tx, mut rx) = mpsc::channel(16);
        let mut st = CapState::default();
        let cap = |line: &str, registered: bool, st: &mut CapState| {
            handle_cap(
                "h",
                "*",
                &Message::parse(line).unwrap(),
                st,
                registered,
                &tx,
            )
        };
        cap("CAP LS 302", false, &mut st);
        assert!(st.negotiating);
        assert_eq!(st.version, 302);
        assert!(rx.try_recv().unwrap().contains("sasl=PLAIN"));
        cap("CAP REQ :server-time sasl", false, &mut st);
        assert!(rx.try_recv().unwrap().contains("ACK"));
        assert!(st.caps.server_time && st.sasl);
        cap("CAP REQ :server-time draft/bogus", false, &mut st);
        assert!(
            rx.try_recv().unwrap().contains("NAK"),
            "unimplemented caps are refused"
        );
        assert!(cap("CAP REQ :-server-time", true, &mut st));
        assert!(!st.caps.server_time);
        cap("CAP REQ :sasl", true, &mut st);
        assert!(rx.try_recv().unwrap().contains("ACK"));
        assert!(
            rx.try_recv().unwrap().contains("NAK"),
            "sasl cannot be requested after registration"
        );
        cap("CAP END", false, &mut st);
        assert!(!st.negotiating);
    }

    #[test]
    fn cap_ls_without_302_hides_values() {
        assert!(!cap_list(0).contains('='));
        assert!(cap_list(302).contains("sasl=PLAIN"));
    }
}
