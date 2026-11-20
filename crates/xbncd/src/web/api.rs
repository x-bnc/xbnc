//! REST API v1 handlers.

use super::{
    audit, clear_cookie, client_ip, hash_token, random_token, session_cookie, ApiError, ApiResult,
    AppState, Auth,
};
use crate::crypto::{self, Secret};
use crate::db::{self, Network, NetworkUpdate, NewNetwork, User, UserUpdate};
use crate::network::{NetAction, NetCmd, NetStatus};
use crate::rbac::{Perm, Role};
use crate::server::{self, Ctx};
use crate::settings::Settings;
use crate::totp;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::SocketAddr;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/i18n", get(i18n_languages))
        .route("/i18n/{code}", get(i18n_texts))
        .route("/setup", get(setup_status).post(setup))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/dashboard", get(dashboard))
        .route("/presets", get(presets))
        .route("/networks", get(list_networks).post(create_network))
        .route(
            "/networks/{id}",
            get(get_network)
                .patch(update_network)
                .delete(delete_network),
        )
        .route("/networks/{id}/control", post(control_network))
        .route(
            "/networks/{id}/on-connect",
            get(get_on_connect).put(put_on_connect),
        )
        .route("/modules", get(module_catalog))
        .route("/networks/{id}/modules", get(network_modules))
        .route("/networks/{id}/modules/{module}", put(put_network_module))
        .route(
            "/networks/{id}/cert",
            get(get_cert).put(put_cert).delete(delete_cert),
        )
        .route("/networks/{id}/cert/generate", post(generate_cert))
        .route("/notes", get(list_notes))
        .route("/notes/{key}", put(put_note).delete(delete_note))
        .route("/me/language", put(set_my_language))
        .route("/me/timezone", put(set_my_timezone))
        .route("/diagnostics", get(diagnostics))
        .route("/tokens", get(list_tokens).post(create_token))
        .route("/tokens/{id}", delete(delete_token))
        .route("/alerts", get(list_alerts))
        .route("/metrics", get(prometheus))
        .route("/updates", get(updates_status))
        .route("/updates/check", post(updates_check))
        .route("/updates/install", post(updates_install))
        .route("/import/preview", post(import_preview))
        .route("/import/apply", post(import_apply))
        .route(
            "/networks/{id}/channels",
            get(list_channels).post(add_channel),
        )
        .route("/networks/{id}/channels/{name}", delete(remove_channel))
        .route("/networks/{id}/clients/{client}", delete(disconnect_client))
        .route("/networks/{id}/conversations", get(conversations))
        .route("/networks/{id}/history", get(history).delete(clear_history))
        .route("/networks/{id}/history/export", get(export_history))
        .route("/users", get(list_users).post(create_user))
        .route(
            "/users/{id}",
            get(get_user).patch(update_user).delete(delete_user),
        )
        .route("/users/{id}/password", post(reset_password))
        .route("/me/password", put(change_my_password))
        .route("/me/avatar", put(set_my_avatar).delete(delete_my_avatar))
        .route("/users/{id}/avatar", get(get_avatar).delete(delete_user_avatar))
        .route("/me/totp/setup", post(totp_setup))
        .route("/me/totp/enable", post(totp_enable))
        .route("/me/totp/disable", post(totp_disable))
        .route("/sessions", get(list_sessions))
        .route("/sessions/{id}", delete(revoke_session))
        .route("/security", get(security_overview))
        .route("/security/blocked/{ip}", delete(unblock_ip))
        .route("/audit", get(audit_log))
        .route("/settings", get(get_settings).put(put_settings))
        .route("/system", get(system_info))
        .route("/openapi.json", get(openapi))
}

// ----------------------------------------------------------------- views

#[derive(Serialize)]
pub struct UserView {
    id: i64,
    username: String,
    role: String,
    enabled: bool,
    max_networks: Option<i64>,
    max_clients: Option<i64>,
    max_history_mb: Option<i64>,
    totp_enabled: bool,
    last_login_at: Option<i64>,
    created_at: i64,
    networks: i64,
    connected_clients: usize,
    language: String,
    last_seen_at: Option<i64>,
    /// Address of the profile photo (changes with the photo), or null.
    avatar: Option<String>,
}

/// Address of a user's photo; `v` changes with each new photo so browsers
/// never show an old one from their cache.
fn avatar_url(user_id: i64, version: Option<i64>) -> Option<String> {
    version.map(|v| format!("/api/v1/users/{user_id}/avatar?v={v}"))
}

async fn with_avatar(ctx: &Ctx, mut v: UserView) -> UserView {
    let id = v.id;
    let ver = ctx
        .db
        .call(move |c| Ok(db::get_avatar(c, id)?.map(|a| a.2)))
        .await
        .unwrap_or(None);
    v.avatar = avatar_url(id, ver);
    v
}

fn user_view(ctx: &Ctx, u: &User, networks: i64) -> UserView {
    UserView {
        id: u.id,
        username: u.username.clone(),
        role: u.role.clone(),
        enabled: u.enabled,
        max_networks: u.max_networks,
        max_clients: u.max_clients,
        max_history_mb: u.max_history_mb,
        totp_enabled: u.totp_enabled,
        last_login_at: u.last_login_at,
        created_at: u.created_at,
        networks,
        connected_clients: ctx.registry.clients_of(u.id),
        language: u.language.clone(),
        last_seen_at: u.last_seen_at,
        avatar: None,
    }
}

#[derive(Serialize)]
pub struct NetworkView {
    id: i64,
    user_id: i64,
    owner: String,
    name: String,
    host: String,
    port: u16,
    tls: bool,
    nick: String,
    username: String,
    realname: String,
    has_server_password: bool,
    sasl_username: Option<String>,
    has_sasl_password: bool,
    enabled: bool,
    /// Live state; absent when the network is disabled or not running.
    status: Option<NetStatus>,
}

fn network_view(n: &Network, owner: &str, status: Option<NetStatus>) -> NetworkView {
    NetworkView {
        id: n.id,
        user_id: n.user_id,
        owner: owner.to_string(),
        name: n.name.clone(),
        host: n.host.clone(),
        port: n.port,
        tls: n.tls,
        nick: n.nick.clone(),
        username: n.username.clone(),
        realname: n.realname.clone(),
        has_server_password: n.server_password_enc.is_some(),
        sasl_username: n.sasl_username.clone(),
        has_sasl_password: n.sasl_password_enc.is_some(),
        enabled: n.enabled,
        status,
    }
}

fn permissions(role: Role) -> Value {
    json!({
        "edit_settings": role.can(Perm::EditSettings),
        "view_system": role.can(Perm::ViewSystem),
        "view_users": role.can(Perm::ViewUsers),
        "manage_users": role.can(Perm::ManageUsers),
        "suspend_users": role.can(Perm::SuspendUsers),
        "view_audit": role.can(Perm::ViewAudit),
        "manage_security": role.can(Perm::ManageSecurity),
        "manage_own_networks": role.can(Perm::ManageOwnNetworks),
        "manage_all_networks": role.can(Perm::ManageAllNetworks),
    })
}

fn me_json(st: &AppState, a: &Auth) -> Value {
    json!({
        "user": user_view(&st.ctx, &a.user, -1),
        "csrf": a.session.csrf,
        "permissions": permissions(a.role),
        "irc": irc_endpoints(&st.ctx),
        "irc_connect": irc_connect(&st.ctx),
        "hostname": st.ctx.config.hostname,
        "version": crate::network::VERSION,
        "language": st.ctx.settings.get().lang_for(&a.user.language).code(),
        "timezone": a.user.timezone,
        "languages": crate::i18n::languages(),
    })
}

/// The names users connect to: the configured hostname (the domain or
/// IP given at installation) and, when different, this machine's address
/// on the local network.
fn connect_hosts(ctx: &Ctx) -> (Option<String>, Option<String>) {
    let h = ctx.config.hostname.trim();
    // "xbnc" (the default) or a bare machine name is no address for others.
    let public = (h.contains('.') || h.contains(':')).then(|| h.to_string());
    let local = crate::netinfo::primary_ip(false)
        .filter(|ip| crate::netinfo::is_local_network(*ip) && !ip.is_loopback())
        .map(|ip| ip.to_string())
        .filter(|ip| public.as_deref() != Some(ip.as_str()));
    (public, local)
}

/// How to connect an IRC client: the hosts, and each listener with the
/// names its certificate is valid for (the others need "accept invalid
/// certificates" in the client).
fn irc_connect(ctx: &Ctx) -> Value {
    let (public, local) = connect_hosts(ctx);
    let names: Vec<String> = public.iter().chain(local.iter()).cloned().collect();
    json!({
        "public_host": public,
        "local_host": local,
        "listeners": irc_endpoints_with(ctx, &names),
    })
}

/// IRC listeners as shown to users in "how to connect" hints.
fn irc_endpoints(ctx: &Ctx) -> Vec<Value> {
    let (public, local) = connect_hosts(ctx);
    let names: Vec<String> = public.iter().chain(local.iter()).cloned().collect();
    irc_endpoints_with(ctx, &names)
}

fn irc_endpoints_with(ctx: &Ctx, names: &[String]) -> Vec<Value> {
    ctx.config
        .listen
        .iter()
        .filter_map(|l| {
            let addr: SocketAddr = l.address.parse().ok()?;
            // A loopback listener is for local web clients, not for users.
            if addr.ip().is_loopback() {
                return None;
            }
            let trusted = l
                .tls_cert
                .as_deref()
                .map(|c| crate::server::cert_trusted_names(&ctx.tls_roots, c, names))
                .unwrap_or_default();
            Some(json!({
                "address": l.address,
                "port": addr.port().to_string(),
                "tls": l.tls_cert.is_some(),
                "lan_only": l.lan_only,
                "trusted_names": trusted,
            }))
        })
        .collect()
}

// --------------------------------------------------------------- public

async fn health(State(st): State<AppState>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": crate::network::VERSION,
        "language": st.ctx.settings.get().general.language,
    }))
}

/// Languages of the panel and of xBNC's IRC messages (public: the sign-in
/// page offers them).
async fn i18n_languages(State(st): State<AppState>) -> Json<Value> {
    Json(json!({
        "default": st.ctx.settings.get().general.language,
        "languages": crate::i18n::languages(),
    }))
}

/// The panel texts of one language (English text → translation).
async fn i18n_texts(
    Path(code): Path<String>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let texts = crate::i18n::panel_texts(&code).ok_or_else(|| ApiError::not_found("language"))?;
    Ok((
        [(axum::http::header::CACHE_CONTROL, "no-cache")],
        Json(texts),
    ))
}

async fn setup_status(State(st): State<AppState>) -> Json<Value> {
    let mut needed = st.ctx.setup_token.lock().expect("setup lock").is_some();
    if needed {
        // An administrator may have been created with the CLI meanwhile.
        if let Ok(n) = st.ctx.db.call(|c| db::count_users(c)).await {
            if n > 0 {
                *st.ctx.setup_token.lock().expect("setup lock") = None;
                let _ = std::fs::remove_file(crate::server::setup_code_path(&st.ctx.config));
                needed = false;
            }
        }
    }
    Json(json!({ "needed": needed }))
}

