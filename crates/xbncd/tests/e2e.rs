//! End-to-end tests: a real xBNC instance in-process, a scripted IRC server
//! acting as the upstream network, and raw TCP IRC clients.

mod common;
use base64::Engine;
use common::*;
use serde_json::json;
use std::time::Duration;
use xbncd::db;

// ------------------------------------------------------------------ tests

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn attach_restores_channel_state() {
    let h = Harness::start().await;
    let (_c, lines) = Client::login(h.addr, "alice/fake@laptop").await;
    assert!(contains(&lines, ":xbnc 001 alice "), "{lines:#?}");
    assert!(
        contains(&lines, "NETWORK=FakeNet"),
        "upstream ISUPPORT is relayed: {lines:#?}"
    );
    assert!(contains(&lines, "alice!u@fake JOIN #test"), "{lines:#?}");
    assert!(
        contains(&lines, "332 alice #test :Test topic"),
        "{lines:#?}"
    );
    let names = lines.iter().find(|l| l.contains(" 353 ")).unwrap();
    assert!(
        names.contains("@alice") && names.contains("+bob") && names.contains("carol"),
        "{names}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn messages_received_while_detached_are_replayed_once() {
    let mut h = Harness::start().await;

    let (mut a, _) = Client::login(h.addr, "alice/fake@laptop").await;
    a.send("PRIVMSG #test :hello from laptop").await;
    h.ircd.expect("PRIVMSG #test :hello from laptop").await;
    a.send("QUIT :bye").await;
    drop(a);
    tokio::time::sleep(Duration::from_millis(300)).await;

    h.ircd
        .send(":bob!b@fake PRIVMSG #test :while you were away");
    h.ircd.send(":bob!b@fake PRIVMSG alice :private while away");
    tokio::time::sleep(Duration::from_millis(300)).await;

    let (mut a, mut lines) = Client::login(h.addr, "alice/fake@laptop").await;
    lines.extend(a.drain(Duration::from_millis(500)).await);
    let away = lines
        .iter()
        .find(|l| l.contains("while you were away"))
        .expect("channel message replayed");
    assert!(
        away.starts_with(":bob!b@fake PRIVMSG #test :["),
        "timestamp prefix for clients without server-time: {away}"
    );
    assert!(
        contains(&lines, ":bob!b@fake PRIVMSG alice :["),
        "private message replayed: {lines:#?}"
    );
    assert!(
        !contains(&lines, "hello from laptop"),
        "own message must not be replayed to the same device"
    );

    // A third login from the same device gets nothing new.
    a.send("QUIT").await;
    drop(a);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (mut a, mut lines) = Client::login(h.addr, "alice/fake@laptop").await;
    lines.extend(a.drain(Duration::from_millis(500)).await);
    assert!(
        !contains(&lines, "while you were away"),
        "no duplicates on reconnection: {lines:#?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_clients_stay_in_sync() {
    let mut h = Harness::start().await;
    let (mut laptop, _) = Client::login(h.addr, "alice/fake@laptop").await;
    let (mut phone, _) = Client::login(h.addr, "alice/fake@phone").await;

    laptop.send("PRIVMSG #test :sent from the laptop").await;
    h.ircd.expect("sent from the laptop").await;
    let got = phone.read_until("sent from the laptop").await;
    assert!(
        got.last()
            .unwrap()
            .starts_with(":alice!u@fake PRIVMSG #test"),
        "{got:#?}"
    );

    h.ircd.send(":carol!c@fake PRIVMSG #test :hi everyone");
    laptop.read_until("hi everyone").await;
    phone.read_until("hi everyone").await;

    // A device that never connected before gets recent history.
    let (mut tablet, mut lines) = Client::login(h.addr, "alice/fake@tablet").await;
    lines.extend(tablet.drain(Duration::from_millis(500)).await);
    assert!(
        contains(&lines, "sent from the laptop") && contains(&lines, "hi everyone"),
        "{lines:#?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chathistory_and_capabilities() {
    let h = Harness::start().await;
    h.ircd
        .send("@time=2024-05-01T10:00:00.000Z :bob!b@fake PRIVMSG #test :old message");
    h.ircd.send(":bob!b@fake PRIVMSG #test :newer message");
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut c = Client::connect(h.addr).await;
    c.send("CAP LS 302").await;
    let ls = c.read_until("CAP * LS").await;
    let ls = ls.last().unwrap();
    assert!(
        ls.contains("draft/chathistory") && ls.contains("sasl=PLAIN"),
        "{ls}"
    );
    assert!(
        ls.contains("multi-prefix")
            && ls.contains("extended-join")
            && !ls.contains("labeled-response"),
        "only implemented caps are advertised: {ls}"
    );
    c.send("CAP REQ :server-time message-tags batch draft/chathistory echo-message")
        .await;
    c.read_until("ACK").await;
    c.send(&format!("PASS alice/fake@hist:{PASSWORD}")).await;
    c.send("NICK alice").await;
    c.send("USER alice 0 * :Alice").await;
    c.send("CAP END").await;
    let burst = c.read_until(" 366 ").await;
    assert!(contains(&burst, "CHATHISTORY=500"), "{burst:#?}");
    let after = c.drain(Duration::from_millis(300)).await;
    assert!(
        !contains(&after, "old message"),
        "chathistory clients are not auto-replayed: {after:#?}"
    );

    c.send("CHATHISTORY LATEST #test * 10").await;
    let batch = c.read_until("BATCH -").await;
    let start = batch.iter().find(|l| l.contains("BATCH +")).unwrap();
    assert!(start.ends_with("chathistory #test"), "{start}");
    let old = batch.iter().find(|l| l.contains("old message")).unwrap();
    assert!(
        old.contains("time=2024-05-01T10:00:00.000Z"),
        "server-time from upstream is kept: {old}"
    );
    assert!(old.contains("msgid=x"), "{old}");
    let new_idx = batch
        .iter()
        .position(|l| l.contains("newer message"))
        .unwrap();
    let old_idx = batch
        .iter()
        .position(|l| l.contains("old message"))
        .unwrap();
    assert!(old_idx < new_idx, "oldest first");

    let msgid = old
        .split("msgid=")
        .nth(1)
        .unwrap()
        .split([';', ' '])
        .next()
        .unwrap()
        .to_string();
    c.send(&format!("CHATHISTORY AFTER #test msgid={msgid} 10"))
        .await;
    let after = c.read_until("BATCH -").await;
    assert!(
        contains(&after, "newer message") && !contains(&after, "old message"),
        "{after:#?}"
    );

    c.send("CHATHISTORY BEFORE #test msgid=x999999 10").await;
    let fail = c.read_until("FAIL").await;
    assert!(fail.last().unwrap().contains("INVALID_MSGREFID"));

    // echo-message: own messages come back with a msgid.
    c.send("PRIVMSG #test :echo me").await;
    let echo = c.read_until("echo me").await;
    assert!(echo.last().unwrap().contains("msgid=x"), "{echo:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sasl_plain_login() {
    let h = Harness::start().await;
    let mut c = Client::connect(h.addr).await;
    c.send("CAP LS 302").await;
    c.read_until("LS").await;
    c.send("CAP REQ :sasl").await;
    c.read_until("ACK").await;
    c.send("AUTHENTICATE PLAIN").await;
    c.read_until("AUTHENTICATE +").await;
    let payload =
        base64::engine::general_purpose::STANDARD.encode(format!("\0alice/fake@sasl\0{PASSWORD}"));
    c.send(&format!("AUTHENTICATE {payload}")).await;
    c.read_until(" 903 ").await;
    c.send("NICK alice").await;
    c.send("USER alice 0 * :Alice").await;
    c.send("CAP END").await;
    let lines = c.read_until(" 366 ").await;
    assert!(contains(&lines, " 001 alice "));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_password_is_rejected_and_ip_blocked() {
    let h = Harness::start().await;
    for _ in 0..3 {
        let mut c = Client::connect(h.addr).await;
        c.send("PASS alice/fake:wrong-password").await;
        c.send("NICK alice").await;
        c.send("USER alice 0 * :Alice").await;
        let lines = c.read_until("ERROR").await;
        assert!(contains(&lines, " 464 "), "{lines:#?}");
    }
    // Even the right password is refused while the address is blocked.
    let mut c = Client::connect(h.addr).await;
    c.send(&format!("PASS alice/fake:{PASSWORD}")).await;
    c.send("NICK alice").await;
    c.send("USER alice 0 * :Alice").await;
    let lines = c.read_until("ERROR").await;
    assert!(
        contains(
            &lines,
            "Too many failed logins from your address (127.0.0.1)"
        ) && contains(&lines, "Security"),
        "{lines:#?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn example_password_is_explained_and_does_not_block() {
    // Regression: copying "YOUR_PASSWORD" from the example counted as a
    // failed login, and a client retrying blocked the address everywhere.
    let h = Harness::start().await;
    for _ in 0..4 {
        let mut c = Client::connect(h.addr).await;
        c.send("PASS alice/fake@hexchat:YOUR_PASSWORD").await;
        c.send("NICK alice").await;
        c.send("USER alice 0 * :Alice").await;
        let lines = c.read_until("ERROR").await;
        assert!(contains(&lines, "is the example"), "{lines:#?}");
    }
    // Not blocked: the real password works.
    let (_c, lines) = Client::login(h.addr, "alice/fake@hexchat").await;
    assert!(contains(&lines, " 001 "), "{lines:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_password_explains_the_format() {
    let h = Harness::start().await;
    let mut c = Client::connect(h.addr).await;
    c.send("PASS alice/fake@hexchat:not-it").await;
    c.send("NICK alice").await;
    c.send("USER alice 0 * :Alice").await;
    let lines = c.read_until("ERROR").await;
    assert!(
        contains(&lines, "Wrong user or password (user \"alice\")")
            && contains(&lines, "<user>/<network>@<device>:<password>"),
        "{lines:#?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_network_is_explained() {
    let h = Harness::start().await;
    let mut c = Client::connect(h.addr).await;
    c.send(&format!("PASS alice/nope:{PASSWORD}")).await;
    c.send("NICK alice").await;
    c.send("USER alice 0 * :Alice").await;
    let lines = c.read_until("ERROR").await;
    assert!(
        contains(&lines, "no network called \"nope\"")
            && contains(&lines, "Your networks: fake")
            && contains(&lines, "alice/fake@hexchat:<password>"),
        "{lines:#?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnects_and_rejoins_after_server_drop() {
    let mut h = Harness::start().await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    h.ircd.send("__CLOSE__");
    c.read_until("Reconnecting in").await;
    // Backoff starts at 5 s (with jitter); allow some margin.
    let mut seen_join = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    while tokio::time::Instant::now() < deadline {
        if let Ok(Some(l)) =
            tokio::time::timeout(Duration::from_millis(500), h.ircd.received.recv()).await
        {
            if l.contains("JOIN #test") {
                seen_join = true;
                break;
            }
        }
    }
    assert!(
        seen_join,
        "bouncer must reconnect and rejoin saved channels"
    );
    c.read_until("Connected to fake").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_commands_and_part_forgets_channel() {
    let mut h = Harness::start().await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    c.send("PRIVMSG *status :status").await;
    let st = c.read_until("Network fake").await;
    assert!(
        st.last().unwrap().contains("connected (server 127.0.0.1"),
        "{st:#?}"
    );

    c.send("PART #test").await;
    h.ircd.expect("PART #test").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let running = h.running.as_ref().unwrap();
    let db = &running.ctx.db;
    let chans = db.with(|c| db::list_channels(c, 1)).unwrap();
    assert!(
        chans.is_empty(),
        "PART removes the saved channel: {chans:?}"
    );

    c.send("JOIN #other secretkey").await;
    h.ircd.expect("JOIN #other secretkey").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let chans = db.with(|c| db::list_channels(c, 1)).unwrap();
    assert_eq!(chans.len(), 1);
    let key = chans[0].key_enc.as_deref().unwrap();
    assert!(
        key.starts_with("v1:") && !key.contains("secretkey"),
        "channel keys are stored encrypted"
    );
    assert_eq!(
        running.ctx.secrets.decrypt(key).unwrap().expose(),
        "secretkey"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn commands_on_connect_are_sent_before_joining() {
    // The assertions live in the harness: MODE and the X login must arrive
    // after registration and before JOIN #test.
    let h = Harness::start_with(Some(
        "/mode $nick +x\n/msg x@channels.undernet.org login alice s3cret",
    ))
    .await;
    let (_c, lines) = Client::login(h.addr, "alice/fake@laptop").await;
    assert!(contains(&lines, "alice!u@fake JOIN #test"), "{lines:#?}");
    // The channel is joined only after the server hid the host (396).
    let hist = &h.ircd.history;
    let x = hist.iter().position(|l| l == "MODE alice +x");
    let join = hist.iter().position(|l| l == "JOIN #test");
    assert!(x.is_some() && join > x, "{hist:#?}");
}

/// Waits until no JOIN reaches the server for `ms` milliseconds.
async fn no_join_for(ircd: &mut FakeIrcd, ms: u64) -> Result<(), String> {
    let got = tokio::time::timeout(Duration::from_millis(ms), async {
        loop {
            if let Some(l) = ircd.received.recv().await {
                if l.starts_with("JOIN") {
                    return l;
                }
            }
        }
    })
    .await;
    match got {
        Ok(l) => Err(l),
        Err(_) => Ok(()),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hidden_host_is_waited_for_before_any_join() {
    // Regression: with /mode $nick +x in the commands on connect, the
    // channels were joined before the host was hidden, showing the real
    // address. Nothing may join until the server confirms it (396).
    let mut h = Harness::start_opts(
        Some("/mode $nick +x\n/join #fromperform"),
        &[("services", true, json!({"service": "auto", "wait": 1}))],
    )
    .await;
    // Past the wait, and the host is still not hidden: no JOIN at all.
    assert_eq!(no_join_for(&mut h.ircd, 1800).await, Ok(()));
    // A client's own JOIN (auto-join of HexChat, for example) waits too.
    let (mut c, _) = Client::login_no_channels(h.addr, "alice/fake@laptop").await;
    c.send("JOIN #fromclient").await;
    let lines = c.read_until("Waiting for your hidden host").await;
    assert!(contains(&lines, "#fromclient"), "{lines:#?}");
    assert_eq!(no_join_for(&mut h.ircd, 500).await, Ok(()));
    // The server hides the host: now everything is joined.
    h.ircd
        .send(":fake 396 alice alice.users.undernet.org :is now your hidden host");
    h.ircd.expect("JOIN #test").await;
    h.ircd.expect("JOIN #fromperform").await;
    h.ircd.expect("JOIN #fromclient").await;
    c.read_until("Your host is now hidden").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn joinnow_joins_before_the_host_is_hidden() {
    let mut h = Harness::start_opts(
        Some("/mode $nick +x"),
        &[("services", true, json!({"service": "auto", "wait": 1}))],
    )
    .await;
    assert_eq!(no_join_for(&mut h.ircd, 1500).await, Ok(()));
    let (mut c, _) = Client::login_no_channels(h.addr, "alice/fake@laptop").await;
    c.send("PRIVMSG *status :joinnow").await;
    h.ircd.expect("JOIN #test").await;
    c.read_until("Joining your channels now").await;
    // Nothing left waiting.
    c.send("PRIVMSG *status :joinnow").await;
    c.read_until("Your channels are not waiting").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_markers_are_shared_between_devices() {
    let h = Harness::start().await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :old news");
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Both devices have been here once (they have a history cursor).
    for dev in ["phone", "laptop"] {
        let (mut c, _) = Client::login(h.addr, &format!("alice/fake@{dev}")).await;
        c.send("QUIT").await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    h.ircd
        .send(":bob!b@fake PRIVMSG #test :read this on the phone");
    h.ircd
        .send(":bob!b@fake PRIVMSG alice :private, still unread");
    tokio::time::sleep(Duration::from_millis(300)).await;

    // The phone supports read markers and reads #test.
    let mut p = Client::connect(h.addr).await;
    p.send("CAP LS 302").await;
    p.send("CAP REQ :draft/read-marker server-time").await;
    p.send(&format!("PASS alice/fake@phone:{PASSWORD}")).await;
    p.send("NICK alice").await;
    p.send("USER alice 0 * :Alice").await;
    p.send("CAP END").await;
    p.read_until("read this on the phone").await;
    let ts = xbncd::util::format_server_time(xbncd::db::now_ms());
    p.send(&format!("MARKREAD #test timestamp={ts}")).await;
    p.read_until(&format!("MARKREAD #test timestamp={ts}"))
        .await;
    // Asking without a timestamp returns the marker; unknown targets give *.
    p.send("MARKREAD #nowhere").await;
    p.read_until("MARKREAD #nowhere *").await;

    // The laptop does not get #test again, but still gets the private message.
    let (mut l, mut lines) = Client::login(h.addr, "alice/fake@laptop").await;
    lines.extend(l.drain(Duration::from_millis(500)).await);
    assert!(!contains(&lines, "read this on the phone"), "{lines:#?}");
    assert!(contains(&lines, "private, still unread"), "{lines:#?}");

    // Writing in a conversation marks it read on the other devices.
    l.send("PRIVMSG bob :replying").await;
    let mk = p.read_until("MARKREAD bob timestamp=").await;
    assert!(mk.last().unwrap().contains("MARKREAD bob timestamp="));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bouncer_networks_for_modern_clients() {
    let h = Harness::start().await;
    // A control connection: no network in the login.
    let mut c = Client::connect(h.addr).await;
    c.send("CAP LS 302").await;
    c.read_until("soju.im/bouncer-networks").await;
    c.send("CAP REQ :soju.im/bouncer-networks soju.im/bouncer-networks-notify batch")
        .await;
    c.send(&format!("PASS alice@goguma:{PASSWORD}")).await;
    c.send("NICK alice").await;
    c.send("USER alice 0 * :Alice").await;
    c.send("CAP END").await;
    c.read_until(" 376 ").await;
    c.send("BOUNCER LISTNETWORKS").await;
    let lines = c.read_until("BATCH -").await;
    let net = lines
        .iter()
        .find(|l| l.contains("BOUNCER NETWORK"))
        .expect("network listed")
        .clone();
    assert!(
        net.contains("name=fake;host=127.0.0.1;") && net.contains("state=connected"),
        "{net}"
    );
    let id: i64 = net
        .split("BOUNCER NETWORK ")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap();

    // A second connection binds to that network and gets its channels.
    let mut b = Client::connect(h.addr).await;
    b.send("CAP LS 302").await;
    b.send("CAP REQ :soju.im/bouncer-networks").await;
    b.send(&format!("PASS alice@goguma:{PASSWORD}")).await;
    b.send(&format!("BOUNCER BIND {id}")).await;
    b.send("NICK alice").await;
    b.send("USER alice 0 * :Alice").await;
    b.send("CAP END").await;
    let burst = b.read_until(" 366 ").await;
    assert!(
        contains(&burst, &format!("BOUNCER_NETID={id}")),
        "{burst:#?}"
    );
    assert!(contains(&burst, "JOIN #test"), "{burst:#?}");

    // Adding and removing networks from the client; the control connection
    // is told about each change.
    c.send("BOUNCER ADDNETWORK name=undernet;host=127.0.0.1;port=1;tls=0")
        .await;
    let added = c.read_until("BOUNCER ADDNETWORK ").await;
    let new_id = added
        .last()
        .unwrap()
        .rsplit(' ')
        .next()
        .unwrap()
        .to_string();
    c.read_until(&format!("BOUNCER NETWORK {new_id} name=undernet;"))
        .await;
    c.send(&format!("BOUNCER DELNETWORK {new_id}")).await;
    c.read_until(&format!("BOUNCER NETWORK {new_id} *")).await;
    c.send("BOUNCER DELNETWORK 99999").await;
    c.read_until("FAIL BOUNCER INVALID_NETID DELNETWORK 99999")
        .await;
}
