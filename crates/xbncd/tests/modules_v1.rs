//! End-to-end tests of the v1.0 modules and IRCv3 support against the
//! scripted IRC server.

mod common;
use common::*;
use serde_json::json;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ircv3_extras_reach_only_clients_that_asked() {
    let h = Harness::start().await;
    let (mut plain, _) = Client::login(h.addr, "alice/fake@laptop").await;
    let (mut modern, burst) = Client::login_caps(
        h.addr,
        "alice/fake@phone",
        "extended-join away-notify multi-prefix",
    )
    .await;
    // The channel burst uses the extended forms for the modern client.
    assert!(contains(&burst, "JOIN #test * :Alice"), "{burst:#?}");
    h.ircd.send(":dave!d@fake JOIN #test daveacct :Dave Real");
    h.ircd.send(":dave!d@fake AWAY :gone fishing");
    h.ircd.send(":fake 353 alice = #test :@+bob dave");
    h.ircd.send(":fake 366 alice #test :End of /NAMES list.");
    let m = modern.read_until(" 366 ").await;
    assert!(contains(&m, "JOIN #test daveacct :Dave Real"), "{m:#?}");
    assert!(contains(&m, "AWAY :gone fishing"), "{m:#?}");
    assert!(contains(&m, ":@+bob dave"), "{m:#?}");
    let p = plain.read_until(" 366 ").await;
    assert!(contains(&p, ":dave!d@fake JOIN #test"), "{p:#?}");
    assert!(!contains(&p, "daveacct"), "{p:#?}");
    assert!(!contains(&p, "AWAY"), "{p:#?}");
    assert!(contains(&p, ":@bob dave"), "{p:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn services_log_in_to_x_and_wait_for_the_hidden_host() {
    let mut h = Harness::start_opts(
        None,
        &[(
            "services",
            true,
            json!({"service": "x", "account": "alicex", "password": "pw secret",
                   "login_on_connect": false, "wait": 30}),
        )],
    )
    .await;
    h.ircd
        .expect("PRIVMSG x@channels.undernet.org :LOGIN alicex pw secret")
        .await;
    h.ircd.expect("MODE alice +x").await;
    // No JOIN until the host is hidden.
    let early = tokio::time::timeout(Duration::from_millis(600), async {
        loop {
            if let Some(l) = h.ircd.received.recv().await {
                if l.starts_with("JOIN") {
                    return l;
                }
            }
        }
    })
    .await;
    assert!(early.is_err(), "joined before the login: {early:?}");
    h.ircd
        .send(":fake 396 alice alicex.users.undernet.org :is now your hidden host");
    h.ircd.expect("JOIN #test").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn services_login_on_connect_uses_the_server_password() {
    let mut h = Harness::start_opts(
        None,
        &[(
            "services",
            true,
            json!({"service": "x", "account": "alicex", "password": "pw", "wait": 1}),
        )],
    )
    .await;
    // Login on Connect goes in PASS, before NICK.
    let pass = h
        .ircd
        .history
        .iter()
        .position(|l| l == "PASS :+x! alicex pw");
    let nick = h.ircd.history.iter().position(|l| l == "NICK alice");
    assert!(pass.is_some() && pass < nick, "{:#?}", h.ircd.history);
    // Without a 396 the channels are not joined, even after the wait:
    // they would show the real address. They are joined once it comes.
    let early = tokio::time::timeout(Duration::from_millis(1500), async {
        loop {
            if let Some(l) = h.ircd.received.recv().await {
                if l.starts_with("JOIN") {
                    return l;
                }
            }
        }
    })
    .await;
    assert!(early.is_err(), "joined with the real host: {early:?}");
    h.ircd
        .send(":fake 396 alice alicex.users.undernet.org :is now your hidden host");
    h.ircd.expect("JOIN #test").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invite_module_asks_the_channel_service() {
    let mut h = Harness::start_opts(None, &[("invite", true, json!({"service": "x"}))]).await;
    h.ircd
        .send(":fake 473 alice #secret :Cannot join channel (+i)");
    h.ircd.expect("PRIVMSG X :INVITE #secret").await;
    h.ircd
        .send(":fake 474 alice #banned :Cannot join channel (+b)");
    h.ircd.expect("PRIVMSG X :UNBAN #banned").await;
    h.ircd.expect("JOIN #banned").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ignore_drops_messages_and_strip_removes_colours() {
    let h = Harness::start_opts(
        None,
        &[
            ("ignore", true, json!({"masks": "troll"})),
            ("stripcontrols", true, json!({})),
        ],
    )
    .await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    h.ircd.send(":troll!t@fake PRIVMSG #test :spam spam");
    h.ircd
        .send(":bob!b@fake PRIVMSG #test :\x0304rojo\x03 y \x02negrita\x02");
    let lines = c.read_until("negrita").await;
    assert!(!contains(&lines, "spam"), "{lines:#?}");
    assert!(contains(&lines, ":rojo y negrita"), "{lines:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crypt_encrypts_and_decrypts_channel_messages() {
    let mut h = Harness::start_opts(
        None,
        &[("crypt", true, json!({"keys": "#test clavesecreta"}))],
    )
    .await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    c.send("PRIVMSG #test :hola mundo").await;
    let l = h.ircd.expect("PRIVMSG #test").await;
    let body = l.split_once(" :").unwrap().1;
    assert!(body.starts_with("+OK "), "{l}");
    assert_eq!(
        xbncd::fish::decrypt("clavesecreta", &body[4..]).unwrap(),
        "hola mundo"
    );
    let enc = xbncd::fish::encrypt("clavesecreta", false, "respuesta cifrada").unwrap();
    h.ircd.send(&format!(":bob!b@fake PRIVMSG #test :{enc}"));
    c.read_until(":respuesta cifrada").await;
    // +p sends in plain text.
    c.send("PRIVMSG #test :+p sin cifrar").await;
    h.ircd.expect("PRIVMSG #test :sin cifrar").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crypt_key_exchange_answers_dh1080() {
    let mut h = Harness::start_opts(None, &[("crypt", true, json!({}))]).await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    let bob = xbncd::fish::Dh1080::new();
    h.ircd.send(&format!(
        ":bob!b@fake NOTICE alice :DH1080_INIT {} CBC",
        bob.public
    ));
    let l = h.ircd.expect("NOTICE bob :DH1080_FINISH").await;
    let theirs = l.split_whitespace().nth(3).unwrap();
    let key = bob.shared_key(theirs).unwrap();
    c.read_until("Key agreed with bob").await;
    c.send("PRIVMSG bob :secreto").await;
    let l = h.ircd.expect("PRIVMSG bob").await;
    let body = l.split_once(" :").unwrap().1;
    assert!(body.starts_with("+OK *"), "CBC: {l}");
    assert_eq!(xbncd::fish::decrypt(&key, &body[4..]).unwrap(), "secreto");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn detached_channels_stay_hidden_until_mentioned() {
    let h = Harness::start_opts(None, &[("detach", true, json!({"channels": "#test"}))]).await;
    let mut c = Client::connect(h.addr).await;
    c.send(&format!("PASS alice/fake@laptop:{PASSWORD}")).await;
    c.send("NICK alice").await;
    c.send("USER alice 0 * :Alice").await;
    let burst = c.read_until(" 422 ").await;
    let burst: Vec<String> = burst
        .into_iter()
        .chain(c.drain(Duration::from_millis(300)).await)
        .collect();
    assert!(!contains(&burst, "JOIN #test"), "{burst:#?}");
    h.ircd.send(":bob!b@fake PRIVMSG #test :nadie me ve");
    let quiet = c.drain(Duration::from_millis(400)).await;
    assert!(!contains(&quiet, "nadie me ve"), "{quiet:#?}");
    h.ircd.send(":bob!b@fake PRIVMSG #test :alice: mira esto");
    let back = c.read_until("mira esto").await;
    assert!(contains(&back, "JOIN #test"), "{back:#?}");
    assert!(
        contains(&back, "nadie me ve"),
        "context is replayed: {back:#?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn flooding_channels_are_hidden() {
    let h = Harness::start_opts(
        None,
        &[("flooddetach", true, json!({"messages": 5, "seconds": 10}))],
    )
    .await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    for i in 0..12 {
        h.ircd
            .send(&format!(":bob!b@fake PRIVMSG #test :flood {i}"));
    }
    let lines = c.read_until("is flooding").await;
    let more = c.drain(Duration::from_millis(400)).await;
    assert!(!contains(&more, "flood 11"), "{lines:#?} {more:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn autocycle_rejoins_to_get_op() {
    let mut h = Harness::start_opts(None, &[("autocycle", true, json!({}))]).await;
    h.ircd.expect("MODE #test").await;
    h.ircd.send(":fake 324 alice #test +nt");
    // The fake NAMES gave us op; take it and let everybody leave.
    h.ircd.send(":fake MODE #test -o alice");
    h.ircd.send(":bob!b@fake PART #test");
    h.ircd.send(":carol!c@fake QUIT :bye");
    h.ircd.expect("PART #test").await;
    h.ircd.send(":alice!u@fake PART #test :cycle");
    h.ircd.expect("JOIN #test").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn latin1_lines_are_decoded_and_sent_back_as_latin1() {
    let mut h =
        Harness::start_opts(None, &[("charset", true, json!({"send_legacy": "#test"}))]).await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    h.ircd
        .send("__LATIN1__:bob!b@fake PRIVMSG #test :el niño come piñones");
    c.read_until(":el niño come piñones").await;
    c.send("PRIVMSG #test :año").await;
    let l = h.ircd.expect("PRIVMSG #test").await;
    // Sent in Latin-1: the fake reads it lossily as UTF-8.
    assert!(l.contains('\u{FFFD}') && !l.contains("año"), "{l:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scheduled_tasks_run_at_their_time() {
    // Pick the current minute (UTC: the test user has no time zone).
    let mut now = time::OffsetDateTime::now_utc();
    if now.second() > 50 {
        tokio::time::sleep(Duration::from_secs(11)).await;
        now = time::OffsetDateTime::now_utc();
    }
    let task = format!(
        "{:02}:{:02} /msg #test hora programada",
        now.hour(),
        now.minute()
    );
    let mut h = Harness::start_opts(None, &[("schedule", true, json!({ "tasks": task }))]).await;
    h.ircd.expect("PRIVMSG #test :hora programada").await;
}
