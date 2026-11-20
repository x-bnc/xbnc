/*
 * factoids: the channel's memory. Short answers to frequent questions.
 *
 *   !def rules                  -> rules: no spam, no flood, be nice.
 *   ?rules                      -> the same (shortcut, can be turned off)
 *   !def rules = no spam...     -> saves it (you and the editors you list)
 *   !def -rules                 -> deletes it
 *   !defs                       -> lists the saved words
 * Also /msg *factoids rules, or /msg *factoids rules = text.
 */
#include "../xbnc_plugin.h"
#include "../xbnc_util.h"

static void key_of(const char *word, char *key, xbnc_u32 max) {
    char n[64];
    u_norm(word, n, sizeof n);
    key[0] = 0;
    xbnc_cat(key, max, "f:");
    xbnc_cat(key, max, n);
}

static int may_edit(const xbnc_event *ev, const char *from, int owner) {
    return owner || u_in_list(ev, "editors", from, 0);
}

static void list_all(void) {
    static char keys[8192];
    xbnc_kv_list("f:", "", keys, sizeof keys);
    u_reset();
    int n = u_count_keys(keys);
    if (!n) { u_say2("Todavía no hay nada guardado. Ejemplo: !def normas = no spam.", "Nothing saved yet. Example: !def rules = no spam."); return; }
    u_say2("Guardado (", "Saved (");
    u_sayn(n);
    u_say("): ");
    char *cur = keys, *k;
    int i = 0;
    while ((k = u_next_key(&cur)) && i < 40) {
        if (i++) u_say(", ");
        u_say(k + 2);
    }
    if (n > 40) u_say(", …");
}

/* Handles "word", "word = text" or "-word". Builds the answer in u_out. */
static void handle(const xbnc_event *ev, const char *arg, const char *from, int owner) {
    static char text[460], word[64], key[80], val[460];
    u_copy(text, sizeof text, arg);
    u_reset();
    char *eq = 0;
    for (char *p = text; *p; p++) if (*p == '=') { eq = p; break; }
    if (eq) {
        *eq = 0;
        char *w = text, *v = (char *)u_skip_spaces(eq + 1);
        xbnc_u32 wl = xbnc_strlen(w);
        while (wl && w[wl - 1] == ' ') w[--wl] = 0;
        w = (char *)u_skip_spaces(w);
        if (!w[0] || xbnc_strlen(w) > 40) { u_say2("Pon una palabra antes del =.", "Put a word before the =."); return; }
        if (!may_edit(ev, from, owner)) { u_say(from); u_say2(": no puedes cambiar las definiciones.", ": you cannot change definitions."); return; }
        key_of(w, key, sizeof key);
        if (!v[0]) { xbnc_kv_del(key); u_say2("Borrado: ", "Deleted: "); u_say(w); return; }
        xbnc_kv_set_str(key, v);
        u_say2("Guardado: ", "Saved: ");
        u_say(w);
        return;
    }
    u_word(text, word, sizeof word);
    if (!word[0]) { list_all(); return; }
    if (word[0] == '-' && word[1]) {
        if (!may_edit(ev, from, owner)) { u_say(from); u_say2(": no puedes borrar definiciones.", ": you cannot delete definitions."); return; }
        key_of(word + 1, key, sizeof key);
        xbnc_kv_del(key);
        u_say2("Borrado: ", "Deleted: ");
        u_say(word + 1);
        return;
    }
    key_of(word, key, sizeof key);
    if (xbnc_kv_get_str(key, val, sizeof val) <= 0) {
        if (owner) { u_say2("No hay nada para «", "Nothing saved for \""); u_say(word); u_say2("». Guárdalo con !def palabra = texto.", "\". Save it with !def word = text."); }
        return;
    }
    u_say(word);
    u_say(": ");
    u_say(val);
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
            xbnc_tell(u_es ? "En un canal: !def palabra (o ?palabra), !def palabra = texto para guardarla, !def -palabra para borrarla, !defs para la lista. Aquí: /msg *factoids palabra."
                           : "In a channel: !def word (or ?word), !def word = text to save it, !def -word to delete it, !defs for the list. Here: /msg *factoids word.");
            return;
        }
        if (u_eq(w, "list") || u_eq(w, "lista")) { list_all(); xbnc_tell(u_out); return; }
        handle(ev, text, "", 1);
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
    if (!trigger[0]) u_copy(trigger, sizeof trigger, "!def");
    const char *rest = u_command(text, trigger);
    if (!rest) rest = u_command(text, "!defs") ? "" : 0;
    if (rest) {
        handle(ev, rest, from, owner);
        if (u_out[0]) xbnc_reply(u_out);
        return;
    }
    if (text[0] == '?' && text[1] && text[1] != ' ' && text[1] != '?' && xbnc_config_bool(ev, "question_mark")) {
        char w[64];
        u_word(text + 1, w, sizeof w);
        static char key[80], val[460];
        key_of(w, key, sizeof key);
        if (xbnc_kv_get_str(key, val, sizeof val) <= 0) return;
        u_reset();
        u_say(w); u_say(": "); u_say(val);
        xbnc_reply(u_out);
    }
}
