/*
 * seen: "!seen nick" says whether someone is in a channel right now or when
 * they were last seen and what they were doing.
 *
 *   !seen bob   ->  bob is in #xbnc and #xbnc-dev right now. Last active
 *                   3 min ago: writing in #xbnc: "is anyone using xBNC?"
 *   !seen ana   ->  ana was last seen 2 h 14 min ago,
 *                   leaving #xbnc (Off to bed).
 *
 * Also as /msg *seen bob (the answer is only for you).
 *
 * Permissions: "presence" (joins, parts, quits, kicks, nick changes and the
 * member list of each channel) and "send" (to answer in channels). What it
 * remembers lives in xBNC's plugin storage (one record per nick), so it
 * survives restarts and holds thousands of people. Private messages are
 * never recorded.
 */
#define XBNC_ARENA_SIZE (1024 * 1024)
#include "../xbnc_plugin.h"

#define MAX_PRESENT 20000
#define HASH_SIZE 65536 /* power of two, > 2 * MAX_PRESENT */
#define MAX_CHANS 64
#define NICK_MAX 32
#define CHAN_MAX 51
#define TEXT_MAX 151

enum { K_NONE = 0, K_MSG, K_ACTION, K_JOIN, K_PART, K_QUIT, K_KICKED, K_NICK_TO, K_NICK_FROM };
static const char KIND_CH[] = "-majpqkNF";

/* People in a channel right now (memory only: rebuilt from NAMES). */
typedef struct {
    char nick[NICK_MAX];
    char key[NICK_MAX];
    unsigned long long present; /* bit i: in channel i */
} who_t;

static who_t P[MAX_PRESENT];
static int H[HASH_SIZE]; /* index + 1 into P, 0 = empty */
static int n_present;
static char CH[MAX_CHANS][CHAN_MAX]; /* folded names, "" = free */
static char CH_SHOWN[MAX_CHANS][CHAN_MAX];

/* A remembered record (stored as "ts\tkind\tnick\tchan\tother\ttext"). */
typedef struct {
    xbnc_i64 ts;
    int kind;
    char nick[NICK_MAX];
    char chan[CHAN_MAX];
    char other[NICK_MAX];
    char text[TEXT_MAX];
} rec_t;

#define MAX_COOL 32
static char COOL_KEY[MAX_COOL][CHAN_MAX];
static xbnc_i64 COOL_TS[MAX_COOL];

static char es;

/* ------------------------------------------------------------ strings */

static int fold_c(int c) {
    if (c >= 'A' && c <= 'Z') return c + 32;
    if (c == '[') return '{';
    if (c == ']') return '}';
    if (c == '\\') return '|';
    if (c == '~') return '^';
    return c;
}

static void fold(const char *s, char *out, xbnc_u32 max) {
    xbnc_u32 i = 0;
    for (; s[i] && i + 1 < max; i++) out[i] = (char)fold_c((unsigned char)s[i]);
    out[i] = 0;
}

static int eq(const char *a, const char *b) {
    while (*a && *a == *b) { a++; b++; }
    return *a == *b;
}

/* Copies at most max-1 bytes without cutting a UTF-8 character; tabs and
 * line breaks become spaces (they separate the stored fields). */
static void copy(char *dst, xbnc_u32 max, const char *src) {
    xbnc_u32 n = 0;
    while (src[n] && n + 1 < max) n++;
    if (src[n]) while (n > 0 && ((unsigned char)src[n] & 0xC0) == 0x80) n--;
    for (xbnc_u32 i = 0; i < n; i++) dst[i] = (src[i] == '\t' || src[i] == '\n' || src[i] == '\r') ? ' ' : src[i];
    dst[n] = 0;
}

static xbnc_u32 hash(const char *k) {
    xbnc_u32 h = 2166136261u;
    while (*k) { h ^= (unsigned char)*k++; h *= 16777619u; }
    return h;
}

/* ------------------------------------------------------------ presence */

static int find(const char *key) {
    xbnc_u32 i = hash(key) & (HASH_SIZE - 1);
    while (H[i]) {
        if (eq(P[H[i] - 1].key, key)) return H[i] - 1;
        i = (i + 1) & (HASH_SIZE - 1);
    }
    return -1;
}

static void forget_all(void) {
    for (int i = 0; i < HASH_SIZE; i++) H[i] = 0;
    n_present = 0;
    for (int c = 0; c < MAX_CHANS; c++) { CH[c][0] = 0; CH_SHOWN[c][0] = 0; }
}

