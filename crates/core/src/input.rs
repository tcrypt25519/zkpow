//! Logic for parsing the prover input and constructing the `Input` struct.

use alloc::vec::Vec;

use crate::{
    check_exact_len, copy_from_bytes, copy_to_bytes, cycle_track, mut_from_bytes, slice_from_bytes,
    BlockHash, BlockTimestamp, Header, NewHeader, ParseError, PublicValuesDigest, State,
    VerifierKeyDigest, NEW_HEADER_SIZE, RECURSIVE_PROOF_SIZE, STATE_SIZE,
};

// ============================================================================
// Input & InputError
// ============================================================================

/// Complete typed prover input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    pub state: State,
    pub recursive_proof: RecursiveProof,
    pub headers: Vec<NewHeader>,
}

/// Borrowed view of the prover input wire format.
#[derive(Debug, Clone, Copy)]
pub struct InputRef<'a> {
    pub state: &'a State,
    pub recursive_proof: &'a RecursiveProof,
    pub headers: &'a [NewHeader],
}

/// Mutable borrowed view of the prover input wire format.
#[derive(Debug)]
pub struct InputMut<'a> {
    pub state: &'a mut State,
    pub recursive_proof: &'a RecursiveProof,
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
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecursiveProof {
    pub verifier_key: VerifierKeyDigest,
    pub public_values_digest: PublicValuesDigest,
}

impl Default for RecursiveProof {
    fn default() -> Self {
        Self {
            verifier_key: VerifierKeyDigest::from_raw([0u32; 8]),
            public_values_digest: PublicValuesDigest::from_raw([0u8; 32]),
        }
    }
}

/// Parse and validation errors for [`Input`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InputError {
    #[error("input parse error: {0}")]
    Parse(ParseError),
    #[error("missing recursive proof metadata for non-genesis state")]
    MissingRecursiveProof,
    #[error("genesis state must not carry recursive proof metadata")]
    UnexpectedRecursiveProof,
    #[error("genesis state input must carry an all-zero genesis hash placeholder")]
    GenesisHashMustBeZero,
    #[error("header payload length {actual} is not a multiple of {NEW_HEADER_SIZE} bytes")]
    HeaderPayloadLengthInvalid { actual: usize },
    #[error("invalid recursive proof length: expected {expected} bytes, got {actual}")]
    InvalidRecursiveProofLength { actual: usize, expected: usize },
}

/// Parse and validation errors for median-time-past private witness hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MedianTimePastHintError {
    #[error("median hint payload missing count: got {actual} bytes")]
    TruncatedCount { actual: usize },
    #[error("median hint count mismatch: expected {expected}, got {actual}")]
    CountMismatch { expected: usize, actual: usize },
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
            check_exact_len(bytes, RECURSIVE_PROOF_SIZE).map_err(|_| {
                InputError::InvalidRecursiveProofLength {
                    actual: bytes.len(),
                    expected: RECURSIVE_PROOF_SIZE,
                }
            })?;
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

