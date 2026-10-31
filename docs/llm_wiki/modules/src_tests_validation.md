# validation Module

**Path:** `crates/sync-headers/src/tests/validation.rs`

## Description

_Auto-generated from `crates/sync-headers/src/tests/validation.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `bitcoin::consensus::Decodable` | `Decodable` |
| `bitcoin::consensus::Encodable` | `Encodable` |
| `bitcoin::consensus::encode` | `encode` |
| `crate::connection::VersionMessage` | `VersionMessage` |
| `crate::connection::create_checksum` | `create_checksum` |
| `crate::msg::MAX_HEADERS_PER_MSG` | `MAX_HEADERS_PER_MSG` |
| `std::io::Read` | `Read` |
| `std::io::Write` | `Write` |
| `std::net::TcpListener` | `TcpListener` |
| `std::thread` | `thread` |
| `super::*` | `*` |
| `zkpow_core::BlockHash` | `BlockHash` |
| `zkpow_core::BlockTimestamp` | `BlockTimestamp` |
| `zkpow_core::BlockWork` | `BlockWork` |
| `zkpow_core::CompactTarget` | `CompactTarget` |
| `zkpow_core::Header` | `Header` |
| `zkpow_core::Target` | `Target` |
| `zkpow_core::calculate_next_work_required` | `calculate_next_work_required` |
| `zkpow_core::check_proof_of_work` | `check_proof_of_work` |
| `zkpow_core::target_from_bits` | `target_from_bits` |
| `zkpow_params::Consensus` | `Consensus` |
| `zkpow_params::ConsensusParams` | `ConsensusParams` |
| `zkpow_params::Mainnet` | `Mainnet` |
| `zkpow_params::WINDOW_MAX_INDEX` | `WINDOW_MAX_INDEX` |
| `zkpow_params::WINDOW_SIZE` | `WINDOW_SIZE` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 3 | 0 |
