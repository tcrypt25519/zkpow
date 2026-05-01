# Test Coverage Gap Analysis — zkpow26

**Date**: 2026-04-30
**Status**: Needs user review and prioritization
**TL;DR**: The test suite has solid foundational coverage but major gaps in retargeting edge cases, MTP timestamp ordering subtleties, error code completeness, recursive proof robustness, and consensus compatibility verification. Below is an exhaustive checklist ordered by priority.

---

## Current Test Inventory

### Integration Tests (`crates/host/src/bin/test_errors.rs`)
| # | Test | What It Covers | Verdict |
|---|------|---------------|---------|
| 1 | `success_100_headers` | Happy path: blocks 1–100 | ✅ Basic |
| 2 | `retarget_boundary_schedule` | First retarget epoch (height 30240→32256): verifies next_nbits matches chain, checks pre-boundary state at 32254, verifies epoch boundary at 2015, and validates bits at 32256 | ✅ Good but isolated to one epoch |
| 3 | `recursive_chain_success` | 2-segment chain (1→10, 11→20) with real proving | ✅ Basic |
| 4 | `error_timestamp_too_old` | MTP violation at block 12 (timestamp set to genesis time) | ✅ Single case |
| 5 | `error_pow_insufficient` | Corrupted nonce at block 1 | ✅ Single case |

### Unit Tests (`crates/core/src/lib.rs` — `#[cfg(test)] mod tests`)
| Category | Tests | Verdict |
|----------|-------|---------|
| Wire sizes | `fixed_width_wire_sizes_match_protocol` | ✅ |
| Failure metadata | `failure_metadata_encoding_round_trips` | ✅ |
| Serialization | `header_and_new_header_round_trip_exact_wire_bytes` | ✅ |
| u256 math | `u256_add_handles_carry_propagation`, `u256_add_wraps_at_256_bits`, `u256_mul_u32_scales_by_small_count` | ✅ |
| PoW validation | `hash_meets_target_accepts_exact_target_boundary` | ✅ (unit only) |
| Chain work batching | `apply_headers_flushes_deferred_chain_work_on_success`, `apply_headers_flushes_deferred_chain_work_before_failure` | ✅ |
| MTP + batching | `median_time_past_uses_upper_median_for_heights_zero_through_twelve`, `median_time_past_keeps_upper_median_after_two_wraps`, `next_overwrites_only_the_next_ring_slot`, `apply_headers_matches_sequential_next_across_median_window_wrap`, `hinted_apply_headers_matches_sorted_apply_headers_across_window_wrap`, `hinted_median_validation_accepts_duplicate_median_values`, `hinted_median_validation_rejects_wrong_rank_hint` | ✅ Good coverage of MTP internals |

### Input Parsing Tests (`crates/core/src/input.rs` — `#[cfg(test)] mod tests`)
| Test | Verdict |
|------|---------|
| `recursive_proof_default_is_zeros`, `parse_from_bytes_genesis_no_proof`, `parse_from_bytes_non_genesis_with_proof` | ✅ |
| `new_header_hints_round_trip`, `new_header_hints_reject_truncated_payload` | ✅ |
| `median_time_past_hints_round_trip`, `median_time_past_hints_reject_wrong_count_length`, `median_time_past_hints_reject_truncated_payload` | ✅ |
| `input_ref_rejects_misaligned_state` | ✅ |

---

## Coverage Gaps — Priority-Ordered Checklist

### 1. Difficulty Retargeting — Edge Cases (CRITICAL)

#### 1.1 Timespan Lower-Bound Clamp (×1/4)
> When actual timespan < `expected_timespan / 4`, it should be clamped UP to that minimum.
- [ ] **Synthetic test**: Fabricate headers where actual_timespan < 1209600/4 = 302,400 seconds (~3.5 days).
  - Example: Set epoch_start_timestamp to 0, last header timestamp to 100,000. Expected: timespan clamped to 302,400.
  - Verify `next_nbits` = result of retargeting with clamped 302,400 (target gets larger = difficulty decreases).
- [ ] **Assert the exact nbits value** produced, not just "it changed."
- [ ] **Test at exact boundary**: timespan = 302,399 (should clamp), timespan = 302,400 (should NOT clamp).

