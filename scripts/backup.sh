#!/usr/bin/env bash
# Creates a consistent backup of xBNC while it keeps running.
#
#   sudo ./scripts/backup.sh [DEST_DIR] [--keep N]
#
# The archive contains the database (online SQLite backup with integrity
# check), the secret key and /etc/xbnc. It holds credentials: it is created
# with mode 0600 and must be stored securely.
set -euo pipefail
XBNCD="$(command -v xbncd || echo /usr/local/bin/xbncd)"

DEST="/var/backups/xbnc"
KEEP=14
while [[ $# -gt 0 ]]; do
    case "$1" in
        --keep) KEEP="$2"; shift 2 ;;
        -h|--help) echo "Usage: sudo $0 [DEST_DIR] [--keep N]"; exit 0 ;;
        *) DEST="$1"; shift ;;
    esac
done
[[ $EUID -eq 0 ]] || { echo "Run as root (sudo)." >&2; exit 1; }

CONFIG="${XBNC_CONFIG:-/etc/xbnc/xbnc.toml}"
DATA_DIR="/var/lib/xbnc"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
umask 077
install -d -m 0700 "$DEST"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
chown xbnc:xbnc "$WORK"

runuser -u xbnc -- env XBNC_CONFIG="$CONFIG" "$XBNCD" backup "$WORK/xbnc.db" >/dev/null
cp -a "$DATA_DIR/secret.key" "$WORK/secret.key"
cp -a /etc/xbnc "$WORK/etc-xbnc"
"$XBNCD" --version > "$WORK/VERSION"

ARCHIVE="$DEST/xbnc-$STAMP.tar.gz"
n=1
while [[ -e "$ARCHIVE" ]]; do
    ARCHIVE="$DEST/xbnc-$STAMP-$n.tar.gz"
    n=$((n + 1))
done
tar -C "$WORK" -czf "$ARCHIVE" .
chmod 0600 "$ARCHIVE"
echo "Backup written: $ARCHIVE"

# Keep only the newest $KEEP archives.
# Archive names have no spaces (xbnc-<UTC timestamp>.tar.gz).
# shellcheck disable=SC2012
mapfile -t OLD < <(ls -1t "$DEST"/xbnc-*.tar.gz 2>/dev/null | tail -n +"$((KEEP + 1))")
for f in "${OLD[@]}"; do
    rm -f -- "$f"
done
