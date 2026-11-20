/*
 * trivia: a quiz game for channels, with points and a ranking.
 *
 *   !trivia             starts a game (10 questions)
 *   !trivia stop        stops it
 *   !trivia top         the ranking
 * (also !trivial). Answer by writing in the channel. A hint appears
 * halfway; if nobody gets it in time, the answer is shown. Whoever answers
 * first gets a point.
 *
 * The owner adds questions with
 *   /msg *trivia add Question? | answer
 * and lists or removes them with /msg *trivia list and /msg *trivia del N.
 */
#define XBNC_ARENA_SIZE (512 * 1024)
#include "../xbnc_plugin.h"
#include "../xbnc_util.h"

typedef struct { const char *q; const char *a; } qa_t;

static const qa_t ES[] = {
    {"¿Cuál es la capital de Australia?", "Canberra"},
    {"¿Cuántos lados tiene un hexágono?", "6"},
    {"¿Qué planeta es conocido como el planeta rojo?", "Marte"},
    {"¿Quién escribió «Don Quijote de la Mancha»?", "Cervantes"},
    {"¿Cuál es el río más largo de España?", "Tajo"},
    {"¿Cuál es el símbolo químico del oro?", "Au"},
    {"¿En qué año llegó Colón a América?", "1492"},
    {"¿Cuál es el océano más grande?", "Pacífico"},
    {"¿Cuántos jugadores tiene un equipo de fútbol en el campo?", "11"},
    {"¿Qué instrumento tocaba Paco de Lucía?", "Guitarra"},
    {"¿Cuál es la montaña más alta del mundo?", "Everest"},
    {"¿Cómo se llama el idioma oficial de Brasil?", "Portugués"},
    {"¿Cuál es el animal terrestre más rápido?", "Guepardo"},
    {"¿Quién pintó «Las Meninas»?", "Velázquez"},
    {"¿Cuántos minutos tiene una hora?", "60"},
    {"¿Qué gas respiramos para vivir?", "Oxígeno"},
    {"¿Cuál es la capital de Canadá?", "Ottawa"},
    {"¿Cuál es el metal más ligero?", "Litio"},
    {"¿Qué ciudad es conocida como la Ciudad Condal?", "Barcelona"},
    {"¿Cuántos continentes hay?", "7"},
    {"¿Quién formuló la teoría de la relatividad?", "Einstein"},
    {"¿Cuál es la lengua más hablada del mundo como lengua materna?", "Chino"},
    {"¿En qué país están las pirámides de Giza?", "Egipto"},
    {"¿Cuál es el hueso más largo del cuerpo humano?", "Fémur"},
    {"¿Cómo se llama el satélite natural de la Tierra?", "Luna"},
    {"¿Qué protocolo usa el puerto 6667 por costumbre?", "IRC"},
    {"¿Cuál es la capital de Japón?", "Tokio"},
    {"¿Cuántas patas tiene una araña?", "8"},
    {"¿De qué color es la sangre de un pulpo?", "Azul"},
    {"¿Quién escribió «Cien años de soledad»?", "García Márquez"},
    {"¿Cuál es el país más grande del mundo?", "Rusia"},
    {"¿Cuál es el elemento más abundante del universo?", "Hidrógeno"},
    {"¿En qué ciudad se encuentra la torre Eiffel?", "París"},
    {"¿Cuántos años tiene un siglo?", "100"},
    {"¿Qué vitamina nos da el sol?", "D"},
    {"¿Cuál es la capital de Argentina?", "Buenos Aires"},
    {"¿Qué deporte practica Rafa Nadal?", "Tenis"},
    {"¿Cuál es el planeta más grande del sistema solar?", "Júpiter"},
    {"¿Cuántos grados tiene un ángulo recto?", "90"},
    {"¿Cómo se llama la red de IRC con el servicio X?", "Undernet"},
    {"¿Cuál es la capital de Italia?", "Roma"},
    {"¿Qué animal es el símbolo de la paz?", "Paloma"},
    {"¿Cuál es el desierto cálido más grande del mundo?", "Sahara"},
    {"¿Cuántos días tiene un año bisiesto?", "366"},
    {"¿Qué pintor cortó parte de su oreja?", "Van Gogh"},
    {"¿Cuál es la capital de México?", "Ciudad de México"},
    {"¿Qué órgano bombea la sangre?", "Corazón"},
    {"¿Cuántas cuerdas tiene una guitarra española?", "6"},
    {"¿Cómo se llama el autor de «La casa de Bernarda Alba»?", "Lorca"},
    {"¿Cuál es la moneda del Reino Unido?", "Libra"},
};

