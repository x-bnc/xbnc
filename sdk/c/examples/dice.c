/*
 * dice: rolls dice in channels and private messages.
 *
 *   !roll            ->  bob rolls one 6-sided die: 4.
 *   !roll 2d6        ->  bob rolls 2 6-sided dice: 3 and 5 (total 8).
 *   !roll d20        ->  one die of 20 sides.
 * Also !dice, !dado and !dados, or the command set in the settings.
 * /msg *dice 3d8 rolls for you alone.
 */
#include "../xbnc_plugin.h"
#include "../xbnc_util.h"

static unsigned long long rng;

static unsigned int next_rand(void) {
    rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17;
    return (unsigned int)(rng >> 16);
}

/* "NdM", "dM", "N" (N dice of 6) or "" (one of 6). Returns 0 on success. */
static int parse_dice(const char *s, int *count, int *sides) {
    int n = 0, m = 0, have_n = 0, have_d = 0;
    s = u_skip_spaces(s);
    while (*s >= '0' && *s <= '9') { n = n * 10 + (*s++ - '0'); have_n = 1; if (n > 1000) return 1; }
    if (*s == 'd' || *s == 'D') {
        have_d = 1;
        s++;
        while (*s >= '0' && *s <= '9') { m = m * 10 + (*s++ - '0'); if (m > 100000) return 1; }
    }
    s = u_skip_spaces(s);
    if (*s) return 1;
    if (!have_d) m = 6;
    if (m < 2) return 1;
    *count = have_n ? n : 1;
    *sides = m;
    return *count >= 1 ? 0 : 1;
}

static const char *match(const char *text, const char *trigger) {
    static const char *aliases[] = {"!dado", "!dados", "!roll", "!dice"};
    const char *r = u_command(text, trigger);
    for (int i = 0; !r && i < 4; i++) r = u_command(text, aliases[i]);
    return r;
}

void on_event(const xbnc_event *ev) {
    static char kind[16], text[512], trigger[64], from[64];
    xbnc_get(ev, "event", kind, sizeof kind);
    u_lang(ev);
    int to_user = u_eq(kind, "command");
    const char *rest;
    xbnc_config(ev, "trigger", trigger, sizeof trigger);
    if (to_user) {
        xbnc_get(ev, "text", text, sizeof text);
        xbnc_get(ev, "nick", from, sizeof from);
        rest = text;
        char w[16];
        u_word(text, w, sizeof w);
        if (u_eq(w, "ayuda") || u_eq(w, "help")) {
            u_reset();
            u_say2("En un canal o privado: ", "In a channel or private message: ");
            u_say(trigger[0] ? trigger : (u_es ? "!dado" : "!roll"));
            u_say2(" 2d6 (dos dados de seis), !dado d20, o solo !dado. Aquí: /msg *dice 3d8.",
                   " 2d6 (two six-sided dice), !roll d20, or just !roll. Here: /msg *dice 3d8.");
            xbnc_tell(u_out);
            return;
        }
    } else if (u_eq(kind, "message")) {
        if (!xbnc_is(ev, "channel") && !xbnc_config_bool(ev, "in_queries")) return;
        xbnc_get(ev, "text", text, sizeof text);
        xbnc_get(ev, "from", from, sizeof from);
        if (xbnc_is(ev, "outgoing")) xbnc_get(ev, "nick", from, sizeof from);
        rest = match(text, trigger);
        if (!rest) return;
    } else {
        return;
    }

    int count, sides;
    u_reset();
    if (parse_dice(rest, &count, &sides) != 0) {
        u_say(from);
        u_say2(": no entiendo la tirada. Ejemplos: !dado, !dado 2d6, !dado d20.",
               ": I do not understand the roll. Examples: !roll, !roll 2d6, !roll d20.");
        u_answer(to_user);
        return;
    }
    xbnc_i64 max = xbnc_config_int(ev, "max_dice", 10);
    if (count > max) count = (int)max;

    rng = (unsigned long long)xbnc_now_ms() * 6364136223846793005ULL + 1442695040888963407ULL;
    for (xbnc_u32 i = 0; text[i]; i++) rng = rng * 31 + (unsigned char)text[i];
    if (rng == 0) rng = 88172645463325252ULL;

    u_say(from);
    if (count == 1) {
        u_say2(" tira un dado de ", " rolls one ");
        u_sayn(sides);
        u_say2(" caras: sale un ", "-sided die: ");
        u_sayn((int)(next_rand() % (unsigned int)sides) + 1);
        u_say(".");
    } else {
        u_say2(" tira ", " rolls ");
        u_sayn(count);
        u_say2(" dados de ", " ");
        u_sayn(sides);
        u_say2(" caras: ", "-sided dice: ");
        xbnc_i64 total = 0;
        for (int i = 0; i < count; i++) {
            int r = (int)(next_rand() % (unsigned int)sides) + 1;
            total += r;
            if (i) u_say(i + 1 == count ? (u_es ? " y " : " and ") : ", ");
            u_sayn(r);
        }
        u_say(" (total ");
        u_sayn(total);
        u_say(").");
    }
    u_answer(to_user);
}
