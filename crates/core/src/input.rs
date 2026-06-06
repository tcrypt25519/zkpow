//! Logic for parsing the public prover input and constructing the `Input` struct.

use alloc::vec::Vec;

use crate::{
    check_exact_len, copy_from_bytes, copy_to_bytes, cycle_track, slice_from_bytes, BlockTimestamp,
    NewHeader, ParseError, PublicChainClaim, PublicValuesDigest, VerifierKeyDigest,
    NEW_HEADER_SIZE, PUBLIC_CHAIN_CLAIM_SIZE, RECURSIVE_PROOF_SIZE,
};

// ============================================================================
// Input & InputError
// ============================================================================

/// Complete typed public prover input.
///
/// The full validation state is supplied separately as private witness data.
/// Its public preimage fields must match [`claim`](Self::claim) before the
/// guest uses it to validate headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    pub claim: PublicChainClaim,
    pub recursive_proof: RecursiveProof,
}

/// Recursive proof metadata that authenticates the current [`State`](crate::State).
///
/// `previous_return_code` must be 0 (success) for the guest to accept the
/// recursive continuation.  A nonzero value means the prior proof committed a
/// validation failure, and extending from it is rejected.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecursiveProof {
    pub verifier_key: VerifierKeyDigest,
    pub public_values_digest: PublicValuesDigest,
    /// Return code from the previous proof (0 = success, nonzero = failure).
    pub previous_return_code: u8,
    /// Padding to maintain 4-byte alignment.
    pub _pad: [u8; 3],
}

impl Default for RecursiveProof {
    fn default() -> Self {
        Self {
            verifier_key: VerifierKeyDigest::from_raw([0u32; 8]),
            public_values_digest: PublicValuesDigest::from_raw([0u8; 32]),
            previous_return_code: 0,
            _pad: [0u8; 3],
        }
    }
}

/// Parse and validation errors for [`Input`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InputError {
    #[error("input parse error: {0}")]
    Parse(ParseError),
}

/// Parse and validation errors for the new-header private witness batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NewHeaderHintError {
    #[error("new-header hint payload length mismatch: expected {expected} bytes, got {actual}")]
    PayloadLengthInvalid { expected: usize, actual: usize },
    #[error("new-header hint parse error: {0}")]
    Parse(ParseError),
}

/// Parse and validation errors for median-time-past private witness hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MedianTimePastHintError {
    #[error("median hint payload length mismatch: expected {expected} bytes, got {actual}")]
    PayloadLengthInvalid { expected: usize, actual: usize },
    #[error("median hint parse error: {0}")]
    Parse(ParseError),
}

impl From<ParseError> for InputError {
    fn from(value: ParseError) -> Self {
        Self::Parse(value)
    }
}

impl RecursiveProof {
    /// Parse and validate a RecursiveProof from exactly [`RECURSIVE_PROOF_SIZE`] bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, InputError> {
        cycle_track("input/recursive_proof", || {
            copy_from_bytes(bytes).map_err(InputError::from)
        })
    }

    /// Serialize to exactly [`RECURSIVE_PROOF_SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; RECURSIVE_PROOF_SIZE] {
        copy_to_bytes(self)
    }
}

/// Serialize new headers.
#[must_use]
pub fn serialize_new_headers(headers: &[NewHeader]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(headers.len() * NEW_HEADER_SIZE);
    for header in headers {
        bytes.extend_from_slice(&header.to_bytes());
    }
    bytes
}

/// Parse new headers as a zero-copy slice reference.
pub fn parse_new_headers(bytes: &[u8]) -> Result<&[NewHeader], NewHeaderHintError> {
    cycle_track("input/parse/new_header_hints", || {
        if !bytes.len().is_multiple_of(NEW_HEADER_SIZE) {
            return Err(NewHeaderHintError::PayloadLengthInvalid {
                expected: bytes.len().div_ceil(NEW_HEADER_SIZE) * NEW_HEADER_SIZE,
                actual: bytes.len(),
            });
        }

        let headers = slice_from_bytes::<NewHeader>(bytes).map_err(NewHeaderHintError::Parse)?;
        Ok(headers)
    })
}

