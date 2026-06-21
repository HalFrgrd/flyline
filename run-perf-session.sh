#!/usr/bin/env bash

# Compile the library in release mode for accurate profiling
echo "Building flyline in release mode..."
cargo build --release || exit 1

EXPORT_PATH="$(pwd)/perf_stats.json"
echo "Starting interactive bash session with performance tracking..."
echo "Profiling statistics will be written to: $EXPORT_PATH"
echo ""
echo "=========================================================="
echo "To start flyline in the new shell, run the following command:"
echo "enable -f $(pwd)/target/release/libflyline.so flyline"
echo "=========================================================="
echo ""

# Launch bash with the metrics output target configured
FLYLINE_PERF_STATS="$EXPORT_PATH" bash --norc --noprofile
