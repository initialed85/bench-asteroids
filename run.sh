#!/usr/bin/env bash
# Serve the game on 0.0.0.0:11111 (run ./build.sh first).
set -euo pipefail
cd "$(dirname "$0")"
exec ./target/release/benchy-server
