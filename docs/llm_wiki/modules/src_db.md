# db Module

**Path:** `crates/store/src/db.rs`

## Description

SQLite access to `headers.db`; every reader and writer in the workspace goes through [`HeaderStore`](../entities/HeaderStore.md). The `headers` table has one row per height with the header plus derived chain work, median-time-past, and ordered timestamp run. The `runs` table holds one [`RunRecord`](../entities/RunRecord.md) of verified progress per proving run. Sync writes with `insert_genesis` and `bulk_insert` and reads the best tip; the prover reads [`HeaderBatchWitness`](../entities/HeaderBatchWitness.md) batches and `state_at_height`. Chain work is stored big-endian, unlike the little-endian `ChainWork` in memory.

## Imports

| Source | Symbols |
|--------|---------|
| `rusqlite::Connection` | `Connection` |
| `rusqlite::OptionalExtension` | `OptionalExtension` |
| `rusqlite::params` | `params` |
| `std::path::Path` | `Path` |
| `sync_headers::ConnectedHeader` | `ConnectedHeader` |
| `sync_headers::Tip` | `Tip` |
| `zkpow_core::BlockHash` | `BlockHash` |
| `zkpow_core::BlockTimestamp` | `BlockTimestamp` |
| `zkpow_core::ChainWork` | `ChainWork` |
| `zkpow_core::CompactTarget` | `CompactTarget` |
| `zkpow_core::Header` | `Header` |
| `zkpow_core::HeaderInput` | `HeaderInput` |
| `zkpow_core::State` | `State` |
| `zkpow_core::VerifierKeyDigest` | `VerifierKeyDigest` |
| `zkpow_core::requires_median_hint` | `requires_median_hint` |
| `zkpow_params::Consensus` | `Consensus` |
| `zkpow_params::WINDOW_SIZE` | `WINDOW_SIZE` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 4 | 1 |

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [Error](../entities/Error.md) | Enum | 18 | `From` | — |
| [Result](../entities/Result.md) | Type alias | 45 | — | — |
| [ResultOpt](../entities/ResultOpt.md) | Type alias | 46 | — | — |
| [HeaderStore](../entities/HeaderStore.md) | Class | 104 | — | The SQLite header store. |
| [RunRecord](../entities/RunRecord.md) | Class | 516 | — | One IVC chain's verified progress, keyed by `run_id` in the `runs` table. |
| [HeaderRecord](../entities/HeaderRecord.md) | Class | 527 | — | A single `headers` row, decoded into typed fields. |
| [HeaderBatchWitness](../entities/HeaderBatchWitness.md) | Class | 537 | — | The SP1 stdin witness for a batch: the new headers plus their median-time-past hints. |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `genesis_state_from_record` | `(genesis: HeaderRecord, genesis_hash: BlockHash) -> State` | — | The anchor state, after checking that the stored record is the record of that anchor. |
| `into` | `() -> Error` | — | — |