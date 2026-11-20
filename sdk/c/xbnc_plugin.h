/*
 * xBNC plugin SDK for C (ABI version 1). Single header, no libc needed.
 *
 *   clang --target=wasm32 -O2 -nostdlib -fno-builtin -Wl,--no-entry \
 *         -Wl,--export-dynamic -o myplugin.raw.wasm myplugin.c
 *   xbncd plugin pack myplugin.raw.wasm myplugin.json -o myplugin.wasm
 *
 * The manifest (myplugin.json) is embedded as the "xbnc.manifest" custom
 * section by `xbncd plugin pack` (or `llvm-objcopy --add-section
 * xbnc.manifest=myplugin.json`). The plugin itself defines one function:
 *
 *   void on_event(const xbnc_event *ev);
 *
 * Inside it, read the event with xbnc_get_* and answer with xbnc_reply(),
 * xbnc_tell(), xbnc_privmsg(), xbnc_notice(), xbnc_raw() and xbnc_state().
 * See docs/PLUGINS.md.
 */
#ifndef XBNC_PLUGIN_H
#define XBNC_PLUGIN_H

typedef unsigned int xbnc_u32;
typedef long long xbnc_i64;

#define XBNC_EXPORT(name) __attribute__((export_name(name)))
#define XBNC_IMPORT(name) __attribute__((import_module("xbnc"), import_name(name)))

XBNC_IMPORT("log") void xbnc_host_log(const char *ptr, int len);
XBNC_IMPORT("now_ms") xbnc_i64 xbnc_now_ms(void);
/* Storage (xBNC 0.8+): only imported when the plugin uses xbnc_kv_*. */
XBNC_IMPORT("kv_get") int xbnc_host_kv_get(const char *k, int kl, char *out, int max);
XBNC_IMPORT("kv_set") int xbnc_host_kv_set(const char *k, int kl, const char *v, int vl);
XBNC_IMPORT("kv_del") int xbnc_host_kv_del(const char *k, int kl);
XBNC_IMPORT("kv_list") int xbnc_host_kv_list(const char *p, int pl, const char *a, int al, char *out, int max);

/* ------------------------------------------------------------ memory */

/* The compiler may call these for struct copies; there is no libc. The
 * volatile pointers stop it from turning the loops back into calls. */
__attribute__((weak)) void *memset(void *d, int c, unsigned long n) {
    volatile unsigned char *p = (volatile unsigned char *)d;
    while (n--) *p++ = (unsigned char)c;
    return d;
}
__attribute__((weak)) void *memcpy(void *d, const void *s, unsigned long n) {
    volatile unsigned char *p = (volatile unsigned char *)d;
    const unsigned char *q = (const unsigned char *)s;
    while (n--) *p++ = *q++;
    return d;
}

#ifndef XBNC_ARENA_SIZE
#define XBNC_ARENA_SIZE (512 * 1024)
#endif
static unsigned char xbnc_arena_[XBNC_ARENA_SIZE];
static xbnc_u32 xbnc_arena_used_;

/* Scratch memory valid until the next event. Returns 0 when full. */
static void *xbnc_scratch(xbnc_u32 n) {
    xbnc_u32 start = (xbnc_arena_used_ + 7u) & ~7u;
    if (start + n > XBNC_ARENA_SIZE || start + n < start) return 0;
    xbnc_arena_used_ = start + n;
    return xbnc_arena_ + start;
}

/* Called by xBNC once per event to place the event JSON. */
XBNC_EXPORT("xbnc_alloc") void *xbnc_alloc(int len) {
    xbnc_arena_used_ = 0; /* a new event: everything from the last one is free */
    return xbnc_scratch((xbnc_u32)len);
}

static xbnc_u32 xbnc_strlen(const char *s) { xbnc_u32 n = 0; while (s && s[n]) n++; return n; }

static int xbnc_streq(const char *a, xbnc_u32 alen, const char *b) {
    xbnc_u32 i = 0;
    for (; i < alen; i++) if (b[i] == 0 || a[i] != b[i]) return 0;
    return b[i] == 0;
}

