#!/bin/sh
# Flycomp installer
# Usage: curl -sSfL https://raw.githubusercontent.com/HalFrgrd/flyline/master/flycomp/install.sh | sh

set -eu

REPO="HalFrgrd/flyline"
INSTALL_DIR="${HOME}/.local/bin"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

say() { printf '\033[1;34m==> \033[0m%s\n' "$*"; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2; }
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

# ---------------------------------------------------------------------------
# GitHub releases API
# ---------------------------------------------------------------------------

get_asset_url() {
    release_json="$1"
    asset_name="$2"
    url="$(printf '%s' "$release_json" | grep '"browser_download_url"' \
        | grep "/${asset_name}\"" | head -1 \
        | sed 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')"
    echo "$url"
}

# ---------------------------------------------------------------------------
# Helpers for portability
# ---------------------------------------------------------------------------

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
        BIN_NAME="flycomp"
    else
        TARGET="${ARCH}-unknown-linux-musl"
        BIN_NAME="flycomp"
    fi

    say "Detected target: ${TARGET}"

    if [ -n "${FLYCOMP_RELEASE_VERSION:-}" ]; then
        say "Using specified release version: ${FLYCOMP_RELEASE_VERSION}"
        VERSION="${FLYCOMP_RELEASE_VERSION}"
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

    ARCHIVE="flycomp-${VERSION}-${TARGET}.tar.gz"
    ARCHIVE_SHA256="${ARCHIVE}.sha256"

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
        (cd "$TMP_DIR" && verify_sha256 "$ARCHIVE_SHA256") \
            || err "Checksum verification failed for ${ARCHIVE}."
    fi

    # Prompt for install directory; read from /dev/tty so it works when piped.
    # Falls back to the default when no terminal is available (e.g. CI).
    say "Enter install directory (leave blank to use: ~/.local/bin)"
    printf '> ' >&2
    input_dir=""
    if [ -t 0 ]; then
        read -r input_dir || true
    elif [ -r /dev/tty ]; then
        read -r input_dir </dev/tty || true
    fi
    if [ -n "$input_dir" ]; then
        case "$input_dir" in
            '~/'*) input_dir="${HOME}/${input_dir#~/}" ;;
            '~')   input_dir="${HOME}" ;;
        esac
        INSTALL_DIR="$input_dir"
    fi

    mkdir -p "$INSTALL_DIR"

    tar xzf "${TMP_DIR}/${ARCHIVE}" -C "$INSTALL_DIR"
    chmod +x "${INSTALL_DIR}/${BIN_NAME}"

    say "Installed: ${INSTALL_DIR}/${BIN_NAME}"
    say ""
    say "Installation complete!"
}

main "$@"
