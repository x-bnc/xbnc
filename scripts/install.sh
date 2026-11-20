#!/usr/bin/env bash
# Installs xBNC as a systemd service on any Linux with systemd (Debian,
# Ubuntu, Fedora, RHEL/Rocky/Alma, Arch, openSUSE…), ready to use:
# certificate, web panel, IRC ports and firewall included.
#
# Nothing has to be installed first. The binary comes ready-made in bin/ for
# 64-bit PCs and servers (x86_64); on other machines, or with --build, the
# installer installs what is needed (build tools and Rust, see prereqs.sh)
# and builds it.
#
#   unzip xbnc-1.0.0.zip && cd xbnc
#   sudo ./scripts/install.sh                  # by IP address (auto-detected)
#   sudo ./scripts/install.sh --domain bnc.example.org --letsencrypt you@example.org
#   sudo ./scripts/install.sh --domain bnc.example.org --letsencrypt you@example.org --nginx
#   sudo ./scripts/install.sh --reconfigure --domain ...   # an existing install
# Run with no options in a terminal, it asks whether to use a domain.
#
# Layout:
#   /usr/local/bin/xbncd            binary (/usr/bin/xbncd with the .deb/.rpm)
#   /etc/xbnc/xbnc.toml             configuration      (root:xbnc 0640)
#   /etc/xbnc/tls/                  TLS certificate    (root:xbnc 0750)
#   /etc/xbnc/locales/              extra languages    (root:xbnc 0750, see TRANSLATING.md)
#   /var/lib/xbnc/                  database + secret  (xbnc:xbnc 0700)
#
# The service runs as the unprivileged system user `xbnc`.
set -euo pipefail

BINARY=""
FORCE_BUILD=0
START=1
DOMAIN=""
ADDRESS=""
LE_EMAIL=""
LE_DNS=""
LE_DNS_CREDS=""
PROXY=0
WEB_PORT=8443
IRC_PORT=6697
PLAIN_PORT=6667
SINGLE=0
DCC_PORTS="50000-50100"
PORTS_GIVEN=0
FIREWALL=1
RECONFIGURE=0
NGINX=0
INTERACTIVE=auto
SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/i18n.sh
. "$SRC_DIR/scripts/i18n.sh"

# Language of the installer, which is also xBNC's default language. English
# unless chosen: with --language CODE, from a menu of the language files
# (locales/*.json) when run in a terminal, or the one of the existing
# configuration when reinstalling without a terminal.
LANGUAGE=""
ASK_LANGUAGE=1
HELP_ONLY=0
prev=""
for a in "$@"; do
    case "$a" in
        --non-interactive|--yes|-y) ASK_LANGUAGE=0 ;;
        -h|--help) ASK_LANGUAGE=0; HELP_ONLY=1 ;;
    esac
    if [[ $prev == --language ]]; then LANGUAGE="$a"; fi
    prev="$a"
done
if [[ -n "$LANGUAGE" ]]; then
    if ! xbnc_language_exists "$LANGUAGE"; then
        echo "Error: unknown language \"$LANGUAGE\". Available: $(xbnc_languages | cut -f1 | tr '\n' ' ')" >&2
        exit 2
    fi
elif [[ $ASK_LANGUAGE -eq 1 && -t 0 && -t 1 && -e /dev/tty ]]; then
    LANGUAGE="$(xbnc_choose_language)"
elif [[ $HELP_ONLY -eq 0 && -f /etc/xbnc/xbnc.toml ]]; then
    LANGUAGE="$(sed -n 's/^language *= *"\([A-Za-z-]*\)".*/\1/p' /etc/xbnc/xbnc.toml | head -n1)"
    xbnc_language_exists "${LANGUAGE:-x}" || LANGUAGE=en
else
    LANGUAGE=en
fi
xbnc_load_texts "$LANGUAGE"

