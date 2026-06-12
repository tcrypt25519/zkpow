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
    genesis_state_from_record, state_from_db_at_height, DbConfig, DbConn, HeaderBatchWitness,
    HeaderRecord,
};
pub use hash::{
    compute_pv_digest, continuation_digest, continuation_digest_from_state, hash_header, sha256d,
};
pub use simulate::{
    compute_final_state_with_hints, median_time_past_hints_from_records, records_to_new_headers,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::db_path;
    use zkpow_core::{BlockTimestamp, CompactTarget, GENESIS_NBITS, GENESIS_TARGET, WINDOW_SIZE};

    fn make_pcs() -> PrivateContinuationState {
        PrivateContinuationState {
            current_nbits: CompactTarget::new(GENESIS_NBITS),
            current_work: ChainWork::from_limbs([1, 2, 3, 4]),
            current_target: GENESIS_TARGET,
            epoch_start_timestamp: BlockTimestamp::new(500),
            timestamps: [BlockTimestamp::new(10); WINDOW_SIZE],
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

    fn open_db() -> DbConn {
        DbConfig::new(db_path())
            .connect()
            .expect("failed to open test database")
    }

    #[test]
    fn db_median_time_hints_match_genesis_seeded_state() {
        let db = open_db();
        let genesis = db.load_header_record(0);
        let genesis_hash = hash_header(&genesis.header);
        let genesis_state = genesis_state_from_record(genesis, genesis_hash);
        let records = db.load_header_records(1, 13);
        let headers = records_to_new_headers(&records);
        let hints = median_time_past_hints_from_records(&records);

        compute_final_state_with_hints(&genesis_state, &headers, &hints);
    }

    #[test]
    fn batch_witness_loader_matches_record_projection() {
        let db = open_db();
        let records = db.load_header_records(1, 13);
        let witness = db.load_header_batch_witness(1, 13);

        assert_eq!(witness.headers, records_to_new_headers(&records));
        assert_eq!(
            witness.median_time_past_hints,
            median_time_past_hints_from_records(&records)
        );
    }

    #[test]
    fn compute_final_state_with_hints_reaches_expected_height() {
        let db = open_db();
        let genesis = db.load_header_record(0);
        let genesis_hash = hash_header(&genesis.header);
        let genesis_state = genesis_state_from_record(genesis, genesis_hash);
        let witness = db.load_header_batch_witness(1, 128);

        let final_state = compute_final_state_with_hints(
            &genesis_state,
            &witness.headers,
            &witness.median_time_past_hints,
        );

        assert_eq!(final_state.height, 128);
    }

    #[test]
    fn applying_headers_one_at_a_time_matches_db_claims() {
        let db = open_db();
        let genesis = db.load_header_record(0);
        let genesis_hash = hash_header(&genesis.header);
        let mut state = genesis_state_from_record(genesis, genesis_hash);
        let witness = db.load_header_batch_witness(1, 64);

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
            let db_claim = db.load_public_claim(height as u64, genesis_hash);
            assert_eq!(state.public_claim(), db_claim);
        }
    }

    #[test]
    fn db_retarget_schedule_matches_height_40320() {
        let db = open_db();
        let genesis = db.load_header_record(0);
        let genesis_hash = hash_header(&genesis.header);
        let genesis_state = genesis_state_from_record(genesis, genesis_hash);
        let witness_1 = db.load_header_batch_witness(1, 40319);
        let state = compute_final_state_with_hints(
            &genesis_state,
            &witness_1.headers,
            &witness_1.median_time_past_hints,
        );
        let pre_boundary_nbits = db.load_compact_target(40319);

        assert_eq!(state.height, 40319);
        assert_eq!(state.current_nbits, pre_boundary_nbits);

        let witness_2 = db.load_header_batch_witness(40320, 1);
        let boundary_state = compute_final_state_with_hints(
            &state,
            &witness_2.headers,
            &witness_2.median_time_past_hints,
        );
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

        let db = open_db();
        let genesis = db.load_header_record(0);
        let genesis_hash = hash_header(&genesis.header);

        for (name, initial_state, record_start, record_count, expected_height) in cases {
            let initial_state = match initial_state {
                InitialState::Genesis => genesis_state_from_record(genesis.clone(), genesis_hash),
                InitialState::DbHeight(height) => {
                    state_from_db_at_height(&db, height, genesis_hash)
                }
            };
            let witness = db.load_header_batch_witness(record_start, record_count);
            let final_state = compute_final_state_with_hints(
                &initial_state,
                &witness.headers,
                &witness.median_time_past_hints,
            );
            let db_claim = db.load_public_claim(expected_height as u64, genesis_hash);
            let db_nbits = db.load_compact_target(expected_height as u64);

            assert_eq!(final_state.height, expected_height, "{name}: height");
            assert_eq!(final_state.public_claim(), db_claim, "{name}: claim");
            assert_eq!(final_state.current_nbits, db_nbits, "{name}: nbits");
            assert_eq!(
                final_state.chain_work, db_claim.chain_work,
                "{name}: chainwork"
            );
        }
    }
}
