Investigate possible cross-batch memory growth in the Bitcoin header local dev harness.

Context:
We recently replaced the old flow:
- Rust binary ran exactly one batch, wrote results, exited
- shell scripts configured params, moved/collected results, and chained multiple batches

with a new consolidated Rust binary:
- takes batch size + number of batches
- runs batches sequentially
- tracks/writes the data that shell scripts previously handled

The new binary worked for normal sizes. Single large batches also worked up to roughly 52,000 headers per batch. The issue appears when multiple large batches continue later into the chain: memory climbs until OOM / process kill / machine unusable on a 64GB machine.

Working hypothesis:
During a batch, high memory is expected because proving generates a large execution trace and shards before aggregation. Memory should drop after shards are produced and the trace is no longer needed. However, the high-water mark and/or post-batch baseline appears to increase across batches of the same size. Since batch sizes are multiples of the 2016 difficulty-adjustment period, each batch should have the same number of adjustment cases. That suggests possible memory/state retention between batches.

Primary goal:
Validate or invalidate the hypothesis that the new multi-batch Rust harness leaks or retains memory across batch boundaries.

Start here:
1. Inspect the existing profile/sample data under `leak/`.
2. Summarize what files are present and what each one can tell us.
3. Look specifically for retained allocations, growing caches, references held across batch loop iterations, aggregation/shard objects not dropped, output buffers, proof artifacts, logging/tracing state, or global/static state.

Then:
4. Inspect the Rust code paths for the new consolidated harness:
   - batch loop orchestration
   - per-batch input/header loading
   - prover execution trace creation
   - shard creation/aggregation
   - result writing
   - any caches, vectors, maps, channels, async tasks, global state, or Arcs/Rcs that may survive between batches
5. Compare the old single-batch behavior to the new multi-batch behavior where useful.

Reproduction rules:
- Do not jump to huge runs first.
- Avoid runs likely to consume all memory.
- Prefer small controlled runs that can show whether baseline RSS rises batch-over-batch.
- Capture batch number, batch size, starting RSS, peak RSS, post-batch RSS, and whether difficulty adjustment boundaries are included.
- Ask before running anything expected to be expensive or risky.

Use Claude Code’s normal tools:
- Use grep/glob/read to find harness/prover/batch-loop code.
- Use bash only for safe inspection and controlled profiling.
- Use TodoWrite to track investigation steps.
- If a debugger/performance subagent exists, use it for profile interpretation; otherwise continue directly.

Deliverables:
1. Short profile-data summary: what the current samples do or do not prove.
2. Evidence table: symptoms by batch/run if extractable.
3. Top 3 likely retention points in code with file/function references.
4. Whether the leak hypothesis is supported, contradicted, or still inconclusive.
5. Next minimal recommended action:
   - code fix candidate,
   - added instrumentation,
   - or smallest safe repro command.
6. Do not implement a fix until the likely cause is identified, unless the change is tiny instrumentation for measurement.
