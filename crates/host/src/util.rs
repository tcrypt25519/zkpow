//! Utilities for the zkpow prover script.
//!
//! Host-side mirror of the zkVM program with header-construction architecture.
//! The prover supplies 44-byte NewHeader structs (version, merkle_root, timestamp, nonce).
//! The host constructs full 80-byte headers from state + NewHeader, matching the circuit.

use sha2::{Digest, Sha256};
use sp1_sdk::SP1PublicValues;

pub use zkpow_core::{
    parse_median_hints, parse_new_headers, serialize_median_hints, serialize_new_headers,
    target_from_bits, u256, work_from_target, ApplyFailure, BlockHash, BlockTimestamp, ChainWork,
    CompactTarget, Header, HeaderChainPublicValues, Input, InputError, MinimalPublicValues,
    NewHeader, NewHeaderHintError, ParseError, PrivateContinuationState, ProofFailure,
    PublicChainClaim, PublicValuesDigest, PublicValuesParseError, RecursiveProof, State, Target,
    ValidationErrorCode, VerifierKeyDigest, GENESIS_TARGET, MINIMAL_PV_SIZE, NEW_HEADER_SIZE,
    PRIVATE_CONTINUATION_STATE_SIZE, PUBLIC_CHAIN_CLAIM_SIZE, STATE_SIZE,
};

#[derive(Debug, Clone)]
pub struct HeaderRecord {
    pub height: u64,
    pub header: Header,
    pub chain_work: ChainWork,
    pub median_time_past: BlockTimestamp,
}

// ============================================================================
// Database & I/O
// ============================================================================

pub fn load_header_records_from_db(
    db_path: &str,
    start_height: u64,
    count: u64,
) -> Vec<HeaderRecord> {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open SQLite database");

    let mut stmt = conn
        .prepare(
            "SELECT height, version, prev, merkle_root, timestamp, n_bits, nonce, chainwork, median_time_past FROM headers WHERE height >= ?1 AND height < ?2 ORDER BY height ASC",
        )
        .unwrap_or_else(|err| {
            panic!("failed to prepare SQL statement for db {}: {}", db_path, err)
        });

    let end_height = start_height + count;
    let rows = stmt
        .query_map(rusqlite::params![start_height, end_height], |row| {
            let height: i64 = row.get(0)?;
            let version: i64 = row.get(1)?;
            let prev: Vec<u8> = row.get(2)?;
            let merkle_root: Vec<u8> = row.get(3)?;
            let timestamp: i64 = row.get(4)?;
            let nbits: i64 = row.get(5)?;
            let nonce: i64 = row.get(6)?;
            let chainwork: Vec<u8> = row.get(7)?;
            let median_time_past: i64 = row.get(8)?;

            let header = Header {
                version: version as u32,
                prev_blockhash: BlockHash::new(prev.try_into().expect("prev must be 32 bytes")),
                merkle_root: merkle_root
                    .try_into()
                    .expect("merkle_root must be 32 bytes"),
                timestamp: BlockTimestamp::new(timestamp as u32),
                compact_target: CompactTarget::new(nbits as u32),
                nonce: nonce as u32,
            };

            Ok(HeaderRecord {
                height: height as u64,
                header,
                chain_work: chain_work_from_db_bytes(&chainwork),
                median_time_past: BlockTimestamp::new(median_time_past as u32),
            })
        })
        .expect("failed to execute query");

    let mut records = Vec::with_capacity(count as usize);
    for row_result in rows {
        records.push(row_result.expect("failed to read header record from database"));
    }

    records
}

fn chain_work_from_db_bytes(bytes: &[u8]) -> ChainWork {
    let raw: [u8; 32] = bytes.try_into().expect("chainwork must be 32 bytes");
    let mut little_endian = raw;
    little_endian.reverse();
    ChainWork::from_le_bytes(little_endian)
}

pub fn load_header_record_from_db(db_path: &str, height: u64) -> HeaderRecord {
    load_header_records_from_db(db_path, height, 1)
        .into_iter()
        .next()
        .expect("exactly one header record should be returned")
}

// ============================================================================
// SHA-256 (host-side)
// ============================================================================

