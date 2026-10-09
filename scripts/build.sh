#!/usr/bin/env bash
# Build the optimized contract WASM -> target/wasm32v1-none/release/cadence.wasm
set -euo pipefail
cd "$(dirname "$0")/.."
stellar contract build
ls -lh target/wasm32v1-none/release/cadence.wasm
