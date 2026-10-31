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
| `zkpow_core::BlockHash` | `BlockHash` |
| `zkpow_core::ChainWork` | `ChainWork` |
| `zkpow_core::CompactTarget` | `CompactTarget` |
| `zkpow_core::Header` | `Header` |
| `zkpow_core::TargetTriplet` | `TargetTriplet` |
| `zkpow_core::block_work_from_target` | `block_work_from_target` |
| `zkpow_core::target_from_bits` | `target_from_bits` |
| `zkpow_params::Consensus` | `Consensus` |
| `zkpow_params::WINDOW_SIZE` | `WINDOW_SIZE` |

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
| [Network](../entities/Network.md) | Enum | 12 | — | Network selection for Bitcoin consensus. |
| [Tip](../entities/Tip.md) | Class | 26 | — | The tip of the best known chain. |
| [Header](../entities/src_types_Header.md) | Class | 94 | — | A raw block header received from the network. |
| [ConnectedHeader](../entities/ConnectedHeader.md) | Class | 114 | — | A header linked onto the chain, with the chain values derived at its height. |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `expand_target` | `(bits: CompactTarget) -> TargetTriplet` | — | Expand `bits` into the `(nbits, target, block_work)` triple a `Tip` caches. |
