# Handoff: bitcoin-header-chain Memory Leak Investigation

**Session**: Codex `019e5615-3d9b-71a2-9f48-83df51d9fc34`
**Date**: 2026-05-23
**Branch**: `codex/gate-jemalloc-memory-diagnostics` (3 commits ahead of `review`)
**PR**: https://github.com/tcrypt25519/zkpow/pull/3

---

## The Problem

When proving multiple batches of Bitcoin headers in a single process (via `MAX_BATCHES > 1`), memory grows across batch boundaries and does not fully return. Each batch's `prove_compressed` phase uses ~7-8 GB, and after artifacts are dropped, the resident baseline ratchets upward instead of returning to the pre-batch level.

This is a real problem for continuous proving: the process eventually OOMs or becomes unusable.

---

## What Was Done

### 1. Jemalloc Feature Gating (COMMITTED: `3cdc43f`)

The jemalloc dependencies were unconditionally included, breaking stable Rust builds. Fixed by:
- Made `jemalloc`, `jemalloc-ctl`, `jemallocator` optional behind `memory-diagnostics` feature
- `memory-diagnostics` requires nightly Rust (uses `#![feature(...)]`)
- Default stable build works again

### 2. Jemalloc Diagnostics Infrastructure (COMMITTED: `7dce94b`)

Added allocator-aware memory snapshots:
- `memory_profiler.rs` — RSS snapshots + jemalloc stats (`allocated`, `active`, `resident`, `mapped`, `retained`)
- `maybe_dump_allocator_stats()` — writes full jemalloc JSON dump to `logs/jemalloc_*.json`
- Per-phase RSS logging in `proof_pipeline.rs` via `timed_sync`/`timed_async` wrappers
- Per-batch memory summary in `session_runner.rs` (start RSS, end RSS, delta)
- `MEMORY_PROFILING=1` — periodic RSS snapshots to `logs/mem_<ts>.log`
- `MEMORY_DIAGNOSTICS_DUMP=1` — jemalloc stats dumps at batch boundaries

### 3. Prover Reuse Across Batches (COMMITTED: `44e1f55`)

The CPU prover was being rebuilt from scratch every batch. Fixed by:
- `PreparedProver` enum (Cpu or Cuda) cached in a `OnceLock<AsyncMutex<Option<...>>>`
- `get_prepared_prover()` checks cache hit by config (backend + device ID)
- `generate_and_save_proofs()` calls `get_prepared_prover()` instead of building fresh
- Eliminated the ~40s per-batch prover rebuild overhead
- Removed duplicate `continuous-prover` binary; `zkpow-host` is now the single entry point
- Both binaries now route through `session_runner::run_batch_session()`

### 4. Post-Drop Measurement Bug Fix (IN WORKING TREE, NOT COMMITTED)

The "after dropping proof artifacts" RSS snapshot was taken before `ProofArtifacts` was actually dropped. This overstated the post-batch baseline. Fixed by adding `drop(artifacts)` before the end-of-batch measurement in `session_runner.rs`.

### 5. CUDA Refactoring (IN WORKING TREE, NOT COMMITTED)

Moved CUDA preflight checks from inline code in `proof_pipeline.rs` to a separate `cuda_env.rs` module. Removed `NumericVersion`, `ComputeCapability`, `CudaGpuInfo` structs from `proof_pipeline.rs`.

---

## Current Evidence

### What We Know

1. **`build_cpu_prover` is the biggest single allocation**: ~6.7 MB → 6.73 GB allocated, ~7 GB resident. This is now cached and only happens once.

2. **After prover reuse, memory still ratchets**: Even with the cached prover, batch 2 starts from a higher resident baseline than batch 1 ended at. The growth is smaller than before, but it still exists.

3. **`retained` stays at 0**: jemalloc's `retained` counter never moves, meaning this is NOT jemalloc hoarding released virtual memory. The memory is in live allocator-managed state.

4. **`resident >> active >> allocated`**: The gap between these counters points to pages left resident after SP1's large transient allocations — the OS hasn't reclaimed the physical pages even though the allocator considers them free.

5. **The growth is inside SP1's proving layer**, not in host application code. The host-side setup phases (`resolve_current_state`, `load_header_records`, `decode_headers`, `simulate_expected_state`) barely move the allocator counters.

### What We Don't Know

- **Exactly which SP1 data structures survive across proofs.** The Codex session traced through `CpuProver` → `Arc<SP1LocalNode>` → `prove_with_mode` → the compression tree and recursion worker, but didn't isolate the specific allocations.

- **Whether the ratchet is in the artifact client.** The in-memory artifact client stores `Vec<u8>` blobs. There's one wasteful copy in `upload_raw`, but it alone doesn't explain the persistent growth.

- **Whether the corrected measurement (after the `drop(artifacts)` fix) materially changes the numbers.** The Codex session was interrupted before the corrected repro completed.

---

## What Needs To Happen Next

### Immediate (finish what was started)

1. **Run the corrected 2-batch repro** with `drop(artifacts)` fix and prover reuse:
   ```bash
   cd ~/code/github.com/tcrypt25519/bitcoin-header-chain
   cargo build --release -p zkpow-host
   MAX_BATCHES=2 NUM_HEADERS=10 MEMORY_PROFILING=1 ./target/release/zkpow-host
   ```
   Extract the `batch_memory_after_drop` lines from `logs/run.jsonl`. Compare batch 1 end RSS vs batch 2 end RSS. This is the ground truth for whether the leak is real after the fixes.

