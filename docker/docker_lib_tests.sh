#!/usr/bin/env bash
set -Eeuo pipefail

# Build and test the library in a Docker container
docker run --rm flyline-builder bash -lc "cargo test --release --lib"
