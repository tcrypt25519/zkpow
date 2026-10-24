# zkpow: How the System Works End to End

zkpow produces one SP1 proof that says: starting from the Bitcoin genesis block, every header up
to height N is consensus-valid, and this is the tip hash and cumulative work at N. Anyone can
check that proof in milliseconds instead of downloading and validating 900k+ headers.

This document explains what the pieces are, what the state machine inside the proof actually
does, and how each batch's proof gets chained onto the one before it. It assumes you know what a
zk proof is and roughly what SP1 does (runs a RISC-V program and proves it ran). Everything
below is about this particular program.

## The problem

A Bitcoin light client wants to know the current chain tip without trusting anyone. The
classic answer is to download all the headers (80 bytes each) and check them yourself. That's
about 75 MB and a lot of SHA-256, and you have to redo it on every device.

The zk answer is to have one machine do that validation once inside a zkVM and hand everyone
else a proof. But a single proof over the whole chain doesn't fit in any prover. So the chain
gets validated in batches, and each batch's proof has to also prove that the previous batch's
proof was valid. That's the recursive part, and it's where most of the design lives.

## The pieces

```text
Bitcoin P2P ──zkpow-cli sync──▶ headers.db
                                    │
                                    ▼
                               controller ──batch──▶ host ──stdin──▶ guest (SP1 zkVM)
                                    ▲                 │                   │
                                    │                 ◀────── proof ──────┘
                                    └── complete() ───┘
                                 (verify proof + compare public values, then advance)
```

- `zkpow-cli sync` pulls headers from Bitcoin P2P peers, validates them natively, and writes
  them to a SQLite file. Each row stores the header plus derived values: cumulative chain work,
  the median time past at that height, and how long the current run of nondecreasing
  timestamps is. Those last two feed the guest's median trick (below).
- The controller owns the database, decides which heights go in the next batch, computes
  what the proof's output *should* be, and verifies every proof before advancing. It also keeps
  a per-run ledger so a run can resume after a crash.
- The host is a stateless pipe. It asks the controller for a batch, serializes the guest's
  input, runs the SP1 prover, writes the proof to disk, and tells the controller it's done.
  It validates nothing on its own.
- The guest is the program that runs inside SP1. It's the only code whose behavior the proof
  actually attests to, and it's tiny: read inputs, verify the previous proof, apply headers,
  commit the result.

The host and controller run in the same process today. The split exists so the trust boundary
is explicit: the controller never believes the host, only the proof.

## What a proof says

The guest commits 185 bytes of public values. Those bytes are the entire interface.

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 152 | `State` (see below) |
| 152 | 32 | verifier key digest of the guest program |
| 184 | 1 | return code, `0` means every header validated |

A consumer of the final proof does four things: verify the SP1 proof against the known guest
verifier key, check the committed verifier key digest matches that same key, check
`State.genesis_hash` is the mainnet genesis hash, and check the return code is `0`. After that
they can trust `block_hash`, `height` and `chain_work` in the state.

The genesis hash is in every proof on purpose. Without it a proof of "a valid PoW chain with
this much work" could be a testnet chain or something someone mined privately. With it, a
verifier needs exactly one hardcoded constant to know which network they're looking at.

## The state machine

`State` is 152 bytes and it's the whole thing. It's the state *after* the last header the
guest accepted, and the guest mutates it in place as it walks the batch.

| Field | Size | What it is |
| --- | --- | --- |
| `chain_work` | 32 | cumulative work, the number that decides which chain is canonical |
| `genesis_hash` | 32 | network identity |
| `block_hash` | 32 | hash of the tip header |
| `height` | 4 | tip height |
| `current_nbits` | 4 | compact difficulty target in force for this epoch |
| `epoch_timestamp` | 4 | timestamp of the first block of the epoch, for the next retarget |
| `timestamps` | 44 | the last 11 block timestamps, a ring indexed by `height % 11` |

The first three fields plus `height` are what a verifier cares about. The last three are what
the guest needs to validate the *next* header: the active difficulty, the retarget baseline,
and the median time past window. They're public too, which is what makes recursion simple.

### Applying one header

