# Installing & Upgrading `flyline`


## Installation

To install `flyline` for the first time, run the following command in your terminal:

```bash
curl -sSfL https://github.com/HalFrgrd/flyline/releases/latest/download/install.sh | sh
```

### Arch Linux

Arch users can install the [AUR package](https://aur.archlinux.org/packages/flyline):

```bash
paru -S flyline
```

### What the installer does:
1. **Platform Detection**: Automatically detects your Operating System (Linux, macOS, FreeBSD), Architecture (x86_64, aarch64, armv7, i686, riscv64gc, powerpc64le), and libc variant (glibc, musl).
2. **Download**: Fetches the matching release tarball (`.tar.gz`) and checksum file directly from the GitHub releases page (latest version or `FLYLINE_INSTALL_VERSION`).
3. **Extraction**: Unpacks the compiled library into `~/.local/lib/` (or your custom `FLYLINE_INSTALL_DIR`).
4. **Symlink Management**: Creates a versioned file (e.g., `libflyline.so.1.2.1`) and updates the `libflyline.so` symlink to point to it.
5. **Shell Configuration**: Appends or updates the dynamic builtin load command in your `~/.bashrc`:
   ```bash
   enable -f ~/.local/lib/libflyline.so flyline
   ```
   *(Note: If `flyline` is already loaded in the shell, detected via the `FLYLINE_VERSION` environment variable, the installer automatically **skips** modifying your `~/.bashrc`, assuming your existing setup is already configured).*


## Upgrading

Upgrading `flyline` to the latest version uses the exact same script. You simply re-run the `curl` command:

```bash
curl -sSfL https://github.com/HalFrgrd/flyline/releases/latest/download/install.sh | sh
```

### Upgrade Notification
When `flyline` is already loaded in your shell, the environment variable `FLYLINE_VERSION` will be detected by the installer, which will print a summary of the version change:
```text
==> Upgrade from v1.2.0 -> v1.2.1, run `flyline changelog` to see what's changed.
```

If the resolved installation directory (`INSTALL_DIR`) differs from the active running load directory (`FLYLINE_LOAD_DIR`), the installer will print a warning:
```text
warning: The upgrade installation directory (~/new-path) is different from the currently running load directory (~/old-path).
warning: Please make sure to update your ~/.bashrc or other startup scripts to point to the new libflyline.
```

### Applying the Upgrade
Because Bash caches loaded dynamic libraries in memory, existing shell sessions will continue to run the older in-memory version. 

To activate the upgrade, simply open a new shell. 

If you want to apply the upgraded binary to your current active shell session without restarting, you can reload it manually:

```bash
enable -d flyline
enable -f $FLYLINE_LOAD_DIR/libflyline.so flyline
```

*(Note: `FLYLINE_LOAD_DIR` is automatically exported by `flyline` at shell startup, pointing to the directory from which the library was loaded).*


## Configuration Variables

You can customize the installation behavior by setting environment variables before running the installation script:

| Environment Variable | Description | Default |
|----------------------|-------------|---------|
| `FLYLINE_INSTALL_DIR` | The destination directory where the shared library is installed. | `~/.local/lib` (or `FLYLINE_LOAD_DIR` if set) |
| `FLYLINE_LOAD_DIR` | Exported by an active `flyline` session. Used by the installer as the default upgrade directory. | *(none)* |
| `FLYLINE_INSTALL_VERSION` | Force the installer to download a specific version tag instead of the latest release. | *(latest release)* |

### Example Custom Installation
To install version `v1.2.0` in a custom directory (`~/apps/lib`):

```bash
export FLYLINE_INSTALL_DIR="~/apps/lib"
export FLYLINE_INSTALL_VERSION="v1.2.0"
curl -sSfL https://github.com/HalFrgrd/flyline/releases/latest/download/install.sh | sh
```
