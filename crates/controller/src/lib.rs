pub mod diagnostics;
pub mod http;

mod digest;
mod validation;

pub use digest::{continuation_digest, continuation_digest_from_state};
pub use validation::{
    compute_final_state_with_hints, median_time_past_hints_from_records, records_to_new_headers,
};

pub use diagnostics::format_claim_mismatch;
pub use zkpow_core::{
    requires_median_hint, serialize_median_hints, serialize_new_headers, sha256d, BlockHash,
    BlockTimestamp, Claim, HeaderChainPublicValues, NewHeader, State,
};
pub use zkpow_store::{
    db_path, genesis_state_from_record, HeaderBatchWitness, HeaderRecord, HeaderStore,
};

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub const NO_HEADERS_REMAINING_PREFIX: &str = "no headers remaining in database";

/// Controller-owned batch session state.
struct ControllerSession {
    current_state: State,
    pending: Option<PendingWork>,
}

struct PendingWork {
    expected_state: State,
    expected_continuation_digest: [u8; 32],
}

/// Initial state selected by the controller when beginning a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStart {
    Genesis,
    TrustedHeight(u32),
    PriorPublicValues(Vec<u8>),
}

/// Controller state and its source database.
pub struct Controller {
    store: HeaderStore,
    genesis_hash: BlockHash,
    sessions: std::collections::HashMap<u64, ControllerSession>,
    next_session_id: u64,
}

/// The data needed to build a batch's SP1 stdin witness and its controller-validated end state.
#[derive(Debug)]
pub struct FetchedBatch {
    pub current_state: State,
    pub headers: Vec<NewHeader>,
    pub median_hints: Vec<BlockTimestamp>,
    pub expected_state: State,
    pub expected_continuation_digest: [u8; 32],
    pub first_new_height: u32,
    pub end_height: u32,
}

impl Controller {
    pub fn new(store: HeaderStore, genesis_hash: BlockHash) -> Self {
        Self {
            store,
            genesis_hash,
            sessions: std::collections::HashMap::new(),
            next_session_id: 1,
        }
    }

    /// Reconstruct full consensus state at an arbitrary height.
    pub fn state_at(&self, height: u32) -> State {
        self.store.state_at_height(height, self.genesis_hash)
    }

    /// Begin a controller-owned proving session.
    pub fn begin_session(&mut self, start: SessionStart) -> Result<u64, BoxError> {
        let current_state = match start {
            SessionStart::Genesis => self.state_at(0),
            SessionStart::TrustedHeight(height) => self.state_at(height),
            SessionStart::PriorPublicValues(bytes) => {
                let prior =
                    HeaderChainPublicValues::parse(&bytes).map_err(|err| err.to_string())?;
                let claim = match prior {
                    HeaderChainPublicValues::Success { claim, .. } => claim,
                    HeaderChainPublicValues::Failure { failure, .. } => {
                        return Err(format!(
                            "previous proof ended in error: {} at height {}",
                            failure.error_code, failure.failure_height
                        )
                        .into())
                    }
                };
                if claim.genesis_hash != self.genesis_hash {
                    return Err("previous proof genesis mismatch".into());
                }
                let state = self.state_at(claim.height);
                if state.public_claim() != claim {
                    return Err(format!(
                        "db state mismatch at height {}:\n{}",
                        claim.height,
                        format_claim_mismatch(&claim, &state.public_claim())
                    )
                    .into());
                }
                state
            }
        };
        let session_id = self.next_session_id;
        self.next_session_id = self
            .next_session_id
            .checked_add(1)
            .ok_or("controller session id exhausted")?;
        self.sessions.insert(
            session_id,
            ControllerSession {
                current_state,
                pending: None,
            },
        );
        Ok(session_id)
    }

    /// Return the next batch selected from the controller session's state.
    pub fn next_work(
        &mut self,
        session_id: u64,
        num_headers: u32,
    ) -> Result<FetchedBatch, BoxError> {
        let current_state = {
            let session = self.session(session_id)?;
            if session.pending.is_some() {
                return Err("controller session has uncompleted work".into());
            }
            session.current_state.clone()
        };
        let batch = self.fetch_batch(&current_state, num_headers)?;
        self.session_mut(session_id)?.pending = Some(PendingWork {
            expected_state: batch.expected_state.clone(),
            expected_continuation_digest: batch.expected_continuation_digest,
        });
        Ok(batch)
    }

    /// Accept guest execution or proof output and advance the controller session.
    pub fn complete_work(&mut self, session_id: u64, public_values: &[u8]) -> Result<(), BoxError> {
        let public_values =
            HeaderChainPublicValues::parse(public_values).map_err(|err| err.to_string())?;
        let session = self.session_mut(session_id)?;
        let pending = session
            .pending
            .take()
            .ok_or("controller session has no pending work")?;
        match public_values {
            HeaderChainPublicValues::Success {
                claim,
                continuation_digest,
                ..
            } if claim == pending.expected_state.public_claim()
                && continuation_digest == pending.expected_continuation_digest =>
            {
                session.current_state = pending.expected_state;
                Ok(())
            }
            HeaderChainPublicValues::Success { claim, .. } => Err(format!(
                "guest public values mismatch:\n{}",
                format_claim_mismatch(&pending.expected_state.public_claim(), &claim)
            )
            .into()),
            HeaderChainPublicValues::Failure { failure, .. } => Err(format!(
                "guest ended in error {} at height {}",
                failure.error_code, failure.failure_height
            )
            .into()),
        }
    }

