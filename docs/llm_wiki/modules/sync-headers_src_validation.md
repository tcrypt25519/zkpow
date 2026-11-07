# validation Module

**Path:** `crates/sync-headers/src/validation.rs`

## Description

_Auto-generated from `crates/sync-headers/src/validation.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `crate::types::ConnectedHeader` | `ConnectedHeader` |
| `crate::types::Header` | `Header` |
| `crate::types::Tip` | `Tip` |
| `zkpow_core::calculate_next_work_required` | `calculate_next_work_required` |
| `zkpow_core::check_proof_of_work` | `check_proof_of_work` |
| `zkpow_params::Consensus` | `Consensus` |
| `zkpow_types::BlockHash` | `BlockHash` |
| `zkpow_types::BlockTimestamp` | `BlockTimestamp` |
| `zkpow_types::TargetTriplet` | `TargetTriplet` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 3 | 0 |

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [ValidationError](../entities/ValidationError.md) | Enum | 7 | — | — |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `connect_headers` | `(tip: &mut Tip, headers: impl IntoIterator<Item = &'a Header >) -> Vec<ConnectedHeader >` | — | Append the valid header prefix to `tip` and return its connected headers. |
