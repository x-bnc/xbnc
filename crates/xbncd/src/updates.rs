//! Checks whether a newer xBNC version has been published. Only reads the
//! release information (version, link, notes): updating is still done by the
//! administrator with scripts/update.sh or the system package.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// `[updates]` in xbnc.toml.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdatesConfig {
    /// Check once a day (and when an administrator asks).
    #[serde(default = "yes")]
    pub check: bool,
    /// GitHub repository with the releases, "owner/name".
    pub repository: Option<String>,
    /// Or any https:// URL answering {"version", "url", "notes", "packages"}.
    pub url: Option<String>,
    /// Ed25519 public key (base64) that signs releases. With it, a
    /// superadministrator can install a new signed package from the panel.
    pub public_key: Option<String>,
}

fn yes() -> bool {
    true
}

impl Default for UpdatesConfig {
    fn default() -> Self {
        UpdatesConfig {
            check: true,
            repository: None,
            url: None,
            public_key: None,
        }
    }
}

impl UpdatesConfig {
    pub fn validate(&self) -> Result<()> {
        if let Some(r) = &self.repository {
            let ok = r.split_once('/').is_some_and(|(o, n)| {
                let part = |s: &str| {
                    !s.is_empty()
                        && s.len() <= 100
                        && s.chars()
                            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
                };
                part(o) && part(n)
            });
            if !ok {
                bail!("updates.repository must look like owner/name");
            }
        }
        if let Some(k) = &self.public_key {
            use base64::Engine;
            let ok = base64::engine::general_purpose::STANDARD
                .decode(k.trim())
                .is_ok_and(|b| b.len() == 32);
            if !ok {
                bail!("updates.public_key must be the base64 Ed25519 key printed by `xbncd release keygen`");
            }
        }
        if let Some(u) = &self.url {
            let parsed = crate::http::Url::parse(u).context("updates.url is not a valid URL")?;
            if !parsed.tls {
                bail!("updates.url must be an https:// URL");
            }
        }
        Ok(())
    }

