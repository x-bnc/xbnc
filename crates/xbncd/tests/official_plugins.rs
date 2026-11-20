//! The official plugins (reminders, tell, polls, channel stats, anti-flood)
//! running in a real bouncer against the scripted IRC server.

mod common;
use common::*;
use serde_json::json;
use std::time::Duration;

const REMIND: &[u8] = include_bytes!("../../../sdk/c/examples/remind.wasm");
const TELL: &[u8] = include_bytes!("../../../sdk/c/examples/tell.wasm");
const POLL: &[u8] = include_bytes!("../../../sdk/c/examples/poll.wasm");
const STATS: &[u8] = include_bytes!("../../../sdk/c/examples/chanstats.wasm");
const FLOOD: &[u8] = include_bytes!("../../../sdk/c/examples/antiflood.wasm");
const DICE: &[u8] = include_bytes!("../../../sdk/c/examples/dice.wasm");
const AUTO: &[u8] = include_bytes!("../../../sdk/c/examples/autoresponder.wasm");

/// Timer events every second (the network actor ticks every 5 s).
fn fast_timer() {
    std::env::set_var("XBNC_PLUGIN_TIMER_SECS", "1");
    let _ = tracing_subscriber::fmt()
        .with_env_filter("warn,xbncd=info")
        .with_test_writer()
        .try_init();
}

async fn expect_slow(ircd: &mut FakeIrcd, needle: &str, secs: u64) -> String {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    let mut seen = Vec::new();
    loop {
        match tokio::time::timeout_at(deadline, ircd.received.recv()).await {
            Ok(Some(l)) if l.contains(needle) => return l,
            Ok(Some(l)) => seen.push(l),
            _ => panic!("ircd never received {needle:?}; got {seen:#?}"),
        }
    }
}

/// Runs SQL on the bouncer's database (SQLite or PostgreSQL).
struct Sql(xbncd::db::Db);
impl Sql {
    fn execute<P: xbncd::sql::Params>(&self, sql: &str, p: P) -> anyhow::Result<usize> {
        self.0.with(|c| Ok(c.execute(sql, p)?))
    }
    fn query_row<T: xbncd::sql::FromCol, P: xbncd::sql::Params>(
        &self,
        sql: &str,
        p: P,
        _f: fn(&xbncd::sql::Row) -> xbncd::sql::SqlResult<T>,
    ) -> anyhow::Result<T> {
        self.0.with(|c| Ok(c.query_row(sql, p, |r| r.get(0))?))
    }
}