static const qa_t EN[] = {
    {"What is the capital of Australia?", "Canberra"},
    {"How many sides does a hexagon have?", "6"},
    {"Which planet is known as the red planet?", "Mars"},
    {"What is the chemical symbol for gold?", "Au"},
    {"What is the largest ocean?", "Pacific"},
    {"Who painted the Mona Lisa?", "Leonardo"},
    {"What is the tallest mountain in the world?", "Everest"},
    {"What is the fastest land animal?", "Cheetah"},
    {"How many continents are there?", "7"},
    {"Who developed the theory of relativity?", "Einstein"},
    {"In which country are the pyramids of Giza?", "Egypt"},
    {"What is the longest bone in the human body?", "Femur"},
    {"What is the capital of Japan?", "Tokyo"},
    {"How many legs does a spider have?", "8"},
    {"Which protocol traditionally uses port 6667?", "IRC"},
    {"What is the largest country in the world?", "Russia"},
    {"What is the largest planet in the solar system?", "Jupiter"},
    {"How many degrees are in a right angle?", "90"},
    {"Which IRC network has the X service?", "Undernet"},
    {"What is the currency of the United Kingdom?", "Pound"},
};

#define N_ES (int)(sizeof ES / sizeof ES[0])
#define N_EN (int)(sizeof EN / sizeof EN[0])

/* The game in progress (one per network). */
static char g_chan[64];
static int g_round, g_rounds, g_unanswered, g_hinted;
static xbnc_i64 g_asked, g_next; /* when asked; when to ask the next one (0: waiting answer) */
static char g_q[400], g_a[120], g_norm[120];
static unsigned long long rng;
static unsigned char used[256];

static unsigned int rnd(void) {
    if (!rng) rng = (unsigned long long)xbnc_now_ms() * 6364136223846793005ULL + 1;
    rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17;
    return (unsigned int)(rng >> 16);
}

static int custom_count(void) {
    static char keys[8192];
    xbnc_kv_list("q:", "", keys, sizeof keys);
    return u_count_keys(keys);
}

/* Picks a question: the owner's own ones first mixed with the built-in set. */
static void pick(void) {
    static char keys[8192], rec[600];
    int nc = custom_count();
    int nb = u_es ? N_ES : N_EN;
    int total = nb + nc;
    int idx = 0;
    for (int tries = 0; tries < 40; tries++) {
        idx = (int)(rnd() % (unsigned)total);
        if (idx >= 256 || !used[idx]) break;
    }
    if (idx < 256) used[idx] = 1;
    if (idx < nb) {
        const qa_t *t = u_es ? &ES[idx] : &EN[idx];
        u_copy(g_q, sizeof g_q, t->q);
        u_copy(g_a, sizeof g_a, t->a);
    } else {
        int want = idx - nb, i = 0;
        xbnc_kv_list("q:", "", keys, sizeof keys);
        char *cur = keys, *k;
        while ((k = u_next_key(&cur))) {
            if (i++ == want) {
                xbnc_kv_get_str(k, rec, sizeof rec);
                char *f[2];
                if (u_split(rec, '\t', f, 2) == 2) { u_copy(g_q, sizeof g_q, f[0]); u_copy(g_a, sizeof g_a, f[1]); }
                break;
            }
        }
    }
    u_norm(g_a, g_norm, sizeof g_norm);
}

