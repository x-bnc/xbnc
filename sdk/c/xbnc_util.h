/*
 * Small helpers shared by the official plugins: IRC case folding, text
 * building, durations, settings lists and field splitting.
 * Include after xbnc_plugin.h.
 */
#ifndef XBNC_UTIL_H
#define XBNC_UTIL_H

/* The answer being built (one line). */
static char u_out[512];
static char u_es = 1; /* answers in Spanish (setting "lang") */

static void u_reset(void) { u_out[0] = 0; }
static void u_say(const char *s) { xbnc_cat(u_out, sizeof u_out, s); }
static void u_sayn(xbnc_i64 v) { char b[24]; xbnc_itoa(v, b); u_say(b); }
/* Spanish or English. */
static void u_say2(const char *es, const char *en) { u_say(u_es ? es : en); }

/* The language of the answers: the plugin's "lang" setting when it is
 * "es" or "en", otherwise the user's language in xBNC. */
static void u_lang(const xbnc_event *ev) {
    char lang[8];
    xbnc_config(ev, "lang", lang, sizeof lang);
    if (!(lang[0] == 'e' && (lang[1] == 's' || lang[1] == 'n')))
        xbnc_get(ev, "lang", lang, sizeof lang);
    u_es = !(lang[0] == 'e' && lang[1] == 'n');
}

static int u_fold_c(int c) {
    if (c >= 'A' && c <= 'Z') return c + 32;
    if (c == '[') return '{';
    if (c == ']') return '}';
    if (c == '\\') return '|';
    if (c == '~') return '^';
    return c;
}

static void u_fold(const char *s, char *out, xbnc_u32 max) {
    xbnc_u32 i = 0;
    for (; s[i] && i + 1 < max; i++) out[i] = (char)u_fold_c((unsigned char)s[i]);
    out[i] = 0;
}

static int u_eq(const char *a, const char *b) {
    while (*a && *a == *b) { a++; b++; }
    return *a == *b;
}

static int u_eq_fold(const char *a, const char *b) {
    while (*a && u_fold_c((unsigned char)*a) == u_fold_c((unsigned char)*b)) { a++; b++; }
    return u_fold_c((unsigned char)*a) == u_fold_c((unsigned char)*b);
}

/* Copies at most max-1 bytes without cutting a UTF-8 character; tabs and
 * line breaks become spaces (they separate stored fields). */
static void u_copy(char *dst, xbnc_u32 max, const char *src) {
    xbnc_u32 n = 0;
    while (src[n] && n + 1 < max) n++;
    if (src[n]) while (n > 0 && ((unsigned char)src[n] & 0xC0) == 0x80) n--;
    for (xbnc_u32 i = 0; i < n; i++) dst[i] = (src[i] == '\t' || src[i] == '\n' || src[i] == '\r') ? ' ' : src[i];
    dst[n] = 0;
}

static const char *u_skip_spaces(const char *s) { while (*s == ' ') s++; return s; }

/* The first word of s into w; returns what follows it. */
static const char *u_word(const char *s, char *w, xbnc_u32 max) {
    xbnc_u32 n = 0;
    s = u_skip_spaces(s);
    while (*s && *s != ' ') { if (n + 1 < max) w[n++] = *s; s++; }
    w[n] = 0;
    return u_skip_spaces(s);
}

/* "!cmd rest": the rest (may be ""), or 0 when text is not the command. */
static const char *u_command(const char *text, const char *trigger) {
    if (!trigger[0] || !xbnc_starts_with(text, trigger)) return 0;
    const char *p = text + xbnc_strlen(trigger);
    if (*p != ' ' && *p != 0) return 0;
    return u_skip_spaces(p);
}

/* Whether name is in a "lines" setting (one per line, commas or spaces
 * also separate). Empty setting: `empty_means`. */
