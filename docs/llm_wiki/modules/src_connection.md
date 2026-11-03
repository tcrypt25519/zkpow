# connection Module

**Path:** `crates/sync-headers/src/connection.rs`

## Description

Frames Bitcoin P2P messages, completes the version handshake, and requests header batches. All fallible connection operations return [ConnectionError](../entities/ConnectionError.md), preserving transport and codec sources alongside structured protocol failures.

## Imports

| Source | Symbols |
|--------|---------|
| `bitcoin::block::Header` | `Header` |
| `bitcoin::consensus::Decodable` | `Decodable` |
| `bitcoin::consensus::Encodable` | `Encodable` |
| `bitcoin::consensus::encode` | `encode` |
| `bitcoin::hashes::Hash` | `Hash` |
| `crate::error::ConnectionError` | `ConnectionError` |
| `crate::msg` | `msg` |
| `crate::types::Header` | `Header` |
| `crate::types::Network` | `Network` |
| `std::io::Read` | `Read` |
| `std::io::Write` | `Write` |
| `std::net::SocketAddr` | `SocketAddr` |
| `std::net::TcpStream` | `TcpStream` |
| `strum_macros::EnumCount` | `EnumCount` |
| `strum_macros::EnumIter` | `EnumIter` |
| `strum_macros::EnumString` | `EnumString` |
| `zkpow_types::BlockHash` | `BlockHash` |
| `zkpow_types::BlockTimestamp` | `BlockTimestamp` |
| `zkpow_types::CompactTarget` | `CompactTarget` |
| `zkpow_types::Header` | `Header` |
| `zkpow_types::MerkleRoot` | `MerkleRoot` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 3 | 0 |

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [Command](../entities/Command.md) | Enum | 26 | — | A Bitcoin P2P network message command. |
| [VersionMessage](../entities/VersionMessage.md) | Class | 96 | — | Simplified version message structure. |
| [PeerConnection](../entities/PeerConnection.md) | Class | 168 | — | A P2P connection to a Bitcoin peer. |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `create_checksum` | `(data: &[u8]) -> [u8 ; 4]` | — | First four bytes of SHA-256d: the P2P message checksum. |