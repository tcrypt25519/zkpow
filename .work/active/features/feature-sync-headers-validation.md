---
id: feature-sync-headers-validation
kind: feature
stage: implementing
tags:
  - consensus
  - refactor
parent: null
depends_on: []
release_binding: null
---

# Feature: Sync Headers Validation Refactor

## Brief
Refactor header validation and connection in `zkpow-sync-headers` to optimize MTP evaluation and streamline `connect_headers` with explicit `ValidationError` states.

## Files in this cluster
- `crates/sync-headers/src/validation.rs`
- `crates/sync-headers/src/types.rs`
- `crates/cli/src/main.rs`

## Design captured retroactively from existing code
The in-flight changes introduce an explicit `ValidationError` enum (`ChainBroken`, `CompactTargetMismatch`, `ProofOfWorkFailed`, `TimestampNotGreater`), separate individual header validation from slice connection, and streamline the MTP calculation loop during peer sync.
