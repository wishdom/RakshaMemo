#!/usr/bin/env bash
set -euo pipefail

# Build and run the native Rust application from any working directory.
if ! command -v cargo >/dev/null 2>&1; then
  echo "Error: Cargo was not found. Install Rust from https://rustup.rs/ and reopen your terminal." >&2
  exit 1
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd -- "$SCRIPT_DIR"
exec cargo run --release --locked -- "$@"
