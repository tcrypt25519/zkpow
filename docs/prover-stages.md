# Prover Invocation Stages

A shared vocabulary for discussing what happens during **one invocation of the
prover on one batch of headers**. Use these stage names in issues, logs, and
conversation so "it's slow in warm processing" or "it fails at predecessor
verification" mean the same thing to everyone.

The lifecycle splits into four groups. Stages are numbered continuously; the
`⟳` mark denotes stages that loop. Where a stage corresponds to a label already
emitted to `logs/run.jsonl` (host `timed_*` phases) or to an SP1 cycle-tracker
region (guest `#[tracked]` functions), that label is given so the name is
greppable.

| # | Stage | Where | Log / track label |
|---|-------|-------|--------------------|
| 0 | Prover Setup | host (once/process) | `build_*_prover`, `setup_vkey`, `cuda_preflight` |
| 1 | Batch Preparation | host | `fetch_batch`, `load_optional_proof`, `build_verifier_key`, `serialize_input` |
| 2 | Input Ingestion | guest | `main` (reads) |
| 3 | Predecessor Verification | guest | `verify_prior_proof` |
| 4 | Cold-Window Validation | guest | `apply_headers` |
| 5 | Warm-Window Validation ⟳ | guest | `apply_headers` |
| 6 | Epoch Retarget ⟳ | guest | `prepare_new_epoch` |
| 7 | Output Commitment | guest | `compute_continuation_digest`, `commit` |
| 8 | Core Shard Proving | prover | inside `prove_compressed` |
| 9 | Recursive Compression | prover | inside `prove_compressed` |
| 10 | Proof Verification | host | `verify_compressed_public_values`, `verify_compressed_proof` |
| 11 | SNARK Wrapping (optional) | prover | `shrink_wrap`, `build_groth16_*`, `prove_groth16` |
| 12 | Persistence & Chaining | host | `save_compressed_proof` |

Stages **2–7 run inside the zkVM** and are the novel, domain-specific heart of
the system; they are what actually gets proven. Stages 8–11 are SP1's proving
machinery. Stages 0–1 and 12 are host orchestration.

---

## Group A — Preparation (host, native)

### 0. Prover Setup
**What happens.** Once per process, the host builds the prover backend
(`Mock` / `Cpu` / `Cuda`) and derives the proving/verifying key from the guest
ELF, caching both in a `OnceLock` for reuse across all batches. On CUDA builds it
runs a preflight (`nvidia-smi` parse: compute capability, VRAM, CUDA version).

- **Slow / costly.** Key setup is non-trivial but amortized across the whole run.
- **Failure modes.** Missing/incompatible GPU or driver; CUDA feature not compiled
  in but `ZKPOW_USE_CUDA=1` requested (config error). See `cuda_env.rs`.
- **Novel.** The verifying-key digest computed here (`hash_u32`) becomes the
  `verifier_key` committed by the guest and the identity a recursive proof is
  pinned to — see stage 3.

### 1. Batch Preparation
**What happens.** The host asks the [controller](../.wiki/features/controller-service.md)
for the next batch (`fetch_batch`): a slice of `NewHeader`s (default 2016), the
sparse median-time-past hints, and the **expected** end-state + continuation
digest the controller derived independently from the DB. If continuing a chain,
the previous compressed proof is loaded (`load_optional_proof`). Everything is
serialized into `SP1Stdin` (`serialize_input`) in the exact order the guest reads
it (verifier key, state, headers, median hints, optional prior proof via `write_proof`).

- **Slow / costly.** Usually cheap. Loading a ~1.3 MB prior proof and (for HTTP
  controllers) the network round-trip dominate.
- **Failure modes.** `NO_HEADERS_REMAINING` (clean end-of-chain, stops the loop);
  a prior proof that is not `Compressed` (recursion requires a compressed inner
  proof); DB gaps.
- **Novel.** The controller computes the expected result *before* proving, so the
  host can cross-check the guest output byte-for-byte (stage 10) and the
  controller only advances its cursor on a match — the integrity checkpoint
  between the trusted DB and the untrusted prover.

