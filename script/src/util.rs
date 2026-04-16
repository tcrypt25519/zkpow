//! Utilities for the Bitcoin header chain prover script.
//!
//! Host-side mirror of the zkVM program with header-construction architecture.
//! The prover supplies 44-byte NewHeader structs (version, merkle_root, timestamp, nonce).
//! The host constructs full 80-byte headers from state + NewHeader, matching the circuit.

use bitcoin_header_chain_core::{
    add_timestamp_window, bits_to_target, retarget_target, target_exceeds, target_to_bits,
    u256_add, work_from_bits, GENESIS_NBITS, WINDOW_SIZE,
};
use sha2::{Digest, Sha256};
use sp1_sdk::SP1PublicValues;

pub use bitcoin_header_chain_core::{
    BlockHash, BlockTimestamp, ChainWork, CompactTarget, Header, HeaderChainPublicValues, Input,
    InputError, NewHeader, ParseError, ProofFailure, PublicValuesDigest, PublicValuesParseError,
    RecursiveProof, State, ValidationErrorCode, VerifierKeyDigest, NEW_HEADER_SIZE, STATE_SIZE,
};

// ============================================================================
// Database & I/O
// ============================================================================

/// Load raw 80-byte concatenated headers from the SQLite database.
pub fn load_headers_from_db(db_path: &str, start_height: u64, count: u64) -> Vec<u8> {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open SQLite database");

    let mut stmt = conn
        .prepare(
            "SELECT raw_header FROM headers WHERE height >= ?1 AND height < ?2 ORDER BY height ASC",
        )
        .expect("failed to prepare SQL statement");

    let end_height = start_height + count;
    let rows = stmt
        .query_map(rusqlite::params![start_height, end_height], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .expect("failed to execute query");

    let mut all_headers = Vec::with_capacity((count * 80) as usize);
    let mut loaded = 0u64;

    for row_result in rows {
        let header_bytes: Vec<u8> = row_result.expect("failed to read raw_header from database");
        assert_eq!(
            header_bytes.len(),
            80,
            "Expected 80-byte header at height {}, got {} bytes",
            start_height + loaded,
            header_bytes.len(),
        );
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

// ============================================================================
// SHA-256 (host-side)
// ============================================================================

/// Compute double SHA-256 of the given data (host-side).
pub fn double_sha256_host(data: &[u8]) -> [u8; 32] {
    let inner = Sha256::digest(data);
    let outer = Sha256::digest(inner);
    outer.into()
}

/// Compute SHA-256 digest (host-side).
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
    let block_hash = BlockHash::from_raw(double_sha256_host(&genesis_header.to_bytes()));
    assert_eq!(
        block_hash, genesis_hash,
        "configured genesis hash must match the supplied genesis header",
    );

    let mut timestamps = [BlockTimestamp::default(); WINDOW_SIZE];
    timestamps[0] = genesis_header.timestamp;

    State {
        header: genesis_header,
        block_hash,
        genesis_hash,
        next_nbits: genesis_header.nbits,
        height: 0,
        chain_work: work_from_bits(genesis_header.nbits),
        epoch_start_timestamp: genesis_header.timestamp,
        timestamps,
        sorted_nibbles: 0,
    }
}

/// Simulate the zkVM program locally to compute the expected [`State`] after
/// validating a batch of headers.
pub fn compute_next_state(initial_state: &State, headers: &[NewHeader]) -> State {
    let mut state = initial_state.clone();

    for new_header in headers.iter().copied() {
        let validated_height = state.height + 1;
        let required_nbits = state.next_nbits;

        let header = new_header.into_header(state.block_hash, required_nbits);
        let block_hash = BlockHash::from_raw(double_sha256_host(&header.to_bytes()));

        let timestamp_count = (state.height as usize).min(WINDOW_SIZE);
        let slot = (state.height as usize) % WINDOW_SIZE;
        state.sorted_nibbles = add_timestamp_window(
            &mut state.timestamps,
            timestamp_count,
            state.sorted_nibbles,
            new_header.timestamp,
            slot,
        );

        if validated_height % 2016 == 0 {
            state.epoch_start_timestamp = new_header.timestamp;
        }

        if (validated_height + 1) % 2016 == 0 {
            let actual_timespan = new_header
                .timestamp
                .wrapping_sub(state.epoch_start_timestamp);
            let expected_timespan: u32 = 2016 * 600;
            let clamped = actual_timespan
                .max(expected_timespan / 4)
                .min(expected_timespan * 4);
            let pow_limit = bits_to_target(CompactTarget::from_consensus(GENESIS_NBITS));
            let mut new_target = retarget_target(state.next_target(), clamped, expected_timespan);
            if target_exceeds(new_target, pow_limit) {
                new_target = pow_limit;
            }
            state.next_nbits = target_to_bits(new_target);
        }

        state.chain_work = u256_add(state.chain_work, work_from_bits(required_nbits));
        state.header = header;
        state.block_hash = block_hash;
        state.height = validated_height;
    }

    state
}
