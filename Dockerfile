# Multi stage docker build using cargo chef.
# https://github.com/LukeMathWalker/cargo-chef
# https://lpalmieri.com/posts/fast-rust-docker-builds/
# the whole idea is to build dependencies in a separate stage and let docker cache them
# so that we don't have to recompile all dependencies on every code change.

# Stage 1: Builder - Use Ubuntu 16.04 for glibc 2.23 compatibility
FROM ubuntu:16.04 AS chef

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

# Stage 2: Planner
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json


# Stage 3: Final Build
FROM chef AS flyline-builder-image
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release
