//! Logic for parsing the proof-carrying state and header batch inputs to the guest.

use alloc::vec::Vec;

use crate::{
    check_exact_len, slice_from_bytes, tracked, BlockTimestamp, Claim, NewHeader, ParseError,
    VerifierKeyDigest, CLAIM_SIZE, NEW_HEADER_SIZE,
};

// ============================================================================
// ProofCarryingState
// ============================================================================

/// The complete output of the prior execution: the proven chain state plus its SP1 proof.
///
/// The full private chain [`State`](crate::State) is supplied separately as private witness data.
/// Its public preimage fields must match [`claim`](Self::claim) before the
/// guest uses it to validate headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofCarryingState {
    pub claim: Claim,
    pub verifier_key: VerifierKeyDigest,
}

/// Parse and validation errors for [`ProofCarryingState`].
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
#[tracked]
pub fn parse_new_headers(bytes: &[u8]) -> Result<&[NewHeader], NewHeaderHintError> {
    if !bytes.len().is_multiple_of(NEW_HEADER_SIZE) {
        return Err(NewHeaderHintError::PayloadLengthInvalid {
            expected: bytes.len().div_ceil(NEW_HEADER_SIZE) * NEW_HEADER_SIZE,
            actual: bytes.len(),
        });
    }

    let headers = slice_from_bytes::<NewHeader>(bytes).map_err(NewHeaderHintError::Parse)?;
    Ok(headers)
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
#[tracked]
pub fn parse_median_hints(
    bytes: &[u8],
    expected_count: usize,
) -> Result<&[BlockTimestamp], MedianTimePastHintError> {
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
}

/// Wire size of the verifier key field in [`ProofCarryingState`]: `VerifierKeyDigest` → `[u32; 8]` → 32 bytes LE.
const VK_WIRE_SIZE: usize = 32;

/// Split and validate the wire layout, returning `(claim_bytes, vk_bytes)`.
///
/// The full [`State`](crate::State) is supplied separately as private witness data
/// and checked against the claim before use.
#[allow(clippy::type_complexity)]
fn split_pcs_wire(bytes: &[u8]) -> Result<(&[u8], &[u8]), InputError> {
    check_exact_len(bytes, CLAIM_SIZE + VK_WIRE_SIZE)?;
    Ok((
        &bytes[..CLAIM_SIZE],
        &bytes[CLAIM_SIZE..CLAIM_SIZE + VK_WIRE_SIZE],
    ))
}

impl ProofCarryingState {
    /// Constructs a new ProofCarryingState.
    pub fn new(claim: Claim, verifier_key: VerifierKeyDigest) -> Self {
        Self {
            claim,
            verifier_key,
        }
    }

    /// Parse and validate from the host/guest wire format.
    #[tracked]
    pub fn parse(bytes: &[u8]) -> Result<Self, InputError> {
        let (claim_bytes, vk_bytes) = split_pcs_wire(bytes)?;
        let claim = cycle_track!({ Claim::parse(claim_bytes).map_err(InputError::from) })?;
        let verifier_key = cycle_track!({
            Ok::<_, InputError>(VerifierKeyDigest::from_bytes(
                vk_bytes.try_into().expect("vk_bytes is exactly 32 bytes"),
            ))
        })?;
        Ok(Self {
            claim,
            verifier_key,
        })
    }

    /// Serialize to the host/guest wire format.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(CLAIM_SIZE + VK_WIRE_SIZE);
        bytes.extend_from_slice(&self.claim.to_bytes());
        bytes.extend_from_slice(&self.verifier_key.to_bytes());
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
    fn test_parse_from_bytes_genesis_no_proof() {
        let genesis_state: State = State {
            height: 0,
            genesis_hash: BlockHash::new([1; 32]),
            block_hash: BlockHash::new([2; 32]),
            ..Default::default()
        };
        let claim = genesis_state.public_claim();
        let vk = VerifierKeyDigest::from_raw([0; 8]);
        let bytes = ProofCarryingState::new(claim, vk).to_bytes();
        let pcs = ProofCarryingState::parse(&bytes).unwrap();

        assert_eq!(pcs.claim.height, 0);
        assert_eq!(pcs.claim.genesis_hash, BlockHash::new([1; 32]));
        assert_eq!(pcs.claim.tip_hash, BlockHash::new([2; 32]));
        assert_eq!(pcs.verifier_key, vk);
    }

    #[test]
    fn test_parse_from_bytes_non_genesis_with_proof() {
        let mut non_genesis_state: State = State {
            height: 100,
            ..Default::default()
        };
        non_genesis_state.genesis_hash = BlockHash::new([1; 32]);
        non_genesis_state.block_hash = BlockHash::new([2; 32]);
        non_genesis_state.header.prev_blockhash = BlockHash::new([3; 32]);

        let expected_verifier_key = VerifierKeyDigest::from_raw([1; 8]);
        let claim = non_genesis_state.public_claim();
        let bytes = ProofCarryingState::new(claim, expected_verifier_key).to_bytes();
        let pcs = ProofCarryingState::parse(&bytes).unwrap();

        assert_eq!(pcs.claim.height, 100);
        assert_eq!(pcs.verifier_key, expected_verifier_key);
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
