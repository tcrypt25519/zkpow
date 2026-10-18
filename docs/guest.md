# Inside the Guest

`docs/overview.md` explains what zkpow is and how proofs chain. This document is about the part
that actually gets proven: the `zkpow-guest` binary and everything its `main` touches. That's
`crates/guest`, the state machine in `crates/core`, the hashing in `crates/sha256`, and the
support crates underneath them (`mtp`, `params`, `types`). The host and controller only show up
where the guest's input or output format forces them to.

Read it if you're going to change any of those crates, or if you're reviewing a change to them
and need to know which invariants a diff can break.

## Why the guest is small

Every instruction the guest executes costs prover time, and the guest runs once per batch of
2016 headers. So the design pushes as much as it can out of the proven program and into two
places that cost nothing to prove:

- The host, which precomputes anything the guest can *check* more cheaply than it can
  *compute*. Those are the hints: median timestamps and block work quotients.
- The type layer, which makes the wire formats plain-old-data so that reading input is a cast
  rather than a parse.

What's left is about a hundred lines in `crates/guest/src/main.rs` and a state transition in
`crates/core/src/state.rs` that does three things per header: a comparison, a hash, and another
comparison.

The constraint that shapes all of it: the guest can't trust anything the prover gives it, but it
can't afford to recompute everything either. Each hint below comes with the check that makes it
safe.

## Crate map

| Crate | Target | Role |
| --- | --- | --- |
| `zkpow-guest` | zkVM only | the `main`. Reads inputs, verifies the prior proof, applies headers, commits |
| `zkpow-core` | both | `State`, `HeaderInput`, `PublicValues`, the consensus math, the hint halt macros |
| `zkpow-sha256` | both | SHA-256 over exactly two message layouts, on the precompile in the VM |
| `zkpow-mtp` | both | the 11-slot timestamp ring and the median rank check |
| `zkpow-params` | both | `Consensus` trait and the `Mainnet` constants |
| `zkpow-types` | both | `u256`, `NonMaxU256`, the branded newtypes, `Header` |
| `sp1-cycle` | both | `#[tracked]` and `track!`, cycle-tracker spans that compile away off-target |
| `sha256_const` | both | `const fn` SHA-256, only for the genesis hash at compile time |

"Both" means the crate is `#![no_std]` and compiles for the host too. Everything that touches a
syscall has a `cfg(target_os = "zkvm")` branch and a plain Rust fallback. That's not a
convenience. The controller uses the fallback to compute the state it expects the guest to
commit, so the two branches have to produce identical results. A change to one side without the
other is a bug even if every test passes on the host.

## The input protocol

The host writes four buffers and optionally one proof. The guest reads them in the same order:

| # | Written by host | Read in guest as | Size |
| --- | --- | --- | --- |
| 1 | verifier key digest, 32 bytes | `&VerifierKeyDigest` (`[u32; 8]`) | 32 |
| 2 | `State::to_bytes()` | `&mut State` | 152 |
| 3 | header inputs, concatenated | `&[HeaderInput]` | 44 × n |
| 4 | median hints, concatenated | `&[BlockTimestamp]` | 4 × m |
| 5 | previous compressed proof, if `height > 0` | never read directly | |

Buffers 1 through 4 go through `read_vec_raw`, which hands back a pointer into SP1's reserved
input region. The guest never copies them. `read_fixed_size_input` and
`read_slice_of_fixed_size_len` in `main.rs` wrap the pointer in a slice and hand it to
`bytemuck`, which checks three things: the length is exactly `size_of::<T>()` (or a multiple
of it for slices), the pointer is aligned for `T`, and nothing else. Every input type is `Pod`
and `repr(C)`, so there are no invalid bit patterns to reject.

Alignment works because SP1 rounds every input buffer up to 8 bytes and places them back to
back in the input region, so each one starts 8-aligned. `State` needs 8 (it holds `u256`
limbs). `HeaderInput` needs 4.

The `State` read is `&mut`. The guest mutates the input buffer in place and commits it at the
end. There's no separate output struct.

Buffer 5 isn't in the input stream at all. `write_proof` puts it in SP1's proof stream, which
only the `verify_sp1_proof` syscall consumes. The guest never sees proof bytes.

