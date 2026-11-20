//! The panel as an IRC client (chat), mentions, per-channel history,
//! notifications and Web Push subscriptions.

use super::{audit, ApiError, ApiResult, AppState, Auth};
use crate::db;
use crate::network::{ClientCaps, ClientHandle, NetCmd};
use crate::server;
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use tokio::sync::mpsc;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/networks/{id}/chat", get(chat))
        .route(
            "/networks/{id}/channels/{name}/history",
            axum::routing::put(set_retention),
        )
        .route("/mentions", get(mentions))
        .route("/me/notifications", get(get_notify).put(put_notify))
        .route("/me/notifications/test", post(test_notify))
        .route("/push/key", get(push_key))
        .route(
            "/me/push",
            get(list_push).post(add_push).delete(remove_push),
        )
}

// ------------------------------------------------------------ chat

/// Commands the panel's chat may send (as the user's own IRC client).
const CHAT_COMMANDS: &[&str] = &[
    "PRIVMSG", "NOTICE", "JOIN", "PART", "TOPIC", "MARKREAD", "MODE", "KICK", "INVITE", "NICK",
    "AWAY", "WHOIS", "WHO", "NAMES",
];

async fn chat(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
    ws: WebSocketUpgrade,
) -> Response {
    if let Err(e) = a.require_session() {
        return e.into_response();
    }
    if let Err(e) = super::api::load_network_pub(&st.ctx, &a, id, false).await {
        return e.into_response();
    }
    // Only the owner chats as themselves (an administrator viewing another
    // user's network must not speak for them).
    let Some(net) = server::network_handle(&st.ctx, id).filter(|n| n.user_id == a.user.id) else {
        return ApiError::bad("You can only chat on your own running networks").into_response();
    };
    ws.max_message_size(16 * 1024)
        .on_upgrade(move |socket| chat_session(st, a, net, socket))
}