    fn session(&self, session_id: u64) -> Result<&ControllerSession, BoxError> {
        self.sessions
            .get(&session_id)
            .ok_or_else(|| format!("unknown controller session {session_id}").into())
    }

    fn session_mut(&mut self, session_id: u64) -> Result<&mut ControllerSession, BoxError> {
        self.sessions
            .get_mut(&session_id)
            .ok_or_else(|| format!("unknown controller session {session_id}").into())
    }

    /// Fetch the next `num_headers` headers after `current_state`.
    pub fn fetch_batch(
        &self,
        current_state: &State,
        num_headers: u32,
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

        let expected_state = self.state_at(end_height);
        let expected_continuation_digest = continuation_digest_from_state(&expected_state);

        Ok(FetchedBatch {
            current_state: current_state.clone(),
            headers: witness.headers,
            median_hints: witness.median_time_past_hints,
            expected_state,
            expected_continuation_digest,
            first_new_height,
            end_height,
        })
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
        let witness = db.load_header_batch_witness(1, 13);

        compute_final_state_with_hints(
            &genesis_state,
            &witness.headers,
            &witness.median_time_past_hints,
        );
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
        let genesis_hash = BlockHash::new(sha256d(&genesis.header.to_bytes()));
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
        let genesis_hash = BlockHash::new(sha256d(&genesis.header.to_bytes()));
        let mut state = genesis_state_from_record(genesis.clone(), genesis_hash);
        let records = db.load_header_records(1, 64);
        let mut previous_ordered_run = genesis.ordered_timestamp_run;

        for (index, record) in records.iter().enumerate() {
            let header = NewHeader::from_header(&record.header);
            let needs_hint = requires_median_hint(record.height, previous_ordered_run);
            state
                .apply_headers(
                    &[header],
                    if needs_hint {
                        core::slice::from_ref(&record.median_time_past)
                    } else {
                        &[]
                    },
                    |new_header, prev_blockhash, compact_target| {
                        let header = (*new_header).into_header(*prev_blockhash, compact_target);
                        BlockHash::new(sha256d(&header.to_bytes()))
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
        let genesis_hash = BlockHash::new(sha256d(&genesis.header.to_bytes()));
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
        let genesis_hash = BlockHash::new(sha256d(&genesis.header.to_bytes()));

        for (name, initial_state, record_start, record_count, expected_height) in cases {
            let initial_state = match initial_state {
                InitialState::Genesis => genesis_state_from_record(genesis.clone(), genesis_hash),
                InitialState::DbHeight(height) => db.state_at_height(height, genesis_hash),
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

    #[test]
    fn fetch_batch_starts_after_genesis() {
        let db = open_db();
        let genesis_hash = BlockHash::new(sha256d(&db.load_header_record(0).header.to_bytes()));
        let controller = Controller::new(db, genesis_hash);
        let current_state = controller.state_at(0);

        let fetched = controller.fetch_batch(&current_state, 1).unwrap();

        assert_eq!(fetched.first_new_height, 1);
        assert_eq!(fetched.end_height, 1);
        assert_eq!(fetched.expected_state.height, 1);
    }

    #[test]
    fn fetch_batch_errors_when_headers_exhausted() {
        let db = open_db();
        let genesis_hash = BlockHash::new(sha256d(&db.load_header_record(0).header.to_bytes()));
        let controller = Controller::new(db, genesis_hash);
        let current_state = controller.state_at(2016);

        let err = controller.fetch_batch(&current_state, 0).unwrap_err();
        assert!(err.to_string().starts_with(NO_HEADERS_REMAINING_PREFIX));
    }

    #[test]
    fn controller_session_advances_only_after_matching_public_values() {
        let db = open_db();
        let genesis = db.load_header_record(0);
        let genesis_hash = BlockHash::new(sha256d(&genesis.header.to_bytes()));
        let mut controller = Controller::new(db, genesis_hash);
        let session = controller.begin_session(SessionStart::Genesis).unwrap();

        let first = controller.next_work(session, 1).unwrap();
        let public_values = zkpow_core::MinimalPublicValues::success(
            &first.expected_state.public_claim(),
            first.expected_continuation_digest,
            zkpow_core::VerifierKeyDigest::default(),
        )
        .to_bytes();
        controller.complete_work(session, &public_values).unwrap();

        assert_eq!(
            controller.next_work(session, 1).unwrap().first_new_height,
            2
        );
    }
}
