#!/usr/bin/env bash
set -euo pipefail

# This script proves an arbitrary number of batches of headers by invoking the
# prover `MAX_BATCHES` times and passing the previous proof to the next batch.
# Each batch will prove `NUM_HEADERS` headers and write output to `RUN_DIR`.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Load environment variables from .env file if it exists
# Do this first so it can pre-set any variables that are needed for the run
ENV_FILE="${ENV_FILE:-$ROOT/.env}"
if [[ -f "${ENV_FILE:-}" ]]; then
  source "$ENV_FILE"
fi

# Run parameters
TIMESTAMP="$(date -u +%Y%m%dT%H%M%S)"
RUN_DIR="$ROOT/profiling/runs/$TIMESTAMP"
BUILD="${BUILD:=true}"
export NUM_HEADERS="${NUM_HEADERS:-100}"
export MAX_BATCHES="${MAX_BATCHES:-}"
export RUST_LOG="${RUST_LOG:-info}"
export GUEST_PROFILING="${GUEST_PROFILING:-0}"
mkdir -p "$RUN_DIR"

# Print run info
printf "Starting ivc run profiling session at %s\n" "$TIMESTAMP"
printf "Using .env: %s\n" "(cat ${ENV_FILE:-dev/null})"
printf "Batch size: %s headers\n" "$NUM_HEADERS"
printf "Batch count: %s\n" "${MAX_BATCHES:-all}"
printf "Build requested: %s\n" "${BUILD:-no}"
printf "Guest profiling: %s\n" "${GUEST_PROFILING:-no}"
printf "Output root: %s\n\n" "$RUN_DIR"

if [[ "${BUILD:-}" == "true" ]]; then
  echo "Building..."
  sh "$ROOT/scripts/build_prover.sh" 2>&1 | tee "$RUN_DIR/build.log"
fi

# Set intiail state and prove batches until we're done
PREV_PROOF="${PREV_PROOF:-}"
BATCH_COUNT=0
while true; do
  if [[ -n "$MAX_BATCHES" && "$BATCH_COUNT" -ge "$MAX_BATCHES" ]]; then
    printf "Reached MAX_BATCHES=%s. Stopping.\n" "$MAX_BATCHES"
    break
  fi

  # We override RUN_DIR and OUTPUT_DIR to keep batch outputs segregated
  BATCH_DIR="$RUN_DIR/batch_$BATCH_COUNT"
  RUN_DIR="$BATCH_DIR"
  export OUTPUT_DIR="$BATCH_DIR/proofs"
  mkdir -p "$OUTPUT_DIR"

  # Run the batch. It exits with the status of the cargo run.
  printf "=== Starting Batch %d ===\n" "$BATCH_COUNT"
  if ! "$ROOT/scripts/prove-batch.sh"; then
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

  BATCH_COUNT=$((BATCH_COUNT + 1))
  export PREV_PROOF="$NEXT_PROOF"
  printf "=== Batch %d complete with proof: %s ===\n\n" "$BATCH_COUNT" "$NEXT_PROOF"
done

unset $PREV_PROOF
