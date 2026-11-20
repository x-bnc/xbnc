/*
 * weather: the weather now and today's forecast, from Open-Meteo (free,
 * no key needed).
 *
 *   !weather London ->  London (United Kingdom): 14 °C, clear sky, wind
 *                       9 km/h, humidity 60 %. Today between 9 and 16 °C.
 * Also !tiempo and !clima. Without a city, the default one from the
 * settings. /msg *weather Paris answers only you.
 */
#define XBNC_ARENA_SIZE (1024 * 1024)
#include "../xbnc_plugin.h"
#include "../xbnc_util.h"

#define SLOTS 8
static char P_TO[SLOTS][64];   /* channel, or "" to tell the user */
static char P_NAME[SLOTS][120]; /* place found */
static int P_STEP[SLOTS];      /* 1 geocoding, 2 forecast, 0 free */
static int p_next;
static xbnc_i64 last_ts;

static const char *describe(int code) {
    static const struct { int c; const char *es; const char *en; } W[] = {
        {0, "cielo despejado", "clear sky"}, {1, "casi despejado", "mainly clear"},
        {2, "parcialmente nuboso", "partly cloudy"}, {3, "nublado", "overcast"},
        {45, "niebla", "fog"}, {48, "niebla helada", "freezing fog"},
        {51, "llovizna débil", "light drizzle"}, {53, "llovizna", "drizzle"}, {55, "llovizna intensa", "dense drizzle"},
        {61, "lluvia débil", "light rain"}, {63, "lluvia", "rain"}, {65, "lluvia fuerte", "heavy rain"},
        {66, "lluvia helada", "freezing rain"}, {67, "lluvia helada fuerte", "heavy freezing rain"},
        {71, "nieve débil", "light snow"}, {73, "nieve", "snow"}, {75, "nieve fuerte", "heavy snow"},
        {77, "granos de nieve", "snow grains"}, {80, "chubascos débiles", "light showers"},
        {81, "chubascos", "showers"}, {82, "chubascos fuertes", "violent showers"},
        {85, "chubascos de nieve", "snow showers"}, {86, "chubascos de nieve fuertes", "heavy snow showers"},
        {95, "tormenta", "thunderstorm"}, {96, "tormenta con granizo", "thunderstorm with hail"},
        {99, "tormenta con granizo fuerte", "thunderstorm with heavy hail"},
    };
    for (unsigned i = 0; i < sizeof W / sizeof W[0]; i++) if (W[i].c == code) return u_es ? W[i].es : W[i].en;
    return u_es ? "tiempo variable" : "changeable weather";
}

static void send_out(int slot) {
    if (P_TO[slot][0]) xbnc_privmsg(P_TO[slot], u_out);
    else xbnc_tell(u_out);
}

static void start(const xbnc_event *ev, const char *city, const char *to) {
    static char url[400], enc[200];
    char place[100];
    u_copy(place, sizeof place, city);
    if (!place[0]) xbnc_config(ev, "city", place, sizeof place);
    if (!place[0]) {
        u_reset();
        u_say2("¿De qué ciudad? Ejemplo: !tiempo Madrid", "Which city? Example: !weather London");
        if (to[0]) xbnc_privmsg(to, u_out); else xbnc_tell(u_out);
        return;
    }
    int slot = p_next;
    p_next = (p_next + 1) % SLOTS;
    u_copy(P_TO[slot], 64, to);
    P_STEP[slot] = 1;
    u_urlenc(place, enc, sizeof enc);
    url[0] = 0;
    xbnc_cat(url, sizeof url, "https://geocoding-api.open-meteo.com/v1/search?count=1&format=json&language=");
    xbnc_cat(url, sizeof url, u_es ? "es" : "en");
    xbnc_cat(url, sizeof url, "&name=");
    xbnc_cat(url, sizeof url, enc);
    char id[8];
    xbnc_itoa(slot, id);
    xbnc_http_get(id, url);
}

