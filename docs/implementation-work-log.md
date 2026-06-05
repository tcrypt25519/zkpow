# Public Boundary Implementation Work Log

## Overview

Implementing the 7-step public boundary plan from `docs/public-boundary-implementation-plan.md`.

## Step 1: Harden Recursive Success Handling

**Status:** In progress

**Goal:** Guest rejects failed prior proofs before any public-value layout change.

**Assumptions:**
- The plan says "introduce an explicit typed recursive-start value instead of treating recursive proof metadata alone as enough." For this step, we keep the full `State` public values but add a `return_code` byte to the committed public values so the guest can distinguish success from failure in a prior proof. The `return_code` is the first byte of the failure metadata (0 = success, nonzero = failure).
- The current `RecursiveProof` carries a `public_values_digest` which is a SHA-256 of the committed bytes. For a success proof, the committed bytes are just `State` (264 bytes). For a failure proof, they are `State || FailureMetadata` (269 bytes). The guest currently only checks the digest matches the state bytes (264 bytes), so it would fail to verify a failure proof anyway. However, the plan asks us to make this explicit with a typed check.
- We add a `previous_return_code` field to the `RecursiveProof` struct so the guest can explicitly check it equals 0. This is the "explicit typed recursive-start value" the plan refers to.

**Changes:**
- `crates/core/src/input.rs`: Add `previous_return_code: u8` to `RecursiveProof`
- `crates/core/src/lib.rs`: Update `RECURSIVE_PROOF_SIZE` constant and wire format
- `crates/guest/src/main.rs`: Add check that `previous_return_code == 0`
- `crates/host/src/bin/test_errors.rs`: Add test for building on failed proof
- `crates/host/src/util.rs`: Update `RecursiveProof` construction

**Commit:** TBD