usage() {
    t usage_title "$0"
    echo
    t usage_intro "$WEB_PORT" "$IRC_PORT" "$PLAIN_PORT"
    echo
    local o
    for o in "--domain|opt_domain" "--letsencrypt|opt_letsencrypt" "--dns|opt_dns" "--dns-credentials|opt_dns_credentials" "--ip|opt_ip" "--language|opt_language" \
             "--nginx|opt_nginx" "--behind-proxy|opt_behind_proxy" "--web-port|opt_web_port" "--irc-port|opt_irc_port" \
             "--plain-port|opt_plain_port" "--single-port|opt_single_port" "--dcc-ports|opt_dcc_ports" "--no-firewall|opt_no_firewall" "--reconfigure|opt_reconfigure" \
             "--binary|opt_binary" "--build|opt_build" "--no-start|opt_no_start" "--non-interactive|opt_non_interactive"; do
        printf '  %s %s\n' "$(pad 24 "${o%%|*} $(t "${o##*|}_arg")")" "$(t "${o##*|}" "$WEB_PORT" "$IRC_PORT" "$PLAIN_PORT")"
    done
}


while [[ $# -gt 0 ]]; do
    case "$1" in
        --binary) BINARY="$2"; shift 2 ;;
        --build) FORCE_BUILD=1; shift ;;
        --domain|--hostname) DOMAIN="$2"; shift 2 ;;
        --ip) ADDRESS="$2"; shift 2 ;;
        --letsencrypt) LE_EMAIL="$2"; shift 2 ;;
        --dns) LE_DNS="$2"; shift 2 ;;
        --dns-credentials) LE_DNS_CREDS="$2"; shift 2 ;;
        --behind-proxy) PROXY=1; shift ;;
        --nginx) NGINX=1; PROXY=1; shift ;;
        --non-interactive|--yes|-y) INTERACTIVE=0; shift ;;
        --web-port) WEB_PORT="$2"; PORTS_GIVEN=1; shift 2 ;;
        --irc-port) IRC_PORT="$2"; PORTS_GIVEN=1; shift 2 ;;
        --single-port) SINGLE=1; IRC_PORT="$2"; WEB_PORT="$2"; PORTS_GIVEN=1; shift 2 ;;
        --dcc-ports) DCC_PORTS="$2"; shift 2 ;;
        --plain-port) PLAIN_PORT="$2"; shift 2 ;;
        --no-firewall) FIREWALL=0; shift ;;
        --reconfigure) RECONFIGURE=1; shift ;;
        --language) shift 2 ;;
        --no-start) START=0; shift ;;
        --packaged) PACKAGED=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) say unknown_option "$1" >&2; usage; exit 2 ;;
    esac
done

is_ip() { [[ "$1" =~ ^[0-9]+(\.[0-9]+){3}$ || "$1" == *:* ]]; }
valid_port() { [[ "$1" =~ ^[0-9]+$ ]] && (( $1 >= 1 && $1 <= 65535 )); }
# Ports browsers refuse to open (Chrome, Firefox, Safari): the panel cannot
# use them. 6667 and 6697, the usual IRC ports, are among them.
BROWSER_BLOCKED=" 1 7 9 11 13 15 17 19 20 21 22 23 25 37 42 43 53 69 77 79 87 95 101 102 103 104 109 110 111 113 115 117 119 123 135 137 139 143 161 179 389 427 465 512 513 514 515 526 530 531 532 540 548 554 556 563 587 601 636 989 990 993 995 1719 1720 1723 2049 3659 4045 4190 5060 5061 6000 6566 6665 6666 6667 6668 6669 6679 6697 10080 "
browser_blocked() { [[ "$BROWSER_BLOCKED" == *" $1 "* ]]; }
# A port another program already listens on (xBNC itself does not count).
port_busy() {
    command -v ss >/dev/null || return 1
    local l
    l="$(ss -ltnpH "sport = :$1" 2>/dev/null)"
    [[ -n "$l" && "$l" != *xbncd* ]]
}
valid_dcc() {
    [[ "$1" == off ]] && return 0
    [[ "$1" =~ ^([0-9]+)-([0-9]+)$ ]] || return 1
    local a="${BASH_REMATCH[1]}" b="${BASH_REMATCH[2]}"
    valid_port "$a" && valid_port "$b" && (( a >= 1024 && a <= b && b - a < 10000 ))
}

[[ $EUID -eq 0 ]] || die run_it_as_root_sudo "$0"
command -v systemctl >/dev/null || die systemd_is_required
valid_port "$WEB_PORT" || die invalid_web_port "$WEB_PORT"
valid_port "$IRC_PORT" || die invalid_irc_port "$IRC_PORT"
valid_dcc "$DCC_PORTS" || die invalid_dcc_ports "$DCC_PORTS"
[[ "$PLAIN_PORT" == off ]] || valid_port "$PLAIN_PORT" || die invalid_plain_port "$PLAIN_PORT"
[[ -z "$LE_EMAIL" || -n "$DOMAIN" ]] || die letsencrypt_needs_domain
# Let's Encrypt by DNS (no port 80): providers with an official certbot
# plugin that reads a credentials file.
DNS_PROVIDERS="cloudflare digitalocean ovh linode rfc2136"
check_dns() {
    [[ -z "$LE_DNS" ]] && return 0
    [[ " $DNS_PROVIDERS " == *" $LE_DNS "* ]] || die unknown_dns_provider "$LE_DNS" "$DNS_PROVIDERS"
    [[ -n "$LE_EMAIL" ]] || die dns_needs_letsencrypt
    [[ -f "$LE_DNS_CREDS" ]] || die dns_credentials_missing "$LE_DNS_CREDS"
}
check_dns

# ---------------------------------------------------------- questions

