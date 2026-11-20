/*
 * quotes: memorable lines of the channel.
 *
 *   !quote add <bob> this is history   ->  Quote 12 saved.
 *   !quote                             ->  #7: <ana> ...
 *   !quote 12                          ->  that one
 *   !quote search history              ->  the latest one with the word
 *   !quote del 12                      ->  deletes it (you and editors)
 * In Spanish: !cita añadir / !cita / !cita buscar / !cita borrar.
 */
#include "../xbnc_plugin.h"
#include "../xbnc_util.h"

static unsigned long long rng;

static unsigned int rnd(void) {
    if (!rng) rng = (unsigned long long)xbnc_now_ms() * 6364136223846793005ULL + 1;
    rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17;
    return (unsigned int)(rng >> 16);
}

static void qkey(xbnc_i64 n, char *key) {
    key[0] = 'q'; key[1] = ':';
    u_pad(n, key + 2, 8);
}

static xbnc_i64 counter(void) {
    char v[24];
    return xbnc_kv_get_str("n", v, sizeof v) > 0 ? u_atoi(v) : 0;
}

/* Shows quote n (record "who\ttime\ttext"). Returns 0 if it does not exist. */
static int show(xbnc_i64 n) {
    static char rec[600];
    char key[16];
    qkey(n, key);
    if (xbnc_kv_get_str(key, rec, sizeof rec) <= 0) return 0;
    char *f[3];
    if (u_split(rec, '\t', f, 3) < 3) return 0;
    u_reset();
    u_say("#"); u_sayn(n); u_say(": "); u_say(f[2]);
    return 1;
}

static void handle(const xbnc_event *ev, const char *rest, const char *from, int owner) {
    static char keys[65536];
    char w[16];
    const char *arg = u_word(rest, w, sizeof w);
    u_reset();
    if (u_eq(w, "añadir") || u_eq(w, "add") || u_eq(w, "anadir")) {
        if (!*arg) { u_say2("Uso: !cita añadir <texto>", "Usage: !quote add <text>"); return; }
        if (!owner && !xbnc_config_bool(ev, "anyone_adds") && !u_in_list(ev, "editors", from, 0)) {
            u_say(from); u_say2(": no puedes añadir citas.", ": you cannot add quotes."); return;
        }
        xbnc_i64 n = counter() + 1;
        char key[16], v[24], rec[600];
        qkey(n, key);
        rec[0] = 0;
        u_field(rec, sizeof rec, from, 0);
        u_fieldn(rec, sizeof rec, xbnc_now_ms(), 0);
        u_field(rec, sizeof rec, arg, 1);
        xbnc_kv_set_str(key, rec);
        xbnc_itoa(n, v);
        xbnc_kv_set_str("n", v);
        u_say2("Cita ", "Quote ");
        u_sayn(n);
        u_say2(" guardada.", " saved.");
        return;
    }
    if (u_eq(w, "borrar") || u_eq(w, "del") || u_eq(w, "delete")) {
        if (!owner && !u_in_list(ev, "editors", from, 0)) { u_say(from); u_say2(": no puedes borrar citas.", ": you cannot delete quotes."); return; }
        char key[16];
        qkey(u_atoi(arg), key);
        if (xbnc_kv_del(key) > 0) { u_say2("Cita borrada.", "Quote deleted."); }
        else u_say2("No existe esa cita.", "No such quote.");
        return;
    }
    xbnc_kv_list("q:", "", keys, sizeof keys);
    int total = u_count_keys(keys);
    if (!total) { u_say2("Todavía no hay citas. Añade una con !cita añadir <texto>.", "No quotes yet. Add one with !quote add <text>."); return; }
    if (u_eq(w, "buscar") || u_eq(w, "search")) {
        if (!*arg) { u_say2("Uso: !cita buscar <palabra>", "Usage: !quote search <word>"); return; }
        /* The newest match wins: walk all and keep the last. */
        static char rec[600];
        char best[16];
        best[0] = 0;
        int hits = 0;
        char *cur = keys, *k;
        while ((k = u_next_key(&cur))) {
            if (xbnc_kv_get_str(k, rec, sizeof rec) <= 0) continue;
            char *f[3];
            if (u_split(rec, '\t', f, 3) < 3) continue;
            if (xbnc_find_ci(f[2], arg)) { u_copy(best, sizeof best, k); hits++; }
        }
        if (!best[0]) { u_say2("Ninguna cita contiene eso.", "No quote contains that."); return; }
        show(u_atoi(best + 2));
        if (hits > 1) { u_say(" ("); u_sayn(hits); u_say2(" coinciden)", " match)"); }
        return;
    }
    if (w[0] >= '0' && w[0] <= '9') {
        if (!show(u_atoi(w))) u_say2("No existe esa cita.", "No such quote.");
        return;
    }
    /* A random one. */
    int pick = (int)(rnd() % (unsigned)total), i = 0;
    char *cur = keys, *k;
    while ((k = u_next_key(&cur))) if (i++ == pick) { show(u_atoi(k + 2)); return; }
}

void on_event(const xbnc_event *ev) {
    static char kind[16], text[512], from[64], conv[64], trigger[32];
    xbnc_get(ev, "event", kind, sizeof kind);
    u_lang(ev);
    if (u_eq(kind, "command")) {
        xbnc_get(ev, "text", text, sizeof text);
        char w[16];
        u_word(text, w, sizeof w);
        if (!text[0] || u_eq(w, "ayuda") || u_eq(w, "help")) {
            xbnc_tell(u_es ? "En un canal: !cita (una al azar), !cita 12, !cita buscar palabra, !cita añadir texto, !cita borrar 12. Aquí igual: /msg *quotes añadir texto."
                           : "In a channel: !quote (a random one), !quote 12, !quote search word, !quote add text, !quote del 12. Here too: /msg *quotes add text.");
            return;
        }
        xbnc_get(ev, "nick", from, sizeof from);
        handle(ev, text, from[0] ? from : "?", 1);
        if (u_out[0]) xbnc_tell(u_out);
        return;
    }
    if (!u_eq(kind, "message") || !xbnc_is(ev, "channel")) return;
    int owner = xbnc_is(ev, "outgoing");
    xbnc_get(ev, "text", text, sizeof text);
    xbnc_get(ev, "conversation", conv, sizeof conv);
    xbnc_get(ev, owner ? "nick" : "from", from, sizeof from);
    if (!u_in_list(ev, "channels", conv, 1)) return;
    xbnc_config(ev, "trigger", trigger, sizeof trigger);
    if (!trigger[0]) u_copy(trigger, sizeof trigger, "!quote");
    const char *rest = u_command(text, trigger);
    if (!rest) rest = u_command(text, "!quote");
    if (!rest) rest = u_command(text, "!cita");
    if (!rest) return;
    handle(ev, rest, from, owner);
    if (u_out[0]) xbnc_reply(u_out);
}