If any cast fails, `inner_main` returns a `ProgramError` and `main` panics on it. In SP1 a
panic is exit code 1. The same thing happens if a buffer is missing: `read_vec_raw` returns a
null pointer, the slice helper maps that to a cast error, and the program panics. That's a
prover setup problem, not a consensus result, and no public values get committed.

### `HeaderInput` is not a header

A Bitcoin header is 80 bytes. The guest gets 44:

```rust
#[repr(C)]
pub struct HeaderInput {
    pub version:     u32,       // 0..4
    pub merkle_root: [u8; 32],  // 4..36
    pub timestamp:   BlockTimestamp, // 36..40
    pub nonce:       u32,       // 40..44
}
```

`prev_blockhash` and `nbits` are missing on purpose. The guest fills them in from its own
state: the hash of the header it just accepted and the compact target for the active epoch. So
the prover doesn't get to claim a parent or a difficulty. If it hands over a header that was
mined on a different parent or at a different difficulty, the guest hashes it with *its* values
and the result fails the target check. There is no separate linkage check because the hash is
the linkage check.

## What `main` does

```rust
let verifier_key: &VerifierKeyDigest = read_fixed_size_input()?;
let state: &mut State = read_fixed_size_input_mut()?;
let headers: &[HeaderInput] = read_slice_of_fixed_size_len()?;
let medians: &[BlockTimestamp] = read_slice_of_fixed_size_len()?;

let apply_result = if state.height == 0 {
    state.apply_headers_from_genesis::<Mainnet>(headers, medians)
} else {
    let actual_pv_hash = zkpow_sha256::of_public_values(
        bytemuck::bytes_of(state),
        verifier_key.to_le_bytes_slice(),
        ReturnCode::SUCCESS.into_u8(),
    );
    verify_sp1_proof(verifier_key.as_raw(), &actual_pv_hash);
    state.apply_headers::<Mainnet>(headers, medians)
};

let return_code = match apply_result {
    Ok(()) => ReturnCode::SUCCESS,
    Err(err) => err.return_code(),
};
commit_public_values(&state, verifier_key, return_code);
```

That's the whole program. Four reads, a branch on whether this is the first batch, the
transition, and a commit. `commit_public_values` writes 185 bytes to the public values stream
and calls `syscall_halt(0)`, so it never returns.

The network is fixed to `Mainnet` at the call site. `zkpow-core` is generic over `C: Consensus`
and every parameter is an associated const, so the generic folds to literals. There's no runtime
network selection and no way to get a testnet proof out of this binary.

### Verifying the previous proof

The recursion is the one place the guest talks to SP1 about something other than I/O. It works
by reconstruction rather than by reading the previous proof's public values:

- Take the input `State`, the input verifier key digest and the byte `0`.
- Hash those 185 bytes with `of_public_values`.
- Call `verify_sp1_proof(vk_digest, hash)`.

SP1 then checks that the proof stream contains a proof under `vk_digest` whose committed public
values hash to exactly `hash`. In execution that's a sanity check by the host-side verifier. In
proving, it becomes a constraint in the recursion layer. Either way the guest doesn't need to
parse anything.

Two consequences fall out of the byte `0`:

- The input state can't be forged. Any changed byte in `State` changes the hash and the
  deferred proof won't match.
- A failure proof can't be extended. A previous proof that committed return code 2 hashes
  differently from the reconstruction with 0, so verification fails.

The verifier key is an *input*. The guest has no way to know its own verifying key, so the host
passes the digest in and the guest commits it unchanged. That means a prover could pass a
foreign program's key and a proof from that program, and the recursion check would pass. What
stops that is the consumer: the final proof's committed key must equal the real zkpow guest
key. A chain started under a foreign key commits that foreign key forever, because every step
copies the input key into its output. So the consumer's one key comparison covers every batch
back to genesis.

At `height == 0` nothing is verified. The genesis `State` is the trusted anchor, built by the
controller from the genesis header with one timestamp in the ring.

## `State`

```text
offset  size  field
     0    32  chain_work       u256, little-endian limbs
    32    32  genesis_hash
    64    32  block_hash       hash of the tip header
    96     4  height
   100     4  current_nbits    compact target for the current epoch
   104     4  epoch_timestamp  timestamp of the epoch's first block
   108    44  timestamps       [u32; 11], slot = height % 11
```

