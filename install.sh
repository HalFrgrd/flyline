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

detect_os() {
    os="$(uname -s)"
    case "$os" in
        Linux) echo "linux" ;;
        Darwin) err "macOS is not supported. Flyline only supports Linux." ;;
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
# Main
# ---------------------------------------------------------------------------

main() {
    detect_os >/dev/null   # exits on non-Linux
    ARCH="$(detect_arch)"
    LIBC="$(detect_libc)"
    TARGET="${ARCH}-unknown-linux-${LIBC}"

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

    ARCHIVE="libflyline-${VERSION}-${TARGET}.tar.gz"
    ARCHIVE_SHA256="${ARCHIVE}.sha256"

    DOWNLOAD_URL="$(get_asset_url "$RELEASE_JSON" "$ARCHIVE")"
    SHA256_URL="$(get_asset_url "$RELEASE_JSON" "$ARCHIVE_SHA256")"

    [ -n "$DOWNLOAD_URL" ] || err "Could not find download URL for ${ARCHIVE} in the latest release.
Please check https://github.com/${REPO}/releases for available assets."

    mkdir -p "$INSTALL_DIR"

    TMP_DIR="$(mktemp -d)"
    # shellcheck disable=SC2064
    trap "rm -rf '$TMP_DIR'" EXIT

    say "Downloading ${ARCHIVE}..."
    download "$DOWNLOAD_URL" "${TMP_DIR}/${ARCHIVE}"

    if [ -n "$SHA256_URL" ]; then
        say "Downloading checksum..."
        download "$SHA256_URL" "${TMP_DIR}/${ARCHIVE_SHA256}"

        say "Verifying checksum..."
        # sha256sum expects the file to be relative to the current directory.
        (cd "$TMP_DIR" && sha256sum -c "$ARCHIVE_SHA256") \
            || err "Checksum verification failed for ${ARCHIVE}."
    fi

    tar xzf "${TMP_DIR}/${ARCHIVE}" -C "$INSTALL_DIR"

    LIB_PATH="${INSTALL_DIR}/libflyline.so"
    say "Installed: ${LIB_PATH}"

    # Update or add 'enable -f ... flyline' in ~/.bashrc.
    ENABLE_CMD="enable -f ${LIB_PATH} flyline"
    if [ -f "$BASHRC" ] && grep -qE '^enable( -f [^ ]*)? flyline( |$)' "$BASHRC"; then
        sed -i -E "s|^enable( -f [^ ]*)? flyline( .*)?$|${ENABLE_CMD}|" "$BASHRC"
        say "Updated flyline configuration in ${BASHRC}"
    else
        printf '\n# Flyline - code-editor-like bash experience\n%s\n' "$ENABLE_CMD" >> "$BASHRC"
        say "Added flyline to ${BASHRC}"
    fi

    say ""
    say "Installation complete!"
    printf '    To activate in the current shell:\n        %s\n' "$ENABLE_CMD"
    printf '    Or open a new terminal.\n'
}

main "$@"
