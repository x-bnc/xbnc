//! Load test: one xBNC process, many users and clients, a synthetic IRC
//! server sending channel traffic. Measures delivery latency (IRC server →
//! xBNC → client), throughput, memory and CPU, and how long all networks
//! take to come back after the server drops every connection.
//!
//!   cargo run --release --example loadtest -- --users 500 --rate 2 --seconds 60
//!
//! Everything runs in this process, so memory and CPU include the load
//! generator and the clients (they are small next to xBNC).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use xbncd::db::{self, Db, NewNetwork};

const PASSWORD: &str = "load-test-password-1";

struct Args {
    users: usize,
    clients: usize,
    rate: f64,
    seconds: u64,
}

fn args() -> Args {
    let mut a = Args {
        users: 200,
        clients: 1,
        rate: 1.0,
        seconds: 30,
    };
    let v: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i + 1 < v.len() {
        match v[i].as_str() {
            "--users" => a.users = v[i + 1].parse().unwrap(),
            "--clients" => a.clients = v[i + 1].parse().unwrap(),
            "--rate" => a.rate = v[i + 1].parse().unwrap(),
            "--seconds" => a.seconds = v[i + 1].parse().unwrap(),
            _ => {}
        }
        i += 2;
    }
    a
}

fn now_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

fn rss_mb() -> f64 {
    xbncd::metrics::rss_bytes().unwrap_or(0) as f64 / 1_048_576.0
}

fn cpu_seconds() -> f64 {
    // utime + stime from /proc/self/stat (clock ticks, 100 per second).
    let s = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let after = s.rsplit_once(')').map(|x| x.1).unwrap_or("");
    let f: Vec<&str> = after.split_whitespace().collect();
    let t = |i: usize| f.get(i).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
    (t(11) + t(12)) / 100.0
}

// ------------------------------------------------------------ fake IRC server

/// A minimal IRC server: registers anyone, echoes JOINs, answers PING, and
/// broadcasts the generator's lines to every connection in #xbnc.
async fn ircd(
    listener: TcpListener,
    feed: broadcast::Sender<String>,
    drop_all: Arc<AtomicU64>,
    connected: Arc<AtomicU64>,
) {
    loop {
        let Ok((s, _)) = listener.accept().await else {
            continue;
        };
        let mut rx = feed.subscribe();
        let drop_all = drop_all.clone();
        let connected = connected.clone();
        tokio::spawn(async move {
            let generation = drop_all.load(Ordering::SeqCst);
            let (r, mut w) = s.into_split();
            let mut lines = BufReader::new(r).lines();
            let mut nick = String::from("*");
            let mut in_chan = false;
            let mut tick = tokio::time::interval(Duration::from_millis(200));
            let mut counted = false;
            loop {
                tokio::select! {
                    l = lines.next_line() => {
                        let Ok(Some(l)) = l else { break };
                        let mut parts = l.splitn(3, ' ');
                        let cmd = parts.next().unwrap_or("").to_ascii_uppercase();
                        let arg = parts.next().unwrap_or("").to_string();
                        let out = match cmd.as_str() {
                            "NICK" => { nick = arg.clone(); String::new() }
                            "USER" => {
                                if !counted { connected.fetch_add(1, Ordering::SeqCst); counted = true; }
                                format!(":load.test 001 {nick} :Welcome\r\n:load.test 005 {nick} CHANTYPES=# CASEMAPPING=rfc1459 :are supported\r\n:load.test 376 {nick} :End of MOTD\r\n")
                            }
                            "JOIN" => {
                                in_chan = true;
                                format!(":{nick}!u@h JOIN {arg}\r\n:load.test 353 {nick} = {arg} :{nick} bot\r\n:load.test 366 {nick} {arg} :End\r\n")
                            }
                            "PING" => format!(":load.test PONG load.test {arg}\r\n"),
                            _ => String::new(),
                        };
                        if !out.is_empty() && w.write_all(out.as_bytes()).await.is_err() { break; }
                    }
                    m = rx.recv() => {
                        match m {
                            Ok(line) if in_chan => { if w.write_all(line.as_bytes()).await.is_err() { break; } }
                            Ok(_) => {}
                            Err(broadcast::error::RecvError::Lagged(_)) => {}
                            Err(_) => break,
                        }
                    }
                    _ = tick.tick() => {
                        if drop_all.load(Ordering::SeqCst) != generation { break; }
                    }
                }
            }
            if counted {
                connected.fetch_sub(1, Ordering::SeqCst);
            }
        });
    }
}

