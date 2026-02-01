#!/usr/bin/env bash
set -Eeuo pipefail

mkdir -p docker/build

# Build and load the image into local Docker with a tag
docker build \
    --file docker/release_builder.Dockerfile \
    --target flyline-builder \
    --tag flyline-builder:latest \
    .

# Build and load the image into local Docker with a tag
docker build \
    --file docker/release_builder.Dockerfile \
    --target flyline-extracted-library \
    --tag flyline-extracted-library:latest \
    .

# Export the built library to a local folder
docker build \
    --file docker/release_builder.Dockerfile \
    --target flyline-extracted-library \
    -o docker/build \
    .
