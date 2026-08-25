use zkpow_core::{HeaderInput, VerifierKeyDigest};

use super::*;
use crate::protocol::{Batch, MEDIAN_HINT_SIZE};

fn sample_batch() -> Batch {
    Batch {
        first_new_height:  7,
        end_height:        8,
        verifier_key:      VerifierKeyDigest::from_raw([0x11223344; 8]),
        current_state:     State::default(),
        header_bytes:      vec![0xAB; 2 * HeaderInput::SIZE],
        median_hint_bytes: vec![0xCD; MEDIAN_HINT_SIZE],
        previous_proof:    Some("/tmp/run/prev_proof.bin".to_string()),
        proof_upload:      "/tmp/run/proof.bin".to_string(),
    }
}

#[test]
fn batch_rejects_header_payload_mismatch() {
    let mut batch = sample_batch();
    batch.header_bytes.pop();
    assert!(batch.validate().is_err());
}
