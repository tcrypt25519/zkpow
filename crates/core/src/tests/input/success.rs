use alloc::{vec, vec::Vec};

use zkvm_test::zkvm_test;

use crate::{cast_slice, serialize_inputs, BlockTimestamp, HeaderInput};

#[zkvm_test]
fn header_hints_round_trip_through_slice_cast() {
    let headers: Vec<HeaderInput> = vec![
        HeaderInput {
            version:     1,
            merkle_root: [4; 32],
            timestamp:   BlockTimestamp::new(100),
            nonce:       0,
        },
        HeaderInput {
            version:     2,
            merkle_root: [5; 32],
            timestamp:   BlockTimestamp::new(200),
            nonce:       1,
        },
    ];

    let bytes = serialize_inputs(&headers);
    let parsed: &[HeaderInput] = cast_slice(&bytes).expect("cast_slice header inputs");
    assert_eq!(parsed, headers.as_slice());
}

#[zkvm_test]
fn median_time_past_hints_round_trip_through_slice_cast() {
    let hints = vec![BlockTimestamp::new(0), BlockTimestamp::new(123), BlockTimestamp::new(456)];

    let bytes = serialize_inputs(&hints);
    let parsed: &[BlockTimestamp] = cast_slice(&bytes).unwrap();
    assert_eq!(parsed, hints.as_slice());
}