/* Drops people who are in no channel (when the table fills up). */
static void compact(void) {
    int n = 0;
    for (int i = 0; i < n_present; i++) if (P[i].present) P[n++] = P[i];
    n_present = n;
    for (int i = 0; i < HASH_SIZE; i++) H[i] = 0;
    for (int i = 0; i < n_present; i++) {
        xbnc_u32 h = hash(P[i].key) & (HASH_SIZE - 1);
        while (H[h]) h = (h + 1) & (HASH_SIZE - 1);
        H[h] = i + 1;
    }
}

static who_t *person(const char *nick, int create) {
    char key[NICK_MAX];
    fold(nick, key, sizeof key);
    if (!key[0]) return 0;
    int idx = find(key);
    if (idx >= 0) return &P[idx];
    if (!create) return 0;
    if (n_present >= MAX_PRESENT) compact();
    if (n_present >= MAX_PRESENT) return 0;
    who_t *w = &P[n_present];
    copy(w->nick, NICK_MAX, nick);
    copy(w->key, NICK_MAX, key);
    w->present = 0;
    xbnc_u32 h = hash(key) & (HASH_SIZE - 1);
    while (H[h]) h = (h + 1) & (HASH_SIZE - 1);
    H[h] = ++n_present;
    return w;
}

static int chan_index(const char *name, int create) {
    char key[CHAN_MAX];
    fold(name, key, sizeof key);
    int free_slot = -1;
    for (int i = 0; i < MAX_CHANS; i++) {
        if (CH[i][0] && eq(CH[i], key)) return i;
        if (!CH[i][0] && free_slot < 0) free_slot = i;
    }
    if (!create || free_slot < 0) return -1;
    copy(CH[free_slot], CHAN_MAX, key);
    copy(CH_SHOWN[free_slot], CHAN_MAX, name);
    return free_slot;
}

static void chan_clear(int c) {
    unsigned long long bit = 1ull << c;
    for (int i = 0; i < n_present; i++) P[i].present &= ~bit;
}

/* ------------------------------------------------------------ records */

static char kvkey[NICK_MAX + 4];
static char line[TEXT_MAX + NICK_MAX * 2 + CHAN_MAX + 64];

static const char *record_key(const char *nick) {
    char f[NICK_MAX];
    fold(nick, f, sizeof f);
    kvkey[0] = 'u'; kvkey[1] = ':'; kvkey[2] = 0;
    xbnc_cat(kvkey, sizeof kvkey, f);
    return kvkey;
}

static void save_record(const char *nick, int kind, const char *chan, const char *text, const char *other, xbnc_i64 now) {
    static char cnick[NICK_MAX], cchan[CHAN_MAX], cother[NICK_MAX], ctext[TEXT_MAX];
    char num[24], k[2] = {KIND_CH[kind], 0};
    if (!nick || !nick[0]) return;
    copy(cnick, sizeof cnick, nick);
    copy(cchan, sizeof cchan, chan ? chan : "");
    copy(cother, sizeof cother, other ? other : "");
    copy(ctext, sizeof ctext, text ? text : "");
    xbnc_itoa(now, num);
    line[0] = 0;
    xbnc_cat(line, sizeof line, num); xbnc_cat(line, sizeof line, "\t");
    xbnc_cat(line, sizeof line, k); xbnc_cat(line, sizeof line, "\t");
    xbnc_cat(line, sizeof line, cnick); xbnc_cat(line, sizeof line, "\t");
    xbnc_cat(line, sizeof line, cchan); xbnc_cat(line, sizeof line, "\t");
    xbnc_cat(line, sizeof line, cother); xbnc_cat(line, sizeof line, "\t");
    xbnc_cat(line, sizeof line, ctext);
    xbnc_kv_set_str(record_key(nick), line);
}

/* Splits the next tab-separated field into out. */
static const char *field(const char *p, char *out, xbnc_u32 max) {
    xbnc_u32 n = 0;
    while (*p && *p != '\t') { if (n + 1 < max) out[n++] = *p; p++; }
    out[n] = 0;
    return *p == '\t' ? p + 1 : p;
}

static int load_record(const char *nick, rec_t *r) {
    if (xbnc_kv_get_str(record_key(nick), line, sizeof line) < 0) return 0;
    char num[24], k[4];
    const char *p = field(line, num, sizeof num);
    p = field(p, k, sizeof k);
    p = field(p, r->nick, sizeof r->nick);
    p = field(p, r->chan, sizeof r->chan);
    p = field(p, r->other, sizeof r->other);
    field(p, r->text, sizeof r->text);
    xbnc_span v = {num, xbnc_strlen(num)};
    r->ts = xbnc_int(v, 0);
    r->kind = 0;
    for (int c = 1; KIND_CH[c]; c++) if (KIND_CH[c] == k[0]) r->kind = c;
    return r->kind != 0;
}

