#!/usr/bin/env bash
set -euo pipefail

# Unconditionally set ROOT and TIMESTAMP
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"

# Load environment variables from .env file if it exists
if [[ -f "$ROOT/.env" ]]; then
  source "$ROOT/.env"
fi

# Set defaults for run configuration
RUN_DIR="${RUN_DIR:-$ROOT/profiling/sp1/$TIMESTAMP}"
LATEST_LINK="${LATEST_LINK:-$ROOT/profiling/sp1/latest}"
PROOFS_DIR="${PROOFS_DIR:-$RUN_DIR/proofs}"

# Set defaults for the guest program
unset sp1_core
export SP1_ENABLE_TOKIO_CONSOLE="${SP1_ENABLE_TOKIO_CONSOLE:-true}"
export TRACE_FILE="${TRACE_FILE:-$RUN_DIR/tracing.json}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
export RUST_LOG="${RUST_LOG:-info}"
export NUM_HEADERS="${NUM_HEADERS:-100}"
export OUTPUT_DIR="${OUTPUT_DIR:-$PROOFS_DIR}"

# Force line-buffered output for proper tee capture
export RUST_TEST_NOCAPTURE=1
export CARGO_TERM_COLOR=never

mkdir -p "$RUN_DIR"
mkdir -p "$PROOFS_DIR"
mkdir -p "$(dirname "$LATEST_LINK")"
ln -sfn "$RUN_DIR" "$LATEST_LINK"

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
} >"$RUN_DIR/meta.txt"

# finalize_cycle_log() {
#   if [[ -f "$RUN_DIR/run.log" ]]; then
#     rg 'cycle-tracker-' "$RUN_DIR/run.log" >"$RUN_DIR/cycle-tracker.log" || : >"$RUN_DIR/cycle-tracker.log"
#   fi
# }
# trap finalize_cycle_log EXIT INT TERM

set +e
# `stdbuf` works by injecting a preload library into the child process tree.
# On macOS that leaks into Apple toolchain subprocesses (for example `xcrun`)
# and can abort linking before the actual build runs. Keep the safer plain
# `cargo run` path on Darwin; use `stdbuf` elsewhere when available.
if [[ "$(uname -s)" == "Darwin" ]]; then
  cargo run --release \
    --manifest-path "$ROOT/crates/host/Cargo.toml" \
    --bin zkpow-host \
    2>&1 | tee "$RUN_DIR/run.log"
elif command -v stdbuf >/dev/null 2>&1; then
  stdbuf -oL -eL cargo run --release \
    --manifest-path "$ROOT/crates/host/Cargo.toml" \
    --bin zkpow-host \
    2>&1 | tee "$RUN_DIR/run.log"
else
  cargo run --release \
    --manifest-path "$ROOT/crates/host/Cargo.toml" \
    --bin zkpow-host \
    2>&1 | tee "$RUN_DIR/run.log"
fi
status=${PIPESTATUS[0]}
set -e

report_file="$RUN_DIR/report.txt"
timings_file="$RUN_DIR/timings.txt"
cycle_tracker_file="$RUN_DIR/cycle-tracker.txt"
prover_gas_file="$RUN_DIR/prover-gas.txt"

{
  rg 'Execution report|cycle tracker:|top hot spans:|cycle hierarchy:|prover gas:|total prover_gas|estimated gas by hot span|assumptions:|TOTAL PROVING TIME|Total proving time|Proving time breakdown:' "$RUN_DIR/run.log" || true
} >"$report_file"

{
  rg 'finished in|TOTAL PROVING TIME|Total proving time|Proving time breakdown:' "$RUN_DIR/run.log" || true
} >"$timings_file"

{
  rg 'cycle tracker:|top hot spans:|cycle hierarchy:|cycles' "$RUN_DIR/run.log" || true
} >"$cycle_tracker_file"

{
  rg 'prover gas:|total prover_gas|estimated gas by hot span|assumptions:' "$RUN_DIR/run.log" || true
} >"$prover_gas_file"

printf '\n=== Run Summary ===\n'
printf 'RUN_DIR: %s\n' "$RUN_DIR"
printf 'PROOFS_DIR: %s\n' "$PROOFS_DIR"
printf 'OUTPUT_DIR: %s\n' "$OUTPUT_DIR"
printf 'run.log: %s\n' "$RUN_DIR/run.log"
printf 'report: %s\n' "$report_file"
printf 'timings: %s\n' "$timings_file"
printf 'cycle tracker report: %s\n' "$cycle_tracker_file"
printf 'prover gas report: %s\n' "$prover_gas_file"
printf 'profiling output written to %s\n' "$RUN_DIR"
printf '\nProofs should be in: %s\n' "$OUTPUT_DIR"
ls -la "$OUTPUT_DIR" 2>/dev/null || printf '(directory may be empty if proving failed)\n'
exit "$status"
