# bigint Module

**Path:** `crates/types/src/bigint.rs`

## Description

_Auto-generated from `crates/types/src/bigint.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `core::ops::Deref` | `Deref` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [u256](../entities/u256.md) | Class | 10 | `core::ops::Add`, `core::ops::MulAssign`, `core::ops::Mul`, `PartialOrd`, `Ord`, `TryFrom` | A 256-bit unsigned integer stored as four little-endian u64 limbs. |
| [NonMaxU256](../entities/NonMaxU256.md) | Class | 166 | `Deref` | A [`u256`] excluding the all-ones maximum. |
