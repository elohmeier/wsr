#!/usr/bin/env bash
# Build the engine to wasm and copy it next to the page. Serve with: python3 web/serve.py
# (it adds the cross-origin isolation headers the worker's SharedArrayBuffer bridge needs)
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cargo build --manifest-path "$root/engine/Cargo.toml" -p wsr-wasm --target wasm32-unknown-unknown --release
cp "$root/engine/target/wasm32-unknown-unknown/release/wsr_wasm.wasm" "$root/web/wsr.wasm"
ls -la "$root/web/wsr.wasm"
