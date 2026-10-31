# lib Module

**Path:** `crates/sha256_const/src/lib.rs`

## Description

_Auto-generated from `crates/sha256_const/src/lib.rs`._

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `hash` | `(message: &[u8]) -> [u8 ; 32]` | — | — |
| `double` | `(message: &[u8]) -> [u8 ; 32]` | — | SHA-256d: `sha256(sha256(message))`, the Bitcoin block and merkle hash. |
