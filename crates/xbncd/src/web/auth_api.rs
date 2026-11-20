//! Passwordless and single sign-on: passkeys (WebAuthn) and OpenID Connect.

use super::api::{record_web_login, start_session};
use super::{audit, client_ip, random_token, ApiError, ApiResult, AppState, Auth, Flow};
use crate::db;
use crate::webauthn::{self, PublicKey};
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::time::Instant;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/methods", get(methods))
        .route("/auth/passkey/options", post(login_options))
        .route("/auth/passkey", post(login_passkey))
        .route("/me/passkeys", get(list_passkeys).post(add_passkey))
        .route("/me/passkeys/options", post(register_options))
        .route("/me/passkeys/{id}", delete(remove_passkey))
        .merge(super::sso::routes())
}

/// The sign-in methods the login page should offer.
async fn methods(State(st): State<AppState>) -> Json<Value> {
    let cfg = &st.ctx.config;
    Json(json!({
        "passkeys": true,
        "oidc": cfg.oidc.as_ref().map(|o| json!({ "name": o.display_name() })),
        "ldap": cfg.ldap.is_some(),
    }))
}

/// The site a passkey belongs to, from the (already checked) Origin header.
fn site(headers: &HeaderMap) -> ApiResult<(String, String)> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::bad("Passkeys need a browser request (no Origin header)"))?
        .trim_end_matches('/')
        .to_string();
    let host = origin.split_once("://").map(|(_, h)| h).unwrap_or("");
    let rp_id = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or("").to_string()
    } else {
        host.rsplit_once(':')
            .map(|(h, _)| h)
            .unwrap_or(host)
            .to_string()
    };
    if rp_id.is_empty() {
        return Err(ApiError::bad("Invalid Origin header"));
    }
    if rp_id.parse::<std::net::IpAddr>().is_ok() {
        return Err(ApiError::bad(
            "Passkeys need the panel to be opened by name (for example https://bnc.example.org or http://localhost), not by IP address",
        ));
    }
    Ok((origin, rp_id))
}

fn challenge() -> Vec<u8> {
    use ring::rand::SecureRandom;
    let mut c = vec![0u8; 32];
    ring::rand::SystemRandom::new()
        .fill(&mut c)
        .expect("random challenge");
    c
}

async fn register_options(
    State(st): State<AppState>,
    a: Auth,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    a.require_session()?;
    let (_, rp_id) = site(&headers)?;
    let uid = a.user.id;
    let existing = st.ctx.db.call(move |c| db::passkeys_of(c, uid)).await?;
    let ch = challenge();
    let id = random_token();
    st.put_flow(
        id.clone(),
        Flow {
            kind: "passkey.register",
            user_id: Some(uid),
            data: ch.clone(),
            extra: rp_id.clone(),
            at: Instant::now(),
        },
    );
    Ok(Json(json!({
        "id": id,
        "publicKey": {
            "challenge": B64URL.encode(&ch),
            "rp": { "id": rp_id, "name": format!("xBNC {}", st.ctx.config.hostname) },
            "user": {
                "id": B64URL.encode(uid.to_be_bytes()),
                "name": a.user.username,
                "displayName": a.user.username,
            },
            "pubKeyCredParams": [ { "type": "public-key", "alg": -7 }, { "type": "public-key", "alg": -257 } ],
            "timeout": 300000,
            "attestation": "none",
            "authenticatorSelection": { "residentKey": "required", "requireResidentKey": true, "userVerification": "required" },
            "excludeCredentials": existing.iter().map(|p| json!({ "type": "public-key", "id": p.cred_id })).collect::<Vec<_>>(),
        }
    })))
}

#[derive(Deserialize)]
struct RegisterReq {
    id: String,
    #[serde(default)]
    label: String,
    client_data_json: String,
    attestation_object: String,
}

async fn add_passkey(
    State(st): State<AppState>,
    a: Auth,
    headers: HeaderMap,
    Json(req): Json<RegisterReq>,
) -> ApiResult<Json<Value>> {
    a.require_session()?;
    let (origin, rp_id) = site(&headers)?;
    let flow = st
        .take_flow(&req.id, "passkey.register")
        .filter(|f| f.user_id == Some(a.user.id) && f.extra == rp_id)
        .ok_or_else(|| ApiError::bad("The passkey request expired; try again"))?;
    let (cred, key, count) = webauthn::verify_registration(
        &req.client_data_json,
        &req.attestation_object,
        &flow.data,
        &origin,
        &rp_id,
    )
    .map_err(|e| ApiError::bad(format!("The passkey could not be verified: {e}")))?;
    let label: String = req.label.trim().chars().take(60).collect();
    let label = if label.is_empty() {
        "Passkey".to_string()
    } else {
        label
    };
    let (uid, cred_b64, enc, l2) = (a.user.id, B64URL.encode(&cred), key.encode(), label.clone());
    let id = st
        .ctx
        .db
        .call(move |c| db::add_passkey(c, uid, &cred_b64, &enc, count as i64, &l2))
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    audit(&st.ctx, &a.user, "passkey.add", label).await;
    Ok(Json(json!({ "id": id })))
}

