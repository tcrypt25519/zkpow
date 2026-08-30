use zkpow_core::{BlockTimestamp, Header, HeaderInput, State};
use zkpow_params::Consensus;
use zkpow_store::HeaderRecord;

/// Apply the cold start batch locally with its sparse median-time-past hints.
pub fn compute_final_state_cold<C: Consensus>(
    initial_state: &State,
    headers: &[HeaderInput],
    hints: &[BlockTimestamp],
) -> State {
    let mut state = *initial_state;
    state
        .apply_headers_from_genesis::<C>(headers, hints)
        .expect("host cold start transition should succeed");
    state
}

/// Apply a warm batch locally with its sparse median-time-past hints.
pub fn compute_final_state_warm<C: Consensus>(
    initial_state: &State,
    headers: &[HeaderInput],
    hints: &[BlockTimestamp],
) -> State {
    let mut state = *initial_state;
    state
        .apply_headers::<C>(headers, hints)
        .expect("host warm transition should succeed");
    state
}

pub fn records_to_new_headers(records: &[HeaderRecord]) -> Vec<HeaderInput> {
    records.iter().map(|record| header_to_input(&record.header)).collect()
}

/// Extract the prover-supplied subset of fields from a full [`Header`].
pub(crate) fn header_to_input(header: &Header) -> HeaderInput {
    HeaderInput {
        version:     header.version,
        merkle_root: header.merkle_root,
        timestamp:   header.timestamp,
        nonce:       header.nonce,
    }
}
