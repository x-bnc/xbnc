#!/usr/bin/env bash
# Signs the packages in dist/ for xBNC's own update check (Ed25519, always)
# and, optionally, with GPG for apt/dnf repositories.
#
#   XBNC_RELEASE_KEY="$(cat xbnc-release.key)" ./packaging/sign-packages.sh
#   GPG_KEY_ID=ABCD1234 ./packaging/sign-packages.sh     # also GPG-sign
#
# Create the Ed25519 key once with `xbncd release keygen` and keep the
# private part out of the repository (a CI secret or an offline machine).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${BIN:-$ROOT/target/release/xbncd}"
cd "$ROOT/dist"
shopt -s nullglob
PKGS=(xbnc_*.deb xbnc-*.rpm)
[[ ${#PKGS[@]} -gt 0 ]] || { echo "No packages in dist/: run packaging/build-packages.sh first" >&2; exit 1; }

if [[ -n "${GPG_KEY_ID:-}" ]]; then
    for p in "${PKGS[@]}"; do
        case "$p" in
            *.deb)
                if command -v dpkg-sig >/dev/null; then
                    dpkg-sig -k "$GPG_KEY_ID" --sign builder "$p"
                else
                    echo "dpkg-sig not installed: skipping the GPG signature of $p" >&2
                fi ;;
            *.rpm)
                rpm --define "_gpg_name $GPG_KEY_ID" --addsign "$p" ;;
        esac
    done
fi

# Ed25519 last: it covers the final bytes, including any GPG signature.
[[ -n "${XBNC_RELEASE_KEY:-}" ]] || { echo "XBNC_RELEASE_KEY is not set" >&2; exit 1; }
"$BIN" release sign "${PKGS[@]}"
sha256sum "${PKGS[@]}" > SHA256SUMS
echo "Signed: ${PKGS[*]} (+ .sig, SHA256SUMS)"
if [[ -n "${XBNC_RELEASE_PUBLIC_KEY:-}" ]]; then
    "$BIN" release verify --public-key "$XBNC_RELEASE_PUBLIC_KEY" "${PKGS[@]}"
fi
