#!/usr/bin/env bash
set -euo pipefail

# Unconditionally set ROOT and TIMESTAMP
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TIMESTAMP="${TIMESTAMP:=$(date -u +%Y%m%dT%H%M%S)}"
ENV_FILE="${ENV_FILE:=$ROOT/.env}"
PROFILE_ROOT="${PROFILE_ROOT:=$ROOT/profiling}"
RUN_DIR="$PROFILE_ROOT/runs/$TIMESTAMP"
OUTPUT_DIR="${OUTPUT_DIR:=$RUN_DIR/output}"
LATEST_LINK="${LATEST_LINK:-$PROFILE_ROOT/latest}"

if [[ -v "${SP1_ENABLE_TOKIO_CONSOLE:-}" ]]; then
 CALLER_SP1_ENABLE_TOKIO_CONSOLE_SET=1
 CALLER_SP1_ENABLE_TOKIO_CONSOLE="$SP1_ENABLE_TOKIO_CONSOLE"
else
 CALLER_SP1_ENABLE_TOKIO_CONSOLE_SET=0
 CALLER_SP1_ENABLE_TOKIO_CONSOLE=""
fi

# Set defaults for the guest program
unset sp1_core
if [[ "${CALLER_SP1_ENABLE_TOKIO_CONSOLE_SET:-}" == "1" ]]; then
 export SP1_ENABLE_TOKIO_CONSOLE="${CALLER_SP1_ENABLE_TOKIO_CONSOLE:-}"
else
 export SP1_ENABLE_TOKIO_CONSOLE="false"
fi
export TRACE_FILE="${TRACE_FILE:-$RUN_DIR/tracing.json}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
export RUST_LOG="${RUST_LOG:-info}"
export NUM_HEADERS="${NUM_HEADERS:-20160}"
export GUEST_PROFILING="${GUEST_PROFILING:-0}"
export PROVE_COMPRESSED_SPANS="${PROVE_COMPRESSED_SPANS:-0}"
export RUST_TEST_NOCAPTURE=1
mkdir -p "$RUN_DIR"
mkdir -p "$OUTPUT_DIR"
mkdir -p "$(dirname "$LATEST_LINK")"
ln -sfn "$RUN_DIR" "$LATEST_LINK"

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

set +e
if [[ "${BUILD:-}" == "true" ]]; then
  sh "$ROOT/scripts/build_prover.sh" 2>&1 | tee "$RUN_DIR/build.log"
fi

MAX_BATCHES=1 "$ROOT/target/release/zkpow-host" > >(tee -a "$RUN_DIR/run.log") 2> >(tee -a "$RUN_DIR/run.log" >&2)

status=${PIPESTATUS[0]}
set -e

report="$RUN_DIR/report.txt"
timings="$RUN_DIR/timings.txt"
cycle_tracker="$RUN_DIR/cycle-tracker.txt"
prover_gas="$RUN_DIR/prover-gas.txt"
chips="$RUN_DIR/chip-byte-interactions.txt"
structured="$RUN_DIR/run.jsonl"
prove_compressed_spans="$RUN_DIR/prove-compressed-spans.txt"

if [[ -f "$ROOT/logs/run.jsonl" ]]; then
  cp "$ROOT/logs/run.jsonl" "$structured"
else
  : >"$structured"
fi

{
  rg 'Execution report|cycle tracker:|top hot spans:|cycle hierarchy:|prover gas:|total prover_gas|estimated gas by hot span|assumptions:|TOTAL PROVING TIME|Total proving time|Proving time breakdown:' "$RUN_DIR/run.log" || true
} >"$report"

{
  rg 'finished in|TOTAL PROVING TIME|Total proving time|Proving time breakdown:' "$RUN_DIR/run.log" || true
} >"$timings"

{
  rg 'cycle tracker:|top hot spans:|cycle hierarchy:|cycles' "$RUN_DIR/run.log" || true
} >"$cycle_tracker"

{
  rg 'prover gas:|total prover_gas|estimated gas by hot span|assumptions:' "$RUN_DIR/run.log" || true
} >"$prover_gas"

{
  printf 'These are static SP1 AIR byte-interaction counts emitted while building prover chips; they are not per-run byte-volume measurements.\n'
  rg 'chip .* has [0-9]+ byte interactions' "$RUN_DIR/run.log" || true
} >"$chips"

if [[ -s "$structured" ]]; then
  python3 "$ROOT/scripts/summarize-prove-spans.py" \
    "$structured" \
    "$prove_compressed_spans" || {
    printf 'failed to summarize prove_compressed spans\n' >"$prove_compressed_spans"
  }
else
  printf 'No structured log was captured.\n' >"$prove_compressed_spans"
fi

printf '\n=== Run Summary ===\n'
printf 'RUN_DIR: %s\n' "$RUN_DIR"
printf 'OUTPUT_DIR: %s\n' "$OUTPUT_DIR"
printf 'run.log: %s\n' "$RUN_DIR/run.log"
printf 'structured log: %s\n' "$structured"
printf 'report: %s\n' "$report"
printf 'timings: %s\n' "$timings"
printf 'cycles: %s\n' "$cycle_tracker"
printf 'prover gas: %s\n' "$prover_gas"
printf 'chips: %s\n' "$chips"
printf 'prove_compressed spans: %s\n' "$prove_compressed_spans"
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