# With a terminal and no --domain, ask how it will be reached.
ask() { # ask VAR KEY [ARG...]: a free answer
    local __v="$1" reply
    shift
    printf '%s ' "$(t "$@")" >/dev/tty
    IFS= read -r reply </dev/tty || reply=""
    printf -v "$__v" '%s' "$reply"
}
yes_no() { # yes_no KEY y|n [ARG...]: yes or no, with a default
    local key="$1" def="$2" r y n
    shift 2
    y="$(t yes_short)" n="$(t no_short)"
    printf '%s %s [%s] ' "$(t "$key" "$@")" "$(t yes_no_hint)" "$([[ $def == y ]] && echo "$y" || echo "$n")" >/dev/tty
    IFS= read -r r </dev/tty || r=""
    [[ -n "$r" ]] || { [[ $def == y ]]; return; }
    [[ "${r,,}" == "${y,,}"* || "${r,,}" == y* ]]
}
ask_port() { # ask_port VAR KEY DEFAULT web|irc: a free port, with a default
    local __v="$1" key="$2" def="$3" kind="$4" p
    while :; do
        printf '%s [%s] ' "$(t "$key")" "$def" >/dev/tty
        IFS= read -r p </dev/tty || p=""
        p="${p:-$def}"
        if ! valid_port "$p"; then say invalid_port "$p"; continue; fi
        if [[ $kind == web ]] && browser_blocked "$p"; then say port_blocked_by_browsers "$p"; continue; fi
        if [[ $kind == irc && "$PLAIN_PORT" == "$p" ]]; then say port_in_use "$p"; continue; fi
        if port_busy "$p"; then say port_in_use "$p"; continue; fi
        break
    done
    printf -v "$__v" '%s' "$p"
}
if [[ $INTERACTIVE == auto ]]; then
    if [[ -t 0 && -t 1 && -z "$DOMAIN" && -z "$ADDRESS" && -e /dev/tty ]]; then INTERACTIVE=1; else INTERACTIVE=0; fi
fi
if [[ $INTERACTIVE -eq 1 ]]; then
    echo
    say xbnc_can_be_used_through
    say for_example_bnc_yourdomain_com
    if yes_no do_you_want_to_use n; then
        while [[ -z "$DOMAIN" ]]; do
            ask DOMAIN domain_it_must_already_point
        done
        if yes_no request_a_free_let_s y; then
            while [[ -z "$LE_EMAIL" ]]; do
                ask LE_EMAIL e_mail_for_let_s
            done
            if ! yes_no can_port_80_be_opened y; then
                say dns_validation_intro "$DNS_PROVIDERS"
                while [[ " $DNS_PROVIDERS " != *" $LE_DNS "* ]]; do
                    ask LE_DNS dns_provider
                done
                while [[ ! -f "$LE_DNS_CREDS" ]]; do
                    ask LE_DNS_CREDS dns_credentials_file "$LE_DNS"
                done
            fi
            if yes_no serve_the_panel_at_https y "$DOMAIN" "$WEB_PORT"; then
                NGINX=1; PROXY=1
            fi
        fi
        if [[ -f /etc/xbnc/xbnc.toml && $RECONFIGURE -eq 0 ]]; then
            if yes_no there_is_already_a_configuration y; then
                RECONFIGURE=1
            fi
        fi
    fi
    # Ports: one for the panel and IRC, or one for each (only when the
    # configuration is written now; a kept one keeps its ports).
    if [[ -f /etc/xbnc/xbnc.toml && $RECONFIGURE -eq 0 ]]; then
        :
    elif [[ $PROXY -eq 0 && $PORTS_GIVEN -eq 0 ]]; then
        echo
        say ports_intro
        if yes_no use_one_port y; then
            SINGLE=1
            ask_port IRC_PORT port_for_panel_and_irc 8443 web
            WEB_PORT="$IRC_PORT"
        else
            ask_port WEB_PORT port_for_panel "$WEB_PORT" web
            ask_port IRC_PORT port_for_irc "$IRC_PORT" irc
        fi
    elif [[ $PROXY -eq 1 && $PORTS_GIVEN -eq 0 ]]; then
        ask_port IRC_PORT port_for_irc "$IRC_PORT" irc
    fi
    echo
fi
if [[ $SINGLE -eq 1 && $PROXY -eq 1 ]]; then die single_port_and_proxy; fi
if [[ $PORTS_GIVEN -eq 1 && -f /etc/xbnc/xbnc.toml && $RECONFIGURE -eq 0 ]]; then die ports_need_reconfigure; fi
if [[ $SINGLE -eq 0 && $WEB_PORT == "$IRC_PORT" && $PROXY -eq 0 ]]; then die same_port_twice "$WEB_PORT"; fi
if [[ $PROXY -eq 0 ]] && browser_blocked "$WEB_PORT"; then die port_blocked_by_browsers "$WEB_PORT"; fi
[[ $NGINX -eq 0 || -n "$DOMAIN" ]] || die nginx_needs_domain
[[ $NGINX -eq 0 || -n "$LE_EMAIL" ]] || die nginx_needs_letsencrypt_email
if [[ -n "$DOMAIN" ]] && is_ip "$DOMAIN"; then
    # --hostname 192.168.1.50 (v0.2 syntax) means "install by IP".
    ADDRESS="$DOMAIN"; DOMAIN=""
fi

# The address users type: the domain, or this machine's main IP address.
detect_ip() {
    local ip=""
    if command -v ip >/dev/null; then
        ip="$(ip -4 route get 1.1.1.1 2>/dev/null | awk '{for (i = 1; i < NF; i++) if ($i == "src") { print $(i + 1); exit }}')"
    fi
    [[ -n "$ip" ]] || ip="$(hostname -I 2>/dev/null | awk '{print $1}')"
    echo "$ip"
}
if [[ -n "$DOMAIN" ]]; then
    PUBLIC="$DOMAIN"
