# lib Module

**Path:** `crates/params/src/lib.rs`

## Description

Crate root of `zkpow-params`, a `no_std` crate between `zkpow-types` and `zkpow-core`. Declares the networks: [`Mainnet`](../entities/Mainnet.md), with the genesis header, PoW limit, genesis work, 600-second blocks, 2016-block epochs, and a 4x timespan clamp; and [`ShortEpoch`](../entities/ShortEpoch.md), mainnet with 32-block epochs, available only under `cfg(test)` or the `test-networks` feature. Code reads values through the trait, as in `Mainnet::PARAMS.genesis`.

## Imports

| Source | Symbols |
|--------|---------|
| `consensus::Consensus` | `Consensus` |
| `consensus::ConsensusParams` | `ConsensusParams` |
| `consensus::WINDOW_LARGE_HALF_SIZE` | `WINDOW_LARGE_HALF_SIZE` |
| `consensus::WINDOW_MAX_INDEX` | `WINDOW_MAX_INDEX` |
| `consensus::WINDOW_SIZE` | `WINDOW_SIZE` |
| `consensus::WINDOW_SMALL_HALF_SIZE` | `WINDOW_SMALL_HALF_SIZE` |
| `zkpow_types::BlockHash` | `BlockHash` |
| `zkpow_types::BlockTimestamp` | `BlockTimestamp` |
| `zkpow_types::BlockWork` | `BlockWork` |
| `zkpow_types::CompactTarget` | `CompactTarget` |
| `zkpow_types::Header` | `Header` |
| `zkpow_types::Target` | `Target` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 2 | 1 |

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [Mainnet](../entities/Mainnet.md) | Class | 47 | `Consensus` | Bitcoin mainnet. |
| [ShortEpoch](../entities/ShortEpoch.md) | Class | 60 | `Consensus` | A synthetic network that differs from [`Mainnet`] only in `epoch_length`, so tests can cross a |
