# Information-First Diff-Oriented Plan

Date: 2026-04-27  
POC: guest in-place state transition + direct commit from input buffer  
TL;DR: Remove `State::clone()` and success-path `to_bytes()` copy by introducing in-place mutation APIs and mutable input parsing, while preserving exact public-value bytes and fallback safety for alignment.

---

## Scope & Invariants

- Keep wire format unchanged:
  - success commit length = `STATE_SIZE` (264)
  - failure commit length = `STATE_SIZE + FAILURE_METADATA_SIZE` (269)
- Keep host-side public value verification unchanged (byte-for-byte).
- Keep `repr(C)`, size/alignment assertions, and LE assumptions intact.
- No changes to hash/consensus logic, only data movement/ownership flow.

---

## File-by-File Diff Plan

## 1) `crates/core/src/lib.rs`

### A. Add in-place API (new primary path)

**Before**
```rust
impl State {
    pub fn apply_headers(
        &self,
        headers: &[NewHeader],
        medians: &[BlockTimestamp],
    ) -> Result<Self, ProofFailure> {
        let mut state = self.clone();
        // mutate state
        Ok(state)
    }
}
```

**After**
```rust
impl State {
    pub fn apply_headers_in_place(
        &mut self,
        headers: &[NewHeader],
        medians: &[BlockTimestamp],
    ) -> Result<(), ProofFailure> {
        // same loop/body as today, mutate `self`
        Ok(())
    }

    pub fn apply_headers(
        &self,
        headers: &[NewHeader],
        medians: &[BlockTimestamp],
    ) -> Result<Self, ProofFailure> {
        let mut state = *self; // or self.clone() if Copy is not available
        state.apply_headers_in_place(headers, medians)?;
        Ok(state)
    }
}
```

**Notes**
- Keep wrapper for compatibility while migrating callers.
- The hot-path clone disappears once guest switches to in-place API.

### B. Optional mutating genesis helper

If currently:
```rust
fn with_genesis_hash(&self, g: BlockHash) -> Self
```
add:
```rust
fn set_genesis_hash(&mut self, g: BlockHash)
```
and retain old helper as wrapper during migration.

---

## 2) `crates/core/src/input.rs`

### A. Add mutable bytes-to-typed helper

**New helper**
```rust
pub(crate) fn mut_from_bytes<T>(bytes: &mut [u8], offset: usize) -> Result<&mut T, ParseError> {
    check_exact_len(bytes, size_of::<T>())?;
    check_aligned::<T>(bytes, offset)?;
    Ok(unsafe { &mut *(bytes.as_mut_ptr() as *mut T) })
}
```

### B. Add mutable parsed input view

**New type**
```rust
pub struct InputMut<'a> {
    pub state: &'a mut State,
    pub recursive_proof: &'a RecursiveProof,
    pub headers: &'a [NewHeader],
}
```

**New parser**
- Parse with `split_at_mut` to isolate state bytes from rest of input.
- Build `&mut State` from state segment.
- Build immutable refs for proof/headers from disjoint remainder segment.
- Keep existing `InputRef::parse` untouched for non-mutating callers.

### C. Alignment fallback helper (for guest use)

Add utility that returns:
- fast path: aligned mutable state ref in original input buffer
- fallback path: owned aligned `State` temp + sync-back method

Example shape:
```rust
enum StateAccess<'a> {
    InPlace(&'a mut State),
    Fallback { temp: State, dst: &'a mut [u8; STATE_SIZE] },
}
```

---

## 3) `crates/guest/src/main.rs`

### A. Switch guest pipeline to mutable parse

**Before**
```rust
let input_bytes = sp1_zkvm::io::read_vec();
let input = parse_input(&input_bytes);
let mut state = input.state.with_genesis_hash(...);
let out = state.apply_headers(...)?;
let state_bytes = out.to_bytes();
commit_slice(&state_bytes);
```

