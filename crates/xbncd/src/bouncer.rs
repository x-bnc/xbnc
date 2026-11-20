//! `soju.im/bouncer-networks`: modern clients list the user's networks,
//! open one connection per network (`BOUNCER BIND`) and add, change or
//! remove networks themselves, without a login per network.

use crate::db::{self, NetworkUpdate, NewNetwork, User};
use crate::events::Event;
use crate::rbac::{Perm, Role};
use crate::server::{self, Ctx};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use xbnc_proto::{escape_tag_value, unescape_tag_value, Message};

/// Attributes of each network: netid -> "name=...;host=...".
pub async fn network_list(ctx: &Arc<Ctx>, user_id: i64) -> BTreeMap<i64, String> {
    let nets = ctx
        .db
        .call(move |c| db::list_networks(c, user_id))
        .await
        .unwrap_or_default();
    let mut out = BTreeMap::new();
    for n in nets {
        let state = if !n.enabled {
            "disconnected"
        } else {
            match server::network_status(ctx, n.id).await {
                Some(s) if s.state == "connected" => "connected",
                Some(_) => "connecting",
                None => "disconnected",
            }
        };
        let mut attrs = vec![
            ("name", n.name.clone()),
            ("host", n.host.clone()),
            ("port", n.port.to_string()),
            ("tls", if n.tls { "1" } else { "0" }.to_string()),
            ("nickname", n.nick.clone()),
            ("username", n.username.clone()),
            ("realname", n.realname.clone()),
            ("state", state.to_string()),
        ];
        if let Some(s) = server::network_status(ctx, n.id).await {
            if let Some(e) = s.last_error.filter(|_| state != "connected") {
                attrs.push(("error", e));
            }
        }
        let text = attrs
            .iter()
            .map(|(k, v)| format!("{k}={}", escape_tag_value(v)))
            .collect::<Vec<_>>()
            .join(";");
        out.insert(n.id, text);
    }
    out
}

fn parse_attrs(s: &str) -> BTreeMap<String, String> {
    s.split(';')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (k.to_string(), unescape_tag_value(v)),
            None => (p.to_string(), String::new()),
        })
        .collect()
}

fn fail(host: &str, code: &str, sub: &str, ctx_arg: Option<&str>, text: &str) -> String {
    match ctx_arg {
        Some(a) => format!(":{host} FAIL BOUNCER {code} {sub} {a} :{text}"),
        None => format!(":{host} FAIL BOUNCER {code} {sub} :{text}"),
    }
}

