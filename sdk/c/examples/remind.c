/*
 * remind: reminders in channels and queries.
 *
 *   !remind 10m stretch your legs   ->  in 10 minutes: "ana: reminder: stretch your legs"
 *   !remind 1h30m call Bob
 *   !remind list / !remind clear
 *   /msg *remind 2h take the pizza out   (the reminder comes as a private note)
 *
 * Permissions: "send" (to remind in the conversation) and "timer" (checked
 * every minute, so a reminder can be up to a minute late). Reminders live in
 * xBNC's plugin storage and survive restarts; while the network is
 * disconnected they wait and go out when it is back.
 */
#include "../xbnc_plugin.h"
#include "../xbnc_util.h"

#define MAX_TOTAL 5000
#define TEXT_MAX 300

static xbnc_u32 seq;

static void count_key(const char *nick, char *key) {
    char f[40];
    u_fold(nick, f, sizeof f);
    key[0] = 0;
    xbnc_cat(key, 64, "n:");
    xbnc_cat(key, 64, f);
}

static int get_count(const char *nick) {
    char key[64], v[24];
    count_key(nick, key);
    if (xbnc_kv_get_str(key, v, sizeof v) < 0) return 0;
    return (int)u_atoi(v);
}

static void set_count(const char *nick, int n) {
    char key[64], v[24];
    count_key(nick, key);
    if (n <= 0) { xbnc_kv_del(key); return; }
    xbnc_itoa(n, v);
    xbnc_kv_set_str(key, v);
}

static int total_count(void) {
    char v[24];
    if (xbnc_kv_get_str("total", v, sizeof v) < 0) return 0;
    return (int)u_atoi(v);
}

static void set_total(int n) {
    char v[24];
    xbnc_itoa(n < 0 ? 0 : n, v);
    xbnc_kv_set_str("total", v);
}

/* Walks the reminders in due order: fn returns 0 to stop. */
typedef int (*visit_fn)(const char *key, char *rec, void *ctx);

static void each(visit_fn fn, void *ctx, int limit) {
    static char keys[16384], after[64], rec[512];
    after[0] = 0;
    int seen = 0;
    while (seen < limit) {
        int n = xbnc_kv_list("r:", after, keys, sizeof keys);
        if (n <= 0) return;
        for (char *cur = keys, *k; seen < limit && (k = u_next_key(&cur));) {
            u_copy(after, sizeof after, k);
            seen++;
            if (xbnc_kv_get_str(k, rec, sizeof rec) >= 0) {
                if (!fn(k, rec, ctx)) return;
            }
        }
    }
}

typedef struct { xbnc_i64 now; int sent; int connected; int clients; } due_ctx;

static int deliver(const char *key, char *rec, void *vctx) {
    due_ctx *c = (due_ctx *)vctx;
    xbnc_i64 due = u_atoi(key + 2);
    if (due > c->now || c->sent >= 5) return 0;
    char *f[5];
    if (u_split(rec, '\t', f, 5) < 5) { xbnc_kv_del(key); return 1; }
    /* f: conversation, nick, created, private (1/0), text */
    int private_note = f[3][0] == '1';
    if (private_note ? !c->clients : !c->connected) return 0;
    u_reset();
    if (private_note) {
        u_say2("Recordatorio: ", "Reminder: ");
        u_say(f[4]);
        u_say2(" (lo pediste ", " (you asked ");
        u_say_ago(c->now - u_atoi(f[2]));
        u_say(")");
        xbnc_tell(u_out);
    } else {
        u_say(f[1]);
        u_say2(": recordatorio: ", ": reminder: ");
        u_say(f[4]);
        xbnc_privmsg(f[0], u_out);
        c->sent++;
    }
    xbnc_kv_del(key);
    set_count(f[1], get_count(f[1]) - 1);
    set_total(total_count() - 1);
    return 1;
}

typedef struct { char fold[40]; int shown; int removed; int clear; xbnc_i64 now; } mine_ctx;