/* ------------------------------------------------------------ answers */

static char out[1024];

static void say(const char *s) { xbnc_cat(out, sizeof out, s); }
static void sayn(xbnc_i64 v) { char b[24]; xbnc_itoa(v, b); say(b); }

static void say_ago(xbnc_i64 ms) {
    xbnc_i64 s = ms / 1000;
    if (s < 0) s = 0;
    xbnc_i64 d = s / 86400, h = (s % 86400) / 3600, m = (s % 3600) / 60;
    if (es) say("hace ");
    if (d) { sayn(d); say(es ? " d " : "d "); sayn(h); say(" h"); }
    else if (h) { sayn(h); say(" h "); sayn(m); say(" min"); }
    else if (m) { sayn(m); say(" min"); }
    else { sayn(s); say(" s"); }
    if (!es) say(" ago");
}

static void say_activity(const rec_t *r, int show_text) {
    switch (r->kind) {
    case K_MSG:
        say(es ? "escribiendo en " : "talking in "); say(r->chan);
        if (show_text && r->text[0]) { say(es ? ": «" : ": \""); say(r->text); say(es ? "»" : "\""); }
        break;
    case K_ACTION:
        say(es ? "en " : "in "); say(r->chan);
        if (show_text && r->text[0]) { say(": * "); say(r->nick); say(" "); say(r->text); }
        break;
    case K_JOIN: say(es ? "entrando en " : "joining "); say(r->chan); break;
    case K_PART:
        say(es ? "saliendo de " : "leaving "); say(r->chan);
        if (show_text && r->text[0]) { say(" ("); say(r->text); say(")"); }
        break;
    case K_QUIT:
        say(es ? "desconectándose" : "quitting");
        if (show_text && r->text[0]) { say(" ("); say(r->text); say(")"); }
        break;
    case K_KICKED:
        say(es ? "expulsado de " : "kicked from "); say(r->chan);
        if (r->other[0]) { say(es ? " por " : " by "); say(r->other); }
        if (show_text && r->text[0]) { say(" ("); say(r->text); say(")"); }
        break;
    case K_NICK_TO: say(es ? "cambiando su nick a " : "changing nick to "); say(r->other); break;
    case K_NICK_FROM: say(es ? "cambiando su nick desde " : "changing nick from "); say(r->other); break;
    }
}

static void describe(const char *who, const char *asker, const char *me, int connected, int show_text, xbnc_i64 now) {
    char key[NICK_MAX], k2[NICK_MAX];
    static rec_t r;
    out[0] = 0;
    fold(who, key, sizeof key);
    fold(me, k2, sizeof k2);
    if (eq(key, k2)) { say(es ? "Soy yo: estoy aquí." : "That's me: I'm right here."); return; }
    fold(asker, k2, sizeof k2);
    if (asker[0] && eq(key, k2)) {
        say(es ? "Te estás buscando a ti mismo: estás aquí." : "Looking for yourself? You're right here.");
        return;
    }
    int has = load_record(who, &r);
    const who_t *w = person(who, 0);
    if (w && w->present && connected) {
        int count = 0, n = 0;
        for (int c = 0; c < MAX_CHANS; c++) if (w->present & (1ull << c)) count++;
        say(w->nick);
        say(es ? " está ahora en " : " is in ");
        for (int c = 0; c < MAX_CHANS; c++) {
            if (!(w->present & (1ull << c))) continue;
            if (n) say(n + 1 == count ? (es ? " y " : " and ") : ", ");
            say(CH_SHOWN[c]);
            n++;
        }
        say(es ? "." : " right now.");
        if (has) {
            say(es ? " Última actividad " : " Last activity ");
            say_ago(now - r.ts);
            say(": ");
            say_activity(&r, show_text);
            say(".");
        }
        return;
    }
    if (!has) {
        say(es ? "No he visto a " : "I haven't seen "); say(who); say(".");
        if (!connected) say(es ? " (Ahora no estoy conectado a la red.)" : " (I'm not connected to the network right now.)");
        return;
    }
    say(r.nick);
    say(es ? " fue visto por última vez " : " was last seen ");
    say_ago(now - r.ts);
    say(", ");
    say_activity(&r, show_text);
    say(".");
}

