//! WebAssembly plugins running inside a real bouncer against the scripted
//! IRC server, using the C SDK examples.

mod common;
use common::*;
use serde_json::json;

const DICE: &[u8] = include_bytes!("../../../sdk/c/examples/dice.wasm");
const AUTORESPONDER: &[u8] = include_bytes!("../../../sdk/c/examples/autoresponder.wasm");

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plugins_answer_on_the_network_and_to_the_user() {
    let mut h = Harness::start_full(
        None,
        &[],
        &[
            (DICE, json!({"trigger": "!roll"})),
            (
                AUTORESPONDER,
                json!({"rules": "hola = ¡Hola, {nick}!", "cooldown": 0}),
            ),
        ],
    )
    .await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;

    // A message from someone in the channel triggers the dice plugin...
    h.ircd.send(":bob!b@fake PRIVMSG #test :!roll 2d6");
    let l = h
        .ircd
        .expect("PRIVMSG #test :bob rolls 2 6-sided dice: ")
        .await;
    assert!(l.contains("(total "), "{l}");
    // ...and the user's client sees what was sent on their behalf.
    c.read_until("bob rolls 2 6-sided dice").await;

    // Keywords: the autoresponder answers in the same channel.
    h.ircd.send(":carol!c@fake PRIVMSG #test :hola gente");
    h.ircd.expect("PRIVMSG #test :¡Hola, carol!").await;

    // /msg *autoresponder talks to the plugin; it answers only to the user.
    c.send("PRIVMSG *autoresponder :list").await;
    let lines = c.read_until(":*autoresponder!xbnc@xbnc PRIVMSG").await;
    assert!(lines.last().unwrap().contains("Rules: hola."), "{lines:?}");

    // *status lists plugins and can turn them off.
    c.send("PRIVMSG *status :modules").await;
    c.read_until("dice").await;
    c.send("PRIVMSG *status :disable dice").await;
    c.read_until("disabled").await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :!roll 2d6");
    h.ircd.send(":bob!b@fake PRIVMSG #test :hola otra vez");
    // The autoresponder still answers; the disabled dice plugin does not.
    let l = h.ircd.expect("PRIVMSG #test :").await;
    assert!(l.contains("¡Hola, bob!"), "{l}");
}

const SEEN: &[u8] = include_bytes!("../../../sdk/c/examples/seen.wasm");

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn seen_plugin_tracks_presence_and_last_activity() {
    let mut h =
        Harness::start_full(None, &[], &[(SEEN, json!({"cooldown": 0, "lang": "es"}))]).await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;

    // bob is in the NAMES list of #test: he is here right now.
    h.ircd.send(":carol!c@fake PRIVMSG #test :!seen bob");
    h.ircd
        .expect("PRIVMSG #test :bob está ahora en #test.")
        .await;

    // What he last said is remembered, also after he quits.
    h.ircd.send(":bob!b@fake PRIVMSG #test :me voy a cenar");
    h.ircd.send(":carol!c@fake PRIVMSG #test :!seen bob");
    let l = h
        .ircd
        .expect("PRIVMSG #test :bob está ahora en #test. Última actividad hace")
        .await;
    assert!(
        l.ends_with("escribiendo en #test: «me voy a cenar»."),
        "{l}"
    );
    h.ircd.send(":bob!b@fake QUIT :Hasta mañana");
    h.ircd.send(":carol!c@fake PRIVMSG #test :!seen BOB");
    let l = h
        .ircd
        .expect("PRIVMSG #test :bob fue visto por última vez hace")
        .await;
    assert!(l.ends_with("desconectándose (Hasta mañana)."), "{l}");

    // Join and part, kick, nick changes.
    h.ircd.send(":dave!d@fake JOIN #test");
    h.ircd.send(":dave!d@fake PART #test :adiós");
    h.ircd.send(":carol!c@fake PRIVMSG #test :!seen dave");
    let l = h.ircd.expect("PRIVMSG #test :dave fue visto").await;
    assert!(l.ends_with("saliendo de #test (adiós)."), "{l}");
    h.ircd.send(":carol!c@fake NICK carola");
    h.ircd.send(":carola!c@fake PRIVMSG #test :!seen carol");
    let l = h.ircd.expect("PRIVMSG #test :carol fue visto").await;
    assert!(l.ends_with("cambiando su nick a carola."), "{l}");
    h.ircd.send(":carola!c@fake KICK #test erin :fuera");
    h.ircd.send(":carola!c@fake PRIVMSG #test :!seen nadie");
    h.ircd.expect("PRIVMSG #test :No he visto a nadie.").await;

    // Private messages are neither stored nor answered.
    h.ircd.send(":zoe!z@fake PRIVMSG alice :!seen bob");
    h.ircd.send(":carola!c@fake PRIVMSG #test :!seen zoe");
    h.ircd.expect("PRIVMSG #test :No he visto a zoe.").await;

    // The user asks privately; the answer is only for them.
    c.send("PRIVMSG *seen :dave").await;
    let lines = c.read_until(":*seen!xbnc@xbnc PRIVMSG").await;
    assert!(
        lines.last().unwrap().contains("dave fue visto"),
        "{lines:?}"
    );

    // English answers.
    c.send("PRIVMSG *status :set seen lang en").await;
    c.read_until("Saved.").await;
    h.ircd.send(":carola!c@fake PRIVMSG #test :!seen dave");
    let l = h.ircd.expect("PRIVMSG #test :dave was last seen").await;
    assert!(l.ends_with("ago, leaving #test (adiós)."), "{l}");

    // What it knows is in the plugin storage, one record per nick.
    let rec: Vec<u8> =
        h.db.with(|c| {
            Ok(c.query_row(
                "SELECT value FROM plugin_kv WHERE plugin_id = 'seen' AND key = 'u:dave'",
                (),
                |r| r.get(0),
            )?)
        })
        .unwrap();
    let rec = String::from_utf8(rec).unwrap();
    assert!(rec.contains("\tp\tdave\t#test\t\tadiós"), "{rec}");

    // After a restart of the plugin (reload), it still remembers.
    c.send("PRIVMSG *status :disable seen").await;
    c.read_until("disabled").await;
    c.send("PRIVMSG *status :enable seen").await;
    c.read_until("enabled").await;
    c.send("PRIVMSG *seen :dave").await;
    let lines = c.read_until(":*seen!xbnc@xbnc PRIVMSG").await;
    assert!(
        lines.last().unwrap().contains("dave was last seen"),
        "{lines:?}"
    );
}
