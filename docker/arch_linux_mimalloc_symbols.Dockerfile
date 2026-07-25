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

# Dockerfile to reproduce Issue #802 with cached dependency builds using cargo-chef
FROM archlinux:latest AS chef

RUN pacman -Sy --noconfirm \
    rust \
    gcc \
    bash \
    git \
    pkgconf \
    make \
    binutils \
    && rm -rf /var/cache/pacman/pkg/*

RUN cargo install cargo-chef --locked
WORKDIR /flyline

FROM chef AS planner
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
COPY examples ./examples
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /flyline/recipe.json recipe.json
ENV CARGO_PROFILE_RELEASE_LTO=true
ENV RUSTFLAGS="-C link-arg=-Wl,-z,pack-relative-relocs -C link-arg=-Wl,-O1,--sort-common,--as-needed,-z,relro,-z,now"
RUN cargo chef cook --release --recipe-path recipe.json

COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
COPY examples ./examples
COPY tests ./tests
RUN cargo build --release

# Inspect dynamic symbol table for undefined mimalloc (mi_) symbols
RUN nm -D target/release/libflyline.so | grep " U mi_" || true

# Test loading libflyline.so in interactive bash
CMD ["bash", "-i", "-c", "enable -f target/release/libflyline.so flyline && echo 'SUCCESS: flyline loaded successfully!'"]
