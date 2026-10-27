#!/usr/bin/env bash
# Reproducible builds for every contract. Output: target/near/<crate>/<crate>.wasm
set -euo pipefail
cd "$(dirname "$0")"
for c in token ve governor constitution agent; do
  (cd "$c" && cargo near build reproducible-wasm)
done
sha256sum target/near/*/*.wasm
