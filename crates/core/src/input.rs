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

/// Borrowed view of the public prover input wire format.
#[derive(Debug, Clone, Copy)]
pub struct InputRef<'a> {
    pub claim: &'a PublicChainClaim,
    pub recursive_proof: &'a RecursiveProof,
}

/// Owned private witness payload containing the batch of new headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewHeaderHints {
    pub headers: Vec<NewHeader>,
}

/// Borrowed private witness payload containing the batch of new headers.
#[derive(Debug, Clone, Copy)]
pub struct NewHeaderHintsRef<'a> {
    pub headers: &'a [NewHeader],
}

/// Owned private witness payload containing one claimed MTP value per header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MedianTimePastHints {
    pub medians: Vec<BlockTimestamp>,
}

/// Borrowed private witness payload containing one claimed MTP value per header.
#[derive(Debug, Clone, Copy)]
pub struct MedianTimePastHintsRef<'a> {
    pub medians: &'a [BlockTimestamp],
}

/// Recursive proof metadata that authenticates the current [`State`].
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
            check_exact_len(bytes, RECURSIVE_PROOF_SIZE).map_err(InputError::from)?;
            copy_from_bytes(bytes).map_err(InputError::from)
        })
    }

    /// Serialize to exactly [`RECURSIVE_PROOF_SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; RECURSIVE_PROOF_SIZE] {
        copy_to_bytes(self)
    }

    /// Borrow a [`RecursiveProof`] directly from aligned protocol bytes.
    pub fn ref_from_bytes(bytes: &[u8]) -> Result<&Self, InputError> {
        crate::ref_from_bytes(bytes).map_err(InputError::from)
    }
}

impl<'a> NewHeaderHintsRef<'a> {
    /// Parse private witness headers. The format is:
    ///
    /// ```text
    /// headers: [NewHeader]
    /// ```
    pub fn parse(bytes: &'a [u8]) -> Result<Self, NewHeaderHintError> {
        cycle_track("input/parse/new_header_hints", || {
            if !bytes.len().is_multiple_of(NEW_HEADER_SIZE) {
                return Err(NewHeaderHintError::PayloadLengthInvalid {
                    expected: bytes.len().div_ceil(NEW_HEADER_SIZE) * NEW_HEADER_SIZE,
                    actual: bytes.len(),
                });
            }

            let headers =
                slice_from_bytes::<NewHeader>(bytes).map_err(NewHeaderHintError::Parse)?;
            Ok(Self { headers })
        })
    }

    #[must_use]
    pub fn to_owned(&self) -> NewHeaderHints {
        NewHeaderHints {
            headers: self.headers.to_vec(),
        }
    }

    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.headers.len() * NEW_HEADER_SIZE);
        for header in self.headers {
            bytes.extend_from_slice(&header.to_bytes());
        }
        bytes
    }
}

impl NewHeaderHints {
    #[must_use]
    pub fn new(headers: Vec<NewHeader>) -> Self {
        Self { headers }
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, NewHeaderHintError> {
        NewHeaderHintsRef::parse(bytes).map(|hints| hints.to_owned())
    }

    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        NewHeaderHintsRef {
            headers: &self.headers,
        }
        .to_bytes()
    }
}

impl<'a> MedianTimePastHintsRef<'a> {
    /// Parse private witness MTP hints. The format is:
    ///
    /// ```text
    /// medians: [BlockTimestamp]
    /// ```
    pub fn parse(bytes: &'a [u8], expected_count: usize) -> Result<Self, MedianTimePastHintError> {
        cycle_track("input/parse/median_time_past_hints", || {
            let expected_len = expected_count * core::mem::size_of::<BlockTimestamp>();
            if bytes.len() != expected_len {
                return Err(MedianTimePastHintError::PayloadLengthInvalid {
                    expected: expected_len,
                    actual: bytes.len(),
                });
            }

            let medians = slice_from_bytes::<BlockTimestamp>(bytes)
                .map_err(MedianTimePastHintError::Parse)?;
            Ok(Self { medians })
        })
    }

    #[must_use]
    pub fn to_owned(&self) -> MedianTimePastHints {
        MedianTimePastHints {
            medians: self.medians.to_vec(),
        }
    }

    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.medians.len() * 4);
        for median in self.medians {
            bytes.extend_from_slice(&median.to_le_bytes());
        }
        bytes
    }
}

impl MedianTimePastHints {
    #[must_use]
    pub fn new(medians: Vec<BlockTimestamp>) -> Self {
        Self { medians }
    }

    pub fn parse(bytes: &[u8], expected_count: usize) -> Result<Self, MedianTimePastHintError> {
        MedianTimePastHintsRef::parse(bytes, expected_count).map(|hints| hints.to_owned())
    }

    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        MedianTimePastHintsRef {
            medians: &self.medians,
        }
        .to_bytes()
    }
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

impl<'a> InputRef<'a> {
    /// Parse and validate input from the aligned host/guest wire format.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, InputError> {
        cycle_track("input/parse", || {
            let (claim_bytes, proof_bytes) = split_input_wire(bytes)?;
            let claim = cycle_track("input/parse/claim", || {
                crate::ref_from_bytes::<PublicChainClaim>(claim_bytes).map_err(InputError::from)
            })?;
            let recursive_proof = RecursiveProof::ref_from_bytes(proof_bytes)?;
            Ok(Self {
                claim,
                recursive_proof,
            })
        })
    }

    pub fn to_owned(&self) -> Input {
        Input {
            claim: *self.claim,
            recursive_proof: *self.recursive_proof,
        }
    }
}

