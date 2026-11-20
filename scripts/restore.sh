#!/usr/bin/env bash
# Restores an archive created by backup.sh.
#
#   sudo ./scripts/restore.sh /var/backups/xbnc/xbnc-20261108T100000Z.tar.gz [--with-config] [--yes]
#
# The current database and key are kept as *.pre-restore-<timestamp> so the
# restore itself can be undone.
set -euo pipefail
XBNCD="$(command -v xbncd || echo /usr/local/bin/xbncd)"

ARCHIVE=""
WITH_CONFIG=0
YES=0
for a in "$@"; do
    case "$a" in
        --with-config) WITH_CONFIG=1 ;;
        --yes) YES=1 ;;
        -h|--help) echo "Usage: sudo $0 ARCHIVE [--with-config] [--yes]"; exit 0 ;;
        *) ARCHIVE="$a" ;;
    esac
done
[[ $EUID -eq 0 ]] || { echo "Run as root (sudo)." >&2; exit 1; }
[[ -f "$ARCHIVE" ]] || { echo "Archive not found: $ARCHIVE" >&2; exit 1; }

DATA_DIR="/var/lib/xbnc"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
tar -C "$WORK" -xzf "$ARCHIVE"
for f in xbnc.db secret.key; do
    [[ -f "$WORK/$f" ]] || { echo "Archive is missing $f; not a backup.sh archive." >&2; exit 1; }
done
echo "Archive created by: $(cat "$WORK/VERSION" 2>/dev/null || echo unknown)"
echo "Installed version:  $("$XBNCD" --version)"

if [[ $YES -ne 1 ]]; then
    read -r -p "Stop xBNC and replace the current database with this backup? [y/N] " answer
    [[ "$answer" =~ ^[yY]$ ]] || { echo "Aborted."; exit 1; }
fi

systemctl stop xbnc
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
for f in xbnc.db xbnc.db-wal xbnc.db-shm secret.key; do
    if [[ -f "$DATA_DIR/$f" ]]; then
        mv "$DATA_DIR/$f" "$DATA_DIR/$f.pre-restore-$STAMP"
    fi
done
install -m 0600 -o xbnc -g xbnc "$WORK/xbnc.db" "$DATA_DIR/xbnc.db"
install -m 0600 -o xbnc -g xbnc "$WORK/secret.key" "$DATA_DIR/secret.key"
if [[ $WITH_CONFIG -eq 1 && -d "$WORK/etc-xbnc" ]]; then
    cp -a /etc/xbnc "/etc/xbnc.pre-restore-$STAMP"
    cp -a "$WORK/etc-xbnc/." /etc/xbnc/
fi
systemctl start xbnc
sleep 2
if systemctl is-active --quiet xbnc; then
    echo "Restore complete. Previous files kept with suffix .pre-restore-$STAMP"
else
    echo "xBNC did not start after the restore; check: journalctl -u xbnc -n 50" >&2
    echo "Previous files are in $DATA_DIR/*.pre-restore-$STAMP" >&2
    exit 1
fi
