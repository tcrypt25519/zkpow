# HeaderStore

**Location:** `crates/store/src/db.rs:87`
**Kind:** Class
**Bases:** —
**Module:** [src_db](../modules/src_db.md)

## Description

The SQLite header store.

## Attributes

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `conn` | `Connection` | *required* | — |

## Methods

| Method | Signature | Decorators | Description |
|--------|-----------|------------|-------------|
| `open` | `(db_path: impl AsRef<Path >) -> Result<Self >` | — | Open the database, creating the schema when missing. |
| `get_best_tip` | `() -> ResultOpt<Tip >` | — | — |
| `bulk_insert` | `(headers: &[ConnectedHeader]) -> Result<() >` | — | — |
| `header_count` | `() -> Result<u64 >` | — | — |
| `insert_genesis` | `(genesis: &ConnectedHeader) -> Result<() >` | — | — |
| `load_compact_target` | `(height: u64) -> ResultOpt<CompactTarget >` | — | — |
| `load_header_records` | `(start_height: u64, count: u64) -> Result<Vec<HeaderRecord>>` | — | — |
| `load_header_record` | `(height: u64) -> ResultOpt<HeaderRecord >` | — | — |
| `load_new_headers` | `(start_height: u64, count: u64) -> Result<Vec<HeaderInput>>` | — | — |
| `load_header_batch_witness` | `(start_height: u64, count: u64) -> Result<HeaderBatchWitness >` | — | Load `count` headers from `start_height`, plus a sparse median hint for each header whose |
| `state_at_height` | `(height: u32, genesis_hash: BlockHash) -> ResultOpt<State >` | — | Reconstruct full consensus state at an arbitrary height. |
| `load_run` | `(run_id: &str) -> ResultOpt<RunRecord >` | — | — |
| `create_run` | `(run_id: &str, record: &RunRecord) -> Result<() >` | — | — |
| `record_run_progress` | `(run_id: &str, verified_height: u32, proof_object: Option<&str >) -> Result<() >` | — | Persist a newly verified height. Callers must only invoke this after |
| `delete_run` | `(run_id: &str) -> Result<() >` | — | Forget a run entirely (operator reset of a halted or abandoned chain). |

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
*No generated relationships detected.*

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [src_db](../modules/src_db.md) | 15 | `conn` |