The prover doesn't hand the guest full 80-byte headers. It hands 44 bytes per header: version,
merkle root, timestamp, nonce. The other two fields, `prev_blockhash` and `nbits`, are taken
from the state. The guest hashes the header using *its* previous block hash and *its* active
difficulty. So there's no separate "does this link to the previous block" check. If the prover
supplied a header that belongs to a different parent or a different difficulty, it hashes to
something else and fails the PoW check.

For each header the guest does, in order:

- Median time past. The header's timestamp must be strictly greater than the median of the
  previous 11 timestamps. Fails with error code 3.
- Proof of work. Compute SHA-256d of the header and check it's at or below the active target.
  Fails with error code 2.
- Advance. Bump `height`, push the timestamp into the ring, and if the new height is a multiple
  of 2016, record this timestamp as the new `epoch_timestamp`.

That's the per-header rule set. The two things that make it cheap enough to prove are how the
median and the difficulty are handled.

### Median time past without sorting

Sorting 11 numbers for every header is surprisingly expensive in a zkVM. The guest avoids it
almost always.

The ring tracks how long the current run of nondecreasing timestamps is. If the last 11
timestamps are all in order, the median is just the value in a fixed slot, computed from a
compile-time table. No comparisons at all. Most of the chain is in order, so most headers take
this path.

