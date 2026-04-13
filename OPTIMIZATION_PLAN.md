# Performance & Security Optimization Plan

## Legend

| Symbol | Meaning |
|--------|---------|
| 🔬 | Needs exploration/validation before implementation |
| 💰 | High-impact, low-effort (do first) |
| 🔧 | Medium effort, moderate impact |
| 🏗️ | High effort, uncertain ROI |
| 🛡️ | Security/stability fix |

---

## 1. SHA-256 Hot Path

### 1.1 Eliminate `try_into().unwrap()` in sha256.rs 💰
**Current:**
```rust
for (j, chunk) in data[0..64].chunks(4).enumerate() {
    w[j] = u32::from_be_bytes(chunk.try_into().unwrap()) as u64;
}
```
**Proposed:** Direct indexing, known at compile time:
```rust
w[0]  = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as u64;
w[1]  = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as u64;
// ... through w[15]
```
**Why:** Eliminates 3 panic points per hash call, removes `chunks()` iterator overhead, gives the compiler exact knowledge of array sizes for better constraint generation.
**Impact:** 3 unwrap → 0 in sha256.rs. Marginal cycle savings, eliminates panic paths.

### 1.2 Inline `state_to_hash` unroll 💰
**Current:** Loop over 8 state elements.
**Proposed:** Unroll 8 assignments with direct indexing.
**Impact:** Eliminates loop overhead. Small but pure win.

### 1.3 Eliminate `header.try_into().unwrap()` in main.rs 💰
**Current:** `double_sha256_80(&header.try_into().unwrap())`
**Problem:** `header` is `&[u8]` (a slice), so `try_into()` produces `[u8; 80]` via copy + runtime length check.
**Proposed:** Parse the header into a `[u8; 80]` once, then pass `&header_array`:
```rust
let header: [u8; 80] = headers_bytes[offset..offset + 80].try_into().unwrap();
let computed_hash = double_sha256_80(&header);
```
Or even better, eliminate the slice intermediate entirely by iterating over 80-byte chunks of `&[u8; 80]`.
**Impact:** Eliminates 2 unwrap points. Removes one copy per header.

### 1.4 Precompute padding constants as array literals 🛡️💰
The padding for 80-byte input is deterministic:
- Block 2 word 4 = `0x80000000`
- Block 2 word 15 = `0x280` (640 bits)
These are already hardcoded constants, so this is already done. No change needed.

---

## 2. Header Parsing

### 2.1 Skip parsing unused fields 💰
**Current:** Parses all 6 fields every iteration:
```rust
let prev_blockhash: [u8; 32] = header[4..36].try_into().unwrap();
let timestamp = u32::from_le_bytes(header[68..72].try_into().unwrap());
let bits = u32::from_le_bytes(header[72..76].try_into().unwrap());
```
**Unused:** `version` (bytes 0-3), `merkle_root` (bytes 36-67), `nonce` (bytes 76-79).
**Impact:** These are simple array slices — the cost is negligible in the zkVM since slicing is free. But removing the dead code improves readability and eliminates potential panic sites.

### 2.2 Replace slice `try_into()` with direct u32 construction 🛡️💰
**Current:** `header[68..72].try_into().unwrap()`
**Proposed:**
```rust
let timestamp = header[68] as u32
    | (header[69] as u32) << 8
    | (header[70] as u32) << 16
    | (header[71] as u32) << 24;
```
**Why:** Eliminates `unwrap()`, gives compiler exact knowledge of bounds, removes runtime slice allocation.
**Impact:** 0 unwrap points. 3 fewer slices per header.

### 2.3 Parse prev_blockhash as direct array copy 🛡️
**Current:** `header[4..36].try_into().unwrap()`
**Proposed:**
```rust
let mut prev_blockhash = [0u8; 32];
prev_blockhash.copy_from_slice(&header[4..36]);
```
**Why:** `copy_from_slice` panics on length mismatch, but since we know the slice is exactly 32 bytes, this is equally safe and avoids `try_into()`.
**Actually better:** Use a typed header struct with `#[repr(C)]` or direct field access.