/// Compute SHA256d of the given data.
pub fn sha256d(data: &[u8]) -> [u8; 32] {
    Sha256::digest(Sha256::digest(data)).into()
}

/// Hash a full Bitcoin header with SHA256d.
#[must_use]
pub fn hash_header(header: &Header) -> BlockHash {
    BlockHash::new(sha256d(&header.to_bytes()))
}

/// Compute SHA-256 digest of public values.
pub fn compute_pv_digest(committed_bytes: &[u8]) -> [u8; 32] {
    let digest = SP1PublicValues::from(committed_bytes).hash();
    digest
        .try_into()
        .expect("SP1 public values hash must be 32 bytes")
}

/// Compute the continuation digest: SHA-256 of the serialized private continuation state.
pub fn continuation_digest(private: &PrivateContinuationState) -> [u8; 32] {
    Sha256::digest(private.to_bytes()).into()
}

/// Compute the continuation digest directly from a [`State`].
pub fn continuation_digest_from_state(state: &State) -> [u8; 32] {
    continuation_digest(&PrivateContinuationState::from_state(state))
}

// ============================================================================
// State Computation (host-side simulation of zkVM logic)
// ============================================================================

pub fn genesis_state_from_record(genesis: HeaderRecord, genesis_hash: BlockHash) -> State {
    let block_hash = hash_header(&genesis.header);
    assert_eq!(
        block_hash, genesis_hash,
        "configured genesis hash must match the supplied genesis header",
    );
    let genesis_work = work_from_target(GENESIS_TARGET);

    let mut timestamps = [BlockTimestamp::default(); zkpow_core::WINDOW_SIZE];
    timestamps[0] = genesis.header.timestamp;

    State {
        header: genesis.header,
        block_hash,
        genesis_hash,
        current_nbits: genesis.header.compact_target,
        height: genesis.height as u32,
        chain_work: genesis.chain_work,
        current_work: genesis_work,
        current_target: GENESIS_TARGET,
        epoch_start_timestamp: genesis.header.timestamp,
        timestamps,
    }
}

pub fn state_from_db_at_height(db_path: &str, height: u32, genesis_hash: BlockHash) -> State {
    if height == 0 {
        let genesis = load_header_record_from_db(db_path, 0);
        return genesis_state_from_record(genesis, genesis_hash);
    }

    let current = load_header_record_from_db(db_path, height as u64);
    let epoch_start_height = (height / zkpow_core::EPOCH_LENGTH) * zkpow_core::EPOCH_LENGTH;
    let epoch_start_record = load_header_record_from_db(db_path, epoch_start_height as u64);
    let window_count = (height as usize + 1).min(zkpow_core::WINDOW_SIZE) as u64;
    let window_start = height as u64 + 1 - window_count;
    let window_records = load_header_records_from_db(db_path, window_start, window_count);

    let mut timestamps = [BlockTimestamp::default(); zkpow_core::WINDOW_SIZE];
    for record in window_records {
        timestamps[record.height as usize % zkpow_core::WINDOW_SIZE] = record.header.timestamp;
    }

    let current_target = target_from_bits(current.header.compact_target);
    let canonical_chain_work = current.chain_work;

    State {
        header: current.header,
        block_hash: hash_header(&current.header),
        genesis_hash,
        current_nbits: current.header.compact_target,
        height,
        chain_work: canonical_chain_work,
        current_work: work_from_target(current_target),
        current_target,
        epoch_start_timestamp: epoch_start_record.header.timestamp,
        timestamps,
    }
}

/// Simulate the zkVM program locally to compute the expected [`State`] after
/// validating a batch of headers.
pub fn compute_final_state(initial_state: &State, headers: &[NewHeader]) -> State {
    let hints = median_time_past_hints_for_headers(initial_state, headers);
    compute_final_state_with_hints(initial_state, headers, &hints)
}

/// Simulate the zkVM program locally using the supplied median-time-past hints.
pub fn compute_final_state_with_hints(
    initial_state: &State,
    headers: &[NewHeader],
    hints: &[BlockTimestamp],
) -> State {
    let mut state = initial_state.clone();
    state
        .apply_headers(headers, hints, hash_header)
        .expect("host state transition should succeed");
    state
}

