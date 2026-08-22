//! The stateful coordinator of the proving pipeline.
//!
//! The controller owns the header database, the persistent per-`run_id`
//! progress ledger, and artifact storage. Hosts are stateless I/O drivers
//! that pull batches, pipe them into the guest, upload the resulting
//! artifacts, and report completions. All validation of results happens
//! here, on the controller's side of the trust boundary.
//!
//! Module map:
//! * [`runtime`] — run sessions, the double-buffered grant pipeline, long-polling, completion validation. Verified
//!   progress persists in the header database's `runs` table (see `zkpow_store`).
//! * [`protocol`] — typed host/controller messages and their HTTP codec.
//! * [`http`] — postcard boundary around the typed runtime API.
//! * [`storage`] — artifact targets (local paths or pre-signed S3 URLs).
//! * [`verify`] — pluggable cryptographic proof verification.

#[cfg(test)]
mod tests;

pub mod diagnostics;
pub mod http;
pub mod protocol;
pub mod runtime;
pub mod storage;
pub mod verify;

mod digest;
mod validation;

pub use diagnostics::format_claim_mismatch;
pub use digest::{continuation_digest, continuation_digest_from_state};
pub use validation::{compute_final_state_with_hints, records_to_new_headers};

/// Headers per controller-selected batch: one difficulty epoch.
pub const DEFAULT_BATCH_SIZE: u32 = Mainnet::PARAMS.epoch_length;
pub use zkpow_core::{requires_median_hint, BlockHash, BlockTimestamp, Claim, HeaderInput, State};
use zkpow_params::{Consensus, Mainnet};
pub use zkpow_store::{db_path, genesis_state_from_record, HeaderBatchWitness, HeaderRecord, HeaderStore, RunRecord};

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub const NO_HEADERS_REMAINING_PREFIX: &str = "no headers remaining in database";