fn kv(h: &Harness, _plugin: &str) -> Sql {
    Sql(h.db.clone())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reminders() {
    fast_timer();
    let mut h = Harness::start_full(None, &[], &[(REMIND, json!({"lang": "es"}))]).await;
    h.ircd
        .send(":carol!c@fake PRIVMSG #test :!remind 1h30m llamar a Bob");
    h.ircd
        .expect("PRIVMSG #test :carol: vale, te lo recuerdo a las ")
        .await;
    h.ircd
        .send(":carol!c@fake PRIVMSG #test :!remind mañana algo");
    h.ircd
        .expect("PRIVMSG #test :carol: no entiendo cuándo")
        .await;
    h.ircd.send(":carol!c@fake PRIVMSG #test :!remind list");
    h.ircd
        .expect("PRIVMSG #test :carol: en 1 h 29 min: llamar a Bob")
        .await;

    // Make it due now.
    let db = kv(&h, "remind");
    db.execute(
        "UPDATE plugin_kv SET key = 'r:000000000000001:0000001' WHERE plugin_id = 'remind' AND key LIKE 'r:%'",
        (),
    )
    .unwrap();
    expect_slow(
        &mut h.ircd,
        "PRIVMSG #test :carol: recordatorio: llamar a Bob",
        15,
    )
    .await;
    h.ircd.send(":carol!c@fake PRIVMSG #test :!remind list");
    h.ircd
        .expect("PRIVMSG #test :carol: no tienes recordatorios pendientes.")
        .await;

    // A private reminder for the user.
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    c.send("PRIVMSG *remind :10m sacar la pizza").await;
    // (the history playback comes first)
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let l = tokio::time::timeout_at(deadline, c.lines.next_line())
            .await
            .expect("answer")
            .unwrap()
            .unwrap();
        if l.starts_with(":*remind!xbnc@xbnc PRIVMSG") {
            assert!(l.contains("te lo recuerdo dentro de 10 min"), "{l}");
            break;
        }
    }
    db.execute(
        "UPDATE plugin_kv SET key = 'r:000000000000002:0000002' WHERE plugin_id = 'remind' AND key LIKE 'r:%'",
        (),
    )
    .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let l = tokio::time::timeout_at(deadline, c.lines.next_line())
            .await
            .expect("private reminder")
            .unwrap()
            .unwrap();
        if l.contains("Recordatorio: sacar la pizza") {
            break;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tell_delivers_when_they_speak_or_join() {
    let mut h = Harness::start_full(None, &[], &[(TELL, json!({"lang": "es"}))]).await;
    h.ircd
        .send(":carol!c@fake PRIVMSG #test :!tell dave el build está arreglado");
    h.ircd
        .expect(
            "PRIVMSG #test :carol: vale, se lo diré a dave en cuanto escriba o entre en el canal.",
        )
        .await;
    h.ircd.send(":carol!c@fake PRIVMSG #test :!tell eve: hola");
    h.ircd.expect("se lo diré a eve").await;
    h.ircd.send(":carol!c@fake PRIVMSG #test :!tell carol yo");
    h.ircd
        .expect("PRIVMSG #test :carol: eso no hace falta.")
        .await;
    // dave speaks.
    h.ircd.send(":Dave!d@fake PRIVMSG #test :buenas");
    let l = h
        .ircd
        .expect("PRIVMSG #test :Dave: carol te dejó un mensaje hace")
        .await;
    assert!(l.ends_with(": el build está arreglado"), "{l}");
    // Only once.
    h.ircd.send(":dave!d@fake PRIVMSG #test :otra vez");
    h.ircd.send(":carol!c@fake PRIVMSG #test :!tell");
    h.ircd
        .expect("PRIVMSG #test :Uso: !tell <nick> <mensaje>")
        .await;
    // eve joins.
    h.ircd.send(":eve!e@fake JOIN #test");
    h.ircd
        .expect("PRIVMSG #test :eve: carol te dejó un mensaje")
        .await;
    // Limit per person.
    for i in 0..5 {
        h.ircd.send(&format!(
            ":carol!c@fake PRIVMSG #test :!tell zed mensaje {i}"
        ));
        h.ircd.expect("se lo diré a zed").await;
    }
    h.ircd
        .send(":carol!c@fake PRIVMSG #test :!tell zed uno más");
    h.ircd
        .expect("zed ya tiene demasiados mensajes esperando.")
        .await;
    h.ircd.send(":zed!z@fake PRIVMSG #test :hola");
    h.ircd
        .expect("PRIVMSG #test :zed: carol te dejó un mensaje")
        .await;
    h.ircd
        .expect("PRIVMSG #test :zed: carol te dejó un mensaje")
        .await;
    h.ircd
        .expect("PRIVMSG #test :zed: carol te dejó un mensaje")
        .await;
    h.ircd
        .expect("PRIVMSG #test :zed: tienes 2 mensajes más")
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn polls() {
    fast_timer();
    let mut h =
        Harness::start_full(None, &[], &[(POLL, json!({"minutes": 1, "lang": "es"}))]).await;
    h.ircd
        .send(":carol!c@fake PRIVMSG #test :!poll ¿Pizza o sushi? | pizza | sushi | las dos");
    let l = h
        .ircd
        .expect("PRIVMSG #test :Encuesta de carol: ¿Pizza o sushi? — 1) pizza 2) sushi 3) las dos.")
        .await;
    assert!(l.contains("Vota con !vote N"), "{l}");
    h.ircd.send(":bob!b@fake PRIVMSG #test :!vote 1");
    // Votes are confirmed privately, not in the channel.
    h.ircd
        .expect("NOTICE bob :Voto registrado en #test: 1) pizza.")
        .await;
    h.ircd.send(":dave!d@fake PRIVMSG #test :!vote 2");
    h.ircd.send(":dave!d@fake PRIVMSG #test :!vote 1"); // changes the vote
    h.ircd.send(":eve!e@fake PRIVMSG #test :!vote 9");
    h.ircd
        .expect("PRIVMSG #test :eve: vota con un número del 1 al 3.")
        .await;
    h.ircd.send(":eve!e@fake PRIVMSG #test :!poll");
    h.ircd
        .expect("1) pizza: 2 (100%), 2) sushi: 0 (0%), 3) las dos: 0 (0%). 2 votos.")
        .await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :!poll end");
    h.ircd
        .expect("PRIVMSG #test :bob: solo quien la creó puede cerrarla.")
        .await;
    h.ircd
        .send(":carol!c@fake PRIVMSG #test :!poll otra | a | b");
    h.ircd.expect("ya hay una encuesta abierta").await;
    h.ircd.send(":carol!c@fake PRIVMSG #test :!poll end");
    h.ircd
        .expect("PRIVMSG #test :Encuesta cerrada: ¿Pizza o sushi?")
        .await;
    // A new one closes on its own.
    h.ircd
        .send(":carol!c@fake PRIVMSG #test :!poll ¿Café? | sí | no");
    h.ircd.expect("Encuesta de carol").await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :!vote 2");
    // Wait until the vote is stored before changing the record.
    h.ircd.send(":bob!b@fake PRIVMSG #test :!poll");
    h.ircd.expect("1 voto.").await;
    let db = kv(&h, "poll");
    // The stored poll ends in the past.
    let rec: Vec<u8> = db
        .query_row(
            "SELECT value FROM plugin_kv WHERE plugin_id = 'poll' AND key = 'p:#test'",
            (),
            |r| r.get(0),
        )
        .unwrap();
    let rec = String::from_utf8(rec).unwrap();
    let mut f: Vec<&str> = rec.split('\t').collect();
    f[2] = "1";
    db.execute(
        "UPDATE plugin_kv SET value = ?1 WHERE plugin_id = 'poll' AND key = 'p:#test'",
        [f.join("\t").into_bytes()],
    )
    .unwrap();
    let l = expect_slow(&mut h.ircd, "PRIVMSG #test :Encuesta cerrada: ¿Café?", 15).await;
    assert!(l.ends_with("Gana: no."), "{l}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn channel_stats() {
    let mut h = Harness::start_full(None, &[], &[(STATS, json!({"top": 3, "lang": "es"}))]).await;
    for _ in 0..3 {
        h.ircd.send(":bob!b@fake PRIVMSG #test :uno dos tres");
    }
    h.ircd.send(":carol!c@fake PRIVMSG #test :hola");
    h.ircd.send(":carol!c@fake PRIVMSG #test :!stats");
    let l = h
        .ircd
        .expect("PRIVMSG #test :#test: 4 mensajes desde el")
        .await;
    assert!(l.ends_with("Más activos: bob 3, carol 1."), "{l}");
    h.ircd.send(":carol!c@fake PRIVMSG #test :!stats BOB");
    let l = h
        .ircd
        .expect("PRIVMSG #test :bob en #test: 3 mensajes, 9 palabras; la última vez hace")
        .await;
    assert!(l.ends_with(" s."), "{l}");
    h.ircd.send(":carol!c@fake PRIVMSG #test :!stats nadie");
    h.ircd
        .expect("PRIVMSG #test :nadie no ha escrito nada en #test")
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn anti_flood() {
    let mut h = Harness::start_full(
        None,
        &[],
        &[(FLOOD, json!({"channels": ["#test"], "lines": 3, "seconds": 10, "action": "ban", "exempt": ["carol"], "lang": "es"}))],
    )
    .await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    // Exempt nicks can say what they like.
    for i in 0..5 {
        h.ircd
            .send(&format!(":carol!c@fake PRIVMSG #test :línea {i}"));
    }
    for i in 0..4 {
        h.ircd
            .send(&format!(":bob!b@spam.example PRIVMSG #test :flood {i}"));
    }
    h.ircd.expect("MODE #test +b *!*@spam.example").await;
    h.ircd.expect("KICK #test bob :Sin flood, por favor").await;
    let l = c.read_until(":*antiflood!xbnc@xbnc PRIVMSG").await;
    assert!(
        l.last()
            .unwrap()
            .contains("Expulsado bob de #test por flood."),
        "{l:?}"
    );
    // Repeating the same line.
    for _ in 0..3 {
        h.ircd.send(":dave!d@fake PRIVMSG #test :compra ya");
    }
    h.ircd.expect("KICK #test dave").await;
    let db = kv(&h, "antiflood");
    let n: i64 = db
        .query_row("SELECT COUNT(*) FROM plugin_kv WHERE plugin_id = 'antiflood' AND key LIKE 'b:%#test *!*@spam.example'", (), |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1, "the ban is remembered to be lifted");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dice_in_channels_and_private_messages() {
    let mut h = Harness::start_full(None, &[], &[(DICE, json!({"lang": "es"}))]).await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :!dado");
    h.ircd
        .expect("PRIVMSG #test :bob tira un dado de 6 caras: sale un ")
        .await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :!roll 3d8");
    let l = h
        .ircd
        .expect("PRIVMSG #test :bob tira 3 dados de 8 caras: ")
        .await;
    assert!(l.contains(" y ") && l.contains("(total "), "{l}");
    // In a private message the answer goes back to that person.
    h.ircd.send(":bob!b@fake PRIVMSG alice :!dice d20");
    h.ircd
        .expect("PRIVMSG bob :bob tira un dado de 20 caras: sale un ")
        .await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :!dado tres");
    h.ircd
        .expect("PRIVMSG #test :bob: no entiendo la tirada.")
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn autoresponder_words_and_private_auto_reply() {
    let mut h = Harness::start_full(
        None,
        &[],
        &[(AUTO, json!({"lang": "es", "cooldown": 0, "rules": "hola = ¡Hola {nick}! Bienvenido a {channel}.\n* = Ahora no estoy."}))],
    )
    .await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :Hola a todos");
    h.ircd
        .expect("PRIVMSG #test :¡Hola bob! Bienvenido a #test.")
        .await;
    // Whole words only.
    h.ircd.send(":bob!b@fake PRIVMSG #test :cholasa");
    h.ircd.send(":carol!c@fake PRIVMSG alice :¿estás?");
    let l = h.ircd.expect("PRIVMSG ").await;
    assert_eq!(l, "PRIVMSG carol :Ahora no estoy.");
    // Once per person every few minutes.
    h.ircd.send(":carol!c@fake PRIVMSG alice :¿hola?");
    h.ircd.send(":carol!c@fake PRIVMSG alice :¿sigues?");
    h.ircd.send(":bob!b@fake PRIVMSG #test :hola");
    let l = h.ircd.expect("PRIVMSG ").await;
    assert_eq!(
        l, "PRIVMSG carol :¡Hola carol! Bienvenido a .",
        "a keyword in a private message still answers"
    );
}
