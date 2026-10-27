#!/usr/bin/env bash
# Builds every tool as a WASM component into dist/<tool>.wasm and prints SHA256s.
set -euo pipefail
cd "$(dirname "$0")"
rustup target add wasm32-wasip2 >/dev/null
cargo build --release --target wasm32-wasip2
mkdir -p dist
for f in target/wasm32-wasip2/release/*.wasm; do cp "$f" dist/; done
sha256sum dist/*.wasm