/// Simulate the zkVM program locally while retaining each intermediate state.
///
/// Returns `(final_state, history)` where `history[i]` is the state after
/// applying `headers[i]`.
pub fn compute_final_state_with_history(
    initial_state: &State,
    headers: &[NewHeader],
    hints: &[BlockTimestamp],
) -> (State, Vec<State>) {
    assert_eq!(
        headers.len(),
        hints.len(),
        "median hint count must match header count"
    );

    let mut state = initial_state.clone();
    let mut history = Vec::with_capacity(headers.len());

    for (index, (header, median)) in headers
        .iter()
        .copied()
        .zip(hints.iter().copied())
        .enumerate()
    {
        state
            .apply_headers(&[header], &[median], hash_header)
            .unwrap_or_else(|err| {
                panic!("host state transition should succeed at header index {index}: {err:?}")
            });
        history.push(state.clone());
    }

    (state, history)
}

pub fn records_to_new_headers(records: &[HeaderRecord]) -> Vec<NewHeader> {
    records
        .iter()
        .map(|record| NewHeader::from_header(&record.header))
        .collect()
}

/// Build the median-time-past witness hints from database header records.
pub fn median_time_past_hints_from_records(records: &[HeaderRecord]) -> Vec<BlockTimestamp> {
    records
        .iter()
        .map(|record| record.median_time_past)
        .collect()
}

fn median_time_past_for_state(state: &State) -> BlockTimestamp {
    let count = state.timestamp_count();
    let mut sorted = state.timestamps;
    if count >= zkpow_core::WINDOW_SIZE {
        sorted.sort_unstable();
        return sorted[zkpow_core::WINDOW_SIZE / 2];
    }

    sorted[..count].sort_unstable();
    sorted[count / 2]
}

/// Build the median-time-past witness hints by sorting on the host.
///
/// This is a host-only fallback for tests/local simulation. Production proof
/// generation should prefer [`median_time_past_hints_from_records`] so the host
/// uses the database-provided MTP column.
pub fn median_time_past_hints_for_headers(
    initial_state: &State,
    headers: &[NewHeader],
) -> Vec<BlockTimestamp> {
    let mut state = initial_state.clone();
    let mut medians = Vec::with_capacity(headers.len());

    for header in headers {
        medians.push(median_time_past_for_state(&state));
        let timestamp_slot = (state.height as usize + 1) % zkpow_core::WINDOW_SIZE;
        state.timestamps[timestamp_slot] = header.timestamp;
        state.height += 1;
    }

    medians
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::db_path;
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
            current_work: work_from_target(GENESIS_TARGET),
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
    fn compute_final_state_with_history_matches_final_state() {
        let genesis = load_header_record_from_db(db_path(), 0);
        let genesis_hash = hash_header(&genesis.header);
        let genesis_state = genesis_state_from_record(genesis, genesis_hash);
        let records = load_header_records_from_db(db_path(), 1, 128);
        let headers = records_to_new_headers(&records);
        let hints = median_time_past_hints_from_records(&records);

        let expected_final = compute_final_state_with_hints(&genesis_state, &headers, &hints);
        let (history_final, history) =
            compute_final_state_with_history(&genesis_state, &headers, &hints);

        assert_eq!(history.len(), headers.len());
        assert_eq!(history_final.public_claim(), expected_final.public_claim());
        assert_eq!(
            history
                .last()
                .expect("history should not be empty")
                .public_claim(),
            expected_final.public_claim()
        );
    }

    #[test]
    fn compute_final_state_with_history_entries_match_db_claims() {
        let genesis = load_header_record_from_db(db_path(), 0);
        let genesis_hash = hash_header(&genesis.header);
        let genesis_state = genesis_state_from_record(genesis, genesis_hash);
        let records = load_header_records_from_db(db_path(), 1, 64);
        let headers = records_to_new_headers(&records);
        let hints = median_time_past_hints_from_records(&records);

        let (_final_state, history) =
            compute_final_state_with_history(&genesis_state, &headers, &hints);

        for (index, state) in history.iter().enumerate() {
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
