pub mod db;

pub use db::{genesis_state_from_record, HeaderBatchWitness, HeaderRecord, HeaderStore, RunRecord};

/// Resolve the SQLite header database path: `ZKPOW_DB_PATH` if set, otherwise
/// `headers.db` at the workspace root.
pub fn db_path() -> std::path::PathBuf {
    match std::env::var("ZKPOW_DB_PATH") {
        Ok(path) => std::path::PathBuf::from(path),
        Err(_) => concat!(env!("CARGO_MANIFEST_DIR"), "/../../headers.db").into(),
    }
}
