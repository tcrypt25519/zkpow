# error Module

**Path:** `crates/sync-headers/src/error.rs`

## Description

_Auto-generated from `crates/sync-headers/src/error.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `crate::connection::Command` | `Command` |
| `crate::types::Tip` | `Tip` |
| `std::net::SocketAddr` | `SocketAddr` |
| `thiserror::Error` | `Error` |

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
| [ConnectionError](../entities/ConnectionError.md) | Enum | 11 | `From` | Transport, wire-format, and protocol failures reported by a peer connection. |
| [SyncHeadersError](../entities/SyncHeadersError.md) | Enum | 65 | — | The outcome of a sync-headers run: either we reached the tip or we hit |
