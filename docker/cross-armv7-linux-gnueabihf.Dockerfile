FROM ubuntu:16.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    gcc \
    libc6-dev \
    gcc-arm-linux-gnueabihf \
    libc6-dev-armhf-cross \
    && rm -rf /var/lib/apt/lists/*

ENV CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc
