# lib Module

**Path:** `crates/worker/src/lib.rs`

## Description

_Auto-generated from `crates/worker/src/lib.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `config::SHARD_PROVER_PERMITS_ENV` | `SHARD_PROVER_PERMITS_ENV` |
| `config::WorkerConfig` | `WorkerConfig` |
| `jobs::execute` | `execute` |
| `jobs::prove_compressed` | `prove_compressed` |
| `sp1_sdk::CpuProver` | `CpuProver` |
| `sp1_sdk::ProverClient` | `ProverClient` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 3 | 2 |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `build_cpu_worker` | *(async)* `(config: &WorkerConfig) -> CpuProver` | — | Build the CPU proving worker, logging the resolved [`WorkerConfig`]. |