// ------------------------------------------------------------ clients

#[derive(Default)]
struct Stats {
    latencies_us: Mutex<Vec<u64>>,
    received: AtomicU64,
}

async fn client(
    addr: std::net::SocketAddr,
    user: String,
    device: usize,
    stats: Arc<Stats>,
    ready: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
) {
    let s = loop {
        match TcpStream::connect(addr).await {
            Ok(s) => break s,
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    };
    let (r, mut w) = s.into_split();
    let login =
        format!("PASS {user}/load@d{device}:{PASSWORD}\r\nNICK {user}\r\nUSER {user} 0 * :x\r\n");
    w.write_all(login.as_bytes()).await.unwrap();
    let mut lines = BufReader::new(r).lines();
    let mut joined = false;
    while let Ok(Some(l)) = lines.next_line().await {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if !joined && l.contains(" 366 ") {
            joined = true;
            ready.fetch_add(1, Ordering::SeqCst);
            continue;
        }
        if let Some(rest) = l.strip_prefix("PING ") {
            let _ = w.write_all(format!("PONG {rest}\r\n").as_bytes()).await;
            continue;
        }
        if let Some(i) = l.find(":load ") {
            let mut it = l[i + 6..].split(' ');
            let _seq = it.next();
            if let Some(ts) = it.next().and_then(|t| t.parse::<u64>().ok()) {
                let lat = now_us().saturating_sub(ts);
                stats.received.fetch_add(1, Ordering::Relaxed);
                stats.latencies_us.lock().unwrap().push(lat);
            }
        }
    }
}

fn pct(v: &mut [u64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_unstable();
    let i = ((v.len() as f64 - 1.0) * p).round() as usize;
    v[i] as f64 / 1000.0
}

fn main() {
    xbncd::util::tune_allocator();
    run();
}

#[tokio::main(flavor = "multi_thread")]
async fn run() {
    let a = args();
    let dir = tempfile::tempdir().unwrap();
    let feed = broadcast::channel::<String>(4096).0;
    let drop_all = Arc::new(AtomicU64::new(0));
    let connected = Arc::new(AtomicU64::new(0));
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ircd_port = l.local_addr().unwrap().port();
    tokio::spawn(ircd(l, feed.clone(), drop_all.clone(), connected.clone()));

    // xBNC with U users, one network each.
    let cfg_path = dir.path().join("xbnc.toml");
    std::fs::write(
        &cfg_path,
        format!(
            "secret_key_file = \"{d}/secret.key\"\n[database]\npath = \"{d}/xbnc.db\"\n[[listen]]\naddress = \"127.0.0.1:0\"\n\
             [limits]\nmax_connections = 20000\nmax_clients_per_user = 20\nlogin_max_failures = 1000\n[log]\nlevel = \"warn\"\n",
            d = dir.path().display()
        ),
    )
    .unwrap();
    let cfg = xbncd::Config::load(&cfg_path).unwrap();
    let db = Db::open(&cfg.database.path).unwrap();
    let hash = xbncd::crypto::hash_password(PASSWORD).unwrap();
    let t0 = Instant::now();
    db.with(|c| {
        for i in 0..a.users {
            let name = format!("u{i}");
            let uid = db::create_user(c, &name, &hash, false)?;
            let nid = db::create_network(
                c,
                uid,
                &NewNetwork {
                    name: "load".into(),
                    host: "127.0.0.1".into(),
                    port: ircd_port,
                    tls: false,
                    nick: name.clone(),
                    username: name.clone(),
                    realname: String::new(),
                    server_password_enc: None,
                    sasl_username: None,
                    sasl_password_enc: None,
                },
                10,
            )?;
            db::upsert_channel(c, nid, "#xbnc", "#xbnc", None, false)?;
        }
        Ok(())
    })
    .unwrap();
    drop(db);
    let setup_s = t0.elapsed().as_secs_f64();
    let rss0 = rss_mb();
    let running = xbncd::start(cfg, true).await.unwrap();
    let addr = running.local_addrs[0];

    // All networks registered?
    let t_start = Instant::now();
    while (connected.load(Ordering::SeqCst) as usize) < a.users {
        if t_start.elapsed() > Duration::from_secs(120) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let connect_s = t_start.elapsed().as_secs_f64();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let rss_idle = rss_mb();

    // Clients.
    let stats = Arc::new(Stats::default());
    let ready = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let t_clients = Instant::now();
    for i in 0..a.users {
        for d in 0..a.clients {
            tokio::spawn(client(
                addr,
                format!("u{i}"),
                d,
                stats.clone(),
                ready.clone(),
                stop.clone(),
            ));
        }
    }
    let want = (a.users * a.clients) as u64;
    while ready.load(Ordering::SeqCst) < want && t_clients.elapsed() < Duration::from_secs(300) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let clients_s = t_clients.elapsed().as_secs_f64();
    let rss_clients = rss_mb();
    stats.latencies_us.lock().unwrap().clear();
    stats.received.store(0, Ordering::SeqCst);

    // Traffic: `rate` messages per second to every network (one line on the
    // fake server reaches every connection in #xbnc).
    let cpu0 = cpu_seconds();
    let t_load = Instant::now();
    let period = Duration::from_secs_f64(1.0 / a.rate.max(0.01));
    let mut seq = 0u64;
    let mut ticker = tokio::time::interval(period);
    while t_load.elapsed() < Duration::from_secs(a.seconds) {
        ticker.tick().await;
        seq += 1;
        let _ = feed.send(format!(
            ":bot!b@h PRIVMSG #xbnc :load {seq} {}\r\n",
            now_us()
        ));
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    let elapsed = t_load.elapsed().as_secs_f64();
    let cpu = (cpu_seconds() - cpu0) / elapsed * 100.0;
    let rss_load = rss_mb();
    let mut lat = std::mem::take(&mut *stats.latencies_us.lock().unwrap());
    let delivered = lat.len() as u64;
    let expected = seq * want;
    let (p50, p95, p99, pmax) = (
        pct(&mut lat, 0.5),
        pct(&mut lat, 0.95),
        pct(&mut lat, 0.99),
        pct(&mut lat, 1.0),
    );

    // Reconnect storm: the server drops everyone.
    let t_storm = Instant::now();
    drop_all.fetch_add(1, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(500)).await;
    while (connected.load(Ordering::SeqCst) as usize) < a.users
        && t_storm.elapsed() < Duration::from_secs(300)
    {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let storm_s = t_storm.elapsed().as_secs_f64();
    let back = connected.load(Ordering::SeqCst);

    let db_mb = std::fs::metadata(dir.path().join("xbnc.db"))
        .map(|m| m.len())
        .unwrap_or(0) as f64
        / 1_048_576.0;
    stop.store(true, Ordering::SeqCst);

    let mut report = HashMap::new();
    report.insert("users", a.users.to_string());
    println!("== xBNC load test ==");
    println!(
        "users/networks: {}   clients: {}   rate: {}/s per network   duration: {}s",
        a.users, want, a.rate, a.seconds
    );
    println!("setup: users created in {setup_s:.1}s, all networks registered in {connect_s:.1}s, clients logged in in {clients_s:.1}s");
    println!("memory (process RSS): start {rss0:.0} MB, networks idle {rss_idle:.0} MB, with clients {rss_clients:.0} MB, under load {rss_load:.0} MB");
    println!(
        "traffic: {} lines sent per network, {delivered}/{expected} delivered to clients ({:.0} msg/s), CPU {cpu:.0}% of one core",
        seq,
        delivered as f64 / elapsed
    );
    println!("latency IRC server -> client: p50 {p50:.1} ms, p95 {p95:.1} ms, p99 {p99:.1} ms, max {pmax:.1} ms");
    println!(
        "reconnect storm: {back}/{} networks back in {storm_s:.1}s",
        a.users
    );
    println!("database after the run: {db_mb:.1} MB");
    let _ = report;
    running.shutdown().await;
}
