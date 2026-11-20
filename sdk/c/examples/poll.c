/*
 * poll: quick polls in a channel.
 *
 *   !poll Pizza or sushi tonight? | pizza | sushi | both
 *   !vote 2            (one vote per person; voting again changes it)
 *   !poll              (shows the count so far)
 *   !poll end          (the person who started it, or you, closes it)
 *
 * It closes on its own after the minutes set in the settings. Permissions:
 * "send" (to talk in the channel) and "timer" (to close it on time).
 */
#include "../xbnc_plugin.h"
#include "../xbnc_util.h"

#define MAX_OPTS 10

typedef struct {
    char creator[40];
    xbnc_i64 started, ends;
    char question[200];
    int n;
    char opts[MAX_OPTS][60];
    int votes[MAX_OPTS];
} poll_t;

static void poll_key(const char *chan, char *key) {
    char f[64];
    u_fold(chan, f, sizeof f);
    key[0] = 0;
    xbnc_cat(key, 96, "p:");
    xbnc_cat(key, 96, f);
}

static int load(const char *key, poll_t *p) {
    static char rec[2048];
    if (xbnc_kv_get_str(key, rec, sizeof rec) < 0) return 0;
    char *f[7];
    if (u_split(rec, '\t', f, 7) < 7) return 0;
    u_copy(p->creator, sizeof p->creator, f[0]);
    p->started = u_atoi(f[1]);
    p->ends = u_atoi(f[2]);
    u_copy(p->question, sizeof p->question, f[3]);
    p->n = (int)u_atoi(f[4]);
    if (p->n < 2 || p->n > MAX_OPTS) return 0;
    char *o[MAX_OPTS], *c[MAX_OPTS];
    int no = u_split(f[5], '\x1f', o, MAX_OPTS), nc = u_split(f[6], ',', c, MAX_OPTS);
    for (int i = 0; i < p->n; i++) {
        u_copy(p->opts[i], sizeof p->opts[i], i < no ? o[i] : "?");
        p->votes[i] = i < nc ? (int)u_atoi(c[i]) : 0;
    }
    return 1;
}

static void save(const char *key, const poll_t *p) {
    static char rec[2048];
    rec[0] = 0;
    u_field(rec, sizeof rec, p->creator, 0);
    u_fieldn(rec, sizeof rec, p->started, 0);
    u_fieldn(rec, sizeof rec, p->ends, 0);
    u_field(rec, sizeof rec, p->question, 0);
    u_fieldn(rec, sizeof rec, p->n, 0);
    for (int i = 0; i < p->n; i++) {
        xbnc_cat(rec, sizeof rec, p->opts[i]);
        xbnc_cat(rec, sizeof rec, i + 1 < p->n ? "\x1f" : "\t");
    }
    for (int i = 0; i < p->n; i++) {
        char b[16];
        xbnc_itoa(p->votes[i], b);
        xbnc_cat(rec, sizeof rec, b);
        if (i + 1 < p->n) xbnc_cat(rec, sizeof rec, ",");
    }
    xbnc_kv_set_str(key, rec);
}

static void say_results(const poll_t *p, int final) {
    int total = 0, best = -1;
    for (int i = 0; i < p->n; i++) { total += p->votes[i]; if (best < 0 || p->votes[i] > p->votes[best]) best = i; }
    u_reset();
    u_say(final ? (u_es ? "Encuesta cerrada: " : "Poll closed: ") : (u_es ? "Encuesta: " : "Poll: "));
    u_say(p->question);
    u_say(" —");
    for (int i = 0; i < p->n; i++) {
        u_say(" ");
        u_sayn(i + 1);
        u_say(") ");
        u_say(p->opts[i]);
        u_say(": ");
        u_sayn(p->votes[i]);
        if (total) { u_say(" ("); u_sayn(p->votes[i] * 100 / total); u_say("%)"); }
        if (i + 1 < p->n) u_say(",");
    }
    u_say(". ");
    if (final && total) {
        int tie = 0;
        for (int i = 0; i < p->n; i++) if (i != best && p->votes[i] == p->votes[best]) tie = 1;
        if (tie) u_say2("Empate.", "It is a tie.");
        else { u_say2("Gana: ", "Winner: "); u_say(p->opts[best]); u_say("."); }
    } else if (!total) {
        u_say2("Sin votos todavía.", "No votes yet.");
    } else {
        u_sayn(total);
        if (total == 1) u_say2(" voto.", " vote.");
        else u_say2(" votos.", " votes.");
    }
}