impl<'a> MedianTimePastHintsRef<'a> {
    /// Parse private witness MTP hints. The format is:
    ///
    /// ```text
    /// count: u32 little-endian
    /// medians: [BlockTimestamp; count]
    /// ```
    pub fn parse(bytes: &'a [u8], expected_count: usize) -> Result<Self, MedianTimePastHintError> {
        cycle_track("input/parse/median_time_past_hints", || {
            let count_bytes = bytes
                .get(..4)
                .ok_or(MedianTimePastHintError::TruncatedCount {
                    actual: bytes.len(),
                })?;
            let actual_count =
                u32::from_le_bytes(count_bytes.try_into().expect("slice length checked above"))
                    as usize;
            if actual_count != expected_count {
                return Err(MedianTimePastHintError::CountMismatch {
                    expected: expected_count,
                    actual: actual_count,
                });
            }

            let expected_len = 4 + (expected_count * core::mem::size_of::<BlockTimestamp>());
            if bytes.len() != expected_len {
                return Err(MedianTimePastHintError::PayloadLengthInvalid {
                    expected: expected_len,
                    actual: bytes.len(),
                });
            }

            let medians = slice_from_bytes::<BlockTimestamp>(&bytes[4..])
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
        let mut bytes = Vec::with_capacity(4 + self.medians.len() * 4);
        bytes.extend_from_slice(&(self.medians.len() as u32).to_le_bytes());
        for median in &self.medians {
            bytes.extend_from_slice(&median.to_consensus().to_le_bytes());
        }
        bytes
    }
}

impl<'a> InputRef<'a> {
    /// Parse and validate input from the aligned host/guest wire format.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, InputError> {
        cycle_track("input/parse", || {
            let state_end = STATE_SIZE;
            let state_bytes = bytes.get(..state_end).ok_or(ParseError::Truncated {
                offset: 0,
                needed: STATE_SIZE,
                actual: bytes.len(),
            })?;
            let state = cycle_track("input/parse/state", || {
                State::ref_from_bytes(state_bytes).map_err(InputError::from)
            })?;

            let proof_start = state_end;
            let proof_end = proof_start.checked_add(RECURSIVE_PROOF_SIZE).ok_or(
                InputError::InvalidRecursiveProofLength {
                    actual: bytes.len().saturating_sub(proof_start),
                    expected: RECURSIVE_PROOF_SIZE,
                },
            )?;
            let proof_bytes = bytes.get(proof_start..proof_end).ok_or(
                InputError::InvalidRecursiveProofLength {
                    actual: bytes.len().saturating_sub(proof_start),
                    expected: RECURSIVE_PROOF_SIZE,
                },
            )?;
            let recursive_proof = RecursiveProof::ref_from_bytes(proof_bytes)?;

            let header_payload = &bytes[proof_end..];
            if !header_payload.len().is_multiple_of(NEW_HEADER_SIZE) {
                return Err(InputError::HeaderPayloadLengthInvalid {
                    actual: header_payload.len(),
                });
            }

            let headers = cycle_track("input/parse/headers", || {
                NewHeader::slice_from_bytes(header_payload).map_err(InputError::from)
            })?;

            if state.height == 0 && state.genesis_hash != BlockHash::default() {
                return Err(InputError::GenesisHashMustBeZero);
            }

            Ok(Self {
                state,
                recursive_proof,
                headers,
            })
        })
    }

    pub fn to_owned<F>(&self, hash_header: F) -> Input
    where
        F: FnOnce(&Header) -> BlockHash + Copy,
    {
        Input {
            state: self.state.with_genesis_hash(hash_header),
            recursive_proof: *self.recursive_proof,
            headers: self.headers.to_vec(),
        }
    }
}

fn ensure_min_len(bytes_len: usize, expected: usize) -> Result<usize, InputError> {
    expected.checked_sub(bytes_len).ok_or_else(|| {
        InputError::from(ParseError::InvalidLength {
            expected,
            actual: bytes_len,
        })
    })
}

impl<'a> InputMut<'a> {
    /// Parse and validate input from the aligned host/guest wire format.
    pub fn parse(bytes: &'a mut [u8]) -> Result<Self, InputError> {
        cycle_track("input/parse_mut", || {
            let state_and_proof_size = STATE_SIZE + RECURSIVE_PROOF_SIZE;
            let headers_size = ensure_min_len(bytes.len(), state_and_proof_size)?;

            let (state_bytes, proof_and_headers) = bytes.split_at_mut(STATE_SIZE);
            let (proof_bytes, headers_bytes) = proof_and_headers.split_at_mut(RECURSIVE_PROOF_SIZE);

            let state = cycle_track("input/parse_mut/state", || {
                mut_from_bytes::<State>(state_bytes).map_err(InputError::from)
            })?;

            let recursive_proof = RecursiveProof::ref_from_bytes(proof_bytes)?;

            if !headers_size.is_multiple_of(NEW_HEADER_SIZE) {
                return Err(InputError::HeaderPayloadLengthInvalid {
                    actual: headers_size,
                });
            }

            let headers = cycle_track("input/parse_mut/headers", || {
                NewHeader::slice_from_bytes(headers_bytes).map_err(InputError::from)
            })?;

            if state.height == 0 && state.genesis_hash != BlockHash::default() {
                return Err(InputError::GenesisHashMustBeZero);
            }

            Ok(Self {
                state,
                recursive_proof,
                headers,
            })
        })
    }
}

impl State {
    /// Fill in the genesis hash when the wire input leaves it unset.
    pub fn update_genesis_hash<F>(&mut self, hash_header: F)
    where
        F: FnOnce(&Header) -> BlockHash + Copy,
    {
        if self.height == 0 && self.genesis_hash == BlockHash::default() {
            let block_hash = hash_header(&self.header);
            self.block_hash = block_hash;
            self.genesis_hash = block_hash;
        }
    }

    /// Clone this state and fill in the genesis hash when the wire input leaves it unset.
    #[must_use]
    pub fn with_genesis_hash<F>(&self, hash_header: F) -> Self
    where
        F: FnOnce(&Header) -> BlockHash + Copy,
    {
        let mut state = self.clone();
        state.update_genesis_hash(hash_header);
        state
    }
}

impl Input {
    /// Constructs a new Input, enforcing invariants.
    pub fn new(
        state: State,
        recursive_proof: RecursiveProof,
        headers: Vec<NewHeader>,
    ) -> Result<Self, InputError> {
        Ok(Self {
            state,
            recursive_proof,
            headers,
        })
    }

    /// Parse and validate input from the host/guest wire format.
    pub fn parse<F>(bytes: &[u8], hash_header: F) -> Result<Self, InputError>
    where
        F: FnOnce(&Header) -> BlockHash + Copy,
    {
        InputRef::parse(bytes).map(|input| input.to_owned(hash_header))
    }

