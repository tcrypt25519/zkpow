# Bitcoin Header Chain Prover — SP1 Implementation Plan

## Overview

This document describes the implementation of a Bitcoin header chain prover using SP1's zero-knowledge virtual machine. The system generates a single proof that validates an arbitrary number of Bitcoin block headers, supports chaining proofs across runs via recursion, and produces on-chain verifiable wrapped proofs.

### Goal

Produce one cryptographic proof that attests to the validity of a chain of Bitcoin block headers, where:

- **Dynamic batch size**: 1 header or 100,000+ headers produce exactly one proof
- **Fail-fast**: Invalid header at any position immediately aborts (no proof generated)
- **Recursive chaining**: A proof from a previous run can be fed into a new run to extend the chain
- **Dual output**: Each run produces both a compressed proof (for chaining) and an optional wrapped proof (for on-chain verification)
- **Genesis-bound**: Every proof is cryptographically bound to a hardcoded mainnet genesis hash, so verifiers always know which chain they're verifying

---

## Design Decisions (Resolved)

### 1. Difficulty Retargeting — Full Constrained Implementation ✅

**Decision**: Implement the complete 2016-block retargeting algorithm inside the zkVM, fully constrained.

**State carried forward:**
- `last_epoch_start_timestamp: u32` — timestamp of the block at the last retarget boundary
- `prev_target: [u8; 32]` — current difficulty target (32-byte little-endian)

**Every 2016 blocks**, the program computes `actual_timespan`, clamps to `[expected/4, expected*4]`, computes the new target, and verifies it matches the block's `bits` field.

### 2. Timestamp Median Check — 11-Block Sliding Window

**Decision**: Implement the full BIP113 median-of-last-11-blocks timestamp check, fully constrained. (Not yet implemented — planned for next phase)

### 3. Maximum Practical Batch Size — No Hardcoded Limit

**Decision**: No hardcoded limit in the zkVM program. The program accepts any `u64` number of headers. Practical limits determined by proving time.

### 4. Genesis Block Handling — Hardcoded in Script, Validated in Program ✅

**Decision**: The Genesis block hash is hardcoded in the host script and passed as a public input. Height 0 gets special treatment: verify double-SHA256 matches genesis hash, no PoW or prev_blockhash check needed.

**The genesis hash is committed as the first public value**, permanently binding every proof to the chain's starting point.

### 5. Header Serialization — Raw 80-Byte Format ✅

**Decision**: Headers are passed as raw 80-byte concatenated buffers. No JSON, no CBOR.

### 6. Error Handling — Committed Error Codes

**Decision**: Instead of panicking, commit an error code before exit so verifiers know what failed. (Currently uses `panic!` — deferred to later phase)

### 7. Testnet vs. Mainnet — Mainnet Only ✅

**Decision**: Mainnet only for the initial implementation.

---

## Implementation Status

### Phase 1: Basic Single-Batch Proof ✅ COMPLETE

**Goal**: Prove validity of a single batch of headers starting from Genesis.

| Component | Status | Notes |
|-----------|--------|-------|
| Program structure (`entrypoint!`, I/O) | ✅ | Full zkVM program with constrained execution |
| Genesis hash verification (height 0) | ✅ | Verifies double-SHA256 matches expected genesis |
| Chain linkage (prev_blockhash) | ✅ | Constrained equality check for every block |
| Proof-of-Work verification | ✅ | SHA-256 precompile via patched `sha2` crate |
| Bits/target conversion | ✅ | Canonical round-trip: `bits→target→bits` verified correct |
| Canonical chain work formula | ✅ | `floor(2^256 / (target + 1))` — exact Bitcoin formula |
| Difficulty retargeting | ✅ | 2016-block retarget with clamping, constrained |
| Public values commitment | ✅ | 189 bytes committed, host-side verified |
| Host script (load, prove, verify) | ✅ | SQLite loading, compressed proof, verification |
| Proof serialization | ✅ | Save/load compressed proof |
| `inspect_proof` binary | ✅ | Human-readable proof inspection |
| Cumulative chain work tracking | ✅ | u256 accumulator, constrained addition |

**Verified output** (100 blocks, genesis → block 99):
- Genesis: `000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f` ✓
- Chain tip: `00000000cd9b12643e6854cb25939b39cd7a1ad0af31a9bd8b2efe67854b1995` ✓
- Cumulative work: `0x0000000000000000000000000000000000000000000000000000006400640064` ✓
- Execution: ~285K cycles, compressed proof in ~30s