#[derive(Deserialize)]
struct SetupReq {
    token: String,
    username: String,
    password: String,
}

async fn setup(
    State(st): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<SetupReq>,
) -> ApiResult<Response> {
    let ip = client_ip(&st.cfg, peer, &headers);
    if st.ctx.limiter.is_blocked(ip) {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "blocked",
            "Too many failed attempts; try again later",
        ));
    }
    let expected = st.ctx.setup_token.lock().expect("setup lock").clone();
    let Some(expected) = expected else {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "already_setup",
            "This bouncer is already set up",
        ));
    };
    let normalise = |s: &str| s.trim().to_ascii_uppercase().replace(' ', "");
    if !super::ct_eq(&normalise(&req.token), &normalise(&expected)) {
        st.ctx.limiter.record_failure(ip);
        return Err(ApiError::unauthorized(
            "bad_setup_token",
            "The setup code is not correct",
        ));
    }
    crypto::check_password_policy(&req.password)?;
    let hash = crypto::hash_password(&req.password)?;
    let name = req.username.clone();
    let user = st
        .ctx
        .db
        .call(move |c| {
            if db::count_users(c)? > 0 {
                anyhow::bail!("This bouncer is already set up");
            }
            let id = db::create_user_with_role(c, &name, &hash, "superadmin")?;
            db::audit(
                c,
                &format!("web:{name}"),
                "setup",
                "first administrator created",
            )?;
            Ok(db::get_user_by_id(c, id)?.expect("just created"))
        })
        .await?;
    *st.ctx.setup_token.lock().expect("setup lock") = None;
    let _ = std::fs::remove_file(crate::server::setup_code_path(&st.ctx.config));
    tracing::info!(user = %user.username, "first administrator created from the web panel");
    start_session(&st, user, ip, &headers).await
}

#[derive(Deserialize)]
struct LoginReq {
    username: String,
    password: String,
    totp_code: Option<String>,
}

pub(super) async fn record_web_login(
    ctx: &Ctx,
    ip: std::net::IpAddr,
    username: &str,
    ok: bool,
    reason: &'static str,
) {
    let name = username.to_string();
    let ip = ip.to_string();
    let _ = ctx
        .db
        .call(move |c| db::record_login(c, &ip, &name, "web", ok, reason))
        .await;
    crate::metrics::Metrics::add(
        if ok {
            &ctx.metrics.client_logins
        } else {
            &ctx.metrics.failed_logins
        },
        1,
    );
}

async fn login(
    State(st): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<LoginReq>,
) -> ApiResult<Response> {
    let ip = client_ip(&st.cfg, peer, &headers);
    if st.ctx.limiter.is_blocked(ip) {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "blocked",
            st.ctx.limiter.blocked_message(ip),
        ));
    }
    let user = match crate::auth::authenticate_ctx(
        &st.ctx,
        req.username.clone(),
        Secret::new(req.password),
    )
    .await?
    {
        crate::auth::AuthResult::Ok(u) => *u,
        crate::auth::AuthResult::Disabled => {
            record_web_login(&st.ctx, ip, &req.username, false, "account disabled").await;
            return Err(ApiError::unauthorized(
                "account_disabled",
                "This account is disabled",
            ));
        }
        crate::auth::AuthResult::BadCredentials => {
            st.ctx.limiter.record_failure(ip);
            record_web_login(&st.ctx, ip, &req.username, false, "bad password").await;
            return Err(ApiError::unauthorized(
                "bad_credentials",
                "Wrong username or password",
            ));
        }
    };
    if user.totp_enabled {
        let Some(code) = req.totp_code.filter(|c| !c.trim().is_empty()) else {
            return Err(ApiError::unauthorized(
                "totp_required",
                "Enter the code from your authenticator app",
            ));
        };
        let secret = user
            .totp_secret_enc
            .as_deref()
            .and_then(|e| st.ctx.secrets.decrypt(e).ok())
            .ok_or_else(|| {
                ApiError::bad("2FA is misconfigured for this account; ask an administrator")
            })?;
        if !totp::verify(secret.expose(), &code, totp::now_unix()) {
            st.ctx.limiter.record_failure(ip);
            record_web_login(&st.ctx, ip, &user.username, false, "bad 2FA code").await;
            return Err(ApiError::unauthorized(
                "bad_totp",
                "The verification code is not correct",
            ));
        }
    }
    st.ctx.limiter.record_success(ip);
    record_web_login(&st.ctx, ip, &user.username, true, "ok").await;
    start_session(&st, user, ip, &headers).await
}

pub(super) async fn start_session(
    st: &AppState,
    user: User,
    ip: std::net::IpAddr,
    headers: &HeaderMap,
) -> ApiResult<Response> {
    let token = random_token();
    let csrf = random_token();
    let hash = hash_token(&token);
    let ua: String = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .chars()
        .take(200)
        .collect();
    let ttl = st.cfg.session_hours as i64 * 3_600_000;
    let uid = user.id;
    let (csrf2, ip2) = (csrf.clone(), ip.to_string());
    let session_id = st
        .ctx
        .db
        .call(move |c| {
            db::set_last_login(c, uid)?;
            db::create_session(c, &hash, uid, &csrf2, &ip2, &ua, ttl)
        })
        .await?;
    let session = st
        .ctx
        .db
        .call(move |c| db::get_session(c, session_id))
        .await?
        .expect("session exists");
    let auth = Auth {
        role: Role::from_db(&user.role),
        user,
        session,
        ip,
        token: None,
    };
    let mut resp = Json(me_json(st, &auth)).into_response();
    resp.headers_mut()
        .insert(header::SET_COOKIE, session_cookie(&st.cfg, &token));
    Ok(resp)
}

async fn logout(State(st): State<AppState>, a: Auth) -> ApiResult<Response> {
    a.require_session()?;
    let id = a.session.id;
    st.ctx.db.call(move |c| db::delete_session(c, id)).await?;
    let mut resp = Json(json!({ "ok": true })).into_response();
    resp.headers_mut()
        .insert(header::SET_COOKIE, clear_cookie(&st.cfg));
    Ok(resp)
}

async fn me(State(st): State<AppState>, a: Auth) -> Json<Value> {
    let mut v = me_json(&st, &a);
    let uid = a.user.id;
    let n = st
        .ctx
        .db
        .call(move |c| db::count_networks(c, uid))
        .await
        .unwrap_or(0);
    v["user"]["networks"] = json!(n);
    let ver = st
        .ctx
        .db
        .call(move |c| Ok(db::get_avatar(c, uid)?.map(|a| a.2)))
        .await
        .unwrap_or(None);
    v["user"]["avatar"] = json!(avatar_url(uid, ver));
    Json(v)
}

// ------------------------------------------------------------- dashboard

async fn visible_networks(ctx: &Ctx, a: &Auth, all: bool) -> ApiResult<Vec<(Network, String)>> {
    let uid = a.user.id;
    let see_all = all && a.role.can(Perm::ManageAllNetworks);
    Ok(ctx
        .db
        .call(move |c| {
            let users = db::list_users(c)?;
            let mut out = Vec::new();
            for u in users.iter().filter(|u| see_all || u.id == uid) {
                for n in db::list_networks(c, u.id)? {
                    out.push((n, u.username.clone()));
                }
            }
            Ok(out)
        })
        .await?)
}

async fn dashboard(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    let ctx = &st.ctx;
    let global = a.role.can(Perm::ViewSystem);
    let nets = visible_networks(ctx, &a, global).await?;
    let mut networks = Vec::new();
    let (mut connected, mut channels, mut clients) = (0, 0, 0);
    for (n, owner) in &nets {
        let status = if n.enabled {
            server::network_status(ctx, n.id).await
        } else {
            None
        };
        if let Some(s) = &status {
            if s.state == "connected" {
                connected += 1;
            }
            channels += s.channels.len();
            clients += s.clients.len();
        }
        networks.push(json!({
            "id": n.id,
            "name": n.name,
            "owner": owner,
            "host": n.host,
            "enabled": n.enabled,
            "state": status.as_ref().map(|s| s.state).unwrap_or("disabled"),
            "nick": status.as_ref().map(|s| s.nick.clone()).unwrap_or_else(|| n.nick.clone()),
            "channels": status.as_ref().map(|s| s.channels.len()).unwrap_or(0),
            "clients": status.as_ref().map(|s| s.clients.len()).unwrap_or(0),
            "last_error": status.as_ref().and_then(|s| s.last_error.clone()),
        }));
    }
    let uid = a.user.id;
    let (messages, db_size, users, failures_24h) = ctx
        .db
        .call(move |c| {
            let (m, size, _) = db::storage_stats(c)?;
            let mine = db::messages_per_user(c, uid)?;
            let users = db::count_users(c)?;
            let f = db::count_failed_logins_since(c, db::now_ms() - 86_400_000)?;
            Ok((if global { m } else { mine }, size, users, f))
        })
        .await?;
    let mut v = json!({
        "scope": if global { "global" } else { "personal" },
        "version": crate::network::VERSION,
        "uptime_seconds": (db::now_ms() - ctx.started_at) / 1000,
        "networks": networks,
        "networks_connected": connected,
        "channels": channels,
        "clients": clients,
        "messages_stored": messages,
    });
    if global {
        v["system"] = json!({
            "users": users,
            "db_size_bytes": db_size,
            "rss_bytes": crate::metrics::rss_bytes(),
            "connections": ctx.connections.load(std::sync::atomic::Ordering::Relaxed),
            "irc_clients": ctx.registry.total_clients(),
            "failed_logins_24h": failures_24h,
            "blocked_ips": ctx.limiter.blocked().len(),
            "counters": ctx.metrics.snapshot(),
        });
    }
    Ok(Json(v))
}

