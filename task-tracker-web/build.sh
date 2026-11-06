#!/usr/bin/env bash
# Build the WebAssembly site into ./dist (requires the wasm32-unknown-unknown
# target and wasm-bindgen-cli matching the version in Cargo.lock).
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown
rm -rf dist
mkdir -p dist
cp www/* dist/
wasm-bindgen --target web --no-typescript --out-dir dist/pkg \
  target/wasm32-unknown-unknown/release/task_tracker_web.wasm
echo "Built site in $(pwd)/dist"
