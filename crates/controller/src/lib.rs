pub mod diagnostics;
pub mod http;

mod digest;
mod simulate;

pub use digest::{continuation_digest, continuation_digest_from_state};
pub use simulate::{
    compute_final_state_with_hints, median_time_past_hints_from_records, records_to_new_headers,
};

pub use diagnostics::format_claim_mismatch;
pub use zkpow_core::{
    serialize_median_hints, serialize_new_headers, sha256d, BlockHash, BlockTimestamp, NewHeader,
    State,
};
pub use zkpow_store::{
    db_path, genesis_state_from_record, HeaderBatchWitness, HeaderRecord, HeaderStore,
};

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub const NO_HEADERS_REMAINING_PREFIX: &str = "no headers remaining in database";

/// Stateless source of chain data for a batch of headers, backed by a local SQLite database.
pub struct Controller {
    store: HeaderStore,
    genesis_hash: BlockHash,
}

/// The data needed to build a batch's SP1 stdin witness, plus (optionally) the
/// dry-run-derived expected end state for cross-checking against the proof's output.
#[derive(Debug)]
pub struct FetchedBatch {
    pub headers: Vec<NewHeader>,
    pub median_hints: Vec<BlockTimestamp>,
    pub expected_state: Option<State>,
    pub expected_continuation_digest: Option<[u8; 32]>,
    pub first_new_height: u32,
    pub end_height: u32,
}

impl Controller {
    pub fn new(store: HeaderStore, genesis_hash: BlockHash) -> Self {
        Self {
            store,
            genesis_hash,
        }
    }

    pub fn genesis_hash(&self) -> BlockHash {
        self.genesis_hash
    }

    /// Reconstruct full consensus state at an arbitrary height.
    pub fn state_at(&self, height: u32) -> State {
        self.store.state_at_height(height, self.genesis_hash)
    }

    /// Fetch the next `num_headers` headers after `current_state`, optionally dry-running
    /// them against the DB-recorded ground truth (`simulate`) to catch consensus-logic
    /// divergence before it reaches the prover.
    pub fn fetch_batch(
        &self,
        current_state: &State,
        num_headers: u32,
        simulate: bool,
    ) -> Result<FetchedBatch, BoxError> {
        let first_new_height = current_state.height + 1;
        let witness = self
            .store
            .load_header_batch_witness(first_new_height as u64, num_headers as u64);
        if witness.headers.is_empty() {
            return Err(format!(
                "{NO_HEADERS_REMAINING_PREFIX}: starting at height {first_new_height}"
            )
            .into());
        }
        let loaded_count = witness.headers.len() as u32;
        let end_height = current_state.height + loaded_count;
        let sparse_median_hints = current_state
            .sparse_median_hints_for_headers(&witness.headers, &witness.median_time_past_hints);

        let (expected_state, expected_continuation_digest) = if simulate {
            let expected_state = self.simulate_expected_state(
                current_state,
                &witness.headers,
                &sparse_median_hints,
                &witness.median_time_past_hints,
                first_new_height,
                end_height,
            )?;
            let digest = continuation_digest_from_state(&expected_state);
            (Some(expected_state), Some(digest))
        } else {
            (None, None)
        };

        Ok(FetchedBatch {
            headers: witness.headers,
            median_hints: sparse_median_hints,
            expected_state,
            expected_continuation_digest,
            first_new_height,
            end_height,
        })
    }

