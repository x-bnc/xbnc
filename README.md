<h1 align="center"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/images/logo-xbnc.png"><img src="docs/images/logo-xbnc-dark.png" alt="xBNC" width="420"></picture></h1>

A modern IRC bouncer written in Rust. It is **persistent**: it runs on your server and keeps your connections to IRC networks open while your client is closed or your computer is off, stores the history and plays it back to each device exactly once when you come back. Everything is configured from a web panel, or from IRC with `/msg *status` if you prefer.

Website and documentation: **[xbnc.org](https://xbnc.org)** · [Wiki](https://xbnc.org/wiki/) · [Wiki en español](https://xbnc.org/wiki/es/)

> **Stable release.** The first complete version of xBNC, tested on real networks (Undernet, Libera.Chat, freenode, Rizon, EvilNet, IRCnet and others) before being declared stable. The current version is on [Releases](https://github.com/x-bnc/xbnc/releases) and the changes in [CHANGELOG.md](CHANGELOG.md). What was tested and the known limits: [docs/STATUS.md](docs/STATUS.md). Found a problem? See [Reporting a problem](#reporting-a-problem).

## What it does

**Bouncer**
- Permanent connections to several networks per user, with verified TLS, SASL (PLAIN and EXTERNAL), automatic reconnection, backup servers and flood protection. If the server reboots, systemd starts xBNC again and it reconnects by itself.
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

- Any Linux with systemd: Debian, Ubuntu, Fedora, Rocky, Alma, RHEL, Arch, openSUSE and others.
- **git**, to download the code (`sudo apt install git`, `sudo dnf install git`…). The installer takes care of everything else.
- To build xBNC: about **2 GB of memory** (swap counts; the installer warns below that and explains how to add it) and **3 GB of free disk**. `scripts/prereqs.sh` installs the build tools with the system's package manager (apt, dnf, yum, pacman, zypper or apk) and Rust with rustup; the installer runs it by itself. Building is only needed when installing or updating.
- To run it: 1 CPU and 512 MB of RAM are enough for personal use.
- Ports: with the installer's recommended answer, **8443** for the panel and IRC with SSL together; with one port each, 8443 for the panel and 6697 for IRC. Also 6667 without SSL (local network only) and 50000-50100 for DCC.

The web panel comes already built inside the source code: Node.js and npm are not needed.

## Installation

```bash
# 1. Download the code (as a normal user, not as root)
git clone https://github.com/x-bnc/xbnc.git

# 2. Go into its folder
cd xbnc

# 3. Install it
sudo ./scripts/install.sh
```

The code does not include the compiled program, so the first time the installer builds it on your server (several minutes; more on a small VPS). It asks for its language, whether you want to use a domain and which ports you want, creates the certificate (self-signed or from Let's Encrypt), opens the ports and installs the service, which starts by itself at every boot. At the end it shows the panel address, the IRC port and the one-time code to create your account. No file needs editing.

New to Linux servers? Follow the **[step-by-step guide for beginners](https://xbnc.org/wiki/#beginner-guide)** ([en español](https://xbnc.org/wiki/es/#beginner-guide)).

**With the .zip of a release:** each stable version on [Releases](https://github.com/x-bnc/xbnc/releases) also has a `.zip` with the program already built for 64-bit PCs and servers (x86_64, glibc 2.28 or later), so nothing has to be compiled: unzip it, go into the `xbnc` folder and run the same `sudo ./scripts/install.sh`. The .zip only exists when a stable version has been published; `git clone` always works.

- With a domain and Let's Encrypt, without questions: `sudo ./scripts/install.sh --domain bnc.example.org --letsencrypt you@example.org`
- With the panel at `https://bnc.example.org` behind Nginx: add `--nginx`.
- Every option: `./scripts/install.sh --help`. Without a terminal (or with `--non-interactive`) it asks nothing; `--language CODE` chooses the language (English by default).
- Redoing the configuration (for example, to move from an IP to a domain): `sudo ./scripts/install.sh --reconfigure`.

**Updating** makes a backup first and, if the service does not start, goes back to the previous version. It does not touch users, networks or history.

```bash
# Installed with git clone: download the changes and build them
cd xbnc
git pull
sudo ./scripts/update.sh --build

# Installed with the .zip: unpack the new one in an empty folder, then
sudo ./scripts/update.sh
```

After `git pull`, always use `--build`: without it, `update.sh` reuses the program built by the previous installation, which is the old version.

What each script in `scripts/` is for (install, update, backup, restore, uninstall…): [The scripts folder](https://xbnc.org/wiki/#scripts).

## Connecting your client

| What you want | Server password |
| --- | --- |
| A user with a single network | `alice:password` |
| Choose the network | `alice/libera:password` |
| Choose the network and name the device | `alice/libera@laptop:password` |

Use the IRC port with SSL that the installer showed: 8443 if you chose one port for the panel and IRC, 6697 if you chose one each (or 6667 without SSL, only from your local network). The panel shows these details ready to copy on each network's page, and `sudo xbncd info` shows them on the server.

## Reporting a problem

1. Download the report from **Your account → Test report** in the panel. It contains no passwords, messages or other people's nicks.
2. Open an [issue](https://github.com/x-bnc/xbnc/issues) with the report and with what you did, what you expected and what happened.

The real-network test plan that version 1.0.0 passed is in [testenv/real/PLAN.md](testenv/real/PLAN.md), if you want to test xBNC on your own networks.

Security flaws are not made public: follow [SECURITY.md](SECURITY.md).

## Translating xBNC

Every text lives in `locales/<code>.json` (English is the base). To add a language: copy `locales/en.json`, translate it, check it with `xbncd locales check` and put it in `/etc/xbnc/locales/`. It appears when xBNC is reloaded, without rebuilding. Complete guide: [TRANSLATING.md](TRANSLATING.md).

## Documentation

The complete documentation, with search and in English and Spanish, is the **[xBNC wiki](https://xbnc.org/wiki/)**: installation, clients, networks, every `*status` command, the panel, the 24 modules, the 18 plugins and how to write new ones, security, configuration, administration, data and migration, the REST API, architecture, threat model, performance, testing and common problems.

The same documentation is in this repository in [docs/](docs/README.md), and in Spanish in [docs/es/](docs/es/README.md), generated from the wiki, with the REST API in [docs/openapi.json](docs/openapi.json). The status of this version is in [docs/STATUS.md](docs/STATUS.md) and the changes in [CHANGELOG.md](CHANGELOG.md).

From the program itself, the panel explains every module and plugin, and from IRC `/msg *status help` answers, as does each plugin with its own name (for example `/msg *poll help`).

## License

**xBNC** — [https://xbnc.org](https://xbnc.org). Apache License 2.0. See [LICENSE](LICENSE), [NOTICE](NOTICE) and [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md). xBNC is written from scratch and contains no code from other bouncers.
