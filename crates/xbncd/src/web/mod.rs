//! Web administration panel and REST API (`/api/v1`).
//!
//! * Sessions: random token in an `HttpOnly`, `SameSite=Strict` cookie; only
//!   its SHA-256 is stored.
//! * CSRF: every state-changing request must carry the session's CSRF token
//!   in `X-CSRF-Token`, and a cross-origin `Origin` header is rejected.
//! * Brute force: the same per-IP limiter as IRC logins, plus a per-IP
//!   request rate limit on the whole API.
//! * Security headers (CSP, frame-ancestors, nosniff...) on every response.
//! * The React panel is compiled into the binary (`web/dist`).

mod admin_api;
mod api;
mod auth_api;
mod chat_api;
mod plugins_api;
mod sso;
mod ws;

use crate::config::WebConfig;
use crate::db::{self, User, WebSession};
use crate::metrics::Metrics;
use crate::rbac::{Perm, Role};
use crate::server::Ctx;
use anyhow::{Context, Result};
use axum::body::Body;
use axum::extract::{ConnectInfo, FromRequestParts, Request, State};
use axum::http::{header, request::Parts, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use rust_embed::RustEmbed;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{info, warn};

pub const SESSION_COOKIE: &str = "xbnc_session";
const CSRF_HEADER: &str = "x-csrf-token";
const MAX_BODY: usize = 256 * 1024;

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
struct Assets;

/// Shared state of the web layer.
pub struct WebState {
    pub ctx: Arc<Ctx>,
    pub cfg: WebConfig,
    rate: Mutex<HashMap<IpAddr, (f64, Instant)>>,
    /// Short-lived sign-in ceremonies (passkey challenges, SSO states).
    pub flows: Mutex<HashMap<String, Flow>>,
}

/// One pending passkey or SSO ceremony, valid for five minutes.
#[derive(Clone)]
pub struct Flow {
    pub kind: &'static str,
    pub user_id: Option<i64>,
    pub data: Vec<u8>,
    pub extra: String,
    pub at: Instant,
}

impl WebState {
    pub fn put_flow(&self, key: String, flow: Flow) {
        let mut m = self.flows.lock().unwrap_or_else(|e| e.into_inner());
        m.retain(|_, f| f.at.elapsed().as_secs() < 300);
        if m.len() < 10_000 {
            m.insert(key, flow);
        }
    }
    /// Takes a ceremony (each one can be used once).
    pub fn take_flow(&self, key: &str, kind: &str) -> Option<Flow> {
        let mut m = self.flows.lock().unwrap_or_else(|e| e.into_inner());
        let f = m.remove(key)?;
        (f.kind == kind && f.at.elapsed().as_secs() < 300).then_some(f)
    }
}

type AppState = Arc<WebState>;

// --------------------------------------------------------------- errors

/// JSON error: `{"error": "<code>", "message": "<text>"}`.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        ApiError {
            status,
            code,
            message: message.into(),
        }
    }
    pub fn bad(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid", message)
    }
    pub fn forbidden() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "You do not have permission to do this",
        )
    }
    pub fn not_found(what: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("{what} not found"),
        )
    }
    pub fn internal(message: impl Into<String>) -> Self {
        let m = message.into();
        tracing::error!(error = %m, "internal error");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "Internal error",
        )
    }
    pub fn unauthorized(code: &'static str, message: &str) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, code, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        #[derive(Serialize)]
        struct Body<'a> {
            error: &'a str,
            message: &'a str,
        }
        (
            self.status,
            axum::Json(Body {
                error: self.code,
                message: &self.message,
            }),
        )
            .into_response()
    }
}

/// Errors from the storage and validation layers become 400 responses with
/// their message (validation messages are written for end users); unexpected
/// database failures are logged.
impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        let msg = format!("{e:#}");
        if msg.contains("database") || msg.contains("SQLITE") || msg.contains("sqlite") {
            warn!(error = %msg, "internal error in web API");
            return ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "Internal error; see the server log",
            );
        }
        ApiError::bad(msg)
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

// ------------------------------------------------------------- helpers

