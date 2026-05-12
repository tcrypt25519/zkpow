# Public Boundary Proposal

This reviews the current proof boundary and proposes a cleaner split between:

- verifier-visible public values
- private continuation state that must remain authenticated for recursion
- auxiliary witness hints that do not need to be authenticated across proofs

The core point is that "private" and "uncommitted" are not the same thing. The
timestamp window, difficulty state, and cached work can be hidden from verifiers,
but the next recursive proof still has to be forced to use the exact continuation
state produced by the previous proof. That requires a public commitment to the
hidden continuation state.

## Current Boundary

Today the committed public value on success is the full `State`:

```rust
pub struct State {
    pub header: Header,
    pub block_hash: BlockHash,
    pub genesis_hash: BlockHash,
    pub next_nbits: CompactTarget,
    pub height: u32,
    pub chain_work: ChainWork,
    pub next_work: ChainWork,
    pub epoch_start_timestamp: BlockTimestamp,
    pub timestamps: [BlockTimestamp; 11],
}
```

This exposes more than a normal verifier needs:

- the full latest header
- the next compact difficulty
- cached next work
- the epoch start timestamp
- the 11-entry median-time-past window

It also makes recursive continuation easy because the next proof can use the
previous public values directly as its starting state. The proposed design should
keep that recursive binding property without revealing the whole state.

## Recommended Split

### Public Values

The public values should describe the claim a verifier actually checks:

```text
genesis_hash:         [u8; 32]
tip_hash:             [u8; 32]
chain_work:           [u8; 32]
height:               u32
return_code:          u8   // 0 = success, nonzero = validation failure
failure_height:       u32  // 0 on success, absolute chain height on failure
continuation_digest:  [u8; 32]
verifier_key:         [u8; 32]
```

This is 169 bytes with explicit packed serialization. If the serialization needs
alignment padding for host-side casting, put `height` next to `return_code` and
`failure_height`; do not reorder the three primary 32-byte values unless it
removes real padding. The normal field order should be genesis hash, tip hash,
chain work, then height.

`verifier_key` is the SP1 verifier-key digest used for recursive proof
verification. Verifiers are responsible for checking that this digest is the
program key they intend to trust.

`return_code` should be one byte, like a Unix return value. `0` means success;
nonzero values are validation errors. A separate status byte and error-code byte
would duplicate the same information.

`failure_height` is the absolute block height where validation failed. A
per-batch header index is not useful to a verifier unless they already know the
hidden batch shape. If we are spending four bytes on failure location, use a
chain-global height.

`chain_work` is mandatory public data. Canonical-chain comparison is the main
reason to carry the proof, so cumulative work is part of the visible claim.

`tip_header` should not be included. A verifier that already has an
80-byte Bitcoin header can double-SHA256 it and compare the result to `tip_hash`.
That keeps the proof interface independent of batch size and avoids committing
header fields that many verifiers do not need. A separate conversion/wrapper
program can add the raw header back later when producing a SNARK interface that
needs header fields directly.

### Private Continuation State

The private continuation state should contain the consensus data needed to
extend the proof but not needed by ordinary verifiers:

```text
next_target:             Target
next_nbits:              CompactTarget
next_work:               ChainWork
epoch_start_timestamp:   BlockTimestamp
median_timestamp_window: [BlockTimestamp; 11]
```

Carry the full difficulty tuple. The point of including these continuation
values is to avoid recomputing expensive derived values inside the program. If
the program were going to recompute target/work from bits every batch or every
iteration, the continuation state would not be serving its purpose.

The type system should still prevent inconsistent triples. Callers should not be
able to supply arbitrary `next_target`, `next_nbits`, and `next_work` values
without validation. They should construct the tuple through a type that either
derives the related values together at retarget time or verifies that a supplied
triple is internally consistent.

The continuation digest should commit to the hidden continuation state:

```text
continuation_digest = H(private_continuation_state)
```

