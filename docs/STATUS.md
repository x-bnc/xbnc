# Status of version 1.0.0

Version 1.0.0 is the first complete version of xBNC and the first **stable** one. Everything planned is implemented, covered by automatic tests and tested on real networks with the whole test plan. What is still left needs people outside the project: an external security audit.

This page lists what is included, how it was checked and the known limits. The test plan for real networks, which version 1.0.0 passed, is in [testenv/real/PLAN.md](../testenv/real/PLAN.md).

## What is included

| Area | Status | Detail |
| --- | --- | --- |
| IRC engine | Complete | Own parser with IRCv3 tags, verified TLS 1.2/1.3, CAP 302, SASL PLAIN and EXTERNAL, reconnection with growing delays, flood protection towards the server, casemapping and ISUPPORT. A line that is not UTF-8 is decoded as Windows-1252. |
| IRCv3 with the server | Complete | `server-time`, `message-tags`, `sasl`, `multi-prefix`, `extended-join`, `away-notify`, `account-notify`, `account-tag`, `chghost`, `userhost-in-names`, `invite-notify`, `setname` and `cap-notify`. |
| IRCv3 with the clients | Complete | The above plus `echo-message`, `batch`, `draft/chathistory`, `draft/event-playback`, `draft/read-marker` and `soju.im/bouncer-networks`. Each client only gets what it asked for; a classic client sees plain old IRC. |
| Bouncer | Complete | Several networks and devices, sync between devices, playback without duplicates per device, web chat in the panel, mentions and notifications (Telegram, Discord, e-mail, webhook and push). |
| Modules | 24 | The 14 of version 0.4 plus the 10 of 1.0: services login, invite and unban, character encoding, detached channels, hiding flooding channels, removing colours, getting op back, FiSH encryption, ignore and scheduled tasks. |
| Plugins | 18 official | Sandboxed WebAssembly with permissions (sending, presence, timer, web access only to the sites the administrator approves), CPU and memory limits, a catalogue signed with Ed25519, development mode and SDKs for C and Rust. |
| IRC commands | Complete | `/msg *status` with 64 commands, grouped and in bold, `help <command>` with an example, and other bouncers' command names as aliases: backup servers with automatic switching, trusted certificates, vhost, disabled and ordered channels, history, modules and administration (users, traffic, notices to everyone, message of the day, restart and shutdown). |
| Languages | Complete | Translation files (`locales/`): English (the base) and Spanish included; any other language is added with a file, without rebuilding, and checked with `xbncd locales check`. See [TRANSLATING.md](../TRANSLATING.md). |
| Panel and API | Complete | Panel in English, Spanish and the installed languages, light and dark themes, phone. REST API `/api/v1` with OpenAPI 3.1, scoped tokens, WebSocket and Prometheus metrics. |
| Users and access | Complete | Five roles, quotas of networks, devices and history, Argon2id, two-step verification, passkeys, OpenID Connect and LDAP. |
| Data | Complete | SQLite or PostgreSQL, active/standby high availability, backups from the panel, per-channel retention, full-text search (and by pieces of words), per-user time zone. |
| Migration | Complete | From another bouncer: users, networks, channels, equivalent modules and logs, with time zone and daylight saving time. |
| Installation | Complete | From GitHub with `git clone` and `scripts/install.sh`, without editing files: the installer builds the program on any Linux with systemd (`scripts/prereqs.sh` installs the build tools and Rust; it needs about 2 GB of memory counting swap and 3 GB of disk). The .zip of each release also brings the program ready-made for x86_64 (glibc 2.28 or later). IP or domain, own certificate or Let's Encrypt, optional Nginx, firewall and systemd. |
| Documentation | Complete | The wiki (a separate website, in English and Spanish), the documentation in `docs/` in English and in `docs/es/` in Spanish (generated from the wiki), `README.md`, `CHANGELOG.md`, `SECURITY.md`, `TRANSLATING.md` and the test plan. |
| Tests on real networks | Done | The whole test plan (`testenv/real/PLAN.md`) on Undernet, Libera.Chat, freenode, Rizon, EvilNet, IRCnet and others. Problems can be reported with the test report of the panel (*Your account → Test report*). |
| External audit | **Pending** | It has to be done by someone outside the project. |

