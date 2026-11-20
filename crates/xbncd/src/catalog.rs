//! Plugin catalog: the official plugins shipped inside xbncd, plus an
//! optional remote catalog whose index is signed with Ed25519.
//!
//! Remote catalog layout (configured in `[catalog]`):
//!   <url>          index JSON: {"plugins": [{"id", "version", "url",
//!                  "sha256", "size", "manifest"}...]}
//!   <url>.sig      base64 Ed25519 signature of the index bytes
//! Each plugin file is checked against the signed SHA-256 before install.

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;

/// Official plugins (built from sdk/c/examples with `make bundle`).
pub const BUNDLED: &[(&str, &[u8])] = &[
    ("seen", include_bytes!("../catalog/seen.wasm")),
    ("remind", include_bytes!("../catalog/remind.wasm")),
    ("tell", include_bytes!("../catalog/tell.wasm")),
    ("poll", include_bytes!("../catalog/poll.wasm")),
    ("chanstats", include_bytes!("../catalog/chanstats.wasm")),
    ("antiflood", include_bytes!("../catalog/antiflood.wasm")),
    (
        "autoresponder",
        include_bytes!("../catalog/autoresponder.wasm"),
    ),
    ("dice", include_bytes!("../catalog/dice.wasm")),
    ("trivia", include_bytes!("../catalog/trivia.wasm")),
    ("greet", include_bytes!("../catalog/greet.wasm")),
    ("urltitle", include_bytes!("../catalog/urltitle.wasm")),
    ("factoids", include_bytes!("../catalog/factoids.wasm")),
    ("weather", include_bytes!("../catalog/weather.wasm")),
    ("news", include_bytes!("../catalog/news.wasm")),
    ("karma", include_bytes!("../catalog/karma.wasm")),
    ("quotes", include_bytes!("../catalog/quotes.wasm")),
    ("badwords", include_bytes!("../catalog/badwords.wasm")),
    ("games", include_bytes!("../catalog/games.wasm")),
];

/// The official remote catalogue (fixes and new plugins published between
/// xBNC versions). Used when `[catalog]` is not configured; empty values
/// switch it off. The public key is the project's (`xbnc-admin catalog
/// keygen`); its private half never leaves the publisher's machine.
pub const OFFICIAL_CATALOG_URL: &str = "https://x-bnc.github.io/xbnc-plugins/index.json";
pub const OFFICIAL_CATALOG_KEY: &str = "";

/// The catalogue in use: the configured one, or the official one.
pub fn effective(config: &crate::config::Config) -> Option<CatalogConfig> {
    if let Some(c) = &config.catalog {
        return Some(c.clone());
    }
    (!OFFICIAL_CATALOG_URL.is_empty() && !OFFICIAL_CATALOG_KEY.is_empty()).then(|| CatalogConfig {
        url: OFFICIAL_CATALOG_URL.to_string(),
        public_key: OFFICIAL_CATALOG_KEY.to_string(),
    })
}

/// Whether a catalogue is the official one (its plugins are part of xBNC:
/// new domains they ask for come approved, as for the bundled ones).
pub fn is_official(c: &CatalogConfig) -> bool {
    !OFFICIAL_CATALOG_KEY.is_empty() && c.public_key == OFFICIAL_CATALOG_KEY
}

/// What `auto_update` would do: the installed plugins the catalogue has
/// in a newer version, as (id, entry).
pub fn pending_updates(installed: &[crate::db::PluginRow], entries: &[RemoteEntry]) -> Vec<RemoteEntry> {
    entries
        .iter()
        .filter(|e| {
            installed
                .iter()
                .any(|p| p.id == e.id && crate::updates::is_newer(&e.version, &p.manifest.version))
        })
        .cloned()
        .collect()
}

/// Once a day: installed plugins with a newer version in the signed
/// catalogue are updated (their settings stay). The index signature and
/// each file's SHA-256 are checked before anything is installed.
pub async fn auto_update_loop(
    ctx: Arc<crate::server::Ctx>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let Some(cfg) = effective(&ctx.config) else {
        return;
    };
    let mut wait = Duration::from_secs(300);
    loop {
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = shutdown.changed() => return,
        }
        wait = Duration::from_secs(24 * 3600);
        match auto_update(&ctx, &cfg).await {
            Ok(ids) if !ids.is_empty() => {
                tracing::info!(plugins = %ids.join(", "), "plugins updated from the catalogue")
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %format!("{e:#}"), "cannot check the plugin catalogue"),
        }
    }
}

