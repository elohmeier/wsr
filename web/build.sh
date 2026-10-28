#!/usr/bin/env bash
# Build the engine and the static Svelte app. Install UI dependencies with npm ci --ignore-scripts.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
bash "$root/web/build-wasm.sh"
npm --prefix "$root/web" run build:ui
node "$root/web/license-notices.mjs"
