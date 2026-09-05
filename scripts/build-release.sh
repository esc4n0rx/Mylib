#!/usr/bin/env bash
# Produces a single release artifact: web bundle embedded into the server binary.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> Building web frontend"
npm --prefix web ci
npm --prefix web run build

echo "==> Building server (embeds web/dist)"
cargo build --release

echo "==> Done: target/release/mylib-server"