/// Updates the installed plugins that have a newer version in the
/// catalogue. Returns their ids.
pub async fn auto_update(ctx: &Arc<crate::server::Ctx>, cfg: &CatalogConfig) -> Result<Vec<String>> {
    let entries = fetch_index(cfg, ctx.tls_client.clone()).await?;
    let installed = ctx.db.call(|c| crate::db::list_plugins(c)).await?;
    let official = is_official(cfg);
    let mut done = Vec::new();
    for e in pending_updates(&installed, &entries) {
        let wasm = match fetch_plugin(&e, ctx.tls_client.clone()).await {
            Ok(w) => w,
            Err(err) => {
                tracing::warn!(plugin = %e.id, error = %format!("{err:#}"), "cannot download the plugin update");
                continue;
            }
        };
        let p = match crate::plugins::Plugin::compile(&ctx.plugins.engine, &wasm) {
            Ok(p) if p.manifest.id == e.id => p,
            _ => {
                tracing::warn!(plugin = %e.id, "the plugin update is not valid; kept the installed one");
                continue;
            }
        };
        let previous = installed.iter().find(|x| x.id == e.id).map(|x| x.manifest.http_domains.clone()).unwrap_or_default();
        crate::plugins::store_file(&ctx.plugins.dir, &e.id, &wasm)?;
        let (m, sha, size, w2) = (p.manifest.clone(), p.sha256.clone(), wasm.len() as i64, wasm.clone());
        ctx.db
            .call(move |c| {
                crate::db::upsert_plugin(c, &m, &sha, size, "catalogue")?;
                crate::db::set_plugin_wasm(c, &m.id, &w2)?;
                if official {
                    let mut approved = crate::db::plugin_http_approved(c, &m.id)?;
                    for d in &m.http_domains {
                        if !previous.contains(d) && !approved.contains(d) {
                            approved.push(d.clone());
                        }
                    }
                    crate::db::set_plugin_http_approved(c, &m.id, &approved)?;
                }
                crate::db::audit(c, "xbnc", "plugin.update_catalog", &format!("{} {}", m.id, m.version))
            })
            .await?;
        ctx.plugins.insert(Arc::new(p));
        done.push(e.id.clone());
    }
    if !done.is_empty() {
        for h in ctx.registry.handles() {
            let _ = h.tx.send(crate::network::NetCmd::Reload).await;
        }
    }
    Ok(done)
}

/// Settings key: the xBNC version that last made the official plugins
/// available.
pub const OFFICIAL_SYNCED: &str = "plugins.official_synced";

/// Makes every official plugin available: on the first start and after each
/// xBNC update, the ones that are missing are installed; at any start, an
/// installed official plugin older than the one shipped here is upgraded.
/// Installing does not turn a plugin on anywhere: each user enables it per
/// network. A plugin a superadministrator removed comes back only with the
/// next xBNC update. Returns the ids installed or upgraded.
pub fn sync_official(db: &crate::db::Db, reg: &crate::plugins::PluginRegistry) -> Vec<String> {
    let version = env!("CARGO_PKG_VERSION");
    let installed = db.with(|c| crate::db::list_plugins(c)).unwrap_or_default();
    let synced = db
        .with(|c| crate::db::get_setting(c, OFFICIAL_SYNCED))
        .ok()
        .flatten();
    let add_missing = synced.as_deref() != Some(version);
    let mut done = Vec::new();
    for (id, wasm) in BUNDLED {
        let Ok(m) = crate::plugins::read_manifest(wasm) else {
            continue;
        };
        let previous = installed.iter().find(|p| p.id == *id);
        let need = match previous {
            None => add_missing,
            // Newer, or the same version built differently (a fix shipped
            // inside the same xBNC version): the shipped file wins.
            Some(p) => {
                crate::updates::is_newer(&m.version, &p.manifest.version)
                    || (m.version == p.manifest.version && p.sha256 != sha256_hex(wasm))
            }
        };
        if !need {
            continue;
        }
        let res = (|| -> Result<()> {
            let p = crate::plugins::Plugin::compile(&reg.engine, wasm)?;
            crate::plugins::store_file(&reg.dir, id, wasm)?;
            db.with(|c| {
                crate::db::upsert_plugin(c, &p.manifest, &p.sha256, wasm.len() as i64, "xbnc")?;
                crate::db::set_plugin_wasm(c, id, wasm)?;
                // The domains of an official plugin are part of xBNC and come
                // approved: without them weather, urltitle and news could
                // never answer. On an upgrade only the domains the previous
                // version did not ask for are added, so a domain an
                // administrator blocked stays blocked.
                let asked_before: &[String] =
                    previous.map(|x| x.manifest.http_domains.as_slice()).unwrap_or(&[]);
                let mut approved = crate::db::plugin_http_approved(c, id)?;
                for d in &p.manifest.http_domains {
                    if !asked_before.contains(d) && !approved.contains(d) {
                        approved.push(d.clone());
                    }
                }
                crate::db::set_plugin_http_approved(c, id, &approved)?;
                crate::db::audit(
                    c,
                    "xbnc",
                    "plugin.install_official",
                    &format!("{id} {}", p.manifest.version),
                )
            })
        })();
        match res {
            Ok(()) => {
                tracing::info!(plugin = %id, version = %m.version, "official plugin installed");
                done.push(id.to_string());
            }
            Err(e) => tracing::warn!(plugin = %id, error = %e, "cannot install official plugin"),
        }
    }
    if add_missing {
        let _ = db.with(|c| crate::db::set_setting(c, OFFICIAL_SYNCED, version));
    }
    reset_old_language_defaults(db);
    approve_official_domains_once(db);
    done
}