static void ask(void) {
    pick();
    g_round++;
    g_asked = xbnc_now_ms();
    g_next = 0;
    g_hinted = 0;
    u_reset();
    u_say2("Pregunta ", "Question ");
    u_sayn(g_round); u_say("/"); u_sayn(g_rounds); u_say(": ");
    u_say(g_q);
    xbnc_privmsg(g_chan, u_out);
}

static void stop_game(const char *why) {
    if (why && g_chan[0]) xbnc_privmsg(g_chan, why);
    g_chan[0] = 0;
    for (int i = 0; i < 256; i++) used[i] = 0;
}

static xbnc_i64 points_of(const char *nick) {
    char key[48], f[40], v[24];
    u_fold(nick, f, sizeof f);
    key[0] = 0; xbnc_cat(key, sizeof key, "s:"); xbnc_cat(key, sizeof key, f);
    if (xbnc_kv_get_str(key, v, sizeof v) <= 0) return 0;
    return u_atoi(v);
}

static xbnc_i64 add_point(const char *nick) {
    char key[48], f[40], v[24];
    u_fold(nick, f, sizeof f);
    key[0] = 0; xbnc_cat(key, sizeof key, "s:"); xbnc_cat(key, sizeof key, f);
    xbnc_i64 p = points_of(nick) + 1;
    xbnc_itoa(p, v);
    xbnc_kv_set_str(key, v);
    /* Keep the nick as written for the ranking. */
    key[0] = 'n';
    xbnc_kv_set_str(key, nick);
    return p;
}

static void say_top(void) {
    static char keys[16384];
    char best[5][40];
    xbnc_i64 pts[5] = {0, 0, 0, 0, 0};
    for (int i = 0; i < 5; i++) best[i][0] = 0;
    xbnc_kv_list("s:", "", keys, sizeof keys);
    char *cur = keys, *k;
    while ((k = u_next_key(&cur))) {
        char v[24];
        if (xbnc_kv_get_str(k, v, sizeof v) <= 0) continue;
        xbnc_i64 p = u_atoi(v);
        char name[40], nk[48];
        nk[0] = 0; xbnc_cat(nk, sizeof nk, "n:"); xbnc_cat(nk, sizeof nk, k + 2);
        if (xbnc_kv_get_str(nk, name, sizeof name) <= 0) u_copy(name, sizeof name, k + 2);
        for (int i = 0; i < 5; i++) {
            if (p > pts[i]) {
                for (int j = 4; j > i; j--) { pts[j] = pts[j - 1]; u_copy(best[j], 40, best[j - 1]); }
                pts[i] = p; u_copy(best[i], 40, name);
                break;
            }
        }
    }
    u_reset();
    if (!pts[0]) { u_say2("Todavía no hay puntos.", "No points yet."); return; }
    u_say2("Ranking: ", "Ranking: ");
    for (int i = 0; i < 5 && pts[i]; i++) {
        if (i) u_say(", ");
        u_sayn(i + 1); u_say(". "); u_say(best[i]); u_say(" ("); u_sayn(pts[i]); u_say(")");
    }
}

static void hint(void) {
    char h[120];
    xbnc_u32 n = 0;
    int letter = 0;
    for (const char *p = g_a; *p && n + 4 < sizeof h; p++) {
        unsigned char c = (unsigned char)*p;
        if ((c & 0xC0) == 0x80) continue; /* rest of a UTF-8 character */
        if (c == ' ') { h[n++] = ' '; h[n++] = ' '; letter = 0; continue; }
        if (letter == 0) {
            /* Show the first letter of each word. */
            xbnc_u32 len = (c < 0x80) ? 1 : (c >= 0xF0 ? 4 : c >= 0xE0 ? 3 : 2);
            for (xbnc_u32 i = 0; i < len && p[i]; i++) h[n++] = p[i];
            h[n++] = ' ';
        } else { h[n++] = '_'; h[n++] = ' '; }
        letter++;
    }
    h[n] = 0;
    u_reset();
    u_say2("Pista: ", "Hint: ");
    u_say(h);
    xbnc_privmsg(g_chan, u_out);
}

