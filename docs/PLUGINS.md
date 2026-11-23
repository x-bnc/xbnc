# Plugins

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

- [Plugins](#plugins)
- [Writing plugins](#plugin-development)

<a id="plugins"></a>

## Plugins

Plugins add features for the whole channel. They run in **sandboxed WebAssembly**: they cannot read files or see other networks, they have CPU and memory limits, and they only do what their permissions allow, which are shown before installing them. Those that go out to the internet can only connect to approved sites.

1. The **18 official plugins come installed**: xBNC installs the missing ones on the first start and after each update, and updates those with an old version (or the same version with a fix). They stay off on every network. One that a superadministrator uninstalls on purpose does not come back until the next version of xBNC (or until it is installed again from *Plugins → Catalogue*).
2. A **superadministrator** can install others from the remote catalogue or by uploading a `.wasm` file, and approves the websites of the plugins that go out to the internet (“Sites it may connect to”). The sites of the official ones (link titles, weather and news) come approved, and any of them can be blocked there.
3. When the official signed catalogue is active, xBNC checks it once a day and updates the installed plugins that have a newer version there (their settings stay). The index signature (Ed25519) and each file's SHA-256 are checked before anything is installed.
4. Each user turns them on in their networks from *Modules*, in the Plugins section, and adjusts their settings.
5. They answer in your xBNC language (or the one of their “Language” setting) and also work with your own messages. The official plugins speak English and Spanish; in another language they answer in English.

| Permission | What it allows |
| --- | --- |
| `send` | Send messages and notices. |
| `raw` | IRC commands such as KICK, MODE or INVITE. Never QUIT, PASS, USER, NICK, CAP, AUTHENTICATE, OPER, SERVER or SQUIT. |
| `timer` | Receive an event every minute (or every 5–60 seconds if the plugin asks for it). |
| `presence` | See joins, parts, kicks, nick changes and the user list of the channels. |
| `http` | Request https pages, only from the domains it declares (up to 10) and that the superadministrator approves. |

> Always write something after the name: `/msg *poll help`. Some clients, such as HexChat, show their own /msg help if you write nothing after it.

<a id="plugin-trivia"></a>

### Trivia

`trivia 1.0.0` · *new in 1.0*

A quiz game for your channels, with hints, points and a ranking.

**How to use it**

In a channel: !trivia to start, !trivia stop, !trivia top (also as !trivial). Answer by writing in the channel. /msg \*trivia add Question? \| answer adds your own questions; list and del N manage them.

**Example**

```text
<carol> !trivia
<y2k> Trivia starts! 10 questions; answer by writing in the channel.
<y2k> Question 1/10: What is the capital of Japan?
<bob> tokyo
<y2k> Correct, bob! It was "Tokyo". You have 1 point.
```

| Setting | Default | Detail |
| --- | --- | --- |
| Command (`trigger`) | !trivia |   |
| Questions per game (`rounds`) | 10 |   |
| Seconds to answer (`seconds`) | 40 | A hint appears halfway. |
| Only in these channels (`channels`) | empty | One per line. Empty: in every channel. |
| Language of the answers (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: send messages, timer. Help from IRC: `/msg *trivia help`

<a id="plugin-greet"></a>

### Welcome

`greet 1.0.0` · *new in 1.0*

Greets people who join your channels, with a different message the first time and personal greetings.

**How to use it**

Automatic. /msg \*greet test #channel nick shows what would be said. {nick} and {channel} are replaced in the messages.

**Example**

```text
* carol has joined #xbnc
<y2k> Welcome to #xbnc, carol! It's your first time here.
```

| Setting | Default | Detail |
| --- | --- | --- |
| First time (`first`) | Welcome to {channel}, {nick}! It's your first time here. | For people who have never been in the channel. {nick} and {channel} are replaced. |
| When they come back (`message`) | Hi {nick}, welcome back to {channel}. | Empty: returning people are not greeted. |
| Personal greetings (`personal`) | empty | One per line: nick and message. Replaces the usual one for that person. |
| Greet privately (notice) (`notice`) | no | Off: the greeting is written in the channel. |
| Minutes before greeting the same person again (`cooldown`) | 30 |   |
| Never greet (`ignore`) | empty | Nicks, one per line (bots, services). |
| Only in these channels (`channels`) | empty | One per line. Empty: in every channel. |
| Language of the answers (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: send messages, joins and parts. Help from IRC: `/msg *greet help`

<a id="plugin-urltitle"></a>

### Link titles

`urltitle 1.0.0` · *new in 1.0*

Shows the title of YouTube, Vimeo, SoundCloud, Dailymotion and Spotify links, Wikipedia articles and GitHub repositories pasted in your channels.

**How to use it**

Automatic. An administrator can block any of these sites in the panel (Plugins → «Sites it may connect to»).

**Example**

```text
<bob> look at this https://www.youtube.com/watch?v=…
<y2k> [YouTube] How to set up an IRC bouncer in 5 minutes · Example Channel
```

| Setting | Default | Detail |
| --- | --- | --- |
| Seconds between titles in a channel (`cooldown`) | 5 |   |
| Also for your own links (`own`) | yes |   |
| Only in these channels (`channels`) | empty | One per line. Empty: in every channel. |
| Language of the answers (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: send messages, web access to approved sites. Help from IRC: `/msg *urltitle help`

<a id="plugin-factoids"></a>

### Definitions

`factoids 1.0.0` · *new in 1.0*

The channel's memory: short answers to frequent questions that you and your editors keep up to date.

**How to use it**

In a channel: !def word (or ?word), !def word = text, !def -word, !defs. Here: /msg \*factoids word.

**Example**

```text
<y2k> !def rules = Be respectful and do not flood.
<y2k> Saved: rules
<carol> ?rules
<y2k> rules: Be respectful and do not flood.
```

| Setting | Default | Detail |
| --- | --- | --- |
| Command (`trigger`) | !def |   |
| Answer ?word too (`question_mark`) | yes |   |
| Who else can change definitions (`editors`) | empty | Nicks, one per line. You always can. |
| Only in these channels (`channels`) | empty | One per line. Empty: in every channel. |
| Language of the answers (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: send messages. Help from IRC: `/msg *factoids help`

<a id="plugin-weather"></a>

### Weather

`weather 1.0.0` · *new in 1.0*

The weather now and today's forecast for any city, from Open-Meteo (free, no key).

**How to use it**

In a channel: !weather London (also !tiempo or !clima). Here: /msg \*weather Paris.

**Example**

```text
<bob> !weather London
<y2k> London (United Kingdom): 14 °C, clear sky, wind 9 km/h, humidity 60 %. Today between 9 and 16 °C.
```

| Setting | Default | Detail |
| --- | --- | --- |
| Command (`trigger`) | !weather |   |
| Default city (`city`) | empty | Used when no city is written. |
| Only in these channels (`channels`) | empty | One per line. Empty: in every channel. |
| Language of the answers (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: send messages, web access to approved sites. Help from IRC: `/msg *weather help`

<a id="plugin-news"></a>

### News

`news 1.0.0` · *new in 1.0*

Posts new entries from RSS and Atom feeds (news sites, blogs, YouTube channels, GitHub releases) in your channels.

**How to use it**

Set the feeds in the panel: one per line, "#channel https://...". /msg \*news list, /msg \*news check.

**Example**

```text
<y2k> [Hacker News] Show HN: an IRC bouncer written in Rust — https://…
```

| Setting | Default | Detail |
| --- | --- | --- |
| Feeds (`feeds`) | empty | One per line: channel and address, for example #xbnc https://hnrss.org/frontpage. Only these sites: YouTube, GitHub, Reddit, Hacker News (hnrss.org), BBC, El País, RTVE, WordPress, Blogspot and Substack. |
| Minutes between checks (`interval`) | 15 |   |
| Language of the answers (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: send messages, timer, web access to approved sites. Help from IRC: `/msg *news help`

<a id="plugin-karma"></a>

### Karma

`karma 1.0.0` · *new in 1.0*

Points between people: nick++ and nick--, with a ranking.

**How to use it**

In a channel: nick++, nick--, !karma nick, !karma top. Here: /msg \*karma nick.

**Example**

```text
<bob> carol++
<y2k> carol has 1 karma.
<carol> !karma top
<y2k> Most karma: carol (1)
```

| Setting | Default | Detail |
| --- | --- | --- |
| Only in these channels (`channels`) | empty | One per line. Empty: in every channel. |
| Language of the answers (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: send messages. Help from IRC: `/msg *karma help`

<a id="plugin-quotes"></a>

### Quotes

`quotes 1.0.0` · *new in 1.0*

Memorable lines of the channel: save them, find them and pull a random one.

**How to use it**

In a channel: !quote, !quote 12, !quote search word, !quote add text, !quote del 12 (also as !cita).

**Example**

```text
<bob> !quote add today is a great day
<y2k> Quote 1 saved.
<carol> !quote 1
<y2k> #1: today is a great day
```

| Setting | Default | Detail |
| --- | --- | --- |
| Command (`trigger`) | !quote |   |
| Anybody can add quotes (`anyone_adds`) | yes | Off: only you and the editors. |
| Editors (`editors`) | empty | Nicks who can add and delete quotes, one per line. |
| Only in these channels (`channels`) | empty | One per line. Empty: in every channel. |
| Language of the answers (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: send messages. Help from IRC: `/msg *quotes help`

<a id="plugin-badwords"></a>

### Forbidden words

`badwords 1.0.0` · *new in 1.0*

Warns, kicks and finally bans people who write forbidden words in your channels (you need op).

**How to use it**

Set the words in the panel. /msg \*badwords test text tells whether a text would be caught.

**Example**

```text
<carol> you are a dummy
<y2k> carol: that word is not allowed here. Next time, kick.
```

| Setting | Default | Detail |
| --- | --- | --- |
| Forbidden words (`words`) | empty | One per line. Case and accents do not matter; a \* at the end matches the start of a word. |
| Ban length (minutes) (`ban_minutes`) | 10 | 0: the ban stays. |
| Kick reason (`reason`) | empty |   |
| Warn privately (notice) (`warn_private`) | no |   |
| Never act on (`exempt`) | empty | Nicks, one per line. |
| Only in these channels (`channels`) | empty | One per line. Empty: in every channel. |
| Language of the answers (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: send messages, IRC commands (kick, ban), timer. Help from IRC: `/msg *badwords help`

<a id="plugin-games"></a>

### Games

`games 1.0.0` · *new in 1.0*

Hangman, magic 8 ball and rock-paper-scissors for your channels.

**How to use it**

In a channel: !8ball question, !rps rock, !hangman and !g letter (or the whole word) to play.

**Example**

```text
<bob> !rps rock
<y2k> rock against scissors: you win, bob!
<bob> !8ball will it work?
<y2k> The 8 ball says: "Ask again later".
```

| Setting | Default | Detail |
| --- | --- | --- |
| Only in these channels (`channels`) | empty | One per line. Empty: in every channel. |
| Language of the answers (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: send messages. Help from IRC: `/msg *games help`

<a id="plugin-dice"></a>

### Dice

`dice 1.1.0`

Rolls dice in channels and private messages: !dado, !dado 2d6, !dado d20.

**How to use it**

In a channel or private message: !dado (one six-sided die), !dado 2d6, !dado d20. Also !roll and !dice.

/msg \*dice 3d8 rolls for you alone.

**Example**

```text
<bob> !roll 2d6
<y2k> bob rolls 2 6-sided dice: 3 and 5 (total 8).
```

| Setting | Default | Detail |
| --- | --- | --- |
| Extra command (`trigger`) | !roll | !dado, !dados, !roll and !dice always work. |
| Most dice per roll (`max_dice`) | 10 |   |
| Answer in private messages (`in_queries`) | yes |   |
| Language of the answers (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: send messages. Help from IRC: `/msg *dice help`

<a id="plugin-poll"></a>

### Polls

`poll 1.1.0`

!poll Question? \| option \| option, then !vote N. Closes on its own and announces the winner.

**How to use it**

In a channel: !poll Pizza or sushi? \| pizza \| sushi (up to 10 options) — vote with !vote 1 — !poll shows the count — !poll end closes it (who started it, or you). It closes on its own after the minutes in the settings.

**Example**

```text
<carol> !poll Coffee or tea? | coffee | tea
<y2k> Poll by carol: Coffee or tea? — 1) coffee 2) tea. Vote with !vote N. Closes in 1 h.
<bob> !vote 2
-y2k- Vote counted in #xbnc: 2) tea. You can change it by voting again.
```

| Setting | Default | Detail |
| --- | --- | --- |
| Command (`trigger`) | !poll |   |
| Vote command (`vote_trigger`) | !vote |   |
| Minutes until it closes (`minutes`) | 60 |   |
| Other people can start polls (`public`) | yes | Off: only you start them; everyone can vote. |
| Language of the answers (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: send messages, timer. Help from IRC: `/msg *poll help`

<a id="plugin-remind"></a>

### Reminders

`remind 1.1.0`

Reminds you (or anyone in the channel) after a while or at a time: !remind 10m take the pizza out, !remind 18:30 meeting.

**How to use it**

In a channel: !remind 10m take the pizza out — !remind 2 hours call Bob — !remind 1h30m … — !remind 18:30 meeting — !remind list — !remind clear. Also !recuerda.

/msg \*remind 10m … reminds you privately. Reminders are checked every minute.

**Example**

```text
<bob> !remind in 10 minutes take the pizza out
<y2k> bob: OK, I will remind you in 10 min.
<y2k> bob: reminder: take the pizza out
```

| Setting | Default | Detail |
| --- | --- | --- |
| Extra command (`trigger`) | !remind | !remind and !recuerda always work. |
| Other people can set reminders (`public`) | yes | Off: only your own and /msg \*remind. |
| Pending reminders per person (`max_per_nick`) | 10 |   |
| Time zone for times like 18:30 (`timezone`) | Europe/Madrid | Europe/Madrid, Europe/London, UTC or a difference such as +1. |
| Language of the answers (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: send messages, timer. Help from IRC: `/msg *remind help`

<a id="plugin-tell"></a>

### Tell

`tell 1.1.0`

!tell nick message: delivers it in the channel when that person speaks or joins.

**How to use it**

In a channel: !tell bob the build is fixed — bob gets it in the channel as soon as he writes something or joins. Works for you and for anyone in the channel.

**Example**

```text
<y2k> !tell carol the build is fixed
<y2k> y2k: OK, I will tell carol as soon as they write or join the channel.
* carol has joined #xbnc
<y2k> carol: y2k left you a message 5 min ago: the build is fixed
```

| Setting | Default | Detail |
| --- | --- | --- |
| Command (`trigger`) | !tell |   |
| Deliver when they join (`on_join`) | yes | Off: only when they write something. |
| Messages waiting per person (`max_per_nick`) | 5 |   |
| Language of the answers (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: send messages, joins and parts. Help from IRC: `/msg *tell help`

<a id="plugin-autoresponder"></a>

### Autoresponder

`autoresponder 1.1.0`

Answers on its own when someone writes a keyword, and (optionally) private messages while you are away.

**How to use it**

Write the rules in the settings, one per line: keyword = answer. {nick} is who wrote and {channel} the channel. The rule \* = text answers any private message.

/msg \*autoresponder shows the rules; /msg \*autoresponder test \<text\> tries them.

**Example**

```text
<bob> hi, anyone?
<y2k> Hi bob! I am not here right now, leave me your message.
```

| Setting | Default | Detail |
| --- | --- | --- |
| Rules (`rules`) | empty | One per line: keyword = answer. Example: hola = Hello {nick}! — \* = answer to any private message. |
| Seconds before the same rule answers again (`cooldown`) | 60 |   |
| Also answer private messages (`in_queries`) | yes |   |
| Only when no client is connected (`only_when_away`) | no | So it answers for you while you are away. |
| Only in these channels (`channels`) | empty | One per line. Empty: every channel. |
| Language of the messages (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: send messages. Help from IRC: `/msg *autoresponder help`

<a id="plugin-chanstats"></a>

### Channel stats

`chanstats 1.1.0`

!stats: messages in the channel and who talks most. !stats nick: one person's count.

**How to use it**

In a channel: !stats shows the messages and who talks most — !stats bob shows one person. /msg \*chanstats #channel shows it to you privately. Counts are saved every minute.

**Example**

```text
<bob> !stats
<y2k> #xbnc: 1204 messages since 3 Nov 2026. Most active: carol 312, bob 280, y2k 199.
```

| Setting | Default | Detail |
| --- | --- | --- |
| Command (`trigger`) | !stats |   |
| People in the ranking (`top`) | 5 |   |
| Answer other people (`public`) | yes | Off: only your own !stats. |
| Only these channels (`channels`) | empty | One per line. Empty: every channel. |
| Language of the answers (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: send messages, timer. Help from IRC: `/msg *chanstats help`

<a id="plugin-seen"></a>

### Seen

`seen 2.1.0`

!seen nick: when someone was last seen and what they were doing, or which channels they are in right now.

**How to use it**

In a channel: !seen bob — when bob was last seen and whether he is here now. /msg \*seen bob, or /msg \*seen stats.

**Example**

```text
<bob> !seen carol
<y2k> carol was last seen 2 h 14 min ago, talking in #xbnc: "off to dinner".
```

| Setting | Default | Detail |
| --- | --- | --- |
| Command (`trigger`) | !seen | What people write before the nick. |
| Answer other people in channels (`public`) | yes | Off: only answers you (your own !seen and /msg \*seen). |
| Seconds between answers in a channel (`cooldown`) | 5 |   |
| Show what they last said (`show_text`) | yes | Off: only says where and when. |
| Answer only in these channels (`channels`) | empty | One per line. Empty: in every channel. Activity is recorded in all of them. |
| Language of the answers (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: send messages, joins and parts. Help from IRC: `/msg *seen help`

<a id="plugin-antiflood"></a>

### Anti-flood

`antiflood 1.1.0`

Warns, kicks or bans people who flood or repeat lines in your channels (you must be a channel operator).

**How to use it**

Works by itself in the channels you list in the settings (you must be a channel operator there). /msg \*antiflood shows its settings.

**Example**

```text
<spam> a
<spam> a
<spam> a
* spam was kicked from #xbnc by y2k (No flooding, please)
```

| Setting | Default | Detail |
| --- | --- | --- |
| Channels to protect (`channels`) | empty | One per line. It does nothing until you list some. |
| Most lines allowed (`lines`) | 5 |   |
| In this many seconds (`seconds`) | 4 |   |
| Same line repeated (`repeats`) | 3 | Times in a row; 0 turns it off. |
| What to do (`action`) | kick | warn (warn first, kick the second time), kick or ban. |
| Ban length in minutes (`ban_minutes`) | 10 | 0: the ban is not lifted. |
| Kick reason (`reason`) | empty |   |
| Never act on these nicks (`exempt`) | empty | One per line. |
| Language of the messages (`lang`) | empty | Empty: your language in xBNC. Or es / en. |

Permissions: IRC commands (kick, ban), send messages, timer. Help from IRC: `/msg *antiflood help`

<a id="plugin-development"></a>

## Writing plugins

A plugin is a WebAssembly file with a JSON manifest inside. There are SDKs for **C** (`sdk/c`, with the 18 official plugins as examples and the `xbnc_util.h` helper) and for **Rust** (`sdk/rust`). Any language that compiles to WebAssembly without WASI works if it follows the interface.

### Manifest

| Field | Required | What it is |
| --- | --- | --- |
| `id` | yes | 1–32 characters a-z 0-9 \_ -, starting with a letter. Cannot be status, notes, watch, events, xbnc, bnc, plugin or plugins. It is the name of its service (\*id). |
| `name` | yes | Visible name, 1–60 characters. |
| `version` | yes | Version, 1–20 characters. |
| `description` | no | What it does. |
| `usage` | no | How it is used (up to 1000 characters). The panel shows it under “How to use it” and xBNC answers with it if the plugin does not answer a command. |
| `author` | no | Author. |
| `homepage` | no | Page of the plugin (https://). |
| `abi` | no | Interface version (1). |
| `permissions` | no | List of send, raw, timer, presence and http. |
| `http_domains` | with http | 1 to 10 domains it asks permission to connect to. |
| `timer_secs` | no | With timer: receive the timer every 5–60 seconds instead of every minute. |
| `settings` | no | Up to 20 settings: {key, type (bool, int, text or lines), label, help, default, min, max}. The panel shows them as a form. |
| `translations` | no | By language code (es, pt, fr…): name, description, usage and, per setting, label and help. The texts at the root of the manifest are the base, in English. |

```json
{
  "id": "hello",
  "name": "Hello",
  "version": "1.0.0",
  "description": "Answers !hello.",
  "usage": "In a channel: !hello",
  "author": "your-name",
  "permissions": ["send"],
  "settings": [
    { "key": "greeting", "type": "text", "label": "Greeting", "default": "Hello!" },
    { "key": "lang", "type": "text", "label": "Language", "help": "Empty: your xBNC language.", "default": "" }
  ],
  "translations": {
    "es": {
      "name": "Hola",
      "description": "Responde a !hello.",
      "usage": "En un canal: !hello",
      "settings": { "greeting": { "label": "Saludo" } }
    }
  }
}
```

### Interface (ABI 1)

The module exports:

| Export | Signature | What it does |
| --- | --- | --- |
| `memory` | memory | Linear memory of the plugin. |
| `xbnc_alloc` | `(len: i32) -> i32` | Reserves len bytes for the event and returns their address. |
| `xbnc_on_event` | `(ptr: i32, len: i32) -> i64` | Receives the event (UTF-8 JSON) and returns (address \<\< 32) \| length of the answer (JSON), or 0 if it does not answer. |

And it can only import from the `xbnc` module (neither WASI nor anything else):

| Import | What it does |
| --- | --- |
| `log(ptr, len)` | Writes a line to the log (at most 10 per event and 500 characters). |
| `now_ms() -> i64` | Milliseconds since 1970. |
| `kv_get`, `kv_set`, `kv_del`, `kv_list` | Key-value store of the plugin on that network: keys up to 200 bytes, values up to 64 KB, 16 MB and 200,000 keys in total, up to 2,000 writes per event (saved together). |

### Events

```json
{
  "event": "message",
  "network": "example", "nick": "y2k", "connected": true, "clients": 1, "lang": "en",
  "config": { "trigger": "!roll" },
  "state": null,
  "command": "PRIVMSG", "conversation": "#xbnc", "channel": true,
  "from": "bob", "from_mask": "bob!b@host", "text": "!roll 2d6",
  "action": false, "outgoing": false
}
```

| Event | When | Own fields |
| --- | --- | --- |
| `load` | The plugin starts on the network (when turned on or when xBNC starts). | — |
| `connected` | xBNC finishes connecting to the network. | — |
| `message` | A message or notice in a channel or private conversation, yours too. | command, conversation, channel, from, from\_mask, text, action, outgoing |
| `command` | Someone writes /msg \*id text. | text |
| `timer` | Every minute, or every timer\_secs (timer permission). | — |
| `join`, `part`, `kick`, `quit`, `nick`, `names` | Movements in the channels (presence permission). | the data of each movement |
| `http` | The answer to a web request (http permission). | id, status, body, error |

Common fields: `network`, `nick` (your current nick), `connected`, `clients` (connected clients), `lang` (your language code: en, es or any installed language), `config` (its settings) and `state` (what it saved).

### Answer

```json
{
  "actions": [
    { "reply": "text in the same conversation" },
    { "tell": "text for you only, as *id" },
    { "privmsg": { "target": "#channel", "text": "..." } },
    { "notice": { "target": "nick", "text": "..." } },
    { "raw": "MODE #channel +o bob" },
    { "http": { "id": "weather", "method": "GET", "url": "https://api.open-meteo.com/..." } }
  ],
  "state": { "whatever you want to keep": 1 }
}
```

- Single-line texts. Actions without permission or badly formed are dropped and noted in the log.
- `state` replaces the plugin's saved state on that network; it comes with every event and survives restarts. The instance is kept between events while the network is running.
- `raw` can never send QUIT, PASS, USER, NICK, CAP, AUTHENTICATE, OPER, SERVER or SQUIT.
- `http`: `('id', 'url', 'method', 'body')`. https only, to approved domains; methods GET, POST, PUT and DELETE; body up to 16 KB (text or JSON); up to 3 requests at a time and 30 per minute per plugin and network. The answer arrives as an `http` event with `id`, `status`, `body` (up to 256 KB) and, if it failed, `error`.

### Limits

| Limit | Value |
| --- | --- |
| File size | 4 MB |
| Memory | 16 MB |
| CPU per event | about 20 million instructions (50 million when starting) |
| Answer | 64 KB and 20 actions; 5 messages per event |
| Saved state | 64 KB |
| Key-value store | 16 MB and 200,000 keys per plugin and network |
| Failures in a row before it stops on a network | 5 |

### C SDK

`sdk/c/xbnc_plugin.h` takes care of the interface. Main functions:

| Function | What it does |
| --- | --- |
| `void on_event(const xbnc_event *ev)` | The function you write: it receives each event. |
| `xbnc_get(ev, "field", buf, max)` | Reads a text field of the event. |
| `xbnc_is(ev, "field")` | Reads a yes/no field. |
| `xbnc_config(ev, "key", buf, max)`, `xbnc_config_int`, `xbnc_config_bool` | Reads a setting. |
| `xbnc_get_state(ev)` | The saved state (JSON). |
| `xbnc_reply(text)`, `xbnc_tell(text)` | Answer in the conversation or to you only. |
| `xbnc_privmsg(target, text)`, `xbnc_notice(target, text)` | Message or notice to whoever you want. |
| `xbnc_raw(line)` | IRC command (raw permission). |
| `xbnc_http_get(id, url)`, `xbnc_http_post_json(id, url, json)` | Web requests (http permission). |
| `xbnc_state(json)` | Save the state. |
| `xbnc_kv_get`, `xbnc_kv_set`, `xbnc_kv_del`, `xbnc_kv_list` (and the \_str variants) | Key-value store. |
| `xbnc_log(text)` | Write to the log. |

`sdk/c/xbnc_util.h` adds what the official plugins use: language of the answer (`u_lang`), reading commands with aliases (`u_command`), walking the store (`u_next_key`), dates, time zones with European daylight saving time and text building.

```c
#include "xbnc_plugin.h"

void on_event(const xbnc_event *ev) {
    static char kind[16], text[256], greeting[128];
    xbnc_get(ev, "event", kind, sizeof kind);
    if (!xbnc_streq(kind, xbnc_strlen(kind), "message")) return;
    xbnc_get(ev, "text", text, sizeof text);
    if (!xbnc_streq(text, xbnc_strlen(text), "!hello")) return;
    xbnc_config(ev, "greeting", greeting, sizeof greeting);
    xbnc_reply(greeting[0] ? greeting : "Hello!");
}
```

```sh
cd sdk/c
clang --target=wasm32 -O2 -nostdlib -fno-builtin -I. \
  -Wl,--no-entry -Wl,--export-dynamic -Wl,--strip-debug -Wl,-z,stack-size=65536 \
  -o hello.raw.wasm hello.c
xbncd plugin pack hello.raw.wasm hello.json -o hello.wasm
xbncd plugin check hello.wasm
sudo -u xbnc xbncd plugin install hello.wasm

# The official examples: make (or make bundle to copy them to the catalogue)
```

It needs clang and wasm-ld (LLVM 15 or later) and llvm-objcopy (or `xbncd plugin pack`).

### Rust SDK

The `xbnc-plugin` crate (`sdk/rust`) provides the `Plugin` trait, the `Event` type (with `kind()`, `text()`, `conversation()`, `is_channel()`, `outgoing()`, `config_str()`, `state()`…), `Response` (with `reply()`, `tell()`, `state()`…) and the `plugin!` macro. Included example (`sdk/rust/examples/stats.rs`):

```rust
//! Example plugin: counts messages per channel and answers "!stats".
//! The counts survive restarts thanks to the persisted state.
//!
//! Build: cargo build -p xbnc-plugin --release --target wasm32-unknown-unknown --example stats
//! then: xbncd plugin pack target/wasm32-unknown-unknown/release/examples/stats.wasm \
//!         sdk/rust/examples/stats.json -o stats.wasm

use xbnc_plugin::{plugin, Event, Plugin, Response};

#[derive(Default)]
struct Stats;

impl Plugin for Stats {
    fn on_event(&mut self, ev: &Event) -> Response {
        let mut r = Response::new();
        let mut counts = ev.state().as_object().cloned().unwrap_or_default();
        match ev.kind() {
            "message" if ev.is_channel() && !ev.outgoing() => {
                let chan = ev.conversation().to_lowercase();
                let n = counts.get(&chan).and_then(|v| v.as_u64()).unwrap_or(0) + 1;
                counts.insert(chan, n.into());
                if ev.text().trim() == ev.config_str("trigger") {
                    r.reply(format!("{} messages counted in {}", n, ev.conversation()));
                }
                r.state(counts.into());
            }
            "command" => {
                if counts.is_empty() {
                    r.tell("Nothing counted yet.");
                }
                let mut v: Vec<_> = counts.iter().collect();
                v.sort_by_key(|e| std::cmp::Reverse(e.1.as_u64()));
                for (chan, n) in v.into_iter().take(10) {
                    r.tell(format!("{chan}: {n}"));
                }
            }
            _ => {}
        }
        r
    }
}

plugin!(Stats);
```

### Development mode

In *Plugins → Development* (superadministrators): with the mode on, xBNC watches `/var/lib/xbnc/plugins/dev/`. A `.wasm` saved there is checked and installed as soon as it changes and every network reloads it. If you put `name.json` next to `name.wasm`, it packs it for you (no need for `xbncd plugin pack`). A new plugin is turned on in the networks of whoever turned the mode on. The results and what the plugin writes are shown in the page's live log.

```sh
sudo cp hello.wasm hello.json /var/lib/xbnc/plugins/dev/
```

### Publishing your own catalogue

Besides the official ones, xBNC can offer the plugins of a signed remote catalogue (`[catalog]` in `xbnc.toml`). The index is a JSON `{"plugins": [{"id", "version", "url", "sha256", "size", "manifest"}]}` and its Ed25519 signature in base64 is published in `index.sig`. Each plugin is checked against the signed SHA-256 before it is installed.

```sh
xbncd plugin catalog-keygen -o catalogue.key        # once; the public key goes in [catalog] public_key
xbncd plugin catalog-entry hello.wasm https://plugins.example.org/hello.wasm   # entry for the index
xbncd plugin catalog-sign --key catalogue.key index.json                       # writes index.json.sig
```
