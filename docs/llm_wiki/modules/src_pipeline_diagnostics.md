# diagnostics Module

**Path:** `crates/host/src/pipeline/diagnostics.rs`

## Description

_Auto-generated from `crates/host/src/pipeline/diagnostics.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `crate::memory_monitor` | `memory_monitor` |
| `crate::pipeline::BoxError` | `BoxError` |
| `sp1_sdk::ExecutionReport` | `ExecutionReport` |
| `std::collections::BTreeSet` | `BTreeSet` |
| `std::sync::Mutex` | `Mutex` |
| `std::sync::OnceLock` | `OnceLock` |
| `std::time::Duration` | `Duration` |
| `std::time::Instant` | `Instant` |
| `tracing::field::Field` | `Field` |
| `tracing::field::Visit` | `Visit` |
| `tracing_subscriber::layer::Context` | `Context` |
| `tracing_subscriber::layer::Layer` | `Layer` |
| `tracing_subscriber::registry::LookupSpan` | `LookupSpan` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 3 | 0 |

## Classes

| Class | Kind | Line | Bases / Target | Description |
|-------|------|------|----------------|-------------|
| [PhaseTiming](../entities/PhaseTiming.md) | Class | 58 | — | One labelled phase's summed wall-clock time within a batch. |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `log_execution_report` | `(report: &ExecutionReport, total_proving_time_secs: f64)` | — | Log a guest execution's instruction count, prover gas, estimated shard fill, and hottest |
| `timed_sync` | `(label: &'static str, f: F) -> Result<T, BoxError >` | — | — |
| `timed_async` | *(async)* `(label: &'static str, f: F) -> Result<T, BoxError >` | — | — |
| `record_debug` | `(field: &Field, value: &dyn std::fmt::Debug)` | — | — |
| `on_event` | `(event: &tracing::Event<'_ >, ctx: Context<'_, S >)` | — | — |
