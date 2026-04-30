#!/bin/sh
# Flyline installer
# Usage: curl -sSfL https://raw.githubusercontent.com/HalFrgrd/flyline/master/install.sh | sh

set -eu

REPO="HalFrgrd/flyline"
INSTALL_DIR="${HOME}/.local/lib"
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

fetch_text() {
    url="$1"
    if command -v curl >/dev/null 2>&1; then
        curl -sSfL "$url"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO- "$url"
    else
        err "Neither curl nor wget is available. Please install one and retry."
    fi
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
    os="$(uname -s)"
    case "$os" in
        Linux) echo "linux" ;;
        Darwin) echo "darwin" ;;
        *) err "Unsupported OS: $os" ;;
    esac
}

detect_arch() {
    arch="$(uname -m)"
    case "$arch" in
        x86_64 | amd64) echo "x86_64" ;;
        aarch64 | arm64) echo "aarch64" ;;
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
# GitHub releases API
# ---------------------------------------------------------------------------

get_asset_url() {
    release_json="$1"
    asset_name="$2"
    # The asset name appears at the end of the URL, preceded by '/' and followed
    # by the closing '"' of the JSON string value.
    url="$(printf '%s' "$release_json" | grep '"browser_download_url"' \
        | grep "/${asset_name}\"" | head -1 \
        | sed 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')"
    echo "$url"
}

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
    else
        LIBC="$(detect_libc)"
        TARGET="${ARCH}-unknown-linux-${LIBC}"
        LIB_NAME="libflyline.so"
    fi

    say "Detected target: ${TARGET}"

    if [ -n "${FLYLINE_RELEASE_VERSION:-}" ]; then
        say "Using specified release version: ${FLYLINE_RELEASE_VERSION}"
        VERSION="${FLYLINE_RELEASE_VERSION}"
        RELEASE_JSON="$(fetch_text "https://api.github.com/repos/${REPO}/releases/tags/${VERSION}")"
        printf '%s' "$RELEASE_JSON" | grep -q '"tag_name"' \
            || err "Could not find release for version ${VERSION}. Please check https://github.com/${REPO}/releases for available versions."
    else
        say "Fetching latest release information..."
        RELEASE_JSON="$(fetch_text "https://api.github.com/repos/${REPO}/releases/latest")"
        VERSION="$(printf '%s' "$RELEASE_JSON" | grep '"tag_name"' | head -1 \
            | sed 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')"
        [ -n "$VERSION" ] || err "Could not determine latest release version from GitHub API."
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

    DOWNLOAD_URL="$(get_asset_url "$RELEASE_JSON" "$ARCHIVE")"
    SHA256_URL="$(get_asset_url "$RELEASE_JSON" "$ARCHIVE_SHA256")"

    [ -n "$DOWNLOAD_URL" ] || err "Could not find download URL for ${ARCHIVE} in the latest release.
Please check https://github.com/${REPO}/releases for available assets."

    TMP_DIR="$(mktemp -d)"
    # shellcheck disable=SC2064
    trap "rm -rf '$TMP_DIR'" EXIT

    say "Downloading ${ARCHIVE} from ${DOWNLOAD_URL}..."
    download "$DOWNLOAD_URL" "${TMP_DIR}/${ARCHIVE}"

    if [ -n "$SHA256_URL" ]; then
        say "Downloading checksum from ${SHA256_URL}..."
        download "$SHA256_URL" "${TMP_DIR}/${ARCHIVE_SHA256}"

        say "Verifying checksum..."
        # Run from TMP_DIR so the relative path in the checksum file resolves.
        (cd "$TMP_DIR" && verify_sha256 "$ARCHIVE_SHA256") \
            || err "Checksum verification failed for ${ARCHIVE}."
    fi

    # Prompt for install directory; read from /dev/tty so it works when piped.
    # Falls back to the default when no terminal is available (e.g. CI).
    say "Enter install directory (leave blank to use: ~/.local/lib)"
    printf '> ' >&2
    input_dir=""
    if [ -t 0 ]; then
        read -r input_dir || true
    elif [ -r /dev/tty ]; then
        read -r input_dir </dev/tty || true
    fi
    if [ -n "$input_dir" ]; then
        # Expand a leading ~/ to $HOME/.
        # shellcheck disable=SC2088
        case "$input_dir" in
            '~/'*) input_dir="${HOME}/${input_dir#~/}" ;;
            '~')   input_dir="${HOME}" ;;
        esac
        INSTALL_DIR="$input_dir"
    fi

    mkdir -p "$INSTALL_DIR"

    tar xzf "${TMP_DIR}/${ARCHIVE}" -C "$INSTALL_DIR"

    LIB_PATH="${INSTALL_DIR}/${LIB_NAME}"
    say "Installed: ${LIB_PATH}"

    # Update or add 'enable -f ... flyline' in ~/.bashrc.
    ENABLE_CMD="enable -f ${LIB_PATH} flyline"
    if [ -f "$BASHRC" ] && grep -qE '^enable( -f [^ ]*)? flyline( |$)' "$BASHRC"; then
        new_content=$(sed -E "s|^enable( -f [^ ]*)? flyline( .*)?$|${ENABLE_CMD}|" "$BASHRC")
        printf '%s' "$new_content" > "$BASHRC"
        say "Updated flyline configuration in ${BASHRC}"
    else
        printf '\n# Flyline - enhanced Bash experience\n%s\n' "$ENABLE_CMD" >> "$BASHRC"
        say "Added flyline to ${BASHRC}"
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
    say "Installation complete!"
    say '    To activate in the current shell:'
    say "        $ENABLE_CMD"
    say '    Or open a new terminal and run the tutorial:'
    say "        flyline run-tutorial"
}

main "$@"
