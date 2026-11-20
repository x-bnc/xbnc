/*
 * karma: points between people.
 *
 *   bob++        ->  bob has 5 karma.
 *   bob--        ->  bob has 4 karma.
 *   !karma bob   ->  bob has 4 karma.
 *   !karma top   ->  Most karma: ana (12), bob (4), carol (2).
 * Nobody can give karma to themselves, and the same person can change
 * someone's karma once a minute.
 */
#include "../xbnc_plugin.h"
#include "../xbnc_util.h"

#define MAX_COOL 128
static char C_KEY[MAX_COOL][96];
static xbnc_i64 C_TS[MAX_COOL];
static int c_next;

static int cooling(const char *key, xbnc_i64 now) {
    for (int i = 0; i < MAX_COOL; i++)
        if (C_KEY[i][0] && u_eq(C_KEY[i], key) && now - C_TS[i] < 60000) return 1;
    u_copy(C_KEY[c_next], 96, key);
    C_TS[c_next] = now;
    c_next = (c_next + 1) % MAX_COOL;
    return 0;
}

static void kkey(const char *nick, char *key, xbnc_u32 max) {
    char f[48];
    u_fold(nick, f, sizeof f);
    key[0] = 0;
    xbnc_cat(key, max, "k:");
    xbnc_cat(key, max, f);
}

static xbnc_i64 get_karma(const char *nick) {
    char key[64], v[24];
    kkey(nick, key, sizeof key);
    return xbnc_kv_get_str(key, v, sizeof v) > 0 ? u_atoi(v) : 0;
}

static xbnc_i64 add_karma(const char *nick, int delta) {
    char key[64], v[24];
    xbnc_i64 k = get_karma(nick) + delta;
    kkey(nick, key, sizeof key);
    xbnc_itoa(k, v);
    xbnc_kv_set_str(key, v);
    key[0] = 'n';
    xbnc_kv_set_str(key, nick);
    return k;
}

static void say_karma(const char *nick) {
    u_reset();
    u_say(nick);
    u_say2(" tiene ", " has ");
    u_sayn(get_karma(nick));
    u_say2(" de karma.", " karma.");
}

static void say_top(void) {
    static char keys[16384];
    char best[5][48];
    xbnc_i64 pts[5];
    int filled = 0;
    xbnc_kv_list("k:", "", keys, sizeof keys);
    char *cur = keys, *k;
    while ((k = u_next_key(&cur))) {
        char v[24], name[48], nk[64];
        if (xbnc_kv_get_str(k, v, sizeof v) <= 0) continue;
        xbnc_i64 p = u_atoi(v);
        nk[0] = 0; xbnc_cat(nk, sizeof nk, "n:"); xbnc_cat(nk, sizeof nk, k + 2);
        if (xbnc_kv_get_str(nk, name, sizeof name) <= 0) u_copy(name, sizeof name, k + 2);
        int pos = filled;
        while (pos > 0 && pts[pos - 1] < p) pos--;
        if (pos >= 5) continue;
        for (int j = (filled < 5 ? filled : 4); j > pos; j--) { pts[j] = pts[j - 1]; u_copy(best[j], 48, best[j - 1]); }
        pts[pos] = p;
        u_copy(best[pos], 48, name);
        if (filled < 5) filled++;
    }
    u_reset();
    if (!filled) { u_say2("Nadie tiene karma todavía.", "Nobody has karma yet."); return; }
    u_say2("Más karma: ", "Most karma: ");
    for (int i = 0; i < filled; i++) {
        if (i) u_say(", ");
        u_say(best[i]); u_say(" ("); u_sayn(pts[i]); u_say(")");
    }
}

/* "nick++" / "nick--" anywhere in the text as its own word. */
static int find_vote(const char *text, char *nick, xbnc_u32 max) {
    for (const char *p = text; *p; p++) {
        if ((p[0] == '+' && p[1] == '+') || (p[0] == '-' && p[1] == '-')) {
            if (p[2] && p[2] != ' ' && p[2] != ',' && p[2] != '.' && p[2] != '!') continue;
            const char *s = p;
            while (s > text && s[-1] != ' ' && s[-1] != '(' ) s--;
            xbnc_u32 n = (xbnc_u32)(p - s);
            if (n == 0 || n + 1 > max) continue;
            for (xbnc_u32 i = 0; i < n; i++) nick[i] = s[i];
            nick[n] = 0;
            if (nick[n - 1] == ':' || nick[n - 1] == ',') nick[--n] = 0;
            if (!n) continue;
            return p[0] == '+' ? 1 : -1;
        }
    }
    return 0;
}

void on_event(const xbnc_event *ev) {
    static char kind[16], text[512], from[64], conv[64];
    xbnc_get(ev, "event", kind, sizeof kind);
    u_lang(ev);
    if (u_eq(kind, "command")) {
        xbnc_get(ev, "text", text, sizeof text);
        char w[64];
        u_word(text, w, sizeof w);
        if (u_eq(w, "top")) { say_top(); xbnc_tell(u_out); return; }
        if (w[0] && !u_eq(w, "ayuda") && !u_eq(w, "help")) { say_karma(w); xbnc_tell(u_out); return; }
        xbnc_tell(u_es ? "En un canal: nick++ o nick-- para dar o quitar karma, !karma nick para verlo y !karma top para el ranking."
                       : "In a channel: nick++ or nick-- to give or take karma, !karma nick to see it and !karma top for the ranking.");
        return;
    }
    if (!u_eq(kind, "message") || !xbnc_is(ev, "channel")) return;
    xbnc_get(ev, "text", text, sizeof text);
    xbnc_get(ev, "conversation", conv, sizeof conv);
    xbnc_get(ev, xbnc_is(ev, "outgoing") ? "nick" : "from", from, sizeof from);
    if (!u_in_list(ev, "channels", conv, 1)) return;
    const char *rest = u_command(text, "!karma");
    if (rest) {
        char w[64];
        u_word(rest, w, sizeof w);
        if (u_eq(w, "top")) say_top();
        else say_karma(w[0] ? w : from);
        xbnc_reply(u_out);
        return;
    }
    char nick[48];
    int delta = find_vote(text, nick, sizeof nick);
    if (!delta) return;
    if (u_eq_fold(nick, from)) {
        u_reset();
        u_say(from);
        u_say2(": no puedes darte karma a ti mismo.", ": you cannot give karma to yourself.");
        xbnc_reply(u_out);
        return;
    }
    char key[96], f1[48], f2[48];
    u_fold(from, f1, sizeof f1);
    u_fold(nick, f2, sizeof f2);
    key[0] = 0; xbnc_cat(key, sizeof key, f1); xbnc_cat(key, sizeof key, ">"); xbnc_cat(key, sizeof key, f2);
    if (cooling(key, xbnc_now_ms())) return;
    add_karma(nick, delta);
    say_karma(nick);
    xbnc_reply(u_out);
}