/// Serialize median hints.
#[must_use]
pub fn serialize_median_hints(medians: &[BlockTimestamp]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(medians.len() * 4);
    for median in medians {
        bytes.extend_from_slice(&median.to_le_bytes());
    }
    bytes
}

/// Parse median hints as a zero-copy slice reference.
pub fn parse_median_hints(
    bytes: &[u8],
    expected_count: usize,
) -> Result<&[BlockTimestamp], MedianTimePastHintError> {
    cycle_track("input/parse/median_time_past_hints", || {
        let expected_len = expected_count * core::mem::size_of::<BlockTimestamp>();
        if bytes.len() != expected_len {
            return Err(MedianTimePastHintError::PayloadLengthInvalid {
                expected: expected_len,
                actual: bytes.len(),
            });
        }

        let medians =
            slice_from_bytes::<BlockTimestamp>(bytes).map_err(MedianTimePastHintError::Parse)?;
        Ok(medians)
    })
}

/// Split and validate the wire layout, returning `(claim_bytes, proof_bytes)`.
///
/// `Input` intentionally carries only the public claim and recursive proof
/// metadata. The full [`State`](crate::State) is supplied separately as private
/// witness data and checked against this claim before use.
fn split_input_wire(bytes: &[u8]) -> Result<(&[u8], &[u8]), InputError> {
    check_exact_len(bytes, PUBLIC_CHAIN_CLAIM_SIZE + RECURSIVE_PROOF_SIZE)?;
    Ok((
        &bytes[..PUBLIC_CHAIN_CLAIM_SIZE],
        &bytes[PUBLIC_CHAIN_CLAIM_SIZE..],
    ))
}

impl Input {
    /// Constructs a new Input.
    pub fn new(claim: PublicChainClaim, recursive_proof: RecursiveProof) -> Self {
        Self {
            claim,
            recursive_proof,
        }
    }

    /// Parse and validate input from the host/guest wire format.
    pub fn parse(bytes: &[u8]) -> Result<Self, InputError> {
        cycle_track("input/parse", || {
            let (claim_bytes, proof_bytes) = split_input_wire(bytes)?;
            let claim = cycle_track("input/parse/claim", || {
                PublicChainClaim::parse(claim_bytes).map_err(InputError::from)
            })?;
            let recursive_proof = RecursiveProof::parse(proof_bytes)?;
            Ok(Self {
                claim,
                recursive_proof,
            })
        })
    }

    /// Serialize to the host/guest wire format.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(PUBLIC_CHAIN_CLAIM_SIZE + RECURSIVE_PROOF_SIZE);
        bytes.extend_from_slice(&self.claim.to_bytes());
        bytes.extend_from_slice(&self.recursive_proof.to_bytes());
        bytes
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::{BlockHash, BlockTimestamp, State};

    #[test]
    fn test_recursive_proof_default_is_zeros() {
        let proof = RecursiveProof::default();
        assert_eq!(proof.verifier_key.as_raw(), &[0u32; 8]);
        assert_eq!(proof.public_values_digest.as_raw(), &[0u8; 32]);
        assert_eq!(proof.previous_return_code, 0);
    }

    #[test]
    fn test_recursive_proof_previous_return_code_round_trips() {
        let proof = RecursiveProof {
            previous_return_code: 3,
            ..Default::default()
        };
        let bytes = proof.to_bytes();
        let parsed = RecursiveProof::parse(&bytes).unwrap();
        assert_eq!(parsed.previous_return_code, 3);
    }

