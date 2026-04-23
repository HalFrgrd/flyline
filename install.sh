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
err() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

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

# Returns 0 (true) if the system bash is older than 4.4, 1 (false) otherwise.
is_bash_pre_4_4() {
    version_str="$(detect_bash_version_parts)"
    major="${version_str%% *}"
    minor="${version_str##* }"
    major="${major:-0}"
    minor="${minor:-0}"
    [ "$major" -lt 4 ] || { [ "$major" -eq 4 ] && [ "$minor" -lt 4 ]; }
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

    if [ "$OS" = "darwin" ]; then
        TARGET="${ARCH}-apple-darwin"
        LIB_NAME="libflyline.dylib"
        # On macOS, bash reads ~/.bash_profile by default; fall back to ~/.bashrc.
        if [ ! -f "${HOME}/.bashrc" ] && [ -f "${HOME}/.bash_profile" ]; then
            BASHRC="${HOME}/.bash_profile"
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

    # When the system bash is older than 4.4 (e.g. macOS ships with 3.2.57),
    # use the pre-bash-4.4 build.
    if is_bash_pre_4_4; then
        say "Detected bash < 4.4, using pre-bash-4.4 build..."
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

    # Update or add 'enable -f ... flyline' in ~/.bashrc (or ~/.bash_profile on macOS).
    ENABLE_CMD="enable -f ${LIB_PATH} flyline"
    if [ -f "$BASHRC" ] && grep -qE '^enable( -f [^ ]*)? flyline( |$)' "$BASHRC"; then
        new_content=$(sed -E "s|^enable( -f [^ ]*)? flyline( .*)?$|${ENABLE_CMD}|" "$BASHRC")
        printf '%s' "$new_content" > "$BASHRC"
        say "Updated flyline configuration in ${BASHRC}"
    else
        printf '\n# Flyline - enhanced Bash experience\n%s\n' "$ENABLE_CMD" >> "$BASHRC"
        say "Added flyline to ${BASHRC}"
    fi


    say ""
    say "Installation complete!"
    printf '    To activate in the current shell:\n        %s\n' "$ENABLE_CMD"
    printf '    Or open a new terminal.\n'
}

main "$@"
