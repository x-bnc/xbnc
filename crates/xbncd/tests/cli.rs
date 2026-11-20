//! The command line, run as a real process.

use std::io::Read;
use std::process::{Command, Stdio};

fn config(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("xbnc.toml");
    std::fs::write(
        &path,
        format!(
            "hostname = \"10.0.0.5\"\nsecret_key_file = \"{0}/secret.key\"\n\
             [database]\npath = \"{0}/xbnc.db\"\n\
             [web]\naddress = \"0.0.0.0:8443\"\ninsecure = true\n",
            dir.display()
        ),
    )
    .unwrap();
    path
}

/// `xbncd info` shows the panel at the address given at installation.
#[test]
fn info_shows_the_installed_address() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let out = Command::new(env!("CARGO_BIN_EXE_xbncd"))
        .args(["-c", cfg.to_str().unwrap(), "info"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.lines()
            .next()
            .unwrap()
            .contains("http://10.0.0.5:8443"),
        "{text}"
    );
}

/// `xbncd info | head -1`: when the reader closes the pipe the command stops
/// quietly (killed by SIGPIPE, like other tools) instead of panicking with
/// "failed printing to stdout: Broken pipe".
#[cfg(unix)]
#[test]
fn info_into_a_closed_pipe_does_not_panic() {
    use std::os::unix::process::ExitStatusExt;
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let mut child = Command::new(env!("CARGO_BIN_EXE_xbncd"))
        .args(["-c", cfg.to_str().unwrap(), "info"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Close the reading end before the command writes anything.
    drop(child.stdout.take());
    let mut err = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    let status = child.wait().unwrap();
    assert!(!err.contains("panicked"), "{err}");
    assert!(
        status.success() || status.signal() == Some(libc_sigpipe()),
        "{status:?} {err}"
    );
}

#[cfg(unix)]
fn libc_sigpipe() -> i32 {
    13
}
