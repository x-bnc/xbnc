# Administration

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

- [Command line](#cli)
- [Alerts and monitoring](#monitoring)
- [Versions and updates](#updates)

<a id="cli"></a>

## Command line

Every command accepts `-c PATH` (or the `XBNC_CONFIG` variable; `/etc/xbnc/xbnc.toml` by default), `-h` for help and `-V` for the version. Those that write to the database are run as the service user: `sudo -u xbnc xbncd …` (in Docker, `docker compose exec xbnc xbncd …`). Afterwards, `sudo systemctl reload xbnc` so the service applies the changes to users, networks and channels. Every action is recorded in the audit log.

| Command | What it does | Options |
| --- | --- | --- |
| `xbncd run` | Runs the bouncer in the foreground (the service does this). SIGHUP (systemctl reload) re-reads networks, channels, users and languages. | — |
| `xbncd init` | Creates the database and the secret key. | — |
| `xbncd check-config` | Checks the configuration file and exits. | — |
| `xbncd info` | Shows the panel address and how clients connect. | — |
| `xbncd setup-code` | Shows the one-time code to create the first administrator. | — |
| `xbncd user add NAME` | Creates a user. The password is asked twice, read from standard input with --password-stdin or from XBNC\_PASSWORD. | --role superadmin\|admin\|moderator\|user\|limited (user by default); --admin (shortcut for --role superadmin); --password-stdin |
| `xbncd user list` | Lists the users. | — |
| `xbncd user passwd NAME` | Changes the password. | --password-stdin |
| `xbncd user role NAME ROLE` | Changes the role. | — |
| `xbncd user enable\|disable NAME` | Reactivates or suspends an account (after a reload, its networks disconnect). | — |
| `xbncd user reset-2fa NAME` | Removes two-step verification (lost phone). | — |
| `xbncd user delete NAME --yes` | Deletes the user, their networks and their history. | --yes (confirm) |
| `xbncd network add USER NAME --host SERVER` | Adds a network. SASL and server passwords are asked for or read from XBNC\_SASL\_PASSWORD and XBNC\_SERVER\_PASSWORD. | --port (6697); --no-tls; --nick; --username; --realname; --server-password; --sasl-user ACCOUNT |
| `xbncd network list USER` | Lists a user's networks. | — |
| `xbncd network enable\|disable USER NAME` | Enables or disables a network. | — |
| `xbncd network delete USER NAME --yes` | Deletes a network. | --yes |
| `xbncd channel add USER NETWORK CHANNEL` | Saves a channel to join automatically. | --key (asks for the channel key) |
| `xbncd channel list USER NETWORK` | Lists the saved channels. | — |
| `xbncd channel delete USER NETWORK CHANNEL` | Removes a saved channel. | — |
| `xbncd plugin install FILE.wasm` | Installs or updates a plugin; each user turns it on in their networks. | --yes (accept the permissions it asks for) |
| `xbncd plugin list` | Lists the installed plugins. | — |
| `xbncd plugin remove ID` | Uninstalls a plugin and deletes its settings on every network. | — |
| `xbncd plugin check FILE.wasm` | Checks a plugin and shows its manifest. | — |
| `xbncd plugin pack PLUGIN.wasm MANIFEST.json -o OUTPUT.wasm` | Puts the manifest (JSON) inside a compiled plugin. | -o, --output (required) |
| `xbncd plugin catalog-keygen -o KEY` | To publish a catalogue: creates the signing key (keep it safe). | -o, --output |
| `xbncd plugin catalog-entry FILE.wasm URL` | To publish a catalogue: prints the index entry of a plugin published at URL (https://). | — |
| `xbncd plugin catalog-sign --key KEY INDEX` | To publish a catalogue: signs the index (writes INDEX.sig). | -k, --key |
| `xbncd backup DESTINATION` | Consistent copy of the database, with xBNC running. | — |
| `xbncd migrate-to-postgres URL` | Copies the SQLite database to a new, empty PostgreSQL one. Stop xBNC first; then set database.url. | — |
| `xbncd import PATH` | Brings users, networks, channels and modules from another bouncer (its data folder, with configs/ and users/, or its configuration file). Without --apply nothing is written. | --apply; --logs (also the log files); --utc-offset ZONE (+02:00 or Europe/Madrid; +00:00 by default) |
| `xbncd import-logs USER NETWORK FOLDER` | Imports text logs (one folder per channel with YYYY-MM-DD.log files) into a network's history. Can be repeated without duplicating. | --utc-offset ZONE |
| `xbncd audit` | Latest administrative actions. | --limit N (50) |
| `xbncd release keygen` | To publish releases: creates the signing key pair. The public key goes in \[updates\] public\_key. | --out FILE (xbnc-release.key) |
| `xbncd release sign FILES…` | Writes FILE.sig next to each package. The private key is read from XBNC\_RELEASE\_KEY or from --key. | --key |
| `xbncd release verify --public-key KEY FILES…` | Checks the .sig signatures. | — |
| `xbncd apply-update` | Installs an update requested from the panel (run as root by the xbnc-update service; checks the signature again). | — |
| `xbncd locales check FILES…` | Checks language files against English: missing texts, texts xBNC does not use, broken {0} or \<b\> tags. Does not need the configuration. | --missing (also lists what is missing) |
| `xbncd locales list` | Languages this server offers, with the share translated of each. | — |

### Examples

```sh
sudo xbncd info
sudo xbncd setup-code
sudo -u xbnc xbncd user add alice
sudo -u xbnc xbncd user add carla --role moderator
echo 'a-long-password' | sudo -u xbnc xbncd user add bob --password-stdin
sudo -u xbnc xbncd user role carla admin
sudo -u xbnc xbncd user reset-2fa alice
sudo -u xbnc xbncd network add alice other --host irc.example.net --sasl-user alice
sudo -u xbnc xbncd network add alice example --host irc.example.org --port 6667 --no-tls
sudo -u xbnc xbncd channel add alice other '#xbnc' --key
sudo -u xbnc xbncd backup /var/lib/xbnc/copy.db
sudo -u xbnc xbncd import /home/old/.bouncer --logs --utc-offset Europe/London             # preview
sudo -u xbnc xbncd import /home/old/.bouncer --logs --utc-offset Europe/London --apply     # import
xbncd locales check pt.json --missing
sudo systemctl reload xbnc
```

### Scripts

| Script | Use |
| --- | --- |
| `scripts/install.sh` | Installs or reconfigures the service (see [Installation](INSTALL.md#install)). |
| `scripts/update.sh [--binary PATH] [--build]` | Updates the binary with a backup first and automatic rollback. After git pull, use --build (see [Installation](INSTALL.md#install)). |
| `scripts/prereqs.sh [--check]` | Installs what is needed to build xBNC (build tools and Rust), or with --check only says what is missing. |
| `scripts/backup.sh [FOLDER] [--keep N]` | Live backup: database, secret key and /etc/xbnc in a 0600 .tar.gz (in /var/backups/xbnc, keeps 14). |
| `scripts/restore.sh ARCHIVE [--with-config] [--yes]` | Restores a backup.sh copy. Keeps the current files as \*.pre-restore-DATE. |
| `scripts/uninstall.sh [--purge [--yes]]` | Removes the service and the binary; with --purge, also configuration, data and the xbnc user (it asks for confirmation unless --yes is given). |

<a id="monitoring"></a>

## Alerts and monitoring

### Alerts

xBNC checks these conditions every 30 seconds. They are adjusted in *Settings → Alerts* (0 turns each one off):

| Alert | Fires when | Default |
| --- | --- | --- |
| `network_down` | A network has been trying to connect for more than N minutes without success (those you disconnect do not count). Up to 10,080 (a week). | 15 minutes |
| `failed_logins` | There are N or more failed sign-ins (IRC, panel and tokens) in the last 10 minutes. | 30 |
| `memory` | The process memory goes over N MB. | off |
| `database` | The database goes over N MB. | off |

When an alert starts and when it is resolved:

- It appears (or disappears) on the panel's **Home** page for administrators.
- It is recorded in the audit log (action `alert`).
- With “Notify administrators over IRC” (on by default), every connected administrator and superadministrator gets a notice from `*status` in their language.
- With a **webhook** (only a superadministrator configures it), xBNC sends a POST with JSON. It has 10 seconds to answer with 2xx; if it fails, it is noted in the service log.

```json
{
  "source": "xbnc",
  "server": "bnc.example.org",
  "state": "firing",
  "alert": "network_down",
  "subject": "alice/other",
  "message": "Network alice/other has been disconnected for more than 15 minutes",
  "since": 1762600000000,
  "text": "[xBNC bnc.example.org] firing: Network alice/other ...",
  "content": "[xBNC bnc.example.org] firing: Network alice/other ..."
}
```

`state` is `firing` or `resolved`. The `text` and `content` fields make it work as it is with the incoming webhooks of Slack and Mattermost (`text`) and Discord (`content`).

### Prometheus metrics

`GET /api/v1/metrics` in Prometheus format, with an administrator's session or a token with the “metrics” scope:

```yaml
scrape_configs:
  - job_name: xbnc
    scheme: https
    metrics_path: /api/v1/metrics
    authorization:
      credentials: xbnc_...
    static_configs:
      - targets: ["bnc.example.org:8443"]
```

| Metric | Type | What it measures |
| --- | --- | --- |
| `xbnc_info` | gauge | Server version (label version). |
| `xbnc_uptime_seconds` | gauge | Seconds since it started. |
| `xbnc_users` | gauge | Accounts. |
| `xbnc_networks` | gauge | Configured networks. |
| `xbnc_networks_running{state}` | gauge | Connections by state: connected, connecting, disconnected, reconnecting. |
| `xbnc_irc_clients` | gauge | Connected IRC clients. |
| `xbnc_connections` | gauge | IRC connections open to the bouncer, also unauthenticated ones. |
| `xbnc_blocked_ips` | gauge | Addresses blocked after failures. |
| `xbnc_server_bytes_received_total`, `xbnc_server_bytes_sent_total` | counter | Bytes received from and sent to the IRC servers. |
| `xbnc_client_bytes_received_total`, `xbnc_client_bytes_sent_total` | counter | Bytes received from and sent to the clients. |
| `xbnc_server_lines_total` | counter | Lines received from the servers. |
| `xbnc_messages_stored_total` | counter | Messages stored in the history. |
| `xbnc_reconnects_total` | counter | Reconnections to IRC servers. |
| `xbnc_client_logins_total` | counter | Successful IRC client logins. |
| `xbnc_failed_logins_total` | counter | Failed logins (IRC and panel). |
| `xbnc_history_messages` | gauge | Messages stored in the history. |
| `xbnc_database_bytes` | gauge | Size of the database. |
| `xbnc_resident_memory_bytes` | gauge | Process memory. |
| `xbnc_log_lines_dropped_total` | counter | Lines of the log module dropped because the disk could not keep up. |
| `xbnc_dcc_relays_total` | counter | DCC sends and chats relayed by xBNC. |
| `xbnc_dcc_bytes_total` | counter | DCC bytes relayed by xBNC. |
| `xbnc_alerts_firing{alert}` | gauge | Active alerts by type. |

The same counters are shown on IRC with `/msg *status traffic` (administrators).

### Grafana and Prometheus rules

In `packaging/monitoring/`:

- `grafana-dashboard.json`: a dashboard ready to import with connected and disconnected networks, IRC clients, users, memory, uptime, messages per second, traffic, networks by state, logins, reconnections, storage, history and active alerts.
- `prometheus-rules.yml`: alerts XbncDown, XbncNetworksDisconnected, XbncManyReconnects, XbncFailedLogins, XbncBlockedAddresses, XbncMemoryHigh, XbncDatabaseLarge, XbncLogLinesDropped and XbncInternalAlert.

### Health check

`GET /api/v1/health` answers without a session; it is meant for load balancers and external monitors.

<a id="updates"></a>

## Versions and updates

### New version notices

With `[updates] repository` (or `url`) in `xbnc.toml`, xBNC checks the latest published version once a day and says so on the Home page and in *Settings → Updates*. On its own it downloads and installs nothing.

### Updating

xBNC is installed from GitHub with `git clone` or from the .zip of a release. To update: `git pull` in the xbnc folder and `sudo ./scripts/update.sh --build`, or unpack the new .zip and run `sudo ./scripts/update.sh` (see [Installation](INSTALL.md#install)).

### Updating from the panel (package installations)

xBNC also includes one-click updates for when it is installed from signed .deb or .rpm packages. With `[updates] public_key`, *Settings → Updates* shows “Update now” to superadministrators. xBNC downloads the package and its `.sig` signature, checks the Ed25519 signature and leaves it in `/var/lib/xbnc/updates/`. The system service `xbnc-update` checks everything again as root without trusting that folder (it verifies with the key in `/etc/xbnc/xbnc.toml`, which xBNC cannot change, and refuses equal or older versions), installs it with dpkg or rpm and xBNC restarts. A package without a signature, signed with another key or modified is never installed. Without `public_key`, or without a package installation, the panel explains how to update by hand.

### Publishing signed releases

```sh
# Once: create the key (the private one is secret; the public one goes in [updates] public_key)
xbncd release keygen --out xbnc-release.key

# For each release: sign the packages (writes FILE.sig)
XBNC_RELEASE_KEY="$(cat xbnc-release.key)" xbncd release sign xbnc_1.0.0_amd64.deb xbnc-1.0.0-1.x86_64.rpm

# Check
xbncd release verify --public-key "BASE64..." xbnc_1.0.0_amd64.deb
```

The repository includes a workflow that builds and signs .deb and .rpm packages and a container image (`.github/workflows/release.yml`). It needs the key as a repository secret (`XBNC_RELEASE_KEY`) and the public key as a variable (`XBNC_RELEASE_PUBLIC_KEY`). xBNC is published as a source .zip (with the ready-made program for x86\_64), so the workflow only runs when it is started by hand from the *Actions* tab.
