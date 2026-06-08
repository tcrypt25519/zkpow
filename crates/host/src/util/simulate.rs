use super::{hash_header, BlockTimestamp, HeaderRecord, NewHeader, State};

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

pub(super) fn median_time_past_for_state(state: &State) -> BlockTimestamp {
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
/// generation should prefer [`super::load_header_batch_witness_from_db`] so the host
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
