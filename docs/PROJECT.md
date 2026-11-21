# Project

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

- [What's new](#changelog)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
- [Security policy](#security-policy)
- [Licence](#license)

<a id="changelog"></a>

## What's new

xBNC 1.0.0, published on 21 November 2026. These are the notes of this version, the same as in the release on GitHub. The full history is in [CHANGELOG.md](https://github.com/x-bnc/xbnc/blob/main/CHANGELOG.md), and every version with its download in [Releases](https://github.com/x-bnc/xbnc/releases).

First complete version, ready for testing on real networks (`testenv/real/PLAN.md`).

### Added

- One port for the panel and IRC (`[web] shared = true`, installer `--single-port N`): browsers get the panel and IRC clients the bouncer, with the same certificate; plain `http://` is sent to `https://`, and an IRC client without SSL is told to turn it on.
- The installer asks for the ports (one for everything, or one for the panel and one for IRC), refuses ports that browsers will not open (6667, 6697…) and ports in use, and refuses port options that an existing configuration would not apply (without `--reconfigure`).
- DCC ports: the installer writes `[dcc] ports = "50000-50100"` and opens them in the firewall (`--dcc-ports A-B|off`).
- Let's Encrypt by DNS for machines without port 80 (`--dns cloudflare|digitalocean|ovh|linode|rfc2136 --dns-credentials FILE`, or asked in the installer).
- Profile photos in *Your account*: cropped to 256×256 in the browser, stored by xBNC after checking the real format (PNG, JPEG or WebP, at most 256 KB and 1024×1024), shown in the panel to the user and the administrators (`PUT/DELETE /api/v1/me/avatar`, `GET/DELETE /api/v1/users/{id}/avatar`).
- The version appears everywhere as `xBNC v1.0.0 - https://xbnc.org` (panel, sign-in, IRC welcome, `/msg *status version`), taken from the version of the code: it is also the default CTCP VERSION reply (it can be changed in the CTCP module; `{version}` is replaced by the running version) and the QUIT message xBNC sends when it leaves a network (disconnect, reconnect, network turned off, shutdown).
- Official signed plugin catalogue: when its key is set, xBNC checks it once a day and updates the installed plugins that have a newer version there, after checking the Ed25519 signature and each SHA-256.
- Installation without installing anything first: the .zip brings the program ready-made for x86\_64 in `bin/` (it runs on any distribution from 2019 on, glibc 2.28 or later, and is checked against `bin/SHA256SUMS` before use). On other machines, or with `--build`, the installer runs the new `scripts/prereqs.sh`, which installs the build tools with the system's package manager (apt, dnf, yum, pacman, zypper or apk) and Rust with rustup, and builds the program as the owner of the source folder, never as root. It warns when there is not enough memory or disk to build. `update.sh` takes the program the same way. `prereqs.sh --check` says what is missing without installing anything.
- The installer speaks the language chosen for the installation, and English is the default. In a terminal it asks first, with a menu of the language files (`locales/*.json`); `--language CODE` chooses it without asking. That language becomes xBNC's default language, and a language that is not built in is installed in `/etc/xbnc/locales`. The texts of the installer, `prereqs.sh` and `update.sh` are in a new `installer` section of the language files, so translating xBNC into a language translates its installer too. They are read with bash and awk only, and `xbncd locales check` checks them like the other texts.
- The official logo in the panel (sign-in page, side menu and phone header, with dark letters on the light theme and white ones on the dark theme), as the icon of the panel, of the browser tab and of the app installed on a phone, in the wiki, on the website and in the README.
- The documentation in Spanish in `docs/es/`, generated from the Spanish wiki like the English one in `docs/`.
- `/msg *status joinnow`: joins the waiting channels at once, without waiting for the hidden host.
- Ten new modules:
  - Services login: Undernet's X with Login on Connect, NickServ, Q and AuthServ, waiting for the hidden host before joining channels, and nick recovery.
  - Invite and unban with X, ChanServ or Q.
  - Character encoding (Latin-1 and Windows-1252).
  - Detached channels (`/quote DETACH #channel`).
  - Hide flooding channels.
  - Strip colours.
  - Get op back when left alone.
  - FiSH encryption (ECB and CBC) with DH1080 key exchange (`/msg *crypt`).
  - Ignore, with SILENCE on Undernet (`/msg *ignore`).
  - Scheduled tasks in your time zone.
- Ten new official plugins: trivia, greetings, link titles, factoids, weather, RSS/Atom news, karma, quotes, bad words and games (hangman, magic 8 ball and rock-paper-scissors).
- IRCv3 with the server: `multi-prefix`, `extended-join`, `away-notify`, `account-notify`, `account-tag`, `chghost`, `userhost-in-names`, `invite-notify`, `setname` and `cap-notify`. They are offered to clients too, and clients that do not ask for them receive the usual messages.
- Time zone per user (Your account): log files and scheduled tasks in your local time.
- Test report (Your account and `GET /api/v1/diagnostics`): capabilities, ISUPPORT and login state of each network, without passwords or messages.
- Security policy (`SECURITY.md`) and real-network test plan (`testenv/real/PLAN.md`).
- Complete documentation in English in `docs/` (installation, clients, networks, commands, panel, modules, plugins, security, configuration, administration, data, API, architecture, threat model, performance, testing and common problems), generated from the English wiki, plus `docs/STATUS.md`. README, CONTRIBUTING, SECURITY and the test plan in English, and a “Test report” template for GitHub issues.
- freenode and EvilNet in the network wizard. The Undernet entry uses port 6667 without TLS, because the network does not offer it.
- Secret module settings (passwords and keys) are stored encrypted and the panel never shows them again.
- Plugins can ask for timer events every 5 to 60 seconds (`timer_secs`).
- Complete `*status`, with 64 commands:
  - Help grouped in sections, with the command names in bold and aligned columns, in English and Spanish. It shows only what your account can use. `help <command>` explains one with an example, and `help <word>` searches.
  - The names used by other bouncers are accepted too (`ListChans`, `ListMods`, `AddTrustedServerFingerprint`…), not case-sensitive.
  - Backup servers per network (`addserver`, `delserver`, `servers`, `jump server`). If a server fails, xBNC tries the next one straight away.
  - Trusted server certificates by their SHA-256 fingerprint (`addfingerprint`), for networks with a self-signed certificate.
  - Local address (vhost) per network and per user (`setbindhost`, `setuserbindhost`), administrators only.
  - Disabled channels that stay saved (`disablechan`, `enablechan`) and joining order (`movechan`, `swapchans`). Also `nicks`, `topics`, and `attach` and `detach` with wildcards.
  - History: `playbuffer`, `clearbuffer` with wildcards, `clearallbuffers`, `clearallchannelbuffers`, `clearallquerybuffers` and `setbuffer`.
  - `addnetwork` and `delnetwork` (with confirmation), `availmods`, `reloadmod` and `uptime`.
  - Administration: `users`, `allnetworks`, `clients <user>`, `traffic`, `broadcast`, `movenetwork`, `rehash`, `listports`, the server's message of the day (`setmotd`, `addmotd`, `clearmotd`, `showmotd`), `restart` and `shutdown`.
- The panel marks disabled channels.
- Translations in files: every text of xBNC (panel, sign-in page, IRC messages, `*status` help, modules) lives in `locales/<code>.json`, with English as the base and Spanish included.
  - A new language is added by copying its file to `/etc/xbnc/locales/` (`locales_dir` in xbnc.toml or `XBNC_LOCALES_DIR`), without rebuilding, and it appears in every language selector and in `/msg *status language`. It is read at start and on reload (`systemctl reload` or `/msg *status rehash`).
  - Whatever a translation lacks is shown in English. Texts with line breaks, control characters or placeholders different from English are ignored.
  - `xbncd locales check` checks a file (what is missing, what is extra, placeholders and bold tags) and `xbncd locales list` shows the installed languages with their share translated.
  - `TRANSLATING.md`: guide for translators.
  - Public API `GET /api/v1/i18n` and `GET /api/v1/i18n/{code}`.
- Wiki in English and Spanish at https://xbnc.org/wiki/, a website of its own that anyone can read without installing xBNC: one page per topic (getting started, using xBNC, features, administration, development, reference, help and project), search across all of it, and the project pages (what's new, roadmap, contributing, security policy and licence).
- The 18 official plugins come installed: xBNC installs the missing ones on the first start and after each update, and updates old ones. They stay off on every network until each user turns them on. One uninstalled on purpose does not come back until the next version.

### Fixed

- Weather, link titles and news never answered: the official plugins that go out to the internet were installed without their sites approved, so every request was refused. They come approved now (installations made before are repaired once); a site blocked by an administrator stays blocked.
- News: with more than one feed, only the first one was checked, and every minute instead of every interval. A feed that cannot be read is told to the user once and shown by `/msg *news list`.
- Feed and page titles: numeric entities (`&#8217;`, `&#x2014;`) and CDATA after a line break are decoded.
- Plugin web requests follow up to 3 redirects, only over https and only to approved sites (moved feeds, Wikipedia articles that are redirects).
- Official plugins are replaced by the shipped ones also when the version number is the same but the file has a fix.
- EvilNet identifies with NickServ: “Automatic” and the network wizard use it, and networks added before with the AuthServ login are sent to NickServ.
- Language: changing it in *Settings* did not apply to someone who had chosen a language on the sign-in page (the panel stayed in that one). Now it is saved and applied when clicked, also to whoever changes it.
- The seen, remind, tell, poll, chanstats and antiflood plugins turned on in 0.8.0 stored “es” as their language and stayed in Spanish after switching to English. On update that old value is cleared and they answer in each user's language.
- Automatic test of the language change in the panel (`testenv/ui/language_check.py`).
- Hidden host: with `/mode $nick +x` in the commands on connect, the channels were joined before the network hid the host, so the real IP showed in every channel. Now no JOIN leaves (the saved channels, those in the commands on connect, a client's own or the rejoins) until the network confirms the hidden host (396). The Services login module with “Hide my IP” behaves the same: if the host is still not hidden after the wait (a failed X login, for example), the channels keep waiting and `*status` explains why, instead of joining with the real IP. New `/msg *status joinnow` joins them anyway.
- IRC login errors say what is wrong and how to fix it, in the server's language: wrong user or password with the right format, the networks of the account when the network does not exist, and how long an address stays blocked and where to unblock it. Copying the example password (“YOUR\_PASSWORD”) is explained and no longer counts as a failed attempt, so a client retrying with it no longer blocks the address on IRC and in the panel.
- The panel's blocked-address message says the IP, the minutes left, that a client retrying with a wrong password also counts, and where an administrator lifts the block.
- “Connect your IRC client” shows the real data: the domain or IP given at installation (not the address the panel happens to be opened with), the IRC port, whether the certificate is valid for that name (and if not, how to accept it), and the local network address with the port without SSL when connecting from home. The device name and the password are written in the panel to build the complete server password (the password is not sent or saved). Before, it always said “laptop” and “YOUR\_PASSWORD”. The step list for each client no longer runs steps together.
- `xbncd info` shows the address clients use (the configured domain or IP), the local network address, and the server password of every network of every user. The installer's summary explains where to connect from the internet and from the local network.
- The self-signed certificate of a domain installation also covers the machine's local addresses.
- The panel address shown by `xbncd info`, by `xbncd setup-code`, in the log and in the setup code file is the one given at installation (`--ip` or the domain), not the local address the machine reports.
- The installer's summary no longer says that xBNC is running when the service was not started (`--no-start`, or without systemd), and says how to start it.
- `xbncd info`, `setup-code`, `audit`, `check-config`, `locales` and `release` stop quietly when their output goes into `head` or another reader that closes early, instead of printing a "Broken pipe" error with a backtrace.
- The installer installs its packages also on Arch (pacman) and openSUSE (zypper).
- Web chat on the phone: a long channel name is cut with "…" instead of being drawn under your nick, and the network list takes less room.
- Web chat: on the phone and on narrow screens the list of people in the channel starts closed, so the messages have the whole width; it opens with its button. On a computer it starts open, as before.

### Security

- Services login: on networks without NickServ (IRCnet, EFnet) the “Automatic” option never sends the password, because anyone could use that nick; xBNC says so in \*status and in the log.
- FiSH encryption: the DH1080 key exchange accepts 5 requests per minute and 100 keys per session, and decrypted text cannot contain line breaks or null characters.

### Changed

- Renaming a network no longer cuts the connection.
- History cursors are saved every 5 seconds instead of 30: much less is repeated after an abrupt stop.
- Search also finds pieces from the middle of a word when there are no whole-word matches.
- `xbncd import --utc-offset` and `import-logs` accept a time zone (`Europe/Madrid`) and apply each day's daylight saving time.
- DCC: if the connection to the network uses IPv6, offers announce the server's public IPv4, which every client understands.
- Lines that are not UTF-8 are decoded as Windows-1252 instead of showing “�”.
- The panel and the server messages are written in English, with Spanish as a translation.
- Saved channels are listed in their joining order instead of alphabetically.
- The release workflow (`.github/workflows/release.yml`), which builds .deb and .rpm packages and container images, only runs when started by hand: during the beta xBNC is published as a source .zip.
- The sign-in page uses the xBNC colours (yellow and black) in the light and dark themes, with the xBNC logo and an IRC window of the `#xbnc` channel.
- Every example channel in the panel, the help, the plugins and the documentation is `#xbnc` (and `#xbnc-dev` when a second one is needed).
- The wiki is no longer inside the app: it is a separate project (`xbnc-wiki`).

<a id="roadmap"></a>

## Roadmap

What comes next, in no fixed order. It can change with what people ask for: propose ideas or vote for them in the [issues](https://github.com/x-bnc/xbnc/issues).

- **Ready-made program for ARM** (Raspberry Pi, ARM VPS). Today the installer builds it on those machines, which takes longer.
- **The official signed plugin catalogue switched on**, so plugin fixes and new plugins reach every server without waiting for a new version of xBNC.
- **More official plugins**, chosen from what users ask for.
- **An official IRC channel** for the community, announced here when it opens.
- **More languages** for the panel, the IRC messages and this wiki: each one is a file anyone can translate ([Translating xBNC](TRANSLATIONS.md#translations)).

What has already been done is in [What's new](#changelog).

<a id="contributing"></a>

## Contributing

xBNC is free software and every contribution helps: a bug report, an idea, a translation, a plugin or code.

### Reporting a problem

Open an [issue](https://github.com/x-bnc/xbnc/issues/new/choose) and include the version (`xbncd --version` or the panel footer), what you did, what you expected and what happened. For problems with a network, attach the test report of *Your account*: it has the capabilities and login state of each network, without passwords or messages. Security problems are never reported in public: see the [security policy](#security-policy).

### Proposing an idea

Open an issue explaining what you want to do and why. Talking about it first avoids work that does not fit.

### Code

```sh
cargo build
cargo test --all
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

- Code and comments are written in English; texts shown to people go to `locales/en.json` with their Spanish translation in `locales/es.json`.
- Every fix comes with a test that fails without it, so the problem cannot come back.
- A new API route is documented in `docs/openapi.json` (a test checks it).
- The panel is built into `web/dist`, which is committed with the changes to the panel.

The details are in [CONTRIBUTING.md](https://github.com/x-bnc/xbnc/blob/main/CONTRIBUTING.md).

### Translations and plugins

A translation is a file per language ([Translating xBNC](TRANSLATIONS.md#translations)). Plugins are written in C, Rust or any language that compiles to WebAssembly ([Writing plugins](PLUGINS.md#plugin-development)).

<a id="security-policy"></a>

## Security policy

### Versions with security fixes

| Version | Fixes |
| --- | --- |
| 1.0.x | Yes |
| 0.x | No: update to 1.0 |

### How to report a vulnerability

Do not open a public issue. Use GitHub's private advisory: [Report a vulnerability](https://github.com/x-bnc/xbnc/security/advisories/new). If you cannot use it, open an issue asking for a private contact, without any detail of the problem.

If you can, include the version, the part affected (IRC, panel, API, plugins, installer…), the steps to reproduce it and what someone exploiting it could achieve.

### What you can expect

- An answer within 3 days at most.
- An assessment within 7 days: if it is confirmed, its severity and a plan.
- A fix published within 90 days at most, sooner if it is serious, keeping you informed.
- Credit in the release notes, unless you prefer not to appear.

Please do not publish the details until a fixed version is out or 90 days have passed, whichever comes first. The scope is everything in the repository: the server, the panel, the API, the SDK, the official plugins and the installer. What xBNC protects and how is in the [threat model](THREAT_MODEL.md#threat-model).

<a id="license"></a>

## Licence

xBNC is free software under the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0): you can use it, study it, change it and share it, also commercially.

- Keep the licence and the copyright notices when you share it or a work based on it.
- Say which files you have changed.
- It comes without warranty.

xBNC is written from scratch and includes no code from other bouncers. The libraries it uses and their licences are in [THIRD\_PARTY\_LICENSES.md](https://github.com/x-bnc/xbnc/blob/main/THIRD_PARTY_LICENSES.md); the full text of the licence is in [LICENSE](https://github.com/x-bnc/xbnc/blob/main/LICENSE).
