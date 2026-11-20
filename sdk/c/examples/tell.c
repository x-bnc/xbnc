/*
 * tell: leave a message for someone who is not around.
 *
 *   !tell bob the build is fixed
 *   ... later, when bob speaks or joins:  "bob: ana left you a message
 *   2 h 5 min ago: the build is fixed"
 *
 * Permissions: "send" (to deliver in the channel) and "presence" (to
 * deliver as soon as they join). Messages live in xBNC's plugin storage.
 */
#include "../xbnc_plugin.h"
#include "../xbnc_util.h"

#define MAX_TOTAL 5000
#define SET_SIZE 8192 /* power of two, > MAX_TOTAL */
#define NICK_MAX 40

/* Folded nicks with messages waiting (memory only, rebuilt on load). */
static char PENDING[SET_SIZE][NICK_MAX];
static int loaded, total;
static xbnc_u32 seq;

static xbnc_u32 hash(const char *k) {
    xbnc_u32 h = 2166136261u;
    while (*k) { h ^= (unsigned char)*k++; h *= 16777619u; }
    return h;
}

static int set_find(const char *key) {
    xbnc_u32 i = hash(key) & (SET_SIZE - 1);
    for (int n = 0; n < SET_SIZE && PENDING[i][0]; n++, i = (i + 1) & (SET_SIZE - 1))
        if (u_eq(PENDING[i], key)) return (int)i;
    return -1;
}

static void set_add(const char *key) {
    if (set_find(key) >= 0) return;
    xbnc_u32 i = hash(key) & (SET_SIZE - 1);
    for (int n = 0; n < SET_SIZE; n++, i = (i + 1) & (SET_SIZE - 1)) {
        if (!PENDING[i][0] || PENDING[i][0] == 1) { u_copy(PENDING[i], NICK_MAX, key); return; }
    }
}

static void set_del(const char *key) {
    int i = set_find(key);
    if (i >= 0) { PENDING[i][0] = 1; PENDING[i][1] = 0; } /* tombstone */
}

/* Who is in which channel now ("#chan nick", folded), from the presence
 * events. */
#define MEM_SIZE 8192
static char MEM[MEM_SIZE][80];

static void mem_key(const char *chan, const char *nick, char *k) {
    char a[48], b[NICK_MAX];
    u_fold(chan, a, sizeof a);
    u_fold(nick, b, sizeof b);
    k[0] = 0;
    xbnc_cat(k, 80, a);
    xbnc_cat(k, 80, " ");
    xbnc_cat(k, 80, b);
}

static int mem_find(const char *k) {
    xbnc_u32 i = hash(k) & (MEM_SIZE - 1);
    for (int n = 0; n < MEM_SIZE && MEM[i][0]; n++, i = (i + 1) & (MEM_SIZE - 1))
        if (u_eq(MEM[i], k)) return (int)i;
    return -1;
}

static void mem_add(const char *chan, const char *nick) {
    char k[80];
    mem_key(chan, nick, k);
    if (mem_find(k) >= 0) return;
    xbnc_u32 i = hash(k) & (MEM_SIZE - 1);
    for (int n = 0; n < MEM_SIZE; n++, i = (i + 1) & (MEM_SIZE - 1))
        if (!MEM[i][0] || MEM[i][0] == 1) { u_copy(MEM[i], 80, k); return; }
}

static void mem_del(const char *chan, const char *nick) {
    char k[80];
    mem_key(chan, nick, k);
    int i = mem_find(k);
    if (i >= 0) { MEM[i][0] = 1; MEM[i][1] = 0; }
}

static int mem_has(const char *chan, const char *nick) {
    char k[80];
    mem_key(chan, nick, k);
    return mem_find(k) >= 0;
}

/* Removes a nick from every channel (quit), or a whole channel (we left). */
static void mem_drop(const char *chan, const char *nick) {
    char a[48], b[NICK_MAX];
    u_fold(chan ? chan : "", a, sizeof a);
    u_fold(nick ? nick : "", b, sizeof b);
    for (int i = 0; i < MEM_SIZE; i++) {
        if (MEM[i][0] <= 1) continue;
        char *sp = MEM[i];
        while (*sp && *sp != ' ') sp++;
        if (!*sp) continue;
        *sp = 0;
        int hit = chan ? u_eq(MEM[i], a) : u_eq(sp + 1, b);
        *sp = ' ';
        if (hit) { MEM[i][0] = 1; MEM[i][1] = 0; }
    }
}