impl Input {
    /// Constructs a new Input.
    pub fn new(claim: PublicChainClaim, recursive_proof: RecursiveProof) -> Self {
        Self {
            claim,
            recursive_proof,
        }
    }
}

impl Input {
    /// Parse and validate input from the host/guest wire format.
    pub fn parse(bytes: &[u8]) -> Result<Self, InputError> {
        InputRef::parse(bytes).map(|input| input.to_owned())
    }
}

impl Input {
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
    use crate::{BlockHash, BlockTimestamp, State, ValidationState};

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
            genesis_hash: BlockHash::from_raw([1; 32]),
            block_hash: BlockHash::from_raw([2; 32]),
            ..Default::default()
        };
        let claim = ValidationState::from_state(&genesis_state).public;
        let input = Input::new(claim, RecursiveProof::default()).to_bytes();
        let input = Input::parse(&input).unwrap();

        assert_eq!(input.claim.height, 0);
        assert_eq!(input.claim.genesis_hash, BlockHash::from_raw([1; 32]));
        assert_eq!(input.claim.tip_hash, BlockHash::from_raw([2; 32]));
        assert_eq!(input.recursive_proof, RecursiveProof::default());
    }

    #[test]
    fn test_parse_from_bytes_non_genesis_with_proof() {
        let mut non_genesis_state: State = State {
            height: 100,
            ..Default::default()
        };
        // For height > 0, genesis_hash must be set
        non_genesis_state.genesis_hash = BlockHash::from_raw([1; 32]);
        non_genesis_state.block_hash = BlockHash::from_raw([2; 32]);
        non_genesis_state.header.prev_blockhash = BlockHash::from_raw([3; 32]);

        let expected_verifier_key = VerifierKeyDigest::from_raw([1; 8]);
        let expected_public_values_digest = PublicValuesDigest::from_raw([2; 32]);
        let recursive_proof_data = RecursiveProof {
            verifier_key: expected_verifier_key,
            public_values_digest: expected_public_values_digest,
            ..Default::default()
        };
        let claim = ValidationState::from_state(&non_genesis_state).public;
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
                timestamp: BlockTimestamp::from_consensus(100),
                nonce: 0,
            },
            NewHeader {
                version: 2,
                merkle_root: [5; 32],
                timestamp: BlockTimestamp::from_consensus(200),
                nonce: 1,
            },
        ];

        let hints = NewHeaderHints::new(headers.clone());
        let bytes = hints.to_bytes();
        let parsed = NewHeaderHints::parse(&bytes).unwrap();

        assert_eq!(parsed, hints);
    }

    #[test]
    fn test_input_ref_rejects_misaligned_claim() {
        let genesis_state: State = State {
            height: 0,
            genesis_hash: BlockHash::default(),
            ..Default::default()
        };
        let claim = ValidationState::from_state(&genesis_state).public;
        let input = Input::new(claim, RecursiveProof::default()).to_bytes();

        let mut misaligned = Vec::with_capacity(input.len() + 1);
        misaligned.push(0);
        misaligned.extend_from_slice(&input);

        let err = InputRef::parse(&misaligned[1..]).unwrap_err();
        assert_eq!(
            err,
            InputError::Parse(ParseError::Misaligned {
                required: core::mem::align_of::<PublicChainClaim>(),
            })
        );
    }

    #[test]
    fn test_new_header_hints_reject_truncated_payload() {
        let headers = NewHeaderHints::new(vec![NewHeader {
            version: 1,
            merkle_root: [4; 32],
            timestamp: BlockTimestamp::from_consensus(100),
            nonce: 0,
        }]);
        let mut bytes = headers.to_bytes();
        bytes.pop();

        let err = NewHeaderHintsRef::parse(&bytes).unwrap_err();
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
        let hints = MedianTimePastHints::new(vec![
            BlockTimestamp::from_consensus(0),
            BlockTimestamp::from_consensus(123),
            BlockTimestamp::from_consensus(456),
        ]);

        let bytes = hints.to_bytes();
        let parsed = MedianTimePastHints::parse(&bytes, 3).unwrap();

        assert_eq!(parsed, hints);
    }

    #[test]
    fn median_time_past_hints_reject_wrong_count_length() {
        let bytes = MedianTimePastHints::new(vec![BlockTimestamp::from_consensus(123)]).to_bytes();

        let err = MedianTimePastHintsRef::parse(&bytes, 2).unwrap_err();

        assert_eq!(
            err,
            MedianTimePastHintError::PayloadLengthInvalid {
                expected: 8,
                actual: 4,
            }
        );
    }

    #[test]
    fn median_time_past_hints_reject_truncated_payload() {
        let mut bytes =
            MedianTimePastHints::new(vec![BlockTimestamp::from_consensus(123)]).to_bytes();
        bytes.pop();

        let err = MedianTimePastHintsRef::parse(&bytes, 1).unwrap_err();

        assert_eq!(
            err,
            MedianTimePastHintError::PayloadLengthInvalid {
                expected: 4,
                actual: 3,
            }
        );
    }
}
