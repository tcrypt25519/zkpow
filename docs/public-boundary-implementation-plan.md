# Public Boundary Implementation Plan

Goal: shrink public values to the verifier-visible claim while keeping recursive
continuation sound. The order below does the least disruptive useful work first.
Each step should compile and pass the normal host/guest checks before moving on.

## Target End State

Public values:

```text
genesis_hash:         [u8; 32]
tip_hash:             [u8; 32]
chain_work:           [u8; 32]
height:               u32
return_code:          u8   // 0 = success, nonzero = validation failure
failure_height:       u32  // 0 on success, absolute chain height on failure
continuation_digest:  [u8; 32]
```

Private continuation state:

```text
next_target:             Target
next_nbits:              CompactTarget
next_work:               ChainWork
epoch_start_timestamp:   BlockTimestamp
median_timestamp_window: [BlockTimestamp; 11]
```

Auxiliary witness:

```text
new_headers:                  [NewHeader]
median_hints:                 [BlockTimestamp]
previous_public_claim:         optional PublicChainClaim
previous_continuation_state:   optional PrivateContinuationState
previous_sp1_proof:            optional recursive proof witness
```

The continuation digest should be:

```text
continuation_digest = H(private_continuation_state)
```

The public-values digest already binds `continuation_digest` to the public
claim. The continuation digest only needs to commit to the hidden continuation
bytes.

## Step 1: Harden Recursive Success Handling

Value on its own: prevents building a new proof on top of a failed proof before
any public-value layout change.

Current behavior already parses previous public values on the host and rejects
`HeaderChainPublicValues::Failure` before generating the next proof. The guest
should also enforce that recursive continuation is only from a successful prior
proof.

Implementation:

- Keep current full-`State` public values for this step.
- Introduce an explicit typed recursive-start value instead of treating
  recursive proof metadata alone as enough.
- For non-genesis recursive input, ensure the guest can distinguish a prior
  success public value from a prior failure public value.
- Add a check equivalent to `previous_return_code == 0`.
- Keep parse failures as proof setup failures/panics, not committed validation
  failures.

Tests:

- Existing recursive success test still passes.
- Add or update a test that attempts to build on a failed previous proof and
  fails before applying new headers.

## Step 2: Change Failure Location To Absolute Height

Value on its own: improves the current public failure output without changing
the success state format.

Current failure metadata carries a per-batch `header_index`. Replace that with
absolute `failure_height`.

Implementation:

- Change `ProofFailure` metadata meaning from batch index to chain height.
- In `State::apply_headers_in_place`, compute failure height as:

```text
last_valid_height + 1
```

for the failed header.

- Update `encode_failure_metadata`, `HeaderChainPublicValues::Failure`, host
  parsing, `inspect_proof`, and `test_errors`.
- Keep the existing error code byte.

Tests:

- Timestamp and PoW failure tests should assert absolute height.
- Recursive failure handling should still reject failed previous proofs.

## Step 3: Introduce Public Claim And Continuation Types

Value on its own: creates the type boundary without changing committed output
yet.

Add typed structs:

```rust
struct PublicChainClaim {
    genesis_hash: BlockHash,
    tip_hash: BlockHash,
    chain_work: ChainWork,
    height: u32,
}

struct DifficultyState {
    next_target: Target,
    next_nbits: CompactTarget,
    next_work: ChainWork,
}

struct PrivateContinuationState {
    difficulty: DifficultyState,
    epoch_start_timestamp: BlockTimestamp,
    timestamps: [BlockTimestamp; WINDOW_SIZE],
}

struct ValidationState {
    public: PublicChainClaim,
    private: PrivateContinuationState,
}
```

`DifficultyState` should be constructed through helpers that derive or validate
`target`, `nBits`, and `work` together. Do not make callers manually assemble an
unchecked tuple.

Implementation:

- Add conversion from current `State` to `(PublicChainClaim,
  PrivateContinuationState)`.
