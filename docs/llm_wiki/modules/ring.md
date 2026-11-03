# ring Module

**Path:** `crates/mtp/src/ring.rs`

## Description

_Auto-generated from `crates/mtp/src/ring.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `core::cmp::Ordering` | `Ordering` |
| `core::marker::PhantomData` | `PhantomData` |
| `crate::WINDOW_SIZE` | `WINDOW_SIZE` |
| `crate::errors::InvalidParts` | `InvalidParts` |
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
| [Window](../entities/Window.md) | Type alias | 11 | — | The consensus timestamp window. |
| [WindowValues](../entities/WindowValues.md) | Type alias | 12 | — | — |
| [Cold](../entities/Cold.md) | Class | 16 | — | The ring holds fewer timestamps than its capacity. |
| [Warm](../entities/Warm.md) | Class | 20 | — | Every slot in the ring holds a timestamp. |
| [RingValues](../entities/RingValues.md) | Type alias | 22 | — | — |
| [Ring](../entities/Ring.md) | Class | 27 | `Default` | The last `N` timestamps, oldest overwritten first. `ordered_run` is the length of the |
