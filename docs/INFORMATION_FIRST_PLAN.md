# Information-First Design Plan for Header Prover Guest Program

## TL;DR

Target the two real guest-side extra moves:

1. `State::clone()` in `apply_headers`
2. `state.to_bytes()` before commit

Refactor to mutate state in place inside the stdin buffer (when aligned), and commit directly from that buffer on success. Keep a safe fallback path for alignment edge cases.

---

## Current Bottlenecks (confirmed)

- `apply_headers` clones state (`~264B memcpy`) before mutation.
- success path serializes state to bytes (`to_bytes`, another memcpy) before `commit_slice`.
- input parsing is already mostly zero-copy via borrowed refs.
- wire format and host verification are strict byte-level; must remain unchanged.

---

## Proposed Migration (phased, low-risk)

## Phase 1 — Core API: in-place transition (no wire changes)

### Files
- `crates/core/src/lib.rs`

### Changes
- Add in-place transition API:
  - from: `fn apply_headers(&self, ...) -> Result<State, ProofFailure>`
  - to: `fn apply_headers_in_place(&mut self, ...) -> Result<(), ProofFailure>`
- Keep existing `apply_headers` as a compatibility wrapper (clone + call in-place) during migration.
- Change `with_genesis_hash` flow to avoid unnecessary clone where possible (or make a mutating variant).

### Why
- Removes the biggest avoidable copy on hot path while minimizing blast radius.

### Acceptance checks
- Existing core tests pass unchanged.
- Existing host verification still passes (byte-identical output).

---

## Phase 2 — Guest input mutability + alias-safe parsing

### Files
- `crates/core/src/input.rs`
- `crates/guest/src/main.rs`

### Changes
- Introduce mutable parser type (`InputMut`) for guest-side use:
  - `state: &mut State`
  - headers/hints as immutable borrows from non-overlapping regions.
- Parse via `split_at_mut` first, then cast state segment to `&mut State` through checked helper.
- Add `mut_from_bytes` with strict guards:
  - exact length
  - alignment check (`align_of::<State>() == 8` requirement)
  - repr/layout assertions retained

### Why
- Enforces one-owner model in Rust terms and avoids aliasing UB.

### Acceptance checks
- Guest compiles with no extra allocations on happy path.
- Borrow checker enforces non-overlapping mutable/immutable regions.

---

## Phase 3 — Direct commit from input buffer (success path)

### Files
- `crates/guest/src/main.rs`

### Changes
- On success: commit `&input_bytes[..STATE_SIZE]` directly (state mutated in place).
- On failure: keep current explicit error metadata append behavior (acceptable extra copy).

### Why
- Eliminates final state serialization memcpy.

### Acceptance checks
- `verify_public_values` and proof pipeline tests pass byte-for-byte.
- Success output remains exactly 264 bytes; error remains 269 bytes.

---

## Phase 4 — Alignment fallback policy (correctness over ideal path)

### Files
- `crates/core/src/input.rs`
- `crates/guest/src/main.rs`

### Changes
- If state slice alignment is invalid:
  - fallback to aligned temporary `State`, mutate, copy back once to buffer before commit.
- Track fallback hit rate (debug counter/log in test mode only).

### Why
- Prevents UB while preserving near-optimal path for normal aligned input.

### Acceptance checks
- Forced-unaligned test passes with identical public-value digest.
- No unsafe mutable ref creation on unaligned memory.

---

## Risks & Mitigations

- **Alignment UB (high)**  
  Mitigate with hard alignment checks + fallback copy path.
- **Mutable aliasing (medium)**  
  Mitigate with `split_at_mut` region separation and API structuring.
- **repr(C)/padding determinism (high)**  
  Keep static size/layout assertions; ensure all committed bytes are deterministic.
- **Backward compatibility (high)**  
  Keep wire format identical; gate with byte-level host tests and golden proof checks.

---

## Alternatives Considered (and rejected)

1. **Keep clone + optimize elsewhere**  
   Low risk, but leaves the largest avoidable guest copy in place.
2. **`repr(packed)` to dodge alignment**  
   Rejected due to unsafe field access/perf/UB hazards.
3. **Full parse/serialize structs each iteration**  
   Rejected: violates information-first objective with extra movement.

---

## Definition of Done

- Happy-path transition uses:
  - zero-copy parse into mutable state view,
  - in-place mutation,
  - direct commit from same backing bytes.
- Proof/public value bytes are unchanged from host expectation.
- Error path semantics unchanged.
- Existing test suite passes, plus new alignment/alias regression tests.