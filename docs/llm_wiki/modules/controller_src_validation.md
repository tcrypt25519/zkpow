# validation Module

**Path:** `crates/controller/src/validation.rs`

## Description

_Auto-generated from `crates/controller/src/validation.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `zkpow_core::BlockTimestamp` | `BlockTimestamp` |
| `zkpow_core::Header` | `Header` |
| `zkpow_core::HeaderInput` | `HeaderInput` |
| `zkpow_core::State` | `State` |
| `zkpow_params::Consensus` | `Consensus` |
| `zkpow_store::HeaderRecord` | `HeaderRecord` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 3 | 0 |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `compute_final_state_cold` | `(initial_state: &State, headers: &[HeaderInput], hints: &[BlockTimestamp]) -> State` | — | Apply the cold start batch locally with its sparse median-time-past hints. |
| `compute_final_state_warm` | `(initial_state: &State, headers: &[HeaderInput], hints: &[BlockTimestamp]) -> State` | — | Apply a warm batch locally with its sparse median-time-past hints. |
| `records_to_new_headers` | `(records: &[HeaderRecord]) -> Vec<HeaderInput >` | — | Strip stored records down to the prover-supplied header fields. |
