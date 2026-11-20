/*
 * greet: welcomes people who join your channels.
 *
 *   * ana joins #xbnc   ->  Welcome to #xbnc, ana! It's your first time here.
 *   * bob joins again   ->  Hello bob, welcome back to #xbnc.
 *
 * Personal greetings (setting "personal", one per line: "nick message")
 * replace the usual one. {nick} and {channel} are replaced. The same person
 * is not greeted twice in a row within the cooldown (net splits, rejoins).
 * Commands: /msg *greet test #xbnc nick shows what would be said.
 */
#include "../xbnc_plugin.h"
#include "../xbnc_util.h"

#define MAX_RECENT 256
static char R_KEY[MAX_RECENT][96];
static xbnc_i64 R_TS[MAX_RECENT];
static int r_next;

static int recently(const char *key, xbnc_i64 now, xbnc_i64 cooldown_ms) {
    for (int i = 0; i < MAX_RECENT; i++)
        if (R_KEY[i][0] && u_eq(R_KEY[i], key) && now - R_TS[i] < cooldown_ms) return 1;
    return 0;
}

static void remember(const char *key, xbnc_i64 now) {
    u_copy(R_KEY[r_next], sizeof R_KEY[0], key);
    R_TS[r_next] = now;
    r_next = (r_next + 1) % MAX_RECENT;
}

/* Replaces {nick} and {channel} in tpl into u_out. */
static void fill(const char *tpl, const char *nick, const char *chan) {
    u_reset();
    for (const char *p = tpl; *p;) {
        if (xbnc_starts_with(p, "{nick}")) { u_say(nick); p += 6; continue; }
        if (xbnc_starts_with(p, "{channel}")) { u_say(chan); p += 9; continue; }
        char one[2] = {*p++, 0};
        u_say(one);
    }
}

/* The personal greeting for nick, if any, into out. */
static int personal(const xbnc_event *ev, const char *nick, char *out, xbnc_u32 max) {
    static char list[4096];
    xbnc_config(ev, "personal", list, sizeof list);
    char *p = list;
    while (*p) {
        char *line = p;
        while (*p && *p != '\n') p++;
        if (*p) *p++ = 0;
        char who[64];
        const char *msg = u_word(line, who, sizeof who);
        if (who[0] && u_eq_fold(who, nick) && *msg) { u_copy(out, max, msg); return 1; }
    }
    return 0;
}

#define DEF_FIRST_EN "Welcome to {channel}, {nick}! It's your first time here."
#define DEF_FIRST_ES "¡Bienvenido a {channel}, {nick}! Es tu primera vez por aquí."
#define DEF_BACK_EN "Hi {nick}, welcome back to {channel}."
#define DEF_BACK_ES "Hola {nick}, bienvenido de nuevo a {channel}."

static void build(const xbnc_event *ev, const char *nick, const char *chan, int first) {
    static char tpl[400];
    if (personal(ev, nick, tpl, sizeof tpl)) { fill(tpl, nick, chan); return; }
    xbnc_config(ev, first ? "first" : "message", tpl, sizeof tpl);
    if (!tpl[0] && first) xbnc_config(ev, "message", tpl, sizeof tpl);
    if (!tpl[0]) { u_reset(); return; }
    /* The manifest defaults are in English; untouched defaults follow the
       reply language. */
    if (u_es && u_eq(tpl, DEF_FIRST_EN)) u_copy(tpl, sizeof tpl, DEF_FIRST_ES);
    else if (u_es && u_eq(tpl, DEF_BACK_EN)) u_copy(tpl, sizeof tpl, DEF_BACK_ES);
    fill(tpl, nick, chan);
}

void on_event(const xbnc_event *ev) {
    static char kind[16], nick[64], chan[64], text[256], key[160];
    xbnc_get(ev, "event", kind, sizeof kind);
    u_lang(ev);
    if (u_eq(kind, "command")) {
        xbnc_get(ev, "text", text, sizeof text);
        char w[16], c[64], n[64];
        const char *r = u_word(text, w, sizeof w);
        r = u_word(r, c, sizeof c);
        u_word(r, n, sizeof n);
        if (u_eq(w, "test") && u_is_channel(c) && n[0]) {
            build(ev, n, c, 0);
            if (!u_out[0]) xbnc_tell(u_es ? "(no se saluda a quien vuelve: el mensaje está vacío)" : "(returning people are not greeted: the message is empty)");
            else xbnc_tell(u_out);
            build(ev, n, c, 1);
            if (u_out[0]) xbnc_tell(u_out);
            return;
        }
        xbnc_tell(u_es ? "Saluda a quien entra en tus canales. Prueba: /msg *greet test #canal nick. Los mensajes se cambian en el panel (Plugins)."
                       : "Greets people who join your channels. Try: /msg *greet test #channel nick. Change the messages in the panel (Plugins).");
        return;
    }
    if (!u_eq(kind, "join") || xbnc_is(ev, "self")) return;
    xbnc_get(ev, "channel", chan, sizeof chan);
    xbnc_get(ev, "from", nick, sizeof nick);
    if (!u_in_list(ev, "channels", chan, 1) || u_in_list(ev, "ignore", nick, 0)) return;
    xbnc_i64 now = xbnc_now_ms();
    char fc[64], fn[64];
    u_fold(chan, fc, sizeof fc);
    u_fold(nick, fn, sizeof fn);
    key[0] = 0;
    xbnc_cat(key, sizeof key, "s:"); xbnc_cat(key, sizeof key, fc);
    xbnc_cat(key, sizeof key, " "); xbnc_cat(key, sizeof key, fn);
    xbnc_i64 cooldown = xbnc_config_int(ev, "cooldown", 30) * 60000;
    if (recently(key, now, cooldown)) return;
    remember(key, now);
    char seen[24];
    int first = xbnc_kv_get_str(key, seen, sizeof seen) <= 0;
    if (first) { char ts[24]; xbnc_itoa(now, ts); xbnc_kv_set_str(key, ts); }
    build(ev, nick, chan, first);
    if (!u_out[0]) return;
    if (xbnc_config_bool(ev, "notice")) xbnc_notice(nick, u_out);
    else xbnc_privmsg(chan, u_out);
}
