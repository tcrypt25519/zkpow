//! Logic for parsing the prover input and constructing the `Input` struct.

use alloc::vec::Vec;

use rkyv::{Archive, Deserialize, Serialize};

use crate::{
    cycle_track, BlockHash, Header, NewHeader, ParseError, PublicValuesDigest, State,
    VerifierKeyDigest, NEW_HEADER_SIZE, RECURSIVE_PROOF_SIZE, STATE_SIZE,
};

// ============================================================================
// Input & InputError
// ============================================================================

/// Complete typed prover input.
#[derive(Archive, Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Input {
    pub state: State,
    pub recursive_proof: RecursiveProof,
    pub headers: Vec<NewHeader>,
}

/// Recursive proof metadata that authenticates the current [`State`].
#[derive(Archive, Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
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

impl RecursiveProof {
    /// Parse and validate a RecursiveProof from exactly [`RECURSIVE_PROOF_SIZE`] bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, InputError> {
        cycle_track("input/recursive_proof", || {
            if bytes.len() != RECURSIVE_PROOF_SIZE {
                return Err(InputError::InvalidRecursiveProofLength {
                    actual: bytes.len(),
                    expected: RECURSIVE_PROOF_SIZE,
                });
            }

            let archived = unsafe { rkyv::archived_root::<Self>(bytes) };
            archived.deserialize(&mut rkyv::Infallible).map_err(|_| {
                InputError::InvalidRecursiveProofLength {
                    actual: bytes.len(),
                    expected: RECURSIVE_PROOF_SIZE,
                }
            })
        })
    }

    /// Serialize to exactly [`RECURSIVE_PROOF_SIZE`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; RECURSIVE_PROOF_SIZE] {
        let bytes =
            rkyv::to_bytes::<_, 1024>(self).expect("failed to serialize recursive proof metadata");
        bytes
            .as_slice()
            .try_into()
            .expect("rkyv recursive proof serialization should fit in RECURSIVE_PROOF_SIZE")
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
        cycle_track("input/parse", || {
            let mut off = 0usize;
            let state = cycle_track("input/parse/state", || {
                let end = off.checked_add(STATE_SIZE).ok_or(ParseError::Truncated {
                    offset: off,
                    needed: STATE_SIZE,
                    actual: bytes.len().saturating_sub(off),
                })?;
                let state_bytes = bytes.get(off..end).ok_or(ParseError::Truncated {
                    offset: off,
                    needed: STATE_SIZE,
                    actual: bytes.len().saturating_sub(off),
                })?;
                off = end;
                State::parse(&state_bytes).map_err(InputError::from)
            });
            let proof_end = off.checked_add(RECURSIVE_PROOF_SIZE).ok_or(
                InputError::InvalidRecursiveProofLength {
                    actual: bytes.len().saturating_sub(off),
                    expected: RECURSIVE_PROOF_SIZE,
                },
            )?;
            let proof_bytes =
                bytes
                    .get(off..proof_end)
                    .ok_or(InputError::InvalidRecursiveProofLength {
                        actual: bytes.len().saturating_sub(off),
                        expected: RECURSIVE_PROOF_SIZE,
                    })?;
            let mut input = Self {
                state: state?,
                recursive_proof: RecursiveProof::parse(proof_bytes)?,
                headers: Vec::new(),
            };
            off = proof_end;

            let header_payload_len = bytes.len().saturating_sub(off);
            if header_payload_len % NEW_HEADER_SIZE != 0 {
                return Err(InputError::HeaderPayloadLengthInvalid {
                    actual: header_payload_len,
                });
            }

            cycle_track("input/parse/headers", || {
                let header_count = header_payload_len / NEW_HEADER_SIZE;
                input.headers = Vec::with_capacity(header_count);
                for header_bytes in bytes[off..].chunks_exact(NEW_HEADER_SIZE) {
                    input.headers.push(NewHeader::parse(header_bytes)?);
                }
                Ok::<(), InputError>(())
            })?;

            if input.state.height == 0 {
                if input.state.genesis_hash != BlockHash::default() {
                    return Err(InputError::GenesisHashMustBeZero);
                }

                cycle_track("input/parse/genesis_hash", || {
                    let block_hash = hash_header(&input.state.header);
                    input.state.block_hash = block_hash;
                    input.state.genesis_hash = block_hash;
                });
            }

            Ok(input)
        })
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
}
