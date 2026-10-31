# platform Module

**Path:** `crates/core/src/halt/platform.rs`

## Description

Host build of the invalid-hint halt. Off the zkVM there is no SP1 syscall, so `halt_invalid_hint` panics with `halt: invalid hint (3)`, and `__invalid_hint_fmt` prints the diagnostic to stderr first. Compiled only when `target_os` is not `zkvm`.

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `halt_invalid_hint` | `() -> !` | — | — |
| `__invalid_hint_fmt` | `(args: core::fmt::Arguments<'_ >) -> !` | — | — |
