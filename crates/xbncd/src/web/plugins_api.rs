//! REST API for WebAssembly plugins: install (superadministrator) and
//! per-network use (network owner or administrator).

use super::{audit, ApiError, ApiResult, AppState, Auth};
use crate::db;
use crate::network::NetCmd;
use crate::plugins::{self, Plugin, PluginInstance};
use crate::rbac::Perm;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::sync::Arc;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/plugins",
            get(list)
                .post(install)
                .layer(DefaultBodyLimit::max(plugins::MAX_WASM_BYTES + 1024)),
        )
        .route(
            "/plugins/inspect",
            post(inspect).layer(DefaultBodyLimit::max(plugins::MAX_WASM_BYTES + 1024)),
        )
        .route("/plugins/dev", get(get_dev).put(put_dev))
        .route("/plugins/catalog", get(catalog))
        .route("/plugins/catalog/{id}", post(catalog_install))
        .route("/plugins/logs", get(logs))
        .route("/plugins/{id}", axum::routing::patch(update).delete(remove))
        .route("/networks/{id}/plugins", get(network_plugins))
        .route("/networks/{id}/plugins/{plugin}", put(put_network_plugin))
}

fn manage(a: &Auth) -> ApiResult<()> {
    a.require_session()?;
    a.require(Perm::EditSettings)
}

/// Compiles and starts the plugin once (off the async threads).
async fn check(st: &AppState, wasm: Bytes) -> ApiResult<Plugin> {
    // The shared engine: network actors instantiate the module with it.
    let e = st.ctx.plugins.engine.clone();
    let res = tokio::task::spawn_blocking(move || -> anyhow::Result<Plugin> {
        let p = Plugin::compile(&e, &wasm)?;
        let p = Arc::new(p);
        PluginInstance::new(&e, p.clone())?;
        Arc::try_unwrap(p).map_err(|_| anyhow::anyhow!("internal error"))
    })
    .await
    .map_err(|e| ApiError::bad(e.to_string()))?;
    res.map_err(|e| ApiError::bad(e.to_string()))
}

async fn list(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    let rows = st
        .ctx
        .db
        .call(|c| {
            let mut v = Vec::new();
            for p in db::list_plugins(c)? {
                let a = db::plugin_http_approved(c, &p.id)?;
                v.push((p, a));
            }
            Ok(v)
        })
        .await?;
    let can_manage = a.role.can(Perm::EditSettings);
    let list: Vec<Value> = rows
        .into_iter()
        .map(|(p, approved)| {
            json!({
                "http_approved": approved,
                "id": p.id,
                "manifest": p.manifest,
                "enabled": p.enabled,
                "loaded": st.ctx.plugins.get(&p.id).is_some(),
                "size": p.size,
                "sha256": p.sha256,
                "installed_at": p.installed_at,
                "installed_by": p.installed_by,
            })
        })
        .collect();
    Ok(Json(
        json!({ "plugins": list, "can_manage": can_manage, "abi": plugins::ABI_VERSION }),
    ))
}

/// Shows what a plugin file is and asks for, without installing it.
async fn inspect(State(st): State<AppState>, a: Auth, body: Bytes) -> ApiResult<Json<Value>> {
    manage(&a)?;
    let p = check(&st, body).await?;
    let installed = st.ctx.db.call(|c| db::list_plugins(c)).await?;
    let current = installed
        .iter()
        .find(|r| r.id == p.manifest.id)
        .map(|r| r.manifest.version.clone());
    Ok(Json(
        json!({ "manifest": p.manifest, "sha256": p.sha256, "installed_version": current }),
    ))
}

async fn install(State(st): State<AppState>, a: Auth, body: Bytes) -> ApiResult<Json<Value>> {
    manage(&a)?;
    install_bytes(&st, &a, body, "plugin.install").await
}