---

## Group B — Guest Execution (zkVM, deterministic)

This is the program whose execution is proven. It runs at least twice per batch:
once as a standalone `execute_program` (to obtain and cross-check public values
cheaply) and again inside proving to generate the trace. Everything here is
measured in **cycles**, not wall-clock — cycle count is the cost that matters.

### 2. Input Ingestion
**What happens.** Four `read_vec()` calls pull the witness streams; each is parsed
into its typed form (`VerifierKeyDigest`, `State`, `&[NewHeader]`,
`&[BlockTimestamp]`). Header and median hints are zero-copy slices.

- **Slow / costly.** Minor. Parsing is length/alignment checks plus a copy for
  `State`.
- **Failure modes.** Any malformed length panics ("should parse") — surfaced as a
  guest abort, not a validation error code. Length checks are the closest thing to
  error code 1 (`HeaderPayloadLengthInvalid`).
- **Novel.** The prior `State` is supplied as **private witness**, not recomputed;
  its trustworthiness is established by stage 3 (recursive) or by the host having
  seeded a trusted anchor (genesis / trusted height).

### 3. Predecessor Verification
**What happens.** Only when `state.height > 0`. The guest reconstructs the prior
proof's public values *from the witnessed `State`* — recompute the continuation
digest (`SHA-256` of the 52-byte continuation data), rebuild the 169-byte
`MinimalPublicValues::success`, hash them — and calls `verify_sp1_proof(vkey,
pv_hash)`. This binds the private starting state to the exact claim the previous
proof committed, under the pinned verifying key.

- **Slow / costly.** A recursive SP1 verification is one of the most
  cycle-expensive single operations in the guest.
- **Failure modes.** Tampered continuation/claim fields change the reconstructed
  hash → verification fails. A failed prior proof cannot be extended (only
  `return_code == 0` reconstructs as `success`). Wrong `verifier_key` → failure.
- **Novel / unique.** This is the recursion link. At genesis (`height == 0`) it is
  **skipped entirely** — the anchor is trusted because the host validated it
  (`genesis_state_from_record`), not because the guest re-checks the genesis hash.
  (`ValidationErrorCode::GenesisHashMismatch` exists but is not emitted by the
  current guest; genesis integrity is a host-side property.)

