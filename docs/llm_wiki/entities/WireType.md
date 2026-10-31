# WireType

**Location:** `crates/wiretype/src/lib.rs:4`
**Kind:** Class
**Bases:** —
**Module:** [wiretype_src_lib](../modules/wiretype_src_lib.md)

## Description

Byte view of a run of `Pod + Zeroable` values. `to_bytes` borrows them as bytes, and `parse` casts a byte slice back to `&[T]` without copying, failing on a bad length or alignment. Implemented for `&[T]` and `Vec<T>`.

## Attributes

*No annotated attributes found.*

## Methods

| Method | Signature | Decorators | Description |
|--------|-----------|------------|-------------|
| `to_bytes` | `() -> &[u8]` | — | — |
| `parse` | `(bytes: &[u8]) -> Result<&[T], PodCastError >` | — | — |

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
*No generated relationships detected.*

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [wiretype_src_lib](../modules/wiretype_src_lib.md) | 2 | — |
