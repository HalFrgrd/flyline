#!/usr/bin/env bash
set -Eeuo pipefail

mkdir -p docker/build

# Build and load the image into local Docker with a tag
docker buildx build \
    --file docker/Dockerfile.builder \
    --target flyline-built-library \
    --tag flyline-built-library:latest \
    --load \
    .

# Export the built library to a local folder
docker buildx build \
    --file docker/Dockerfile.builder \
    --target flyline-built-library \
    --output type=local,dest=docker/build \
    .
