# What xBNC is

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

- [What xBNC is](#intro)
- [How it works](#how-it-works)

<a id="intro"></a>

## What xBNC is

xBNC is an **IRC bouncer**: a program that stays connected to your IRC networks around the clock, even when you close your client or switch off your computer. When you come back, it gives you what you missed, exactly once on each device.

You install it on a server of your own (a VPS, a Raspberry Pi, a computer at home), use it with your usual IRC client and configure everything from a web panel, or from IRC with `/msg *status`. It is a single program written in Rust, with the panel built in.

- **24** native modules, configured per network
- **18** official WebAssembly plugins
- **2+** languages: English and Spanish built in, and any other with a translation file

### What it includes

- **Bouncer:** permanent connections to several networks per user, with verified TLS, SASL PLAIN and EXTERNAL, automatic reconnection with growing delays and flood protection towards the server.
- **Several devices at once:** what you write on one appears on the others, and each one keeps its own account of what it has read.
- **History:** channels and private messages with full-text search, filters, TXT and JSON export, general and per-channel retention, and per-user quotas.
- **Full IRCv3** with servers and clients ([complete list](IRCV3.md#ircv3)); classic clients see plain old IRC.
- **Web chat** in the panel, a mentions view and read markers shared between devices.
- **Notifications** of mentions and private messages by Telegram, Discord, e-mail, a web address of your own or browser and phone push notifications.
- **24 modules** (services login, automatic away, auto op, FiSH encryption, detached channels, scheduled tasks…) and **18 plugins** already installed (trivia, polls, reminders, weather…).
- **Multi-user:** five roles, quotas, suspension, audit log, two-step verification, passkeys, OpenID Connect and LDAP.
- **Data:** SQLite or PostgreSQL, active/standby high availability, backups from the panel or the command line, migration from another bouncer.
- **Administration:** web panel, REST API with tokens, WebSocket, Prometheus metrics with a Grafana dashboard, alerts in the panel, over IRC and by webhook.
- **Security:** Argon2id, encrypted credentials, brute-force blocking, CSRF, CSP, an unprivileged service, sandboxed plugins and continuous *fuzzing*.

> **Version 1.0.0, test release.** It is the first complete version and is being tested on real networks before it is declared stable. See [Testing the beta](TESTING.md#beta) to take part.

<a id="how-it-works"></a>

## How it works

```text
Your devices                          xBNC                          IRC networks

Laptop  · HexChat  ──┐       ┌─────────────────────┐       ┌──▶ Undernet
Phone   · Goguma   ──┼──────▶│ always connected    │───────┼──▶ Libera.Chat
Browser · panel    ──┘       │ history per device  │       ├──▶ Rizon
(off: nothing is lost)       │ modules and plugins │       └──▶ IRCnet…
                             │ web panel and API   │
                             └─────────────────────┘
        TLS · user/network@device              TLS · SASL · services
```

Your IRC client no longer connects to the network but to xBNC. xBNC keeps **one connection per network** that is not cut when you close the client, and presents itself to each device as if it were that network's server.

1. **You connect** with the server password `user/network@device:password`. The device name (`@laptop`, `@phone`) is optional, but it lets each device keep its own account of what it has read.
2. **xBNC brings you up to date**: it joins your channels for you, sends the topic and the user list, and plays back the messages that arrived since your last connection on that device, without repeating what you already saw.
3. **While you are away**, xBNC stays in the channels, stores the history, answers CTCP, marks you as away and runs your modules and plugins.
4. **All your devices stay in sync**: what you write on the phone appears on the laptop, and read markers are shared.

### What you get when you connect

1. xBNC's welcome with the parameters (ISUPPORT) of the real network.
2. The network's MOTD (unless you turn on [Hide the MOTD](MODULES.md#mod-block_motd)), followed by xBNC's name and version.
3. The channels you are in, with their topic and user list.
4. The messages this device has not seen. If the client does not support `server-time`, each message starts with the date and time in brackets.
5. If you missed private messages or mentions, a summary from `*status` (module [Automatic away](MODULES.md#mod-away)).
6. If the network is disconnected, a notice from `*status` with the reason.
7. This server's message of the day, if a superadministrator has set one (`/msg *status setmotd`).

### History without duplicates

- Each device (the `@name` of the login, or `default` if you give none) has a cursor per network: the last message delivered to it. Two connections with the same name share the cursor.
- When it connects it receives the messages after its cursor, up to `playback_max` (1000 by default). A new device receives the most recent `new_client_playback` messages (100 by default).
- Cursors are saved on disconnection and every 5 seconds, and never go backwards. If the process stops abruptly, at most a few seconds are repeated.
- Clients with `draft/chathistory` get no automatic playback: they ask for what they need with `CHATHISTORY` (up to `chathistory_max`, 500 by default).

### The connection to each network

- States: disconnected, registering and connected. During registration the IRCv3 capabilities, SASL and the nick (with an alternative if it is taken) are negotiated.
- Maximum registration time: 60 seconds. If the server is silent for 90 seconds it is sent a PING, and after 180 the connection is considered lost.
- Automatic reconnection with delays growing from 5 seconds to 5 minutes, with a random ±20 % variation so that thousands of networks do not come back in the same second. If the network says you are reconnecting too fast, xBNC waits at least 2 minutes.
- Output to the server goes through a rate limiter (a burst of 9 lines, then 2 per second) so the network does not throw you out for flooding.
- The server's TLS certificate is always verified against the system's root certificates. For a network with its own or an expired certificate you can trust that particular certificate by its SHA-256 fingerprint (`/msg *status addfingerprint`); there is no option to turn verification off.
- Each network can have backup servers (`/msg *status addserver`). If connecting to a server fails, xBNC tries the next one a second later; after going through the whole list it goes back to the growing delays. If a server that was connected drops the connection, xBNC first goes back to that same server.
- The local address used to connect (vhost) can be set per network or per user (`/msg *status setbindhost`, administrators only).
- If a line is not UTF-8 it is read as Windows-1252, so accented letters from old clients show correctly.

### Virtual users

xBNC adds “users” whose names start with `*`. They do not exist on the network: they are the way to talk to xBNC from any client. `*status`, `*notes`, `*watch`, `*crypt`, `*ignore` and one per plugin (`*trivia`, `*remind`…). See [IRC commands](COMMANDS.md#commands).
