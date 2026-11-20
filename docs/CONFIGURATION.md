<a id="configuration"></a>

# Configuration (xbnc.toml)

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

`/etc/xbnc/xbnc.toml` only holds process settings: users, networks and channels are in the database. The installer creates it for you. After editing it: `sudo xbncd check-config` and `sudo systemctl restart xbnc`. An unknown key is an error, so that a typo does not go unnoticed. What you set in *Settings* in the panel takes priority over `[history]` and `[limits]`.

### General

| Key | Default | What it does |
| --- | --- | --- |
| `hostname` | xbnc | Server name the clients see. |
| `language` | en | Default language of the IRC messages and of the panel: en, es or any language installed in locales\_dir. Each user can choose their own. The installer sets the one you choose. |
| `locales_dir` | /etc/xbnc/locales | Folder of extra languages and corrections (one \<code\>.json file per language, see [Translating xBNC](TRANSLATIONS.md#translations)). Read at start and on reload. The XBNC\_LOCALES\_DIR environment variable takes priority. |
| `secret_key_file` | required | Path of the 32-byte key that encrypts the credentials. Created by xbncd init with permissions 0600. **Back it up**: without it they cannot be decrypted. |

### \[database\]

| Key | Default | What it does |
| --- | --- | --- |
| `path` | required | SQLite file. Its folder is also the data folder (plugins, log files, backups). |
| `url` | — | PostgreSQL instead of SQLite: postgres://user:password@host/db. Add ?sslmode=require to encrypt the connection. |
| `ha` | false | High availability (needs url): several nodes share the database; one works and the others wait to take over. |

### \[\[listen\]\] (one per port)

| Key | Default | What it does |
| --- | --- | --- |
| `address` | required | Address and port: 0.0.0.0:6697 or \[::\]:6697. |
| `tls_cert` | — | Certificate chain (PEM). Required unless insecure, lan\_only or a loopback address. |
| `tls_key` | — | Private key (PEM). |
| `insecure` | false | Allow plain text on a public address (not recommended). |
| `lan_only` | false | Local network clients only (loopback, private, link-local, CGNAT/Tailscale, IPv6 ULA). The rest get an explanatory ERROR. May go without TLS. |

### \[web\]

| Key | Default | What it does |
| --- | --- | --- |
| `address` | required | Address of the panel and the API, for example 0.0.0.0:8443 or 127.0.0.1:8080. Without \[web\], there is no panel. |
| `tls_cert, tls_key` | — | Certificate of the panel. |
| `insecure` | false | Unencrypted HTTP on a public address (only behind a TLS proxy on a trusted network). |
| `secure_cookies` | true with TLS | Marks the session cookie as Secure. Set it to true behind a TLS proxy. |
| `public_url` | — | Public URL of the panel (https://bnc.example.org). Used to check the Origin of requests and as the base for OpenID Connect. |
| `session_hours` | 12 | Session length in hours (sliding). |
| `trust_proxy` | false | Trust X-Forwarded-For and X-Real-IP from a proxy on loopback, so blocking uses the real IP. |
| `shared` | false | Serve the panel on the IRC port of address (one of the \[\[listen\]\] entries, with TLS): browsers get the panel and IRC clients the bouncer, on the same port, with the listener's certificate (leave tls\_cert and tls\_key out here). |

### \[history\]

| Key | Default | What it does |
| --- | --- | --- |
| `retention_days` | 30 | Delete messages older than N days. 0, forever. |
| `playback_max` | 1000 | Maximum messages played back on reconnection. |
| `new_client_playback` | 100 | Messages for a device that had never connected. |
| `chathistory_max` | 500 | Maximum limit accepted in CHATHISTORY. |

### \[limits\]

| Key | Default | What it does |
| --- | --- | --- |
| `max_connections` | 1000 | IRC client connections in total. |
| `max_clients_per_user` | 10 | Devices connected per user. |
| `max_networks_per_user` | 10 | Networks per user. |
| `login_max_failures` | 5 | Failures before an IP is blocked. |
| `login_window_seconds` | 600 | Window in which failures are counted. |
| `login_block_seconds` | 900 | Length of the block. |
| `registration_timeout_seconds` | 30 | Time for a client to complete its login. |
| `client_queue` | 4096 | Lines queued per client before disconnecting it as slow (when it comes back it gets what was pending). |

### \[log\]

| Key | Default | What it does |
| --- | --- | --- |
| `level` | info | Filter: error, warn, info, debug, trace, or per module (info,xbncd=debug). The RUST\_LOG variable takes priority. Credentials are masked at every level. |

### \[dcc\]

| Key | Default | What it does |
| --- | --- | --- |
| `public_ip` | the one xBNC uses towards the network | Address announced on IRC. Needed behind NAT. If the connection to the network is IPv6, the public IPv4 is announced. |
| `ports` | any free one | Range for the tunnels, for example 50000-50100 (open it in the firewall and the router). The installer writes 50000-50100 and opens it. |

### \[smtp\] (e-mail notifications)

| Key | Default | What it does |
| --- | --- | --- |
| `host` | required | Mail server. |
| `port` | 587 | Port. |
| `security` | starttls | starttls, tls (port 465) or none (local relays only). |
| `username, password` | — | Credentials. |
| `from` | required | Sender, for example xBNC \<xbnc@example.org\>. |

### \[oidc\] (OpenID Connect)

| Key | Default | What it does |
| --- | --- | --- |
| `name` | — | Text of the button (Google, Keycloak…). |
| `issuer` | required | URL of the issuer; its /.well-known/openid-configuration is read. |
| `client_id, client_secret` | required | Details of the application registered with the provider. The return address is `https://YOUR-PANEL/api/v1/auth/oidc/callback` (uses public\_url). |
| `scopes` | openid profile email | Scopes requested. |
| `username_claim` | preferred\_username | Claim used as the name of automatically created accounts. |
| `auto_create` | false | Create the xBNC account the first time someone signs in. |
| `link_existing` | false | Link with a local account of the same name (only with providers you control). |
| `default_role` | user | Role of the accounts created. |
| `allowed_groups` | \[\] (everyone) | If given, only members of one of these groups may sign in. |
| `groups_claim` | groups | Claim with the groups. |

### \[ldap\]

| Key | Default | What it does |
| --- | --- | --- |
| `url` | required | ldaps://host:636, or ldap://host:389 with starttls = true. |
| `starttls` | false | Use STARTTLS. |
| `bind_dn` | required | DN used to bind; {{username}} is replaced by the name typed. |
| `ca_file` | system certificates | PEM file with the authority of the LDAP server. |
| `auto_create, link_existing, default_role` | false, false, user | As in \[oidc\]. |

### \[updates\]

| Key | Default | What it does |
| --- | --- | --- |
| `check` | true | Check once a day whether there is a new version (and when an administrator asks). Downloads nothing. |
| `repository` | — | GitHub repository with the releases (owner/name). |
| `url` | — | Or any https URL answering {{"version", "url", "notes", "packages"}}. |
| `public_key` | — | Ed25519 public key of the releases. With it, and xBNC installed from a package, a superadministrator can update from the panel. |

### \[catalog\] (remote plugin catalogue)

| Key | Default | What it does |
| --- | --- | --- |
| `url` | — | https:// address of the JSON index. |
| `public_key` | — | Ed25519 public key (base64) that signs the index. |

### Complete example

```toml
hostname = "bnc.example.org"
language = "en"
secret_key_file = "/var/lib/xbnc/secret.key"
# locales_dir = "/etc/xbnc/locales"

[database]
path = "/var/lib/xbnc/xbnc.db"
# url = "postgres://xbnc:password@127.0.0.1/xbnc?sslmode=require"
# ha = true

[[listen]]
address = "0.0.0.0:6697"
tls_cert = "/etc/xbnc/tls/fullchain.pem"
tls_key = "/etc/xbnc/tls/privkey.pem"

[[listen]]
address = "0.0.0.0:6667"
lan_only = true

[web]
address = "0.0.0.0:8443"
tls_cert = "/etc/xbnc/tls/fullchain.pem"
tls_key = "/etc/xbnc/tls/privkey.pem"
public_url = "https://bnc.example.org:8443"
session_hours = 12

[history]
retention_days = 30
playback_max = 1000
new_client_playback = 100
chathistory_max = 500

[limits]
max_connections = 1000
max_clients_per_user = 10
max_networks_per_user = 10
login_max_failures = 5
login_window_seconds = 600
login_block_seconds = 900
registration_timeout_seconds = 30
client_queue = 4096

[log]
level = "info"

[dcc]
public_ip = "203.0.113.10"
ports = "50000-50100"

[smtp]
host = "smtp.example.org"
port = 587
security = "starttls"
username = "xbnc@example.org"
password = "..."
from = "xBNC <xbnc@example.org>"

[oidc]
name = "Keycloak"
issuer = "https://sso.example.org/realms/home"
client_id = "xbnc"
client_secret = "..."
auto_create = true
default_role = "user"
allowed_groups = ["irc"]

[ldap]
url = "ldaps://ldap.example.org:636"
bind_dn = "uid={username},ou=people,dc=example,dc=org"

[updates]
repository = "owner/xbnc"

[catalog]
url = "https://plugins.example.org/index.json"
public_key = "BASE64..."
```
