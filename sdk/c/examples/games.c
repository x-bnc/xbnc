/*
 * games: small channel games.
 *
 *   !8ball will it rain tomorrow?   ->  The 8 ball says: "Don't count on it".
 *   !rps rock                       ->  Rock against scissors: you win, bob!
 *   !hangman                        ->  starts hangman: _ _ _ _ _ _ (6 lives)
 *   !g e   /   !g word              ->  guess a letter or the whole word
 * In Spanish: !bola8, !ppt piedra, !ahorcado and !a.
 */
#include "../xbnc_plugin.h"
#include "../xbnc_util.h"

static unsigned long long rng;

static unsigned int rnd(void) {
    if (!rng) rng = (unsigned long long)xbnc_now_ms() * 6364136223846793005ULL + 1;
    rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17;
    return (unsigned int)(rng >> 16);
}

static const char *BALL_ES[] = {
    "Sí, sin duda", "Es seguro", "Puedes contar con ello", "Lo más probable", "Pinta bien",
    "Pregunta más tarde", "Mejor no te lo digo ahora", "No puedo predecirlo", "Concéntrate y vuelve a preguntar",
    "No cuentes con ello", "Mi respuesta es no", "Mis fuentes dicen que no", "Muy dudoso", "Ni lo sueñes",
};
static const char *BALL_EN[] = {
    "It is certain", "Without a doubt", "You may rely on it", "Most likely", "Outlook good",
    "Ask again later", "Better not tell you now", "Cannot predict now", "Concentrate and ask again",
    "Don't count on it", "My reply is no", "My sources say no", "Very doubtful", "Not a chance",
};

static const char *WORDS_ES[] = {
    "bouncer", "servidor", "teclado", "canal", "mensaje", "conexion", "pantalla", "ordenador", "internet",
    "biblioteca", "murcielago", "guitarra", "montaña", "chocolate", "elefante", "mariposa", "bicicleta",
    "televisor", "frigorifico", "paraguas", "calendario", "aventura", "tormenta", "helicoptero", "dinosaurio",
    "astronauta", "programa", "contraseña", "navegador", "ventilador",
};
static const char *WORDS_EN[] = {
    "bouncer", "server", "keyboard", "channel", "message", "connection", "screen", "computer", "internet",
    "library", "guitar", "mountain", "chocolate", "elephant", "butterfly", "bicycle", "umbrella", "calendar",
    "adventure", "thunderstorm", "helicopter", "dinosaur", "astronaut", "program", "password", "browser",
};

/* Hangman (one game per network). */
static char h_chan[64], h_word[40], h_used[64];
static int h_lives;
static xbnc_i64 h_last;

/* Letters of the word as bytes; "ñ" counts as one letter (2 bytes). */
static void show_board(void) {
    u_reset();
    for (const char *p = h_word; *p;) {
        int len = ((unsigned char)*p >= 0xC0) ? 2 : 1;
        char letter[3] = {p[0], len == 2 ? p[1] : 0, 0};
        int known = xbnc_find_ci(h_used, letter) != 0;
        if (known) u_say(letter); else u_say("_");
        p += len;
        if (*p) u_say(" ");
    }
    u_say("  (");
    u_sayn(h_lives);
    u_say(h_lives == 1 ? (u_es ? " vida" : " life") : (u_es ? " vidas" : " lives"));
    if (h_used[0]) { u_say2(", usadas: ", ", used: "); u_say(h_used); }
    u_say(")");
}

static int solved(void) {
    for (const char *p = h_word; *p;) {
        int len = ((unsigned char)*p >= 0xC0) ? 2 : 1;
        char letter[3] = {p[0], len == 2 ? p[1] : 0, 0};
        if (!xbnc_find_ci(h_used, letter)) return 0;
        p += len;
    }
    return 1;
}