static void xbnc_log(const char *s) { xbnc_host_log(s, (int)xbnc_strlen(s)); }

/* ------------------------------------------------------------ JSON reading */

typedef struct { const char *p; xbnc_u32 n; } xbnc_span;
typedef struct { xbnc_span json; } xbnc_event;

static xbnc_u32 xbnc_ws_(const char *p, xbnc_u32 i, xbnc_u32 n) {
    while (i < n && (p[i] == ' ' || p[i] == '\n' || p[i] == '\r' || p[i] == '\t')) i++;
    return i;
}

/* Index just past the JSON value starting at i (or n on error). */
static xbnc_u32 xbnc_skip_(const char *p, xbnc_u32 i, xbnc_u32 n) {
    i = xbnc_ws_(p, i, n);
    if (i >= n) return n;
    if (p[i] == '"') {
        for (i++; i < n; i++) {
            if (p[i] == '\\') i++;
            else if (p[i] == '"') return i + 1;
        }
        return n;
    }
    if (p[i] == '{' || p[i] == '[') {
        int depth = 0;
        for (; i < n; i++) {
            char c = p[i];
            if (c == '"') { i = xbnc_skip_(p, i, n) - 1; continue; }
            if (c == '{' || c == '[') depth++;
            else if (c == '}' || c == ']') { if (--depth == 0) return i + 1; }
        }
        return n;
    }
    while (i < n && p[i] != ',' && p[i] != '}' && p[i] != ']') i++;
    return i;
}

/* The raw value of `key` in the object `obj` (top level only). */
static xbnc_span xbnc_field(xbnc_span obj, const char *key) {
    xbnc_span none = {0, 0};
    const char *p = obj.p; xbnc_u32 n = obj.n, i = xbnc_ws_(p, 0, n);
    if (i >= n || p[i] != '{') return none;
    i++;
    while (1) {
        i = xbnc_ws_(p, i, n);
        if (i >= n || p[i] != '"') return none;
        xbnc_u32 ks = i + 1, ke = xbnc_skip_(p, i, n) - 1;
        i = xbnc_ws_(p, ke + 1, n);
        if (i >= n || p[i] != ':') return none;
        xbnc_u32 vs = xbnc_ws_(p, i + 1, n), ve = xbnc_skip_(p, vs, n);
        if (ke >= ks && xbnc_streq(p + ks, ke - ks, key)) { xbnc_span s = {p + vs, ve - vs}; return s; }
        i = xbnc_ws_(p, ve, n);
        if (i >= n || p[i] != ',') return none;
        i++;
    }
}

static void xbnc_put_utf8_(char *out, xbnc_u32 *o, xbnc_u32 max, xbnc_u32 cp) {
    char b[4]; xbnc_u32 k = 0;
    if (cp < 0x80) b[k++] = (char)cp;
    else if (cp < 0x800) { b[k++] = (char)(0xC0 | (cp >> 6)); b[k++] = (char)(0x80 | (cp & 0x3F)); }
    else if (cp < 0x10000) { b[k++] = (char)(0xE0 | (cp >> 12)); b[k++] = (char)(0x80 | ((cp >> 6) & 0x3F)); b[k++] = (char)(0x80 | (cp & 0x3F)); }
    else { b[k++] = (char)(0xF0 | (cp >> 18)); b[k++] = (char)(0x80 | ((cp >> 12) & 0x3F)); b[k++] = (char)(0x80 | ((cp >> 6) & 0x3F)); b[k++] = (char)(0x80 | (cp & 0x3F)); }
    for (xbnc_u32 j = 0; j < k; j++) if (*o + 1 < max) out[(*o)++] = b[j];
}

