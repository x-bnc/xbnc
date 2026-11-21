#!/usr/bin/env bash
# Builds xbnc_<version>_<arch>.deb (dpkg-deb) and xbnc-<version>-1.<arch>.rpm
# (rpmbuild, when available) into dist/, for x86_64 or aarch64.
#
#   cargo build --release && ./packaging/build-packages.sh
#   BIN=target/aarch64-unknown-linux-gnu/release/xbncd ARCH=aarch64 ./packaging/build-packages.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
BIN="${BIN:-$ROOT/target/release/xbncd}"
OUT="$ROOT/dist"
[[ -x "$BIN" ]] || { echo "Build first: cargo build --release" >&2; exit 1; }
ARCH="${ARCH:-$(uname -m)}"
case "$ARCH" in
    x86_64|amd64) ARCH=x86_64; DEB_ARCH=amd64 ;;
    aarch64|arm64) ARCH=aarch64; DEB_ARCH=arm64 ;;
    *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac
# A cross-compiled binary cannot run here: only check the version when it can.
if [[ "$ARCH" == "$(uname -m)" ]]; then
    [[ "$("$BIN" --version)" == *"$VERSION"* ]] || { echo "$BIN is not version $VERSION" >&2; exit 1; }
fi
mkdir -p "$OUT"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

stage() { # $1 = root dir, $2 = systemd unit dir
    local r="$1"
    install -Dm0755 "$BIN" "$r/usr/bin/xbncd"
    install -Dm0755 "$ROOT/packaging/xbnc-setup" "$r/usr/sbin/xbnc-setup"
    install -d "$r$2"
    sed 's|/usr/local/bin/xbncd|/usr/bin/xbncd|g; s|/usr/local/share/doc/xbnc|/usr/share/doc/xbnc|g' \
        "$ROOT/packaging/systemd/xbnc.service" > "$r$2/xbnc.service"
    install -m0644 "$ROOT"/packaging/systemd/xbnc-update.{path,service} "$r$2/"
    chmod 0644 "$r$2/xbnc.service"
    install -d "$r/usr/share/xbnc/scripts" "$r/usr/share/xbnc/packaging/systemd" "$r/usr/share/doc/xbnc/docs"
    # install.sh finds its files relative to its own directory.
    install -m0755 "$ROOT"/scripts/*.sh "$r/usr/share/xbnc/scripts/"
    cp "$r$2"/xbnc*.{service,path} "$r/usr/share/xbnc/packaging/systemd/"
    install -m0644 "$ROOT/packaging/xbnc.toml.example" "$r/usr/share/xbnc/packaging/"
    cp -r "$ROOT/packaging/nginx" "$ROOT/packaging/monitoring" "$r/usr/share/xbnc/packaging/"
    install -m0644 "$ROOT"/{README.md,CHANGELOG.md,LICENSE,NOTICE,THIRD_PARTY_LICENSES.md} "$r/usr/share/doc/xbnc/"
    cp "$ROOT"/docs/*.md "$ROOT/docs/openapi.json" "$r/usr/share/doc/xbnc/docs/"
}

# ---------------------------------------------------------- .deb
D="$STAGE/deb"
stage "$D" /lib/systemd/system
install -d "$D/DEBIAN"
SIZE="$(du -sk "$D" | cut -f1)"
cat > "$D/DEBIAN/control" <<CTL
Package: xbnc
Version: $VERSION
Architecture: $DEB_ARCH
Maintainer: y2k
Installed-Size: $SIZE
Depends: ca-certificates, openssl, systemd
Section: net
Priority: optional
Homepage: https://github.com/
Description: IRC bouncer with a web panel
 xBNC keeps you connected to IRC networks, stores history for each of your
 devices and is managed from a web panel. https://xbnc.org
CTL
install -m0755 "$ROOT"/packaging/deb/{postinst,prerm,postrm} "$D/DEBIAN/"
DEB="$OUT/xbnc_${VERSION}_${DEB_ARCH}.deb"
dpkg-deb --root-owner-group --build "$D" "$DEB" >/dev/null
echo "Built $DEB"

# ---------------------------------------------------------- .rpm
if command -v rpmbuild >/dev/null; then
    R="$STAGE/rpmroot"
    stage "$R" /usr/lib/systemd/system
    rpmbuild -bb --target "$ARCH" --define "_topdir $STAGE/rpmbuild" --define "xbnc_version $VERSION" \
        --define "xbnc_root $R" --define "xbnc_arch $ARCH" --define "__strip /bin/true" \
        "$ROOT/packaging/rpm/xbnc.spec" >/dev/null
    cp "$STAGE"/rpmbuild/RPMS/"$ARCH"/xbnc-*.rpm "$OUT/"
    echo "Built $(ls "$OUT"/xbnc-"$VERSION"*.rpm)"
else
    echo "rpmbuild not found: skipping the .rpm (CI builds it in a Fedora container)"
fi
