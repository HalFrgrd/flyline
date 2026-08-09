#!/bin/sh
# Flyline installer
# Usage: curl -sSfL https://github.com/HalFrgrd/flyline/releases/latest/download/install.sh | sh

set -eu

expand_path() {
    case "$1" in
        '~/'*) echo "${HOME}/${1#~/}" ;;
        '~')   echo "${HOME}" ;;
        *)     echo "$1" ;;
    esac
}

REPO="HalFrgrd/flyline"
if [ -n "${FLYLINE_INSTALL_DIR:-}" ]; then
    INSTALL_DIR="$(expand_path "$FLYLINE_INSTALL_DIR")"
elif [ -n "${FLYLINE_LOAD_DIR:-}" ]; then
    INSTALL_DIR="$(expand_path "$FLYLINE_LOAD_DIR")"
else
    INSTALL_DIR="${HOME}/.local/lib"
fi
BASHRC="${HOME}/.bashrc"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

say() { printf '\033[1;34m==> \033[0m%s\n' "$*"; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2; }
err() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }
err_no_exit() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; }

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || err "Required command not found: $1"
}

download() {
    url="$1"
    dest="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -sSfL -o "$dest" "$url"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$dest" "$url"
    else
        err "Neither curl nor wget is available. Please install one and retry."
    fi
}

get_latest_version() {
    url="https://github.com/${REPO}/releases/latest"
    if command -v curl >/dev/null 2>&1; then
        tag_url="$(curl -sI "$url" | grep -i '^location:' | head -1)"
    elif command -v wget >/dev/null 2>&1; then
        tag_url="$(wget --max-redirect=0 --server-response -O /dev/null "$url" 2>&1 | grep -i 'location:' | head -1)"
    else
        err "Neither curl nor wget is available. Please install one and retry."
    fi
    version="$(printf '%s' "$tag_url" | sed 's|.*/||' | cut -d' ' -f1 | tr -d '\r\n')"
    [ -n "$version" ] || err "Could not determine latest version from GitHub Release redirect."
    echo "$version"
}

# ---------------------------------------------------------------------------
# Platform detection
# ---------------------------------------------------------------------------

# Detect the version of the system bash as "major minor" integers.
detect_bash_version_parts() {
    bash_bin="$(command -v bash 2>/dev/null || true)"
    [ -n "$bash_bin" ] || { echo "0 0"; return; }
    "$bash_bin" -c 'echo "${BASH_VERSINFO[0]} ${BASH_VERSINFO[1]}"' 2>/dev/null || echo "0 0"
}

# Returns 0 (true) if the given major.minor version is >= 4.4, 1 (false) otherwise.
is_bash_version_4_4_or_later() {
    major="$1"; minor="$2"
    [ "${major:-0}" -gt 4 ] || { [ "${major:-0}" -eq 4 ] && [ "${minor:-0}" -ge 4 ]; }
}

# Returns 0 (true) if the system bash is older than 4.4, 1 (false) otherwise.
is_system_bash_pre_4_4() {
    version_str="$(detect_bash_version_parts)"
    major="${version_str%% *}"
    minor="${version_str##* }"
    ! is_bash_version_4_4_or_later "$major" "$minor"
}

# Returns the path to a Homebrew-installed bash >= 4.4, or an empty string.
find_homebrew_bash() {
    for candidate in "/opt/homebrew/bin/bash" "/usr/local/bin/bash"; do
        if [ -x "$candidate" ]; then
            v="$("$candidate" -c 'echo "${BASH_VERSINFO[0]} ${BASH_VERSINFO[1]}"' 2>/dev/null || echo "0 0")"
            major="${v%% *}"; minor="${v##* }"
            if is_bash_version_4_4_or_later "$major" "$minor"; then
                echo "$candidate"
                return
            fi
        fi
    done
    echo ""
}

detect_os() {
    if [ -n "${TERMUX_VERSION:-}" ] || [ -d "/data/data/com.termux" ] || (uname -o 2>/dev/null | grep -qi android) || (uname -a 2>/dev/null | grep -qi android); then
        echo "android"
        return
    fi
    os="$(uname -s)"
    case "$os" in
        Linux) echo "linux" ;;
        Darwin) echo "darwin" ;;
        FreeBSD) echo "freebsd" ;;
        *) err "Unsupported OS: $os" ;;
    esac
}

detect_arch() {
    arch="$(uname -m)"
    case "$arch" in
        x86_64 | amd64) echo "x86_64" ;;
        aarch64 | arm64) echo "aarch64" ;;
        armv7* | armhf) echo "armv7" ;;
        i386 | i486 | i586 | i686) echo "i686" ;;
        riscv64) echo "riscv64gc" ;;
        ppc64le | powerpc64le) echo "powerpc64le" ;;
        *) err "Unsupported architecture: $arch" ;;
    esac
}

