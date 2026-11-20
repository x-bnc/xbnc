<a id="threat-model"></a>

# Threat model

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

What xBNC protects, from whom and with which measure at each point. It is the guide for the external audit and for reviewing each change.

### What is protected

| Asset | Why it matters |
| --- | --- |
| Passwords of the xBNC accounts | They give access to the networks, the history and, for administrators, the whole server. |
| Network credentials (SASL, server password, channel keys, commands on connect, services passwords, FiSH keys, certificates) | They allow impersonating the user on the networks. |
| History and logs | Private conversations and channels: personal information. |
| Panel sessions and API tokens | As good as the password while valid. |
| The secret key (secret.key) | Decrypts every stored credential. |
| The availability of the service | If it goes down, users lose messages and presence. |
| The reputation of the server's IP | Every connection leaves from it. |

### Who may attack

| Actor | What they control | What they try |
| --- | --- | --- |
| Anyone on the internet | Connections to the IRC, panel and DCC ports | Guess passwords, exhaust resources, exploit the parser. |
| Someone on an IRC network | Messages, CTCP, DCC, channel names and nicks | Inject commands, make xBNC connect to internal networks, flood the history. |
| A malicious IRC server | Everything coming through a network's connection | The above, plus malformed answers, huge lines and disconnection loops. |
| An xBNC user | Their account, networks, modules and plugins | Read other people's data, escalate to administrator, abuse the server. |
| An administrator | Other people's users and networks | Escalate to superadministrator, change global settings. |
| The author of a plugin | The code a superadministrator installs | Escape the sandbox, read data, act without permission. |
| The author of a language file | A file an administrator copies to locales\_dir | Inject IRC commands or HTML through the texts (refused: they are validated and treated as plain text). |
| A malicious website | The browser of a signed-in user | CSRF, session theft, WebSocket hijacking. |
| Whoever gets the backups | The .tar.gz files | Credentials and conversations. |

Out of scope: someone who is already root on the machine, the server provider and failures of the operating system or of the IRC networks.

### IRC client connections (6697/6667)

| Threat | Measure |
| --- | --- |
| Password brute force | Per-IP blocking shared with the panel and tokens; Argon2id; the same cost for users that do not exist. |
| Exhausting connections or memory | Global connection limit, 16 unauthenticated per IP, clients per user, registration time and bounded queues. |
| Huge or binary lines | 8703-byte limit per line and lossy UTF-8. |
| Command injection with CR/LF | Any CR or LF cuts the line; texts of modules, plugins and language files are validated without control characters. |
| Plain text on the internet | Ports without TLS only accept the local network or require insecure = true. |
| Credentials in the logs | Masking of PASS, AUTHENTICATE, OPER and messages to services at every level. |

### Connections to the IRC networks

| Threat | Measure |
| --- | --- |
| Server impersonation | TLS verified with the system store, with no option to turn it off; a trusted certificate only accepts its exact SHA-256 fingerprint. |
| Malformed answers | Own parser without unsafe, with tests and fuzzing. |
| History flooding | Configurable retention, optional events, bounded queues; the log module drops lines if the disk cannot keep up; hide flooding channels. |
| Disconnection loops | Growing delays with random variation and a minimum wait after throttling notices. |
| Being thrown out for flooding | Rate limiting on the output. |
| Theft of the services password | On networks without NickServ it is never sent in automatic mode. |

### DCC

| Threat | Measure |
| --- | --- |
| Using the bouncer to reach internal networks (SSRF) | Offers to internal addresses or ports below 1024 refused unless the user allows them. |
| Making xBNC connect to a third party | For your own sends it only connects back to your device's IP. |
| Ports open indefinitely | One port per offer, a single connection, a timeout, closing when idle and a size limit. |
| Hijacking the transfer | When receiving, the port only accepts the client's IP. |

### Web panel and API

| Threat | Measure |
| --- | --- |
| CSRF | CSRF token in a header, SameSite=Strict cookie and Origin check. |
| XSS | React escapes all content; CSP without external or inline scripts; IRC texts are never interpreted as HTML; translations are plain text. |
| Session theft | HttpOnly and Secure cookie; only the token's SHA-256 in the database; sliding expiry; changing the password closes the other sessions. |
| WebSocket hijacking | Origin check, the session revalidated every 30 s and no API tokens. |
| Privilege escalation | Permissions checked on the server in every route; nobody changes their own role; there is always a superadministrator. |
| Seeing other users' data | Other people's networks answer 404, not 403. |
| API abuse and slowloris | 20 requests/s per IP, bounded bodies, 64 connections per IP and 4,096 in total, closing after 2 minutes without traffic, 10 s for the TLS handshake. |
| Leaked tokens | Scope, expiry, last use and IP visible, immediate revocation, only their SHA-256 stored. |
| Stolen first access | One-time code, only in the service log. |
| Webhook to internal sites | Only a superadministrator configures it; no redirects, no credentials in the URL, 10 s at most. |