static int xbnc_hex_(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

/* Copies a JSON string value into out (NUL-terminated, truncated to max).
 * Returns the length, or -1 if the value is not a string. */
static int xbnc_string(xbnc_span v, char *out, xbnc_u32 max) {
    if (!v.p || v.n < 2 || v.p[0] != '"' || max == 0) { if (max) out[0] = 0; return -1; }
    xbnc_u32 o = 0;
    for (xbnc_u32 i = 1; i + 1 < v.n; i++) {
        char c = v.p[i];
        if (c == '\\' && i + 2 < v.n) {
            char e = v.p[++i];
            xbnc_u32 cp = (xbnc_u32)e;
            if (e == 'n') cp = '\n'; else if (e == 't') cp = '\t'; else if (e == 'r') cp = '\r';
            else if (e == 'b') cp = 8; else if (e == 'f') cp = 12;
            else if (e == 'u' && i + 4 < v.n) {
                cp = 0;
                for (int k = 1; k <= 4; k++) { int h = xbnc_hex_(v.p[i + k]); cp = cp * 16 + (h < 0 ? 0 : (xbnc_u32)h); }
                i += 4;
                if (cp >= 0xD800 && cp < 0xDC00 && i + 6 < v.n && v.p[i + 1] == '\\' && v.p[i + 2] == 'u') {
                    xbnc_u32 lo = 0;
                    for (int k = 3; k <= 6; k++) { int h = xbnc_hex_(v.p[i + k]); lo = lo * 16 + (h < 0 ? 0 : (xbnc_u32)h); }
                    if (lo >= 0xDC00 && lo < 0xE000) { cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00); i += 6; }
                }
            }
            xbnc_put_utf8_(out, &o, max, cp);
        } else if (o + 1 < max) {
            out[o++] = c;
        }
    }
    out[o] = 0;
    return (int)o;
}

static xbnc_i64 xbnc_int(xbnc_span v, xbnc_i64 fallback) {
    if (!v.p || v.n == 0) return fallback;
    xbnc_u32 i = 0; int neg = 0; xbnc_i64 r = 0;
    if (v.p[0] == '-') { neg = 1; i = 1; }
    if (i >= v.n || v.p[i] < '0' || v.p[i] > '9') return fallback;
    for (; i < v.n && v.p[i] >= '0' && v.p[i] <= '9'; i++) r = r * 10 + (v.p[i] - '0');
    return neg ? -r : r;
}

static int xbnc_bool(xbnc_span v) { return v.p && v.n >= 4 && v.p[0] == 't'; }

/* Shortcuts for the event and its "config" object. */
static int xbnc_get(const xbnc_event *ev, const char *key, char *out, xbnc_u32 max) {
    return xbnc_string(xbnc_field(ev->json, key), out, max);
}
static int xbnc_is(const xbnc_event *ev, const char *key) { return xbnc_bool(xbnc_field(ev->json, key)); }
static int xbnc_config(const xbnc_event *ev, const char *key, char *out, xbnc_u32 max) {
    return xbnc_string(xbnc_field(xbnc_field(ev->json, "config"), key), out, max);
}
static xbnc_i64 xbnc_config_int(const xbnc_event *ev, const char *key, xbnc_i64 fallback) {
    return xbnc_int(xbnc_field(xbnc_field(ev->json, "config"), key), fallback);
}
static int xbnc_config_bool(const xbnc_event *ev, const char *key) {
    return xbnc_bool(xbnc_field(xbnc_field(ev->json, "config"), key));
}
/* The raw JSON of the persisted state (what the plugin last saved). */
static xbnc_span xbnc_get_state(const xbnc_event *ev) { return xbnc_field(ev->json, "state"); }

/* ------------------------------------------------------------ answer */

static char *xbnc_out_;
static xbnc_u32 xbnc_out_len_, xbnc_out_cap_, xbnc_out_actions_;
static int xbnc_out_state_set_;

static void xbnc_out_raw_(const char *s, xbnc_u32 n) {
    if (!xbnc_out_) {
        xbnc_out_cap_ = 64 * 1024 - 64;
        xbnc_out_ = (char *)xbnc_scratch(xbnc_out_cap_ + 64);
        xbnc_out_len_ = 0;
        if (!xbnc_out_) return;
        xbnc_out_[xbnc_out_len_++] = '{';
    }
    for (xbnc_u32 i = 0; i < n && xbnc_out_len_ < xbnc_out_cap_; i++) xbnc_out_[xbnc_out_len_++] = s[i];
}
static void xbnc_out_s_(const char *s) { xbnc_out_raw_(s, xbnc_strlen(s)); }

