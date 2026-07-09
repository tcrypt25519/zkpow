//! Logic for parsing the proof-carrying state and header batch inputs to the guest.

use crate::{
    check_exact_len, slice_from_bytes, tracked, BlockTimestamp, InputError,
    MedianTimePastHintError, NewHeader, NewHeaderHintError, VerifierKeyDigest, NEW_HEADER_SIZE,
};
use alloc::vec::Vec;
use core::mem;

/// Wire size of [`ProofCarryingState`]: `VerifierKeyDigest` → `[u32; 8]` → 32 bytes LE.
const PCS_WIRE_SIZE: usize = mem::size_of::<ProofCarryingState>();

/// Authentication context for a recursive continuation.
///
/// The full private chain [`State`](crate::State) is supplied separately as private witness data.
/// Its public claim is derived by the guest when verifying the prior proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofCarryingState {
    pub verifier_key: VerifierKeyDigest,
}

impl ProofCarryingState {
    /// Constructs a new ProofCarryingState.
    pub fn new(verifier_key: VerifierKeyDigest) -> Self {
        Self { verifier_key }
    }

    /// Parse and validate from the host/guest wire format.
    #[tracked]
    pub fn parse(bytes: &[u8]) -> Result<Self, InputError> {
        check_exact_len(bytes, PCS_WIRE_SIZE)?;
        let verifier_key = cycle_track!({
            Ok::<_, InputError>(VerifierKeyDigest::from_bytes(
                bytes.try_into().expect("bytes is exactly 32 bytes"),
            ))
        })?;
        Ok(Self { verifier_key })
    }

    /// Serialize to the host/guest wire format.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(PCS_WIRE_SIZE);
        bytes.extend_from_slice(&self.verifier_key.to_bytes());
        bytes
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

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::{BlockTimestamp, ParseError};

    #[test]
    fn proof_carrying_state_round_trips_verifier_key() {
        let vk = VerifierKeyDigest::from_raw([0; 8]);
        let bytes = ProofCarryingState::new(vk).to_bytes();
        let pcs = ProofCarryingState::parse(&bytes).unwrap();

        assert_eq!(pcs.verifier_key, vk);
    }

    #[test]
    fn proof_carrying_state_rejects_wrong_length() {
        let err = ProofCarryingState::parse(&[0u8; 31]).unwrap_err();

        assert_eq!(
            err,
            InputError::Parse(ParseError::InvalidLength {
                expected: 32,
                actual: 31,
            })
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
