# ==============================================================================
# ISSUE SUMMARIES
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
# - Root Cause: `flyline_dummy_allocator_keep_alive()` was never executed, so
#   LLVM's Dead Allocation Elimination (DSE) pass recognized that paired malloc/free
#   calls without intermediate reads or writes have zero observable side effects,
#   optimizing the function body down to `ret void`. Linker LTO / `--gc-sections`
#   then stripped unreferenced `mi_*` symbols from the dynamic symbol table.
# - Solution: Execute `ensure_mimalloc_symbols_retained()` inside `flyline_load_common()`
#   using `std::hint::black_box(false)`. This forces LTO to retain `mi_*` allocator
#   symbols in `libflyline.so`'s dynamic symbol table with zero runtime allocation overhead.
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
CMD ["bash", "-i", "-c", "enable -f /usr/lib/bash/libflyline.so flyline && echo 'SUCCESS: flyline AUR package loaded successfully!'"]
