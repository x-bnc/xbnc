//! The v1.0 official plugins running in a real bouncer against the scripted
//! IRC server. (The web plugins, urltitle, weather and news, are checked
//! for their requests; their answers need the real sites.)

mod common;
use common::*;
use serde_json::json;
use std::time::Duration;

const TRIVIA: &[u8] = include_bytes!("../../../sdk/c/examples/trivia.wasm");
const GREET: &[u8] = include_bytes!("../../../sdk/c/examples/greet.wasm");
const FACTOIDS: &[u8] = include_bytes!("../../../sdk/c/examples/factoids.wasm");
const KARMA: &[u8] = include_bytes!("../../../sdk/c/examples/karma.wasm");
const QUOTES: &[u8] = include_bytes!("../../../sdk/c/examples/quotes.wasm");
const BADWORDS: &[u8] = include_bytes!("../../../sdk/c/examples/badwords.wasm");
const GAMES: &[u8] = include_bytes!("../../../sdk/c/examples/games.wasm");
const URLTITLE: &[u8] = include_bytes!("../../../sdk/c/examples/urltitle.wasm");
const WEATHER: &[u8] = include_bytes!("../../../sdk/c/examples/weather.wasm");
const NEWS: &[u8] = include_bytes!("../../../sdk/c/examples/news.wasm");

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn trivia_asks_and_scores() {
    std::env::set_var("XBNC_PLUGIN_TIMER_SECS", "1");
    let mut h =
        Harness::start_full(None, &[], &[(TRIVIA, json!({"lang": "es", "rounds": 1}))]).await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :!trivial");
    h.ircd
        .expect("PRIVMSG #test :¡Empieza el trivial! 1 preguntas")
        .await;
    let q = h.ircd.expect("PRIVMSG #test :Pregunta 1/1: ").await;
    assert!(q.contains('?'), "{q}");
    h.ircd
        .send(":bob!b@fake PRIVMSG #test :respuesta equivocada seguro");
    h.ircd.send(":bob!b@fake PRIVMSG #test :!trivial");
    h.ircd
        .expect("PRIVMSG #test :Ya hay una partida en #test")
        .await;
    // The English default command works as well as the Spanish one.
    h.ircd.send(":bob!b@fake PRIVMSG #test :!trivia");
    h.ircd
        .expect("PRIVMSG #test :Ya hay una partida en #test")
        .await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :!trivial stop");
    h.ircd.expect("PRIVMSG #test :Partida parada.").await;
    // Own question, answered with different case and accents.
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    c.send("PRIVMSG *trivia :add ¿Color del cielo? | Azúl")
        .await;
    c.read_until("Pregunta añadida").await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :!trivial top");
    h.ircd.expect("PRIVMSG #test :Todavía no hay puntos.").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn greet_welcomes_first_time_and_returning_people() {
    let mut h = Harness::start_full(
        None,
        &[],
        &[(
            GREET,
            json!({"lang": "es", "first": "¡Hola {nick}, bienvenida a {channel}!", "message": "De vuelta, {nick}.",
                   "cooldown": 0, "personal": "carol Llegó la jefa."}),
        )],
    )
    .await;
    h.ircd.send(":dana!d@fake JOIN #test");
    h.ircd
        .expect("PRIVMSG #test :¡Hola dana, bienvenida a #test!")
        .await;
    h.ircd.send(":dana!d@fake PART #test");
    h.ircd.send(":dana!d@fake JOIN #test");
    h.ircd.expect("PRIVMSG #test :De vuelta, dana.").await;
    h.ircd.send(":carol!c@fake JOIN #test");
    h.ircd.expect("PRIVMSG #test :Llegó la jefa.").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn factoids_karma_quotes_and_games() {
    let mut h = Harness::start_full(
        None,
        &[],
        &[
            (FACTOIDS, json!({"lang": "es", "editors": "bob"})),
            (KARMA, json!({"lang": "es"})),
            (QUOTES, json!({"lang": "es"})),
            (GAMES, json!({"lang": "es"})),
        ],
    )
    .await;
    h.ircd
        .send(":bob!b@fake PRIVMSG #test :!def normas = no spam, respeto");
    h.ircd.expect("PRIVMSG #test :Guardado: normas").await;
    h.ircd.send(":carol!c@fake PRIVMSG #test :?Normas");
    h.ircd
        .expect("PRIVMSG #test :Normas: no spam, respeto")
        .await;
    h.ircd
        .send(":carol!c@fake PRIVMSG #test :!def normas = otra cosa");
    h.ircd
        .expect("PRIVMSG #test :carol: no puedes cambiar las definiciones.")
        .await;

    h.ircd.send(":carol!c@fake PRIVMSG #test :bob++ gracias");
    h.ircd.expect("PRIVMSG #test :bob tiene 1 de karma.").await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :bob++");
    h.ircd
        .expect("PRIVMSG #test :bob: no puedes darte karma a ti mismo.")
        .await;
    h.ircd.send(":carol!c@fake PRIVMSG #test :!karma top");
    h.ircd.expect("PRIVMSG #test :Más karma: bob (1)").await;

    h.ircd
        .send(":bob!b@fake PRIVMSG #test :!cita añadir <ana> esto es historia");
    h.ircd.expect("PRIVMSG #test :Cita 1 guardada.").await;
    h.ircd
        .send(":bob!b@fake PRIVMSG #test :!quote search historia");
    h.ircd
        .expect("PRIVMSG #test :#1: <ana> esto es historia")
        .await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :!cita 1");
    h.ircd
        .expect("PRIVMSG #test :#1: <ana> esto es historia")
        .await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :!quote 1");
    h.ircd
        .expect("PRIVMSG #test :#1: <ana> esto es historia")
        .await;

    h.ircd
        .send(":bob!b@fake PRIVMSG #test :!8ball ¿funcionará?");
    h.ircd.expect("PRIVMSG #test :La bola 8 dice: «").await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :!ppt piedra");
    let l = h.ircd.expect("PRIVMSG #test :piedra contra ").await;
    assert!(
        l.contains("empate") || l.contains("ganas") || l.contains("pierdes"),
        "{l}"
    );
    h.ircd.send(":bob!b@fake PRIVMSG #test :!ahorcado");
    let l = h.ircd.expect("PRIVMSG #test :_").await;
    assert!(l.contains("(6 vidas)"), "{l}");
    h.ircd.send(":bob!b@fake PRIVMSG #test :!a zzzzzzzzzz");
    h.ircd.expect("PRIVMSG #test :No es esa.").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn badwords_warns_kicks_and_bans() {
    let mut h = Harness::start_full(
        None,
        &[],
        &[(
            BADWORDS,
            json!({"lang": "es", "words": "tont*\ncaca", "ban_minutes": 5}),
        )],
    )
    .await;
    h.ircd
        .send(":troll!t@bad.example PRIVMSG #test :eres TONTÍSIMO");
    h.ircd
        .expect("PRIVMSG #test :troll: esa palabra no está permitida aquí.")
        .await;
    h.ircd.send(":troll!t@bad.example PRIVMSG #test :caca");
    h.ircd
        .expect("KICK #test troll :Lenguaje no permitido")
        .await;
    h.ircd
        .send(":troll!t@bad.example PRIVMSG #test :Caca otra vez");
    h.ircd.expect("MODE #test +b *!*@bad.example").await;
    h.ircd.expect("KICK #test troll").await;
    // Whole words: "cacahuete" is fine.
    h.ircd
        .send(":ana!a@fake PRIVMSG #test :me gusta el cacahuete");
    tokio::time::sleep(Duration::from_millis(400)).await;
    while let Ok(l) = h.ircd.received.try_recv() {
        assert!(!l.contains("ana"), "{l}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn web_plugins_only_reach_approved_sites() {
    // Not approved: the request is refused inside xBNC and nothing is said.
    let mut h = Harness::start_full(
        None,
        &[],
        &[
            (URLTITLE, json!({"lang": "es"})),
            (WEATHER, json!({"lang": "es"})),
            (
                NEWS,
                json!({"lang": "es", "feeds": "#test https://hnrss.org/frontpage"}),
            ),
        ],
    )
    .await;
    h.ircd
        .send(":bob!b@fake PRIVMSG #test :mira https://www.youtube.com/watch?v=abc");
    h.ircd.send(":bob!b@fake PRIVMSG #test :!tiempo Madrid");
    h.ircd
        .expect("PRIVMSG #test :No se pudo consultar el tiempo ahora mismo.")
        .await;
    // The plugin answers one question every 3 seconds.
    tokio::time::sleep(Duration::from_millis(3100)).await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :!weather London");
    h.ircd
        .expect("PRIVMSG #test :No se pudo consultar el tiempo ahora mismo.")
        .await;
    let (mut c, _) = Client::login(h.addr, "alice/fake@laptop").await;
    c.send("PRIVMSG *news :list").await;
    c.read_until("#test https://hnrss.org/frontpage").await;
}
