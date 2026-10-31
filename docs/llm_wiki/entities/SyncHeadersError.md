# SyncHeadersError

**Location:** `crates/sync-headers/src/error.rs:65`
**Kind:** Enum
**Bases:** —
**Module:** [error](../modules/error.md)

**Decorators:** `@Error`, `@Debug`

## Description

The outcome of a sync-headers run: either we reached the tip or we hit
an error partway through.

In both cases the caller gets back the last known good tip so that
another attempt can resume from there.

The `E` type parameter is the error type returned by the caller's header
processing callback.

`Connect` and `Peer` retain [ConnectionError](ConnectionError.md) directly as their error source, allowing standard error-chain downcasts to recover the concrete type. The sync wrapper adds peer or progress context without converting protocol failures into generic I/O errors. Exhausted connection retries retain the final attempt's failure. Callback errors keep the caller's `E` type, and every variant retains the latest usable tip.

The public sync boundary moves its owned current tip into a terminal failure. Internal peer attempts use a private error enum without a tip; their mutable reference already preserves accepted progress, so peer failover does not clone, box, or restore a tip snapshot.

## Attributes

| Name | Declared value | Description |
|------|-------|-------------|
| `Connect` | — | — |
| `Peer` | — | — |
| `Callback` | — | — |
| `NoCandidates` | — | — |

## Methods

| Method | Signature | Decorators | Description |
|--------|-----------|------------|-------------|
| `latest_tip` | `() -> &Tip` | — | — |
| `is_recoverable` | `() -> bool` | — | Whether the caller should try another peer and retry. |

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
*No generated relationships detected.*

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [error](../modules/error.md) | 2 | `Callback`, `Connect`, `NoCandidates`, `Peer` |
