# zkpow26

Zero-knowledge proof system for Bitcoin header chain validation using SP1 zkVM.
Proves that a batch of block headers are valid (PoW, chain linkage, difficulty
retargeting, median time past) and can be recursively chained to extend a proof
from any trusted starting point.

**Read `AGENT_RULES.md` before making any code changes.** It contains the
information-level rules, forbidden code categories, and the new-function
proposal process.

---

## Project Structure

```
bitcoin-header-chain/
├── Cargo.toml                  # Workspace: core, guest, host
├── headers.db                  # Bitcoin header chain SQLite database (~258 MB)
├── AGENT_RULES.md              # Mandatory rules for code changes
├── scripts/
│   ├── prove-batch.sh          # Single-batch proving (→ continuous-prover)
│   ├── prove-chain.sh          # Multi-batch loop (→ continuous-prover per batch)
│   ├── build_prover.sh         # Build the host binary
│   ├── docker-prover.sh        # Docker-based proving
│   ├── vast_deploy.sh          # Vast.ai GPU instance deployment
│   ├── vast_monitor.sh         # Vast.ai instance monitoring
│   ├── pull_outputs_loop.sh    # Pull proof outputs from remote instances
│   └── summarize-prove-spans.py # Summarize SP1 span timing from logs
└── crates/
    ├── core/                   # Shared consensus types (no_std, no ZKVM deps)
    │   └── src/
    │       ├── lib.rs           # Header, NewHeader, State layout, PV, validation
    │       ├── types.rs         # BlockHash, Target, ChainWork, CompactTarget, u256
    │       ├── brand.rs         # Branded type helpers
    │       ├── input.rs         # Input, RecursiveProof, NewHeaderHints, MTP hints
    │       └── env/
    │           ├── mod.rs       # StateInner<E>, cycle_track, Env trait
    │           └── host.rs      # HostEnvironment (feature-gated)
    ├── guest/                   # zkVM program (constrained, RISC-V)
    │   └── src/
    │       ├── main.rs          # Input → validate → commit PV → halt
    │       └── sha256.rs         # SP1 precompile SHA-256 calls
    └── host/                    # Host proving orchestration
        ├── Cargo.toml           # Features: CUDA, memory-diagnostics, slow-tests
        ├── build.rs             # Compiles guest to RISC-V ELF via sp1-build
        └── src/
            ├── lib.rs           # Re-exports
            ├── main.rs           # zkpow-host binary (single batch, default)
            ├── batch_runner.rs   # run_single_batch() → proof_pipeline
            ├── session_runner.rs # run_batch_session() — multi-batch loop
            ├── proof_pipeline.rs # Core proof generation + save logic
            ├── util.rs           # DB loading, state reconstruction, SHA-256
            ├── observability.rs  # Tracing init (stderr + JSON file)
            ├── memory_profiler.rs # RSS tracking
            ├── cuda_env.rs       # CUDA preflight checks
            └── bin/
                ├── continuous_prover.rs  # Multi-batch binary (default MAX_BATCHES=1)
                ├── test_errors.rs       # Fast validation test suite
                └── inspect_proof.rs     # Human-readable proof display
```

---

## How to Prove a Batch

### Quick Start

