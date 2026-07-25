# ==============================================================================
# ISSUE SUMMARIES & ROOT CAUSE EXPLANATION
# ==============================================================================
# Issue #780 (https://github.com/HalFrgrd/flyline/issues/780):
# - Problem: Pre-compiled GitHub release binary (v1.2.5) failed to load via
#   `enable -f libflyline.so` with `undefined symbol: mi_free` on Arch Linux.
# - Initial Attempt (v1.2.6): Added an uncalled helper function
#   (`flyline_dummy_allocator_keep_alive`) referencing `mi_free` / `mi_malloc`
#   to prevent LTO from stripping mimalloc C allocator symbols.
#
# Issue #802 (https://github.com/HalFrgrd/flyline/issues/802):
# - Problem: AUR package users reported `enable -f libflyline.so` failed on v1.3.0
#   with `undefined symbol: mi_free` or `mi_malloc_aligned`.
# - Root Cause & LLVM Inlining: Under Link-Time Optimization (`lto = true`), LLVM
#   inlines mimalloc's fast-path C allocation routines (like inline page frees)
#   directly into calling Rust code. Because allocation routines were inlined
#   everywhere, no compiled LLVM IR instruction called the standalone C allocator
#   functions (`@mi_free`, `@mi_malloc_aligned`) directly anymore.
#   When the Arch Linux linker ran `--gc-sections` and `--strip-all`, it discarded
#   the standalone function definitions, leaving undefined dynamic symbol entries
#   (`U mi_free`) in `.dynsym` that caused `dlopen()` / `enable -f` to fail.
#
# - Solution (`ensure_mimalloc_symbols_retained`):
#   We execute `GLOBAL.alloc(layout)` and `GLOBAL.dealloc(ptr, layout)` with
#   `std::hint::black_box(ptr)` inside `ensure_mimalloc_symbols_retained()`, invoked
#   unconditionally during `flyline_load_common()`. `std::hint::black_box(ptr)` acts as an
#   opaque optimization barrier, forcing LLVM and the linker to retain mimalloc's C object
#   code inside `libflyline.so`. This prevents Dead Allocation Elimination (DSE) and
#   symbol stripping from discarding the allocator engine, keeping `libflyline.so`
#   100% self-contained with 0 undefined dynamic symbol demands.
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
