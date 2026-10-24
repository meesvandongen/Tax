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

# pnpm comes from the packageManager field in web/package.json, through corepack.
export COREPACK_ENABLE_DOWNLOAD_PROMPT=0
if ! corepack enable pnpm 2>/dev/null; then
  # Node's bin directory is not writable: put the pnpm shim in a user directory.
  mkdir -p "$HOME/.local/bin"
  corepack enable --install-directory "$HOME/.local/bin" pnpm
  export PATH="$HOME/.local/bin:$PATH"
fi

cd web
pnpm install --frozen-lockfile
pnpm run build
