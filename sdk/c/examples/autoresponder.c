/*
 * autoresponder: answers on its own when someone writes a keyword.
 *
 * Rules, one per line in the settings:   keyword = answer
 *   hello = Hello {nick}! Welcome to {channel}.
 *   web = Our website: https://example.org
 *   * = I am away right now; I will answer when I am back.   (any private message)
 * {nick} is who wrote, {channel} the channel. Keywords match whole words,
 * without caring about upper or lower case. /msg *autoresponder shows the
 * rules and "test <text>" tries them.
 */
#include "../xbnc_plugin.h"
#include "../xbnc_util.h"

#define MAX_RULES 64
#define RULES_BUF 32768

static char rules[RULES_BUF];
static char *keys[MAX_RULES], *vals[MAX_RULES];
static int nrules;
static xbnc_i64 last_rule[MAX_RULES];
/* Private auto-reply: once per nick every few minutes. */
static char replied_nick[32][40];
static xbnc_i64 replied_at[32];

static char *trim(char *s) {
    while (*s == ' ' || *s == '\t') s++;
    xbnc_u32 n = xbnc_strlen(s);
    while (n && (s[n - 1] == ' ' || s[n - 1] == '\t' || s[n - 1] == '\r')) s[--n] = 0;
    return s;
}

static void parse_rules(const xbnc_event *ev) {
    xbnc_config(ev, "rules", rules, sizeof rules);
    nrules = 0;
    char *line = rules;
    while (*line && nrules < MAX_RULES) {
        char *end = line;
        while (*end && *end != '\n') end++;
        char next = *end;
        *end = 0;
        char *eq = line;
        while (*eq && *eq != '=') eq++;
        if (*eq == '=') {
            *eq = 0;
            char *k = trim(line), *v = trim(eq + 1);
            if (k[0] && v[0]) { keys[nrules] = k; vals[nrules] = v; nrules++; }
        }
        line = next ? end + 1 : end;
    }
}

/* Decodes the UTF-8 character at p (0 at the end). */
static unsigned int cp_at(const unsigned char *p) {
    if (p[0] < 0x80) return p[0];
    if ((p[0] & 0xE0) == 0xC0 && p[1]) return ((p[0] & 0x1Fu) << 6) | (p[1] & 0x3Fu);
    if ((p[0] & 0xF0) == 0xE0 && p[1] && p[2]) return ((p[0] & 0x0Fu) << 12) | ((p[1] & 0x3Fu) << 6) | (p[2] & 0x3Fu);
    return 0x10000; /* treat other characters as letters */
}

/* Letters and digits (ASCII, accented Latin letters and other scripts);
 * punctuation such as ¿ ¡ « » “ ” is not part of a word. */
static int is_word_cp(unsigned int c) {
    if (c < 0x80) return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_';
    if (c < 0xC0) return 0;
    if (c == 0xD7 || c == 0xF7) return 0;
    if (c >= 0x2000 && c <= 0x206F) return 0; /* general punctuation */
    return 1;
}

/* The character that ends just before p. */
static unsigned int cp_before(const char *text, const char *p) {
    const unsigned char *q = (const unsigned char *)p;
    if (p == text) return ' ';
    q--;
    while ((const char *)q > text && (*q & 0xC0) == 0x80) q--;
    return cp_at(q);
}

static int is_word_char(unsigned char c) {
    return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c >= 0x80 || c == '_';
}

/* Whole-word, case-insensitive search of key in text. */
static int has_word(const char *text, const char *key) {
    xbnc_u32 kl = xbnc_strlen(key);
    for (const char *p = text; *p; p++) {
        xbnc_u32 i = 0;
        while (i < kl && p[i] && u_fold_c((unsigned char)p[i]) == u_fold_c((unsigned char)key[i])) i++;
        if (i == kl) {
            int before = !is_word_cp(cp_before(text, p)) || !is_word_char((unsigned char)key[0]);
            int after = !is_word_cp(cp_at((const unsigned char *)p + kl)) || !is_word_char((unsigned char)key[kl - 1]);
            if (before && after) return 1;
        }
    }
    return 0;
}

