use zkpow_core::{BlockHash, BlockTimestamp, CompactTarget, Header, HeaderInput, State};
use zkpow_sha256;
use zkpow_store::HeaderRecord;

/// Materialize the full header from prover fields plus authenticated chain
/// context, then hash it with double SHA-256. Host-side analog of the guest's
/// direct word-injection hashing.
pub fn new_header_hash(
    new_header: &HeaderInput,
    prev_blockhash: &BlockHash,
    compact_target: CompactTarget,
) -> BlockHash {
    let header = Header {
        version: new_header.version,
        prev_blockhash: *prev_blockhash,
        merkle_root: new_header.merkle_root,
        timestamp: new_header.timestamp,
        compact_target,
        nonce: new_header.nonce,
    };
    BlockHash::new(zkpow_sha256::double(&header.to_bytes()))
}

/// Apply a batch locally with its sparse median-time-past hints.
pub fn compute_final_state_with_hints(
    initial_state: &State,
    headers: &[HeaderInput],
    hints: &[BlockTimestamp],
) -> State {
    let mut state = *initial_state;
    state
        .apply_headers(headers, hints)
        .expect("host state transition should succeed");
    state
}

pub fn records_to_new_headers(records: &[HeaderRecord]) -> Vec<HeaderInput> {
    records
        .iter()
        .map(|record| HeaderInput::from_header(&record.header))
        .collect()
}

/// Extract dense database median-time-past values from header records.
pub fn median_time_past_hints_from_records(records: &[HeaderRecord]) -> Vec<BlockTimestamp> {
    records
        .iter()
        .map(|record| record.median_time_past)
        .collect()
}
