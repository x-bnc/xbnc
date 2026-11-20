# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning follows [SemVer](https://semver.org/).

## [1.0.0] - 2026-11-19

First complete version, ready for testing on real networks (`testenv/real/PLAN.md`).

### Added

- One port for the panel and IRC (`[web] shared = true`, installer `--single-port N`): browsers get the panel and IRC clients the bouncer, with the same certificate; plain `http://` is sent to `https://`, and an IRC client without SSL is told to turn it on.
- The installer asks for the ports (one for everything, or one for the panel and one for IRC), refuses ports that browsers will not open (6667, 6697…) and ports in use, and refuses port options that an existing configuration would not apply (without `--reconfigure`).
- DCC ports: the installer writes `[dcc] ports = "50000-50100"` and opens them in the firewall (`--dcc-ports A-B|off`).
- Let's Encrypt by DNS for machines without port 80 (`--dns cloudflare|digitalocean|ovh|linode|rfc2136 --dns-credentials FILE`, or asked in the installer).
- Profile photos in *Your account*: cropped to 256×256 in the browser, stored by xBNC after checking the real format (PNG, JPEG or WebP, at most 256 KB and 1024×1024), shown in the panel to the user and the administrators (`PUT/DELETE /api/v1/me/avatar`, `GET/DELETE /api/v1/users/{id}/avatar`).
- `/msg *status version` and the default CTCP VERSION reply say `xBNC 1.0.0 Made by y2k - https://xbnc.org`; the reply can be changed in the CTCP module (`{version}` is replaced by the running version).
- Official signed plugin catalogue: when its key is set, xBNC checks it once a day and updates the installed plugins that have a newer version there, after checking the Ed25519 signature and each SHA-256.
- Installation without installing anything first: the .zip brings the program ready-made for x86_64 in `bin/` (it runs on any distribution from 2019 on, glibc 2.28 or later, and is checked against `bin/SHA256SUMS` before use). On other machines, or with `--build`, the installer runs the new `scripts/prereqs.sh`, which installs the build tools with the system's package manager (apt, dnf, yum, pacman, zypper or apk) and Rust with rustup, and builds the program as the owner of the source folder, never as root. It warns when there is not enough memory or disk to build. `update.sh` takes the program the same way. `prereqs.sh --check` says what is missing without installing anything.
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
- Wiki in English and Spanish on a single page, with a language selector, and a page per language. It is a separate project (`xbnc-wiki`), a website of its own that anyone can read without installing xBNC, built with its `build.py` from one content file per language, so it can be translated too.
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
- IRC login errors say what is wrong and how to fix it, in the server's language: wrong user or password with the right format, the networks of the account when the network does not exist, and how long an address stays blocked and where to unblock it. Copying the example password (“YOUR_PASSWORD”) is explained and no longer counts as a failed attempt, so a client retrying with it no longer blocks the address on IRC and in the panel.
- The panel's blocked-address message says the IP, the minutes left, that a client retrying with a wrong password also counts, and where an administrator lifts the block.
- “Connect your IRC client” shows the real data: the domain or IP given at installation (not the address the panel happens to be opened with), the IRC port, whether the certificate is valid for that name (and if not, how to accept it), and the local network address with the port without SSL when connecting from home. The device name and the password are written in the panel to build the complete server password (the password is not sent or saved). Before, it always said “laptop” and “YOUR_PASSWORD”. The step list for each client no longer runs steps together.
- `xbncd info` shows the address clients use (the configured domain or IP), the local network address, and the server password of every network of every user. The installer's summary explains where to connect from the internet and from the local network.
- The self-signed certificate of a domain installation also covers the machine's local addresses.
- The panel address shown by `xbncd info`, by `xbncd setup-code`, in the log and in the setup code file is the one given at installation (`--ip` or the domain), not the local address the machine reports.
- The installer's summary no longer says that xBNC is running when the service was not started (`--no-start`, or without systemd), and says how to start it.
- `xbncd info`, `setup-code`, `audit`, `check-config`, `locales` and `release` stop quietly when their output goes into `head` or another reader that closes early, instead of printing a "Broken pipe" error with a backtrace.
- The installer installs its packages also on Arch (pacman) and openSUSE (zypper).
- Web chat on the phone: a long channel name is cut with "…" instead of being drawn under your nick, and the network list takes less room.
- Web chat: on the phone and on narrow screens the list of people in the channel starts closed, so the messages have the whole width; it opens with its button. On a computer it starts open, as before.