/* ------------------------------------------------------------ helpers */

static int cooldown_ok(const char *conv, xbnc_i64 now, xbnc_i64 secs) {
    if (secs <= 0) return 1;
    char key[CHAN_MAX];
    fold(conv, key, sizeof key);
    int slot = -1, oldest = 0;
    for (int i = 0; i < MAX_COOL; i++) {
        if (COOL_KEY[i][0] && eq(COOL_KEY[i], key)) { slot = i; break; }
        if (COOL_TS[i] < COOL_TS[oldest]) oldest = i;
    }
    if (slot >= 0 && now - COOL_TS[slot] < secs * 1000) return 0;
    if (slot < 0) { slot = oldest; copy(COOL_KEY[slot], CHAN_MAX, key); }
    COOL_TS[slot] = now;
    return 1;
}

static int channel_allowed(const xbnc_event *ev, const char *chan) {
    static char list[2048];
    char key[CHAN_MAX], item[CHAN_MAX];
    xbnc_config(ev, "channels", list, sizeof list);
    fold(chan, key, sizeof key);
    int any = 0;
    xbnc_u32 i = 0;
    while (list[i]) {
        xbnc_u32 n = 0;
        while (list[i] == ' ' || list[i] == ',' || list[i] == '\n') i++;
        while (list[i] && list[i] != '\n' && list[i] != ',' && list[i] != ' ') {
            if (n + 1 < sizeof item) item[n++] = (char)fold_c((unsigned char)list[i]);
            i++;
        }
        item[n] = 0;
        if (n) { any = 1; if (eq(item, key)) return 1; }
    }
    return !any;
}

static const char *query(const char *text, const char *trigger) {
    if (!trigger[0] || !xbnc_starts_with(text, trigger)) return 0;
    const char *p = text + xbnc_strlen(trigger);
    if (*p != ' ') return 0;
    while (*p == ' ') p++;
    return *p ? p : 0;
}

static void first_word(const char *s, char *w, xbnc_u32 max) {
    xbnc_u32 n = 0;
    while (*s && *s != ' ' && n + 1 < max) w[n++] = *s++;
    /* "!seen bob." or "!seen bob?" */
    while (n > 0 && (w[n - 1] == '.' || w[n - 1] == '?' || w[n - 1] == '!' || w[n - 1] == ',')) n--;
    w[n] = 0;
}

/* ------------------------------------------------------------ events */