async fn install_bytes(st: &AppState, a: &Auth, body: Bytes, what: &str) -> ApiResult<Json<Value>> {
    let st = st.clone();
    let size = body.len() as i64;
    let p = check(&st, body.clone()).await?;
    let id = p.manifest.id.clone();
    let dir = st.ctx.plugins.dir.clone();
    let (i2, b2) = (id.clone(), body.clone());
    tokio::task::spawn_blocking(move || plugins::store_file(&dir, &i2, &b2))
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?
        .map_err(|e| ApiError::bad(e.to_string()))?;
    let (m, sha, by) = (
        p.manifest.clone(),
        p.sha256.clone(),
        a.user.username.clone(),
    );
    let b3 = body.clone();
    st.ctx
        .db
        .call(move |c| {
            db::upsert_plugin(c, &m, &sha, size, &by)?;
            db::set_plugin_wasm(c, &m.id, &b3)
        })
        .await?;
    st.ctx.plugins.insert(Arc::new(p));
    reload_all(&st).await;
    let m = st.ctx.plugins.get(&id).map(|p| p.manifest.clone());
    audit(
        &st.ctx,
        &a.user,
        what,
        format!(
            "{id} {}",
            m.as_ref().map(|m| m.version.as_str()).unwrap_or("")
        ),
    )
    .await;
    Ok(Json(json!({ "id": id, "manifest": m })))
}

async fn reload_all(st: &AppState) {
    for h in st.ctx.registry.handles() {
        let _ = h.tx.send(NetCmd::Reload).await;
    }
}

#[derive(Deserialize)]
struct UpdateReq {
    enabled: Option<bool>,
    /// Domains (from the plugin's `http_domains`) it may reach.
    http_approved: Option<Vec<String>>,
}

async fn update(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<String>,
    Json(req): Json<UpdateReq>,
) -> ApiResult<Json<Value>> {
    manage(&a)?;
    let row = st
        .ctx
        .db
        .call(|c| db::list_plugins(c))
        .await?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| ApiError::not_found("Plugin"))?;
    if let Some(d) = &req.http_approved {
        if let Some(bad) = d.iter().find(|x| !row.manifest.http_domains.contains(x)) {
            return Err(ApiError::bad(format!(
                "The plugin did not ask for {bad}; only the domains it asks for can be approved"
            )));
        }
    }
    let (i2, r2) = (id.clone(), req.http_approved.clone());
    st.ctx
        .db
        .call(move |c| {
            if let Some(e) = req.enabled {
                db::set_plugin_enabled(c, &i2, e)?;
            }
            if let Some(d) = r2 {
                db::set_plugin_http_approved(c, &i2, &d)?;
            }
            Ok(())
        })
        .await?;
    reload_all(&st).await;
    let mut what = Vec::new();
    if let Some(e) = req.enabled {
        what.push(if e {
            "on".to_string()
        } else {
            "off".to_string()
        });
    }
    if let Some(d) = &req.http_approved {
        what.push(format!(
            "web: {}",
            if d.is_empty() {
                "-".into()
            } else {
                d.join(", ")
            }
        ));
    }
    audit(
        &st.ctx,
        &a.user,
        "plugin.update",
        format!("{id} {}", what.join("; ")),
    )
    .await;
    let i3 = id.clone();
    let approved = st
        .ctx
        .db
        .call(move |c| db::plugin_http_approved(c, &i3))
        .await?;
    Ok(Json(
        json!({ "id": id, "enabled": req.enabled.unwrap_or(row.enabled), "http_approved": approved }),
    ))
}

async fn remove(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    manage(&a)?;
    let i2 = id.clone();
    if !st.ctx.db.call(move |c| db::delete_plugin(c, &i2)).await? {
        return Err(ApiError::not_found("Plugin"));
    }
    st.ctx.plugins.remove(&id);
    let _ = std::fs::remove_file(st.ctx.plugins.path_of(&id));
    reload_all(&st).await;
    audit(&st.ctx, &a.user, "plugin.remove", id.clone()).await;
    Ok(Json(json!({ "ok": true })))
}