When a timestamp goes backwards (which happens, miners' clocks drift), the window isn't
sorted and the slot trick doesn't work. For those headers the controller supplies a *hint*: the
median value, precomputed in the database. The guest doesn't trust the hint. It counts how many
window values are less than, equal to and greater than the hint, and halts if the hint isn't at
exactly the median rank. Checking a claimed median is a linear scan. Finding one would be a
sort.

Hints are sparse. The database only emits one for heights where the window isn't provably
ordered, and the guest halts if any hints are left over at the end of the batch or missing when
needed. A bad or extra hint isn't a validation failure of the chain. It's a prover error, so the
guest exits with SP1's invalid-hint exit code and no usable proof comes out.

### Cold start

The median rule needs 11 timestamps and genesis only gives one. The first 10 headers after
genesis run through a separate cold path that sorts a partial window (cheap, it only happens
once per chain) and uses the genesis difficulty directly. The very first batch must contain at
least 10 headers or the guest fails with error code 5. Once the window is full the warm path
takes over and the cold code never runs again.

### Difficulty epochs

Difficulty changes every 2016 blocks, so within a batch it changes rarely. The guest processes
a batch in segments that end at epoch boundaries. At the start of each segment it computes the
active target, compact bits and per-block work once, then reuses them for every header in the
segment.

At a boundary the retarget is Bitcoin's: actual epoch timespan clamped to between a quarter and
four times the expected two weeks, old target scaled by that ratio, capped at the genesis
target, then round-tripped through the compact `nbits` encoding so the guest validates against
the same truncated target Bitcoin Core does.

Per-block work is `floor(2^256 / (target + 1))`. A 256-bit division is also expensive in a
zkVM, so the guest computes the quotient *unconstrained* (SP1 lets you run code the prover
isn't held to) and then verifies the result with a single 256-bit multiply syscall. A wrong
quotient halts as an invalid hint.

Chain work is accumulated per segment, not per header: `work * headers_in_segment` added once
when the segment ends, including when it ends because a header failed. So the committed
`chain_work` is always complete up to the last accepted header.

## Recursion

Here's the trick that makes the whole chain collapse into one proof.

The guest's input `State` for batch N+1 is exactly the `State` that batch N's proof committed.
Before applying any headers, the guest for N+1 does this:

- Serialize `state || verifier_key || 0` (0 meaning success) into the 185 public value bytes
  the previous proof must have committed.
- SHA-256 them.
- Call SP1's `verify_sp1_proof(verifier_key, hash)`, which checks that a proof under that
  verifier key with exactly those public values was supplied as a witness.

The prover can't lie about the starting state because any change to any byte of `State` changes
the hash and the verification fails. And the prover can't extend a *failed* proof because the
guest hardcodes return code `0` into the reconstructed bytes. A proof that committed error code
2 has a different hash and won't verify.

The verifier key is part of the public values and part of what gets hashed, so the chain is
also pinned to one guest program. Swap in a different guest and the previous proof's committed
key won't match.

At height 0 there's nothing to verify. The genesis state is accepted as the trusted anchor, with
`genesis_hash` and `block_hash` both set to the genesis header's hash and one timestamp in the
ring. Block 1 is the first header the guest validates.

So the induction is:

```text
genesis State ──batch 1──▶ proof_1 commits State_1
State_1 + proof_1 ──batch 2──▶ proof_2 commits State_2    (guest verified proof_1 inside)
State_2 + proof_2 ──batch 3──▶ proof_3 commits State_3    (guest verified proof_2 inside)
...
```

Only the last proof matters. It transitively covers every header since genesis.

## Failure proofs

When a header is invalid the guest doesn't panic. It stops, commits the last valid `State`
with a nonzero return code, and halts normally. That proof verifies fine as an SP1 proof. It
just says "this chain was valid up to `state.height` and the header at `state.height + 1` was
rejected for reason X."

| Code | Meaning |
| --- | --- |
| 0 | success |
| 2 | header hash above target |
| 3 | timestamp not above median time past |
| 5 | first batch had fewer than 10 headers |

A failure proof is useful as evidence that a prover was fed bad data. It's a dead end for the
chain: the recursion check above refuses to extend it. In practice the host notices the nonzero
code and errors out before uploading anything, and the controller would reject the mismatched
public values anyway.

## The controller's job

The controller never looks at the proof to learn what happened. It already knows what *should*
have happened. For every batch it hands out, it also computes the expected end state straight
from the database and serializes the exact 185 bytes the guest must commit.

When the host reports completion, the controller:

- reads the uploaded proof file,
- checks its public value bytes are identical to the expected bytes,
- runs SP1 verification against the guest verifier key,
- writes the new verified height and proof file name to the `runs` table.

If any of that fails the run halts and the host gets an error on its next pull. Only one batch
can be outstanding per run. The controller won't hand out batch N+1 until batch N has verified.

Progress lives in the database, keyed by `ZKPOW_RUN_ID`. Restarting the host with the same run
id picks up at the last verified height and feeds the stored proof back in as the recursive
witness. If the guest program changes (new verifier key), the run is reset and starts over,
because old proofs can't be extended under a new key.

## Running it

```bash
cargo build --release

# Populate headers.db from Bitcoin P2P
cargo run --release -p zkpow-cli -- sync --db headers.db

# Prove from genesis, one epoch (2016 headers) per batch
ZKPOW_DB_PATH=headers.db ./target/release/zkpow-host

# Read a proof's public values
cargo run --release -p zkpow-host --bin inspect-proof -- proof_store/dev/proof_2016.bin
```

The knobs that matter:

| Variable | Default | Effect |
| --- | --- | --- |
| `ZKPOW_RUN_ID` | `dev` | which chain of proofs to extend |
| `ZKPOW_BATCH_SIZE` | 2016 | headers per proof |
| `ZKPOW_BATCH_COUNT` | 500 | stop after this many batches |
| `ZKPOW_DB_PATH` | `headers.db` | the header database |
| `ZKPOW_PROOF_STORE_DIR` | `proof_store` | where proofs land, under `<run_id>/proof_<height>.bin` |
| `ZKPOW_EXECUTE_ONLY` | off | run the guest without proving, for timing and debugging |

The prover is SP1's CPU backend producing *compressed* proofs, which is the form SP1 can verify
recursively. Wrapping the final proof into a Groth16 SNARK for on-chain use is a separate step
that isn't part of this pipeline.

## Where to look

- `crates/guest/src/main.rs` is the proven program. It's about a hundred lines.
- `crates/core/src/state.rs` is the state machine: cold start, warm batches, epoch segments.
- `crates/core/src/public_values.rs` is the 185-byte layout and the return codes.
- `crates/mtp/` is the timestamp ring and the median rank check.
- `crates/controller/src/runtime.rs` is batch selection, expected values and completion.
- `crates/host/src/pipeline/mod.rs` is the prove loop.
