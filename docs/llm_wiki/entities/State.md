# State

**Location:** `crates/core/src/state.rs:17`
**Kind:** Class
**Bases:** —
**Module:** [state](../modules/state.md)

## Description

The canonical chain state the prover attests to and each batch of transitions starts
from. While a batch is being applied it still describes the last committed header; it
reflects the updated chain only after the batch commits.

## Attributes

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `chain_work` | `ChainWork` | *required* | — |
| `genesis_hash` | `BlockHash` | *required* | — |
| `block_hash` | `BlockHash` | *required* | — |
| `height` | `u32` | *required* | — |
| `current_nbits` | `CompactTarget` | *required* | — |
| `epoch_timestamp` | `BlockTimestamp` | *required* | — |
| `timestamps` | `mtp::WindowValues` | *required* | — |

## Methods

| Method | Signature | Decorators | Description |
|--------|-----------|------------|-------------|
| `anchor` | `() -> Self` | — | The state that follows the network's anchor header, derived from that header's own |
| `apply_headers_from_genesis` | `(headers: &[HeaderInput], median_hints: &[BlockTimestamp]) -> Result<(), ApplyError >` | — | Apply the cold start batch. This is executed exactly once for a chain, applying the first `window_size - 1` |
| `apply_headers` | `(headers: &[HeaderInput], median_hints: &[BlockTimestamp]) -> Result<(), ApplyError >` | — | Apply headers while assuming the MTP window is full. The caller is responsible for only |

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
*No generated relationships detected.*

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [state](../modules/state.md) | 3 | `block_hash`, `chain_work`, `current_nbits`, `epoch_timestamp`, `genesis_hash`, `height`, `timestamps` |
