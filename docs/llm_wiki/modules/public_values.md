# public_values Module

**Path:** `crates/core/src/public_values.rs`

## Description

The values the guest commits. [`PublicValues`](../entities/PublicValues.md) is a hand-serialized 185-byte layout: the 152-byte [`State`](../entities/State.md), a 32-byte [`VerifierKeyDigest`](../entities/VerifierKeyDigest.md), and a one-byte return code. [`ReturnCode`](../entities/ReturnCode.md) is `0` for success; [`ErrorCode`](../entities/ErrorCode.md) is the non-zero byte for a failure (2 PoW, 3 timestamp, 5 cold headers, 255 unknown). A code this build does not name is still a valid failure, because public values may come from other guest builds.

## Imports

| Source | Symbols |
|--------|---------|
| `core::fmt::Debug` | `Debug` |
| `core::fmt::Display` | `Display` |
| `core::num::NonZeroU8` | `NonZeroU8` |
| `crate::ParseError` | `ParseError` |
| `crate::State` | `State` |
| `crate::check_exact_len` | `check_exact_len` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [VerifierKeyDigest](../entities/VerifierKeyDigest.md) | Class | 14 | `From` | Digest of a program verifier key. |
| [ErrorCode](../entities/ErrorCode.md) | Class | 61 | `core::fmt::Display` | The non-zero exit status the guest commits when validation fails. |
| [ReturnCode](../entities/ReturnCode.md) | Class | 119 | `From`, `core::fmt::Display` | A validation result: `None` means success, `Some(ErrorCode)` means failure. |
| [PublicValues](../entities/PublicValues.md) | Class | 200 | — | Wire layout of the public values committed by the prover. |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `from` | `(code: ErrorCode) -> u8` | — | — |
| `from` | `(code: ReturnCode) -> u8` | — | — |
