# PublicValues

**Location:** `crates/core/src/public_values.rs:200`
**Kind:** Class
**Bases:** —
**Module:** [public_values](../modules/public_values.md)

**Decorators:** `@Debug`, `@Clone`, `@PartialEq`, `@Eq`

## Description

Wire layout of the public values committed by the prover.

Layout (185 bytes, little-endian):
```text
 0..152  state                 [u8; 152]  complete State bytes
152..184 verifier_key          [u8; 32]   ([u32; 8] LE words)
184      return_code            u8         (0 = success, >0 = failure)
```

Serialized manually to avoid struct padding; use [`PublicValues::to_bytes`]
and [`PublicValues::parse`] rather than transmuting.

## Attributes

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `state` | `State` | *required* | — |
| `verifier_key` | `VerifierKeyDigest` | *required* | — |
| `return_code` | `ReturnCode` | *required* | — |

## Methods

| Method | Signature | Decorators | Description |
|--------|-----------|------------|-------------|
| `new` | `(state: State, verifier_key: VerifierKeyDigest, return_code: ReturnCode) -> Self` | — | Create public values from their constituent parts. |
| `to_bytes` | `() -> [u8 ; Self::SIZE]` | — | Serialize to exactly [`PublicValues::SIZE`] bytes (no padding). |
| `serialize` | `(state: &State, verifier_key: &VerifierKeyDigest, return_code: ReturnCode) -> [u8 ; Self::SIZE]` | — | [`PublicValues::to_bytes`] from borrowed parts, without building a `PublicValues`. |
| `parse` | `(bytes: &[u8]) -> Result<Self, ParseError >` | — | Deserialize from exactly [`PublicValues::SIZE`] bytes. |
| `is_success` | `() -> bool` | — | Return `true` if the public values report success. |
| `is_failure` | `() -> bool` | — | Return `true` if the public values report a validation failure. |
| `validation_error_code` | `() -> Option<ErrorCode >` | — | Return the validation error code if this is a failure, or `None` on success. |

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
*No generated relationships detected.*

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [public_values](../modules/public_values.md) | 7 | `return_code`, `state`, `verifier_key` |