2. **Commit the working tree changes.** The `drop(artifacts)` fix, `cuda_env.rs` refactor, and `DB_PATH` override are all in the working tree but uncommitted. Merge or rebase onto `codex/gate-jemalloc-memory-diagnostics` and push.

### Short-term (isolate the SP1-side growth)

3. **Add per-phase jemalloc snapshots around `prove_compressed`.** The `timed_async` wrapper already logs start/end RSS. Extend it to also log jemalloc `allocated`/`active`/`resident`/`mapped` at phase boundaries. This will show exactly which sub-phase of compressed proving leaves memory behind.

4. **Test the artifact client hypothesis.** Add a `maybe_dump_allocator_stats()` call immediately before and after `prove_compressed` in `generate_compressed_proof_with_prover`. If the artifact client is the source, the `allocated` delta will match the artifact sizes.

5. **Check if SP1's `SP1LocalNode` accumulates state.** The `CpuProver` is `Arc<SP1LocalNode>`. After each proof, call `maybe_dump_allocator_stats()` and compare. If the node itself grows, the fix is in SP1 (upstream patch or fork).

### Medium-term (fix or mitigate)

6. **If the growth is in SP1's artifact client**: Look at whether `upload_raw` clones can be eliminated, or whether the client can be reset between proofs without rebuilding the prover.

7. **If the growth is in SP1's compression tree / recursion worker**: These have explicit `try_delete_proofs` cleanup paths. Check whether they're being called for all intermediate artifacts, or whether some are left behind.

8. **If the growth is OS-level page retention (resident >> active)**: Consider calling `jemalloc_ctl::mallctl::dirty_decay` or `purge` between batches to force the OS to reclaim pages. This is a mitigation, not a fix.

9. **Nuclear option**: If the in-process leak can't be fixed, the multi-process approach (fresh `zkpow-host` per batch via `prove-chain.sh`) is the reliable fallback. The current codebase already supports both paths.

---

## Key Files

| File | Role |
|------|------|
| `crates/host/src/proof_pipeline.rs` | Core proof generation. `PreparedProver` cache at line ~631. `generate_and_save_proofs()` at line ~692. |
| `crates/host/src/session_runner.rs` | Multi-batch loop. `run_batch_session()`. `drop(artifacts)` fix at line ~111. |
| `crates/host/src/batch_runner.rs` | Thin wrapper: `run_single_batch()` → `generate_and_save_proofs()` |
| `crates/host/src/memory_profiler.rs` | RSS + jemalloc snapshots. `current_rss_kb()`, `maybe_dump_allocator_stats()`, `spawn_mem_logger()` |
| `crates/host/src/cuda_env.rs` | CUDA preflight checks (moved from proof_pipeline) |
| `crates/host/src/main.rs` | `zkpow-host` binary — now the single entry point |
| `crates/host/src/bin/continuous_prover.rs` | **DELETED** in working tree (was redundant) |
| `scripts/prove-batch.sh` | Single-batch shell wrapper → `continuous-prover` binary |
| `scripts/prove-chain.sh` | Multi-batch shell loop → `prove-batch.sh` per batch |

## Environment Variables for Diagnostics

| Variable | Effect |
|----------|--------|
| `MEMORY_PROFILING=1` | Periodic RSS snapshots to `logs/mem_<ts>.log` |
| `MEMORY_DIAGNOSTICS_DUMP=1` | Jemalloc stats dumps at batch boundaries (requires `--features memory-diagnostics`) |
| `MAX_BATCHES=N` | Run N batches in one process |
| `NUM_HEADERS=N` | Headers per batch (use small values like 10 for fast repros) |
| `RUST_LOG=info` | Enable stderr tracing (JSON log always on) |

## Build Commands

```bash
# Stable (default)
cargo build --release

# With memory diagnostics (nightly only)
cargo +nightly build --release -p zkpow-host --features memory-diagnostics

# Verify both paths
cargo check --workspace --all-targets
cargo +nightly check -p zkpow-host --features memory-diagnostics --all-targets
```

## Known Gotchas

- **Stale binaries**: The Codex session repeatedly hit issues where an old `target/release/zkpow-host` was being used instead of the freshly built one. Always `cargo build --release` before running repros, or check the binary mtime.

- **Stale processes**: The Codex session also had runaway `zkpow-host` processes from previous runs. Always `pkill -f zkpow-host` before starting a new bounded repro.

- **The `continuous-prover` binary still exists in `scripts/`**: The scripts were rewired to call `continuous-prover`, but the binary was deleted from the working tree. The scripts need to be updated to call `zkpow-host` instead, or the binary needs to be restored. The current state is inconsistent.

- **Working tree is dirty**: Many changes are uncommitted. The Codex branch has 3 commits, but the working tree has additional uncommitted changes (drop fix, CUDA refactor, DB_PATH override, script rewiring).

- **DB path**: The default is `headers.db` relative to the host crate manifest. The Codex branch added `DB_PATH` env var override; the working tree has this but it's not committed.

---

## User Context

The user is frustrated with the Codex session because:
1. It spent too much time waiting on builds and explaining instead of producing results
2. It kept asking for permission instead of executing
3. It killed a user's running process by mistake
4. It didn't finish the corrected repro before the session ended
5. It worked in a clean worktree instead of the live repo, making changes invisible

**When continuing this work**: Execute immediately. Don't ask for permission. Run bounded repros with small NUM_HEADERS. Kill stale processes before starting new ones. Commit results. If waiting on a build, do something else productive in parallel.
