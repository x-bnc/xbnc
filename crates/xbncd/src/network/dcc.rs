//! The `dcc` module inside the network actor: rewrites DCC offers so the
//! transfer goes through xBNC (see `crate::dcc` for the relay itself).

use super::*;
use crate::dcc::{self as relay, RelayLimits};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// How long a port pair is remembered for RESUME/ACCEPT.
const MAP_TTL: Duration = Duration::from_secs(1800);
const IDLE: Duration = Duration::from_secs(300);

/// Ports of one relayed offer, on the client side and on the IRC side.
#[derive(Debug, Clone)]
pub(super) struct PortMap {
    pub client_port: u16,
    pub net_port: u16,
    pub nick_cf: String,
    pub at: Instant,
}

impl NetActor {
    fn dcc_limits(&self, cfg: &crate::modules::DccCfg) -> RelayLimits {
        RelayLimits {
            max_bytes: cfg.max_size_mb.saturating_mul(1024 * 1024),
            wait: Duration::from_secs(cfg.wait),
            idle: IDLE,
        }
    }

    fn dcc_remember(&mut self, client_port: u16, net_port: u16, nick: &str) {
        let now = Instant::now();
        self.rt
            .dcc_map
            .retain(|m| now.duration_since(m.at) < MAP_TTL);
        if self.rt.dcc_map.len() >= 200 {
            self.rt.dcc_map.remove(0);
        }
        let nick_cf = self.fold(nick);
        self.rt.dcc_map.push(PortMap {
            client_port,
            net_port,
            nick_cf,
            at: now,
        });
    }

    /// A DCC request from someone on IRC. True if it was handled here.
    pub(super) async fn dcc_from_network(&mut self, msg: &Message, from: &str, text: &str) -> bool {
        let Some(cfg) = self.mods.dcc else {
            return false;
        };
        let Some(mut d) = relay::parse(text) else {
            return false;
        };
        let prefix = msg.prefix.clone().unwrap_or_else(|| from.to_string());
        let me = self.current_nick();
        match d.kind.as_str() {
            "SEND" | "CHAT" if d.port != 0 => {
                let Some(ip) = d.ip else { return false };
                if d.port < 1024 {
                    let t = self.t("dcc_port_refused", &[from, &d.kind, &d.port.to_string()]);
                    self.notice_all(&t);
                    return true;
                }
                if relay::is_internal(ip) && !cfg.private_addresses {
                    let t = self.t("dcc_private_refused", &[from, &d.kind]);
                    self.notice_all(&t);
                    return true;
                }
                if self.clients.is_empty() {
                    return false;
                }
                let range = self.ctx.config.dcc.port_range().ok().flatten();
                let limits = self.dcc_limits(&cfg);
                let target = SocketAddr::new(ip, d.port);
                let clients: Vec<(ClientId, Option<SocketAddr>, Option<SocketAddr>)> = self
                    .clients
                    .iter()
                    .map(|c| (c.h.id, c.h.peer_addr, c.h.local_addr))
                    .collect();
                let net_port = d.port;
                for (cid, peer, local) in clients {
                    let (Some(peer), Some(local)) = (peer, local) else {
                        continue;
                    };
                    let listener = match relay::bind(local.ip(), range).await {
                        Ok(l) => l,
                        Err(e) => {
                            warn!(error = %e, "DCC: cannot open a relay port");
                            continue;
                        }
                    };
                    let Ok(port) = listener.local_addr().map(|a| a.port()) else {
                        continue;
                    };
                    relay::spawn_relay(
                        listener,
                        Some(peer.ip()),
                        target,
                        limits,
                        self.ctx.dcc.clone(),
                        format!("{}/{} {} from {from}", self.owner, self.net.name, d.kind),
                    );
                    self.dcc_remember(port, net_port, from);
                    d.ip = Some(local.ip());
                    d.port = port;
                    let line = format!(":{prefix} PRIVMSG {me} :{}", d.render());
                    self.send_to(cid, line);
                }
                true
            }
            "RESUME" | "ACCEPT" => {
                let cf = self.fold(from);
                let Some(m) = self
                    .rt
                    .dcc_map
                    .iter()
                    .find(|m| m.net_port == d.port && m.nick_cf == cf)
                else {
                    return false;
                };
                d.port = m.client_port;
                let line = format!(":{prefix} PRIVMSG {me} :{}", d.render());
                let ids: Vec<ClientId> = self.clients.iter().map(|c| c.h.id).collect();
                for id in ids {
                    self.send_to(id, line.clone());
                }
                true
            }
            _ => false,
        }
    }