### Phase 2: Difficulty Retargeting ✅ COMPLETE (merged into Phase 1)

The retargeting logic was included in the initial Phase 1 implementation since it's straightforward arithmetic that doesn't add significant cycle cost. It's been verified working (100 blocks all at genesis difficulty, so retarget not triggered yet, but the code path is present and constrained).

### Phase 3: Timestamp Median Check ⏳ NEXT

**Goal**: Full BIP113 median-of-11-blocks timestamp validation, fully constrained.

**What's needed:**
1. Circular buffer `[u32; 11]` in the zkVM program
2. Median computation (sort 11 elements, take index 5)
3. `timestamp > median` check for every block (once buffer is full)
4. `timestamp < prev + 2h` upper bound check
5. **Carry forward median state in public values** — adds 48 bytes (`[u32; 11]` + `count` u32). This is required for secure recursive chaining: the next batch's program needs the actual buffer state, not just a hash of it. Hashing the pre-image would only save 12 bytes (48 → 36) which isn't worth the complexity.

**Updated public values layout** (189 → 237 bytes):
| Offset | Size | Field |
|--------|------|-------|
| 0..32 | 32 | genesis_hash |
| 32..64 | 32 | final_header_hash |
| 64..72 | 8 | num_headers |
| 72..152 | 80 | final_header |
| 152..184 | 32 | cumulative_chain_work |
| 184..188 | 4 | last_epoch_start_timestamp |
| 188..232 | 44 | median_timestamp_buffer (`[u32; 11]`) |
| 232..236 | 4 | median_timestamp_count |
| 236..237 | 1 | success_code |

### Phase 4: Recursive Chaining ⏳ PLANNED

**Goal**: Support extending a chain using a previous proof via deferred proofs.

**What's needed:**
1. Add `verify` feature to `program/Cargo.toml`
2. Program reads optional deferred proof from stdin:
   - `has_prev_proof: bool`
   - `prev_vk_digest: [u32; 8]`
   - `prev_pv_digest: [u8; 32]`
3. Program calls `sp1_zkvm::lib::verify::verify_sp1_proof(vk_digest, pv_digest)`
4. Program extracts starting state from the **previous proof's public values**:
   - Previous final_hash → this block's starting prev_hash
   - Previous chain_work → this block's starting cumulative_chain_work
   - Previous epoch_start_timestamp → this block's starting state
   - Previous genesis_hash → re-commit (same chain anchor)
   - Previous median buffer + count → this block's starting median state
5. Host script:
   - Load previous `SP1ProofWithPublicValues` directly (no extraction needed)
   - Pass the proof + VK to stdin via `stdin.write_proof(proof, vk)`
   - Prove and verify

**Key insight**: The `SP1ProofWithPublicValues` is consumed as-is by the verifier — no need to manually extract the inner recursion proof. The entire proof-with-public-values object binds the cryptographic proof to a specific set of public input values. The recursive proof proves that:
- The previous proof was valid (via `verify_sp1_proof`)
- The previous proof's public values contained the starting state we're using
- All new headers are valid against that starting state
- The chain is continuous from Genesis through the new tip

### Phase 5: Error Handling ⏳ PLANNED

**Goal**: Structured error codes instead of panics.

**What's needed:**
1. Define `ValidationError` enum with variants for each failure mode
2. Replace `panic!()` with error code commitment + early exit
3. Host script checks error code and reports diagnostics
4. Consider: can we early-exit cleanly in the zkVM, or do we need to run to completion?

### Phase 6: On-Chain Verification ⏳ PLANNED

**Goal**: Produce Groth16 proofs for smart contract verification.

**What's needed:**
1. `.groth16()` proof generation (already supported by SP1 SDK)
2. Save both compressed and Groth16 proofs
3. Deploy/verify with Solidity verifier contract

### Phase 7: Performance Optimization ⏳ PLANNED

**Goal**: Minimize proving time for large batches.

**What's needed:**
1. Add cycle tracker labels to identify hot paths
2. Profile with 1, 10, 100, 1000, 10000 headers
3. Consider direct SHA-256 syscalls vs. patched `sha2` crate
4. Benchmark cumulative chain work cost (u256 arithmetic)
5. Optimize header parsing (minimize allocations)

---

## Public Values Layout (Current: 189 bytes)