async fn network_plugins(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    super::api::load_network_pub(&st.ctx, &a, id, false).await?;
    let (all, rows) = st
        .ctx
        .db
        .call(move |c| Ok((db::list_plugins(c)?, db::plugin_config_rows(c, id)?)))
        .await?;
    let list: Vec<Value> = all
        .into_iter()
        .filter(|p| p.enabled && st.ctx.plugins.get(&p.id).is_some())
        .map(|p| {
            let (enabled, cfg) = rows
                .iter()
                .find(|r| r.0 == p.id)
                .map(|r| (r.1, r.2.clone()))
                .unwrap_or((false, Map::new()));
            let config = plugins::validate_config(&p.manifest.settings, &cfg).unwrap_or_default();
            json!({ "id": p.id, "manifest": p.manifest, "enabled": enabled, "config": config })
        })
        .collect();
    Ok(Json(json!({ "plugins": list })))
}

#[derive(Deserialize)]
struct NetPluginReq {
    enabled: bool,
    #[serde(default)]
    config: Map<String, Value>,
}

async fn put_network_plugin(
    State(st): State<AppState>,
    a: Auth,
    Path((id, plugin)): Path<(i64, String)>,
    Json(req): Json<NetPluginReq>,
) -> ApiResult<Json<Value>> {
    let (n, owner) = super::api::load_network_pub(&st.ctx, &a, id, true).await?;
    let Some(p) = st.ctx.plugins.get(&plugin) else {
        return Err(ApiError::not_found("Plugin"));
    };
    let pid = plugin.clone();
    let enabled_globally = st
        .ctx
        .db
        .call(move |c| {
            Ok(db::list_plugins(c)?
                .into_iter()
                .any(|r| r.id == pid && r.enabled))
        })
        .await?;
    if !enabled_globally {
        return Err(ApiError::bad("This plugin is turned off on this server"));
    }
    let config = plugins::validate_config(&p.manifest.settings, &req.config)
        .map_err(|e| ApiError::bad(format!("{plugin}: {e}")))?;
    let (p2, c2) = (plugin.clone(), config.clone());
    st.ctx
        .db
        .call(move |c| db::set_plugin_config(c, id, &p2, req.enabled, &c2))
        .await?;
    if let Some(h) = crate::server::network_handle(&st.ctx, id) {
        let _ = h.tx.send(NetCmd::Reload).await;
    }
    audit(
        &st.ctx,
        &a.user,
        "network.plugin",
        format!(
            "{owner}/{} {plugin} {}",
            n.name,
            if req.enabled { "on" } else { "off" }
        ),
    )
    .await;
    Ok(Json(
        json!({ "id": plugin, "enabled": req.enabled, "config": config }),
    ))
}

// ------------------------------------------------------------ developer mode

async fn get_dev(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    manage(&a)?;
    Ok(Json(json!({
        "enabled": st.ctx.plugins.dev.load(std::sync::atomic::Ordering::Relaxed),
        "folder": st.ctx.plugins.dev_dir().display().to_string(),
    })))
}

#[derive(Deserialize)]
struct DevReq {
    enabled: bool,
}

async fn put_dev(
    State(st): State<AppState>,
    a: Auth,
    Json(req): Json<DevReq>,
) -> ApiResult<Json<Value>> {
    manage(&a)?;
    let v = if req.enabled { "1" } else { "0" };
    let uid = a.user.id.to_string();
    st.ctx
        .db
        .call(move |c| {
            db::set_setting(c, crate::plugin_dev::SETTING, v)?;
            // New plugins from the dev folder are turned on in this
            // person's networks, so they can try them at once.
            db::set_setting(c, crate::plugin_dev::USER_SETTING, &uid)
        })
        .await?;
    st.ctx
        .plugins
        .dev
        .store(req.enabled, std::sync::atomic::Ordering::Relaxed);
    if req.enabled {
        let _ = std::fs::create_dir_all(st.ctx.plugins.dev_dir());
    }
    audit(&st.ctx, &a.user, "plugin.dev", v.to_string()).await;
    get_dev(State(st), a).await
}

