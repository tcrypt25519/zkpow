//! The stateful coordinator of the proving pipeline.
//!
//! The controller owns the header database, the persistent per-`run_id`
//! progress ledger, and artifact storage. Hosts are stateless I/O drivers
//! that pull batches, pipe them into the guest, upload the resulting
//! artifacts, and report completions. All validation of results happens
//! here, on the controller's side of the trust boundary.
//!
//! Module map:
//! * [`runtime`]: run sessions, long-polling, completion validation. Verified progress persists in the header
//!   database's `runs` table (see `zkpow_store`).
//! * [`protocol`]: typed host/controller messages.
//! * [`storage`]: local-disk artifact targets.
//! * [`verify`]: pluggable cryptographic proof verification.

#[cfg(test)]
mod tests;

pub mod protocol;
pub mod runtime;
pub mod storage;
pub mod verify;

mod validation;

pub use validation::{compute_final_state_cold, compute_final_state_warm, records_to_new_headers};
pub use zkpow_core::{requires_median_hint, BlockHash, BlockTimestamp, HeaderInput, State};
pub use zkpow_store::{db_path, genesis_state_from_record, HeaderBatchWitness, HeaderRecord, HeaderStore, RunRecord};

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Exhaustion error prefix; hosts match on it to stop cleanly instead of failing.
pub const NO_HEADERS_REMAINING_PREFIX: &str = "no headers remaining in database";