### Security

- Services login: on networks without NickServ (IRCnet, EFnet) the “Automatic” option never sends the password, because anyone could use that nick; xBNC says so in *status and in the log.
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

## [0.8.1] - 2026-11-11

### Fixed

- Official plugins reviewed on a real IRC network: all of them answer `/msg *plugin` with their help, also work with your own messages and answer in your xBNC language.
  - Dice: `!dado`, `!dados`, `!roll` and `!dice`, also in private, with a clear text (“bob rolls 2 6-sided dice: 3 and 5 (total 8)”).
  - Polls: the vote is confirmed in private, `!vote` without a poll says so, aliases `!encuesta` and `!votar`, and a clear message when the options are missing.
  - Reminders: `!recuerda en 10 minutos …`, `!recuerda 2 horas y 30 minutos …` and `!recuerda a las 18:30 …` (with time zone).
  - Tell: you can leave messages yourself, it knows whether the person is in the channel, and messages are delivered when they join or speak.
  - Auto-replies: whole words (also after ¿ ¡), replies to private messages with the `*` rule, and `/msg *autoresponder test`.
  - Stats: aliases `!estadisticas` and `!top`, and `/msg *chanstats #channel`.
- If a plugin does not answer `/msg *plugin`, xBNC explains what it does and how it is used; the panel shows it under “How to use it”.
- Modules: the network is chosen from a list (works with many networks) and one network's configuration can be copied to the others.
- Development mode: packs the `name.json` you put next to the `.wasm` on its own and turns new plugins on in your networks.
- `scripts/install.sh`: asks whether you want to use a domain, checks the DNS, warns about the router ports and, with `--nginx`, leaves the panel at `https://domain` with Let's Encrypt.
- Nginx example: allows uploading plugins and restoring backups.

### Changed

- Dark theme in black, yellow and grey.
- The version appears everywhere as “xBNC 0.8.1 Made by y2k” (panel, sign-in, `*status version`, CTCP VERSION and IRC welcome), taken from the version it is built with.

## [0.8.0] - 2026-11-10

### Added

- Plugins: `presence` permission (joins, parts, nick changes and user list), large own storage, `http` permission limited to the domains the administrator approves, a catalogue with one-click installation and a development mode with a live log.
- Official plugins: `!seen`, reminders, `!tell`, polls, channel stats and anti-flood.
- Web chat in the panel, a mentions view and read markers shared between devices (`draft/read-marker`).
- Several networks in a single connection for modern clients (`soju.im/bouncer-networks`).
- Notifications of mentions and private messages by Telegram, Discord, e-mail, your own web address and push notifications (the panel can be installed as an app).
- Automatic away message and nick; history retention per channel; history quotas per user.
- Backups from the panel: create, download, restore and automatic ones with rotation.
- Passkeys, sign-in with OpenID Connect and with LDAP.
- PostgreSQL as the database (`database.url`), active/standby high availability (`database.ha`) and `xbncd migrate-to-postgres`.
- ARM64 packages and Docker image; import of IPv6 servers; Grafana dashboard and Prometheus alerts.
- Undernet: the wizard identifies with X when connecting (Login on Connect, mode +x).

## [0.7.0] - 2026-11-09

Testing and hardening before production, signed packages and updating from the panel. Still a test release; see `docs/STATUS.md`.

### Added

- DEB and RPM packages (`packaging/build-packages.sh`): they install the service and run `xbnc-setup` on the first installation; updating is installing the new package.
- Ed25519 release signing: `xbncd release keygen`, `sign` and `verify`, and `packaging/sign-packages.sh` (with optional GPG for apt/dnf repositories).
- Automatic publishing when a tag is created (`.github/workflows/release.yml`): signed packages, a GitHub release and the Docker image on `ghcr.io` with an SBOM.
- New version notices on Home and in *Settings* (`[updates] repository` or `url`), checked once a day.
- **Updating from the panel** with DEB/RPM packages and `[updates] public_key`: xBNC downloads and verifies the signed package, and the `xbnc-update` service (as `root`) verifies it again without trusting the xBNC process, refuses old versions and installs it. The result is shown in the panel and in the audit log.
- API: `GET /updates`, `POST /updates/check`, `POST /updates/install`.
- Continuous *fuzzing* (`fuzz/`, weekly in CI) of the IRC parser, DCC, the configuration importer, search, plugin manifests and running arbitrary plugins.
- Threat model (`docs/THREAT_MODEL.md`).
- Tests against real IRC servers (`tests/real_ircd.rs`): Ergo, InspIRCd, UnrealIRCd, Solanum and miniircd in CI.
- Load test (`cargo run --release --example loadtest`) and results in `docs/PERFORMANCE.md`.

