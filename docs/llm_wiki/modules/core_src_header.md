# header Module

**Path:** `crates/core/src/header.rs`

## Description

Defines [`HeaderInput`](../entities/HeaderInput.md), the four header fields a prover supplies: `version`, `merkle_root`, `timestamp`, and `nonce`. `to_header` fills in `prev_blockhash` from the state being extended and `compact_target` from the active epoch, so the prover chooses neither.

## Imports

| Source | Symbols |
|--------|---------|
| `crate::BlockHash` | `BlockHash` |
| `crate::BlockTimestamp` | `BlockTimestamp` |
| `crate::CompactTarget` | `CompactTarget` |
| `crate::Header` | `Header` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [HeaderInput](../entities/HeaderInput.md) | Class | 8 | — | Prover-supplied fields for a new header. |
