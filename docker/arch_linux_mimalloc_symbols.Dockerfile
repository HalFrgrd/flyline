# ==============================================================================
# ISSUE SUMMARY & ROOT CAUSE EXPLANATION
# ==============================================================================
# Issue #780 & #802 (Arch Linux AUR Package / `enable -f` `undefined symbol: mi_free`):
#
# - Problem:
#   Building `flyline` on Arch Linux using `makepkg` caused dynamic plugin loading
#   via `enable -f /usr/lib/bash/libflyline.so flyline` to fail with:
#     `bash: enable: cannot open shared object libflyline.so: undefined symbol: mi_free`
#
# - The Full Picture & Why Arch Linux Is Affected:
#   1. Rust PR #146232 ("Make the allocator shim participate in LTO again"):
#      Under release profiles with `lto = true`, `rustc` includes the global allocator shim
#      in LLVM LTO dead-code elimination. Because static C functions in `libmimalloc-sys.a`
#      (like `mi_free` and `mi_malloc_aligned`) are not directly invoked by a `#[no_mangle]`
#      Rust entry point, LLVM LTO prunes them from the compiled bitcode.
#   2. `rustc` Version Script:
#      `rustc` auto-generates a dynamic linker `--version-script` (`local: *;`) for `cdylib`
#      targets, marking non-`#[no_mangle]` C static functions in `libmimalloc-sys.a` as hidden.
#   3. Arch Linux `makepkg` Flags (`-Wl,--as-needed` & GCC `-flto`):
#      Arch Linux's `/etc/makepkg.conf` sets `RUSTFLAGS="-C link-arg=-Wl,--as-needed"` and
#      `OPTIONS=(... lto ...)` (which injects GCC `-flto` into `LDFLAGS`).
#      Under `-Wl,--as-needed`, GNU `ld` converts hidden static C symbols stripped by LTO
#      into undefined dynamic import entries (`U mi_free`) in `.dynsym`, expecting the host
#      process (Bash) to resolve them at runtime. When `dlopen()` runs in Bash, loading fails.
#
# - Solution (`options=('!lto')` in PKGBUILD):
#   Adding `options=('!lto')` to `PKGBUILD` instructs `makepkg` to disable system GCC `-flto`
#   flags for the build process. Without system `-flto` interference, `libmimalloc-sys.a`'s
#   static machine code is cleanly embedded in `libflyline.so`'s `.text` section. GNU `ld`
#   under `-Wl,--as-needed` resolves all internal C calls locally inside the shared library.
#   This guarantees `libflyline.so` is 100% self-contained with 0 undefined dynamic `mi_*`
#   symbols across all Arch Linux builds.
# ==============================================================================

FROM archlinux:latest

RUN pacman -Sy --noconfirm \
    base-devel \
    rust \
    bash \
    git \
    sudo \
    && rm -rf /var/cache/pacman/pkg/*

# Create a non-root builder user required by makepkg
RUN useradd -m builder && \
    echo "builder ALL=(ALL) NOPASSWD: ALL" >> /etc/sudoers

WORKDIR /home/builder/build
COPY . src/
COPY docker/PKGBUILD PKGBUILD

RUN chown -R builder:builder /home/builder

USER builder
RUN makepkg --noconfirm -s

USER root
RUN pacman -U flyline-*.pkg.tar.zst --noconfirm

# Inspect dynamic symbol table of installed package for undefined mimalloc (mi_) symbols
RUN nm -D /usr/lib/bash/libflyline.so | grep " U mi_" || true

# Test loading the installed flyline package in interactive bash
RUN bash -i -c "enable -f /usr/lib/bash/libflyline.so flyline && echo 'SUCCESS: flyline AUR package loaded successfully!'"