else
    [[ -n "$ADDRESS" ]] || ADDRESS="$(detect_ip)"
    [[ -n "$ADDRESS" ]] || die cannot_detect_this_machine_s
    PUBLIC="$ADDRESS"
fi
URL_HOST="$PUBLIC"
[[ "$PUBLIC" == *:* ]] && URL_HOST="[$PUBLIC]"

# Installed from a .deb/.rpm: the package put the binary in /usr/bin and the
# unit in the system unit directory; this script only configures.
PACKAGED="${PACKAGED:-0}"
if [[ "$PACKAGED" == 1 ]]; then
    BIN=/usr/bin/xbncd
    BINARY="$BIN"
else
    BIN=/usr/local/bin/xbncd
    step finding_the_program
    # shellcheck source=scripts/binary.sh
    . "$SRC_DIR/scripts/binary.sh"
    xbnc_get_binary "$FORCE_BUILD"
fi
[[ -x "$BINARY" ]] || die binary_not_found "$BINARY"
"$BINARY" --version >/dev/null || die does_not_run_on_this "$BINARY"

if [[ "$PACKAGED" != 1 || -n "$LE_EMAIL" ]]; then
step installing_dependencies
PKGS="ca-certificates openssl"
[[ -n "$LE_EMAIL" ]] && PKGS="$PKGS certbot curl"
[[ -n "$LE_DNS" ]] && PKGS="$PKGS python3-certbot-dns-$LE_DNS"
[[ $NGINX -eq 1 ]] && PKGS="$PKGS nginx"
if command -v apt-get >/dev/null; then
    # shellcheck disable=SC2086
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends $PKGS >/dev/null
elif command -v dnf >/dev/null; then
    # shellcheck disable=SC2086
    dnf install -y $PKGS >/dev/null
elif command -v yum >/dev/null; then
    # shellcheck disable=SC2086
    yum install -y $PKGS >/dev/null
elif command -v pacman >/dev/null; then
    # shellcheck disable=SC2086
    pacman -Sy --needed --noconfirm $PKGS >/dev/null
elif command -v zypper >/dev/null; then
    # shellcheck disable=SC2086
    zypper --non-interactive --quiet install $PKGS
else
    note 4 unknown_package_manager_make_sure "$PKGS"
fi
fi

step creating_the_xbnc_system_user
if ! id xbnc >/dev/null 2>&1; then
    useradd --system --home-dir /var/lib/xbnc --shell /usr/sbin/nologin --user-group xbnc
fi

WAS_RUNNING=0
systemctl is-active --quiet xbnc 2>/dev/null && WAS_RUNNING=1

if [[ "$PACKAGED" != 1 ]]; then
    step installing_the_binary_to "$BIN"
    install -m 0755 "$BINARY" "$BIN"
fi

step creating_directories
install -d -m 0750 -o root -g xbnc /etc/xbnc /etc/xbnc/tls /etc/xbnc/locales
install -d -m 0700 -o xbnc -g xbnc /var/lib/xbnc

# A language that is not built in (English and Spanish are) comes as a file:
# xBNC reads it from /etc/xbnc/locales.
if [[ "$LANGUAGE" != en && "$LANGUAGE" != es && -f "$SRC_DIR/locales/$LANGUAGE.json" ]]; then
    LANG_NAME="$(xbnc_languages | awk -F'\t' -v c="$LANGUAGE" '$1 == c { print $2 }')"
    step installing_language "$LANG_NAME" "$LANGUAGE"
    install -m 0640 -o root -g xbnc "$SRC_DIR/locales/$LANGUAGE.json" "/etc/xbnc/locales/$LANGUAGE.json"
fi

STAMP="$(date +%Y%m%d%H%M%S)"
CERT=/etc/xbnc/tls/fullchain.pem
KEY=/etc/xbnc/tls/privkey.pem

# ---------------------------------------------------------- certificate

self_signed() {
    [[ -f "$CERT" ]] || return 1
    [[ "$(openssl x509 -in "$CERT" -noout -subject -nameopt RFC2253 2>/dev/null | sed 's/^subject=//')" == \
       "$(openssl x509 -in "$CERT" -noout -issuer -nameopt RFC2253 2>/dev/null | sed 's/^issuer=//')" ]]
}

