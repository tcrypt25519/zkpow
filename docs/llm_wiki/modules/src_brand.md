# brand Module

**Path:** `crates/types/src/brand.rs`

## Description

_Auto-generated from `crates/types/src/brand.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `core::fmt` | `fmt` |
| `crate::bigint::NonMaxU256` | `NonMaxU256` |
| `crate::bigint::u256` | `u256` |
| `wiretype::wiretype` | `wiretype` |

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
| `fmt` | `(f: &mut fmt::Formatter) -> fmt::Result` | — | — |
| `from_limbs` | `(limbs: [u64 ; 4]) -> Self` | — | Build a target from four little-endian 64-bit limbs. |
| `as_inner_u256` | `() -> &u256` | — | — |
| `as_inner_u256_mut` | `() -> &mut u256` | — | — |
| `into_inner_u256` | `() -> u256` | — | — |
| `add` | `(rhs: BlockWork) -> Self::Output` | — | — |
| `add_assign` | `(rhs: BlockWork)` | — | — |
| `mul` | `(rhs: u32) -> Self::Output` | — | — |
| `fmt` | `(f: &mut fmt::Formatter<'_ >) -> fmt::Result` | — | — |
