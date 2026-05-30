# SP1 Deferred Artifact Leak — Investigation & Fix

**Status:** Fixed in SP1 fork at `crates/prover/src/worker/prover/deferred.rs`.
**Impact:** ~192 MB accumulation per recursive proof iteration (IVC batches 2+).
**Root cause:** A serialized witness artifact was uploaded to the in-memory store but
never deleted after use.

---

## Background: SP1's In-Memory Artifact Store

SP1's `CpuProver` uses an `InMemoryArtifactClient` — a `HashMap<String, Vec<u8>>`
wrapped in a `RwLock`. Every piece of data that passes between proving stages
(stdin, execution output, shard proofs, compressed proofs, witness data) is
serialized with `bincode`, given a UUID key, and inserted into this map.

Entries survive until someone explicitly calls `try_delete`, `delete_batch`, or
`remove_ref` on them. There is no GC, no reference counting, and no automatic
expiry. If a creator forgets to delete an entry, the bytes stay in the map
forever for that prover instance's lifetime.

Most artifacts are tracked by the controller (`mod.rs:514-529`) and cleaned up
after the proof is assembled:

```
common_input_artifact
stdin_artifact
execution_output_artifact
core_proof_artifact          (if Core mode)
compress_proof_artifact      (if Compressed mode)
shrinkwrap_proof_artifact
groth16_proof_artifact       (if Groth16 mode)
plonk_proof_artifact         (if Plonk mode)
```

One artifact was missing from this list.

---

## The Deferred / IVC Path

When a proof carries a recursive continuation (i.e., every zkpow batch after
genesis), SP1 uses a "deferred" recursion step to verify the prior proof inside
the new circuit. The path is:

```
controller/deferred.rs :: DeferredInputs::emit_deferred_tasks()
  → artifact_client.create_artifact()          // allocate key
  → artifact_client.upload(&deferred_data, …)  // insert ~180 MB of witness data
  → submit RecursionDeferredTaskRequest to worker
```

The witness data inside `SP1DeferredData` is:

```rust
pub struct SP1DeferredData {
    pub input: SP1ShapedWitnessValues<…>,   // vks_and_proofs — the prior proof shard data
    pub vk_merkle_proofs: Vec<MerkleProof<…>>,
    pub start_reconstruct_deferred_digest: [SP1Field; POSEIDON_NUM_WORDS],
    pub deferred_proof_index: SP1Field,
}
```

`SP1ShapedWitnessValues.vks_and_proofs` holds the full proof object from the
previous batch. This is substantial — roughly 150-200 MB serialized, which is
consistent with what was observed.

The worker that consumes this artifact lives in
`worker/prover/deferred.rs :: SP1DeferredWorker::call()`:

```rust
async fn call(&self, input: RecursionDeferredTaskRequest) -> Result<TaskMetadata, TaskError> {
    let (common_input, deferred_data) = tokio::try_join!(
        self.artifact_client.download::<CommonProverInput>(&common_input),
        self.artifact_client.download::<SP1DeferredData>(&deferred_data),   // <-- download
    )?;

    // ... use deferred_data to build the circuit witness and prove ...
}
```

After `download`, the artifact's bytes have been deserialized into Rust values.
The originating `Vec<u8>` in the HashMap is no longer needed. But there was no
`try_delete` call. The HashMap entry persisted forever.

---

## Why the Controller Didn't Clean It Up

The controller's final cleanup list is built at the *start* of `prove()` from
artifacts it creates directly. The `deferred_data` artifact is created *inside*
`emit_deferred_tasks()`, which is called from a spawned sub-task during proof
execution — after the cleanup list has been assembled. There is no mechanism for
sub-tasks to register artifacts for later cleanup. The controller simply never
knew about it.

The `common_input` artifact that is also passed to the deferred worker *is*
cleaned up, because it was created and registered in the controller's own scope
before proof execution begins.

---

## The Fix

In `SP1DeferredWorker::call()`, clone the artifact identifier before consuming
it in the `download` call, then immediately delete the store entry:

```rust
// Clone the artifact ID before moving deferred_data into the download call.
let deferred_data_artifact = deferred_data.clone();

let (common_input, deferred_data) = tokio::try_join!(
    self.artifact_client.download::<CommonProverInput>(&common_input),
    self.artifact_client.download::<SP1DeferredData>(&deferred_data),
)?;

// Delete the artifact now that we have its contents in memory.
self.artifact_client
    .try_delete(&deferred_data_artifact, ArtifactType::UnspecifiedArtifactType)
    .await?;
```

The `try_delete` name is correct: it removes the `Vec<u8>` from the HashMap and
lets the OS reclaim the pages on the next allocator purge. The deserialized Rust
values in `deferred_data` are still alive for the duration of `call()` and are
dropped naturally when the function returns.

---

## Why It's Safe to Delete Here

The deferred_data artifact:

- Is produced by `emit_deferred_tasks()` solely for the purpose of ferrying the
  witness data from the controller to `SP1DeferredWorker::call()`.
- Has exactly one consumer: `SP1DeferredWorker::call()`.
- Is not referenced by any other part of the pipeline after the download.
- Is not the output artifact (`output`) — the output is a separate `Artifact`
  that carries the finished deferred proof and is read downstream.

The download makes a full deserialized copy. Deleting the serialized bytes from
the store is safe the moment `tokio::try_join!` returns successfully.

---

## Stress Test Results

The `sp1_stress` binary (`crates/host/src/bin/sp1_stress.rs`) proves a minimal
chain of single-header batches in a loop with the prover cached. The fixed-point
memory table measures retained RSS and live heap at each iteration boundary.

**Before fix** (not measured directly, inferred from production runs):
~192 MB per recursive iteration, growing monotonically without bound.

**After fix:**

| Iter | End RSS | Growth |
|------|--------:|-------:|
| 1 (genesis, no recursion) | 6,596 MB | — |
| 2 (first recursive proof) | 6,777 MB | **+181 MB** |
| 3 (second recursive proof) | 6,777 MB | **≈ 0** |

The +181 MB step from iteration 1 to 2 is expected and not a leak. It reflects
the first-time initialization of the IVC path: loading the program for recursive
proof verification, its proving key, and supporting structures into the prover's
internal caches. Because iteration 1 starts at genesis (no prior proof to
verify), this code path is not exercised until iteration 2. The data is reused
for all subsequent iterations, which is why iteration 3 shows zero growth.

---

## Where to Upstream

The fix belongs in the SP1 repository:

```
crates/prover/src/worker/prover/deferred.rs
  SP1DeferredWorker::call()
```

The change is small and localized. The only addition is the `deferred_data.clone()`
before the `try_join!` and the `try_delete` call immediately after. No API
changes, no new types, no behavioral changes to any other path.
