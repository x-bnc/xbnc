//! Tests against a real IRC server (not the scripted one). Ignored unless
//! asked for:
//!
//!   XBNC_IRCD=127.0.0.1:6667 cargo test -p xbncd --test real_ircd -- --ignored
//!
//! CI runs them against Ergo and InspIRCd. Nicks and channels get a random
//! suffix, so a shared test server works too.

mod common;
use common::{Client, PASSWORD};
use std::net::SocketAddr;
use std::time::Duration;
use xbncd::db::{self, Db, NewNetwork};

fn ircd() -> Option<(String, u16)> {
    let v = std::env::var("XBNC_IRCD").ok()?;
    let (h, p) = v.rsplit_once(':')?;
    Some((h.to_string(), p.parse().ok()?))
}

fn suffix() -> String {
    format!("{:04}", db::now_ms() % 10_000)
}

/// A plain IRC client talking to the real server directly.
async fn direct(host: &str, port: u16, nick: &str) -> Client {
    let addr: SocketAddr = tokio::net::lookup_host((host, port))
        .await
        .unwrap()
        .next()
        .unwrap();
    let mut c = Client::connect(addr).await;
    c.send(&format!("NICK {nick}")).await;
    c.send(&format!("USER {nick} 0 * :{nick}")).await;
    // Answer the server's PING during registration (some servers send one).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let l = tokio::time::timeout_at(deadline, c.lines.next_line())
            .await
            .expect("registration timed out")
            .unwrap()
            .unwrap();
        if let Some(rest) = l.strip_prefix("PING ") {
            c.send(&format!("PONG {rest}")).await;
        }
        if l.contains(" 001 ") {
            break;
        }
    }
    c
}

async fn start_bouncer(
    host: &str,
    port: u16,
    nick: &str,
    chan: &str,
) -> (tempfile::TempDir, xbncd::Running) {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("xbnc.toml");
    std::fs::write(
        &p,
        format!(
            "secret_key_file = \"{d}/secret.key\"\n[database]\npath = \"{d}/xbnc.db\"\n[[listen]]\naddress = \"127.0.0.1:0\"\n",
            d = dir.path().display()
        ),
    )
    .unwrap();
    let cfg = xbncd::Config::load(&p).unwrap();
    let db = Db::open(&cfg.database.path).unwrap();
    let hash = xbncd::crypto::hash_password(PASSWORD).unwrap();
    db.with(|c| {
        let uid = db::create_user(c, "alice", &hash, true)?;
        let nid = db::create_network(
            c,
            uid,
            &NewNetwork {
                name: "real".into(),
                host: host.into(),
                port,
                tls: false,
                nick: nick.into(),
                username: "alice".into(),
                realname: "xBNC test".into(),
                server_password_enc: None,
                sasl_username: None,
                sasl_password_enc: None,
            },
            10,
        )?;
        db::upsert_channel(c, nid, chan, &chan.to_lowercase(), None, false)
    })
    .unwrap();
    drop(db);
    let running = xbncd::start(cfg, true).await.unwrap();
    (dir, running)
}

async fn bouncer_client(addr: SocketAddr, device: &str, nick: &str, chan: &str) -> Client {
    let mut c = Client::connect(addr).await;
    c.send(&format!("PASS alice/real@{device}:{PASSWORD}"))
        .await;
    c.send(&format!("NICK {nick}")).await;
    c.send("USER alice 0 * :Alice").await;
    c.read_until(&format!("366 {nick} {chan}")).await;
    c
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs a real IRC server: set XBNC_IRCD=host:port"]
async fn bouncer_against_a_real_server() {
    let Some((host, port)) = ircd() else {
        eprintln!("XBNC_IRCD not set; skipping");
        return;
    };
    let s = suffix();
    let (alice, bob_nick, chan) = (format!("xa{s}"), format!("xb{s}"), format!("#xbnc{s}"));

    let (_dir, running) = start_bouncer(&host, port, &alice, &chan).await;
    let addr = running.local_addrs[0];
    // Give the bouncer time to register and join.
    tokio::time::sleep(Duration::from_secs(3)).await;

    let mut bob = direct(&host, port, &bob_nick).await;
    bob.send(&format!("JOIN {chan}")).await;
    bob.read_until(&format!("366 {bob_nick} {chan}")).await;

    // The device sees the channel, with bob in it.
    let mut laptop = bouncer_client(addr, "laptop", &alice, &chan).await;
    laptop.send(&format!("NAMES {chan}")).await;
    let names = laptop.read_until(&format!("366 {alice} {chan}")).await;
    assert!(
        names
            .iter()
            .any(|l| l.contains(" 353 ") && l.contains(&bob_nick)),
        "{names:#?}"
    );

    // Messages both ways.
    bob.send(&format!("PRIVMSG {chan} :hola desde la red"))
        .await;
    laptop.read_until("hola desde la red").await;
    laptop
        .send(&format!("PRIVMSG {chan} :hola desde xBNC"))
        .await;
    bob.read_until("hola desde xBNC").await;

    // A second device of the same user sees what the first one sent.
    let mut phone = bouncer_client(addr, "phone", &alice, &chan).await;
    laptop.send(&format!("PRIVMSG {chan} :sincronizado")).await;
    phone.read_until("sincronizado").await;
    drop(phone);

    // Offline: what arrives while the laptop is away is played back once.
    drop(laptop);
    tokio::time::sleep(Duration::from_millis(500)).await;
    bob.send(&format!("PRIVMSG {chan} :mientras no estabas 1"))
        .await;
    bob.send(&format!("PRIVMSG {alice} :privado mientras no estabas"))
        .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let mut laptop = Client::connect(addr).await;
    laptop
        .send(&format!("PASS alice/real@laptop:{PASSWORD}"))
        .await;
    laptop.send(&format!("NICK {alice}")).await;
    laptop.send("USER alice 0 * :Alice").await;
    laptop.read_until("mientras no estabas 1").await;
    laptop.read_until("privado mientras no estabas").await;
    let again = laptop.drain(Duration::from_secs(1)).await;
    assert!(
        !again.iter().any(|l| l.contains("mientras no estabas 1")),
        "replayed twice"
    );

    // Nick changes and parts from the network reach the device.
    bob.send(&format!("NICK {bob_nick}x")).await;
    let l = laptop.read_until(&format!("{bob_nick}x")).await;
    assert!(l.last().unwrap().contains(" NICK "), "{l:#?}");
    bob.send(&format!("PART {chan} :adios")).await;
    laptop.read_until("PART").await;

    // The bouncer stays on the network when every device leaves.
    drop(laptop);
    tokio::time::sleep(Duration::from_secs(1)).await;
    bob.send(&format!("WHOIS {alice}")).await;
    let whois = bob.read_until(" 318 ").await;
    assert!(
        whois.iter().any(|l| l.contains(" 311 ")),
        "alice left the network: {whois:#?}"
    );
}
