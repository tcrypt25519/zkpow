# peers Module

**Path:** `crates/sync-headers/src/peers.rs`

## Description

_Auto-generated from `crates/sync-headers/src/peers.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `rand::seq::IndexedRandom` | `IndexedRandom` |
| `std::collections::HashMap` | `HashMap` |
| `std::collections::HashSet` | `HashSet` |
| `std::fmt` | `fmt` |
| `std::net::SocketAddr` | `SocketAddr` |
| `std::net::ToSocketAddrs` | `ToSocketAddrs` |
| `std::time::SystemTime` | `SystemTime` |

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
| [PeerStatus](../entities/PeerStatus.md) | Enum | 29 | `fmt::Display` | Lifecycle status of a known peer address. |
| [PeerEntry](../entities/PeerEntry.md) | Class | 56 | — | A known peer address with its current status and a timestamp. |
| [PeerStore](../entities/PeerStore.md) | Class | 79 | — | A collection of discovered peers with status tracking. |
