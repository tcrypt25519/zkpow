# Duplicate Computation Audit

This note covers the current `crates/guest` and `crates/core` code with an emphasis on repeated computation and repeated validation in the prover hot path.

I focused on places where the code proves or checks the same fact more than once, and on places where one derived value is converted back into another derived value when the earlier form was already available.

## Executive Summary

The biggest concrete duplicate work I found in the current hot path is the per-header difficulty expansion in `State::next_inner`: the code computes the expanded target from `next_nbits`, then computes it again during PoW validation. That is a real cycle leak in the core validation loop.

The next most obvious duplicate is at the guest/core boundary: `crates/guest/src/main.rs` pre-checks header payload length, then `InputMut::parse` repeats the same length logic anyway. That is smaller than the target duplication, but it is exactly the kind of repeated guard path you described.

I did **not** find the specific retarget round-trip you were worried about in the current retarget code. The retarget path already computes `new_target`, then derives `next_nbits` and `next_work` from that same `new_target` directly. The main remaining target/bits/work duplication is in the per-header validation path, not the retarget boundary.

## Findings

### 1. Hot-path target expansion happens twice per header

**Where**

- [crates/core/src/lib.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/lib.rs:578)
- [crates/core/src/lib.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/lib.rs:645)
- [crates/core/src/lib.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/lib.rs:667)
- [crates/core/src/lib.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/lib.rs:1136)

**What happens**

`State::next_inner` snapshots:

- `required_nbits = self.next_nbits`
- `required_target = self.next_target()`
- `required_work = self.next_work`

But `self.next_target()` is just `bits_to_target(self.next_nbits)`.

Later in the same call, PoW validation does:

- `hash_meets_target(block_hash, required_nbits)`

and `hash_meets_target` immediately does `bits_to_target(nbits)` again.

So the same difficulty is expanded from compact form twice in one header-validation step:

1. once for `required_target`
2. once again inside `hash_meets_target`

**Why it matters**

This is on the core per-header path. It is the cleanest remaining example of repeated derived-value computation in the consensus loop.

**What to change**

The smallest change would be to make PoW validation operate on the expanded target that `next_inner` already computed. For example:

- add `hash_meets_expanded_target(hash, target)`
- keep `hash_meets_target(hash, nbits)` only as a convenience wrapper
- call the expanded-target form from `next_inner`

That removes one `bits_to_target` per header without changing the serialized state shape.

**Stronger design option**

If this area keeps accumulating special cases, consider a small internal `DifficultyContext` or similar bundle containing:

- `bits`
- `target`
- `work`

Then compute that bundle once when difficulty changes and pass it through validation. That would make the coupling between these three values explicit.

### 2. Guest pre-validates header payload length, then core parses and validates it again

**Where**

- [crates/guest/src/main.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/guest/src/main.rs:67)
- [crates/core/src/input.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/input.rs:272)

**What happens**

`parse_input` in the guest does this first:

- compute `min_len = STATE_SIZE + RECURSIVE_PROOF_SIZE`
- if the payload is long enough to contain state + proof, check whether the remaining bytes are a multiple of `NEW_HEADER_SIZE`
- on failure, commit `HeaderPayloadLengthInvalid`

Then it calls `InputMut::parse`, which repeats the same overall structure:

- ensure the buffer is at least `STATE_SIZE + RECURSIVE_PROOF_SIZE`
- split state/proof/header regions
- check whether the header region length is a multiple of `NEW_HEADER_SIZE`

**Why it matters**

This is a direct duplicate guard path in the guest entrypoint. It is not as expensive as the per-header target duplication, but it is exactly the same shape of repeated validation work you called out.

**What to change**

Unify the logic so the guest can still commit the authenticated state on `HeaderPayloadLengthInvalid` without redoing the length math in two layers. A few possible shapes:

- make `InputMut::parse` return a richer error that the guest can catch and map to `commit_header_payload_length_error`
- factor the shared wire-splitting logic into a helper that returns `(state_bytes, proof_bytes, header_bytes)` plus a structured length-status result
- add a guest-oriented parse entrypoint that returns either parsed input or a recoverable `HeaderPayloadLengthInvalid` carrying the already-borrowed state

