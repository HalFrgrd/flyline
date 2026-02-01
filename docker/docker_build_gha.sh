#!/usr/bin/env bash
set -Eeuo pipefail

# Build and load the image into local Docker with a tag
docker buildx build \
    --progress=plain \
    --file docker/release_builder.Dockerfile \
    --target flyline-builder \
    --tag flyline-builder:latest \
    --load \
    --cache-to=type=gha,mode=max \
    --cache-from=type=gha,mode=max \
    .
