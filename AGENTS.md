# Bitcoin Header Chain

## Purpose

Zero-knowledge Bitcoin header-chain validation using SP1 zkVM.
The system proves a contiguous batch of headers, optionally extends a prior compressed proof recursively, and emits both a compressed SP1 proof and a Groth16 proof.

## Current Workspace Layout

This repo is currently organized as a Cargo workspace under `crates/`:

- `crates/core` — shared consensus types, input encoding, state transitions, and validation logic.
- `crates/guest` — zkVM guest program entrypoint plus SP1-compatible SHA helpers.
- `crates/host` — proof orchestration, SQLite loading, observability, proof saving, and host-side binaries.
- `contracts/` — Foundry relay scaffold for on-chain verification and proof-state anchoring.
- `scripts/` — helper scripts such as profiling.
- `docs/` — plans and ADRs.

Authoritative workspace members are defined in [Cargo.toml](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/Cargo.toml).
If comments, scripts, or stale docs disagree with the workspace manifest, trust the manifest.

## Refactor Status

This repo is mid-migration away from the older top-level `program/`, `script/`, and `core/` layout.
When editing or adding code:

- Use `crates/core`, `crates/guest`, and `crates/host`.
- Do not recreate the deleted top-level directories.
- Be aware that some comments and scripts still use legacy names like `bitcoin-header-chain-script` or `script/Cargo.toml`.
- `crates/zkpow-core` exists in the tree but is not a workspace member; do not treat it as the active shared crate unless you are explicitly cleaning up migration leftovers.

## Crate Boundaries

Keep responsibilities strict:

- `crates/core`: pure consensus/state logic only. No host I/O, no SQLite, no proving orchestration.
- `crates/guest`: zkVM-safe execution only. No host-only crates or convenience hashing crates.
- `crates/host`: env parsing, DB access, proof generation, proof verification, observability, and file output.
- `contracts/`: Solidity-side proof consumption and relay state.

If a change affects consensus semantics, start in `crates/core` and let host/guest adapt around it.

## Proof Pipeline Facts

The host pipeline is centered in [crates/host/src/proof_pipeline.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/host/src/proof_pipeline.rs).
Key behavior:

- `config_from_env()` currently reads `PREV_PROOF`, `NUM_HEADERS`, and `OUTPUT_DIR`.
- The DB path defaults internally; current commands assume the checked-in SQLite file at repo root.
- `generate_and_save_proofs()` loads the prior proof if present, derives the current state, loads new headers from SQLite, executes the guest, proves compressed, verifies public values, then shrink-wraps into Groth16.
- The guest verifies the recursive proof only when `state.height > 0`.

## Guest Program Facts

The guest entrypoint lives in [crates/guest/src/main.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/guest/src/main.rs).
Important invariants:

- Input is serialized once on the host via `Input::to_bytes()`.
- On recursive runs, the recursive proof witness is written separately with `stdin.write_proof(...)`.
- The guest commits serialized success state on success, or last valid state + error metadata on failure.
- Guest hashing must remain SP1-compatible.

## SHA / Hashing Rules

- Keep guest hashing in `crates/guest/src/sha256.rs` using SP1-compatible paths.
- Do not introduce `sha2` or other host-style hashing dependencies into the guest.
- Host-side digesting for proof plumbing is fine in `crates/host`.

## Contracts Boundary

The Solidity relay in `contracts/` does not want the raw internal `rkyv` state blob as its long-term interface.
The contract README documents a compact proof-boundary summary format.
If you change the proof output or public-values boundary, verify whether the Solidity-facing serialization contract needs to change as well.

## Commands

Run commands from repo root unless there is a specific reason not to.

### Rust

```bash
cargo build --release
cargo run --release -p zkpow-host --bin zkpow-host
PREV_PROOF=./proof_height_1_to_100.bin NUM_HEADERS=100 OUTPUT_DIR=. \
  cargo run --release -p zkpow-host --bin zkpow-host
cargo run --release -p zkpow-host --bin test_errors
cargo run --release -p zkpow-host --bin inspect_proof -- ./proof_height_1_to_100.bin
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

### Solidity

```bash
forge build --root contracts
forge test --root contracts
```

## Observability / Profiling

- Host observability initializes Tokio console and tracing in [crates/host/src/observability.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/host/src/observability.rs).
- `TOKIO_CONSOLE_BIND` defaults to `127.0.0.1:6669`.
- `RUST_LOG=info` is the normal first step when you need runtime visibility.
- `scripts/profile-sp1.sh` exists, but verify it still targets the current workspace before relying on it. It still contains legacy `script/` references at the time of writing.

## Agent Working Rules

- Prefer codebase-memory graph tools for code discovery before falling back to grep.
- Stage only the files you intentionally changed. This repo is often worked in with an already-dirty refactor branch.
- When changing interfaces shared across host/core/guest, update all three layers in one pass.
- When touching proof-boundary semantics, check `contracts/README.md` and the relay scaffold before declaring the change complete.
- When a comment or usage string conflicts with `cargo metadata`, treat the manifest and actual targets as canonical.

## Fast Orientation

Useful files:

- [Cargo.toml](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/Cargo.toml)
- [crates/core/src/lib.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/lib.rs)
- [crates/core/src/input.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/input.rs)
- [crates/guest/src/main.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/guest/src/main.rs)
- [crates/guest/src/sha256.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/guest/src/sha256.rs)
- [crates/host/src/proof_pipeline.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/host/src/proof_pipeline.rs)
- [crates/host/src/util.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/host/src/util.rs)
- [crates/host/src/observability.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/host/src/observability.rs)
- [contracts/README.md](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/contracts/README.md)
