# PeerConnection

**Location:** `crates/sync-headers/src/connection.rs:150`
**Kind:** Class
**Bases:** —
**Module:** [src_connection](../modules/src_connection.md)

## Description

A P2P connection to a Bitcoin peer.

## Attributes

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `stream` | `TcpStream` | *required* | — |
| `network_magic` | `[u8 ; 4]` | *required* | — |

## Methods

| Method | Signature | Decorators | Description |
|--------|-----------|------------|-------------|
| `connect` | `(addr: SocketAddr, network: Network, start_height: i32) -> std::io::Result<Self >` | — | Open the TCP connection and complete the version handshake. |
| `send_getheaders` | `(locators: &[BlockHash], stop_hash: BlockHash) -> std::io::Result<() >` | — | — |
| `read_headers` | `() -> std::io::Result<Vec<Header>>` | — | Read a `headers` message and return validated headers using our own types. |

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
*No generated relationships detected.*

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [src_connection](../modules/src_connection.md) | 3 | `network_magic`, `stream` |