static void hangman(const char *conv, const char *from, const char *arg, int start) {
    if (start) {
        if (h_chan[0] && xbnc_now_ms() - h_last < 600000) {
            u_reset(); u_say2("Ya hay un ahorcado en ", "Hangman is already running in "); u_say(h_chan);
            xbnc_reply(u_out);
            return;
        }
        const char **list = u_es ? WORDS_ES : WORDS_EN;
        int n = u_es ? (int)(sizeof WORDS_ES / sizeof WORDS_ES[0]) : (int)(sizeof WORDS_EN / sizeof WORDS_EN[0]);
        u_copy(h_word, sizeof h_word, list[rnd() % (unsigned)n]);
        u_copy(h_chan, sizeof h_chan, conv);
        h_used[0] = 0;
        h_lives = 6;
        h_last = xbnc_now_ms();
        show_board();
        xbnc_reply(u_out);
        return;
    }
    if (!h_chan[0] || !u_eq_fold(h_chan, conv)) return;
    h_last = xbnc_now_ms();
    char guess[40];
    u_norm(arg, guess, sizeof guess);
    if (!guess[0]) return;
    xbnc_u32 gl = xbnc_strlen(guess);
    int one_letter = gl == 1 || (gl == 2 && (unsigned char)guess[0] >= 0xC0);
    if (!one_letter) {
        char w[40];
        u_norm(h_word, w, sizeof w);
        u_reset();
        if (u_eq(w, guess)) {
            u_say2("¡Correcto, ", "Correct, "); u_say(from); u_say2("! Era «", "! It was \""); u_say(h_word); u_say2("».", "\".");
            h_chan[0] = 0;
        } else {
            h_lives--;
            if (h_lives <= 0) { u_say2("¡Ahorcado! La palabra era «", "Hanged! The word was \""); u_say(h_word); u_say2("».", "\"."); h_chan[0] = 0; }
            else { u_say2("No es esa. ", "Not that one. "); xbnc_reply(u_out); show_board(); }
        }
        xbnc_reply(u_out);
        return;
    }
    if (xbnc_find_ci(h_used, guess)) { u_reset(); u_say2("Esa letra ya se ha dicho.", "That letter was already said."); xbnc_reply(u_out); return; }
    if (h_used[0]) xbnc_cat(h_used, sizeof h_used, ",");
    xbnc_cat(h_used, sizeof h_used, guess);
    char w[40];
    u_norm(h_word, w, sizeof w);
    /* Normalised so "a" also finds "á". */
    if (!xbnc_find_ci(w, guess) && !xbnc_find_ci(h_word, guess)) h_lives--;
    else {
        /* Mark the accented forms as found too. */
        for (const char *p = h_word; *p;) {
            int len = ((unsigned char)*p >= 0xC0) ? 2 : 1;
            char letter[3] = {p[0], len == 2 ? p[1] : 0, 0}, nl[4];
            u_norm(letter, nl, sizeof nl);
            if (u_eq(nl, guess) && !xbnc_find_ci(h_used, letter)) { xbnc_cat(h_used, sizeof h_used, ","); xbnc_cat(h_used, sizeof h_used, letter); }
            p += len;
        }
    }
    if (solved()) {
        u_reset(); u_say2("¡Resuelto por ", "Solved by "); u_say(from); u_say2("! Era «", "! It was \""); u_say(h_word); u_say2("».", "\".");
        h_chan[0] = 0;
    } else if (h_lives <= 0) {
        u_reset(); u_say2("¡Ahorcado! La palabra era «", "Hanged! The word was \""); u_say(h_word); u_say2("».", "\".");
        h_chan[0] = 0;
    } else show_board();
    xbnc_reply(u_out);
}

static void rps(const char *from, const char *arg) {
    static const char *ES[] = {"piedra", "papel", "tijera"};
    static const char *EN[] = {"rock", "paper", "scissors"};
    char a[16];
    u_norm(arg, a, sizeof a);
    int me = -1;
    for (int i = 0; i < 3; i++) if (u_eq(a, ES[i]) || u_eq(a, EN[i]) || (a[0] && !a[1] && a[0] == ES[i][0])) me = i;
    if (a[0] == 't' && !a[1]) me = 2;
    u_reset();
    if (me < 0) { u_say2("Elige piedra, papel o tijera: !ppt piedra", "Pick rock, paper or scissors: !rps rock"); return; }
    int it = (int)(rnd() % 3);
    const char **n = u_es ? ES : EN;
    u_say(n[me]); u_say2(" contra ", " against "); u_say(n[it]); u_say(": ");
    if (me == it) u_say2("empate.", "a draw.");
    else if ((me + 3 - it) % 3 == 1) { u_say2("¡ganas, ", "you win, "); u_say(from); u_say("!"); }
    else { u_say2("pierdes, ", "you lose, "); u_say(from); u_say("."); }
}

void on_event(const xbnc_event *ev) {
    static char kind[16], text[512], from[64], conv[64];
    xbnc_get(ev, "event", kind, sizeof kind);
    u_lang(ev);
    if (u_eq(kind, "command")) {
        xbnc_tell(u_es ? "En un canal: !8ball pregunta, !ppt piedra|papel|tijera, !ahorcado para empezar y !a letra (o la palabra) para jugar."
                       : "In a channel: !8ball question, !rps rock|paper|scissors, !hangman to start and !g letter (or the word) to play.");
        return;
    }
    if (!u_eq(kind, "message") || !xbnc_is(ev, "channel")) return;
    xbnc_get(ev, "text", text, sizeof text);
    xbnc_get(ev, "conversation", conv, sizeof conv);
    xbnc_get(ev, xbnc_is(ev, "outgoing") ? "nick" : "from", from, sizeof from);
    if (!u_in_list(ev, "channels", conv, 1)) return;
    const char *r;
    if ((r = u_command(text, "!8ball")) || (r = u_command(text, "!bola8"))) {
        u_reset();
        if (!*r) { u_say2("Hazme una pregunta: !8ball ¿lloverá mañana?", "Ask me something: !8ball will it rain tomorrow?"); xbnc_reply(u_out); return; }
        int n = (int)(sizeof BALL_ES / sizeof BALL_ES[0]);
        u_say2("La bola 8 dice: «", "The 8 ball says: \"");
        u_say((u_es ? BALL_ES : BALL_EN)[rnd() % (unsigned)n]);
        u_say2("».", "\".");
        xbnc_reply(u_out);
        return;
    }
    if ((r = u_command(text, "!ppt")) || (r = u_command(text, "!rps"))) { rps(from, r); xbnc_reply(u_out); return; }
    if (u_command(text, "!ahorcado") || u_command(text, "!hangman")) { hangman(conv, from, "", 1); return; }
    if ((r = u_command(text, "!a")) || (r = u_command(text, "!g"))) { hangman(conv, from, r, 0); return; }
}
