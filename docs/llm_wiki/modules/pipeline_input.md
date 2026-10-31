# input Module

**Path:** `crates/host/src/pipeline/input.rs`

## Description

_Auto-generated from `crates/host/src/pipeline/input.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `crate::pipeline::BoxError` | `BoxError` |
| `crate::pipeline::ProofGenerationConfig` | `ProofGenerationConfig` |
| `crate::pipeline::ProverBackend` | `ProverBackend` |
| `crate::util` | `util` |
| `crate::util::VerifierKeyDigest` | `VerifierKeyDigest` |
| `sp1_sdk::SP1Proof` | `SP1Proof` |
| `sp1_sdk::prelude::*` | `*` |
| `std::path::PathBuf` | `PathBuf` |
| `zkpow_controller::ENV_ZKPOW_DB_PATH` | `ENV_ZKPOW_DB_PATH` |
| `zkpow_params::Consensus` | `Consensus` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 3 | 0 |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `config_from_env` | `() -> Result<ProofGenerationConfig, BoxError >` | — | Read the session config from `ZKPOW_*` variables. Batch size defaults to one difficulty epoch. |
| `proof_store_dir_from_env` | `() -> PathBuf` | — | — |
| `build_stdin` | `(verifier_key: VerifierKeyDigest, state: &util::State, header_bytes: Vec<u8 >, median_hint_bytes: Vec<u8 >, previous_proof: Option<(SP1ProofWithPublicValues, &sp1_prover::SP1VerifyingKey) >) -> Result<SP1Stdin, BoxError >` | — | Assemble the guest's stdin. Header and median-hint bytes come straight |
