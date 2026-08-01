use zkpow_core::{VerifierKeyDigest, CONTINUATION_DATA_SIZE, NEW_HEADER_SIZE};

use super::*;
use crate::protocol::{decode, encode, Batch, FileLocation, MEDIAN_HINT_SIZE};

fn sample_batch() -> Batch {
    Batch {
        first_new_height: 7,
        end_height: 8,
        verifier_key: VerifierKeyDigest::from_raw([0x11223344; 8]),
        current_state: State::default(),
        header_bytes: vec![0xAB; 2 * NEW_HEADER_SIZE],
        median_hint_bytes: vec![0xCD; MEDIAN_HINT_SIZE],
        previous_proof: Some(FileLocation::Url("https://example.test/get".to_string())),
        proof_upload: FileLocation::Path("/tmp/run/proof.bin".to_string()),
        continuation_bytes: vec![0xEF; CONTINUATION_DATA_SIZE],
        continuation_upload: FileLocation::Path("/tmp/run/continuation.bin".to_string()),
    }
}

#[test]
fn batch_round_trips() {
    let batch = sample_batch();
    assert_eq!(decode::<Batch>(&encode(&batch).unwrap()).unwrap(), batch);
}

#[test]
fn batch_rejects_trailing_bytes() {
    let mut bytes = encode(&sample_batch()).unwrap();
    bytes.push(0);
    assert!(decode::<Batch>(&bytes).is_err());
}

#[test]
fn batch_rejects_header_payload_mismatch() {
    let mut batch = sample_batch();
    batch.header_bytes.pop();
    assert!(batch.validate().is_err());
}