### Plugins

| Threat | Measure |
| --- | --- |
| Escaping the sandbox | Wasmi interpreter without JIT, module validated when loaded, no WASI nor imports other than xbnc's, advanced WebAssembly features turned off. |
| Blocking or crashing the server | Fuel per event, limited memory, execution that never grows the native stack, calls in separate threads, stopping after five failures. |
| Acting without permission | Declarative actions checked against the permissions; dangerous commands always forbidden; maximum messages per event; web access only to approved domains and with limits. |
| Reading other people's data | Each instance only receives the events of its network; state and store separated per network. |
| File changed after installation | Its SHA-256 is stored and it is not loaded if it does not match; files 0600. |
| Malicious plugin installed | Only a superadministrator installs, seeing permissions and fingerprint; recorded in the audit log; signed remote catalogue. |

### Import from another bouncer

| Threat | Measure |
| --- | --- |
| File crafted to create a superadministrator | The import never grants that role. |
| Weak inherited passwords | Salted SHA-256 compared in constant time and replaced by Argon2id; formats that cannot be checked get a random password. |
| Abusive paths or sizes | File up to 4 MB, limited nesting, validated names; logs are only read from the folders given. |
| Leaks in the preview | The preview includes no secrets. |

### Updates from the panel

| Threat | Measure |
| --- | --- |
| Fake package (compromised download server, DNS or network) | Ed25519 signature over the package name and its SHA-256; the public key is only in `xbnc.toml`, owned by root. |
| Going back to a version with known flaws | Equal or older versions than the installed one are refused. |
| A compromised `xbnc` process trying to install something as root | The helper (`xbnc-update.service`) trusts nothing in the service's folder: it copies the package to `/run` (0700), checks the signature and the version on that copy, never follows symbolic links, validates the file name and uses each request only once. |
| Fake update request | Only superadministrators, from a panel session (not with API tokens) and with CSRF; recorded in the audit log. |
| Endless downloads or redirects to plain text | 200 MB and 10 minutes at most; `https://` only, 5 redirects at most. |

### Data at rest and backups

| Threat | Measure |
| --- | --- |
| Theft of the database | Network credentials encrypted with XChaCha20-Poly1305; the key outside the database, with 0600 permissions checked at start. |
| Theft of a backup | 0600 file; it includes the key, so it must be stored encrypted. |
| Access from other users of the system | Service run as the unprivileged `xbnc` user, 0700 folders and systemd hardening. |

### Accepted risks

- The history is stored unencrypted in the database: encrypting it would prevent search and playback. It is protected by the system's permissions.
- IRC clients have no second factor: the bouncer password is their only protection (hence per-IP blocking and Argon2id).
- A superadministrator can install a plugin with permission to send messages on behalf of whoever turns it on: it is a decision of trust that the panel shows before installing.
- Every connection to the networks leaves from the server's IP.
- FiSH is old cryptography (Blowfish, without integrity); decrypted messages are stored in the history like the rest.

### Review

Each version reviews these tables: a new feature that opens a surface not listed here is added to them. The tests behind each measure are in `crates/xbncd/tests/` and in the unit tests of each module; fuzzing covers the IRC parser, DCC, the imported configuration file, search, and the manifests and execution of plugins (see [Test environment](TESTING.md#test-environment)).

**Internal review of version 0.7.** The whole code was reviewed with these tables as a guide (it does not replace the external audit):

- **SQL queries:** all with bound parameters; the parts built with `format!` are only fixed column lists and clauses.
- **API authorisation:** every route checks the session or token and the permission; network routes check the owner. Fixed: `PATCH /users/{id}` answered 404 or 403 to accounts without permission depending on whether the user existed.
- **External processes:** the only one is `dpkg` or `rpm` in the update helper, with fixed arguments and the package already checked.
- **File paths:** logs, plugins and updates build paths only from validated names; the update helper never follows symbolic links.
- **Denial of service:** added the limit of unauthenticated IRC connections per IP, the panel's connection limits and idle time, and the limit of simultaneous Argon2id calculations.
- **Plugins:** the interpreter moved to the loop execution mode after fuzzing found that the chained-call mode could exhaust the native stack in unoptimised builds.
- **`unsafe` code:** only one call to `mallopt` at start (memory allocator setting).
- **Dependencies:** `cargo-deny` in CI (licences and security advisories).

**Internal review of version 1.0.** The new code of 1.0 was reviewed and three points were fixed before publishing: on networks without services the password is never sent to whoever uses the nick “NickServ”; the FiSH key exchange has a limit of requests and stored keys; decrypted text cannot break lines. Language files are validated (placeholders, tags, no control characters or line breaks) and a text that fails is shown in English.
