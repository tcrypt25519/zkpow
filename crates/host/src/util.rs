//! Utilities for the zkpow prover script.

mod hash;

pub use zkpow_core::{
    parse_median_hints, parse_new_headers, serialize_median_hints, serialize_new_headers,
    target_from_bits, u256, work_from_target, ApplyFailure, BlockHash, BlockTimestamp, ChainWork,
    Claim, CompactTarget, ContinuationData, Header, HeaderChainPublicValues, InputError,
    MinimalPublicValues, NewHeader, NewHeaderHintError, ParseError, ProofCarryingState,
    ProofFailure, PublicValuesDigest, PublicValuesParseError, State, Target, ValidationErrorCode,
    VerifierKeyDigest, CLAIM_SIZE, CONTINUATION_DATA_SIZE, GENESIS_TARGET, MINIMAL_PV_SIZE,
    NEW_HEADER_SIZE, STATE_SIZE,
};

pub use hash::compute_pv_digest;
pub use zkpow_controller::{
    compute_final_state_with_hints, continuation_digest, continuation_digest_from_state,
    median_time_past_hints_from_records, records_to_new_headers,
};
