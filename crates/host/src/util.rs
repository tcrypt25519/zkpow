//! Utilities for the zkpow prover script.

mod hash;

pub use hash::compute_pv_digest;
pub use zkpow_controller::{compute_final_state_cold, compute_final_state_warm, records_to_new_headers};
pub use zkpow_core::{
    block_work_from_target, cast_slice, serialize_inputs, target_from_bits, u256, ApplyError, BlockHash,
    BlockTimestamp, ChainWork, CompactTarget, ErrorCode, Header, HeaderInput, ParseError, PublicValues,
    ReportableError, ReturnCode, State, Target, VerifierKeyDigest,
};
