# Installation and first steps

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

- [Installation](#install)
- [First steps](#first-steps)

<a id="install"></a>

## Installation

### Requirements

- Any Linux with systemd: Debian, Ubuntu, Fedora, Rocky, Alma, RHEL, Arch, openSUSE and others. Nothing else has to be installed first.
- On 64-bit PCs and servers (x86\_64, the usual on a VPS) the program comes ready-made inside the .zip, in `bin/`, and works on any distribution from 2019 on (glibc 2.28 or later).
- On other machines (ARM, such as a Raspberry Pi or an ARM VPS) or on older systems, the installer builds it by itself: it installs the build tools with the system's package manager and Rust with rustup, the official installer (see *Building from the source code*, below). It needs 2 GB of memory (swap included) and 3 GB of disk while it builds.
- 1 CPU and 512 MB of RAM for personal use. Measured: 2,000 users with their networks and clients in about 150 MB (see [Performance](PERFORMANCE.md#performance)).
- Ports: 6697 (IRC with TLS), 6667 (IRC without TLS, local network only), 8443 (panel). With Let's Encrypt, also 80; with Nginx, 443.

The web panel comes already built inside the source code: Node.js and npm are not needed.

### Install

```sh
# 1. Unpack the .zip (as a normal user)
unzip xbnc-1.0.0.zip && cd xbnc

# 2. Install it
sudo ./scripts/install.sh
```

That is all. The installer uses the ready-made program when there is one for your machine, and otherwise builds it first (it takes a few minutes the first time).

Run in a terminal without options, the installer first asks for its language, which is also xBNC's default language: English (the default), Spanish or any other language file in `locales/`. Then it asks whether you want to use a domain, the e-mail address for Let's Encrypt and whether you want the panel at `https://yourdomain` with Nginx. At the end it shows the panel address and the one-time code to create your account:

```
  Panel:   https://bnc.example.org:8443
  IRC:     bnc.example.org, port 6697 (SSL)
  Code to create your account:  K7QM-2XPA-9DWE
```

Open the panel, enter the code and create the first administrator (it will be a superadministrator). To see the address and ports again: `sudo xbncd info`; to see the code again: `sudo xbncd setup-code`.

### What the installer does

1. Finds the program: the ready-made one for this machine in `bin/`, checked against `bin/SHA256SUMS`, or it builds it (see below).
2. Checks the system and, if you use a domain, that its DNS points to this machine and which ports you must open on your router.
3. Creates the unprivileged system user `xbnc` and the folders with their permissions.
4. Installs the binary in `/usr/local/bin/xbncd` and the systemd service with isolation measures (`ProtectSystem=strict`, `NoNewPrivileges`, no capabilities, a system call filter).
5. Creates the certificate: self-signed for the IP (or the domain), or from Let's Encrypt with automatic renewal. Renewal reloads xBNC (and Nginx, if used).
6. Writes `/etc/xbnc/xbnc.toml` with the version in its header, and creates the database and the secret key.
7. With `--nginx`, installs and configures Nginx: redirection from port 80, the panel on 443, WebSockets and uploads of plugins and backups.
8. Asks for the ports: one port for the panel and IRC (browsers get the panel and IRC clients the bouncer, with the same certificate) or one for each. It refuses ports that browsers will not open (6667 and 6697, the usual IRC ports, are among them) and ports another program is using.
9. Opens the ports in ufw or firewalld (also the DCC range, 50000-50100) and starts the service.

### Installer options

| Option | What it does |
| --- | --- |
| `--domain NAME` | Use a domain (bnc.example.org) instead of the IP. |
| `--letsencrypt EMAIL` | With --domain: a free Let's Encrypt certificate with automatic renewal. Needs port 80 reachable from the internet, or --dns. |
| `--dns PROVIDER` | With --letsencrypt: check the domain by DNS instead of port 80 (cloudflare, digitalocean, ovh, linode or rfc2136). Renewals work the same way. |
| `--dns-credentials FILE` | With --dns: the provider's credentials file for its certbot-dns plugin (it is copied with permissions 600). |
| `--nginx` | With --domain and --letsencrypt: installs and configures Nginx to serve the panel at https://DOMAIN (port 443). |
| `--ip ADDRESS` | IP to use if the one detected is not right (several interfaces or a VPN). |
| `--language CODE` | Language of the installer and default language of xBNC: en, es or any language file in locales/. Without it, the installer asks in a menu (English by default). |
| `--behind-proxy` | Panel on 127.0.0.1:8080, to publish it with your own proxy. |
| `--web-port N` | Panel port (8443 by default). |
| `--irc-port N` | IRC port with SSL (6697 by default). |
| `--single-port N` | One port for the panel and IRC with SSL: https://ADDRESS:N in the browser and the same port in the IRC client. Browsers refuse some ports (6667, 6697…); 8443 works. |
| `--dcc-ports A-B\|off` | Ports for DCC transfers, written in \[dcc\] and opened in the firewall (50000-50100 by default); off leaves them closed. |
| `--plain-port N\|off` | IRC port without SSL, local network only (6667 by default); off removes it. |
| `--no-firewall` | Do not open ports in ufw or firewalld. |
| `--reconfigure` | Redo the configuration and the certificate of an existing installation (keeps a copy of the previous one). Does not touch users, networks or history. |
| `--binary PATH` | Binary to install. By default, the ready-made one in bin/ for this machine, or it is built when there is none. |
| `--build` | Build from the source code even if there is a ready-made binary. |
| `--no-start` | Install without starting the service. |
| `--non-interactive` | Ask nothing (by default it asks when run in a terminal). |
| `--hostname NAME` | Another name for --domain (older syntax); with an IP it means --ip. |

To change the ports of an existing installation, add `--reconfigure`: without it the installer keeps the configuration and refuses port options that would not apply.

### Building from the source code

The installer does it by itself when there is no ready-made program for your machine, or with `--build`. It runs `scripts/prereqs.sh`, which:

1. Installs the C compiler, `make`, `pkg-config` and `curl` with the system's package manager: apt, dnf, yum, pacman, zypper or apk.
2. Installs Rust with rustup, the official installer, for the user who unpacked the .zip, in their home folder (`~/.cargo`). The Rust packaged by many distributions is too old, so it is only used if it is at least version 1.89; if not, rustup is installed alongside it. If Rust was already installed with rustup but is old, it is updated.

Then it builds xBNC as that user (never as root) with `cargo build --release --locked`, which uses exactly the versions of the libraries that were tested. It warns if the machine has less than 2 GB of memory and explains how to add swap.

To see what is missing without installing anything: `./scripts/prereqs.sh --check`. To prepare everything and build by hand:

```sh
sudo ./scripts/prereqs.sh
cargo build --release          # as a normal user; ~/.cargo/bin must be in the PATH
sudo ./scripts/install.sh       # uses target/release/xbncd
```

### Examples

```sh
# By IP, with a self-signed certificate
sudo ./scripts/install.sh

# With a domain and Let's Encrypt, panel at https://bnc.example.org:8443
sudo ./scripts/install.sh --domain bnc.example.org --letsencrypt you@example.org

# The same, with the panel at https://bnc.example.org behind Nginx
sudo ./scripts/install.sh --domain bnc.example.org --letsencrypt you@example.org --nginx

# Move an existing installation to a domain
sudo ./scripts/install.sh --reconfigure --domain bnc.example.org --letsencrypt you@example.org

# Change the detected IP
sudo ./scripts/install.sh --reconfigure --ip 192.168.1.50

# One port for the panel and IRC: https://bnc.example.org:7000 and port 7000 with SSL
sudo ./scripts/install.sh --domain bnc.example.org --letsencrypt you@example.org --single-port 7000

# Let's Encrypt by DNS (port 80 closed), with Cloudflare
sudo ./scripts/install.sh --domain bnc.example.org --letsencrypt you@example.org --dns cloudflare --dns-credentials /root/cloudflare.ini
```

### Where everything goes

| Path | Contents |
| --- | --- |
| `/usr/local/bin/xbncd` | The program. |
| `/etc/xbnc/xbnc.toml` | Process configuration (root:xbnc, 0640). |
| `/etc/xbnc/tls/` | TLS certificate: fullchain.pem and privkey.pem (root:xbnc, 0750). |
| `/etc/xbnc/locales/` | Extra languages and translation corrections (see [Translating xBNC](TRANSLATIONS.md#translations)). |
| `/var/lib/xbnc/` | Database and secret key (xbnc:xbnc, 0700). **Back up secret.key**: without it the stored passwords cannot be decrypted. |
| `/var/lib/xbnc/logs/` | Log files of the log module (folders 0700, files 0600). |
| `/var/lib/xbnc/plugins/` | Installed plugins (0600). In `dev/`, those of development mode. |
| `/var/lib/xbnc/backups/` | Backups made from the panel. |

The service runs as the unprivileged user `xbnc`. `sudo systemctl status xbnc`, `sudo systemctl restart xbnc`; `sudo systemctl reload xbnc` makes it re-read networks, channels and users changed from the command line, and the language files. The log: `sudo journalctl -u xbnc -f`.

### Using it away from home

- Forward port **6697** on your router to the xBNC machine and connect to your public IP or to a dynamic DNS name (DuckDNS and the like). Port 6667 need not be opened: xBNC refuses it from outside.
- If you do not want to open the panel (8443) to the internet, manage it through an SSH tunnel: `ssh -L 8443:127.0.0.1:8443 user@your-public-ip` and open `https://localhost:8443`.
- With a domain pointing to your home, reinstall with `--reconfigure --domain … --letsencrypt …` to get rid of the certificate warnings.

### Panel behind your own Nginx

If you already run Nginx and prefer to configure it yourself, install with `--domain bnc.example.org --behind-proxy` and use the included example:

```sh
sudo apt install -y nginx
echo 'map $http_upgrade $connection_upgrade { default upgrade; "" close; }' | sudo tee /etc/nginx/conf.d/upgrade.conf
sudo cp packaging/nginx/xbnc.conf /etc/nginx/sites-available/xbnc   # change server_name and certificates
sudo ln -s ../sites-available/xbnc /etc/nginx/sites-enabled/xbnc
sudo nginx -t && sudo systemctl reload nginx
```

Contents of `packaging/nginx/xbnc.conf`:

```nginx
# Nginx reverse proxy for the xBNC web panel.
# Copy to /etc/nginx/sites-available/xbnc, adjust server_name and the
# certificate paths, then: ln -s ../sites-available/xbnc /etc/nginx/sites-enabled/
# In xbnc.toml: [web] address = "127.0.0.1:8080", public_url = "https://bnc.example.org",
# secure_cookies = true, trust_proxy = true.

server {
    listen 80;
    listen [::]:80;
    server_name bnc.example.org;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl;
    listen [::]:443 ssl;
    http2 on;
    server_name bnc.example.org;

    ssl_certificate     /etc/letsencrypt/live/bnc.example.org/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/bnc.example.org/privkey.pem;
    ssl_protocols TLSv1.2 TLSv1.3;

    # Plugins are up to 4 MB.
    client_max_body_size 8m;

    # Restoring a backup uploads the whole archive.
    location = /api/v1/backups/restore {
        client_max_body_size 4g;
        proxy_request_buffering off;
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 3600s;
    }

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_set_header X-Forwarded-Proto $scheme;
        # WebSockets: live dashboard, web chat, plugin log
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection $connection_upgrade;
        proxy_read_timeout 3600s;
    }
}

# Needed once per Nginx configuration (e.g. in /etc/nginx/conf.d/upgrade.conf):
# map $http_upgrade $connection_upgrade { default upgrade; '' close; }
```

In `xbnc.toml`, with the panel behind Nginx: `address = "127.0.0.1:8080"`, `public_url = "https://bnc.example.org"`, `secure_cookies = true` and `trust_proxy = true`.

### Certificate from another authority

Copy your certificate and key to `/etc/xbnc/tls/fullchain.pem` and `/etc/xbnc/tls/privkey.pem` (owner root:xbnc, the key with permissions 0640) and restart with `sudo systemctl restart xbnc`. The certificate is read at start; `reload` is not enough.

### Docker

```sh
mkdir -p tls
cp /path/fullchain.pem /path/privkey.pem tls/
# Edit XBNC_HOSTNAME and XBNC_PUBLIC_URL in docker-compose.yml
docker compose up -d --build
docker compose logs xbnc | grep "setup code"
```

Open `https://bnc.example.org:8443/`, enter the code and create your account. The command line: `docker compose exec xbnc xbncd user add admin --admin`. The data lives in the `xbnc-data` volume. The container runs as the unprivileged user `xbnc` (uid 10001), with a read-only file system, no capabilities and `no-new-privileges`.

| Variable | Use |
| --- | --- |
| `XBNC_HOSTNAME` | Server name (only on the first start). |
| `XBNC_TLS_CERT`, `XBNC_TLS_KEY` | Paths of the certificate and key inside the container; they turn on TLS on 6697 (IRC) and 8443 (panel). |
| `XBNC_INSECURE=1` | Plain text on 6667 (IRC) and 8080 (panel), for testing only. |
| `XBNC_PUBLIC_URL` | Public URL of the panel. |
| `XBNC_WEB=0` | Turns the panel off. |
| `XBNC_LOCALES_DIR` | Folder of extra languages (see [Translating xBNC](TRANSLATIONS.md#translations)). |
| `RUST_LOG` | Log level, for example info or debug. |

### Updating

```sh
unzip xbnc-new.zip && cd xbnc     # or git pull
sudo ./scripts/update.sh
```

`update.sh` takes the new program the same way as the installer (the ready-made one in `bin/`, or it builds it; `--build` forces building), checks it and the configuration, makes a backup, stops the service, swaps the binary and starts it. If it does not start, it goes back to the previous binary **and** to the database from before the update (a new binary may have upgraded the schema, and an old one refuses to open a newer schema). Nothing is ever updated on its own. With `--binary PATH` it installs another binary.

If you also want to redo the configuration (for example, to move to a domain), run `sudo ./scripts/install.sh --reconfigure` afterwards.

### Uninstalling

```sh
sudo ./scripts/uninstall.sh            # removes the service and the binary; keeps /etc/xbnc and /var/lib/xbnc
sudo ./scripts/uninstall.sh --purge    # also deletes the configuration, the data and the xbnc user
```

> Port 6667 without SSL only accepts connections from your local network (private addresses, loopback, link-local, CGNAT/Tailscale and IPv6 ULA). From the internet xBNC refuses it with a message pointing to 6697.

<a id="first-steps"></a>

## First steps

1. **Create your account** in the panel with the one-time code. The first account is a superadministrator.
2. **Add a network** in *Networks → Add network*. The wizard has four steps: choose the network (the well-known ones come ready), your nick and name, the identification (SASL, X, Q…) and the channels.
3. **Connect your client**. The network page has the card “Connect your IRC client” with your exact details and the steps for HexChat, mIRC, WeeChat and irssi, with buttons to copy them.
4. **Turn on modules and plugins** in *Modules*. Choose the network at the top; with many networks, “Copy to other networks” repeats the configuration.
5. **Choose your time zone and language** in *Your account*: the time zone is used in the log files and in scheduled tasks.
6. **Turn on two-step verification** and, if you like, a passkey, in *Your account*.

### Networks ready in the wizard

| Network | Server | Port | TLS | Identification | Description |
| --- | --- | --- | --- | --- | --- |
| Undernet | `irc.undernet.org` | 6667 | no | X with Login on Connect | Classic network with all kinds of channels |
| Libera.Chat | `irc.libera.chat` | 6697 | yes | SASL | Free software communities and open source projects |
| freenode | `irc.freenode.net` | 6697 | yes | SASL | Historic network of projects and communities |
| Rizon | `irc.rizon.net` | 6697 | yes | SASL or NickServ | Anime, games and all kinds of communities |
| EvilNet | `irc.evilnet.org` | 6697 | yes | NickServ | Network with NickServ services |
| IRCnet | `open.ircnet.net` | 6697 | yes | no NickServ | Historic network, very popular in Europe |
| OFTC | `irc.oftc.net` | 6697 | yes | NickServ | Debian, Tor, the Linux kernel and other projects |
| EFnet | `irc.efnet.org` | 6697 | yes | no services | One of the oldest networks |
| QuakeNet | `irc.quakenet.org` | 6667 | no | Q | Games and European communities |
| DALnet | `irc.dal.net` | 6697 | yes | SASL | Social and support communities |
| hackint | `irc.hackint.org` | 6697 | yes | SASL | Hackerspaces and technical communities |

> Undernet and QuakeNet do not offer TLS: the connection to the network goes over 6667 unencrypted. The connection between your client and xBNC is encrypted.

### The same from the command line

```sh
sudo -u xbnc xbncd user add admin --admin
sudo -u xbnc xbncd network add admin libera --host irc.libera.chat --port 6697 --sasl-user myaccount
sudo -u xbnc xbncd channel add admin libera '#xbnc'
sudo systemctl reload xbnc
```

The SASL password is asked for at that moment (or read from `XBNC_SASL_PASSWORD`) and stored encrypted.