async fn list_passkeys(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    let uid = a.user.id;
    let list = st.ctx.db.call(move |c| db::passkeys_of(c, uid)).await?;
    Ok(Json(json!(list)))
}

async fn remove_passkey(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    a.require_session()?;
    let uid = a.user.id;
    if !st
        .ctx
        .db
        .call(move |c| db::delete_passkey(c, uid, id))
        .await?
    {
        return Err(ApiError::not_found("Passkey"));
    }
    audit(&st.ctx, &a.user, "passkey.delete", id.to_string()).await;
    Ok(Json(json!({ "ok": true })))
}

async fn login_options(State(st): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let (_, rp_id) = site(&headers)?;
    let ch = challenge();
    let id = random_token();
    st.put_flow(
        id.clone(),
        Flow {
            kind: "passkey.login",
            user_id: None,
            data: ch.clone(),
            extra: rp_id.clone(),
            at: Instant::now(),
        },
    );
    Ok(Json(json!({
        "id": id,
        "publicKey": {
            "challenge": B64URL.encode(&ch),
            "rpId": rp_id,
            "timeout": 300000,
            "userVerification": "required",
            "allowCredentials": [],
        }
    })))
}

#[derive(Deserialize)]
struct LoginReq {
    id: String,
    credential_id: String,
    client_data_json: String,
    authenticator_data: String,
    signature: String,
}

async fn login_passkey(
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
    let (origin, rp_id) = site(&headers)?;
    let flow = st
        .take_flow(&req.id, "passkey.login")
        .filter(|f| f.extra == rp_id)
        .ok_or_else(|| ApiError::bad("The passkey request expired; try again"))?;
    let cred = req.credential_id.trim_end_matches('=').to_string();
    let found = st
        .ctx
        .db
        .call(move |c| {
            let Some(pk) = db::passkey_by_cred(c, &cred)? else {
                return Ok(None);
            };
            let user = db::get_user_by_id(c, pk.user_id)?;
            Ok(user.map(|u| (pk, u)))
        })
        .await?;
    let Some((pk, user)) = found else {
        st.ctx.limiter.record_failure(ip);
        record_web_login(&st.ctx, ip, "?", false, "unknown passkey").await;
        return Err(ApiError::unauthorized(
            "bad_passkey",
            "This passkey is not registered here",
        ));
    };
    let key = PublicKey::decode(&pk.public_key).map_err(|e| ApiError::internal(e.to_string()))?;
    let verified = webauthn::verify_assertion(
        &key,
        pk.sign_count as u32,
        &req.client_data_json,
        &req.authenticator_data,
        &req.signature,
        &flow.data,
        &origin,
        &rp_id,
    );
    let (count, uv) = match verified {
        Ok(v) => v,
        Err(e) => {
            st.ctx.limiter.record_failure(ip);
            tracing::info!(error = %e, "passkey sign-in refused");
            record_web_login(&st.ctx, ip, &user.username, false, "bad passkey").await;
            return Err(ApiError::unauthorized(
                "bad_passkey",
                "The passkey could not be verified",
            ));
        }
    };
    if !uv {
        return Err(ApiError::unauthorized(
            "bad_passkey",
            "The passkey did not verify who you are (PIN, fingerprint or face)",
        ));
    }
    if !user.enabled {
        record_web_login(&st.ctx, ip, &user.username, false, "account disabled").await;
        return Err(ApiError::unauthorized(
            "account_disabled",
            "This account is disabled",
        ));
    }
    let pid = pk.id;
    st.ctx
        .db
        .call(move |c| db::touch_passkey(c, pid, count as i64))
        .await?;
    st.ctx.limiter.record_success(ip);
    record_web_login(&st.ctx, ip, &user.username, true, "passkey").await;
    // A verified passkey is already two factors (the device and its PIN or
    // biometrics), so the authenticator app code is not asked.
    start_session(&st, user, ip, &headers).await
}
