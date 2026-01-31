#!/bin/bash
set -e

#!/usr/bin/env bash
set -Eeuo pipefail

container_name="flyline-tmp"

cleanup() {
	docker rm -f "$container_name" >/dev/null 2>&1 || true
}
trap cleanup EXIT

docker build --target builder --tag flyline-builder --file Dockerfile .
docker create --name "$container_name" flyline-builder
docker cp "$container_name":/app/target/release/libflyline.so ./libflyline.so
# Container is removed by the trap on EXIT