152 bytes, `repr(C)`, no padding, `Pod`. `State::to_bytes` is a `bytemuck` cast. The host
serializes it, the guest reads it, mutates it, and commits the same bytes back. The layout is
the wire format, which is why there's a `compile_error!` on big-endian targets.

The first four fields are the claim: this much work, on this network, ending at this hash and
height. The last three are what the next batch needs to keep validating without any other
context: the active difficulty, where the epoch started (for the next retarget), and the
timestamps for the median rule. All of it is public. That's what keeps the recursion to a
single hash comparison.

## Applying headers

The transition has two entry points. `apply_headers_from_genesis` runs once per chain, when
`height == 0`. `apply_headers` runs for every batch after that. Both end up in `seal_header`
for each header, which is the only function that rejects one.

### `seal_header`

```rust
if new_header.timestamp <= known_median {
    return Err(ApplyError::TimestampTooOld { .. });
}
let next_hash_words = hash_header_input(new_header, hash_words, active_nbits);
if !check_proof_of_work_words(&next_hash_words, active_target) {
    return Err(ApplyError::PowInsufficient { .. });
}
self.advance_state::<C>(new_header.timestamp);
*hash_words = next_hash_words;
```

The timestamp check comes first because it's a single compare and skips the hash when it
fails. Then the hash, then the target compare, then the state advance. `advance_state` bumps
`height` and, if the new height is a multiple of 2016, records the timestamp as the new
`epoch_timestamp`. It can't fail.

The hash is kept as `[u64; 8]` SHA state words between headers, not as 32 bytes. The previous
hash feeds the next header's first block directly, so there's no pack-and-unpack per header.
`State.block_hash` only gets written at the end of a batch or on failure.

### Cold start

The median rule wants 11 timestamps and genesis provides one. `apply_headers_from_genesis`
splits the batch at 10 headers. The first 10 go through `apply_cold_batch`, which:

- builds a `Window<Cold>` from the one genesis timestamp,
- validates each header against the genesis compact target and the genesis work, no retarget
  logic at all,
- takes the median from the ring's ordered fast path or, if the partial window isn't ordered,
  by sorting a copy,
- after 10 pushes converts the ring to `Window<Warm>` and hands it to the warm path for
  whatever headers remain.

Fewer than 10 headers in the first batch is `ApplyError::InsufficientColdHeaders`, error code
5. The sort is fine here because it runs at most 10 times in the life of a chain.

### Warm batches and epoch segments

`apply_warm_batch` walks the batch in segments. A segment ends at a difficulty boundary, so
within a batch of 2016 headers there are at most two. At the start of each segment
`new_epoch_manager` decides what the active difficulty is:

- If the *next* height is a multiple of 2016, compute the retarget from `current_nbits`,
  `epoch_timestamp` and the newest timestamp in the ring.
- Otherwise expand `current_nbits` and compute the block work for it.

Either way that gives `(active_nbits, active_target, active_block_work)` once per segment,
and every header in the segment reuses them. The target expansion and the block work
verification are the expensive parts and they run once per 2016 headers, not once per header.

`EpochManager` holds those values plus a `&mut State` and the segment's start height. Its
`Drop` impl is where chain work gets accounted:

```rust
fn flush(&mut self) {
    let processed = self.state.height - self.start_height;
    if processed == 0 {
        return;
    }
    self.state.chain_work += self.active_block_work * processed;
    self.state.current_nbits = self.active_nbits;
}
```

One multiply per segment instead of one 256-bit add per header. The early return matters: a
segment that accepted zero headers must not write its `active_nbits` back, or the next manager
would retarget again from the already retargeted value. The `Drop` runs on the error path too,
so a failure proof's `chain_work` covers every header accepted before the rejected one.

### Retarget

`calculate_next_target_required` is Bitcoin's rule:

- `timespan = previous_timestamp - epoch_start_timestamp`, as `i64` because clocks go
  backwards.
- Clamp to `[expected / 4, expected * 4]` where `expected` is 2016 × 600 seconds.
- `target = old_target * timespan / expected`, as a `u256 * u32` then a limb-wise division by
  a `u128`.
- Cap at the genesis target (the PoW limit).
- Round-trip through `target_to_bits` and `target_from_bits`.

