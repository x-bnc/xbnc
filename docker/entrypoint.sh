#!/bin/sh
# Container entrypoint: writes a default config on first start, initialises
# the database, then runs `xbncd "$@"`.
#
# Environment (first start only):
#   XBNC_HOSTNAME      server name (default: xbnc)
#   XBNC_TLS_CERT/KEY  PEM files in the container; TLS on 6697 (IRC) and 8443 (panel)
#   XBNC_INSECURE=1    plain text on 6667 (IRC) and 8080 (panel) - test setups only
#   XBNC_PUBLIC_URL    public URL of the panel, e.g. https://bnc.example.org
#   XBNC_WEB=0         disable the web panel
set -eu

CONFIG="${XBNC_CONFIG:-/data/xbnc.toml}"

if [ ! -f "$CONFIG" ]; then
    HOST="${XBNC_HOSTNAME:-xbnc}"
    WEB="${XBNC_WEB:-1}"
    {
        echo "hostname = \"$HOST\""
        echo "secret_key_file = \"/data/secret.key\""
        echo "[database]"
        echo "path = \"/data/xbnc.db\""
        if [ -n "${XBNC_TLS_CERT:-}" ] && [ -n "${XBNC_TLS_KEY:-}" ]; then
            echo "[[listen]]"
            echo "address = \"0.0.0.0:6697\""
            echo "tls_cert = \"$XBNC_TLS_CERT\""
            echo "tls_key = \"$XBNC_TLS_KEY\""
            if [ "$WEB" = "1" ]; then
                echo "[web]"
                echo "address = \"0.0.0.0:8443\""
                echo "tls_cert = \"$XBNC_TLS_CERT\""
                echo "tls_key = \"$XBNC_TLS_KEY\""
                [ -n "${XBNC_PUBLIC_URL:-}" ] && echo "public_url = \"$XBNC_PUBLIC_URL\""
            fi
        elif [ "${XBNC_INSECURE:-0}" = "1" ]; then
            echo "[[listen]]"
            echo "address = \"0.0.0.0:6667\""
            echo "insecure = true"
            if [ "$WEB" = "1" ]; then
                echo "[web]"
                echo "address = \"0.0.0.0:8080\""
                echo "insecure = true"
                [ -n "${XBNC_PUBLIC_URL:-}" ] && echo "public_url = \"$XBNC_PUBLIC_URL\""
            fi
        else
            echo "xbnc: set XBNC_TLS_CERT and XBNC_TLS_KEY, or XBNC_INSECURE=1 for a test setup" >&2
            exit 1
        fi
    } > "$CONFIG"
    chmod 600 "$CONFIG"
    echo "xbnc: wrote default configuration to $CONFIG" >&2
fi

if [ "${1:-run}" = "run" ]; then
    xbncd init >&2
fi

exec xbncd "$@"
