#!/usr/bin/env bash
set -Eeuo pipefail

# Build and test the library in a Docker container
docker build --target flyline-builder --tag flyline-builder --file docker/Dockerfile.builder .
docker run --rm flyline-builder bash -lc "cargo test --release --lib"
