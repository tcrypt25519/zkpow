# connection Module

**Path:** `crates/sync-headers/src/connection.rs`

## Description

_Auto-generated from `crates/sync-headers/src/connection.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `bitcoin::block::Header` | `Header` |
| `bitcoin::consensus::Decodable` | `Decodable` |
| `bitcoin::consensus::Encodable` | `Encodable` |
| `bitcoin::consensus::encode` | `encode` |
| `bitcoin::hashes::Hash` | `Hash` |
| `crate::msg` | `msg` |
| `crate::types::Header` | `Header` |
| `crate::types::Network` | `Network` |
| `std::io::Read` | `Read` |
| `std::io::Write` | `Write` |
| `std::net::SocketAddr` | `SocketAddr` |
| `std::net::TcpStream` | `TcpStream` |
| `zkpow_core::BlockHash` | `BlockHash` |
| `zkpow_core::BlockTimestamp` | `BlockTimestamp` |
| `zkpow_core::CompactTarget` | `CompactTarget` |
| `zkpow_core::Header` | `Header` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 2 | 0 |

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [Command](../entities/Command.md) | Enum | 22 | — | A Bitcoin P2P network message command. |
| [VersionMessage](../entities/VersionMessage.md) | Class | 88 | `Decodable` | Simplified version message structure. |
| [PeerConnection](../entities/PeerConnection.md) | Class | 150 | — | A P2P connection to a Bitcoin peer. |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `create_checksum` | `(data: &[u8]) -> [u8 ; 4]` | — | First four bytes of SHA-256d: the P2P message checksum. |