/// Settings key: the official plugins installed before their domains came
/// approved have been given them.
pub const OFFICIAL_HTTP_FIXED: &str = "plugins.official_http_fixed";

/// Official plugins installed by an earlier 1.0.0 build had no approved
/// domains, so every web request of weather, urltitle and news was refused.
/// Runs once: an official plugin with no approved domain gets the ones it
/// asks for. Afterwards an administrator's choice is never changed.
pub fn approve_official_domains_once(db: &crate::db::Db) -> usize {
    let done = db
        .with(|c| crate::db::get_setting(c, OFFICIAL_HTTP_FIXED))
        .ok()
        .flatten();
    if done.is_some() {
        return 0;
    }
    let fixed = db
        .with(|c| {
            let mut n = 0;
            for p in crate::db::list_plugins(c)? {
                if bundled(&p.id).is_none() || p.manifest.http_domains.is_empty() {
                    continue;
                }
                if crate::db::plugin_http_approved(c, &p.id)?.is_empty() {
                    crate::db::set_plugin_http_approved(c, &p.id, &p.manifest.http_domains)?;
                    n += 1;
                }
            }
            crate::db::set_setting(c, OFFICIAL_HTTP_FIXED, "1")?;
            Ok(n)
        })
        .unwrap_or(0);
    if fixed > 0 {
        tracing::info!(plugins = fixed, "approved the web domains of the official plugins");
    }
    fixed
}

/// Settings key: the 0.8.0 language defaults have been cleared.
pub const LANG_DEFAULTS_FIXED: &str = "plugins.lang_defaults_fixed";

/// In 0.8.0 these official plugins had "es" as the default of their
/// "lang" setting, and enabling one stored it. Since 0.8.1 the default is
/// empty (the user's xBNC language), so a stored "es" left by 0.8.0 kept
/// those plugins in Spanish after the user switched to English. Clears it
/// once; a language chosen after this runs is kept.
pub fn reset_old_language_defaults(db: &crate::db::Db) -> usize {
    const OLD_DEFAULT_ES: &[&str] = &["seen", "remind", "tell", "poll", "chanstats", "antiflood"];
    if db
        .with(|c| crate::db::get_setting(c, LANG_DEFAULTS_FIXED))
        .ok()
        .flatten()
        .is_some()
    {
        return 0;
    }
    let res = db.with(|c| {
        let mut fixed = 0;
        for u in crate::db::list_users(c)? {
            for n in crate::db::list_networks(c, u.id)? {
                for (pid, enabled, mut cfg, _) in crate::db::plugin_config_rows(c, n.id)? {
                    if !OLD_DEFAULT_ES.contains(&pid.as_str())
                        || cfg.get("lang").and_then(Value::as_str) != Some("es")
                    {
                        continue;
                    }
                    cfg.insert("lang".into(), Value::String(String::new()));
                    crate::db::set_plugin_config(c, n.id, &pid, enabled, &cfg)?;
                    fixed += 1;
                }
            }
        }
        crate::db::set_setting(c, LANG_DEFAULTS_FIXED, "1")?;
        Ok(fixed)
    });
    match res {
        Ok(n) => {
            if n > 0 {
                tracing::info!(
                    count = n,
                    "plugins now answer in each user's language (old 0.8.0 default cleared)"
                );
            }
            n
        }
        Err(e) => {
            tracing::warn!(error = %e, "cannot clear the old plugin language defaults");
            0
        }
    }
}

