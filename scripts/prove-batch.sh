#!/usr/bin/env bash
set -euo pipefail

# Unconditionally set ZKPOW_ROOT and TIMESTAMP
ZKPOW_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%S)"
ENV_FILE="${ENV_FILE:=$ZKPOW_ROOT/.env}"
export PROFILE_ROOT="${PROFILE_ROOT:=$ZKPOW_ROOT/profiling}"
export OUT_DIR="$PROFILE_ROOT/runs/$TIMESTAMP"
#PROFILE_ROOT="${PROFILE_ROOT:=$ZKPOW_ROOT/profiling/runs}";
#PROFILE_OUT="${OUT_DIR}"

#if [[ -v "$SP1_ENABLE_TOKIO_CONSOLE" ]]; then
#  CALLER_SP1_ENABLE_TOKIO_CONSOLE_SET=1
#  CALLER_SP1_ENABLE_TOKIO_CONSOLE="$SP1_ENABLE_TOKIO_CONSOLE"
#else
#  CALLER_SP1_ENABLE_TOKIO_CONSOLE_SET=0
#  CALLER_SP1_ENABLE_TOKIO_CONSOLE=""
#fi

# Set defaults for run configuration
#OUT_DIR="${OUT_DIR:-$PROFILE_ROOT/$TIMESTAMP}"
LATEST_LINK="${LATEST_LINK:-$PROFILE_ROOT/latest}"
export ZKPOW_OUTPUT_DIR="${ZKPOW_OUTPUT_DIR:=$OUT_DIR/output}"

# Set defaults for the guest program
#unset sp1_core
#if [[ "$CALLER_SP1_ENABLE_TOKIO_CONSOLE_SET" == "1" ]]; then
#  export SP1_ENABLE_TOKIO_CONSOLE="$CALLER_SP1_ENABLE_TOKIO_CONSOLE"
#else
#  export SP1_ENABLE_TOKIO_CONSOLE="false"
#fi
export TRACE_FILE="${TRACE_FILE:-$OUT_DIR/tracing.json}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
export RUST_LOG="${RUST_LOG:-info}"
export ZKPOW_BATCH_SIZE="${ZKPOW_BATCH_SIZE:-20160}"
export ZKPOW_GUEST_PROFILING="${ZKPOW_GUEST_PROFILING:-0}"
export ZKPOW_PROVE_COMPRESSED_SPANS="${ZKPOW_PROVE_COMPRESSED_SPANS:-0}"
export RUST_TEST_NOCAPTURE=1
mkdir -p "$OUT_DIR"
mkdir -p "$ZKPOW_OUTPUT_DIR"
mkdir -p "$(dirname "$LATEST_LINK")"
ln -sfn "$OUT_DIR" "$LATEST_LINK"

{
  printf 'repo=%s\n' "$ZKPOW_ROOT"
  printf 'commit=%s\n' "$(git -C "$ZKPOW_ROOT" rev-parse HEAD)"
  printf 'timestamp=%s\n' "$TIMESTAMP"
  printf 'rust_log=%s\n' "$RUST_LOG"
  printf 'num_headers=%s\n' "$ZKPOW_BATCH_SIZE"
  printf 'guest_profiling=%s\n' "$ZKPOW_GUEST_PROFILING"
  printf 'prove_compressed_spans=%s\n' "$ZKPOW_PROVE_COMPRESSED_SPANS"
  printf 'prev_proof=%s\n' "${ZKPOW_PREV_PROOF:-}"
  printf 'output_dir=%s\n' "$ZKPOW_OUTPUT_DIR"
} >"$OUT_DIR/meta.txt"

rm -f "$ZKPOW_ROOT/logs/run.jsonl"

set +e
if [[ "$BUILD" == "true" ]]; then
  cargo build --release \
    --manifest-path "$ZKPOW_ROOT/crates/host/Cargo.toml" \
    --bin zkpow-host 2>&1 | tee "$OUT_DIR/build.log"
