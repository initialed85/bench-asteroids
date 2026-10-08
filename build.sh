#!/usr/bin/env bash
# Build the wasm client, generate the JS glue, and build the native server.
set -euo pipefail
cd "$(dirname "$0")"

WB_VERSION=0.2.129

if command -v wasm-bindgen >/dev/null 2>&1; then
  WB=wasm-bindgen
else
  WB=tools/wasm-bindgen
  if [ ! -x "$WB" ]; then
    mkdir -p tools
    url="https://github.com/wasm-bindgen/wasm-bindgen/releases/download/${WB_VERSION}/wasm-bindgen-${WB_VERSION}-x86_64-unknown-linux-musl.tar.gz"
    echo "fetching wasm-bindgen ${WB_VERSION} -> tools/"
    curl -fsSL "$url" -o tools/wasm-bindgen.tar.gz
    tar -xzf tools/wasm-bindgen.tar.gz -C tools --strip-components=1
  fi
fi

echo "building client (wasm32-unknown-unknown)"
cargo build --release --target wasm32-unknown-unknown -p benchy-client

echo "generating web/benchy_client.js"
"$WB" --target web --remove-name-section --out-dir web --out-name benchy_client target/wasm32-unknown-unknown/release/benchy_client.wasm
gzip -9 -k -f web/benchy_client_bg.wasm

echo "building server"
cargo build --release -p benchy-server

echo "done. run: ./run.sh"