    fn simulate_expected_state(
        &self,
        current_state: &State,
        headers: &[NewHeader],
        median_hints: &[BlockTimestamp],
        dense_median_hints: &[BlockTimestamp],
        first_new_height: u32,
        end_height: u32,
    ) -> Result<State, BoxError> {
        let expected_state = compute_final_state_with_hints(current_state, headers, median_hints);
        let db_end_claim = self
            .store
            .load_public_claim(end_height as u64, self.genesis_hash);

        if expected_state.public_claim() == db_end_claim {
            return Ok(expected_state);
        }

        let (bad_index, actual_state) = self.find_first_diverging_state(
            current_state,
            headers,
            dense_median_hints,
            first_new_height,
        );

        let bad_height = first_new_height + (bad_index as u32);
        let expected_claim = self
            .store
            .load_public_claim(bad_height as u64, self.genesis_hash);
        let actual_claim = actual_state.public_claim();

        Err(format!(
            "host simulation diverges from database\n  first bad header index in batch: {}\n  bad height: {}\n{}",
            bad_index,
            bad_height,
            format_claim_mismatch(&actual_claim, &expected_claim),
        )
        .into())
    }

    fn find_first_diverging_state(
        &self,
        current_state: &State,
        headers: &[NewHeader],
        dense_median_hints: &[BlockTimestamp],
        first_new_height: u32,
    ) -> (usize, State) {
        assert!(
            !headers.is_empty(),
            "find_first_diverging_state requires at least one header"
        );
        assert_eq!(
            headers.len(),
            dense_median_hints.len(),
            "dense median hint count must match header count"
        );

        let mut replay_state = current_state.clone();
        let mut replayed_states = Vec::new();
        let mut lo = 0usize;
        let mut hi = headers.len() - 1;

        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            replay_states_through(
                &mut replay_state,
                &mut replayed_states,
                headers,
                dense_median_hints,
                mid,
            );

            let height = first_new_height + (mid as u32);
            let db_claim = self
                .store
                .load_public_claim(height as u64, self.genesis_hash);
            if replayed_states[mid].public_claim() == db_claim {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }

        replay_states_through(
            &mut replay_state,
            &mut replayed_states,
            headers,
            dense_median_hints,
            lo,
        );

        (lo, replayed_states[lo].clone())
    }
}

