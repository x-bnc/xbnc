# shellcheck shell=bash
# Finds the xbncd binary to install. Sourced by install.sh and update.sh,
# which set SRC_DIR and LANGUAGE and load the texts (i18n.sh).
#
# In order:
#   1. --binary PATH, if given;
#   2. the ready-made binary for this machine in bin/ (checked against
#      bin/SHA256SUMS), unless --build was given or target/release/xbncd is
#      newer (built here from changed code);
#   3. target/release/xbncd, if it was already built;
#   4. otherwise it builds it: scripts/prereqs.sh installs what is missing
#      (build tools and Rust) and cargo builds it as the owner of the
#      source folder.
# Sets BINARY to the result.

ready_made_binary() { # prints the path of a verified ready-made binary, or nothing
    local arch file sums
    case "$(uname -m)" in
        x86_64|amd64) arch=x86_64 ;;
        aarch64|arm64) arch=aarch64 ;;
        *) return 0 ;;
    esac
    file="$SRC_DIR/bin/xbncd-linux-$arch"
    sums="$SRC_DIR/bin/SHA256SUMS"
    [[ -f "$file" && -f "$sums" ]] || return 0
    if ! (cd "$SRC_DIR/bin" && grep " xbncd-linux-$arch\$" SHA256SUMS | sha256sum -c --quiet - >/dev/null 2>&1); then
        note 4 bin_the_ready_made_binary_does "$file" >&2
        return 0
    fi
    chmod 0755 "$file" 2>/dev/null || true
    if ! "$file" --version >/dev/null 2>&1; then
        note 4 bin_the_ready_made_binary_does_2 >&2
        return 0
    fi
    echo "$file"
}

build_from_source() {
    local owner home cargo mem_kb swap_kb free_kb
    step bin_preparing_to_build_from_the
    bash "$SRC_DIR/scripts/prereqs.sh" --language "$LANGUAGE" || die bin_build_requirements_are_missing
    owner="$(stat -c %U "$SRC_DIR")"
    home="$(getent passwd "$owner" | cut -d: -f6)"
    cargo="$home/.cargo/bin/cargo"
    if [[ ! -x "$cargo" ]]; then
        cargo="$(command -v cargo || true)"
    fi
    [[ -n "$cargo" ]] || die bin_cargo_not_found

    # A release build needs about 2 GB of memory and 3 GB of disk.
    mem_kb="$(awk '/^MemTotal:/{print $2}' /proc/meminfo 2>/dev/null || echo 0)"
    swap_kb="$(awk '/^SwapTotal:/{print $2}' /proc/meminfo 2>/dev/null || echo 0)"
    if (( mem_kb + swap_kb < 1900000 )); then
        note 4 bin_warning_this_machine_has_less
        note 4 bin_if_it_fails_add_swap
        echo "      sudo fallocate -l 2G /swapfile && sudo chmod 600 /swapfile && sudo mkswap /swapfile && sudo swapon /swapfile"
    fi
    free_kb="$(df -Pk "$SRC_DIR" | awk 'NR==2{print $4}')"
    if (( free_kb < 3000000 )); then
        die bin_about_3_gb_of_free "$SRC_DIR"
    fi

    step bin_building_xbnc_as_it_takes "$owner"
    # The owner's own home and Rust folders, never those of whoever ran sudo.
    local clean=(env -u RUSTUP_HOME -u CARGO_HOME -u RUSTUP_TOOLCHAIN -u CARGO_TARGET_DIR HOME="$home")
    local run=("${clean[@]}")
    if [[ "$(id -un)" != "$owner" ]]; then
        if command -v runuser >/dev/null; then run=(runuser -u "$owner" -- "${clean[@]}"); else run=(sudo -u "$owner" -H "${clean[@]}"); fi
    fi
    (cd "$SRC_DIR" && "${run[@]}" "$cargo" build --release --locked) || die bin_the_build_failed
    BINARY="$SRC_DIR/target/release/xbncd"
}

xbnc_get_binary() { # xbnc_get_binary FORCE_BUILD(0|1)
    local force="${1:-0}" ready built="$SRC_DIR/target/release/xbncd"
    if [[ -n "${BINARY:-}" ]]; then
        return 0
    fi
    if [[ "$force" == 1 ]]; then
        build_from_source
        return 0
    fi
    ready="$(ready_made_binary)"
    if [[ -n "$ready" && ! ( -x "$built" && "$built" -nt "$ready" ) ]]; then
        note 4 bin_using_the_ready_made_binary "${ready#"$SRC_DIR"/}"
        BINARY="$ready"
    elif [[ -x "$built" ]]; then
        note 4 bin_using_the_built_binary_target
        BINARY="$built"
    else
        build_from_source
    fi
}