---

## 3. Error Handling & Stability

### 3.1 Replace all remaining `unwrap()` with `commit_error_and_exit` 🛡️🔧
**Count:** 22 total (19 in main.rs, 3 in sha256.rs)

**Safe (size-asserted, cannot panic with valid inputs):**
- `header[4..36].try_into().unwrap()` — guaranteed by byte count check at line 522
- `header[68..72].try_into().unwrap()` — same
- `header[72..76].try_into().unwrap()` — same
- `header.try_into().unwrap()` for double_sha256_80 — same
- `old_target[0..8].try_into().unwrap()` — `old_target` is `[u8; 32]`, guaranteed
- sha256.rs `chunk.try_into().unwrap()` — guaranteed by `.chunks(4)` on exact-size ranges

**Risky (could panic on malformed input from stdin):**
- `prev_public_values[0..32].try_into().unwrap()` — only checked for length ≥ 237, but slice bounds are safe
- All other PV slice extractions — same

**Verdict:** All 22 unwraps are actually safe given the invariants enforced by the byte count check and the PV length check. However, replacing them with explicit indexing or `.copy_from_slice()` eliminates the `unwrap()` calls entirely and satisfies the "no unwrap" principle.

**Recommended:** Replace all `try_into().unwrap()` with direct construction (as in §2.2). This is purely defensive — these cannot panic with valid inputs, but the code is cleaner without `unwrap()`.