### 4. Cold-Window Validation
**What happens.** Runs only while `height < WINDOW_SIZE - 1` (i.e. < 10), i.e.
near genesis. The median-time-past window isn't full yet, so the median is read
**directly** from the timestamp ring at the upper-median index
(`height.div_ceil(2)`) — no hints, no rank check. Each header still goes through
the common per-header check (stage 5's `apply_header_after_median_check`).

- **Slow / costly.** Negligible; only the first ≤10 blocks of the entire chain
  ever hit this path.
- **Failure modes.** Same per-header failures as warm (see stage 5).
- **Novel.** Exists purely because MTP is undefined on a partially-filled window.
  Most invocations skip it (any batch starting past height 10).

### 5. Warm-Window Validation ⟳
**What happens.** The steady-state path and the bulk of the work. Headers are
processed in **epoch-aligned chunks** (a chunk never straddles a 2016 boundary so
difficulty is constant within it). For each header:
1. **Establish the median.** If the timestamp window is fully ordered, read the
   upper-median slot directly. Otherwise consume one **sparse median hint** and
   *rank-check* it against the 11 timestamps (accept only if it is exactly the
   upper-median rank). See `docs/state-v2.md` and the median discussion in the
   wiki.
2. **MTP rule:** `timestamp <= median` → `TimestampTooOld` (error 3).
3. **Hash** the header via the SP1 SHA-256 precompile
   (`sha256d_new_header`), building the 80-byte header from disjoint `NewHeader`
   fields + authenticated `prev_blockhash`/`nbits` without materializing bytes.
4. **PoW rule:** `SHA256d(header) > target` → `PowInsufficient` (error 2).
5. **Mutate state:** bump height, write the timestamp into its ring slot, set
   `current_nbits`, reset `epoch_start_timestamp` on an epoch boundary.

Chain-work is **not** added per header; the `EpochManager` counts headers in the
chunk and adds `active_work × count` in one deferred `flush` (on chunk end, on
`Drop`, and explicitly before any failure return).

- **Slow / costly.** Dominant cycle consumer. Cost is roughly linear in header
  count and driven by the two double-SHA-256 hashes per header. The sparse-hint
  rank-check adds an 11-element scan only on the unordered path.
- **Failure modes.** `TimestampTooOld` (3), `PowInsufficient` (2); a missing or
  mis-ranked median hint **panics** (a witness/DB bug, not a consensus error).
- **Novel / unique.**
  - Deferred work accumulation: the flush **must** run before returning a failure,
    or `last_valid_state.chain_work` would be understated. This is a load-bearing
    invariant (`EpochManager::flush`/`Drop`).
  - Median hints are *validated by rank*, never trusted — the host supplies them
    only when the guest can't cheaply derive the median, trading a small witness
    for fewer guest cycles.
  - Building the header hash from disjoint fields avoids ever materializing an
    80-byte buffer, saving cycles.

### 6. Epoch Retarget ⟳
**What happens.** Interleaved with stage 5: at the **start** of each chunk,
`EpochManager::new` checks whether the next height crosses a 2016-block boundary.
If so it runs `prepare_new_epoch` → `calculate_next_target_required` (clamp the
epoch timespan to `[¼×, 4×]` expected, rescale the target, cap at
`GENESIS_TARGET`, normalize through compact-bits truncation) and derives the new
`work_from_target`. Otherwise it carries the current difficulty forward. The
loop is therefore: `[retarget?] → process chunk → [retarget?] → process chunk …`
until headers are exhausted or a header fails.

- **Slow / costly.** `work_from_target` is a 257-step binary long division and the
  single most expensive arithmetic routine; it runs once per chunk (once per
  epoch in a large batch), not per header. `GENESIS_WORK` is precomputed to skip
  it for genesis difficulty.
- **Failure modes.** `prepare_new_epoch` expects a valid (non-`u256::MAX`) target;
  an impossible target panics ("invalid blockchain data") rather than returning a
  consensus error — it would mean corrupt input.
- **Novel / unique.** Difficulty/work are computed once per epoch and cached in the
  `EpochManager`, matching the host `sync-headers` optimization (target refreshed
  only at retarget boundaries). Guest and host must agree on this or claims
  diverge.

### 7. Output Commitment
**What happens.** On success: compute the continuation digest of the advanced
state, build `MinimalPublicValues::success`, `commit_slice` the 169 bytes, and
`syscall_halt(0)`. On failure (from stage 4/5/6): after the deferred-work flush,
build `MinimalPublicValues::failure` with the error code and **absolute**
`failure_height`, commit, and halt. Either way the batch produces exactly one
169-byte committed output.

- **Slow / costly.** Two fixed-size SHA-256s (52-byte continuation, and the
  169-byte hashing already done in stage 3 for the predecessor); trivial vs
  stage 5.
- **Failure modes.** None internal — this stage *encodes* failure rather than
  producing it. Malformed output would only arise from a wire-size/layout bug.
- **Novel.** A failed batch still yields a valid proof (a "failure proof")
  committing *that* and *where* validation broke, so a verifier learns the outcome
  without trusting the prover. The committed output is intentionally small (no raw
  tip header) — see the public-boundary design.

---

## Group C — Proof Generation (prover)

Stages 8–9 are SP1 internals; we don't control their sub-steps, but the
vocabulary is useful for reading timings. Both happen inside the single
`prove_compressed` host phase.

### 8. Core Shard Proving
**What happens.** SP1 re-executes the guest to produce an execution trace, splits
it into shards, and generates a STARK proof for each shard.

- **Slow / costly.** The heaviest wall-clock phase and the reason a GPU matters.
  Cost scales with total cycles (⇒ with header count). This is where VRAM/throughput
  dominate.
- **Failure modes.** Out-of-memory on the GPU; excessively large batches. Tune with
  `ZKPOW_BATCH_SIZE`.
- **Novel.** Note the program is executed here **again** (proving re-runs it); the
  earlier standalone `execute_program` is a cheap cross-check, not the trace run.

### 9. Recursive Compression
**What happens.** The shard proofs are recursively aggregated (proofs verifying
proofs) down to a single fixed-size **compressed** STARK proof (~1.3 MB), suitable
for off-chain verification and as the recursive witness for the next batch.

- **Slow / costly.** Significant; grows with shard count. This is the "sharded
  proof aggregation" stage.
- **Failure modes.** Same resource pressures as stage 8.
- **Novel.** The output is what stage 3 of the *next* invocation verifies — the
  chain is only as parallelizable as this recursion allows.

### 10. Proof Verification (host)
**What happens.** The host re-parses the committed public values and compares them
**byte-for-byte** to the expected values computed in stage 1
(`verify_compressed_public_values`), then cryptographically verifies the proof
against the verifying key (`verify_compressed_proof`).

- **Slow / costly.** Verification is cheap relative to proving.
- **Failure modes.** Public-values mismatch (guest computed a different claim than
  the controller expected → hard error, halts the run); a failure-coded result
  (the batch didn't validate). Both stop advancement.
- **Novel.** Double-check by construction: correctness is asserted both by the
  independent host computation and by the cryptographic proof.

### 11. SNARK Wrapping — optional
**What happens.** Only when `ZKPOW_GENERATE_GROTH16=1`. The compressed proof is
`shrink_wrap`-ped, Groth16 artifacts are built, a witness is constructed, and a
gnark-based `Groth16Bn254Prover` produces a ~200-byte BN254 SNARK. The host
asserts the SNARK's public inputs match the program VK hash and the compressed
proof's public-values digest, then natively verifies it.

- **Slow / costly.** "Much slower" per batch (its own artifact build + gnark
  proving); off by default. Typically run only for the batches that go on-chain.
- **Failure modes.** VK-hash or public-values-digest mismatch (asserts); artifact
  build failures.
- **Novel.** This is the only artifact cheap enough to verify on Ethereum
  (~100k gas), consumed by the Solidity relay.

---

## Group D — Finalization (host)

### 12. Persistence & Chaining
**What happens.** The compressed proof is saved to
`proof_height_{first}_to_{end}.bin` (and `_groth16.bin` if produced), then the
resume pointer is recorded (`record_progress`) — **artifact written before
pointer**, so a crash never leaves a dangling reference. The controller advances
its cursor (`complete_work`). In the batch loop, the just-saved compressed proof
becomes the next batch's recursive witness, returning to stage 1.

- **Slow / costly.** Local FS is fast; S3/Spaces upload adds latency.
- **Failure modes.** Disk/S3 write errors. A crash between artifact and pointer is
  safe by design (pointer simply isn't advanced).
- **Novel.** Cross-process resume works because only the pointer + last proof are
  needed to reconstruct the recursive chain state; no zk state is persisted.

---

## Notes on the naming vs. the original sketch

- "Validating starting state" is renamed **Predecessor Verification** — it is
  specifically recursive-proof verification and does **not** run at genesis.
- Cold / Warm / Epoch map directly to the code (`apply_headers` cold branch, warm
  loop, `EpochManager`). Retarget is the *entry* of each warm chunk, so stages 5
  and 6 interleave rather than alternate cleanly.
- "Core STARK proof generation" → **Core Shard Proving**; "sharded proof
  aggregation" → **Recursive Compression**. Both live inside SP1's
  `prove().compressed()`.
- Added the stages the sketch omitted but that occur every run: setup, batch
  preparation, host-side verification, optional SNARK wrapping, and persistence.

Wire formats, exact byte layouts, and error-code numbers are deliberately **not**
restated here (they live in `crates/core/src/public_values.rs` /
`crates/core/src/state.rs` and are still in flux). This doc is about *stages*, not
*formats*.
