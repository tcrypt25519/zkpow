> ⚠️ **Stale:** Source no longer found in codebase. Run `llm-wiki lint` to audit.

# p2p_sync Module

**Path:** `crates/sync-headers/src/p2p_sync.rs`

## Description

Coordinates peer selection, connection retries, and header-batch callbacks. The private `PeerSyncError` enum carries connection, peer, or callback failures without a tip snapshot. Accepted progress stays in the mutable tip and counters shared with each peer attempt. Peer failures retry directly from that state. When sync returns a terminal [SyncHeadersError](../entities/SyncHeadersError.md), it moves its owned tip into the error instead of cloning it. Exhausted retries retain the last failure, and a partially accepted batch reports `ConnectionError::InvalidHeaders` with accepted and total counts before trying another peer.

## Imports

| Source | Symbols |
|--------|---------|
| `crate::connection::PeerConnection` | `PeerConnection` |
| `crate::error::ConnectionError` | `ConnectionError` |
| `crate::error::SyncHeadersError` | `SyncHeadersError` |
| `crate::msg::MAX_HEADERS_PER_MSG` | `MAX_HEADERS_PER_MSG` |
| `crate::peers::PeerStore` | `PeerStore` |
| `crate::types::Header` | `Header` |
| `crate::types::Network` | `Network` |
| `crate::types::Tip` | `Tip` |
| `std::net::SocketAddr` | `SocketAddr` |
| `std::thread` | `thread` |
| `std::time::Duration` | `Duration` |
| `zkpow_params::Consensus` | `Consensus` |
| `zkpow_types::BlockHash` | `BlockHash` |

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