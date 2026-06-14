# Guest Program Data Flow

This document is a thorough, text-based map of how information flows **through the
zkpow guest program** (`crates/guest/src/main.rs`) — the constrained code that
executes inside the SP1 zkVM. It traces every input from its on-the-wire byte
layout, through parsing and authentication, into the per-header validation loop,
all the way to the committed public output and the recursive binding that links
one proof to the next.

All type names, field names, wire sizes, and error codes below match the source
of truth in `crates/core` (`lib.rs`, `state.rs`, `input.rs`, `types.rs`) and the
guest entrypoint in `crates/guest`. Where a step corresponds to a
`cycle_track(...)` instrumentation label, that label is given so the diagram
lines up with the profiling output described in `AGENTS.md` and
`docs/cycle-tracking.md`.

> Scope note: the *host* (`crates/host`) loads headers from the database,
> simulates the expected end state, serializes stdin, runs the prover, and
> verifies the committed public values. This document focuses on the **guest**;
> the host appears only at the boundaries (what it writes in, what it checks on
> the way out).

---

## 1. Terminology and wire sizes

These are the canonical types that cross the host→guest boundary or are
constructed inside the guest. Sizes are asserted by
`fixed_width_wire_sizes_match_protocol` in `crates/core/src/lib.rs`.

| Type | Const | Bytes | Role |
| ---- | ----- | ----- | ---- |
| `ProofCarryingState` (PCS) | — | 168 | **Public** input envelope: `Claim ∥ VerifierKeyDigest ∥ Proof` |
| `Claim` | `CLAIM_SIZE` | 100 | Verifier-visible chain claim: `genesis_hash, tip_hash, chain_work, height` |
| `VerifierKeyDigest` | `VK_WIRE_SIZE` | 32 | `[u32; 8]` LE — identifies the program being verified recursively |
| `Proof` | `PROOF_SIZE` | 36 | `public_values_digest (32) ∥ exit_code (1) ∥ _pad (3)` |
| `State` | `STATE_SIZE` | 296 | **Private** witness: full authenticated validation state |
| `NewHeader` | `NEW_HEADER_SIZE` | 44 | **Private** prover-supplied header fields (one per block) |
| `BlockTimestamp` (median hint) | — | 4 | **Private** median-time-past hint (one per block) |
| compressed SP1 proof | — | var | **Private** recursive proof witness (only when `height > 0`) |
| `ContinuationData` | `CONTINUATION_DATA_SIZE` | 116 | Private cached fields, hashed into the continuation digest |
| `MinimalPublicValues` | `MINIMAL_PV_SIZE` | 169 | **Committed output** of the guest |

Other constants referenced below: `WINDOW_SIZE = 11` (median window),
`EPOCH_LENGTH = 2016` (difficulty retarget period), `GENESIS_TARGET` /
`GENESIS_NBITS` (mainnet PoW limit).

`Branded<Tag, T>` (see `brand.rs`) is a zero-cost newtype wrapper. The
underlying types are: `BlockHash = [u8; 32]`, `BlockTimestamp = u32`,
`CompactTarget = u32` (Bitcoin `nBits`), `Target = u256`, `ChainWork = u256`.
A `u256` is four little-endian `u64` limbs. **The whole crate requires a
little-endian target** (`compile_error!` guards this), so every fixed-width
serialization is a direct LE byte copy.

---

## 2. Top-level data flow (host ↔ guest ↔ output)

