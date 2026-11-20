#!/usr/bin/env bash
# Installs everything needed to build xBNC from source, on any Linux:
#   - the C compiler and build tools, with the system's package manager
#     (apt, dnf, yum, pacman, zypper or apk);
#   - Rust, with rustup, the official installer. The Rust packaged by many
#     distributions is too old for xBNC, so it is not used unless it is new
#     enough.
#
#   sudo ./scripts/prereqs.sh              # install what is missing
#   ./scripts/prereqs.sh --check           # only say what is missing (no root needed)
#
# install.sh runs it by itself when there is no ready-made binary for this
# machine, so most people never need to run it by hand.
#
# Rust is installed for the user who owns the source folder (the one who
# unpacked the .zip), in their home folder (~/.cargo, ~/.rustup), never for
# the whole system. Nothing is downloaded except the system packages and
# rustup from https://sh.rustup.rs.
set -euo pipefail

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHECK=0
BUILD_USER=""
LANGUAGE=en
prev=""
for a in "$@"; do
    if [[ $prev == --language ]]; then LANGUAGE="$a"; fi
    prev="$a"
done
# shellcheck source=scripts/i18n.sh
. "$SRC_DIR/scripts/i18n.sh"
xbnc_language_exists "$LANGUAGE" || LANGUAGE=en
xbnc_load_texts "$LANGUAGE"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --check) CHECK=1; shift ;;
        --user) BUILD_USER="${2:-}"; shift 2 ;;
        --language) shift 2 ;;
        -h|--help)
            say pre_usage_sudo_check_user_user "$0"
            exit 0 ;;
        *) die pre_unknown_option "$1" ;;
    esac
done

# The minimum Rust version, read from Cargo.toml so it never gets out of date.
MIN_RUST="$(sed -n 's/^rust-version *= *"\([0-9.]*\)".*/\1/p' "$SRC_DIR/Cargo.toml" | head -n1)"
[[ -n "$MIN_RUST" ]] || die pre_rust_version_not_found_in

# Who builds: by default the owner of the source folder.
[[ -n "$BUILD_USER" ]] || BUILD_USER="$(stat -c %U "$SRC_DIR")"
id "$BUILD_USER" >/dev/null 2>&1 || die pre_user_does_not_exist "$BUILD_USER"
BUILD_HOME="$(getent passwd "$BUILD_USER" | cut -d: -f6)"
[[ -n "$BUILD_HOME" && -d "$BUILD_HOME" ]] || die pre_user_has_no_home_folder "$BUILD_USER"

as_builder() { # runs a command as the build user, with their own home and Rust folders
    # RUSTUP_HOME and CARGO_HOME of whoever ran sudo must not leak into the
    # build user's environment: rustup would look for Rust in root's home.
    local clean=(env -u RUSTUP_HOME -u CARGO_HOME -u RUSTUP_TOOLCHAIN HOME="$BUILD_HOME")
    if [[ "$(id -un)" == "$BUILD_USER" ]]; then
        "${clean[@]}" "$@"
    elif command -v runuser >/dev/null; then
        runuser -u "$BUILD_USER" -- "${clean[@]}" "$@"
    else
        sudo -u "$BUILD_USER" -H "${clean[@]}" "$@"
    fi
}

# --------------------------------------------------------------- system
# shellcheck disable=SC1091
. /etc/os-release 2>/dev/null || true
DISTRO="${PRETTY_NAME:-Linux}"

if command -v apt-get >/dev/null; then
    PM=apt;    PKGS="build-essential pkg-config curl ca-certificates"
elif command -v dnf >/dev/null; then
    PM=dnf;    PKGS="gcc gcc-c++ make pkgconfig curl ca-certificates"
elif command -v yum >/dev/null; then
    PM=yum;    PKGS="gcc gcc-c++ make pkgconfig curl ca-certificates"
elif command -v pacman >/dev/null; then
    PM=pacman; PKGS="base-devel curl ca-certificates"
elif command -v zypper >/dev/null; then
    PM=zypper; PKGS="gcc gcc-c++ make pkg-config curl ca-certificates"
elif command -v apk >/dev/null; then
    PM=apk;    PKGS="build-base pkgconf curl ca-certificates"
else
    PM=""
    PKGS=""
fi

missing_tools() {
    local tool out=""
    for tool in cc make curl pkg-config; do
        command -v "$tool" >/dev/null || out="$out $tool"
    done
    echo "${out# }"
}

# ------------------------------------------------------------------ Rust
# The cargo the build user would get: rustup's first, then the PATH.
find_cargo() {
    if [[ -x "$BUILD_HOME/.cargo/bin/cargo" ]]; then
        echo "$BUILD_HOME/.cargo/bin/cargo"
    else
        as_builder sh -c 'command -v cargo' 2>/dev/null || true
    fi
}