### Security

- At most 16 unauthenticated IRC connections per IP address: a single machine can no longer take every slot of `max_connections`.
- Panel: at most 64 connections per IP and 4,096 in total, and connections without traffic for 2 minutes are closed (before, a single machine could exhaust the file descriptors by opening connections or sending requests byte by byte).
- The API no longer reveals which user identifiers exist to accounts not allowed to manage them.

### Fixed (found with *fuzzing*)

- The IRC parser refuses lines with CR, LF or NUL inside and commands that are not alphanumeric.
- DCC offers with control characters are refused.
- `xbncd plugin pack` with sections of impossible size returned an overflow instead of an error.
- A plugin with a long loop could exhaust the native stack and crash the process in builds where the compiler does not optimise the interpreter's calls: Wasmi now uses the loop execution mode, which never grows the stack.

### Fixed (found with the load tests)

- Memory: with many simultaneous logins, glibc kept the Argon2id blocks per thread (1.6 GB with 500 clients; now 55 MB).
- Argon2id is limited to one run per CPU at a time.
- History is stored by a writer thread that groups messages in one transaction: latency with 500 networks in a burst goes down from 70 ms to 21 ms and CPU to less than half.

### Fixed

- In English, Home, Users, Security and the examples of commands on connect showed some words in Spanish; `npm run i18n:check` now detects texts written outside `t()`.
- Imported history (new ids with old times) is ordered by time in search, pagination and playback.

## [0.6.0] - 2026-11-09

Plugins and DCC. Still a test release; see `docs/STATUS.md`.

### Added

- Third-party plugins in WebAssembly (`docs/PLUGINS.md`):
  - run with Wasmi: validated module, fuel (CPU) per event, 16 MB of memory, no access to files or the network (no WASI);
  - events (`load`, `connected`, `message`, `command`, `timer`) and declarative answers (`reply`, `tell`, `privmsg`, `notice`, `raw`) checked against the `send`, `raw` and `timer` permissions;
  - per-network settings validated with the manifest's schema, and persistent state;
  - they stop after five failures in a row and are not loaded if the file changes after installation;
  - **Plugins** page in the panel (install with a preview of the permissions, turn on on the server, uninstall) and a *Plugins* section in each network's Modules tab;
  - `*status modules/enable/disable/get/set` for plugins and `/msg *plugin` to talk to each one;
  - CLI: `xbncd plugin pack`, `check`, `install`, `remove` and `list`;
  - SDK for C (`sdk/c`, a header without libc, with the *dice* and *autoresponder* examples) and for Rust (`sdk/rust`, with the *stats* example).
- `dcc` module: sends (`DCC SEND`) and chats (`DCC CHAT`) through xBNC, in both directions, with passive offers and resume, a size limit and protection against connections to internal networks. Options `[dcc] public_ip` and `ports` in `xbnc.toml`.
- Metrics `xbnc_dcc_relays_total` and `xbnc_dcc_bytes_total`.
- API: `/plugins`, `/plugins/inspect`, `/plugins/{id}`, `/networks/{id}/plugins`. Schema migration 7. 140 tests.

### Fixed

