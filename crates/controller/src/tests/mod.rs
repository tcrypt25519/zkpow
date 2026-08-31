use zkpow_params::Mainnet;

use super::*;
use crate::validation::header_to_input;

mod protocol;
mod runtime;
mod storage;

fn open_db() -> HeaderStore {
    HeaderStore::open(db_path()).expect("failed to open test database")
}

#[test]
fn db_median_time_hints_match_genesis_seeded_state() {
    let db = open_db();
    let genesis = db.load_header_record(0);
    let genesis_hash = BlockHash::from(&genesis.header);
    let genesis_state = genesis_state_from_record(genesis, genesis_hash);
    let witness = db.load_header_batch_witness(1, 13);

    compute_final_state_cold::<Mainnet>(&genesis_state, &witness.headers, &witness.median_time_past_hints);
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
    let genesis_hash = BlockHash::from(&genesis.header);
    let genesis_state = genesis_state_from_record(genesis, genesis_hash);
    let witness = db.load_header_batch_witness(1, 128);
    let final_state =
        compute_final_state_cold::<Mainnet>(&genesis_state, &witness.headers, &witness.median_time_past_hints);

    assert_eq!(final_state.height, 128);
}

#[test]
fn applying_headers_one_at_a_time_matches_db_claims() {
    let db = open_db();
    let genesis = db.load_header_record(0);
    let genesis_hash = BlockHash::from(&genesis.header);
    let mut state = genesis_state_from_record(genesis.clone(), genesis_hash);
    let records = db.load_header_records(1, 64);
    let mut previous_ordered_run = genesis.ordered_timestamp_run;

    for (index, record) in records.iter().enumerate() {
        let header = header_to_input(&record.header);
        let needs_hint = requires_median_hint(record.height, previous_ordered_run);
        let hints = if needs_hint {
            core::slice::from_ref(&record.median_time_past)
        } else {
            &[]
        };
        let result = if state.height == 0 {
            state.apply_headers_from_genesis::<Mainnet>(&[header], hints)
        } else {
            state.apply_headers::<Mainnet>(&[header], hints)
        };
        let height = (index as u32) + 1;
        if let Err(err) = result {
            panic!("header {height} failed to apply: {err}");
        }
        assert_eq!(state.height, height);
        assert_eq!(state.chain_work, record.chain_work);
        assert_eq!(state.block_hash, BlockHash::from(&record.header));
        assert_eq!(state.genesis_hash, genesis_hash);
        previous_ordered_run = record.ordered_timestamp_run;
    }
}

#[test]
fn db_retarget_schedule_matches_height_40320() {
    let db = open_db();
    let genesis = db.load_header_record(0);
    let genesis_hash = BlockHash::from(&genesis.header);
    let genesis_state = genesis_state_from_record(genesis, genesis_hash);
    let witness_1 = db.load_header_batch_witness(1, 40319);
    let state =
        compute_final_state_cold::<Mainnet>(&genesis_state, &witness_1.headers, &witness_1.median_time_past_hints);
    let pre_boundary_nbits = db.load_compact_target(40319);

    assert_eq!(state.height, 40319);
    assert_eq!(state.current_nbits, pre_boundary_nbits);

    let witness_2 = db.load_header_batch_witness(40320, 1);
    let boundary_state =
        compute_final_state_warm::<Mainnet>(&state, &witness_2.headers, &witness_2.median_time_past_hints);
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
    let genesis_hash = BlockHash::from(&genesis.header);

    for (name, source, record_start, record_count, expected_height) in cases {
        let is_genesis = matches!(source, InitialState::Genesis);
        let initial_state = match source {
            InitialState::Genesis => genesis_state_from_record(genesis.clone(), genesis_hash),
            InitialState::DbHeight(height) => db.state_at_height::<Mainnet>(height, genesis_hash),
        };
        let witness = db.load_header_batch_witness(record_start, record_count);
        let final_state = if is_genesis {
            compute_final_state_cold::<Mainnet>(&initial_state, &witness.headers, &witness.median_time_past_hints)
        } else {
            compute_final_state_warm::<Mainnet>(&initial_state, &witness.headers, &witness.median_time_past_hints)
        };
        let db_record = db.load_header_record(expected_height as u64);
        let db_nbits = db.load_compact_target(expected_height as u64);

        assert_eq!(final_state.height, expected_height, "{name}: height");
        assert_eq!(final_state.block_hash, BlockHash::from(&db_record.header), "{name}: tip hash");
        assert_eq!(final_state.chain_work, db_record.chain_work, "{name}: chainwork");
        assert_eq!(final_state.current_nbits, db_nbits, "{name}: nbits");
    }
}
