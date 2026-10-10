#!/usr/bin/env bash
# Browser build: optimised WASM + JS bindings + assets, into dist/.
# Requires:  rustup target add wasm32-unknown-unknown
#             cargo install wasm-bindgen-cli --version <wasm-bindgen version in Cargo.lock>
# Optional:  wasm-opt (binaryen) to shrink the size further.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --target wasm32-unknown-unknown --profile wasm-release --bin psx-souls
rm -rf dist && mkdir -p dist
wasm-bindgen --target web --no-typescript --out-dir dist --out-name psx-souls \
    target/wasm32-unknown-unknown/wasm-release/psx-souls.wasm
if command -v wasm-opt >/dev/null; then
    wasm-opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int dist/psx-souls_bg.wasm -o dist/psx-souls_bg.wasm
fi
cp -r assets dist/
cp web/index.html dist/
echo "dist/ ready ($(du -h dist/psx-souls_bg.wasm | cut -f1) of WASM). Serve with: tools/serve_web.sh"
