use zkpow_core::{BlockTimestamp as BTimestamp, CompactTarget, ContinuationData, GENESIS_NBITS, WINDOW_SIZE};

use super::*;

mod diagnostics;
mod protocol;
mod runtime;
mod storage;

fn open_db() -> HeaderStore {
    HeaderStore::open(db_path()).expect("failed to open test database")
}

fn make_pcs() -> ContinuationData {
    ContinuationData {
        current_nbits: CompactTarget::new(GENESIS_NBITS),
        epoch_start_timestamp: BTimestamp::new(500),
        timestamps: [BTimestamp::new(10); WINDOW_SIZE],
    }
}

#[test]
fn continuation_digest_changes_on_any_byte_mutation() {
    let pcs = make_pcs();
    let base_digest = continuation_digest(&pcs);

    let bytes = pcs.to_bytes();
    for i in 0..bytes.len() {
        let mut mutated = bytes;
        mutated[i] ^= 0xFF;
        if let Ok(pcs2) = ContinuationData::parse(&mutated) {
            let digest2 = continuation_digest(&pcs2);
            assert_ne!(base_digest, digest2, "digest unchanged after mutating byte {i}");
        }
    }
}

#[test]
fn continuation_digest_is_deterministic() {
    let pcs = make_pcs();
    assert_eq!(continuation_digest(&pcs), continuation_digest(&pcs));
}

#[test]
fn db_median_time_hints_match_genesis_seeded_state() {
    let db = open_db();
    let genesis = db.load_header_record(0);
    let genesis_hash = BlockHash::new(zkpow_sha256::double(&genesis.header.to_bytes()));
    let genesis_state = genesis_state_from_record(genesis, genesis_hash);
    let witness = db.load_header_batch_witness(1, 13);

    compute_final_state_with_hints(&genesis_state, &witness.headers, &witness.median_time_past_hints);
}

#[test]
fn batch_witness_loader_matches_record_projection() {
    let db = open_db();
    let records = db.load_header_records(1, 13);
    let witness = db.load_header_batch_witness(1, 13);

    assert_eq!(witness.headers, records_to_new_headers(&records));
    assert!(witness.median_time_past_hints.is_empty());
}

#[test]
fn compute_final_state_with_hints_reaches_expected_height() {
    let db = open_db();
    let genesis = db.load_header_record(0);
    let genesis_hash = BlockHash::new(zkpow_sha256::double(&genesis.header.to_bytes()));
    let genesis_state = genesis_state_from_record(genesis, genesis_hash);
    let witness = db.load_header_batch_witness(1, 128);
    let final_state =
        compute_final_state_with_hints(&genesis_state, &witness.headers, &witness.median_time_past_hints);

    assert_eq!(final_state.height, 128);
}

#[test]
fn applying_headers_one_at_a_time_matches_db_claims() {
    let db = open_db();
    let genesis = db.load_header_record(0);
    let genesis_hash = BlockHash::new(zkpow_sha256::double(&genesis.header.to_bytes()));
    let mut state = genesis_state_from_record(genesis.clone(), genesis_hash);
    let records = db.load_header_records(1, 64);
    let mut previous_ordered_run = genesis.ordered_timestamp_run;

    for (index, record) in records.iter().enumerate() {
        let header = HeaderInput::from_header(&record.header);
        let needs_hint = requires_median_hint(record.height, previous_ordered_run);
        state
            .apply_headers(
                &[header],
                if needs_hint {
                    core::slice::from_ref(&record.median_time_past)
                } else {
                    &[]
                },
            )
            .expect("header should apply");
        let height = (index as u32) + 1;
        let db_claim = db.load_public_claim(height as u64, genesis_hash);
        assert_eq!(state.public_claim(), db_claim);
        previous_ordered_run = record.ordered_timestamp_run;
    }
}

#[test]
fn db_retarget_schedule_matches_height_40320() {
    let db = open_db();
    let genesis = db.load_header_record(0);
    let genesis_hash = BlockHash::new(zkpow_sha256::double(&genesis.header.to_bytes()));
    let genesis_state = genesis_state_from_record(genesis, genesis_hash);
    let witness_1 = db.load_header_batch_witness(1, 40319);
    let state =
        compute_final_state_with_hints(&genesis_state, &witness_1.headers, &witness_1.median_time_past_hints);
    let pre_boundary_nbits = db.load_compact_target(40319);

    assert_eq!(state.height, 40319);
    assert_eq!(state.current_nbits, pre_boundary_nbits);

    let witness_2 = db.load_header_batch_witness(40320, 1);
    let boundary_state =
        compute_final_state_with_hints(&state, &witness_2.headers, &witness_2.median_time_past_hints);
    assert_eq!(boundary_state.height, 40320);
    let boundary_nbits = db.load_compact_target(40320);
    assert_eq!(boundary_state.current_nbits, boundary_nbits);
}

#[test]
fn db_chainwork_matches_simulated_batches() {
    enum InitialState {
        Genesis,
        DbHeight(u32),
    }

    let cases = [
        ("genesis-started batch", InitialState::Genesis, 1, 4096, 4096),
        ("retarget-boundary batch", InitialState::DbHeight(30240), 30241, 2016, 32256),
    ];

    let db = open_db();
    let genesis = db.load_header_record(0);
    let genesis_hash = BlockHash::new(zkpow_sha256::double(&genesis.header.to_bytes()));

    for (name, initial_state, record_start, record_count, expected_height) in cases {
        let initial_state = match initial_state {
            InitialState::Genesis => genesis_state_from_record(genesis.clone(), genesis_hash),
            InitialState::DbHeight(height) => db.state_at_height(height, genesis_hash),
        };
        let witness = db.load_header_batch_witness(record_start, record_count);
        let final_state =
            compute_final_state_with_hints(&initial_state, &witness.headers, &witness.median_time_past_hints);
        let db_claim = db.load_public_claim(expected_height as u64, genesis_hash);
        let db_nbits = db.load_compact_target(expected_height as u64);

        assert_eq!(final_state.height, expected_height, "{name}: height");
        assert_eq!(final_state.public_claim(), db_claim, "{name}: claim");
        assert_eq!(final_state.current_nbits, db_nbits, "{name}: nbits");
        assert_eq!(final_state.chain_work, db_claim.chain_work, "{name}: chainwork");
    }
}
