#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cargo build \
  --release \
  --manifest-path "$ROOT/crates/host/Cargo.toml" \
  --bin zkpow-host