```bash
# Build everything (includes guest ELF compilation)
cargo build --release

# Prove a single batch: genesis → block 100 (default NUM_HEADERS=100)
cargo run --release -p zkpow-host --bin zkpow-host

# Or use the profiling script (captures logs, timing, cycle reports)
BUILD=true NUM_HEADERS=100 ./scripts/prove-batch.sh
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `NUM_HEADERS` | 100 | Headers to prove in this batch |
| `PREV_PROOF` | *(none)* | Path to previous compressed proof (for chaining) |
| `OUTPUT_DIR` | `.` | Where to write proof files |
| `MAX_BATCHES` | 1 | Number of batches in a session (1=single, >1=continuous) |
| `GENERATE_GROTH16` | false | Also produce a Groth16 SNARK (~200 bytes for Ethereum) |
| `CUDA` | false | Use GPU prover (requires `--features CUDA` at build) |
| `CUDA_DEVICE_ID` | *(none)* | GPU device ID (only valid with `CUDA=1`) |
| `RUST_LOG` | off | Tracing filter for stderr (JSON log always at info+) |
| `GUEST_PROFILING` | 0 | Enable cycle-tracker report spans in guest |
| `PROVE_COMPRESSED_SPANS` | 0 | Enable DEBUG-level SP1 span timing in JSON log |
| `MEMORY_PROFILING` | 0 | Write RSS snapshots to `logs/mem_<ts>.log` every 1s |

### Proof Files

Each batch produces:
- `proof_height_X_to_Y.bin` — Compressed SP1 proof (~1.3 MB, for recursive chaining)
- `proof_height_X_to_Y_groth16.bin` — Groth16 SNARK (~200 bytes, ~100k gas on Ethereum)
  (only when `GENERATE_GROTH16=1`)

---

## How to Prove Multiple Batches

There are **two approaches**. Both use the same underlying proof pipeline. The
difference is process management and memory behavior.

### Approach 1: Multi-Process (Shell Loop) — Currently Reliable

Each batch runs in a **fresh process**. Memory resets between batches.

```bash
# Prove 10 batches, each 2016 headers, fresh process per batch
MAX_BATCHES=10 NUM_HEADERS=2016 ./scripts/prove-chain.sh
```

**How it works:**
1. `prove-chain.sh` builds `continuous-prover` once
2. Loops, calling `prove-batch.sh` with `MAX_BATCHES=1` each time
3. Each `prove-batch.sh` invocation spawns a **new process**
4. The previous batch's compressed proof is passed as `PREV_PROOF` to the next
5. Memory resets completely between batches — no cross-batch retention

**When to use:** Production runs, large batches, or when memory growth is a
concern. This is the safe default.

**Known issue:** Each process must reinitialize the SP1 prover (key setup,
constraint building), adding ~30-60s overhead per batch.

### Approach 2: Single-Process (In-Process Loop) — Currently Has Memory Issues

All batches run in **one long-lived process**. The prover and proving key are
reused across batches.

```bash
# Prove 10 batches in one process, each 2016 headers
MAX_BATCHES=10 NUM_HEADERS=2016 cargo run --release -p zkpow-host --bin continuous-prover
```

**How it works:**
1. `continuous-prover` calls `session_runner::run_batch_session()`
2. The loop calls `batch_runner::run_single_batch()` for each batch
3. The SP1 prover and proving key are initialized once and cached in a
   `OnceLock` (`PREPARED_PROVER`)
4. Each batch's compressed proof becomes `PREV_PROOF` for the next
5. Proof artifacts are dropped after each batch, but RSS does not fully return

**When to use:** Profiling, development, or when the per-batch prover
initialization overhead is unacceptable.

**Known issue:** `prove_compressed` retains significant memory after each batch.
Post-batch RSS compounds across batches (~15.7 GB → 16.2 GB → 16.2 GB observed
in testing). The retained growth is inside SP1's proving layer, not in
application code. A fresh child process resets this baseline.

### Execution Flow (Both Approaches)

```
Script / Binary
  → session_runner::run_batch_session()
    → batch_runner::run_single_batch()
      → proof_pipeline::generate_and_save_proofs()
        → config_from_env()           # Read env vars
        → load_previous_proof()        # Load PREV_PROOF if present
        → resolve_current_state()      # DB-backed state reconstruction
        → load_header_records()        # SQLite query
        → decode_headers()             # → NewHeader structs
        → load_median_time_past_hints() # From DB column
        → simulate_expected_state()     # Host mirror of guest logic
        → build_recursive_proof()      # Wire the recursive proof witness
        → build_stdin()                # Serialize all inputs
        → execute()                    # Run guest program
        → prove_compressed()           # Generate SP1 compressed proof
        → verify_compressed_proof()    # Self-check
        → save_compressed_proof()      # Write .bin file
        → [optional] generate_groth16_proof()
        → [optional] save_groth16_proof()
