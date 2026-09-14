#!/bin/sh
# Build the real Xilem widget tree for the browser; desktop dependencies stay untouched.
set -eu
cd "$(dirname "$0")/.."
python3 web/prepare.py
cargo build --manifest-path web/Cargo.toml --locked --target wasm32-unknown-unknown --release
bindgen_tool=${RUNEBENDER_WASM_BINDGEN:-wasm-bindgen}
if [ "$("$bindgen_tool" --version)" != 'wasm-bindgen 0.2.127' ]; then
  echo 'This lockfile needs wasm-bindgen-cli 0.2.127. Set RUNEBENDER_WASM_BINDGEN to that executable.' >&2
  exit 1
fi
"$bindgen_tool" web/target/wasm32-unknown-unknown/release/runebender_browser.wasm --target web --out-dir web/pkg
