# Bitcoin Header Chain Prover — Implementation Plan

## Overview

Produce one cryptographic proof that attests to the validity of a chain of Bitcoin block headers, where:

- **Dynamic batch size**: 1 header or 100,000+ headers produce exactly one proof
- **Fail-fast**: Invalid header at any position produces a proof with an error code
- **Recursive chaining**: A proof from a previous run can be fed into a new run to extend the chain
- **Dual output**: Each run produces both a compressed proof (for chaining) and an optional wrapped proof (for on-chain verification)
- **Genesis-bound**: Every proof is cryptographically bound to a hardcoded mainnet genesis hash

---

## Design Decisions

| Decision | Choice |
|----------|--------|
| Difficulty retargeting | Full constrained 2016-block algorithm with clamping |
| Timestamp median | Full BIP113 11-block sliding window, state in public values |
| Batch size limit | None (u64 max), practical limits from proving time |
| Genesis handling | Hardcoded in script, validated in program, committed as PV |
| Header serialization | Raw 80-byte concatenated buffers |
| Error handling | Structured error codes, valid proofs on failure |
| Network | Mainnet only |
| median_count | Removed — derivable as `min(11, total_validated - 1)` |

---

## Implementation Status

### Phase 1-5: ✅ COMPLETE

| Component | Status |
|-----------|--------|
| Genesis hash verification | ✅ |
| Chain linkage (prev_blockhash) | ✅ |
| PoW via SHA-256 precompile | ✅ |
| Canonical chain work `floor(2^256/(target+1))` | ✅ |
| Difficulty retargeting (2016 blocks) | ✅ |
| BIP113 median-of-11 timestamp check | ✅ |
| Recursive chaining (`verify_sp1_proof`) | ✅ |
| Structured error handling (10 codes) | ✅ |
| `inspect_proof` binary | ✅ |

**Verified**: Run 1 (0→99) → Run 2 (100→199) both produce valid compressed proofs with recursive chaining.

### Public Values Layout (237 bytes)

| Offset | Size | Field | Notes |
|--------|------|-------|-------|
| 0..32 | 32 | genesis_hash | Trusted anchor |
| 32..64 | 32 | final_header_hash | SHA256d of last header |
| 64..72 | 8 | num_headers | Total validated count |
| 72..152 | 80 | final_header | Full 80 bytes |
| 152..184 | 32 | cumulative_chain_work | u256 LE |
| 184..188 | 4 | last_epoch_start_timestamp | For next retarget |
| 188..232 | 44 | median_timestamp_buffer | `[u32; 11]` LE |
| 232..233 | 1 | success_code | 0=success, 1-10=error |
| 233..237 | 4 | error_detail | Header index on error |

**Derivable fields** (not committed):
- `median_count` = `min(11, num_headers - 1)` for `num_headers > 0`, else 0
- `current_target` = extract from `final_header[72..76]` bits field

### Error Codes

| Code | Name | Trigger |
|------|------|---------|
| 0 | Success | All headers valid |
| 1 | Genesis hash mismatch | Height 0 hash ≠ expected genesis |
| 2 | Prev blockhash mismatch | `header.prev_blockhash ≠ prev_hash` |
| 3 | PoW insufficient | `SHA256d(header) > target` |
| 4 | Timestamp too old | `timestamp ≤ median_of_last_11` |
| 5 | Timestamp too future | `timestamp > prev + 2h` |
| 6 | Bits mismatch | `header.bits ≠ target_to_bits(computed_target)` |
| 7 | Header count mismatch | `headers_bytes.len() ≠ num_headers * 80` |
| 8 | Prev proof too short | Previous PV < 237 bytes |
| 9 | Prev genesis mismatch | Previous PV genesis ≠ current genesis |
| 10 | Prev proof failed | Previous proof `success_code ≠ 0` |

---

## Architecture

```
bitcoin-header-chain/
├── program/
│   ├── Cargo.toml          # sp1-zkvm + patched sha2
│   └── src/main.rs         # zkVM program (~540 lines)
└── script/
    ├── Cargo.toml          # sp1-sdk, rusqlite, etc.
    ├── build.rs            # sp1_build::build_program
    └── src/
        ├── lib.rs          # pub mod util
        ├── main.rs         # Host: load → prove → verify
        ├── util.rs         # DB loading, work calculation, PV builder
        └── bin/
            └── inspect_proof.rs  # Human-readable proof display
```

### Data Flow

```
Host:
  1. genesis_hash (hardcoded)
  2. has_prev_proof = prev_proof_path.is_some()
  3. [if prev] pv_bytes, vk_digest, pv_digest from previous PV
  4. [if prev] stdin.write_proof(inner_proof, vk)
  5. start_height, num_headers, headers_bytes

zkVM:
  6. Read genesis_hash → commit PV[0:32]
  7. Read has_prev_proof
  8. [if prev] Read vk_digest, pv_digest, pv_bytes
  9. [if prev] verify_sp1_proof(vk_digest, pv_digest)
  10. [if prev] Extract prev_final_hash, prev_work, prev_epoch_ts, prev_median from PV
  11. Read start_height, num_headers, headers_bytes
  12. For each header:
      - Height 0: verify genesis, init state
      - Height > 0: check prev_hash, median, retarget, bits, PoW
      - On error: commit_error_and_exit(code, header_index) → HALT(0)
  13. On success: commit all PVs → HALT(0)

Host:
  14. Verify proof cryptographically
  15. Verify PV matches expected (computed independently)
  16. Save proof
```

### Recursive Chaining

```
Run 1: Genesis → Block 99
  PV: genesis=mainnet | final_hash=block99 | count=100 | work=W1 | ... | success=0

Run 2: Block 100 → 199 (extends Run 1)
  Host: loads proof_0_99.bin, passes via stdin.write_proof()
  zkVM: verify_sp1_proof() → extracts block99 hash as prev_hash
        validates blocks 100-199 → commits total count=200, work=W1+W2
  PV: genesis=mainnet | final_hash=block199 | count=200 | work=W1+W2 | ... | success=0

  proof_100_199.bin proves the ENTIRE chain from Genesis through Block 199.
```

---

## Performance Metrics

| Metric | Value |
|--------|-------|
| Cycles (100 headers, genesis difficulty) | ~293K |
| Compressed proof time (CPU) | ~30-50s |
| Proof size | ~1.3 MB |
| Public values | 237 bytes |
| Work per header (genesis) | 0x100010001 (~4.3B) |

**Estimates for larger batches:**
| Headers | Cycles | Est. Time (CPU) |
|---------|--------|-----------------|
| 1,000 | ~2.9M | ~5 min |
| 10,000 | ~29M | ~50 min |
| 100,000 | ~290M | ~8 hours |

---

## Remaining Phases

### Phase 7: On-Chain Verification ✅ COMPLETE

- Groth16 proof generation via `client.prove(&pk, stdin).groth16().await`
- Produces a BN254 Groth16 SNARK proof (~200 bytes, ~100k gas on Ethereum)
- Both compressed and Groth16 proofs are saved each run
- Groth16 proof verified cryptographically before saving
- Files: `proof_height_X_to_Y.bin` (compressed) and `proof_height_X_to_Y_groth16.bin` (on-chain)

### Phase 8: Performance ⏳

- Direct SHA-256 syscalls vs. patched crate
- Cycle tracker profiling
- Batch size optimization
