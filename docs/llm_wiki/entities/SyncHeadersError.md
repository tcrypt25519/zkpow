# SyncHeadersError

**Location:** `crates/sync-headers/src/error.rs:18`
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