#### 1.2 Timespan Upper-Bound Clamp (×4)
> When actual timespan > `expected_timespan * 4`, it should be clamped DOWN.
- [ ] **Synthetic test**: Fabricate headers where actual_timespan > 4,838,400 seconds (~56 days).
  - Example: epoch_start_timestamp = 0, last header timestamp = 5,000,000. Expected: timespan clamped to 4,838,400.
- [ ] **Test at exact boundary**: 4,838,400 (no clamp), 4,838,401 (clamped).

#### 1.3 Timespan Wrapping (wrapping_sub)
- [ ] **Test wrapping timespan**: epoch_start_timestamp = `u32::MAX - 100`, last header timestamp = 200.
  - The `wrapping_sub` produces a timespan that wraps. This is semantically wrong (order reversal) but the code uses `wrapping_sub` so it must handle it.
  - What happens? The result is a huge timespan that would be clamped at the upper bound (×4).
- [ ] Document whether this is intentional or a bug.

#### 1.4 Genesis Pow Limit Clamping
> When `retarget_target` produces a target exceeding the genesis pow limit, clamp to `GENESIS_NBITS`.
- [ ] **Synthetic test**: Create a scenario where the recalculated target would be larger than genesis limit.
  - E.g., start with genesis target, make actual_timespan very small (clamped to ×1/4) → target should get 4× smaller. So this only happens with repeated difficulty decreases. Easier: start with a target VERY close to genesis limit and timespan near the lower clamp.
  - Verify `next_nbits` is exactly `0x1d00ffff` (486604799).
- [ ] **Cross-check with real bitcoind**: At height 32256, the difficulty increased (target got smaller). Verify the genesis clamp isn't wrongly triggered.

