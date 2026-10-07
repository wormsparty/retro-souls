#!/usr/bin/env bash
# Build navigateur : WASM optimisé + bindings JS + assets, dans dist/.
# Prérequis : rustup target add wasm32-unknown-unknown
#             cargo install wasm-bindgen-cli --version <version de wasm-bindgen dans Cargo.lock>
# Optionnel : wasm-opt (binaryen) pour réduire encore la taille.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --target wasm32-unknown-unknown --profile wasm-release --bin souls
rm -rf dist && mkdir -p dist
wasm-bindgen --target web --no-typescript --out-dir dist --out-name souls \
    target/wasm32-unknown-unknown/wasm-release/souls.wasm
if command -v wasm-opt >/dev/null; then
    wasm-opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int dist/souls_bg.wasm -o dist/souls_bg.wasm
fi
cp -r assets dist/
cp web/index.html dist/
echo "dist/ prêt ($(du -h dist/souls_bg.wasm | cut -f1) de WASM). Servir avec : tools/serve_web.sh"
