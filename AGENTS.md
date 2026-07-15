# zkpow26

Zero-knowledge proof system for Bitcoin header chain validation using SP1 zkVM.
Proves that a batch of block headers is valid (PoW, chain linkage, difficulty
retargeting, median time past) and can be recursively chained to extend a proof
from any trusted starting point.

## Quick Start

```bash
# Build (compiles the guest program to a RISC-V ELF as part of the build)
cargo build --release

# Fill the header database (Bitcoin P2P by default)
cargo run --release -p zkpow-cli -- sync --db headers.db

# Run the prover
ZKPOW_DB_PATH=headers.db ./target/release/zkpow-host   # or: ./scripts/run.sh

# Fast test suite (no proving)
cargo run --release --bin test_errors

# Inspect a produced proof
cargo run --release --bin inspect_proof -- proof_height_0_to_99.bin
```

GPU proving needs a CUDA build (`--features CUDA`) and `ZKPOW_USE_CUDA=1`; without
it the prover runs on CPU / mock. See `contrib/infra/` for the container setup.

## Where Things Live

```text
crates/
  core         shared consensus types, state machine, PoW/difficulty math
  core-macros  #[tracked] cycle-tracking attribute
  guest        the SP1 zkVM program (constrained execution)
  host         stateless prover driver: pull batch, run guest, prove, upload
  controller   run coordinator: batch grants, run ledger, artifact storage,
               result validation (DB <-> prover trust boundary; see prover.md)
  store        headers.db SQLite gateway
  sync-headers header ingestion (Bitcoin P2P + Alchemy RPC)
  cli          zkpow-cli sync command
```

The guest and host share `core`; behavior that must differ between them is
selected by cargo feature, not by caller choice.

## Cost Discipline

At every I/O, serialization, FFI, syscall, or API boundary, trace each value
from its source to its final consumer before adding a conversion. If that
consumer accepts a reference, retain the original backing storage and pass a
borrow. Do not allocate, copy, deserialize, or wrap data only to borrow it
again. A conversion must prove that it changes the representation a consumer
requires, validates untrusted data, or extends the value's required lifetime;
otherwise remove it.

For pure computation, bind the result at the first use and reuse it. A repeated
call on the same immutable input needs a concrete reason. Treat loops, helper
layers, and separate call sites as suspicious until they prove they eliminate
work or data that would otherwise be duplicated.

## Debugging

Every run writes newline-delimited JSON to `logs/run.jsonl` (always produced, no
env var needed). Set `RUST_LOG=info` for stderr output including the SP1 cycle
tracker.

```bash
cat logs/run.jsonl | jq 'select(.level == "ERROR")'
```

## Notes

- `cargo clippy --all-targets` prints "Skipping build due to clippy invocation"
  for the host binary (the `sp1-build` dependency suppresses ELF compilation under
  clippy). This is expected; use `cargo build --release` to verify it compiles.
- Wire formats (public-values layout, error codes, struct sizes) live in the code
  and are still in flux — read `crates/core/src/public_values.rs` and
  `crates/core/src/state.rs` rather than relying on a copy here.
