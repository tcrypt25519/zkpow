//! Logic for parsing the proof-carrying state and header batch inputs to the guest.

use crate::{
    slice_from_bytes, tracked, BlockTimestamp, InputParseError, NewHeader, NEW_HEADER_SIZE,
};
use alloc::vec::Vec;

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
pub fn parse_new_headers(bytes: &[u8]) -> Result<&[NewHeader], InputParseError> {
    if !bytes.len().is_multiple_of(NEW_HEADER_SIZE) {
        return Err(InputParseError::InvalidLength {
            expected: bytes.len().div_ceil(NEW_HEADER_SIZE) * NEW_HEADER_SIZE,
            actual: bytes.len(),
        });
    }

    let headers = slice_from_bytes::<NewHeader>(bytes).map_err(InputParseError::Parse)?;
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
pub fn parse_median_hints(bytes: &[u8]) -> Result<&[BlockTimestamp], InputParseError> {
    let hint_size = core::mem::size_of::<BlockTimestamp>();
    if !bytes.len().is_multiple_of(hint_size) {
        return Err(InputParseError::InvalidLength {
            expected: bytes.len().div_ceil(hint_size) * hint_size,
            actual: bytes.len(),
        });
    }

    let medians = slice_from_bytes::<BlockTimestamp>(bytes).map_err(InputParseError::Parse)?;
    Ok(medians)
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::BlockTimestamp;

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
            InputParseError::InvalidLength {
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
        let parsed = parse_median_hints(&bytes).unwrap();

        assert_eq!(parsed, hints.as_slice());
    }

    #[test]
    fn median_time_past_hints_reject_invalid_lengths() {
        let err = parse_median_hints(&[1, 2, 3]).unwrap_err();
        assert_eq!(
            err,
            InputParseError::InvalidLength {
                expected: 4,
                actual: 3,
            }
        );
    }
}