| Offset | Size | Field | Notes |
|--------|------|-------|-------|
| 0..32 | 32 | genesis_hash | Trusted anchor — mainnet or other chain |
| 32..64 | 32 | final_header_hash | SHA256d of last validated header |
| 64..72 | 8 | num_headers | Count of headers validated |
| 72..152 | 80 | final_header | Full 80-byte header (verifier has exact data) |
| 152..184 | 32 | cumulative_chain_work | u256 LE, sum of work for all headers |
| 184..188 | 4 | last_epoch_start_timestamp | Timestamp at last retarget boundary |
| 188..189 | 1 | success_code | 0 = success, non-zero = error |

**Not included** (derivable or deferred):
- `current_target` — derivable from final_header's bits field
- `deferred_proofs_digest` — will be added in Phase 4 (recursive chaining)

---

## Architecture

### Two-Part Structure

```
examples/bitcoin-header-chain/
├── program/                    # zkVM program (constrained execution)
│   ├── Cargo.toml
│   └── src/
│       └── main.rs             # Header validation logic
└── script/                     # Host script (proving orchestration)
    ├── Cargo.toml
    ├── build.rs                # Compiles program to ELF
    ├── src/
    │   ├── lib.rs
    │   ├── main.rs             # Proving/verification orchestration
    │   ├── util.rs             # Header loading, work calculation
    │   └── bin/
    │       └── inspect_proof.rs # Human-readable proof inspector
    └── bitcoin-header-chain-proof.bin  # Generated proof (gitignored)
```

### Data Flow

```
Host (script):
  1. Load genesis hash (hardcoded mainnet)
  2. Load raw 80-byte headers from SQLite
  3. Compute expected outputs (final_hash, chain_work, etc.)
  4. Prepare stdin: genesis_hash, start_height, num_headers, headers_bytes

  ↓ (stdin)

zkVM (program):
  5. Read genesis_hash → commit as PV[0:32]
  6. Read start_height, num_headers, headers_bytes
  7. For each header:
     a. Parse 80 bytes → version, prev_blockhash, merkle_root, timestamp, bits, nonce
     b. Height 0: verify double_sha256 == genesis_hash
     c. Height > 0:
        - Verify prev_blockhash == prev_hash
        - If height % 2016 == 0: retarget difficulty
        - Verify bits == target_to_bits(current_target)
        - Verify double_sha256 meets PoW target
     d. cumulative_chain_work += work_from_bits(bits)
  8. Commit: final_hash, num_headers, final_header, chain_work, epoch_ts, success
  9. HALT(0)

  ↓ (proof)

Host (script):
  10. client.verify(proof, vk) → cryptographic verification
  11. Assert proof.public_values == expected_public_values → semantic verification
  12. Save proof to file
```

### Recursive Chaining (Phase 4)

```
Run 1: Genesis → Block N
  Public values: genesis_hash | final_hash_1 | count_1 | header_1 | work_1 | epoch_1 | 0
  → proof_1_compressed.bin

Run 2: Block N → Block M (recursive chaining)
  stdin: genesis_hash + proof_1 + new_headers
  Program:
    - verify_sp1_proof(vk_digest_1, pv_digest_1)
    - Extract starting state from proof_1's public values
    - Validate new headers
    - Commit: same genesis_hash | final_hash_2 | total_count | header_2 | total_work | epoch_2 | 0
  → proof_2_compressed.bin (proves everything from Genesis through Block M)
```

---

## Constraint Analysis

### Fully Constrained Operations

| Operation | What's Constrained |
|-----------|-------------------|
| Genesis hash verification | double-SHA256 comparison |
| Chain linkage | 32-byte equality check per block |
| PoW verification | SHA-256 precompile (AIR chips) |
| Bits/target round-trip | Compact encoding arithmetic |
| Difficulty retargeting | u256 multiply/divide with clamping |
| Chain work accumulation | u256 addition per block |
| Public value commitments | All bytes hashed into proof's committed_value_digest |

### Not Constrained (And Why It's Safe)

| What | Why Not Constrained | Why It's Safe |
|------|-------------------|---------------|
| Header bytes at read time | Prover chooses inputs | Public values commit genesis_hash + final_hash + full header; verifier checks |
| Host's SQLite parsing | Host is untrusted | zkVM parses raw bytes; malformed data causes panic/failure |
| Cumulative work value | Prover computes it | The work_from_bits formula is constrained (canonical formula), so the result is enforced |

---

## Performance Metrics (Phase 1, 100 blocks)

