//! OpenID Connect sign-in and linking of outside identities.

use super::api::{record_web_login, start_session};
use super::{audit, client_ip, random_token, ApiError, ApiResult, AppState, Auth, Flow};
use crate::db;
use crate::sso::{self, Provision};
use axum::extract::{ConnectInfo, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::time::Instant;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/oidc/start", get(start))
        .route("/auth/oidc/callback", get(callback))
        .route("/me/identities", get(identities).delete(unlink))
        .route("/me/identities/oidc", post(link_start))
}

fn redirect_uri(st: &AppState) -> ApiResult<String> {
    let base = st
        .cfg
        .public_url
        .as_deref()
        .ok_or_else(|| ApiError::bad("Single sign-on needs web.public_url"))?;
    Ok(format!(
        "{}/api/v1/auth/oidc/callback",
        base.trim_end_matches('/')
    ))
}

/// Starts a sign-in (or, with a user, a link) and returns the provider URL.
async fn begin(st: &AppState, user_id: Option<i64>) -> ApiResult<String> {
    let cfg = st
        .ctx
        .config
        .oidc
        .as_ref()
        .ok_or_else(|| ApiError::not_found("Single sign-on"))?;
    let redirect = redirect_uri(st)?;
    let p = st
        .ctx
        .oidc
        .provider(cfg, st.ctx.tls_client.clone(), false)
        .await
        .map_err(|e| ApiError::bad(format!("Cannot reach the sign-in provider: {e:#}")))?;
    let (state, nonce, verifier) = (random_token(), random_token(), random_token());
    st.put_flow(
        state.clone(),
        Flow {
            kind: "oidc",
            user_id,
            data: verifier.clone().into_bytes(),
            extra: nonce.clone(),
            at: Instant::now(),
        },
    );
    Ok(sso::authorize_url(
        &p, cfg, &redirect, &state, &nonce, &verifier,
    ))
}

