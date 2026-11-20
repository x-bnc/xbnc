//! Backups from the panel: a `.tar.gz` with the database (online copy with
//! integrity check), the secret key and the installed plugins. Restoring
//! stages the archive; it is applied when xBNC starts again.

use crate::db::{self, Db};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Largest archive accepted for a restore.
pub const MAX_RESTORE_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct BackupInfo {
    pub name: String,
    pub size: u64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub version: String,
    pub schema: i64,
    pub created_at: i64,
    pub created_by: String,
}

pub fn backups_dir(cfg: &crate::config::Config) -> PathBuf {
    data_dir(cfg).join("backups")
}

fn data_dir(cfg: &crate::config::Config) -> PathBuf {
    cfg.database
        .path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn restore_dir(cfg: &crate::config::Config) -> PathBuf {
    data_dir(cfg).join("restore-pending")
}

/// A backup file name chosen by xBNC (also what downloads must match).
pub fn valid_name(name: &str) -> bool {
    name.starts_with("xbnc-")
        && name.ends_with(".tar.gz")
        && name.len() < 80
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        && !name.contains("..")
}

#[cfg(unix)]
fn private_file(path: &Path) -> Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    Ok(std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?)
}
#[cfg(not(unix))]
fn private_file(path: &Path) -> Result<std::fs::File> {
    Ok(std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?)
}

fn private_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Creates a backup now (blocking: run it off the async threads).
pub fn create(cfg: &crate::config::Config, db: &Db, by: &str) -> Result<BackupInfo> {
    let dir = backups_dir(cfg);
    private_dir(&dir)?;
    let now = db::now_ms();
    let stamp = time::OffsetDateTime::from_unix_timestamp(now / 1000)
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH)
        .format(time::macros::format_description!(
            "[year][month][day]T[hour][minute][second]Z"
        ))
        .unwrap_or_default();
    let mut name = format!("xbnc-{stamp}.tar.gz");
    let mut n = 1;
    while dir.join(&name).exists() {
        name = format!("xbnc-{stamp}-{n}.tar.gz");
        n += 1;
    }
    let tmp_db = dir.join(format!(".{name}.db"));
    let _ = std::fs::remove_file(&tmp_db);
    db.with(|c| db::backup_to(c, &tmp_db))?;
    let schema = db.with(|c| db::schema_version(c))?;
    let result = (|| -> Result<()> {
        let file = private_file(&dir.join(format!(".{name}.part")))?;
        let gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut tar = tar::Builder::new(gz);
        tar.mode(tar::HeaderMode::Deterministic);
        let manifest = Manifest {
            format: 1,
            version: crate::network::VERSION.to_string(),
            schema,
            created_at: now,
            created_by: by.to_string(),
        };
        append_bytes(
            &mut tar,
            "manifest.json",
            &serde_json::to_vec_pretty(&manifest)?,
        )?;
        append_file(&mut tar, "xbnc.db", &tmp_db)?;
        if cfg.secret_key_file.exists() {
            append_file(&mut tar, "secret.key", &cfg.secret_key_file)?;
        }
        let plugins = data_dir(cfg).join("plugins");
        if let Ok(rd) = std::fs::read_dir(&plugins) {
            for e in rd.flatten() {
                let p = e.path();
                let fname = e.file_name().to_string_lossy().to_string();
                if p.is_file() && fname.ends_with(".wasm") && !fname.starts_with('.') {
                    append_file(&mut tar, &format!("plugins/{fname}"), &p)?;
                }
            }
        }
        let gz = tar.into_inner()?;
        let mut file = gz.finish()?;
        file.flush()?;
        file.sync_all()?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&tmp_db);
    if let Err(e) = result {
        let _ = std::fs::remove_file(dir.join(format!(".{name}.part")));
        return Err(e);
    }
    std::fs::rename(dir.join(format!(".{name}.part")), dir.join(&name))?;
    let size = std::fs::metadata(dir.join(&name))?.len();
    Ok(BackupInfo {
        name,
        size,
        created_at: now,
    })
}

fn append_bytes<W: Write>(tar: &mut tar::Builder<W>, name: &str, data: &[u8]) -> Result<()> {
    let mut h = tar::Header::new_gnu();
    h.set_size(data.len() as u64);
    h.set_mode(0o600);
    h.set_cksum();
    tar.append_data(&mut h, name, data)?;
    Ok(())
}

fn append_file<W: Write>(tar: &mut tar::Builder<W>, name: &str, path: &Path) -> Result<()> {
    let mut f =
        std::fs::File::open(path).with_context(|| format!("cannot read {}", path.display()))?;
    let len = f.metadata()?.len();
    let mut h = tar::Header::new_gnu();
    h.set_size(len);
    h.set_mode(0o600);
    h.set_cksum();
    tar.append_data(&mut h, name, &mut f)?;
    Ok(())
}

