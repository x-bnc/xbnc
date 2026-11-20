/*
 * antiflood: protects your channels from people who flood.
 *
 * Someone who writes more than N lines in S seconds, or repeats the same
 * line several times, is warned, kicked or banned for a while (your
 * choice). Only acts in the channels you list, and only works where you
 * are a channel operator. Nicks in "exempt" are never touched.
 *
 * Permissions: "raw" (KICK and MODE +b/-b), "send" (the warning notice)
 * and "timer" (to lift bans when their time is up).
 */
#include "../xbnc_plugin.h"
#include "../xbnc_util.h"

#define SLOTS 2048
#define HIST 20

typedef struct {
    char chan[64];
    char nick[40];
    xbnc_i64 ts[HIST]; /* ring of recent line times */
    int pos;
    xbnc_u32 last_hash;
    int repeats;
    xbnc_i64 last_seen;
    xbnc_i64 warned;   /* last warning */
} track_t;

static track_t T[SLOTS];

static xbnc_u32 hash(const char *k) {
    xbnc_u32 h = 2166136261u;
    while (*k) { h ^= (unsigned char)u_fold_c((unsigned char)*k++); h *= 16777619u; }
    return h;
}

static track_t *slot(const char *chan, const char *nick, xbnc_i64 now) {
    xbnc_u32 h = (hash(chan) * 31u + hash(nick)) & (SLOTS - 1);
    track_t *oldest = 0;
    for (int i = 0; i < 8; i++) {
        track_t *t = &T[(h + (xbnc_u32)i) & (SLOTS - 1)];
        if (t->chan[0] && u_eq_fold(t->chan, chan) && u_eq_fold(t->nick, nick)) return t;
        if (!oldest || t->last_seen < oldest->last_seen) oldest = t;
    }
    /* Reuse the least recently active of the nearby slots. */
    for (int i = 0; i < HIST; i++) oldest->ts[i] = 0;
    oldest->pos = 0;
    oldest->last_hash = 0;
    oldest->repeats = 0;
    oldest->warned = 0;
    u_copy(oldest->chan, sizeof oldest->chan, chan);
    u_copy(oldest->nick, sizeof oldest->nick, nick);
    oldest->last_seen = now;
    return oldest;
}

/* Lifts the bans whose time is up (at most 4 per minute). */
static void unban(xbnc_i64 now) {
    static char keys[4096], line[300];
    int n = xbnc_kv_list("b:", "", keys, sizeof keys);
    if (n <= 0) return;
    int done = 0;
    for (char *cur = keys, *k; (k = u_next_key(&cur));) {
        /* b:<expiry>:<chan> <mask> */
        xbnc_i64 exp = u_atoi(k + 2);
        if (exp > now || done >= 4) break;
        const char *rest = k + 2;
        while (*rest && *rest != ':') rest++;
        if (*rest) {
            line[0] = 0;
            xbnc_cat(line, sizeof line, "MODE ");
            const char *sp = rest + 1;
            char chan[64];
            xbnc_u32 j = 0;
            while (*sp && *sp != ' ' && j + 1 < sizeof chan) chan[j++] = *sp++;
            chan[j] = 0;
            xbnc_cat(line, sizeof line, chan);
            xbnc_cat(line, sizeof line, " -b ");
            xbnc_cat(line, sizeof line, u_skip_spaces(sp));
            xbnc_raw(line);
        }
        xbnc_kv_del(k);
        done++;
    }
}

