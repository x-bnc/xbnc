/*
 * badwords: keeps unwanted words out of your channels.
 *
 * Someone writes a forbidden word: the first time they get a warning, the
 * second time a kick, the third time a ban for some minutes (removed
 * automatically). Strikes are forgotten after an hour. You need op (or
 * halfop) in the channel; you and the nicks in "exempt" are never touched.
 *
 * Words are whole words, without caring about case or accents; a * at the
 * end matches the beginning of a word (idiot* matches idiot and idiots).
 * /msg *badwords test <text> tells whether a text would be caught.
 */
#include "../xbnc_plugin.h"
#include "../xbnc_util.h"

#define MAX_STRIKES 256
static char S_KEY[MAX_STRIKES][96];
static int S_N[MAX_STRIKES];
static xbnc_i64 S_TS[MAX_STRIKES];

#define MAX_BANS 64
static char B_CHAN[MAX_BANS][64];
static char B_MASK[MAX_BANS][128];
static xbnc_i64 B_UNTIL[MAX_BANS];

static int is_word_char(unsigned char c) {
    return (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c >= 0x80 || c == '_';
}

/* Whether normalised text contains the normalised pattern as a word. */
static int has_word(const char *text, const char *pat) {
    xbnc_u32 pl = xbnc_strlen(pat);
    int prefix = 0;
    if (pl && pat[pl - 1] == '*') { prefix = 1; pl--; }
    if (!pl) return 0;
    for (const char *p = text; *p; p++) {
        if (p != text && is_word_char((unsigned char)p[-1])) continue;
        xbnc_u32 i = 0;
        while (i < pl && p[i] == pat[i]) i++;
        if (i < pl) continue;
        if (prefix || !is_word_char((unsigned char)p[pl])) return 1;
    }
    return 0;
}

/* The forbidden word found in text, or 0. */
static int caught(const xbnc_event *ev, const char *text, char *which, xbnc_u32 max) {
    static char list[4096], norm[600];
    char pat[64];
    u_norm(text, norm, sizeof norm);
    xbnc_config(ev, "words", list, sizeof list);
    char *p = list;
    while (*p) {
        char *line = p;
        while (*p && *p != '\n') p++;
        if (*p) *p++ = 0;
        u_norm(u_skip_spaces(line), pat, sizeof pat);
        if (pat[0] && has_word(norm, pat)) { u_copy(which, max, pat); return 1; }
    }
    return 0;
}

static int strike(const char *key, xbnc_i64 now) {
    int free_slot = -1, oldest = 0;
    for (int i = 0; i < MAX_STRIKES; i++) {
        if (S_KEY[i][0] && u_eq(S_KEY[i], key)) {
            if (now - S_TS[i] > 3600000) S_N[i] = 0;
            S_N[i]++;
            S_TS[i] = now;
            return S_N[i];
        }
        if (!S_KEY[i][0] && free_slot < 0) free_slot = i;
        if (S_TS[i] < S_TS[oldest]) oldest = i;
    }
    int s = free_slot >= 0 ? free_slot : oldest;
    u_copy(S_KEY[s], 96, key);
    S_N[s] = 1;
    S_TS[s] = now;
    return 1;
}

/* *!*@host from nick!user@host */
static void ban_mask(const char *full, const char *nick, char *out, xbnc_u32 max) {
    const char *at = 0;
    for (const char *p = full; *p; p++) if (*p == '@') at = p;
    out[0] = 0;
    if (at && at[1]) { xbnc_cat(out, max, "*!*"); xbnc_cat(out, max, at); }
    else { xbnc_cat(out, max, nick); xbnc_cat(out, max, "!*@*"); }
}

static void unban_due(xbnc_i64 now) {
    char line[256];
    for (int i = 0; i < MAX_BANS; i++) {
        if (!B_CHAN[i][0] || now < B_UNTIL[i]) continue;
        line[0] = 0;
        xbnc_cat(line, sizeof line, "MODE "); xbnc_cat(line, sizeof line, B_CHAN[i]);
        xbnc_cat(line, sizeof line, " -b "); xbnc_cat(line, sizeof line, B_MASK[i]);
        xbnc_raw(line);
        B_CHAN[i][0] = 0;
        return; /* one per tick is enough */
    }
}

static void remember_ban(const char *chan, const char *mask, xbnc_i64 until) {
    int slot = 0;
    for (int i = 0; i < MAX_BANS; i++) if (!B_CHAN[i][0]) { slot = i; break; }
    u_copy(B_CHAN[slot], 64, chan);
    u_copy(B_MASK[slot], 128, mask);
    B_UNTIL[slot] = until;
}

void on_event(const xbnc_event *ev) {
    static char kind[16], text[512], from[64], mask[200], conv[64], word[64], line[512];
    xbnc_get(ev, "event", kind, sizeof kind);
    u_lang(ev);
    xbnc_i64 now = xbnc_now_ms();
    if (u_eq(kind, "timer")) { unban_due(now); return; }
    if (u_eq(kind, "command")) {
        xbnc_get(ev, "text", text, sizeof text);
        char w[16];
        const char *r = u_word(text, w, sizeof w);
        if (u_eq(w, "test") || u_eq(w, "probar")) {
            u_reset();
            if (caught(ev, r, word, sizeof word)) { u_say2("Se detectaría: ", "Would be caught: "); u_say(word); }
            else u_say2("No se detectaría.", "Would not be caught.");
            xbnc_tell(u_out);
            return;
        }
        xbnc_tell(u_es ? "Avisa, echa y banea a quien escribe palabras prohibidas en tus canales (necesitas op). Las palabras se ponen en el panel. Prueba: /msg *badwords test texto."
                       : "Warns, kicks and bans people who write forbidden words in your channels (you need op). Set the words in the panel. Try: /msg *badwords test text.");
        return;
    }
    if (!u_eq(kind, "message") || !xbnc_is(ev, "channel") || xbnc_is(ev, "outgoing")) return;
    xbnc_get(ev, "conversation", conv, sizeof conv);
    xbnc_get(ev, "from", from, sizeof from);
    xbnc_get(ev, "from_mask", mask, sizeof mask);
    xbnc_get(ev, "text", text, sizeof text);
    if (!u_in_list(ev, "channels", conv, 1) || u_in_list(ev, "exempt", from, 0)) return;
    if (!caught(ev, text, word, sizeof word)) return;
    char key[96], f[48], c[48];
    u_fold(from, f, sizeof f);
    u_fold(conv, c, sizeof c);
    key[0] = 0; xbnc_cat(key, sizeof key, c); xbnc_cat(key, sizeof key, " "); xbnc_cat(key, sizeof key, f);
    int n = strike(key, now);
    char reason[200];
    reason[0] = 0;
    xbnc_config(ev, "reason", reason, sizeof reason);
    if (!reason[0]) u_copy(reason, sizeof reason, u_es ? "Lenguaje no permitido" : "Language not allowed");
    if (n == 1) {
        u_reset();
        u_say(from);
        u_say2(": esa palabra no está permitida aquí. La próxima vez, kick.", ": that word is not allowed here. Next time, kick.");
        if (xbnc_config_bool(ev, "warn_private")) xbnc_notice(from, u_out); else xbnc_reply(u_out);
        return;
    }
    if (n >= 3) {
        char bm[160];
        ban_mask(mask, from, bm, sizeof bm);
        xbnc_i64 mins = xbnc_config_int(ev, "ban_minutes", 10);
        line[0] = 0;
        xbnc_cat(line, sizeof line, "MODE "); xbnc_cat(line, sizeof line, conv);
        xbnc_cat(line, sizeof line, " +b "); xbnc_cat(line, sizeof line, bm);
        xbnc_raw(line);
        if (mins > 0) remember_ban(conv, bm, now + mins * 60000);
    }
    line[0] = 0;
    xbnc_cat(line, sizeof line, "KICK "); xbnc_cat(line, sizeof line, conv);
    xbnc_cat(line, sizeof line, " "); xbnc_cat(line, sizeof line, from);
    xbnc_cat(line, sizeof line, " :"); xbnc_cat(line, sizeof line, reason);
    xbnc_raw(line);
}
