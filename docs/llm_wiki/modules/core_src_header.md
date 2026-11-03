# header Module

**Path:** `crates/core/src/header.rs`

## Description

Defines [`HeaderInput`](../entities/HeaderInput.md), the four header fields a prover supplies: `version`, `merkle_root`, `timestamp`, and `nonce`. `to_header` fills in `prev_blockhash` from the state being extended and `compact_target` from the active epoch, so the prover chooses neither.

## Imports

| Source | Symbols |
|--------|---------|
| `zkpow_types::BlockHash` | `BlockHash` |
| `zkpow_types::BlockTimestamp` | `BlockTimestamp` |
| `zkpow_types::CompactTarget` | `CompactTarget` |
| `zkpow_types::Header` | `Header` |
| `zkpow_types::MerkleRoot` | `MerkleRoot` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 1 | 0 |

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [HeaderInput](../entities/HeaderInput.md) | Class | 8 | — | Prover-supplied fields for a new header. |