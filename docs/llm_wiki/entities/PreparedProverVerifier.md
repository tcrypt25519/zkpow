# PreparedProverVerifier

**Location:** `crates/host/src/pipeline/verify.rs:12`
**Kind:** Class
**Bases:** `ProofVerifier`
**Module:** [pipeline_verify](../modules/pipeline_verify.md)

## Description

_Auto-generated from `PreparedProverVerifier` in `crates/host/src/pipeline/verify.rs`._

## Attributes

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `prepared` | `PreparedProver` | *required* | — |

## Methods

| Method | Signature | Decorators | Description |
|--------|-----------|------------|-------------|
| `verifier_key_digest` | `() -> zkpow_core::VerifierKeyDigest` | — | — |
| `verify_compressed` | `(proof_bytes: &[u8], expected_public_values: &[u8]) -> Result<(), String >` | — | — |

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
```mermaid
flowchart LR
    n0["PreparedProverVerifier (crates/host/src/pipeline/verify.rs)"]
    n1["ProofVerifier (crates/controller/src/verify.rs)"]
    n0 --> n1
    click n0 "../modules/pipeline_verify.md"
    click n1 "../modules/src_verify.md"
```

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [pipeline_verify](../modules/pipeline_verify.md) | 2 | `prepared` |

### Structure

| Kind | Entity | Module |
|---|---|---|
| Base | `ProofVerifier` | [src_verify](../modules/src_verify.md) |