```text
        HOST (crates/host, unconstrained)                 GUEST (crates/guest, in zkVM)
  ┌───────────────────────────────────────────┐   ┌──────────────────────────────────────────┐
  │ prepare_batch():                           │   │ main():                                    │
  │   • load current_state from DB / prev proof│   │   read 4 (or 5) stdin frames               │
  │   • load NewHeader batch + median hints    │   │   parse + authenticate inputs              │
  │   • simulate expected_state (replay)       │   │   [if height>0] verify prior SP1 proof     │
  │ build_stdin():  write frames ──────────────┼──▶│   apply_headers(): per-block state machine │
  │   [0] ProofCarryingState   (168 B, public) │   │   commit MinimalPublicValues (169 B)       │
  │   [1] State witness        (296 B, private)│   │   syscall_halt(0)                          │
  │   [2] NewHeader[]          (44·N, private) │   └───────────────────┬──────────────────────┘
  │   [3] BlockTimestamp[]     ( 4·N, private) │                       │ committed public values
  │   [4] compressed proof     (var, private)* │                       ▼
  │                                            │   ┌──────────────────────────────────────────┐
  │ verify_public_values():  ◀─────────────────┼───│ MinimalPublicValues (169 B)                │
  │   parse + compare to expected_pv           │   │  genesis, tip, chain_work, height,         │
  └───────────────────────────────────────────┘   │  return_code, failure_height,              │
                                                   │  continuation_digest, verifier_key         │
   * frame [4] present only when claim.height > 0  └──────────────────────────────────────────┘
```

The guest is a **pure function of its stdin**: it reads four length-prefixed
byte vectors (plus, for non-genesis batches, one recursive proof), validates a
contiguous run of Bitcoin headers, and commits exactly 169 bytes. It never
reads the database, the clock, or any ambient state. Anything not in the public
`Claim` is supplied as private witness and must be *authenticated* before use.

---

## 3. The stdin frames (byte-exact input format)

`build_stdin()` (`crates/host/src/pipeline/input.rs`) writes the frames in this
exact order; the guest reads them in the same order with `read_vec()`.

### Frame [0] — `ProofCarryingState` (168 B, **public**)

This is the only *public* input. The verifier of the resulting proof sees it.
It carries the claim being extended, the verifier key for recursion, and the
authentication data for the prior proof.

```text
ProofCarryingState  (168 bytes)              parsed by ProofCarryingState::parse → split_pcs_wire
┌──────── Claim (100 B) ────────┬─ VK (32 B) ─┬──────── Proof (36 B) ────────┐
│  0   genesis_hash    [u8;32]  │ 100 verifier│ 132 public_values_digest 32B │
│ 32   tip_hash        [u8;32]  │     _key    │ 164 exit_code             1B │
│ 64   chain_work    u256 LE 32 │  [u32;8] LE │ 165 _pad                  3B │
│ 96   height          u32 LE   │  32 bytes   │                              │
└───────────────────────────────┴─────────────┴──────────────────────────────┘
```

### Frame [1] — `State` witness (296 B, **private**)

The full authenticated state. Its *public* half (`genesis_hash`, `block_hash`,
`chain_work`, `height`) must equal `Claim`; its *private* half is the cached
difficulty/timestamp data needed to validate the next batch. Serialized as a
raw `repr(C)` memory image (LE target), so offsets are struct field offsets:

```text
State  (296 bytes, repr(C), 8-byte aligned)        parsed by State::parse (copy_from_bytes)
┌─────────────────────────────── Header (80 B) ───────────────────────────────┐
│   0  version           u32        (tip header, raw Bitcoin 80-byte layout)   │
│   4  prev_blockhash    [u8;32]                                               │
│  36  merkle_root       [u8;32]                                               │
│  68  timestamp         u32                                                   │
│  72  compact_target    u32  (nBits)                                          │
│  76  nonce             u32                                                   │
├──────────────────────────────────────────────────────────────────────────────┤
│  80  block_hash        [u8;32]    ── public (= Claim.tip_hash)               │
│ 112  genesis_hash      [u8;32]    ── public (= Claim.genesis_hash)           │
│ 144  current_nbits     u32        ── private (cached difficulty, nBits)      │
│ 148  height            u32        ── public (= Claim.height)                 │
│ 152  chain_work        u256 LE 32 ── public (= Claim.chain_work)             │
│ 184  current_work      u256 LE 32 ── private (per-block work this epoch)     │
│ 216  current_target    u256 LE 32 ── private (expanded target this epoch)    │
│ 248  epoch_start_timestamp  u32   ── private (first timestamp of epoch)      │
│ 252  timestamps        [u32;11]   ── private (median-time-past ring buffer)  │
└──────────────────────────────────────────────────────────────────────────────┘
                                       │
            ContinuationData (116 B) = │ current_nbits, current_work, current_target,
            the private subset hashed  │ epoch_start_timestamp, timestamps[11]
            into continuation_digest   ▼
```

