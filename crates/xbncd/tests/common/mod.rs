//! Shared harness for end-to-end tests: a real xBNC instance in-process, a
//! scripted IRC server acting as the upstream network, and raw TCP clients.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use xbncd::db::{self, Db, NewNetwork};

pub const PASSWORD: &str = "password123";
pub const WAIT: Duration = Duration::from_secs(5);

// ------------------------------------------------------------ fake ircd

/// A minimal IRC server that registers the bouncer, echoes JOINs and lets the
/// test inject arbitrary lines. Every line received is reported to the test.
pub struct FakeIrcd {
    pub addr: SocketAddr,
    /// Every line consumed by `expect` so far.
    pub history: Vec<String>,
    pub received: mpsc::UnboundedReceiver<String>,
    pub inject: mpsc::UnboundedSender<String>,
}

impl FakeIrcd {
    pub async fn start() -> FakeIrcd {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (rec_tx, rec_rx) = mpsc::unbounded_channel();
        let (inj_tx, inj_rx) = mpsc::unbounded_channel::<String>();
        let inj_rx = Arc::new(Mutex::new(inj_rx));
        tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else {
                    return;
                };
                serve_fake(sock, rec_tx.clone(), inj_rx.clone()).await;
            }
        });
        FakeIrcd {
            addr,
            history: Vec::new(),
            received: rec_rx,
            inject: inj_tx,
        }
    }

    pub async fn expect(&mut self, needle: &str) -> String {
        let mut seen = Vec::new();
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            match tokio::time::timeout_at(deadline, self.received.recv()).await {
                Ok(Some(l)) if l.contains(needle) => {
                    self.history.push(l.clone());
                    return l;
                }
                Ok(Some(l)) => {
                    self.history.push(l.clone());
                    seen.push(l)
                }
                _ => panic!("ircd never received {needle:?}; got {seen:#?}"),
            }
        }
    }

    pub fn send(&self, line: &str) {
        self.inject.send(line.to_string()).unwrap();
    }
}

async fn serve_fake(
    sock: TcpStream,
    rec: mpsc::UnboundedSender<String>,
    inject: Arc<Mutex<mpsc::UnboundedReceiver<String>>>,
) {
    let (r, mut w) = sock.into_split();
    let mut reader = BufReader::new(r);
    let mut buf = Vec::new();
    let mut inject = inject.lock().await;
    let mut nick = String::new();
    let mut user = false;
    let mut cap_open = false;
    let mut registered = false;
    loop {
        tokio::select! {
            n = reader.read_until(b'\n', &mut buf) => {
                if !matches!(n, Ok(n) if n > 0) { return; }
                // Lossy, so tests can see what non-UTF-8 bytes were sent.
                let l = String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string();
                buf.clear();
                let _ = rec.send(l.clone());
                let parts: Vec<&str> = l.splitn(3, ' ').collect();
                let mut out: Vec<String> = Vec::new();
                match parts[0] {
                    "CAP" if parts.get(1) == Some(&"LS") => {
                        cap_open = true;
                        out.push(":fake CAP * LS :server-time message-tags".into());
                    }
                    "CAP" if parts.get(1) == Some(&"REQ") => {
                        let caps = l.split_once(':').map(|x| x.1).unwrap_or("");
                        out.push(format!(":fake CAP * ACK :{caps}"));
                    }
                    "CAP" if parts.get(1) == Some(&"END") => cap_open = false,
                    "NICK" => nick = parts[1].trim_start_matches(':').to_string(),
                    "USER" => user = true,
                    "PING" => out.push(format!(":fake PONG fake {}", parts.get(1).unwrap_or(&""))),
                    "JOIN" => {
                        for c in parts[1].split(',') {
                            out.push(format!(":{nick}!u@fake JOIN {c}"));
                            out.push(format!(":fake 332 {nick} {c} :Test topic"));
                            out.push(format!(":fake 353 {nick} = {c} :@{nick} +bob carol"));
                            out.push(format!(":fake 366 {nick} {c} :End of /NAMES list."));
                        }
                    }
                    "PRIVMSG" | "NOTICE" | "QUIT" | "PART" => {}
                    _ => {}
                }
                if !registered && !nick.is_empty() && user && !cap_open {
                    registered = true;
                    out.push(format!(":fake 001 {nick} :Welcome to FakeNet {nick}"));
                    out.push(format!(":fake 005 {nick} CASEMAPPING=rfc1459 CHANTYPES=# PREFIX=(ov)@+ NETWORK=FakeNet :are supported"));
                    out.push(format!(":fake 422 {nick} :MOTD File is missing"));
                }
                for o in out {
                    if w.write_all(format!("{o}\r\n").as_bytes()).await.is_err() { return; }
                }
            }
            i = inject.recv() => {
                let Some(i) = i else { return };
                if i == "__CLOSE__" { return; }
                // `__LATIN1__text` is sent in Latin-1 instead of UTF-8.
                let bytes: Vec<u8> = match i.strip_prefix("__LATIN1__") {
                    Some(t) => t.chars().map(|c| c as u32 as u8).chain(*b"\r\n").collect(),
                    None => format!("{i}\r\n").into_bytes(),
                };
                if w.write_all(&bytes).await.is_err() { return; }
            }
        }
    }
}

