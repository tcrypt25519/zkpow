#[cfg(test)]
mod tests;

pub mod db;
pub mod flat;

pub use db::{genesis_state_from_record, HeaderBatchWitness, HeaderRecord, HeaderStore, RunRecord};
pub use flat::{HeaderBinReader, HEADER_RECORD_SIZE};

/// Resolve the SQLite header database path: `ZKPOW_DB_PATH` if set, otherwise
/// `headers.db` at the workspace root. This is the sync-writable system of
/// record; also useful for out-of-band querying.
pub fn db_path() -> std::path::PathBuf {
    match std::env::var("ZKPOW_DB_PATH") {
        Ok(path) => std::path::PathBuf::from(path),
        Err(_) => concat!(env!("CARGO_MANIFEST_DIR"), "/../../headers.db").into(),
    }
}

/// Resolve the read-only `headers.bin` snapshot path: `ZKPOW_HEADERS_BIN_PATH`
/// if set, otherwise `headers.bin` at the workspace root. Generated from
/// [`db_path`] by `zkpow-cli dump-bin`; the proving hot path reads this
/// instead of the database.
pub fn headers_bin_path() -> std::path::PathBuf {
    match std::env::var("ZKPOW_HEADERS_BIN_PATH") {
        Ok(path) => std::path::PathBuf::from(path),
        Err(_) => concat!(env!("CARGO_MANIFEST_DIR"), "/../../headers.bin").into(),
    }
}
