---
id: idea-controller-batch-timeout
created: 2026-11-04
updated: 2026-11-04
---

# Controller Batch Lease Timeout and Host Heartbeat Protocol

Implement controller batch lease timeout (5 min) and periodic (30s) host heartbeat protocol based on `docs/plans/controller-heartbeat-and-batch-timeout.md`. Expire and reissue pending batches if host fails to report progress, and support graceful prover cancellation upon controller abort signal.
