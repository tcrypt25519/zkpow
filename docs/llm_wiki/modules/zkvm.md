# zkvm Module

**Path:** `crates/core/src/halt/zkvm.rs`

## Description

Guest build of the invalid-hint halt: re-exports `halt_invalid_hint` and `__invalid_hint_fmt` from `sp1_lib`, which exit with code 3. Compiled only for `target_os = "zkvm"`.

## Imports

| Source | Symbols |
|--------|---------|
| `sp1_lib::__invalid_hint_fmt` | `__invalid_hint_fmt` |
| `sp1_lib::halt_invalid_hint` | `halt_invalid_hint` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 1 | 0 |