    /// A DCC request the user sends from a client. Returns the text to send
    /// to the network (rewritten when relayed), or None to drop it.
    pub(super) async fn dcc_from_client(
        &mut self,
        id: ClientId,
        target: &str,
        text: &str,
    ) -> Option<String> {
        let Some(cfg) = self.mods.dcc else {
            return Some(text.to_string());
        };
        let Some(mut d) = relay::parse(text) else {
            return Some(text.to_string());
        };
        let peer = self
            .clients
            .iter()
            .find(|c| c.h.id == id)
            .and_then(|c| c.h.peer_addr);
        match d.kind.as_str() {
            "SEND" | "CHAT" => {
                let announced = self
                    .ctx
                    .config
                    .dcc
                    .public_ip
                    .or_else(|| announce_addr(self.upstream_local));
                let Some(public) = announced else {
                    let t = self.t("dcc_no_address", &[]);
                    let line = self.status_line("NOTICE", &t);
                    self.send_to(id, line);
                    return None;
                };
                d.ip = Some(public);
                if d.port == 0 {
                    // Passive offer: the other side will offer back, and that
                    // offer is relayed. Just hide the device's address.
                    return Some(d.render());
                }
                let peer = peer?;
                let bind_ip = match public {
                    IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                    IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
                };
                let range = self.ctx.config.dcc.port_range().ok().flatten();
                let listener = match relay::bind(bind_ip, range).await {
                    Ok(l) => l,
                    Err(e) => {
                        warn!(error = %e, "DCC: cannot open a relay port");
                        let t = self.t("dcc_no_port", &[]);
                        let line = self.status_line("NOTICE", &t);
                        self.send_to(id, line);
                        return None;
                    }
                };
                let port = listener.local_addr().ok()?.port();
                // Always connect back to the device that made the offer, never
                // to an address written in the message.
                let back = SocketAddr::new(peer.ip(), d.port);
                relay::spawn_relay(
                    listener,
                    None,
                    back,
                    self.dcc_limits(&cfg),
                    self.ctx.dcc.clone(),
                    format!("{}/{} {} to {target}", self.owner, self.net.name, d.kind),
                );
                self.dcc_remember(d.port, port, target);
                d.port = port;
                Some(d.render())
            }
            "RESUME" | "ACCEPT" => {
                let cf = self.fold(target);
                if let Some(m) = self
                    .rt
                    .dcc_map
                    .iter()
                    .find(|m| m.client_port == d.port && m.nick_cf == cf)
                {
                    d.port = m.net_port;
                }
                Some(d.render())
            }
            _ => Some(text.to_string()),
        }
    }
}

/// Address announced in DCC offers when `[dcc] public_ip` is not set. Most
/// IRC clients only understand IPv4 offers, so when the connection to the
/// network uses IPv6 the server's public IPv4 address is preferred.
fn announce_addr(upstream: Option<IpAddr>) -> Option<IpAddr> {
    if let Some(IpAddr::V4(v4)) = upstream {
        return Some(IpAddr::V4(v4));
    }
    let v4 = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|s| {
            // No packet is sent: this only asks the kernel for the route.
            s.connect((Ipv4Addr::new(192, 0, 2, 1), 9))?;
            s.local_addr()
        })
        .ok()
        .map(|a| a.ip())
        .filter(|ip| ip.is_ipv4() && !relay::is_internal(*ip));
    v4.or(upstream)
}
