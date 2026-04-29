# zkpow26

Zero-knowledge proof system for Bitcoin header chain validation using SP1 zkVM.
Proves that a batch of block headers are valid (PoW, chain linkage, difficulty retargeting, BIP113 MTP) and can be recursively chained to extend a proof from any trusted starting point.

## Quick Start

```bash
# Build
cargo build --release

# Run prover (genesis → block 99)
cd script && cargo run --release --bin zkpow-host

# Run prover extending a previous proof
PREV_PROOF=proof_height_0_to_99.bin START_HEIGHT=100 NUM_HEADERS=100 \
  cargo run --release --bin zkpow-host

# Run test suite (fast, no proving)
cargo run --release --bin test_errors

# Inspect a proof
cargo run --release --bin inspect_proof -- proof_height_0_to_99.bin

# Clippy (clean)
cargo clippy --all-targets -- -D warnings
```

## Project Structure

```
bitcoin-header-chain/
├── program/                    # zkVM program (constrained execution)
│   ├── Cargo.toml              # Only sp1-zkvm (verify feature)
│   └── src/
│       ├── main.rs             # Header validation logic
│       └── sha256.rs           # Specialized SHA-256 precompile calls
└── script/                     # Host script (proving orchestration)
    ├── Cargo.toml
    ├── build.rs                # Compiles program to RISC-V ELF
    └── src/
        ├── main.rs             # Prove → verify → save (compressed + Groth16)
        ├── util.rs             # DB loading, work calculation, PV builder
        ├── lib.rs              # pub mod util
        └── bin/
            ├── test_errors.rs      # Automated tests (8/8 pass)
            └── inspect_proof.rs    # Human-readable proof display
```

## Architecture

### Data Flow
```
Host: genesis_hash, start_height, num_headers, headers_bytes
  ↓ stdin
zkVM: reads inputs → validates headers → commits public values → HALT(0)
  ↓ proof
Host: verifies proof → verifies public values → saves proof
```

### Public Values Layout (237 bytes)
| Offset | Size | Field |
|--------|------|-------|
| 0..32 | 32 | genesis_hash |
| 32..64 | 32 | final_header_hash |
| 64..72 | 8 | num_headers (total validated) |
| 72..152 | 80 | final_header (raw bytes) |
| 152..184 | 32 | cumulative_chain_work (u256 LE) |
| 184..188 | 4 | last_epoch_start_timestamp |
| 188..232 | 44 | median_timestamp_buffer (\[u32; 11]) |
| 232..233 | 1 | success_code (0=success, 1-7=error) |
| 233..237 | 4 | error_detail (header index on error) |

### Median Count is Derivable
Do NOT commit median_count to public values. It's computed as:
```
count = min(11, total_validated - 1)  for total_validated > 0
count = 0                              for total_validated == 0
```

## SHA-256 Implementation

Uses direct SP1 precompile syscalls — no `sha2` crate in the program.

- `sha256_80(&[u8; 80])` → `[u8; 32]`: Bitcoin block header. Hardcoded for exactly 2 blocks. No loops, no branching.
- `sha256_32(&[u8; 32])` → `[u8; 32]`: Intermediate hash. Hardcoded for exactly 1 block.
- `double_sha256_80(&[u8; 80])` → `[u8; 32]`: Composition of the above.

The host script still uses the `sha2` crate (via workspace) for `compute_pv_digest()` — this is appropriate since the host needs SHA-256 to verify it built the same public values the program committed.

## Error Codes

| Code | Name | Trigger |
|------|------|---------|
| 0 | Success | All headers valid |
| 1 | Genesis hash mismatch | Height 0 hash ≠ expected |
| 2 | Prev blockhash mismatch | `header.prev ≠ prev_hash` |
| 3 | PoW insufficient | `SHA256d(header) > target` |
| 4 | Timestamp too old | `timestamp ≤ median_of_last_11` |
| 5 | Height mismatch | `start_height ≠ prev_num_headers` (or ≠ 0 at genesis) |
| 6 | Bits mismatch | `header.bits ≠ expected` |
| 7 | Header count mismatch | `len ≠ num_headers * 80` |

Not used (reserved): 8, 9, 10.

## Proof Files

Each run produces two files:
- `proof_height_X_to_Y.bin` — Compressed proof (~1.3 MB, for off-chain verification and recursive chaining)
- `proof_height_X_to_Y_groth16.bin` — Groth16 SNARK (~200 bytes, ~100k gas on Ethereum)

## Clippy Note

`cargo clippy --all-targets` prints "Skipping build due to clippy invocation" for the script binary because the `sp1-build` dependency triggers ELF compilation which clippy suppresses. This is expected. Run `cargo build --release` to verify the script compiles.

## Cycle Tracker

Run with `RUST_LOG=info` to see cycle tracker output:
```
stdout: cycle-tracker-start: parse
stdout: cycle-tracker-end: parse
stdout: cycle-tracker-start: sha256d
stdout: cycle-tracker-end: sha256d
stdout: cycle-tracker-start: retarget
stdout: cycle-tracker-end: retarget
```