```

---

## Database

The prover reads from `headers.db` (SQLite, ~258 MB). The path is hardcoded as
`DEFAULT_DB_PATH` relative to the host crate manifest.

**Required columns:**

| Column | Type | Description |
|--------|------|-------------|
| `height` | INTEGER | Block height (primary key) |
| `version` | INTEGER | Block version |
| `prev` | BLOB(32) | Previous block hash |
| `merkle_root` | BLOB(32) | Merkle root |
| `timestamp` | INTEGER | Block timestamp |
| `n_bits` | INTEGER | Compact difficulty (nBits) |
| `nonce` | INTEGER | Nonce |
| `chainwork` | BLOB(32) | Cumulative chain work (256-bit LE) |
| `median_time_past` | INTEGER | Median of last 11 timestamps |

**State reconstruction** (`state_from_db_at_height`):
- At height 0: loads genesis record, builds genesis state
- At height N > 0: loads record at N, record at N+1 (for next_nbits/next_target),
  epoch-start record, and the timestamp window (last 11 records)
- All data comes from the DB — no recomputation from state

---

## Architecture

### Data Flow

```
Host: load state from DB, load headers, load MTP hints
  → serialize as stdin (input || state || header_hints || mtp_hints || [recursive_proof])
  → zkVM: parse → validate headers → commit MinimalPublicValues → halt
  → Host: verify proof → verify public values → save proof files
```

### Guest Input Protocol (5 reads from stdin)

1. `encoded_input: Vec<u8>` — `PublicChainClaim || RecursiveProof`
2. `state_witness: Vec<u8>` — Full `State` bytes
3. `header_hints: Vec<u8>` — `NewHeaderHints` (44-byte NewHeader structs)
4. `median_time_past_hints: Vec<u8>` — `MedianTimePastHints` (u32 per header)
5. If `claim.height > 0`: recursive proof witness (via `write_proof`)

### State Wire Sizes

| Constant | Bytes | Description |
|----------|-------|-------------|
| `STATE_SIZE` | 296 | Full `State` (header + hash + genesis + nbits + height + chain_work + next_work + next_target + epoch_ts + timestamps) |
| `NEW_HEADER_SIZE` | 44 | `NewHeader` (version + merkle_root + timestamp + nonce) |
| `PUBLIC_CHAIN_CLAIM_SIZE` | 100 | `PublicChainClaim` (genesis_hash + tip_hash + chain_work + height) |
| `RECURSIVE_PROOF_SIZE` | 68 | `RecursiveProof` (verifier_key + pv_digest + return_code) |
| `PROOF_CARRYING_STATE_SIZE` | 168 | `PublicChainClaim + RecursiveProof` |
| `PRIVATE_CONTINUATION_STATE_SIZE` | 116 | Next nbits + work + target + epoch_ts + timestamps |
| `MINIMAL_PV_SIZE` | 169 | Committed public values (success or failure) |

### MinimalPublicValues Layout (169 bytes)

| Offset | Size | Field |
|--------|------|-------|
| 0..32 | 32 | genesis_hash |
| 32..64 | 32 | tip_hash |
| 64..96 | 32 | chain_work |
| 96..100 | 4 | height |
| 100..101 | 1 | success_code (0=success, 1-4=error) |
| 101..105 | 4 | failure_height (0 on success) |
| 105..137 | 32 | continuation_digest |
| 137..169 | 32 | verifier_key_digest |

### Error Codes

| Code | Name | Trigger |
|------|------|---------|
| 0 | Success | All headers valid |
| 1 | Header payload length invalid | Input/hint payload length is malformed |
| 2 | PoW insufficient | `SHA256d(header) > target` |
| 3 | Timestamp too old | `timestamp ≤ median_time_past` |
| 4 | Genesis hash mismatch | At height 0, first validated header hash ≠ expected genesis hash |

Notes:
- Prev-blockhash and bits-mismatch checks are derived from authenticated state
  when materializing `Header` from `NewHeader` — they are not separate error codes.
- A failed prior proof (non-zero return code in `RecursiveProof`) is rejected
  during recursive proof verification, not as a validation error code.

---

## SHA-256 Implementation (Guest)

Uses direct SP1 precompile syscalls — no `sha2` crate in the guest program.

- `sha256d_80bytes(&[u8; 80])` → `[u8; 32]`: Double-SHA-256 of a Bitcoin block header
- `sha256_116bytes(&[u8; 116])` → `[u8; 32]`: `PrivateContinuationState` (2 SHA-256 blocks)
- `sha256_169bytes(&[u8; 169])` → `[u8; 32]`: `MinimalPublicValues` (3 SHA-256 blocks)

The host uses the `sha2` crate for `compute_pv_digest()` and
`continuation_digest_from_state()` — this is correct since the host needs to
verify it built the same values the program committed.

---

## Testing

```bash
# Fast validation test suite (no proving, runs against headers.db)
cargo run --release --bin test_errors

