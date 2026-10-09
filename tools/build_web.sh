#!/usr/bin/env bash
# Browser build: optimised WASM + JS bindings + assets, into dist/.
# Requires:  rustup target add wasm32-unknown-unknown
#             cargo install wasm-bindgen-cli --version <wasm-bindgen version in Cargo.lock>
# Optional:  wasm-opt (binaryen) to shrink the size further.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --target wasm32-unknown-unknown --profile wasm-release --bin giants-flame
rm -rf dist && mkdir -p dist
wasm-bindgen --target web --no-typescript --out-dir dist --out-name giants-flame \
    target/wasm32-unknown-unknown/wasm-release/giants-flame.wasm
if command -v wasm-opt >/dev/null; then
    wasm-opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int dist/giants-flame_bg.wasm -o dist/giants-flame_bg.wasm
fi
cp -r assets dist/
cp web/index.html dist/
echo "dist/ ready ($(du -h dist/giants-flame_bg.wasm | cut -f1) of WASM). Serve with: tools/serve_web.sh"
