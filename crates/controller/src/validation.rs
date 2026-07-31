use zkpow_core::{BlockTimestamp, HeaderInput, State};
use zkpow_store::HeaderRecord;

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
    records.iter().map(|record| HeaderInput::from_header(&record.header)).collect()
}
