# Stage 1: Builder - Use Ubuntu 18.04 for glibc 2.27 compatibility
FROM ubuntu:22.04 AS builder

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


# Build directly from the repository source


# # Copy the real source code
COPY Cargo.toml Cargo.lock ./
COPY src ./src


# Build the actual library
RUN cargo build --release
