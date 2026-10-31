# state Module

**Path:** `crates/core/src/state.rs`

## Description

Defines [`State`](../entities/State.md), the chain state a proof attests to: cumulative work, genesis and tip hashes, height, current `nbits`, the epoch's first timestamp, and the 11-slot timestamp window. `State::anchor` derives the height-0 state from the network's anchor header.

`apply_headers_from_genesis` runs once per chain: it applies the cold batch until the window is full, then passes the rest to the warm path. `apply_headers` assumes a full window, checks each header's median-time-past (from an ordered window or a prover hint) and proof of work, and retargets at each epoch boundary. A rejected header leaves the state at the last header that passed.

## Imports

| Source | Symbols |
|--------|---------|
| `crate::ApplyError` | `ApplyError` |
| `crate::BlockHash` | `BlockHash` |
| `crate::BlockTimestamp` | `BlockTimestamp` |
| `crate::ChainWork` | `ChainWork` |
| `crate::CompactTarget` | `CompactTarget` |
| `crate::HeaderInput` | `HeaderInput` |
| `crate::Target` | `Target` |
| `crate::TargetTriplet` | `TargetTriplet` |
| `crate::block_work_from_target` | `block_work_from_target` |
| `crate::calculate_next_work_required` | `calculate_next_work_required` |
| `crate::check_proof_of_work_words` | `check_proof_of_work_words` |
| `crate::hash_header_input` | `hash_header_input` |
| `crate::target_from_bits` | `target_from_bits` |
| `zkpow_mtp::Cold` | `Cold` |
| `zkpow_mtp::WINDOW_SIZE` | `WINDOW_SIZE` |
| `zkpow_mtp::Warm` | `Warm` |
| `zkpow_mtp::errors::NotMedian` | `NotMedian` |
| `zkpow_mtp::self` | `self` |
| `zkpow_mtp::timestamp_count` | `timestamp_count` |
| `zkpow_mtp::timestamp_slot` | `timestamp_slot` |
| `zkpow_params::Consensus` | `Consensus` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 2 | 0 |

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [State](../entities/State.md) | Class | 17 | — | The canonical chain state the prover attests to and each batch of transitions starts |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `drop` | `()` | — | — |
