# Testing

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

- [Test environment](#test-environment)
- [Test report](#test-report)
- [Testing the beta](#beta)

<a id="test-environment"></a>

## Test environment

### With Docker

`testenv/` brings up xBNC with two different IRC servers (Ergo and InspIRCd) on an isolated network, all in plain text on purpose:

```sh
cd testenv
docker compose up -d --build
./setup.sh
# IRC: localhost:6667, password test/ergo@laptop:testpassword1
# Panel: http://localhost:8080 (user test, password testpassword1)
```

Do not publish those ports on a public interface.

### Automatic tests

```sh
cargo test --workspace                                   # every test
XBNC_TEST_POSTGRES=postgres://postgres@127.0.0.1/postgres cargo test --workspace   # also with PostgreSQL
cargo clippy --workspace --all-targets -- -D warnings
cd web && npm run build && npm run i18n:check            # panel and translations
xbncd locales check locales/*.json                       # language files
python3 ../xbnc-wiki/build.py --app .                   # the wiki (a separate project)
```

They cover the protocol, the units, the API against a real bouncer, IRC end to end, every `*status` command, the switch to a backup server, the trusted certificate with a real TLS handshake, a language added as a file, modules and plugins against a simulated IRC server, the panel, SSO and real IRC servers in CI (Ergo, InspIRCd, UnrealIRCd, Solanum and miniircd). *Fuzzing* (`fuzz/`) runs every week. `testenv/ui/language_check.py` checks in a real browser that every way of changing the language works.

### Tests on real networks

The test plan per network, module and plugin is in `testenv/real/PLAN.md`. See [Testing the beta](#beta).

### The test sets

| Set | Where | What it covers |
| --- | --- | --- |
| Protocol (16) | `crates/xbnc-proto` | Parser and serialiser, tag escaping, prefixes, casemapping, ISUPPORT, CTCP. |
| Units of the service (159) | `crates/xbncd/src` | Configuration, Argon2id, encryption and key permissions, database and migrations (SQLite and PostgreSQL, and upgrading an old schema), history and search, cursors, sessions, backups, login, brute-force limiter, log masking, codec, CAP, roles, settings, TOTP (RFC 6238 vectors), metrics, tokens, cookies, origin, IP behind a proxy, OpenAPI coverage, HTTP→HTTPS redirect, module catalogue and validation, globs, mentions, aliases, logs, client and trusted certificates (with a real TLS handshake), FiSH, time zones, languages (every key used exists, Spanish complete, files checked and loaded), plugins (permissions, limits, fuzzing regressions), DCC, HTTP client, release signatures, the update helper and connection limits, which names a TLS certificate is valid for, and the panel address shown after installing, and the texts of the installer (complete in English and Spanish, checked in any language file, and every key the scripts use exists). |
| API (13) | `tests/api.rs` | First run with a one-time code, login, sessions, CSRF, origin, headers, brute force, networks, channels, history, export, isolation between users, roles, live settings, 2FA, panel, OpenAPI, modules, certificates, notes, language, tokens, metrics, alerts, import and plugins. |
| End to end (16) | `tests/e2e.rs` | The whole bouncer against a simulated IRC server (scenarios below). |
| Modules (18 + 12) | `tests/modules.rs`, `tests/modules_v1.rs` | Each module against the simulated server: away and summary, keep nick, replies to the device that asked, events, CTCP, sticky channels, aliases, auto op, rejoin after a kick, watch words, MOTD and logs; and the modules of 1.0: IRCv3 extras only for clients that asked, services login (X with the hidden host and Login on Connect), invite, ignore and remove colours, FiSH in both directions and DH1080, detached channels, flooding channels, autocycle, Latin-1 and scheduled tasks. |
| `*status` commands (12) | `tests/status_commands.rs` | Grouped help, other bouncers' command names, backup servers (added, jumped to, promoted and automatic switch), trusted fingerprints, bind hosts, disabled and ordered channels, buffers, MOTD and broadcast, permissions, networks from IRC and a language added as a file. |
| Plugins (2 + 5 + 7) | `tests/plugins.rs`, `tests/plugins_v1.rs`, `tests/official_plugins.rs` | Plugins answering on the network and to the user, and each official plugin: seen, trivia, greet, factoids, karma, quotes, games, badwords, web access only to approved sites, reminders, tell, polls, channel stats, anti-flood, dice and autoresponder. |
| Shared port (2) | `tests/shared_port.rs` | The panel and IRC on one TLS port: a browser gets the panel, an IRC client the bouncer, plain http:// is sent to https:// and an IRC client without SSL is told to turn it on. |
| Panel (3) | `tests/panel.rs` | Chat in the panel, mentions and channel history, plugin development mode and the one-click catalogue. |
| SSO (3) | `tests/sso.rs` | Passkeys, OpenID Connect and LDAP. |
| Command line (2) | `tests/cli.rs` | `xbncd info` run as a real process: the panel at the address given at installation, and no error when its output goes into `head`. |
| PostgreSQL (1) | `tests/postgres.rs` | Moving an SQLite database to PostgreSQL (with `XBNC_TEST_POSTGRES`). |
| Real IRC server (1) | `tests/real_ircd.rs` | A real xBNC against a real IRC server (see below). |
| Rust SDK (3) | `sdk/rust` | Events and answers of the SDK. |

### End-to-end scenarios

1. On connecting, the client gets the welcome, the network's ISUPPORT, its channels, the topic and the list of users.
2. A message received with the client closed is played back on reconnecting, with its time; your own messages are not played back to the same device and a third connection gets no duplicates.
3. Two devices connected at once stay in sync; a new device gets the recent history.
4. `CHATHISTORY` LATEST, AFTER and errors, `batch`, the server's `server-time` kept, `msgid` and `echo-message`; capabilities that are not implemented are not announced.
5. Login with SASL PLAIN.
6. A wrong password is refused and the IP is blocked after three failures, even with the right password.
7. Unknown network: the error lists the networks available.
8. Server outage: the bouncer says so, reconnects with growing delays and joins the channels again.
9. `*status`, and `PART` and `JOIN` with a key saved (the key is stored encrypted).
10. Commands on connect are sent before joining channels.
11. Read markers are shared between devices.
12. `soju.im/bouncer-networks` for modern clients.
13. With `/mode $nick +x` in the commands on connect, no JOIN reaches the server (saved channels, those in the commands, a client's own) until the server hides the host (396).
14. `/msg *status joinnow` joins the held channels.
15. A wrong password gets the right format, and the address is blocked after the failures with the minutes left and where to unblock it.
16. The example password (“YOUR\_PASSWORD”) is explained and does not block the address.

### Real IRC servers

`tests/real_ircd.rs` connects a real xBNC to a real IRC server (variable `XBNC_IRCD=host:port`) and checks the list of users, messages in both directions, sync between two devices, playback without duplicates with the client closed, a nick change, leaving a channel and staying connected to the network:

```sh
XBNC_IRCD=127.0.0.1:6667 cargo test -p xbncd --test real_ircd -- --ignored
```

CI runs it against Ergo, InspIRCd, UnrealIRCd, Solanum and miniircd (`testenv/ci/start-ircd.sh` builds or installs each one with a minimal configuration).

### Fuzzing

`fuzz/` has six libFuzzer targets:

| Target | What it receives |
| --- | --- |
| `irc_message` | Arbitrary IRC lines: parser, serialiser and the round trip. |
| `dcc` | DCC offers. |
| `import_config` | Configuration files of another bouncer. |
| `search_query` | Search queries against the full-text index. |
| `plugin_manifest` | WebAssembly files with arbitrary manifests. |
| `plugin_runtime` | Arbitrary WebAssembly modules loaded and run as plugins. |

```sh
cargo install cargo-fuzz          # needs Rust nightly
cd fuzz && mkdir -p corpus/irc_message && cp seeds/irc_message/* corpus/irc_message/
cargo +nightly fuzz run irc_message corpus/irc_message -- -max_total_time=600
```

CI (`.github/workflows/fuzz.yml`) runs each target for 20 minutes every week, keeps the corpus between runs and first checks that the failures already found (`fuzz/regressions/`) are still fixed. The failures found so far, all fixed and with their test:

1. The parser accepted CR, LF or NUL inside a line.
2. The parser accepted commands that are not alphanumeric (`@ @`).
3. DCC offers accepted control characters.
4. Embedding a manifest overflowed with sections of impossible size.
5. A plugin with a long loop exhausted the process's native stack in builds where the compiler does not turn the interpreter's calls into jumps: the interpreter now uses the loop execution mode, which never grows the stack.

### Load tests

`cargo run --release --example loadtest -- --users 2000 --rate 1 --seconds 60`. Results and method in [Performance](PERFORMANCE.md#performance).

### The panel in a browser

The panel has been gone through in Chrome with Playwright against a real bouncer with a real IRC client connected: first run, network wizard, channels, history and search, users, security, settings, 2FA, modules, plugins (installation with the permissions preview, per-network settings), import from another bouncer, alerts, updates, backups, chat, the sign-in page, the light and dark themes and the phone, in English and in Spanish, with no errors in the console apart from the expected 401 before signing in. `testenv/ui/language_check.py URL USER PASSWORD` repeats in a real browser every way of changing the language (sign-in page, *Your account*, *Settings*, a language added as a file) and checks that no text is left in another language.

### Manual checklist

Use it with `testenv/` or with a real installation:

- The service starts and stops cleanly (`systemctl start` and `stop xbnc`; the log shows *xBNC stopped*).
- An administrator and several users are created with the command line.
- A user with two networks is connected to both at once.
- A classic client (HexChat, WeeChat, irssi or mIRC) connects with `user/network@device:password`.
- With the client closed, someone else writes in the channel and in private; on reconnecting those messages arrive exactly once.
- Two clients of the same user see what the other one writes.
- `docker compose restart ergo` (or cutting the network): xBNC says so, reconnects and joins the channels again.
- `/msg *status status` shows the right state.
- Five wrong passwords block the IP for 15 minutes.
- `scripts/backup.sh` and `scripts/restore.sh` bring back users, networks and history.
- `journalctl -u xbnc` contains no passwords after logging in, SASL and `/msg NickServ IDENTIFY`.
- From the panel: create a user, add a network with the wizard, join a channel and see the history.
- An ordinary user does not see Users, Security or Settings, nor other people's networks.
- Turn on two-step verification and sign in with the code.
- Change the retention in Settings and check in System that it applies without restarting.
- Memory use with 1 user and 2 networks (`systemctl status xbnc`): note it down to compare in future versions.

<a id="test-report"></a>

## Test report

In *Your account → Test report → Download report* (or `GET /api/v1/diagnostics`) you download a JSON file with what is needed to diagnose a problem with a network, **without passwords, messages, other people's nicks or the IPs of your devices**.

| Field | Contents |
| --- | --- |
| `report`, `format` | xbnc-diagnostics, format version 1. |
| `version`, `brand` | xBNC version and “xBNC v1.0.0 - https://xbnc.org”. |
| `os`, `arch`, `database` | System, architecture and database (sqlite or postgresql). |
| `started_at`, `generated_at` | When xBNC started and when the report was generated. |
| `timezone`, `language` | Your time zone and language. |
| `counters` | Server counters (traffic, messages, reconnections, logins). |
| `networks[]` | Per network: name, host, port, tls, enabled, whether it has SASL or a server password, saved\_channels, active modules and plugins, and live. |
| `networks[].live` | state, server, network\_name, connected\_since, last\_error, caps\_offered, caps\_acked, isupport, services\_logged\_in, host\_hidden, channels\_joined and clients (device and capabilities of each one). |

<a id="beta"></a>

## Testing the beta

Version 1.0.0 is being tested on real networks (Undernet, Libera.Chat, freenode, Rizon, EvilNet, IRCnet and others) before it is declared stable.

1. Install it and add your networks.
2. Follow the test plan in `testenv/real/PLAN.md`: one section per network, module and plugin.
3. If something fails, download the [test report](#test-report) from *Your account*.
4. Send it with what you did, what you expected, what happened, the network, the client, the approximate time, the version (`/msg *status version`) and the warnings of the log (`sudo journalctl -u xbnc --since "1 hour ago" | grep -E "WARN|ERROR"`; review it before sending it).

If the project is on GitHub, use the “Test report” issue template. Security flaws are not made public: follow `SECURITY.md`.

### Known limits

- Undernet and QuakeNet do not offer TLS: the connection to those networks is unencrypted.
- FiSH is old cryptography (Blowfish, without integrity): it protects from the network's servers, it is no replacement for end-to-end encryption. Decrypted messages are stored in your history like the rest.
- Some clients do not send commands they do not know: aliases need `/quote`.
- DCC between an IPv6-only client and an IPv4-only one is not translated.
- Search by pieces of words is only used when there are no whole-word matches.
- If the process stops abruptly, a few seconds of history may be repeated.
- In a language added as a file, the replies of the official plugins come in English (they speak English and Spanish).
- In this beta installation is from the source code; no .deb or .rpm packages are published.
- Still to be checked on real networks: login with X, NickServ, Q and AuthServ; invite and unban; plugins with web access; FiSH with real clients; installer with real systemd and Let's Encrypt; DCC between networks; SASL EXTERNAL; importer with real installations.
