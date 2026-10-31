# PeerConnection

**Location:** `crates/sync-headers/src/connection.rs:168`
**Kind:** Class
**Bases:** —
**Module:** [src_connection](../modules/src_connection.md)

## Description

A P2P connection to a Bitcoin peer.

Connection setup, header requests, and header reads return [ConnectionError](ConnectionError.md). Socket and Bitcoin codec failures retain their sources; wire validation reports structured protocol variants. The same error type flows through the private handshake and message-framing operations.

## Attributes

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `stream` | `TcpStream` | *required* | — |
| `network_magic` | `[u8 ; 4]` | *required* | — |

## Methods

| Method | Signature | Decorators | Description |
|--------|-----------|------------|-------------|
| `connect` | `(addr: SocketAddr, network: Network, start_height: i32) -> Result<Self, ConnectionError >` | — | Open the TCP connection and complete the version handshake. |
| `send_getheaders` | `(locators: &[BlockHash], stop_hash: BlockHash) -> Result<(), ConnectionError >` | — | — |
| `read_headers` | `() -> Result<Vec<Header>, ConnectionError >` | — | Read a `headers` message and return validated headers using our own types. |

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
*No generated relationships detected.*

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [src_connection](../modules/src_connection.md) | 3 | `network_magic`, `stream` |
