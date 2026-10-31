# run_id Module

**Path:** `crates/types/src/run_id.rs`

## Description

_Auto-generated from `crates/types/src/run_id.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `core::fmt` | `fmt` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [RunIdError](../entities/RunIdError.md) | Enum | 5 | `fmt::Display`, `core::error::Error` | An error representing a failed attempt to construct a [`RunId`]. |
| [RunId](../entities/RunId.md) | Class | 33 | `core::ops::Deref`, `AsRef`, `core::borrow::Borrow`, `fmt::Display`, `fmt::Debug`, `TryFrom`, `core::str::FromStr` | A strongly-typed 8-byte ASCII alphanumeric identifier for proving runs. |
