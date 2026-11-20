/*
 * chanstats: who talks most in each channel.
 *
 *   !stats          ->  #xbnc: 12345 messages since 3 Nov. Most active:
 *                       ana 1200, bob 900, eva 450...
 *   !stats bob      ->  bob in #xbnc: 900 messages, 4512 words, last seen 3 min ago.
 *
 * Counts are kept in memory and saved to xBNC's plugin storage every
 * minute. Permissions: "send" (to answer) and "timer" (to save).
 */
#include "../xbnc_plugin.h"
#include "../xbnc_util.h"

#define MAX_PENDING 4096
#define TOP 10

typedef struct {
    char chan[64];   /* folded */
    char nick[40];   /* as written */
    xbnc_i64 msgs, words, last;
} delta_t;

static delta_t D[MAX_PENDING];
static int nd;

typedef struct {
    xbnc_i64 total, since;
    int n;
    char nick[TOP][40];
    xbnc_i64 count[TOP];
} chan_t;

static void key_of(char *out, xbnc_u32 max, const char *kind, const char *chan, const char *nick) {
    char f[64];
    out[0] = 0;
    xbnc_cat(out, max, kind);
    xbnc_cat(out, max, chan);
    if (nick) {
        u_fold(nick, f, sizeof f);
        xbnc_cat(out, max, ":");
        xbnc_cat(out, max, f);
    }
}

static void load_chan(const char *chan, chan_t *c) {
    static char k[128], rec[2048];
    key_of(k, sizeof k, "c:", chan, 0);
    c->total = 0; c->since = 0; c->n = 0;
    if (xbnc_kv_get_str(k, rec, sizeof rec) < 0) return;
    char *f[3];
    if (u_split(rec, '\t', f, 3) < 3) return;
    c->total = u_atoi(f[0]);
    c->since = u_atoi(f[1]);
    char *items[TOP];
    int n = f[2][0] ? u_split(f[2], ';', items, TOP) : 0;
    for (int i = 0; i < n; i++) {
        char *eqp = items[i];
        while (*eqp && *eqp != '=') eqp++;
        if (!*eqp) continue;
        *eqp = 0;
        u_copy(c->nick[c->n], 40, items[i]);
        c->count[c->n] = u_atoi(eqp + 1);
        c->n++;
    }
}

static void save_chan(const char *chan, const chan_t *c) {
    static char k[128], rec[2048];
    key_of(k, sizeof k, "c:", chan, 0);
    rec[0] = 0;
    u_fieldn(rec, sizeof rec, c->total, 0);
    u_fieldn(rec, sizeof rec, c->since, 0);
    for (int i = 0; i < c->n; i++) {
        char b[24];
        if (i) xbnc_cat(rec, sizeof rec, ";");
        xbnc_cat(rec, sizeof rec, c->nick[i]);
        xbnc_cat(rec, sizeof rec, "=");
        xbnc_itoa(c->count[i], b);
        xbnc_cat(rec, sizeof rec, b);
    }
    xbnc_kv_set_str(k, rec);
}

/* Puts nick with its new total in the top list. */
static void top_update(chan_t *c, const char *nick, xbnc_i64 count) {
    int at = -1;
    for (int i = 0; i < c->n; i++) if (u_eq_fold(c->nick[i], nick)) at = i;
    if (at < 0) {
        if (c->n < TOP) at = c->n++;
        else if (count > c->count[TOP - 1]) at = TOP - 1;
        else return;
    }
    u_copy(c->nick[at], 40, nick);
    c->count[at] = count;
    while (at > 0 && c->count[at] > c->count[at - 1]) {
        char tn[40];
        u_copy(tn, 40, c->nick[at - 1]);
        xbnc_i64 tc = c->count[at - 1];
        u_copy(c->nick[at - 1], 40, c->nick[at]);
        c->count[at - 1] = c->count[at];
        u_copy(c->nick[at], 40, tn);
        c->count[at] = tc;
        at--;
    }
}

/* Saves up to `limit` pending counts (storage allows 2000 writes per event). */
static void flush(int limit) {
    static char k[160], rec[256];
    static chan_t c;
    while (nd > 0 && limit > 0) {
        /* All pending entries of the channel of the last one. */
        char chan[64];
        u_copy(chan, sizeof chan, D[nd - 1].chan);
        load_chan(chan, &c);
        if (!c.since) c.since = xbnc_now_ms();
        int i = 0;
        while (i < nd && limit > 0) {
            if (!u_eq(D[i].chan, chan)) { i++; continue; }
            delta_t *d = &D[i];
            key_of(k, sizeof k, "s:", chan, d->nick);
            xbnc_i64 msgs = 0, words = 0;
            if (xbnc_kv_get_str(k, rec, sizeof rec) >= 0) {
                char *f[4];
                if (u_split(rec, '\t', f, 4) >= 2) { msgs = u_atoi(f[0]); words = u_atoi(f[1]); }
            }
            msgs += d->msgs;
            words += d->words;
            c.total += d->msgs;
            rec[0] = 0;
            u_fieldn(rec, sizeof rec, msgs, 0);
            u_fieldn(rec, sizeof rec, words, 0);
            u_fieldn(rec, sizeof rec, d->last, 0);
            u_field(rec, sizeof rec, d->nick, 1);
            xbnc_kv_set_str(k, rec);
            top_update(&c, d->nick, msgs);
            D[i] = D[--nd];
            limit--;
        }
        save_chan(chan, &c);
        limit--;
    }
}

