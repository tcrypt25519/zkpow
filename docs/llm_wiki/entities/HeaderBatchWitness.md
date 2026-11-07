# HeaderBatchWitness

**Location:** `crates/store/src/db.rs:483`
**Kind:** Class
**Bases:** —
**Module:** [src_db](../modules/src_db.md)

**Decorators:** `@Debug`, `@Clone`

## Description

The SP1 stdin witness for a batch: the new headers plus their median-time-past hints.

## Attributes

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `headers` | `Vec<HeaderInput >` | *required* | — |
| `median_time_past_hints` | `Vec<BlockTimestamp >` | *required* | — |

## Methods

*No public methods. Inherits from base classes.*

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
```mermaid
flowchart LR
    n0["HeaderBatchWitness (crates/store/src/db.rs)"]
    n1["HeaderStore.load_header_batch_witness (crates/store/src/db.rs)"]
    n1 --> n0
    click n0 "../modules/src_db.md"
    click n1 "../modules/src_db.md"
```

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [src_db](../modules/src_db.md) | 0 | `headers`, `median_time_past_hints` |

### References

| Reference | Kind | Source | Call sites |
|---|---|---|---:|
| `HeaderStore.load_header_batch_witness` | type_reference | [src_db](../modules/src_db.md) | — |
