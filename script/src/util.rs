//! Utilities for the Bitcoin header chain prover script.
//!
//! Host-side mirror of the zkVM program with header-construction architecture.
//! The prover supplies 44-byte NewHeader structs (version, merkle_root, timestamp, nonce).
//! The host constructs full 80-byte headers from state + NewHeader, matching the circuit.

use bitcoin_header_chain_core::{
    bits_to_target, retarget_target, target_exceeds, target_to_bits, u256_add, work_from_bits,
    GENESIS_NBITS, MAINNET_GENESIS_HASH_RAW, NIBBLE_BITS, NIBBLE_MASK, WINDOW_SIZE,
};
use sha2::{Digest, Sha256};
use sp1_sdk::SP1PublicValues;

pub use bitcoin_header_chain_core::{
    HeaderChainPublicValues, NewHeader, ProofFailure, PublicValuesParseError, State,
    ValidationErrorCode, NEW_HEADER_SIZE, STATE_SIZE,
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

/// Convert raw 80-byte headers (from DB) to 44-byte NewHeader format (for zkVM input).
pub fn raw_headers_to_new_headers(raw_headers: &[u8]) -> Vec<u8> {
    assert_eq!(
        raw_headers.len() % 80,
        0,
        "raw_headers must be a multiple of 80 bytes"
    );
    let count = raw_headers.len() / 80;
    let mut out = Vec::with_capacity(count * NEW_HEADER_SIZE);
    for i in 0..count {
        let offset = i * 80;
        let raw: [u8; 80] = raw_headers[offset..offset + 80].try_into().unwrap();
        let nh = NewHeader::from_raw_header(&raw);
        out.extend_from_slice(&nh.version.to_le_bytes());
        out.extend_from_slice(&nh.merkle_root);
        out.extend_from_slice(&nh.timestamp.to_le_bytes());
        out.extend_from_slice(&nh.nonce.to_le_bytes());
    }
    out
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
// Header Construction (host-side, identical to program)
// ============================================================================

/// Build the full 80-byte Bitcoin block header from authenticated state + NewHeader.
fn construct_header(state: &State, new_header: &NewHeader) -> [u8; 80] {
    let mut header = [0u8; 80];
    header[0..4].copy_from_slice(&new_header.version.to_le_bytes());
    header[4..36].copy_from_slice(&state.prev_blockhash);
    header[36..68].copy_from_slice(&new_header.merkle_root);
    header[68..72].copy_from_slice(&new_header.timestamp.to_le_bytes());
    header[72..76].copy_from_slice(&state.nbits.to_le_bytes());
    header[76..80].copy_from_slice(&new_header.nonce.to_le_bytes());
    header
}

// ============================================================================
// Median Timestamp Window (host-side, identical to program)
// ============================================================================

#[inline]
fn get_nibble(packed: u64, pos: usize) -> u8 {
    ((packed >> (pos * NIBBLE_BITS)) & NIBBLE_MASK) as u8
}

fn find_insert_position(
    timestamps: &[u32; WINDOW_SIZE],
    packed: u64,
    count: usize,
    ts: u32,
) -> usize {
    for i in 0..count {
        let idx = get_nibble(packed, i) as usize;
        if ts < timestamps[idx] {
            return i;
        }
    }
    count
}

fn find_index_position(packed: u64, count: usize, target: usize) -> usize {
    for i in 0..count {
        if get_nibble(packed, i) as usize == target {
            return i;
        }
    }
    count
}

fn remove_nibble(packed: u64, pos: usize, _count: usize) -> u64 {
    let lower_mask = (1u64 << (pos * NIBBLE_BITS)) - 1;
    let lower = packed & lower_mask;
    let upper = (packed >> ((pos + 1) * NIBBLE_BITS)) << (pos * NIBBLE_BITS);
    lower | upper
}

fn insert_nibble(packed: u64, pos: usize, val: u8, count: usize) -> u64 {
    let lower_mask = (1u64 << (pos * NIBBLE_BITS)) - 1;
    let lower = packed & lower_mask;
    let upper = (packed & !lower_mask) << NIBBLE_BITS;
    let new_packed = lower | ((val as u64) << (pos * NIBBLE_BITS)) | upper;
    let new_mask = (1u64 << ((count + 1) * NIBBLE_BITS)) - 1;
    new_packed & new_mask
}

fn add_timestamp_window(
    timestamps: &mut [u32; WINDOW_SIZE],
    prev_count: usize,
    packed: u64,
    ts: u32,
    slot: usize,
) -> u64 {
    if prev_count < WINDOW_SIZE {
        timestamps[slot] = ts;
        let pos = find_insert_position(timestamps, packed, prev_count, ts);
        insert_nibble(packed, pos, slot as u8, prev_count)
    } else {
        let pos_old = find_index_position(packed, WINDOW_SIZE, slot);
        let without = remove_nibble(packed, pos_old, WINDOW_SIZE);
        let pos_new = find_insert_position(timestamps, without, WINDOW_SIZE - 1, ts);
        timestamps[slot] = ts;
        insert_nibble(without, pos_new, slot as u8, WINDOW_SIZE - 1)
    }
}

// ============================================================================
// State Computation (host-side simulation of zkVM logic)
// ============================================================================

/// Simulate the zkVM program locally to compute the expected State after
/// validating a batch of headers, optionally extending from a previous state.
///
/// `headers_bytes` must be in NewHeader format (44 bytes per header).
pub fn compute_expected_state(
    _start_height: u32,
    num_headers: u32,
    new_headers_bytes: &[u8],
    prev_state: Option<&State>,
) -> State {
    let mut state = prev_state.cloned().unwrap_or(State {
        genesis_hash: [0u8; 32],
        prev_blockhash: [0u8; 32],
        nbits: GENESIS_NBITS,
        target: bits_to_target(GENESIS_NBITS),
        height: 0,
        chain_work: [0u64; 4],
        epoch_start_timestamp: 0,
        timestamps: [0u32; WINDOW_SIZE],
        sorted_nibbles: 0,
    });

    for i in 0..num_headers {
        let offset = (i as usize) * NEW_HEADER_SIZE;
        let new_header = NewHeader::from_bytes(new_headers_bytes, offset);
        let validated_count = state.height + 1;

        // Construct the full header (matching the circuit)
        let header = construct_header(&state, &new_header);
        let block_hash = double_sha256_host(&header);

        if state.height == 0 {
            assert_eq!(
                block_hash, MAINNET_GENESIS_HASH_RAW,
                "constructed genesis header does not match mainnet genesis hash",
            );
            // Genesis block
            state.genesis_hash = block_hash;
            state.chain_work = work_from_bits(state.nbits);
            state.epoch_start_timestamp = new_header.timestamp;
            state.timestamps[0] = new_header.timestamp;
            state.sorted_nibbles = 0;
        } else {
            // Median timestamp count (before adding this one)
            let timestamp_count = (state.height as usize).min(WINDOW_SIZE);

            // The first block of a new epoch becomes the reference point for the
            // next retarget window.
            if state.height % 2016 == 0 {
                state.epoch_start_timestamp = new_header.timestamp;
            }

            // `state.height` is the next chain height to validate. Once this block
            // is accepted, `validated_count` becomes the number of blocks in the
            // authenticated prefix. Retargeting runs at that point so the new
            // difficulty applies to the next header we construct.
            if validated_count % 2016 == 0 {
                let actual_timespan = new_header
                    .timestamp
                    .wrapping_sub(state.epoch_start_timestamp);
                let expected_timespan: u32 = 2016 * 600;
                let clamped = actual_timespan
                    .max(expected_timespan / 4)
                    .min(expected_timespan * 4);
                let pow_limit = bits_to_target(GENESIS_NBITS);
                let mut new_target = retarget_target(&state.target, clamped, expected_timespan);
                if target_exceeds(&new_target, &pow_limit) {
                    new_target = pow_limit;
                }
                state.nbits = target_to_bits(&new_target);
                state.target = new_target;
            }

            // Add timestamp to circular buffer
            let slot = (state.height as usize) % WINDOW_SIZE;
            state.sorted_nibbles = add_timestamp_window(
                &mut state.timestamps,
                timestamp_count,
                state.sorted_nibbles,
                new_header.timestamp,
                slot,
            );
            state.chain_work = u256_add(state.chain_work, work_from_bits(state.nbits));
        }

        state.prev_blockhash = block_hash;
        state.height = validated_count;
    }

    state
}
