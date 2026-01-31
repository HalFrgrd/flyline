#!/usr/bin/env bash
set -Eeuo pipefail

echo "cwd: $(pwd)"

docker buildx build \
    --file "docker/Dockerfile.builder" \
    --target flyline-builder \
    --tag flyline-builder:latest \
    --cache-from type=gha,scope=builtlib \
    --cache-to type=gha,mode=max,scope=builtlib \
    --load \
    "."

docker buildx build \
    --file "docker/Dockerfile.builder" \
    --target flyline-extracted-library \
    --tag flyline-extracted-library:latest \
    --cache-from type=gha,scope=builtlib \
    --cache-to type=gha,mode=max,scope=builtlib \
    --load \
    "."
