# Refactor Plan: zkpow26

**Branch:** `ts-refactor-typed-input`
**Date:** 2026-04-26
**Scope:** `crates/core`, `crates/guest`, `crates/host`

---

## Executive Summary

The codebase is well-structured but carries **dead code**, **near-duplicated logic**, **API inconsistencies**, and **verbose manual implementations** that can be simplified without changing behavior. This plan groups every issue by category, provides exact file/line references, and proposes the minimal surgical fix.

---

## 1. Dead / Unused Code (Remove)

### 1.1 `NextState` struct and impl — `crates/core/src/lib.rs` L898-939
**What:** A `NextState<'a>` struct with a `validate()` method that duplicates timestamp and PoW checks already performed inside `State::next_inner()`.
**Evidence:** `grep NextState` shows zero call sites outside its own definition.
**Fix:** Delete the struct, its `impl`, and the `#[allow(clippy::result_large_err)]` on `apply_headers` (was only there to match `NextState`'s old API).
**Status:** DONE

### 1.2 `cycle_macro` crate — `crates/cycle_macro/`
**What:** A proc-macro crate for cycle tracking that is not referenced in `Cargo.toml` workspace members and not imported by any source file.
**Evidence:** `grep cycle_macro` across the repo returns only its own `Cargo.toml`.
**Fix:** Delete the entire `crates/cycle_macro/` directory.
**Status:** ALREADY GONE

### 1.3 `crates/core/build.rs`
**What:** A build script that only prints `cargo:rerun-if-env-changed=SP1_PROVE` and has a 10-line comment explaining why it intentionally does nothing.
**Fix:** Delete the file.
**Status:** ALREADY GONE

### 1.4 `State::next()` — `crates/core/src/lib.rs`
**What:** A thin wrapper around `next_inner(new_header, hash_header, None, true)`.
**Evidence:** Only used in unit tests.
**Fix:** Make it `#[cfg(test)]`.
**Status:** DONE

### 1.5 Reserved error codes in AGENTS.md
**What:** The markdown documents error codes 1-7. The actual `ValidationErrorCode` enum only defines 4 variants with different values.
**Fix:** Update AGENTS.md to match the actual enum, or expand the enum if the missing codes are needed.
**Status:** PENDING

---

## 2. Code Duplication (Merge / Extract)

### 2.1 `apply_headers` — `crates/core/src/lib.rs`
**What:** Two ~120-line methods shared the same loop structure, chain-work batching, error handling, and retargeting logic. One maintained a sorted sliding window; the other validated prover-supplied hints.
**Fix:** Replaced with a single `apply_headers(headers, median_hints, hash_header)` that always requires hints. Host-side callers compute hints via `median_time_past_hints_for_headers()`.
**Status:** DONE

### 2.2 256-bit comparison logic — `crates/core/src/lib.rs`
**What:** `target_exceeds()` and `hash_meets_target()` both iterate 32 bytes in reverse order comparing magnitudes.
**Fix:** Extract a `u256_le(lhs: &[u8; 32], rhs: &[u8; 32]) -> Ordering` helper.
**Status:** PENDING

### 2.3 SHA-256 block setup — `crates/guest/src/sha256.rs`
**What:** Each SHA-256 function manually assigns `w[0]` through `w[15]` with 16 repetitive `be_u64(...)` calls per block.
**Fix:** Add a `macro_rules! sha256_block` or a `#[inline(always)] fn fill_w_from_bytes(w: &mut [u64; 64], data: &[u8])` helper.
**Status:** PENDING

---

## 3. API / Design Inconsistencies (Fix)

### 3.1 `check_aligned` ignores its `offset` parameter — `crates/core/src/lib.rs`
**What:** The function signature takes `offset: usize` but only uses it for the error message. It checks `bytes.as_ptr()` alignment, not `bytes.as_ptr().add(offset)`.
**Fix:** Either (a) change the check to `(address + offset).is_multiple_of(required)`, or (b) remove the `offset` parameter entirely since all callers pass slices already positioned at the correct offset.
**Status:** PENDING

### 3.2 `ref_from_bytes` / `slice_from_bytes` take unused `offset` — `crates/core/src/lib.rs`
**What:** Both functions accept `offset` but cast `bytes.as_ptr()` directly, never adding the offset. The parameter is only used for error messages.
**Fix:** Remove the `offset` parameter from both functions and from all call sites.
**Status:** PENDING

### 3.3 Two cycle-tracking mechanisms
**What:** `crates/core` uses `cycle_track("label", || { ... })`. `crates/guest` uses `#[sp1_derive::cycle_tracker]`.
**Fix:** Pick one and document the split, or unify via a macro.
**Status:** PENDING

---

## 4. Simplification Opportunities

### 4.1 Newtype boilerplate macro — `crates/core/src/lib.rs`
**What:** 7 `#[repr(transparent)]` newtypes with identical `from_raw`/`as_raw`/`into_raw`/`From` impls (~200 lines).
**Fix:** Define a single `macro_rules! transparent_newtype`.
**Status:** PENDING

### 4.2 `state_to_hash` manual byte extraction — `crates/guest/src/sha256.rs`
**What:** 20 lines of manual byte indexing; could be a 4-line loop.
**Fix:** Use a loop over the 8 state words.
**Status:** PENDING

### 4.3 `run_command_stdout` command string formatting — `crates/host/src/proof_pipeline.rs`
**What:** The formatted command string is built identically in 3 places.
**Fix:** Extract a `fn format_cmd(program: &str, args: &[&str]) -> String` helper.
**Status:** PENDING

### 4.4 `bits_to_target` / `target_to_bits` byte manipulation
**What:** Manual byte arithmetic with repeated bounds checks.
**Fix:** These are consensus-critical and already correct; leave them alone unless property-based tests are added first.
**Status:** WON'T FIX

---

## 5. Potential Bugs / Risks

### 5.1 `MedianTimePastHintsRef::parse` alignment assumption
**What:** `slice_from_bytes` checks if `bytes[4..].as_ptr()` is 4-byte aligned. This is an implicit assumption.
**Fix:** After fixing `check_aligned` (3.1), this becomes safe. Alternatively, use `copy_from_bytes` instead of `slice_from_bytes`.
**Status:** PENDING

### 5.2 `u256_add` carry logic
**What:** The carry loop is slightly subtle.
**Fix:** Add an inline comment explaining carry is always 0 or 1.
**Status:** PENDING

---

## 6. Summary Table

| # | Issue | File | Action | Risk | Status |
|---|-------|------|--------|------|--------|
| 1 | `NextState` unused | `core/src/lib.rs` | Delete | None | DONE |
| 2 | `cycle_macro` crate unused | `crates/cycle_macro/` | Delete dir | None | DONE |
| 3 | `core/build.rs` does nothing | `core/build.rs` | Delete | None | DONE |
| 4 | `State::next()` test-only | `core/src/lib.rs` | `#[cfg(test)]` | None | DONE |
| 5 | Error code docs mismatch | `AGENTS.md` | Sync docs or enum | Low | PENDING |
| 6 | `apply_headers` duplication | `core/src/lib.rs` | Merge to hinted-only | Medium | DONE |
| 7 | 256-bit compare duplication | `core/src/lib.rs` | Extract `u256_le` | Low | PENDING |
| 8 | SHA-256 block setup verbose | `guest/src/sha256.rs` | Macro/helper | Low | PENDING |
| 9 | `check_aligned` ignores offset | `core/src/lib.rs` | Fix or remove offset | Low | PENDING |
| 10 | `ref_from_bytes` unused offset | `core/src/lib.rs` | Remove param | Low | PENDING |
| 11 | Newtype boilerplate | `core/src/lib.rs` | Macro-ize | Low | PENDING |
| 12 | `state_to_hash` verbose | `guest/src/sha256.rs` | Loop | None | PENDING |
| 13 | `run_command_stdout` fmt dup | `host/src/proof_pipeline.rs` | Extract helper | None | PENDING |

---

## Execution Order

1. **Phase 1 — Safe deletions** (items 1-4): Remove dead code. Run `cargo test`.
2. **Phase 2 — API cleanup** (items 9-10): Fix `offset` confusion. Run tests.
3. **Phase 3 — Deduplication** (items 6-7): Merge `apply_headers` paths. Run tests.
4. **Phase 4 — Macros & helpers** (items 8, 11-13): Add macros for boilerplate. Run tests.
5. **Phase 5 — Documentation** (item 5): Update AGENTS.md and any doc comments.
