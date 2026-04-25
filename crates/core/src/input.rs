//! Logic for parsing the prover input and constructing the `Input` struct.

use alloc::vec::Vec;

use crate::{
    check_exact_len, copy_from_bytes, copy_to_bytes, cycle_track, slice_from_bytes, BlockHash,
    BlockTimestamp, Header, NewHeader, ParseError, PublicValuesDigest, State, VerifierKeyDigest,
    NEW_HEADER_SIZE, RECURSIVE_PROOF_SIZE, STATE_SIZE,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    Parse(ParseError),
    MissingRecursiveProof,
    UnexpectedRecursiveProof,
    GenesisHashMustBeZero,
    HeaderPayloadLengthInvalid { actual: usize },
    InvalidRecursiveProofLength { actual: usize, expected: usize },
}

/// Parse and validation errors for median-time-past private witness hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MedianTimePastHintError {
    TruncatedCount { actual: usize },
    CountMismatch { expected: usize, actual: usize },
    PayloadLengthInvalid { expected: usize, actual: usize },
    Parse(ParseError),
}

impl From<ParseError> for InputError {
    fn from(value: ParseError) -> Self {
        Self::Parse(value)
    }
}

impl core::fmt::Display for InputError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Parse(err) => write!(f, "input parse error: {}", err),
            Self::MissingRecursiveProof => {
                write!(f, "missing recursive proof metadata for non-genesis state")
            }
            Self::UnexpectedRecursiveProof => {
                write!(f, "genesis state must not carry recursive proof metadata")
            }
            Self::GenesisHashMustBeZero => write!(
                f,
                "genesis state input must carry an all-zero genesis hash placeholder"
            ),
            Self::HeaderPayloadLengthInvalid { actual } => write!(
                f,
                "header payload length {} is not a multiple of {} bytes",
                actual, NEW_HEADER_SIZE
            ),
            Self::InvalidRecursiveProofLength { actual, expected } => write!(
                f,
                "invalid recursive proof length: expected {} bytes, got {}",
                expected, actual
            ),
        }
    }
}

impl core::fmt::Display for MedianTimePastHintError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TruncatedCount { actual } => {
                write!(f, "median hint payload missing count: got {} bytes", actual)
            }
            Self::CountMismatch { expected, actual } => write!(
                f,
                "median hint count mismatch: expected {}, got {}",
                expected, actual
            ),
            Self::PayloadLengthInvalid { expected, actual } => write!(
                f,
                "median hint payload length mismatch: expected {} bytes, got {}",
                expected, actual
            ),
            Self::Parse(err) => write!(f, "median hint parse error: {}", err),
        }
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
    pub fn ref_from_bytes(bytes: &[u8], offset: usize) -> Result<&Self, InputError> {
        crate::ref_from_bytes(bytes, offset).map_err(InputError::from)
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

            let medians = slice_from_bytes::<BlockTimestamp>(&bytes[4..], 4)
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
                State::ref_from_bytes(state_bytes, 0).map_err(InputError::from)
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
            let recursive_proof = RecursiveProof::ref_from_bytes(proof_bytes, proof_start)?;

            let header_payload = &bytes[proof_end..];
            if !header_payload.len().is_multiple_of(NEW_HEADER_SIZE) {
                return Err(InputError::HeaderPayloadLengthInvalid {
                    actual: header_payload.len(),
                });
            }

            let headers = cycle_track("input/parse/headers", || {
                NewHeader::slice_from_bytes(header_payload, proof_end).map_err(InputError::from)
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
        let mut state = self.state.clone();
        if state.height == 0 {
            let block_hash = hash_header(&state.header);
            state.block_hash = block_hash;
            state.genesis_hash = block_hash;
        }
        Input {
            state,
            recursive_proof: *self.recursive_proof,
            headers: self.headers.to_vec(),
        }
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
        let mut input = self.clone();
        if input.state.height == 0 {
            input.state.genesis_hash = BlockHash::default();
        }

        let mut bytes = Vec::with_capacity(
            STATE_SIZE + RECURSIVE_PROOF_SIZE + (input.headers.len() * NEW_HEADER_SIZE),
        );
        bytes.extend_from_slice(&input.state.to_bytes());
        bytes.extend_from_slice(&input.recursive_proof.to_bytes());
        for header in &input.headers {
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
                offset: 0,
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