- Add conversion back into current `State` while the old transition code still
  exists.
- Add fixed-width serialization for the new types.
- Add `continuation_digest(private_continuation_state)`.

Tests:

- Round-trip current `State` through the split representation.
- Mutating any byte of private continuation changes `continuation_digest`.
- Difficulty triple validation rejects inconsistent target/bits/work.

## Step 4: Commit Continuation Digest Beside Current State

Value on its own: validates the new digest path while preserving current proof
consumers.

Temporarily append `continuation_digest` to the current public output:

```text
current full State || existing failure metadata if any || continuation_digest
```

This is transitional internal scaffolding, not a compatibility promise. It lets
the host and guest prove that they compute the same private continuation digest
before the old public `State` is removed.

Implementation:

- Guest computes continuation state from final `State`.
- Guest commits the digest after current success/failure output.
- Host expected public values include the same digest.
- Proof inspection displays the digest.

Tests:

- `test_errors` passes with the extended output.
- Add a unit test for public-value parser length and digest extraction.

## Step 5: Move Recursive Start To Public Claim Plus Continuation Witness

Value on its own: changes recursive continuation to the final soundness model
while the old public state is still available as a cross-check.

For proof `n + 1`, the prover supplies:

```text
previous_public_claim
previous_continuation_state
previous_recursive_proof
```

The guest:

1. Hashes `previous_continuation_state`.
2. Checks it equals the continuation digest committed by proof `n`.
3. Reconstructs proof `n` public values in canonical form.
4. Hashes those public values.
5. Verifies proof `n` against that public-values digest.
6. Checks prior return code is success.
7. Uses `previous_public_claim + previous_continuation_state` as the starting
   validation state.

While old public values still include full `State`, also cross-check that the
split start state matches the old public `State`. Remove this cross-check in the
final cutover.

Tests:

- Recursive success test passes through the new witness path.
- Changing one byte of previous continuation state fails.
- Changing previous public claim fails.
- A failed previous proof cannot be extended.

## Step 6: Replace Public Values With The Minimal Format

Value on its own: removes unnecessary public data and finalizes the verifier
interface.

Commit only:

```text
genesis_hash
tip_hash
chain_work
height
return_code
failure_height
continuation_digest
```

Implementation:

- Replace `HeaderChainPublicValues::Success(State)` with a public-output type
  carrying the minimal fields.
- Replace committed failure output with the same public-output type using
  nonzero `return_code` and absolute `failure_height`.
- Remove full `State` parsing from public values.
- Update host expected public values.
- Update `inspect_proof`.
- Update Groth16/public-values digest handling to use the new bytes.

Tests:

- Existing success and failure tests pass against the minimal public values.
- Recursive proof chaining still passes.
- Inspect tool prints only public claim, return code, failure height, and
  continuation digest.

## Step 7: Remove Transitional State Plumbing

Value on its own: reduces code surface after the new boundary is proven.

Remove:

- old full-`State` public-value parser
- old recursive-start path that reads a full public `State`
- transitional public-value digest cross-checks
- any proof-inspection code that assumes public values contain the timestamp
  window, next bits, or epoch timestamp

Keep:

- dense 11-timestamp window as private continuation state
- hidden header witness
- median hints
- full difficulty tuple in continuation state

Validation:

- `cargo test -p zkpow-core`
- `cargo build --release`
- `cargo run --release --bin test_errors`
- `cargo clippy --all-targets -- -D warnings`

## Notes

- Do not preserve compatibility with existing proof files. There are no external
  users of the current format.
- Do not add raw tip header to the default public values. A later wrapper program
  can expose raw header fields for SNARK-facing use.
- Do not recompute target/work every batch just to reduce continuation bytes.
  The continuation state carries target, compact bits, and work specifically to
  avoid expensive repeated derivation.
- Keep the active MTP window as private continuation state. Reconstructing it
  from strided header lookbacks is not part of this plan.
