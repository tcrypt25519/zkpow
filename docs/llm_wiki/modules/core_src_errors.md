# errors Module

**Path:** `crates/core/src/errors.rs`

## Description

Error types for the state transition. [`ReportableError`](../entities/ReportableError.md) maps a typed error to the [`ErrorCode`](../entities/ErrorCode.md) the guest commits, falling back to `UNKNOWN_ERROR`. [`ApplyError`](../entities/ApplyError.md) covers consensus rejections of a header (timestamp not above the median, insufficient proof of work, too few headers for a cold start), each with its own code. [`ParseError`](../entities/ParseError.md) covers fixed-width wire decoding. A bad prover hint is not an error here; it halts through [halt](core_src_halt_mod.md).

## Imports

| Source | Symbols |
|--------|---------|
| `bytemuck::PodCastError` | `PodCastError` |
| `crate::BlockHash` | `BlockHash` |
| `crate::BlockTimestamp` | `BlockTimestamp` |
| `crate::CompactTarget` | `CompactTarget` |
| `crate::ErrorCode` | `ErrorCode` |
| `crate::ReturnCode` | `ReturnCode` |

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
| [ReportableError](../entities/ReportableError.md) | Class | 6 | — | An error that can end the guest program. Transforms typed errors into [`ErrorCode`]s. |
| [ParseError](../entities/ParseError.md) | Enum | 19 | `From` | Reading a fixed-width wire encoding out of a byte slice. |
| [ApplyError](../entities/ApplyError.md) | Enum | 46 | `From`, `ReportableError` | Applying headers to a `State` (`crate::State`). |
