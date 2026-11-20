//! Superadministrator tools: backups (create, download, restore, delete),
//! restarting the service and per-user quotas.

use super::{audit, ApiError, ApiResult, AppState, Auth};
use crate::backup;
use crate::db;
use crate::rbac::Perm;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/backups", get(list).post(create))
        .route(
            "/backups/restore",
            post(upload_restore)
                .delete(cancel_restore)
                .layer(DefaultBodyLimit::disable()),
        )
        .route("/backups/{name}", get(download).delete(remove))
        .route("/backups/{name}/restore", post(restore_existing))
        .route("/system/restart", post(restart))
        .route("/users/{id}/usage", get(user_usage))
}

/// Backups hold every credential: superadministrators only, from a session.
fn superadmin(a: &Auth) -> ApiResult<()> {
    a.require_session()?;
    a.require(Perm::ManageUsers)?;
    a.require(Perm::EditSettings)
}

async fn list(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    superadmin(&a)?;
    let cfg = st.ctx.config.clone();
    let (items, pending) =
        tokio::task::spawn_blocking(move || (backup::list(&cfg), backup::restore_pending(&cfg)))
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
    let postgres = st.ctx.db.is_postgres();
    Ok(Json(json!({
        "postgres": postgres,
        "backups": items,
        "restore_pending": pending,
        "folder": backup::backups_dir(&st.ctx.config).display().to_string(),
        "settings": st.ctx.settings.get().backups,
    })))
}

async fn create(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    superadmin(&a)?;
    let ctx = st.ctx.clone();
    let by = a.user.username.clone();
    let info = tokio::task::spawn_blocking(move || backup::create(&ctx.config, &ctx.db, &by))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(|e| ApiError::bad(format!("Cannot create the backup: {e:#}")))?;
    let keep = st.ctx.settings.get().backups.keep as usize;
    let cfg = st.ctx.config.clone();
    if st.ctx.settings.get().backups.enabled {
        let _ = tokio::task::spawn_blocking(move || backup::rotate(&cfg, keep)).await;
    }
    audit(&st.ctx, &a.user, "backup.create", info.name.clone()).await;
    Ok(Json(json!(info)))
}

async fn download(
    State(st): State<AppState>,
    a: Auth,
    Path(name): Path<String>,
) -> ApiResult<Response> {
    superadmin(&a)?;
    if !backup::valid_name(&name) {
        return Err(ApiError::not_found("Backup"));
    }
    let path = backup::backups_dir(&st.ctx.config).join(&name);
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| ApiError::not_found("Backup"))?;
    let len = file.metadata().await.map(|m| m.len()).unwrap_or(0);
    audit(&st.ctx, &a.user, "backup.download", name.clone()).await;
    let stream = tokio_util::io::ReaderStream::new(file);
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/gzip")
        .header(header::CONTENT_LENGTH, len)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{name}\""),
        )
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()))
}

async fn remove(
    State(st): State<AppState>,
    a: Auth,
    Path(name): Path<String>,
) -> ApiResult<Json<Value>> {
    superadmin(&a)?;
    let cfg = st.ctx.config.clone();
    let n2 = name.clone();
    let found = tokio::task::spawn_blocking(move || backup::delete(&cfg, &n2))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .map_err(|e| ApiError::bad(e.to_string()))?;
    if !found {
        return Err(ApiError::not_found("Backup"));
    }
    audit(&st.ctx, &a.user, "backup.delete", name).await;
    Ok(Json(json!({ "ok": true })))
}

async fn stage(
    st: &AppState,
    a: &Auth,
    path: std::path::PathBuf,
    what: String,
) -> ApiResult<Json<Value>> {
    let cfg = st.ctx.config.clone();
    let p2 = path.clone();
    let res = tokio::task::spawn_blocking(move || backup::stage_restore(&cfg, &p2))
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let m = res.map_err(|e| ApiError::bad(format!("Cannot restore this backup: {e:#}")))?;
    audit(
        &st.ctx,
        &a.user,
        "backup.restore",
        format!("{what} (from {}, made by {})", m.version, m.created_by),
    )
    .await;
    Ok(Json(json!({ "staged": true, "manifest": m })))
}

/// Restore from an uploaded archive (raw body, application/gzip).
async fn upload_restore(State(st): State<AppState>, a: Auth, body: Body) -> ApiResult<Json<Value>> {
    superadmin(&a)?;
    let dir = backup::backups_dir(&st.ctx.config);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let path = dir.join(format!(".upload-{}.tar.gz", db::now_ms()));
    let mut f = tokio::fs::File::create(&path)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let mut stream = body.into_data_stream();
    let mut total: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => {
                let _ = tokio::fs::remove_file(&path).await;
                return Err(ApiError::bad(format!("Upload interrupted: {e}")));
            }
        };
        total += chunk.len() as u64;
        if total > backup::MAX_RESTORE_BYTES {
            let _ = tokio::fs::remove_file(&path).await;
            return Err(ApiError::bad("The file is too large"));
        }
        if f.write_all(&chunk).await.is_err() {
            let _ = tokio::fs::remove_file(&path).await;
            return Err(ApiError::internal("Cannot write the upload".to_string()));
        }
    }
    let _ = f.flush().await;
    drop(f);
    let r = stage(&st, &a, path.clone(), "uploaded archive".into()).await;
    let _ = tokio::fs::remove_file(&path).await;
    r
}

async fn restore_existing(
    State(st): State<AppState>,
    a: Auth,
    Path(name): Path<String>,
) -> ApiResult<Json<Value>> {
    superadmin(&a)?;
    if !backup::valid_name(&name) {
        return Err(ApiError::not_found("Backup"));
    }
    let path = backup::backups_dir(&st.ctx.config).join(&name);
    if !path.exists() {
        return Err(ApiError::not_found("Backup"));
    }
    stage(&st, &a, path, name).await
}

async fn cancel_restore(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    superadmin(&a)?;
    let cfg = st.ctx.config.clone();
    let _ = tokio::task::spawn_blocking(move || backup::cancel_restore(&cfg)).await;
    audit(&st.ctx, &a.user, "backup.restore_cancel", String::new()).await;
    Ok(Json(json!({ "ok": true })))
}

async fn restart(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    superadmin(&a)?;
    audit(&st.ctx, &a.user, "system.restart", String::new()).await;
    let ctx = st.ctx.clone();
    tokio::spawn(async move {
        // Let the answer reach the browser first.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        ctx.restart.notify_one();
    });
    Ok(Json(json!({ "restarting": true })))
}

/// What a user is using against their quotas.
async fn user_usage(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    if id != a.user.id {
        a.require(Perm::ViewUsers)?;
    }
    let ctx = st.ctx.clone();
    let (user, usage) = st
        .ctx
        .db
        .call(move |c| {
            let u = db::get_user_by_id(c, id)?.ok_or_else(|| anyhow::anyhow!("User not found"))?;
            Ok((u, db::user_usage(c, id)?))
        })
        .await
        .map_err(|_| ApiError::not_found("User"))?;
    let limits = ctx.settings.get().limits;
    Ok(Json(json!({
        "networks": usage.networks,
        "max_networks": user.max_networks.map(|m| m as usize).unwrap_or(limits.max_networks_per_user),
        "clients": ctx.registry.clients_of(id),
        "max_clients": user.max_clients.map(|m| m as usize).unwrap_or(limits.max_clients_per_user),
        "history_messages": usage.messages,
        "history_bytes": usage.bytes,
        "max_history_mb": user.max_history_mb,
    })))
}
