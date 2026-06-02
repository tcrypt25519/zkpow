#!/usr/bin/env bash
set -euo pipefail

# This script runs batches of a given size continuously by repeatedly invoking
# the same binary path used for single-batch proving.

ZKPOW_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
ENV_FILE="${ENV_FILE:-$ZKPOW_ROOT/.env}"
OUT_DIR="$ZKPOW_ROOT/profiling/runs/$TIMESTAMP"

# Defaults
export NUM_HEADERS="${NUM_HEADERS:-100}"
export RUST_LOG="${RUST_LOG:-info}"
export GUEST_PROFILING="${GUEST_PROFILING:-0}"
export MAX_BATCHES="${MAX_BATCHES:-}"

# Starting state
PREV_PROOF="${PREV_PROOF:-}"
BATCH_COUNT=0

# Load environment variables from .env file if it exists
if [[ -f "${ENV_FILE:=$ZKPOW_ROOT/.env}" ]]; then
  source "$ENV_FILE"
fi

mkdir -p "$OUT_DIR"
printf "Starting ivc run profiling session at %s\n" "$TIMESTAMP"
printf "Batch size: %s headers\n" "$NUM_HEADERS"
printf "Batch count: %s\n" "${MAX_BATCHES:-all}"
printf "Output root: %s\n\n" "$OUT_DIR"

if [[ "${BUILD:=true}" == "true" ]]; then
  cargo build --release \
    --manifest-path "$ZKPOW_ROOT/crates/host/Cargo.toml" \
    --bin zkpow-host \
    2>&1 | tee "$OUT_DIR/build.log" && export BUILD=false
fi

while true; do
  if [[ -n "$MAX_BATCHES" && "$BATCH_COUNT" -ge "$MAX_BATCHES" ]]; then
    printf "Reached MAX_BATCHES=%s. Stopping.\n" "$MAX_BATCHES"
    break
  fi

  BATCH_COUNT=$((BATCH_COUNT + 1))
  BATCH_DIR="$OUT_DIR/batch_$BATCH_COUNT"
  mkdir -p "$BATCH_DIR"

  printf "=== Starting Batch %d ===\n" "$BATCH_COUNT"

  # Run the standard profiling script for one batch
  # We override OUT_DIR and OUTPUT_DIR to keep batch outputs segregated
  export OUT_DIR="$BATCH_DIR"
  export OUTPUT_DIR="$BATCH_DIR/proofs"

  # Run the script. It exits with the status of the cargo run.
  if ! "$ZKPOW_ROOT/scripts/prove-batch.sh"; then
    printf "\nError: Batch %d failed. Stopping.\n" "$BATCH_COUNT"
    exit 1
  fi

  # Find the generated proof to pass to the next batch
  # The host script names proofs like proof_height_X_to_Y.bin (compressed)
  # We look for the .bin file (excluding groth16) in the batch's proof directory.
  NEXT_PROOF=$(find "$OUTPUT_DIR" -name "*.bin" ! -name "*groth16*" | head -n 1)

  if [[ -z "$NEXT_PROOF" ]]; then
    printf "\nError: No compressed proof found in %s. Stopping.\n" "$OUTPUT_DIR"
    exit 1
  fi

  export PREV_PROOF="$NEXT_PROOF"
  printf "=== Batch %d complete. Next proof: %s ===\n\n" "$BATCH_COUNT"
done

unset $PREV_PROOF