### Frame [2] — `NewHeader[]` (44·N B, **private**)

The prover-supplied fields for each new block, packed back to back. The guest
does **not** trust `prev_blockhash` or `compact_target` from the prover; it
*reconstructs* the full header from authenticated state (see §6, header
materialization). `parse_new_headers` requires the length be a multiple of 44.

```text
NewHeader  (44 bytes each)        repeated N times (N = batch size)
┌───────────────────────────────────────────────────────────┐
│  0  version      u32                                        │
│  4  merkle_root  [u8;32]                                    │
│ 36  timestamp    u32                                        │
│ 40  nonce        u32                                        │
└───────────────────────────────────────────────────────────┘
  [ NewHeader_0 | NewHeader_1 | ... | NewHeader_{N-1} ]
```

### Frame [3] — `BlockTimestamp[]` median hints (4·N B, **private**)

One claimed median-time-past per header, in the same order. The guest validates
each hint is the correct rank statistic of the current 11-slot window before
trusting it (see §6, median check). `parse_median_hints` requires the count to
equal the number of headers (`expected_count = header_hints.len()`).

```text
[ median_0 (u32 LE) | median_1 | ... | median_{N-1} ]    length must equal N
```

### Frame [4] — compressed SP1 proof (variable, **private**, conditional)

Present **only when `claim.height > 0`** (a non-genesis batch). Written by
`stdin.write_proof(inner_proof, vk)`. The guest consumes it implicitly through
the `verify_sp1_proof` syscall during recursive verification (§5). For a genesis
batch (`height == 0`) this frame is absent and recursion is skipped.

---

## 4. Guest `main()` — top-level control flow

```text
                         sp1_zkvm::entrypoint!(main)
                                   │
   cycle_track("main")            ▼
   ┌─────────────────────────────────────────────────────────────────────────┐
   │ READ STDIN (order matters)                                               │
   │   input_bytes        = io::read_vec()   ← frame [0] ProofCarryingState   │
   │   state_bytes        = io::read_vec()   ← frame [1] State witness        │
   │   header_hint_bytes  = io::read_vec()   ← frame [2] NewHeader[]          │
   │   median_hint_bytes  = io::read_vec()   ← frame [3] BlockTimestamp[]     │
   └─────────────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
   ┌─────────────────────────────────────────────────────────────────────────┐
   │ PARSE + AUTHENTICATE                                                     │
   │   pcs   = ProofCarryingState::parse(input_bytes)   "input/parse"         │
   │   state = State::parse(state_bytes)                "input/parse_state_…" │
   │   ── bind witness to claim ──  "input/verify_state_claim"                │
   │      assert state.genesis_hash == pcs.claim.genesis_hash                 │
   │          && state.block_hash  == pcs.claim.tip_hash                      │
   │          && state.chain_work  == pcs.claim.chain_work                    │
   │          && state.height      == pcs.claim.height                        │
   │   header_hints = parse_new_headers(header_hint_bytes)                    │
   │   median_hints = parse_median_hints(median_hint_bytes, header_hints.len)│
   └─────────────────────────────────────────────────────────────────────────┘
                                   │
                  pcs.claim.height > 0 ?
                   ┌───────────────┴────────────────┐
                 yes (recursive)                   no (genesis anchor)
                   │                                 │
                   ▼                                 │
   ┌──────────────────────────────────────┐         │
   │ verify_prior_proof()  "proof/verify"  │         │
   │  (see §5 — recursive binding)         │         │
   └──────────────────────────────────────┘         │
                   └───────────────┬─────────────────┘
                                   ▼
   ┌─────────────────────────────────────────────────────────────────────────┐
   │ state.apply_headers(header_hints, median_hints, hash_header)            │
   │                                 (see §6 — the per-header state machine)  │
   └─────────────────────────────────────────────────────────────────────────┘
              Err(ApplyFailure)            │            Ok(())
                   │                       │              │
                   ▼                       │              ▼
   ┌───────────────────────────────┐      │   ┌──────────────────────────────────┐
   │ digest = continuation_digest( │      │   │ digest = continuation_digest(    │
   │           failure.last_valid_ │      │   │           state )                 │
   │           state )             │      │   │ pv = MinimalPublicValues::success(│
   │ pv = MinimalPublicValues::    │      │   │        state.public_claim(),      │
   │      failure(last_valid_claim,│      │   │        digest, pcs.verifier_key)  │
   │      error_code, failure_     │      │   └──────────────────────────────────┘
   │      height, digest, vk)      │      │              │
   └───────────────────────────────┘      │              │
                   └───────────────────────┴──────────────┘
                                   ▼
   ┌─────────────────────────────────────────────────────────────────────────┐
   │ commit_minimal_pv(pv):  io::commit_slice(pv.to_bytes())  +  halt(0)      │
   └─────────────────────────────────────────────────────────────────────────┘
```

