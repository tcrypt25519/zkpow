# consensus Module

**Path:** `crates/params/src/consensus.rs`

## Description

Defines [`ConsensusParams`](../entities/ConsensusParams.md), the independent values that make a network, and the [`Consensus`](../entities/Consensus.md) trait that binds one set to a type. Derived values (expected epoch time, the epoch timespan clamps, genesis target and work) are `const fn`s, and `Consensus::GENESIS_HASH` is computed once at compile time with `sha256_const`. `WINDOW_SIZE` (11) and its half sizes are plain constants rather than params because they size arrays.

## Imports

| Source | Symbols |
|--------|---------|
| `sha256_const::double` | `double` |
| `zkpow_types::BlockHash` | `BlockHash` |
| `zkpow_types::BlockWork` | `BlockWork` |
| `zkpow_types::ChainWork` | `ChainWork` |
| `zkpow_types::CompactTarget` | `CompactTarget` |
| `zkpow_types::Header` | `Header` |
| `zkpow_types::Target` | `Target` |

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
| [ConsensusParams](../entities/ConsensusParams.md) | Class | 9 | — | The parameters that define a network. |
| [Consensus](../entities/Consensus.md) | Class | 77 | — | Binds one parameter set to a type, so consensus code can be generic over the network and still |