//! End-to-end tests of the native modules against the scripted IRC server.

mod common;
use common::*;
use serde_json::json;
use std::time::Duration;

/// Reads ircd lines until `needle`; panics if `forbidden` shows up first.
async fn expect_without(h: &mut Harness, needle: &str, forbidden: &str) {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        match tokio::time::timeout_at(deadline, h.ircd.received.recv()).await {
            Ok(Some(l)) if l.contains(forbidden) => panic!("ircd received {l:?}"),
            Ok(Some(l)) if l.contains(needle) => return,
            Ok(Some(_)) => {}
            _ => panic!("ircd never received {needle:?}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn away_is_set_when_detached_and_summarised_on_return() {
    let mut h = Harness::start_opts(
        None,
        &[(
            "away",
            true,
            json!({"delay": 0, "message": "brb", "nick": "{nick}|away"}),
        )],
    )
    .await;
    let l = h.ircd.expect("AWAY").await;
    assert!(l.ends_with("brb"), "{l}");
    // The away nick (the fake server does not echo NICK, so we stay "alice").
    h.ircd.expect("NICK alice|away").await;
    h.ircd.send(":bob!b@fake PRIVMSG alice :are you there?");
    h.ircd.send(":carol!c@fake PRIVMSG #test :alice: ping");
    h.ircd.send(":carol!c@fake PRIVMSG #test :unrelated");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    // Coming back clears the away status and restores the nick...
    h.ircd.expect("AWAY").await;
    // ...and tells the user what they missed.
    let lines = c.read_until("While you were away").await;
    let s = lines.last().unwrap();
    assert!(
        s.contains("1 private message(s) and 1 mention(s) in #test"),
        "{s}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn keepnick_takes_the_nick_back() {
    let mut h = Harness::start().await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    // The server renames us; keepnick asks whether "alice" is free.
    h.ircd.send(":alice!u@fake NICK alice_");
    h.ircd.expect("ISON alice").await;
    h.ircd.send(":fake 303 alice_ :");
    h.ircd.expect("NICK alice").await;
    // A failed attempt is not shown to the user.
    h.ircd
        .send(":fake 433 alice_ alice :Nickname is already in use");
    // When the holder quits, keepnick tries again at once.
    h.ircd.send(":alice!x@elsewhere QUIT :bye");
    h.ircd.expect("NICK alice").await;
    let lines = c.drain(Duration::from_millis(400)).await;
    assert!(!contains(&lines, " 433 "), "{lines:#?}");
    assert!(
        !contains(&lines, " 303 "),
        "ISON replies are internal: {lines:#?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn route_replies_answer_only_the_asking_client() {
    let mut h = Harness::start().await;
    let (mut a, _) = Client::login(h.addr, "alice/fake@laptop").await;
    let (mut b, _) = Client::login(h.addr, "alice/fake@phone").await;
    a.send("WHOIS bob").await;
    h.ircd.expect("WHOIS bob").await;
    h.ircd.send(":fake 311 alice bob b fake * :Bob");
    h.ircd.send(":fake 318 alice bob :End of /WHOIS list.");
    let got = a.read_until(" 318 ").await;
    assert!(contains(&got, " 311 alice bob "), "{got:#?}");
    let other = b.drain(Duration::from_millis(400)).await;
    assert!(!contains(&other, " 311 "), "{other:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn events_are_kept_in_history_for_new_devices() {
    let h = Harness::start().await;
    let (mut a, _) = Client::login(h.addr, "alice/fake@laptop").await;
    h.ircd.send(":dave!d@fake JOIN #test");
    h.ircd.send(":dave!d@fake PART #test :later");
    a.read_until("PART #test").await;
    let (_b, lines) = Client::login(h.addr, "alice/fake@phone").await;
    let more = {
        let mut b = _b;
        b.drain(Duration::from_millis(500)).await
    };
    let all: Vec<String> = lines.into_iter().chain(more).collect();
    assert!(
        contains(&all, "*events!xbnc@")
            && contains(&all, "dave joined")
            && contains(&all, "dave left (later)"),
        "{all:#?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ctcp_version_is_answered_while_detached() {
    let mut h = Harness::start().await;
    h.ircd.send(":bob!b@fake PRIVMSG alice :\x01VERSION\x01");
    let l = h.ircd.expect("NOTICE bob").await;
    assert!(l.contains("\x01VERSION xBNC "), "{l:?}");
    assert!(l.contains("Made by y2k - https://xbnc.org\x01"), "{l:?}");
}

/// The default VERSION reply advertises xBNC even with a client attached
/// (the user can change the text in the CTCP module).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ctcp_default_version_is_answered_while_attached() {
    let mut h = Harness::start().await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    h.ircd.send(":bob!b@fake PRIVMSG alice :\x01VERSION\x01");
    let l = h.ircd.expect("NOTICE bob").await;
    assert!(l.contains(concat!("\x01VERSION xBNC ", env!("CARGO_PKG_VERSION"), " Made by y2k - https://xbnc.org\x01")), "{l:?}");
    let lines = c.drain(Duration::from_millis(400)).await;
    assert!(!contains(&lines, "\x01VERSION"), "{lines:#?}");
    c.send("PRIVMSG *status :version").await;
    c.read_until("https://xbnc.org").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ctcp_custom_version_and_blocking() {
    let mut h = Harness::start_opts(
        None,
        &[(
            "ctcp",
            true,
            json!({"version": "MyClient 1.0", "block": "TIME"}),
        )],
    )
    .await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    h.ircd.send(":bob!b@fake PRIVMSG alice :\x01TIME\x01");
    h.ircd.send(":bob!b@fake PRIVMSG alice :\x01VERSION\x01");
    let l = h.ircd.expect("NOTICE bob").await;
    assert!(l.contains("\x01VERSION MyClient 1.0\x01"), "{l:?}");
    let lines = c.drain(Duration::from_millis(400)).await;
    assert!(
        !contains(&lines, "\x01TIME"),
        "blocked CTCP reached the client: {lines:#?}"
    );
    assert!(
        !contains(&lines, "\x01VERSION"),
        "answered CTCP reached the client: {lines:#?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sticky_channels_cannot_be_left() {
    let mut h =
        Harness::start_opts(None, &[("stickychan", true, json!({"channels": "#test"}))]).await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    c.send("PART #test :bye").await;
    let lines = c.read_until("sticky").await;
    assert!(contains(&lines, "#test is a sticky channel"), "{lines:#?}");
    c.send("PRIVMSG #test :still here").await;
    expect_without(&mut h, "PRIVMSG #test :still here", "PART #test").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aliases_expand_to_irc_commands() {
    let mut h = Harness::start_opts(
        None,
        &[(
            "alias",
            true,
            json!({"aliases": "ghost /msg NickServ GHOST $1\nop /mode $1 +o $nick"}),
        )],
    )
    .await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    c.send("GHOST bob").await;
    h.ircd.expect("PRIVMSG NickServ :GHOST bob").await;
    c.send("OP #test").await;
    h.ircd.expect("MODE #test +o alice").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn autoop_by_mask_and_by_challenge() {
    let mut h = Harness::start_opts(
        None,
        &[(
            "autoop",
            true,
            json!({"delay": 0, "entries": "o *!*@trusted.example\nv carl!*@* #test k3y"}),
        )],
    )
    .await;
    h.ircd.send(":eve!e@trusted.example JOIN #test");
    h.ircd.expect("MODE #test +o eve").await;
    h.ircd.send(":mallory!m@evil.example JOIN #test");
    h.ircd.send(":carl!c@somewhere JOIN #test");
    let l = h.ircd.expect("NOTICE carl :!XBNC-AUTOOP CHALLENGE ").await;
    let challenge = l.rsplit(' ').next().unwrap().to_string();
    // A wrong answer gives nothing; the right one gives voice.
    h.ircd
        .send(":carl!c@somewhere NOTICE alice :!XBNC-AUTOOP RESPONSE 0000");
    h.ircd.send(":carl!c@somewhere JOIN #test");
    let l = h.ircd.expect("NOTICE carl :!XBNC-AUTOOP CHALLENGE ").await;
    let challenge2 = l.rsplit(' ').next().unwrap().to_string();
    assert_ne!(challenge, challenge2);
    let resp = xbncd::modules::autoop_response("k3y", &challenge2);
    h.ircd.send(&format!(
        ":carl!c@somewhere NOTICE alice :!XBNC-AUTOOP RESPONSE {resp}"
    ));
    expect_without(&mut h, "MODE #test +v carl", "mallory").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kickrejoin_rejoins_after_a_kick() {
    let mut h = Harness::start_opts(None, &[("kickrejoin", true, json!({"delay": 0}))]).await;
    h.ircd.send(":op!o@fake KICK #test alice :out");
    h.ircd.expect("JOIN #test").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn watch_copies_matches_to_star_watch() {
    let h = Harness::start_opts(None, &[("watch", true, json!({"patterns": "*deploy*"}))]).await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :the deploy failed");
    let lines = c.read_until("*watch!xbnc@").await;
    assert!(
        lines
            .last()
            .unwrap()
            .contains("PRIVMSG alice :[#test] <bob> the deploy failed"),
        "{lines:#?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_notes_and_language_from_irc() {
    let h = Harness::start().await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    c.send("PRIVMSG *notes :add todo buy milk").await;
    c.read_until("Note todo saved.").await;
    c.send("PRIVMSG *notes :list").await;
    c.read_until("todo: buy milk").await;
    c.send("PRIVMSG *status :set away delay 30").await;
    c.read_until("Saved.").await;
    c.send("PRIVMSG *status :get away").await;
    c.read_until("delay = 30").await;
    c.send("PRIVMSG *status :set away delay 99999").await;
    c.read_until("Not saved: delay: must be between").await;
    c.send("PRIVMSG *status :disable keepnick").await;
    c.read_until("Module keepnick disabled.").await;
    c.send("PRIVMSG *status :modules").await;
    let lines = c.read_until("ctcp").await;
    assert!(contains(&lines, "keepnick"), "{lines:#?}");
    c.send("PRIVMSG *status :language es").await;
    c.read_until("Idioma cambiado a español.").await;
    c.send("PRIVMSG *status :help").await;
    c.read_until("comandos de *status").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn motd_is_replayed_or_blocked() {
    let h = Harness::start().await;
    h.ircd.send(":fake 375 alice :- fake Message of the Day -");
    h.ircd.send(":fake 372 alice :- welcome to fakenet");
    h.ircd.send(":fake 376 alice :End of /MOTD command.");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (_c, lines) = Client::login(h.addr, "alice/fake@laptop").await;
    assert!(
        contains(&lines, " 372 alice :- welcome to fakenet"),
        "{lines:#?}"
    );

    let h = Harness::start_opts(None, &[("block_motd", true, json!({}))]).await;
    h.ircd.send(":fake 375 alice :- MOTD -");
    h.ircd.send(":fake 372 alice :- secret motd");
    h.ircd.send(":fake 376 alice :End of /MOTD command.");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (_c, lines) = Client::login(h.addr, "alice/fake@laptop").await;
    assert!(!contains(&lines, "secret motd"), "{lines:#?}");
    assert!(contains(&lines, " 422 alice :MOTD hidden"), "{lines:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn log_module_writes_classic_log_files() {
    let h = Harness::start_opts(None, &[("log", true, json!({}))]).await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :logged line");
    let dir = h._dir.path().join("logs/alice/fake/#test");
    let mut content = String::new();
    for _ in 0..50 {
        if let Some(f) = std::fs::read_dir(&dir).ok().and_then(|mut d| d.next()) {
            content = std::fs::read_to_string(f.unwrap().path()).unwrap_or_default();
            if content.contains("logged line") {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(content.contains("] <bob> logged line"), "{content:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dcc_goes_through_the_bouncer() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut h =
        Harness::start_opts(None, &[("dcc", true, json!({"private_addresses": true}))]).await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    let ip = u32::from(std::net::Ipv4Addr::LOCALHOST);

    // Incoming file: bob offers it from his own address...
    let bob = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bob_port = bob.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut s, _) = bob.accept().await.unwrap();
        s.write_all(b"hello").await.unwrap();
    });
    h.ircd.send(&format!(
        ":bob!b@fake PRIVMSG alice :\x01DCC SEND notes.txt {ip} {bob_port} 5\x01"
    ));
    // ...and the client gets an offer pointing at xBNC instead.
    let lines = c.read_until("DCC SEND notes.txt").await;
    let offer = lines.last().unwrap();
    let port: u16 = offer
        .trim_end_matches('\x01')
        .rsplit(' ')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    assert_ne!(port, bob_port, "{offer}");
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let mut got = Vec::new();
    s.read_to_end(&mut got).await.unwrap();
    assert_eq!(got, b"hello");

    // Outgoing file: the client offers from its own (private) address...
    let dev = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dev_port = dev.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut s, _) = dev.accept().await.unwrap();
        s.write_all(b"from the laptop").await.unwrap();
    });
    c.send(&format!(
        "PRIVMSG bob :\x01DCC SEND photo.jpg 3232235777 {dev_port} 15\x01"
    ))
    .await;
    // ...and bob sees xBNC's address and a relay port.
    let l = h.ircd.expect("DCC SEND photo.jpg").await;
    let words: Vec<&str> = l.trim_end_matches('\x01').split(' ').collect();
    assert_eq!(words[words.len() - 3], ip.to_string(), "{l}");
    let relay_port: u16 = words[words.len() - 2].parse().unwrap();
    assert_ne!(relay_port, dev_port);
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", relay_port))
        .await
        .unwrap();
    let mut got = Vec::new();
    s.read_to_end(&mut got).await.unwrap();
    assert_eq!(got, b"from the laptop");

    // RESUME is translated back to the port bob knows.
    c.send(&format!(
        "PRIVMSG bob :\x01DCC ACCEPT photo.jpg {dev_port} 100\x01"
    ))
    .await;
    h.ircd
        .expect(&format!("DCC ACCEPT photo.jpg {relay_port} 100"))
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dcc_to_private_addresses_is_refused_by_default() {
    let h = Harness::start_opts(None, &[("dcc", true, json!({}))]).await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    let ip = u32::from(std::net::Ipv4Addr::new(10, 0, 0, 5));
    h.ircd.send(&format!(
        ":mallory!m@fake PRIVMSG alice :\x01DCC SEND x {ip} 6379 5\x01"
    ));
    c.read_until("private address").await;
    h.ircd
        .send(":mallory!m@fake PRIVMSG alice :\x01DCC CHAT chat 134744072 25\x01");
    c.read_until("reserved").await;
}