static void fill(const char *tpl, const char *nick, const char *chan) {
    u_reset();
    while (*tpl) {
        if (xbnc_starts_with(tpl, "{nick}")) { u_say(nick); tpl += 6; continue; }
        if (xbnc_starts_with(tpl, "{channel}")) { u_say(chan); tpl += 9; continue; }
        char c[2] = {*tpl++, 0};
        u_say(c);
    }
}

/* The first rule that matches (private messages also match "*"). */
static int find_rule(const char *text, int private_msg) {
    for (int i = 0; i < nrules; i++)
        if (!u_eq(keys[i], "*") && has_word(text, keys[i])) return i;
    if (private_msg)
        for (int i = 0; i < nrules; i++)
            if (u_eq(keys[i], "*")) return i;
    return -1;
}

void on_event(const xbnc_event *ev) {
    static char kind[16], text[512], from[64], conv[128];
    xbnc_get(ev, "event", kind, sizeof kind);
    u_lang(ev);
    parse_rules(ev);
    xbnc_i64 now = xbnc_now_ms();

    if (u_eq(kind, "command")) {
        xbnc_get(ev, "text", text, sizeof text);
        char w[16];
        const char *rest = u_word(text, w, sizeof w);
        if (u_eq(w, "test") || u_eq(w, "probar")) {
            int r = find_rule(rest, 0);
            u_reset();
            if (r < 0) u_say2("Ninguna regla responde a ese texto.", "No rule answers that text.");
            else { fill(vals[r], "nick", "#canal"); }
            xbnc_tell(u_out);
            return;
        }
        if (!nrules) {
            xbnc_tell(u_es ? "Todavía no hay reglas. Añádelas en el panel (Redes > tu red > Plugins > Respuestas automáticas), una por línea: palabra = respuesta."
                           : "No rules yet. Add them in the panel (Networks > your network > Plugins > Autoresponder), one per line: keyword = answer.");
            return;
        }
        u_reset();
        u_say2("Reglas: ", "Rules: ");
        for (int i = 0; i < nrules && i < 15; i++) {
            if (i) u_say(" | ");
            u_say(keys[i]);
        }
        u_say2(". Prueba una con: /msg *autoresponder test <texto>", ". Try one with: /msg *autoresponder test <text>");
        xbnc_tell(u_out);
        return;
    }
    if (!u_eq(kind, "message") || xbnc_is(ev, "outgoing") || !nrules) return;
    int channel = xbnc_is(ev, "channel");
    if (!channel && !xbnc_config_bool(ev, "in_queries")) return;
    if (xbnc_config_bool(ev, "only_when_away") && xbnc_int(xbnc_field(ev->json, "clients"), 0) > 0) return;
    xbnc_get(ev, "conversation", conv, sizeof conv);
    if (channel && !u_in_list(ev, "channels", conv, 1)) return;
    xbnc_get(ev, "text", text, sizeof text);
    xbnc_get(ev, "from", from, sizeof from);

    int r = find_rule(text, !channel);
    if (r < 0) return;
    xbnc_i64 cooldown = xbnc_config_int(ev, "cooldown", 60) * 1000;
    if (now - last_rule[r] < cooldown) return;
    if (u_eq(keys[r], "*")) {
        /* The private auto-reply goes once per person every 10 minutes. */
        int slot = -1, oldest = 0;
        for (int i = 0; i < 32; i++) {
            if (u_eq_fold(replied_nick[i], from)) { slot = i; break; }
            if (replied_at[i] < replied_at[oldest]) oldest = i;
        }
        if (slot >= 0 && now - replied_at[slot] < 600000) return;
        if (slot < 0) slot = oldest;
        u_copy(replied_nick[slot], 40, from);
        replied_at[slot] = now;
    } else {
        last_rule[r] = now;
    }
    fill(vals[r], from, channel ? conv : "");
    xbnc_reply(u_out);
}