pub fn bundled(id: &str) -> Option<&'static [u8]> {
    BUNDLED.iter().find(|(i, _)| *i == id).map(|(_, w)| *w)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogConfig {
    /// https:// address of the index JSON.
    pub url: String,
    /// Base64 Ed25519 public key that signs the index.
    pub public_key: String,
}

impl CatalogConfig {
    pub fn validate(&self) -> Result<()> {
        if !self.url.starts_with("https://") {
            bail!("catalog.url must use https://");
        }
        let k = B64
            .decode(self.public_key.trim())
            .context("catalog.public_key is not base64")?;
        if k.len() != 32 {
            bail!("catalog.public_key must be a 32-byte Ed25519 key");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteEntry {
    pub id: String,
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub manifest: Value,
}

#[derive(Debug, Deserialize)]
struct Index {
    plugins: Vec<RemoteEntry>,
}

/// Verifies a signed index and returns its entries.
pub fn parse_index(index: &[u8], sig_b64: &str, public_key_b64: &str) -> Result<Vec<RemoteEntry>> {
    let key = B64.decode(public_key_b64.trim())?;
    let sig = B64
        .decode(sig_b64.trim())
        .context("the signature is not base64")?;
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key)
        .verify(index, &sig)
        .map_err(|_| anyhow::anyhow!("the catalog signature is not valid"))?;
    let i: Index = serde_json::from_slice(index).context("the catalog index is not valid JSON")?;
    for e in &i.plugins {
        if !e.url.starts_with("https://") || e.sha256.len() != 64 {
            bail!("catalog entry {} is not valid", e.id);
        }
    }
    Ok(i.plugins)
}

pub async fn fetch_index(
    cfg: &CatalogConfig,
    tls: Arc<rustls::ClientConfig>,
) -> Result<Vec<RemoteEntry>> {
    let get = |url: String, max: usize| {
        let tls = tls.clone();
        async move {
            let r = crate::http::request_public(
                "GET",
                &url,
                None,
                tls,
                max,
                Duration::from_secs(15),
                &[],
            )
            .await?;
            if r.status != 200 {
                bail!("{url} answered {}", r.status);
            }
            Ok::<Vec<u8>, anyhow::Error>(r.body)
        }
    };
    let index = get(cfg.url.clone(), 1024 * 1024).await?;
    let sig = get(format!("{}.sig", cfg.url), 1024).await?;
    parse_index(&index, &String::from_utf8_lossy(&sig), &cfg.public_key)
}

/// Downloads a remote plugin and checks it against the signed hash.
pub async fn fetch_plugin(e: &RemoteEntry, tls: Arc<rustls::ClientConfig>) -> Result<Vec<u8>> {
    let r = crate::http::request_public(
        "GET",
        &e.url,
        None,
        tls,
        crate::plugins::MAX_WASM_BYTES,
        Duration::from_secs(30),
        &[],
    )
    .await?;
    if r.status != 200 {
        bail!("{} answered {}", e.url, r.status);
    }
    let got = hex(&Sha256::digest(&r.body));
    if !got.eq_ignore_ascii_case(&e.sha256) {
        bail!("the downloaded file does not match the catalog (SHA-256 {got})");
    }
    Ok(r.body)
}

fn sha256_hex(b: &[u8]) -> String {
    hex(&Sha256::digest(b))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// `xbncd plugin catalog-keygen`: a new signing key (PKCS#8, base64) and
/// its public key.
pub fn keygen() -> Result<(String, String)> {
    use ring::signature::KeyPair;
    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|_| anyhow::anyhow!("cannot generate a key"))?;
    let kp = ring::signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| anyhow::anyhow!("bad key"))?;
    Ok((
        B64.encode(pkcs8.as_ref()),
        B64.encode(kp.public_key().as_ref()),
    ))
}

/// `xbncd plugin catalog-sign`: the base64 signature of an index.
pub fn sign(private_pkcs8_b64: &str, index: &[u8]) -> Result<String> {
    let k = B64
        .decode(private_pkcs8_b64.trim())
        .context("the key file is not base64")?;
    let kp = ring::signature::Ed25519KeyPair::from_pkcs8(&k)
        .map_err(|_| anyhow::anyhow!("not an Ed25519 key"))?;
    Ok(B64.encode(kp.sign(index).as_ref()))
}

/// Builds an index entry for a plugin file (for catalog publishers).
pub fn entry_for(wasm: &[u8], url: &str) -> Result<RemoteEntry> {
    let m = crate::plugins::read_manifest(wasm)?;
    Ok(RemoteEntry {
        id: m.id.clone(),
        version: m.version.clone(),
        url: url.to_string(),
        sha256: hex(&Sha256::digest(wasm)),
        size: wasm.len() as u64,
        manifest: serde_json::to_value(&m)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Someone who enabled plugins in 0.8.0 (lang stored as "es") must get
    /// answers in their own language after updating; a language chosen
    /// afterwards is kept.
    #[test]
    fn old_spanish_defaults_follow_the_user_language() {
        use crate::db;
        use serde_json::{json, Map};
        let dir = tempfile::tempdir().unwrap();
        let d = db::Db::open(&dir.path().join("x.db")).unwrap();
        let obj = |v: Value| -> Map<String, Value> { v.as_object().unwrap().clone() };
        let nid = d
            .with(|c| {
                let uid = db::create_user(c, "ana", "x", false)?;
                let n = db::NewNetwork {
                    name: "undernet".into(),
                    host: "irc.undernet.org".into(),
                    port: 6667,
                    tls: false,
                    nick: "ana".into(),
                    username: "ana".into(),
                    realname: "ana".into(),
                    server_password_enc: None,
                    sasl_username: None,
                    sasl_password_enc: None,
                };
                let nid = db::create_network(c, uid, &n, 10)?;
                for id in ["seen", "dice"] {
                    let m = crate::plugins::read_manifest(bundled(id).unwrap())?;
                    db::upsert_plugin(c, &m, "sha", 1, "test")?;
                }
                db::set_plugin_config(
                    c,
                    nid,
                    "seen",
                    true,
                    &obj(json!({"lang": "es", "trigger": "!seen"})),
                )?;
                db::set_plugin_config(c, nid, "dice", true, &obj(json!({"lang": "es"})))?;
                Ok(nid)
            })
            .unwrap();
        assert_eq!(reset_old_language_defaults(&d), 1);
        let rows = d.with(|c| db::plugin_config_rows(c, nid)).unwrap();
        let get = |id: &str| rows.iter().find(|r| r.0 == id).unwrap().2.clone();
        assert_eq!(get("seen")["lang"], "");
        assert_eq!(get("seen")["trigger"], "!seen");
        // dice never had "es" as its default: a stored "es" was a choice.
        assert_eq!(get("dice")["lang"], "es");
        // Runs once: a later choice of Spanish stays.
        d.with(|c| db::set_plugin_config(c, nid, "seen", true, &obj(json!({"lang": "es"}))))
            .unwrap();
        assert_eq!(reset_old_language_defaults(&d), 0);
        let rows = d.with(|c| db::plugin_config_rows(c, nid)).unwrap();
        assert_eq!(rows.iter().find(|r| r.0 == "seen").unwrap().2["lang"], "es");
    }

    /// Regression: weather, urltitle and news were installed with no
    /// approved domain, so every web request was refused ("Could not get
    /// the weather right now"). They must come approved, an upgrade must
    /// not re-approve a domain an administrator blocked, and installs made
    /// before the fix are repaired once.
    #[test]
    fn official_web_plugins_come_with_their_domains_approved() {
        use crate::db;
        let dir = tempfile::tempdir().unwrap();
        let d = db::Db::open(&dir.path().join("x.db")).unwrap();
        let reg = crate::plugins::PluginRegistry::new(dir.path().join("plugins"));
        sync_official(&d, &reg);
        for id in ["weather", "urltitle", "news"] {
            let m = crate::plugins::read_manifest(bundled(id).unwrap()).unwrap();
            assert!(!m.http_domains.is_empty(), "{id}");
            let a = d.with(|c| db::plugin_http_approved(c, id)).unwrap();
            assert_eq!(a, m.http_domains, "{id}");
            for host in &m.http_domains {
                assert!(crate::plugins::domain_allowed(&a, host), "{id} {host}");
            }
        }
        // The same version built differently is replaced by the shipped one.
        d.with(|c| {
            c.execute("UPDATE plugins SET sha256 = 'old' WHERE id = 'dice'", [] as [&str; 0])?;
            Ok(())
        })
        .unwrap();
        assert_eq!(sync_official(&d, &reg), vec!["dice".to_string()]);
        assert!(sync_official(&d, &reg).is_empty());
        // An administrator blocks one; a later start keeps it blocked.
        d.with(|c| db::set_plugin_http_approved(c, "weather", &[])).unwrap();
        sync_official(&d, &reg);
        assert!(d.with(|c| db::plugin_http_approved(c, "weather")).unwrap().is_empty());
        // An install made before the fix (no approvals, repair not run yet).
        d.with(|c| {
            db::set_plugin_http_approved(c, "news", &[])?;
            c.execute("DELETE FROM settings WHERE key = ?1", [OFFICIAL_HTTP_FIXED])?;
            Ok(())
        })
        .unwrap();
        assert_eq!(approve_official_domains_once(&d), 2);
        assert!(!d.with(|c| db::plugin_http_approved(c, "news")).unwrap().is_empty());
        assert_eq!(approve_official_domains_once(&d), 0);
    }

    /// The weather plugin end to end with answers in the exact format of
    /// Open-Meteo (compact JSON, "current_units" before "current").
    #[test]
    fn weather_answers_from_open_meteo() {
        use crate::plugins::{Action, Plugin, PluginInstance};
        use serde_json::json;
        let e = crate::plugins::engine();
        let p = Arc::new(Plugin::compile(&e, bundled("weather").unwrap()).unwrap());
        let mut inst = PluginInstance::new(&e, p).unwrap();
        let base = |mut v: Value, lang: &str| {
            v["lang"] = json!(lang);
            v["config"] = json!({});
            v["state"] = Value::Null;
            v
        };
        let msg = json!({"event": "message", "command": "PRIVMSG", "conversation": "#xbnc",
            "channel": true, "from": "bob", "from_mask": "bob!b@h", "text": "!tiempo Madrid",
            "action": false, "outgoing": false});
        let out = inst.call(&base(msg, "es")).unwrap();
        let (id, url) = match &out.actions[..] {
            [Action::Http { id, url, method, .. }] if method == "GET" => (id.clone(), url.clone()),
            other => panic!("expected one request, got {other:?}"),
        };
        assert!(url.starts_with("https://geocoding-api.open-meteo.com/v1/search?"), "{url}");
        assert!(url.contains("name=Madrid") && url.contains("language=es"), "{url}");
        let geo = r#"{"results":[{"id":3117735,"name":"Madrid","latitude":40.4165,"longitude":-3.70256,"elevation":667.0,"feature_code":"PPLC","country_code":"ES","admin1_id":3117732,"timezone":"Europe/Madrid","population":3255944,"country_id":2510769,"country":"España","admin1":"Madrid"}],"generationtime_ms":0.6}"#;
        let ev = json!({"event": "http", "id": id, "status": 200, "body": geo, "error": null});
        let out = inst.call(&base(ev, "es")).unwrap();
        let (id, url) = match &out.actions[..] {
            [Action::Http { id, url, .. }] => (id.clone(), url.clone()),
            other => panic!("expected the forecast request, got {other:?}"),
        };
        assert!(url.starts_with("https://api.open-meteo.com/v1/forecast?latitude=40.4165&longitude=-3.70256&"), "{url}");
        let fc = r#"{"latitude":40.4,"longitude":-3.7000003,"generationtime_ms":0.05,"utc_offset_seconds":3600,"timezone":"Europe/Madrid","timezone_abbreviation":"GMT+1","elevation":667.0,"current_units":{"time":"iso8601","interval":"seconds","temperature_2m":"°C","relative_humidity_2m":"%","weather_code":"wmo code","wind_speed_10m":"km/h"},"current":{"time":"2026-11-20T13:45","interval":900,"temperature_2m":14.3,"relative_humidity_2m":58,"weather_code":2,"wind_speed_10m":9.4},"daily_units":{"time":"iso8601","temperature_2m_max":"°C","temperature_2m_min":"°C"},"daily":{"time":["2026-11-20"],"temperature_2m_max":[16.1],"temperature_2m_min":[5.2]}}"#;
        let ev = json!({"event": "http", "id": id, "status": 200, "body": fc, "error": null});
        let out = inst.call(&base(ev, "es")).unwrap();
        assert_eq!(
            out.actions,
            vec![Action::Privmsg {
                target: "#xbnc".into(),
                text: "Madrid (España): 14 °C, parcialmente nuboso, viento 9 km/h, humedad 58 %. Hoy entre 5 y 16 °C.".into()
            }]
        );
    }

    /// news end to end: the first look only remembers, then new entries are
    /// posted with numeric entities decoded and CDATA (with spaces before
    /// it) read; a feed that cannot be read is told to the user once and
    /// shown by "list".
    #[test]
    fn news_posts_new_entries_and_reports_errors() {
        use crate::db;
        use crate::plugins::{Action, Plugin, PluginInstance};
        use serde_json::json;
        let dir = tempfile::tempdir().unwrap();
        let d = db::Db::open(&dir.path().join("x.db")).unwrap();
        let nid = d
            .with(|c| {
                let uid = db::create_user(c, "ana", "x", false)?;
                let n = db::NewNetwork {
                    name: "libera".into(),
                    host: "irc.libera.chat".into(),
                    port: 6697,
                    tls: true,
                    nick: "ana".into(),
                    username: "ana".into(),
                    realname: "ana".into(),
                    server_password_enc: None,
                    sasl_username: None,
                    sasl_password_enc: None,
                };
                let nid = db::create_network(c, uid, &n, 10)?;
                let m = crate::plugins::read_manifest(bundled("news").unwrap())?;
                db::upsert_plugin(c, &m, "sha", 1, "test")?;
                Ok(nid)
            })
            .unwrap();
        let e = crate::plugins::engine();
        let p = Arc::new(Plugin::compile(&e, bundled("news").unwrap()).unwrap());
        let mut inst = PluginInstance::new(&e, p).unwrap();
        inst.attach_storage(d.clone(), nid);
        let cfg = json!({"feeds": "#xbnc https://hnrss.org/frontpage\n#xbnc-dev https://elsewhere.example/feed"});
        let ev = |mut v: Value| {
            v["lang"] = json!("en");
            v["config"] = cfg.clone();
            v["state"] = Value::Null;
            v["connected"] = json!(true);
            v
        };
        let fetch = |inst: &mut PluginInstance| -> (String, String) {
            let out = inst.call(&ev(json!({"event": "timer"}))).unwrap();
            match &out.actions[..] {
                [Action::Http { id, url, .. }] => (id.clone(), url.clone()),
                other => panic!("expected a request, got {other:?}"),
            }
        };
        let item = |t: &str, l: &str| format!("<item>\n<title>\n  <![CDATA[{t}]]>\n</title>\n<link>{l}</link>\n</item>");
        let feed = |items: &[String]| format!("<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>HN</title>{}</channel></rss>", items.concat());
        let (id, url) = fetch(&mut inst);
        // Regression: with several feeds the setting was reloaded at every
        // event, so feed 1 was asked again (every minute) and feed 2 never.
        let out = inst.call(&ev(json!({"event": "timer"}))).unwrap();
        assert!(
            matches!(&out.actions[..], [Action::Http { url, .. }] if url == "https://elsewhere.example/feed"),
            "{:?}",
            out.actions
        );
        let out = inst.call(&ev(json!({"event": "timer"}))).unwrap();
        assert!(out.actions.is_empty(), "nothing due: {:?}", out.actions);
        let out = inst.call(&ev(json!({"event": "http", "id": "1", "status": 200, "body": "<rss></rss>", "error": null}))).unwrap();
        assert!(out.actions.is_empty());
        assert_eq!(url, "https://hnrss.org/frontpage");
        let first = feed(&[item("Old one", "https://a.example/1")]);
        let out = inst.call(&ev(json!({"event": "http", "id": id, "status": 200, "body": first, "error": null}))).unwrap();
        assert!(out.actions.is_empty(), "the first look only remembers");
        // Next check: a new entry with numeric entities.
        inst.call(&ev(json!({"event": "command", "text": "check"}))).unwrap();
        let (id, u2) = fetch(&mut inst);
        assert_eq!(u2, "https://hnrss.org/frontpage");
        let second = feed(&[
            item("It&#8217;s here &#x2014; xBNC&#38;co", "https://a.example/2"),
            item("Old one", "https://a.example/1"),
        ]);
        let out = inst.call(&ev(json!({"event": "http", "id": id, "status": 200, "body": second, "error": null}))).unwrap();
        assert_eq!(
            out.actions,
            vec![Action::Privmsg {
                target: "#xbnc".into(),
                text: "[News] It\u{2019}s here \u{2014} xBNC&co \u{2014} https://a.example/2".into()
            }]
        );
        // The second feed is refused: the user hears it once, list shows it.
        let (id, url) = fetch(&mut inst);
        assert_eq!(url, "https://elsewhere.example/feed");
        let refused = json!({"event": "http", "id": id, "status": 0, "body": "",
            "error": "elsewhere.example is not an approved domain for this plugin"});
        let out = inst.call(&ev(refused.clone())).unwrap();
        assert!(matches!(&out.actions[..], [Action::Tell(t)] if t.starts_with("Cannot read https://elsewhere.example/feed: elsewhere.example is not an approved domain")), "{:?}", out.actions);
        let out = inst.call(&ev(refused)).unwrap();
        assert!(out.actions.is_empty(), "told only once");
        let out = inst.call(&ev(json!({"event": "command", "text": "list"}))).unwrap();
        assert!(out.actions.iter().any(|a| matches!(a, Action::Tell(t) if t.contains("elsewhere.example/feed (error: "))), "{:?}", out.actions);
    }

    /// urltitle end to end with a YouTube oEmbed answer.
    #[test]
    fn urltitle_shows_youtube_titles() {
        use crate::plugins::{Action, Plugin, PluginInstance};
        use serde_json::json;
        let e = crate::plugins::engine();
        let p = Arc::new(Plugin::compile(&e, bundled("urltitle").unwrap()).unwrap());
        let mut inst = PluginInstance::new(&e, p).unwrap();
        let ev = |mut v: Value| {
            v["lang"] = json!("en");
            v["config"] = json!({});
            v["state"] = Value::Null;
            v
        };
        let msg = json!({"event": "message", "command": "PRIVMSG", "conversation": "#xbnc", "channel": true,
            "from": "bob", "from_mask": "bob!b@h", "text": "look https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "action": false, "outgoing": false});
        let out = inst.call(&ev(msg)).unwrap();
        let (id, url) = match &out.actions[..] {
            [Action::Http { id, url, .. }] => (id.clone(), url.clone()),
            other => panic!("expected a request, got {other:?}"),
        };
        assert!(url.starts_with("https://www.youtube.com/oembed?format=json&url=https%3A%2F%2Fwww.youtube.com%2Fwatch%3Fv%3DdQw4w9WgXcQ"), "{url}");
        let body = r#"{"title":"Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster)","author_name":"Rick Astley","author_url":"https://www.youtube.com/@RickAstleyYT","type":"video","height":113,"width":200,"version":"1.0","provider_name":"YouTube","provider_url":"https://www.youtube.com/"}"#;
        let out = inst.call(&ev(json!({"event": "http", "id": id, "status": 200, "body": body, "error": null}))).unwrap();
        assert_eq!(
            out.actions,
            vec![Action::Privmsg {
                target: "#xbnc".into(),
                text: "[YouTube] Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster)".into()
            }]
        );
    }

    /// Only installed plugins with a newer version in the catalogue are
    /// updated; nothing is installed that was not there.
    #[test]
    fn catalogue_updates_only_newer_installed_plugins() {
        use crate::db;
        let dir = tempfile::tempdir().unwrap();
        let d = db::Db::open(&dir.path().join("x.db")).unwrap();
        d.with(|c| {
            for id in ["dice", "seen"] {
                let m = crate::plugins::read_manifest(bundled(id).unwrap())?;
                db::upsert_plugin(c, &m, "sha", 1, "test")?;
            }
            Ok(())
        })
        .unwrap();
        let installed = d.with(|c| db::list_plugins(c)).unwrap();
        let e = |id: &str, v: &str| RemoteEntry {
            id: id.into(),
            version: v.into(),
            url: format!("https://plugins.example.org/{id}.wasm"),
            sha256: "x".into(),
            size: 1,
            manifest: Value::Null,
        };
        let list = vec![e("dice", "9.0.0"), e("seen", "1.0.0"), e("new", "1.0.0")];
        let ids: Vec<_> = pending_updates(&installed, &list).into_iter().map(|e| e.id).collect();
        assert_eq!(ids, vec!["dice".to_string()]);
        // Without a configured catalogue and without the official key, none.
        assert!(OFFICIAL_CATALOG_KEY.is_empty() || !OFFICIAL_CATALOG_URL.is_empty());
    }

    #[test]
    fn bundled_plugins_are_valid() {
        let e = crate::plugins::engine();
        for (id, w) in BUNDLED {
            let p =
                crate::plugins::Plugin::compile(&e, w).unwrap_or_else(|err| panic!("{id}: {err}"));
            assert_eq!(&p.manifest.id, id);
            crate::plugins::PluginInstance::new(&e, Arc::new(p)).unwrap();
        }
    }

    #[test]
    fn signed_index() {
        let (sk, pk) = keygen().unwrap();
        let e = entry_for(
            bundled("dice").unwrap(),
            "https://plugins.example.org/dice.wasm",
        )
        .unwrap();
        let idx = serde_json::to_vec(&serde_json::json!({ "plugins": [e] })).unwrap();
        let sig = sign(&sk, &idx).unwrap();
        let list = parse_index(&idx, &sig, &pk).unwrap();
        assert_eq!(list[0].id, "dice");
        let mut bad = idx.clone();
        bad[5] ^= 1;
        assert!(parse_index(&bad, &sig, &pk).is_err());
        let (_, other) = keygen().unwrap();
        assert!(parse_index(&idx, &sig, &other).is_err());
    }
}