/// Well-known networks for the "add network" wizard.
async fn presets() -> Json<Value> {
    Json(json!([
        { "name": "libera", "label": "Libera.Chat", "host": "irc.libera.chat", "port": 6697, "tls": true, "sasl": true, "description": "Free software communities and open source projects" },
        { "name": "oftc", "label": "OFTC", "host": "irc.oftc.net", "port": 6697, "tls": true, "sasl": false, "description": "Debian, Tor, the Linux kernel and other projects",
          "service": { "bot": "NickServ", "template": "/msg NickServ IDENTIFY {pass} {user}" } },
        { "name": "efnet", "label": "EFnet", "host": "irc.efnet.org", "port": 6697, "tls": true, "sasl": false, "description": "One of the oldest networks" },
        { "name": "ircnet", "label": "IRCnet", "host": "open.ircnet.net", "port": 6697, "tls": true, "sasl": false, "description": "Historic network, very popular in Europe" },
        { "name": "freenode", "label": "freenode", "host": "irc.freenode.net", "port": 6697, "tls": true, "sasl": true, "description": "Historic network of projects and communities" },
        { "name": "evilnet", "label": "EvilNet", "host": "irc.evilnet.org", "port": 6697, "tls": true, "sasl": false, "description": "Network with NickServ services",
          "service": { "bot": "NickServ", "template": "/msg NickServ IDENTIFY {user} {pass}" } },
        { "name": "rizon", "label": "Rizon", "host": "irc.rizon.net", "port": 6697, "tls": true, "sasl": true, "description": "Anime, games and all kinds of communities" },
        { "name": "undernet", "label": "Undernet", "host": "irc.undernet.org", "port": 6667, "tls": false, "sasl": false, "description": "Classic network with all kinds of channels",
          "service": { "bot": "X", "server_password": "+x! {user} {pass}" } },
        { "name": "quakenet", "label": "QuakeNet", "host": "irc.quakenet.org", "port": 6667, "tls": false, "sasl": false, "description": "Games and European communities",
          "service": { "bot": "Q", "template": "/msg Q@CServe.quakenet.org AUTH {user} {pass}\n/mode $nick +x" } },
        { "name": "dalnet", "label": "DALnet", "host": "irc.dal.net", "port": 6697, "tls": true, "sasl": true, "description": "Social and support communities" },
        { "name": "hackint", "label": "hackint", "host": "irc.hackint.org", "port": 6697, "tls": true, "sasl": true, "description": "Hackerspaces and technical communities" }
    ]))
}

// --------------------------------------------------------------- networks

/// Loads a network the caller may see; `write` requires edit rights.
pub(super) async fn load_network_pub(
    ctx: &Ctx,
    a: &Auth,
    id: i64,
    write: bool,
) -> ApiResult<(Network, String)> {
    load_network(ctx, a, id, write).await
}

async fn load_network(ctx: &Ctx, a: &Auth, id: i64, write: bool) -> ApiResult<(Network, String)> {
    let found = ctx
        .db
        .call(move |c| {
            let Some(n) = db::get_network_by_id(c, id)? else {
                return Ok(None);
            };
            let owner = db::get_user_by_id(c, n.user_id)?
                .map(|u| u.username)
                .unwrap_or_default();
            Ok(Some((n, owner)))
        })
        .await?;
    let Some((n, owner)) = found else {
        return Err(ApiError::not_found("Network"));
    };
    let mine = n.user_id == a.user.id;
    let allowed = if write {
        (mine && a.role.can(Perm::ManageOwnNetworks)) || a.role.can(Perm::ManageAllNetworks)
    } else {
        mine || a.role.can(Perm::ManageAllNetworks)
    };
    if !allowed {
        // Do not reveal other users' networks to people who cannot see them.
        return Err(if mine || a.role.can(Perm::ManageAllNetworks) {
            ApiError::forbidden()
        } else {
            ApiError::not_found("Network")
        });
    }
    Ok((n, owner))
}

#[derive(Deserialize)]
struct ListNetworksQuery {
    #[serde(default)]
    all: bool,
}

async fn list_networks(
    State(st): State<AppState>,
    a: Auth,
    Query(q): Query<ListNetworksQuery>,
) -> ApiResult<Json<Vec<NetworkView>>> {
    let nets = visible_networks(&st.ctx, &a, q.all).await?;
    let mut out = Vec::new();
    for (n, owner) in nets {
        let status = if n.enabled {
            server::network_status(&st.ctx, n.id).await
        } else {
            None
        };
        out.push(network_view(&n, &owner, status));
    }
    Ok(Json(out))
}