# Core library unit tests
cargo test -p zkpow-core

# Full workspace tests
cargo test --workspace

# Slow proof-generation tests (requires headers.db)
cargo test -p zkpow-host --features slow-tests

# Inspect a proof file
cargo run --release --bin inspect_proof -- proof_height_1_to_100.bin
```

---

## Debugging

### Structured JSON Logs

Every run writes newline-delimited JSON to `logs/run.jsonl` (always on at
info+ level, regardless of `RUST_LOG`).

```bash
# All log entries
cat logs/run.jsonl | jq .

# Errors only
cat logs/run.jsonl | jq 'select(.level == "ERROR")'

# Phase timing (per-phase RSS and duration)
cat logs/run.jsonl | jq 'select(.fields.message | test("started|finished in"))'

# Memory snapshots (batch start/end RSS)
cat logs/run.jsonl | jq 'select(.fields.message | test("Batch memory"))'

# Proving time breakdown
cat logs/run.jsonl | jq 'select(.fields.message | test("PROVING TIME|breakdown"))'

# Cycle tracker output
cat logs/run.jsonl | jq 'select(.fields.message | test("cycle|instruction|span|gas"))'
```

### Cycle Tracker (Guest)

Run with `GUEST_PROFILING=1` to emit cycle-tracker markers that show up in the
execution report:

```bash
GUEST_PROFILING=1 cargo run --release -p zkpow-host --bin zkpow-host
```

### Memory Profiling

```bash
# Enable periodic RSS logging
MEMORY_PROFILING=1 cargo run --release -p zkpow-host --bin zkpow-host

# For memory-diagnostics feature (jemalloc stats)
cargo run --release -p zkpow-host --bin continuous-prover --features memory-diagnostics
```

### Profiling Script Output

`prove-batch.sh` captures structured output in a timestamped run directory:

```
profiling/runs/<timestamp>/
├── meta.txt              # Run configuration
├── run.log               # Merged stdout/stderr
├── run.jsonl             # Structured JSON log (copy of logs/run.jsonl)
├── report.txt            # Cycle tracker, hot spans, gas summary
├── timings.txt           # Phase timing summary
├── cycle-tracker.txt     # Cycle tracker details
├── prover-gas.txt        # Gas estimates
├── chips.txt             # AIR byte-interaction counts
└── prove-compressed-spans.txt  # SP1 internal span timing
```

---

## Build Notes

```bash
# Standard build (includes guest ELF compilation via sp1-build)
cargo build --release

# Build with CUDA support
cargo build --release -p zkpow-host --features CUDA

# Build with memory diagnostics (jemalloc)
cargo build --release -p zkpow-host --features memory-diagnostics

# Clippy (note: skips script binary due to sp1-build)
cargo clippy --all-targets -- -D warnings
# Verify the full build separately:
cargo build --release
```

### Clippy Caveat

`cargo clippy --all-targets` prints "Skipping build due to clippy invocation"
for the host binary because `sp1-build` triggers ELF compilation which clippy
suppresses. This is expected. Run `cargo build --release` to verify the host
compiles.

---

## Remote Proving (Vast.ai)

```bash
# Deploy to a GPU instance
./scripts/vast_deploy.sh

# Monitor running instances
./scripts/vast_monitor.sh

# Pull proof outputs from remote
./scripts/pull_outputs_loop.sh
```

See `.env.example` for configuration (API keys, Docker image, instance sizing,
GPU query parameters).

---

## Key Crate Features

| Crate | Feature | Description |
|-------|---------|-------------|
| `zkpow-core` | `host` | Enables `HostEnvironment` state type (full access) |
| `zkpow-core` | `profiling` | Enables `cycle_track_report` with SP1 report-backed markers |
| `zkpow-host` | `CUDA` | GPU prover via `sp1-sdk/cuda` |
| `zkpow-host` | `memory-diagnostics` | jemalloc global allocator + RSS tracking |
| `zkpow-host` | `slow-tests` | Proof-generation integration tests |

---

## SP1 Version

All SP1 dependencies are pinned to tag `v6.1.0` from
`https://github.com/succinctlabs/sp1`. The SHA-2 crate uses the SP1-patched
version from `sp1-patches/RustCrypto-hashes`.
