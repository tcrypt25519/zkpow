use alloc::vec;

use bytemuck::PodCastError;
use zkvm_test::zkvm_test;

use zkpow_types::{BlockTimestamp, MerkleRoot};

use crate::HeaderInput;

#[zkvm_test]
fn header_hints_reject_truncated_payload_with_trailing_slop() {
    let headers = vec![HeaderInput {
        version:     1,
        merkle_root: MerkleRoot::new([4; 32]),
        timestamp:   BlockTimestamp::new(100),
        nonce:       0,
    }];
    let mut bytes = bytemuck::cast_slice::<_, u8>(&headers).to_vec();
    bytes.pop();

    let err: PodCastError = bytemuck::try_cast_slice::<u8, HeaderInput>(&bytes).expect_err("expected slop error");
    assert_eq!(err, PodCastError::OutputSliceWouldHaveSlop);
}

#[zkvm_test]
fn median_time_past_hints_reject_invalid_byte_lengths() {
    // Variant is placement-dependent (alignment is checked before length),
    // so only assert that a 3-byte payload is rejected.
    assert!(bytemuck::try_cast_slice::<u8, BlockTimestamp>(&[1, 2, 3]).is_err());
}
