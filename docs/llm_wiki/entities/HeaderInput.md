# HeaderInput

**Location:** `crates/core/src/header.rs:8`
**Kind:** Class
**Bases:** —
**Module:** [core_src_header](../modules/core_src_header.md)

## Description

Prover-supplied fields for a new header.

The remaining header fields are not supplied: `prev_blockhash` comes from the state being
extended and `compact_target` from the active epoch, so a prover cannot choose either.

## Attributes

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `version` | `u32` | *required* | — |
| `merkle_root` | `MerkleRoot` | *required* | — |
| `timestamp` | `BlockTimestamp` | *required* | — |
| `nonce` | `u32` | *required* | — |

## Methods

| Method | Signature | Decorators | Description |
|--------|-----------|------------|-------------|
| `to_header` | `(prev_blockhash: BlockHash, compact_target: CompactTarget) -> Header` | — | Complete this input into a full [`Header`] using the two fields a prover does not choose: |

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
*No generated relationships detected.*

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [core_src_header](../modules/core_src_header.md) | 1 | `merkle_root`, `nonce`, `timestamp`, `version` |
