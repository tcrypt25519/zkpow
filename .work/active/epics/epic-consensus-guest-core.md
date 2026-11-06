---
id: epic-consensus-guest-core
kind: epic
stage: drafting
tags:
  - consensus
  - security
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-11-05
updated: 2026-11-05
---

# Consensus and Guest Proving Core

## Brief
This epic delivers the foundational cryptographic execution core running inside the SP1 zkVM. It encompasses the minimal RISC-V guest program and its underlying consensus state machine, executing pure Satoshi-era proof-of-work validation across Bitcoin block headers.

The core verifies double-SHA-256 header hashes against compact difficulty targets, enforces the 11-block Median Time Past (MTP) rule without in-guest sorting, validates 2,016-block difficulty retargets using verified unconstrained quotient multiplication, and recursively verifies previous batch proofs via `verify_sp1_proof`. The guest attests to chain state by committing a strict 185-byte public values payload (`State`, guest `verifier_key`, and return code).

This epic does not cover external host orchestration, P2P network synchronization, or database persistence; it represents the self-verifying cryptographic boundary where the system's soundness strictly resides.

## Foundation references
- `docs/VISION.md` — Genesis-to-Tip Chain Proof, Satoshi-Era Consensus Parity
- `docs/SPEC.md` — §1 Consensus Verification Scope, §2 Public Values Specification, §3 Guest Input Stream Protocol
- `docs/ARCHITECTURE.md` — §1 System Topology (SP1 zkVM Sandbox), §2.5 The Guest Program, §3 Zero-Knowledge Optimizations
- `docs/PRINCIPLES.md` — Principle 1 (Strict Consensus Truth), Principle 2 (Soundness Rests Exclusively on Guest ZKP), Principle 3 (ZK Cycle Economy)

## Anticipated child features
- `feature-guest-recursion-induction`: Recursive proof verification inside SP1 via `verify_sp1_proof` and public value reconstruction.
- `feature-guest-header-decompression`: Parsing 44-byte compressed header streams and reconstructing full 80-byte wire headers from internal state.
- `feature-guest-mtp-optimization`: 11-element timestamp ring with static slot lookup for ordered windows and linear rank checking for hinted disordered windows.
- `feature-guest-retarget-arithmetic`: Unconstrained 256-bit quotient calculation verified via single 256-bit multiplication syscall.
