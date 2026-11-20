<a id="api"></a>

# REST API

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

Everything the panel does is in the API. Base: `/api/v1`, at the same address as the panel. JSON format; errors are `{"error": "code", "message": "text"}` with the right HTTP status. The OpenAPI 3.1 specification is at `GET /api/v1/openapi.json` (and in `docs/openapi.json`); a test checks that it documents every route.

### API tokens

Create them in *Your account → API tokens*. The full token (`xbnc_` and 43 characters) is only shown when created: xBNC stores its SHA-256. Send it in the header `Authorization: Bearer xbnc_…`; with a token no CSRF is needed.

| Scope | Can |
| --- | --- |
| `read` | GET requests only. |
| `write` | Everything your account can do, except signing out, changing the password, two-step verification, passkeys and creating or revoking tokens (those need a panel session and answer 403 session\_required). |
| `metrics` | GET /metrics only. |

They expire after 30, 90 or 365 days, or never; at most 50 per user. The list shows the prefix, the last use and the last IP. A wrong token counts as a failed attempt (the same blocks as IRC and the panel). Creating and revoking is recorded in the audit log.

### Panel session

1. `POST /auth/login` with `{"username", "password"}` (and `totp_code` if the account has two-step verification). The answer brings the `xbnc_session` cookie and, in the body, `csrf`.
2. Send the cookie with every request, and with POST, PUT, PATCH and DELETE the header `X-CSRF-Token` with that value (`GET /auth/me` returns it again).
3. Requests with an `Origin` header from another site are refused; curl and scripts do not send it.

### Examples

```sh
B=https://bnc.example.org:8443/api/v1
T="Authorization: Bearer xbnc_..."

# Networks and their state
curl -s -H "$T" $B/networks

# Search the history of network 1
curl -s -H "$T" "$B/networks/1/history?target=%23xbnc&q=meeting&limit=20"

# Reconnect a network (write token)
curl -s -X POST -H "$T" -H 'Content-Type: application/json' -d '{"action":"jump"}' $B/networks/1/control

# Turn on a module
curl -s -X PUT -H "$T" -H 'Content-Type: application/json' \
  -d '{"enabled":true,"config":{"delay":60,"summary":true}}' $B/networks/1/modules/away

# Languages and the Spanish panel texts (no session needed)
curl -s $B/i18n
curl -s $B/i18n/es

# With a session instead of a token: create a user and give them a network
curl -s -c jar -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"your-password"}' $B/auth/login > login.json
CSRF=$(python3 -c 'import json;print(json.load(open("login.json"))["csrf"])')
curl -s -b jar -H "X-CSRF-Token: $CSRF" -H 'Content-Type: application/json' \
  -d '{"username":"bruno","password":"a-long-password","role":"user","max_networks":3}' $B/users
curl -s -b jar -H "X-CSRF-Token: $CSRF" -H 'Content-Type: application/json' \
  -d '{"user_id":2,"name":"libera","host":"irc.libera.chat","port":6697,"tls":true,"nick":"bruno",
       "sasl_username":"bruno","sasl_password":"secret","channels":["#xbnc","#xbnc-dev"]}' $B/networks
```

### All routes