fi

ZKPOW_BATCH_COUNT=1 "$ZKPOW_ROOT/target/release/zkpow-host" > >(tee -a "$OUT_DIR/run.log") 2> >(tee -a "$OUT_DIR/run.log" >&2)

status=${PIPESTATUS[0]}
set -e

report="$OUT_DIR/report.txt"
timings="$OUT_DIR/timings.txt"
cycle_tracker="$OUT_DIR/cycle-tracker.txt"
prover_gas="$OUT_DIR/prover-gas.txt"
chips="$OUT_DIR/chip-byte-interactions.txt"
structured="$OUT_DIR/run.jsonl"
prove_compressed_spans="$OUT_DIR/prove-compressed-spans.txt"

if [[ -f "$ZKPOW_ROOT/logs/run.jsonl" ]]; then
  cp "$ZKPOW_ROOT/logs/run.jsonl" "$structured"
else
  : >"$structured"
fi

{
  rg 'Execution report|cycle tracker:|top hot spans:|cycle hierarchy:|prover gas:|total prover_gas|estimated gas by hot span|assumptions:|TOTAL PROVING TIME|Total proving time|Proving time breakdown:' "$OUT_DIR/run.log" || true
} >"$report"

{
  rg 'finished in|TOTAL PROVING TIME|Total proving time|Proving time breakdown:' "$OUT_DIR/run.log" || true
} >"$timings"

{
  rg 'cycle tracker:|top hot spans:|cycle hierarchy:|cycles' "$OUT_DIR/run.log" || true
} >"$cycle_tracker"

{
  rg 'prover gas:|total prover_gas|estimated gas by hot span|assumptions:' "$OUT_DIR/run.log" || true
} >"$prover_gas"

{
  printf 'These are static SP1 AIR byte-interaction counts emitted while building prover chips; they are not per-run byte-volume measurements.\n'
  rg 'chip .* has [0-9]+ byte interactions' "$OUT_DIR/run.log" || true
} >"$chips"

if [[ -s "$structured" ]]; then
  python3 "$ZKPOW_ROOT/scripts/summarize-prove-spans.py" \
    "$structured" \
    "$prove_compressed_spans" || {
    printf 'failed to summarize prove_compressed spans\n' >"$prove_compressed_spans"
  }
else
  printf 'No structured log was captured.\n' >"$prove_compressed_spans"
fi

printf '\n=== Run Summary ===\n'
printf 'OUT_DIR: %s\n' "$OUT_DIR"
printf 'OUTPUT_DIR: %s\n' "$ZKPOW_OUTPUT_DIR"
printf 'run.log: %s\n' "$OUT_DIR/run.log"
printf 'structured log: %s\n' "$structured"
printf 'report: %s\n' "$report"
printf 'timings: %s\n' "$timings"
printf 'cycles: %s\n' "$cycle_tracker"
printf 'prover gas: %s\n' "$prover_gas"
printf 'chips: %s\n' "$chips"
printf 'prove_compressed spans: %s\n' "$prove_compressed_spans"
if [[ "$ZKPOW_GUEST_PROFILING" != "1" ]]; then
  printf 'cycle tracking is disabled by default; rerun with ZKPOW_GUEST_PROFILING=1 to enable report-backed guest cycle spans\n'
fi
if [[ "$ZKPOW_PROVE_COMPRESSED_SPANS" != "1" ]]; then
  printf 'prove_compressed internal span capture is disabled by default; rerun with ZKPOW_PROVE_COMPRESSED_SPANS=1 for DEBUG-level SP1 span timing\n'
fi
printf 'profiling output written to %s\n' "$OUT_DIR"
printf '\nProofs should be in: %s\n' "$ZKPOW_OUTPUT_DIR"
ls -la "$ZKPOW_OUTPUT_DIR" 2>/dev/null || printf '(directory may be empty if proving failed)\n'
exit "$status"
