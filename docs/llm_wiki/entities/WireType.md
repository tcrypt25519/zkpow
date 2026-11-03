# WireType

**Location:** `crates/wiretype/src/implementation.rs:4`
**Kind:** Class
**Bases:** —
**Module:** [implementation](../modules/implementation.md)

## Description

Byte view of a run of `Pod + Zeroable` values. `to_bytes` borrows them as bytes, and `parse` casts a byte slice back to `&[T]` without copying, failing on a bad length or alignment. Implemented for `&[T]` and `Vec<T>`.

## Attributes

*No annotated attributes found.*

## Methods

| Method | Signature | Decorators | Description |
|--------|-----------|------------|-------------|
| `to_bytes` | `() -> Self::Bytes` | — | — |
| `as_bytes` | `() -> &[u8]` | — | — |
| `parse` | `(bytes: &[u8]) -> Result<&Self::Inner, PodCastError >` | — | — |

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
*No generated relationships detected.*

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [implementation](../modules/implementation.md) | 3 | — |
