<a id="security"></a>

# Users, roles and security

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

### Roles

| Role | Who it is for |
| --- | --- |
| `superadmin` | Everything, including system settings, plugins, backups, updates, import and managing other administrators. |
| `admin` | Manages users (except administrators and superadministrators), every network and security; sees the system. |
| `moderator` | Sees users and the audit log, and can suspend and reactivate normal and limited users. |
| `user` | Manages their own networks, channels and history. It is the default role. |
| `limited` | Uses networks an administrator prepares for them; manages channels and history, but does not add, edit or delete networks. |

### What each role can do

| Permission | superadmin | admin | moderator | user | limited |
| --- | --- | --- | --- | --- | --- |
| Edit system settings, install plugins, backups, import, update | yes | — | — | — | — |
| See the system, manage users, security and every network | yes | yes | — | — | — |
| See users and the audit log, suspend users | yes | yes | yes | — | — |
| Manage their own networks | yes | yes | yes | yes | — |
| Use their networks, channels, history, chat, modules and plugins | yes | yes | yes | yes | yes |

An administrator only manages roles below their own; nobody can change their own role or promote themselves, and there is always at least one superadministrator. Users do not see other people's networks (the API answers 404).

### Accounts

- User names: 1 to 32 characters `A-Z a-z 0-9 . _ -`, not case-sensitive. Passwords: at least 10 characters.
- Per-user quotas: networks (`max_networks`), devices (`max_clients`) and history space in MB (`max_history_mb`). Empty: the server's general value.
- Suspending an account closes its access and disconnects its networks. Deleting it also removes its networks and history.
- Changing or resetting the password closes the account's other panel sessions.

### Ways to sign in to the panel

- **User and password**, with optional **two-step verification** (TOTP, RFC 6238, with a QR code) using Google Authenticator, Aegis, 1Password or another compatible app. The secret is stored encrypted; a wrong code counts as a failed attempt. IRC clients still sign in with the password only: the protocol has no second step.
- **Passkeys** (WebAuthn): sign in with the device's fingerprint, face or PIN. They are added and removed in *Your account*.
- **OpenID Connect** (Google, Microsoft, Keycloak, Authentik…) and **LDAP**, if the administrator configures them in `xbnc.toml` (see [Configuration](CONFIGURATION.md#configuration)). An external account is linked and unlinked in *Your account*.

### Security measures

| Area | Measure |
| --- | --- |
| Passwords | Argon2id (m = 19 MiB, t = 2, p = 1). A user that does not exist costs the same to check, so as not to reveal which accounts exist. At most one computation per CPU at a time, so a flood of attempts cannot exhaust the memory. |
| Stored credentials | SASL and server passwords, channel keys, commands on connect, services passwords, FiSH keys, TOTP secrets and certificate keys, encrypted with XChaCha20-Poly1305. The key lives outside the database (secret.key, permissions 0600 checked at start). |
| TLS | rustls, TLS 1.2 and 1.3 only. Towards the networks the certificate is verified with the system store, with no option to turn it off. A trusted certificate (addfingerprint) accepts only that exact SHA-256 fingerprint, and the signatures of the TLS handshake are still checked. |
| Plain text | Forbidden on public addresses unless insecure = true is set explicitly. The default port without TLS only accepts the local network. |
| Brute force | IP blocked after 5 failures in 10 minutes for 15 minutes (configurable), shared between IRC (PASS and SASL), the panel and API tokens. At most 16 unauthenticated IRC connections per IP. |
| Logs | Passwords, AUTHENTICATE, OPER and messages to services (NickServ IDENTIFY, X LOGIN, Q AUTH…) are never logged, at any level. |
| Input | Lines limited to 8703 bytes; any CR or LF inside a line cuts it, to prevent command injection; nicks, names and texts of modules and plugins validated. |
| Resources | Limits on total connections and on clients and networks per user; bounded queues per client and towards the server; maximum registration time. |
| SQL | Every query uses bound parameters. |
| Privileges | The service runs as the unprivileged xbnc user, with systemd isolation. In Docker: unprivileged user, read-only, no capabilities. |
| Panel: sessions | A random 256-bit token in an HttpOnly, SameSite=Strict and Secure cookie; the database only holds its SHA-256. Sliding expiry (12 h by default). |
| Panel: CSRF and origin | CSRF token required on every request that changes something; requests and WebSockets with another site's Origin are refused. |
| Panel: headers | Strict CSP without external or inline scripts, X-Frame-Options: DENY, nosniff, Referrer-Policy: no-referrer, HSTS with TLS and Cache-Control: no-store on the API. |
| Panel: abuse | 20 requests per second per IP, bodies up to 256 KB (4 MB for plugins), 64 connections per IP and 4,096 in total, closing after 2 minutes without traffic and 10 s for the TLS handshake. With trust\_proxy the real IP behind Nginx is used. |
| First access | The first account can only be created with a one-time code that the service writes to its log and to a private file (0600) in its data folder, which sudo xbncd setup-code shows. |
| Secrets in the API | Passwords are never returned; only whether they exist. |
| API tokens | 256 bits with the xbnc\_ prefix; only their SHA-256 is stored. Scope, expiry, last use and IP visible, immediate revocation. |
| Modules | Built into the program: no external code is loaded. Auto op: \*!\*@\* is refused; with a key, a random challenge answered with HMAC-SHA256 and compared in constant time. |
| Log files | Paths built only from sanitised names (a channel cannot leave its folder), folders 0700 and files 0600. |
| Plugins | WebAssembly in the Wasmi interpreter, with fuel (CPU) per event, 16 MB of memory, no access to files or a network of their own. Actions checked against the permissions. Only a superadministrator installs, seeing the SHA-256 fingerprint; a file that changes after installation is not loaded. Five failures in a row stop it. |
| DCC | Offers from IRC cannot make xBNC connect to internal networks or to ports below 1024 (unless allowed). For your own sends it only connects back to your device's IP. Each port accepts a single connection, with a timeout, a size limit and closing when idle. |
| Services login | On networks without NickServ (IRCnet, EFnet) automatic mode never sends the password. |
| Translations | Language files are data, not code: an IRC text with line breaks or control characters, or with placeholders different from the English ones, is ignored. Files of 2 MB at most and 100 languages. |
| FiSH | Key exchange accepts 5 requests per minute and 100 keys per session; decrypted text cannot break lines. |
| Webhooks | http(s) only, no user in the URL or control characters, certificate verified, no redirects followed, 10 s at most. |
| Import | Superadministrators only; it never grants that role; the preview returns no secrets. |
| Audit log | Every action of the panel, of IRC and of the command line is recorded with date, author and detail (Security → Audit, or xbncd audit). |
| Fuzzing | IRC parser, DCC, importer, search, manifests and plugin execution, every week in CI with earlier failures as regressions. |

### Recommendations

- Use a real certificate (Let's Encrypt) and do not publish plain-text ports.
- Keep backups encrypted: they contain the secret key.
- Every connection to the networks leaves from the server's IP: if a user misbehaves on a network, the server's IP may end up banned.
- Turn on two-step verification or a passkey on administration accounts.

To report a vulnerability, follow `SECURITY.md`: do not publish it; use the repository's private advisory. Answer within 3 days at most, assessment within 7 and a fix within 90 at most. The complete threat model is in [Threat model](THREAT_MODEL.md#threat-model).
