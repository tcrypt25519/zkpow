# p2p_sync Module

**Path:** `crates/sync-headers/src/p2p_sync.rs`

## Description

_Auto-generated from `crates/sync-headers/src/p2p_sync.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `crate::connection::PeerConnection` | `PeerConnection` |
| `crate::error::SyncHeadersError` | `SyncHeadersError` |
| `crate::msg::MAX_HEADERS_PER_MSG` | `MAX_HEADERS_PER_MSG` |
| `crate::peers::PeerStore` | `PeerStore` |
| `crate::types::Header` | `Header` |
| `crate::types::Network` | `Network` |
| `crate::types::Tip` | `Tip` |
| `std::net::SocketAddr` | `SocketAddr` |
| `std::thread` | `thread` |
| `std::time::Duration` | `Duration` |
| `zkpow_core::BlockHash` | `BlockHash` |
| `zkpow_params::Consensus` | `Consensus` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 2 | 0 |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `sync` | `(network: Network, anchor: &Tip, max_headers: u32, peer_store: &mut PeerStore, mut on_headers: C) -> Result<Tip, SyncHeadersError<E>>` | — | Sync headers from the Bitcoin P2P network, passing each batch to a callback. |