### 3.2 Add validation for bits field range 🛡️💰
**Current:** No validation that `bits` represents a valid target.
**Proposal:** Reject bits values that would produce overflow in `bits_to_target`:
```rust
if exponent < 3 || exponent > 29 {
    commit_error_and_exit(..., STATUS_INVALID_BITS, i as u32);
}
```
**Why:** An exponent < 3 would produce a negative shift. An exponent > 29 would produce a target > 2^256.
**Impact:** Prevents potential undefined behavior (though Rust's shift semantics would wrap, not panic).

### 3.3 Add validation for timestamp range 🛡️
**Current:** No bounds on timestamp values.
**Proposal:** Reject timestamps < genesis timestamp or > reasonable future bound:
```rust
if timestamp < GENESIS_TIMESTAMP || timestamp > MAX_REASONABLE_TIMESTAMP {
    commit_error_and_exit(..., STATUS_INVALID_TIMESTAMP, i as u32);
}
```
**Why:** A timestamp of 0 or 2^32-1 would corrupt the median window and epoch tracking.
**Impact:** Defensive — prevents garbage-in-garbage-out scenarios.

### 3.4 Add error code for invalid bits/timestamp 🛡️
Currently reserved codes 5, 8, 9, 10. Reassign:
- 5 → `STATUS_INVALID_TIMESTAMP`
- 8 → `STATUS_INVALID_BITS`
- 9, 10 → reserved for future use

---

## 4. Median Window Optimization

### 4.1 Eliminate `rebuild_packed` on resume 🔬💰
**Current:** When resuming from a previous proof, the program reads `median_timestamps` from the PV and calls `rebuild_packed()` to reconstruct the sorted indices.
**Observation:** The previous proof's PV already contains timestamps that were committed in height order. The `rebuild_packed` insertion sort costs ~55 comparisons.
**Proposal:** Commit `median_packed` as part of the PV (adds 8 bytes for the u64). On resume, read packed directly instead of rebuilding.
**Host verification:** The host already reconstructs packed from timestamps — add a check that the PV's packed matches the reconstructed value.
**Trade-off:** +8 bytes per PV, -55 comparisons per resume. Likely not worth it — 55 comparisons is negligible compared to SHA-256 (~15K cycles per header × 100 headers = 1.5M cycles).
**Verdict:** Skip. Not worth the PV bloat.

### 4.2 Verify `median_head` derivation correctness 🔬🛡️
**Current:** `prev_median_head = (prev_num_headers % 11) as u8` when window is full.
**Verification needed:** Trace through the circular buffer state machine to confirm this formula is correct for all N ≥ 11.
**Manual trace:** After N blocks, head = N % 11. ✅ Verified correct.
**Action:** Add a comment documenting the invariant.

---

## 5. Retargeting Math

### 5.1 Check for overflow in `retarget_target` 🛡️🔬
**Current:**
```rust
let prod = (old_u64[i] as u128) * (actual_timespan as u128) + carry;
```
**Analysis:** `actual_timespan` is clamped to `[expected/4, expected*4]` = `[302400, 4838400]`. `old_u64[i]` ≤ 2^64. Product ≤ 2^64 × 4838400 ≈ 2^86, fits in u128. ✅ No overflow.

**Division:** `val / (expected_timespan as u128)` where expected = 1209600. `val` is at most (2^128 - 1). Division is always valid (divisor ≠ 0). ✅

**Verdict:** No overflow risk. No change needed.

### 5.2 Optimize `retarget_target` u256 multiply/divide 🔧
**Current:** Generic u256 × u32 multiply with 4 iterations, then u256 ÷ u32 divide with 4 iterations.
**Observation:** This runs once per 2016 blocks. For a 100-header batch, it never runs. Even for a 100K-header batch, it runs ~50 times.
**Impact:** Negligible. Don't optimize.

---

## 6. Cycle Profiling 🔬

### 6.1 Measure actual cycle breakdown
**Current:** Cycle tracker labels exist (`parse`, `sha256d`, `retarget`) but we don't have the actual numbers.
**Action:** Run the program with cycle tracker enabled and collect per-section cycle counts:
```
stdout: cycle-tracker-start: parse
stdout: cycle-tracker-end: parse
stdout: cycle-tracker-start: sha256d
stdout: cycle-tracker-end: sha256d
```
Parse these from the dry-run output to get:
- Total cycles for `parse` across 100 headers
- Total cycles for `sha256d` across 200 calls (100 genesis checks + 100 PoW checks, though genesis is only 1)
- Per-header average

**Hypothesis:** SHA-256 dominates (>90% of cycles). Parsing and median are negligible.

### 6.2 Compare direct syscalls vs. patched sha2 crate 🔬
**Action:** Revert to the patched `sha2` crate temporarily, measure cycles, compare with direct syscall implementation.
**Expected:** Direct syscalls should be faster (no iterator overhead, no trait dispatch), but the difference may be small since the patched crate just wraps the same syscalls.
**Decision point:** If the difference is <5%, consider reverting to the crate for code simplicity.

---

## 7. Proof Generation & Host Side

### 7.1 Parallel proof generation 🔧
**Current:** Compressed proof generates first, then Groth16. Sequential.
**Proposal:** Spawn two tasks, one for compressed and one for Groth16, using `tokio::join!`.
**Caveat:** Both use the same `stdin` (needs clone). The prover client may not support concurrent proofs.
**Investigation needed:** Does `ProverClient` support concurrent `prove()` calls? If not, this won't help.

### 7.2 Validate ELF integrity 🛡️💰
**Current:** The host trusts whatever ELF `include_elf!` embeds.
**Proposal:** Compute and log the ELF hash at startup:
```rust
let elf_hash = Sha256::digest(ELF.as_ref());
tracing::info!("ELF SHA-256: {}", hex::encode(elf_hash));
```
**Why:** Makes it easy to verify the prover is running the expected program. Useful for auditing.

### 7.3 Validate proof file before loading 🛡️
**Current:** `SP1ProofWithPublicValues::load(path)` can panic on corrupt files.
**Proposal:** Check file exists and has reasonable size before loading.
**Impact:** Minor UX improvement.

---

## 8. Recursive Chaining Correctness

### 8.1 Verify start_height consistency ✅ FIXED
**Fix applied:** The program now validates that `start_height` from stdin matches
`prev_num_headers` from the previous proof's public values. If no previous proof
is provided, `start_height` must be 0 (genesis). New error code `STATUS_HEIGHT_MISMATCH = 5`.

### 8.2 Validate median window consistency on resume 🛡️🔬
**Current:** When resuming, the program reads median_timestamps from the PV and rebuilds packed. It trusts the timestamps are valid.
**Risk:** If the previous proof's PV was tampered with, the timestamps could be garbage.
**Mitigation:** The recursion circuit verifies that the previous proof's PV matches the committed value digest. So the timestamps are cryptographically bound to the previous proof. No additional validation needed.
**Verdict:** Safe as-is. The STARK proof guarantees PV integrity.

---

## 9. Public Values Optimization

### 9.1 Reduce commit_slice calls 🔬
**Current:** 11 `commit_slice` calls (genesis, final_hash, num_headers, final_header, 4× chain_work, epoch_ts, 11× median_timestamps, success, error_detail).
**Proposal:** Batch some commits. E.g., commit `final_hash + num_headers` as a single 40-byte slice.
**Impact:** Fewer syscalls. Unclear if syscall overhead is significant in the zkVM.
**Investigation needed:** Measure cycle impact of N commits vs. 1 commit of N bytes.

### 9.2 Commit median_count explicitly (reconsider) 🔬
**Current:** median_count is derivable from num_headers.
**Trade-off:** Deriving saves 4 bytes but requires a division (mod 11) on the host side.
**Verdict:** Keep as-is. Division is cheap on the host.

---

## 10. Consensus Correctness

### 10.1 Validate header version field 🛡️
**Current:** Version is parsed but not validated.
**Bitcoin rule:** Version must be ≥ 1 (and specific version bits for BIP9 signaling).
**Proposal:** Add validation:
```rust
let version = i32::from_le_bytes(header[0..4].try_into().unwrap());
if version < 1 {
    commit_error_and_exit(..., STATUS_INVALID_VERSION, i as u32);
}
```
**Impact:** Rejects obviously invalid headers.

### 10.2 Validate merkle root is non-zero 🛡️
**Current:** Merkle root is parsed but not checked.
**Bitcoin rule:** A block with an empty merkle root is invalid.
**Proposal:**
```rust
if merkle_root == [0u8; 32] {
    commit_error_and_exit(..., STATUS_EMPTY_MERKLE_ROOT, i as u32);
}
```
**Impact:** Minimal — no real block has an empty merkle root.

---

## Priority Matrix

| Priority | Items | Rationale |
|----------|-------|-----------|
| **P0 (Do now)** | 1.1, 1.2, 2.2, 7.2 | Zero-effort wins: eliminate unwrap(), improve auditability |
| **P1 (Next)** | 2.3, 3.2, 3.3, 8.1 | Stability: prevent edge-case panics, verify chaining correctness |
| **P2 (Explore)** | 6.1, 6.2, 9.1, 7.1 | Measurement-driven: can't optimize what we haven't measured |
| **P3 (Later)** | 10.1, 10.2, 3.4 | Consensus: nice-to-have but not critical for correctness |
| **Skip** | 4.1, 5.2, 9.2 | Not worth the trade-off |

---

## Open Questions for Exploration

1. **What's the actual cycle count for sha256d per header?** The cycle tracker shows the labels but we need the numbers. Estimate: ~3 syscall_extend + 3 syscall_compress per double-SHA256. Each extend is ~48 rows, each compress is ~64 rounds. Total per header ≈ (48 + 64) × 2 = 224 rows. But the actual constraint count depends on the chip implementation.

2. **Does `io::read_vec()` copy data?** If the hint stream provides data via a syscall that writes directly to a pre-allocated buffer, the copy could be avoided.

3. **How does Groth16 proving time scale with batch size?** Compressed proof is ~30s for 100 headers. Groth16 wraps the compressed proof, so it should be constant time regardless of batch size. Need to verify.

4. **What's the overhead of `println!` for cycle tracking?** Does it add constraints? If so, removing it in production would save cycles.
