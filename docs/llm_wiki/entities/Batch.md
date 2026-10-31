# Batch

**Location:** `crates/controller/src/protocol.rs:13`
**Kind:** Class
**Bases:** —
**Module:** [src_protocol](../modules/src_protocol.md)

**Decorators:** `@Debug`, `@Clone`, `@PartialEq`, `@Eq`

## Description

Everything required to prove the next controller-selected batch.

## Attributes

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `first_new_height` | `u32` | *required* | — |
| `end_height` | `u32` | *required* | — |
| `verifier_key` | `VerifierKeyDigest` | *required* | — |
| `current_state` | `State` | *required* | — |
| `header_bytes` | `Vec<u8 >` | *required* | — |
| `median_hint_bytes` | `Vec<u8 >` | *required* | — |
| `previous_proof` | `Option<String >` | *required* | — |
| `proof_upload` | `String` | *required* | — |

## Methods

| Method | Signature | Decorators | Description |
|--------|-----------|------------|-------------|
| `validate` | `() -> Result<(), String >` | — | Check the payload lengths against the batch's height range. |

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
*No generated relationships detected.*

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [src_protocol](../modules/src_protocol.md) | 1 | `current_state`, `end_height`, `first_new_height`, `header_bytes`, `median_hint_bytes`, `previous_proof`, `proof_upload`, `verifier_key` |
