---
id: epic-transaction-inclusion
kind: epic
stage: drafting
tags:
  - consensus
  - security
parent: null
depends_on:
  - epic-consensus-guest-core
release_binding: null
gate_origin: null
created: 2026-11-05
updated: 2026-11-05
---

# Transaction Inclusion Verification

## Brief
This epic delivers the cryptographic mechanics for verifying Bitcoin transaction inclusion proofs against the canonical header chain certified by zkpow. It allows light clients and cross-chain contracts to prove that a specific transaction was confirmed in a block with a given amount of accumulated chain work.

The inclusion mechanism anchors to the 32-byte Merkle root committed inside the block headers verified by zkpow. It provides verification algorithms for standard Bitcoin Merkle branch proofs (SPV proofs), verifying transaction hashes up to the committed block header Merkle root and checking confirmation depth against tip height and cumulative work.

This epic does not transform zkpow into a monolithic transaction indexer or on-demand proof-serving API; it supplies the mathematical verification tools to validate user-supplied inclusion proofs against proven headers.

## Foundation references
- `docs/VISION.md` — Core Capabilities, Target Audience (Cross-Chain Bridges & Relayers)
- `docs/SPEC.md` — §1 Consensus Scope, §3 Guest Input Stream Protocol (Merkle Roots in Compressed Headers)
- `docs/ARCHITECTURE.md` — §2.5 Guest Program (Merkle Root Binding)
- `docs/PRINCIPLES.md` — Principle 1 (Strict Consensus Truth)

## Anticipated child features
- `feature-merkle-branch-verifier`: Verification algorithm for standard Bitcoin transaction Merkle branch inclusion proofs.
- `feature-spv-confirmation-depth`: Calculating confirmation depth and cumulative work atop the block containing the verified transaction.