| Metric | Value |
|--------|-------|
| Execution cycles | ~285K |
| Compressed proof time (CPU) | ~30s |
| Proof size (compressed) | ~1.3 MB |
| Public values size | 189 bytes |
| Work per block (genesis difficulty) | ~4,295,032,833 (`0x100010001`) |
| Total cumulative work (100 blocks) | ~429,503,283,300 (`0x6400640064`) |

**Estimates for larger batches:**
| Batch Size | Estimated Cycles | Estimated Proof Time |
|------------|-----------------|---------------------|
| 1,000 headers | ~2.8M | ~5 min |
| 10,000 headers | ~28M | ~50 min |
| 100,000 headers | ~280M | ~8 hours |

---

## Remaining Open Questions

### 1. Median Buffer Size for First Batch

When starting from Genesis, the first 10 blocks don't have 11 previous timestamps. The program should skip the median check until the buffer is full. For recursive chaining, the median buffer state needs to be either:
- **Option A**: Carried forward in public values (adds 44 bytes: 11×u32 + count)
- **Option B**: Reconstructed from the previous batch's final headers (requires committing the last 11 headers)
- **Option C**: Assume the previous proof already validated the median checks, so only new blocks need checking (requires knowing when the buffer becomes full across batches)

**Recommendation**: Option A — carry forward `[u32; 11]` and count as public values. Adds minimal overhead and is unambiguous.

### 2. `verify_sp1_proof` API Verification

The exact signature is `verify_sp1_proof(vk_digest: &[u32; 8], pv_digest: &[u8; 32])`. Need to confirm:
- Does the `pv_digest` need to be computed by the program, or is it automatically tracked?
- What's the exact type for `stdin.write_proof()` — does it accept `SP1ProofWithPublicValues` directly, or does it need the inner proof + VK separately?
- How are the previous proof's public values made available inside the zkVM program? Does `verify_sp1_proof` expose them, or do they need to be passed as separate stdin inputs?

### 3. Early Exit vs. Panic for Error Handling

The zkVM may not support clean early exits (the execution trace must reach HALT). If so, error handling must use `panic!()`. The error code can still be committed before the panic, but the panic prevents proof generation entirely.

**Decision pending**: Test whether a clean `return` after committing error code produces a valid proof (it shouldn't — the program must HALT normally).

---

## Key SP1 References

| Resource | Path |
|----------|------|
| SHA-256 precompile syscalls | `crates/zkvm/entrypoint/src/syscalls/sha_extend.rs`, `sha_compress.rs` |
| Proof verification syscall | `crates/zkvm/entrypoint/src/syscalls/verify.rs` |
| `verify_sp1_proof` API | `crates/zkvm/lib/src/verify.rs` |
| Public values commitment | `crates/zkvm/entrypoint/src/syscalls/halt.rs` |
| Recursion public values | `crates/recursion/executor/src/public_values.rs` |
| Example: Tendermint light client | `examples/tendermint/` |
| Example: recursive verification | `crates/test-artifacts/programs/verify-proof/` |
| SP1 patched SHA-256 crate | `https://github.com/sp1-patches/RustCrypto-hashes` |

---

## Appendix: Canonical Chain Work Formula

The program uses Bitcoin's exact formula: `work = floor(2^256 / (target + 1))`.

The implementation:
1. `target = mantissa * 2^k` where `k = 8 * (exponent - 3)`
2. `n = 256 - k`
3. `R = 2^n mod mantissa` (computed via binary exponentiation)
4. `Q = (2^n - R) / mantissa = floor(2^n / mantissa)` (computed via long division)
5. `work = Q` if `Q <= R * 2^k`, else `Q - 1`

The `+1` in the denominator is critical:
- It prevents division by zero for the theoretical case of target = 2^256 - 1
- It ensures exact results for all valid Bitcoin targets
- The `Q <= R * 2^k` adjustment handles the case where `floor(2^256 / target)` differs from `floor(2^256 / (target + 1))` by exactly 1

For genesis difficulty (`0x1d00ffff`):
- `k = 208`, `n = 48`, `mantissa = 0x00ffff = 65535`
- `Q = 2^48 / 65535 = 4295032833 = 0x100010001`
- `R = 2^48 mod 65535 = 1`
- `R * 2^k = 1 * 2^208` which is astronomically larger than Q
- So `Q <= R * 2^k` is true → `work = Q = 0x100010001`
