<a id="architecture"></a>

# Architecture

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

xBNC is a single program (`xbncd`) with the panel built in. The code is split into `crates/xbnc-proto` (IRC protocol without I/O: parser and serialiser with IRCv3 tags, prefixes, casemapping, ISUPPORT and CTCP) and `crates/xbncd` (service, CLI, API and panel), plus the plugin SDKs in `sdk/`, the panel (React and TypeScript) in `web/` and the translations in `locales/`.

### How it works inside

- Each network of each user is an independent task that exclusively owns its connection to the server, the state of the network and the list of clients. Nobody else touches that state: there are no locks on the path of the messages.
- Each client is a reading task and a writing task, with a bounded queue. If a client does not read, it is disconnected instead of piling up memory; when it comes back it gets what was pending.
- The panel, the API and the command line never touch the IRC connections: they write to the database and send orders to the network tasks (state, control, join, part, reload, disconnect a client, alert, broadcast).
- Every database access goes through a separate thread pool. The messages of every network go through a single writer thread that stores everything waiting (up to 512) in one transaction, keeping each network's order.
- Modules are code of the program itself, called at specific points (server line, client command, timer, connection, client joining or leaving). Plugins receive their events from the network task and run in the blocking thread pool.
- When a message arrives: it is parsed, the state is updated (nick, channels, members, modes, topic), it is stored with the server's time and an identifier, and it is sent to each client adapted to its capabilities.
- Texts are looked up in the loaded languages (built in, plus the files of `locales_dir`) by key, with English as the fallback.

### Modules of the code

| Module (crates/xbncd/src) | What it does |
| --- | --- |
| `main.rs` | The `xbncd` command: running the service and the command line (users, networks, backups, import, languages, releases, updates). |
| `config` | Reading and validating `xbnc.toml`. |
| `db`, `sql` | Storage: versioned migrations, users, networks, channels, messages, cursors, audit log and backups; `sql` gives SQLite (rusqlite) and PostgreSQL the same interface. |
| `crypto` | Argon2id, XChaCha20-Poly1305 encryption of the stored credentials and a `Secret` type that is never printed. |
| `auth` | The login format, password checks, the brute-force limiter and the masking of lines in the logs. |
| `util` | IRC line codec (length limit, lossy UTF-8, protection against CR/LF injection) and time helpers. |
| `network/` | The task of each network of each user: connection to the IRC server, state, history and attached clients. `mods.rs` and `extra.rs` hold the behaviour of the modules, `status.rs` and `status_more.rs` the `*status`, `*notes` and `*watch` commands, `plugins.rs` delivers events to plugins, `dcc.rs` decides which DCC offers go through xBNC and `readmark.rs` keeps the read markers. |
| `downstream`, `bouncer`, `events` | Client connections: registration, CAP, SASL, PASS and relaying to the network task; `soju.im/bouncer-networks`; notices to clients when a network changes. |
| `modules` | Catalogue of the modules, schema and validation of their settings and the typed configuration per network. |
| `perform` | Turns client-style commands (`/msg`, `/mode`…) into IRC, for the commands on connect, aliases and scheduled tasks. |
| `logs` | Log files, written in a separate thread. |
| `certs` | Client certificates (generation, validation and fingerprints) and trusted server certificates. |
| `fish` | FiSH encryption: Blowfish ECB and CBC and the DH1080 key exchange. |
| `i18n` | Languages: the built-in English and Spanish and the files of `locales_dir`. |
| `netinfo` | Local network addresses and the address shown for `0.0.0.0`. |
| `server`, `ha` | Shared context, the registry of network tasks, TLS listeners, maintenance and reloading; active/standby high availability with PostgreSQL. |
| `rbac`, `settings` | The five roles and their permissions; history settings and limits editable live (database over `xbnc.toml`). |
| `totp`, `webauthn`, `sso` | Two-step verification (RFC 6238), passkeys, OpenID Connect and LDAP. |
| `tz` | Time zones with daylight saving time, with the database built in. |
| `metrics`, `alerts` | Traffic and message counters, process memory and CPU; alert rules checked every 30 s, with notices in the panel, over IRC and by webhook. |
| `notify`, `push` | Notifications by Telegram, Discord, e-mail, your own web address and Web Push. |
| `plugins`, `plugin_dev`, `catalog` | WebAssembly plugins: manifest, validation, the Wasmi engine with limits, instances and actions; development mode; the built-in catalogue and the signed remote one. |
| `dcc` | Reading and rewriting DCC offers and the one-use relays. |
| `http` | A minimal HTTP/1.1 client (verified TLS, bounded size and time) for webhooks, notifications, plugins with web access and update checks. |
| `updates`, `release` | New version notices, downloading and checking packages, the `xbncd apply-update` helper and Ed25519 signatures. |
| `backup` | Backups from the panel and restoring them. |
| `import` | Migration from another bouncer: reading its configuration, module data, plan, applying it in one transaction and importing logs. |
| `web/` | HTTP server (axum): the `/api/v1` API, WebSocket, sessions, CSRF, headers and the panel files built into the binary. |