rust_version() { # version of the rustc next to the given cargo, or empty
    local dir rustc
    dir="$(dirname "$1")"
    rustc="$dir/rustc"
    [[ -x "$rustc" ]] || rustc="$(as_builder sh -c 'command -v rustc' 2>/dev/null || true)"
    [[ -n "$rustc" ]] || return 0
    { as_builder "$rustc" --version 2>/dev/null || true; } | awk '{print $2}' | sed 's/-.*//'
}

new_enough() { # new_enough HAVE NEED
    [[ "$(printf '%s\n%s\n' "$2" "$1" | sort -V | head -n1)" == "$2" ]]
}

rust_ok() {
    local cargo ver
    cargo="$(find_cargo)"
    [[ -n "$cargo" ]] || return 1
    ver="$(rust_version "$cargo")"
    [[ -n "$ver" ]] && new_enough "$ver" "$MIN_RUST"
}

# ----------------------------------------------------------------- check
say pre_system_building_user_minimum_rust "$DISTRO" "$BUILD_USER" "$MIN_RUST"

TOOLS_MISSING="$(missing_tools)"
CARGO="$(find_cargo)"
HAVE_RUST=""
[[ -n "$CARGO" ]] && HAVE_RUST="$(rust_version "$CARGO")"

if [[ $CHECK -eq 1 ]]; then
    ok=1
    if [[ -n "$TOOLS_MISSING" ]]; then
        ok=0; say pre_missing_tools "$TOOLS_MISSING"
    else
        say pre_build_tools_ok
    fi
    if rust_ok; then
        say pre_rust_ok "$HAVE_RUST" "$CARGO"
    elif [[ -n "$HAVE_RUST" ]]; then
        ok=0; say pre_rust_is_too_old_or "$HAVE_RUST" "$MIN_RUST"
    else
        ok=0; say pre_rust_is_not_installed
    fi
    if [[ $ok -eq 1 ]]; then
        say pre_everything_is_ready_to_build
        exit 0
    fi
    say pre_run_sudo "$0"
    exit 1
fi

# --------------------------------------------------------------- install
if [[ -n "$TOOLS_MISSING" ]]; then
    [[ $EUID -eq 0 ]] || die pre_missing_run_it_as_root "$TOOLS_MISSING" "$0"
    [[ -n "$PM" ]] || die pre_unknown_package_manager_install_a
    step pre_installing_the_build_tools "$PM"
    # shellcheck disable=SC2086
    case "$PM" in
        apt)    apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends $PKGS >/dev/null ;;
        dnf)    dnf install -y -q $PKGS ;;
        yum)    yum install -y -q $PKGS ;;
        pacman) pacman -Sy --needed --noconfirm $PKGS >/dev/null ;;
        zypper) zypper --non-interactive --quiet install $PKGS ;;
        apk)    apk add --no-cache -q $PKGS ;;
    esac
    TOOLS_MISSING="$(missing_tools)"
    [[ -z "$TOOLS_MISSING" ]] || die pre_still_missing "$TOOLS_MISSING"
else
    note 4 pre_build_tools_already_installed
fi

if rust_ok; then
    note 4 pre_rust_already_installed "$HAVE_RUST" "$CARGO"
elif [[ -x "$BUILD_HOME/.cargo/bin/rustup" ]]; then
    step pre_updating_rust_or_newer_is "$MIN_RUST" "$HAVE_RUST"
    as_builder "$BUILD_HOME/.cargo/bin/rustup" update stable
    as_builder "$BUILD_HOME/.cargo/bin/rustup" default stable
else
    if [[ -n "$HAVE_RUST" ]]; then
        step pre_the_system_s_rust_is "$HAVE_RUST" "$BUILD_USER"
    else
        step pre_installing_rust_with_rustup_for "$BUILD_USER"
    fi
    # Official installer, over HTTPS only. Minimal profile: compiler, cargo
    # and the standard library. The PATH check is skipped because an old
    # Rust from the system may be present; xBNC uses rustup's by full path.
    # Downloaded to a file first, so a failed download is an error and not
    # an empty script that "succeeds". The single quotes are on purpose: the
    # variables belong to the inner shell.
    # shellcheck disable=SC2016
    as_builder sh -c 'set -e; f="$(mktemp)"; trap "rm -f \"\$f\"" EXIT
        curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs -o "$f"
        RUSTUP_INIT_SKIP_PATH_CHECK=yes sh "$f" -y -q --profile minimal --default-toolchain stable' \
        || die pre_rust_could_not_be_installed
fi

rust_ok || die pre_rust_is_still_not_ready
CARGO="$(find_cargo)"
say pre_ready_at "$(as_builder "$CARGO" --version)" "$CARGO"
