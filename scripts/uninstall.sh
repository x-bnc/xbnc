#!/usr/bin/env bash
# Removes the xBNC service and binary.
#
#   sudo ./scripts/uninstall.sh            # keeps config and data
#   sudo ./scripts/uninstall.sh --purge    # also deletes config, data and the xbnc user
set -euo pipefail
if [[ -x /usr/bin/xbncd ]] && { dpkg -S /usr/bin/xbncd >/dev/null 2>&1 || rpm -qf /usr/bin/xbncd >/dev/null 2>&1; }; then
    echo "xBNC was installed from a package: remove it with your package manager (apt remove xbnc / dnf remove xbnc)."
    exit 1
fi

PURGE=0
YES=0
for a in "$@"; do
    case "$a" in
        --purge) PURGE=1 ;;
        --yes) YES=1 ;;
        -h|--help) echo "Usage: sudo $0 [--purge [--yes]]"; exit 0 ;;
        *) echo "unknown option: $a" >&2; exit 2 ;;
    esac
done
[[ $EUID -eq 0 ]] || { echo "Run as root (sudo)." >&2; exit 1; }

if systemctl list-unit-files xbnc.service >/dev/null 2>&1; then
    systemctl disable --now xbnc 2>/dev/null || true
fi
rm -f /etc/systemd/system/xbnc.service
systemctl daemon-reload || true
rm -f /usr/local/bin/xbncd /usr/local/bin/xbncd.prev
rm -rf /usr/local/share/doc/xbnc
echo "Service and binary removed."

if [[ $PURGE -eq 1 ]]; then
    if [[ $YES -ne 1 ]]; then
        read -r -p "Delete /etc/xbnc and /var/lib/xbnc (ALL users, networks and history)? Type 'delete': " answer
        [[ "$answer" == "delete" ]] || { echo "Aborted; data kept."; exit 1; }
    fi
    rm -rf /etc/xbnc /var/lib/xbnc
    userdel xbnc 2>/dev/null || true
    echo "Configuration, data and the xbnc user were deleted."
else
    echo "Kept /etc/xbnc and /var/lib/xbnc. Use --purge to delete them."
fi
