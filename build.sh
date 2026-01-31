#!/usr/bin/env bash
set -Eeuo pipefail

mkdir -p docker_build

# docker build --target builder --tag flyline-test_builder --file Dockerfile .
# # Run unit tests inside the builder image; use bash -lc to execute the command
# docker run --rm flyline-test_builder bash -lc "cargo test --release --lib"


# This doesnt build a container. The only file in the image's filesystem will be the built library
DOCKER_BUILDKIT=1 docker build --file Dockerfile --target flyline_built_library -o docker_build/ .