    /// Where to look, when checking is on and a source is configured.
    pub fn source(&self) -> Option<String> {
        if !self.check {
            return None;
        }
        if let Some(u) = &self.url {
            return Some(u.clone());
        }
        self.repository
            .as_ref()
            .map(|r| format!("https://api.github.com/repos/{r}/releases/latest"))
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateState {
    pub latest: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub checked_at: Option<i64>,
    pub error: Option<String>,
    /// Release files: (name, https URL).
    #[serde(skip)]
    pub packages: Vec<(String, String)>,
}

#[derive(Default)]
pub struct Updates {
    state: Mutex<UpdateState>,
}

impl Updates {
    pub fn state(&self) -> UpdateState {
        self.state.lock().expect("updates lock").clone()
    }
}

/// "v1.2.3", "1.2.3-beta.1" → comparable (major, minor, patch, is_release).
fn version_key(v: &str) -> Option<(u64, u64, u64, bool)> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let (core, pre) = match v.split_once(['-', '+']) {
        Some((c, p)) => (c, !p.is_empty()),
        None => (v, false),
    };
    let mut it = core.split('.').map(|p| p.parse::<u64>().ok());
    let (a, b, c) = (
        it.next()??,
        it.next().unwrap_or(Some(0))?,
        it.next().unwrap_or(Some(0))?,
    );
    Some((a, b, c, !pre))
}

/// True when `candidate` is a newer version than `current`.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (version_key(candidate), version_key(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Reads {version, url, notes} from a GitHub release or a custom answer.
pub fn parse_release(body: &[u8]) -> Result<(String, Option<String>, Option<String>)> {
    parse_release_full(body).map(|r| (r.0, r.1, r.2))
}

type Release = (
    String,
    Option<String>,
    Option<String>,
    Vec<(String, String)>,
);

/// Like [`parse_release`], plus the downloadable files (GitHub `assets`, or
/// `packages: [{name, url}]`).
pub fn parse_release_full(body: &[u8]) -> Result<Release> {
    let v: Value = serde_json::from_slice(body).context("the answer is not JSON")?;
    let version = v
        .get("tag_name")
        .or_else(|| v.get("version"))
        .and_then(Value::as_str)
        .context("the answer has no version")?;
    let clean = |s: &str, max: usize| -> String {
        s.chars()
            .filter(|c| *c == '\n' || !c.is_control())
            .take(max)
            .collect()
    };
    let version = clean(version, 40);
    if version_key(&version).is_none() {
        bail!("{version:?} is not a version number");
    }
    let url = v
        .get("html_url")
        .or_else(|| v.get("url"))
        .and_then(Value::as_str)
        .filter(|u| u.starts_with("https://") && u.len() <= 300 && !u.chars().any(char::is_control))
        .map(str::to_string);
    let notes = v
        .get("body")
        .or_else(|| v.get("notes"))
        .and_then(Value::as_str)
        .map(|n| clean(n, 4000));
    let mut files = Vec::new();
    let list = v
        .get("assets")
        .or_else(|| v.get("packages"))
        .and_then(Value::as_array);
    for a in list.into_iter().flatten().take(50) {
        let name = a.get("name").and_then(Value::as_str).unwrap_or("");
        let url = a
            .get("browser_download_url")
            .or_else(|| a.get("url"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let name_ok = !name.is_empty()
            && name.len() <= 100
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
            && !name.starts_with('.');
        if name_ok
            && url.starts_with("https://")
            && url.len() <= 500
            && !url.chars().any(char::is_control)
        {
            files.push((name.to_string(), url.to_string()));
        }
    }
    Ok((version, url, notes, files))
}

pub async fn check_now(ctx: &Arc<crate::server::Ctx>) -> UpdateState {
    let source = ctx.config.updates.source();
    let result = match &source {
        None => Err(anyhow::anyhow!("update checks are not configured")),
        Some(url) => crate::http::request("GET", url, None, ctx.tls_client.clone(), 256 * 1024)
            .await
            .and_then(|r| {
                if r.status != 200 {
                    bail!("the server answered {}", r.status)
                }
                parse_release_full(&r.body)
            }),
    };
    let mut st = ctx.updates.state.lock().expect("updates lock");
    st.checked_at = Some(crate::db::now_ms());
    match result {
        Ok((version, url, notes, files)) => {
            st.packages = files;
            if is_newer(&version, crate::network::VERSION) {
                tracing::info!(latest = %version, "a newer xBNC version is available");
            }
            st.latest = Some(version.trim_start_matches(['v', 'V']).to_string());
            st.url = url;
            st.notes = notes;
            st.error = None;
        }
        Err(e) => {
            if source.is_some() {
                tracing::warn!(error = %e, "update check failed");
            }
            st.error = Some(e.to_string());
        }
    }
    st.clone()
}

/// Checks a minute after start and then every day.
pub async fn update_loop(
    ctx: Arc<crate::server::Ctx>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    if ctx.config.updates.source().is_none() {
        return;
    }
    let mut wait = Duration::from_secs(60);
    loop {
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = shutdown.changed() => return,
        }
        check_now(&ctx).await;
        wait = Duration::from_secs(24 * 3600);
    }
}

// ------------------------------------------------------------ one-click update

/// Where the daemon leaves a verified package for the root helper.
pub fn spool_dir(ctx: &crate::server::Ctx) -> std::path::PathBuf {
    ctx.config
        .database
        .path
        .parent()
        .map(|p| p.join("updates"))
        .unwrap_or_else(|| "updates".into())
}

/// Which package format this installation uses, if it came from a package.
pub fn package_kind() -> Option<&'static str> {
    let exe = std::env::current_exe().ok()?;
    if exe != std::path::Path::new("/usr/bin/xbncd") {
        return None;
    }
    if std::path::Path::new("/usr/bin/dpkg").exists() {
        Some("deb")
    } else if std::path::Path::new("/usr/bin/rpm").exists() {
        Some("rpm")
    } else {
        None
    }
}

/// Why the panel cannot update by itself (None when it can).
pub fn one_click_blocker(ctx: &crate::server::Ctx) -> Option<&'static str> {
    if ctx.config.updates.public_key.is_none() {
        return Some("no_key");
    }
    if package_kind().is_none() {
        return Some("not_package");
    }
    None
}

/// Picks the package for this system from the release files.
pub fn pick_package(
    files: &[(String, String)],
    kind: &str,
    version: &str,
) -> Option<(String, String, String)> {
    // The package for the architecture this binary was built for.
    let (deb, rpm) = if cfg!(target_arch = "aarch64") {
        ("_arm64.deb", ".aarch64.rpm")
    } else {
        ("_amd64.deb", ".x86_64.rpm")
    };
    let suffix = if kind == "deb" { deb } else { rpm };
    let arch_ok = cfg!(any(target_arch = "x86_64", target_arch = "aarch64"));
    let (name, url) = files.iter().find(|(n, _)| {
        arch_ok
            && n.ends_with(suffix)
            && crate::release::package_version(n).as_deref() == Some(version)
    })?;
    let sig = files.iter().find(|(n, _)| *n == format!("{name}.sig"))?;
    Some((name.clone(), url.clone(), sig.1.clone()))
}