pub fn random_token() -> String {
    use chacha20poly1305::aead::rand_core::RngCore;
    let mut b = [0u8; 32];
    chacha20poly1305::aead::OsRng.fill_bytes(&mut b);
    B64URL.encode(b)
}

pub fn hash_token(token: &str) -> String {
    let d = Sha256::digest(token.as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// Human-friendly one-time setup code, e.g. `K7QX-3MPA-9RTE`.
pub fn generate_setup_token() -> String {
    use chacha20poly1305::aead::rand_core::RngCore;
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut b = [0u8; 12];
    chacha20poly1305::aead::OsRng.fill_bytes(&mut b);
    let s: String = b
        .iter()
        .map(|x| ALPHABET[(*x as usize) % ALPHABET.len()] as char)
        .collect();
    format!("{}-{}-{}", &s[0..4], &s[4..8], &s[8..12])
}

/// Constant-time comparison for tokens.
pub fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// Real client address, honouring a trusted local reverse proxy.
pub fn client_ip(cfg: &WebConfig, peer: SocketAddr, headers: &HeaderMap) -> IpAddr {
    if cfg.trust_proxy && peer.ip().is_loopback() {
        let fwd = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next_back())
            .or_else(|| headers.get("x-real-ip").and_then(|v| v.to_str().ok()));
        if let Some(ip) = fwd.and_then(|s| s.trim().parse().ok()) {
            return ip;
        }
    }
    peer.ip()
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == SESSION_COOKIE)
        .map(|(_, v)| v.to_string())
}

pub fn session_cookie(cfg: &WebConfig, token: &str) -> HeaderValue {
    let secure = if cfg.secure_cookies() { "; Secure" } else { "" };
    let v = format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}{secure}",
        cfg.session_hours * 3600
    );
    HeaderValue::from_str(&v).expect("cookie is ASCII")
}

pub fn clear_cookie(cfg: &WebConfig) -> HeaderValue {
    let secure = if cfg.secure_cookies() { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{secure}"
    ))
    .expect("cookie is ASCII")
}

/// Records an administrative action from the panel.
pub async fn audit(ctx: &Ctx, user: &User, action: &str, detail: String) {
    let actor = format!("web:{}", user.username);
    let action = action.to_string();
    if let Err(e) = ctx
        .db
        .call(move |c| db::audit(c, &actor, &action, &detail))
        .await
    {
        warn!(error = %e, "cannot write audit log");
    }
}

// --------------------------------------------------------- auth extractor

/// An authenticated panel request.
pub struct Auth {
    pub user: User,
    pub role: Role,
    /// The panel session; for API tokens a placeholder with id 0.
    pub session: WebSession,
    pub ip: IpAddr,
    /// Set when the request was authenticated with an API token.
    pub token: Option<db::ApiToken>,
}

/// What an API token may do.
pub const TOKEN_SCOPES: &[&str] = &["read", "write", "metrics"];

impl Auth {
    /// Actions that only a person in the panel may take (passwords, 2FA,
    /// tokens, logging out): refused for API tokens.
    pub fn require_session(&self) -> ApiResult<()> {
        if self.token.is_some() {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "session_required",
                "This action needs a panel session, not an API token",
            ));
        }
        Ok(())
    }

    pub fn require(&self, p: Perm) -> ApiResult<()> {
        if self.role.can(p) {
            Ok(())
        } else {
            Err(ApiError::forbidden())
        }
    }
}