static int u_in_list(const xbnc_event *ev, const char *setting, const char *name, int empty_means) {
    static char list[2048];
    char key[64], item[64];
    xbnc_config(ev, setting, list, sizeof list);
    u_fold(name, key, sizeof key);
    int any = 0;
    xbnc_u32 i = 0;
    while (list[i]) {
        xbnc_u32 n = 0;
        while (list[i] == ' ' || list[i] == ',' || list[i] == '\n' || list[i] == '\r') i++;
        while (list[i] && list[i] != '\n' && list[i] != '\r' && list[i] != ',' && list[i] != ' ') {
            if (n + 1 < sizeof item) item[n++] = (char)u_fold_c((unsigned char)list[i]);
            i++;
        }
        item[n] = 0;
        if (n) { any = 1; if (u_eq(item, key)) return 1; }
    }
    return any ? 0 : empty_means;
}

/* "1h30m", "45m", "2d", "90s", "10" (minutes). Returns ms, or -1. */
static xbnc_i64 u_parse_duration(const char *s) {
    xbnc_i64 total = 0, n = 0;
    int digits = 0, units = 0;
    for (; *s; s++) {
        char c = (char)u_fold_c((unsigned char)*s);
        if (c >= '0' && c <= '9') { n = n * 10 + (c - '0'); digits = 1; if (n > 100000) return -1; continue; }
        if (!digits) return -1;
        xbnc_i64 mul;
        if (c == 's') mul = 1000;
        else if (c == 'm') mul = 60000;
        else if (c == 'h') mul = 3600000;
        else if (c == 'd') mul = 86400000;
        else if (c == 'w') mul = 7 * 86400000LL;
        else return -1;
        total += n * mul; n = 0; digits = 0; units = 1;
    }
    (void)units;
    if (digits) total += n * 60000; /* a bare number is minutes */
    return total > 0 ? total : -1;
}

/* "2 h 5 min" / "3 d 4 h" / "40 s". */
static void u_say_span(xbnc_i64 ms) {
    xbnc_i64 s = ms / 1000;
    if (s < 0) s = 0;
    xbnc_i64 d = s / 86400, h = (s % 86400) / 3600, m = (s % 3600) / 60;
    if (d) { u_sayn(d); u_say(" d"); if (h) { u_say(" "); u_sayn(h); u_say(" h"); } }
    else if (h) { u_sayn(h); u_say(" h"); if (m) { u_say(" "); u_sayn(m); u_say(" min"); } }
    else if (m) { u_sayn(m); u_say(" min"); }
    else { u_sayn(s); u_say(" s"); }
}

static void u_say_ago(xbnc_i64 ms) {
    if (u_es) u_say("hace ");
    u_say_span(ms);
    if (!u_es) u_say(" ago");
}

/* Fixed-width number for keys that sort by time. */
static void u_pad(xbnc_i64 v, char *out, int width) {
    char tmp[24];
    int n = xbnc_itoa(v < 0 ? 0 : v, tmp);
    int o = 0;
    while (o < width - n) out[o++] = '0';
    for (int i = 0; i < n; i++) out[o++] = tmp[i];
    out[o] = 0;
}

static xbnc_i64 u_atoi(const char *s) {
    xbnc_i64 v = 0; int neg = 0;
    if (*s == '-') { neg = 1; s++; }
    while (*s >= '0' && *s <= '9') v = v * 10 + (*s++ - '0');
    return neg ? -v : v;
}

/* Splits a stored record "a\tb\tc" in place: fields[i] point into rec. */
static int u_split(char *rec, char sep, char **fields, int max) {
    int n = 0;
    if (max <= 0) return 0;
    fields[n++] = rec;
    for (char *p = rec; *p; p++) {
        if (*p == sep) { *p = 0; if (n < max) fields[n++] = p + 1; else break; }
    }
    return n;
}

/* Appends "field" + separator to a record being built. */
static void u_field(char *rec, xbnc_u32 max, const char *v, int last) {
    xbnc_cat(rec, max, v);
    if (!last) xbnc_cat(rec, max, "\t");
}

static void u_fieldn(char *rec, xbnc_u32 max, xbnc_i64 v, int last) {
    char b[24];
    xbnc_itoa(v, b);
    u_field(rec, max, b, last);
}

/* The next key of an xbnc_kv_list() result (keys end with \n), or 0 at
 * the end. The buffer is changed in place. */
