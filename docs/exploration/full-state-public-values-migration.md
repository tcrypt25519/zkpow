# Full State Public Values Migration

## Decision

Replace the partitioned recursive authentication model with one `PublicValues` protocol type containing the complete `State`, the verifier key digest, and the return code. Remove `MinimalPublicValues`, `Claim`, `ContinuationData`, the continuation digest, `serialize_minimal_public_values`, and `of_minimal_public_values`.

The purpose is correctness rather than only reducing copies. Every consensus relevant field in `State` must automatically be covered by recursive proof verification. There must not be independent field lists that can diverge when `State` changes.

## Current authenticated data

`State` is a 152 byte `#[repr(C)]`, `Pod`, `Zeroable` value with this exact byte layout:

```text
  0.. 32  chain_work
 32.. 64  genesis_hash
 64.. 96  block_hash
 96..100  height
100..104  current_nbits
104..108  epoch_start_timestamp
108..152  timestamps[0..11]
```

The current 165 byte `MinimalPublicValues` separately lists four fields from `State`, replaces the remaining 52 bytes with their 32 byte SHA-256 digest, and adds the verifier key and return code:

```text
  0.. 32  chain_work
 32.. 64  verifier_key
 64.. 96  continuation_digest
 96..128  genesis_hash
128..160  tip_hash
160..164  height
164       return_code
```

The guest manually reconstructs this layout again in `zkpow_sha256::of_minimal_public_values`. Consequently, authenticated field coverage is duplicated across `State`, `Claim`, `ContinuationData`, the serializer, and the fixed input hash function.

## Required representation

The new protocol wire format is exactly 185 bytes:

```text
  0..152  complete State bytes
152..184  VerifierKeyDigest bytes
184       ReturnCode byte
```

The Rust model should be:

```rust
pub struct PublicValues {
    pub state: State,
    pub verifier_key: VerifierKeyDigest,
    pub return_code: ReturnCode,
}
```

`PublicValues` cannot derive `Pod` because `ReturnCode` is represented by `Option<NonZeroU8>`. It therefore needs explicit `to_bytes` and `parse` methods for the exact 185 byte protocol format. The complete `State` portion can be copied with `bytemuck::bytes_of` and parsed with `bytemuck::try_from_bytes` followed by one owned copy on the host. The guest should serialize only once when committing its final public values.

## Recursive verification flow

The host continues to provide the prior `State` and verifier key as guest input.

For a recursive batch, the guest computes SHA-256 over the exact bytes:

```text
prior State || verifier key || success return code
```

It passes that digest to `verify_sp1_proof`. This is the hash of the previous proof's complete public values. No continuation extraction or nested continuation hash remains.

After applying headers, the guest commits:

```text
updated State || verifier key || actual return code
```

The recursive hash covers 185 bytes and requires four SHA-256 compression blocks. The implementation must match raw SHA-256 exactly. A test must compare the guest optimized hash with the host SP1 public value hash implementation.

## Data flow changes

### Core

`PublicValues` replaces `MinimalPublicValues`. Its accessors expose success, failure, height, and validation error information through `state` and `return_code`.

`State::public_claim` and `State::continuation_bytes` are removed. A consumer needing chain information accesses `PublicValues.state` directly. Store helpers returning `Claim` must be removed or replaced with an existing complete type according to their callers.

### Guest

The guest no longer creates or hashes a `Claim`, `ContinuationData`, or continuation digest. It hashes the complete prior state with verifier key and success code, verifies recursion, updates state, serializes one final `PublicValues`, and commits it.

### Controller

The controller builds expected proof output as `PublicValues::new(expected_state, verifier_key, return_code).to_bytes()`. It no longer independently reconstructs a claim or continuation digest.

### Host

Previous proof validation and execution result validation parse `PublicValues`. Height is `public_values.state.height`. Error handling remains based on `return_code`.

### Store

`Claim` producing helpers require caller analysis. They must not be retained merely as compatibility wrappers. If callers only need chain work, hashes, or height, they should access those fields from the existing record or complete `State` directly.

## Protocol properties

Publishing all 52 continuation bytes binds them directly through the proof's public values. Hashing those bytes first does not produce a stronger binding. It only reduces public output by 20 bytes and can hide the continuation values. Neither benefit justifies maintaining multiple authenticated field partitions here.

With the complete state in public values, adding or moving a field in `State` necessarily changes the public value bytes and recursive hash. There is no separate list where the field can be forgotten.

## External compatibility

The Solidity contract under `contrib/contracts` already expects a different 212 byte public value format and is incompatible with the current 165 byte Rust format. This migration does not create that incompatibility, but it also does not resolve it. Contract support requires a separate explicit migration or wrapper design and is outside this implementation unless requested.

The local wiki and `docs/public-boundary-proposal.md` describe older formats and are stale relative to this decision.

## Verification requirements

1. Assert `State` remains exactly 152 bytes.
2. Assert `PublicValues::SIZE == 185`.
3. Round trip success and every valid failure return code.
4. Reject incorrect public value lengths and invalid return codes.
5. Assert serialized state bytes are exactly `bytemuck::bytes_of(&state)`.
6. Compare optimized guest SHA-256 of the 185 byte format against standard SHA-256 or SP1 public value hashing.
7. Verify controller expected bytes match guest output.
8. Run core, SHA-256, controller, host, store, and workspace checks/tests.
