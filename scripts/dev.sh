#!/usr/bin/env bash
# Runs the Rust backend and the Vite dev server together.
# Backend: http://localhost:8096  |  Frontend: http://localhost:5173 (proxies /api)
set -euo pipefail
cd "$(dirname "$0")/.."

cargo run &
BACKEND_PID=$!
trap 'kill "$BACKEND_PID" 2>/dev/null || true' EXIT

npm --prefix web run dev
