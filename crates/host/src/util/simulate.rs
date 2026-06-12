use super::{hash_header, BlockTimestamp, HeaderRecord, NewHeader, State};

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
