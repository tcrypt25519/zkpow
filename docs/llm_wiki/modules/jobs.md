# jobs Module

**Path:** `crates/worker/src/jobs.rs`

## Description

_Auto-generated from `crates/worker/src/jobs.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `sp1_sdk::ExecutionReport` | `ExecutionReport` |
| `sp1_sdk::prelude::*` | `*` |
| `std::fmt` | `fmt` |
| `std::sync::atomic::AtomicU64` | `AtomicU64` |
| `std::sync::atomic::Ordering` | `Ordering` |
| `std::time::Instant` | `Instant` |
| `tracing::Instrument` | `Instrument` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 2 | 0 |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `execute` | *(async)* `(prover: &P, elf: Elf, stdin: SP1Stdin) -> Result<(Vec<u8>, ExecutionReport), String >` | — | Execute the guest program on `stdin` with deferred proof verification |
| `prove_compressed` | *(async)* `(prover: &'a P, proving_key: &'a P::ProvingKey, stdin: SP1Stdin) -> Result<SP1ProofWithPublicValues, P::Error >` | — | Prove `stdin` in compressed mode. |