Key invariant: the guest **always halts with exit code 0** and **always commits
169 bytes**. Validation *failures* are not zkVM errors — they are committed as a
nonzero `return_code` inside the public values. The proof still proves "this is
what happened." Only malformed inputs (lengths that don't parse, a hint that is
not the true median, a rejected recursion) cause a `panic!`, which aborts
proving entirely and produces no proof.

---

## 5. Recursive proof verification (when `height > 0`)

For any batch that extends a prior proof, the guest must prove that the `State`
witness it is about to trust is exactly the state the previous proof ended in.
The public `Claim` covers `genesis/tip/chain_work/height`, but the *private*
cached fields (`current_work`, `current_target`, `current_nbits`,
`epoch_start_timestamp`, `timestamps`) are not in the claim. They are bound
instead through the **continuation digest**.

`verify_prior_proof()` (`crates/guest/src/main.rs`), label `proof/verify`:

```text
   prior_continuation_bytes = state.continuation_bytes()   ← 116 B from THIS witness
                                   │
   ┌───────────────────────────────────────────────────────────────────────┐
   │ 0. if pcs.proof.exit_code != 0  →  panic("continuation rejected")      │
   │       (a prior FAILURE proof can never be extended)                    │
   │                                                                        │
   │ 1. continuation_digest = sha256_116bytes(prior_continuation_bytes)     │
   │       "proof/continuation_digest"                                      │
   │                                                                        │
   │ 2. reconstruct the prior proof's public values:                       │
   │      prior_pv = MinimalPublicValues::success(                         │
   │                   pcs.claim,            ← public, already trusted      │
   │                   continuation_digest,  ← recomputed from witness      │
   │                   pcs.verifier_key)                                    │
   │      actual_pv_hash = sha256_169bytes(prior_pv.to_bytes())            │
   │       "proof/public_values_digest"                                    │
   │                                                                        │
   │ 3. if actual_pv_hash != pcs.proof.public_values_digest → panic         │
   │                                                                        │
   │ 4. verify_sp1_proof(verifier_key, public_values_digest)               │
   │       "proof/sp1_verify"  (consumes the compressed proof, frame [4])  │
   └───────────────────────────────────────────────────────────────────────┘
```

Why this is sound: the prior proof committed
`public_values_digest = SHA256(prior MinimalPublicValues)`, and those public
values embed `continuation_digest = SHA256(ContinuationData)`. The current guest
recomputes both hashes from the witness it was handed. If the prover tampered
with *any* private cached field (say, inflating `current_work`), the recomputed
`continuation_digest` changes, the reconstructed `prior_pv` hash changes, and
step 3 rejects. Thus the entire 296-byte `State` is authenticated even though
only 100 bytes of it (`Claim`) are public.

```text
   proof N           public values N            continuation N
   ┌──────┐ commits  ┌───────────────┐ embeds   ┌──────────────┐
   │ SP1  │────────▶ │ MinimalPublic │────────▶ │ SHA256(Cont- │
   │proof │ pv_digest│ Values (169B) │  cont.   │ inuationData)│
   └──────┘          └───────────────┘  digest  └──────────────┘
       ▲                                              ▲
       │ verify_sp1_proof                             │ recomputed by guest N+1
       │                                              │ from State witness N+1
   batch N+1 ─────────────────────────────────────────┘
   (must reproduce both digests exactly, or panic)
```

