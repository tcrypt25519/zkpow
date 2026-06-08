//! Utilities for the zkpow prover script.

mod db;
mod hash;
mod simulate;

pub use zkpow_core::{
    parse_median_hints, parse_new_headers, serialize_median_hints, serialize_new_headers,
    target_from_bits, u256, work_from_target, ApplyFailure, BlockHash, BlockTimestamp, ChainWork,
    CompactTarget, Header, HeaderChainPublicValues, Input, InputError, MinimalPublicValues,
    NewHeader, NewHeaderHintError, ParseError, PrivateContinuationState, ProofFailure,
    PublicChainClaim, PublicValuesDigest, PublicValuesParseError, RecursiveProof, State, Target,
    ValidationErrorCode, VerifierKeyDigest, GENESIS_TARGET, MINIMAL_PV_SIZE, NEW_HEADER_SIZE,
    PRIVATE_CONTINUATION_STATE_SIZE, PUBLIC_CHAIN_CLAIM_SIZE, STATE_SIZE,
};

pub use db::{
    genesis_state_from_record, load_header_batch_witness_from_db, load_header_record_from_db,
    load_header_records_from_db, load_new_headers_from_db, state_from_db_at_height,
    HeaderBatchWitness, HeaderRecord,
};
pub use hash::{
    compute_pv_digest, continuation_digest, continuation_digest_from_state, hash_header, sha256d,
};
pub use simulate::{
    compute_final_state, compute_final_state_with_hints, median_time_past_hints_for_headers,
    median_time_past_hints_from_records, records_to_new_headers,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::db_path;
    use crate::util::simulate::median_time_past_for_state;
    use zkpow_core::{
        BlockHash, BlockTimestamp, CompactTarget, NewHeader, GENESIS_NBITS, GENESIS_TARGET,
        WINDOW_SIZE,
    };

    fn make_pcs() -> PrivateContinuationState {
        PrivateContinuationState {
            current_nbits: CompactTarget::new(GENESIS_NBITS),
            current_work: ChainWork::from_limbs([1, 2, 3, 4]),
            current_target: GENESIS_TARGET,
            epoch_start_timestamp: BlockTimestamp::new(500),
            timestamps: [BlockTimestamp::new(10); WINDOW_SIZE],
        }
    }

    fn ts(seconds: u32) -> BlockTimestamp {
        BlockTimestamp::new(seconds)
    }

    fn median_test_state(timestamps: &[u32]) -> State {
        assert!(!timestamps.is_empty());
        assert!(timestamps.len() <= WINDOW_SIZE);

        let mut state = State {
            height: timestamps.len() as u32 - 1,
            current_nbits: CompactTarget::new(GENESIS_NBITS),
            current_work: work_from_target(GENESIS_TARGET)
                .expect("GENESIS_TARGET is a valid target"),
            current_target: GENESIS_TARGET,
            ..State::default()
        };

        for (slot, timestamp) in state.timestamps.iter_mut().zip(timestamps.iter().copied()) {
            *slot = ts(timestamp);
        }

        state
    }

    fn candidate_header_after(state: &State) -> NewHeader {
        let timestamp = state
            .timestamps
            .iter()
            .map(|timestamp| timestamp.into_inner())
            .max()
            .unwrap_or_default()
            .saturating_add(1);

        NewHeader {
            version: 1,
            merkle_root: [0x22; 32],
            timestamp: ts(timestamp),
            nonce: 7,
        }
    }

    #[test]
    fn host_sorted_median_hints_agree_with_core_rank_check() {
        let cases: &[&[u32]] = &[
            &[1],
            &[1, 2],
            &[1, 2, 3],
            &[10, 1, 20, 2, 30],
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            &[1, 2, 3, 4, 5, 6, 6, 6, 7, 8, 9],
            &[90, 20, 20, 80, 30, 70, 40, 60, 50, 50, 50],
        ];

        for timestamps in cases {
            let state = median_test_state(timestamps);
            let sorted_median = median_time_past_for_state(&state);
            let header = candidate_header_after(&state);

            let mut accepted = state.clone();
            accepted
                .apply_headers(&[header], &[sorted_median], |_| BlockHash::new([0; 32]))
                .expect("core rank check should accept host-sorted median");

            if sorted_median.into_inner() > 0 {
                let lower = ts(sorted_median.into_inner() - 1);
                let rejected = std::panic::catch_unwind(|| {
                    let mut state = state.clone();
                    state
                        .apply_headers(&[header], &[lower], |_| BlockHash::new([0; 32]))
                        .unwrap();
                });
                assert!(
                    rejected.is_err(),
                    "core rank check should reject lower non-median for {timestamps:?}"
                );
            }

            let higher = ts(sorted_median.into_inner().saturating_add(1));
            if higher != sorted_median {
                let rejected = std::panic::catch_unwind(|| {
                    let mut state = state.clone();
                    state
                        .apply_headers(&[header], &[higher], |_| BlockHash::new([0; 32]))
                        .unwrap();
                });
                assert!(
                    rejected.is_err(),
                    "core rank check should reject higher non-median for {timestamps:?}"
                );
            }
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
            if let Ok(pcs2) = PrivateContinuationState::parse(&mutated) {
                let digest2 = continuation_digest(&pcs2);
                assert_ne!(
                    base_digest, digest2,
                    "digest unchanged after mutating byte {i}"
                );
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
        let genesis = load_header_record_from_db(db_path(), 0);
        let genesis_hash = hash_header(&genesis.header);
        let genesis_state = genesis_state_from_record(genesis, genesis_hash);
        let records = load_header_records_from_db(db_path(), 1, 13);
        let headers = records_to_new_headers(&records);
        let hints = median_time_past_hints_from_records(&records);

        compute_final_state_with_hints(&genesis_state, &headers, &hints);
    }

    #[test]
    fn batch_witness_loader_matches_record_projection() {
        let records = load_header_records_from_db(db_path(), 1, 13);
        let witness = load_header_batch_witness_from_db(db_path(), 1, 13);

        assert_eq!(witness.headers, records_to_new_headers(&records));
        assert_eq!(
            witness.median_time_past_hints,
            median_time_past_hints_from_records(&records)
        );
    }

    #[test]
    fn compute_final_state_with_hints_reaches_expected_height() {
        let genesis = load_header_record_from_db(db_path(), 0);
        let genesis_hash = hash_header(&genesis.header);
        let genesis_state = genesis_state_from_record(genesis, genesis_hash);
        let witness = load_header_batch_witness_from_db(db_path(), 1, 128);

        let final_state = compute_final_state_with_hints(
            &genesis_state,
            &witness.headers,
            &witness.median_time_past_hints,
        );

        assert_eq!(final_state.height, 128);
    }

    #[test]
    fn applying_headers_one_at_a_time_matches_db_claims() {
        let genesis = load_header_record_from_db(db_path(), 0);
        let genesis_hash = hash_header(&genesis.header);
        let mut state = genesis_state_from_record(genesis, genesis_hash);
        let witness = load_header_batch_witness_from_db(db_path(), 1, 64);

        for (index, (header, median_time_past)) in witness
            .headers
            .iter()
            .copied()
            .zip(witness.median_time_past_hints.iter().copied())
            .enumerate()
        {
            state
                .apply_headers(&[header], &[median_time_past], hash_header)
                .expect("header should apply");
            let height = (index as u32) + 1;
            let db_state = state_from_db_at_height(db_path(), height, genesis_hash);
            assert_eq!(state.public_claim(), db_state.public_claim());
        }
    }

    #[test]
    fn db_retarget_schedule_matches_height_40320() {
        let genesis = load_header_record_from_db(db_path(), 0);
        let genesis_hash = hash_header(&genesis.header);
        let genesis_state = genesis_state_from_record(genesis, genesis_hash);
        let records = load_header_records_from_db(db_path(), 1, 40319);
        let headers = records_to_new_headers(&records);
        let state = compute_final_state(&genesis_state, &headers);
        let pre_boundary = load_header_record_from_db(db_path(), 40319);

        assert_eq!(state.height, 40319);
        assert_eq!(state.current_nbits, pre_boundary.header.compact_target);

        let boundary = load_header_record_from_db(db_path(), 40320);
        let boundary_state =
            compute_final_state(&state, &[NewHeader::from_header(&boundary.header)]);
        assert_eq!(boundary_state.height, 40320);
        assert_eq!(boundary_state.current_nbits, boundary.header.compact_target);
    }

    #[test]
    fn db_chainwork_matches_simulated_batches() {
        enum InitialState {
            Genesis,
            DbHeight(u32),
        }

        let cases = [
            (
                "genesis-started batch",
                InitialState::Genesis,
                1,
                4096,
                4096,
            ),
            (
                "retarget-boundary batch",
                InitialState::DbHeight(30240),
                30241,
                2016,
                32256,
            ),
        ];

        let genesis = load_header_record_from_db(db_path(), 0);
        let genesis_hash = hash_header(&genesis.header);

        for (name, initial_state, record_start, record_count, expected_height) in cases {
            let initial_state = match initial_state {
                InitialState::Genesis => genesis_state_from_record(genesis.clone(), genesis_hash),
                InitialState::DbHeight(height) => {
                    state_from_db_at_height(db_path(), height, genesis_hash)
                }
            };
            let records = load_header_records_from_db(db_path(), record_start, record_count);
            let headers = records_to_new_headers(&records);
            let hints = median_time_past_hints_from_records(&records);
            let final_state = compute_final_state_with_hints(&initial_state, &headers, &hints);
            let db_state = state_from_db_at_height(db_path(), expected_height, genesis_hash);

            assert_eq!(final_state.height, expected_height, "{name}: height");
            assert_eq!(final_state.block_hash, db_state.block_hash, "{name}: hash");
            assert_eq!(
                final_state.current_nbits, db_state.current_nbits,
                "{name}: nbits"
            );
            assert_eq!(
                final_state.chain_work, db_state.chain_work,
                "{name}: chainwork"
            );
        }
    }
}
