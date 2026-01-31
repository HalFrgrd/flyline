# Stage 1: Builder - Use Ubuntu 18.04 for glibc 2.27 compatibility
FROM ubuntu:22.04 AS chef

# Prevent interactive prompts during package installation
ENV DEBIAN_FRONTEND=noninteractive

# Install build dependencies
RUN apt-get update && apt-get install -y \
    curl \
    build-essential \
    pkg-config \
    binutils \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"


# Set working directory
WORKDIR /app

RUN cargo install cargo-chef --locked

FROM chef AS planner
COPY --exclude=.git/ --exclude=target/ . . 
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY --exclude=.git/ --exclude=target/ . . 
RUN cargo build --release
