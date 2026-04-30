# Duplicate Logic Review

Scope: `crates/core`, `crates/guest`, and `crates/host`.

This pass focused on places where the same value is derived more than once, where a derived value is written into multiple forms, or where the code serializes the same state repeatedly inside one control-flow path.

## Findings

### 1. `genesis_state` computes genesis work twice

File: [`crates/host/src/util.rs`](./crates/host/src/util.rs)

The host genesis constructor derives the same work value twice:

```rust
chain_work: Target::from(genesis_header.nbits).work(),
next_work: Target::from(genesis_header.nbits).work(),
```

Those two fields start with the same value, so this is pure duplicate computation. `Target::work()` is not a trivial accessor; it expands compact bits into a target and then derives work from that target. This should be bound once and reused.

Suggested shape:

```rust
let genesis_work = Target::from(genesis_header.nbits).work();
```

and then assign `chain_work` and `next_work` from that binding.

### 2. The new witness formats store count information that is already derivable

Files:
[`crates/core/src/input.rs`](./crates/core/src/input.rs)
[`crates/guest/src/main.rs`](./crates/guest/src/main.rs)
[`crates/host/src/proof_pipeline.rs`](./crates/host/src/proof_pipeline.rs)
[`crates/host/src/bin/test_errors.rs`](./crates/host/src/bin/test_errors.rs)

Both `NewHeaderHints` and `MedianTimePastHints` use a `u32` count prefix followed by a packed payload. That count is redundant with the payload length:

- for `NewHeaderHints`, the number of headers is already encoded by `bytes.len() / NEW_HEADER_SIZE`
- for `MedianTimePastHints`, the number of medians is already encoded by `bytes.len() / size_of::<BlockTimestamp>()`
- the host already knows the count when it builds the witness
- the guest already knows the expected median count from `header_hints.headers.len()`

This means the witness carries the same information twice: once in the prefix and once in the payload length. If the goal is to minimize both bytes and parse work, the count field is not necessary.

The current shape is still valid, but it is not minimal. If you want to trim this further, the cleaner format is:

- header hints: raw packed `NewHeader` bytes only
- median hints: raw packed `BlockTimestamp` bytes only

The parser can infer the count from the remaining length and fail if the payload is not an even multiple of the element size.

### 3. The guest serializes the committed state twice in the proof-verification path

File: [`crates/guest/src/main.rs`](./crates/guest/src/main.rs)

`verify_recursive_proof` does:

```rust
let actual_public_values_digest = sha256_264bytes(&state.to_bytes());
```

and then `main` later commits the state again via:

```rust
commit_success(&input_bytes[..STATE_SIZE]);
```

Those two paths are operating on the same authenticated state after `update_genesis_hash()`. The code currently materializes the state bytes twice, once through `State::to_bytes()` and once through the mutable input buffer. That is avoidable.

The lowest-cost shape is to serialize once in `main`, then reuse that serialized buffer both for the recursive-proof digest check and for the success commitment. That keeps the guest on one copy of the state bytes instead of two.

### 4. Host public-values verification serializes the same state twice on mismatch

File: [`crates/host/src/proof_pipeline.rs`](./crates/host/src/proof_pipeline.rs)

`verify_public_values` compares:

```rust
if state.to_bytes() != expected_pv {
```

and then, if the check fails, it calls `state.to_bytes()` again to print the mismatch:

```rust
hex::encode(state.to_bytes())
```

That is duplicated work on the failure path. It is not as hot as the proving path, but it is still avoidable. If you want to keep the byte-level comparison, cache the serialized bytes once. If you want the cleaner typed path, compare `state` to `expected_state` and only serialize for the error message.

## Notes

- I did not see another obvious target/work duplication in the current core transition path after the earlier cleanup.
- The retarget path is still in the better shape: it uses the expanded target once and derives the bits/work from that value rather than re-expanding the compact representation later.