impl FromRequestParts<AppState> for Auth {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0)
            .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 0)));
        let ip = client_ip(&st.cfg, peer, &parts.headers);
        if let Some(bearer) = bearer_token(&parts.headers) {
            return token_auth(st, parts, ip, bearer).await;
        }
        let token = cookie_token(&parts.headers)
            .ok_or_else(|| ApiError::unauthorized("not_logged_in", "Please log in"))?;
        let hash = hash_token(&token);
        let ttl = st.cfg.session_hours as i64 * 3_600_000;
        let found = st
            .ctx
            .db
            .call(move |c| {
                let Some(s) = db::touch_session(c, &hash, ttl)? else {
                    return Ok(None);
                };
                let u = db::get_user_by_id(c, s.user_id)?;
                Ok(u.map(|u| (s, u)))
            })
            .await?;
        let Some((session, user)) = found else {
            return Err(ApiError::unauthorized(
                "session_expired",
                "Your session has expired; please log in again",
            ));
        };
        if !user.enabled {
            return Err(ApiError::unauthorized(
                "account_disabled",
                "This account is disabled",
            ));
        }
        let unsafe_method = !matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS);
        if unsafe_method {
            let sent = parts
                .headers
                .get(CSRF_HEADER)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if !ct_eq(sent, &session.csrf) {
                return Err(ApiError::new(
                    StatusCode::FORBIDDEN,
                    "csrf",
                    "Missing or invalid CSRF token",
                ));
            }
        }
        Ok(Auth {
            role: Role::from_db(&user.role),
            user,
            session,
            ip,
            token: None,
        })
    }
}

/// `Authorization: Bearer xbnc_...`
pub fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let v = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let t = v
        .strip_prefix("Bearer ")
        .or_else(|| v.strip_prefix("bearer "))?;
    let t = t.trim();
    (t.starts_with("xbnc_") && t.len() <= 100).then(|| t.to_string())
}

/// Authenticates an API token. Failures count towards the IP's brute-force
/// limit, like passwords.
async fn token_auth(
    st: &AppState,
    parts: &Parts,
    ip: IpAddr,
    bearer: String,
) -> Result<Auth, ApiError> {
    if st.ctx.limiter.is_blocked(ip) {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "blocked",
            "Too many failed attempts from this address; try again later",
        ));
    }
    let hash = hash_token(&bearer);
    let ip_s = ip.to_string();
    let found = st
        .ctx
        .db
        .call(move |c| {
            let Some(t) = db::use_token(c, &hash, &ip_s)? else {
                return Ok(None);
            };
            Ok(db::get_user_by_id(c, t.user_id)?.map(|u| (t, u)))
        })
        .await?;
    let Some((token, user)) = found else {
        st.ctx.limiter.record_failure(ip);
        Metrics::add(&st.ctx.metrics.failed_logins, 1);
        // Never store any part of the presented secret.
        let ip_s = ip.to_string();
        let _ = st
            .ctx
            .db
            .call(move |c| db::record_login(c, &ip_s, "", "api", false, "invalid API token"))
            .await;
        return Err(ApiError::unauthorized(
            "invalid_token",
            "Invalid or expired API token",
        ));
    };
    if !user.enabled {
        return Err(ApiError::unauthorized(
            "account_disabled",
            "This account is disabled",
        ));
    }
    let read_only = matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS);
    match token.scope.as_str() {
        "write" => {}
        "read" if read_only => {}
        // Inside the nested API router the path has lost its "/api/v1" prefix.
        "metrics" if read_only && matches!(parts.uri.path(), "/metrics" | "/api/v1/metrics") => {}
        "read" => {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "token_read_only",
                "This API token can only read",
            ))
        }
        _ => {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "token_scope",
                "This API token can only read metrics",
            ))
        }
    }
    let session = WebSession {
        id: 0,
        user_id: user.id,
        csrf: String::new(),
        created_at: token.created_at,
        last_seen: db::now_ms(),
        expires_at: token.expires_at.unwrap_or(i64::MAX),
        ip: ip.to_string(),
        user_agent: format!("API token {}", token.name),
    };
    Ok(Auth {
        role: Role::from_db(&user.role),
        user,
        session,
        ip,
        token: Some(token),
    })
}

// ------------------------------------------------------------ middleware

/// Rejects requests whose `Origin` is not this site (CSRF / cross-site
/// WebSocket hijacking defence in depth).
fn origin_ok(cfg: &WebConfig, headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return true; // same-origin GETs and non-browser clients
    };
    if let Some(public) = &cfg.public_url {
        if origin.trim_end_matches('/') == public.trim_end_matches('/') {
            return true;
        }
    }
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let origin_host = origin.split_once("://").map(|(_, h)| h).unwrap_or(origin);
    !host.is_empty() && origin_host.eq_ignore_ascii_case(host)
}

