---
id: epic-pipeline-orchestration
kind: epic
stage: drafting
tags:
  - consensus
  - refactor
parent: null
depends_on:
  - epic-consensus-guest-core
  - epic-header-sync-store
release_binding: null
gate_origin: null
created: 2026-11-05
updated: 2026-11-05
---

# Controller and Prover Pipeline Orchestration

## Brief
This epic delivers the coordinator and pipeline glue connecting the SQLite header database to the SP1 zkVM prover. It orchestrates batch extraction, witness assembly, expected state precomputation, untrusted host execution, and operational proof verification.

The controller determines batch boundaries (2,016, 4,032, or 8,064 headers), constructs the guest input stream (compressed headers, sparse median hints, and recursive proof witnesses), and computes the expected 185-byte public values commitment. When the host completes a batch proof, the controller verifies public values bit-for-bit, executes native SP1 proof verification, and transactionally updates the run ledger. It also enforces batch leases and host heartbeat monitoring to catch stalls or evicted workers.

This epic treats the host runner as adversarial and untrusted. The controller's own precomputation and verification exist purely to fail fast and optimize compute usage, adhering to the principle that cryptographic security rests entirely within the guest ZKP.

## Foundation references
- `docs/VISION.md` — Core Capabilities, Anti-Vision
- `docs/SPEC.md` — §2 Public Values Specification, §3 Guest Input Stream Protocol, §4 Batch Sizing
- `docs/ARCHITECTURE.md` — §1 Trust Boundary Model, §2.3 The Controller, §2.4 The Host Prover, §4 Fault Tolerance & Recovery
- `docs/PRINCIPLES.md` — Principle 2 (Controller Validation Is Strictly Operational), Principle 4 (Crash Resilience)

## Anticipated child features
- `feature-controller-batch-scheduler`: Batch boundary calculation, variable batch sizing (2016..8064), and expected `State` serialization.
- `feature-controller-lease-heartbeat`: Batch lease timeout tracking, host periodic heartbeat protocol, and worker eviction handling.
- `feature-host-prover-pipeline`: Untrusted host STDIN streaming, SP1 environment execution, and compressed proof artifact persistence.
- `feature-controller-verification-ledger`: Independent SP1 verification of produced proofs and transactional commit to SQLite `runs`.
