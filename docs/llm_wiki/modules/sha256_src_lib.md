# lib Module

**Path:** `crates/sha256/src/lib.rs`

## Description

SHA-256 for host and guest. Each function is unrolled for one fixed input size with the padding and block layout hardcoded: `of_header` and `of_header_words` for an 80-byte header, `of_digest` and `of_digest_words` for the second pass of SHA-256d, and `of_public_values` for the 185-byte public values. State is eight `u64` words, each holding one 32-bit word, to match SP1's `syscall_sha256_extend` and `syscall_sha256_compress`; off the zkVM the `sha2` compression function is used. `hash_to_state` and `state_to_hash` convert between that form and bytes.

## Imports

| Source | Symbols |
|--------|---------|
| `sp1_zkvm::syscalls::syscall_sha256_compress` | `syscall_sha256_compress` |
| `sp1_zkvm::syscalls::syscall_sha256_extend` | `syscall_sha256_extend` |

## Local dependency map

<!-- Auto-generated local dependency summary. Do not edit by hand. -->
*No internal module dependencies detected.*

### External packages

| Language | Used packages | Undeclared packages |
|---|---:|---:|
| rust | 1 | 0 |

## Functions

| Function | Signature | Decorators | Description |
|----------|-----------|------------|-------------|
| `state_to_hash` | `(state: &[u64 ; 8]) -> [u8 ; 32]` | — | Extract a 32-byte hash from the SHA-256 state. |
| `hash_to_state` | `(hash: &[u8 ; 32]) -> [u64 ; 8]` | — | Pack a 32-byte hash into 8 SHA state words. |
| `of_digest_words` | `(prev_state: &[u64 ; 8]) -> [u64 ; 8]` | — | Second pass of a SHA-256d: the first pass leaves its digest in `prev_state` |
| `of_digest` | `(prev_state: &[u64 ; 8]) -> [u8 ; 32]` | — | [`of_digest_words`] unpacked to bytes. |
| `of_public_values` | `(state_bytes: &[u8], verifier_key: &[u8], return_code: u8) -> [u8 ; 32]` | — | Compute SHA-256 of the complete 185 byte public values. |
| `of_header_words` | `(version: u32, prev_blockhash: &[u64 ; 8], merkle_root: &[u8 ; 32], timestamp: u32, compact_target: u32, nonce: u32) -> [u64 ; 8]` | — | Compute SHA256d over a Bitcoin header, returning the 8 SHA state words directly. |
| `of_header` | `(version: u32, prev_blockhash: &[u8 ; 32], merkle_root: &[u8 ; 32], timestamp: u32, compact_target: u32, nonce: u32) -> [u8 ; 32]` | — | Compute SHA256d over a Bitcoin header represented by disjoint fields. |
