/*
 * news: posts new entries from RSS and Atom feeds in your channels.
 *
 * Setting "feeds", one per line:  #xbnc https://hnrss.org/frontpage
 *   ->  [News] Title of the entry — https://link...
 *
 * Each feed is checked every few minutes (setting "interval"); the first
 * check only remembers what is there, so nothing old is posted. Only the
 * sites this plugin lists can be read; they come approved and an
 * administrator can block any of them (Plugins → “Sites it may connect to”).
 * /msg *news list shows the feeds and /msg *news check checks them now.
 */
#define XBNC_ARENA_SIZE (2 * 1024 * 1024)
#include "../xbnc_plugin.h"
#include "../xbnc_util.h"

#define MAX_FEEDS 20
static char F_CHAN[MAX_FEEDS][64];
static char F_URL[MAX_FEEDS][400];
static xbnc_i64 F_NEXT[MAX_FEEDS];
static int F_BUSY[MAX_FEEDS];
static char F_ERR[MAX_FEEDS][160]; /* last error, shown by "list" */
static int F_TOLD[MAX_FEEDS];      /* the user was told about this error */
static int n_feeds;
static char feeds_cfg[8192];

static xbnc_u32 hash(const char *k) {
    xbnc_u32 h = 2166136261u;
    while (*k) { h ^= (unsigned char)*k++; h *= 16777619u; }
    return h;
}

/* Reads the "feeds" setting when it changes. */
static void load(const xbnc_event *ev) {
    static char cfg[8192];
    xbnc_config(ev, "feeds", cfg, sizeof cfg);
    if (u_eq(cfg, feeds_cfg)) return;
    /* A byte copy: u_copy() turns new lines into spaces, and then the
     * setting never matched again, so the feeds were reloaded (and only the
     * first one checked) at every event. */
    xbnc_u32 k = 0;
    while (cfg[k] && k + 1 < sizeof feeds_cfg) { feeds_cfg[k] = cfg[k]; k++; }
    feeds_cfg[k] = 0;
    n_feeds = 0;
    char *p = cfg;
    while (*p && n_feeds < MAX_FEEDS) {
        char *line = p;
        while (*p && *p != '\n') p++;
        if (*p) *p++ = 0;
        char chan[64], url[400];
        const char *r = u_word(line, chan, sizeof chan);
        u_word(r, url, sizeof url);
        if (!u_is_channel(chan) || !xbnc_starts_with(url, "https://")) continue;
        u_copy(F_CHAN[n_feeds], 64, chan);
        u_copy(F_URL[n_feeds], 400, url);
        F_NEXT[n_feeds] = 0;
        F_BUSY[n_feeds] = 0;
        F_ERR[n_feeds][0] = 0;
        F_TOLD[n_feeds] = 0;
        n_feeds++;
    }
}

static void fetch(int i) {
    char id[8];
    xbnc_itoa(i, id);
    F_BUSY[i] = 1;
    xbnc_http_get(id, F_URL[i]);
}

/* The link of an entry: <link>url</link> (RSS) or <link href="url"/> (Atom). */
static void entry_link(const char *item, const char *end, char *out, xbnc_u32 max) {
    out[0] = 0;
    const char *p = item;
    while ((p = xbnc_find_ci(p, "<link")) && p < end) {
        const char *q = p + 5;
        if (*q != '>' && *q != ' ' && *q != '/') { p = q; continue; }
        if (*q == '>') {
            char tmp[400];
            if (u_tag_text(p, "link", tmp, sizeof tmp)) { u_html_text(tmp); u_copy(out, max, tmp); }
            return;
        }
        const char *h = xbnc_find_ci(q, "href=\"");
        const char *close = xbnc_find_ci(q, ">");
        if (h && (!close || h < close)) {
            h += 6;
            xbnc_u32 n = 0;
            while (h[n] && h[n] != '"' && n + 1 < max) { out[n] = h[n]; n++; }
            out[n] = 0;
            u_html_text(out);
            /* Prefer rel="alternate" or no rel at all. */
            const char *rel = xbnc_find_ci(q, "rel=\"");
            if (!rel || (close && rel > close) || xbnc_starts_with(rel + 5, "alternate")) return;
        }
        p = q;
    }
}