The round-trip is the step people cut when they think they're simplifying. Bitcoin Core stores
`nbits` in the header and validates against the *expanded* `nbits`, which has lost the low
bits of the computed target. If the guest validated against the full-precision target it would
be proving a slightly different consensus rule, and it would accept or reject headers that
Core doesn't. That happened at the height 40320 retarget: the full-precision target gave
`0x1c6546c0` where consensus says `0x1c654657`. Commit `1668256` added the round-trip.

`epoch_start_timestamp` is the timestamp of the epoch's *first* block, not the last block of
the previous epoch. Same as Core, including the off-by-one that makes a 2016-block epoch
measure 2015 intervals.

### Block work and the quotient hint

Per-block work is `floor(2^256 / (target + 1))`. `target` is a `NonMaxU256`, so `+ 1` can't
wrap and that's a type guarantee rather than a check.

A 256-bit division in the VM is thousands of cycles. The guest doesn't do one. In
`block_work_from_target`:

```rust
sp1_lib::unconstrained!({
    let work = compute_block_work_division(target);
    sp1_lib::io::hint_slice(bytemuck::bytes_of(&work.into_inner().into_limbs()));
});
let limbs: [u64; 4] = /* read_vec_raw, 32 bytes */;
let work = BlockWork::from_limbs(limbs);
verify_block_work_witness(work, divisor);
```

The `unconstrained!` block runs the long division in the executor but generates no
constraints. It writes the answer to the hint stream and the guest reads it right back. Then
`verify_block_work_witness` checks it with one 256-bit multiply syscall. For divisor `d` and
claimed quotient `q`, `q` is the exact floor if and only if:

```text
2^256 < (q + 1) · d <= 2^256 + d
```

One syscall computes `q · d + d` as a 512-bit `(lo, hi)`. The check is `hi == 1`, `lo != 0`
and `lo <= d`. A wrong quotient halts with the invalid hint exit code.

Off-target the same function computes the quotient with `compute_block_work_division` and
runs the same verification, so the host exercises the check too.

### Median time past

The rule: a header's timestamp must be strictly greater than the median of the previous 11
timestamps. Bitcoin takes the element at index 5 of the sorted window, so for 11 values it's
the 6th smallest.

The naive version sorts 11 values per header. `zkpow-mtp` avoids that nearly always.

`Ring<11, Warm>` stores the values in a circular buffer indexed by `height % 11`, plus
`ordered_run`: the length of the nondecreasing run ending at the newest value, capped at 11.
If `ordered_run == 11`, the window is sorted in insertion order and the median is at a fixed
offset from the oldest slot:

```rust
const MEDIAN_SLOT: [usize; N] = /* (i + N/2) % N for each i */;
pub fn median(&self) -> Option<u32> {
    (self.ordered_run >= N).then(|| self.values[Self::MEDIAN_SLOT[self.next]])
}
```

No comparisons. Just a table lookup. Most of the chain is in order, so most headers take this
path.

When a timestamp goes backwards the run resets to 1 and `median()` returns `None` for the next
10 headers, until the window is nondecreasing end to end again. For those, the guest pulls the
next value from the `medians` input and checks it by rank:

```rust
fn rank_median(values: &[u32], half: usize, claimed: u32) -> Result<(), NotMedian> {
    // count less and equal
    if less > half { return Err(NotMedian::TooNew); }
    if less + equal <= half { return Err(NotMedian::TooOld); }
    Ok(())
}
```

With `half = 5`: at most 5 values below the claim and at least 6 at or below it. That's exactly
"the claim is the value at sorted index 5", and it also forces the claim to be one of the 11
values. Checking a median is 11 compares. Finding one is a sort.

The hint stream is positional. The guest consumes one hint each time `median()` is `None`, in
order, and halts as an invalid hint if it runs out or if any are left when the batch ends. So
the controller has to predict exactly which heights the guest will ask for. The guest's
criterion is `ordered_run < 11` on *its* ring. The controller's is `requires_median_hint` over
a run length stored per header in the database, which counts nondecreasing *links* rather than
values and so is ordered at 10. Same threshold, different counter. If they ever disagree the
guest halts on the first batch where they differ. That's loud, not silent, which is the
point.

