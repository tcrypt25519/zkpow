#!/usr/bin/env bash
set -euo pipefail

# This script runs batches of a given size continuously by repeatedly invoking
# the same binary path used for single-batch proving. Resume state lives in
# the controller's run ledger, so each invocation continues the chain
# automatically; per-batch profiling output stays segregated.

ZKPOW_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
ENV_FILE="${ENV_FILE:-$ZKPOW_ROOT/.env}"
OUT_DIR="$ZKPOW_ROOT/profiling/runs/$TIMESTAMP"

# Defaults
export ZKPOW_BATCH_SIZE="${ZKPOW_BATCH_SIZE:-100}"
export RUST_LOG="${RUST_LOG:-info}"
export ZKPOW_GUEST_PROFILING="${ZKPOW_GUEST_PROFILING:-0}"
CHAIN_BATCH_COUNT="${ZKPOW_BATCH_COUNT:-}"

# One run id, ledger, and artifact store for the whole chain: the controller
# tracks verified progress and re-serves the next batch on each invocation.
export ZKPOW_RUN_ID="${ZKPOW_RUN_ID:-chain-$TIMESTAMP}"
export ZKPOW_LEDGER_PATH="${ZKPOW_LEDGER_PATH:-$OUT_DIR/ledger.db}"
export ZKPOW_PROOF_STORE_DIR="${ZKPOW_PROOF_STORE_DIR:-$OUT_DIR/proofs}"

BATCH_COUNT=0

# Load environment variables from .env file if it exists
if [[ -f "${ENV_FILE:=$ZKPOW_ROOT/.env}" ]]; then
  source "$ENV_FILE"
fi

mkdir -p "$OUT_DIR"
printf "Starting ivc run profiling session at %s\n" "$TIMESTAMP"
printf "Run id: %s\n" "$ZKPOW_RUN_ID"
printf "Batch size: %s headers\n" "$ZKPOW_BATCH_SIZE"
printf "Batch count: %s\n" "${CHAIN_BATCH_COUNT:-all}"
printf "Output root: %s\n\n" "$OUT_DIR"

if [[ "${BUILD:=true}" == "true" ]]; then
  cargo build --release \
    --manifest-path "$ZKPOW_ROOT/crates/host/Cargo.toml" \
    --bin zkpow-host \
    2>&1 | tee "$OUT_DIR/build.log" && export BUILD=false
fi

while true; do
  if [[ -n "$CHAIN_BATCH_COUNT" && "$BATCH_COUNT" -ge "$CHAIN_BATCH_COUNT" ]]; then
    printf "Reached ZKPOW_BATCH_COUNT=%s. Stopping.\n" "$CHAIN_BATCH_COUNT"
    break
  fi

  BATCH_COUNT=$((BATCH_COUNT + 1))
  BATCH_DIR="$OUT_DIR/batch_$BATCH_COUNT"
  mkdir -p "$BATCH_DIR"

  printf "=== Starting Batch %d ===\n" "$BATCH_COUNT"

  # Keep per-batch profiling logs segregated; the host proves exactly one
  # batch per invocation and the ledger carries the chain forward.
  export OUT_DIR="$BATCH_DIR"

  if ! ZKPOW_BATCH_COUNT=1 "$ZKPOW_ROOT/scripts/prove-batch.sh"; then
    printf "\nError: Batch %d failed. Stopping.\n" "$BATCH_COUNT"
    exit 1
  fi

  printf "=== Batch %d complete ===\n\n" "$BATCH_COUNT"
done
