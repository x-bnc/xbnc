//! Developer mode for plugins: a `.wasm` saved in `<plugins>/dev` is
//! checked and installed as soon as it changes, and every network reloads
//! it. A `name.json` manifest next to `name.wasm` is packed in by itself
//! (no need for `xbncd plugin pack`), and a plugin installed for the first
//! time is turned on in the networks of whoever enabled developer mode.
//! Results go to the plugins' live log.

use crate::db;
use crate::network::NetCmd;
use crate::plugins::{self, Plugin, PluginInstance};
use crate::server::Ctx;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::watch;

pub const SETTING: &str = "plugins.dev";
/// The user who turned developer mode on.
pub const USER_SETTING: &str = "plugins.dev_user";

pub async fn dev_loop(ctx: Arc<Ctx>, mut shutdown: watch::Receiver<bool>) {
    let on = ctx
        .db
        .call(|c| db::get_setting(c, SETTING))
        .await
        .ok()
        .flatten()
        .is_some_and(|v| v == "1");
    ctx.plugins.dev.store(on, Ordering::Relaxed);
    let mut seen: HashMap<PathBuf, (SystemTime, u64)> = HashMap::new();
    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(2)) => {}
            _ = shutdown.changed() => return,
        }
        if !ctx.plugins.dev.load(Ordering::Relaxed) {
            seen.clear();
            continue;
        }
        let dir = ctx.plugins.dev_dir();
        let _ = std::fs::create_dir_all(&dir);
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("wasm") {
                continue;
            }
            let Ok(m) = e.metadata() else { continue };
            // A change to the manifest next to it counts as a change too.
            let json = std::fs::metadata(path.with_extension("json")).ok();
            let newest = [
                m.modified().ok(),
                json.as_ref().and_then(|j| j.modified().ok()),
            ]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or(SystemTime::UNIX_EPOCH);
            let key = (newest, m.len() + json.map(|j| j.len()).unwrap_or(0));
            if seen.get(&path) == Some(&key) {
                continue;
            }
            seen.insert(path.clone(), key);
            // Files already there when the mode is turned on are installed
            // too, so the developer sees their current build running.
            // Wait for the writer to finish (the size must stay the same).
            tokio::time::sleep(Duration::from_millis(300)).await;
            let size_now = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
                + std::fs::metadata(path.with_extension("json"))
                    .map(|m| m.len())
                    .unwrap_or(0);
            if size_now != key.1 {
                seen.remove(&path);
                continue;
            }
            install(&ctx, &path).await;
        }
    }
}

async fn install(ctx: &Arc<Ctx>, path: &Path) {
    let file = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    let wasm = match std::fs::read(path) {
        Ok(w) if w.len() <= plugins::MAX_WASM_BYTES => w,
        Ok(_) => {
            ctx.plugins
                .logs
                .push(&file, "", "error", "the file is larger than 4 MB");
            return;
        }
        Err(e) => {
            ctx.plugins
                .logs
                .push(&file, "", "error", &format!("cannot read: {e}"));
            return;
        }
    };
    // name.json next to name.wasm: packed in here.
    let manifest_path = path.with_extension("json");
    let wasm = if manifest_path.exists() {
        let packed = std::fs::read(&manifest_path)
            .map_err(anyhow::Error::from)
            .and_then(|m| plugins::embed_manifest(&wasm, &m));
        match packed {
            Ok(w) => w,
            Err(e) => {
                ctx.plugins.logs.push(
                    &file,
                    "",
                    "error",
                    &format!(
                        "{}: {e:#}",
                        manifest_path
                            .file_name()
                            .map(|f| f.to_string_lossy())
                            .unwrap_or_default()
                    ),
                );
                return;
            }
        }
    } else {
        wasm
    };
    let engine = ctx.plugins.engine.clone();
    let w2 = wasm.clone();
    let checked = tokio::task::spawn_blocking(move || -> anyhow::Result<Plugin> {
        let p = Arc::new(Plugin::compile(&engine, &w2)?);
        PluginInstance::new(&engine, p.clone())?;
        Arc::try_unwrap(p).map_err(|_| anyhow::anyhow!("internal error"))
    })
    .await;
    let p = match checked {
        Ok(Ok(p)) => p,
        Ok(Err(e)) => {
            let mut msg = format!("not installed: {e:#}");
            if msg.contains("manifest") && !manifest_path.exists() {
                msg.push_str(&format!(
                    " (put the manifest next to it as {}.json)",
                    path.file_stem()
                        .map(|f| f.to_string_lossy())
                        .unwrap_or_default()
                ));
            }
            ctx.plugins.logs.push(&file, "", "error", &msg);
            return;
        }
        Err(e) => {
            ctx.plugins
                .logs
                .push(&file, "", "error", &format!("not installed: {e}"));
            return;
        }
    };
    let id = p.manifest.id.clone();
    if let Err(e) = plugins::store_file(&ctx.plugins.dir, &id, &wasm) {
        ctx.plugins
            .logs
            .push(&id, "", "error", &format!("cannot save: {e}"));
        return;
    }
    let (m, sha, size, w2) = (
        p.manifest.clone(),
        p.sha256.clone(),
        wasm.len() as i64,
        wasm.clone(),
    );
    let res = ctx
        .db
        .call(move |c| {
            db::upsert_plugin(c, &m, &sha, size, "developer mode")?;
            db::set_plugin_wasm(c, &m.id, &w2)?;
            // In developer mode the developer is the administrator.
            db::set_plugin_http_approved(c, &m.id, &m.http_domains)?;
            // First install: on in the developer's networks.
            let dev_user = db::get_setting(c, USER_SETTING)?.and_then(|v| v.parse::<i64>().ok());
            if let Some(uid) = dev_user {
                for n in db::list_networks(c, uid)? {
                    let configured = db::plugin_config_rows(c, n.id)?.iter().any(|r| r.0 == m.id);
                    if !configured {
                        db::set_plugin_config(c, n.id, &m.id, true, &serde_json::Map::new())?;
                    }
                }
            }
            Ok(())
        })
        .await;
    if let Err(e) = res {
        ctx.plugins
            .logs
            .push(&id, "", "error", &format!("cannot install: {e}"));
        return;
    }
    let version = p.manifest.version.clone();
    ctx.plugins.insert(Arc::new(p));
    for h in ctx.registry.handles() {
        let _ = h.tx.send(NetCmd::Reload).await;
    }
    ctx.plugins.logs.push(
        &id,
        "",
        "info",
        &format!("installed {version} from {file}; networks reloaded"),
    );
}
