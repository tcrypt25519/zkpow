//! Utilities for the zkpow prover script.

mod hash;

pub use hash::compute_pv_digest;
// pub use zkpow_controller::{
//     compute_final_state_with_hints, continuation_digest, continuation_digest_from_state, records_to_new_headers,
// };
pub use zkpow_core::{
    block_work_from_target, cast_slice, serialize_inputs, target_from_bits, u256, ApplyFailure, BlockHash,
    BlockTimestamp, ChainWork, Claim, CompactTarget, ContinuationData, ErrorCode, ErrorCodeNames, Header, HeaderInput,
    InputParseError, MinimalPublicValues, ParseError, PublicValuesDigest, ReturnCode, State, Target, VerifierKeyDigest,
    CLAIM_SIZE, CONTINUATION_DATA_SIZE, GENESIS_TARGET, MINIMAL_PV_SIZE,
};