void on_event(const xbnc_event *ev) {
    static char kind[16], chan[CHAN_MAX * 2], from[NICK_MAX * 2], text[1024], me[NICK_MAX * 2];
    static char trigger[32], lang[8], target[NICK_MAX * 2], who[NICK_MAX * 2], newn[NICK_MAX * 2];
    xbnc_i64 now = xbnc_now_ms();
    xbnc_get(ev, "event", kind, sizeof kind);
    xbnc_get(ev, "nick", me, sizeof me);
    xbnc_config(ev, "lang", lang, sizeof lang);
    if (!(lang[0] == 'e' && (lang[1] == 's' || lang[1] == 'n'))) xbnc_get(ev, "lang", lang, sizeof lang);
    es = !(lang[0] == 'e' && lang[1] == 'n');
    int connected = xbnc_is(ev, "connected");

    if (eq(kind, "connected")) { forget_all(); return; }

    if (eq(kind, "names")) {
        xbnc_get(ev, "channel", chan, sizeof chan);
        int c = chan_index(chan, 1);
        if (c < 0) return;
        chan_clear(c);
        xbnc_span list = xbnc_field(ev->json, "members");
        if (!list.p || list.p[0] != '[') return;
        static char nick[NICK_MAX * 2];
        xbnc_u32 i = 1;
        while (i < list.n) {
            i = xbnc_ws_(list.p, i, list.n);
            xbnc_u32 e = xbnc_skip_(list.p, i, list.n);
            xbnc_span v = {list.p + i, e - i};
            if (xbnc_string(v, nick, sizeof nick) > 0) {
                who_t *w = person(nick, 1);
                if (w) w->present |= 1ull << c;
            }
            i = xbnc_ws_(list.p, e, list.n);
            if (i >= list.n || list.p[i] != ',') break;
            i++;
        }
        return;
    }

    if (eq(kind, "join")) {
        xbnc_get(ev, "channel", chan, sizeof chan);
        xbnc_get(ev, "from", from, sizeof from);
        int c = chan_index(chan, 1);
        if (xbnc_is(ev, "self")) { if (c >= 0) chan_clear(c); return; }
        who_t *w = person(from, 1);
        if (w && c >= 0) w->present |= 1ull << c;
        save_record(from, K_JOIN, chan, 0, 0, now);
        return;
    }

    if (eq(kind, "part") || eq(kind, "kick")) {
        int is_kick = eq(kind, "kick");
        xbnc_get(ev, "channel", chan, sizeof chan);
        xbnc_get(ev, "from", from, sizeof from);
        xbnc_get(ev, is_kick ? "target" : "from", target, sizeof target);
        xbnc_get(ev, "text", text, sizeof text);
        int c = chan_index(chan, 0);
        if (xbnc_is(ev, "self")) {
            if (c >= 0) { chan_clear(c); CH[c][0] = 0; CH_SHOWN[c][0] = 0; }
            return;
        }
        who_t *w = person(target, 0);
        if (w && c >= 0) w->present &= ~(1ull << c);
        if (is_kick) save_record(target, K_KICKED, chan, text, from, now);
        else save_record(target, K_PART, chan, text, 0, now);
        return;
    }

    if (eq(kind, "quit")) {
        xbnc_get(ev, "from", from, sizeof from);
        xbnc_get(ev, "text", text, sizeof text);
        who_t *w = person(from, 0);
        if (w) w->present = 0;
        save_record(from, K_QUIT, 0, text, 0, now);
        return;
    }

    if (eq(kind, "nick")) {
        xbnc_get(ev, "from", from, sizeof from);
        xbnc_get(ev, "new", newn, sizeof newn);
        if (xbnc_is(ev, "self")) return;
        who_t *o = person(from, 0);
        unsigned long long pres = o ? o->present : 0;
        if (o) o->present = 0;
        save_record(from, K_NICK_TO, 0, 0, newn, now);
        who_t *n = person(newn, 1);
        if (n) n->present = pres;
        save_record(newn, K_NICK_FROM, 0, 0, from, now);
        return;
    }

    if (eq(kind, "message")) {
        if (!xbnc_is(ev, "channel")) return; /* private messages: never stored nor answered */
        xbnc_get(ev, "text", text, sizeof text);
        xbnc_get(ev, "conversation", chan, sizeof chan);
        xbnc_get(ev, "from", from, sizeof from);
        int outgoing = xbnc_is(ev, "outgoing");
        xbnc_config(ev, "trigger", trigger, sizeof trigger);
        if (!trigger[0]) { trigger[0] = 0; xbnc_cat(trigger, sizeof trigger, "!seen"); }
        const char *q = query(text, trigger);
        if (!outgoing) {
            int c = chan_index(chan, 0);
            if (c >= 0) { who_t *w = person(from, 1); if (w) w->present |= 1ull << c; }
            save_record(from, xbnc_is(ev, "action") ? K_ACTION : K_MSG, chan, q ? "" : text, 0, now);
        }
        if (q && (outgoing || xbnc_config_bool(ev, "public")) && channel_allowed(ev, chan)
            && cooldown_ok(chan, now, outgoing ? 0 : xbnc_config_int(ev, "cooldown", 5))) {
            first_word(q, who, NICK_MAX);
            if (!who[0]) return;
            describe(who, outgoing ? "" : from, me, connected, xbnc_config_bool(ev, "show_text"), now);
            xbnc_reply(out);
        }
        return;
    }

    if (eq(kind, "command")) {
        xbnc_get(ev, "text", text, sizeof text);
        first_word(text, who, NICK_MAX);
        if (!who[0] || eq(who, "help") || eq(who, "ayuda")) {
            xbnc_tell(es ? "Uso: /msg *seen <nick>, o escribe «!seen <nick>» en un canal. «/msg *seen stats» muestra quién está ahora en tus canales."
                         : "Usage: /msg *seen <nick>, or write \"!seen <nick>\" in a channel. \"/msg *seen stats\" shows who is in your channels now.");
            return;
        }
        if (eq(who, "stats")) {
            int online = 0, chans = 0;
            for (int i = 0; i < n_present; i++) if (P[i].present) online++;
            for (int c = 0; c < MAX_CHANS; c++) if (CH[c][0]) chans++;
            out[0] = 0;
            sayn(online);
            say(es ? " personas están ahora en tus " : " people are now in your ");
            sayn(chans);
            say(es ? " canales." : " channels.");
            xbnc_tell(out);
            return;
        }
        describe(who, "", me, connected, 1, now);
        xbnc_tell(out);
        return;
    }
}