static void xbnc_out_str_(const char *s) {
    static const char hex[] = "0123456789abcdef";
    xbnc_out_s_("\"");
    for (xbnc_u32 i = 0; s[i]; i++) {
        unsigned char c = (unsigned char)s[i];
        if (c == '"' || c == '\\') { char e[2] = {'\\', (char)c}; xbnc_out_raw_(e, 2); }
        else if (c < 0x20) { char e[6] = {'\\', 'u', '0', '0', hex[c >> 4], hex[c & 15]}; xbnc_out_raw_(e, 6); }
        else xbnc_out_raw_((const char *)&c, 1);
    }
    xbnc_out_s_("\"");
}

static void xbnc_action_begin_(const char *name) {
    if (xbnc_out_state_set_) return; /* actions must come before the state */
    xbnc_out_s_(xbnc_out_actions_ == 0 ? "\"actions\":[{\"" : ",{\"");
    xbnc_out_s_(name);
    xbnc_out_s_("\":");
    xbnc_out_actions_++;
}

/* Answers in the conversation the event came from (permission "send"). */
static void xbnc_reply(const char *text) { xbnc_action_begin_("reply"); xbnc_out_str_(text); xbnc_out_s_("}"); }
/* Message from *<plugin> to the user's own clients (always allowed). */
static void xbnc_tell(const char *text) { xbnc_action_begin_("tell"); xbnc_out_str_(text); xbnc_out_s_("}"); }
static void xbnc_privmsg(const char *target, const char *text) {
    xbnc_action_begin_("privmsg"); xbnc_out_s_("{\"target\":"); xbnc_out_str_(target);
    xbnc_out_s_(",\"text\":"); xbnc_out_str_(text); xbnc_out_s_("}}");
}
static void xbnc_notice(const char *target, const char *text) {
    xbnc_action_begin_("notice"); xbnc_out_s_("{\"target\":"); xbnc_out_str_(target);
    xbnc_out_s_(",\"text\":"); xbnc_out_str_(text); xbnc_out_s_("}}");
}
/* Any IRC line (permission "raw"). */
static void xbnc_raw(const char *line) { xbnc_action_begin_("raw"); xbnc_out_str_(line); xbnc_out_s_("}"); }
/* Web GET to an approved domain (permission "http"). The answer arrives
 * later as an event "http" with fields id, status, body and error. */
static void xbnc_http_get(const char *id, const char *url) {
    xbnc_action_begin_("http"); xbnc_out_s_("{\"id\":"); xbnc_out_str_(id);
    xbnc_out_s_(",\"url\":"); xbnc_out_str_(url); xbnc_out_s_("}}");
}
/* Web POST of a JSON text (permission "http"). */
static void xbnc_http_post_json(const char *id, const char *url, const char *json) {
    xbnc_action_begin_("http"); xbnc_out_s_("{\"id\":"); xbnc_out_str_(id);
    xbnc_out_s_(",\"method\":\"POST\",\"url\":"); xbnc_out_str_(url);
    xbnc_out_s_(",\"content_type\":\"application/json\",\"body\":"); xbnc_out_str_(json); xbnc_out_s_("}}");
}

/* Replaces the persisted state with this JSON value (call it last). */
static void xbnc_state(const char *json) {
    if (xbnc_out_state_set_) return;
    if (xbnc_out_actions_) xbnc_out_s_("],");
    xbnc_out_s_("\"state\":");
    xbnc_out_s_(json);
    xbnc_out_state_set_ = 1;
}

/* ------------------------------------------------------------ entry point */

void on_event(const xbnc_event *ev);

XBNC_EXPORT("xbnc_on_event") xbnc_i64 xbnc_on_event(const char *ptr, int len) {
    xbnc_out_ = 0; xbnc_out_len_ = 0; xbnc_out_actions_ = 0; xbnc_out_state_set_ = 0;
    xbnc_event ev = {{ptr, (xbnc_u32)len}};
    on_event(&ev);
    if (!xbnc_out_) return 0;
    if (xbnc_out_actions_ && !xbnc_out_state_set_) xbnc_out_s_("]");
    xbnc_out_s_("}");
    return ((xbnc_i64)(xbnc_u32)xbnc_out_ << 32) | (xbnc_i64)xbnc_out_len_;
}