async fn guard(
    State(st): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    if path.starts_with("/api/") {
        // Any WebSocket handshake (live events, chat, plugin logs).
        let is_ws = req
            .headers()
            .get(header::UPGRADE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
        let unsafe_method = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
        if (unsafe_method || is_ws) && !origin_ok(&st.cfg, req.headers()) {
            return ApiError::new(
                StatusCode::FORBIDDEN,
                "bad_origin",
                "Cross-origin request refused",
            )
            .into_response();
        }
        // Token bucket per IP: 20 requests/s, burst 100.
        let ip = client_ip(&st.cfg, peer, req.headers());
        let allowed = {
            let mut map = st.rate.lock().expect("rate lock");
            let now = Instant::now();
            if map.len() > 50_000 {
                map.retain(|_, (_, t)| now.duration_since(*t).as_secs() < 60);
            }
            let (tokens, last) = map.entry(ip).or_insert((100.0, now));
            *tokens = (*tokens + now.duration_since(*last).as_secs_f64() * 20.0).min(100.0);
            *last = now;
            if *tokens >= 1.0 {
                *tokens -= 1.0;
                true
            } else {
                false
            }
        };
        if !allowed {
            return ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "Too many requests; slow down",
            )
            .into_response();
        }
    }
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    // A stricter policy set by the handler (profile photos) is kept.
    h.entry(header::CONTENT_SECURITY_POLICY).or_insert(
        HeaderValue::from_static(
            "default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; \
             script-src 'self'; connect-src 'self'; font-src 'self' data:; frame-ancestors 'none'; \
             base-uri 'none'; form-action 'self'; object-src 'none'",
        ),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    h.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    h.insert(
        "cross-origin-opener-policy",
        HeaderValue::from_static("same-origin"),
    );
    if st.cfg.uses_tls() {
        h.insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000"),
        );
    }
    if path.starts_with("/api/") && !h.contains_key(header::CACHE_CONTROL) {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    resp
}

// ------------------------------------------------------------ static files

async fn static_files(req: Request) -> Response {
    let path = req.uri().path().trim_start_matches('/');
    if path.starts_with("api/") {
        return ApiError::not_found("API endpoint").into_response();
    }
    let (file, name) = match Assets::get(path).filter(|_| !path.is_empty()) {
        Some(f) => (f, path.to_string()),
        None => match Assets::get("index.html") {
            Some(f) => (f, "index.html".to_string()),
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "The web panel was not built into this binary (web/dist is empty).",
                )
                    .into_response()
            }
        },
    };
    let cache = if name.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    Response::builder()
        .header(header::CONTENT_TYPE, file.metadata.mimetype())
        .header(header::CACHE_CONTROL, cache)
        .body(Body::from(file.data.into_owned()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

// -------------------------------------------------------------- listener

/// Open panel connections in total and per address (loopback, where a
/// reverse proxy connects from, only counts towards the total).
const WEB_MAX_CONNECTIONS: usize = 4096;
const WEB_MAX_PER_IP: usize = 64;
/// A connection with no traffic in either direction for this long is
/// closed (the live WebSocket sends every 2 seconds, so it never idles).
const WEB_IDLE: std::time::Duration = std::time::Duration::from_secs(120);

static WEB_CONNS: std::sync::LazyLock<std::sync::Mutex<(usize, HashMap<std::net::IpAddr, usize>)>> =
    std::sync::LazyLock::new(Default::default);

/// One open panel connection; released when dropped. Without these limits
/// one host could open sockets (or send a request a byte at a time) until
/// the process ran out of file descriptors, taking IRC down with it.
struct WebSlot(std::net::IpAddr);

impl WebSlot {
    fn acquire(ip: std::net::IpAddr) -> Option<WebSlot> {
        let mut g = WEB_CONNS.lock().unwrap_or_else(|e| e.into_inner());
        let (total, per_ip) = &mut *g;
        if *total >= WEB_MAX_CONNECTIONS {
            return None;
        }
        if !ip.is_loopback() {
            let n = per_ip.entry(ip).or_insert(0);
            if *n >= WEB_MAX_PER_IP {
                return None;
            }
            *n += 1;
        }
        *total += 1;
        Some(WebSlot(ip))
    }
}

impl Drop for WebSlot {
    fn drop(&mut self) {
        let mut g = WEB_CONNS.lock().unwrap_or_else(|e| e.into_inner());
        let (total, per_ip) = &mut *g;
        *total = total.saturating_sub(1);
        if !self.0.is_loopback() {
            if let Some(n) = per_ip.get_mut(&self.0) {
                *n -= 1;
                if *n == 0 {
                    per_ip.remove(&self.0);
                }
            }
        }
    }
}

/// A panel connection with its slot and an idle timeout.
struct Limited<IO> {
    io: IO,
    _slot: WebSlot,
    idle: std::pin::Pin<Box<tokio::time::Sleep>>,
    limit: std::time::Duration,
}

impl<IO> Limited<IO> {
    fn new(io: IO, slot: WebSlot) -> Self {
        Self::with_idle(io, slot, WEB_IDLE)
    }

    fn with_idle(io: IO, slot: WebSlot, limit: std::time::Duration) -> Self {
        Limited {
            io,
            _slot: slot,
            idle: Box::pin(tokio::time::sleep(limit)),
            limit,
        }
    }

    fn touch(&mut self) {
        self.idle
            .as_mut()
            .reset(tokio::time::Instant::now() + self.limit);
    }

    fn timed_out(&mut self, cx: &mut std::task::Context<'_>) -> bool {
        std::future::Future::poll(self.idle.as_mut(), cx).is_ready()
    }
}

impl<IO: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for Limited<IO> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        use std::task::Poll;
        match std::pin::Pin::new(&mut self.io).poll_read(cx, buf) {
            Poll::Ready(r) => {
                self.touch();
                Poll::Ready(r)
            }
            Poll::Pending if self.timed_out(cx) => {
                Poll::Ready(Err(std::io::ErrorKind::TimedOut.into()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<IO: tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite for Limited<IO> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        use std::task::Poll;
        match std::pin::Pin::new(&mut self.io).poll_write(cx, buf) {
            Poll::Ready(r) => {
                self.touch();
                Poll::Ready(r)
            }
            Poll::Pending if self.timed_out(cx) => {
                Poll::Ready(Err(std::io::ErrorKind::TimedOut.into()))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.io).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.io).poll_shutdown(cx)
    }
}

/// Plain-HTTP listener with the connection limits.
struct PlainListener {
    inner: TcpListener,
}

impl axum::serve::Listener for PlainListener {
    type Io = Limited<tokio::net::TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.inner.accept().await {
                Ok((tcp, peer)) => match WebSlot::acquire(peer.ip()) {
                    Some(slot) => return (Limited::new(tcp, slot), peer),
                    None => drop(tcp),
                },
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}

type TlsIo = Limited<tokio_rustls::server::TlsStream<tokio::net::TcpStream>>;

/// TLS listener that performs handshakes concurrently, so a slow client
/// cannot stall the accept loop.
struct TlsListener {
    rx: mpsc::Receiver<(TlsIo, SocketAddr)>,
    addr: SocketAddr,
}

impl axum::serve::Listener for TlsListener {
    type Io = TlsIo;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.rx.recv().await {
            Some(v) => v,
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Ok(self.addr)
    }
}

/// Browsers handed over by the IRC listener of a shared port (`[web]
/// shared = true`), already past TLS.
struct SharedListener {
    rx: mpsc::Receiver<(crate::util::BoxStream, SocketAddr)>,
    addr: SocketAddr,
}

impl axum::serve::Listener for SharedListener {
    type Io = Limited<crate::util::BoxStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.rx.recv().await {
                Some((io, peer)) => {
                    if let Some(slot) = WebSlot::acquire(peer.ip()) {
                        return (Limited::new(io, slot), peer);
                    }
                }
                None => std::future::pending::<()>().await,
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Ok(self.addr)
    }
}

/// Where the IRC listener of a shared port sends browsers.
pub type SharedSender = mpsc::Sender<(crate::util::BoxStream, SocketAddr)>;
pub type SharedReceiver = mpsc::Receiver<(crate::util::BoxStream, SocketAddr)>;

/// Answers a plain-HTTP request that reached the TLS port with a redirect to
/// the same address over https, instead of a page of TLS garbage.
pub(crate) async fn redirect_to_https(mut tcp: tokio::net::TcpStream, port: u16) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut buf = Vec::with_capacity(2048);
    let mut chunk = [0u8; 1024];
    let read = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while buf.len() < 8192 && !buf.windows(4).any(|w| w == b"\r\n\r\n") {
            match tcp.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
            }
        }
    })
    .await;
    if read.is_err() {
        return;
    }
    let head = String::from_utf8_lossy(&buf);
    if let Some(location) = https_location(&head, port) {
        let body = format!("Use HTTPS: {location}\n");
        let resp = format!(
            "HTTP/1.1 301 Moved Permanently\r\nLocation: {location}\r\n\
             Content-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = tcp.write_all(resp.as_bytes()).await;
        let _ = tcp.shutdown().await;
    }
}

/// `https://<host><path>` for an HTTP request head, or None if it is not a
/// plausible HTTP request. Host and path are restricted to safe characters
/// so nothing from the request can inject headers.
fn https_location(head: &str, port: u16) -> Option<String> {
    let mut lines = head.split("\r\n");
    let mut req = lines.next()?.split(' ');
    let method = req.next()?;
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) {
        return None;
    }
    let path = req.next().unwrap_or("/");
    let path_ok =
        path.starts_with('/') && path.len() <= 2048 && path.bytes().all(|b| b.is_ascii_graphic());
    let path = if path_ok { path } else { "/" };
    let host = lines
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case("host")
                .then(|| v.trim().to_string())
        })
        .filter(|h| {
            !h.is_empty()
                && h.len() <= 255
                && h.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".-:[]".contains(&b))
        })?;
    // Keep the port the browser used (it is the TLS port).
    let has_port = host.rsplit_once(':').is_some_and(|(h, p)| {
        !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && !h.ends_with(':')
    }) && !host.ends_with(']');
    let host = if has_port || port == 443 {
        host
    } else {
        format!("{host}:{port}")
    };
    Some(format!("https://{host}{path}"))
}