The proof public-values hash already binds `continuation_digest` to
`genesis_hash`, `tip_hash`, `chain_work`, `height`, `return_code`, and
`failure_height`. Repeating those fields inside the continuation digest is not
necessary.

### Auxiliary Witness Hints

Auxiliary witness should contain data that helps execution but is either fully
validated during the proof or does not need to persist across recursive steps:

```text
new_headers:                  [NewHeader]
median_hints:                 [BlockTimestamp]
previous_public_claim:         optional PublicChainClaim, for recursive proofs
previous_continuation_state:   optional PrivateContinuationState, for recursive proofs
previous_sp1_proof:            optional recursive proof witness
```

The important distinction:

- `new_headers` are ephemeral witness data. The proof validates them and commits
  only the resulting public claim.
- `median_hints` are ephemeral witness data. The proof validates each hint
  against the authenticated private median window.
- the previous continuation state is private, but not merely a hint. It must
  match the previous proof's `continuation_digest`.
- previous public values do not need to be passed as raw bytes if the recursive
  input carries the previous public claim plus previous continuation state. The
  guest can reconstruct the canonical previous public-values bytes and hash them
  to check the recursive proof digest.

## Recursive Verification Flow

For a recursive proof, the guest should:

1. Verify the previous SP1 proof against `previous_public_values_digest`.
2. Read `previous_public_claim` and `previous_continuation_state` as private
   witness data.
3. Recompute `continuation_digest` from `previous_public_claim` plus
   `previous_continuation_state`.
4. Reconstruct the canonical previous public-values bytes:
   `previous_public_claim`, success `return_code`, zero `failure_height`, and
   the recomputed `continuation_digest`.
5. Hash those reconstructed public values and check the digest equals
   `previous_public_values_digest`.
6. Use `previous_public_claim` plus `previous_continuation_state` as the
   starting validation state.

The current code avoids step 2 because the full `State` bytes are themselves the
public values. Once public values become smaller than the full continuation
state, the recursive input needs the previous public claim and hidden
continuation state, but it does not need a separate raw copy of previous public
values.

## Type Shape

A good implementation target is to split today's `State` into three typed values:

```rust
struct PublicChainClaim {
    genesis_hash: BlockHash,
    tip_hash: BlockHash,
    chain_work: ChainWork,
    height: u32,
}

struct PrivateContinuationState {
    next_target: Target,
    next_nbits: CompactTarget,
    next_work: ChainWork,
    epoch_start_timestamp: BlockTimestamp,
    timestamps: [BlockTimestamp; WINDOW_SIZE],
}

struct ValidationState {
    public: PublicChainClaim,
    private: PrivateContinuationState,
}
```

`ValidationState` is what the guest mutates while processing headers.
`PublicChainClaim` is what ordinary verifiers inspect.
`PrivateContinuationState` is hidden but committed by `continuation_digest`.

This also gives the type system a better place to enforce the target/compact
target/work invariant. Callers should not be able to supply an arbitrary
`next_target`, `next_nbits`, and `next_work` triple. They should construct it
through a type that derives or validates the cached values together.

## Migration Plan

1. Add the new public value and continuation-state types.
2. Add `continuation_digest` computation and tests that changing any public
   claim field or private continuation field changes the digest.
3. Change recursive input to include previous public claim and previous private
   continuation state as private witness data.
4. Change guest success/failure commits to emit the new public values.
5. Update host verification and proof inspection to parse only the new public
   values.
6. Remove the old full-`State` public value format in the same implementation
   pass. There is no external compatibility requirement for the current format.

## Decision Points

The main settled decisions from this review are:

- `chain_work` stays public.
- the raw tip header is not part of these public values.
- the continuation state carries `next_target`, `next_nbits`, and `next_work`.
- recursive continuation uses typed previous public claim plus typed previous
  continuation state, not a separate raw `previous_public_values` witness.
