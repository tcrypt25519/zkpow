/// Environment variable controlling how many shards the CPU worker may
/// prove concurrently.
///
/// Read directly by the vendored `sp1-prover` crate's
/// `cpu_worker_builder_with_machine` (the semaphore that actually gates
/// concurrency lives there); mirrored here so the resolved value is visible
/// through this crate's own config and logging.
pub const SHARD_PROVER_PERMITS_ENV: &str = "SHARD_PROVER_PERMITS";

/// Matches the default baked into `cpu_worker_builder_with_machine` when
/// `SHARD_PROVER_PERMITS` is unset.
const DEFAULT_SHARD_PROVER_PERMITS: usize = 4;

/// Resolved configuration for the CPU proving worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerConfig {
    /// Maximum number of shards proven concurrently, shared across the
    /// core, compress, shrink, and wrap proving stages.
    pub shard_prover_permits: usize,
}

impl WorkerConfig {
    /// Read the worker configuration from the environment.
    #[must_use]
    pub fn from_env() -> Self {
        let raw = std::env::var(SHARD_PROVER_PERMITS_ENV).ok();
        Self {
            shard_prover_permits: parse_shard_prover_permits(raw.as_deref()),
        }
    }
}

/// Parses the `SHARD_PROVER_PERMITS` value, falling back to
/// [`DEFAULT_SHARD_PROVER_PERMITS`] when unset, empty, zero, or not a valid
/// positive integer. Split out from [`WorkerConfig::from_env`] so the
/// parsing rules are unit-testable without touching real process env vars.
pub(crate) fn parse_shard_prover_permits(raw: Option<&str>) -> usize {
    raw.and_then(|value| value.parse::<usize>().ok())
        .filter(|&value| value > 0)
        .unwrap_or(DEFAULT_SHARD_PROVER_PERMITS)
}