fn found(url: &str) -> Response {
    let mut r = StatusCode::FOUND.into_response();
    if let Ok(v) = HeaderValue::from_str(url) {
        r.headers_mut().insert(header::LOCATION, v);
    }
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

/// Back to the panel. A meta refresh (not a redirect) so that the browser
/// treats the next page as same-site and sends the new session cookie.
fn back_to_panel(path: &str) -> Response {
    let path: String = path
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "/?=&%-_.".contains(*c))
        .collect();
    let html = format!(
        "<!doctype html><meta charset=utf-8><meta http-equiv=refresh content=\"0;url={path}\"><title>xBNC</title><a href=\"{path}\">xBNC</a>"
    );
    let mut r = (StatusCode::OK, html).into_response();
    r.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

fn fail(msg: &str) -> Response {
    back_to_panel(&format!("/?sso_error={}", sso::url_encode(msg)))
}

async fn start(State(st): State<AppState>) -> Response {
    match begin(&st, None).await {
        Ok(url) => found(&url),
        Err(e) => fail(&e.message),
    }
}

async fn link_start(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    a.require_session()?;
    Ok(Json(json!({ "url": begin(&st, Some(a.user.id)).await? })))
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn callback(
    State(st): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let ip = client_ip(&st.cfg, peer, &headers);
    if st.ctx.limiter.is_blocked(ip) {
        return fail(&st.ctx.limiter.blocked_message(ip));
    }
    let Some(flow) = q.state.as_deref().and_then(|s| st.take_flow(s, "oidc")) else {
        return fail("The sign-in took too long or was already used; try again");
    };
    if let Some(e) = q.error {
        return fail(&format!(
            "The provider cancelled the sign-in: {}",
            q.error_description.unwrap_or(e)
        ));
    }
    let (Some(code), Some(cfg)) = (q.code, st.ctx.config.oidc.clone()) else {
        return fail("The provider did not return a code");
    };
    let Ok(redirect) = redirect_uri(&st) else {
        return fail("Single sign-on needs web.public_url");
    };
    let verifier = String::from_utf8_lossy(&flow.data).to_string();
    let claims = match sso::exchange_code(
        &st.ctx.oidc,
        &cfg,
        st.ctx.tls_client.clone(),
        &code,
        &redirect,
        &verifier,
        &flow.extra,
    )
    .await
    {
        Ok(c) => c,
        Err(e) => {
            tracing::info!(error = %e, "OpenID Connect sign-in refused");
            record_web_login(&st.ctx, ip, "?", false, "oidc refused").await;
            return fail(&format!("{e:#}"));
        }
    };
    let subject = claims["sub"].as_str().unwrap_or_default().to_string();

    // Linking from the profile.
    if let Some(uid) = flow.user_id {
        let s2 = subject.clone();
        let res = st
            .ctx
            .db
            .call(move |c| {
                if let Some(other) = db::identity_user(c, "oidc", &s2)? {
                    if other != uid {
                        anyhow::bail!("that account is already linked to another xBNC user");
                    }
                }
                db::link_identity(c, "oidc", &s2, uid)?;
                db::get_user_by_id(c, uid)
            })
            .await;
        return match res {
            Ok(Some(u)) => {
                audit(&st.ctx, &u, "sso.link", format!("oidc {subject}")).await;
                back_to_panel("/profile?sso=linked")
            }
            Ok(None) => fail("User not found"),
            Err(e) => back_to_panel(&format!(
                "/profile?sso_error={}",
                sso::url_encode(&e.to_string())
            )),
        };
    }

    let username = claims[cfg.username_claim.as_str()]
        .as_str()
        .or(claims["email"].as_str())
        .and_then(sso::username_from);
    let prov = sso::provision(
        &st.ctx.db,
        "oidc",
        &subject,
        username,
        cfg.auto_create,
        cfg.link_existing,
        &cfg.default_role,
    )
    .await;
    let user = match prov {
        Ok(Provision::User(u)) => *u,
        Ok(Provision::NotLinked) => {
            record_web_login(&st.ctx, ip, "?", false, "oidc: not linked").await;
            return fail("No xBNC account is linked to this identity. Sign in with your password and link it from your profile, or ask an administrator.");
        }
        Ok(Provision::NameTaken(n)) => {
            record_web_login(&st.ctx, ip, &n, false, "oidc: name taken").await;
            return fail(&format!("A local user named {n} already exists. Sign in with your password and link it from your profile."));
        }
        Err(e) => return fail(&format!("{e:#}")),
    };
    if !user.enabled {
        record_web_login(&st.ctx, ip, &user.username, false, "account disabled").await;
        return fail("This account is disabled");
    }
    st.ctx.limiter.record_success(ip);
    record_web_login(&st.ctx, ip, &user.username, true, "oidc").await;
    match start_session(&st, user, ip, &headers).await {
        Ok(r) => {
            let mut out = back_to_panel("/");
            if let Some(c) = r.headers().get(header::SET_COOKIE) {
                out.headers_mut().insert(header::SET_COOKIE, c.clone());
            }
            out
        }
        Err(e) => fail(&e.message),
    }
}

async fn identities(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    let uid = a.user.id;
    let list = st.ctx.db.call(move |c| db::identities_of(c, uid)).await?;
    Ok(Json(json!(list
        .into_iter()
        .map(|(provider, subject, created_at)| json!({ "provider": provider, "subject": subject, "created_at": created_at }))
        .collect::<Vec<_>>())))
}

#[derive(Deserialize)]
struct UnlinkReq {
    provider: String,
    subject: String,
}

async fn unlink(
    State(st): State<AppState>,
    a: Auth,
    Json(req): Json<UnlinkReq>,
) -> ApiResult<Json<Value>> {
    a.require_session()?;
    let uid = a.user.id;
    let (p, s) = (req.provider.clone(), req.subject.clone());
    if !st
        .ctx
        .db
        .call(move |c| db::unlink_identity(c, uid, &p, &s))
        .await?
    {
        return Err(ApiError::not_found("Linked account"));
    }
    audit(
        &st.ctx,
        &a.user,
        "sso.unlink",
        format!("{} {}", req.provider, req.subject),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}