/* ------------------------------------------------------------ storage (xBNC 0.8+)
 * A key-value store per plugin and network, kept in xBNC's database: keys
 * up to 200 bytes, values up to 64 KB, 16 MB in total. Writes made during
 * an event are saved together when the event ends (and dropped if it fails).
 */

/* Copies the value into out (up to max bytes). Returns its full length, or
 * -1 if the key does not exist. */
static int xbnc_kv_get(const char *key, void *out, xbnc_u32 max) {
    return xbnc_host_kv_get(key, (int)xbnc_strlen(key), (char *)out, (int)max);
}
/* Like xbnc_kv_get for text: always NUL-terminated. */
static int xbnc_kv_get_str(const char *key, char *out, xbnc_u32 max) {
    if (max == 0) return -1;
    int n = xbnc_host_kv_get(key, (int)xbnc_strlen(key), out, (int)max - 1);
    out[n < 0 ? 0 : ((xbnc_u32)n < max - 1 ? (xbnc_u32)n : max - 1)] = 0;
    return n;
}
/* 0 when saved, -1 when too big or over the quota. */
static int xbnc_kv_set(const char *key, const void *value, xbnc_u32 len) {
    return xbnc_host_kv_set(key, (int)xbnc_strlen(key), (const char *)value, (int)len);
}
static int xbnc_kv_set_str(const char *key, const char *value) {
    return xbnc_kv_set(key, value, xbnc_strlen(value));
}
static int xbnc_kv_del(const char *key) { return xbnc_host_kv_del(key, (int)xbnc_strlen(key)); }
/* Keys starting with prefix that sort after `after`, separated by \n
 * (NUL-terminated). Returns the bytes written. */
static int xbnc_kv_list(const char *prefix, const char *after, char *out, xbnc_u32 max) {
    if (max == 0) return 0;
    int n = xbnc_host_kv_list(prefix, (int)xbnc_strlen(prefix), after, (int)xbnc_strlen(after), out, (int)max - 1);
    out[n < 0 ? 0 : n] = 0;
    return n < 0 ? 0 : n;
}

/* ------------------------------------------------------------ small helpers */

static int xbnc_lower_(int c) { return (c >= 'A' && c <= 'Z') ? c + 32 : c; }

/* Case-insensitive (ASCII) substring search. */
static const char *xbnc_find_ci(const char *hay, const char *needle) {
    xbnc_u32 n = xbnc_strlen(needle);
    if (n == 0) return hay;
    for (; *hay; hay++) {
        xbnc_u32 i = 0;
        while (i < n && hay[i] && xbnc_lower_(hay[i]) == xbnc_lower_(needle[i])) i++;
        if (i == n) return hay;
    }
    return 0;
}

static int xbnc_starts_with(const char *s, const char *prefix) {
    while (*prefix) if (*s++ != *prefix++) return 0;
    return 1;
}

/* Writes a decimal number; returns the number of characters. */
static int xbnc_itoa(xbnc_i64 v, char *out) {
    char tmp[24]; int n = 0, o = 0;
    xbnc_u32 neg = v < 0;
    unsigned long long u = neg ? (unsigned long long)(-(v + 1)) + 1 : (unsigned long long)v;
    do { tmp[n++] = (char)('0' + u % 10); u /= 10; } while (u);
    if (neg) out[o++] = '-';
    while (n) out[o++] = tmp[--n];
    out[o] = 0;
    return o;
}

/* Appends src to dst (of size max), always NUL-terminated. */
static void xbnc_cat(char *dst, xbnc_u32 max, const char *src) {
    xbnc_u32 n = xbnc_strlen(dst);
    while (*src && n + 1 < max) dst[n++] = *src++;
    dst[n] = 0;
}

#endif
