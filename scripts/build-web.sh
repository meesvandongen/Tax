#!/usr/bin/env bash
# Build the static site (web/dist) from a clean machine, e.g. Cloudflare
# Workers Builds, whose image has Node but no Rust. Installs a minimal Rust
# toolchain with the wasm32 target if cargo is missing, then runs the web build
# (which compiles the simulation to WebAssembly).
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v cargo >/dev/null 2>&1; then
  if [ -f "$HOME/.cargo/env" ]; then
    . "$HOME/.cargo/env"
  else
    echo "cargo not found; installing a minimal Rust toolchain"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain stable --target wasm32-unknown-unknown
    . "$HOME/.cargo/env"
  fi
fi
rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true

cd web
npm ci
npm run build
