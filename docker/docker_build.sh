#!/usr/bin/env bash
set -Eeuo pipefail

./docker/docker_lib_tests.sh

mkdir -p docker/build
DOCKER_BUILDKIT=1 docker build --file docker/Dockerfile.builder --target flyline-built-library -o docker/build/ .
