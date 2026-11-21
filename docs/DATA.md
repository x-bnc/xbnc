<a id="data"></a>

# Backups, databases and migration

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

### Backups from the panel

In *Settings → Backups* (superadministrators): make a backup now, download it, delete it, restore it and schedule automatic backups (every N hours, keeping the last M). Each backup is a `.tar.gz` with the database (a live copy with an integrity check), the secret key and the installed plugins, stored in `/var/lib/xbnc/backups/`.

Restoring leaves the backup staged (one can also be uploaded from your computer) and it is applied when xBNC restarts; until then it can be cancelled. Backups contain the secret key: keep them encrypted.

### Backups from the command line

```sh
sudo ./scripts/backup.sh                       # in /var/backups/xbnc, keeps 14
sudo ./scripts/backup.sh /mnt/backups --keep 30

# Every night at 3:30
echo '30 3 * * * root /path/to/xbnc/scripts/backup.sh >/dev/null' | sudo tee /etc/cron.d/xbnc-backup

# Restore (stops the service, keeps the current files as *.pre-restore-DATE, restores and starts)
sudo ./scripts/restore.sh /var/backups/xbnc/xbnc-20261108T033000Z.tar.gz
sudo ./scripts/restore.sh ARCHIVE --with-config    # also /etc/xbnc

# Only the database
sudo -u xbnc xbncd backup /var/lib/xbnc/copy.db
```

### PostgreSQL

```sh
sudo systemctl stop xbnc
sudo -u xbnc xbncd migrate-to-postgres postgres://xbnc:password@127.0.0.1/xbnc
# in /etc/xbnc/xbnc.toml:
#   [database]
#   url = "postgres://xbnc:password@127.0.0.1/xbnc"
sudo systemctl start xbnc
```

The target database must be empty. With `?sslmode=require` the connection is encrypted. Full-text search works the same with SQLite and PostgreSQL.

### High availability

With `ha = true`, several servers run xBNC against the same PostgreSQL database. The one holding a PostgreSQL lock works and the others wait. If the active one goes down, its connection closes, the lock is released and another takes over within seconds. An active node that loses the database stops at once, so there are never two at the same time. Clients must be able to reach the active node (a floating IP or a DNS name that changes).

### Migrating from another bouncer

xBNC brings over what you had in your previous bouncer if it uses the classic block configuration format (`<User name>`, `<Network name>`, `<Chan #channel>`).

| What | How it ends up in xBNC |
| --- | --- |
| Users | With their name. Administrators become admin (never superadmin). Users that already exist in xBNC are not touched. |
| Passwords | Those stored with salted SHA-256 keep working and move to Argon2id the first time each user signs in. Those in another format (MD5) get a new random password, shown only once. |
| Networks | Server, port, TLS, server password, nick, ident and real name. Disabled networks stay disabled. If there were several servers, the first is used (add the others afterwards as backup servers with `/msg *status addserver`). IPv6 servers are supported. |
| Channels | With their keys (encrypted). Disabled ones are not imported. |
| SASL, NickServ and commands on connect | SASL goes to the network; the NickServ password and the commands on connect go to “Commands on connect” (encrypted). They need the data folder. |
| Client certificate | Imported if it is in the data folder and its key matches. |
| Modules | The xBNC equivalent is turned on (table below). |
| Logs | The daily text files go into each network's history, with search. Can be repeated without duplicating. |

**From the panel** (*Users → Import from another bouncer*, superadministrators): choose the .conf file or paste its contents, click “See what will be imported” (nothing is written) and then “Import”. From the panel only the configuration is read; SASL, NickServ, commands on connect, certificates and logs are in the data folder and are imported on the server.

```sh
# Preview: changes nothing
sudo -u xbnc xbncd import /path/to/the/folder
# Import, with the logs (which were in London time)
sudo -u xbnc xbncd import /path/to/the/folder --apply --logs --utc-offset Europe/London
sudo systemctl reload xbnc

# Logs of one particular network, separately
sudo -u xbnc xbncd import-logs alice example /path/to/logs/example --utc-offset America/New_York
```

The logs folder has one sub-folder per channel or conversation (`#xbnc/`, `bob/`) with `YYYY-MM-DD.log` files. Messages, actions (`* nick …`), notices (`-nick- …`) and joins, parts, quits, kicks, nick changes, topics and modes are recognised. With a named time zone (Europe/London) each day's daylight saving time is applied.

| Module of the previous bouncer | In xBNC |
| --- | --- |
| `keepnick` | Keep nick |
| `simple_away`, `away`, `autoaway`, `awaynick` | Automatic away (with the delay and message of simple\_away) |
| `autoop` | Automatic op and voice (add the entries again: the syntax is different) |
| `kickrejoin` | Rejoin after a kick (with its delay) |
| `stickychan` | Sticky channels (with the list) |
| `buffextras` | Events in the history |
| `log` | Log files |
| `route_replies` | Replies to whoever asked |
| `cert` | Client certificate |
| `alias`, `watch` | Aliases and Watch words (add the entries again) |
| `block_motd` | Hide the MOTD |
| `ctcpreplies` | CTCP replies (with the reply to VERSION) |
| `perform`, `nickserv`, `sasl` | Commands on connect and the network's SASL |
| `chansaver`, `savebuff`, `playback`, `clientbuffer`, `webadmin`, `controlpanel`, `fail2ban`, `lastseen`, `notes`, `clearbufferonmsg`, `simple_disconnect`, `missingmotd`, `certauth`, `listsockets`, `identfile`, `partyline` | Already included in xBNC, always on |

Any other module shows in the preview as “no equivalent”. After importing: networks with several servers use only the first; networks without TLS are flagged; if you used the same ports, stop the previous bouncer before starting xBNC; and configure your clients with the `user/network` format.
