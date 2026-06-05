#!/usr/bin/env bash
set -euo pipefail

ZKPOW_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

LOG="$ZKPOW_ROOT/build.log"

if [ -n "${OUT_DIR:-""}" ]; then
    LOG="$OUT_DIR/build.log"
fi

mkdir -p "$(dirname $LOG)"

cargo build --release \
    --manifest-path "$ZKPOW_ROOT/crates/host/Cargo.toml" \
    -F memory-diagnostics \
    --bin zkpow-host \
  2>&1   | tee "$LOG"