static char *u_next_key(char **cursor) {
    char *k = *cursor;
    if (!k || !*k) return 0;
    char *e = k;
    while (*e && *e != '\n') e++;
    if (*e) { *e = 0; *cursor = e + 1; } else *cursor = e;
    return k;
}

/* How many keys an xbnc_kv_list() result holds. */
static int u_count_keys(const char *keys) {
    int n = 0;
    for (; *keys; keys++) if (*keys == '\n') n++;
    return n;
}

static int u_is_channel(const char *s) { return s[0] == '#' || s[0] == '&' || s[0] == '+' || s[0] == '!'; }

/* Sends the built answer: to the conversation (reply) or, when the user
 * wrote to *plugin, back to them (tell). */
static void u_answer(int to_user) {
    if (!u_out[0]) return;
    if (to_user) xbnc_tell(u_out); else xbnc_reply(u_out);
}

/* ---- dates (proleptic Gregorian, days since 1970-01-01) ---- */

static xbnc_i64 u_days_from_civil(xbnc_i64 y, xbnc_i64 m, xbnc_i64 d) {
    y -= m <= 2;
    xbnc_i64 era = (y >= 0 ? y : y - 399) / 400;
    xbnc_i64 yoe = y - era * 400;
    xbnc_i64 doy = (153 * (m + (m > 2 ? -3 : 9)) + 2) / 5 + d - 1;
    xbnc_i64 doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    return era * 146097 + doe - 719468;
}

static void u_civil_from_days(xbnc_i64 z, xbnc_i64 *y, xbnc_i64 *m, xbnc_i64 *d) {
    z += 719468;
    xbnc_i64 era = (z >= 0 ? z : z - 146096) / 146097;
    xbnc_i64 doe = z - era * 146097;
    xbnc_i64 yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    xbnc_i64 doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    xbnc_i64 mp = (5 * doy + 2) / 153;
    *d = doy - (153 * mp + 2) / 5 + 1;
    *m = mp < 10 ? mp + 3 : mp - 9;
    *y = yoe + era * 400 + (*m <= 2);
}

/* Day of the month of the last Sunday of a month. */
static xbnc_i64 u_last_sunday(xbnc_i64 y, xbnc_i64 m) {
    xbnc_i64 nm = m == 12 ? 1 : m + 1, ny = m == 12 ? y + 1 : y;
    xbnc_i64 last = u_days_from_civil(ny, nm, 1) - 1;       /* last day */
    xbnc_i64 wd = (last + 4) % 7;                           /* 0 = Sunday */
    xbnc_i64 y2, m2, d2;
    u_civil_from_days(last - wd, &y2, &m2, &d2);
    return d2;
}

/* Offset from UTC (ms) of a time zone at a moment: "Europe/Madrid" and
 * the other European capitals (with summer time), "UTC", or "+2" / "-5". */
static xbnc_i64 u_tz_offset(const char *tz, xbnc_i64 utc_ms) {
    static const struct { const char *name; int hours; } EU[] = {
        {"europe/london", 0}, {"europe/lisbon", 0}, {"europe/dublin", 0}, {"atlantic/canary", 0}, {"wet", 0},
        {"europe/madrid", 1}, {"europe/paris", 1}, {"europe/berlin", 1}, {"europe/rome", 1},
        {"europe/amsterdam", 1}, {"europe/brussels", 1}, {"europe/vienna", 1}, {"europe/zurich", 1},
        {"europe/stockholm", 1}, {"europe/oslo", 1}, {"europe/copenhagen", 1}, {"europe/warsaw", 1},
        {"europe/prague", 1}, {"europe/budapest", 1}, {"cet", 1},
        {"europe/athens", 2}, {"europe/helsinki", 2}, {"europe/bucharest", 2}, {"europe/sofia", 2}, {"eet", 2},
    };
    char f[40];
    u_fold(tz, f, sizeof f);
    if (!f[0]) u_copy(f, sizeof f, "europe/madrid");
    if (f[0] == '+' || f[0] == '-' || (f[0] >= '0' && f[0] <= '9')) {
        xbnc_i64 h = u_atoi(f[0] == '+' ? f + 1 : f);
        if (h < -12 || h > 14) h = 0;
        return h * 3600000;
    }
    for (unsigned i = 0; i < sizeof EU / sizeof EU[0]; i++) {
        if (!u_eq(f, EU[i].name)) continue;
        /* EU summer time: last Sunday of March 01:00 UTC to last Sunday of October 01:00 UTC. */
        xbnc_i64 y, m, d;
        u_civil_from_days(utc_ms / 86400000, &y, &m, &d);
        xbnc_i64 start = (u_days_from_civil(y, 3, u_last_sunday(y, 3)) * 24 + 1) * 3600000;
        xbnc_i64 end = (u_days_from_civil(y, 10, u_last_sunday(y, 10)) * 24 + 1) * 3600000;
        xbnc_i64 summer = utc_ms >= start && utc_ms < end;
        return (EU[i].hours + summer) * 3600000;
    }
    return 0; /* UTC or unknown */
}

