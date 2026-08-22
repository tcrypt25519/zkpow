use zkpow_core::{HeaderInput, VerifierKeyDigest};

use super::*;
use crate::protocol::{Batch, FileLocation, MEDIAN_HINT_SIZE};

fn sample_batch() -> Batch {
    Batch {
        first_new_height:  7,
        end_height:        8,
        verifier_key:      VerifierKeyDigest::from_raw([0x11223344; 8]),
        current_state:     State::default(),
        header_bytes:      vec![0xAB; 2 * HeaderInput::SIZE],
        median_hint_bytes: vec![0xCD; MEDIAN_HINT_SIZE],
        previous_proof: Some(FileLocation::Url("https://example.test/get".to_string())),
        proof_upload: FileLocation::Path("/tmp/run/proof.bin".to_string()),
    }
}

#[test]
fn batch_round_trips() {
    let batch = sample_batch();
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&batch).unwrap();
    let decoded: Batch = rkyv::from_bytes::<Batch, rkyv::rancor::Error>(&bytes).unwrap();
    assert_eq!(decoded, batch);
}

#[test]
fn batch_rejects_trailing_bytes() {
    let mut bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&sample_batch()).unwrap();
    bytes.push(0);
    assert!(rkyv::from_bytes::<Batch, rkyv::rancor::Error>(&bytes).is_err());
}

#[test]
fn batch_rejects_header_payload_mismatch() {
    let mut batch = sample_batch();
    batch.header_bytes.pop();
    assert!(batch.validate().is_err());
}
