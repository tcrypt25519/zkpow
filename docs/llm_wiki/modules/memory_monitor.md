# memory_monitor Module

**Path:** `crates/host/src/memory_monitor.rs`

## Description

_Auto-generated from `crates/host/src/memory_monitor.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `crate::pipeline::input::ENV_ZKPOW_MEMORY_LOGGING` | `ENV_ZKPOW_MEMORY_LOGGING` |
| `std::fs` | `fs` |
| `std::process::Command` | `Command` |
| `std::process::self` | `self` |
| `std::str::from_utf8` | `from_utf8` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [StageSample](../entities/StageSample.md) | Class | 14 | — | — |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `rss_kb` | `() -> Option<u64 >` | — | Resident set size of this process in KiB, on Linux and macOS. |
| `sample` | `() -> StageSample` | — | Sample RSS. |
| `log_point` | `(label: &'static str, message: &'static str) -> StageSample` | — | Sample and log a memory data point. When memory logging is disabled this |
| `log_delta` | `(label: &'static str, started: StageSample, finished: StageSample, elapsed: std::time::Duration, message: &'static str)` | — | — |
| `logging_enabled` | `() -> bool` | — | — |
