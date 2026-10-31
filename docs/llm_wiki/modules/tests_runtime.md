# runtime Module

**Path:** `crates/controller/src/tests/runtime.rs`

## Description

_Auto-generated from `crates/controller/src/tests/runtime.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `crate::protocol::Batch` | `Batch` |
| `crate::runtime::ControllerConfig` | `ControllerConfig` |
| `crate::runtime::ControllerRuntime` | `ControllerRuntime` |
| `crate::storage::ArtifactStore` | `ArtifactStore` |
| `crate::storage::LocalArtifactStore` | `LocalArtifactStore` |
| `crate::verify::ProofVerifier` | `ProofVerifier` |
| `std::sync::Arc` | `Arc` |
| `std::sync::Mutex` | `Mutex` |
| `std::sync::atomic::AtomicBool` | `AtomicBool` |
| `std::sync::atomic::AtomicU64` | `AtomicU64` |
| `std::sync::atomic::Ordering` | `Ordering` |
| `std::time::Duration` | `Duration` |
| `super::*` | `*` |
| `zkpow_core::PublicValues` | `PublicValues` |
| `zkpow_core::ReturnCode` | `ReturnCode` |
| `zkpow_core::VerifierKeyDigest` | `VerifierKeyDigest` |
| `zkpow_store::HeaderStore` | `HeaderStore` |

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
| `put_target` | `(run_id: &str, object: &str) -> Result<String, crate::BoxError >` | — | — |
| `get_target` | `(run_id: &str, object: &str) -> Result<String, crate::BoxError >` | — | — |
| `fetch` | `(run_id: &str, object: &str) -> Result<Vec<u8>, crate::BoxError >` | — | — |
| `verifier_key_digest` | `() -> VerifierKeyDigest` | — | — |
| `verify_compressed` | `(proof: &[u8], expected: &[u8]) -> Result<(), String >` | — | — |
| `drop` | `()` | — | — |
