# lib Module

**Path:** `crates/wiretype_macro/src/lib.rs`

## Description

_Auto-generated from `crates/wiretype_macro/src/lib.rs`._

## Imports

| Source | Symbols |
|--------|---------|
| `proc_macro::TokenStream` | `TokenStream` |
| `quote::quote` | `quote` |
| `syn::DeriveInput` | `DeriveInput` |
| `syn::parse_macro_input` | `parse_macro_input` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 3 | 1 |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `wiretype` | `(attr: TokenStream, item: TokenStream) -> TokenStream` | — | Make a struct serializable across the CPU and RISC-V boundary. Derives the bytemuck |
