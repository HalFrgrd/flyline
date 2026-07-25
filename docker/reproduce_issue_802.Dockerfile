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
