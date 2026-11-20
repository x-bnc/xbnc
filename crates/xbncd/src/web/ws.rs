//! WebSocket feed for the dashboard: pushes live figures every 2 seconds.

use super::{AppState, Auth};
use crate::db;
use crate::rbac::Perm;
use crate::server;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::time::Duration;

pub async fn handler(State(st): State<AppState>, a: Auth, ws: WebSocketUpgrade) -> Response {
    if let Err(e) = a.require_session() {
        return e.into_response();
    }
    ws.max_message_size(16 * 1024)
        .on_upgrade(move |socket| feed(st, a, socket))
}

async fn feed(st: AppState, a: Auth, mut socket: WebSocket) {
    let global = a.role.can(Perm::ViewSystem);
    let uid = a.user.id;
    let all = a.role.can(Perm::ManageAllNetworks) && global;
    let token_check_every = 15; // ticks (30 s): re-validate the session
    let mut tick: u64 = 0;
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    loop {
        tokio::select! {
            _ = interval.tick() => {}
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                _ => continue,
            },
        }
        tick += 1;
        if tick.is_multiple_of(token_check_every) {
            let sid = a.session.id;
            let alive = st
                .ctx
                .db
                .call(move |c| {
                    Ok(db::get_session(c, sid)?.is_some_and(|s| s.expires_at > db::now_ms()))
                })
                .await
                .unwrap_or(false);
            if !alive {
                let _ = socket
                    .send(Message::Text(json!({"type": "logout"}).to_string().into()))
                    .await;
                return;
            }
        }
        let nets = st
            .ctx
            .db
            .call(move |c| {
                let mut out = Vec::new();
                for u in db::list_users(c)?
                    .into_iter()
                    .filter(|u| all || u.id == uid)
                {
                    for n in db::list_networks(c, u.id)? {
                        out.push((n.id, n.name, n.enabled, u.username.clone()));
                    }
                }
                Ok(out)
            })
            .await
            .unwrap_or_default();
        let mut networks = Vec::new();
        for (id, name, enabled, owner) in nets {
            let s = if enabled {
                server::network_status(&st.ctx, id).await
            } else {
                None
            };
            networks.push(json!({
                "id": id,
                "name": name,
                "owner": owner,
                "state": s.as_ref().map(|s| s.state).unwrap_or("disabled"),
                "nick": s.as_ref().map(|s| s.nick.clone()),
                "channels": s.as_ref().map(|s| s.channels.len()).unwrap_or(0),
                "clients": s.as_ref().map(|s| s.clients.len()).unwrap_or(0),
                "last_error": s.as_ref().and_then(|s| s.last_error.clone()),
            }));
        }
        let mut payload = json!({
            "type": "stats",
            "ts": db::now_ms(),
            "uptime_seconds": (db::now_ms() - st.ctx.started_at) / 1000,
            "networks": networks,
        });
        if global {
            payload["system"] = json!({
                "rss_bytes": crate::metrics::rss_bytes(),
                "cpu_percent": st.ctx.metrics.cpu_percent(),
                "irc_clients": st.ctx.registry.total_clients(),
                "connections": st.ctx.connections.load(std::sync::atomic::Ordering::Relaxed),
                "blocked_ips": st.ctx.limiter.blocked().len(),
                "counters": st.ctx.metrics.snapshot(),
            });
        }
        if socket
            .send(Message::Text(payload.to_string().into()))
            .await
            .is_err()
        {
            return;
        }
    }
}
