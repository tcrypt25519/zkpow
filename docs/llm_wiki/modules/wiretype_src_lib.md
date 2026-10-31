# lib Module

**Path:** `crates/wiretype/src/lib.rs`

## Description

Fixed-layout wire encoding. Re-exports the `#[wiretype]` attribute macro from `wiretype-macro` and defines [`WireType`](../entities/WireType.md) for slices and `Vec`s of `Pod` values.

## Imports

| Source | Symbols |
|--------|---------|
| `bytemuck::Pod` | `Pod` |
| `bytemuck::PodCastError` | `PodCastError` |
| `bytemuck::Zeroable` | `Zeroable` |
| `bytemuck::cast_slice` | `cast_slice` |
| `bytemuck::try_cast_slice` | `try_cast_slice` |
| `wiretype_macro::wiretype` | `wiretype` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 2 | 0 |

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [WireType](../entities/WireType.md) | Class | 4 | — | — |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `to_bytes` | `() -> &[u8]` | — | — |
| `parse` | `(bytes: &[u8]) -> Result<&[T], PodCastError >` | — | — |
