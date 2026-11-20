/*
 * urltitle: shows the title of videos and pages that people paste.
 *
 *   bob: look https://www.youtube.com/watch?v=dQw4w9WgXcQ
 *   ->   [YouTube] Rick Astley - Never Gonna Give You Up · Rick Astley
 *
 * YouTube, Vimeo, SoundCloud, Dailymotion and Spotify links use their
 * public oEmbed services; Wikipedia articles and GitHub repositories use
 * their public APIs (title and summary).
 * These sites come approved; an administrator can block any of them in the
 * panel (Plugins → “Sites it may connect to”).
 * At most one title per channel every few seconds.
 */
#define XBNC_ARENA_SIZE (2 * 1024 * 1024)
#include "../xbnc_plugin.h"
#include "../xbnc_util.h"

#define SLOTS 16
static char P_CHAN[SLOTS][64];
static char P_SITE[SLOTS][16];
static int P_HTML[SLOTS];
static int p_next;

#define MAX_COOL 32
static char C_CHAN[MAX_COOL][64];
static xbnc_i64 C_TS[MAX_COOL];

static int cooling(const char *chan, xbnc_i64 now, xbnc_i64 ms) {
    int free_slot = 0;
    xbnc_i64 oldest = now;
    for (int i = 0; i < MAX_COOL; i++) {
        if (C_CHAN[i][0] && u_eq_fold(C_CHAN[i], chan)) {
            if (now - C_TS[i] < ms) return 1;
            C_TS[i] = now;
            return 0;
        }
        if (C_TS[i] < oldest) { oldest = C_TS[i]; free_slot = i; }
    }
    u_copy(C_CHAN[free_slot], 64, chan);
    C_TS[free_slot] = now;
    return 0;
}

static int host_is(const char *url, const char *host) {
    const char *p = xbnc_find_ci(url, "://");
    if (!p) return 0;
    p += 3;
    xbnc_u32 n = xbnc_strlen(host);
    /* exact host or a subdomain of it */
    const char *end = p;
    while (*end && *end != '/' && *end != ':' && *end != '?') end++;
    xbnc_u32 len = (xbnc_u32)(end - p);
    if (len < n) return 0;
    const char *tail = end - n;
    for (xbnc_u32 i = 0; i < n; i++) if ((tail[i] | 0x20) != (host[i] | 0x20)) return 0;
    return len == n || tail[-1] == '.';
}

/* Builds the request for a link; returns 0 if the site is not supported. */
static int request_for(const char *url, char *req, xbnc_u32 max, char *site, int *html) {
    static char enc[1200];
    u_urlenc(url, enc, sizeof enc);
    const char *base = 0;
    *html = 0;
    if (host_is(url, "youtube.com") || host_is(url, "youtu.be")) { base = "https://www.youtube.com/oembed?format=json&url="; u_copy(site, 16, "YouTube"); }
    else if (host_is(url, "vimeo.com")) { base = "https://vimeo.com/api/oembed.json?url="; u_copy(site, 16, "Vimeo"); }
    else if (host_is(url, "soundcloud.com")) { base = "https://soundcloud.com/oembed?format=json&url="; u_copy(site, 16, "SoundCloud"); }
    else if (host_is(url, "dailymotion.com") || host_is(url, "dai.ly")) { base = "https://www.dailymotion.com/services/oembed?url="; u_copy(site, 16, "Dailymotion"); }
    else if (host_is(url, "open.spotify.com")) { base = "https://open.spotify.com/oembed?url="; u_copy(site, 16, "Spotify"); }
    else if (host_is(url, "wikipedia.org")) {
        /* https://es.wikipedia.org/wiki/Title → /api/rest_v1/page/summary/Title */
        const char *h = xbnc_find_ci(url, "://") + 3;
        const char *w = xbnc_find_ci(h, "/wiki/");
        if (!w) return 0;
        req[0] = 0;
        xbnc_cat(req, max, "https://");
        xbnc_u32 hl = (xbnc_u32)(w - h);
        char host[80];
        if (hl >= sizeof host) return 0;
        for (xbnc_u32 i = 0; i < hl; i++) host[i] = h[i];
        host[hl] = 0;
        xbnc_cat(req, max, host);
        xbnc_cat(req, max, "/api/rest_v1/page/summary/");
        char page[400];
        u_copy(page, sizeof page, w + 6);
        for (char *p = page; *p; p++) if (*p == '#' || *p == '?') { *p = 0; break; }
        xbnc_cat(req, max, page);
        u_copy(site, 16, "Wikipedia");
        *html = 2;
        return 1;
    } else if (host_is(url, "github.com")) {
        /* https://github.com/owner/repo[/...] → api.github.com/repos/owner/repo */
        const char *p = xbnc_find_ci(url, "github.com/") + 11;
        char owner[100], repo[100];
        xbnc_u32 n = 0;
        while (*p && *p != '/' && n + 1 < sizeof owner) owner[n++] = *p++;
        owner[n] = 0;
        if (*p != '/') return 0;
        p++;
        n = 0;
        while (*p && *p != '/' && *p != '?' && *p != '#' && n + 1 < sizeof repo) repo[n++] = *p++;
        repo[n] = 0;
        if (!owner[0] || !repo[0]) return 0;
        req[0] = 0;
        xbnc_cat(req, max, "https://api.github.com/repos/");
        xbnc_cat(req, max, owner); xbnc_cat(req, max, "/"); xbnc_cat(req, max, repo);
        u_copy(site, 16, "GitHub");
        *html = 3;
        return 1;
    } else return 0;
    req[0] = 0;
    xbnc_cat(req, max, base);
    xbnc_cat(req, max, enc);
    return 1;
}