static void tick(const xbnc_event *ev) {
    if (!g_chan[0]) return;
    xbnc_i64 now = xbnc_now_ms();
    xbnc_i64 secs = xbnc_config_int(ev, "seconds", 40);
    if (g_next) {
        if (now >= g_next) ask();
        return;
    }
    if (!g_hinted && now - g_asked >= secs * 500) { g_hinted = 1; hint(); return; }
    if (now - g_asked >= secs * 1000) {
        u_reset();
        u_say2("Nadie lo ha acertado. La respuesta era: ", "Nobody got it. The answer was: ");
        u_say(g_a);
        xbnc_privmsg(g_chan, u_out);
        if (++g_unanswered >= 3) { stop_game(u_es ? "Partida terminada: tres preguntas sin respuesta." : "Game over: three questions in a row unanswered."); return; }
        if (g_round >= g_rounds) { stop_game(u_es ? "Fin de la partida. Escribe !trivial top para ver el ranking." : "Game over. Write !trivia top for the ranking."); return; }
        g_next = now + 5000;
    }
}

static void command(const xbnc_event *ev, const char *text) {
    char w[16], rest[512];
    const char *r = u_word(text, w, sizeof w);
    u_copy(rest, sizeof rest, r);
    u_reset();
    if (u_eq(w, "add") || u_eq(w, "añadir")) {
        char *bar = 0;
        for (char *p = rest; *p; p++) if (*p == '|') { bar = p; break; }
        if (!bar) { xbnc_tell(u_es ? "Uso: add ¿Pregunta? | respuesta" : "Usage: add Question? | answer"); return; }
        *bar = 0;
        char *q = rest, *a = (char *)u_skip_spaces(bar + 1);
        xbnc_u32 ql = xbnc_strlen(q);
        while (ql && q[ql - 1] == ' ') q[--ql] = 0;
        if (!q[0] || !a[0]) { xbnc_tell(u_es ? "Faltan la pregunta o la respuesta." : "The question or the answer is missing."); return; }
        if (custom_count() >= 2000) { xbnc_tell(u_es ? "Ya hay 2000 preguntas propias." : "There are already 2000 questions."); return; }
        char key[24], rec[600];
        u_copy(key, sizeof key, "q:");
        u_pad(xbnc_now_ms(), key + 2, 15);
        rec[0] = 0;
        u_field(rec, sizeof rec, q, 0);
        u_field(rec, sizeof rec, a, 1);
        xbnc_kv_set_str(key, rec);
        u_say2("Pregunta añadida. Tienes ", "Question added. You have ");
        u_sayn(custom_count());
        u_say2(" preguntas propias.", " questions of your own.");
        xbnc_tell(u_out);
    } else if (u_eq(w, "list") || u_eq(w, "lista")) {
        static char keys[8192], rec[600];
        xbnc_kv_list("q:", "", keys, sizeof keys);
        char *cur = keys, *k;
        int i = 0, shown = 0;
        while ((k = u_next_key(&cur)) && shown < 5) {
            i++;
            if (xbnc_kv_get_str(k, rec, sizeof rec) <= 0) continue;
            u_reset(); u_sayn(i); u_say(". ");
            for (char *p = rec; *p; p++) if (*p == '\t') *p = '|';
            u_say(rec);
            xbnc_tell(u_out);
            shown++;
        }
        if (!i) xbnc_tell(u_es ? "No hay preguntas propias. Añade con: add ¿Pregunta? | respuesta" : "No questions of your own. Add with: add Question? | answer");
    } else if (u_eq(w, "del") || u_eq(w, "borrar")) {
        static char keys[8192];
        xbnc_i64 n = u_atoi(rest);
        xbnc_kv_list("q:", "", keys, sizeof keys);
        char *cur = keys, *k;
        int i = 0;
        while ((k = u_next_key(&cur))) if (++i == n) { xbnc_kv_del(k); xbnc_tell(u_es ? "Borrada." : "Deleted."); return; }
        xbnc_tell(u_es ? "No existe esa pregunta." : "No such question.");
    } else if (u_eq(w, "top")) {
        say_top();
        xbnc_tell(u_out);
    } else {
        char trig[32];
        xbnc_config(ev, "trigger", trig, sizeof trig);
        u_say2("En un canal: ", "In a channel: ");
        const char *t = trig[0] ? trig : (u_es ? "!trivial" : "!trivia");
        u_say(t);
        u_say2(" para jugar, ", " to play, ");
        u_say(t);
        u_say2(" stop para parar y ", " stop to stop and ");
        u_say(t);
        u_say2(" top para el ranking. Aquí: add ¿Pregunta? | respuesta, list, del N.",
               " top for the ranking. Here: add Question? | answer, list, del N.");
        xbnc_tell(u_out);
    }
}