static void on_feed(int i, char *body) {
    static char key[40], last[400], newest[400], title[400], link[400];
    static const char *starts[8];
    key[0] = 0;
    xbnc_cat(key, sizeof key, "last:");
    char h[16];
    xbnc_itoa(hash(F_URL[i]) ^ hash(F_CHAN[i]), h);
    xbnc_cat(key, sizeof key, h);
    int known = xbnc_kv_get_str(key, last, sizeof last) > 0;
    /* Collect up to 8 entries, newest first (feeds list them that way). */
    int n = 0;
    const char *p = body;
    const char *tag = xbnc_find_ci(body, "<entry") ? "<entry" : "<item";
    while (n < 8 && (p = xbnc_find_ci(p, tag))) {
        char nx = p[xbnc_strlen(tag)];
        if (nx == '>' || nx == ' ' || nx == '\n' || nx == '\r') starts[n++] = p;
        p += 4;
    }
    if (!n) return;
    newest[0] = 0;
    int stop = n;
    for (int k = 0; k < n; k++) {
        const char *end = k + 1 < n ? starts[k + 1] : body + xbnc_strlen(body);
        entry_link(starts[k], end, link, sizeof link);
        if (!link[0]) u_tag_text(starts[k], "guid", link, sizeof link);
        if (k == 0) u_copy(newest, sizeof newest, link);
        if (known && u_eq(link, last)) { stop = k; break; }
    }
    if (newest[0]) xbnc_kv_set_str(key, newest);
    if (!known) return; /* first look: remember only */
    int max = 3;
    if (stop > max) stop = max;
    char label[40];
    u_copy(label, sizeof label, u_es ? "Noticias" : "News");
    for (int k = stop - 1; k >= 0; k--) {
        const char *end = k + 1 < n ? starts[k + 1] : body + xbnc_strlen(body);
        title[0] = 0;
        u_tag_text(starts[k], "title", title, sizeof title);
        u_html_text(title);
        entry_link(starts[k], end, link, sizeof link);
        if (!title[0]) continue;
        u_reset();
        u_say("["); u_say(label); u_say("] ");
        u_say(title);
        if (link[0]) { u_say(" \xe2\x80\x94 "); u_say(link); }
        xbnc_privmsg(F_CHAN[i], u_out);
    }
}

void on_event(const xbnc_event *ev) {
    static char kind[16], text[256], id[8], body[262144];
    xbnc_get(ev, "event", kind, sizeof kind);
    u_lang(ev);
    load(ev);
    xbnc_i64 now = xbnc_now_ms();
    if (u_eq(kind, "http")) {
        xbnc_get(ev, "id", id, sizeof id);
        int i = (int)u_atoi(id);
        if (i < 0 || i >= n_feeds) return;
        F_BUSY[i] = 0;
        if (xbnc_int(xbnc_field(ev->json, "status"), 0) != 200) {
            char err[200];
            xbnc_get(ev, "error", err, sizeof err);
            if (!err[0]) {
                char st[16];
                xbnc_itoa(xbnc_int(xbnc_field(ev->json, "status"), 0), st);
                u_copy(err, sizeof err, "HTTP ");
                xbnc_cat(err, sizeof err, st);
            }
            u_copy(F_ERR[i], sizeof F_ERR[0], err);
            u_reset(); u_say("news: "); u_say(F_URL[i]); u_say(": "); u_say(err);
            xbnc_log(u_out);
            /* Tell the user once per feed, not at every check. */
            if (!F_TOLD[i]) {
                F_TOLD[i] = 1;
                u_reset();
                u_say2("No puedo leer ", "Cannot read ");
                u_say(F_URL[i]); u_say(": "); u_say(err);
                u_say2(". Solo se pueden usar los sitios de la lista del plugin.", ". Only the sites in the plugin's list can be used.");
                xbnc_tell(u_out);
            }
            return;
        }
        F_ERR[i][0] = 0;
        F_TOLD[i] = 0;
        xbnc_get(ev, "body", body, sizeof body);
        on_feed(i, body);
        return;
    }
    if (u_eq(kind, "timer")) {
        xbnc_i64 every = xbnc_config_int(ev, "interval", 15);
        if (every < 5) every = 5;
        /* One request per timer tick, spread over the feeds. */
        for (int i = 0; i < n_feeds; i++) {
            if (F_BUSY[i] || now < F_NEXT[i]) continue;
            F_NEXT[i] = now + every * 60000 + i * 7000;
            fetch(i);
            break;
        }
        return;
    }
    if (u_eq(kind, "command")) {
        xbnc_get(ev, "text", text, sizeof text);
        char w[16];
        u_word(text, w, sizeof w);
        if (u_eq(w, "list") || u_eq(w, "lista")) {
            if (!n_feeds) { xbnc_tell(u_es ? "No hay fuentes. Añádelas en el panel: una por línea, «#canal https://...»." : "No feeds. Add them in the panel: one per line, \"#channel https://...\"."); return; }
            for (int i = 0; i < n_feeds && i < 5; i++) {
                u_reset(); u_say(F_CHAN[i]); u_say(" "); u_say(F_URL[i]);
                if (F_ERR[i][0]) { u_say2(" (error: ", " (error: "); u_say(F_ERR[i]); u_say(")"); }
                xbnc_tell(u_out);
            }
            return;
        }
        if (u_eq(w, "check") || u_eq(w, "comprobar")) {
            for (int i = 0; i < n_feeds; i++) F_NEXT[i] = 0;
            xbnc_tell(u_es ? "Comprobando las fuentes en el próximo minuto." : "Checking the feeds within the next minute.");
            return;
        }
        xbnc_tell(u_es ? "Publica las novedades de fuentes RSS/Atom en tus canales. Configura las fuentes en el panel. Aquí: list, check."
                       : "Posts new entries from RSS/Atom feeds in your channels. Set the feeds in the panel. Here: list, check.");
    }
}
