#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
RUN_DIR="$ROOT/profiling/sp1/$TIMESTAMP"
PROOFS_DIR="$RUN_DIR/proofs"
LATEST_LINK="$ROOT/profiling/sp1/latest"

mkdir -p "$PROOFS_DIR"
ln -sfn "$RUN_DIR" "$LATEST_LINK"

export RUST_LOG="${RUST_LOG:-info}"
export NUM_HEADERS="${NUM_HEADERS:-100}"
export OUTPUT_DIR="$PROOFS_DIR"

if [[ -n "${PREV_PROOF:-}" ]]; then
  export PREV_PROOF
fi

{
  printf 'repo=%s\n' "$ROOT"
  printf 'commit=%s\n' "$(git -C "$ROOT" rev-parse HEAD)"
  printf 'timestamp=%s\n' "$TIMESTAMP"
  printf 'rust_log=%s\n' "$RUST_LOG"
  printf 'num_headers=%s\n' "$NUM_HEADERS"
  printf 'prev_proof=%s\n' "${PREV_PROOF:-}"
  printf 'output_dir=%s\n' "$OUTPUT_DIR"
} > "$RUN_DIR/meta.txt"

finalize_cycle_log() {
  if [[ -f "$RUN_DIR/run.log" ]]; then
    rg 'cycle-tracker-' "$RUN_DIR/run.log" > "$RUN_DIR/cycle-tracker.log" || : > "$RUN_DIR/cycle-tracker.log"
  fi
}
trap finalize_cycle_log EXIT INT TERM

set +e
cargo run --release \
  --manifest-path "$ROOT/script/Cargo.toml" \
  --bin bitcoin-header-chain-script \
  2>&1 | tee "$RUN_DIR/run.log"
status=${PIPESTATUS[0]}
set -e

printf 'profiling output written to %s\n' "$RUN_DIR"
exit "$status"
