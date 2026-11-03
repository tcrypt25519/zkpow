# lib Module

**Path:** `crates/mtp/src/lib.rs`

## Description

_Auto-generated from `crates/mtp/src/lib.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `errors::InsufficientColdHeaders` | `InsufficientColdHeaders` |
| `errors::InvalidParts` | `InvalidParts` |
| `ring::Cold` | `Cold` |
| `ring::Ring` | `Ring` |
| `ring::RingValues` | `RingValues` |
| `ring::Warm` | `Warm` |
| `ring::Window` | `Window` |
| `ring::WindowValues` | `WindowValues` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 2 | 2 |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `split_off_cold_headers` | `(headers: &[T]) -> Result<(&[T], &[T]), InsufficientColdHeaders >` | — | Splits headers into the prefix that fills an empty consensus window and the warm remainder. |
| `timestamp_at` | `(timestamps: &[T ; WINDOW_SIZE], height: u32) -> T` | — | Read the timestamp recorded for `height` in the persisted window. |
| `record_timestamp` | `(timestamps: &mut [T ; WINDOW_SIZE], height: u32, timestamp: T)` | — | Record a timestamp in the window slot for its chain height. |
| `median_timestamp` | `(timestamps: &[T ; WINDOW_SIZE], height: u32, ordered_run: u64) -> T` | — | The upper median of the populated slots, using the ordered run when available. |
| `has_ordered_timestamp_window` | `(ordered_timestamp_run: u64) -> bool` | — | Whether the run of nondecreasing links orders the entire window, pinning the median to a known slot. |
| `advance_ordered_timestamp_run` | `(ordered_timestamp_run: u64, previous_timestamp: u32, timestamp: u32) -> u64` | — | Extend the run of nondecreasing links by one, or reset it to zero on a decrease. |
| `ordered_timestamp_run` | `(timestamps: &[T ; WINDOW_SIZE], height: u32) -> u64` | — | Recount the nondecreasing links by walking the window backwards from the slot of `height`. |
| `ordered_median_timestamp_slot` | `(height: u32) -> usize` | — | The slot holding the median while the window ending at `height` is fully ordered. |
| `requires_median_hint` | `(height: u64, previous_ordered_timestamp_run: u64) -> bool` | — | Whether the block at `height` needs a prover-supplied median: its window is full but not |