/* Deletes every vote of a channel's poll. */
static void clear_votes(const char *chan) {
    static char prefix[96], keys[8192];
    char f[64];
    u_fold(chan, f, sizeof f);
    prefix[0] = 0;
    xbnc_cat(prefix, sizeof prefix, "v:");
    xbnc_cat(prefix, sizeof prefix, f);
    xbnc_cat(prefix, sizeof prefix, ":");
    for (int round = 0; round < 50; round++) {
        int n = xbnc_kv_list(prefix, "", keys, sizeof keys);
        if (n <= 0) return;
        for (char *cur = keys, *k; (k = u_next_key(&cur));) {
            xbnc_kv_del(k);
        }
    }
}

static void finish(const char *key, const char *chan, const poll_t *p) {
    say_results(p, 1);
    xbnc_privmsg(chan, u_out);
    xbnc_kv_del(key);
    clear_votes(chan);
}

/* Closes the polls whose time is up (at most 4 per minute). */
static void tick(xbnc_i64 now) {
    static char keys[8192];
    int n = xbnc_kv_list("p:", "", keys, sizeof keys);
    if (n <= 0) return;
    int done = 0;
    for (char *cur = keys, *k; (k = u_next_key(&cur));) {
        poll_t p;
        if (done < 4 && load(k, &p) && p.ends <= now) {
            finish(k, k + 2, &p);
            done++;
        }
    }
}