- The language chosen on the sign-in page is saved to the account and is no longer overwritten by the previous language.
- Changing the default language in *Settings* applies at once to the panel (if the account uses the server's) and to open IRC connections.
- The IRC welcome (001-003) comes in the user's language.
- Changing the language or the modules no longer reconnects a network disconnected by hand.
- The audit log of changes in *Settings* does not show the webhook URL (it often carries a secret).

## [0.5.0] - 2026-11-09

Migration, search and monitoring. Still a test release; see `docs/STATUS.md`.

### Added

- Migration from another bouncer (`docs/DATA.md`):
  - it reads the block configuration file and, on the server, the data folder (SASL, NickServ, commands on connect, client certificates);
  - it creates users, networks, channels with their keys and the equivalent modules, with a preview and notices of what cannot be brought over;
  - salted SHA-256 passwords keep working and move to Argon2id on first sign-in; the others get a new password shown once;
  - panel: *Users > Import from another bouncer*; CLI: `xbncd import` (preview by default, `--apply`, `--logs`, `--utc-offset`).
- Import of text logs into the history (`xbncd import-logs`): messages, actions, notices and events; it can be repeated without duplicating.
- Full-text search in the history with SQLite FTS5: by words and prefixes, phrases in quotes, ignoring accents.
- API tokens (*Your account > API tokens*): read-only, read-write or metrics-only scope; expiry; last use and IP; revocation. The account's security actions still require a panel session.
- `GET /api/v1/metrics` in Prometheus format: networks by state, clients, traffic, messages, reconnections, logins, memory, database and active alerts.
- Configurable alerts in *Settings*: network without connection, failed logins, memory and database size. They are shown on Home, recorded in the audit log, sent over IRC to administrators and can be sent to a webhook compatible with Slack, Discord and Mattermost.
- Author credit (y2k) in the panel footer and in `xbncd --version`.
- API: `/tokens`, `/alerts`, `/metrics`, `/import/preview` and `/import/apply`. Schema migrations 5 and 6. 125 tests.

### Changed

- The automatic op challenge between bouncers uses HMAC-SHA256 and the `!XBNC-AUTOOP` tag.
- The documentation and the panel no longer refer to other products.

## [0.4.0] - 2026-11-08

Native modules and languages. Still a test release; see `docs/STATUS.md`.

### Added

- 13 modules configurable per network (`docs/MODULES.md`):
  - keep nick (keepnick);
  - automatic away with a summary on return (simple_away, autoaway, awaystore);
  - automatic op, halfop and voice by mask, with a key challenge (autoop);
  - rejoin after a kick (kickrejoin) and sticky channels (stickychan);
  - channel events in the history (buffextras), with `draft/event-playback` for modern clients;
  - log files in the classic IRC format, with retention (log);
  - replies to the device that asked (route_replies);
  - client certificate, generated or uploaded, with fingerprints and SASL EXTERNAL (cert);
  - aliases, watch words (watch), hide the MOTD (block_motd) and CTCP replies with flood protection (ctcpflood).
- The network's MOTD is stored and shown to clients when they connect and with `/motd`.
- Modules tab in each network and a Modules page with every module and a checkbox per network.
- Larger `*status`: `networks`, `modules`, `enable`, `disable`, `get`, `set`, `addline`, `dellines`, `setnick`, `language`, `clearhistory` and `seen`. New `*notes` and `*watch`.
- Notes per user (panel and `/msg *notes`) and last connection in Users.
- Complete panel in Spanish and English, with a selector on the sign-in page and in Your account; xBNC's IRC messages in each user's language; default language in `xbnc.toml` (`language`) and in Settings. `npm run i18n:check` in CI.
- Installer in Spanish or English depending on the system or with `--language`; it writes the default language.
- Redirection from `http://` to `https://` on the panel's TLS port.
- API: `/modules`, `/networks/{id}/modules`, `/networks/{id}/cert`, `/notes` and `/me/language`.
- Schema migration 4. 113 tests (14 new module tests against a simulated IRC server).

### Changed

- xBNC's CTCP replies (VERSION, PING, TIME) also work with no client connected.
- The panel's setup notice points to `sudo xbncd setup-code`.

## [0.3.0] - 2026-11-08

Installation without editing files and a better experience when connecting clients. Still a test release; see `docs/STATUS.md`.

### Added

- Automatic installer: by default it detects the machine's IP, generates a self-signed certificate valid for it, serves the panel at `https://IP:8443` with TLS, opens IRC on 6697 with SSL and on 6667 without SSL for the local network only, opens the ports in `ufw`/`firewalld` if they are active, waits for the service to start and shows the panel address and the setup code.
- Installer options: `--domain`, `--letsencrypt` (automatic certificate and renewal), `--ip`, `--behind-proxy`, `--web-port`, `--irc-port`, `--plain-port`, `--no-firewall`, `--reconfigure`.
- `lan_only` ports: they only accept clients from the local network (including Tailscale and local IPv6 addresses); the rest get a notice with the right port.
- Clear messages in the IRC client when using a TLS port without SSL, and TLS failures logged (certificate refused, TLS against a plain-text port).
- Commands on connect per network (like mIRC's *perform*), encrypted, written as in the client (`/msg`, `/mode`, `/quote`) with `$nick`. They are sent before joining the channels.
- Network wizard: login with X (Undernet), Q (QuakeNet, new in the list) and NickServ (OFTC) for networks without SASL.
- *Connect your IRC client* card with steps for HexChat, mIRC, WeeChat and irssi, a notice about self-signed certificates and the local port without SSL.
- Explanations in the panel for the usual connection errors.
- CLI: `xbncd info` (panel address and ports) and `xbncd setup-code`.
- Schema migration 3. 82 tests.

### Changed

- The setup code notice shows the real IP instead of `0.0.0.0`.
- After a “reconnecting too fast” notice from the network, xBNC waits at least 2 minutes before trying again.
- The example configuration serves the panel directly with TLS on 8443; the Nginx option is commented out.
- If the first administrator is created with the CLI, the panel stops asking for the code without restarting.

## [0.2.0] - 2026-11-08

Web panel, API and roles. Still a test release; see `docs/STATUS.md`.

### Added

- Web panel (React + TypeScript + Vite) built into the binary: live dashboard, network wizard with well-known networks, network management (connect, disconnect, reconnect, edit, delete), channels, connected devices, history with search across every conversation and TXT/JSON export, users, security, settings and account. Light and dark mode, responsive and keyboard navigable.
- Initial setup from the browser with a one-time code the service writes to its log.
- REST API `/api/v1` with an OpenAPI 3.1 specification and a WebSocket with statistics every 2 s.
- Five roles (superadministrator, administrator, moderator, user, limited user) with permissions and safeguards (nobody removes their own role; there is always a superadministrator).
- Network and device limits per user.
- History, limits and blocking settings editable live from the panel, stored in the database.
- Two-step verification (TOTP, RFC 6238) with a QR code for the panel.
- Panel sessions with an `HttpOnly`/`SameSite=Strict` cookie, CSRF, origin check, CSP and security headers, a per-IP request limit and brute-force blocking shared with IRC.
- Log of sign-in attempts (IRC and panel), IP unblocking and an audit log of every panel action.
- Metrics of memory, CPU, traffic, messages, reconnections and logins.
- Panel served with its own TLS or behind Nginx (`packaging/nginx/xbnc.conf`, `trust_proxy`).
- CLI: `xbncd user add --role`, `xbncd user role` and `xbncd user reset-2fa`.
- Schema migration 2 (applied automatically at start).
- 77 tests (8 new API tests against a real bouncer, and unit tests of roles, settings, TOTP, metrics, web and database).

### Changed

- Users created with `--admin` in v0.1.0 become superadministrators.
- The Docker image also builds the panel and exposes ports 8080/8443.

## [0.1.0] - 2026-11-08

First test release. Not fit for production; see `docs/STATUS.md`.

### Added

- Own IRC engine (`xbnc-proto`): parser and serialiser with IRCv3 tags, *casemapping*, ISUPPORT and CTCP.
- Connection to IRC networks with verified TLS 1.2/1.3, CAP 302, SASL PLAIN, alternative nick, reconnection with exponential *backoff* and *jitter*, *keepalive* and outgoing *flood* control.
- Multi-user, multi-network bouncer: channel state, topics and members; several simultaneous clients kept in sync.
- History in SQLite with per-device playback without duplicates and configurable retention.
- Capabilities for clients: `server-time`, `message-tags`, `echo-message`, `batch`, `sasl`, `draft/chathistory` (LATEST, BEFORE, AFTER, AROUND, BETWEEN, TARGETS).
- Login `user/network@device:password` and SASL PLAIN.
- `*status` commands: `help`, `status`, `channels`, `clients`, `jump`, `connect`, `disconnect`, `version`.
- Security: Argon2id, credential encryption with XChaCha20-Poly1305, per-IP brute-force blocking, credentials masked in logs, connection and queue limits.
- CLI: `init`, `check-config`, `user`, `network`, `channel`, `audit`, `backup`; reload with `SIGHUP`.
- systemd installer with *hardening*, Dockerfile, Docker Compose, test environment with Ergo and InspIRCd, uninstall, backup, restore and update scripts with *rollback*.
- 55 automatic tests and CI (rustfmt, clippy, MSRV 1.89, cargo-deny, shellcheck, Docker image).