---

## 6. `apply_headers` — the per-header state machine

This is the heart of the guest: `State::apply_headers` in
`crates/core/src/state.rs`, label `state/apply_headers`. It walks the header
batch and median hints in lockstep (`headers.len() == median_hints.len()` is
asserted), mutating `self` (the `State`) in place. On the first invalid header it
returns `Err(ApplyFailure)` carrying the **last valid** state; otherwise it
returns `Ok(())` with `self` advanced to the new tip.

```text
 for (header_index, (new_header, claimed_median)) in zip(headers, median_hints):
 ┌────────────────────────────────────────────────────────────────────────────┐
 │ A. candidate_height   = self.height + 1            "…/candidate_height"      │
 │    previous_timestamp = self.timestamps[height % 11] "…/load_previous_ts"    │
 │    timestamp_slot     = candidate_height % 11                                │
 ├────────────────────────────────────────────────────────────────────────────┤
 │ B. MEDIAN HINT CHECK   "…/median_hint_check"                                 │
 │      assert median_time_past_hinted(claimed_median)                         │
 │      └─ proves claimed_median is the true upper-median of the current window│
 │         (rank check, see §6.1).  Bad hint → panic.                          │
 ├────────────────────────────────────────────────────────────────────────────┤
 │ C. MEDIAN-TIME-PAST RULE  "…/validate/median_time_past"                      │
 │      if new_header.timestamp <= claimed_median:                             │
 │          flush pending chain work                                           │
 │          return Err(TimestampTooOld, failure_height = candidate_height)     │
 ├────────────────────────────────────────────────────────────────────────────┤
 │ D. DIFFICULTY RETARGET?   "…/check_retarget"                                 │
 │      active_nbits/target/work = current_*  (default: carry the epoch)       │
 │      if candidate_height % 2016 == 0:                                        │
 │          flush pending chain work                                           │
 │          (active_nbits, active_target, active_work) =                       │
 │                 prepare_new_epoch(previous_timestamp)   (see §6.2)          │
 ├────────────────────────────────────────────────────────────────────────────┤
 │ E. MATERIALIZE HEADER     "…/build_header"                                   │
 │      header = new_header.into_header(                                       │
 │                 prev_blockhash = self.block_hash,   ← authenticated link    │
 │                 compact_target = active_nbits)      ← authenticated nBits   │
 ├────────────────────────────────────────────────────────────────────────────┤
 │ F. HASH                  "…/hash_header" → "crypto/hash_header"              │
 │      block_hash = SHA256d(header.to_bytes())   (see §7)                     │
 ├────────────────────────────────────────────────────────────────────────────┤
 │ G. PROOF-OF-WORK RULE    "…/validate/pow" → "pow/check_proof_of_work"        │
 │      if u256(block_hash) > active_target:                                    │
 │          flush pending chain work                                           │
 │          return Err(PowInsufficient, failure_height = candidate_height)     │
 ├────────────────────────────────────────────────────────────────────────────┤
 │ H. CHAIN-WORK RUN ACCUMULATION  (run-length optimization, see §6.3)         │
 │      if pending_run_work != Some(active_work):                              │
 │          flush pending chain work; start new run at active_work            │
 │      pending_run_count += 1                                                 │
 ├────────────────────────────────────────────────────────────────────────────┤
 │ I. COMMIT THIS HEADER TO STATE   "…/assign_state"                           │
 │      self.height            = candidate_height                              │
 │      self.timestamps[slot]  = header.timestamp                             │
 │      self.current_nbits     = active_nbits                                  │
 │      self.current_target    = active_target                                │
 │      self.current_work      = active_work                                  │
 │      if candidate_height % 2016 == 0:                                       │
 │          self.epoch_start_timestamp = header.timestamp                     │
 │      self.header            = header                                        │
 │      self.block_hash        = block_hash                                   │
 └────────────────────────────────────────────────────────────────────────────┘
 after the loop:  flush pending chain work  →  Ok(())
```

