# ring Module

**Path:** `crates/mtp/src/ring.rs`

## Description

_Auto-generated from `crates/mtp/src/ring.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `core::cmp::Ordering` | `Ordering` |
| `crate::WINDOW_SIZE` | `WINDOW_SIZE` |
| `crate::errors::NotMedian` | `NotMedian` |
| `zkpow_types::BlockTimestamp` | `BlockTimestamp` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 1 | 0 |

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [Window](../entities/Window.md) | Type alias | 8 | — | The consensus timestamp window. |
| [WindowValues](../entities/WindowValues.md) | Type alias | 9 | — | — |
| [RingValues](../entities/RingValues.md) | Type alias | 10 | — | — |
| [Ring](../entities/Ring.md) | Class | 15 | `Default` | The last `N` timestamps, oldest overwritten first. `ordered_run` is the length of the |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `next` | `() -> Option<Self::Item >` | — | — |