fn replay_states_through(
    state: &mut State,
    replayed_states: &mut Vec<State>,
    headers: &[NewHeader],
    dense_median_hints: &[BlockTimestamp],
    target_index: usize,
) {
    while replayed_states.len() <= target_index {
        let index = replayed_states.len();
        let median_hints =
            state.sparse_median_hints_for_headers(&[headers[index]], &[dense_median_hints[index]]);
        state
            .apply_headers(
                &[headers[index]],
                &median_hints,
                |new_header, prev_blockhash, compact_target| {
                    let header = (*new_header).into_header(*prev_blockhash, compact_target);
                    BlockHash::new(sha256d(&header.to_bytes()))
                },
            )
            .unwrap_or_else(|err| {
                panic!("host state transition should succeed at header index {index}: {err:?}")
            });
        replayed_states.push(state.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zkpow_core::{
        BlockTimestamp as BTimestamp, CompactTarget, ContinuationData, GENESIS_NBITS, WINDOW_SIZE,
    };

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
        let db = open_db();
        let genesis = db.load_header_record(0);
        let genesis_hash = BlockHash::new(sha256d(&genesis.header.to_bytes()));
        let genesis_state = genesis_state_from_record(genesis, genesis_hash);
        let records = db.load_header_records(1, 13);
        let headers = records_to_new_headers(&records);
        let dense_hints = median_time_past_hints_from_records(&records);
        let hints = genesis_state.sparse_median_hints_for_headers(&headers, &dense_hints);

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
        let genesis_hash = BlockHash::new(sha256d(&genesis.header.to_bytes()));
        let genesis_state = genesis_state_from_record(genesis, genesis_hash);
        let witness = db.load_header_batch_witness(1, 128);
        let sparse_hints = genesis_state
            .sparse_median_hints_for_headers(&witness.headers, &witness.median_time_past_hints);

        let final_state =
            compute_final_state_with_hints(&genesis_state, &witness.headers, &sparse_hints);

        assert_eq!(final_state.height, 128);
    }

    #[test]
    fn applying_headers_one_at_a_time_matches_db_claims() {
        let db = open_db();
        let genesis = db.load_header_record(0);
        let genesis_hash = BlockHash::new(sha256d(&genesis.header.to_bytes()));
        let mut state = genesis_state_from_record(genesis, genesis_hash);
        let witness = db.load_header_batch_witness(1, 64);

        for (index, (header, median_time_past)) in witness
            .headers
            .iter()
            .copied()
            .zip(witness.median_time_past_hints.iter().copied())
            .enumerate()
        {
            let sparse_hints =
                state.sparse_median_hints_for_headers(&[header], &[median_time_past]);
            state
                .apply_headers(
                    &[header],
                    &sparse_hints,
                    |new_header, prev_blockhash, compact_target| {
                        let header = (*new_header).into_header(*prev_blockhash, compact_target);
                        BlockHash::new(sha256d(&header.to_bytes()))
                    },
                )
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
        let genesis_hash = BlockHash::new(sha256d(&genesis.header.to_bytes()));
        let genesis_state = genesis_state_from_record(genesis, genesis_hash);
        let witness_1 = db.load_header_batch_witness(1, 40319);
        let sparse_hints_1 = genesis_state
            .sparse_median_hints_for_headers(&witness_1.headers, &witness_1.median_time_past_hints);
        let state =
            compute_final_state_with_hints(&genesis_state, &witness_1.headers, &sparse_hints_1);
        let pre_boundary_nbits = db.load_compact_target(40319);

        assert_eq!(state.height, 40319);
        assert_eq!(state.current_nbits, pre_boundary_nbits);

        let witness_2 = db.load_header_batch_witness(40320, 1);
        let sparse_hints_2 = state
            .sparse_median_hints_for_headers(&witness_2.headers, &witness_2.median_time_past_hints);
        let boundary_state =
            compute_final_state_with_hints(&state, &witness_2.headers, &sparse_hints_2);
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
        let genesis_hash = BlockHash::new(sha256d(&genesis.header.to_bytes()));

        for (name, initial_state, record_start, record_count, expected_height) in cases {
            let initial_state = match initial_state {
                InitialState::Genesis => genesis_state_from_record(genesis.clone(), genesis_hash),
                InitialState::DbHeight(height) => db.state_at_height(height, genesis_hash),
            };
            let witness = db.load_header_batch_witness(record_start, record_count);
            let sparse_hints = initial_state
                .sparse_median_hints_for_headers(&witness.headers, &witness.median_time_past_hints);
            let final_state =
                compute_final_state_with_hints(&initial_state, &witness.headers, &sparse_hints);
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

    #[test]
    fn fetch_batch_starts_after_genesis() {
        let db = open_db();
        let genesis_hash = BlockHash::new(sha256d(&db.load_header_record(0).header.to_bytes()));
        let controller = Controller::new(db, genesis_hash);
        let current_state = controller.state_at(0);

        let fetched = controller.fetch_batch(&current_state, 1, true).unwrap();

        assert_eq!(fetched.first_new_height, 1);
        assert_eq!(fetched.end_height, 1);
        assert!(fetched.expected_state.is_some());
        assert!(fetched.expected_continuation_digest.is_some());
    }

    #[test]
    fn fetch_batch_without_simulate_skips_expected_state() {
        let db = open_db();
        let genesis_hash = BlockHash::new(sha256d(&db.load_header_record(0).header.to_bytes()));
        let controller = Controller::new(db, genesis_hash);
        let current_state = controller.state_at(0);

        let fetched = controller.fetch_batch(&current_state, 1, false).unwrap();

        assert!(fetched.expected_state.is_none());
        assert!(fetched.expected_continuation_digest.is_none());
    }

    #[test]
    fn fetch_batch_errors_when_headers_exhausted() {
        let db = open_db();
        let genesis_hash = BlockHash::new(sha256d(&db.load_header_record(0).header.to_bytes()));
        let controller = Controller::new(db, genesis_hash);
        let current_state = controller.state_at(2016);

        let err = controller.fetch_batch(&current_state, 0, true).unwrap_err();
        assert!(err.to_string().starts_with(NO_HEADERS_REMAINING_PREFIX));
    }
}
