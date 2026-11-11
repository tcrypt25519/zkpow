#[cfg(any(test, feature = "test-utils"))]
pub mod tests;

#[cfg(any(test, feature = "test-utils"))]
pub use tests::test_utils;

pub mod db;

pub use db::{
    genesis_state_from_record, Error, HeaderBatchWitness, HeaderRecord, HeaderStore, Result, ResultOpt, RunRecord,
    SQLITE_MEMORY_DATABASE,
};
