//! Reading the proof-carrying state, header batch, and hint inputs to the guest.

use alloc::vec::Vec;

use bytemuck::{Pod, PodCastError};

pub fn cast_slice<B: Pod>(bytes: &[u8]) -> Result<&[B], PodCastError> {
    bytemuck::try_cast_slice(bytes)
}

pub fn serialize_inputs<B: Pod>(values: &[B]) -> Vec<u8> {
    bytemuck::cast_slice(values).to_vec()
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use bytemuck::PodCastError;

    use super::*;
    use crate::{BlockTimestamp, HeaderInput};

    #[test]
    fn test_new_header_hints_round_trip() {
        let headers: Vec<HeaderInput> = vec![
            HeaderInput {
                version: 1,
                merkle_root: [4; 32],
                timestamp: BlockTimestamp::new(100),
                nonce: 0,
            },
            HeaderInput {
                version: 2,
                merkle_root: [5; 32],
                timestamp: BlockTimestamp::new(200),
                nonce: 1,
            },
        ];

        let bytes = serialize_inputs(&headers);
        let parsed: &[HeaderInput] = cast_slice(&bytes).expect("a");
        assert_eq!(parsed, headers.as_slice());
    }

    #[test]
    fn test_new_header_hints_reject_truncated_payload() {
        let headers = vec![HeaderInput {
            version: 1,
            merkle_root: [4; 32],
            timestamp: BlockTimestamp::new(100),
            nonce: 0,
        }];
        let mut bytes = serialize_inputs(&headers);
        bytes.pop();

        let err: PodCastError = cast_slice::<HeaderInput>(&bytes).expect_err("a");
        assert_eq!(err, PodCastError::OutputSliceWouldHaveSlop)
    }

    #[test]
    fn median_time_past_hints_round_trip() {
        let hints = vec![BlockTimestamp::new(0), BlockTimestamp::new(123), BlockTimestamp::new(456)];

        let bytes = serialize_inputs(&hints);
        let parsed: &[BlockTimestamp] = cast_slice(&bytes).unwrap();
        assert_eq!(parsed, hints.as_slice());
    }

    #[test]
    fn median_time_past_hints_reject_invalid_lengths() {
        // Variant is placement-dependent (alignment is checked before length),
        // so only assert that a 3-byte payload is rejected.
        assert!(cast_slice::<BlockTimestamp>(&[1, 2, 3]).is_err());
    }
}
