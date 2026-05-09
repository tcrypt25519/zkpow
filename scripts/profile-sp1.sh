#!/usr/bin/env bash
set -euo pipefail

# Unconditionally set ROOT and TIMESTAMP
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"

if [[ -v "$SP1_ENABLE_TOKIO_CONSOLE" ]]; then
  CALLER_SP1_ENABLE_TOKIO_CONSOLE_SET=1
  CALLER_SP1_ENABLE_TOKIO_CONSOLE="$SP1_ENABLE_TOKIO_CONSOLE"
else
  CALLER_SP1_ENABLE_TOKIO_CONSOLE_SET=0
  CALLER_SP1_ENABLE_TOKIO_CONSOLE=""
fi

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
if [[ "$CALLER_SP1_ENABLE_TOKIO_CONSOLE_SET" == "1" ]]; then
  export SP1_ENABLE_TOKIO_CONSOLE="$CALLER_SP1_ENABLE_TOKIO_CONSOLE"
else
  export SP1_ENABLE_TOKIO_CONSOLE="false"
fi
export TRACE_FILE="${TRACE_FILE:-$RUN_DIR/tracing.json}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
export RUST_LOG="${RUST_LOG:-info}"
export NUM_HEADERS="${NUM_HEADERS:-100}"
export OUTPUT_DIR="${OUTPUT_DIR:-$PROOFS_DIR}"
export GUEST_PROFILING="${GUEST_PROFILING:-0}"
export PROVE_COMPRESSED_SPANS="${PROVE_COMPRESSED_SPANS:-0}"

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
  printf 'guest_profiling=%s\n' "$GUEST_PROFILING"
  printf 'prove_compressed_spans=%s\n' "$PROVE_COMPRESSED_SPANS"
  printf 'prev_proof=%s\n' "${PREV_PROOF:-}"
  printf 'output_dir=%s\n' "$OUTPUT_DIR"
} >"$RUN_DIR/meta.txt"

rm -f "$ROOT/logs/run.jsonl"

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
chip_byte_interactions_file="$RUN_DIR/chip-byte-interactions.txt"
structured_log_file="$RUN_DIR/run.jsonl"
prove_compressed_spans_file="$RUN_DIR/prove-compressed-spans.txt"

if [[ -f "$ROOT/logs/run.jsonl" ]]; then
  cp "$ROOT/logs/run.jsonl" "$structured_log_file"
else
  : >"$structured_log_file"
fi

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

{
  printf 'These are static SP1 AIR byte-interaction counts emitted while building prover chips; they are not per-run byte-volume measurements.\n'
  rg 'chip .* has [0-9]+ byte interactions' "$RUN_DIR/run.log" || true
} >"$chip_byte_interactions_file"

if [[ -s "$structured_log_file" ]]; then
  python3 "$ROOT/scripts/summarize-prove-spans.py" \
    "$structured_log_file" \
    "$prove_compressed_spans_file" || {
      printf 'failed to summarize prove_compressed spans\n' >"$prove_compressed_spans_file"
    }
else
  printf 'No structured log was captured.\n' >"$prove_compressed_spans_file"
fi

printf '\n=== Run Summary ===\n'
printf 'RUN_DIR: %s\n' "$RUN_DIR"
printf 'PROOFS_DIR: %s\n' "$PROOFS_DIR"
printf 'OUTPUT_DIR: %s\n' "$OUTPUT_DIR"
printf 'run.log: %s\n' "$RUN_DIR/run.log"
printf 'structured log: %s\n' "$structured_log_file"
printf 'report: %s\n' "$report_file"
printf 'timings: %s\n' "$timings_file"
printf 'cycle tracker report: %s\n' "$cycle_tracker_file"
printf 'prover gas report: %s\n' "$prover_gas_file"
printf 'chip byte-interaction metadata: %s\n' "$chip_byte_interactions_file"
printf 'prove_compressed span summary: %s\n' "$prove_compressed_spans_file"
if [[ "$GUEST_PROFILING" != "1" ]]; then
  printf 'cycle tracking is disabled by default; rerun with GUEST_PROFILING=1 to enable report-backed guest cycle spans\n'
fi
if [[ "$PROVE_COMPRESSED_SPANS" != "1" ]]; then
  printf 'prove_compressed internal span capture is disabled by default; rerun with PROVE_COMPRESSED_SPANS=1 for DEBUG-level SP1 span timing\n'
fi
printf 'profiling output written to %s\n' "$RUN_DIR"
printf '\nProofs should be in: %s\n' "$OUTPUT_DIR"
ls -la "$OUTPUT_DIR" 2>/dev/null || printf '(directory may be empty if proving failed)\n'
exit "$status"