/// Downloads and verifies the newest package and queues it for the root
/// helper (`xbncd apply-update`, started by the `xbnc-update.path` unit).
pub async fn queue_install(ctx: &Arc<crate::server::Ctx>, by: &str) -> Result<String> {
    if let Some(b) = one_click_blocker(ctx) {
        bail!(if b == "no_key" {
            "updates.public_key is not set"
        } else {
            "xBNC was not installed from a package"
        });
    }
    let st = ctx.updates.state();
    let latest = st.latest.clone().context("check for updates first")?;
    if !is_newer(&latest, crate::network::VERSION) {
        bail!("there is no newer version");
    }
    let kind = package_kind().context("unknown package format")?;
    let (name, url, sig_url) = pick_package(&st.packages, kind, &latest)
        .context("the release has no signed package for this system")?;
    let tls = ctx.tls_client.clone();
    let data = crate::http::download(
        &url,
        tls.clone(),
        200 * 1024 * 1024,
        Duration::from_secs(600),
    )
    .await?;
    let sig = crate::http::download(&sig_url, tls, 4096, Duration::from_secs(30)).await?;
    let sig = String::from_utf8(sig).context("invalid signature file")?;
    let key = ctx.config.updates.public_key.clone().unwrap_or_default();
    crate::release::verify(&key, &name, &data, &sig)?;
    let dir = spool_dir(ctx);
    let (n2, by2) = (name.clone(), by.to_string());
    tokio::task::spawn_blocking(move || -> Result<()> {
        std::fs::create_dir_all(&dir)?;
        let _ = std::fs::remove_file(dir.join("result.json"));
        std::fs::write(dir.join(&n2), &data)?;
        std::fs::write(dir.join(format!("{n2}.sig")), sig.as_bytes())?;
        let pending =
            serde_json::json!({ "package": n2, "requested_by": by2, "at": crate::db::now_ms() });
        // Written last: its appearance starts the root helper.
        let tmp = dir.join(".pending.json");
        std::fs::write(&tmp, pending.to_string())?;
        std::fs::rename(tmp, dir.join("pending.json"))?;
        Ok(())
    })
    .await??;
    Ok(latest)
}

/// Result of the last one-click update, written by the root helper.
pub fn last_result(ctx: &crate::server::Ctx) -> Option<Value> {
    let raw = std::fs::read(spool_dir(ctx).join("result.json")).ok()?;
    if raw.len() > 8192 {
        return None;
    }
    serde_json::from_slice(&raw).ok()
}

/// Run as root by `xbnc-update.service`. Never trusts the daemon: copies
/// the package out of the daemon-writable directory, verifies the signature
/// with the key from the root-owned configuration, refuses downgrades and
/// installs it with the system package manager.
pub fn apply_pending(cfg: &crate::config::Config) -> Result<String> {
    apply_pending_in(cfg, std::path::Path::new("/run/xbnc-update"))
}