static int mine(const char *key, char *rec, void *vctx) {
    mine_ctx *c = (mine_ctx *)vctx;
    char *f[5];
    if (u_split(rec, '\t', f, 5) < 5) return 1;
    char nf[40];
    u_fold(f[1], nf, sizeof nf);
    if (!u_eq(nf, c->fold)) return 1;
    if (c->clear) {
        xbnc_kv_del(key);
        c->removed++;
        return 1;
    }
    if (c->shown >= 5) { c->shown++; return 1; }
    u_say(c->shown ? " | " : " ");
    u_say2("en ", "in ");
    u_say_span(u_atoi(key + 2) - c->now);
    u_say(": ");
    char t[60];
    u_copy(t, sizeof t, f[4]);
    u_say(t);
    c->shown++;
    return 1;
}

/* Unit of a word ("min", "horas"...) in ms, or 0. */
static xbnc_i64 unit_ms(const char *w) {
    static const struct { const char *w; xbnc_i64 ms; } U[] = {
        {"s", 1000}, {"seg", 1000}, {"segs", 1000}, {"segundo", 1000}, {"segundos", 1000}, {"sec", 1000}, {"second", 1000}, {"seconds", 1000},
        {"m", 60000}, {"min", 60000}, {"mins", 60000}, {"minuto", 60000}, {"minutos", 60000}, {"minute", 60000}, {"minutes", 60000},
        {"h", 3600000}, {"hora", 3600000}, {"horas", 3600000}, {"hour", 3600000}, {"hours", 3600000},
        {"d", 86400000}, {"dia", 86400000}, {"dias", 86400000}, {"día", 86400000}, {"días", 86400000}, {"day", 86400000}, {"days", 86400000},
        {"w", 604800000}, {"semana", 604800000}, {"semanas", 604800000}, {"week", 604800000}, {"weeks", 604800000},
    };
    char f[16];
    u_fold(w, f, sizeof f);
    for (unsigned i = 0; i < sizeof U / sizeof U[0]; i++) if (u_eq(f, U[i].w)) return U[i].ms;
    return 0;
}

static int all_digits(const char *w) {
    if (!*w) return 0;
    for (; *w; w++) if (*w < '0' || *w > '9') return 0;
    return 1;
}

static int is_word(const char *w, const char *a, const char *b2) {
    char f[24];
    u_fold(w, f, sizeof f);
    return u_eq(f, a) || (b2 && u_eq(f, b2));
}

/* When: "10m", "1h30m", "10 min", "2 horas y 30 minutos", "en 5 minutos",
 * "18:30", "a las 18:30". Returns the moment (ms UTC) or -1; *rest points
 * to the text after it. */
static xbnc_i64 parse_when(const char *s, xbnc_i64 now, const char *tz, const char **rest) {
    char w[24], w2[24];
    const char *cur = u_skip_spaces(s);
    const char *q = u_word(cur, w, sizeof w);
    if (is_word(w, "en", "in") || is_word(w, "dentro", 0)) {
        cur = q;
        q = u_word(cur, w, sizeof w);
        if (is_word(w, "de", 0)) cur = q;
    }
    q = u_word(cur, w, sizeof w);
    if (is_word(w, "a", "at")) {
        q = u_word(q, w2, sizeof w2);
        if (is_word(w2, "las", "la")) q = u_word(q, w2, sizeof w2);
        xbnc_i64 at = u_next_clock(w2, tz, now);
        if (at < 0) return -1;
        *rest = q;
        return at;
    }
    xbnc_i64 at = u_next_clock(w, tz, now);
    if (at >= 0) { *rest = q; return at; }

    xbnc_i64 total = 0;
    int parts = 0;
    for (;;) {
        q = u_word(cur, w, sizeof w);
        xbnc_i64 d = -1;
        const char *after = q;
        if (all_digits(w)) {
            const char *q2 = u_word(q, w2, sizeof w2);
            xbnc_i64 u = unit_ms(w2);
            if (u) { d = u_atoi(w) * u; after = q2; }
            else if (!parts) d = u_atoi(w) * 60000; /* a bare number is minutes */
        } else if (w[0] >= '0' && w[0] <= '9') {
            d = u_parse_duration(w);
        }
        if (d <= 0) break;
        total += d;
        parts++;
        cur = after;
        if (total > 400LL * 86400000LL) return -1;
        /* Optional "y" / "and" when another amount follows. */
        q = u_word(cur, w2, sizeof w2);
        if (is_word(w2, "y", "and")) {
            char w3[24];
            u_word(q, w3, sizeof w3);
            if (w3[0] >= '0' && w3[0] <= '9') cur = q;
        }
    }
    if (!parts) return -1;
    *rest = cur;
    return now + total;
}