/* "18:30" in local time: the next moment it happens (ms UTC), or -1. */
static xbnc_i64 u_next_clock(const char *s, const char *tz, xbnc_i64 now) {
    xbnc_i64 h = 0, m = 0;
    int n = 0;
    while (*s >= '0' && *s <= '9' && n < 2) { h = h * 10 + (*s++ - '0'); n++; }
    if (!n || (*s != ':' && *s != '.' && *s != 'h')) return -1;
    s++;
    n = 0;
    while (*s >= '0' && *s <= '9' && n < 2) { m = m * 10 + (*s++ - '0'); n++; }
    if (n != 2 || *s || h > 23 || m > 59) return -1;
    xbnc_i64 off = u_tz_offset(tz, now);
    xbnc_i64 local = now + off;
    xbnc_i64 day = local / 86400000;
    xbnc_i64 at = day * 86400000 + (h * 60 + m) * 60000;
    if (at <= local) at += 86400000;
    return at - u_tz_offset(tz, at - off);
}

/* "18:30" for a moment, in local time. */
static void u_say_clock(xbnc_i64 utc_ms, const char *tz) {
    xbnc_i64 local = utc_ms + u_tz_offset(tz, utc_ms);
    xbnc_i64 mins = (local / 60000) % 1440;
    char b[8];
    b[0] = (char)('0' + mins / 600); b[1] = (char)('0' + (mins / 60) % 10); b[2] = ':';
    b[3] = (char)('0' + (mins % 60) / 10); b[4] = (char)('0' + mins % 10); b[5] = 0;
    u_say(b);
}

/* ---- JSON and text helpers for the v1.0 plugins ---- */

/* First element of a JSON array span ("[{...}, ...]"). */
static xbnc_span u_array_first(xbnc_span a) {
    xbnc_span none = {0, 0};
    if (!a.p || a.n < 2 || a.p[0] != '[') return none;
    xbnc_u32 i = xbnc_ws_(a.p, 1, a.n);
    if (i >= a.n || a.p[i] == ']') return none;
    xbnc_u32 e = xbnc_skip_(a.p, i, a.n);
    xbnc_span r = {a.p + i, e - i};
    return r;
}

/* A JSON number rounded to the nearest whole number (fallback if absent). */
static xbnc_i64 u_num_round(xbnc_span v, xbnc_i64 fallback) {
    if (!v.p || !v.n) return fallback;
    xbnc_u32 i = 0;
    int neg = 0, any = 0;
    if (v.p[i] == '-') { neg = 1; i++; }
    xbnc_i64 whole = 0;
    while (i < v.n && v.p[i] >= '0' && v.p[i] <= '9') { whole = whole * 10 + (v.p[i] - '0'); i++; any = 1; }
    if (i < v.n && v.p[i] == '.' && i + 1 < v.n && v.p[i + 1] >= '5' && v.p[i + 1] <= '9') whole++;
    if (!any) return fallback;
    return neg ? -whole : whole;
}

