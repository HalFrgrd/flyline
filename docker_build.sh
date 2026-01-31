#!/usr/bin/env bash
set -Eeuo pipefail

# Build and test the library in a Docker container
docker build --target builder --tag flyline-test_builder --file Dockerfile .
docker run --rm flyline-test_builder bash -lc "cargo test --release --lib"

# Build the flyline library as a separate output
mkdir -p docker_build
DOCKER_BUILDKIT=1 docker build --file Dockerfile --target flyline_built_library -o docker_build/ .