## How it was checked

- **More than 270 automatic tests**, which pass with SQLite and with PostgreSQL: protocol, units, the API against a real bouncer, IRC end to end, every `*status` command, switching to a backup server, the trusted certificate (with a real TLS handshake), upgrading the database, modules and plugins against a simulated IRC server, panel and SSO. One more (`real_ircd`) only runs against a real IRC server; CI runs it against Ergo, InspIRCd, UnrealIRCd, Solanum and miniircd. The details are in [TESTING.md](TESTING.md).
- **Manual tests on a real IRC server (InspIRCd)** with xBNC and several clients: official plugins; the modules of 1.0 (character encoding, removing colours, FiSH in both directions, ignore, detached channels, getting op back, scheduled tasks); earlier modules (auto op, aliases, watch words, rejoin after a kick, away); IRCv3 towards the clients; the help and the `*status` commands in Spanish and in English, including `shutdown`.
- **Internal security review** of the new code of 1.0. It fixed three points before publishing:
  - on networks without services the password is never sent to whoever uses the nick “NickServ”;
  - the FiSH key exchange has a limit of requests and of stored keys;
  - decrypted text cannot break lines.
- **Hidden host on a real IRC server**: InspIRCd with cloaking and `/mode $nick +x` in the commands on connect; the other users of the channel see xBNC join only with the hidden host.
- **Continuous fuzzing** of the IRC parser, DCC, the importer, search, the manifests and the execution of plugins.
- **Panel** checked in Chrome in the light and dark themes and on the phone, with no errors in the console, an automatic check that no translation is missing, and `testenv/ui/language_check.py`, which checks in a real browser every way of changing the language.
- The .zip is unpacked, built and its tests pass from scratch.

## Known limits

**Tested on real networks** with the whole test plan before declaring it stable: the installer on x86_64 and on ARM (Raspberry Pi) with real systemd; TLS and SASL (PLAIN, and EXTERNAL with a client certificate generated in the panel); login with the services (X, NickServ, Q, AuthServ) and invite and unban; the modules and the official plugins, including the ones with web access; FiSH against HexChat in channels and in private (with DH1080 key exchange); DCC sending and receiving through xBNC; the importer. What came up was fixed before publishing it.

**Not done yet:**

- An external security audit: it has to be done by someone outside the project.
- A load test on dedicated hardware: the current measurement is 2,000 users on 2 vCPU with everything on the same machine (see [PERFORMANCE.md](PERFORMANCE.md)).

**Design limits:**

- **Undernet does not offer TLS.** The connection goes through port 6667 unencrypted, and the X password travels in clear text to the server, as with any client.
- **FiSH is old cryptography** (Blowfish, without integrity): it protects from the servers and from casual logs; it is no replacement for end-to-end encryption. Decrypted messages are stored in your xBNC history like the rest.
- Some clients do not send commands they do not know: to use an alias in them you have to type `/quote name`.
- DCC between a client that only uses IPv6 and one that only uses IPv4 is not translated. If the connection to the network is IPv6, offers announce the server's public IPv4.
- Search by pieces of words is only used when there are no whole-word matches.
- If the process stops abruptly, up to about 5 seconds of history may be repeated.
- In a language added as a file, the replies of the official plugins come in English (they speak English and Spanish).
- No `.deb` or `.rpm` packages and no ready-made Docker image are published: xBNC is installed from the source code with `git clone` and `scripts/install.sh` (or from the .zip of a release, which also brings the program ready-made for x86_64), or with `docker compose up -d --build`, which builds the image from the source (see [INSTALL.md](INSTALL.md)).

## Next step

1. Version 1.1.0: see the [roadmap](PROJECT.md#roadmap).
2. An external security audit.