make_self_signed() {
    local san cn="$PUBLIC" extra
    if [[ -n "$DOMAIN" ]]; then
        san="DNS:$DOMAIN,DNS:localhost,IP:127.0.0.1"
    else
        san="IP:$ADDRESS,IP:127.0.0.1,DNS:localhost"
    fi
    # Also cover the machine's other addresses (the local network one, so
    # clients at home can use it too) and its short name.
    for extra in $(hostname -I 2>/dev/null); do
        [[ "$extra" == "$ADDRESS" ]] || san="$san,IP:$extra"
    done
    extra="$(hostname -s 2>/dev/null || true)"
    [[ -n "$extra" && "$extra" != localhost ]] && san="$san,DNS:$extra"
    step generating_a_self_signed_certificate "$PUBLIC"
    rm -f "$CERT" "$KEY"
    openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -days 3650 \
        -subj "/CN=$cn" -addext "subjectAltName=$san" \
        -keyout "$KEY" -out "$CERT" 2>/dev/null
    chown root:xbnc "$CERT" "$KEY"
    chmod 0640 "$KEY"
    chmod 0644 "$CERT"
}

# ---------------------------------------------------------- domain checks

public_ip() {
    local ip="" u
    command -v curl >/dev/null || return 0
    for u in https://api.ipify.org https://ifconfig.me https://icanhazip.com; do
        ip="$(curl -4 -fsS --max-time 5 "$u" 2>/dev/null || true)"
        if [[ "$ip" =~ ^[0-9]+(\.[0-9]+){3}$ ]]; then echo "$ip"; return 0; fi
    done
}
if [[ -n "$DOMAIN" ]]; then
    RESOLVED="$(getent ahostsv4 "$DOMAIN" 2>/dev/null | awk 'NR==1{print $1}' || true)"
    MYIP="$(public_ip)"
    if [[ -z "$RESOLVED" ]]; then
        note 4 warning_does_not_resolve_yet "$DOMAIN" "${MYIP:+ ($MYIP)}"
    elif [[ -n "$MYIP" && "$RESOLVED" != "$MYIP" ]]; then
        note 4 warning_points_to_but_this "$DOMAIN" "$RESOLVED" "$MYIP"
    fi
    LOCALIP="$(detect_ip)"
    if [[ -n "$LOCALIP" && -n "$MYIP" && "$LOCALIP" != "$MYIP" ]]; then
        note 4 this_machine_is_behind_a "$LOCALIP"
        if [[ $NGINX -eq 1 ]]; then
            note 6 80_and_443_certificate_and "$IRC_PORT"
        else
            if [[ -n "$LE_EMAIL" && -z "$LE_DNS" ]]; then
                note 6 ports_cert_panel_irc "$WEB_PORT" "$IRC_PORT"
            else
                note 6 ports_panel_irc "$WEB_PORT" "$IRC_PORT"
            fi
        fi
        if [[ $INTERACTIVE -eq 1 && -n "$LE_EMAIL" ]]; then
            ask _ press_enter_when_they_are
        fi
    fi
fi

if [[ -n "$LE_EMAIL" ]]; then
    step requesting_a_let_s_encrypt "$DOMAIN"
    if [[ -z "$LE_DNS" ]] && command -v ufw >/dev/null && ufw status 2>/dev/null | grep -q "Status: active" && [[ $FIREWALL -eq 1 ]]; then
        ufw allow 80/tcp >/dev/null
    fi
    LE_AUTH=(--standalone)
    if [[ -n "$LE_DNS" ]]; then
        # By DNS: certbot writes a TXT record with the provider's API, so
        # port 80 is not needed (renewals do the same).
        install -d -m 0700 /etc/letsencrypt/xbnc
        install -m 0600 "$LE_DNS_CREDS" "/etc/letsencrypt/xbnc/$LE_DNS.ini"
        LE_AUTH=("--dns-$LE_DNS" "--dns-$LE_DNS-credentials" "/etc/letsencrypt/xbnc/$LE_DNS.ini")
    fi
    # Something (Nginx) already on port 80: answer the challenge through it.
    if [[ -z "$LE_DNS" ]] && command -v nginx >/dev/null && systemctl is-active --quiet nginx 2>/dev/null; then
        install -d -m 0755 /var/www/letsencrypt
        if [[ ! -f /etc/nginx/sites-available/xbnc && ! -f /etc/nginx/conf.d/xbnc.conf ]]; then
            cat > /etc/nginx/conf.d/xbnc-acme.conf <<ACME
# Let's Encrypt challenges for $DOMAIN (written by xBNC's install.sh).
server {
    listen 80;
    listen [::]:80;
    server_name $DOMAIN;
    location /.well-known/acme-challenge/ { root /var/www/letsencrypt; }
}
ACME
            nginx -t >/dev/null 2>&1 && systemctl reload nginx
        fi
        LE_AUTH=(--webroot -w /var/www/letsencrypt)
    fi
    certbot certonly "${LE_AUTH[@]}" --non-interactive --agree-tos --keep-until-expiring -m "$LE_EMAIL" -d "$DOMAIN" \
        || die let_s_encrypt_could_not "$DOMAIN"
    install -d -m 0755 /etc/letsencrypt/renewal-hooks/deploy
    cat > /etc/letsencrypt/renewal-hooks/deploy/xbnc.sh <<EOF