#### 1.5 Epoch Boundary Timing — First Real Difficulty Change
> Height 32256 on mainnet: first time difficulty actually changes (nbits goes from 0x1d00ffff to 0x1d00d86a).
- [ ] **Verify exact timespan**: epoch_start_timestamp should be block 30240's timestamp (1261130161), last timestamp should be block 32255's timestamp (1262152739). Timespan = 1,022,578 seconds.
- [ ] **Assert that timespan is within [302,400, 4,838,400]** — it's 1,022,578 which is in range (no clamping should occur).
- [ ] **Assert the exact nbits value**: 486594666 (`0x1d00d86a`).
- [ ] **Assert epoch_start_timestamp is correctly updated** at height 32256 (should equal block 32256's timestamp = 1262153464) for the next epoch calculation.

#### 1.6 Second Difficulty Adjustment (Height ~34,272)
> Mainnet height 34272: second retarget, nbits goes from 0x1d00d86a → 0x1d00c428.
- [ ] **Test epoch 16→17 boundary**: Verify nbits transitions from 486594666 to 486589480.
- [ ] **Assert timespan, clamp behavior**.

#### 1.7 Third+ Retargets With Significant Changes
> At height ~40,320: first time nbits diverges significantly from genesis (0x1c654657 = 476399191).
- [ ] **Test epoch 19→20 boundary** (height 40320): Verify nbits = 476399191.
- [ ] This exercises a substantially different target → good for validating target compact encoding correctness.

#### 1.8 Retarget With Unchanged Difficulty
- [ ] **Synthetic test**: Epoch where actual timespan equals expected timespan (or close enough that rounded result = same target).
  - Timespan = 1,209,600 → no change in target.
  - Verify `next_nbits` stays the same.

#### 1.9 epoch_start_timestamp Tracking
> `epoch_start_timestamp` is set to the header timestamp when `self.height % 2016 == 0` AFTER increment.
- [ ] **Verify epoch_start_timestamp at height 0** (genesis): should equal genesis timestamp.
- [ ] **Verify epoch_start_timestamp at height 2016**: After applying block 2016, height becomes 2016, so epoch_start_timestamp gets set to block 2016's timestamp. Verify correct.
- [ ] **Verify epoch_start_timestamp for blocks NOT on epoch boundary**: Should remain unchanged from previous boundary.
- [ ] **Test with synthetic series** where expected epoch starts are easy to predict.

---

### 2. Median Time Past — Subtle Ordering Rules (CRITICAL)

#### 2.1 Timestamp = Median Should FAIL
> Rule: `timestamp > median_time_past`, NOT `>=`.
- [ ] **Synthetic test**: Full window (11 timestamps), construct a header whose timestamp exactly equals the computed median. Assert `TimestampTooOld`.
- [ ] **Synthetic test**: Timestamp = median + 1. Assert success.

#### 2.2 Non-Strictly-Increasing Timestamps
> Timestamps don't need to be strictly increasing; only the MTP rule applies.
- [ ] **Synthetic test**: Block N timestamp = 1000, block N+1 timestamp = 900, but median is 800 → should PASS.
- [ ] **Synthetic test**: Block N timestamp = 1000, block N+1 timestamp = 1000 (same timestamp) → may fail if median happens to be 1000, but should pass if median < 1000. Test both.
- [ ] **Synthetic test**: Two consecutive blocks with decreasing timestamps, but both above MTP → both pass.

#### 2.3 Median Hint Validation — Error Granularity
> The zkVM asserts hints are valid via `median_hint_is_valid` which checks rank.
- [ ] **Hint too low**: Claim median lower than true median (rank check fails → `<` count exceeds median_index). Should panic. Already partially tested.
- [ ] **Hint too high**: Claim median higher than true median (rank check fails → fewer than median_index values are ≤ claimed). Test this case explicitly.
- [ ] **Hint for empty window** (height 0): Any value should be accepted since `timestamp_count() == 0` → returns true. Test various hint values at height 0.
- [ ] **Hint at edge of rank validity**: For window with duplicates, the valid range of median values can be narrow. Test hints at the exact bounds.

#### 2.4 MTP At Various Window Fill Levels
- [ ] **Height 1**: Only 1 timestamp. Median = that timestamp. Timestamp > median should fail, timestamp ≤ median should fail. Need timestamp AT LEAST median + 1.
- [ ] **Height 2**: 2 timestamps. Upper median = higher one (index 1). Timestamp must be > that.
- [ ] **Every fill level 1–11**: Explicitly test the median computation and threshold at each height.

---

### 3. Error Code Completeness (HIGH)

#### 3.1 Missing Error Code 4: GenesisHashMismatch
- [ ] **Test**: Provide a non-genesis state with `genesis_hash` that doesn't match the expected mainnet genesis.
- [ ] **Test**: Recursive proof continuation where genesis_hash should match the previous proof's genesis_hash but doesn't.

#### 3.2 Error at Various Batch Positions
- [ ] **Error at index 0** (first header): Already tested in `test_error_pow_insufficient`. ✅
- [ ] **Error at last header** (index N-1): Every preceding header should be in `last_valid_state`.
- [ ] **Error mid-batch** (e.g., index 5 in a batch of 10): Verify `last_valid_state` has correctly accumulated the first 5 headers.
- [ ] **Error immediately after a retarget**: Error on the first header after a difficulty adjustment boundary.

#### 3.3 Error State Validation
- [ ] **Verify `last_valid_state.chain_work`** after mid-batch failure equals sum of work for all successfully validated headers in the batch.
- [ ] **Verify `last_valid_state.height`** equals correct count after partial batch.
- [ ] **Verify `last_valid_state.timestamps`** ring buffer is correct after partial batch.

---

### 4. Proof-of-Work — Boundary and Edge Cases (HIGH)

#### 4.1 Hash Exactly At Target
- [ ] **Synthetic test**: Construct a header whose hash equals the expanded target exactly. Should pass (≤ check). Already a unit test — add an integration test that goes through the full zkVM.
- [ ] **Synthetic test**: Hash = target + 1 (one bit above). Should fail with `PowInsufficient`.

#### 4.2 Compact Target Encoding (bits_to_target / target_to_bits)
- [ ] **Maximum difficulty** (smallest target): e.g., nbits = 0x01003456 → exponent=1, mantissa=0x3456. Verify round-trip.
- [ ] **Zero mantissa**: nbits where mantissa is 0 → target should be all zeros.
- [ ] **Exponent overflow**: nbits where exponent-3 > 31 (e.g., 0x2300ffff, exponent=35, offset=32). Bytes beyond index 31 are silently dropped. Is this correct?
- [ ] **Large exponent but small mantissa**: e.g., 0x1c000001 → verify correctness.
- [ ] **MSB of mantissa set** (negative target): e.g., 0x1d008000. The target_to_bits logic handles this via the `mantissa & 0x800000` check. Verify correctness.

#### 4.3 work_from_target Edge Cases
- [ ] **Zero target** (genesis bits=0): work_from_target → should return ChainWork::default().
- [ ] **Target with divisor = [1,0,0,0,0]**: The code returns default. Verify when this occurs.
- [ ] **Chain work accumulation across many blocks**: Verify chain_work matches known bitcoind values for blocks at heights like 10,000, 50,000, 100,000.

---

### 5. Recursive Proof Chaining — Robustness (MEDIUM-HIGH)

#### 5.1 Multi-Segment Chain
- [ ] **3+ segments**: Chain proofs across 3 or more segments (e.g., blocks 1→10, 11→20, 21→30). Verify all public values propagate correctly.
- [ ] **Single-block segments**: Each recursive proof covers exactly 1 block. Stress test the recursive verification path.
- [ ] **Segment crossing retarget boundary**: First segment ends at height 32255 (last block of old difficulty), second segment starts at 32256 (new difficulty). Verify nbits transitions correctly in the second proof.

#### 5.2 Public Values Digest Verification
- [ ] **Wrong PV digest**: Provide a proof with correct VK but wrong PV digest → verify failure.
- [ ] **Wrong VK**: Provide a proof with wrong verifier key → verify failure.
- [ ] **Both wrong**: Complete garbage proof metadata → verify failure.
- [ ] **All-zero proof** (RecursiveProof::default() when height > 0): Should this fail at PV digest check or VK check? Currently untested.

#### 5.3 Digest Computation Consistency
- [ ] **Cross-validate**: Compute PV digest on host (via `compute_pv_digest`) and compare with the value the guest would produce via `sha256_264bytes`.

---

### 6. SHA-256 Implementation Verification (MEDIUM)

#### 6.1 Known Test Vectors
- [ ] **sha256_80bytes**: Test against known Bitcoin genesis block header → hash `000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f`.
- [ ] **sha256d_80bytes**: Same — should match genesis block hash.
- [ ] **sha256_32bytes**: Test against sha2 crate output for known 32-byte inputs.
- [ ] **sha256_264bytes**: Test against sha2 crate output for known State byte sequences.

#### 6.2 Cross-Implementation Comparison
- [ ] **Compare guest sha256_80bytes output** with host `sha2::Sha256` output. They should be identical.
- [ ] **Compare guest sha256d_80bytes** with host `sha256d` function. They should be identical.

#### 6.3 State Sizing Verification
- [ ] **sha256_264bytes**: Verify that 264 bytes is exactly `STATE_SIZE` (already confirmed: STATE_SIZE = 264). But document that any State layout change requires updating the SHA-256 padding (the length encoded is 2112 bits = 264 bytes).

---

### 7. Input Parsing — Additional Cases (MEDIUM)

#### 7.1 NewHeader Batch Sizes
- [ ] **Zero headers** (empty batch): What happens? Should it succeed trivially or be rejected?
- [ ] **Single header**: Already exercised but not as a focused test.
- [ ] **Large batch** (1000+ headers): Stress test memory and performance.
- [ ] **Batch exactly aligned to a retarget boundary**: Last header + 1 is a new epoch.

#### 7.2 Malformed State Inputs
- [ ] **State with height > 0 but genesis_hash is zero**: Should be rejected in continuation mode.
- [ ] **State with impossible field combinations** (e.g., height 0 but non-zero timestamps).
- [ ] **Unaligned buffer for recursive proof**: Already tested for State, but not for RecursiveProof.

#### 7.3 MedianTimePast Hint Mismatches
- [ ] **Hint count != header count**: Should panic with assertion in `apply_headers_in_place`.
- [ ] **Hint count correct but values wrong**: Should fail `median_hint_is_valid` check.

---

### 8. Bitcoind Consensus Compatibility (MEDIUM)

#### 8.1 Known Chain Work Values
- [ ] **Block 100,000**: Verify chain_work matches bitcoind's `getblockheader` output.
- [ ] **Block 200,000**: Same.
- [ ] **First 10,000 blocks**: Cross-validate all block hashes against known mainnet values.

#### 8.2 Block Hash Verification
- [ ] **Genesis block**: hash_header should produce the known genesis hash.
- [ ] **Block 1** (first after genesis): Verify hash matches mainnet.
- [ ] **Block 170** (first halving): Verify correct.
- [ ] **Multiple random blocks in first 50,000**: Verify 10+ known block hashes.

#### 8.3 Difficulty Reference Values
- [ ] **Manual calculation**: For known epochs, compute the retarget manually and verify against bitcoind's nbits values.
- [ ] **Epoch 17 (height 34272)**: Expected nbits = 486589480 (`0x1d00c428`). Verify.
- [ ] **Epoch 18 (height 36288)**: Expected nbits = 486588017 (`0x1d00be71`). Verify.

---

### 9. State Machine Edge Cases (LOW-MEDIUM)

#### 9.1 u32 Height Overflow
- [ ] **Height approaching u32::MAX**: Bitcoin has ~850,000 blocks. u32 wraps at 4,294,967,296. Test behavior near wrap point (synthetic).
- [ ] **Timestamp slot calculation at u32 wrap**: `next_timestamp_slot = height % 11` — correct even at overflow.

#### 9.2 Chain Work Overflow
- [ ] **u256 chain_work overflow**: Accumulate work until the 256-bit value wraps. Test behavior.
- [ ] **Deferred work accumulation with overflow**: When flushing a large pending run, the `u256_mul_u32` + `u256_add` could overflow. Test this.

#### 9.3 Minimum/Maximum nbits in Retargeting
- [ ] **Target retargeting from near-minimum target**: If current target is near the smallest possible, retargeting UP (timespan small) should increase it. Test.
- [ ] **Target retargeting from near-maximum target**: Retargeting DOWN (timespan large) should decrease it, potentially hitting genesis limit clamp.

---

### 10. Integration / End-to-End Tests (MEDIUM)

#### 10.1 Real Proving Pipeline (not just mock execution)
- [ ] **Full prove + verify**: Use `ProverClient::prove()` (not mock/execute) for a realistic test, verifying both compressed and Groth16 proofs.
- [ ] **Compressed proof on real hardware**: Tests with mock execution use simulated SP1. Real prove tests catch CUDA/CPU-specific issues.

#### 10.2 Long-Range Validation
- [ ] **Validate 10,000+ real mainnet blocks** through the zkVM execution path.
- [ ] **Validate across 10+ retarget boundaries**, verifying state after each.

#### 10.3 Python/Rust Host Comparison
- [ ] **Cross-validate host-side state computation** against a Python reference (e.g., python-bitcoinlib) for a sample of blocks.

---

### 11. Regression / Invariant Tests (LOW-MEDIUM)

#### 11.1 Fuzz Targets
- [ ] **Fuzz NewHeader construction**: Generate random NewHeader values, verify that `NewHeader::parse(NewHeader::to_bytes())` round-trips.
- [ ] **Fuzz State serialization**: Generate random-ish State values, verify round-trip.
- [ ] **Fuzz apply_headers vs sequential next**: Random timestamp sequences, verify batched and sequential produce identical state (already a targeted test — make it a fuzz test with >1000 random sequences).

#### 11.2 Deterministic Behavior
- [ ] **Same inputs → same public values**: Run same inputs multiple times, verify identical PVs.
- [ ] **Same inputs → same proof** (for deterministic proving mode).

#### 11.3 Memory Safety
- [ ] **miri**: Run miri on the core crate to catch undefined behavior in unsafe blocks.
- [ ] **Valgrind**: Run the host binary under valgrind for a small batch.

---

### 12. Documentation / Test Infrastructure

#### 12.1 Test Harness Improvements
- [ ] **Helper to build synthetic State with specific timestamps and nbits**: Currently tests construct states manually. A builder pattern would make new tests easier.
- [ ] **Helper to generate headers with specific timestamps**: Make it easy to construct sequences for MTP testing.
- [ ] **Parameterized test runner**: Many tests share the same skeleton (build state, create headers, run, assert). Extract into a reusable harness.

#### 12.2 Test Coverage Reporting
- [ ] **Set up `cargo tarpaulin` or `cargo llvm-cov`** to generate coverage reports.
- [ ] **Add CI step**: Fail if coverage drops below a threshold.

---

## Priority Summary

| Priority | Category | Tests Needed | Effort |
|----------|----------|-------------|--------|
| 🔴 Critical | Retargeting edge cases (×1/4, ×4 clamps, wrapping) | 8–10 | Medium |
| 🔴 Critical | MTP ordering subtleties (equal-to-median, decreasing timestamps) | 6–8 | Medium |
| 🟠 High | Missing error code tests (GenesisHashMismatch, error positions) | 5–7 | Low |
| 🟠 High | PoW boundary cases (exact target, compact encoding edges) | 6–8 | Medium |
| 🟡 Medium | Recursive proof robustness (3+ segments, wrong digests) | 5–6 | Medium |
| 🟡 Medium | SHA-256 test vectors | 4–5 | Low |
| 🟡 Medium | Consensus compatibility (known block hashes, chain work) | 8–10 | Medium |
| 🟢 Low | State machine edge cases (overflow, wrapping) | 4–5 | Low |
| 🟢 Low | Fuzz/invariant tests | 3–4 | Medium |
| 🟢 Low | Test infrastructure improvements | 3–4 | Medium |

---

## Appendix A: Key Mainnet Test Vectors

### Genesis
- **Height 0**: timestamp=1231006505, nbits=486604799 (`0x1d00ffff`), hash=`000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f`

### First Retarget Epoch (Epoch 15 → 16, blocks 30240→32256)
| Field | Value |
|-------|-------|
| Epoch start height | 30240 |
| Epoch start timestamp | 1261130161 |
| Last block height | 32255 |
| Last block timestamp | 1262152739 |
| Actual timespan | 1,022,578 sec (~11.83 days) |
| Expected timespan | 1,209,600 sec (14 days) |
| Clamped? | No (within [×1/4, ×4]) |
| Old nbits | 486604799 (`0x1d00ffff`) |
| New nbits (at 32256) | 486594666 (`0x1d00d86a`) |

### Second Retarget (Epoch 16 → 17, blocks 32256→34272)
| Field | Value |
|-------|-------|
| Epoch start height | 32256 |
| Epoch start timestamp | 1262153464 |
| Last block height | 34271 |
| New nbits (at 34272) | 486589480 (`0x1d00c428`) |

### Third Retarget (Epoch 17 → 18, blocks 34272→36288)
| Field | Value |
|-------|-------|
| New nbits (at 36288) | 486588017 (`0x1d00be71`) |

### First Large Difficulty Change (Epoch 19 → 20, blocks 38304→40320)
| Field | Value |
|-------|-------|
| New nbits (at 40320) | 476399191 (`0x1c654657`) |

### MTP Window Reference
| Height | Median (upper) | Note |
|--------|---------------|------|
| 0 | None | No MTP check |
| 1 | timestamp[0] | 1 element, upper median = index 0 |
| 2 | max(t[0], t[1]) | 2 elements, upper = index 1 |
| 11 | sorted[5] | Full window, upper median = 11/2 = 5 (0-indexed) |
| 12 | sorted[5] (of 11 most recent) | Window wraps |

---

## Appendix B: Error Code Reference

| Code | Name | Trigger | Currently Tested? |
|------|------|---------|-------------------|
| 0 | Success | All headers valid | ✅ (implicit) |
| 1 | HeaderPayloadLengthInvalid | Input length mismatch (host-side) | ❌ |
| 2 | PowInsufficient | SHA256d(header) > target | ✅ (single case) |
| 3 | TimestampTooOld | timestamp ≤ MTP | ✅ (single case) |
| 4 | GenesisHashMismatch | Height 0 hash ≠ expected | ❌ |

---

*Generated by AdaL for the zkpow26 project.*