async fn chat_session(
    st: AppState,
    a: Auth,
    net: crate::network::NetHandle,
    mut socket: WebSocket,
) {
    let ctx = st.ctx.clone();
    let max = a
        .user
        .max_clients
        .map(|m| m.max(0) as usize)
        .unwrap_or(ctx.settings.get().limits.max_clients_per_user);
    if !ctx.registry.acquire_client(a.user.id, max) {
        let _ = socket
            .send(WsMessage::Text(
                json!({"type":"error","text":"Too many simultaneous clients for this account"})
                    .to_string()
                    .into(),
            ))
            .await;
        return;
    }
    let (tx, mut rx) = mpsc::channel::<String>(ctx.settings.get().limits.client_queue);
    let cid = ctx.next_client_id.fetch_add(1, Ordering::Relaxed);
    let handle = ClientHandle {
        id: cid,
        peer: format!("{} (panel)", a.ip),
        peer_addr: None,
        local_addr: None,
        name: "web".into(),
        tx,
        caps: ClientCaps {
            server_time: true,
            message_tags: true,
            echo_message: true,
            batch: false,
            // History comes from the REST API, not as playback.
            chathistory: true,
            event_playback: false,
            read_marker: true,
            bouncer_networks: false,
            bouncer_networks_notify: false,
            ..Default::default()
        },
    };
    if net.tx.send(NetCmd::Attach(handle)).await.is_err() {
        ctx.registry.release_client(a.user.id);
        return;
    }
    // Keeps quiet conversations from hitting the panel's idle timeout.
    let mut ping = tokio::time::interval(std::time::Duration::from_secs(30));
    loop {
        tokio::select! {
            _ = ping.tick() => {
                if socket.send(WsMessage::Ping(Default::default())).await.is_err() {
                    break;
                }
            }
            line = rx.recv() => match line {
                Some(l) => {
                    if socket.send(WsMessage::Text(json!({"type":"line","line":l}).to_string().into())).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            msg = socket.recv() => match msg {
                Some(Ok(WsMessage::Text(t))) => {
                    let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                    let Some(line) = v.get("line").and_then(Value::as_str) else { continue };
                    if line.len() > 2048 || line.contains(['\r', '\n', '\0']) {
                        continue;
                    }
                    let Ok(m) = xbnc_proto::Message::parse(line) else { continue };
                    if !CHAT_COMMANDS.contains(&m.command.to_ascii_uppercase().as_str()) {
                        let _ = socket.send(WsMessage::Text(json!({"type":"error","text":format!("{} is not available in the panel chat", m.command)}).to_string().into())).await;
                        continue;
                    }
                    if net.tx.send(NetCmd::FromClient(cid, m)).await.is_err() {
                        break;
                    }
                }
                Some(Ok(WsMessage::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            },
        }
    }
    let _ = net.tx.send(NetCmd::Detach(cid)).await;
    ctx.registry.release_client(a.user.id);
}

// ------------------------------------------------------------ mentions

#[derive(Deserialize)]
struct MentionsQuery {
    before: Option<i64>,
    limit: Option<usize>,
}

async fn mentions(
    State(st): State<AppState>,
    a: Auth,
    Query(q): Query<MentionsQuery>,
) -> ApiResult<Json<Value>> {
    let uid = a.user.id;
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let (nets, rows) = st
        .ctx
        .db
        .call(move |c| {
            let nets = db::list_networks(c, uid)?;
            let ids: Vec<i64> = nets.iter().map(|n| n.id).collect();
            Ok((nets, db::mentions(c, &ids, q.before, limit)?))
        })
        .await?;
    let name = |id: i64| {
        nets.iter()
            .find(|n| n.id == id)
            .map(|n| n.name.clone())
            .unwrap_or_default()
    };
    let list: Vec<Value> = rows
        .iter()
        .map(|m| {
            json!({
                "id": m.id,
                "network_id": m.network_id,
                "network": name(m.network_id),
                "target": m.target,
                "ts": m.ts,
                "nick": xbnc_proto::Prefix::parse(&m.sender).nick,
                "text": m.text,
            })
        })
        .collect();
    let more = list.len() == limit;
    Ok(Json(json!({ "mentions": list, "more": more })))
}

// ------------------------------------------------------- channel history

#[derive(Deserialize)]
struct RetentionReq {
    /// null: the server setting; -1: do not store; 0: forever; N: days.
    retention_days: Option<i64>,
}

async fn set_retention(
    State(st): State<AppState>,
    a: Auth,
    Path((id, name)): Path<(i64, String)>,
    Json(req): Json<RetentionReq>,
) -> ApiResult<Json<Value>> {
    let (n, owner) = super::api::load_network_pub(&st.ctx, &a, id, true).await?;
    let cf = xbnc_proto::CaseMapping::Rfc1459.fold(&name);
    let days = req.retention_days;
    let found = st
        .ctx
        .db
        .call(move |c| db::set_channel_retention(c, id, &cf, days))
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    if !found {
        return Err(ApiError::not_found("Channel"));
    }
    if let Some(h) = server::network_handle(&st.ctx, id) {
        let _ = h.tx.send(NetCmd::Reload).await;
    }
    let what = days.map_or("server setting".to_string(), |d| match d {
        -1 => "not stored".into(),
        0 => "forever".into(),
        d => format!("{d} days"),
    });
    audit(
        &st.ctx,
        &a.user,
        "channel.history",
        format!("{owner}/{} {name}: {what}", n.name),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

// ------------------------------------------------------- notifications

fn masked(s: &crate::notify::NotifySettings) -> Value {
    let mut v = serde_json::to_value(s).unwrap_or_default();
    // The bot token is never sent back whole.
    if !s.telegram_token.is_empty() {
        let tail: String = s
            .telegram_token
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        v["telegram_token"] = json!(format!("…{tail}"));
    }
    v
}

async fn get_notify(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    let (ctx, uid) = (st.ctx.clone(), a.user.id);
    let s = st
        .ctx
        .db
        .call(move |c| crate::notify::load(&ctx, c, uid))
        .await?;
    Ok(Json(json!({
        "settings": masked(&s),
        "email_available": st.ctx.config.smtp.is_some(),
    })))
}

async fn put_notify(
    State(st): State<AppState>,
    a: Auth,
    Json(mut req): Json<crate::notify::NotifySettings>,
) -> ApiResult<Json<Value>> {
    a.require_session()?;
    let (ctx, uid) = (st.ctx.clone(), a.user.id);
    let old = st
        .ctx
        .db
        .call(move |c| crate::notify::load(&ctx, c, uid))
        .await?;
    // "…abcd" means "keep the token".
    if req.telegram_token.starts_with('…') {
        req.telegram_token = old.telegram_token.clone();
    }
    req.validate().map_err(|e| ApiError::bad(e.to_string()))?;
    if !req.email.is_empty() && st.ctx.config.smtp.is_none() {
        return Err(ApiError::bad("E-mail is not configured on this server"));
    }
    let (ctx, s) = (st.ctx.clone(), req.clone());
    st.ctx
        .db
        .call(move |c| crate::notify::save(&ctx, c, uid, &s))
        .await?;
    audit(
        &st.ctx,
        &a.user,
        "notifications.update",
        format!("enabled={}", req.enabled),
    )
    .await;
    Ok(Json(json!({ "settings": masked(&req) })))
}

async fn test_notify(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    a.require_session()?;
    let (ctx, uid) = (st.ctx.clone(), a.user.id);
    let s = st
        .ctx
        .db
        .call(move |c| crate::notify::load(&ctx, c, uid))
        .await?;
    let h = crate::notify::Highlight {
        network: "xBNC".into(),
        network_id: 0,
        conversation: "#xbnc".into(),
        from: "xBNC".into(),
        text: "Test notification".into(),
        private: false,
        ts: db::now_ms(),
    };
    let results = crate::notify::deliver(&st.ctx, uid, &s, &h, 0).await;
    let out: Vec<Value> = results
        .into_iter()
        .map(|(ch, r)| json!({ "channel": ch, "ok": r.is_ok(), "error": r.err().map(|e| format!("{e:#}")) }))
        .collect();
    Ok(Json(json!({ "results": out })))
}

// ------------------------------------------------------------ push

async fn push_key(State(st): State<AppState>, _a: Auth) -> ApiResult<Json<Value>> {
    let v = crate::notify::vapid(&st.ctx).await?;
    Ok(Json(json!({ "public_key": v.public_b64 })))
}

async fn list_push(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    let uid = a.user.id;
    let subs = st
        .ctx
        .db
        .call(move |c| db::push_subscriptions(c, uid))
        .await?;
    Ok(Json(json!(subs)))
}

#[derive(Deserialize)]
struct PushKeys {
    p256dh: String,
    auth: String,
}

#[derive(Deserialize)]
struct PushReq {
    endpoint: String,
    keys: PushKeys,
    #[serde(default)]
    label: String,
}

async fn add_push(
    State(st): State<AppState>,
    a: Auth,
    Json(req): Json<PushReq>,
) -> ApiResult<Json<Value>> {
    a.require_session()?;
    let ok_endpoint =
        req.endpoint.len() <= 1000 && crate::http::Url::parse(&req.endpoint).is_some_and(|u| u.tls);
    let ok_keys = req.keys.p256dh.len() <= 200
        && req.keys.auth.len() <= 100
        && req
            .keys
            .p256dh
            .chars()
            .chain(req.keys.auth.chars())
            .all(|c| c.is_ascii_alphanumeric() || "-_=".contains(c));
    if !ok_endpoint || !ok_keys {
        return Err(ApiError::bad("Invalid push subscription"));
    }
    // Check the keys now rather than at the first notification.
    crate::push::encrypt(&req.keys.p256dh, &req.keys.auth, b"x")
        .map_err(|_| ApiError::bad("Invalid push subscription"))?;
    let label: String = req
        .label
        .chars()
        .filter(|c| !c.is_control())
        .take(60)
        .collect();
    let sub = db::PushSubscription {
        endpoint: req.endpoint,
        p256dh: req.keys.p256dh,
        auth: req.keys.auth,
        label,
        created_at: 0,
    };
    let uid = a.user.id;
    st.ctx
        .db
        .call(move |c| db::add_push_subscription(c, uid, &sub))
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct PushDel {
    endpoint: String,
}

async fn remove_push(
    State(st): State<AppState>,
    a: Auth,
    Json(req): Json<PushDel>,
) -> ApiResult<Json<Value>> {
    a.require_session()?;
    let uid = a.user.id;
    st.ctx
        .db
        .call(move |c| db::delete_push_subscription(c, uid, &req.endpoint))
        .await?;
    Ok(Json(json!({ "ok": true })))
}