/// Handles `BOUNCER <subcommand>` from a registered client.
pub async fn command(ctx: &Arc<Ctx>, user: &User, msg: &Message, tx: &mpsc::Sender<String>) {
    let host = ctx.config.hostname.clone();
    let send = |l: String| {
        let _ = tx.try_send(l);
    };
    let sub = msg.param(0).unwrap_or("").to_ascii_uppercase();
    let role = Role::from_db(&user.role);
    match sub.as_str() {
        "LISTNETWORKS" => {
            let list = network_list(ctx, user.id).await;
            let tag = format!("bn{}", crate::db::now_ms() % 100_000);
            send(format!(":{host} BATCH +{tag} soju.im/bouncer-networks"));
            for (id, attrs) in list {
                send(format!("@batch={tag} :{host} BOUNCER NETWORK {id} {attrs}"));
            }
            send(format!(":{host} BATCH -{tag}"));
        }
        "BIND" => send(fail(
            &host,
            "REGISTRATION_IS_COMPLETED",
            "BIND",
            None,
            "Cannot bind after registration",
        )),
        "ADDNETWORK" => {
            if !role.can(Perm::ManageOwnNetworks) {
                send(fail(
                    &host,
                    "ACCOUNT_REQUIRED",
                    "ADDNETWORK",
                    None,
                    "Your account cannot add networks",
                ));
                return;
            }
            let a = parse_attrs(msg.param(1).unwrap_or(""));
            let Some(host_attr) = a.get("host").filter(|h| !h.is_empty()) else {
                send(fail(
                    &host,
                    "NEED_ATTRIBUTE",
                    "ADDNETWORK",
                    Some("host"),
                    "A host is required",
                ));
                return;
            };
            let tls = a.get("tls").is_none_or(|v| v != "0");
            let nick = a
                .get("nickname")
                .cloned()
                .unwrap_or_else(|| user.username.clone());
            let name = a.get("name").cloned().unwrap_or_else(|| {
                // "irc.undernet.org" -> "undernet"
                let parts: Vec<&str> = host_attr.split('.').collect();
                let base = if parts.len() >= 2 {
                    parts[parts.len() - 2]
                } else {
                    host_attr.as_str()
                };
                base.chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                    .collect()
            });
            let nn = NewNetwork {
                name,
                host: host_attr.clone(),
                port: a
                    .get("port")
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(if tls { 6697 } else { 6667 }),
                tls,
                username: a.get("username").cloned().unwrap_or_else(|| nick.clone()),
                realname: a.get("realname").cloned().unwrap_or_else(|| nick.clone()),
                nick,
                server_password_enc: a
                    .get("pass")
                    .filter(|p| !p.is_empty())
                    .map(|p| ctx.secrets.encrypt(p)),
                sasl_username: None,
                sasl_password_enc: None,
            };
            let uid = user.id;
            let max = user
                .max_networks
                .map(|m| m.max(0) as usize)
                .unwrap_or(ctx.settings.get().limits.max_networks_per_user);
            match ctx
                .db
                .call(move |c| db::create_network(c, uid, &nn, max))
                .await
            {
                Ok(id) => {
                    let detail = format!("{}/{} (from IRC)", user.username, id);
                    let who = user.username.clone();
                    let _ = ctx
                        .db
                        .call(move |c| db::audit(c, &who, "network.create", &detail))
                        .await;
                    let _ = server::sync_networks(ctx).await;
                    ctx.events.send(Event::NetworksChanged { user_id: user.id });
                    send(format!(":{host} BOUNCER ADDNETWORK {id}"));
                }
                Err(e) => send(fail(
                    &host,
                    "INVALID_ATTRIBUTE",
                    "ADDNETWORK",
                    None,
                    &e.to_string(),
                )),
            }
        }
        "CHANGENETWORK" | "DELNETWORK" => {
            let Some(id) = msg.param(1).and_then(|v| v.parse::<i64>().ok()) else {
                send(fail(
                    &host,
                    "INVALID_NETID",
                    &sub,
                    msg.param(1),
                    "Invalid network id",
                ));
                return;
            };
            let uid = user.id;
            let owned = ctx
                .db
                .call(move |c| db::get_network_by_id(c, id))
                .await
                .ok()
                .flatten()
                .filter(|n| n.user_id == uid);
            let Some(net) = owned else {
                send(fail(
                    &host,
                    "INVALID_NETID",
                    &sub,
                    Some(&id.to_string()),
                    "Unknown network",
                ));
                return;
            };
            if !role.can(Perm::ManageOwnNetworks) {
                send(fail(
                    &host,
                    "ACCOUNT_REQUIRED",
                    &sub,
                    Some(&id.to_string()),
                    "Your account cannot change networks",
                ));
                return;
            }
            if sub == "DELNETWORK" {
                let _ = ctx.db.call(move |c| db::delete_network_by_id(c, id)).await;
                let detail = format!("{}/{} (from IRC)", user.username, net.name);
                let who = user.username.clone();
                let _ = ctx
                    .db
                    .call(move |c| db::audit(c, &who, "network.delete", &detail))
                    .await;
                let _ = server::sync_networks(ctx).await;
                ctx.events.send(Event::NetworksChanged { user_id: user.id });
                send(format!(":{host} BOUNCER DELNETWORK {id}"));
                return;
            }
            let a = parse_attrs(msg.param(2).unwrap_or(""));
            let upd = NetworkUpdate {
                name: a.get("name").cloned(),
                host: a.get("host").cloned(),
                port: a.get("port").and_then(|p| p.parse().ok()),
                tls: a.get("tls").map(|v| v != "0"),
                nick: a.get("nickname").cloned(),
                username: a.get("username").cloned(),
                realname: a.get("realname").cloned(),
                enabled: None,
                server_password_enc: a
                    .get("pass")
                    .map(|p| (!p.is_empty()).then(|| ctx.secrets.encrypt(p))),
                sasl_username: None,
                sasl_password_enc: None,
            };
            let renamed = upd.name.clone().filter(|n| *n != net.name);
            let reconnect = upd.host.is_some()
                || upd.port.is_some()
                || upd.tls.is_some()
                || upd.nick.is_some()
                || upd.username.is_some()
                || upd.realname.is_some()
                || upd.server_password_enc.is_some();
            match ctx.db.call(move |c| db::update_network(c, id, &upd)).await {
                Ok(_) => {
                    if let Some(name) = renamed {
                        let _ = server::rename_network(ctx, id, &name).await;
                    }
                    if !reconnect {
                    } else if let Some(h) = server::network_handle(ctx, id) {
                        let _ = h.tx.send(crate::network::NetCmd::Reload).await;
                        let _ =
                            h.tx.send(crate::network::NetCmd::Control(
                                crate::network::NetAction::Jump,
                            ))
                            .await;
                    }
                    ctx.events.send(Event::NetworksChanged { user_id: user.id });
                    send(format!(":{host} BOUNCER CHANGENETWORK {id}"));
                }
                Err(e) => send(fail(
                    &host,
                    "INVALID_ATTRIBUTE",
                    "CHANGENETWORK",
                    Some(&id.to_string()),
                    &e.to_string(),
                )),
            }
        }
        _ => send(fail(
            &host,
            "UNKNOWN_COMMAND",
            &sub,
            None,
            "Unknown BOUNCER subcommand",
        )),
    }
}

/// Lines to send after a change: new or changed networks, and `*` for
/// removed ones. `known` is updated.
pub fn diff(
    known: &mut BTreeMap<i64, String>,
    now: BTreeMap<i64, String>,
    host: &str,
) -> Vec<String> {
    let mut lines = Vec::new();
    for id in known.keys() {
        if !now.contains_key(id) {
            lines.push(format!(":{host} BOUNCER NETWORK {id} *"));
        }
    }
    for (id, attrs) in &now {
        if known.get(id) != Some(attrs) {
            lines.push(format!(":{host} BOUNCER NETWORK {id} {attrs}"));
        }
    }
    *known = now;
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_and_diff() {
        let a = parse_attrs("name=Under\\snet;host=irc.undernet.org;tls=0");
        assert_eq!(a["name"], "Under net");
        assert_eq!(a["tls"], "0");
        let mut known = BTreeMap::from([(1, "name=a".to_string()), (2, "name=b".to_string())]);
        let now = BTreeMap::from([(1, "name=a".to_string()), (3, "name=c".to_string())]);
        let lines = diff(&mut known, now, "h");
        assert_eq!(
            lines,
            vec![":h BOUNCER NETWORK 2 *", ":h BOUNCER NETWORK 3 name=c"]
        );
    }
}
