#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

LOG="$ROOT/build.log"

if [ -n "${RUN_DIR:-""}" ]; then
    LOG="$RUN_DIR/build.log"
fi

mkdir -p "$(dirname $LOG)"

cargo build --release \
    --manifest-path "$ROOT/crates/host/Cargo.toml" \
    -F memory-diagnostics \
    --bin zkpow-host \
  2>&1   | tee "$LOG"