/// Live plugin output over a WebSocket: the recent lines, then new ones.
async fn logs(
    State(st): State<AppState>,
    a: Auth,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    if let Err(e) = manage(&a) {
        return e.into_response();
    }
    ws.max_message_size(1024).on_upgrade(move |mut socket| async move {
        use axum::extract::ws::Message;
        let mut rx = st.ctx.plugins.logs.tx.subscribe();
        for l in st.ctx.plugins.logs.recent() {
            if socket.send(Message::Text(json!(l).to_string().into())).await.is_err() {
                return;
            }
        }
        let mut ping = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            tokio::select! {
                l = rx.recv() => match l {
                    Ok(l) => {
                        if socket.send(Message::Text(json!(l).to_string().into())).await.is_err() {
                            return;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        let note = json!({"ts": db::now_ms(), "plugin": "", "network": "", "level": "warn", "text": format!("{n} lines skipped")});
                        let _ = socket.send(Message::Text(note.to_string().into())).await;
                    }
                    Err(_) => return,
                },
                m = socket.recv() => match m {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                    _ => {}
                },
                _ = ping.tick() => {
                    if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                        return;
                    }
                }
            }
        }
    })
}

// ------------------------------------------------------------ catalog

/// Official plugins and, when configured, the signed remote catalog.
async fn catalog(State(st): State<AppState>, a: Auth) -> ApiResult<Json<Value>> {
    manage(&a)?;
    let installed = st.ctx.db.call(|c| db::list_plugins(c)).await?;
    let version_of = |id: &str| {
        installed
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.manifest.version.clone())
    };
    let mut list = Vec::new();
    for (id, wasm) in crate::catalog::BUNDLED {
        let Ok(m) = plugins::read_manifest(wasm) else {
            continue;
        };
        list.push(json!({
            "id": id,
            "source": "official",
            "manifest": m,
            "size": wasm.len(),
            "installed_version": version_of(id),
        }));
    }
    let mut remote_error = None;
    if let Some(cfg) = crate::catalog::effective(&st.ctx.config) {
        match crate::catalog::fetch_index(&cfg, st.ctx.tls_client.clone()).await {
            Ok(entries) => {
                for e in entries {
                    if crate::catalog::bundled(&e.id).is_some() {
                        continue;
                    }
                    list.push(json!({
                        "id": e.id,
                        "source": "remote",
                        "manifest": e.manifest,
                        "size": e.size,
                        "installed_version": version_of(&e.id),
                    }));
                }
            }
            Err(e) => remote_error = Some(format!("{e:#}")),
        }
    }
    Ok(Json(json!({
        "plugins": list,
        "remote": crate::catalog::effective(&st.ctx.config).map(|c| c.url),
        "remote_error": remote_error,
    })))
}

/// One-click install from the catalog.
async fn catalog_install(
    State(st): State<AppState>,
    a: Auth,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    manage(&a)?;
    a.require_session()?;
    let wasm = if let Some(w) = crate::catalog::bundled(&id) {
        w.to_vec()
    } else {
        let cfg = crate::catalog::effective(&st.ctx.config)
            .ok_or_else(|| ApiError::not_found("Plugin"))?;
        let entries = crate::catalog::fetch_index(&cfg, st.ctx.tls_client.clone())
            .await
            .map_err(|e| ApiError::bad(format!("Cannot read the plugin catalog: {e:#}")))?;
        let e = entries
            .into_iter()
            .find(|e| e.id == id)
            .ok_or_else(|| ApiError::not_found("Plugin"))?;
        let w = crate::catalog::fetch_plugin(&e, st.ctx.tls_client.clone())
            .await
            .map_err(|e| ApiError::bad(format!("Cannot download the plugin: {e:#}")))?;
        let m = plugins::read_manifest(&w).map_err(|e| ApiError::bad(e.to_string()))?;
        if m.id != e.id {
            return Err(ApiError::bad(
                "The downloaded plugin is not the one in the catalog",
            ));
        }
        w
    };
    install_bytes(&st, &a, Bytes::from(wasm), "plugin.install_catalog").await
}
