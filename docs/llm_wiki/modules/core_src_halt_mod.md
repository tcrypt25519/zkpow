# mod Module

**Path:** `crates/core/src/halt/mod.rs`

## Description

Halting on a bad prover hint. Inside the zkVM this exits with code 3, distinct from an ordinary panic (code 1), so a prover cannot pass off a wrong hint as a crashing program. Selects the [zkvm](zkvm.md) or [platform](platform.md) backend by target and exports the `invalid_hint!`, `assert_hint!`, `assert_hint_eq!`, and `assert_hint_ne!` macros, which take `panic!`-style format arguments.

## Imports

| Source | Symbols |
|--------|---------|
| `platform::__invalid_hint_fmt` | `__invalid_hint_fmt` |
| `platform::halt_invalid_hint` | `halt_invalid_hint` |
| `zkvm::__invalid_hint_fmt` | `__invalid_hint_fmt` |
| `zkvm::halt_invalid_hint` | `halt_invalid_hint` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 2 | 2 |
