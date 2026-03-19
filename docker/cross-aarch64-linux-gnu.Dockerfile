FROM ubuntu:16.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    gcc \
    libc6-dev \
    gcc-aarch64-linux-gnu \
    libc6-dev-arm64-cross \
    && rm -rf /var/lib/apt/lists/*

ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
