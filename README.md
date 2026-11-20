<h1 align="center"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/logo-xbnc.png"><img src="docs/images/logo-xbnc-dark.png" alt="xBNC" width="420"></picture></h1>

A modern IRC bouncer written in Rust. It keeps your connections to IRC networks open while your client is closed, stores the history and plays it back to each device exactly once when you come back. Everything is configured from a web panel, or from IRC with `/msg *status` if you prefer.

> **Version 1.0.0, public test release.** It is the first complete version and is open to testing on real networks (Undernet, Libera.Chat, freenode, Rizon, EvilNet, IRCnet and others) before it is declared stable for production. What works, what is missing and the known limits: [docs/STATUS.md](docs/STATUS.md). How to take part in the tests: [Testing the beta](#testing-the-beta).

## What it does

**Bouncer**
- Permanent connections to several networks per user, with verified TLS, SASL (PLAIN and EXTERNAL), automatic reconnection, backup servers and flood protection.
- Several devices at once: what you write on one appears on the others, and each one gets what it missed exactly once.
- Use your usual client (mIRC, HexChat, WeeChat, irssi…) with server, port and password. Modern clients also get history (`CHATHISTORY`), read markers and all your networks in a single connection.
- Full IRCv3 with servers and clients (`multi-prefix`, `extended-join`, `away-notify`, `account-notify`, `chghost`, `setname`…). Clients that do not ask for it receive plain old IRC.
- Web chat in the panel, and notifications of mentions and private messages by Telegram, Discord, e-mail, webhook or push notifications.
- From IRC, `/msg *status help` with 64 grouped commands: backup servers, trusted certificates, bind hosts, disabled and ordered channels, history, modules and administration (users, traffic, notices to everyone, message of the day, restart). The names used by other bouncers are accepted too.

**24 native modules**, configured per network from the panel or with `/msg *status`:
- **Connection and account:** services login (Undernet's X with Login on Connect and your IP hidden before joining channels, NickServ, Q, AuthServ), automatic invite and unban, client certificate with SASL EXTERNAL, keep nick.
- **Channels:** sticky channels, rejoin after a kick, automatic op and voice, get op back when left alone, detached channels, hide flooding channels.
- **Messages:** automatic away with a summary, watch words, aliases, ignore (with SILENCE on Undernet), strip colours, Latin-1 encoding, FiSH encryption with DH1080 key exchange, scheduled tasks, CTCP replies, hide the MOTD.
- **History:** channel events, log files in the classic IRC format, replies only to the device that asked, DCC through xBNC without revealing your IP.

**18 official plugins** (sandboxed WebAssembly, with permissions): trivia, greetings, link titles, factoids, weather, RSS news, karma, quotes, bad words, games, dice, polls, reminders, tell, auto-replies, channel stats, seen and anti-flood. They come installed and turned off: each user turns them on in their networks from *Modules*. C and Rust SDKs to write your own.

**Web panel**
- Live dashboard, a wizard to add networks with the well-known ones ready, modules and plugins per network with copying the configuration to other networks.
- History with full-text search and export, users with five roles and quotas, security, audit log, backups and alerts.
- Two-step verification, passkeys, OpenID Connect and LDAP.
- In English and Spanish, and in any other language with a translation file ([TRANSLATING.md](TRANSLATING.md)); light and dark theme, and comfortable on a phone.

**Security**
- Argon2id, encrypted credentials and module keys, per-IP brute-force blocking, CSRF, CSP and `HttpOnly` sessions.
- The service runs without privileges and passwords never appear in the logs.
- A written threat model, continuous *fuzzing* and a vulnerability disclosure policy ([SECURITY.md](SECURITY.md)).

## Requirements

- Any Linux with systemd: Debian, Ubuntu, Fedora, Rocky, Alma, RHEL, Arch, openSUSE and others. Nothing has to be installed first.
- On 64-bit PCs and servers (x86_64) the program comes ready-made in `bin/` and runs on any distribution from 2019 on (glibc 2.28 or later).
- On other machines (ARM) or older systems the installer builds it by itself: `scripts/prereqs.sh` installs the build tools with the system's package manager (apt, dnf, yum, pacman, zypper or apk) and Rust with rustup. Building needs 2 GB of memory (swap included) and 3 GB of disk.
- 1 CPU and 512 MB of RAM are enough for personal use.

The web panel comes already built inside the source code: Node.js and npm are not needed.

## Installation

```bash
unzip xbnc-1.0.0.zip && cd xbnc
sudo ./scripts/install.sh
```

The installer asks whether you want to use a domain, creates the certificate (self-signed or from Let's Encrypt), opens the ports and, at the end, shows the panel address and the one-time code to create your account. No file needs editing.

- With a domain and Let's Encrypt, without questions: `sudo ./scripts/install.sh --domain bnc.example.org --letsencrypt you@example.org`
- With the panel at `https://bnc.example.org` behind Nginx: add `--nginx`.
- Updating an existing installation: unpack the new version and run `sudo ./scripts/update.sh`. It makes a backup first and, if the service does not start, goes back to the previous version. It does not touch users, networks or history.
- Redoing the configuration (for example, to move from an IP to a domain): `sudo ./scripts/install.sh --reconfigure`.
- Every option: `./scripts/install.sh --help`. To build from the source code even on x86_64: `--build`.
- The installer asks first for its language, which is also xBNC's default language: English (the default), Spanish, or any other language file in `locales/`. Without a terminal it uses English; `--language CODE` chooses it.

## Connecting your client

| What you want | Server password |
| --- | --- |
| A user with a single network | `alice:password` |
| Choose the network | `alice/libera:password` |
| Choose the network and name the device | `alice/libera@laptop:password` |

Port 6697 with SSL (or 6667 without SSL, only from your local network). The panel shows these details ready to copy on each network's page.

## Testing the beta

1. Install xBNC as explained above and add your networks from **Networks → Add network**.
2. Follow the checklist in [testenv/real/PLAN.md](testenv/real/PLAN.md). It has one section per network and one per module and plugin.
3. When something fails or is unclear, download the report from **Your account → Test report** and send it with what you did, what you expected and what happened. The report contains no passwords, messages or other people's nicks.

Security flaws are not made public: follow [SECURITY.md](SECURITY.md).

## Translating xBNC

Every text lives in `locales/<code>.json` (English is the base). To add a language: copy `locales/en.json`, translate it, check it with `xbncd locales check` and put it in `/etc/xbnc/locales/`. It appears when xBNC is reloaded, without rebuilding. Complete guide: [TRANSLATING.md](TRANSLATING.md).

## Documentation

The complete documentation is in [docs/](docs/README.md), and in Spanish in [docs/es/](docs/es/README.md): installation, clients, networks, every `*status` command, the panel, the 24 modules, the 18 plugins and how to write new ones, security, configuration, administration, data and migration, the REST API ([docs/openapi.json](docs/openapi.json)), architecture, threat model, performance, testing and common problems. The status of this version is in [docs/STATUS.md](docs/STATUS.md) and the changes in [CHANGELOG.md](CHANGELOG.md).

The same documentation, with search and in English and Spanish, is published as the **xBNC wiki**, a website of its own (a separate project, `xbnc-wiki`), so anyone can read it without installing xBNC. The files of `docs/` and `docs/es/` are generated from the wiki.

From the program itself, the panel explains every module and plugin, and `/msg *status help` and `/msg *plugin help` answer from IRC.

## License

Made by **y2k**. Apache License 2.0. See [LICENSE](LICENSE), [NOTICE](NOTICE) and [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md). xBNC is written from scratch and contains no code from other bouncers.
