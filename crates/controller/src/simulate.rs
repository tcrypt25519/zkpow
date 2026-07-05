use zkpow_core::{sha256d, BlockHash, BlockTimestamp, NewHeader, State};
use zkpow_store::HeaderRecord;

/// Simulate the zkVM program locally using the supplied median-time-past hints.
pub fn compute_final_state_with_hints(
    initial_state: &State,
    headers: &[NewHeader],
    hints: &[BlockTimestamp],
) -> State {
    let mut state = initial_state.clone();
    state
        .apply_headers(
            headers,
            hints,
            |new_header, prev_blockhash, compact_target| {
                let header = (*new_header).into_header(*prev_blockhash, compact_target);
                BlockHash::new(sha256d(&header.to_bytes()))
            },
        )
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
pub fn median_time_past_hints_from_records(records: &[HeaderRecord]) -> Vec<BlockTimestamp> {
    records
        .iter()
        .map(|record| record.median_time_past)
        .collect()
}
