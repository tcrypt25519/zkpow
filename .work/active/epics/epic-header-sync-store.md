---
id: epic-header-sync-store
kind: epic
stage: drafting
tags:
  - consensus
  - infra
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-11-05
updated: 2026-11-05
---

# Header Synchronization and Storage Ledger

## Brief
This epic delivers the Bitcoin P2P header ingestion pipeline and SQLite storage ledger. It provides the mechanism for fetching raw 80-byte Bitcoin block headers from network peers, validating initial syntactic and proof-of-work criteria, precomputing zero-knowledge execution metadata, and maintaining historical headers in a transactional store.

The synchronization pipeline precomputes cumulative chain work, evaluates Median Time Past over 11-block sliding windows, and detects non-decreasing timestamp runs to generate sparse median hints required by the guest prover. The storage engine tracks raw headers, calculated metrics, and per-run execution state to enable deterministic crash recovery and historical batch extraction.

This epic does not invoke the SP1 prover or verify ZK proofs; its responsibility is feeding clean, indexed, and hint-augmented header data to the controller.

## Foundation references
- `docs/VISION.md` — The Problem (Bandwidth & Storage Overhead)
- `docs/SPEC.md` — §3 Guest Input Stream Protocol (Median Hints & Compressed Headers)
- `docs/ARCHITECTURE.md` — §1 System Topology (Bitcoin Network & SQLite Storage), §2.1 Synchronizer, §2.2 Storage Engine
- `docs/PRINCIPLES.md` — Principle 4 (Unconditional Crash Resilience & Idempotence)

## Anticipated child features
- `feature-p2p-header-sync`: Resilient Bitcoin P2P peer connection management, header discovery, and bulk downloading.
- `feature-sync-validation-mtp`: Header validation and non-decreasing timestamp run tracking during peer sync.
- `feature-sqlite-store-ledger`: Schema management, transactional batch querying, run state ledgering, and embedded in-memory test fixtures.