/// Backups in the folder, newest first.
pub fn list(cfg: &crate::config::Config) -> Vec<BackupInfo> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(backups_dir(cfg)) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !valid_name(&name) {
                continue;
            }
            if let Ok(m) = e.metadata() {
                let created_at = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                out.push(BackupInfo {
                    name,
                    size: m.len(),
                    created_at,
                });
            }
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.name.cmp(&a.name)));
    out
}

/// Keeps the newest `keep` backups.
pub fn rotate(cfg: &crate::config::Config, keep: usize) -> usize {
    let mut removed = 0;
    for b in list(cfg).into_iter().skip(keep.max(1)) {
        if std::fs::remove_file(backups_dir(cfg).join(&b.name)).is_ok() {
            removed += 1;
        }
    }
    removed
}

pub fn delete(cfg: &crate::config::Config, name: &str) -> Result<bool> {
    if !valid_name(name) {
        bail!("invalid backup name");
    }
    match std::fs::remove_file(backups_dir(cfg).join(name)) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Unpacks and checks an archive into `dest` (which must not exist).
/// Only the expected entries are accepted: no links, no paths outside.
pub fn unpack_checked(archive: &Path, dest: &Path) -> Result<Manifest> {
    private_dir(dest)?;
    let f = std::fs::File::open(archive)?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(f));
    let mut total: u64 = 0;
    let mut seen_db = false;
    for entry in tar.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            bail!("the archive contains something that is not a file");
        }
        let path = entry.path()?.to_string_lossy().to_string();
        let ok = matches!(path.as_str(), "manifest.json" | "xbnc.db" | "secret.key")
            || (path.starts_with("plugins/")
                && path.ends_with(".wasm")
                && !path[8..].contains(['/', '\\'])
                && !path[8..].starts_with('.')
                && path.len() < 100);
        if !ok || path.contains("..") {
            bail!("unexpected file in the archive: {path}");
        }
        total += entry.size();
        if total > MAX_RESTORE_BYTES {
            bail!("the archive is too large");
        }
        let target = dest.join(&path);
        if let Some(parent) = target.parent() {
            private_dir(parent)?;
        }
        let mut out = private_file(&target)?;
        let copied = std::io::copy(&mut (&mut entry).take(MAX_RESTORE_BYTES), &mut out)?;
        if copied != entry.size() {
            bail!("truncated archive");
        }
        seen_db |= path == "xbnc.db";
    }
    if !seen_db {
        bail!("the archive has no database (xbnc.db)");
    }
    let manifest: Manifest = serde_json::from_slice(
        &std::fs::read(dest.join("manifest.json")).context("the archive has no manifest.json")?,
    )
    .context("invalid manifest.json")?;
    if manifest.format != 1 {
        bail!("unknown backup format {}", manifest.format);
    }
    if let Ok(key) = std::fs::read(dest.join("secret.key")) {
        if key.len() != 32 {
            bail!("the secret key in the archive is not valid");
        }
    }
    let conn = rusqlite::Connection::open_with_flags(
        dest.join("xbnc.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let check: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if check != "ok" {
        bail!("the database in the archive is damaged: {check}");
    }
    let schema: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if schema > db::SCHEMA_VERSION {
        bail!("the backup comes from a newer xBNC (schema {schema}); update first");
    }
    Ok(manifest)
}

/// Checks an archive and stages it; it is applied on the next start.
pub fn stage_restore(cfg: &crate::config::Config, archive: &Path) -> Result<Manifest> {
    let dir = restore_dir(cfg);
    let _ = std::fs::remove_dir_all(&dir);
    let tmp = data_dir(cfg).join(".restore-check");
    let _ = std::fs::remove_dir_all(&tmp);
    let m = match unpack_checked(archive, &tmp) {
        Ok(m) => m,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
    };
    std::fs::rename(&tmp, &dir)?;
    std::fs::write(dir.join("READY"), b"1")?;
    Ok(m)
}

pub fn restore_pending(cfg: &crate::config::Config) -> bool {
    restore_dir(cfg).join("READY").exists()
}

pub fn cancel_restore(cfg: &crate::config::Config) {
    let _ = std::fs::remove_dir_all(restore_dir(cfg));
}