#!/bin/sh
# Copies the renewed certificate for xBNC (installed by xBNC's install.sh).
set -e
for d in \$RENEWED_DOMAINS; do [ "\$d" = "$DOMAIN" ] && ok=1; done
[ -n "\$ok" ] || exit 0
install -m 0644 -o root -g xbnc "\$RENEWED_LINEAGE/fullchain.pem" $CERT
install -m 0640 -o root -g xbnc "\$RENEWED_LINEAGE/privkey.pem" $KEY
systemctl restart xbnc
systemctl reload nginx 2>/dev/null || true
EOF
    chmod 0755 /etc/letsencrypt/renewal-hooks/deploy/xbnc.sh
    install -m 0644 -o root -g xbnc "/etc/letsencrypt/live/$DOMAIN/fullchain.pem" "$CERT"
    install -m 0640 -o root -g xbnc "/etc/letsencrypt/live/$DOMAIN/privkey.pem" "$KEY"
elif [[ ! -f "$CERT" ]] || { [[ $RECONFIGURE -eq 1 ]] && self_signed; }; then
    make_self_signed
fi

# ---------------------------------------------------------- configuration

write_config() {
    local web plain="" dcc=""
    if [[ "$DCC_PORTS" != off ]]; then
        dcc="
# Ports for DCC transfers through xBNC (opened in the firewall).
[dcc]
ports = \"$DCC_PORTS\"
"
    fi
    if [[ $SINGLE -eq 1 ]]; then
        web="[web]
# On the IRC port: browsers get the panel (https://$URL_HOST:$IRC_PORT) and
# IRC clients the bouncer, with the same certificate.
address = \"0.0.0.0:$IRC_PORT\"
shared = true
session_hours = 12"
        if [[ -n "$DOMAIN" ]]; then
            web="$web
public_url = \"https://$URL_HOST:$IRC_PORT\""
        fi
    elif [[ $PROXY -eq 1 ]]; then
        web="[web]
# Published by a TLS reverse proxy (Nginx: /etc/nginx/sites-available/xbnc).
address = \"127.0.0.1:8080\"
public_url = \"https://$URL_HOST\"
secure_cookies = true
trust_proxy = true
session_hours = 12"
    else
        web="[web]
# Served directly with TLS. Open https://$URL_HOST:$WEB_PORT
address = \"0.0.0.0:$WEB_PORT\"
tls_cert = \"$CERT\"
tls_key = \"$KEY\"
session_hours = 12"
        if [[ -n "$DOMAIN" ]]; then
            web="$web
public_url = \"https://$URL_HOST:$WEB_PORT\""
        fi
    fi
    if [[ "$PLAIN_PORT" != off ]]; then
        plain="
# Without SSL, for clients on the local network only (others are refused
# with a message telling them to use port $IRC_PORT with SSL).
[[listen]]
address = \"0.0.0.0:$PLAIN_PORT\"
lan_only = true
"
    fi
    cat <<EOF
# xBNC configuration, generated by install.sh ($("$BINARY" --version 2>/dev/null | awk '{print "xBNC " $2}')) on $(date -u +%Y-%m-%d).
# Users, networks, channels and most settings are managed in the web panel.
# Changes here need:  sudo xbncd check-config && sudo systemctl restart xbnc

# Server name shown to IRC clients.
hostname = "$PUBLIC"

# Default language for messages and the panel (users can pick their own).
language = "$LANGUAGE"

# Key that encrypts stored credentials. BACK IT UP (scripts/backup.sh does).
secret_key_file = "/var/lib/xbnc/secret.key"

[database]
path = "/var/lib/xbnc/xbnc.db"

# IRC clients, with SSL/TLS.
[[listen]]
address = "0.0.0.0:$IRC_PORT"
tls_cert = "$CERT"
tls_key = "$KEY"
$plain
$web
$dcc

# Defaults; the panel (Ajustes) can override them without restarting.
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
EOF
}

CONFIG=/etc/xbnc/xbnc.toml
if [[ ! -f "$CONFIG" || $RECONFIGURE -eq 1 ]]; then
    if [[ -f "$CONFIG" ]]; then
        cp -p "$CONFIG" "$CONFIG.bak-$STAMP"
        note 4 previous_configuration_saved_as_bak "$CONFIG" "$STAMP"
    fi
    step writing "$CONFIG"
    write_config > "$CONFIG.new"
    chown root:xbnc "$CONFIG.new"
    chmod 0640 "$CONFIG.new"
    mv "$CONFIG.new" "$CONFIG"
else
    step keeping_the_existing_configuration_use "$CONFIG"
    # The language chosen now becomes xBNC's default language.
    if grep -q '^language *= *"' "$CONFIG" && ! grep -q "^language *= *\"$LANGUAGE\"" "$CONFIG"; then
        sed -i "s/^language *= *\"[A-Za-z-]*\"/language = \"$LANGUAGE\"/" "$CONFIG"
        note 4 default_language_set "$LANGUAGE"
    fi
fi
$BIN --config "$CONFIG" check-config >/dev/null \
    || die the_configuration_is_not_valid "$CONFIG"

step preparing_the_database_and_secret
runuser -u xbnc -- env XBNC_CONFIG="$CONFIG" $BIN init >/dev/null

