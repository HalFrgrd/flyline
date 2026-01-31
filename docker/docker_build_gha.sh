#!/usr/bin/env bash
set -Eeuo pipefail

echo "cwd: $(pwd)"

mkdir docker/build

docker buildx build \
    --file "docker/Dockerfile.builder" \
    --target flyline_built_library \
    --cache-from type=gha,scope=builtlib \
    --cache-to type=gha,mode=max,scope=builtlib \
    --output type=local,dest="docker/build" \
    "."