/// Builds the router (used by `serve` and by tests).
pub fn router(ctx: Arc<Ctx>, cfg: WebConfig) -> Router {
    let st = Arc::new(WebState {
        ctx,
        cfg,
        rate: Mutex::new(HashMap::new()),
        flows: Mutex::new(HashMap::new()),
    });
    Router::new()
        .nest(
            "/api/v1",
            api::routes()
                .merge(plugins_api::routes())
                .merge(chat_api::routes())
                .merge(admin_api::routes())
                .merge(auth_api::routes()),
        )
        .route("/api/v1/ws", axum::routing::get(ws::handler))
        .fallback(static_files)
        .layer(middleware::from_fn_with_state(st.clone(), guard))
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY))
        .with_state(st)
}

/// Binds the web listener and serves the panel until shutdown.
pub async fn serve(
    ctx: Arc<Ctx>,
    cfg: &WebConfig,
    mut shutdown: watch::Receiver<bool>,
    shared: Option<(SharedReceiver, SocketAddr)>,
) -> Result<(SocketAddr, JoinHandle<()>)> {
    if let Some((rx, addr)) = shared {
        // The IRC listener owns the port and its TLS; browsers come here.
        let app = router(ctx, cfg.clone()).into_make_service_with_connect_info::<SocketAddr>();
        info!(address = %addr, "web panel on the shared IRC port");
        let signal = async move {
            let _ = shutdown.changed().await;
        };
        let sl = axum::serve::ListenerExt::tap_io(SharedListener { rx, addr }, |_| {});
        let task = tokio::spawn(async move {
            if let Err(e) = axum::serve(sl, app).with_graceful_shutdown(signal).await {
                warn!(error = %e, "web server stopped");
            }
        });
        return Ok((addr, task));
    }
    let listener = TcpListener::bind(&cfg.address)
        .await
        .with_context(|| format!("cannot listen on {} for the web panel", cfg.address))?;
    let addr = listener.local_addr()?;
    let app = router(ctx, cfg.clone()).into_make_service_with_connect_info::<SocketAddr>();
    let tls = match (&cfg.tls_cert, &cfg.tls_key) {
        (Some(c), Some(k)) => Some(crate::server::tls_acceptor_from(c, k)?),
        _ => None,
    };
    info!(address = %addr, tls = tls.is_some(), "web panel listening");
    let signal = async move {
        let _ = shutdown.changed().await;
    };
    let task = match tls {
        None => tokio::spawn(async move {
            let pl = axum::serve::ListenerExt::tap_io(PlainListener { inner: listener }, |_| {});
            if let Err(e) = axum::serve(pl, app).with_graceful_shutdown(signal).await {
                warn!(error = %e, "web server stopped");
            }
        }),
        Some(acceptor) => {
            let (tx, rx) = mpsc::channel(64);
            tokio::spawn(async move {
                loop {
                    let Ok((tcp, peer)) = listener.accept().await else {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        continue;
                    };
                    let Some(slot) = WebSlot::acquire(peer.ip()) else {
                        drop(tcp);
                        continue;
                    };
                    let acc = acceptor.clone();
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let mut first = [0u8; 1];
                        let peek = tokio::time::timeout(
                            std::time::Duration::from_secs(10),
                            tcp.peek(&mut first),
                        )
                        .await;
                        if !matches!(peek, Ok(Ok(1))) {
                            return;
                        }
                        if first[0] != 0x16 {
                            // Plain HTTP on the TLS port: send the browser to https://.
                            redirect_to_https(tcp, addr.port()).await;
                            return;
                        }
                        if let Ok(Ok(s)) = tokio::time::timeout(
                            std::time::Duration::from_secs(10),
                            acc.accept(tcp),
                        )
                        .await
                        {
                            let _ = tx.send((Limited::new(s, slot), peer)).await;
                        }
                    });
                }
            });
            // tap_io gives the listener axum's ConnectInfo<SocketAddr> support.
            let tl = axum::serve::ListenerExt::tap_io(TlsListener { rx, addr }, |_| {});
            tokio::spawn(async move {
                if let Err(e) = axum::serve(tl, app).with_graceful_shutdown(signal).await {
                    warn!(error = %e, "web server stopped");
                }
            })
        }
    };
    Ok((addr, task))
}

