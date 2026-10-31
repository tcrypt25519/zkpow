# ConnectedHeader

**Location:** `crates/sync-headers/src/types.rs:124`
**Kind:** Class
**Bases:** —
**Module:** [src_types](../modules/src_types.md)

**Decorators:** `@Debug`, `@Clone`

## Description

A header linked onto the chain, with the chain values derived at its height.

## Attributes

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `header` | `Header` | *required* | — |
| `block_hash` | `BlockHash` | *required* | — |
| `chainwork` | `ChainWork` | *required* | — |
| `ordered_timestamp_run` | `u64` | *required* | — |
| `height` | `u32` | *required* | — |
| `median_time_past` | `u32` | *required* | — |

## Methods

| Method | Signature | Decorators | Description |
|--------|-----------|------------|-------------|
| `new` | `(header: Header, block_hash: BlockHash, height: u32, chainwork: ChainWork, median_time_past: u32, ordered_timestamp_run: u64) -> Self` | — | — |
| `block_hash` | `() -> BlockHash` | — | — |
| `prev_blockhash` | `() -> BlockHash` | — | — |

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
*No generated relationships detected.*

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [src_types](../modules/src_types.md) | 3 | `block_hash`, `chainwork`, `header`, `height`, `median_time_past`, `ordered_timestamp_run` |
