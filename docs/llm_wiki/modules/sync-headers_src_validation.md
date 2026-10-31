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
| `crate::types::expand_target` | `expand_target` |
| `zkpow_core::BlockTimestamp` | `BlockTimestamp` |
| `zkpow_core::advance_ordered_timestamp_run` | `advance_ordered_timestamp_run` |
| `zkpow_core::calculate_next_work_required` | `calculate_next_work_required` |
| `zkpow_core::check_proof_of_work` | `check_proof_of_work` |
| `zkpow_core::has_ordered_timestamp_window` | `has_ordered_timestamp_window` |
| `zkpow_core::ordered_median_timestamp_slot` | `ordered_median_timestamp_slot` |
| `zkpow_core::timestamp_slot` | `timestamp_slot` |
| `zkpow_params::Consensus` | `Consensus` |
| `zkpow_params::WINDOW_SIZE` | `WINDOW_SIZE` |

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
| `connect_headers` | `(anchor: &Tip, headers: &[Header]) -> Vec<ConnectedHeader >` | — | Validate and link `headers` onto `anchor`: linkage, PoW against the active target, and the |
| `tip_from_connected` | `(anchor: &Tip, connected: &[ConnectedHeader]) -> Tip` | — | The tip after appending `connected` to `anchor`. Panics when `connected` is empty. |