void on_event(const xbnc_event *ev) {
    static char kind[16], text[512], from[64], conv[64], trigger[32];
    xbnc_get(ev, "event", kind, sizeof kind);
    u_lang(ev);
    if (u_eq(kind, "timer")) { tick(ev); return; }
    if (u_eq(kind, "connected")) { g_chan[0] = 0; return; }
    if (u_eq(kind, "command")) { xbnc_get(ev, "text", text, sizeof text); command(ev, text); return; }
    if (!u_eq(kind, "message") || !xbnc_is(ev, "channel")) return;
    xbnc_get(ev, "text", text, sizeof text);
    xbnc_get(ev, "conversation", conv, sizeof conv);
    xbnc_get(ev, xbnc_is(ev, "outgoing") ? "nick" : "from", from, sizeof from);
    xbnc_config(ev, "trigger", trigger, sizeof trigger);
    if (!trigger[0]) u_copy(trigger, sizeof trigger, "!trivia");
    const char *rest = u_command(text, trigger);
    if (!rest) rest = u_command(text, "!trivia");
    if (!rest) rest = u_command(text, "!trivial");
    if (rest) {
        if (!u_in_list(ev, "channels", conv, 1)) return;
        char w[16];
        u_word(rest, w, sizeof w);
        if (u_eq(w, "top") || u_eq(w, "ranking")) { say_top(); xbnc_reply(u_out); return; }
        if (u_eq(w, "stop") || u_eq(w, "parar")) {
            if (g_chan[0] && u_eq_fold(g_chan, conv)) stop_game(u_es ? "Partida parada." : "Game stopped.");
            return;
        }
        if (g_chan[0]) {
            u_reset();
            u_say2("Ya hay una partida en ", "A game is already running in ");
            u_say(g_chan);
            xbnc_reply(u_out);
            return;
        }
        u_copy(g_chan, sizeof g_chan, conv);
        g_round = 0; g_unanswered = 0;
        g_rounds = (int)xbnc_config_int(ev, "rounds", 10);
        u_reset();
        u_say2("¡Empieza el trivial! ", "Trivia starts! ");
        u_sayn(g_rounds);
        u_say2(" preguntas; responde escribiendo en el canal.", " questions; answer by writing in the channel.");
        xbnc_reply(u_out);
        ask();
        return;
    }
    /* An answer? */
    if (!g_chan[0] || g_next || !u_eq_fold(g_chan, conv)) return;
    char norm[160];
    u_norm(text, norm, sizeof norm);
    if (!norm[0] || !u_eq(norm, g_norm)) return;
    xbnc_i64 p = add_point(from);
    g_unanswered = 0;
    u_reset();
    u_say2("¡Correcto, ", "Correct, ");
    u_say(from);
    u_say2("! Era «", "! It was \"");
    u_say(g_a);
    u_say2("». Llevas ", "\". You have ");
    u_sayn(p);
    u_say(p == 1 ? (u_es ? " punto." : " point.") : (u_es ? " puntos." : " points."));
    xbnc_reply(u_out);
    if (g_round >= g_rounds) {
        say_top();
        xbnc_privmsg(g_chan, u_out);
        stop_game(u_es ? "Fin de la partida." : "Game over.");
        return;
    }
    g_next = xbnc_now_ms() + 4000;
}
