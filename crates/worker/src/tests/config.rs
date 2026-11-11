use crate::config::{WorkerConfig, DEFAULT_SHARD_PROVER_PERMITS};

#[test]
fn default_worker_config() {
    assert_eq!(WorkerConfig::default().shard_prover_permits, DEFAULT_SHARD_PROVER_PERMITS);
    assert_eq!(WorkerConfig::new(8).shard_prover_permits, 8);
}
