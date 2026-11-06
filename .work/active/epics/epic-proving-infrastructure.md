---
id: epic-proving-infrastructure
kind: epic
stage: drafting
tags:
  - infra
  - perf
parent: null
depends_on:
  - epic-pipeline-orchestration
release_binding: null
gate_origin: null
created: 2026-11-05
updated: 2026-11-05
---

# Proving Infrastructure and Live Tip Cadence

## Brief
This epic delivers the physical and cloud execution infrastructure required to prove the Bitcoin header chain and maintain live currency with the chain tip. It establishes automated GPU provisioning, offsite artifact synchronization, and regular (~hourly) proving execution.

The infrastructure orchestrates runs across local Apple Silicon GPUs and high-throughput datacenter GPUs (Vast.ai RTX 4090 / H100 instances). It provides bootstrap scripts for pulling historical header databases from Backblaze B2 S3 storage, confirms flat memory footprint and zero VRAM spikes during multi-batch proving runs, validates mid-batch interrupt-and-resume resilience, and synchronizes generated proof artifacts continuously to remote object storage.

This epic does not introduce distributed cluster consensus or decentralized prover markets; it hardens single-node proving throughput, benchmark stability, and continuous near-real-time updates.

## Foundation references
- `docs/VISION.md` — Near-Real-Time Currency, Anti-Vision (No Distributed Prover Cluster)
- `docs/SPEC.md` — §4 Batch Sizing and Execution Performance (Sublinear Proving Growth)
- `docs/ARCHITECTURE.md` — §4 Fault Tolerance and Recovery (Deterministic Resume, Offsite Sync)
- `docs/PRINCIPLES.md` — Principle 3 (ZK Cycle Economy), Principle 4 (Unconditional Crash Resilience & Idempotence)

## Anticipated child features
- `feature-vast-gpu-orchestration`: Vast.ai instance lifecycle management, environment configuration, and CUDA runner provisioning.
- `feature-offsite-b2-sync`: Continuous background replication of proof artifacts and header snapshots to Backblaze B2 S3 storage.
- `feature-sublinear-batch-benchmarks`: Empirical verification of sublinear prove-time scaling across 2,016, 4,032, and 8,064 headers on datacenter GPUs.
- `feature-live-tip-daemon`: Periodic (~hourly) synchronization and batch proving daemon keeping tip proofs current.