The ring is rebuilt from `State.timestamps` at the start of every batch. `Warm::from_parts`
walks the slots from oldest to newest to recompute `ordered_run`, so the run length is never
part of the committed state. Only the 11 values are.

## Hashing

`zkpow-sha256` doesn't implement SHA-256. It implements two message layouts on top of SP1's
`sha256_extend` and `sha256_compress` syscalls, with the padding precomputed for each.

The state is `[u64; 8]` with each 32-bit word in the low bits. That's what the RV64
syscalls expect. `hash_to_state` and `state_to_hash` convert to and from 32 bytes, big-endian
per word.

| Function | Message | Blocks | Padding |
| --- | --- | --- | --- |
| `of_header_words` | 80-byte header, then SHA-256d | 2 + 1 | `0x80` at byte 80, length `0x280` |
| `of_public_values` | 185-byte public values | 4 | `0x80` after the return code, length `0x5C8` |

The header hash writes the 16 message words straight into the schedule buffer: `version`
byte-swapped, the previous hash as 8 state words, 7 words of merkle root. Second block: the
last merkle word, timestamp, nbits, nonce, then padding. The second SHA-256 pass takes the 8
state words as its first 8 message words. Three compress calls per header and no 80-byte
buffer is ever built.

The public values hash is 4 blocks because 185 bytes plus the terminator and the 8-byte length
is 194, which is 2 bytes over 3 blocks. So the length word sits alone in block 4.

Off-target, `compress_schedule` goes through the `sha2` crate's `compress256`. The
`crates/sha256/src/tests` compare both layouts against a plain `sha2` digest of the
equivalent bytes. Change either layout and those tests are what catch a padding mistake.

The workspace `Cargo.toml` patches `sha2` to SP1's fork so any `sha2` use inside the VM hits
the precompile. The guest's hot path doesn't go through `sha2` at all. It calls the syscalls
directly.

## Public values and exit codes

```text
  0..152  State
152..184  verifier key digest, [u32; 8] little-endian
184       return code
```

`PublicValues::serialize` writes them from borrowed parts so the guest doesn't build a struct.
`PublicValues::parse` is the host-side inverse.

The return code is a `u8` wrapped in `ReturnCode(Option<ErrorCode>)`, where `ErrorCode` is a
`NonZeroU8`. Zero is success. `ErrorCode` is deliberately not an error type: it's a byte with
no context, so it only exists at the point of commit. The errors that happen in the transition
are ordinary enums (`ApplyError`) with heights and hashes in them, and `ReportableError` is the
one trait that reduces an error to its byte.

| Code | `ApplyError` variant | When |
| --- | --- | --- |
| 0 | | every header accepted |
| 2 | `PowInsufficient` | hash above the active target |
| 3 | `TimestampTooOld` | timestamp not above the median |
| 5 | `InsufficientColdHeaders` | first batch shorter than 10 |
| 255 | | an error with no code of its own. Nothing emits it today |

Codes 1 and 4 have names in `ErrorCode` but no producer.

A consensus failure is a normal halt. The guest commits the state as of the last accepted
header with the nonzero code and exits 0 from SP1's point of view. The proof is valid and says
"valid chain up to `height`, header at `height + 1` rejected for this reason." The recursion
check refuses to extend it because of the hardcoded success byte.

There are three distinct ways the guest can stop and they mean different things:

| SP1 exit code | Cause | Public values |
| --- | --- | --- |
| 0 | `commit_public_values`, success or consensus failure | 185 bytes committed |
| 1 | a Rust panic: input cast failure, `expect`, `unreachable!` | none |
| 3 | `invalid_hint!`: bad or missing hint, or a wrong block work quotient | none |

The 3 vs 1 split is a security property. A malicious prover can trigger a panic trivially by
withholding input. It must not be able to make a bad hint look like an ordinary program bug,
and the separate exit code is how SP1 keeps those apart. `zkpow_core::invalid_hint!` and the
`assert_hint*` macros are the only sanctioned way to reject a hint. Off-target they print the
same diagnostic and panic with a message the tests match on.

One gap worth knowing: the guest's own `read_fixed_size_input` turns an empty input stream
into a cast error and therefore a panic (exit 1), whereas SP1's `read_vec` would halt with
exit 3. Missing input is prover-controlled, so it arguably belongs in the 3 bucket. Today it
doesn't.

