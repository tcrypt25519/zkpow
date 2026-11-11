/// Matches the default baked into `cpu_worker_builder_with_machine` when
/// `SHARD_PROVER_PERMITS` is unset.
pub const DEFAULT_SHARD_PROVER_PERMITS: usize = 4;

/// Resolved configuration for the CPU proving worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerConfig {
    /// Maximum number of shards proven concurrently, shared across the
    /// core, compress, shrink, and wrap proving stages.
    pub shard_prover_permits: usize,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            shard_prover_permits: DEFAULT_SHARD_PROVER_PERMITS,
        }
    }
}

impl WorkerConfig {
    #[must_use]
    pub const fn new(shard_prover_permits: usize) -> Self {
        Self { shard_prover_permits }
    }
}