void on_event(const xbnc_event *ev) {
    static char kind[16], text[512], conv[64], url[600], req[1600], id[8];
    static char body[262144], title[400], author[200];
    xbnc_get(ev, "event", kind, sizeof kind);
    u_lang(ev);
    if (u_eq(kind, "command")) {
        xbnc_tell(u_es ? "Muestra el título de los enlaces de YouTube, Vimeo, SoundCloud, Dailymotion, Spotify, Wikipedia y GitHub que se pegan en tus canales. Un administrador puede bloquear cualquiera de estos sitios en el panel."
                       : "Shows the title of YouTube, Vimeo, SoundCloud, Dailymotion, Spotify, Wikipedia and GitHub links pasted in your channels. An administrator can block any of these sites in the panel.");
        return;
    }
    if (u_eq(kind, "http")) {
        xbnc_get(ev, "id", id, sizeof id);
        int slot = (int)u_atoi(id);
        if (slot < 0 || slot >= SLOTS || !P_CHAN[slot][0]) return;
        if (xbnc_int(xbnc_field(ev->json, "status"), 0) != 200) { P_CHAN[slot][0] = 0; return; }
        xbnc_get(ev, "body", body, sizeof body);
        title[0] = author[0] = 0;
        xbnc_span b = {body, xbnc_strlen(body)};
        if (P_HTML[slot] == 3) {
            xbnc_string(xbnc_field(b, "full_name"), title, sizeof title);
            static char desc[300];
            desc[0] = 0;
            xbnc_string(xbnc_field(b, "description"), desc, sizeof desc);
            xbnc_i64 stars = xbnc_int(xbnc_field(b, "stargazers_count"), -1);
            if (desc[0]) { xbnc_cat(title, sizeof title, " \xe2\x80\x94 "); xbnc_cat(title, sizeof title, desc); }
            if (stars >= 0) { char n[24]; xbnc_itoa(stars, n); xbnc_cat(title, sizeof title, " (\xe2\x98\x85 "); xbnc_cat(title, sizeof title, n); xbnc_cat(title, sizeof title, ")"); }
        } else {
            xbnc_string(xbnc_field(b, "title"), title, sizeof title);
            xbnc_string(xbnc_field(b, P_HTML[slot] == 2 ? "description" : "author_name"), author, sizeof author);
        }
        if (title[0]) {
            u_reset();
            u_say("["); u_say(P_SITE[slot]); u_say("] ");
            u_say(title);
            if (author[0] && !xbnc_find_ci(title, author)) { u_say(" \xc2\xb7 "); u_say(author); }
            xbnc_privmsg(P_CHAN[slot], u_out);
        }
        P_CHAN[slot][0] = 0;
        return;
    }
    if (!u_eq(kind, "message") || !xbnc_is(ev, "channel")) return;
    if (xbnc_is(ev, "outgoing") && !xbnc_config_bool(ev, "own")) return;
    xbnc_get(ev, "text", text, sizeof text);
    xbnc_get(ev, "conversation", conv, sizeof conv);
    if (!u_in_list(ev, "channels", conv, 1)) return;
    if (!u_find_url(text, url, sizeof url)) return;
    char site[16];
    int html;
    if (!request_for(url, req, sizeof req, site, &html)) return;
    if (cooling(conv, xbnc_now_ms(), xbnc_config_int(ev, "cooldown", 5) * 1000)) return;
    int slot = p_next;
    p_next = (p_next + 1) % SLOTS;
    u_copy(P_CHAN[slot], 64, conv);
    u_copy(P_SITE[slot], 16, site);
    P_HTML[slot] = html;
    xbnc_itoa(slot, id);
    xbnc_http_get(id, req);
}