fn apply_pending_in(cfg: &crate::config::Config, work: &std::path::Path) -> Result<String> {
    let spool = cfg
        .database
        .path
        .parent()
        .map(|p| p.join("updates"))
        .context("no data directory")?;
    // The directory belongs to the daemon's user: never follow links in it.
    if std::fs::symlink_metadata(&spool)
        .map(|m| !m.is_dir())
        .unwrap_or(true)
    {
        bail!("{} is not a directory", spool.display());
    }
    let pending_path = spool.join("pending.json");
    let raw = read_nofollow(&pending_path, 8192).context("no update is pending")?;
    std::fs::remove_file(&pending_path)?;
    let p: Value = serde_json::from_slice(&raw).context("invalid pending.json")?;
    let name = p
        .get("package")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let version = crate::release::package_version(&name).context("invalid package name")?;
    if name.contains('/') || name.starts_with('.') {
        bail!("invalid package name");
    }
    let key = cfg
        .updates
        .public_key
        .clone()
        .context("updates.public_key is not set")?;
    let _ = std::fs::remove_dir_all(work);
    std::fs::create_dir_all(work)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(work, std::fs::Permissions::from_mode(0o700))?;
    }
    let pkg = work.join(&name);
    let data = read_nofollow(&spool.join(&name), 200 * 1024 * 1024)?;
    let sig = String::from_utf8(read_nofollow(&spool.join(format!("{name}.sig")), 4096)?)
        .context("invalid signature file")?;
    // Verify the copy we will install, not the daemon's file.
    std::fs::write(&pkg, &data)?;
    crate::release::verify(&key, &name, &std::fs::read(&pkg)?, &sig)?;
    if !is_newer(&version, crate::network::VERSION) {
        bail!(
            "{version} is not newer than the installed {}",
            crate::network::VERSION
        );
    }
    let (prog, args): (&str, Vec<&str>) = if name.ends_with(".deb") {
        ("dpkg", vec!["-i"])
    } else {
        ("rpm", vec!["-U"])
    };
    let out = std::process::Command::new(prog)
        .args(&args)
        .arg(&pkg)
        .output()
        .with_context(|| format!("cannot run {prog}"))?;
    let _ = std::fs::remove_dir_all(work);
    for f in [name.clone(), format!("{name}.sig")] {
        let _ = std::fs::remove_file(spool.join(f));
    }
    if !out.status.success() {
        bail!(
            "{prog} failed: {}",
            String::from_utf8_lossy(&out.stderr)
                .chars()
                .take(500)
                .collect::<String>()
        );
    }
    Ok(version)
}