The panel (`web/`) is a React and TypeScript application built with Vite. `web/dist` is built into the binary, so xBNC stays a single file. The official plugins are built into the binary too (`crates/xbncd/catalog/`).

### State machines

They are kept separate:

1. **Connection to the network**: `Disconnected → Registering → Connected`. While registering, xBNC negotiates the IRCv3 capabilities, SASL and the nick (with an alternative if it is taken). Times: 30 s to connect, 60 s to register, a PING after 90 s of silence and disconnection at 180 s. Reconnection with growing delays from 5 s to 5 minutes with ±20 % random variation, and at least 2 minutes after a throttling notice. If a server fails and the network has backup servers, the next one is tried after 1 second.
2. **Client session**: registration (`CAP`, `PASS` or SASL, `NICK`, `USER`) with a time limit, and then a session attached to a network.
3. **User and bouncer**: which network tasks exist and how many clients each user has.

### The path of a message

1. The network task reads a line from the server and parses it.
2. It updates the state (nick, channels, members, modes, topic).
3. If it is a PRIVMSG, NOTICE or ACTION from someone (or an event, with the [Events](MODULES.md#mod-events) module), it stores it in `messages` with the server's time (`server-time`) or the local one, and gives it the identifier `msgid = x<id>`.
4. It sends it to each attached client adapted to its capabilities (removing the tags the client did not ask for) and moves each client's cursor forward.

What a client sends goes to the server, is stored as outgoing and is relayed to the user's other clients (and to the sender itself if it asked for `echo-message`).

### History without duplicates

- Each device (`@device` in the login, `default` if it is not given) has a cursor per network in `client_cursors`: the identifier of the last message delivered.
- When it connects, it gets the messages after its cursor (up to `playback_max`, 1,000 by default). A new device gets the most recent `new_client_playback` (100 by default).
- Cursors are saved when the device disconnects and every 5 seconds; they never go back.
- Clients with `draft/chathistory` get no automatic playback: they ask for what they need with `CHATHISTORY`. Messages already read on another device (`draft/read-marker`) are not played back again.

### Database schema

| Table | Contents | Since migration |
| --- | --- | --- |
| `users` | User, Argon2id hash, role, limits and quotas, encrypted TOTP secret, language, time zone, bind host, last login and last connection. | 1 (2, 4, 8, 9, 10) |
| `networks` | Server, port, TLS, nick, SASL, commands on connect, client certificate, backup servers and trusted certificates; passwords and keys encrypted. | 1 (3, 4, 10) |
| `channels` | Saved channels per network, with an encrypted key, retention, disabled and position. | 1 (8, 10) |
| `messages` | History: network, conversation, time, sender, command, text, outgoing and mention. | 1 (8) |
| `client_cursors` | Last message delivered per network and device. | 1 |
| `audit_log` | Administrative actions. | 1 |
| `web_sessions` | Panel sessions (token hash, CSRF, IP, browser, expiry). | 2 |
| `settings` | Settings changed from the panel. | 2 |
| `login_events` | Login attempts over IRC and in the panel (10,000 at most). | 2 |
| `module_config` | Modules per network: enabled and validated JSON settings. | 4 |
| `notes` | Each user's notes. | 4 |
| `messages_fts` | Full-text index of `messages.text`, kept in sync by triggers (SQLite; PostgreSQL uses its own text search). | 5 |
| `api_tokens` | API tokens: SHA-256 hash, prefix, scope, expiry, last use and IP. | 6 |
| `plugins` | Installed plugins: manifest, SHA-256, size, enabled on the server, approved web domains and the module itself. | 7 (8) |
| `plugin_config` | Plugins per network: enabled, settings and saved state. | 7 |
| `plugin_kv` | Each plugin's key-value store per network. | 8 |
| `notify_settings` | Notification settings of each user. | 8 |
| `push_subscriptions` | Web Push subscriptions of each user's browsers. | 8 |
| `read_markers` | Last message read in each conversation. | 8 |
| `passkeys` | Passkeys: credential, public key, counter, name and last use. | 8 |
| `user_identities` | Outside identities (OpenID Connect, LDAP) linked to local users. | 8 |

PostgreSQL has the same schema; it is supported from version 8 of the schema (`PG_MIGRATIONS`), and every new migration is added to both lists. Channel events (JOIN, PART…) are stored in `messages` with their command. Passwords imported from another bouncer with SHA-256 are kept as `legacy-sha256$<hash>$<salt>` until the first correct login, when they are replaced by Argon2id. Migrations are applied at start inside a transaction, and a binary refuses to open a newer schema than it knows.
