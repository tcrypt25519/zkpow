//! Utilities for the zkpow prover script.
//!
//! Host-side mirror of the zkVM program with header-construction architecture.
//! The prover supplies 44-byte NewHeader structs (version, merkle_root, timestamp, nonce).
//! The host constructs full 80-byte headers from state + NewHeader, matching the circuit.

use sha2::{Digest, Sha256};
use sp1_sdk::SP1PublicValues;

pub use zkpow_core::{
    u256, work_from_target, ApplyFailure, BlockHash, BlockTimestamp, ChainWork, CompactTarget,
    Header, HeaderChainPublicValues, Input, InputError, MedianTimePastHints, MinimalPublicValues,
    NewHeader, NewHeaderHintError, NewHeaderHints, NewHeaderHintsRef, ParseError,
    PrivateContinuationState, ProofFailure, PublicChainClaim, PublicValuesDigest,
    PublicValuesParseError, RecursiveProof, State, Target, ValidationErrorCode, ValidationState,
    VerifierKeyDigest, GENESIS_TARGET, MINIMAL_PV_SIZE, NEW_HEADER_SIZE,
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

/// Load raw 80-byte concatenated headers from the SQLite database.
pub fn load_headers_from_db(db_path: &str, start_height: u64, count: u64) -> Vec<u8> {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open SQLite database");

    let mut stmt = conn
        .prepare(
            "SELECT version, prev, merkle_root, timestamp, n_bits, nonce FROM headers WHERE height >= ?1 AND height < ?2 ORDER BY height ASC",
        )
        .unwrap_or_else(|err| {
            panic!("failed to prepare SQL statement for db {}: {}", db_path, err)
        });

    let end_height = start_height + count;
    let rows = stmt
        .query_map(rusqlite::params![start_height, end_height], |row| {
            let version: i64 = row.get(0)?;
            let prev: [u8; 32] = row.get(1)?;
            let merkle_root: [u8; 32] = row.get(2)?;
            let timestamp: i64 = row.get(3)?;
            let nbits: i64 = row.get(4)?;
            let nonce: i64 = row.get(5)?;

            let header = Header {
                version: version as u32,
                prev_blockhash: BlockHash::from_raw(prev),
                merkle_root,
                timestamp: BlockTimestamp::new(timestamp as u32),
                compact_target: CompactTarget::from_consensus(nbits as u32),
                nonce: nonce as u32,
            };
            Ok(header.to_bytes())
        })
        .expect("failed to execute query");

    let mut all_headers = Vec::with_capacity((count * 80) as usize);
    let mut loaded = 0u64;

    for row_result in rows {
        let header_bytes = row_result.expect("failed to read header from database");
        all_headers.extend_from_slice(&header_bytes);
        loaded += 1;
    }

    assert_eq!(
        loaded, count,
        "Expected to load {} headers, but only loaded {} from database",
        count, loaded,
    );

    all_headers
}

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
            let prev: [u8; 32] = row.get(2)?;
            let merkle_root: [u8; 32] = row.get(3)?;
            let timestamp: i64 = row.get(4)?;
            let nbits: i64 = row.get(5)?;
            let nonce: i64 = row.get(6)?;
            let chainwork: [u8; 32] = row.get(7)?;
            let median_time_past: i64 = row.get(8)?;

            let header = Header {
                version: version as u32,
                prev_blockhash: BlockHash::from_raw(prev),
                merkle_root,
                timestamp: BlockTimestamp::new(timestamp as u32),
                compact_target: CompactTarget::from_consensus(nbits as u32),
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

    let records: Vec<HeaderRecord> = rows
        .map(|row_result| row_result.expect("failed to read header record from database"))
        .collect();

    assert_eq!(
        records.len() as u64,
        count,
        "Expected to load {} headers, but only loaded {} from database",
        count,
        records.len(),
    );

    records
}

pub fn chain_work_from_db_bytes(bytes: &[u8]) -> ChainWork {
    let raw: [u8; 32] = bytes.try_into().expect("chainwork must be 32 bytes");
    let mut little_endian = raw;
    little_endian.reverse();
    ChainWork::from_le_bytes(little_endian)
}

/// Convert raw 80-byte headers (from DB) to typed [`NewHeader`] values.
pub fn raw_headers_to_new_headers(raw_headers: &[u8]) -> Vec<NewHeader> {
    assert_eq!(
        raw_headers.len() % 80,
        0,
        "raw_headers must be a multiple of 80 bytes"
    );
    let count = raw_headers.len() / 80;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let offset = i * 80;
        let raw: [u8; 80] = raw_headers[offset..offset + 80].try_into().unwrap();
        let header = Header::parse(&raw).expect("raw Bitcoin header should parse");
        out.push(NewHeader::from_header(&header));
    }
    out
}

