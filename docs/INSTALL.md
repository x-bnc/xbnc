# Installation and first steps

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

- [Installation](#install)
- [Step-by-step guide for beginners](#beginner-guide)
- [The scripts folder](#scripts)
- [First steps](#first-steps)

<a id="install"></a>

## Installation

### Requirements

- Any Linux with systemd: Debian, Ubuntu, Fedora, Rocky, Alma, RHEL, Arch, openSUSE and others.
- **git**, to download the code from GitHub (`sudo apt install git`, `sudo dnf install git`…). The installer takes care of everything else.
- To build xBNC: about **2 GB of memory** (swap counts; the installer warns below that and explains how to add it) and **3 GB of free disk**. Building is needed only when installing or updating; running xBNC needs much less (next point).
- To run it: 1 CPU and 512 MB of RAM for personal use. Measured: 2,000 users with their networks and clients in about 150 MB (see [Performance](PERFORMANCE.md#performance)).
- Ports: with the installer's recommended answer, **8443** for the panel and IRC with SSL together; with one port each, 8443 for the panel and 6697 for IRC with SSL. Also 6667 for IRC without SSL (local network only) and 50000-50100 for DCC. With Let's Encrypt, also 80; with Nginx, 80 and 443.

The web panel comes already built inside the source code: Node.js and npm are not needed.

### Install from GitHub

```sh
# 1. Download the code (as a normal user, not as root)
git clone https://github.com/x-bnc/xbnc.git

# 2. Go into its folder
cd xbnc

# 3. Install it
sudo ./scripts/install.sh
```

That is all. The code from GitHub does not include the compiled program, so the first time the installer builds it on your server: it installs the build tools and Rust, compiles xBNC (several minutes; more on a small VPS) and installs it. If you have never set up a server, follow the [step-by-step guide](#beginner-guide).

### With the .zip of a release

Each stable version published on [GitHub Releases](https://github.com/x-bnc/xbnc/releases) also has a `.zip` with the program already built for 64-bit PCs and servers (x86\_64, the usual on a VPS), in `bin/` and checked against `bin/SHA256SUMS`. It works on any distribution from 2019 on (glibc 2.28 or later), and nothing has to be compiled:

```sh
unzip xbnc-1.0.0.zip && cd xbnc
sudo ./scripts/install.sh
```

> The .zip only exists when a stable version has been published on Releases. `git clone` always works: it just takes longer because it builds the program.

### The installer's questions

Run in a terminal without options, the installer asks:

1. **The language** of the installer, which is also xBNC's default language: English (the default), Spanish or any other language file in `locales/`.
2. **Whether to use a domain.** No by default: xBNC works with the machine's IP, which it detects by itself. With a domain it also asks whether to request a free Let's Encrypt certificate, the e-mail address for its notices, whether port 80 can be reached from the internet (if not, the DNS provider and its credentials file) and whether to serve the panel at `https://yourdomain` with Nginx.
3. **The ports.** One port for the panel and IRC, 8443 by default (browsers get the panel and IRC clients the bouncer, with the same certificate), or one port each: 8443 for the panel and 6697 for IRC. With Nginx it only asks for the IRC port (6697). It refuses ports that another program is already using and, for the panel, ports that browsers will not open (6667 and 6697, the usual IRC ports, are among them).

With `--non-interactive`, or without a terminal, it asks nothing: it uses the options given and, without port options, puts the panel on 8443 and IRC on 6697. In every case it also adds port 6667 without SSL for the local network; `--plain-port` changes it or removes it.

At the end it shows a summary like this one (here, installed by IP with the default answers):

```
  ┌──────────────────────────────────────────────────────────────
  │  xBNC is installed and running
  │
  │  1. Open the panel:  https://203.0.113.10:8443  (same port as IRC)
  │     Your browser will warn about the certificate (it is self-signed):
  │     choose "Advanced" and "Continue" / "Accept the risk".
  │  2. Enter the setup code:  K7QM-2XPA-9DWE
  │     and create your administrator account.
  │  3. Add your networks in "Networks → Add network".
  │
  │  IRC client (HexChat, mIRC, WeeChat, irssi…):
  │    Server 203.0.113.10, port 8443 with SSL
  │    From your local network: server 203.0.113.10, port 6667 without SSL
  │    Server password: your user/your network@device:your password
  │    (the user and password of step 2 and the network of step 3).
  │    The panel gives it to you ready to copy in "Networks → your network", and
  │    "sudo xbncd info" shows it with your real users and networks.
  └──────────────────────────────────────────────────────────────

  Later:
    sudo xbncd info          panel address and ports
    sudo xbncd setup-code    setup code
```

Open the panel, enter the code and create the first administrator (it will be a superadministrator). The line “From your local network” is for a machine at home; on a VPS, ignore it. To see the address and ports again: `sudo xbncd info`; to see the code again: `sudo xbncd setup-code`.

### What the installer does

1. Asks the questions above and, if you use a domain, checks that its DNS points to this machine and tells you which ports to open on your router.
2. Finds the program: the ready-made one for this machine in `bin/` (only in the .zip of a release), checked against `bin/SHA256SUMS`, or it builds it (see below).
3. Installs what it needs with the system's package manager: `ca-certificates` and `openssl`, plus `certbot` with Let's Encrypt and `nginx` with `--nginx`.
4. Creates the unprivileged system user `xbnc` and the folders with their permissions.
5. Installs the binary in `/usr/local/bin/xbncd` and the documentation in `/usr/local/share/doc/xbnc`.
6. Creates the certificate: self-signed for 10 years, for the IP (or the domain) and the machine's other addresses, or from Let's Encrypt with automatic renewal. Each renewal copies the new certificate, restarts xBNC and reloads Nginx if it is used.
7. Writes `/etc/xbnc/xbnc.toml` with the version in its header, and creates the database and the secret key.
8. With `--nginx`, configures Nginx: redirection from port 80, the panel on 443, WebSockets and uploads of plugins and backups.
9. Opens the ports in ufw or firewalld: the IRC port, the panel port when it is a separate one, the DCC range (50000-50100) and 80 and 443 when they are needed. With ufw, port 6667 is only opened to private networks.
10. Installs the systemd service with isolation measures (`ProtectSystem=strict`, `NoNewPrivileges`, no capabilities, a system call filter) and starts it. It starts by itself at every boot and is restarted if it fails.

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

To change the ports of an existing installation, add `--reconfigure`: without it the installer keeps the configuration and refuses port options that would not apply. `--reconfigure` writes `xbnc.toml` again from the answers and options of that run, not from the current file: in a terminal it asks the questions again, and without a terminal (or with `--non-interactive`) ports you do not give take their default values (8443 for the panel and 6697 for IRC). Your own edits to `xbnc.toml` are in the copy it keeps (`xbnc.toml.bak-DATE`).

### Building from the source code

The installer does it by itself when there is no ready-made program for your machine (always with `git clone`), or with `--build`. It runs `scripts/prereqs.sh`, which:

1. Installs the C compiler, `make`, `pkg-config` and `curl` with the system's package manager: apt, dnf, yum, pacman, zypper or apk.
2. Installs Rust with rustup, the official installer, for the owner of the xbnc folder (the user who cloned or unpacked it), in their home folder (`~/.cargo`). The Rust packaged by many distributions is too old, so it is only used if it is at least version 1.89; if not, rustup is installed alongside it. If Rust was already installed with rustup but is old, it is updated.

Then it builds xBNC as that user (not as the user who ran sudo) with `cargo build --release --locked`, which uses exactly the versions of the libraries that were tested. It warns if the machine has less than 2 GB of memory and explains how to add swap.

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
| `/usr/local/share/doc/xbnc/` | The documentation, the changelog, the licence and the language files of the installed version. |
| `/etc/xbnc/xbnc.toml` | Process configuration (root:xbnc, 0640). |
| `/etc/xbnc/tls/` | TLS certificate: fullchain.pem and privkey.pem (root:xbnc, 0750). |
| `/etc/xbnc/locales/` | Extra languages and translation corrections (see [Translating xBNC](TRANSLATIONS.md#translations)). |
| `/var/lib/xbnc/` | Database and secret key (xbnc:xbnc, 0700). **Back up secret.key**: without it the stored passwords cannot be decrypted. |
| `/var/lib/xbnc/logs/` | Log files of the log module (folders 0700, files 0600). |
| `/var/lib/xbnc/plugins/` | Installed plugins (0600). In `dev/`, those of development mode. |
| `/var/lib/xbnc/backups/` | Backups made from the panel. |

The service runs as the unprivileged user `xbnc`. `sudo systemctl status xbnc`, `sudo systemctl restart xbnc`; `sudo systemctl reload xbnc` makes it re-read networks, channels and users changed from the command line, and the language files. The log: `sudo journalctl -u xbnc -f`.

### Using it away from home

- Forward the IRC port with SSL on your router to the xBNC machine (8443 if you chose one port for the panel and IRC, 6697 if you chose one each; the installer tells you which) and connect to your public IP or to a dynamic DNS name (DuckDNS and the like). Port 6667 need not be opened: xBNC refuses it from outside.
- With one port each, if you do not want to open the panel (8443) to the internet, manage it through an SSH tunnel: `ssh -L 8443:127.0.0.1:8443 user@your-public-ip` and open `https://localhost:8443`. With a single port, the panel and IRC go together: opening the port opens both.
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
# Installed with git clone: download the changes and build them
cd ~/xbnc
git pull
sudo ./scripts/update.sh --build

# Installed with the .zip: unpack the new one in an empty folder
mkdir ~/xbnc-new && cd ~/xbnc-new
unzip ~/xbnc-NEW-VERSION.zip && cd xbnc
sudo ./scripts/update.sh
```

`update.sh` takes the new program the same way as the installer (the ready-made one in `bin/`, or it builds it; `--build` forces building), checks it and the configuration, makes a backup, stops the service, swaps the binary and starts it. If it does not start, it goes back to the previous binary **and** to the database from before the update (a new binary may have upgraded the schema, and an old one refuses to open a newer schema). Nothing is ever updated on its own. With `--binary PATH` it installs another binary.

> After `git pull`, always use `--build`: without it, update.sh takes the program already built in `target/` by the previous installation, which is the old version.

If you also want to redo the configuration (for example, to move to a domain), run `sudo ./scripts/install.sh --reconfigure` afterwards.

### Uninstalling

```sh
sudo ./scripts/uninstall.sh            # removes the service and the binary; keeps /etc/xbnc and /var/lib/xbnc
sudo ./scripts/uninstall.sh --purge    # also deletes the configuration, the data and the xbnc user
```

> Port 6667 without SSL only accepts connections from your local network (private addresses, loopback, link-local, CGNAT/Tailscale and IPv6 ULA). From the internet xBNC refuses it with a message pointing to the port with SSL.

<a id="beginner-guide"></a>

## Step-by-step guide for beginners

This guide is for people who have never set up a server. It goes from zero to chatting through xBNC, one step at a time. Copy each command, paste it in the terminal and press Enter.

### What you need

- **A Linux server that is on all the time.** xBNC is persistent because it lives there and not on your computer. The easiest choice is a rented VPS (a small virtual server, a few euros a month) with Debian 12 or Ubuntu 24.04. A Raspberry Pi or a computer at home that is never switched off also works.
- **At least 1 GB of RAM.** Building xBNC needs about 2 GB counting swap, and step 4 shows how to add swap if you are short.
- **The server's IP address and access as root** (or a user with sudo). Your VPS provider gives you both when you create it.
- **An IRC client** on your computer or phone (HexChat, mIRC, WeeChat, irssi, Goguma…), or just the browser: the panel has a web chat.

### Step 1. Connect to the server

Open a terminal: on Windows, *PowerShell* or *Windows Terminal*; on macOS and Linux, *Terminal*. Connect with SSH using your server's IP (here 203.0.113.10 as an example):

```sh
ssh root@203.0.113.10
```

The first time it asks whether you trust the server: type `yes`. Then enter the password (nothing appears while you type; that is normal).

### Step 2. Create a normal user

Do not install as root: the installer builds xBNC as the owner of its folder, and it is safer to do it with a normal user. If your provider already gave you a user with sudo, skip this step.

```sh
# Debian and Ubuntu
adduser alice
usermod -aG sudo alice

# Fedora, Rocky, Alma and RHEL
adduser alice && passwd alice
usermod -aG wheel alice

# Then, on any system, become that user
su - alice
```

Choose your own name instead of `alice`. From now on, commands that start with `sudo` will ask for that user's password.

### Step 3. Install git

git is the program that downloads the code from GitHub.

```sh
sudo apt update && sudo apt install -y git      # Debian and Ubuntu
sudo dnf install -y git                         # Fedora, Rocky, Alma and RHEL
sudo pacman -S git                              # Arch
sudo zypper install git                         # openSUSE
```

Run only the line for your system.

### Step 4. Check the memory and the disk

```sh
free -h      # memory: add up the "total" of Mem and Swap
df -h ~      # disk: "Avail" must be at least 3G
```

If memory and swap together are less than 2 GB, add 2 GB of swap (a file on the disk that works as extra memory). It is only needed to build, but it does no harm to keep it:

```sh
sudo fallocate -l 2G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile
echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
```

### Step 5. Download xBNC

```sh
cd ~
git clone https://github.com/x-bnc/xbnc.git
cd xbnc
```

Now you have a folder called `xbnc` in your home folder with all the code, and you are inside it. Keep this folder: it is used to update.

### Step 6. Run the installer

```sh
sudo ./scripts/install.sh
```

The installer asks a few questions. If you are not sure, press Enter: it takes the value shown in brackets, which is the recommended one.

| Question | What to answer |
| --- | --- |
| Language | The language of the installer, which is also the default language of xBNC. Type the number of the language; 1, English, is the default. |
| Do you want to use a domain? | If you do not have a domain, answer no (the default): xBNC works with the IP. If you have one (bnc.example.org) and it already points to this server, answer yes and write it. |
| Request a free Let's Encrypt certificate? | Only with a domain. Yes gets a certificate that browsers and clients trust, with no warnings. It asks for an e-mail for its notices. |
| Can port 80 of this machine be reached from the internet? | Only with Let's Encrypt, which checks the domain through port 80. If you cannot open it, answer no: it then asks for your DNS provider (Cloudflare, DigitalOcean, OVH, Linode or RFC 2136) and its credentials file. |
| Serve the panel at https://DOMAIN with Nginx? | Only with a domain and Let's Encrypt. Yes puts the panel at a normal address without a port number. |
| Use one port for the panel and IRC? | Yes (the default): the panel and IRC clients share port 8443. No: one port for each (8443 for the panel and 6697 for IRC). With Nginx this question is not asked: the panel goes on 443 and it only asks for the IRC port (6697). |
| Port… | Press Enter to take the suggested port. If it is taken by another program, the installer says so and asks for another one. |

Then it works by itself: it installs the build tools and Rust, shows *Building xBNC as alice (it takes a few minutes)* and, when it finishes, creates the certificate, opens the ports in the firewall and installs and starts the service. At the end it shows a summary like this one:

```
  ┌──────────────────────────────────────────────────────────────
  │  xBNC is installed and running
  │
  │  1. Open the panel:  https://203.0.113.10:8443  (same port as IRC)
  │     Your browser will warn about the certificate (it is self-signed):
  │     choose "Advanced" and "Continue" / "Accept the risk".
  │  2. Enter the setup code:  K7QM-2XPA-9DWE
  │     and create your administrator account.
  │  3. Add your networks in "Networks → Add network".
  │
  │  IRC client (HexChat, mIRC, WeeChat, irssi…):
  │    Server 203.0.113.10, port 8443 with SSL
  │    From your local network: server 203.0.113.10, port 6667 without SSL
  │    Server password: your user/your network@device:your password
  │    (the user and password of step 2 and the network of step 3).
  │    The panel gives it to you ready to copy in "Networks → your network", and
  │    "sudo xbncd info" shows it with your real users and networks.
  └──────────────────────────────────────────────────────────────

  Later:
    sudo xbncd info          panel address and ports
    sudo xbncd setup-code    setup code
```

Write down the panel address, the code and the IRC port. On a VPS, ignore the line “From your local network”: it is for a machine at home. If you lose them: `sudo xbncd info` shows the panel address, the IRC ports and the server password of each network, and `sudo xbncd setup-code` shows the code.

### Step 7. Open the panel and create your account

1. Open the panel address in your browser.
2. If you installed with the IP, the browser warns that the connection is not private. That is expected: your own server created the certificate, not an authority. Choose *Advanced* and continue to the site. With a domain and Let's Encrypt there is no warning.
3. Enter the code and create your user and password. This first account is the superadministrator.
4. Recommended: turn on two-step verification in *Your account*.

### Step 8. Add your IRC network

In *Networks → Add network*, choose your network from the list (or write its server, for example `irc.example.org`), your nick, how you identify yourself and the channels, for example `#xbnc`. As soon as you save it, xBNC connects to the network and joins the channels, even with no client connected. See [First steps](#first-steps).

### Step 9. Connect your IRC client

In the panel, *Networks → your network → Connect your IRC client* shows your exact details, with steps for HexChat, mIRC, WeeChat and irssi. In short:

- **Server:** the IP or domain of the summary. **Port:** the one in the line “port … with SSL” of the summary (8443 if you chose one port, 6697 if you chose one each), with SSL turned on.
- **Server password:** `alice/network:your-password`, with your user, the name of the network in xBNC and the panel password.
- If you installed with the IP, turn on “accept invalid certificates” in the client, for the same reason as in step 7.

More details in [Connecting your IRC client](CLIENTS.md#clients).

### Step 10. Check that it is persistent

Close the IRC client, or switch off the computer, and come back after a while. Your nick was still in the channels, and when you connect again you receive the messages you missed. That is what a bouncer does.

### Useful commands for later

| To… | Run |
| --- | --- |
| See whether xBNC is running | `sudo systemctl status xbnc` |
| See the panel address, the IRC ports and the server passwords | `sudo xbncd info` |
| See the installed version | `xbncd --version` |
| See the last messages of the log | `sudo journalctl -u xbnc -n 50` |
| Restart xBNC | `sudo systemctl restart xbnc` |
| Update to the latest version | `cd ~/xbnc && git pull && sudo ./scripts/update.sh --build` |
| Make a backup | `cd ~/xbnc && sudo ./scripts/backup.sh` |

If something fails, look in [Common problems](TROUBLESHOOTING.md#troubleshooting). Every script is explained in [The scripts folder](#scripts).

<a id="scripts"></a>

## The scripts folder

The `scripts/` folder of the code has everything needed to install and look after xBNC. Run them from the `xbnc` folder, with `sudo`.

| Script | What it is for | How to use it |
| --- | --- | --- |
| `install.sh` | Installs xBNC: the program, the system user, the service, the certificate, the configuration and the firewall. Also changes an existing installation with `--reconfigure`. | `sudo ./scripts/install.sh`<br>Options in [Installation](#install). |
| `update.sh` | Updates to a new version: checks the new program and the configuration, makes a backup, swaps the binary and restarts. If xBNC does not start, it goes back to the previous version by itself. It only runs when you call it. | `sudo ./scripts/update.sh` with the .zip.<br>`git pull` and then `sudo ./scripts/update.sh --build` with git: --build builds the new code. |
| `backup.sh` | Makes a consistent backup while xBNC is running: the database, the secret key and `/etc/xbnc`, in a .tar.gz in `/var/backups/xbnc`. The file has permissions 0600 because it contains credentials: keep it in a safe place. | `sudo ./scripts/backup.sh [FOLDER] [--keep N]`<br>`--keep` keeps only the last N (14 by default). |
| `restore.sh` | Restores a backup made with backup.sh. Before replacing anything it keeps the current database and key as `*.pre-restore-DATE`. | `sudo ./scripts/restore.sh FILE [--with-config] [--yes]`<br>`--with-config` also restores `/etc/xbnc`. |
| `uninstall.sh` | Removes the service and the program. By default it keeps the configuration and the data, in case you install again. | `sudo ./scripts/uninstall.sh`<br>`--purge` also deletes the configuration, the data and the xbnc user, after asking you to type `delete` (`--purge --yes` does not ask). The backups in /var/backups/xbnc are kept. |
| `prereqs.sh` | Installs what is needed to build: the C compiler, make, pkg-config, curl and Rust. The installer and update.sh run it by themselves when they have to build. | `./scripts/prereqs.sh --check` only says what is missing.<br>`sudo ./scripts/prereqs.sh` installs it. |
| `binary.sh` | Finds the program to install (the ready-made one in `bin/` of the .zip) or builds it, and checks the memory and disk first. It is part of install.sh and update.sh. | It is not run by hand. |
| `i18n.sh` | Loads the installer texts from the language files in `locales/`, so the scripts speak your language. It is part of the other scripts. | It is not run by hand. |

> Backups contain the secret key and the configuration with credentials. Do not share them and do not leave them in a public folder.

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
sudo -u xbnc xbncd network add admin example --host irc.example.org --port 6697 --sasl-user myaccount
sudo -u xbnc xbncd channel add admin example '#xbnc'
sudo systemctl reload xbnc
```

The SASL password is asked for at that moment (or read from `XBNC_SASL_PASSWORD`) and stored encrypted.