**After (target fast path)**
```rust
let mut input_bytes = sp1_zkvm::io::read_vec();
let input = parse_input_mut(&mut input_bytes)?;
input.state.set_genesis_hash(...); // or equivalent
input.state.apply_headers_in_place(input.headers, medians)?;
sp1_zkvm::io::commit_slice(&input_bytes[..STATE_SIZE]);
```

### B. Preserve error path format

- Keep existing failure metadata append flow.
- If failure occurs and state is in fallback temp, serialize/copy once to maintain canonical bytes.
- Commit exactly `STATE_SIZE + FAILURE_METADATA_SIZE`.

### C. Explicit branch for alignment fallback

Pseudocode:
```rust
match parse_input_mut_or_fallback(&mut input_bytes) {
    InPlace(view) => { /* mutate in place, direct commit */ }
    Fallback(view) => {
        /* mutate temp, copy-back once, then commit */
    }
}
```

---

## 4) `crates/host/src/proof_pipeline.rs` (likely no logic changes)

- No intended functional changes.
- If compile errors arise due to API shifts, adapt call sites minimally.
- Keep `verify_public_values` behavior as regression oracle.

---

## 5) Tests

### A. Existing tests (must remain green)

- Core tests validating header application and byte layout.
- Host verification path that compares committed bytes vs simulated expected bytes.
- Error-path tests in `test_errors` binary.

### B. New tests to add

1. **In-place parity**
   - Arrange identical input state/headers.
   - Compare result of old wrapper `apply_headers` vs new `apply_headers_in_place`.
   - Assert all fields equal.

2. **Commit-byte parity**
   - For known vector, assert success public values are byte-identical to pre-refactor output.

3. **Forced unaligned fallback**
   - Construct intentionally unaligned buffer for state segment.
   - Ensure parser selects fallback path.
   - Assert output bytes identical to aligned path.

4. **Error-path length + metadata**
   - Assert exact 269-byte output and unchanged metadata encoding.

---

## Suggested Edit Sequence (minimal conflict order)

1. Add `apply_headers_in_place` in `core/lib.rs` + keep wrapper.
2. Add `mut_from_bytes` + `InputMut` parser in `core/input.rs`.
3. Update guest `main.rs` to use mutable parser + in-place apply.
4. Add fallback path for unaligned state.
5. Run tests and byte-parity checks.
6. Remove no-longer-used clone/to_bytes calls in guest success path.

---

## Concrete Search/Replace Checklist

- Find `apply_headers(` call sites and migrate guest first.
- Find `state.to_bytes()` in guest success path and replace with direct `commit_slice(&input_bytes[..STATE_SIZE])`.
- Keep failure-path `state + metadata` emission unchanged.
- Confirm no new `Vec` allocations introduced in guest hot loop.

---

## Validation Commands

```bash
cargo build --release > /dev/null 2>&1 && echo OK || echo FAIL
cargo test -p core > /dev/null 2>&1 && echo OK || echo FAIL
cargo run --release --bin test_errors > /dev/null 2>&1 && echo OK || echo FAIL
cargo run --release --bin inspect_proof -- proof_height_0_to_99.bin > /dev/null 2>&1 && echo OK || echo FAIL
```

If repo uses workspace-wide checks:
```bash
cargo test > /dev/null 2>&1 && echo OK || echo FAIL
cargo clippy --all-targets -- -D warnings > /dev/null 2>&1 && echo OK || echo FAIL
```

---

## Rollback Plan

- Keep old `apply_headers` wrapper until all guest/host callsites and tests are migrated.
- Guard mutable parser usage behind local branch-level feature flag if needed.
- If any byte-parity regression appears, revert guest commit path to `to_bytes()` first, keep in-place core API (smaller rollback).

---

## Done Criteria

- Guest happy path:
  - zero-copy parse view into input buffer
  - in-place state mutation
  - direct commit from input buffer
- No UB on alignment edge cases (fallback verified).
- All existing tests pass; new parity/alignment tests pass.
- Public-value bytes match prior behavior exactly.