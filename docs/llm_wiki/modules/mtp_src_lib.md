# lib Module

**Path:** `crates/mtp/src/lib.rs`

## Description

_Auto-generated from `crates/mtp/src/lib.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `errors::InsufficientColdHeaders` | `InsufficientColdHeaders` |
| `ring::Ring` | `Ring` |
| `ring::RingValues` | `RingValues` |
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
