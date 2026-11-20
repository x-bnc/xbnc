//! Plain-text logs (the `log` module), in the classic IRC log format that
//! common log tools understand:
//!
//! `<data dir>/logs/<user>/<network>/<target>/<YYYY-MM-DD>.log`
//!
//! Writing happens on a dedicated thread fed by a bounded channel, so a slow
//! disk never blocks a network actor; if the queue is full, lines are
//! dropped and counted. Paths are built only from sanitised components, so
//! a channel or nick name can never escape the log directory. Times are in
//! the user's time zone (UTC when none is set).

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

enum Job {
    Line {
        user: String,
        network: String,
        target: String,
        ts: i64,
        text: String,
    },
    /// Deletes files older than `days` for one network.
    Cleanup {
        user: String,
        network: String,
        days: u64,
    },
}

#[derive(Clone)]
pub struct LogSink {
    tx: SyncSender<Job>,
    root: Arc<PathBuf>,
    pub dropped: Arc<AtomicU64>,
}

const QUEUE: usize = 20_000;
const OPEN_FILES: usize = 64;

impl LogSink {
    pub fn new(root: PathBuf) -> LogSink {
        let (tx, rx) = sync_channel(QUEUE);
        let r = root.clone();
        std::thread::Builder::new()
            .name("xbnc-logs".into())
            .spawn(move || writer(r, rx))
            .expect("spawn log thread");
        LogSink {
            tx,
            root: Arc::new(root),
            dropped: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Queues one already-formatted line (without timestamp). `ts` is the
    /// local wall-clock time in milliseconds (UTC time plus the offset).
    pub fn write(&self, user: &str, network: &str, target: &str, ts: i64, text: String) {
        let job = Job::Line {
            user: user.to_string(),
            network: network.to_string(),
            target: target.to_string(),
            ts,
            text,
        };
        if let Err(TrySendError::Full(_)) = self.tx.try_send(job) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn cleanup(&self, user: &str, network: &str, days: u64) {
        let _ = self.tx.try_send(Job::Cleanup {
            user: user.to_string(),
            network: network.to_string(),
            days,
        });
    }
}

/// Makes one safe path component: letters, digits and `#&+._-` are kept,
/// everything else becomes `%XX`; `.` and `..` cannot result.
pub fn component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.to_lowercase().bytes() {
        if b.is_ascii_alphanumeric() || b"#&+_-".contains(&b) || (b == b'.' && !out.is_empty()) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    out.truncate(120);
    out
}

pub fn dir_for(root: &Path, user: &str, network: &str) -> PathBuf {
    root.join(component(user)).join(component(network))
}

fn day_and_time(ts: i64) -> (String, String) {
    let t = time::OffsetDateTime::from_unix_timestamp_nanos(ts as i128 * 1_000_000)
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    (
        format!("{:04}-{:02}-{:02}", t.year(), t.month() as u8, t.day()),
        format!("{:02}:{:02}:{:02}", t.hour(), t.minute(), t.second()),
    )
}

fn create_dir(p: &Path) -> std::io::Result<()> {
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(p)
}

fn open_append(p: &Path) -> std::io::Result<File> {
    let mut o = OpenOptions::new();
    o.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(p)
}

fn writer(root: PathBuf, rx: Receiver<Job>) {
    let mut files: HashMap<PathBuf, (File, Instant)> = HashMap::new();
    let mut warned = false;
    while let Ok(job) = rx.recv() {
        match job {
            Job::Line {
                user,
                network,
                target,
                ts,
                text,
            } => {
                let (day, clock) = day_and_time(ts);
                let dir = dir_for(&root, &user, &network).join(component(&target));
                let path = dir.join(format!("{day}.log"));
                if !files.contains_key(&path) {
                    if files.len() >= OPEN_FILES {
                        // Close the least recently used file.
                        if let Some(old) = files
                            .iter()
                            .min_by_key(|(_, (_, t))| *t)
                            .map(|(p, _)| p.clone())
                        {
                            files.remove(&old);
                        }
                    }
                    match create_dir(&dir).and_then(|_| open_append(&path)) {
                        Ok(f) => {
                            files.insert(path.clone(), (f, Instant::now()));
                        }
                        Err(e) => {
                            if !warned {
                                tracing::warn!(error = %e, path = %path.display(), "cannot write log file");
                                warned = true;
                            }
                            continue;
                        }
                    }
                }
                if let Some((f, used)) = files.get_mut(&path) {
                    *used = Instant::now();
                    let _ = writeln!(f, "[{clock}] {text}");
                }
            }
            Job::Cleanup {
                user,
                network,
                days,
            } => {
                files.clear();
                if days > 0 {
                    cleanup(&dir_for(&root, &user, &network), days);
                }
            }
        }
    }
}

fn cleanup(dir: &Path, days: u64) {
    let cutoff = SystemTime::now() - Duration::from_secs(days * 86_400);
    let Ok(targets) = std::fs::read_dir(dir) else {
        return;
    };
    for t in targets.flatten() {
        let Ok(entries) = std::fs::read_dir(t.path()) else {
            continue;
        };
        for e in entries.flatten() {
            let old = e
                .metadata()
                .and_then(|m| m.modified())
                .map(|m| m < cutoff)
                .unwrap_or(false);
            if old && e.path().extension().is_some_and(|x| x == "log") {
                let _ = std::fs::remove_file(e.path());
            }
        }
        let _ = std::fs::remove_dir(t.path()); // only succeeds when empty
    }
}

/// Formats a stored message as a classic IRC log line.
pub fn format_line(command: &str, sender: &str, target: &str, text: &str) -> String {
    let nick = sender.split('!').next().unwrap_or(sender);
    let host = sender.split_once('!').map(|(_, h)| h).unwrap_or("");
    let reason = |t: &str| {
        if t.is_empty() {
            String::new()
        } else {
            format!(" ({t})")
        }
    };
    match command {
        "PRIVMSG" => match text.strip_prefix("\x01ACTION ") {
            Some(a) => format!("* {nick} {}", a.trim_end_matches('\x01')),
            None => format!("<{nick}> {text}"),
        },
        "NOTICE" => format!("-{nick}- {text}"),
        "JOIN" => format!("*** Joins: {nick} ({host})"),
        "PART" => format!("*** Parts: {nick} ({host}){}", reason(text)),
        "QUIT" => format!("*** Quits: {nick} ({host}){}", reason(text)),
        "KICK" => {
            let (victim, why) = text.split_once(' ').unwrap_or((text, ""));
            format!("*** {victim} was kicked by {nick}{}", reason(why))
        }
        "NICK" => format!("*** {nick} is now known as {text}"),
        "TOPIC" => format!("*** {nick} changes topic to '{text}'"),
        "MODE" => format!("*** {nick} sets mode: {text}"),
        _ => format!("{command} {target} {text}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn components_are_safe() {
        assert_eq!(component("#Xbnc-Dev"), "#xbnc-dev");
        assert_eq!(component(".."), "%2E.");
        assert_eq!(component("."), "%2E");
        assert_eq!(component("a/../b"), "a%2F..%2Fb");
        assert_eq!(component(""), "_");
        assert_eq!(component("nick\\x"), "nick%5Cx");
        assert!(!component("#a\0b").contains('\0'));
        let p = dir_for(Path::new("/r"), "../../etc", "x");
        assert!(p.starts_with("/r"));
        assert_eq!(p.components().count(), 4);
    }

    #[test]
    fn classic_format() {
        assert_eq!(format_line("PRIVMSG", "bob!u@h", "#c", "hi"), "<bob> hi");
        assert_eq!(
            format_line("PRIVMSG", "bob!u@h", "#c", "\x01ACTION waves\x01"),
            "* bob waves"
        );
        assert_eq!(format_line("NOTICE", "bob", "#c", "hey"), "-bob- hey");
        assert_eq!(
            format_line("JOIN", "bob!u@h", "#c", ""),
            "*** Joins: bob (u@h)"
        );
        assert_eq!(
            format_line("KICK", "op!u@h", "#c", "bob spam"),
            "*** bob was kicked by op (spam)"
        );
        assert_eq!(
            format_line("QUIT", "bob!u@h", "#c", ""),
            "*** Quits: bob (u@h)"
        );
    }

    #[test]
    fn writes_and_cleans() {
        let dir = tempfile::tempdir().unwrap();
        let sink = LogSink::new(dir.path().to_path_buf());
        let ts = 1_700_000_000_000; // 2023-11-14 22:13:20 UTC
        sink.write("Alice", "libera", "#Xbnc-Dev", ts, "<bob> hi".into());
        sink.write(
            "Alice",
            "libera",
            "#Xbnc-Dev",
            ts + 1000,
            "<bob> again".into(),
        );
        let file = dir.path().join("alice/libera/#xbnc-dev/2023-11-14.log");
        for _ in 0..100 {
            if std::fs::read_to_string(&file)
                .map(|s| s.lines().count())
                .ok()
                == Some(2)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "[22:13:20] <bob> hi\n[22:13:21] <bob> again\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        // Retention keeps fresh files.
        sink.cleanup("alice", "libera", 1);
        std::thread::sleep(Duration::from_millis(100));
        assert!(file.exists());
    }
}
