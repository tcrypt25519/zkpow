# input Module

**Path:** `crates/core/src/input.rs`

## Description

Zero-copy helpers for guest input. `cast_slice` reinterprets raw bytes as `Pod` values and fails on a bad length or alignment; `serialize_inputs` copies values back out to bytes.

## Imports

| Source | Symbols |
|--------|---------|
| `alloc::vec::Vec` | `Vec` |
| `bytemuck::Pod` | `Pod` |
| `bytemuck::PodCastError` | `PodCastError` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 1 | 0 |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `cast_slice` | `(bytes: &[u8]) -> Result<&[B], PodCastError >` | — | Reinterpret raw input bytes as `Pod` values without copying. |
| `serialize_inputs` | `(values: &[B]) -> Vec<u8 >` | — | The inverse of [`cast_slice`]: the values' bytes, copied into a `Vec`. |
