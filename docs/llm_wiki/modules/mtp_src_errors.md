# errors Module

**Path:** `crates/mtp/src/errors.rs`

## Description

_Auto-generated from `crates/mtp/src/errors.rs`._

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
| [NotMedian](../entities/NotMedian.md) | Enum | 5 | `fmt::Display` | Why a claimed median is not the window's median. |
| [InvalidParts](../entities/InvalidParts.md) | Enum | 21 | — | Why persisted ring parts could not be rebuilt in the requested fill state. |
| [InsufficientColdHeaders](../entities/InsufficientColdHeaders.md) | Class | 29 | `fmt::Display` | Why a header slice cannot be split into the cold prefix and warm remainder. |
