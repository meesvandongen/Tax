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

# pnpm comes from the packageManager field in web/package.json. If the pnpm on
# PATH is missing or can't switch to that version, get it through a corepack
# recent enough for pnpm 12 (the one bundled with Node is too old).
export COREPACK_ENABLE_DOWNLOAD_PROMPT=0
want=$(node -p "require('./package.json').packageManager.split('@')[1].split('+')[0]")
if [ "$(pnpm --version 2>/dev/null)" != "$want" ]; then
  echo "installing pnpm $want through corepack"
  npm install -g corepack@latest
  corepack enable pnpm
fi

pnpm install --frozen-lockfile
pnpm run build
