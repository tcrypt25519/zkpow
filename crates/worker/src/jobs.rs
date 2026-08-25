use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use sp1_sdk::{prelude::*, ExecutionReport};
use tracing::Instrument;

static JOB_COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_job_id() -> u64 {
    JOB_COUNTER.fetch_add(1, Ordering::Relaxed)
}

struct StdinShape {
    items:           usize,
    bytes:           usize,
    proof_witnesses: usize,
}

fn stdin_shape(stdin: &SP1Stdin) -> StdinShape {
    StdinShape {
        items:           stdin.buffer.len(),
        bytes:           stdin.buffer.iter().map(Vec::len).sum(),
        proof_witnesses: stdin.proofs.len(),
    }
}

/// Execute the guest program on `stdin` with deferred proof verification
/// disabled.
///
/// Wraps the call in a `worker_job` tracing span tagged with a process-unique
/// job id and the stdin's shape, and logs completion (elapsed time, cycle
/// count, gas) or failure as events on that span.
///
/// Returns the execution error's `Display` string rather than
/// `sp1_core_executor::ExecutionError` directly: the SDK's `ExecuteRequest`
/// future is not generic over `Prover::Error` the way `ProveRequest` is, and
/// callers here only ever convert the error to a boxed error anyway.
pub async fn execute<P>(prover: &P, elf: Elf, stdin: SP1Stdin) -> Result<(Vec<u8>, ExecutionReport), String>
where
    P: Prover,
{
    let job_id = next_job_id();
    let shape = stdin_shape(&stdin);
    let span = tracing::info_span!(
        "worker_job",
        job_id,
        kind = "execute",
        stdin_items = shape.items,
        stdin_bytes = shape.bytes,
        proof_witnesses = shape.proof_witnesses,
    );
    async move {
        tracing::debug!("execute job started");
        let started = Instant::now();
        match prover.execute(elf, stdin).deferred_proof_verification(false).await {
            Ok((public_values, report)) => {
                tracing::info!(
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    cycles = report.total_instruction_count(),
                    gas = ?report.gas(),
                    "execute job finished"
                );
                Ok((public_values.to_vec(), report))
            },
            Err(err) => {
                tracing::error!(
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    %err,
                    "execute job failed"
                );
                Err(err.to_string())
            },
        }
    }
    .instrument(span)
    .await
}

/// Prove `stdin` in compressed mode.
///
/// Wraps the call in a `worker_job` tracing span tagged with a process-unique
/// job id and the stdin's shape, and logs completion (elapsed time, public
/// values size) or failure as events on that span.
pub async fn prove_compressed<'a, P>(
    prover: &'a P,
    proving_key: &'a P::ProvingKey,
    stdin: SP1Stdin,
) -> Result<SP1ProofWithPublicValues, P::Error>
where
    P: Prover,
    P::Error: fmt::Display,
{
    let job_id = next_job_id();
    let shape = stdin_shape(&stdin);
    let span = tracing::info_span!(
        "worker_job",
        job_id,
        kind = "prove_compressed",
        stdin_items = shape.items,
        stdin_bytes = shape.bytes,
        proof_witnesses = shape.proof_witnesses,
    );
    async move {
        tracing::debug!("prove_compressed job started");
        let started = Instant::now();
        match prover.prove(proving_key, stdin).compressed().await {
            Ok(proof) => {
                tracing::info!(
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    public_values_bytes = proof.public_values.as_slice().len(),
                    "prove_compressed job finished"
                );
                Ok(proof)
            },
            Err(err) => {
                tracing::error!(
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    %err,
                    "prove_compressed job failed"
                );
                Err(err)
            },
        }
    }
    .instrument(span)
    .await
}
