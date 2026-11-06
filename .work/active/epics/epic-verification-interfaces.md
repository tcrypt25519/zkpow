---
id: epic-verification-interfaces
kind: epic
stage: drafting
tags:
  - security
  - docs
parent: null
depends_on:
  - epic-consensus-guest-core
  - epic-pipeline-orchestration
release_binding: null
gate_origin: null
created: 2026-11-05
updated: 2026-11-05
---

# Verification Interfaces and On-Chain Groth16 Integration

## Brief
This epic delivers the verification artifacts, libraries, and wrapping mechanisms consumed by external light clients, bridges, and smart contracts. It guarantees that any party can independently verify proof-of-work validity from genesis to tip in milliseconds without trusting zkpow infrastructure.

The verification layer provides standalone CLI inspection commands and lightweight Rust verification crates for native SP1 compressed recursive proofs. For smart contract consumers, it wraps SP1 compressed proofs into Groth16 SNARKs compatible with Succinct's standard on-chain Ethereum verifier contracts (`ISP1Verifier`).

This epic does not author, deploy, or govern custom smart contracts; it adheres strictly to off-the-shelf SP1 verifier contracts, eliminating bespoke bridge contract management.

## Foundation references
- `docs/VISION.md` — Universal Verifier Compatibility, Anti-Vision (No Custom Smart Contract Suite)
- `docs/SPEC.md` — §2 Public Values Specification, §5 Verification Interfaces (SP1 Native & EVM Groth16)
- `docs/ARCHITECTURE.md` — §1 Trust Boundaries, §2.4 Host Prover (Proof Inspection)
- `docs/PRINCIPLES.md` — Principle 5 (Standard Verifier Compatibility)

## Anticipated child features
- `feature-cli-proof-inspector`: Command-line inspection and validation of public values commitments and verifier key digests.
- `feature-sp1-verifier-crate`: Minimal, standalone Rust verification library for client applications.
- `feature-groth16-evm-wrapper`: Automated packaging of recursive SP1 proofs into Groth16 SNARKs targeting standard `ISP1Verifier` contracts.