if [[ "$PACKAGED" != 1 ]]; then
    step installing_the_documentation
    install -d /usr/local/share/doc/xbnc
    cp -r "$SRC_DIR/README.md" "$SRC_DIR/CHANGELOG.md" "$SRC_DIR/LICENSE" "$SRC_DIR/TRANSLATING.md" "$SRC_DIR/docs" "$SRC_DIR/locales" /usr/local/share/doc/xbnc/
fi

# ---------------------------------------------------------- nginx

setup_nginx() {
    step configuring_nginx_for_https "$DOMAIN"
    local site=/etc/nginx/sites-available/xbnc enabled=/etc/nginx/sites-enabled/xbnc
    if [[ ! -d /etc/nginx/sites-available ]]; then
        site=/etc/nginx/conf.d/xbnc.conf; enabled=""
    fi
    [[ -f "$site" ]] && cp -p "$site" "$site.bak-$STAMP"
    rm -f /etc/nginx/conf.d/xbnc-acme.conf
    install -d -m 0755 /var/www/letsencrypt
    cat > "$site" <<NGX
# xBNC web panel at https://$DOMAIN (written by xBNC's install.sh).
map \$http_upgrade \$xbnc_connection_upgrade { default upgrade; '' close; }

server {
    listen 80;
    listen [::]:80;
    server_name $DOMAIN;
    location /.well-known/acme-challenge/ { root /var/www/letsencrypt; }
    location / { return 301 https://\$host\$request_uri; }
}

server {
    listen 443 ssl;
    listen [::]:443 ssl;
    http2 on;
    server_name $DOMAIN;

    ssl_certificate     /etc/letsencrypt/live/$DOMAIN/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/$DOMAIN/privkey.pem;
    ssl_protocols TLSv1.2 TLSv1.3;

    # Plugins are up to 4 MB.
    client_max_body_size 8m;

    # Restoring a backup uploads the whole archive.
    location = /api/v1/backups/restore {
        client_max_body_size 4g;
        proxy_request_buffering off;
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host \$host;
        proxy_set_header X-Forwarded-For \$remote_addr;
        proxy_set_header X-Forwarded-Proto \$scheme;
        proxy_read_timeout 3600s;
    }

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host \$host;
        proxy_set_header X-Forwarded-For \$remote_addr;
        proxy_set_header X-Forwarded-Proto \$scheme;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection \$xbnc_connection_upgrade;
        proxy_read_timeout 3600s;
    }
}
NGX
    if [[ -n "$enabled" ]]; then ln -sf ../sites-available/xbnc "$enabled"; fi
    # Nginx older than 1.25.1 does not know "http2 on;".
    if ! nginx -t >/dev/null 2>&1; then
        sed -i 's/^    http2 on;$//; s/listen 443 ssl;/listen 443 ssl http2;/; s/listen \[::\]:443 ssl;/listen [::]:443 ssl http2;/' "$site"
    fi
    # Without IPv6 on this machine the [::] listeners fail.
    if ! nginx -t >/dev/null 2>&1 && nginx -t 2>&1 | grep -q "Address family not supported"; then
        sed -i '/listen \[::\]:/d' "$site"
    fi
    if ! nginx -t >/dev/null 2>&1; then
        nginx -t || true
        die the_nginx_configuration_is_not "$site"
    fi
    systemctl enable --now nginx >/dev/null 2>&1 || true
    systemctl reload nginx
}
if [[ $NGINX -eq 1 ]]; then setup_nginx; fi

# ---------------------------------------------------------- firewall

open_firewall() {
    local ports=("$IRC_PORT")
    [[ $PROXY -eq 1 || $SINGLE -eq 1 ]] || ports+=("$WEB_PORT")
    [[ "$DCC_PORTS" == off ]] || ports+=("$DCC_PORTS")
    if [[ $NGINX -eq 1 ]]; then ports+=(80 443); elif [[ -n "$LE_EMAIL" && -z "$LE_DNS" ]]; then ports+=(80); fi
    if command -v ufw >/dev/null && ufw status 2>/dev/null | grep -q "Status: active"; then
        step opening_ports_in_ufw
        local p net
        for p in "${ports[@]}"; do ufw allow "${p/-/:}/tcp" >/dev/null; done
        if [[ "$PLAIN_PORT" != off ]]; then
            for net in 10.0.0.0/8 172.16.0.0/12 192.168.0.0/16 100.64.0.0/10; do
                ufw allow from "$net" to any port "$PLAIN_PORT" proto tcp >/dev/null
            done
        fi
    elif command -v firewall-cmd >/dev/null && firewall-cmd --state >/dev/null 2>&1; then
        step opening_ports_in_firewalld
        local p
        for p in "${ports[@]}"; do firewall-cmd --quiet --permanent --add-port="$p/tcp"; done
        # xBNC itself refuses clients outside the local network on this port.
        [[ "$PLAIN_PORT" != off ]] && firewall-cmd --quiet --permanent --add-port="$PLAIN_PORT/tcp"
        firewall-cmd --quiet --reload
    fi
}
[[ $FIREWALL -eq 1 ]] && open_firewall

# ---------------------------------------------------------- service