// ----------------------------------------------------------- IRC client

pub struct Client {
    pub lines: tokio::io::Lines<BufReader<OwnedReadHalf>>,
    pub w: OwnedWriteHalf,
}

impl Client {
    pub async fn connect(addr: SocketAddr) -> Client {
        let s = TcpStream::connect(addr).await.unwrap();
        let (r, w) = s.into_split();
        Client {
            lines: BufReader::new(r).lines(),
            w,
        }
    }

    /// Write errors are ignored: the server may legitimately close the
    /// connection first (e.g. a blocked address); assertions read the reply.
    pub async fn send(&mut self, line: &str) {
        let _ = self.w.write_all(format!("{line}\r\n").as_bytes()).await;
    }

    /// Reads until a line containing `needle` arrives; returns every line read.
    pub async fn read_until(&mut self, needle: &str) -> Vec<String> {
        let mut seen = Vec::new();
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            match tokio::time::timeout_at(deadline, self.lines.next_line()).await {
                Ok(Ok(Some(l))) => {
                    let done = l.contains(needle);
                    seen.push(l);
                    if done {
                        return seen;
                    }
                }
                _ => panic!("client never received {needle:?}; got {seen:#?}"),
            }
        }
    }

    /// Collects lines for a short period.
    pub async fn drain(&mut self, d: Duration) -> Vec<String> {
        let mut seen = Vec::new();
        let deadline = tokio::time::Instant::now() + d;
        while let Ok(Ok(Some(l))) = tokio::time::timeout_at(deadline, self.lines.next_line()).await
        {
            seen.push(l);
        }
        seen
    }

    /// Login negotiating the given capabilities first.
    pub async fn login_caps(addr: SocketAddr, login: &str, caps: &str) -> (Client, Vec<String>) {
        let mut c = Client::connect(addr).await;
        c.send("CAP LS 302").await;
        c.send(&format!("CAP REQ :{caps}")).await;
        c.send(&format!("PASS {login}:{PASSWORD}")).await;
        c.send("NICK alice").await;
        c.send("USER alice 0 * :Alice").await;
        let ack = c.read_until(" ACK ").await;
        assert!(contains(&ack, caps), "{ack:#?}");
        c.send("CAP END").await;
        let lines = c.read_until(" 366 ").await;
        (c, lines)
    }

    /// PASS/NICK/USER login; returns the lines up to the end of the channel burst.
    pub async fn login(addr: SocketAddr, login: &str) -> (Client, Vec<String>) {
        let mut c = Client::connect(addr).await;
        c.send(&format!("PASS {login}:{PASSWORD}")).await;
        c.send("NICK alice").await;
        c.send("USER alice 0 * :Alice").await;
        let lines = c.read_until(" 366 ").await;
        (c, lines)
    }

    /// Like `login`, but returns at the end of the welcome, for when no
    /// channel is joined yet.
    pub async fn login_no_channels(addr: SocketAddr, login: &str) -> (Client, Vec<String>) {
        let mut c = Client::connect(addr).await;
        c.send(&format!("PASS {login}:{PASSWORD}")).await;
        c.send("NICK alice").await;
        c.send("USER alice 0 * :Alice").await;
        let lines = c.read_until(" 422 ").await;
        (c, lines)
    }
}

// --------------------------------------------------------------- harness

pub struct Harness {
    /// The bouncer's database (SQLite, or PostgreSQL with XBNC_TEST_POSTGRES).
    pub db: Db,
    pub _dir: tempfile::TempDir,
    pub running: Option<xbncd::Running>,
    pub addr: SocketAddr,
    pub ircd: FakeIrcd,
}

impl Harness {
    pub async fn start() -> Harness {
        Harness::start_with(None).await
    }

    /// `on_connect`: commands the network sends after registering.
    pub async fn start_with(on_connect: Option<&str>) -> Harness {
        Harness::start_opts(on_connect, &[]).await
    }

    /// `modules`: (module id, enabled, settings JSON) stored before start.
    pub async fn start_opts(
        on_connect: Option<&str>,
        modules: &[(&str, bool, serde_json::Value)],
    ) -> Harness {
        Harness::start_full(on_connect, modules, &[]).await
    }

    /// Also installs WebAssembly plugins, enabled on the network with the
    /// given settings.
    pub async fn start_full(
        on_connect: Option<&str>,
        modules: &[(&str, bool, serde_json::Value)],
        plugins: &[(&[u8], serde_json::Value)],
    ) -> Harness {
        Harness::start_all(on_connect, modules, plugins, "", "").await
    }