Ordering matters: every early `return Err(...)` first **flushes** any deferred
chain work, so the `last_valid_state` carried in `ApplyFailure` already reflects
all work up to (but not including) the failing header. `chain_work` is therefore
correct in both the success and failure public values.

### 6.1 Median-time-past hint validation (`median_time_past_hinted`)

Computing a true median requires sorting, which is expensive in-circuit. Instead
the prover *claims* the median and the guest cheaply verifies the claim is the
correct **upper median** by rank. Label `state/median_time_past_hinted`.

```text
 window_len = min(height + 1, 11)        (timestamp_count())
 if window_len == 0: accept (genesis has no history)
 median_index = window_len / 2

 loop over timestamps[0..window_len]  "…/loop":
     count how many are  < / == / > claimed_median  →  (less, equal, greater)

 accept iff  "…/check_counts":
     less + equal + greater == window_len           (all accounted for)
   && less <= median_index                          (not too many below)
   && less + equal > median_index                   (claimed value reaches rank)
```

This accepts the value that sits at rank `median_index` (the upper median for
even-sized windows), and correctly handles duplicate timestamps. The matching
host-side reference implementation `median_time_past()` (in the core test
module) sorts and indexes `[count/2]`, and the tests assert the two agree across
window-wrap boundaries.

### 6.2 Difficulty retarget (`prepare_new_epoch` → `calculate_next_target_required`)

Triggered when `candidate_height` is a multiple of `EPOCH_LENGTH = 2016`. Label
`state/prepare_new_epoch`.

```text
 actual_timespan  = previous_timestamp - epoch_start_timestamp      (i64)
 clamped_timespan = clamp(actual_timespan, EXPECTED/4, EXPECTED*4)   (Bitcoin rule)
                    EXPECTED = 2016 * 600 s
 new_target_raw   = old_target * clamped_timespan / EXPECTED         (u256 long arithmetic)
 clamped_target   = min(new_target_raw, GENESIS_TARGET)              (difficulty floor)
 new_nbits        = target_to_bits(clamped_target)                   (compact encoding)
 new_target       = target_from_bits(new_nbits)                      (re-expand → truncation)
 new_work         = work_from_target(new_target)                     (floor(2^256/(target+1)))
        ▼
 returns (active_nbits, active_target, active_work) used for THIS block onward
```

The target is re-expanded from its compact `nBits` form so the in-circuit value
matches the truncation Bitcoin applies on the wire. `work_from_target` uses
non-restoring binary long division (label `pow/work_from_target`) to compute the
per-block cumulative-work increment.

### 6.3 Chain-work run-length accumulation

Within a single difficulty epoch every block contributes the *same*
`current_work`. Rather than do a 256-bit add per block, the loop counts a "run"
of identical per-block work and applies it as one multiply-then-add when the run
ends (epoch change, failure, or end of batch). Helper `apply_chain_work_run`,
label `state/apply_headers/chain_work_flush`:

```text
 run starts:   pending_run_work = Some(active_work), pending_run_count = 0
 each header:  pending_run_count += 1
 flush:        chain_work += pending_run_work * pending_run_count   (u256 mul_u32 + add)
               pending_run_count = 0
```

A flush happens whenever `active_work` changes (a retarget), before any error
return, and once more after the loop. The batched result is asserted equal to
the naive per-block accumulation by the core tests
(`apply_headers_flushes_deferred_chain_work_*`).

### 6.4 Header materialization (`NewHeader::into_header`)

The prover only supplies the four non-deterministic fields (`version`,
`merkle_root`, `timestamp`, `nonce`). The two consensus-critical fields are
**injected from authenticated state**, which is how chain linkage and the
correct difficulty are enforced *without* a dedicated error code:

```text
   NewHeader (prover)                 authenticated state            full Header (80 B)
   ┌──────────────┐                   ┌────────────────┐            ┌──────────────────┐
   │ version      │──────────────────▶│                │            │ version          │
   │              │   self.block_hash │ prev_blockhash │───────────▶│ prev_blockhash   │ ← chain link
   │ merkle_root  │──────────────────▶│                │            │ merkle_root      │
   │ timestamp    │──────────────────▶│                │            │ timestamp        │
   │              │   active_nbits     │ compact_target │───────────▶│ compact_target   │ ← difficulty
   │ nonce        │──────────────────▶│                │            │ nonce            │
   └──────────────┘                   └────────────────┘            └──────────────────┘
```

If the prover lies about which block follows which, the injected
`prev_blockhash` won't match the real chain, the `SHA256d` will differ, and
either PoW fails (G) or — for a recursive batch — the resulting tip won't extend
into a verifiable next proof. There is no separate "prev-blockhash mismatch" or
"bits mismatch" error code by design; these are *structurally* impossible to
forge because they are not prover inputs.

---

## 7. SHA-256 inside the guest (`crates/guest/src/sha256.rs`)

The guest calls SP1 SHA-256 precompile syscalls directly
(`syscall_sha256_extend`, `syscall_sha256_compress`) — no `sha2` crate. Each
function is hardcoded for one exact input size, so there are no loops or
branches on block count. Padding and the trailing 64-bit big-endian length are
baked in per size.

| Function | Input | Blocks | Used for |
| -------- | ----- | ------ | -------- |
| `sha256_80bytes` | 80 B | 2 | first half of header double-hash |
| `sha256_32bytes` | 32 B | 1 | second half of header double-hash |
| `sha256d_80bytes` | 80 B | 2+1 | `SHA256(SHA256(header))` → `block_hash` (label `crypto/hash_header`) |
| `sha256_116bytes` | 116 B | 2 | `ContinuationData` → continuation digest |
| `sha256_169bytes` | 169 B | 3 | `MinimalPublicValues` → public-values digest (recursion) |

```text
   header (80 B) ──sha256_80bytes──▶ digest (32 B) ──sha256_32bytes──▶ block_hash (32 B)
                            (this pair = sha256d_80bytes)
```

---

## 8. Output — `MinimalPublicValues` (169 B committed)

Built by `MinimalPublicValues::success(...)` or `::failure(...)` and committed
with `io::commit_slice`, then `syscall_halt(0)`. Serialized manually (no struct
padding):

```text
MinimalPublicValues  (169 bytes)              io::commit_slice → proof public values
┌──────────────────────────────────────────────────────────────────────────────┐
│   0  genesis_hash         [u8;32]   ← from public_claim                        │
│  32  tip_hash             [u8;32]   ← last valid tip (success: new tip)        │
│  64  chain_work           u256 LE 32                                           │
│  96  height               u32 LE    ← last valid height                        │
│ 100  return_code          u8        0 = success, else ValidationErrorCode      │
│ 101  failure_height       u32 LE    0 on success; absolute height of bad block │
│ 105  continuation_digest  [u8;32]   SHA256(ContinuationData of committed state)│
│ 137  verifier_key         [u8;32]   [u32;8] LE (= pcs.verifier_key)            │
└──────────────────────────────────────────────────────────────────────────────┘
```

- **Success**: `tip_hash/chain_work/height` describe the newly extended tip;
  `return_code = 0`, `failure_height = 0`.
- **Failure**: the fields describe the **last valid** state (everything before
  the bad header); `return_code` is the error code and `failure_height` is the
  absolute chain height of the offending block (`last_valid_height + 1`). The
  proof still verifies — it proves "the chain is valid up to here, and the next
  header is invalid for this reason."

The `continuation_digest` is what the *next* batch will recompute and check in
§5, closing the recursive loop.

> Note on the median window: `ContinuationData` does not store a separate
> "median count." Per `AGENTS.md`, the count is derivable as
> `min(11, total_validated - 1)` (0 when nothing validated) — matching
> `State::timestamp_count()`.

---

## 9. Error and failure taxonomy

There are two distinct failure modes with very different consequences:

```text
 ┌───────────────────────────────────────────────────────────────────────────┐
 │ (a) PANIC / abort — no proof is produced                                   │
 │     • input frame fails to parse (length not a multiple / wrong count)     │
 │     • State witness public half != Claim   ("input/verify_state_claim")    │
 │     • median hint is not the true median   ("…/median_hint_check")         │
 │     • recursion rejected (exit_code != 0, digest mismatch, bad SP1 proof)  │
 │     → these are prover errors / forgery attempts; proving simply fails.    │
 ├───────────────────────────────────────────────────────────────────────────┤
 │ (b) VALIDATION FAILURE — committed as nonzero return_code, halt(0)         │
 │     • the proof IS produced and verifies; it attests the failure.          │
 └───────────────────────────────────────────────────────────────────────────┘
```

`ValidationErrorCode` (`crates/core/src/types.rs`):

| Code | Name | How it arises |
| ---- | ---- | ------------- |
| 1 | `HeaderPayloadLengthInvalid` | Reserved; malformed input length is currently surfaced as a parse **panic** (a), not a committed code. |
| 2 | `PowInsufficient` | `SHA256d(header) > active_target` — step G. Committed (b). |
| 3 | `TimestampTooOld` | `header.timestamp <= claimed_median` — step C. Committed (b). |
| 4 | `GenesisHashMismatch` | Reserved. The height-0 state is a **trusted anchor** (ADR-0001/0002): genesis is not re-checked at runtime, so this code is not emitted by `apply_headers`. |

Prev-blockhash and `nBits` mismatches have **no error code**: those fields are
injected from authenticated state in step E, so a wrong link can only manifest
as a PoW failure (code 2) or a non-extendable tip — never as forged-but-accepted
data.

---

## 10. Recursive chaining across batches (the big picture)

Each guest run extends a `Claim` by one batch of headers and emits a new
`Claim` + `continuation_digest` inside its public values. The host feeds proof N
as frame [4] and the matching `State` as frame [1] into run N+1. Genesis (run 0)
has `height == 0`, so it skips §5 and is anchored directly on the trusted
`State`.

```text
   genesis State (height 0, trusted anchor)
        │   frames [0..3]            (no frame [4]; height == 0)
        ▼
   ┌──────────┐  PV_0 (tip@H1, cont_digest_0)
   │ guest #0 │ ───────────────────────────────┐
   └──────────┘                                 │
        State@H1 + proof_0 + headers H1..H2 ────┤ frames [0..4]
        ▼                                       │
   ┌──────────┐  PV_1 (tip@H2, cont_digest_1)   │ §5 binds State@H1 to PV_0
   │ guest #1 │ ───────────────────────────────┤   via cont_digest_0
   └──────────┘                                 │
        State@H2 + proof_1 + headers H2..H3 ────┘ frames [0..4]
        ▼
   ┌──────────┐  PV_2 ...
   │ guest #2 │
   └──────────┘
```

The chain of trust is: each proof verifies the previous proof *and* re-derives
the previous continuation digest from the witness it was handed, so a single
final proof transitively attests every header from the trusted anchor to the
current tip — while only the 169-byte public values ever need to be checked
on-chain.

---

## 11. Where to look in the code

| Concern | File / symbol |
| ------- | ------------- |
| Guest entrypoint, stdin reads, commit, halt | `crates/guest/src/main.rs` (`main`) |
| Recursive proof verification | `crates/guest/src/main.rs` (`verify_prior_proof`) |
| SHA-256 precompile wrappers | `crates/guest/src/sha256.rs` |
| Per-header state machine | `crates/core/src/state.rs` (`State::apply_headers`) |
| Median hint validation | `crates/core/src/state.rs` (`median_time_past_hinted`) |
| Difficulty retarget math | `crates/core/src/lib.rs` (`calculate_next_target_required`, `work_from_target`) |
| Input parsing / wire split | `crates/core/src/input.rs` |
| Type layouts & wire sizes | `crates/core/src/lib.rs`, `crates/core/src/types.rs` |
| Host stdin construction | `crates/host/src/pipeline/input.rs` (`build_stdin`, `build_proof`) |
| Host PV verification | `crates/host/src/pipeline/execution.rs` (`verify_public_values`) |
| Related diagrams | `docs/diagrams/01_genesis_trusted_anchor.d2`, `02_state_claim_continuation_binding.d2` |
```