step installing_the_systemd_service
if [[ "$PACKAGED" != 1 ]]; then
    install -m 0644 "$SRC_DIR/packaging/systemd/xbnc.service" /etc/systemd/system/xbnc.service
fi
if [[ ! -d /run/systemd/system ]]; then
    note 4 systemd_is_not_running_container
    START=0
else
    systemctl daemon-reload
fi
if [[ $START -eq 1 ]]; then
    if [[ $WAS_RUNNING -eq 1 ]]; then
        systemctl restart xbnc
    else
        systemctl enable --now xbnc >/dev/null 2>&1
    fi
    for _ in $(seq 1 20); do
        systemctl is-active --quiet xbnc && break
        sleep 0.5
    done
    if ! systemctl is-active --quiet xbnc; then
        journalctl -u xbnc -n 20 --no-pager || true
        die the_service_did_not_start
    fi
fi

# ---------------------------------------------------------- summary

if [[ $NGINX -eq 1 ]]; then
    PANEL="https://$URL_HOST"
elif [[ $PROXY -eq 1 ]]; then
    PANEL="https://$URL_HOST  ($(say publish_it_with_nginx): packaging/nginx/xbnc.conf)"
else
    PANEL="https://$URL_HOST:$WEB_PORT"
fi
[[ $SINGLE -eq 1 ]] && PANEL="https://$URL_HOST:$IRC_PORT  ($(t same_port_as_irc))"
CODE=""
if [[ $START -eq 1 ]]; then
    for _ in $(seq 1 20); do
        [[ -f /var/lib/xbnc/setup-code ]] && break
        sleep 0.5
    done
    [[ -f /var/lib/xbnc/setup-code ]] && CODE="$(head -n1 /var/lib/xbnc/setup-code)"
fi

box() { echo "  │  $(t "$@")"; }
box_in() { # INDENT KEY [ARG...]: a summary line under the one before
    local n="$1"
    shift
    printf '  │  %*s%s\n' "$n" '' "$(t "$@")"
}
echo
echo "  ┌──────────────────────────────────────────────────────────────"
if [[ $START -eq 1 ]]; then
    box xbnc_is_installed_and_running
else
    box xbnc_is_installed_the_service
fi
echo "  │"
box 1_open_the_panel "$PANEL"
if [[ -z "$LE_EMAIL" ]] && self_signed; then
    box_in 3 your_browser_will_warn_about
    box_in 3 choose_advanced_and_continue_accept
fi
if [[ -n "$CODE" ]]; then
    box 2_enter_the_setup_code "$CODE"
    box_in 3 and_create_your_administrator_account
elif [[ $START -eq 1 ]]; then
    box 2_sign_in_with_your
else
    box 2_start_the_service_sudo
    box_in 3 and_get_the_setup_code
fi
box 3_add_your_networks_in
echo "  │"
LANIP="$(detect_ip)"
box irc_client_hexchat_mirc_weechat
box_in 2 server_port_with_ssl "$PUBLIC" "$IRC_PORT"
if [[ "$PLAIN_PORT" != off ]]; then
    box_in 2 from_your_local_network_server "${LANIP:-$PUBLIC}" "$PLAIN_PORT"
elif [[ -n "$LANIP" && "$LANIP" != "$PUBLIC" ]]; then
    box_in 2 from_your_local_network_server_2 "$LANIP" "$IRC_PORT"
fi
box_in 2 server_password_your_user_your
box_in 2 the_user_and_password_of
box_in 2 the_panel_gives_it_to
box_in 2 sudo_xbncd_info_shows_it
echo "  └──────────────────────────────────────────────────────────────"
echo
t later | sed 's/^/  /'
printf '    %-24s %s\n' "sudo xbncd info" "$(t later_info)"
printf '    %-24s %s\n' "sudo xbncd setup-code" "$(t later_setup_code)"
if [[ -n "$DOMAIN" ]]; then
    echo
    note 2 if_there_is_a_router "$(detect_ip)"
    if [[ $NGINX -eq 1 ]]; then
        note 2 80_443_and_the_certificate "$IRC_PORT"
    else
        if [[ $SINGLE -eq 1 && -n "$LE_EMAIL" && -z "$LE_DNS" ]]; then
            note 2 router_port_single_le "$IRC_PORT"
        elif [[ $SINGLE -eq 1 ]]; then
            note 2 router_port_single "$IRC_PORT"
        elif [[ -n "$LE_EMAIL" && -z "$LE_DNS" ]]; then
            note 2 router_ports_le "$WEB_PORT" "$IRC_PORT"
        else
            note 2 router_ports "$WEB_PORT" "$IRC_PORT"
        fi
    fi
    [[ "$DCC_PORTS" == off ]] || note 2 router_dcc_ports "$DCC_PORTS"
    note 2 if_the_domain_does_not "$(detect_ip)"
fi
if [[ -z "$DOMAIN" ]]; then
    echo
    note 2 from_outside_your_network_forward "$IRC_PORT" "$ADDRESS"
    [[ $SINGLE -eq 1 ]] || note 2 better_not_expose_the_panel "$WEB_PORT"
fi
