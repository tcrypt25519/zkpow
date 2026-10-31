# Tip

**Location:** `crates/sync-headers/src/types.rs:28`
**Kind:** Class
**Bases:** —
**Module:** [src_types](../modules/src_types.md)

**Decorators:** `@Debug`, `@Clone`

## Description

The tip of the best known chain.

## Attributes

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `hash` | `BlockHash` | *required* | — |
| `height` | `u32` | *required* | — |
| `chainwork` | `ChainWork` | *required* | — |
| `timestamps` | `[u32 ; WINDOW_SIZE]` | *required* | — |
| `ordered_timestamp_run` | `u64` | *required* | — |
| `epoch_timestamp` | `u32` | *required* | — |
| `current_nbits` | `CompactTarget` | *required* | — |
| `current_target` | `Target` | *required* | — |
| `current_block_work` | `BlockWork` | *required* | — |

## Methods

| Method | Signature | Decorators | Description |
|--------|-----------|------------|-------------|
| `new` | `(hash: BlockHash, height: u32, chainwork: ChainWork, timestamps: [u32 ; WINDOW_SIZE], ordered_timestamp_run: u64, epoch_timestamp: u32, current_nbits: CompactTarget, current_target: Target, current_block_work: BlockWork) -> Self` | — | — |
| `genesis` | `(hash: BlockHash) -> Self` | — | — |

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
*No generated relationships detected.*

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [src_types](../modules/src_types.md) | 2 | `chainwork`, `current_block_work`, `current_nbits`, `current_target`, `epoch_timestamp`, `hash`, `height`, `ordered_timestamp_run`, `timestamps` |