/// Load and parse a single header from the SQLite database.
pub fn load_header_from_db(db_path: &str, height: u64) -> Header {
    let raw_headers = load_headers_from_db(db_path, height, 1);
    let raw_header: [u8; 80] = raw_headers
        .as_slice()
        .try_into()
        .expect("exactly one raw header should be returned");
    Header::parse(&raw_header).expect("raw Bitcoin header should parse")
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
    BlockHash::from_raw(sha256d(&header.to_bytes()))
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

// ============================================================================
// State Computation (host-side simulation of zkVM logic)
// ============================================================================

/// Build the initial genesis state from a host-selected genesis header.
pub fn genesis_state(genesis_header: Header, genesis_hash: BlockHash) -> State {
    let block_hash = hash_header(&genesis_header);
    assert_eq!(
        block_hash, genesis_hash,
        "configured genesis hash must match the supplied genesis header",
    );
    let genesis_work = work_from_target(GENESIS_TARGET);

    State {
        header: genesis_header,
        block_hash,
        genesis_hash,
        next_nbits: genesis_header.compact_target,
        height: 0,
        chain_work: genesis_work,
        next_work: genesis_work,
        next_target: GENESIS_TARGET,
        epoch_start_timestamp: genesis_header.timestamp,
        timestamps: [BlockTimestamp::default(); zkpow_core::WINDOW_SIZE],
        _environment: core::marker::PhantomData,
    }
}

pub fn genesis_state_from_record(genesis: HeaderRecord, genesis_hash: BlockHash) -> State {
    let mut state = genesis_state(genesis.header, genesis_hash);
    state.height = genesis.height as u32;
    state.chain_work = genesis.chain_work;
    state
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
    hints: &MedianTimePastHints,
) -> State {
    let mut state = initial_state.clone();
    state
        .apply_headers(headers, &hints.medians, hash_header)
        .expect("host state transition should succeed");
    state
}

pub fn records_to_new_headers(records: &[HeaderRecord]) -> Vec<NewHeader> {
    records
        .iter()
        .map(|record| NewHeader::from_header(&record.header))
        .collect()
}

/// Build the median-time-past witness hints from database header records.
pub fn median_time_past_hints_from_records(records: &[HeaderRecord]) -> MedianTimePastHints {
    MedianTimePastHints::new(
        records
            .iter()
            .map(|record| record.median_time_past)
            .collect(),
    )
}

/// Build the median-time-past witness hints by sorting on the host.
///
/// This is a host-only fallback for tests/local simulation. Production proof
/// generation should prefer [`median_time_past_hints_from_records`] so the host
/// uses the database-provided MTP column.
pub fn median_time_past_hints_for_headers(
    initial_state: &State,
    headers: &[NewHeader],
) -> MedianTimePastHints {
    let mut state = initial_state.clone();
    let mut medians = Vec::with_capacity(headers.len());

    for header in headers {
        medians.push(state.median_time_past());
        let timestamp_slot = (state.height as usize) % zkpow_core::WINDOW_SIZE;
        state.timestamps[timestamp_slot] = header.timestamp;
        state.height += 1;
    }

    MedianTimePastHints::new(medians)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zkpow_core::{BlockTimestamp, CompactTarget, GENESIS_NBITS, WINDOW_SIZE};

    fn make_pcs() -> PrivateContinuationState {
        PrivateContinuationState {
            next_nbits: CompactTarget::from_consensus(GENESIS_NBITS),
            next_work: zkpow_core::ChainWork::from_limbs([1, 2, 3, 4]),
            next_target: zkpow_core::GENESIS_TARGET,
            epoch_start_timestamp: BlockTimestamp::from_consensus(500),
            timestamps: [BlockTimestamp::from_consensus(10); WINDOW_SIZE],
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
}
