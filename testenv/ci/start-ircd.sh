#!/usr/bin/env bash
# Builds or installs an IRC server and starts it on 127.0.0.1:6667 for the
# real-IRCd tests (tests/real_ircd.rs). Used by the CI; works on Ubuntu.
#
#   testenv/ci/start-ircd.sh unrealircd|solanum
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
case "${1:-}" in
    unrealircd)
        sudo apt-get install -y build-essential pkg-config libssl-dev libpcre2-dev \
            libargon2-dev libsodium-dev libc-ares-dev libcurl4-openssl-dev libjansson-dev >/dev/null
        src="$(mktemp -d)"
        curl -fsSL https://www.unrealircd.org/downloads/unrealircd-latest.tar.gz \
            | tar -xz -C "$src" --strip-components=1
        cp "$HERE/unrealircd.config.settings" "$src/config.settings"
        (cd "$src" && ./Config -quick && make -j"$(nproc)" >/dev/null && make install >/dev/null)
        cp "$HERE/unrealircd.conf" "$HOME/unrealircd/conf/unrealircd.conf"
        "$HOME/unrealircd/unrealircd" start
        ;;
    solanum)
        sudo apt-get install -y solanum >/dev/null
        # The Debian/Ubuntu package runs it as its own user with its own
        # directories: only the configuration is replaced.
        sudo cp "$HERE/solanum.conf" /etc/solanum/ircd.conf
        sudo systemctl restart solanum || sudo journalctl -u solanum -n 50 --no-pager
        ;;
    *)
        echo "usage: $0 unrealircd|solanum" >&2
        exit 2 ;;
esac
for _ in $(seq 60); do
    nc -z 127.0.0.1 6667 && { echo "$1 is listening on 127.0.0.1:6667"; exit 0; }
    sleep 2
done
echo "$1 did not start" >&2
systemctl is-active solanum >/dev/null 2>&1 && sudo journalctl -u solanum -n 50 --no-pager >&2
[[ -f "$HOME/unrealircd/logs/ircd.log" ]] && tail -50 "$HOME/unrealircd/logs/ircd.log" >&2
exit 1