## Cycle accounting

`#[sp1_cycle::tracked]` on a function and `sp1_cycle::track!(expr)` on an expression wrap the
body in `cycle-tracker-report-start/end` writes to fd 1. SP1's executor sums them into the
execution report by label, and the label is `module_path/function_name` or
`module_path:line`. Both macros expand to the bare body unless the crate is built for the zkVM
*and* the `cycle-tracking` feature is on.

Turn it on with `GUEST_PROFILING=1` when building the host. The host's `build.rs` forwards
that as the guest's `profiling` feature, which enables `zkpow-core/profiling` and
`sp1-cycle/cycle-tracking`. Then `ZKPOW_EXECUTE_ONLY=1` runs the guest without proving and
prints the report.

Annotating a function costs two syscall writes per call, so don't track anything in the
per-header path when measuring a per-header change. The spans currently there are on
`seal_header`'s callees and the per-segment functions, which is coarse enough not to distort
the per-header number much.

`crates/guest/Cargo.toml` also declares a `dummy-pow` feature. Nothing reads it. Don't rely on
it.

## Testing

The core crates test the same code twice. `cargo test -p zkpow-core` runs the host build with
the fallback branches. `cargo zkvm test -p zkpow-core` (from the `zkvm-test` repo, with the
`succinct` toolchain) compiles the same tests for `riscv64im-succinct-zkvm-elf` and runs each
one in its own executor, reporting cycles. `use zkvm_test::zkvm_test;` is the attribute, and
it's a plain `#[test]` on the host.

Things the test suites lean on:

- `test_hash_mock` in `crates/core/src/lib.rs`. A thread-local that makes `hash_header_input`
  return all zeros (passes any target) or all `0xFF` (fails every target), so tests don't mine.
  It lives in `zkpow-core` because `cfg(test)` is per crate and the mock has to be compiled
  into the crate under test.
- `#[zkvm_test(exit_code = 3, should_panic(expected = "halt: invalid hint (3)"))]` for hint
  rejections. The `should_panic` half is what the host build matches on, the `exit_code` half
  is what the VM build matches on.
- `fixture = "headers_fixture.bin"` for tests that run real mainnet headers end to end through
  `apply_headers_from_genesis`, with `max_cycles` as a regression bound.
- `ShortEpoch` in `zkpow-params`, behind the `test-networks` feature. It's `Mainnet` with
  `epoch_length = 32`, so a retarget boundary lands inside a small warm batch and the test
  proves the boundary is read from the parameters.

`crates/core/src/tests/` is split by concern: `pow`, `mtp`, `retarget`, `state`,
`public_values` and `input`, each with `success.rs` and `failure.rs`, plus `halt.rs` for the
hint macros. A new rule gets a
failure test showing the rejection and the code it maps to, and a success test showing the
state after acceptance.

## What to check in a review

- **Both branches.** Anything under `cfg(target_os = "zkvm")` has a twin. The controller runs
  the twin to predict the commit. If a diff touches one, find the other.
- **Hint positions.** A change to when the guest asks for a median hint has to be mirrored in
  `requires_median_hint` and the controller. The symptom of getting this wrong is an invalid
  hint halt on some batch, not a wrong proof.
- **Hint checks.** Every value read from the hint stream needs a check that fails with
  `invalid_hint!`, never with `return Err`. An `ApplyError` becomes a valid failure proof. A
  hint problem must not.
- **The round-trip.** `calculate_next_target_required` must end with `target_to_bits` then
  `target_from_bits`. See the retarget section for why.
- **Wire layouts.** `State`, `HeaderInput` and `PublicValues` are byte-for-byte contracts with
  the controller and with every proof already on disk. A field reorder changes the verifier
  key and invalidates the run.
- **Segment flush.** `EpochManager::flush` must run on both the success and the error path,
  and must skip the `nbits` write when it processed nothing.
- **Per-header cost.** Nothing new in `seal_header` or its callees without a cycle number from
  `cargo zkvm test` before and after.
- **ADR-0001** is why `genesis_hash` is in every state. **ADR-0002** is why the median path
  takes a hint instead of sorting. Its description of a `State<E>` and an `env/` module is out
  of date, but the failure mode it records, an agent "optimizing" the hint path into a sort, is
  still the most likely bad change to this code.