/* The raw text of a JSON number ("40.4165"), for building URLs. */
static void u_num_text(xbnc_span v, char *out, xbnc_u32 max) {
    xbnc_u32 n = 0;
    for (xbnc_u32 i = 0; v.p && i < v.n && n + 1 < max; i++) {
        char c = v.p[i];
        if ((c >= '0' && c <= '9') || c == '-' || c == '.' || c == 'e' || c == 'E' || c == '+') out[n++] = c;
        else break;
    }
    out[n] = 0;
}

/* Lower-case ASCII, accents removed (á→a, ñ→n, ü→u...), other bytes kept:
 * for comparing answers people type. */
static void u_norm(const char *s, char *out, xbnc_u32 max) {
    xbnc_u32 n = 0;
    while (*s && n + 1 < max) {
        unsigned char c = (unsigned char)*s;
        if (c == 0xC3 && s[1]) {
            unsigned char d = (unsigned char)s[1] | 0x20; /* 0x80-0x9F upper → lower */
            char r = 0;
            if (d >= 0xA0 && d <= 0xA5) r = 'a';
            else if (d == 0xA7) r = 'c';
            else if (d >= 0xA8 && d <= 0xAB) r = 'e';
            else if (d >= 0xAC && d <= 0xAF) r = 'i';
            else if (d == 0xB1) r = 'n';
            else if (d >= 0xB2 && d <= 0xB6) r = 'o';
            else if (d >= 0xB9 && d <= 0xBC) r = 'u';
            if (r) { out[n++] = r; s += 2; continue; }
        }
        out[n++] = (char)((c >= 'A' && c <= 'Z') ? c + 32 : c);
        s++;
    }
    while (n > 0 && (out[n - 1] == ' ' || out[n - 1] == '.' || out[n - 1] == '!' || out[n - 1] == '?')) n--;
    out[n] = 0;
}

/* The first http(s) link in text, or 0. Copies it into url. */
static const char *u_find_url(const char *text, char *url, xbnc_u32 max) {
    const char *p = xbnc_find_ci(text, "https://");
    const char *q = xbnc_find_ci(text, "http://");
    if (!p || (q && q < p)) p = q;
    if (!p) return 0;
    xbnc_u32 n = 0;
    while (p[n] && p[n] != ' ' && p[n] != '\t' && p[n] != '>' && p[n] != '"' && n + 1 < max) { url[n] = p[n]; n++; }
    while (n > 0 && (url[n - 1] == ')' || url[n - 1] == '.' || url[n - 1] == ',' || url[n - 1] == ';')) n--;
    url[n] = 0;
    return p;
}

