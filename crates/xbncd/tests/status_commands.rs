//! The full `*status` command set (1.0): help, aliases, servers, channels,
//! buffers, the message of the day, broadcasts and permissions.

mod common;
use common::*;
use std::time::Duration;
use xbncd::db;

async fn ask(c: &mut Client, cmd: &str, until: &str) -> Vec<String> {
    c.send(&format!("PRIVMSG *status :{cmd}")).await;
    c.read_until(until).await
}

fn nid(h: &Harness) -> i64 {
    h.db.with(|c| {
        let u = db::get_user(c, "alice")?.unwrap();
        Ok(db::get_network(c, u.id, "fake")?.unwrap().id)
    })
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn help_is_grouped_bold_and_explains_each_command() {
    let h = Harness::start().await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    let help = ask(&mut c, "help", "/msg *notes help").await;
    // Command names in bold, sections underlined, administration visible to
    // an administrator.
    assert!(
        contains(&help, "\x02addserver\x02 <server> [[+]port] [password]"),
        "{help:#?}"
    );
    assert!(contains(&help, "\x02\x1fChannels\x1f\x02"), "{help:#?}");
    assert!(
        contains(&help, "\x02\x1fAdministration\x1f\x02"),
        "{help:#?}"
    );
    assert!(contains(&help, "\x02shutdown\x02"), "{help:#?}");
    // Every line is a NOTICE-free PRIVMSG from *status, none of them empty.
    assert!(help.iter().all(|l| !l.ends_with(" :")), "{help:#?}");

    let one = ask(&mut c, "help ListChans", "Also accepted").await;
    assert!(contains(&one, "\x02channels\x02"), "{one:#?}");
    assert!(contains(&one, "Example: /msg *status channels"), "{one:#?}");
    assert!(contains(&one, "Also accepted: listchans"), "{one:#?}");

    let search = ask(&mut c, "help buffer", "setbuffer").await;
    assert!(contains(&search, "\x02playbuffer\x02"), "{search:#?}");

    let unknown = ask(&mut c, "frobnicate", "Unknown command").await;
    assert!(contains(&unknown, "\"frobnicate\""), "{unknown:#?}");

    // In Spanish too.
    ask(&mut c, "language es", "Idioma").await;
    let es = ask(&mut c, "help", "/msg *notes help").await;
    assert!(contains(&es, "\x02\x1fCanales\x1f\x02"), "{es:#?}");
    assert!(
        contains(&es, "\x02addserver\x02 <servidor> [[+]puerto] [contraseña]"),
        "{es:#?}"
    );
    let one = ask(&mut c, "help setbuffer", "Ejemplo:").await;
    assert!(contains(&one, "siempre: no se borra"), "{one:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn other_bouncer_names_work_case_insensitively() {
    let h = Harness::start().await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    let r = ask(&mut c, "ListMods", "Modules of fake").await;
    assert!(contains(&r, "Modules of fake"), "{r:#?}");
    let r = ask(&mut c, "LISTNETWORKS", "fake (").await;
    assert!(contains(&r, "fake (127.0.0.1:"), "{r:#?}");
    let r = ask(&mut c, "listavailmods", "enable <name>").await;
    assert!(contains(&r, "\x02keepnick"), "{r:#?}");
    assert!(contains(&r, "\x02crypt"), "{r:#?}");
    let r = ask(&mut c, "Uptime", "running for").await;
    assert!(contains(&r, "Connected to fake"), "{r:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn backup_servers_can_be_added_jumped_to_and_promoted() {
    let mut h = Harness::start().await;
    let mut alt = FakeIrcd::start().await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    let port = alt.addr.port();

    let r = ask(&mut c, "addserver 127.0.0.1 nope", "Usage:").await;
    assert!(contains(&r, "\x02addserver\x02"), "{r:#?}");
    let r = ask(
        &mut c,
        &format!("addserver 127.0.0.1 {port} secretpw"),
        "added",
    )
    .await;
    assert!(contains(&r, &format!("127.0.0.1:{port}")), "{r:#?}");
    let r = ask(&mut c, &format!("addserver 127.0.0.1 {port}"), "already").await;
    assert!(contains(&r, "already in the list"), "{r:#?}");

    let r = ask(&mut c, "servers", &format!(":{port}")).await;
    assert!(contains(&r, "(main, no TLS)"), "{r:#?}");
    assert!(contains(&r, "← in use"), "{r:#?}");
    assert!(contains(&r, "(backup, no TLS, password)"), "{r:#?}");
    // The backup password is stored encrypted.
    let n = nid(&h);
    let o = h.db.with(|c| db::get_net_options(c, n)).unwrap();
    assert_eq!(o.servers.len(), 1);
    assert!(o.servers[0]
        .password_enc
        .as_deref()
        .is_some_and(|p| !p.contains("secretpw")));

    // Jump to the backup by name: the bouncer registers there with its
    // password.
    let r = ask(&mut c, &format!("jump 127.0.0.1:{port}"), "econnect").await;
    assert!(!r.is_empty());
    alt.expect("PASS").await;
    alt.expect("NICK alice").await;
    alt.expect("JOIN #test").await;
    let r = ask(&mut c, "jump nowhere.example", "not a server").await;
    assert!(contains(&r, "addserver"), "{r:#?}");

    // Removing the main server promotes the backup.
    let main = h.ircd.addr.port();
    let r = ask(
        &mut c,
        &format!("delserver 127.0.0.1 {main}"),
        "now the main",
    )
    .await;
    assert!(contains(&r, &format!("127.0.0.1:{port}")), "{r:#?}");
    let net = h.db.with(|c| db::get_network_by_id(c, n)).unwrap().unwrap();
    assert_eq!(net.port, port);
    assert!(net.server_password_enc.is_some());
    let r = ask(&mut c, "delserver 127.0.0.1", "only server").await;
    assert!(!r.is_empty());
    let _ = &mut h;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn trusted_fingerprints_are_validated_and_listed() {
    let h = Harness::start().await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    let r = ask(&mut c, "addfingerprint 12:34", "Usage:").await;
    assert!(!r.is_empty());
    let fp = "3F:9A:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:01:23:45:67:89:AB:CD:EF:10:32:54:76:98:C2";
    let r = ask(
        &mut c,
        &format!("AddTrustedServerFingerprint {fp}"),
        "trusted.",
    )
    .await;
    assert!(contains(&r, fp), "{r:#?}");
    // Without colons and in lower case it is the same certificate.
    let plain = fp.replace(':', "").to_lowercase();
    let r = ask(
        &mut c,
        &format!("addfingerprint {plain}"),
        "already trusted",
    )
    .await;
    assert!(!r.is_empty());
    let r = ask(&mut c, "listtrustedserverfingerprints", fp).await;
    assert!(contains(&r, "SHA-256"), "{r:#?}");
    let r = ask(&mut c, &format!("delfingerprint {plain}"), "removed").await;
    assert!(!r.is_empty());
    let r = ask(&mut c, "fingerprints", "No trusted").await;
    assert!(!r.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bind_hosts_must_belong_to_this_machine() {
    let h = Harness::start().await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    let r = ask(&mut c, "setbindhost 203.0.113.250", "not an address").await;
    assert!(!r.is_empty());
    let r = ask(&mut c, "setbindhost banana", "Usage:").await;
    assert!(!r.is_empty());
    let r = ask(&mut c, "setbindhost 127.0.0.1", "saved").await;
    assert!(contains(&r, "\x02127.0.0.1\x02"), "{r:#?}");
    let r = ask(&mut c, "setuserbindhost 127.0.0.1", "saved").await;
    assert!(!r.is_empty());
    let r = ask(&mut c, "showbindhost", "Local address").await;
    assert!(contains(&r, "this network \x02127.0.0.1\x02"), "{r:#?}");
    // The next connection really comes from that address.
    let r = ask(&mut c, "jump", "econnect").await;
    assert!(!r.is_empty());
    tokio::time::sleep(Duration::from_millis(600)).await;
    let r = ask(&mut c, "showbindhost", "In use now").await;
    assert!(contains(&r, "In use now: 127.0.0.1"), "{r:#?}");
    let r = ask(&mut c, "clearbindhost", "automatically").await;
    assert!(!r.is_empty());
    let r = ask(&mut c, "clearuserbindhost", "automatically").await;
    assert!(!r.is_empty());
    let uid =
        h.db.with(|c| Ok(db::get_user(c, "alice")?.unwrap().id))
            .unwrap();
    assert_eq!(h.db.with(|c| db::get_user_bind_host(c, uid)).unwrap(), "");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn channels_can_be_disabled_reordered_and_listed() {
    let mut h = Harness::start().await;
    let n = nid(&h);
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    c.send("JOIN #b,#a").await;
    h.ircd.expect("JOIN #b,#a").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let order = |h: &Harness| -> Vec<String> {
        h.db.with(|c| db::list_channels(c, n))
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect()
    };
    assert_eq!(order(&h), ["#test", "#b", "#a"]);

    let r = ask(&mut c, "listchans", " 3. ").await;
    assert!(contains(&r, "\x02#test"), "{r:#?}");
    assert!(contains(&r, "3 users (1 op), you: @, saved"), "{r:#?}");

    ask(&mut c, "movechan #a 1", " 3. ").await;
    assert_eq!(order(&h), ["#a", "#test", "#b"]);
    ask(&mut c, "swapchans #a #b", " 3. ").await;
    assert_eq!(order(&h), ["#b", "#test", "#a"]);

    let r = ask(&mut c, "disablechan #t*", "Disabled").await;
    assert!(contains(&r, "#test"), "{r:#?}");
    h.ircd.expect("PART #test").await;
    let ch = h.db.with(|c| db::list_channels(c, n)).unwrap();
    assert!(ch.iter().find(|c| c.name == "#test").unwrap().disabled);
    // A disabled channel is not joined on reconnect.
    ask(&mut c, "jump", "econnect").await;
    let j = h.ircd.expect("JOIN").await;
    assert!(!j.contains("#test"), "{j}");
    tokio::time::sleep(Duration::from_millis(400)).await;
    let r = ask(&mut c, "enablechan #test", "Enabled").await;
    assert!(!r.is_empty());
    let j = h.ircd.expect("JOIN").await;
    assert_eq!(j, "JOIN #test", "only the enabled channel is joined");
    tokio::time::sleep(Duration::from_millis(300)).await;

    let r = ask(&mut c, "nicks #b", "users").await;
    assert!(contains(&r, "@alice +bob carol"), "{r:#?}");
    let r = ask(&mut c, "topics", "#test").await;
    assert!(contains(&r, "Test topic"), "{r:#?}");
    let r = ask(&mut c, "detach #a", "#a").await;
    assert!(!r.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn buffers_play_back_clear_and_set_retention() {
    let h = Harness::start().await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    for i in 0..5 {
        h.ircd
            .send(&format!(":bob!b@fake PRIVMSG #test :linea {i}"));
    }
    h.ircd.send(":bob!b@fake PRIVMSG alice :hola en privado");
    c.read_until("hola en privado").await;
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let r = ask(&mut c, "playbuffer #test 3", "End of the playback").await;
    assert!(contains(&r, "last 3 lines"), "{r:#?}");
    assert!(contains(&r, "linea 4") && contains(&r, "linea 2"), "{r:#?}");
    assert!(!contains(&r, "linea 1"), "{r:#?}");

    let r = ask(&mut c, "setbuffer #test nada", "not stored").await;
    assert!(!r.is_empty());
    let n = nid(&h);
    let ch = h.db.with(|c| db::list_channels(c, n)).unwrap();
    assert_eq!(ch[0].retention_days, Some(-1));
    ask(&mut c, "setbuffer #test servidor", "server setting").await;
    let r = ask(&mut c, "setbuffer #nope 5", "not a saved channel").await;
    assert!(!r.is_empty());

    let r = ask(&mut c, "clearallquerybuffers", "private conversations").await;
    assert!(contains(&r, "1 message deleted"), "{r:#?}");
    let r = ask(&mut c, "ClearBuffer #te*", "deleted from").await;
    assert!(contains(&r, "5 messages deleted from #test"), "{r:#?}");
    let r = ask(&mut c, "clearallbuffers", "deleted from").await;
    assert!(contains(&r, "0 messages"), "{r:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn motd_and_broadcast_reach_users() {
    let h = Harness::start().await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    let r = ask(&mut c, "showmotd", "no message of the day").await;
    assert!(!r.is_empty());
    ask(
        &mut c,
        "setmotd Bienvenidos a \x02xBNC\x02",
        "saved (1 line)",
    )
    .await;
    ask(&mut c, "addmotd Segunda linea", "saved (2 lines)").await;
    let r = ask(&mut c, "showmotd", "Segunda linea").await;
    assert!(contains(&r, "Bienvenidos a \x02xBNC\x02"), "{r:#?}");
    // A device that connects now gets it.
    let (_other, burst) = Client::login(h.addr, "alice/fake@phone").await;
    let mut other = _other;
    let mut all = burst;
    if !contains(&all, "Segunda linea") {
        all.extend(other.read_until("Segunda linea").await);
    }
    assert!(contains(&all, "NOTICE alice :Bienvenidos"), "{all:#?}");

    ask(&mut c, "broadcast Reinicio a las 23:00", "Notice sent to 1").await;
    let b = other.read_until("Reinicio").await;
    assert!(
        contains(&b, "[Administrator notice] Reinicio a las 23:00"),
        "{b:#?}"
    );
    ask(&mut c, "clearmotd", "removed").await;
    assert!(h.running.as_ref().unwrap().ctx.server_motd().is_empty());

    let r = ask(&mut c, "traffic", "Messages stored").await;
    assert!(contains(&r, "From IRC servers"), "{r:#?}");
    let r = ask(&mut c, "listports", "xbnc.toml").await;
    assert!(contains(&r, "127.0.0.1:"), "{r:#?}");
    let r = ask(&mut c, "listusers", "account:").await;
    let r2 = c.read_until("alice").await;
    assert!(
        contains(&r, "\x02alice\x02") || contains(&r2, "\x02alice\x02"),
        "{r:#?}{r2:#?}"
    );
    let r = ask(&mut c, "listallusernetworks", "alice/fake").await;
    assert!(contains(&r, "\x02alice/fake\x02"), "{r:#?}");
    let r = ask(&mut c, "saveconfig", "Nothing to save").await;
    assert!(!r.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ordinary_users_do_not_see_or_run_admin_commands() {
    let h = Harness::start().await;
    h.db.with(|c| db::set_user_role(c, "alice", "user"))
        .unwrap();
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    let help = ask(&mut c, "help", "/msg *notes help").await;
    assert!(!contains(&help, "Administration"), "{help:#?}");
    assert!(!contains(&help, "\x02shutdown\x02"), "{help:#?}");
    assert!(!contains(&help, "\x02setbindhost\x02"), "{help:#?}");
    assert!(contains(&help, "\x02addserver\x02"), "{help:#?}");
    for cmd in [
        "shutdown",
        "restart",
        "broadcast hola",
        "listusers",
        "setmotd x",
        "setbindhost 127.0.0.1",
    ] {
        let r = ask(&mut c, cmd, "can use this command").await;
        assert!(contains(&r, "Only a"), "{cmd}: {r:#?}");
    }
    let r = ask(&mut c, "help shutdown", "Only for superadministrators").await;
    assert!(!r.is_empty());
    let r = ask(&mut c, "clients bob", "another user").await;
    assert!(!r.is_empty());
    // xBNC is still running.
    let r = ask(&mut c, "version", "xBNC").await;
    assert!(!r.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn networks_can_be_added_deleted_and_moved_from_irc() {
    let h = Harness::start().await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    let r = ask(
        &mut c,
        "addnetwork libera irc.libera.invalid +6697",
        "added",
    )
    .await;
    assert!(contains(&r, "\x02alice/libera\x02"), "{r:#?}");
    let uid =
        h.db.with(|c| Ok(db::get_user(c, "alice")?.unwrap().id))
            .unwrap();
    let net =
        h.db.with(|c| db::get_network(c, uid, "libera"))
            .unwrap()
            .unwrap();
    assert!(net.tls && net.port == 6697);

    let r = ask(&mut c, "delnetwork libera", "To confirm").await;
    assert!(!r.is_empty());
    assert!(h
        .db
        .with(|c| db::get_network(c, uid, "libera"))
        .unwrap()
        .is_some());

    let hash = xbncd::crypto::hash_password(PASSWORD).unwrap();
    h.db.with(|c| db::create_user(c, "bob", &hash, false))
        .unwrap();
    let r = ask(&mut c, "movenetwork alice libera bob lib2", "bob/lib2").await;
    assert!(contains(&r, "moved"), "{r:#?}");
    let bob =
        h.db.with(|c| Ok(db::get_user(c, "bob")?.unwrap().id))
            .unwrap();
    assert!(h
        .db
        .with(|c| db::get_network(c, bob, "lib2"))
        .unwrap()
        .is_some());
    let r = ask(&mut c, "movenetwork alice nada bob", "Cannot move").await;
    assert!(!r.is_empty());

    ask(
        &mut c,
        "addnetwork borrar irc.example.invalid 6667",
        "added",
    )
    .await;
    let r = ask(&mut c, "delnetwork borrar confirm", "deleted").await;
    assert!(!r.is_empty());
    assert!(h
        .db
        .with(|c| db::get_network(c, uid, "borrar"))
        .unwrap()
        .is_none());
    let audit = h.db.with(|c| db::recent_audit(c, 50)).unwrap();
    assert!(audit.iter().any(|a| a.2 == "network.move"), "{audit:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failing_server_makes_xbnc_try_the_next_one() {
    let h = Harness::start().await;
    let mut alt = FakeIrcd::start().await;
    let n = nid(&h);
    // A port nobody listens on as the main server, the fake one as backup.
    let dead = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_port = dead.local_addr().unwrap().port();
    drop(dead);
    h.db.with(|c| {
        db::update_network(
            c,
            n,
            &db::NetworkUpdate {
                port: Some(dead_port),
                ..Default::default()
            },
        )?;
        db::set_net_options(
            c,
            n,
            &db::NetOptions {
                servers: vec![db::AltServer {
                    host: "127.0.0.1".into(),
                    port: alt.addr.port(),
                    tls: false,
                    password_enc: None,
                }],
                ..Default::default()
            },
        )
    })
    .unwrap();
    let ctx = h.running.as_ref().unwrap().ctx.clone();
    xbncd::server::restart_network(&ctx, n).await.unwrap();
    alt.expect("NICK alice").await;
    alt.expect("JOIN #test").await;
}

/// A language added as a file (no recompiling) is offered and used, and
/// what it does not translate falls back to English.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_language_dropped_in_as_a_file_is_used() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("pt.json"),
        r#"{"language": {"code": "pt", "name": "Português", "english_name": "Portuguese", "locale": "pt-PT"},
            "irc": {"language_set": "Idioma alterado para português.",
                    "st.xbnc_status_commands": "<b>xBNC</b>: comandos do *status",
                    "help.group.channels": "Canais",
                    "help.channels.desc": "Os teus canais"},
            "panel": {"Joined": "Dentro"}}"#,
    )
    .unwrap();
    let report = xbncd::i18n::load_dir(dir.path());
    assert!(report[0].contains("pt loaded"), "{report:?}");
    let h = Harness::start().await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    let r = ask(&mut c, "language pt", "português").await;
    assert!(!r.is_empty());
    let help = ask(&mut c, "help", "/msg *notes help").await;
    assert!(
        contains(&help, "\x02xBNC\x02: comandos do *status"),
        "{help:#?}"
    );
    assert!(contains(&help, "\x02\x1fCanais\x1f\x02"), "{help:#?}");
    assert!(contains(&help, "Os teus canais"), "{help:#?}");
    // Not translated: English.
    assert!(
        contains(&help, "\x02\x1fAdministration\x1f\x02"),
        "{help:#?}"
    );
    let uid =
        h.db.with(|c| Ok(db::get_user(c, "alice")?.unwrap().id))
            .unwrap();
    let u = h.db.with(|c| db::get_user_by_id(c, uid)).unwrap().unwrap();
    assert_eq!(u.language, "pt");
    let langs = xbncd::i18n::languages();
    assert!(langs
        .iter()
        .any(|l| l.code == "pt" && l.name == "Português"));
    assert_eq!(
        xbncd::i18n::panel_texts("pt")
            .unwrap()
            .get("Joined")
            .map(String::as_str),
        Some("Dentro")
    );
}
