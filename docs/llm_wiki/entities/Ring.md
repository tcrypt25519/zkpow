# Ring

**Location:** `crates/mtp/src/ring.rs:27`
**Kind:** Class
**Bases:** `Default`
**Module:** [ring](../modules/ring.md)

**Decorators:** `@Debug`, `@Clone`, `@Copy`, `@PartialEq`, `@Eq`

## Description

The last `N` timestamps, oldest overwritten first. `ordered_run` is the length of the
nondecreasing run ending at the newest value, capped at `N`.

## Attributes

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `values` | `RingValues<N >` | *required* | — |
| `next` | `usize` | *required* | — |
| `len` | `usize` | *required* | — |
| `ordered_run` | `usize` | *required* | — |
| `last_value` | `BlockTimestamp` | *required* | — |
| `fill` | `PhantomData<F >` | *required* | — |

## Methods

| Method | Signature | Decorators | Description |
|--------|-----------|------------|-------------|
| `new_values` | `() -> RingValues<N >` | — | — |
| `len` | `() -> usize` | — | The number of timestamps currently held. |
| `is_empty` | `() -> bool` | — | Whether the ring contains no timestamps. |
| `parts` | `() -> (&RingValues<N>, usize, usize)` | — | Returns the backing slots, next write position, and number of values held. |
| `values` | `() -> RingValues<N >` | — | — |
| `push` | `(value: BlockTimestamp)` | — | Inserts a timestamp. |
| `new` | `() -> Self` | — | Creates an empty ring. |
| `from_parts` | `(values: RingValues<N >, len: usize) -> Result<Self, InvalidParts >` | — | Rebuilds a cold ring from its first `len` slots in chronological order. |
| `median` | `() -> Option<BlockTimestamp >` | — | The upper median, or `None` while the ring is empty or holds a decrease. |
| `median_by_sort` | `() -> Option<BlockTimestamp >` | — | The upper median by sorting a copy of the values held, or `None` while empty. |
| `is_value_median` | `(claimed_median: BlockTimestamp) -> Result<(), NotMedian >` | — | Checks a claimed median by rank. |
| `warm` | `() -> Result<Ring<N, Warm>, InvalidParts >` | — | Converts to a warm ring once every slot holds a timestamp. |
| `default` | `() -> Self` | — | — |
| `from_parts` | `(values: RingValues<N >, next: usize, len: usize) -> Result<Self, InvalidParts >` | — | Rebuilds a warm ring from its slots and next write position, which is also the oldest slot. |
| `push` | `(value: BlockTimestamp)` | — | Inserts a timestamp, overwriting the oldest slot. |
| `newest` | `() -> BlockTimestamp` | — | The most recently pushed timestamp. |
| `median` | `() -> Option<BlockTimestamp >` | — | The upper median in constant time, or `None` while the window holds a decrease. |
| `is_value_median` | `(claimed_median: BlockTimestamp) -> Result<(), NotMedian >` | — | Checks a claimed median by rank. |

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
```mermaid
flowchart LR
    n0["Ring (crates/mtp/src/ring.rs)"]
    n1["Default"]
    n2["Ring.from_parts (crates/mtp/src/ring.rs)"]
    n3["Ring.new_values (crates/mtp/src/ring.rs)"]
    n4["Ring.parts (crates/mtp/src/ring.rs)"]
    n5["Ring.values (crates/mtp/src/ring.rs)"]
    n6["Ring.warm (crates/mtp/src/ring.rs)"]
    n0 --> n1
    n2 --> n0
    n3 --> n0
    n4 --> n0
    n5 --> n0
    n6 --> n0
    click n0 "../modules/ring.md"
    click n2 "../modules/ring.md"
    click n3 "../modules/ring.md"
    click n4 "../modules/ring.md"
    click n5 "../modules/ring.md"
    click n6 "../modules/ring.md"
```

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [ring](../modules/ring.md) | 18 | `fill`, `last_value`, `len`, `next`, `ordered_run`, `values` |

### Structure

| Kind | Entity | Module |
|---|---|---|
| Base | `Default` | — |

### References

| Reference | Kind | Source | Call sites |
|---|---|---|---:|
| `Ring.from_parts` | type_reference | [ring](../modules/ring.md) | — |
| `Ring.new_values` | type_reference | [ring](../modules/ring.md) | — |
| `Ring.parts` | type_reference | [ring](../modules/ring.md) | — |
| `Ring.values` | type_reference | [ring](../modules/ring.md) | — |
| `Ring.warm` | type_reference | [ring](../modules/ring.md) | — |
