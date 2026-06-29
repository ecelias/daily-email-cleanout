#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP="$ROOT/desktop"

if ! xcode-select -p >/dev/null 2>&1; then
  echo "Xcode Command Line Tools are required. Run: xcode-select --install" >&2
  exit 1
fi

if ! command -v node >/dev/null 2>&1; then
  echo "Node.js 20 or newer is required." >&2
  exit 1
fi

if [[ ! -x "$HOME/.cargo/bin/cargo" ]] && ! command -v cargo >/dev/null 2>&1; then
  echo "Rust is required. Install it from https://rustup.rs" >&2
  exit 1
fi

export PATH="$HOME/.cargo/bin:$PATH"

cd "$DESKTOP"
npm install
npm run check
npm run test
npm run desktop:build

echo
echo "Build complete:"
find src-tauri/target/release/bundle -maxdepth 3 \
  \( -name "*.app" -o -name "*.dmg" \) -print
