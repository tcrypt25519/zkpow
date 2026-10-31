# observability Module

**Path:** `crates/host/src/observability.rs`

## Description

_Auto-generated from `crates/host/src/observability.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `crate::pipeline::diagnostics::ShardAreas` | `ShardAreas` |
| `crate::pipeline::input::ENV_ZKPOW_SHOW_COMPRESSED_PROOF_SPANS` | `ENV_ZKPOW_SHOW_COMPRESSED_PROOF_SPANS` |
| `std::sync::Once` | `Once` |
| `tracing_appender::non_blocking::WorkerGuard` | `WorkerGuard` |
| `tracing_subscriber::EnvFilter` | `EnvFilter` |
| `tracing_subscriber::fmt::format::FmtSpan` | `FmtSpan` |
| `tracing_subscriber::prelude::*` | `*` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 2 | 0 |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `init` | `()` | — | Initialize tracing. |
