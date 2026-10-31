# ConsensusParams

**Location:** `crates/params/src/consensus.rs:24`
**Kind:** Class
**Bases:** —
**Module:** [consensus](../modules/consensus.md)

**Decorators:** `@Clone`, `@Copy`, `@Debug`, `@PartialEq`, `@Eq`

## Description

The parameters that define a network.

Only independent values are fields. Anything derivable is a `const fn`, so there is exactly one
definition of each and a `C: Consensus` bound still folds down to literals at the call site.

## Attributes

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `genesis` | `Header` | *required* | — |
| `genesis_target` | `Target` | *required* | — |
| `genesis_work` | `BlockWork` | *required* | — |
| `block_time` | `u32` | *required* | — |
| `epoch_length` | `u32` | *required* | — |
| `max_epoch_over_mul` | `u8` | *required* | — |
| `min_epoch_under_div` | `u8` | *required* | — |

## Methods

| Method | Signature | Decorators | Description |
|--------|-----------|------------|-------------|
| `genesis_hash` | `() -> BlockHash` | — | SHA-256d of the genesis header. Prefer [`Consensus::GENESIS_HASH`], which holds the result |
| `expected_epoch_time` | `() -> u32` | — | Seconds a difficulty epoch is expected to take. |
| `max_epoch_timespan` | `() -> i64` | — | Upper clamp on a measured epoch timespan. `i64` because the measurement is a difference of |
| `min_epoch_timespan` | `() -> i64` | — | Lower clamp on a measured epoch timespan. |
| `compact_target` | `() -> CompactTarget` | — | The compact target the genesis header commits to. |
| `target` | `() -> Target` | — | The target in force at genesis: the PoW limit. |
| `block_work` | `() -> BlockWork` | — | Work proven by one block at the genesis target. |
| `chain_work` | `() -> ChainWork` | — | [`ConsensusParams::genesis_work`] as the chain's cumulative work after genesis. |

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
*No generated relationships detected.*

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [consensus](../modules/consensus.md) | 8 | `block_time`, `epoch_length`, `genesis`, `genesis_target`, `genesis_work`, `max_epoch_over_mul`, `min_epoch_under_div` |
