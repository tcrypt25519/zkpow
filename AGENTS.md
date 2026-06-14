# zkpow26

Zero-knowledge proof system for Bitcoin header chain validation using SP1 zkVM.
Proves that a batch of block headers are valid (PoW, chain linkage, difficulty
retargeting, median time past) and can be recursively chained to extend a proof
from any trusted starting point.

## Quick Start

```bash
# Build
cargo build --release

# Run prover (genesis → first batch; default batch size 2016)
cargo run --release -p zkpow-host --bin zkpow-host

# Run prover extending a previous proof
ZKPOW_PREV_PROOF=proof_height_1_to_2016.bin \
  cargo run --release -p zkpow-host --bin zkpow-host

# Tune batch size / number of batches per process
ZKPOW_BATCH_SIZE=2016 ZKPOW_BATCH_COUNT=10 \
  cargo run --release -p zkpow-host --bin zkpow-host

# Also emit a Groth16-wrapped proof (slower)
ZKPOW_GENERATE_GROTH16=1 cargo run --release -p zkpow-host --bin zkpow-host

# Run test suite (fast, no proving)
cargo run --release -p zkpow-host --bin test_errors

# Inspect a proof
cargo run --release -p zkpow-host --bin inspect_proof -- proof_height_1_to_2016.bin

# Clippy (clean)
cargo clippy --all-targets -- -D warnings
```

## Project Structure

```text
crates/
├── core/                       # Shared consensus types + pure logic (no_std)
│   └── src/
│       ├── lib.rs              # Types, wire sizes, PoW/difficulty/work helpers, public values
│       ├── state.rs            # State + apply_headers (per-header state machine)
│       ├── input.rs            # ProofCarryingState / Proof / hint parsing
│       ├── types.rs            # u256 + branded newtypes (Target, ChainWork, …)
│       └── brand.rs            # Zero-cost Branded<Tag, T> newtype wrapper
├── guest/                      # zkVM program (constrained execution)
│   └── src/
│       ├── main.rs             # Reads stdin, verifies recursion, applies headers, commits PV
│       └── sha256.rs           # Specialized SHA-256 precompile calls
├── host/                       # Host script (proving orchestration)
│   ├── build.rs                # Compiles the guest program to a RISC-V ELF
│   └── src/
│       ├── main.rs             # Entry point → run_batch_session()
│       ├── pipeline/           # batch prep, input serialization, execution, proof gen
│       ├── util/               # DB loading, hashing, simulation helpers
│       └── bin/
│           ├── test_errors.rs      # Automated validation tests (no proving)
│           ├── inspect_proof.rs    # Human-readable proof display
│           └── sp1_stress.rs       # SP1 stress/benchmark harness
└── memory-usage/               # Allocation tracking utilities
```

## Architecture

### Data Flow

```text
Host: ProofCarryingState (Claim ∥ verifier_key ∥ Proof), State witness,
      NewHeader[], median hints, [compressed prior proof if height > 0]
  ↓ stdin (4 or 5 length-prefixed frames)
zkVM: parse + authenticate inputs → [verify prior proof] → apply_headers
      → commit MinimalPublicValues → HALT(0)
  ↓ proof
Host: verifies proof → verifies public values → saves proof
```

See `docs/diagrams/03_guest_data_flow.md` for the full byte-level data flow
through the guest program.

### Public Values Layout — `MinimalPublicValues` (169 bytes)

| Offset | Size | Field |
| ------ | ---- | ----- |
| 0..32 | 32 | genesis_hash |
| 32..64 | 32 | tip_hash |
| 64..96 | 32 | chain_work (u256 LE) |
| 96..100 | 4 | height (u32 LE, last valid height) |
| 100..101 | 1 | return_code (0 = success, nonzero = `ValidationErrorCode`) |
| 101..105 | 4 | failure_height (0 on success; absolute chain height of the bad block) |
| 105..137 | 32 | continuation_digest (SHA-256 of `ContinuationData`) |
| 137..169 | 32 | verifier_key (\[u32; 8] LE) |

The private cached fields (current_nbits, current_work, current_target,
epoch_start_timestamp, timestamps\[11]) are not committed directly — they are
bound through `continuation_digest`, which the next batch recomputes from its
State witness. See `docs/diagrams/03_guest_data_flow.md`.

### Median Window Count is Derivable

Do NOT commit the median-window count to public values. It equals
`State::timestamp_count()`:

```text
count = min(height + 1, 11)
```

(At the genesis anchor, height = 0, so the window holds a single timestamp.)

## SHA-256 Implementation

Uses direct SP1 precompile syscalls — no `sha2` crate in the program.

- `sha256_80bytes(&[u8; 80])` → `[u8; 32]`: Bitcoin block header. Hardcoded for exactly 2 blocks. No loops, no branching.
- `sha256_32bytes(&[u8; 32])` → `[u8; 32]`: Intermediate hash. Hardcoded for exactly 1 block.
- `sha256_116bytes(&[u8; 116])` → `[u8; 32]`: Serialized `ContinuationData` (116 bytes = 2 SHA-256 blocks). Used for the continuation digest.
- `sha256_169bytes(&[u8; 169])` → `[u8; 32]`: Serialized `MinimalPublicValues` (169 bytes = 3 SHA-256 blocks). Used to reconstruct the prior proof's public-values digest during recursion.
- `sha256d_80bytes(&[u8; 80])` → `[u8; 32]`: Double-SHA-256 of a Bitcoin block header (SHA256(SHA256(header))).

