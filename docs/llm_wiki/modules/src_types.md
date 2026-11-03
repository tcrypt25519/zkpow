# types Module

**Path:** `crates/sync-headers/src/types.rs`

## Description

_Auto-generated from `crates/sync-headers/src/types.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `strum_macros::Display` | `Display` |
| `strum_macros::EnumCount` | `EnumCount` |
| `strum_macros::EnumString` | `EnumString` |
| `zkpow_core::block_work_from_target` | `block_work_from_target` |
| `zkpow_core::target_from_bits` | `target_from_bits` |
| `zkpow_params::Consensus` | `Consensus` |
| `zkpow_params::WINDOW_SIZE` | `WINDOW_SIZE` |
| `zkpow_types::BlockHash` | `BlockHash` |
| `zkpow_types::ChainWork` | `ChainWork` |
| `zkpow_types::CompactTarget` | `CompactTarget` |
| `zkpow_types::Header` | `Header` |
| `zkpow_types::TargetTriplet` | `TargetTriplet` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 4 | 0 |

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [Network](../entities/Network.md) | Enum | 11 | — | Network selection for Bitcoin consensus. |
| [Tip](../entities/Tip.md) | Class | 25 | — | The tip of the best known chain. |
| [Header](../entities/src_types_Header.md) | Class | 93 | — | A raw block header received from the network. |
| [ConnectedHeader](../entities/ConnectedHeader.md) | Class | 113 | — | A header linked onto the chain, with the chain values derived at its height. |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `expand_target` | `(bits: CompactTarget) -> TargetTriplet` | — | Expand `bits` into the `(nbits, target, block_work)` triple a `Tip` caches. |