async fn get_network(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Json<NetworkView>> {
    let (n, owner) = load_network(&st.ctx, &a, id, false).await?;
    let status = if n.enabled {
        server::network_status(&st.ctx, id).await
    } else {
        None
    };
    Ok(Json(network_view(&n, &owner, status)))
}

#[derive(Deserialize)]
struct CreateNetworkReq {
    user_id: Option<i64>,
    name: String,
    host: String,
    port: u16,
    #[serde(default = "yes")]
    tls: bool,
    nick: String,
    username: Option<String>,
    realname: Option<String>,
    server_password: Option<String>,
    sasl_username: Option<String>,
    sasl_password: Option<String>,
    #[serde(default)]
    channels: Vec<String>,
    /// Commands sent after connecting (see `perform`).
    on_connect: Option<String>,
}

fn yes() -> bool {
    true
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn valid_channel(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && name.starts_with(['#', '&', '!', '+'])
        && !name.contains([' ', ',', '\x07'])
}

async fn create_network(
    State(st): State<AppState>,
    a: Auth,
    Json(req): Json<CreateNetworkReq>,
) -> ApiResult<(StatusCode, Json<NetworkView>)> {
    let ctx = st.ctx.clone();
    let target = req.user_id.unwrap_or(a.user.id);
    if target == a.user.id {
        a.require(Perm::ManageOwnNetworks)?;
    } else {
        a.require(Perm::ManageAllNetworks)?;
    }
    let sasl_user = non_empty(req.sasl_username);
    let sasl_pass = non_empty(req.sasl_password);
    if sasl_user.is_some() != sasl_pass.is_some() {
        return Err(ApiError::bad(
            "SASL needs both an account name and a password",
        ));
    }
    for ch in &req.channels {
        if !valid_channel(ch) {
            return Err(ApiError::bad(format!("Invalid channel name: {ch}")));
        }
    }
    let on_connect = non_empty(req.on_connect.clone());
    if let Some(t) = &on_connect {
        crate::perform::validate(t)
            .map_err(|e| ApiError::bad(format!("Commands on connect: {e}")))?;
    }
    let on_connect_enc = on_connect.map(|t| ctx.secrets.encrypt(&t));
    let nick = req.nick.trim().to_string();
    let nn = NewNetwork {
        name: req.name.trim().to_string(),
        host: req.host.trim().to_string(),
        port: req.port,
        tls: req.tls,
        username: non_empty(req.username).unwrap_or_else(|| nick.clone()),
        realname: non_empty(req.realname).unwrap_or_else(|| nick.clone()),
        nick,
        server_password_enc: non_empty(req.server_password).map(|p| ctx.secrets.encrypt(&p)),
        sasl_username: sasl_user,
        sasl_password_enc: sasl_pass.map(|p| ctx.secrets.encrypt(&p)),
    };
    let default_max = ctx.settings.get().limits.max_networks_per_user;
    let channels = req.channels.clone();
    let nn2 = nn.clone();
    let (net, owner) = ctx
        .db
        .call(move |c| {
            let owner =
                db::get_user_by_id(c, target)?.ok_or_else(|| anyhow::anyhow!("User not found"))?;
            let max = owner
                .max_networks
                .map(|m| m.max(0) as usize)
                .unwrap_or(default_max);
            let id = db::create_network(c, target, &nn2, max)?;
            if let Some(enc) = &on_connect_enc {
                db::set_on_connect(c, id, Some(enc))?;
            }
            let cm = xbnc_proto::CaseMapping::Rfc1459;
            for ch in &channels {
                db::upsert_channel(c, id, ch, &cm.fold(ch), None, false)?;
            }
            Ok((
                db::get_network_by_id(c, id)?.expect("created"),
                owner.username,
            ))
        })
        .await?;
    audit(
        &ctx,
        &a.user,
        "network.create",
        format!(
            "{owner}/{} {}:{} tls={}",
            net.name, net.host, net.port, net.tls
        ),
    )
    .await;
    server::sync_networks(&ctx).await?;
    Ok((StatusCode::CREATED, Json(network_view(&net, &owner, None))))
}

/// PATCH body. Secrets: omit to keep, `""` to clear, a value to replace.
#[derive(Deserialize, Default)]
struct UpdateNetworkReq {
    name: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    tls: Option<bool>,
    nick: Option<String>,
    username: Option<String>,
    realname: Option<String>,
    enabled: Option<bool>,
    server_password: Option<String>,
    sasl_username: Option<String>,
    sasl_password: Option<String>,
}

async fn update_network(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
    Json(req): Json<UpdateNetworkReq>,
) -> ApiResult<Json<NetworkView>> {
    let ctx = st.ctx.clone();
    let (old, owner) = load_network(&ctx, &a, id, true).await?;
    let secret = |v: Option<String>| -> Option<Option<String>> {
        v.map(|s| {
            if s.is_empty() {
                None
            } else {
                Some(ctx.secrets.encrypt(&s))
            }
        })
    };
    let upd = NetworkUpdate {
        name: req.name.map(|s| s.trim().to_string()),
        host: req.host.map(|s| s.trim().to_string()),
        port: req.port,
        tls: req.tls,
        nick: req.nick.map(|s| s.trim().to_string()),
        username: req.username.map(|s| s.trim().to_string()),
        realname: req.realname,
        enabled: req.enabled,
        server_password_enc: secret(req.server_password),
        sasl_username: req
            .sasl_username
            .map(|s| Some(s.trim().to_string()).filter(|v| !v.is_empty())),
        sasl_password_enc: secret(req.sasl_password),
    };
    let new = ctx
        .db
        .call(move |c| db::update_network(c, id, &upd))
        .await?
        .ok_or_else(|| ApiError::not_found("Network"))?;
    let mut changes = Vec::new();
    macro_rules! diff {
        ($f:ident) => {
            if old.$f != new.$f {
                changes.push(stringify!($f));
            }
        };
    }
    diff!(name);
    diff!(host);
    diff!(port);
    diff!(tls);
    diff!(nick);
    diff!(username);
    diff!(realname);
    diff!(enabled);
    diff!(server_password_enc);
    diff!(sasl_username);
    diff!(sasl_password_enc);
    audit(
        &ctx,
        &a.user,
        "network.update",
        format!("{owner}/{} changed: {}", new.name, changes.join(", ")),
    )
    .await;
    let connection_changed = changes.iter().any(|c| !matches!(*c, "enabled" | "name"));
    if old.name != new.name {
        server::rename_network(&ctx, id, &new.name).await?;
    }
    if old.enabled != new.enabled {
        server::sync_networks(&ctx).await?;
    } else if connection_changed {
        // Connection settings changed: reconnect so they take effect now.
        if let Some(h) = server::network_handle(&ctx, id) {
            let _ = h.tx.send(NetCmd::Reload).await;
            let _ = h.tx.send(NetCmd::Control(NetAction::Jump)).await;
        }
    }
    let status = if new.enabled {
        server::network_status(&ctx, id).await
    } else {
        None
    };
    Ok(Json(network_view(&new, &owner, status)))
}

#[derive(Serialize, Deserialize)]
struct OnConnect {
    commands: String,
}

/// The commands are shown decrypted to whoever may edit the network: they
/// are meant to be read and edited like a client's perform list.
async fn get_on_connect(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Json<OnConnect>> {
    load_network(&st.ctx, &a, id, true).await?;
    let enc = st.ctx.db.call(move |c| db::get_on_connect(c, id)).await?;
    let commands = match enc {
        Some(e) => st.ctx.secrets.decrypt(&e)?.expose().to_string(),
        None => String::new(),
    };
    Ok(Json(OnConnect { commands }))
}

async fn put_on_connect(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
    Json(req): Json<OnConnect>,
) -> ApiResult<Json<OnConnect>> {
    let (n, owner) = load_network(&st.ctx, &a, id, true).await?;
    let text = req.commands.trim().to_string();
    crate::perform::validate(&text)
        .map_err(|e| ApiError::bad(format!("Commands on connect: {e}")))?;
    let enc = (!text.is_empty()).then(|| st.ctx.secrets.encrypt(&text));
    st.ctx
        .db
        .call(move |c| db::set_on_connect(c, id, enc.as_deref()))
        .await?;
    let count = text.lines().filter(|l| !l.trim().is_empty()).count();
    audit(
        &st.ctx,
        &a.user,
        "network.on_connect",
        format!("{owner}/{} commands on connect: {count}", n.name),
    )
    .await;
    Ok(Json(OnConnect { commands: text }))
}

// ---------------------------------------------------------------- modules

async fn module_catalog(_a: Auth) -> Json<Value> {
    Json(json!({
        "modules": crate::modules::MODULES,
        "builtin": crate::modules::BUILTIN,
    }))
}

async fn network_modules(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    load_network(&st.ctx, &a, id, false).await?;
    let rows = st.ctx.db.call(move |c| db::module_rows(c, id)).await?;
    let list: Vec<Value> = crate::modules::effective(&rows)
        .into_iter()
        .map(|(m, s)| {
            let (config, secrets_set) = crate::modules::redact(m.id, &s.config);
            json!({ "id": m.id, "enabled": s.enabled, "config": config, "secrets_set": secrets_set })
        })
        .collect();
    Ok(Json(json!({ "modules": list })))
}

#[derive(Deserialize)]
struct ModuleReq {
    enabled: bool,
    #[serde(default)]
    config: serde_json::Map<String, Value>,
}

async fn put_network_module(
    State(st): State<AppState>,
    a: Auth,
    Path((id, module)): Path<(i64, String)>,
    Json(req): Json<ModuleReq>,
) -> ApiResult<Json<Value>> {
    let (n, owner) = load_network(&st.ctx, &a, id, true).await?;
    crate::modules::validate_plain(&module, &req.config)
        .map_err(|e| ApiError::bad(format!("{module}: {e}")))?;
    let m3 = module.clone();
    let old = st
        .ctx
        .db
        .call(move |c| db::module_rows(c, id))
        .await?
        .remove(&m3)
        .map(|r| r.config);
    let secrets = st.ctx.secrets.clone();
    let sealed = crate::modules::seal(&module, &req.config, old.as_ref(), &|p| secrets.encrypt(p));
    let config = crate::modules::validate(&module, &sealed)
        .map_err(|e| ApiError::bad(format!("{module}: {e}")))?;
    let state = crate::modules::ModState {
        enabled: req.enabled,
        config,
    };
    let saved = state.clone();
    let m2 = module.clone();
    st.ctx
        .db
        .call(move |c| db::set_module(c, id, &m2, &saved))
        .await?;
    if let Some(h) = crate::server::network_handle(&st.ctx, id) {
        let _ = h.tx.send(NetCmd::Reload).await;
    }
    audit(
        &st.ctx,
        &a.user,
        "network.module",
        format!(
            "{owner}/{} {module} {}",
            n.name,
            if state.enabled { "on" } else { "off" }
        ),
    )
    .await;
    let (config, secrets_set) = crate::modules::redact(&module, &state.config);
    Ok(Json(
        json!({ "id": module, "enabled": state.enabled, "config": config, "secrets_set": secrets_set }),
    ))
}

// ------------------------------------------------------- client certificate

async fn cert_view(ctx: &Ctx, id: i64) -> ApiResult<Value> {
    let stored = ctx.db.call(move |c| db::get_client_cert(c, id)).await?;
    Ok(match stored {
        Some((cert, _)) => json!({
            "present": true,
            "fingerprints": crate::certs::fingerprints(&cert).ok(),
            "certificate": cert,
        }),
        None => json!({ "present": false }),
    })
}

async fn get_cert(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    load_network(&st.ctx, &a, id, true).await?;
    Ok(Json(cert_view(&st.ctx, id).await?))
}

async fn store_cert(
    st: &AppState,
    a: &Auth,
    id: i64,
    cert: String,
    key: String,
    how: &str,
) -> ApiResult<Json<Value>> {
    let (n, owner) = load_network(&st.ctx, a, id, true).await?;
    crate::certs::parse(&cert, &key).map_err(|e| ApiError::bad(e.to_string()))?;
    let key_enc = st.ctx.secrets.encrypt(&key);
    st.ctx
        .db
        .call(move |c| db::set_client_cert(c, id, Some((&cert, &key_enc))))
        .await?;
    audit(
        &st.ctx,
        &a.user,
        "network.cert",
        format!("{owner}/{} {how}", n.name),
    )
    .await;
    Ok(Json(cert_view(&st.ctx, id).await?))
}

async fn generate_cert(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let (n, _) = load_network(&st.ctx, &a, id, true).await?;
    let (cert, key) = crate::certs::generate(&n.nick).map_err(ApiError::from)?;
    store_cert(&st, &a, id, cert, key, "generated").await
}

#[derive(Deserialize)]
struct CertReq {
    certificate: String,
    private_key: String,
}

async fn put_cert(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
    Json(req): Json<CertReq>,
) -> ApiResult<Json<Value>> {
    store_cert(&st, &a, id, req.certificate, req.private_key, "uploaded").await
}

async fn delete_cert(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let (n, owner) = load_network(&st.ctx, &a, id, true).await?;
    st.ctx
        .db
        .call(move |c| db::set_client_cert(c, id, None))
        .await?;
    audit(
        &st.ctx,
        &a.user,
        "network.cert",
        format!("{owner}/{} removed", n.name),
    )
    .await;
    Ok(Json(json!({ "present": false })))
}

// ------------------------------------------------------------------ notes

async fn list_notes(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    let uid = a.user.id;
    let notes = st.ctx.db.call(move |c| db::list_notes(c, uid)).await?;
    Ok(Json(json!({
        "notes": notes
            .into_iter()
            .map(|(k, t, ts)| json!({ "key": k, "text": t, "updated_at": ts }))
            .collect::<Vec<_>>()
    })))
}

#[derive(Deserialize)]
struct NoteReq {
    text: String,
}

fn valid_note_key(k: &str) -> bool {
    !k.is_empty()
        && k.len() <= 32
        && k.chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

async fn put_note(
    State(st): State<AppState>,
    a: Auth,
    Path(key): Path<String>,
    Json(req): Json<NoteReq>,
) -> ApiResult<Json<Value>> {
    let text = req.text.trim().to_string();
    if !valid_note_key(&key) {
        return Err(ApiError::bad(
            "Note keys are up to 32 letters, digits, - or _",
        ));
    }
    if text.is_empty() || text.chars().count() > 400 || text.chars().any(char::is_control) {
        return Err(ApiError::bad(
            "Note text must be 1 to 400 characters on one line",
        ));
    }
    let uid = a.user.id;
    let (k, t) = (key.clone(), text.clone());
    st.ctx
        .db
        .call(move |c| db::set_note(c, uid, &k, &t))
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    Ok(Json(json!({ "key": key, "text": text })))
}

async fn delete_note(
    State(st): State<AppState>,
    a: Auth,
    Path(key): Path<String>,
) -> ApiResult<Json<Value>> {
    let uid = a.user.id;
    let found = st
        .ctx
        .db
        .call(move |c| db::delete_note(c, uid, &key))
        .await?;
    if !found {
        return Err(ApiError::not_found("Note"));
    }
    Ok(Json(json!({ "ok": true })))
}

// ------------------------------------------------------------- API tokens

async fn list_tokens(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    a.require_session()?;
    let uid = a.user.id;
    let tokens = st.ctx.db.call(move |c| db::list_tokens(c, uid)).await?;
    Ok(Json(
        json!({ "tokens": tokens, "scopes": super::TOKEN_SCOPES }),
    ))
}

#[derive(Deserialize)]
struct TokenReq {
    name: String,
    scope: String,
    /// Days until it expires; 0 means never.
    #[serde(default = "default_token_days")]
    expires_days: u32,
}

fn default_token_days() -> u32 {
    90
}

async fn create_token(
    State(st): State<AppState>,
    a: Auth,
    Json(req): Json<TokenReq>,
) -> ApiResult<Json<Value>> {
    a.require_session()?;
    let name = req.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 60 || name.chars().any(char::is_control) {
        return Err(ApiError::bad("Token names are 1 to 60 characters"));
    }
    if !super::TOKEN_SCOPES.contains(&req.scope.as_str()) {
        return Err(ApiError::bad("Scope must be read, write or metrics"));
    }
    if req.expires_days > 3650 {
        return Err(ApiError::bad("A token can last at most 3650 days"));
    }
    let secret = format!("xbnc_{}", super::random_token());
    let hash = super::hash_token(&secret);
    let prefix: String = secret.chars().take(12).collect();
    let expires =
        (req.expires_days > 0).then(|| db::now_ms() + req.expires_days as i64 * 86_400_000);
    let uid = a.user.id;
    let (n2, s2, p2) = (name.clone(), req.scope.clone(), prefix.clone());
    let id = st
        .ctx
        .db
        .call(move |c| db::create_token(c, uid, &n2, &hash, &p2, &s2, expires))
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    audit(
        &st.ctx,
        &a.user,
        "token.create",
        format!("{name} ({})", req.scope),
    )
    .await;
    Ok(Json(json!({
        "id": id,
        "name": name,
        "scope": req.scope,
        "prefix": prefix,
        "expires_at": expires,
        // Shown once: only its hash is stored.
        "token": secret,
    })))
}

async fn delete_token(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    a.require_session()?;
    let uid = a.user.id;
    if !st
        .ctx
        .db
        .call(move |c| db::delete_token(c, uid, id))
        .await?
    {
        return Err(ApiError::not_found("Token"));
    }
    audit(&st.ctx, &a.user, "token.delete", format!("token {id}")).await;
    Ok(Json(json!({ "ok": true })))
}

async fn list_alerts(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    a.require(Perm::ViewSystem)?;
    Ok(Json(json!({ "alerts": st.ctx.alerts.active() })))
}

#[derive(Deserialize)]
struct ImportReq {
    /// The other bouncer's configuration file.
    config: String,
}

/// Importing creates accounts (including administrators), so only a
/// superadministrator may do it, from a panel session.
fn require_import(a: &Auth) -> ApiResult<()> {
    a.require_session()?;
    a.require(Perm::ManageUsers)?;
    a.require(Perm::EditSettings)?;
    Ok(())
}

async fn import_preview(
    State(st): State<AppState>,
    a: Auth,
    Json(req): Json<ImportReq>,
) -> ApiResult<Json<Value>> {
    require_import(&a)?;
    let plan = st
        .ctx
        .db
        .call(move |c| crate::import::parse(&req.config, None, c))
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    Ok(Json(json!({ "plan": plan })))
}

async fn import_apply(
    State(st): State<AppState>,
    a: Auth,
    Json(req): Json<ImportReq>,
) -> ApiResult<Json<Value>> {
    require_import(&a)?;
    let max = st.ctx.settings.get().limits.max_networks_per_user;
    let ctx = st.ctx.clone();
    let report = st
        .ctx
        .db
        .call(move |c| {
            let plan = crate::import::parse(&req.config, None, c)?;
            crate::import::apply(c, &plan, &ctx.secrets, max)
        })
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    audit(
        &st.ctx,
        &a.user,
        "import",
        format!(
            "{} users, {} networks, {} channels",
            report.users_created.len(),
            report.networks_created,
            report.channels_created
        ),
    )
    .await;
    server::sync_networks(&st.ctx).await?;
    Ok(Json(json!({ "report": report })))
}

fn updates_json(st: &AppState, s: crate::updates::UpdateState) -> Value {
    let current = crate::network::VERSION;
    let newer = s
        .latest
        .as_deref()
        .is_some_and(|l| crate::updates::is_newer(l, current));
    json!({
        "current": current,
        "configured": st.ctx.config.updates.source().is_some(),
        "latest": s.latest,
        "newer": newer,
        "url": s.url,
        "notes": s.notes,
        "checked_at": s.checked_at,
        "error": s.error,
        "one_click": crate::updates::one_click_blocker(&st.ctx).is_none(),
        "one_click_blocker": crate::updates::one_click_blocker(&st.ctx),
        "installing": crate::updates::spool_dir(&st.ctx).join("pending.json").exists(),
        "last_result": crate::updates::last_result(&st.ctx),
    })
}

async fn updates_status(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    a.require(Perm::ViewSystem)?;
    let s = st.ctx.updates.state();
    Ok(Json(updates_json(&st, s)))
}

async fn updates_check(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    a.require_session()?;
    a.require(Perm::ViewSystem)?;
    if st.ctx.config.updates.source().is_none() {
        return Err(ApiError::bad(
            "Update checks are not configured: set [updates] repository in xbnc.toml",
        ));
    }
    let s = crate::updates::check_now(&st.ctx).await;
    Ok(Json(updates_json(&st, s)))
}

/// Downloads, verifies and queues the newest signed package. The system
/// helper (`xbnc-update.service`, run as root) verifies it again and
/// installs it; the service restarts afterwards.
async fn updates_install(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    require_import(&a)?;
    if crate::updates::spool_dir(&st.ctx)
        .join("pending.json")
        .exists()
    {
        return Err(ApiError::bad("An update is already being installed"));
    }
    let version = crate::updates::queue_install(&st.ctx, &a.user.username)
        .await
        .map_err(|e| ApiError::bad(format!("Cannot update: {e:#}")))?;
    super::audit(
        &st.ctx,
        &a.user,
        "update.install",
        format!("version {version}"),
    )
    .await;
    Ok(Json(json!({ "queued": version })))
}

/// Prometheus text exposition (format 0.0.4). Needs an account that can
/// view the system: a session, or an API token of any scope (a `metrics`
/// token can read nothing else).
async fn prometheus(State(st): State<AppState>, a: Auth) -> ApiResult<Response> {
    a.require(Perm::ViewSystem)?;
    let ctx = &st.ctx;
    let (users, (messages, db_bytes, networks)) = ctx
        .db
        .call(|c| Ok((db::count_users(c)?, db::storage_stats(c)?)))
        .await?;
    let mut states: std::collections::BTreeMap<&'static str, u64> = Default::default();
    for s in ["connected", "connecting", "disconnected", "reconnecting"] {
        states.insert(s, 0);
    }
    for h in ctx.registry.handles() {
        let state = crate::server::network_status(ctx, h.network_id)
            .await
            .map(|s| s.state)
            .unwrap_or("unknown");
        *states.entry(state).or_default() += 1;
    }
    let c = ctx.metrics.snapshot();
    let alerts = ctx.alerts.active();
    let mut out = String::with_capacity(4096);
    let mut m = |name: &str, kind: &str, help: &str, samples: &[(String, f64)]| {
        out.push_str(&format!(
            "# HELP xbnc_{name} {help}\n# TYPE xbnc_{name} {kind}\n"
        ));
        for (labels, v) in samples {
            out.push_str(&format!("xbnc_{name}{labels} {v}\n"));
        }
    };
    let one = |v: f64| vec![(String::new(), v)];
    m(
        "info",
        "gauge",
        "Version of this xBNC server.",
        &[(format!("{{version=\"{}\"}}", crate::network::VERSION), 1.0)],
    );
    m(
        "uptime_seconds",
        "gauge",
        "Seconds since xBNC started.",
        &one(((db::now_ms() - ctx.started_at) / 1000) as f64),
    );
    m("users", "gauge", "Accounts.", &one(users as f64));
    m(
        "networks",
        "gauge",
        "Configured networks.",
        &one(networks as f64),
    );
    let by_state: Vec<(String, f64)> = states
        .iter()
        .map(|(k, v)| (format!("{{state=\"{k}\"}}"), *v as f64))
        .collect();
    m(
        "networks_running",
        "gauge",
        "Running network connections by state.",
        &by_state,
    );
    m(
        "irc_clients",
        "gauge",
        "IRC clients attached.",
        &one(ctx.registry.total_clients() as f64),
    );
    m(
        "connections",
        "gauge",
        "Open IRC connections to the bouncer (including unauthenticated).",
        &one(ctx.connections.load(std::sync::atomic::Ordering::Relaxed) as f64),
    );
    m(
        "blocked_ips",
        "gauge",
        "Addresses blocked after failed logins.",
        &one(ctx.limiter.blocked().len() as f64),
    );
    let counters: [(&str, &str, u64); 9] = [
        (
            "server_bytes_received_total",
            "Bytes received from IRC servers.",
            c.bytes_from_servers,
        ),
        (
            "server_bytes_sent_total",
            "Bytes sent to IRC servers.",
            c.bytes_to_servers,
        ),
        (
            "client_bytes_received_total",
            "Bytes received from IRC clients.",
            c.bytes_from_clients,
        ),
        (
            "client_bytes_sent_total",
            "Bytes sent to IRC clients.",
            c.bytes_to_clients,
        ),
        (
            "server_lines_total",
            "Lines received from IRC servers.",
            c.lines_from_servers,
        ),
        (
            "messages_stored_total",
            "Messages written to the history.",
            c.messages_stored,
        ),
        (
            "reconnects_total",
            "Reconnections to IRC servers.",
            c.reconnects,
        ),
        (
            "client_logins_total",
            "Successful IRC client logins.",
            c.client_logins,
        ),
        (
            "failed_logins_total",
            "Failed logins (IRC and panel).",
            c.failed_logins,
        ),
    ];
    for (name, help, v) in counters {
        m(name, "counter", help, &one(v as f64));
    }
    m(
        "history_messages",
        "gauge",
        "Messages kept in the history.",
        &one(messages as f64),
    );
    m(
        "database_bytes",
        "gauge",
        "Size of the database.",
        &one(db_bytes as f64),
    );
    if let Some(rss) = crate::metrics::rss_bytes() {
        m(
            "resident_memory_bytes",
            "gauge",
            "Resident memory of the process.",
            &one(rss as f64),
        );
    }
    m(
        "log_lines_dropped_total",
        "counter",
        "Log file lines dropped because the disk was too slow.",
        &one(ctx.logs.dropped.load(std::sync::atomic::Ordering::Relaxed) as f64),
    );
    m(
        "dcc_relays_total",
        "counter",
        "DCC transfers and chats relayed.",
        &one(ctx.dcc.relays.load(std::sync::atomic::Ordering::Relaxed) as f64),
    );
    m(
        "dcc_bytes_total",
        "counter",
        "Bytes relayed for DCC.",
        &one(ctx.dcc.bytes.load(std::sync::atomic::Ordering::Relaxed) as f64),
    );
    let firing: Vec<(String, f64)> = ["network_down", "failed_logins", "memory", "database"]
        .iter()
        .map(|k| {
            let n = alerts.iter().filter(|a| a.kind == *k).count();
            (format!("{{alert=\"{k}\"}}"), n as f64)
        })
        .collect();
    m("alerts_firing", "gauge", "Active alerts by kind.", &firing);
    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        out,
    )
        .into_response())
}

// --------------------------------------------------------------- language

#[derive(Deserialize)]
struct LanguageReq {
    /// Language code, or empty for the server default.
    language: String,
}

async fn set_my_language(
    State(st): State<AppState>,
    a: Auth,
    Json(req): Json<LanguageReq>,
) -> ApiResult<Json<Value>> {
    let code = if req.language.trim().is_empty() {
        String::new()
    } else {
        crate::i18n::Lang::parse(&req.language)
            .ok_or_else(|| ApiError::bad("Unsupported language"))?
            .code()
            .to_string()
    };
    let uid = a.user.id;
    let c2 = code.clone();
    st.ctx
        .db
        .call(move |c| db::set_language(c, uid, &c2))
        .await?;
    for h in st.ctx.registry.handles() {
        if h.user_id == uid {
            let _ = h.tx.send(NetCmd::Reload).await;
        }
    }
    let effective = st.ctx.settings.get().lang_for(&code).code();
    Ok(Json(json!({ "language": code, "effective": effective })))
}

/// Report for testing on real networks: what each of the caller's networks
/// supports and how xBNC handled it. No passwords, messages, nicks of other
/// people or client addresses are included.
async fn diagnostics(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    let uid = a.user.id;
    let (nets, rows) = st
        .ctx
        .db
        .call(move |c| {
            let nets = db::list_networks(c, uid)?;
            let mut rows = Vec::new();
            for n in &nets {
                rows.push((
                    db::module_rows(c, n.id)?,
                    db::plugin_config_rows(c, n.id)?,
                    db::list_channels(c, n.id)?.len(),
                ));
            }
            Ok((nets, rows))
        })
        .await?;
    let mut out = Vec::new();
    for (n, (mods, plugins, saved_channels)) in nets.iter().zip(rows) {
        let live = server::network_status(&st.ctx, n.id).await;
        let modules: Vec<&str> = crate::modules::effective(&mods)
            .into_iter()
            .filter(|(_, s)| s.enabled)
            .map(|(m, _)| m.id)
            .collect();
        let plugins: Vec<String> = plugins
            .into_iter()
            .filter(|(_, on, ..)| *on)
            .map(|(id, ..)| id)
            .collect();
        let live = live.map(|l| {
            json!({
                "state": l.state,
                "server": l.server,
                "network_name": l.network_name,
                "connected_since": l.connected_since,
                "last_error": l.last_error,
                "caps_offered": l.caps_offered,
                "caps_acked": l.caps,
                "isupport": l.isupport,
                "services_logged_in": l.logged_in,
                "host_hidden": l.host_hidden,
                "channels_joined": l.channels.len(),
                "clients": l.clients.iter().map(|c| json!({
                    "device": c.name,
                    "caps": c.caps,
                })).collect::<Vec<_>>(),
            })
        });
        out.push(json!({
            "name": n.name,
            "host": n.host,
            "port": n.port,
            "tls": n.tls,
            "enabled": n.enabled,
            "sasl": n.sasl_username.is_some(),
            "server_password": n.server_password_enc.is_some(),
            "saved_channels": saved_channels,
            "modules": modules,
            "plugins": plugins,
            "live": live,
        }));
    }
    let m = st.ctx.metrics.snapshot();
    Ok(Json(json!({
        "report": "xbnc-diagnostics",
        "format": 1,
        "version": crate::network::VERSION,
        "brand": crate::network::BRAND,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "database": if st.ctx.db.is_postgres() { "postgresql" } else { "sqlite" },
        "started_at": st.ctx.started_at,
        "generated_at": db::now_ms(),
        "timezone": a.user.timezone,
        "language": st.ctx.settings.get().lang_for(&a.user.language).code(),
        "counters": m,
        "networks": out,
    })))
}

#[derive(Deserialize)]
struct TimezoneReq {
    /// IANA time zone name, or empty for UTC.
    timezone: String,
}

async fn set_my_timezone(
    State(st): State<AppState>,
    a: Auth,
    Json(req): Json<TimezoneReq>,
) -> ApiResult<Json<Value>> {
    let tz = crate::tz::Zone::parse(&req.timezone).map_err(ApiError::bad)?;
    let name = if req.timezone.trim().is_empty() {
        String::new()
    } else {
        tz.name().to_string()
    };
    let uid = a.user.id;
    let n2 = name.clone();
    st.ctx
        .db
        .call(move |c| db::set_timezone(c, uid, &n2))
        .await?;
    for h in st.ctx.registry.handles() {
        if h.user_id == uid {
            let _ = h.tx.send(NetCmd::Reload).await;
        }
    }
    Ok(Json(json!({ "timezone": name })))
}

async fn delete_network(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let (n, owner) = load_network(&st.ctx, &a, id, true).await?;
    st.ctx
        .db
        .call(move |c| db::delete_network_by_id(c, id))
        .await?;
    audit(
        &st.ctx,
        &a.user,
        "network.delete",
        format!("{owner}/{}", n.name),
    )
    .await;
    server::sync_networks(&st.ctx).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct ControlReq {
    action: NetAction,
}

async fn control_network(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
    Json(req): Json<ControlReq>,
) -> ApiResult<Json<Value>> {
    let (n, owner) = load_network(&st.ctx, &a, id, false).await?;
    if !n.enabled {
        return Err(ApiError::bad("This network is disabled; enable it first"));
    }
    let h = server::network_handle(&st.ctx, id)
        .ok_or_else(|| ApiError::bad("The network is not running"))?;
    let _ = h.tx.send(NetCmd::Control(req.action)).await;
    audit(
        &st.ctx,
        &a.user,
        "network.control",
        format!("{owner}/{} {:?}", n.name, req.action),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

// --------------------------------------------------------------- channels

async fn list_channels(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let (n, _) = load_network(&st.ctx, &a, id, false).await?;
    let saved = st.ctx.db.call(move |c| db::list_channels(c, id)).await?;
    let status = if n.enabled {
        server::network_status(&st.ctx, id).await
    } else {
        None
    };
    let live = status
        .as_ref()
        .map(|s| s.channels.clone())
        .unwrap_or_default();
    let cm = xbnc_proto::CaseMapping::Rfc1459;
    let mut rows: Vec<Value> = saved
        .iter()
        .map(|ch| {
            let l = live.iter().find(|c| cm.eq(&c.name, &ch.name));
            json!({
                "name": ch.name,
                "saved": true,
                "has_key": ch.key_enc.is_some(),
                "retention_days": ch.retention_days,
                "disabled": ch.disabled,
                "position": ch.position,
                "joined": l.is_some(),
                "topic": l.and_then(|c| c.topic.clone()),
                "members": l.map(|c| c.members),
                "ops": l.map(|c| c.ops),
            })
        })
        .collect();
    for l in live
        .iter()
        .filter(|l| !saved.iter().any(|s| cm.eq(&s.name, &l.name)))
    {
        rows.push(json!({ "name": l.name, "saved": false, "has_key": false, "joined": true, "topic": l.topic, "members": l.members, "ops": l.ops }));
    }
    Ok(Json(json!(rows)))
}

#[derive(Deserialize)]
struct AddChannelReq {
    name: String,
    key: Option<String>,
}

async fn add_channel(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
    Json(req): Json<AddChannelReq>,
) -> ApiResult<Json<Value>> {
    let (n, owner) = load_network(&st.ctx, &a, id, false).await?;
    let name = req.name.trim().to_string();
    if !valid_channel(&name) {
        return Err(ApiError::bad(
            "Channel names start with # and have no spaces or commas",
        ));
    }
    let key = req
        .key
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty());
    if key.as_deref().is_some_and(|k| k.contains([' ', ','])) {
        return Err(ApiError::bad(
            "Channel keys cannot contain spaces or commas",
        ));
    }
    match server::network_handle(&st.ctx, id) {
        Some(h) => {
            let _ =
                h.tx.send(NetCmd::Join {
                    channel: name.clone(),
                    key,
                })
                .await;
        }
        None => {
            let enc = key.map(|k| st.ctx.secrets.encrypt(&k));
            let cf = xbnc_proto::CaseMapping::Rfc1459.fold(&name);
            let nm = name.clone();
            st.ctx
                .db
                .call(move |c| db::upsert_channel(c, id, &nm, &cf, enc.as_deref(), false))
                .await?;
        }
    }
    audit(
        &st.ctx,
        &a.user,
        "channel.add",
        format!("{owner}/{} {name}", n.name),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

async fn remove_channel(
    State(st): State<AppState>,
    a: Auth,
    Path((id, name)): Path<(i64, String)>,
) -> ApiResult<Json<Value>> {
    let (n, owner) = load_network(&st.ctx, &a, id, false).await?;
    match server::network_handle(&st.ctx, id) {
        Some(h) => {
            let _ =
                h.tx.send(NetCmd::Part {
                    channel: name.clone(),
                })
                .await;
        }
        None => {
            let cf = xbnc_proto::CaseMapping::Rfc1459.fold(&name);
            st.ctx
                .db
                .call(move |c| db::delete_channel(c, id, &cf))
                .await?;
        }
    }
    audit(
        &st.ctx,
        &a.user,
        "channel.remove",
        format!("{owner}/{} {name}", n.name),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

async fn disconnect_client(
    State(st): State<AppState>,
    a: Auth,
    Path((id, client)): Path<(i64, u64)>,
) -> ApiResult<Json<Value>> {
    let (n, owner) = load_network(&st.ctx, &a, id, false).await?;
    if let Some(h) = server::network_handle(&st.ctx, id) {
        let _ = h.tx.send(NetCmd::DisconnectClient(client)).await;
    }
    audit(
        &st.ctx,
        &a.user,
        "client.disconnect",
        format!("{owner}/{} client {client}", n.name),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------- history

async fn conversations(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    load_network(&st.ctx, &a, id, false).await?;
    let rows = st
        .ctx
        .db
        .call(move |c| db::conversation_list(c, id))
        .await?;
    Ok(Json(json!(rows
        .into_iter()
        .map(|(t, n, last)| json!({ "target": t, "messages": n, "last_ts": last }))
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
struct HistoryQuery {
    target: Option<String>,
    q: Option<String>,
    sender: Option<String>,
    before_id: Option<i64>,
    from: Option<i64>,
    to: Option<i64>,
    limit: Option<usize>,
}

fn msg_json(m: &db::StoredMessage) -> Value {
    let nick = xbnc_proto::Prefix::parse(&m.sender).nick.to_string();
    json!({
        "id": m.id,
        "target": m.target,
        "ts": m.ts,
        "nick": nick,
        "sender": m.sender,
        "command": m.command,
        "text": m.text,
        "outgoing": m.outgoing,
    })
}

async fn history(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
    Query(q): Query<HistoryQuery>,
) -> ApiResult<Json<Value>> {
    load_network(&st.ctx, &a, id, false).await?;
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let target_cf = q.target.map(|t| xbnc_proto::CaseMapping::Rfc1459.fold(&t));
    let rows = st
        .ctx
        .db
        .call(move |c| {
            db::search_messages(
                c,
                id,
                &db::HistoryFilter {
                    target_cf: target_cf.as_deref(),
                    text: q.q.as_deref(),
                    sender: q.sender.as_deref(),
                    before_id: q.before_id,
                    from_ts: q.from,
                    to_ts: q.to,
                    limit,
                },
            )
        })
        .await?;
    let more = rows.len() == limit;
    Ok(Json(
        json!({ "messages": rows.iter().map(msg_json).collect::<Vec<_>>(), "more": more }),
    ))
}

#[derive(Deserialize)]
struct ExportQuery {
    target: Option<String>,
    #[serde(default)]
    format: Option<String>,
}

async fn export_history(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
    Query(q): Query<ExportQuery>,
) -> ApiResult<Response> {
    let (n, owner) = load_network(&st.ctx, &a, id, false).await?;
    let target_cf = q
        .target
        .clone()
        .map(|t| xbnc_proto::CaseMapping::Rfc1459.fold(&t));
    let mut rows = st
        .ctx
        .db
        .call(move |c| {
            db::search_messages(
                c,
                id,
                &db::HistoryFilter {
                    target_cf: target_cf.as_deref(),
                    text: None,
                    sender: None,
                    before_id: None,
                    from_ts: None,
                    to_ts: None,
                    limit: 200_000,
                },
            )
        })
        .await?;
    rows.reverse();
    audit(
        &st.ctx,
        &a.user,
        "history.export",
        format!(
            "{owner}/{} {}",
            n.name,
            q.target.clone().unwrap_or_else(|| "*".into())
        ),
    )
    .await;
    let base = format!(
        "xbnc-{}-{}",
        n.name,
        q.target
            .as_deref()
            .unwrap_or("all")
            .replace(['#', '/', '\\', '"'], "")
    );
    let (body, ctype, ext) = if q.format.as_deref() == Some("json") {
        (
            serde_json::to_string_pretty(&rows.iter().map(msg_json).collect::<Vec<_>>())
                .unwrap_or_default(),
            "application/json",
            "json",
        )
    } else {
        let mut out = String::new();
        for m in &rows {
            let nick = xbnc_proto::Prefix::parse(&m.sender).nick;
            let time = crate::util::format_short_time(m.ts);
            let text = match m.text.strip_prefix("\x01ACTION ") {
                Some(rest) => format!("* {nick} {}", rest.trim_end_matches('\x01')),
                None if m.command == "NOTICE" => format!("-{nick}- {}", m.text),
                None => format!("<{nick}> {}", m.text),
            };
            out.push_str(&format!("[{time}] {} {text}\n", m.target));
        }
        (out, "text/plain; charset=utf-8", "txt")
    };
    Ok((
        [
            (header::CONTENT_TYPE, ctype.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{base}.{ext}\""),
            ),
        ],
        body,
    )
        .into_response())
}

#[derive(Deserialize)]
struct ClearQuery {
    target: Option<String>,
}

async fn clear_history(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
    Query(q): Query<ClearQuery>,
) -> ApiResult<Json<Value>> {
    let (n, owner) = load_network(&st.ctx, &a, id, false).await?;
    let target_cf = q
        .target
        .clone()
        .map(|t| xbnc_proto::CaseMapping::Rfc1459.fold(&t));
    let deleted = st
        .ctx
        .db
        .call(move |c| db::delete_history(c, id, target_cf.as_deref()))
        .await?;
    audit(
        &st.ctx,
        &a.user,
        "history.delete",
        format!(
            "{owner}/{} {} ({deleted} messages)",
            n.name,
            q.target.unwrap_or_else(|| "*".into())
        ),
    )
    .await;
    Ok(Json(json!({ "deleted": deleted })))
}

// ------------------------------------------------------------------ users

async fn list_users(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Vec<UserView>>> {
    a.require(Perm::ViewUsers)?;
    let rows = st
        .ctx
        .db
        .call(|c| {
            let users = db::list_users(c)?;
            let photos = db::avatar_versions(c)?;
            users
                .into_iter()
                .map(|u| {
                    let n = db::count_networks(c, u.id)?;
                    let p = photos.get(&u.id).copied();
                    Ok((u, n, p))
                })
                .collect::<anyhow::Result<Vec<_>>>()
        })
        .await?;
    Ok(Json(
        rows.iter()
            .map(|(u, n, p)| {
                let mut v = user_view(&st.ctx, u, *n);
                v.avatar = avatar_url(u.id, *p);
                v
            })
            .collect(),
    ))
}

async fn get_user(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Json<UserView>> {
    if id != a.user.id {
        a.require(Perm::ViewUsers)?;
    }
    let (u, n) = st
        .ctx
        .db
        .call(move |c| Ok(db::get_user_by_id(c, id)?.map(|u| (u, db::count_networks(c, id)))))
        .await?
        .ok_or_else(|| ApiError::not_found("User"))?;
    Ok(Json(with_avatar(&st.ctx, user_view(&st.ctx, &u, n?)).await))
}

#[derive(Deserialize)]
struct CreateUserReq {
    username: String,
    password: String,
    role: Option<String>,
    max_networks: Option<i64>,
    max_clients: Option<i64>,
}

async fn create_user(
    State(st): State<AppState>,
    a: Auth,
    Json(req): Json<CreateUserReq>,
) -> ApiResult<(StatusCode, Json<UserView>)> {
    a.require(Perm::ManageUsers)?;
    let role = Role::parse(req.role.as_deref().unwrap_or("user"))
        .ok_or_else(|| ApiError::bad("Unknown role"))?;
    if !a.role.can_manage_role(role) {
        return Err(ApiError::forbidden());
    }
    crypto::check_password_policy(&req.password)?;
    let hash = crypto::hash_password(&req.password)?;
    let name = req.username.trim().to_string();
    let upd = UserUpdate {
        max_networks: Some(req.max_networks),
        max_clients: Some(req.max_clients),
        ..Default::default()
    };
    let u = st
        .ctx
        .db
        .call(move |c| {
            let id = db::create_user_with_role(c, &name, &hash, role.as_str())?;
            db::update_user(c, id, &upd)?;
            Ok(db::get_user_by_id(c, id)?.expect("created"))
        })
        .await?;
    audit(
        &st.ctx,
        &a.user,
        "user.create",
        format!("{} role={}", u.username, u.role),
    )
    .await;
    Ok((StatusCode::CREATED, Json(user_view(&st.ctx, &u, 0))))
}

#[derive(Deserialize)]
struct UpdateUserReq {
    role: Option<String>,
    enabled: Option<bool>,
    /// `null` clears the override; omit to keep it.
    #[serde(default, deserialize_with = "double_option")]
    max_networks: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double_option")]
    max_clients: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double_option")]
    max_history_mb: Option<Option<i64>>,
}

fn double_option<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<i64>>, D::Error> {
    Ok(Some(Option::<i64>::deserialize(d)?))
}

async fn superadmins(ctx: &Ctx) -> ApiResult<usize> {
    Ok(ctx
        .db
        .call(|c| {
            Ok(db::list_users(c)?
                .iter()
                .filter(|u| u.role == "superadmin" && u.enabled)
                .count())
        })
        .await?)
}

async fn update_user(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
    Json(req): Json<UpdateUserReq>,
) -> ApiResult<Json<UserView>> {
    // Refuse before the lookup, so accounts without these permissions
    // cannot tell which user ids exist.
    if !a.role.can(Perm::ManageUsers) && !a.role.can(Perm::SuspendUsers) {
        return Err(ApiError::forbidden());
    }
    let target = st
        .ctx
        .db
        .call(move |c| db::get_user_by_id(c, id))
        .await?
        .ok_or_else(|| ApiError::not_found("User"))?;
    let cur_role = Role::from_db(&target.role);
    let only_enable = req.role.is_none()
        && req.max_networks.is_none()
        && req.max_clients.is_none()
        && req.max_history_mb.is_none()
        && req.enabled.is_some();
    if only_enable {
        if !a.role.can(Perm::SuspendUsers) || !a.role.can_suspend(cur_role) {
            return Err(ApiError::forbidden());
        }
    } else {
        a.require(Perm::ManageUsers)?;
        if !a.role.can_manage_role(cur_role) {
            return Err(ApiError::forbidden());
        }
    }
    if id == a.user.id && (req.role.is_some() || req.enabled == Some(false)) {
        return Err(ApiError::bad(
            "You cannot change your own role or suspend yourself",
        ));
    }
    let new_role = match &req.role {
        Some(r) => {
            let r = Role::parse(r).ok_or_else(|| ApiError::bad("Unknown role"))?;
            if !a.role.can_manage_role(r) {
                return Err(ApiError::forbidden());
            }
            Some(r)
        }
        None => None,
    };
    let losing_superadmin = cur_role == Role::Superadmin
        && (new_role.is_some_and(|r| r != Role::Superadmin) || req.enabled == Some(false));
    if losing_superadmin && superadmins(&st.ctx).await? <= 1 {
        return Err(ApiError::bad(
            "There must be at least one active superadmin",
        ));
    }
    let upd = UserUpdate {
        role: new_role.map(|r| r.as_str().to_string()),
        enabled: req.enabled,
        max_networks: req.max_networks,
        max_clients: req.max_clients,
        max_history_mb: req.max_history_mb,
    };
    let (u, n) = st
        .ctx
        .db
        .call(move |c| {
            db::update_user(c, id, &upd)?;
            if upd.enabled == Some(false) {
                db::delete_user_sessions(c, id, None)?;
            }
            let u = db::get_user_by_id(c, id)?.expect("exists");
            let n = db::count_networks(c, id)?;
            Ok((u, n))
        })
        .await?;
    audit(
        &st.ctx,
        &a.user,
        "user.update",
        format!("{} role={} enabled={}", u.username, u.role, u.enabled),
    )
    .await;
    if req.enabled.is_some() {
        server::sync_networks(&st.ctx).await?;
    }
    Ok(Json(with_avatar(&st.ctx, user_view(&st.ctx, &u, n)).await))
}

// ------------------------------------------------------------- photos

async fn set_my_avatar(
    State(st): State<AppState>,
    a: Auth,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<Value>> {
    let claimed = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let mime = crate::avatar::check(&claimed, &body).map_err(|e| ApiError::bad(e.to_string()))?;
    let uid = a.user.id;
    let ver = st
        .ctx
        .db
        .call(move |c| db::set_avatar(c, uid, mime, &body))
        .await?;
    audit(&st.ctx, &a.user, "user.avatar", a.user.username.clone()).await;
    Ok(Json(json!({ "avatar": avatar_url(uid, Some(ver)) })))
}

async fn delete_my_avatar(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    let uid = a.user.id;
    st.ctx.db.call(move |c| db::delete_avatar(c, uid)).await?;
    Ok(Json(json!({ "avatar": null })))
}

/// A user's photo: for that user, and for those who can see the users.
async fn get_avatar(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    if id != a.user.id && !a.role.can(Perm::ViewUsers) {
        return Err(ApiError::forbidden());
    }
    let (mime, data, _) = st
        .ctx
        .db
        .call(move |c| db::get_avatar(c, id))
        .await?
        .ok_or_else(|| ApiError::not_found("Photo"))?;
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "private, max-age=31536000, immutable".to_string()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'; sandbox".to_string()),
            (header::CONTENT_DISPOSITION, "inline; filename=\"avatar\"".to_string()),
        ],
        data,
    )
        .into_response())
}

/// An administrator removes someone's photo (one they may manage).
async fn delete_user_avatar(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    if id != a.user.id {
        a.require(Perm::ManageUsers)?;
        let target = st
            .ctx
            .db
            .call(move |c| db::get_user_by_id(c, id))
            .await?
            .ok_or_else(|| ApiError::not_found("User"))?;
        if !a.role.can_manage_role(Role::from_db(&target.role)) {
            return Err(ApiError::forbidden());
        }
        audit(&st.ctx, &a.user, "user.avatar_delete", target.username.clone()).await;
    }
    st.ctx.db.call(move |c| db::delete_avatar(c, id)).await?;
    Ok(Json(json!({ "avatar": null })))
}

async fn delete_user(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    a.require(Perm::ManageUsers)?;
    if id == a.user.id {
        return Err(ApiError::bad("You cannot delete your own account"));
    }
    let target = st
        .ctx
        .db
        .call(move |c| db::get_user_by_id(c, id))
        .await?
        .ok_or_else(|| ApiError::not_found("User"))?;
    let role = Role::from_db(&target.role);
    if !a.role.can_manage_role(role) {
        return Err(ApiError::forbidden());
    }
    if role == Role::Superadmin && superadmins(&st.ctx).await? <= 1 {
        return Err(ApiError::bad(
            "There must be at least one active superadmin",
        ));
    }
    st.ctx
        .db
        .call(move |c| db::delete_user_by_id(c, id))
        .await?;
    audit(&st.ctx, &a.user, "user.delete", target.username.clone()).await;
    server::sync_networks(&st.ctx).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct PasswordReq {
    password: String,
}

async fn reset_password(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
    Json(req): Json<PasswordReq>,
) -> ApiResult<Json<Value>> {
    a.require(Perm::ManageUsers)?;
    let target = st
        .ctx
        .db
        .call(move |c| db::get_user_by_id(c, id))
        .await?
        .ok_or_else(|| ApiError::not_found("User"))?;
    if !a.role.can_manage_role(Role::from_db(&target.role)) && id != a.user.id {
        return Err(ApiError::forbidden());
    }
    crypto::check_password_policy(&req.password)?;
    let hash = crypto::hash_password(&req.password)?;
    st.ctx
        .db
        .call(move |c| {
            db::set_password_by_id(c, id, &hash)?;
            db::delete_user_sessions(c, id, None)
        })
        .await?;
    audit(&st.ctx, &a.user, "user.password_reset", target.username).await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct ChangePasswordReq {
    current_password: String,
    new_password: String,
}

async fn change_my_password(
    State(st): State<AppState>,
    a: Auth,
    Json(req): Json<ChangePasswordReq>,
) -> ApiResult<Json<Value>> {
    a.require_session()?;
    if !crypto::verify_password(&a.user.password_hash, &req.current_password) {
        return Err(ApiError::bad("Your current password is not correct"));
    }
    crypto::check_password_policy(&req.new_password)?;
    let hash = crypto::hash_password(&req.new_password)?;
    let (id, sid) = (a.user.id, a.session.id);
    st.ctx
        .db
        .call(move |c| {
            db::set_password_by_id(c, id, &hash)?;
            db::delete_user_sessions(c, id, Some(sid))
        })
        .await?;
    audit(
        &st.ctx,
        &a.user,
        "me.password",
        "changed own password".into(),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

// -------------------------------------------------------------------- 2FA

async fn totp_setup(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    a.require_session()?;
    if a.user.totp_enabled {
        return Err(ApiError::bad("Two-factor authentication is already active"));
    }
    let secret = totp::generate_secret();
    let enc = st.ctx.secrets.encrypt(&secret);
    let id = a.user.id;
    st.ctx
        .db
        .call(move |c| db::set_totp(c, id, Some(&enc), false))
        .await?;
    let issuer = format!("xBNC {}", st.ctx.config.hostname);
    let uri = totp::provisioning_uri(&secret, &a.user.username, &issuer);
    Ok(Json(
        json!({ "secret": secret, "uri": uri, "qr_svg": totp::qr_svg(&uri) }),
    ))
}

#[derive(Deserialize)]
struct CodeReq {
    code: String,
}

async fn totp_enable(
    State(st): State<AppState>,
    a: Auth,
    Json(req): Json<CodeReq>,
) -> ApiResult<Json<Value>> {
    a.require_session()?;
    let secret = a
        .user
        .totp_secret_enc
        .as_deref()
        .and_then(|e| st.ctx.secrets.decrypt(e).ok())
        .ok_or_else(|| ApiError::bad("Start the 2FA setup first"))?;
    if !totp::verify(secret.expose(), &req.code, totp::now_unix()) {
        return Err(ApiError::bad(
            "The code is not correct; check the time on your phone and try again",
        ));
    }
    let id = a.user.id;
    let enc = a.user.totp_secret_enc.clone();
    st.ctx
        .db
        .call(move |c| db::set_totp(c, id, enc.as_deref(), true))
        .await?;
    audit(&st.ctx, &a.user, "me.2fa", "enabled".into()).await;
    Ok(Json(json!({ "ok": true })))
}

async fn totp_disable(
    State(st): State<AppState>,
    a: Auth,
    Json(req): Json<PasswordReq>,
) -> ApiResult<Json<Value>> {
    a.require_session()?;
    if !crypto::verify_password(&a.user.password_hash, &req.password) {
        return Err(ApiError::bad("Your password is not correct"));
    }
    let id = a.user.id;
    st.ctx
        .db
        .call(move |c| db::set_totp(c, id, None, false))
        .await?;
    audit(&st.ctx, &a.user, "me.2fa", "disabled".into()).await;
    Ok(Json(json!({ "ok": true })))
}

// --------------------------------------------------------------- sessions

#[derive(Deserialize)]
struct AllQuery {
    #[serde(default)]
    all: bool,
}

async fn list_sessions(
    State(st): State<AppState>,
    a: Auth,
    Query(q): Query<AllQuery>,
) -> ApiResult<Json<Value>> {
    let filter = if q.all {
        a.require(Perm::ManageSecurity)?;
        None
    } else {
        Some(a.user.id)
    };
    let me = a.session.id;
    let rows = st
        .ctx
        .db
        .call(move |c| db::list_sessions(c, filter))
        .await?;
    Ok(Json(json!(rows
        .iter()
        .map(|(s, user)| json!({
            "id": s.id, "username": user, "ip": s.ip, "user_agent": s.user_agent,
            "created_at": s.created_at, "last_seen": s.last_seen, "expires_at": s.expires_at,
            "current": s.id == me,
        }))
        .collect::<Vec<_>>())))
}

async fn revoke_session(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let s = st
        .ctx
        .db
        .call(move |c| db::get_session(c, id))
        .await?
        .ok_or_else(|| ApiError::not_found("Session"))?;
    if s.user_id != a.user.id {
        a.require(Perm::ManageSecurity)?;
    }
    st.ctx.db.call(move |c| db::delete_session(c, id)).await?;
    audit(
        &st.ctx,
        &a.user,
        "session.revoke",
        format!("session {id} of user {}", s.user_id),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

// --------------------------------------------------------------- security

#[derive(Deserialize)]
struct LoginsQuery {
    #[serde(default)]
    failures: bool,
    limit: Option<usize>,
}

async fn security_overview(
    State(st): State<AppState>,
    a: Auth,
    Query(q): Query<LoginsQuery>,
) -> ApiResult<Json<Value>> {
    a.require(Perm::ViewAudit)?;
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let (events, f24) = st
        .ctx
        .db
        .call(move |c| {
            Ok((
                db::recent_logins(c, limit, q.failures)?,
                db::count_failed_logins_since(c, db::now_ms() - 86_400_000)?,
            ))
        })
        .await?;
    let l = st.ctx.settings.get().limits;
    Ok(Json(json!({
        "blocked": st.ctx.limiter.blocked(),
        "failed_logins_24h": f24,
        "policy": {
            "max_failures": l.login_max_failures,
            "window_seconds": l.login_window_seconds,
            "block_seconds": l.login_block_seconds,
            "min_password_length": 10,
        },
        "events": events.iter().map(|e| json!({
            "ts": e.ts, "ip": e.ip, "username": e.username, "source": e.source, "success": e.success, "reason": e.reason,
        })).collect::<Vec<_>>(),
    })))
}

async fn unblock_ip(
    State(st): State<AppState>,
    a: Auth,
    Path(ip): Path<String>,
) -> ApiResult<Json<Value>> {
    a.require(Perm::ManageSecurity)?;
    let addr: std::net::IpAddr = ip
        .parse()
        .map_err(|_| ApiError::bad("Invalid IP address"))?;
    let removed = st.ctx.limiter.unblock(addr);
    audit(&st.ctx, &a.user, "security.unblock", ip).await;
    Ok(Json(json!({ "ok": removed })))
}

#[derive(Deserialize)]
struct LimitQuery {
    limit: Option<usize>,
}

async fn audit_log(
    State(st): State<AppState>,
    a: Auth,
    Query(q): Query<LimitQuery>,
) -> ApiResult<Json<Value>> {
    a.require(Perm::ViewAudit)?;
    let limit = q.limit.unwrap_or(200).clamp(1, 2000);
    let rows = st.ctx.db.call(move |c| db::recent_audit(c, limit)).await?;
    Ok(Json(json!(rows
        .into_iter()
        .map(|(ts, actor, action, detail)| json!({ "ts": ts, "actor": actor, "action": action, "detail": detail }))
        .collect::<Vec<_>>())))
}

// --------------------------------------------------------------- settings

async fn get_settings(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    a.require(Perm::ViewSystem)?;
    Ok(Json(json!({
        "current": st.ctx.settings.get(),
        "defaults": st.ctx.settings.defaults(),
        "editable": a.role.can(Perm::EditSettings),
    })))
}

async fn put_settings(
    State(st): State<AppState>,
    a: Auth,
    Json(new): Json<Settings>,
) -> ApiResult<Json<Value>> {
    a.require(Perm::EditSettings)?;
    let old = st.ctx.settings.get();
    st.ctx.settings.save(&st.ctx.db, new.clone()).await?;
    st.ctx.apply_settings();
    let mut changed = Vec::new();
    let (o, n) = (
        serde_json::to_value(&old).unwrap_or_default(),
        serde_json::to_value(&new).unwrap_or_default(),
    );
    for section in ["history", "limits", "general", "alerts"] {
        if let (Some(os), Some(ns)) = (o[section].as_object(), n[section].as_object()) {
            for (k, v) in ns {
                if os.get(k) != Some(v) {
                    if k == "webhook_url" {
                        // Webhook URLs usually embed a secret token.
                        changed.push(format!("{section}.{k} changed"));
                    } else {
                        changed.push(format!("{section}.{k}={v}"));
                    }
                }
            }
        }
    }
    audit(&st.ctx, &a.user, "settings.update", changed.join(", ")).await;
    if old.general.language != new.general.language {
        // Users without their own language follow the new default on IRC too.
        for h in st.ctx.registry.handles() {
            let _ = h.tx.send(NetCmd::Reload).await;
        }
    }
    Ok(Json(json!({ "ok": true, "current": new })))
}

async fn system_info(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    a.require(Perm::ViewSystem)?;
    let ctx = &st.ctx;
    let (messages, size, networks, schema) = ctx
        .db
        .call(|c| {
            let (m, s, n) = db::storage_stats(c)?;
            Ok((m, s, n, db::schema_version(c)?))
        })
        .await?;
    let os = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|t| {
            t.lines().find_map(|l| {
                l.strip_prefix("PRETTY_NAME=")
                    .map(|v| v.trim_matches('"').to_string())
            })
        });
    Ok(Json(json!({
        "version": crate::network::VERSION,
        "hostname": ctx.config.hostname,
        "started_at": ctx.started_at,
        "uptime_seconds": (db::now_ms() - ctx.started_at) / 1000,
        "os": os,
        "arch": std::env::consts::ARCH,
        "database": { "path": ctx.config.database.path, "size_bytes": size, "schema_version": schema, "messages": messages, "networks": networks },
        "irc_listeners": ctx.config.listen.iter().map(|l| json!({ "address": l.address, "tls": l.tls_cert.is_some() })).collect::<Vec<_>>(),
        "web": { "address": st.cfg.address, "tls": st.cfg.uses_tls(), "shared": st.cfg.shared, "public_url": st.cfg.public_url, "session_hours": st.cfg.session_hours },
        "rss_bytes": crate::metrics::rss_bytes(),
        "counters": ctx.metrics.snapshot(),
        "config_only": ["hostname", "listen", "database.path", "secret_key_file", "web", "log.level"],
    })))
}

async fn openapi() -> Response {
    (
        [(header::CONTENT_TYPE, "application/json")],
        include_str!("../../../../docs/openapi.json"),
    )
        .into_response()
}
