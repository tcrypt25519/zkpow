# runtime Module

**Path:** `crates/controller/src/runtime.rs`

## Description

_Auto-generated from `crates/controller/src/runtime.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `crate::NO_HEADERS_REMAINING_PREFIX` | `NO_HEADERS_REMAINING_PREFIX` |
| `crate::protocol::Batch` | `Batch` |
| `crate::protocol::ControllerError` | `ControllerError` |
| `crate::storage::ArtifactStore` | `ArtifactStore` |
| `crate::verify::ProofVerifier` | `ProofVerifier` |
| `std::collections::HashMap` | `HashMap` |
| `std::marker::PhantomData` | `PhantomData` |
| `std::sync::Arc` | `Arc` |
| `std::sync::Mutex` | `Mutex` |
| `std::sync::MutexGuard` | `MutexGuard` |
| `std::time::Duration` | `Duration` |
| `tokio::sync::Notify` | `Notify` |
| `wiretype::WireType` | `WireType` |
| `zkpow_core::PublicValues` | `PublicValues` |
| `zkpow_core::ReturnCode` | `ReturnCode` |
| `zkpow_core::State` | `State` |
| `zkpow_core::VerifierKeyDigest` | `VerifierKeyDigest` |
| `zkpow_params::Consensus` | `Consensus` |
| `zkpow_store::HeaderStore` | `HeaderStore` |
| `zkpow_store::RunRecord` | `RunRecord` |
| `zkpow_types::BlockHash` | `BlockHash` |
| `zkpow_types::RunId` | `RunId` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 6 | 0 |

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [ControllerConfig](../entities/ControllerConfig.md) | Class | 31 | — | — |
| [ControllerRuntime](../entities/ControllerRuntime.md) | Class | 120 | `Clone` | The controller handle. Clones share one core. |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `validate_run_id` | `(run_id: &str) -> Result<RunId, ControllerError >` | — | — |