/* Reads every waiting message once to know who has mail. */
static void load_all(void) {
    static char keys[16384], after[128], nick[NICK_MAX];
    for (int i = 0; i < SET_SIZE; i++) PENDING[i][0] = 0;
    total = 0;
    after[0] = 0;
    for (;;) {
        int n = xbnc_kv_list("m:", after, keys, sizeof keys);
        if (n <= 0) break;
        for (char *cur = keys, *k; (k = u_next_key(&cur));) {
            u_copy(after, sizeof after, k);
            /* m:<nick>:<ts>:<seq> */
            xbnc_u32 j = 0;
            const char *p = k + 2;
            while (*p && *p != ':' && j + 1 < NICK_MAX) nick[j++] = *p++;
            nick[j] = 0;
            if (nick[0]) { set_add(nick); total++; }
        }
    }
    loaded = 1;
}

/* Delivers up to 3 messages for `who` in `where`. */
static void deliver(const char *who, const char *where, xbnc_i64 now) {
    char key[NICK_MAX];
    u_fold(who, key, sizeof key);
    if (set_find(key) < 0) return;
    static char prefix[64], keys[4096], rec[600];
    prefix[0] = 0;
    xbnc_cat(prefix, sizeof prefix, "m:");
    xbnc_cat(prefix, sizeof prefix, key);
    xbnc_cat(prefix, sizeof prefix, ":");
    int n = xbnc_kv_list(prefix, "", keys, sizeof keys);
    if (n <= 0) { set_del(key); return; }
    int sent = 0, left = 0;
    for (char *cur = keys, *k; (k = u_next_key(&cur));) {
        if (sent < 3) {
            if (xbnc_kv_get_str(k, rec, sizeof rec) >= 0) {
                char *f[3];
                if (u_split(rec, '\t', f, 3) == 3) {
                    xbnc_i64 ts = u_atoi(k + xbnc_strlen(prefix));
                    u_reset();
                    u_say(who);
                    u_say(": ");
                    u_say(f[0]);
                    u_say2(" te dejó un mensaje ", " left you a message ");
                    u_say_ago(now - ts);
                    u_say(": ");
                    u_say(f[2]);
                    xbnc_privmsg(where, u_out);
                }
            }
            xbnc_kv_del(k);
            total--;
            sent++;
        } else {
            left++;
        }
    }
    if (left) {
        u_reset();
        u_say(who);
        u_say2(": tienes ", ": you have ");
        u_sayn(left);
        u_say2(" mensajes más; escribe algo para verlos.", " more messages; say something to see them.");
        xbnc_privmsg(where, u_out);
    } else {
        set_del(key);
    }
}