void on_event(const xbnc_event *ev) {
    static char kind[16], text[600], from[64], chan[96], trigger[32], vote[32], me[64], key[96];
    xbnc_i64 now = xbnc_now_ms();
    xbnc_get(ev, "event", kind, sizeof kind);
    u_lang(ev);
    if (u_eq(kind, "timer")) { if (xbnc_is(ev, "connected")) tick(now); return; }
    if (u_eq(kind, "command")) {
        u_reset();
        u_say2("En un canal: !poll ¿Pregunta? | opción 1 | opción 2 (hasta 10). Para votar: !vote 1. Ver el recuento: !poll. Cerrarla: !poll end. También !encuesta y !votar.",
               "In a channel: !poll Question? | option 1 | option 2 (up to 10). To vote: !vote 1. See the count: !poll. Close it: !poll end.");
        xbnc_tell(u_out);
        return;
    }
    if (!u_eq(kind, "message") || !xbnc_is(ev, "channel")) return;
    xbnc_get(ev, "text", text, sizeof text);
    xbnc_get(ev, "from", from, sizeof from);
    xbnc_get(ev, "conversation", chan, sizeof chan);
    xbnc_get(ev, "nick", me, sizeof me);
    int mine = xbnc_is(ev, "outgoing");
    if (mine) u_copy(from, sizeof from, me);
    xbnc_config(ev, "trigger", trigger, sizeof trigger);
    xbnc_config(ev, "vote_trigger", vote, sizeof vote);
    if (!trigger[0]) u_copy(trigger, sizeof trigger, "!poll");
    if (!vote[0]) u_copy(vote, sizeof vote, "!vote");
    poll_key(chan, key);
    poll_t p;
    int have = load(key, &p);

    const char *v = u_command(text, vote);
    if (!v) v = u_command(text, "!vote");
    if (!v) v = u_command(text, "!votar");
    if (!v) v = u_command(text, "!voto");
    if (v) {
        if (!have) {
            u_reset();
            u_say2("No hay ninguna encuesta abierta en ", "There is no open poll in ");
            u_say(chan);
            u_say(".");
            if (!mine) xbnc_notice(from, u_out); else xbnc_tell(u_out);
            return;
        }
        xbnc_i64 choice = u_atoi(v);
        if (choice < 1 || choice > p.n) {
            u_reset();
            u_say(from);
            u_say2(": vota con un número del 1 al ", ": vote with a number from 1 to ");
            u_sayn(p.n);
            u_say(".");
            xbnc_reply(u_out);
            return;
        }
        static char vk[160], prev[16], f[64];
        u_fold(from, f, sizeof f);
        vk[0] = 0;
        xbnc_cat(vk, sizeof vk, "v:");
        xbnc_cat(vk, sizeof vk, key + 2);
        xbnc_cat(vk, sizeof vk, ":");
        xbnc_cat(vk, sizeof vk, f);
        if (xbnc_kv_get_str(vk, prev, sizeof prev) >= 0) {
            int old = (int)u_atoi(prev);
            if (old >= 1 && old <= p.n && p.votes[old - 1] > 0) p.votes[old - 1]--;
        }
        p.votes[choice - 1]++;
        char b[16];
        xbnc_itoa(choice, b);
        xbnc_kv_set_str(vk, b);
        save(key, &p);
        /* Confirmed in private, so a big channel is not flooded. */
        u_reset();
        u_say2("Voto registrado en ", "Vote counted in ");
        u_say(chan);
        u_say(": ");
        u_sayn(choice);
        u_say(") ");
        u_say(p.opts[choice - 1]);
        u_say2(". Puedes cambiarlo votando otra vez.", ". You can change it by voting again.");
        if (!mine) xbnc_notice(from, u_out); else xbnc_tell(u_out);
        return;
    }

    const char *rest = u_command(text, trigger);
    if (!rest) rest = u_command(text, "!poll");
    if (!rest) rest = u_command(text, "!encuesta");
    if (!rest) return;
    if (!*rest) {
        if (!have) {
            u_reset();
            u_say2("No hay ninguna encuesta abierta. Para crear una: ", "There is no open poll. To start one: ");
            u_say(trigger);
            u_say2(" ¿Pregunta? | opción 1 | opción 2", " Question? | option 1 | option 2");
            xbnc_reply(u_out);
            return;
        }
        say_results(&p, 0);
        u_say(" ");
        u_say(vote);
        u_say(" N");
        xbnc_reply(u_out);
        return;
    }
    if (u_eq(rest, "end") || u_eq(rest, "fin") || u_eq(rest, "cerrar")) {
        if (!have) return;
        if (!mine && !u_eq_fold(from, p.creator)) {
            u_reset();
            u_say(from);
            u_say2(": solo quien la creó puede cerrarla.", ": only who started it can close it.");
            xbnc_reply(u_out);
            return;
        }
        finish(key, chan, &p);
        return;
    }
    if (!mine && !xbnc_config_bool(ev, "public")) return;
    u_reset();
    if (have) {
        u_say(from);
        u_say2(": ya hay una encuesta abierta (", ": there is already an open poll (");
        u_say(trigger);
        u_say2(" para verla).", " to see it).");
        xbnc_reply(u_out);
        return;
    }
    static char buf[600];
    u_copy(buf, sizeof buf, rest);
    char *parts[MAX_OPTS + 1];
    /* Options separated by | (or ; when there is no |). */
    char sep = '|';
    int has_bar = 0;
    for (char *c = buf; *c; c++) if (*c == '|') has_bar = 1;
    if (!has_bar) sep = ';';
    int n = u_split(buf, sep, parts, MAX_OPTS + 1);
    for (int i = 0; i < n; i++) {
        char *s = parts[i];
        while (*s == ' ') s++;
        xbnc_u32 l = xbnc_strlen(s);
        while (l && s[l - 1] == ' ') s[--l] = 0;
        parts[i] = s;
    }
    if (n < 3 || !parts[0][0]) {
        u_say2("Para crear una encuesta escribe la pregunta y al menos dos opciones separadas con |, por ejemplo: ",
               "To start a poll write the question and at least two options separated with |, for example: ");
        u_say(trigger);
        u_say2(" ¿Pizza o sushi? | pizza | sushi", " Pizza or sushi? | pizza | sushi");
        xbnc_reply(u_out);
        return;
    }
    u_copy(p.creator, sizeof p.creator, from);
    p.started = now;
    xbnc_i64 minutes = xbnc_config_int(ev, "minutes", 60);
    p.ends = now + minutes * 60000;
    u_copy(p.question, sizeof p.question, parts[0]);
    p.n = n - 1;
    for (int i = 0; i < p.n; i++) {
        u_copy(p.opts[i], sizeof p.opts[i], parts[i + 1][0] ? parts[i + 1] : "?");
        p.votes[i] = 0;
    }
    clear_votes(chan);
    save(key, &p);
    u_say2("Encuesta de ", "Poll by ");
    u_say(from);
    u_say(": ");
    u_say(p.question);
    u_say(" —");
    for (int i = 0; i < p.n; i++) {
        u_say(" ");
        u_sayn(i + 1);
        u_say(") ");
        u_say(p.opts[i]);
    }
    u_say2(". Vota con ", ". Vote with ");
    u_say(vote);
    u_say2(" N. Se cierra en ", " N. Closes in ");
    u_say_span(minutes * 60000);
    u_say(".");
    xbnc_reply(u_out);
}
