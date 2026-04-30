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
schema_version:       u8
status:               u8   // 0 = success, nonzero = failure
error_code:           u8   // 0 on success
reserved:             u8
error_header_index:   u32  // 0 on success
genesis_hash:         [u8; 32]
height:               u32
tip_hash:             [u8; 32]
chain_work:           [u8; 32]
continuation_digest:  [u8; 32]
```

This is 140 bytes with explicit packed serialization.

`chain_work` should stay public unless this proof is only meant to prove "some
valid header chain exists." If the verifier wants canonicality or wants to
compare competing tips, cumulative work is part of the visible claim.

`tip_header` should not be included by default. A verifier that already has an
80-byte Bitcoin header can double-SHA256 it and compare the result to `tip_hash`.
That keeps the proof interface independent of batch size and avoids committing
header fields that many verifiers do not need. If a contract or application needs
header fields without doing SHA256, add a second public-values flavor that
includes the full 80-byte `tip_header`.

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

The current implementation stores `next_nbits` and `next_work`, deriving
`Target` when needed. A cleaned-up version can make `Target` the canonical
difficulty field and derive/update `next_nbits` and `next_work` together only
when the target changes. Whether the caches are stored or derived should be an
implementation decision behind a validated type constructor, not caller-supplied
loose fields.

The continuation digest should bind both the visible claim and the hidden
continuation state:

```text
continuation_digest =
    H(domain || schema_version || genesis_hash || height || tip_hash || chain_work || private_continuation_state)
```

Binding the visible fields into the digest prevents a hidden continuation state
from being transplanted onto a different public tip with the same private
continuation bytes.

### Auxiliary Witness Hints

Auxiliary witness should contain data that helps execution but is either fully
validated during the proof or does not need to persist across recursive steps:

```text
new_headers:        [NewHeader]
median_hints:       [BlockTimestamp]
previous_public_values_v2: optional bytes, for recursive proofs
previous_continuation_state: optional private continuation state, for recursive proofs
previous_sp1_proof: optional recursive proof witness
```

The important distinction:

- `new_headers` are ephemeral witness data. The proof validates them and commits
  only the resulting public claim.
- `median_hints` are ephemeral witness data. The proof validates each hint
  against the authenticated private median window.
- the previous continuation state is private, but not merely a hint. It must
  match the previous proof's `continuation_digest`.

## Recursive Verification Flow

For a recursive proof, the guest should:

1. Verify the previous SP1 proof against `previous_public_values_digest`.
2. Read `previous_public_values_v2` as private witness bytes.
3. Hash `previous_public_values_v2` and check it equals
   `previous_public_values_digest`.
4. Parse `previous_public_values_v2`.
5. Read `previous_continuation_state` as private witness.
6. Recompute `continuation_digest` from the parsed visible fields plus the
   private continuation state.
7. Check it equals the digest committed in `previous_public_values_v2`.
8. Use the parsed visible fields plus the private continuation state as the
   starting validation state.

The current code avoids step 2 because the full `State` bytes are themselves the
public values. Once public values become smaller than the full continuation
state, the guest needs either the previous public values or another bound way to
recover the previous `continuation_digest`.

## Type Shape

A good implementation target is to split today's `State` into three typed values:

```rust
struct PublicChainClaim {
    genesis_hash: BlockHash,
    height: u32,
    tip_hash: BlockHash,
    chain_work: ChainWork,
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
through a type that derives the cached values together.

## Migration Plan

1. Add the new public value and continuation-state types alongside the current
   `State` parser.
2. Add `continuation_digest` computation and tests that changing any public
   claim field or private continuation field changes the digest.
3. Change recursive input to include previous public values and previous private
   continuation state as private witness data.
4. Change guest success/failure commits to emit `PublicValuesV2`.
5. Update host verification and proof inspection to parse both old and new
   public values during the transition.
6. Once existing proof compatibility is no longer needed, remove the old
   full-`State` public value format.

## Decision Points

The main open decisions are:

- Whether `chain_work` is required in public values. I recommend yes if the proof
  is meant to support canonical-chain comparisons.
- Whether the default public values include the full 80-byte tip header. I
  recommend no for the minimal relay path, with an optional header-rich format if
  a verifier needs timestamp, merkle root, or bits without supplying the raw
  header separately.
- Whether private continuation stores `Target` directly, or stores compact bits
  plus cached work as today. I recommend making expanded `Target` canonical
  internally and deriving compact bits/work through one validated difficulty
  state type.