/// Reads a regular file without following a symbolic link, up to `max` bytes.
pub fn read_nofollow(path: &std::path::Path, max: u64) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut o = std::fs::OpenOptions::new();
    o.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.custom_flags(libc::O_NOFOLLOW);
    }
    let f = o
        .open(path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    let meta = f.metadata()?;
    if !meta.is_file() || meta.len() > max {
        bail!(
            "{} is not a regular file of at most {max} bytes",
            path.display()
        );
    }
    let mut buf = Vec::new();
    f.take(max + 1).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Replaces a file the daemon's user can see, without following a link it
/// may have left there.
pub fn write_nofollow(path: &std::path::Path, data: &[u8], mode: u32) -> Result<()> {
    use std::io::Write;
    let _ = std::fs::remove_file(path);
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    o.open(path)?.write_all(data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("v0.7.0", "0.6.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.7.0", "0.7.0-beta.1"));
        assert!(!is_newer("0.7.0-beta.1", "0.7.0"));
        assert!(!is_newer("0.6.0", "0.6.0"));
        assert!(!is_newer("garbage", "0.6.0"));
        assert!(is_newer("0.10", "0.9.5"));
    }

    #[test]
    fn releases() {
        let (v, u, n) = parse_release(br#"{"tag_name":"v0.7.0","html_url":"https://github.com/o/r/releases/tag/v0.7.0","body":"Fixes\n- a"}"#).unwrap();
        assert_eq!(
            (v.as_str(), u.as_deref(), n.as_deref()),
            (
                "v0.7.0",
                Some("https://github.com/o/r/releases/tag/v0.7.0"),
                Some("Fixes\n- a")
            )
        );
        let (v, u, _) =
            parse_release(br#"{"version":"1.0.0","url":"javascript:alert(1)"}"#).unwrap();
        assert_eq!((v.as_str(), u), ("1.0.0", None));
        assert!(parse_release(br#"{"version":"latest"}"#).is_err());
        assert!(parse_release(b"<html>").is_err());
    }

    #[test]
    fn config() {
        let c = UpdatesConfig {
            check: true,
            repository: Some("y2k/xbnc".into()),
            url: None,
            public_key: None,
        };
        assert!(c.validate().is_ok());
        assert_eq!(
            c.source().unwrap(),
            "https://api.github.com/repos/y2k/xbnc/releases/latest"
        );
        for bad in ["noslash", "a/b/c", "a/b c", "/x"] {
            let c = UpdatesConfig {
                check: true,
                repository: Some(bad.into()),
                url: None,
                public_key: None,
            };
            assert!(c.validate().is_err(), "{bad}");
        }
        let c = UpdatesConfig {
            check: true,
            repository: None,
            url: Some("http://example.org/v.json".into()),
            public_key: None,
        };
        assert!(c.validate().is_err());
        assert!(UpdatesConfig { check: false, ..c }.source().is_none());
        let bad_key = UpdatesConfig {
            public_key: Some("not-a-key".into()),
            ..Default::default()
        };
        assert!(bad_key.validate().is_err());
    }

    #[test]
    fn release_files() {
        let body = br#"{"tag_name":"v9.0.0","assets":[
            {"name":"xbnc_9.0.0_amd64.deb","browser_download_url":"https://h/x.deb"},
            {"name":"xbnc_9.0.0_amd64.deb.sig","browser_download_url":"https://h/x.deb.sig"},
            {"name":"xbnc-9.0.0-1.x86_64.rpm","browser_download_url":"https://h/x.rpm"},
            {"name":"../evil.deb","browser_download_url":"https://h/e"},
            {"name":"plain.deb","browser_download_url":"http://h/p"}]}"#;
        let (_, _, _, files) = parse_release_full(body).unwrap();
        assert_eq!(files.len(), 3);
        if cfg!(target_arch = "x86_64") {
            let (name, url, sig) = pick_package(&files, "deb", "9.0.0").unwrap();
            assert_eq!(
                (name.as_str(), url.as_str(), sig.as_str()),
                (
                    "xbnc_9.0.0_amd64.deb",
                    "https://h/x.deb",
                    "https://h/x.deb.sig"
                )
            );
            // No signature, no package.
            assert!(pick_package(&files, "rpm", "9.0.0").is_none());
            assert!(pick_package(&files, "deb", "9.0.1").is_none());
        }
    }

    fn spool_config(dir: &std::path::Path, key: &str) -> crate::config::Config {
        let toml = format!(
            "secret_key_file = \"/nonexistent/key\"\n[database]\npath = \"{}\"\n[updates]\npublic_key = \"{key}\"\n",
            dir.join("xbnc.db").display()
        );
        let c: crate::config::Config = toml::from_str(&toml).unwrap();
        c.validate().unwrap();
        c
    }

    #[test]
    fn root_helper_trusts_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let (private, public) = crate::release::keygen().unwrap();
        let cfg = spool_config(tmp.path(), &public);
        let spool = tmp.path().join("updates");
        let work = tmp.path().join("work");
        std::fs::create_dir_all(&spool).unwrap();
        let queue = |name: &str, data: &[u8], signed: &[u8]| {
            std::fs::write(spool.join(name), data).unwrap();
            let sig = crate::release::sign(&private, name, signed).unwrap();
            std::fs::write(spool.join(format!("{name}.sig")), sig).unwrap();
            std::fs::write(
                spool.join("pending.json"),
                format!(r#"{{"package":"{name}"}}"#),
            )
            .unwrap();
        };
        let err = |e: anyhow::Error| format!("{e:#}");

        // Nothing queued.
        assert!(apply_pending_in(&cfg, &work).is_err());
        // Changed after signing.
        queue("xbnc_99.0.0_amd64.deb", b"evil", b"good");
        assert!(
            err(apply_pending_in(&cfg, &work).unwrap_err()).contains("signature does not match")
        );
        assert!(
            !spool.join("pending.json").exists(),
            "a request is consumed once"
        );
        // Correctly signed but older: no downgrades.
        queue("xbnc_0.0.1_amd64.deb", b"old", b"old");
        assert!(err(apply_pending_in(&cfg, &work).unwrap_err()).contains("not newer"));
        // Names that are not packages, or that escape the directory.
        for name in ["../xbnc_99.0.0_amd64.deb", "xbnc.deb", "x"] {
            std::fs::write(
                spool.join("pending.json"),
                format!(r#"{{"package":"{name}"}}"#),
            )
            .unwrap();
            assert!(apply_pending_in(&cfg, &work).is_err(), "{name}");
        }
        #[cfg(unix)]
        {
            // A link left by the daemon's user is never followed.
            std::fs::write(tmp.path().join("secret"), b"x").unwrap();
            std::os::unix::fs::symlink(tmp.path().join("secret"), spool.join("pending.json"))
                .unwrap();
            assert!(apply_pending_in(&cfg, &work).is_err());
            let _ = std::fs::remove_file(spool.join("pending.json"));
            std::os::unix::fs::symlink(tmp.path().join("secret"), spool.join("result.json"))
                .unwrap();
            write_nofollow(&spool.join("result.json"), b"{}", 0o644).unwrap();
            assert_eq!(std::fs::read(tmp.path().join("secret")).unwrap(), b"x");
        }
    }
}