    #[test]
    fn test_parse_from_bytes_genesis_no_proof() {
        let genesis_state: State = State {
            height: 0,
            genesis_hash: BlockHash::new([1; 32]),
            block_hash: BlockHash::new([2; 32]),
            ..Default::default()
        };
        let claim = genesis_state.public_claim();
        let input = Input::new(claim, RecursiveProof::default()).to_bytes();
        let input = Input::parse(&input).unwrap();

        assert_eq!(input.claim.height, 0);
        assert_eq!(input.claim.genesis_hash, BlockHash::new([1; 32]));
        assert_eq!(input.claim.tip_hash, BlockHash::new([2; 32]));
        assert_eq!(input.recursive_proof, RecursiveProof::default());
    }

    #[test]
    fn test_parse_from_bytes_non_genesis_with_proof() {
        let mut non_genesis_state: State = State {
            height: 100,
            ..Default::default()
        };
        // For height > 0, genesis_hash must be set
        non_genesis_state.genesis_hash = BlockHash::new([1; 32]);
        non_genesis_state.block_hash = BlockHash::new([2; 32]);
        non_genesis_state.header.prev_blockhash = BlockHash::new([3; 32]);

        let expected_verifier_key = VerifierKeyDigest::from_raw([1; 8]);
        let expected_public_values_digest = PublicValuesDigest::from_raw([2; 32]);
        let recursive_proof_data = RecursiveProof {
            verifier_key: expected_verifier_key,
            public_values_digest: expected_public_values_digest,
            ..Default::default()
        };
        let claim = non_genesis_state.public_claim();
        let input = Input::new(claim, recursive_proof_data).to_bytes();
        let input = Input::parse(&input).unwrap();

        assert_eq!(input.claim.height, 100);
        assert_eq!(input.recursive_proof.verifier_key, expected_verifier_key);
        assert_eq!(
            input.recursive_proof.public_values_digest,
            expected_public_values_digest
        );
    }

    #[test]
    fn test_new_header_hints_round_trip() {
        let headers: Vec<NewHeader> = vec![
            NewHeader {
                version: 1,
                merkle_root: [4; 32],
                timestamp: BlockTimestamp::new(100),
                nonce: 0,
            },
            NewHeader {
                version: 2,
                merkle_root: [5; 32],
                timestamp: BlockTimestamp::new(200),
                nonce: 1,
            },
        ];

        let bytes = serialize_new_headers(&headers);
        let parsed = parse_new_headers(&bytes).unwrap();

        assert_eq!(parsed, headers.as_slice());
    }

    #[test]
    fn test_new_header_hints_reject_truncated_payload() {
        let headers = vec![NewHeader {
            version: 1,
            merkle_root: [4; 32],
            timestamp: BlockTimestamp::new(100),
            nonce: 0,
        }];
        let mut bytes = serialize_new_headers(&headers);
        bytes.pop();

        let err = parse_new_headers(&bytes).unwrap_err();
        assert_eq!(
            err,
            NewHeaderHintError::PayloadLengthInvalid {
                expected: 44,
                actual: 43,
            }
        );
    }

    #[test]
    fn median_time_past_hints_round_trip() {
        let hints = vec![
            BlockTimestamp::new(0),
            BlockTimestamp::new(123),
            BlockTimestamp::new(456),
        ];

        let bytes = serialize_median_hints(&hints);
        let parsed = parse_median_hints(&bytes, 3).unwrap();

        assert_eq!(parsed, hints.as_slice());
    }

    #[test]
    fn median_time_past_hints_reject_invalid_lengths() {
        let one_hint = serialize_median_hints(&[BlockTimestamp::new(123)]);
        let mut truncated_hint = one_hint.clone();
        truncated_hint.pop();

        let cases = [
            (
                "wrong hint count",
                one_hint,
                2,
                MedianTimePastHintError::PayloadLengthInvalid {
                    expected: 8,
                    actual: 4,
                },
            ),
            (
                "truncated payload",
                truncated_hint,
                1,
                MedianTimePastHintError::PayloadLengthInvalid {
                    expected: 4,
                    actual: 3,
                },
            ),
        ];

        for (name, bytes, expected_count, expected_error) in cases {
            let err = parse_median_hints(&bytes, expected_count).unwrap_err();
            assert_eq!(err, expected_error, "{name}");
        }
    }
}
