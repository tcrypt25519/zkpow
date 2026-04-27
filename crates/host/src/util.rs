//! Utilities for the Bitcoin header chain prover script.
//!
//! Host-side mirror of the zkVM program with header-construction architecture.
//! The prover supplies 44-byte NewHeader structs (version, merkle_root, timestamp, nonce).
//! The host constructs full 80-byte headers from state + NewHeader, matching the circuit.

use sha2::{Digest, Sha256};
use sp1_sdk::SP1PublicValues;

pub use zkpow_core::{
    BlockHash, BlockTimestamp, ChainWork, CompactTarget, Header, HeaderChainPublicValues, Input,
    InputError, MedianTimePastHints, NewHeader, ParseError, ProofFailure, PublicValuesDigest,
    PublicValuesParseError, RecursiveProof, State, Target, ValidationErrorCode, VerifierKeyDigest,
    NEW_HEADER_SIZE, STATE_SIZE,
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
            let prev: Vec<u8> = row.get(1)?;
            let merkle_root: Vec<u8> = row.get(2)?;
            let timestamp: i64 = row.get(3)?;
            let nbits: i64 = row.get(4)?;
            let nonce: i64 = row.get(5)?;

            let header = Header {
                version: version as u32,
                prev_blockhash: BlockHash::from_raw(
                    prev.try_into().expect("prev must be 32 bytes"),
                ),
                merkle_root: merkle_root
                    .try_into()
                    .expect("merkle_root must be 32 bytes"),
                timestamp: BlockTimestamp::from_consensus(timestamp as u32),
                nbits: zkpow_core::CompactTarget::from_consensus(nbits as u32),
                nonce: nonce as u32,
            };
            Ok(header.to_bytes())
        })
        .expect("failed to execute query");

    let mut all_headers = Vec::with_capacity((count * 80) as usize);
    let mut loaded = 0u64;

    for row_result in rows {
        let header_bytes: [u8; 80] = row_result.expect("failed to read header from database");
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
            let prev: Vec<u8> = row.get(2)?;
            let merkle_root: Vec<u8> = row.get(3)?;
            let timestamp: i64 = row.get(4)?;
            let nbits: i64 = row.get(5)?;
            let nonce: i64 = row.get(6)?;
            let chainwork: Vec<u8> = row.get(7)?;
            let median_time_past: i64 = row.get(8)?;

            let header = Header {
                version: version as u32,
                prev_blockhash: BlockHash::from_raw(
                    prev.try_into().expect("prev must be 32 bytes"),
                ),
                merkle_root: merkle_root
                    .try_into()
                    .expect("merkle_root must be 32 bytes"),
                timestamp: BlockTimestamp::from_consensus(timestamp as u32),
                nbits: CompactTarget::from_consensus(nbits as u32),
                nonce: nonce as u32,
            };

            Ok(HeaderRecord {
                height: height as u64,
                header,
                chain_work: chain_work_from_db_bytes(&chainwork),
                median_time_past: BlockTimestamp::from_consensus(median_time_past as u32),
            })
        })
        .expect("failed to execute query");

    let mut records = Vec::with_capacity(count as usize);
    for row_result in rows {
        records.push(row_result.expect("failed to read header record from database"));
    }

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
    let mut limbs = [0u64; 4];
    for (idx, limb) in limbs.iter_mut().enumerate() {
        let start = idx * 8;
        *limb = u64::from_le_bytes(little_endian[start..start + 8].try_into().unwrap());
    }
    ChainWork::from_limbs(limbs)
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
        out.push(NewHeader::from_raw_header(&raw));
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

    State {
        header: genesis_header,
        block_hash,
        genesis_hash,
        next_nbits: genesis_header.nbits,
        height: 0,
        chain_work: Target::from(genesis_header.nbits).work(),
        next_work: Target::from(genesis_header.nbits).work(),
        epoch_start_timestamp: genesis_header.timestamp,
        timestamps: [BlockTimestamp::default(); zkpow_core::WINDOW_SIZE],
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
    initial_state
        .apply_headers(headers, &hints.medians, hash_header)
        .expect("host state transition should succeed")
}

pub fn records_to_new_headers(records: &[HeaderRecord]) -> Vec<NewHeader> {
    records
        .iter()
        .map(|record| NewHeader::from_header(&record.header))
        .collect()
}

/// Build the median-time-past witness hints by mirroring the validator state.
pub fn median_time_past_hints_for_headers(
    initial_state: &State,
    headers: &[NewHeader],
) -> MedianTimePastHints {
    let mut state = initial_state.clone();
    let mut medians = Vec::with_capacity(headers.len());

    for header in headers {
        medians.push(state.median_time_past().unwrap_or_default());
        let timestamp_slot = (state.height as usize) % zkpow_core::WINDOW_SIZE;
        state.timestamps[timestamp_slot] = header.timestamp;
        state.height += 1;
    }

    MedianTimePastHints::new(medians)
}