/// Called at startup before the database is opened: swaps in a staged
/// restore, keeping the replaced files next to it.
pub fn apply_pending_restore(cfg: &crate::config::Config) -> Result<Option<Manifest>> {
    let dir = restore_dir(cfg);
    if !dir.join("READY").exists() {
        return Ok(None);
    }
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(dir.join("manifest.json"))?)?;
    let stamp = db::now_ms();
    let db_path = &cfg.database.path;
    let keep = |p: &Path| -> Result<()> {
        if p.exists() {
            let mut name = p.file_name().unwrap_or_default().to_os_string();
            name.push(format!(".before-restore-{stamp}"));
            std::fs::rename(p, p.with_file_name(name))?;
        }
        Ok(())
    };
    for suffix in ["", "-wal", "-shm"] {
        let mut p = db_path.as_os_str().to_os_string();
        p.push(suffix);
        keep(Path::new(&p))?;
    }
    std::fs::rename(dir.join("xbnc.db"), db_path)?;
    if dir.join("secret.key").exists() {
        let same =
            std::fs::read(&cfg.secret_key_file).ok() == std::fs::read(dir.join("secret.key")).ok();
        if !same {
            keep(&cfg.secret_key_file)?;
            std::fs::rename(dir.join("secret.key"), &cfg.secret_key_file)?;
        }
    }
    let plugins_src = dir.join("plugins");
    if plugins_src.exists() {
        let plugins = data_dir(cfg).join("plugins");
        keep(&plugins)?;
        std::fs::rename(&plugins_src, &plugins)?;
    }
    std::fs::remove_dir_all(&dir)?;
    Ok(Some(manifest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(dir: &Path) -> crate::config::Config {
        let toml = format!(
            "secret_key_file = \"{d}/secret.key\"\n[database]\npath = \"{d}/xbnc.db\"\n",
            d = dir.display()
        );
        toml::from_str(&toml).unwrap()
    }

    #[test]
    fn backup_restore_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let c = cfg(tmp.path());
        std::fs::write(&c.secret_key_file, [7u8; 32]).unwrap();
        std::fs::create_dir_all(tmp.path().join("plugins")).unwrap();
        std::fs::write(tmp.path().join("plugins/dice.wasm"), b"\0asm").unwrap();
        let db = Db::open(&c.database.path).unwrap();
        db.with(|c| db::create_user(c, "alice", "x", true)).unwrap();
        let b = create(&c, &db, "alice").unwrap();
        assert!(valid_name(&b.name) && b.size > 0);
        assert_eq!(list(&c).len(), 1);

        // Changes after the backup...
        db.with(|c| db::create_user(c, "bob", "x", false)).unwrap();
        stage_restore(&c, &backups_dir(&c).join(&b.name)).unwrap();
        assert!(restore_pending(&c));
        drop(db);
        // ...are gone once the restore is applied at startup.
        let m = apply_pending_restore(&c).unwrap().unwrap();
        assert_eq!(m.created_by, "alice");
        let db = Db::open(&c.database.path).unwrap();
        let users = db.with(|c| db::list_users(c)).unwrap();
        assert_eq!(
            users
                .iter()
                .map(|u| u.username.as_str())
                .collect::<Vec<_>>(),
            vec!["alice"]
        );
        assert!(tmp.path().join("plugins/dice.wasm").exists());
        assert!(!restore_pending(&c));

        // Rotation keeps the newest.
        create(&c, &db, "alice").unwrap();
        create(&c, &db, "alice").unwrap();
        assert_eq!(rotate(&c, 2), 1);
        assert_eq!(list(&c).len(), 2);
    }

    #[test]
    fn hostile_archives_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let make = |entries: &[(&str, &[u8])]| {
            let p = tmp.path().join(format!("a{}.tar.gz", entries.len()));
            let f = std::fs::File::create(&p).unwrap();
            let mut t = tar::Builder::new(flate2::write::GzEncoder::new(
                f,
                flate2::Compression::fast(),
            ));
            for (name, data) in entries {
                let mut h = tar::Header::new_gnu();
                h.set_size(data.len() as u64);
                h.set_mode(0o600);
                // Bypass the builder's own path checks to forge an entry.
                h.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name.as_bytes());
                h.set_cksum();
                t.append(&h, *data).unwrap();
            }
            t.into_inner().unwrap().finish().unwrap();
            p
        };
        for bad in [
            make(&[("../evil", b"x")]),
            make(&[("plugins/../../x.wasm", b"x")]),
            make(&[(
                "manifest.json",
                br#"{"format":1,"version":"1","schema":1,"created_at":0,"created_by":"x"}"#,
            )]),
        ] {
            let dest = tmp
                .path()
                .join(format!("out-{}", bad.display().to_string().len()));
            assert!(unpack_checked(&bad, &dest).is_err(), "{}", bad.display());
        }
    }
}
