use std::path::PathBuf;

#[cfg(any(test, feature = "test-utils"))]
pub mod tests;

#[cfg(any(test, feature = "test-utils"))]
pub use tests::test_utils;

pub mod db;

pub const ENV_ZKPOW_DB_PATH: &str = "ZKPOW_DB_PATH";
pub const DEFAULT_DB_FILE_NAME: &str = "headers.db";
const WORKSPACE_ROOT_RELATIVE_TO_STORE: &str = "../..";

pub use db::{
    genesis_state_from_record, Error, HeaderBatchWitness, HeaderRecord, HeaderStore, Result, ResultOpt, RunRecord,
    SQLITE_MEMORY_DATABASE,
};

/// Resolve the SQLite header database path: `ZKPOW_DB_PATH` if set, otherwise
/// `headers.db` at the workspace root. This is the sync-writable system of
/// record; also useful for out-of-band querying.
pub fn db_path() -> PathBuf {
    match std::env::var(ENV_ZKPOW_DB_PATH) {
        Ok(path) => PathBuf::from(path),
        Err(_) => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(WORKSPACE_ROOT_RELATIVE_TO_STORE)
            .join(DEFAULT_DB_FILE_NAME),
    }
}