detect_libc() {
    # 1. Inspect the interpreter of the running shell executable — most reliable.
    shell_exe="/proc/$$/exe"
    if [ ! -e "$shell_exe" ]; then
        shell_exe="$(command -v sh || true)"
    fi
    if [ -n "$shell_exe" ] && command -v readelf >/dev/null 2>&1; then
        interp="$(readelf -l "$shell_exe" 2>/dev/null | grep 'interpreter' | grep -o '\[.*\]' | tr -d '[]')" || true
        case "$interp" in
            *musl*) echo "musl"; return ;;
            *) echo "gnu"; return ;;
        esac
    fi

    # 2. Ask ldd directly — musl's ldd prints "musl libc" on --version.
    if ldd --version 2>&1 | grep -qi musl; then
        echo "musl"
        return
    fi

    # 3. Look for the musl dynamic linker on disk.
    if ls /lib/ld-musl-* >/dev/null 2>&1; then
        echo "musl"
        return
    fi

    # 4. Fall back to GNU libc.
    echo "gnu"
}

# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# Helpers for portability
# ---------------------------------------------------------------------------

# Portable checksum verification: supports sha256sum (Linux) and shasum (macOS).
verify_sha256() {
    sha256_file="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum -c "$sha256_file"
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 -c "$sha256_file"
    else
        err "No checksum tool found (sha256sum or shasum). Cannot verify download."
    fi
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    OS="$(detect_os)"
    ARCH="$(detect_arch)"

    if is_system_bash_pre_4_4; then
        use_bash_pre_4_4=true
    else
        use_bash_pre_4_4=false
    fi

    if [ "$OS" = "darwin" ]; then
        TARGET="${ARCH}-apple-darwin"
        LIB_NAME="libflyline.dylib"

        # Flyline can run on the 3.2.57 version of Bash.
        # However, the Bash binary on macOS is often compiled without linkable symbols required to load the Flyline plugin.
        if $use_bash_pre_4_4; then
            BREW_BASH="$(find_homebrew_bash)"
            if [ -n "$BREW_BASH" ]; then
                warn "Your system Bash is older than 4.4. This version won't have been compiled with custom plugin support."
                warn "Ensure that you use $BREW_BASH for flyline."
                use_bash_pre_4_4=false
            else
                err_no_exit "Your system Bash is older than 4.4. This version won't have been compiled with custom plugin support."
                err_no_exit "Please install a newer Bash before trying to use flyline:"
                err "    brew install bash"
            fi
        fi
    elif [ "$OS" = "freebsd" ]; then
        if [ "$ARCH" != "x86_64" ]; then
            err "Unsupported FreeBSD architecture: $ARCH. Only x86_64 is supported."
        fi
        TARGET="x86_64-unknown-freebsd"
        LIB_NAME="libflyline.so"
    elif [ "$OS" = "android" ]; then
        case "$ARCH" in
            aarch64)
                TARGET="aarch64-linux-android"
                ;;
            x86_64)
                TARGET="x86_64-linux-android"
                ;;
            armv7)
                TARGET="armv7-linux-androideabi"
                ;;
            i686)
                TARGET="i686-linux-android"
                ;;
            *)
                err "Unsupported Android architecture: $ARCH"
                ;;
        esac
        LIB_NAME="libflyline.so"
    else
        LIBC="$(detect_libc)"
        case "$ARCH" in
            armv7)
                if [ "$LIBC" = "gnu" ]; then
                    TARGET="armv7-unknown-linux-gnueabihf"
                else
                    err "Unsupported libc ($LIBC) for armv7. Only gnu (gnueabihf) is supported."
                fi
                ;;
            *)
                TARGET="${ARCH}-unknown-linux-${LIBC}"
                ;;
        esac
        LIB_NAME="libflyline.so"
    fi

    say "Detected target: ${TARGET}"

    if [ -n "${FLYLINE_INSTALL_VERSION:-}" ]; then
        say "Using specified release version: ${FLYLINE_INSTALL_VERSION}"
        VERSION="${FLYLINE_INSTALL_VERSION}"
    else
        say "Fetching latest release information..."
        VERSION="$(get_latest_version)"
        say "Latest version: ${VERSION}"
    fi

    ARCHIVE_STEM="libflyline-${VERSION}-${TARGET}"

    if $use_bash_pre_4_4; then
        say "Detected Bash < 4.4, using pre-bash-4.4 build..."
        ARCHIVE="${ARCHIVE_STEM}_pre_bash_4_4.tar.gz"
        ARCHIVE_SHA256="${ARCHIVE}.sha256"
    else
        ARCHIVE="${ARCHIVE_STEM}.tar.gz"
        ARCHIVE_SHA256="${ARCHIVE}.sha256"
    fi

    DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE}"
    SHA256_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE_SHA256}"

    TMP_DIR="$(mktemp -d)"
    # shellcheck disable=SC2064
    trap "rm -rf '$TMP_DIR'" EXIT

    say "Downloading ${ARCHIVE} from
    ${DOWNLOAD_URL}..."
    download "$DOWNLOAD_URL" "${TMP_DIR}/${ARCHIVE}"

    if [ -n "$SHA256_URL" ]; then
        say "Downloading checksum from ${SHA256_URL}..."
        download "$SHA256_URL" "${TMP_DIR}/${ARCHIVE_SHA256}"

        say "Verifying checksum..."
        # Run from TMP_DIR so the relative path in the checksum file resolves.
        (cd "$TMP_DIR" && verify_sha256 "$ARCHIVE_SHA256") \
            || err "Checksum verification failed for ${ARCHIVE}."
    fi


    mkdir -p "$INSTALL_DIR"

    tar xzf "${TMP_DIR}/${ARCHIVE}" -C "$INSTALL_DIR"

    VERSION_NO_V="${VERSION#v}"
    LIB_VERSIONED="${LIB_NAME}.${VERSION_NO_V}"

    if [ -f "${INSTALL_DIR}/${LIB_VERSIONED}" ]; then
        say "Creating symlink ${LIB_NAME} -> ${LIB_VERSIONED}..."
        rm -f "${INSTALL_DIR}/${LIB_NAME}"
        (cd "$INSTALL_DIR" && ln -s "$LIB_VERSIONED" "$LIB_NAME")
    else
        if [ -f "${INSTALL_DIR}/${LIB_NAME}" ]; then
            warn "Expected to find versioned library ${LIB_VERSIONED}, but found ${LIB_NAME} instead."
        else
            err "Failed to find the installed library file in ${INSTALL_DIR}."
        fi
    fi

    LIB_PATH="${INSTALL_DIR}/${LIB_NAME}"
    say "Installed: ${LIB_PATH}"

    # Verify that the library can be loaded by system bash before updating ~/.bashrc
    if command -v bash >/dev/null 2>&1; then
        load_test="$(bash -c "enable -f '$LIB_PATH' flyline" 2>&1 || true)"
        if echo "$load_test" | grep -q "dlopen failed"; then
            warn "Failed to load ${LIB_PATH} with system bash (dlopen test failed)."
            warn "Skipping automatic modification of ${BASHRC}."
            warn "You can try loading it manually with:"
            warn "    enable -f ${LIB_PATH} flyline"
            return 0
        fi
    fi

    # Update or add 'enable -f ... flyline' in ~/.bashrc.
    if [ -z "${FLYLINE_VERSION:-}" ]; then
        ENABLE_CMD="enable -f ${LIB_PATH} flyline"
        printf '\n# Flyline - enhanced Bash experience\n%s\n' "$ENABLE_CMD" >> "$BASHRC"
        say "Added flyline to ${BASHRC}"
    else
        say "Flyline is already installed (detected ${FLYLINE_VERSION}); skipping .bashrc modification."
    fi


    # On macOS, login shells read ~/.bash_profile (not ~/.bashrc).
    # Warn the user if ~/.bash_profile does not appear to source ~/.bashrc.
    if [ "$OS" = "darwin" ]; then
        BASH_PROFILE="${HOME}/.bash_profile"
        if [ -f "$BASH_PROFILE" ]; then
            if ! grep -qE '(source|\.)[[:space:]]+(~|\$\{?HOME\}?)/\.bashrc([[:space:]]|$)' "$BASH_PROFILE"; then
                warn "Your ${BASH_PROFILE} does not appear to source ~/.bashrc."
                warn "On macOS, login shells read ~/.bash_profile, so flyline may not load in new terminals."
                warn "Consider adding the following to ${BASH_PROFILE}:"
                warn '    if [ -f ~/.bashrc ]; then . ~/.bashrc; fi'
            fi
        else
            warn "${BASH_PROFILE} does not exist."
            warn "On macOS, login shells read ~/.bash_profile, so flyline may not load in new terminals."
            warn "Consider creating ${BASH_PROFILE} with the following content:"
            warn '    if [ -f ~/.bashrc ]; then . ~/.bashrc; fi'
        fi
    fi

    say ""
    if [ -n "${FLYLINE_VERSION:-}" ]; then
        say "Upgrade from ${FLYLINE_VERSION} -> ${VERSION}, run \`flyline changelog\` to see what's changed."
        say "To activate the upgrade, open a new shell."
        if [ -n "${FLYLINE_LOAD_DIR:-}" ]; then
            resolved_load_dir="$(expand_path "$FLYLINE_LOAD_DIR")"
            if [ "$resolved_load_dir" != "$INSTALL_DIR" ]; then
                warn "The upgrade installation directory ($INSTALL_DIR) is different from the currently running load directory ($resolved_load_dir)."
                warn "Please make sure to update your ~/.bashrc or other startup scripts to point to the new libflyline."
            fi
        fi
    else
        say "Installation complete!"
        say '    To activate in the current shell:'
        if [ -z "${FLYLINE_INSTALL_DIR:-}" ]; then
            say "        enable -f ${LIB_PATH} flyline"
        else
            say "        enable -d flyline && enable -f ${LIB_PATH} flyline"
        fi
        say '    Or open a new terminal and run the tutorial:'
        say "        flyline run-tutorial"
    fi

    # Detect if ble.sh is running
    if [ -n "${BLE_SESSION_ID:-}" ] || [ -n "${_ble_version:-}" ]; then
        say ""
        warn "ble.sh (Bash Line Editor) is detected."
        warn "Please turn it off/disable it before starting flyline to avoid conflicts."
    fi
}

main "$@"