| Method | Route | What it does | Who |
| --- | --- | --- | --- |
| `GET` | `/health` | Health check. | no session |
| `GET` | `/i18n` | Installed languages (code, name, share translated, translators) and the server's default language. | no session |
| `GET` | `/i18n/{code}` | Panel texts in one language: English text → translation. | no session |
| `GET, POST` | `/setup` | First setup with the one-time code. | no session |
| `POST` | `/auth/login` | Sign in: {username, password, totp\_code}. Returns the xbnc\_session cookie and the csrf field. | no session |
| `POST` | `/auth/logout` | Close the session. | session |
| `GET` | `/auth/me` | Your account, your permissions and the CSRF token. | session or token |
| `GET` | `/auth/methods` | Ways of signing in the server offers (passkeys, OpenID Connect, LDAP). | no session |
| `GET` | `/auth/oidc/start` | Redirects to the OpenID Connect provider. | no session |
| `GET` | `/auth/oidc/callback` | Return from the OpenID Connect provider. | no session |
| `POST` | `/auth/passkey/options` | Starts signing in with a passkey (WebAuthn challenge). | no session |
| `POST` | `/auth/passkey` | Signs in with the passkey's answer. | no session |
| `GET` | `/dashboard` | Summary of the Home page (global for administrators). | session or token |
| `GET` | `/presets` | Well-known networks for the wizard. | session or token |
| `GET, POST` | `/networks` | List (?all=true: every network, administrators) and create networks: {user\_id, name, host, port, tls, nick, username, realname, server\_password, sasl\_username, sasl\_password, channels, on\_connect}. | create: user or above |
| `GET, PATCH, DELETE` | `/networks/{id}` | Show with live state, edit (an omitted secret is kept, "" deletes it) and delete. | owner or admin |
| `POST` | `/networks/{id}/control` | {action: connect \| disconnect \| jump}. | owner or admin |
| `GET, POST` | `/networks/{id}/channels` | Channels, with disabled and position (joining order); add {name, key}. | owner or admin |
| `DELETE` | `/networks/{id}/channels/{name}` | Remove a channel (encode # as %23). | owner or admin |
| `PUT` | `/networks/{id}/channels/{name}/history` | Retention of the channel: {retention\_days: null (server setting) \| -1 (do not store) \| 0 (forever) \| N days}. | owner or admin |
| `DELETE` | `/networks/{id}/clients/{client}` | Disconnect a device. | owner or admin |
| `GET` | `/networks/{id}/conversations` | Conversations with their number of messages. | owner or admin |
| `GET, DELETE` | `/networks/{id}/history` | Search: target, q, sender, before\_id, from, to, limit (up to 500), newest first. DELETE deletes (?target= one conversation). | owner or admin |
| `GET` | `/networks/{id}/history/export` | Download: ?format=txt\|json and ?target=. | owner or admin |
| `GET` | `/networks/{id}/chat` | WebSocket of the panel chat: the panel as one more IRC client. Messages {line: "PRIVMSG #xbnc :hello"} in both directions. | owner only, session |
| `GET, PUT` | `/networks/{id}/on-connect` | Commands on connect: {commands: "/msg X login …"}; empty deletes them. | owner or admin |
| `GET, PUT, DELETE` | `/networks/{id}/cert` | Client certificate: SHA-1/256/512 fingerprints; upload {certificate, private\_key} in PEM; delete. The key is never returned. | owner or admin |
| `POST` | `/networks/{id}/cert/generate` | Generate a new certificate. | owner or admin |
| `GET` | `/modules` | Module catalogue: ids, settings with type and limits. | session or token |
| `GET` | `/networks/{id}/modules` | State of a network's modules (secrets empty, plus secrets\_set). | owner or admin |
| `PUT` | `/networks/{id}/modules/{module}` | {enabled, config}: validated and applied at once. An empty secret keeps the stored one; null deletes it. | owner or admin |
| `GET` | `/networks/{id}/plugins` | Plugins of a network. | owner or admin |
| `PUT` | `/networks/{id}/plugins/{plugin}` | {enabled, config}. | owner or admin |
| `GET` | `/mentions` | Messages that mention you, newest first (?before=ms&limit=). | session or token |
| `GET` | `/notes` | Your notes. | session or token |
| `PUT, DELETE` | `/notes/{key}` | Save {text} (up to 400 characters) or delete a note. | session or token |
| `PUT` | `/me/language` | {language: code of an installed language (en, es…) \| ""} (empty: the server's). | session or token |
| `PUT` | `/me/timezone` | {timezone: "Europe/London"} (IANA name; empty: UTC). | session or token |
| `PUT` | `/me/password` | Change your password (closes your other sessions). | session only |
| `PUT` | `/me/avatar` | Profile photo: the image bytes with Content-Type image/png, image/jpeg or image/webp (at most 256 KB and 1024×1024 pixels; the real format is checked). | session or token |
| `DELETE` | `/me/avatar` | Remove your profile photo. | session or token |
| `GET` | `/users/{id}/avatar` | A user's photo: yours, or anyone's with view\_users. The address with ?v= is in the avatar field of users and /auth/me. | session or token |
| `DELETE` | `/users/{id}/avatar` | Remove a user's photo (manage\_users, on roles below yours). | session or token |
| `POST` | `/me/totp/setup` | Start turning on two-step verification (secret and QR code). | session only |
| `POST` | `/me/totp/enable` | Turn it on with a code. | session only |
| `POST` | `/me/totp/disable` | Turn it off. | session only |
| `GET, POST` | `/me/passkeys` | Your passkeys; register one. | session only |
| `POST` | `/me/passkeys/options` | Start registering a passkey. | session only |
| `DELETE` | `/me/passkeys/{id}` | Remove a passkey. | session only |
| `GET, DELETE` | `/me/identities` | Linked external accounts (OpenID Connect, LDAP); unlink. | session only |
| `POST` | `/me/identities/oidc` | Start linking your OpenID Connect account. | session only |
| `GET, PUT` | `/me/notifications` | Notification settings (the Telegram token comes back hidden). | session or token |
| `POST` | `/me/notifications/test` | Send a test notification to every channel. | session or token |
| `GET, POST, DELETE` | `/me/push` | Devices with push; subscribe {endpoint, keys: {p256dh, auth}, label}; remove {endpoint}. | session or token |
| `GET` | `/push/key` | VAPID public key for Web Push. | session or token |
| `GET` | `/diagnostics` | Test report: what each of your networks supports and how xBNC handled it (no secrets or messages). | session or token |
| `GET, POST` | `/tokens` | Your tokens; create {name, scope: read \| write \| metrics, expires\_days: 0–3650} (0: never expires). The token only comes in the answer. | session only |
| `DELETE` | `/tokens/{id}` | Revoke a token. | session only |
| `GET` | `/sessions` | Panel sessions (?all=true: all of them, administrators). | session |
| `DELETE` | `/sessions/{id}` | Close a session. | session |
| `GET, POST` | `/users` | List and create {username, password, role, max\_networks, max\_clients}. | see: moderator; create: admin |
| `GET, PATCH, DELETE` | `/users/{id}` | Show, change {role, enabled, max\_networks, max\_clients, max\_history\_mb} (null removes the limit) and delete. | depends on the role |
| `POST` | `/users/{id}/password` | Reset the password (closes their sessions). | admin |
| `GET` | `/users/{id}/usage` | Networks, clients and history space used against their quotas. | moderator |
| `GET` | `/security` | Sign-ins and blocked IPs. | moderator |
| `DELETE` | `/security/blocked/{ip}` | Unblock an IP. | admin |
| `GET` | `/audit` | Audit log. | moderator |
| `GET, PUT` | `/settings` | Live settings. | see: admin; edit: superadmin |
| `GET` | `/system` | Version, database, listeners and counters. | admin |
| `POST` | `/system/restart` | Restart xBNC (the service starts it again). | superadmin |
| `GET` | `/alerts` | Active alerts. | admin |
| `GET` | `/metrics` | Metrics in Prometheus format. | admin or metrics token |
| `GET` | `/ws` | WebSocket of the Home page: a {type: "stats"} message every 2 seconds with the state of the networks and, for administrators, memory, CPU, clients, connections, blocked IPs and counters; {type: "logout"} when the session ends. | session only |
| `GET, POST` | `/backups` | Backups, staged restore and settings of the automatic ones; make a backup now. | superadmin |
| `GET, DELETE` | `/backups/{name}` | Download or delete a backup. | superadmin |
| `POST` | `/backups/{name}/restore` | Stage the restore of a backup on the server. | superadmin |
| `POST, DELETE` | `/backups/restore` | Upload a backup (.tar.gz) and stage it; cancel the staged restore. | superadmin |
| `GET` | `/plugins` | Installed plugins. | session or token |
| `POST` | `/plugins` | Install or update: the body is the .wasm (application/wasm, up to 4 MB). | superadmin, session |
| `POST` | `/plugins/inspect` | Check a .wasm without installing it. | superadmin, session |
| `PATCH, DELETE` | `/plugins/{id}` | {enabled} on the server and approved domains; uninstall. | superadmin, session |
| `GET` | `/plugins/catalog` | Catalogue: official plugins and signed remote catalogue. | session or token |
| `POST` | `/plugins/catalog/{id}` | One-click install from the catalogue. | superadmin, session |
| `GET, PUT` | `/plugins/dev` | Development mode: state and folder; turn it on or off. | superadmin, session |
| `GET` | `/plugins/logs` | WebSocket with the plugins' log: last lines and new ones. | superadmin, session |
| `GET` | `/updates` | Installed and latest published version, whether it can be updated from the panel and the result of the last update. | admin |
| `POST` | `/updates/check` | Check now whether there is a new version. | admin, session |
| `POST` | `/updates/install` | Download, verify and install the signed package of the latest version. | superadmin, session |
| `POST` | `/import/preview` | Preview of the import: {config: "file contents"}. Writes nothing. | superadmin, session |
| `POST` | `/import/apply` | Import. | superadmin, session |

“owner or admin”: the owner of the network or an administrator. “session only”: not accepted with a token. Passwords are never returned: networks say `has_sasl_password` and `has_server_password`.