The host script still uses the `sha2` crate (via workspace) for
`compute_pv_digest()` — this is appropriate since the host needs SHA-256 to
verify it built the same public values the program committed.

### State Wire Sizes

| Constant | Bytes | Description |
| -------- | ----- | ----------- |
| `STATE_SIZE` | 296 | Full `State` (public + private, serialized as the recursive-chaining witness) |
| `CLAIM_SIZE` | 100 | `Claim` (genesis_hash=32, tip_hash=32, chain_work=32, height=4) |
| `PROOF_SIZE` | 36 | `Proof` (public_values_digest=32, exit_code=1, _pad=3) |
| `CONTINUATION_DATA_SIZE` | 116 | `ContinuationData` (current_nbits=4, current_work=32, current_target=32, epoch_start_timestamp=4, timestamps=44) |
| `MINIMAL_PV_SIZE` | 169 | `MinimalPublicValues` committed output |

The public wire input `ProofCarryingState` is `CLAIM_SIZE + 32 (verifier_key) +
PROOF_SIZE = 168` bytes.

## Error Codes

| Code | Name | Trigger |
| ---- | ---- | ------- |
| 0 | Success (return_code) | All headers valid |
| 1 | `HeaderPayloadLengthInvalid` | Reserved. Malformed input lengths currently abort proving as a parse panic, not a committed code. |
| 2 | `PowInsufficient` | `SHA256d(header) > active_target` (committed in public values) |
| 3 | `TimestampTooOld` | `header.timestamp ≤ claimed_median` (committed in public values) |
| 4 | `GenesisHashMismatch` | Reserved. The height-0 state is a trusted anchor (see ADRs), so genesis is not re-checked at runtime. |

Notes:

- `return_code` 0 is success; it is not a `ValidationErrorCode` variant. The enum
  defines only codes 1–4 (`crates/core/src/types.rs`).
- Validation failures (codes 2, 3) are committed in `MinimalPublicValues` and the
  proof still verifies. Malformed inputs and rejected recursion `panic!`, so no
  proof is produced.
- Prev-blockhash and `nBits` mismatches are not separate error codes by design:
  those fields are injected from authenticated state when materializing `Header`
  from `NewHeader`, so they cannot be forged.

## Proof Files

Each run produces two files:

- `proof_height_X_to_Y.bin` — Compressed proof (~1.3 MB, for off-chain
verification and recursive chaining)
- `proof_height_X_to_Y_groth16.bin` — Groth16 SNARK (~200 bytes, ~100k gas on Ethereum)

## Clippy Note

`cargo clippy --all-targets` prints "Skipping build due to clippy invocation"
for the script binary because the `sp1-build` dependency triggers ELF
compilation which clippy suppresses. This is expected. Run
`cargo build --release` to verify the script compiles.

## Debugging

### Structured JSON logs

Every run writes newline-delimited JSON to `logs/run.jsonl` (created automatically,
overwritten each run). No environment variable needed — the file is always produced.

**Quick queries:**

```bash
# All log entries from the last run
cat logs/run.jsonl | jq .

# Errors only
cat logs/run.jsonl | jq 'select(.level == "ERROR")'

# Proof pipeline progress (info-level events)
cat logs/run.jsonl | jq 'select(.level == "INFO") | .fields.message'

# Cycle tracker / execution report entries
cat logs/run.jsonl | jq 'select(.fields.message | test("cycle|instruction|span|gas"; "i"))'

# Last N entries
tail -n 20 logs/run.jsonl | jq .
```

**Log entry schema:**

```json
{
  "timestamp": "2026-04-29T07:00:00.000000Z",
  "level": "INFO",
  "fields": { "message": "Human-readable description" },
  "target": "zkpow_host::proof_pipeline",
  "span": { "name": "span-name" },
  "spans": [{ "name": "parent-span" }]
}
```

**Common debugging workflows:**

- Proof failed → `cat logs/run.jsonl | jq 'select(.level == "ERROR")'`
- Check backend/config used → `cat logs/run.jsonl | jq 'select(.fields.message | test("Starting proof|backend"))'`
- Inspect cycle breakdown → `cat logs/run.jsonl | jq 'select(.fields.message | test("cycles|hot span|hierarchy"))'`
- Timing summary → `cat logs/run.jsonl | jq 'select(.fields.message | test("PROVING TIME|breakdown|seconds"))'`

### Cycle Tracker (stderr)

Run with `RUST_LOG=info` to also see cycle tracker output on stderr:

```shell
stdout: cycle-tracker-start: parse
stdout: cycle-tracker-end: parse
stdout: cycle-tracker-start: sha256d
stdout: cycle-tracker-end: sha256d
stdout: cycle-tracker-start: retarget
stdout: cycle-tracker-end: retarget
```

The same data appears in `logs/run.jsonl` without needing `RUST_LOG=info`.
