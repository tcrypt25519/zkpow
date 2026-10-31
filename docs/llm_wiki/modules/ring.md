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

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [Window](../entities/Window.md) | Type alias | 9 | — | The consensus timestamp window. |
| [Cold](../entities/Cold.md) | Class | 13 | — | The ring holds fewer timestamps than its capacity. |
| [Warm](../entities/Warm.md) | Class | 17 | — | Every slot in the ring holds a timestamp. |
| [Ring](../entities/Ring.md) | Class | 22 | `Default` | The last `N` timestamps, oldest overwritten first. `ordered_run` is the length of the |