    /// Serialize to the host/guest wire format.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(
            STATE_SIZE + RECURSIVE_PROOF_SIZE + (self.headers.len() * NEW_HEADER_SIZE),
        );
        let mut state = self.state.clone();
        // TODO: This genesis_hash setting logic should be somewhere; but not here.
        if state.height == 0 {
            state.genesis_hash = BlockHash::default();
        }
        bytes.extend_from_slice(&state.to_bytes());
        bytes.extend_from_slice(&self.recursive_proof.to_bytes());
        for header in &self.headers {
            bytes.extend_from_slice(&header.to_bytes());
        }
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
    use crate::{BlockHash, BlockTimestamp, Header};

    // Dummy hash_header for testing
    fn dummy_hash_header(_header: &Header) -> BlockHash {
        BlockHash::default() // All zeros hash
    }

    #[test]
    fn test_recursive_proof_default_is_zeros() {
        let proof = RecursiveProof::default();
        assert_eq!(proof.verifier_key.as_raw(), &[0u32; 8]);
        assert_eq!(proof.public_values_digest.as_raw(), &[0u8; 32]);
    }

    #[test]
    fn test_parse_from_bytes_genesis_no_proof() {
        let genesis_state = State {
            height: 0,
            genesis_hash: BlockHash::default(), // Should be default for initial parse
            ..Default::default()
        };
        let input = Input::new(genesis_state, RecursiveProof::default(), Vec::new())
            .unwrap()
            .to_bytes();
        let input = Input::parse(&input, dummy_hash_header).unwrap();

        assert_eq!(input.state.height, 0);
        assert_eq!(input.recursive_proof, RecursiveProof::default());
        assert!(input.headers.is_empty());
    }

    #[test]
    fn test_parse_from_bytes_non_genesis_with_proof() {
        let mut non_genesis_state = State {
            height: 100,
            ..Default::default()
        };
        // For height > 0, genesis_hash must be set
        non_genesis_state.genesis_hash = BlockHash::from_raw([1; 32]);
        non_genesis_state.block_hash = BlockHash::from_raw([2; 32]);
        non_genesis_state.header.prev_blockhash = BlockHash::from_raw([3; 32]);

        let headers: Vec<NewHeader> = vec![NewHeader {
            version: 1,
            merkle_root: [4; 32],
            timestamp: BlockTimestamp::from_consensus(100),
            nonce: 0,
        }];

        let expected_verifier_key = VerifierKeyDigest::from_raw([1; 8]);
        let expected_public_values_digest = PublicValuesDigest::from_raw([2; 32]);
        let recursive_proof_data = RecursiveProof {
            verifier_key: expected_verifier_key,
            public_values_digest: expected_public_values_digest,
        };
        let input = Input::new(non_genesis_state, recursive_proof_data, headers.clone())
            .unwrap()
            .to_bytes();
        let input = Input::parse(&input, dummy_hash_header).unwrap();

        assert_eq!(input.state.height, 100);
        assert_eq!(input.recursive_proof.verifier_key, expected_verifier_key);
        assert_eq!(
            input.recursive_proof.public_values_digest,
            expected_public_values_digest
        );
        assert_eq!(input.headers, headers);
    }

    #[test]
    fn test_parse_from_bytes_genesis_with_headers() {
        let genesis_state = State {
            height: 0,
            genesis_hash: BlockHash::default(), // Should be default for initial parse
            ..Default::default()
        };
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

        let input = Input::new(genesis_state, RecursiveProof::default(), headers.clone())
            .unwrap()
            .to_bytes();
        let input = Input::parse(&input, dummy_hash_header).unwrap();

        assert_eq!(input.state.height, 0);
        assert_eq!(input.recursive_proof, RecursiveProof::default());
        assert_eq!(input.headers, headers);
    }

    #[test]
    fn test_input_ref_rejects_misaligned_state() {
        let genesis_state = State {
            height: 0,
            genesis_hash: BlockHash::default(),
            ..Default::default()
        };
        let input = Input::new(genesis_state, RecursiveProof::default(), Vec::new())
            .unwrap()
            .to_bytes();

        let mut misaligned = Vec::with_capacity(input.len() + 1);
        misaligned.push(0);
        misaligned.extend_from_slice(&input);

        let err = InputRef::parse(&misaligned[1..]).unwrap_err();
        assert_eq!(
            err,
            InputError::Parse(ParseError::Misaligned {
                required: core::mem::align_of::<State>(),
            })
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
    fn median_time_past_hints_reject_count_mismatch() {
        let bytes = MedianTimePastHints::new(vec![BlockTimestamp::from_consensus(123)]).to_bytes();

        let err = MedianTimePastHintsRef::parse(&bytes, 2).unwrap_err();

        assert_eq!(
            err,
            MedianTimePastHintError::CountMismatch {
                expected: 2,
                actual: 1,
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
                expected: 8,
                actual: 7,
            }
        );
    }
}