/* Percent-encodes text for a query string. */
static void u_urlenc(const char *s, char *out, xbnc_u32 max) {
    static const char hex[] = "0123456789ABCDEF";
    xbnc_u32 n = 0;
    for (; *s && n + 4 < max; s++) {
        unsigned char c = (unsigned char)*s;
        if ((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '-' || c == '_' || c == '.' || c == '~') out[n++] = (char)c;
        else { out[n++] = '%'; out[n++] = hex[c >> 4]; out[n++] = hex[c & 15]; }
    }
    out[n] = 0;
}

/* Replaces the common HTML entities in place and squeezes whitespace. */
static void u_html_text(char *s) {
    static const struct { const char *e; const char *r; } E[] = {
        {"&amp;", "&"}, {"&quot;", "\""}, {"&#39;", "'"}, {"&#039;", "'"}, {"&apos;", "'"},
        {"&lt;", "<"}, {"&gt;", ">"}, {"&nbsp;", " "}, {"&#x27;", "'"}, {"&ntilde;", "\xc3\xb1"},
        {"&aacute;", "\xc3\xa1"}, {"&eacute;", "\xc3\xa9"}, {"&iacute;", "\xc3\xad"},
        {"&oacute;", "\xc3\xb3"}, {"&uacute;", "\xc3\xba"},
    };
    char *w = s;
    int space = 0;
    for (char *r = s; *r;) {
        int done = 0;
        if (r[0] == '&' && r[1] == '#') {
            /* Numeric entity: &#8217; or &#x2019; becomes UTF-8. */
            char *q = r + 2;
            int hex = (*q == 'x' || *q == 'X');
            if (hex) q++;
            xbnc_u32 cp = 0;
            int digits = 0;
            for (; digits < 8; q++, digits++) {
                char d = *q;
                int v = (d >= '0' && d <= '9') ? d - '0'
                      : (hex && d >= 'a' && d <= 'f') ? d - 'a' + 10
                      : (hex && d >= 'A' && d <= 'F') ? d - 'A' + 10 : -1;
                if (v < 0) break;
                cp = cp * (hex ? 16 : 10) + (xbnc_u32)v;
            }
            if (digits && *q == ';' && cp >= 32 && cp <= 0x10FFFF && !(cp >= 0xD800 && cp <= 0xDFFF)) {
                if (cp == 0xA0) cp = ' ';
                if (cp == ' ') {
                    r = q + 1;
                    if (!space && w != s) { *w++ = ' '; space = 1; }
                    continue;
                }
                if (cp < 0x80) *w++ = (char)cp;
                else if (cp < 0x800) { *w++ = (char)(0xC0 | (cp >> 6)); *w++ = (char)(0x80 | (cp & 0x3F)); }
                else if (cp < 0x10000) { *w++ = (char)(0xE0 | (cp >> 12)); *w++ = (char)(0x80 | ((cp >> 6) & 0x3F)); *w++ = (char)(0x80 | (cp & 0x3F)); }
                else { *w++ = (char)(0xF0 | (cp >> 18)); *w++ = (char)(0x80 | ((cp >> 12) & 0x3F)); *w++ = (char)(0x80 | ((cp >> 6) & 0x3F)); *w++ = (char)(0x80 | (cp & 0x3F)); }
                r = q + 1; space = 0;
                continue;
            }
        }
        if (*r == '&') {
            for (unsigned i = 0; i < sizeof E / sizeof E[0]; i++) {
                xbnc_u32 l = xbnc_strlen(E[i].e);
                if (xbnc_starts_with(r, E[i].e)) {
                    for (const char *x = E[i].r; *x; x++) *w++ = *x;
                    r += l; done = 1; space = 0; break;
                }
            }
        }
        if (done) continue;
        char c = *r++;
        if (c == '\n' || c == '\r' || c == '\t') c = ' ';
        if (c == ' ') { if (space || w == s) continue; space = 1; } else space = 0;
        *w++ = c;
    }
    while (w > s && w[-1] == ' ') w--;
    *w = 0;
}

/* Text between <tag ...> and </tag> (case-insensitive) starting at from;
 * CDATA is unwrapped. Returns the position after the closing tag, or 0. */
static const char *u_tag_text(const char *from, const char *tag, char *out, xbnc_u32 max) {
    char open[24], close[24];
    open[0] = 0; xbnc_cat(open, sizeof open, "<"); xbnc_cat(open, sizeof open, tag);
    close[0] = 0; xbnc_cat(close, sizeof close, "</"); xbnc_cat(close, sizeof close, tag);
    const char *p = xbnc_find_ci(from, open);
    while (p) {
        char nx = p[xbnc_strlen(open)];
        if (nx == '>' || nx == ' ' || nx == '\t' || nx == '\n') break;
        p = xbnc_find_ci(p + 1, open);
    }
    if (!p) return 0;
    while (*p && *p != '>') p++;
    if (!*p) return 0;
    p++;
    const char *e = xbnc_find_ci(p, close);
    if (!e) return 0;
    const char *a = p, *b = e;
    const char *ws = a;
    while (ws < e && (*ws == ' ' || *ws == '\n' || *ws == '\r' || *ws == '\t')) ws++;
    if (xbnc_starts_with(ws, "<![CDATA[")) a = ws;
    if (xbnc_starts_with(a, "<![CDATA[")) { a += 9; const char *c = xbnc_find_ci(a, "]]>"); if (c && c < e) b = c; }
    xbnc_u32 n = 0;
    while (a < b && n + 1 < max) out[n++] = *a++;
    /* Cut: do not leave half a UTF-8 character. */
    while (a < b && n > 0 && ((unsigned char)*a & 0xC0) == 0x80) { n--; a--; }
    out[n] = 0;
    return e + xbnc_strlen(close);
}

#endif
