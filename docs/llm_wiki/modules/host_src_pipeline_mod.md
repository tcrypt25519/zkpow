# mod Module

**Path:** `crates/host/src/pipeline/mod.rs`

## Description

_Auto-generated from `crates/host/src/pipeline/mod.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `crate::memory_monitor` | `memory_monitor` |
| `diagnostics::clear_phase_timings` | `clear_phase_timings` |
| `diagnostics::clear_shard_areas` | `clear_shard_areas` |
| `diagnostics::collected_phase_timings` | `collected_phase_timings` |
| `diagnostics::log_execution_report` | `log_execution_report` |
| `diagnostics::log_shard_areas` | `log_shard_areas` |
| `diagnostics::timed_sync` | `timed_sync` |
| `execution::execute_batch` | `execute_batch` |
| `input::build_stdin` | `build_stdin` |
| `input::config_from_env` | `config_from_env` |
| `proof::PreparedProver` | `PreparedProver` |
| `proof::compressed::generate_compressed_proof` | `generate_compressed_proof` |
| `proof::get_prepared_prover` | `get_prepared_prover` |
| `sp1_sdk::ProverClient` | `ProverClient` |
| `sp1_sdk::SP1ProofWithPublicValues` | `SP1ProofWithPublicValues` |
| `sp1_sdk::prelude::*` | `*` |
| `std::time::Instant` | `Instant` |
| `wiretype::WireType` | `WireType` |
| `zkpow_controller::NO_HEADERS_REMAINING_PREFIX` | `NO_HEADERS_REMAINING_PREFIX` |
| `zkpow_controller::protocol::Batch` | `Batch` |
| `zkpow_controller::runtime::ControllerConfig` | `ControllerConfig` |
| `zkpow_controller::runtime::ControllerRuntime` | `ControllerRuntime` |
| `zkpow_controller::storage::ArtifactStore` | `ArtifactStore` |
| `zkpow_controller::storage::LocalArtifactStore` | `LocalArtifactStore` |
| `zkpow_controller::verify::ProofVerifier` | `ProofVerifier` |
| `zkpow_params::Consensus` | `Consensus` |
| `zkpow_store::HeaderStore` | `HeaderStore` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 9 | 4 |

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [BoxError](../entities/host_src_pipeline_mod_BoxError.md) | Type alias | 42 | — | — |
| [ProverBackend](../entities/ProverBackend.md) | Enum | 48 | — | Which SP1 prover the host uses. `Mock` executes without real proving. |
| [ProofGenerationConfig](../entities/ProofGenerationConfig.md) | Class | 55 | — | One proving session's settings, read from `ZKPOW_*` variables by [`input::config_from_env`]. |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `run_batch_session` | *(async)* `() -> Result<u32, BoxError >` | — | Prove controller-selected batches until `ZKPOW_BATCH_COUNT` is reached or the header database |
| `run_batch_session_with_store` | *(async)* `(db: HeaderStore, config: ProofGenerationConfig) -> Result<u32, BoxError >` | — | Prove batches using the provided `store` and `config`. |
| `run_execution_session` | *(async)* `(db: &HeaderStore, config: ProofGenerationConfig) -> Result<u32, BoxError >` | — | — |
