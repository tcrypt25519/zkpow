# VersionMessage

**Location:** `crates/sync-headers/src/connection.rs:96`
**Kind:** Class
**Bases:** —
**Module:** [src_connection](../modules/src_connection.md)

**Decorators:** `@Debug`

## Description

Simplified version message structure.

The crate-local decoder returns [ConnectionError](ConnectionError.md), preserving Bitcoin decoding failures and reporting an oversized user agent with its actual length and limit before allocation. It accepts a missing optional relay byte and rejects trailing bytes after the version payload. It uses an inherent decoder rather than Bitcoin's `Decodable` trait so protocol checks can return the connection enum directly.

## Attributes

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `version` | `i32` | *required* | — |
| `services` | `u64` | *required* | — |
| `timestamp` | `i64` | *required* | — |
| `receiver_services` | `u64` | *required* | — |
| `sender_services` | `u64` | *required* | — |
| `nonce` | `u64` | *required* | — |
| `user_agent` | `String` | *required* | — |
| `starting_height` | `i32` | *required* | — |

## Methods

*No public methods. Inherits from base classes.*

## Relationships

<!-- Auto-generated relationship summary. Do not edit by hand. -->
*No generated relationships detected.*

### Summary

| Module | Methods | Attributes |
|---|---:|---|
| [src_connection](../modules/src_connection.md) | 0 | `nonce`, `receiver_services`, `sender_services`, `services`, `starting_height`, `timestamp`, `user_agent`, `version` |