    /// `web_extra` goes in the `[web]` section, `extra` after it.
    pub async fn start_all(
        on_connect: Option<&str>,
        modules: &[(&str, bool, serde_json::Value)],
        plugins: &[(&[u8], serde_json::Value)],
        web_extra: &str,
        extra: &str,
    ) -> Harness {
        let mut ircd = FakeIrcd::start().await;
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("xbnc.toml");
        std::fs::write(
            &cfg_path,
            format!(
                r#"
                secret_key_file = "{d}/secret.key"
                [database]
                path = "{d}/xbnc.db"
                {pg}
                [[listen]]
                address = "127.0.0.1:0"
                [limits]
                login_max_failures = 3
                [web]
                address = "127.0.0.1:0"
                {web_extra}
                {extra}
                "#,
                d = dir.path().display(),
                pg = pg_test_database()
                    .map(|u| format!("url = \"{u}\""))
                    .unwrap_or_default(),
            ),
        )
        .unwrap();
        let cfg = xbncd::Config::load(&cfg_path).unwrap();
        let db = Db::open_config(&cfg).unwrap();
        let hash = xbncd::crypto::hash_password(PASSWORD).unwrap();
        let on_connect_enc = on_connect.map(|t| {
            xbncd::crypto::SecretBox::load(&cfg.secret_key_file, true)
                .unwrap()
                .encrypt(t)
        });
        db.with(|c| {
            let uid = db::create_user(c, "alice", &hash, true)?;
            let nid = db::create_network(
                c,
                uid,
                &NewNetwork {
                    name: "fake".into(),
                    host: "127.0.0.1".into(),
                    port: ircd.addr.port(),
                    tls: false,
                    nick: "alice".into(),
                    username: "alice".into(),
                    realname: "Alice".into(),
                    server_password_enc: None,
                    sasl_username: None,
                    sasl_password_enc: None,
                },
                10,
            )?;
            if let Some(enc) = &on_connect_enc {
                db::set_on_connect(c, nid, Some(enc))?;
            }
            for (id, enabled, cfg) in modules {
                let config = xbncd::modules::validate(id, cfg.as_object().unwrap())?;
                db::set_module(
                    c,
                    nid,
                    id,
                    &xbncd::modules::ModState {
                        enabled: *enabled,
                        config,
                    },
                )?;
            }
            for (wasm, cfg) in plugins {
                let p = xbncd::plugins::Plugin::compile(&xbncd::plugins::engine(), wasm)?;
                xbncd::plugins::store_file(&dir.path().join("plugins"), &p.manifest.id, wasm)?;
                db::upsert_plugin(c, &p.manifest, &p.sha256, wasm.len() as i64, "test")?;
                let config = xbncd::plugins::validate_config(
                    &p.manifest.settings,
                    cfg.as_object().unwrap(),
                )?;
                db::set_plugin_config(c, nid, &p.manifest.id, true, &config)?;
            }
            // An install that already went through the 0.8.0 language fix:
            // a language set here is the user's choice.
            db::set_setting(c, xbncd::catalog::LANG_DEFAULTS_FIXED, "1")?;
            db::upsert_channel(c, nid, "#test", "#test", None, false)
        })
        .unwrap();
        let running = xbncd::start(cfg, true).await.unwrap();
        let addr = running.local_addrs[0];
        // The bouncer registers and joins the saved channel on its own.
        ircd.expect("NICK alice").await;
        if let Some(text) = on_connect {
            // Commands on connect go out before the channels are joined.
            if text.contains("+x") {
                ircd.expect("MODE alice +x").await;
            }
            if text.contains("login alice s3cret") {
                ircd.expect("PRIVMSG x@channels.undernet.org :login alice s3cret")
                    .await;
                // Like Undernet: logged in to X with +x, the host is hidden,
                // and only then may the channels be joined.
                if text.contains("+x") {
                    ircd.send(":fake 396 alice alice.users.undernet.org :is now your hidden host");
                }
            }
        }
        // With the services module the joins wait for the login; those
        // tests check the order themselves.
        if !modules.iter().any(|(id, on, _)| *id == "services" && *on) {
            ircd.expect("JOIN #test").await;
        }
        // Let the JOIN echo and NAMES reach the bouncer.
        tokio::time::sleep(Duration::from_millis(200)).await;
        Harness {
            db,
            _dir: dir,
            running: Some(running),
            addr,
            ircd,
        }
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        if let Some(r) = self.running.take() {
            // Best effort: actors stop when the runtime shuts down.
            drop(r);
        }
    }
}

pub fn contains(lines: &[String], needle: &str) -> bool {
    lines.iter().any(|l| l.contains(needle))
}

/// With XBNC_TEST_POSTGRES=postgres://user@host:port, each test gets a new,
/// empty PostgreSQL database; the URL to use is returned.
pub fn pg_test_database() -> Option<String> {
    let base = std::env::var("XBNC_TEST_POSTGRES").ok()?;
    let name = format!(
        "xbnc_t_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let base2 = base.clone();
    let n2 = name.clone();
    std::thread::spawn(move || {
        let mut c =
            postgres::Client::connect(&format!("{base2}/postgres"), postgres::NoTls).unwrap();
        c.batch_execute(&format!("CREATE DATABASE {n2}")).unwrap();
    })
    .join()
    .unwrap();
    Some(format!("{base}/{name}"))
}