static void count(const char *chan, const char *nick, const char *text, xbnc_i64 now) {
    char f[64];
    u_fold(chan, f, sizeof f);
    xbnc_i64 words = 0;
    int in = 0;
    for (const char *p = text; *p; p++) {
        if (*p == ' ') in = 0;
        else if (!in) { in = 1; words++; }
    }
    for (int i = 0; i < nd; i++) {
        if (u_eq(D[i].chan, f) && u_eq_fold(D[i].nick, nick)) {
            D[i].msgs++;
            D[i].words += words;
            D[i].last = now;
            u_copy(D[i].nick, 40, nick);
            return;
        }
    }
    if (nd >= MAX_PENDING) flush(1500);
    if (nd >= MAX_PENDING) return;
    delta_t *d = &D[nd++];
    u_copy(d->chan, sizeof d->chan, f);
    u_copy(d->nick, sizeof d->nick, nick);
    d->msgs = 1;
    d->words = words;
    d->last = now;
}

static void say_date(xbnc_i64 ms) {
    /* Days since the epoch to a civil date (Howard Hinnant's algorithm). */
    static const char *es_m[] = {"ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"};
    static const char *en_m[] = {"Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"};
    xbnc_i64 z = ms / 86400000 + 719468;
    xbnc_i64 era = z / 146097, doe = z - era * 146097;
    xbnc_i64 yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    xbnc_i64 y = yoe + era * 400, doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    xbnc_i64 mp = (5 * doy + 2) / 153, d = doy - (153 * mp + 2) / 5 + 1;
    xbnc_i64 m = mp < 10 ? mp + 3 : mp - 9;
    if (m <= 2) y++;
    u_sayn(d);
    u_say(" ");
    u_say(u_es ? es_m[m - 1] : en_m[m - 1]);
    u_say(" ");
    u_sayn(y);
}

void on_event(const xbnc_event *ev) {
    static char kind[16], text[600], from[64], chan[96], trigger[32], me[64], who[64];
    xbnc_i64 now = xbnc_now_ms();
    xbnc_get(ev, "event", kind, sizeof kind);
    u_lang(ev);
    if (u_eq(kind, "timer")) { flush(1800); return; }
    int to_user = u_eq(kind, "command");
    const char *rest;
    if (to_user) {
        /* /msg *chanstats #channel [nick] */
        xbnc_get(ev, "text", text, sizeof text);
        rest = u_word(text, chan, sizeof chan);
        if (!u_is_channel(chan)) {
            u_reset();
            u_say2("En un canal: !stats o !stats nick. Aquí: /msg *chanstats #canal [nick].",
                   "In a channel: !stats or !stats nick. Here: /msg *chanstats #channel [nick].");
            xbnc_tell(u_out);
            return;
        }
    } else {
        if (!u_eq(kind, "message") || !xbnc_is(ev, "channel")) return;
        xbnc_get(ev, "text", text, sizeof text);
        xbnc_get(ev, "from", from, sizeof from);
        xbnc_get(ev, "conversation", chan, sizeof chan);
        xbnc_get(ev, "nick", me, sizeof me);
        if (xbnc_is(ev, "outgoing")) u_copy(from, sizeof from, me);
        if (!u_in_list(ev, "channels", chan, 1)) return;
        xbnc_config(ev, "trigger", trigger, sizeof trigger);
        if (!trigger[0]) u_copy(trigger, sizeof trigger, "!stats");
        rest = u_command(text, trigger);
        if (!rest) rest = u_command(text, "!stats");
        if (!rest) rest = u_command(text, "!estadisticas");
        if (!rest) rest = u_command(text, "!top");
        if (!rest) {
            count(chan, from, text, now);
            return;
        }
        if (!xbnc_is(ev, "outgoing") && !xbnc_config_bool(ev, "public")) return;
    }
    flush(1800);
    char f[64];
    u_fold(chan, f, sizeof f);
    u_word(rest, who, sizeof who);
    u_reset();
    if (who[0]) {
        static char k[160], rec[256];
        key_of(k, sizeof k, "s:", f, who);
        if (xbnc_kv_get_str(k, rec, sizeof rec) < 0) {
            u_say(who);
            u_say2(" no ha escrito nada en ", " has not written anything in ");
            u_say(chan);
            u_say2(" desde que cuento.", " since I started counting.");
        } else {
            char *fl[4];
            u_split(rec, '\t', fl, 4);
            u_say(fl[3] && fl[3][0] ? fl[3] : who);
            u_say2(" en ", " in ");
            u_say(chan);
            u_say(": ");
            u_say(fl[0]);
            u_say2(" mensajes, ", " messages, ");
            u_say(fl[1]);
            u_say2(" palabras; la última vez ", " words; last time ");
            u_say_ago(now - u_atoi(fl[2]));
            u_say(".");
        }
        u_answer(to_user);
        return;
    }
    static chan_t c;
    load_chan(f, &c);
    if (!c.total) {
        u_say2("Todavía no tengo estadísticas de ", "I have no statistics for ");
        u_say(chan);
        u_say(".");
        u_answer(to_user);
        return;
    }
    u_say(chan);
    u_say(": ");
    u_sayn(c.total);
    u_say2(" mensajes desde el ", " messages since ");
    say_date(c.since);
    u_say2(". Más activos: ", ". Most active: ");
    xbnc_i64 shown = xbnc_config_int(ev, "top", 5);
    for (int i = 0; i < c.n && i < shown; i++) {
        if (i) u_say(", ");
        u_say(c.nick[i]);
        u_say(" ");
        u_sayn(c.count[i]);
    }
    u_say(".");
    u_answer(to_user);
}