void on_event(const xbnc_event *ev) {
    static char kind[16], text[600], from[64], conv[128], trigger[32], word[32], tz[40];
    xbnc_i64 now = xbnc_now_ms();
    xbnc_get(ev, "event", kind, sizeof kind);
    u_lang(ev);
    xbnc_config(ev, "timezone", tz, sizeof tz);
    if (!trigger[0]) u_copy(trigger, sizeof trigger, "!remind");

    if (u_eq(kind, "timer") || u_eq(kind, "connected") || u_eq(kind, "attach")) {
        due_ctx c = { now, 0, xbnc_is(ev, "connected"), (int)xbnc_int(xbnc_field(ev->json, "clients"), 0) };
        each(deliver, &c, 200);
        return;
    }

    int to_user = u_eq(kind, "command");
    const char *rest;
    if (to_user) {
        xbnc_get(ev, "text", text, sizeof text);
        xbnc_get(ev, "nick", from, sizeof from);
        conv[0] = 0;
        rest = text;
    } else if (u_eq(kind, "message")) {
        xbnc_get(ev, "text", text, sizeof text);
        xbnc_get(ev, "from", from, sizeof from);
        xbnc_get(ev, "conversation", conv, sizeof conv);
        xbnc_config(ev, "trigger", trigger, sizeof trigger);
        rest = u_command(text, trigger);
        static const char *aliases[] = {"!remind", "!recuerda", "!recordar", "!recordatorio", "!recuerdame", "!recuérdame"};
        for (int i = 0; !rest && i < 6; i++) rest = u_command(text, aliases[i]);
        if (!rest) return;
        if (!xbnc_is(ev, "outgoing") && !xbnc_config_bool(ev, "public")) return;
        if (xbnc_is(ev, "outgoing")) xbnc_get(ev, "nick", from, sizeof from);
    } else {
        return;
    }

    /* In Spanish the examples use !recuerda. */
    if (u_es && u_eq(trigger, "!remind")) u_copy(trigger, sizeof trigger, "!recuerda");
    const char *msg = u_word(rest, word, sizeof word);
    u_reset();
    if (!word[0] || u_eq(word, "help") || u_eq(word, "ayuda")) {
        const char *c = to_user ? "/msg *remind" : trigger;
        u_say2("Uso: ", "Usage: ");
        u_say(c);
        u_say2(" 10m sacar la pizza | ", " 10m take the pizza out | ");
        u_say(c);
        u_say2(" 2 horas llamar a Bob | ", " 2 hours call Bob | ");
        u_say(c);
        u_say2(" 18:30 reunión | ", " 18:30 meeting | ");
        u_say(c);
        u_say2(" lista | ", " list | ");
        u_say(c);
        u_say2(" borrar", " clear");
        u_answer(to_user);
        return;
    }
    if (u_eq(word, "list") || u_eq(word, "lista") || u_eq(word, "clear") || u_eq(word, "borrar")) {
        mine_ctx c;
        u_fold(from, c.fold, sizeof c.fold);
        c.shown = 0; c.removed = 0; c.now = now;
        c.clear = word[0] == 'c' || word[0] == 'b';
        if (!c.clear) { u_say(from); u_say(":"); }
        each(mine, &c, MAX_TOTAL);
        u_reset();
        if (c.clear) {
            set_count(from, 0);
            set_total(total_count() - c.removed);
            u_say(from);
            u_say2(": borrados ", ": removed ");
            u_sayn(c.removed);
            u_say2(" recordatorios.", " reminders.");
        } else if (c.shown == 0) {
            u_say(from);
            u_say2(": no tienes recordatorios pendientes.", ": you have no pending reminders.");
        } else {
            u_say(from); u_say(":");
            c.shown = 0;
            each(mine, &c, MAX_TOTAL);
            if (c.shown > 5) { u_say(" (+"); u_sayn(c.shown - 5); u_say(")"); }
        }
        u_answer(to_user);
        return;
    }

    const char *after = 0;
    xbnc_i64 due = parse_when(rest, now, tz, &after);
    msg = after ? u_skip_spaces(after) : "";
    if (due < 0 || !*msg) {
        u_say(from);
        u_say2(": no entiendo cuándo. Ejemplos: ", ": I do not understand when. Examples: ");
        u_say(to_user ? "/msg *remind" : trigger);
        u_say2(" 10m sacar la pizza, ", " 10m take the pizza out, ");
        u_say(to_user ? "/msg *remind" : trigger);
        u_say2(" 18:30 reunión", " 18:30 meeting");
        u_answer(to_user);
        return;
    }
    xbnc_i64 span = due - now;
    if (span < 60000) { span = 60000; due = now + span; }
    if (span > 365LL * 86400000LL) {
        u_say(from);
        u_say2(": como mucho un año.", ": one year at most.");
        u_answer(to_user);
        return;
    }
    xbnc_i64 max = xbnc_config_int(ev, "max_per_nick", 10);
    if (get_count(from) >= max || total_count() >= MAX_TOTAL) {
        u_say(from);
        u_say2(": tienes demasiados recordatorios pendientes.", ": you have too many pending reminders.");
        u_answer(to_user);
        return;
    }
    char key[64], pad[24];
    key[0] = 0;
    xbnc_cat(key, sizeof key, "r:");
    u_pad(due, pad, 15);
    xbnc_cat(key, sizeof key, pad);
    xbnc_cat(key, sizeof key, ":");
    u_pad((now % 1000000) * 10 + (seq++ % 10), pad, 7);
    xbnc_cat(key, sizeof key, pad);
    static char rec[512], t[TEXT_MAX];
    rec[0] = 0;
    u_copy(t, sizeof t, msg);
    u_field(rec, sizeof rec, to_user ? "*" : conv, 0);
    u_field(rec, sizeof rec, from, 0);
    u_fieldn(rec, sizeof rec, now, 0);
    u_field(rec, sizeof rec, to_user ? "1" : "0", 0);
    u_field(rec, sizeof rec, t, 1);
    if (xbnc_kv_set_str(key, rec) != 0) {
        u_say2("No queda espacio para más recordatorios.", "There is no room for more reminders.");
        u_answer(to_user);
        return;
    }
    set_count(from, get_count(from) + 1);
    set_total(total_count() + 1);
    u_say(from);
    u_say2(": vale, te lo recuerdo ", ": OK, I will remind you ");
    if (span >= 3600000) {
        u_say2("a las ", "at ");
        u_say_clock(due, tz);
        if (span >= 86400000 || (due + u_tz_offset(tz, due)) / 86400000 != (now + u_tz_offset(tz, now)) / 86400000) {
            u_say(" (");
            u_say2("dentro de ", "in ");
            u_say_span(span);
            u_say(")");
        }
    } else {
        u_say2("dentro de ", "in ");
        u_say_span(span);
    }
    u_say(".");
    u_answer(to_user);
}
