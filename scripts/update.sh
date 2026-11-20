#!/usr/bin/env bash
# Updates the installed xbncd binary with automatic rollback.
#
#   unzip xbnc-NEW.zip && cd xbnc && sudo ./scripts/update.sh [--binary PATH] [--build]
#
# The new binary is the ready-made one in bin/ when there is one for this
# machine, otherwise it is built (see binary.sh and prereqs.sh).
# Steps: verify the new binary, take a backup, stop, swap the binary,
# start, and check the service. If it does not come up, the previous binary
# AND the pre-update database are restored (a newer binary may already have
# migrated the schema, which an older binary refuses to open).
# Nothing is updated automatically: this script only runs when you call it.
set -euo pipefail
if [[ -x /usr/bin/xbncd ]] && { dpkg -S /usr/bin/xbncd >/dev/null 2>&1 || rpm -qf /usr/bin/xbncd >/dev/null 2>&1; }; then
    echo "xBNC was installed from a package: update it with your package manager (apt install ./xbnc_*.deb, or dnf install ./xbnc-*.rpm)."
    exit 1
fi

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/i18n.sh
. "$SRC_DIR/scripts/i18n.sh"
# The language of the installation (xbnc.toml), or --language.
LANGUAGE="$(sed -n 's/^language *= *"\([A-Za-z-]*\)".*/\1/p' /etc/xbnc/xbnc.toml 2>/dev/null | head -n1)"
prev=""
for a in "$@"; do
    if [[ $prev == --language ]]; then LANGUAGE="$a"; fi
    prev="$a"
done
xbnc_language_exists "${LANGUAGE:-x}" || LANGUAGE=en
xbnc_load_texts "$LANGUAGE"
BINARY=""
FORCE_BUILD=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --binary) BINARY="$2"; shift 2 ;;
        --build) FORCE_BUILD=1; shift ;;
        --language) shift 2 ;;
        -h|--help) say upd_usage_sudo_binary_path_build "$0"; exit 0 ;;
        *) die upd_unknown_option "$1" ;;
    esac
done
[[ $EUID -eq 0 ]] || die upd_run_it_as_root_sudo "$0"
# shellcheck source=scripts/binary.sh
. "$SRC_DIR/scripts/binary.sh"
xbnc_get_binary "$FORCE_BUILD"
[[ -x "$BINARY" ]] || die upd_binary_not_found "$BINARY"

NEW_VERSION="$("$BINARY" --version)"
OLD_VERSION="$(/usr/local/bin/xbncd --version 2>/dev/null || echo none)"
say upd_current "$OLD_VERSION"
say upd_new "$NEW_VERSION"
XBNC_CONFIG=/etc/xbnc/xbnc.toml "$BINARY" check-config

step upd_backup
BACKUP_OUT="$("$SRC_DIR/scripts/backup.sh")"
echo "$BACKUP_OUT"
ARCHIVE="$(echo "$BACKUP_OUT" | sed -n 's/^Backup written: //p')"

step upd_installing
# Folder for extra languages (TRANSLATING.md); older installs lack it.
install -d -m 0750 -o root -g xbnc /etc/xbnc/locales
systemctl stop xbnc
cp -a /usr/local/bin/xbncd /usr/local/bin/xbncd.prev
install -m 0755 "$BINARY" /usr/local/bin/xbncd
# Languages installed as files (not built in) get the texts of the new version.
for f in "$SRC_DIR"/locales/*.json; do
    code="$(basename "$f" .json)"
    [[ "$code" == en || "$code" == es ]] && continue
    [[ -f "/etc/xbnc/locales/$code.json" ]] && install -m 0640 -o root -g xbnc "$f" "/etc/xbnc/locales/$code.json"
done
systemctl start xbnc
sleep 3
if systemctl is-active --quiet xbnc; then
    say upd_complete "$NEW_VERSION"
    exit 0
fi

echo "!! $(t upd_rolling_back)" >&2
journalctl -u xbnc -n 20 --no-pager >&2 || true
systemctl stop xbnc || true
install -m 0755 /usr/local/bin/xbncd.prev /usr/local/bin/xbncd
"$SRC_DIR/scripts/restore.sh" "$ARCHIVE" --yes
say upd_rolled_back "$OLD_VERSION" >&2
exit 1
