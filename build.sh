#!/usr/bin/env bash
set -Eeuo pipefail

mkdir -p docker_build
# container_name="flyline-tmp"

# cleanup() {
# 	docker rm -f "$container_name" >/dev/null 2>&1 || true
# }
# trap cleanup EXIT

# docker build --target flyline-builder-image --tag flyline-builder --file Dockerfile .
# docker create --name "$container_name" flyline-builder
# docker cp "$container_name":/app/target/release/libflyline.so ./docker_build/libflyline.so
# # Container is removed by the trap on EXIT

# this is a better way since we don't need to create and copy from a container
DOCKER_BUILDKIT=1 docker build --file Dockerfile --target flyline_built_library -o docker_build/ .