static void on_http(const xbnc_event *ev) {
    static char body[200000], id[8], url[400], lat[32], lon[32], country[80];
    xbnc_get(ev, "id", id, sizeof id);
    int slot = (int)u_atoi(id);
    if (slot < 0 || slot >= SLOTS || !P_STEP[slot]) return;
    int step = P_STEP[slot];
    P_STEP[slot] = 0;
    if (xbnc_int(xbnc_field(ev->json, "status"), 0) != 200) {
        u_reset();
        u_say2("No se pudo consultar el tiempo ahora mismo.", "Could not get the weather right now.");
        send_out(slot);
        return;
    }
    xbnc_get(ev, "body", body, sizeof body);
    xbnc_span b = {body, xbnc_strlen(body)};
    if (step == 1) {
        xbnc_span r = u_array_first(xbnc_field(b, "results"));
        if (!r.p) {
            u_reset();
            u_say2("No encuentro ese sitio.", "I cannot find that place.");
            send_out(slot);
            return;
        }
        xbnc_string(xbnc_field(r, "name"), P_NAME[slot], sizeof P_NAME[0]);
        country[0] = 0;
        xbnc_string(xbnc_field(r, "country"), country, sizeof country);
        if (country[0]) {
            xbnc_cat(P_NAME[slot], sizeof P_NAME[0], " (");
            xbnc_cat(P_NAME[slot], sizeof P_NAME[0], country);
            xbnc_cat(P_NAME[slot], sizeof P_NAME[0], ")");
        }
        u_num_text(xbnc_field(r, "latitude"), lat, sizeof lat);
        u_num_text(xbnc_field(r, "longitude"), lon, sizeof lon);
        url[0] = 0;
        xbnc_cat(url, sizeof url, "https://api.open-meteo.com/v1/forecast?latitude=");
        xbnc_cat(url, sizeof url, lat);
        xbnc_cat(url, sizeof url, "&longitude=");
        xbnc_cat(url, sizeof url, lon);
        xbnc_cat(url, sizeof url, "&current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m"
                                  "&daily=temperature_2m_max,temperature_2m_min&forecast_days=1&timezone=auto");
        P_STEP[slot] = 2;
        xbnc_itoa(slot, id);
        xbnc_http_get(id, url);
        return;
    }
    xbnc_span cur = xbnc_field(b, "current");
    xbnc_span daily = xbnc_field(b, "daily");
    if (!cur.p) return;
    xbnc_i64 t = u_num_round(xbnc_field(cur, "temperature_2m"), -999);
    xbnc_i64 hum = u_num_round(xbnc_field(cur, "relative_humidity_2m"), -1);
    xbnc_i64 wind = u_num_round(xbnc_field(cur, "wind_speed_10m"), -1);
    xbnc_i64 code = u_num_round(xbnc_field(cur, "weather_code"), -1);
    xbnc_i64 tmax = u_num_round(u_array_first(xbnc_field(daily, "temperature_2m_max")), -999);
    xbnc_i64 tmin = u_num_round(u_array_first(xbnc_field(daily, "temperature_2m_min")), -999);
    u_reset();
    u_say(P_NAME[slot]);
    u_say(": ");
    if (t != -999) { u_sayn(t); u_say(" \xc2\xb0" "C, "); }
    u_say(describe((int)code));
    if (wind >= 0) { u_say2(", viento ", ", wind "); u_sayn(wind); u_say(" km/h"); }
    if (hum >= 0) { u_say2(", humedad ", ", humidity "); u_sayn(hum); u_say(" %"); }
    u_say(".");
    if (tmax != -999 && tmin != -999) {
        u_say2(" Hoy entre ", " Today between ");
        u_sayn(tmin); u_say2(" y ", " and "); u_sayn(tmax); u_say(" \xc2\xb0" "C.");
    }
    send_out(slot);
}

void on_event(const xbnc_event *ev) {
    static char kind[16], text[512], conv[64], trigger[32];
    xbnc_get(ev, "event", kind, sizeof kind);
    u_lang(ev);
    if (u_eq(kind, "http")) { on_http(ev); return; }
    if (u_eq(kind, "command")) {
        xbnc_get(ev, "text", text, sizeof text);
        char w[16];
        u_word(text, w, sizeof w);
        if (u_eq(w, "ayuda") || u_eq(w, "help")) {
            xbnc_tell(u_es ? "En un canal: !tiempo Madrid (o !weather). Sin ciudad, la de los ajustes. Aquí: /msg *weather Sevilla."
                           : "In a channel: !weather London (or !tiempo). Without a city, the one in the settings. Here: /msg *weather Paris.");
            return;
        }
        start(ev, text, "");
        return;
    }
    if (!u_eq(kind, "message") || !xbnc_is(ev, "channel")) return;
    xbnc_get(ev, "text", text, sizeof text);
    xbnc_get(ev, "conversation", conv, sizeof conv);
    xbnc_config(ev, "trigger", trigger, sizeof trigger);
    if (!trigger[0]) u_copy(trigger, sizeof trigger, "!weather");
    const char *rest = u_command(text, trigger);
    if (!rest) rest = u_command(text, "!weather");
    if (!rest) rest = u_command(text, "!tiempo");
    if (!rest) rest = u_command(text, "!clima");
    if (!rest || !u_in_list(ev, "channels", conv, 1)) return;
    xbnc_i64 now = xbnc_now_ms();
    if (now - last_ts < 3000) return; /* one question every few seconds */
    last_ts = now;
    start(ev, rest, conv);
}
