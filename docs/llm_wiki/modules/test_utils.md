# test_utils Module

**Path:** `crates/store/src/tests/test_utils.rs`

## Description

_Auto-generated from `crates/store/src/tests/test_utils.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `crate::HeaderStore` | `HeaderStore` |
| `crate::db::SQLITE_MEMORY_DATABASE` | `SQLITE_MEMORY_DATABASE` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
```mermaid
flowchart LR
    n0["crates/store/src/lib.rs"]
    n1["crates/store/src/tests/test_utils.rs"]
    n0 --> n1
    click n0 "../modules/store_src_lib.md"
    click n1 "../modules/test_utils.md"
```

### Internal neighbors

| Direction | Module |
|---|---|
| Inbound | [store_src_lib](../modules/store_src_lib.md) |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `in_memory_store` | `() -> HeaderStore` | — | — |
| `create_test_fixture_store` | `() -> HeaderStore` | — | — |