The important thing is to keep one source of truth for the wire-length checks.

### 3. `InputRef::parse` and `InputMut::parse` duplicate the same wire parsing logic

**Where**

- [crates/core/src/input.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/input.rs:197)
- [crates/core/src/input.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/input.rs:270)

**What happens**

The borrowed immutable parser and the borrowed mutable parser both do the same work:

- locate state bytes
- locate proof bytes
- validate proof length
- validate header payload divisibility
- parse header slice
- enforce the genesis placeholder rule

They differ mainly in whether the state is borrowed as `&State` or `&mut State`.

**Why it matters**

This is definition-level duplication more than runtime duplication, but it matters because this is exactly the kind of parsing code that gets performance and correctness tweaks over time. Keeping two copies increases the chance that a future optimization or invariant check lands in one path and not the other.

**What to change**

Factor out the shared wire-layout split and shared validation. The split helper could return byte subslices, and each parser could then do only its borrow-specific step:

- immutable path: `State::ref_from_bytes`
- mutable path: `mut_from_bytes::<State>`

That would reduce maintenance duplication without forcing a larger API change.

### 4. The retarget path itself is already in the better shape

**Where**

- [crates/core/src/lib.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/lib.rs:692)

**What I checked**

I looked specifically for the pattern:

1. derive `target`
2. derive `bits`
3. derive `work` by going back through `bits`

The current retarget code does **not** do that. It does:

1. `new_target = retarget_target(required_target, ...)`
2. `self.next_nbits = target_to_bits(new_target)`
3. `self.next_work = new_target.work()`

So the retarget boundary is already using the expanded target as the source of truth for both compact bits and work.

**Why this note matters**

This narrows the optimization target. The remaining waste is mostly:

- repeated `bits -> target` expansion in the per-header path
- structural parse duplication at the guest/core boundary

not the retarget block itself.

## Secondary Improvements

### 5. `hash_meets_target` appears to reject a valid boundary case

**Where**

- [crates/core/src/lib.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/lib.rs:1136)

**What happens**

`hash_meets_target` returns `true` only when:

- `u256_le(hash, target) == Ordering::Less`

That rejects the case where `hash == target`.

Bitcoin PoW validity is `hash <= target`, not `hash < target`.

**Why it matters**

This is not a duplication issue. It is a correctness issue. A block whose hash is exactly equal to the target should be accepted.

**What to change**

Accept both `Less` and `Equal`, and add a direct boundary test for that exact case.

### 6. `InputError` still advertises recursive-proof invariants that the parser no longer enforces

**Where**

- [crates/core/src/input.rs](/Users/tcrypt/code/github.com/tcrypt25519/bitcoin-header-chain/crates/core/src/input.rs:71)

**What happens**

`InputError` still includes:

- `MissingRecursiveProof`
- `UnexpectedRecursiveProof`

but the current parse path never constructs either variant.

The parser always reads the recursive-proof metadata region, and the guest decides later whether to verify it based on `state.height`.

**Why it matters**

This is mostly clarity debt, but it weakens the signal about where the real invariant lives.

**What to change**

Either:

- remove those variants if they are obsolete, or
- restore explicit parser-side enforcement if they still reflect intended protocol rules

## Suggested Order If You Decide To Change Things

1. Remove the duplicate `bits_to_target` in `next_inner` / PoW validation.
2. Collapse the guest pre-check and `InputMut::parse` length logic into one source of truth.
3. Factor shared wire parsing out of `InputRef::parse` and `InputMut::parse`.
4. Fix the `hash <= target` correctness edge if it is still present when you pick this work up.

## Bottom Line

The main hot-path duplicate I would attack first is the repeated compact-target expansion in `State::next_inner`. The main structural duplication I would attack second is the guest parse pre-check plus core parse re-check.

The retarget path itself is already closer to the right model than you suspected. The current waste is mostly in per-header validation and parse-path layering.