void on_event(const xbnc_event *ev) {
    static char kind[16], text[600], from[64], conv[128], trigger[32], target[64], me[64];
    xbnc_i64 now = xbnc_now_ms();
    xbnc_get(ev, "event", kind, sizeof kind);
    u_lang(ev);
    if (!loaded || u_eq(kind, "load")) load_all();
    if (u_eq(kind, "connected")) { for (int i = 0; i < MEM_SIZE; i++) MEM[i][0] = 0; return; }

    if (u_eq(kind, "command")) {
        u_reset();
        u_say2("En un canal: !tell bob mensaje — bob lo recibe en el canal en cuanto escriba algo o entre. También !avisa.",
               "In a channel: !tell bob message — bob gets it in the channel as soon as he writes something or joins.");
        xbnc_tell(u_out);
        return;
    }

    /* Presence: who is in each channel. */
    if (u_eq(kind, "names")) {
        xbnc_get(ev, "channel", conv, sizeof conv);
        mem_drop(conv, 0);
        xbnc_span list = xbnc_field(ev->json, "members");
        if (list.p && list.p[0] == '[') {
            static char nick[NICK_MAX * 2];
            xbnc_u32 i = 1;
            while (i < list.n) {
                i = xbnc_ws_(list.p, i, list.n);
                xbnc_u32 e = xbnc_skip_(list.p, i, list.n);
                xbnc_span v = {list.p + i, e - i};
                if (xbnc_string(v, nick, sizeof nick) > 0) mem_add(conv, nick);
                i = xbnc_ws_(list.p, e, list.n);
                if (i >= list.n || list.p[i] != ',') break;
                i++;
            }
        }
        return;
    }
    if (u_eq(kind, "part") || u_eq(kind, "kick")) {
        xbnc_get(ev, "channel", conv, sizeof conv);
        xbnc_get(ev, u_eq(kind, "kick") ? "target" : "from", from, sizeof from);
        if (xbnc_is(ev, "self")) mem_drop(conv, 0); else mem_del(conv, from);
        return;
    }
    if (u_eq(kind, "quit")) {
        xbnc_get(ev, "from", from, sizeof from);
        mem_drop(0, from);
        return;
    }
    if (u_eq(kind, "nick")) {
        static char newn[NICK_MAX];
        xbnc_get(ev, "from", from, sizeof from);
        xbnc_get(ev, "new", newn, sizeof newn);
        /* Keep the channels: re-add under the new nick. */
        static char a[48], b[NICK_MAX];
        u_fold(from, b, sizeof b);
        for (int i = 0; i < MEM_SIZE; i++) {
            if (MEM[i][0] <= 1) continue;
            char *sp = MEM[i];
            while (*sp && *sp != ' ') sp++;
            if (!*sp || !u_eq(sp + 1, b)) continue;
            *sp = 0;
            u_copy(a, sizeof a, MEM[i]);
            MEM[i][0] = 1; MEM[i][1] = 0;
            mem_add(a, newn);
        }
        return;
    }
    if (u_eq(kind, "join")) {
        xbnc_get(ev, "from", from, sizeof from);
        xbnc_get(ev, "channel", conv, sizeof conv);
        if (xbnc_is(ev, "self")) return;
        mem_add(conv, from);
        if (!xbnc_is(ev, "connected") || !xbnc_config_bool(ev, "on_join")) return;
        deliver(from, conv, now);
        return;
    }
    if (!u_eq(kind, "message") || !xbnc_is(ev, "connected")) return;
    int outgoing = xbnc_is(ev, "outgoing");
    xbnc_get(ev, "text", text, sizeof text);
    xbnc_get(ev, "from", from, sizeof from);
    xbnc_get(ev, "conversation", conv, sizeof conv);
    xbnc_get(ev, "nick", me, sizeof me);
    if (outgoing) u_copy(from, sizeof from, me);
    xbnc_config(ev, "trigger", trigger, sizeof trigger);
    if (!trigger[0]) u_copy(trigger, sizeof trigger, "!tell");

    const char *rest = u_command(text, trigger);
    if (!rest) rest = u_command(text, "!tell");
    if (!rest) rest = u_command(text, "!avisa");
    if (!rest) {
        if (!outgoing && u_is_channel(conv)) deliver(from, conv, now);
        return;
    }
    const char *msg = u_word(rest, target, sizeof target);
    while (target[0] && target[xbnc_strlen(target) - 1] == ':') target[xbnc_strlen(target) - 1] = 0;
    u_reset();
    if (!target[0] || !*msg) {
        u_say2("Uso: ", "Usage: ");
        u_say(trigger);
        u_say2(" <nick> <mensaje>", " <nick> <message>");
        xbnc_reply(u_out);
        return;
    }
    if (u_eq_fold(target, from) || u_eq_fold(target, me)) {
        u_say(from);
        u_say2(": eso no hace falta.", ": no need for that.");
        xbnc_reply(u_out);
        return;
    }
    char key[NICK_MAX];
    u_fold(target, key, sizeof key);
    static char prefix[64], keys[4096];
    prefix[0] = 0;
    xbnc_cat(prefix, sizeof prefix, "m:");
    xbnc_cat(prefix, sizeof prefix, key);
    xbnc_cat(prefix, sizeof prefix, ":");
    int waiting = xbnc_kv_list(prefix, "", keys, sizeof keys) > 0 ? u_count_keys(keys) : 0;
    if (waiting >= xbnc_config_int(ev, "max_per_nick", 5) || total >= MAX_TOTAL) {
        u_say(from);
        u_say(": ");
        u_say(target);
        u_say2(" ya tiene demasiados mensajes esperando.", " already has too many messages waiting.");
        xbnc_reply(u_out);
        return;
    }
    static char k[96], pad[24], rec[600], t[400];
    k[0] = 0;
    xbnc_cat(k, sizeof k, prefix);
    u_pad(now, pad, 15);
    xbnc_cat(k, sizeof k, pad);
    xbnc_cat(k, sizeof k, ":");
    u_pad(seq++ % 1000, pad, 3);
    xbnc_cat(k, sizeof k, pad);
    rec[0] = 0;
    u_copy(t, sizeof t, msg);
    u_field(rec, sizeof rec, from, 0);
    u_field(rec, sizeof rec, conv, 0);
    u_field(rec, sizeof rec, t, 1);
    if (xbnc_kv_set_str(k, rec) != 0) return;
    set_add(key);
    total++;
    u_say(from);
    if (mem_has(conv, target)) {
        u_say2(": vale; ", ": OK; ");
        u_say(target);
        u_say2(" está en el canal, se lo diré en cuanto escriba algo.", " is in the channel, I will tell them as soon as they write something.");
    } else {
        u_say2(": vale, se lo diré a ", ": OK, I will tell ");
        u_say(target);
        u_say2(" en cuanto escriba o entre en el canal.", " as soon as they write or join the channel.");
    }
    xbnc_reply(u_out);
}