#[cfg(test)]
mod tests {

    #[test]
    fn web_connection_limits() {
        let ip: std::net::IpAddr = "198.51.100.9".parse().unwrap();
        let held: Vec<_> = (0..WEB_MAX_PER_IP)
            .map(|_| WebSlot::acquire(ip).unwrap())
            .collect();
        assert!(WebSlot::acquire(ip).is_none());
        assert!(WebSlot::acquire("198.51.100.10".parse().unwrap()).is_some());
        drop(held);
        assert!(WebSlot::acquire(ip).is_some());
    }

    #[tokio::test]
    async fn idle_panel_connections_are_closed() {
        use tokio::io::AsyncReadExt;
        let (a, _b) = tokio::io::duplex(64);
        let slot = WebSlot::acquire("198.51.100.11".parse().unwrap()).unwrap();
        let mut l = Limited::with_idle(a, slot, std::time::Duration::from_millis(50));
        let mut buf = [0u8; 8];
        let err = l.read(&mut buf).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    }

    use super::*;

    #[test]
    fn https_redirect_location() {
        let req =
            |h: &str| format!("GET /networks/1?x=2 HTTP/1.1\r\nHost: {h}\r\nAccept: */*\r\n\r\n");
        assert_eq!(
            https_location(&req("192.168.1.139:8443"), 8443).unwrap(),
            "https://192.168.1.139:8443/networks/1?x=2"
        );
        assert_eq!(
            https_location(&req("[fd00::5]:8443"), 8443).unwrap(),
            "https://[fd00::5]:8443/networks/1?x=2"
        );
        assert_eq!(
            https_location(&req("[fd00::5]"), 8443).unwrap(),
            "https://[fd00::5]:8443/networks/1?x=2"
        );
        assert_eq!(
            https_location(&req("bnc.example.org"), 443).unwrap(),
            "https://bnc.example.org/networks/1?x=2"
        );
        // Header injection and junk are refused.
        assert!(
            https_location(&req("evil.org\r\nSet-Cookie: a=b"), 8443).is_none()
                || !https_location(&req("evil.org\r\nSet-Cookie: a=b"), 8443)
                    .unwrap()
                    .contains('\n')
        );
        assert!(https_location(&req("a b"), 8443).is_none());
        assert!(https_location("\x16\x03\x01garbage", 8443).is_none());
        assert_eq!(
            https_location("GET http://x/\r\nHost: h:1\r\n\r\n", 1).unwrap(),
            "https://h:1/"
        );
    }