void on_event(const xbnc_event *ev) {
    static char kind[16], text[600], from[64], mask[200], chan[96], me[64], action[16], reason[200], line[400];
    xbnc_i64 now = xbnc_now_ms();
    xbnc_get(ev, "event", kind, sizeof kind);
    u_lang(ev);
    if (u_eq(kind, "timer")) { if (xbnc_is(ev, "connected")) unban(now); return; }
    if (u_eq(kind, "command")) {
        static char list[512];
        u_reset();
        xbnc_config(ev, "channels", list, sizeof list);
        for (char *c = list; *c; c++) if (*c == '\n' || *c == '\r') *c = ' ';
        if (!list[0]) {
            u_say2("No protege ningún canal todavía: añádelos en el panel (Redes > tu red > Plugins > Anti-flood). Tienes que ser operador en ellos.",
                   "It protects no channel yet: add them in the panel (Networks > your network > Plugins > Anti-flood). You must be an operator there.");
        } else {
            xbnc_config(ev, "action", action, sizeof action);
            u_say2("Protege: ", "Protects: ");
            u_say(list);
            u_say2(". Límite: ", ". Limit: ");
            u_sayn(xbnc_config_int(ev, "lines", 5));
            u_say2(" líneas en ", " lines in ");
            u_sayn(xbnc_config_int(ev, "seconds", 4));
            u_say2(" s. Acción: ", " s. Action: ");
            u_say(action[0] ? action : "kick");
            u_say(".");
        }
        xbnc_tell(u_out);
        return;
    }
    if (!u_eq(kind, "message") || !xbnc_is(ev, "channel") || xbnc_is(ev, "outgoing")) return;
    xbnc_get(ev, "conversation", chan, sizeof chan);
    if (!u_in_list(ev, "channels", chan, 0)) return;
    xbnc_get(ev, "from", from, sizeof from);
    xbnc_get(ev, "nick", me, sizeof me);
    if (!from[0] || u_eq_fold(from, me) || u_in_list(ev, "exempt", from, 0)) return;
    xbnc_get(ev, "text", text, sizeof text);

    track_t *t = slot(chan, from, now);
    t->last_seen = now;
    t->ts[t->pos] = now;
    t->pos = (t->pos + 1) % HIST;
    xbnc_u32 h = hash(text);
    if (h == t->last_hash) t->repeats++; else { t->repeats = 1; t->last_hash = h; }

    xbnc_i64 max_lines = xbnc_config_int(ev, "lines", 5);
    xbnc_i64 window = xbnc_config_int(ev, "seconds", 4) * 1000;
    xbnc_i64 max_repeat = xbnc_config_int(ev, "repeats", 3);
    if (max_lines > HIST) max_lines = HIST;
    int recent = 0;
    for (int i = 0; i < HIST; i++) if (t->ts[i] && now - t->ts[i] <= window) recent++;
    int flood = recent > max_lines;
    int repeat = max_repeat > 1 && t->repeats >= max_repeat;
    if (!flood && !repeat) return;

    xbnc_config(ev, "action", action, sizeof action);
    xbnc_config(ev, "reason", reason, sizeof reason);
    if (!reason[0]) u_copy(reason, sizeof reason, u_es ? "Sin flood, por favor" : "No flooding, please");
    /* Start over so one burst is punished once. */
    for (int i = 0; i < HIST; i++) t->ts[i] = 0;
    t->repeats = 0;

    int warn_first = u_eq(action, "warn");
    if (warn_first && (!t->warned || now - t->warned > 300000)) {
        t->warned = now;
        u_reset();
        u_say(from);
        u_say2(": por favor, no hagas flood en ", ": please do not flood ");
        u_say(chan);
        u_say2(". La próxima vez te echaré.", ". Next time you will be kicked.");
        xbnc_notice(from, u_out);
        return;
    }
    if (u_eq(action, "ban")) {
        xbnc_get(ev, "from_mask", mask, sizeof mask);
        const char *at = mask;
        while (*at && *at != '@') at++;
        if (*at) {
            static char banmask[160], key[300], pad[24];
            banmask[0] = 0;
            xbnc_cat(banmask, sizeof banmask, "*!*");
            xbnc_cat(banmask, sizeof banmask, at);
            line[0] = 0;
            xbnc_cat(line, sizeof line, "MODE ");
            xbnc_cat(line, sizeof line, chan);
            xbnc_cat(line, sizeof line, " +b ");
            xbnc_cat(line, sizeof line, banmask);
            xbnc_raw(line);
            xbnc_i64 minutes = xbnc_config_int(ev, "ban_minutes", 10);
            if (minutes > 0) {
                key[0] = 0;
                xbnc_cat(key, sizeof key, "b:");
                u_pad(now + minutes * 60000, pad, 15);
                xbnc_cat(key, sizeof key, pad);
                xbnc_cat(key, sizeof key, ":");
                xbnc_cat(key, sizeof key, chan);
                xbnc_cat(key, sizeof key, " ");
                xbnc_cat(key, sizeof key, banmask);
                xbnc_kv_set_str(key, "1");
            }
        }
    }
    line[0] = 0;
    xbnc_cat(line, sizeof line, "KICK ");
    xbnc_cat(line, sizeof line, chan);
    xbnc_cat(line, sizeof line, " ");
    xbnc_cat(line, sizeof line, from);
    xbnc_cat(line, sizeof line, " :");
    xbnc_cat(line, sizeof line, reason);
    xbnc_raw(line);
    u_reset();
    u_say2("Expulsado ", "Kicked ");
    u_say(from);
    u_say2(" de ", " from ");
    u_say(chan);
    u_say(flood ? (u_es ? " por flood." : " for flooding.") : (u_es ? " por repetir mensajes." : " for repeating lines."));
    xbnc_tell(u_out);
}