    #[test]
    fn tokens_and_codes() {
        let t = random_token();
        assert_eq!(t.len(), 43);
        assert_ne!(t, random_token());
        assert_eq!(hash_token("abc").len(), 64);
        let code = generate_setup_token();
        assert_eq!(code.len(), 14);
        assert!(ct_eq("abc", "abc") && !ct_eq("abc", "abd") && !ct_eq("abc", "ab"));
    }

    fn cfg() -> WebConfig {
        WebConfig {
            address: "127.0.0.1:0".into(),
            tls_cert: None,
            tls_key: None,
            insecure: false,
            secure_cookies: None,
            public_url: Some("https://bnc.example.org".into()),
            session_hours: 12,
            shared: false,
            trust_proxy: true,
        }
    }

    #[test]
    fn origin_checks() {
        let c = cfg();
        let mut h = HeaderMap::new();
        assert!(origin_ok(&c, &h));
        h.insert(header::HOST, HeaderValue::from_static("localhost:8080"));
        h.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://localhost:8080"),
        );
        assert!(origin_ok(&c, &h));
        h.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://evil.example"),
        );
        assert!(!origin_ok(&c, &h));
        h.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://bnc.example.org"),
        );
        assert!(origin_ok(&c, &h));
    }

    #[test]
    fn proxy_ip() {
        let c = cfg();
        let mut h = HeaderMap::new();
        h.insert(
            "x-forwarded-for",
            HeaderValue::from_static("198.51.100.7, 203.0.113.9"),
        );
        let local: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let remote: SocketAddr = "192.0.2.1:5000".parse().unwrap();
        assert_eq!(client_ip(&c, local, &h).to_string(), "203.0.113.9");
        assert_eq!(
            client_ip(&c, remote, &h).to_string(),
            "192.0.2.1",
            "only a local proxy is trusted"
        );
    }

    #[test]
    fn cookies() {
        let mut c = cfg();
        assert!(!session_cookie(&c, "t").to_str().unwrap().contains("Secure"));
        c.secure_cookies = Some(true);
        let v = session_cookie(&c, "t");
        let v = v.to_str().unwrap();
        assert!(v.contains("HttpOnly") && v.contains("SameSite=Strict") && v.contains("Secure"));
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            HeaderValue::from_static("a=b; xbnc_session=tok123"),
        );
        assert_eq!(cookie_token(&h).as_deref(), Some("tok123"));
    }

    #[test]
    fn openapi_documents_every_route() {
        let src = concat!(
            include_str!("api.rs"),
            include_str!("plugins_api.rs"),
            include_str!("chat_api.rs"),
            include_str!("admin_api.rs"),
            include_str!("auth_api.rs"),
            include_str!("sso.rs")
        );
        let spec: serde_json::Value =
            serde_json::from_str(include_str!("../../../../docs/openapi.json")).unwrap();
        let mut n = 0;
        for part in src.split(".route(").skip(1) {
            // Both `.route("/x", ...)` and rustfmt's multi-line form.
            let Some(rest) = part.trim_start().strip_prefix('"') else {
                continue;
            };
            let path = rest.split('"').next().unwrap();
            if path == "/openapi.json" {
                continue;
            }
            assert!(
                spec["paths"].get(path).is_some(),
                "route {path} missing from docs/openapi.json"
            );
            n += 1;
        }
        assert!(n > 25);
    }
}
